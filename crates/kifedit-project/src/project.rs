use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use kifedit_check::{Diagnostic, Severity, check_file};
use kifedit_db::{File, ProjectInput};
use kifedit_reason::{QueryResult, Taxonomy, evaluate, project_taxonomy};
use kifedit_sema::{FileId, Index, Interner, SemaDb, file_index, lower_file, project_index};
use kifedit_syntax::parser::{Parse, parse};
use kifedit_syntax::{TextEdit, apply_edits};
use rayon::prelude::*;
use salsa::Setter;

use crate::db::RootDatabase;
use crate::locate::{LocatedDiagnostic, locate};
use crate::{Config, CreateFileError, EditFileError, ProjectError, QueryError};

// 查詢文字不屬於任何專案檔案
const QUERY_FILE: FileId = FileId(u32::MAX);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRole {
    Library,
    Editable,
}

#[derive(Debug)]
pub struct SourceFile {
    pub id: FileId,
    pub path: PathBuf,
    pub role: FileRole,
    pub input: File, // 文字存在 salsa 資料庫，以 Project::text 讀取
}

#[derive(Debug)]
pub struct Project {
    root: PathBuf,
    config_path: PathBuf,
    config: Config,
    pub(crate) db: RootDatabase,
    pub(crate) files: Vec<SourceFile>,
    input: ProjectInput,
    versions: Vec<u64>, // 每個檔案的版本號（§14.4），每次修改加 1
}

impl Project {
    pub fn open(config_path: &Path) -> Result<Self, ProjectError> {
        let config = Config::load(config_path)?;
        let root = config_path.parent().unwrap_or(Path::new(".")).to_owned();

        // 同一個檔案同時在 library 與 editable 中時，editable 優先
        let editable = config.editable_paths(&root)?;
        let mut entries: Vec<(FileRole, PathBuf)> = config
            .library_paths(&root)?
            .into_iter()
            .filter(|p| !editable.contains(p))
            .map(|p| (FileRole::Library, p))
            .collect();
        entries.extend(editable.into_iter().map(|p| (FileRole::Editable, p)));

        let texts: Vec<String> = entries
            .par_iter()
            .map(|(_, path)| {
                std::fs::read_to_string(path).map_err(|source| ProjectError::Read {
                    path: path.clone(),
                    source,
                })
            })
            .collect::<Result<_, ProjectError>>()?;

        let db = RootDatabase::default();
        let files: Vec<SourceFile> = entries
            .into_iter()
            .zip(texts)
            .enumerate()
            .map(|(i, ((role, path), text))| SourceFile {
                id: FileId(i as u32),
                path,
                role,
                input: File::new(&db, FileId(i as u32), text),
            })
            .collect();
        let input = ProjectInput::new(&db, files.iter().map(|f| f.input).collect());

        // 每個檔案一個工作：解析、建索引（§15.2 策略 1）；每個執行緒持有資料庫的複本
        files.par_iter().for_each_with(db.clone(), |db, file| {
            file_index(&*db, file.input);
        });
        project_taxonomy(&db, input);

        let versions = vec![0; files.len()];
        Ok(Self {
            root,
            config_path: config_path.to_owned(),
            config,
            db,
            files,
            input,
            versions,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    pub fn text(&self, id: FileId) -> Option<&str> {
        Some(self.file(id)?.input.text(&self.db))
    }

    pub fn parse(&self, id: FileId) -> Option<&Parse> {
        Some(kifedit_db::parse(&self.db, self.file(id)?.input))
    }

    pub fn interner(&self) -> &Interner {
        self.db.interner()
    }

    pub fn index(&self) -> &Index {
        project_index(&self.db, self.input)
    }

    pub fn taxonomy(&self) -> &Taxonomy {
        project_taxonomy(&self.db, self.input)
    }

    pub fn version(&self, id: FileId) -> u64 {
        self.versions.get(id.0 as usize).copied().unwrap_or(0)
    }

    /// 一個檔案的靜態檢查結果（§7），已套用 `[checks]` 的嚴重度覆寫；只重查受修改影響的部分
    pub fn diagnostics(&self, id: FileId) -> Vec<Diagnostic> {
        let Some(file) = self.file(id) else { return Vec::new() };
        with_overrides(&self.config.checks, check_file(&self.db, self.input, file.input))
    }

    /// 與 diagnostics 相同，另外附上行、欄與 related 的位置
    pub fn located_diagnostics(&self, id: FileId) -> Vec<LocatedDiagnostic> {
        let text = self.text(id).unwrap_or_default();
        locate(text, self.diagnostics(id), self.index(), |f| self.text(f))
    }

    /// 唯讀快照，可交給其他執行緒檢查。修改專案時，快照上進行中的查詢會被取消。
    /// 快照不可閒置保留：salsa 寫入前會等所有複本都被丟棄
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            db: self.db.clone(),
            input: self.input,
            files: self
                .files
                .iter()
                .map(|f| SnapshotFile {
                    id: f.id,
                    role: f.role,
                    input: f.input,
                    version: self.version(f.id),
                })
                .collect(),
            checks: self.config.checks.clone(),
        }
    }

    /// 平行檢查所有檔案，結果留在資料庫中；之後的 diagnostics 直接取用
    pub fn check_all(&self) {
        let project = self.input;
        let inputs: Vec<File> = self.files.iter().map(|f| f.input).collect();
        inputs.into_par_iter().for_each_with(self.db.clone(), |db, file| {
            check_file(&*db, project, file);
        });
    }

    /// 在專案目錄下建立空白的 editable 檔，並加進 project.toml 的 [files] editable。
    /// project.toml 只改這一個陣列，註解與排版不變（同 cargo add）
    pub fn create_file(&mut self, name: &str) -> Result<FileId, CreateFileError> {
        let name = name.trim().replace('\\', "/");
        if name.is_empty() {
            return Err(CreateFileError::EmptyName);
        }
        let relative = Path::new(&name);
        if relative.extension().is_none_or(|e| e != "kif") {
            return Err(CreateFileError::NotKif);
        }
        if !relative.components().all(|c| matches!(c, std::path::Component::Normal(_))) {
            return Err(CreateFileError::OutsideProject { name });
        }
        let path = self.root.join(relative);
        if path.exists() || self.files.iter().any(|f| f.path == path) {
            return Err(CreateFileError::Exists { path });
        }

        // 先準備好 project.toml 的新內容；之後寫入失敗時刪掉剛建立的檔案
        let config_error = |message: String| CreateFileError::Config {
            path: self.config_path.clone(),
            message,
        };
        let text = std::fs::read_to_string(&self.config_path).map_err(|e| config_error(e.to_string()))?;
        let mut document: toml_edit::DocumentMut = text.parse().map_err(|e: toml_edit::TomlError| config_error(e.to_string()))?;
        let files = document["files"].or_insert(toml_edit::table());
        let editable = files["editable"].or_insert(toml_edit::value(toml_edit::Array::new()));
        let editable = editable
            .as_array_mut()
            .ok_or_else(|| config_error("[files] editable is not an array.".to_string()))?;
        // 已被 editable 的 pattern 包含時不寫入
        let included = self.config.editable_includes(relative);
        if !included {
            editable.push(name.as_str());
        }

        let create = |e: io::Error| CreateFileError::Create { path: path.clone(), source: e };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(create)?;
        }
        std::fs::File::create_new(&path).map_err(create)?;
        if !included && let Err(e) = std::fs::write(&self.config_path, document.to_string()) {
            let _ = std::fs::remove_file(&path);
            return Err(config_error(e.to_string()));
        }

        let id = FileId(self.files.len() as u32);
        let input = File::new(&self.db, id, String::new());
        self.files.push(SourceFile {
            id,
            path,
            role: FileRole::Editable,
            input,
        });
        self.versions.push(0);
        if !included {
            self.config.files.editable.push(PathBuf::from(&name));
        }
        let inputs: Vec<File> = self.files.iter().map(|f| f.input).collect();
        self.input.set_files(&mut self.db).to(inputs);
        Ok(id)
    }

    /// 套用修改，回傳反向修改；索引與分類由 salsa 查詢在下次讀取時更新
    pub fn edit(&mut self, id: FileId, edits: &[TextEdit]) -> Result<Vec<TextEdit>, EditFileError> {
        let file = self.file(id).ok_or(EditFileError::UnknownFile)?;
        if file.role == FileRole::Library {
            return Err(EditFileError::LibraryFile {
                path: file.path.clone(),
            });
        }
        let input = file.input;
        let mut text = input.text(&self.db).clone();
        let inverse = apply_edits(&mut text, edits)?;
        input.set_text(&mut self.db).to(text);
        self.versions[id.0 as usize] += 1;
        Ok(inverse)
    }

    /// 內建層的結構化查詢，例如 "(instance ?X Camera)"（§5.4）
    pub fn query(&self, text: &str, limit: usize) -> Result<QueryResult, QueryError> {
        let parsed = parse(text);
        if !parsed.errors.is_empty() {
            return Err(QueryError::Syntax);
        }
        let formulas = lower_file(QUERY_FILE, &parsed.syntax(), self.interner());
        let [formula] = formulas.as_slice() else {
            return Err(QueryError::NotOneQuery);
        };
        Ok(evaluate(
            &formula.term,
            self.index(),
            self.taxonomy(),
            self.interner(),
            limit,
        ))
    }

    pub fn save(&self, id: FileId) -> io::Result<()> {
        let file = self.file(id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "There is no file with this ID.")
        })?;
        std::fs::write(&file.path, file.input.text(&self.db))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotFile {
    pub id: FileId,
    pub role: FileRole,
    pub version: u64,
    input: File,
}

pub struct Snapshot {
    db: RootDatabase,
    input: ProjectInput,
    files: Vec<SnapshotFile>,
    checks: BTreeMap<String, Severity>,
}

impl Snapshot {
    pub fn files(&self) -> &[SnapshotFile] {
        &self.files
    }

    pub fn diagnostics(&self, id: FileId) -> Vec<Diagnostic> {
        let Some(file) = self.files.get(id.0 as usize) else { return Vec::new() };
        with_overrides(&self.checks, check_file(&self.db, self.input, file.input))
    }

    pub fn text(&self, id: FileId) -> Option<&str> {
        Some(self.files.get(id.0 as usize)?.input.text(&self.db))
    }

    /// 為已取得的診斷加上行、欄與 related 的位置
    pub fn locate(&self, id: FileId, diagnostics: Vec<Diagnostic>) -> Vec<LocatedDiagnostic> {
        let text = self.text(id).unwrap_or_default();
        locate(text, diagnostics, project_index(&self.db, self.input), |f| self.text(f))
    }
}

fn with_overrides(checks: &BTreeMap<String, Severity>, diagnostics: &[Diagnostic]) -> Vec<Diagnostic> {
    diagnostics
        .iter()
        .filter_map(|d| {
            let severity = checks.get(d.code).copied().unwrap_or(d.severity);
            (severity != Severity::Off).then(|| Diagnostic { severity, ..d.clone() })
        })
        .collect()
}

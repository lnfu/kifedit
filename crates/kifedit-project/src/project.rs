use std::collections::{BTreeMap, HashMap};
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
use crate::{Config, EditFileError, ProjectError, QueryError};

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
    config: Config,
    pub(crate) db: RootDatabase,
    // 以 FileId 為索引。刪除的檔案留下 None，FileId 不重複使用（同 rust-analyzer 的 VFS）
    slots: Vec<Option<SourceFile>>,
    ids: HashMap<PathBuf, FileId>, // 同一個路徑永遠是同一個 FileId
    input: ProjectInput,
    versions: Vec<u64>, // 每個檔案的版本號（§14.4），每次修改加 1
}

/// refresh_files 的結果
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileChanges {
    pub added: Vec<FileId>,
    pub removed: Vec<FileId>,
}

impl Project {
    pub fn open(config_path: &Path) -> Result<Self, ProjectError> {
        let config = Config::load(config_path)?;
        let root = config_path.parent().unwrap_or(Path::new(".")).to_owned();

        let entries = config.files(&root)?;

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
        let slots: Vec<Option<SourceFile>> = entries
            .into_iter()
            .zip(texts)
            .enumerate()
            .map(|(i, ((role, path), text))| SourceFile {
                id: FileId(i as u32),
                path,
                role,
                input: File::new(&db, FileId(i as u32), text),
            })
            .map(Some)
            .collect();
        let input = ProjectInput::new(&db, slots.iter().flatten().map(|f| f.input).collect());

        // 每個檔案一個工作：解析、建索引（§15.2 策略 1）；每個執行緒持有資料庫的複本
        slots.par_iter().flatten().for_each_with(db.clone(), |db, file| {
            file_index(&*db, file.input);
        });
        project_taxonomy(&db, input);

        let ids = slots.iter().flatten().map(|f| (f.path.clone(), f.id)).collect();
        let versions = vec![0; slots.len()];
        Ok(Self {
            root,
            config,
            db,
            slots,
            ids,
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

    /// 專案目前的檔案，依 FileId 排序
    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.slots.iter().flatten()
    }

    pub fn file(&self, id: FileId) -> Option<&SourceFile> {
        self.slots.get(id.0 as usize)?.as_ref()
    }

    pub fn file_id(&self, path: &Path) -> Option<FileId> {
        let id = *self.ids.get(path)?;
        self.file(id).map(|_| id)
    }

    /// 依目前的磁碟內容與 .kifignore 重新決定專案的檔案：加入新的 .kif 檔，移除已不存在或被排除的檔案，
    /// 更新角色。已在專案中的檔案保留記憶體中的文字（可能有未存檔的修改）
    pub fn refresh_files(&mut self) -> Result<FileChanges, ProjectError> {
        let wanted: HashMap<PathBuf, FileRole> =
            self.config.files(&self.root)?.into_iter().map(|(role, path)| (path, role)).collect();
        let mut changes = FileChanges::default();

        for slot in &mut self.slots {
            let Some(file) = slot else { continue };
            match wanted.get(&file.path) {
                Some(&role) => file.role = role,
                None => {
                    changes.removed.push(file.id);
                    *slot = None;
                }
            }
        }
        let mut added: Vec<(&PathBuf, FileRole)> = wanted
            .iter()
            .filter(|(path, _)| self.file_id(path).is_none())
            .map(|(path, &role)| (path, role))
            .collect();
        added.sort_by_key(|(path, _)| *path);
        for (path, role) in added {
            let text = std::fs::read_to_string(path).map_err(|source| ProjectError::Read {
                path: path.clone(),
                source,
            })?;
            let id = match self.ids.get(path) {
                Some(&id) => id,
                None => {
                    let id = FileId(self.slots.len() as u32);
                    self.slots.push(None);
                    self.versions.push(0);
                    self.ids.insert(path.clone(), id);
                    id
                }
            };
            self.slots[id.0 as usize] = Some(SourceFile {
                id,
                path: path.clone(),
                role,
                input: File::new(&self.db, id, text),
            });
            changes.added.push(id);
        }

        if !changes.added.is_empty() || !changes.removed.is_empty() {
            for &id in changes.added.iter().chain(&changes.removed) {
                self.versions[id.0 as usize] += 1;
            }
            let inputs: Vec<File> = self.files().map(|f| f.input).collect();
            let project = self.input;
            project.set_files(&mut self.db).to(inputs);
        }
        Ok(changes)
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
            slots: self
                .slots
                .iter()
                .map(|slot| {
                    slot.as_ref().map(|f| SnapshotFile {
                        id: f.id,
                        role: f.role,
                        input: f.input,
                        version: self.version(f.id),
                    })
                })
                .collect(),
            checks: self.config.checks.clone(),
        }
    }

    /// 平行檢查所有檔案，結果留在資料庫中；之後的 diagnostics 直接取用
    pub fn check_all(&self) {
        let project = self.input;
        let inputs: Vec<File> = self.files().map(|f| f.input).collect();
        inputs.into_par_iter().for_each_with(self.db.clone(), |db, file| {
            check_file(&*db, project, file);
        });
    }

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
    slots: Vec<Option<SnapshotFile>>,
    checks: BTreeMap<String, Severity>,
}

impl Snapshot {
    pub fn files(&self) -> impl Iterator<Item = &SnapshotFile> {
        self.slots.iter().flatten()
    }

    pub fn file(&self, id: FileId) -> Option<&SnapshotFile> {
        self.slots.get(id.0 as usize)?.as_ref()
    }

    pub fn diagnostics(&self, id: FileId) -> Vec<Diagnostic> {
        let Some(file) = self.file(id) else { return Vec::new() };
        with_overrides(&self.checks, check_file(&self.db, self.input, file.input))
    }

    pub fn text(&self, id: FileId) -> Option<&str> {
        Some(self.file(id)?.input.text(&self.db))
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

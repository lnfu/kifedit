use std::io;
use std::path::{Path, PathBuf};

use kifedit_reason::{QueryResult, Taxonomy, evaluate};
use kifedit_sema::{FileId, FileIndex, Index, Interner, lower_file};
use kifedit_syntax::parser::parse;
use kifedit_syntax::{Document, TextEdit};
use rayon::prelude::*;

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
    pub document: Document,
}

#[derive(Debug)]
pub struct Project {
    root: PathBuf,
    config: Config,
    pub(crate) files: Vec<SourceFile>,
    pub(crate) interner: Interner,
    pub(crate) index: Index,
    taxonomy: Taxonomy,
}

impl Project {
    pub fn open(config_path: &Path) -> Result<Self, ProjectError> {
        let config = Config::load(config_path)?;
        let root = config_path.parent().unwrap_or(Path::new(".")).to_owned();

        let mut entries: Vec<(FileRole, PathBuf)> = Vec::new();
        let roles = [
            (FileRole::Library, config.library_paths(&root)),
            (FileRole::Editable, config.editable_paths(&root)),
        ];
        for (role, paths) in roles {
            for path in paths {
                if entries.iter().any(|(_, p)| *p == path) {
                    return Err(ProjectError::DuplicateFile { path });
                }
                entries.push((role, path));
            }
        }

        // 每個檔案一個工作：讀檔、解析、建索引（§15.2 策略 1）
        let interner = Interner::default();
        let loaded: Vec<(SourceFile, FileIndex)> = entries
            .into_par_iter()
            .enumerate()
            .map(|(i, (role, path))| {
                let id = FileId(i as u32);
                let text = std::fs::read_to_string(&path).map_err(|source| ProjectError::Read {
                    path: path.clone(),
                    source,
                })?;
                let document = Document::new(text);
                let file_index = FileIndex::build(id, &document.parse().syntax(), &interner);
                Ok((
                    SourceFile {
                        id,
                        path,
                        role,
                        document,
                    },
                    file_index,
                ))
            })
            .collect::<Result<_, ProjectError>>()?;

        let mut index = Index::default();
        let mut files = Vec::with_capacity(loaded.len());
        for (file, file_index) in loaded {
            index.insert_file(file.id, file_index);
            files.push(file);
        }
        let taxonomy = Taxonomy::build(&index, &interner);

        Ok(Self {
            root,
            config,
            files,
            interner,
            index,
            taxonomy,
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

    pub fn interner(&self) -> &Interner {
        &self.interner
    }

    pub fn index(&self) -> &Index {
        &self.index
    }

    pub fn taxonomy(&self) -> &Taxonomy {
        &self.taxonomy
    }

    /// 套用修改，更新該檔的索引並重建分類，回傳反向修改
    pub fn edit(&mut self, id: FileId, edits: &[TextEdit]) -> Result<Vec<TextEdit>, EditFileError> {
        let file = self
            .files
            .get_mut(id.0 as usize)
            .ok_or(EditFileError::UnknownFile)?;
        if file.role == FileRole::Library {
            return Err(EditFileError::LibraryFile {
                path: file.path.clone(),
            });
        }
        let inverse = file.document.apply(edits)?;
        let file_index = FileIndex::build(id, &file.document.parse().syntax(), &self.interner);
        self.index.insert_file(id, file_index);
        self.taxonomy = Taxonomy::build(&self.index, &self.interner);
        Ok(inverse)
    }

    /// 內建層的結構化查詢，例如 "(instance ?X Camera)"（§5.4）
    pub fn query(&self, text: &str, limit: usize) -> Result<QueryResult, QueryError> {
        let parsed = parse(text);
        if !parsed.errors.is_empty() {
            return Err(QueryError::Syntax);
        }
        let formulas = lower_file(QUERY_FILE, &parsed.syntax(), &self.interner);
        let [formula] = formulas.as_slice() else {
            return Err(QueryError::NotOneQuery);
        };
        Ok(evaluate(
            &formula.term,
            &self.index,
            &self.taxonomy,
            &self.interner,
            limit,
        ))
    }

    pub fn save(&self, id: FileId) -> io::Result<()> {
        let file = self.file(id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "There is no file with this ID.")
        })?;
        std::fs::write(&file.path, file.document.text())
    }
}

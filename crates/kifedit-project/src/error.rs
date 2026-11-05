use std::io;
use std::path::PathBuf;

use kifedit_syntax::EditError;

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("Cannot read the file {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },

    #[error("The project file {} is not correct: {source}", path.display())]
    Config {
        path: PathBuf,
        source: toml::de::Error,
    },

    #[error("The file {} occurs more than one time in [files].", path.display())]
    DuplicateFile { path: PathBuf },

    #[error("The project file {} names a check that does not exist in [checks]: {code}", path.display())]
    UnknownCheck { path: PathBuf, code: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    #[error("The query has a syntax error. Write one query in parentheses, for example (instance ?X Camera).")]
    Syntax,

    #[error("Write one query in parentheses, for example (instance ?X Camera).")]
    NotOneQuery,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateFileError {
    #[error("Write a file name.")]
    EmptyName,

    #[error("The file name must end with .kif.")]
    NotKif,

    #[error("The file must be in the project folder: {name}")]
    OutsideProject { name: String },

    #[error("The file exists: {}", path.display())]
    Exists { path: PathBuf },

    #[error("Cannot create the file {}: {source}", path.display())]
    Create { path: PathBuf, source: io::Error },

    #[error("Cannot change the project file {}: {message}", path.display())]
    Config { path: PathBuf, message: String },
}

#[derive(Debug, thiserror::Error)]
pub enum EditFileError {
    #[error("There is no file with this ID.")]
    UnknownFile,

    #[error("You cannot edit a library file: {}", path.display())]
    LibraryFile { path: PathBuf },

    #[error(transparent)]
    Edit(#[from] EditError),
}

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

    #[error("The pattern in [files] readonly is not correct: {pattern}: {message}")]
    BadPattern { pattern: String, message: String },

    #[error("The pattern in [files] readonly matches no files: {pattern}")]
    NoMatch { pattern: String },

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
pub enum EditFileError {
    #[error("There is no file with this ID.")]
    UnknownFile,

    #[error("You cannot edit a read-only file: {}", path.display())]
    LibraryFile { path: PathBuf },

    #[error(transparent)]
    Edit(#[from] EditError),
}

mod config;
mod error;
mod project;

pub use config::{
    CONFIG_FILE_NAME, Config, FilesSection, FormatSection, LeanSection, ProjectSection, Severity,
    TestsSection, Translation, VampireSection, find_config,
};
pub use error::{EditFileError, ProjectError};
pub use project::{FileRole, Project, SourceFile};

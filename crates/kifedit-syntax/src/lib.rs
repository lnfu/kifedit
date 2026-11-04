pub mod edit;
mod error;
mod format;
pub mod lexer;
pub mod parser;
mod syntax_kind;

pub use rowan::{TextRange, TextSize};

pub use edit::{EditError, TextEdit, apply_edits};
pub use error::{SyntaxError, SyntaxErrorKind};
pub use format::{FormatOptions, format_file};
pub use syntax_kind::{KifLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

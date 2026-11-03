pub mod lexer;
pub mod parser;
mod syntax_kind;

pub use syntax_kind::{KifLanguage, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

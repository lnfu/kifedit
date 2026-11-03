#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    LParen = 0, // (
    RParen,     // )
    Str,        // "hello"
    Number,     // 42
    Word,       // subclass
    Var,        // ?X
    RowVar,     // @ROW
    Comment,    // ; this is a comment
    Whitespace,
    ErrorToken, // "abc
}

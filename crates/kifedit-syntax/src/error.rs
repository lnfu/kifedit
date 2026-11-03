use rowan::TextRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyntaxErrorKind {
    UnclosedList,   // (a (b
    UnopenedRParen, // (a))
    TopLevelAtom,   // 頂層的裸原子，例如 a
    UnclosedString, // "abc
}

impl SyntaxErrorKind {
    pub fn code(self) -> &'static str {
        "E001"
    }

    pub fn message(self) -> &'static str {
        match self {
            Self::UnclosedList => "The list has no closing parenthesis.",
            Self::UnopenedRParen => "This closing parenthesis has no opening parenthesis.",
            Self::TopLevelAtom => "A top-level form must start with an opening parenthesis.",
            Self::UnclosedString => "The string has no closing quotation mark.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub range: TextRange, // 位元組範圍，與樹上節點的位置同一種型別
    pub kind: SyntaxErrorKind,
}

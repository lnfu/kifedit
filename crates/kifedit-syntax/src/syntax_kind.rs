#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
pub enum SyntaxKind {
    // 記號
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

    // 節點
    File,
    TopLevelForm, // (subclass A B)
    List,         // 巢狀的子串列
    Error,        // 未閉合的公式、多餘的 )、頂層的裸原子；必須是最後一個變體
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// rowan 用來區分語言的標記型別，本身沒有值
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KifLanguage {}

impl rowan::Language for KifLanguage {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> SyntaxKind {
        assert!(raw.0 <= SyntaxKind::Error as u16);
        // SAFETY: SyntaxKind 是 repr(u16)，0..=Error 的每個值都是合法變體
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }

    fn kind_to_raw(kind: SyntaxKind) -> rowan::SyntaxKind {
        kind.into()
    }
}

pub type SyntaxNode = rowan::SyntaxNode<KifLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<KifLanguage>;
pub type SyntaxElement = rowan::SyntaxElement<KifLanguage>;

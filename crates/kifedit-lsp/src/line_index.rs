use kifedit_syntax::TextRange;
use tower_lsp::lsp_types::{Position, Range};

/// 位元組位置 → LSP 位置（行從 0 開始，欄以 UTF-16 計算）
pub struct LineIndex<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self { text, starts }
    }

    pub fn position(&self, offset: usize) -> Position {
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let character = self.text[self.starts[line]..offset].encode_utf16().count();
        Position::new(line as u32, character as u32)
    }

    pub fn range(&self, range: TextRange) -> Range {
        Range::new(self.position(range.start().into()), self.position(range.end().into()))
    }
}

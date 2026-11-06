use kifedit_syntax::TextRange;
use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent};

pub fn apply_change(text: &mut String, change: TextDocumentContentChangeEvent) {
    let Some(range) = change.range else {
        *text = change.text;
        return;
    };
    let index = LineIndex::new(text);
    let start = index.offset(range.start);
    let end = index.offset(range.end).max(start);
    text.replace_range(start..end, &change.text);
}

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

    /// LSP 位置 → 位元組位置；超出行尾時停在行尾
    pub fn offset(&self, position: Position) -> usize {
        let Some(&start) = self.starts.get(position.line as usize) else {
            return self.text.len();
        };
        let mut units = 0;
        for (i, c) in self.text[start..].char_indices() {
            if units >= position.character as usize || c == '\n' {
                return start + i;
            }
            units += c.len_utf16();
        }
        self.text.len()
    }

    pub fn range(&self, range: TextRange) -> Range {
        Range::new(
            self.position(range.start().into()),
            self.position(range.end().into()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn change(
        range: Option<((u32, u32), (u32, u32))>,
        text: &str,
    ) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: range
                .map(|((a, b), (c, d))| Range::new(Position::new(a, b), Position::new(c, d))),
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn apply_change_replaces_a_range() {
        let mut text = "(subclass A B)\n(instance x A)\n".to_string();
        apply_change(&mut text, change(Some(((1, 10), (1, 11))), "y"));
        apply_change(&mut text, change(Some(((0, 10), (0, 10))), "My"));
        assert_eq!(text, "(subclass MyA B)\n(instance y A)\n");
    }

    #[test]
    fn apply_change_counts_utf16_units() {
        let mut text = "(documentation 😀 \"相機\")".to_string();
        apply_change(&mut text, change(Some(((0, 20), (0, 21))), "片"));
        assert_eq!(text, "(documentation 😀 \"相片\")");
    }

    #[test]
    fn apply_change_at_end_and_without_range() {
        let mut text = "(a)\n".to_string();
        apply_change(&mut text, change(Some(((1, 0), (1, 0))), "(b)"));
        assert_eq!(text, "(a)\n(b)");
        apply_change(&mut text, change(None, "(c)"));
        assert_eq!(text, "(c)");
    }
}

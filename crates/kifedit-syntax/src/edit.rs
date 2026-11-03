use std::fmt;
use std::ops::Range;

use rowan::{TextRange, TextSize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: TextRange,
    pub new_text: String,
}

impl TextEdit {
    pub fn replace(range: TextRange, new_text: impl Into<String>) -> Self {
        Self {
            range,
            new_text: new_text.into(),
        }
    }

    pub fn insert(offset: TextSize, text: impl Into<String>) -> Self {
        Self::replace(TextRange::empty(offset), text)
    }

    pub fn delete(range: TextRange) -> Self {
        Self::replace(range, String::new())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EditError {
    Overlap,
    OutOfBounds,
    NotCharBoundary,
}

impl EditError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Overlap => "Two edits change the same text.",
            Self::OutOfBounds => "The edit range is not in the text.",
            Self::NotCharBoundary => {
                "The start or the end of the edit range is in the middle of a character."
            }
        }
    }
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for EditError {}

/// 檢查並套用一組修改，回傳反向修改（套用到結果上即可還原，用於復原）。
/// 任何一個修改不合法時，整組都不套用，text 保持不變。
pub fn apply_edits(text: &mut String, edits: &[TextEdit]) -> Result<Vec<TextEdit>, EditError> {
    // 依 (起點, 終點) 穩定排序：同一位置的插入排在取代之前，多個插入保持陣列順序
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| (e.range.start(), e.range.end()));

    let len = TextSize::of(text.as_str());
    for edit in &sorted {
        if edit.range.end() > len {
            return Err(EditError::OutOfBounds);
        }
        let Range { start, end } = to_range(edit.range);
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            return Err(EditError::NotCharBoundary);
        }
    }
    if sorted
        .windows(2)
        .any(|pair| pair[0].range.end() > pair[1].range.start())
    {
        return Err(EditError::Overlap);
    }

    // 反向修改的位置要換算到修改後的文字：加上前面所有修改造成的長度變化
    let mut inverse = Vec::with_capacity(sorted.len());
    let mut shift: i64 = 0;
    for edit in &sorted {
        let start = i64::from(u32::from(edit.range.start())) + shift;
        let new_len = TextSize::of(edit.new_text.as_str());
        let range = TextRange::at(TextSize::new(start as u32), new_len);
        let old_text = text[to_range(edit.range)].to_string();
        inverse.push(TextEdit::replace(range, old_text));
        shift += i64::from(u32::from(new_len)) - i64::from(u32::from(edit.range.len()));
    }

    // 從檔尾往檔頭套用，前面修改的位置不受影響
    for edit in sorted.iter().rev() {
        text.replace_range(to_range(edit.range), &edit.new_text);
    }
    Ok(inverse)
}

fn to_range(range: TextRange) -> Range<usize> {
    range.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replace(start: u32, end: u32, text: &str) -> TextEdit {
        TextEdit::replace(TextRange::new(start.into(), end.into()), text)
    }

    fn apply(text: &str, edits: &[TextEdit]) -> Result<String, EditError> {
        let mut text = text.to_string();
        apply_edits(&mut text, edits)?;
        Ok(text)
    }

    #[test]
    fn single_replace() {
        // B 位於 12..13
        assert_eq!(
            apply("(subclass A B)", &[replace(12, 13, "C")]),
            Ok("(subclass A C)".into())
        );
    }

    #[test]
    fn ranges_refer_to_original_text() {
        let edits = [replace(1, 2, "xx"), replace(5, 6, "yy")];
        assert_eq!(apply("(a b c)", &edits), Ok("(xx b yy)".into()));
        // 陣列順序不影響結果
        let reversed = [replace(5, 6, "yy"), replace(1, 2, "xx")];
        assert_eq!(apply("(a b c)", &reversed), Ok("(xx b yy)".into()));
    }

    #[test]
    fn inserts_at_same_offset_keep_array_order() {
        let edits = [
            TextEdit::insert(1.into(), "X"),
            TextEdit::insert(1.into(), "Y"),
        ];
        assert_eq!(apply("ab", &edits), Ok("aXYb".into()));
    }

    #[test]
    fn overlapping_edits_are_rejected() {
        let edits = [replace(1, 3, "x"), replace(2, 4, "y")];
        assert_eq!(apply("abcdef", &edits), Err(EditError::Overlap));
    }

    #[test]
    fn out_of_bounds_is_rejected() {
        assert_eq!(
            apply("abc", &[replace(2, 4, "x")]),
            Err(EditError::OutOfBounds)
        );
    }

    #[test]
    fn range_inside_character_is_rejected() {
        // 「中」在 UTF-8 中佔 3 個位元組
        assert_eq!(
            apply("中", &[replace(0, 1, "x")]),
            Err(EditError::NotCharBoundary)
        );
    }

    #[test]
    fn invalid_edit_leaves_text_unchanged() {
        let mut text = "abcdef".to_string();
        let edits = [replace(0, 1, "x"), replace(2, 9, "y")];
        assert_eq!(apply_edits(&mut text, &edits), Err(EditError::OutOfBounds));
        assert_eq!(text, "abcdef");
    }

    #[test]
    fn inverse_restores_original() {
        let original = "(documentation A Lang \"舊的說明\")\n(instance A B)\n";
        let mut text = original.to_string();
        // 位置：字串記號 22..36（中文每字 3 位元組），第 2 行從 38 開始，「 B」位於 49..51
        let edits = [
            replace(22, 36, "\"新的說明，比較長\""),         // 取代字串
            TextEdit::insert(38.into(), "(subclass A C)\n"), // 插入一行
            TextEdit::delete(TextRange::new(49.into(), 51.into())), // 刪除「 B」
        ];
        let inverse = apply_edits(&mut text, &edits).unwrap();
        assert_eq!(
            text,
            "(documentation A Lang \"新的說明，比較長\")\n(subclass A C)\n(instance A)\n"
        );

        apply_edits(&mut text, &inverse).unwrap();
        assert_eq!(text, original);
    }
}

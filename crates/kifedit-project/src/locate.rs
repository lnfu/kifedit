//! 把診斷的位元組位置換成行與欄，related 換成檔案與行；問題面板與 CLI 使用

use std::collections::HashMap;

use kifedit_sema::{FileId, FormulaId, Index};

use crate::Diagnostic;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Location {
    pub file: FileId,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocatedDiagnostic {
    pub diagnostic: Diagnostic,
    pub line: usize,   // 從 1 開始
    pub column: usize, // 從 1 開始，以字元計算
    pub related: Vec<Location>,
}

/// 每一行開頭的位元組位置，用二分搜尋找行號
pub(crate) struct Lines<'a> {
    text: &'a str,
    starts: Vec<usize>,
}

impl<'a> Lines<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self { text, starts }
    }

    pub(crate) fn position(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.text.len());
        let line = self.starts.partition_point(|&s| s <= offset);
        let start = self.starts[line - 1];
        let column = self
            .text
            .get(start..offset)
            .map_or(0, |s| s.chars().count())
            + 1;
        (line, column)
    }
}

pub(crate) fn locate<'a>(
    text: &str,
    diagnostics: Vec<Diagnostic>,
    index: &Index,
    file_text: impl Fn(FileId) -> Option<&'a str>,
) -> Vec<LocatedDiagnostic> {
    let lines = Lines::new(text);
    let mut others: HashMap<FileId, Lines<'a>> = HashMap::new();
    let mut related_location = |id: FormulaId| -> Option<Location> {
        let formula = index.formula(id)?;
        let other = match others.entry(id.file) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(Lines::new(file_text(id.file)?))
            }
        };
        let (line, _) = other.position(formula.range.start().into());
        Some(Location {
            file: id.file,
            line,
        })
    };
    diagnostics
        .into_iter()
        .map(|diagnostic| {
            let (line, column) = lines.position(diagnostic.range.start().into());
            let related = diagnostic
                .related
                .iter()
                .filter_map(|&id| related_location(id))
                .collect();
            LocatedDiagnostic {
                diagnostic,
                line,
                column,
                related,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_columns_start_at_one_and_count_characters() {
        let lines = Lines::new("(a b)\n(相機 c)\n");
        assert_eq!(lines.position(0), (1, 1));
        assert_eq!(lines.position(3), (1, 4));
        assert_eq!(lines.position(6), (2, 1));
        assert_eq!(lines.position(6 + 1 + "相機".len() + 1), (2, 5));
    }
}

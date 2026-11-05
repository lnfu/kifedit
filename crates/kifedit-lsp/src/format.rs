use kifedit_project::{FileRole, Project};
use kifedit_sema::FileId;
use kifedit_syntax::{TextEdit, TextRange, TextSize, format_file};

const UNKNOWN: &str = "There is no file with this ID.";

/// 與 Prettier 相同：有語法錯誤的檔案不格式化
pub fn file_edit(project: &Project, file: FileId) -> Result<Option<TextEdit>, String> {
    editable(project, file)?;
    let before = project.text(file).unwrap_or_default();
    let after = format_file(&project.parse(file).ok_or(UNKNOWN)?.syntax(), &project.config().format.options());
    Ok(minimal_edit(before, &after))
}

fn editable(project: &Project, file: FileId) -> Result<&kifedit_project::SourceFile, String> {
    let source = project.file(file).ok_or(UNKNOWN)?;
    if source.role != FileRole::Editable {
        return Err(format!("You cannot format a library file: {}", source.path.display()));
    }
    if !project.parse(file).is_some_and(|p| p.errors.is_empty()) {
        return Err(format!(
            "The file has syntax errors. Correct them, then format the file: {}",
            source.path.display()
        ));
    }
    Ok(source)
}

// 只替換有差異的部分，復原歷史與游標位置才不受整份替換影響
fn minimal_edit(before: &str, after: &str) -> Option<TextEdit> {
    if before == after {
        return None;
    }
    let (b, a) = (before.as_bytes(), after.as_bytes());
    let mut prefix = b.iter().zip(a).take_while(|(x, y)| x == y).count();
    while !before.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let room = b.len().min(a.len()) - prefix;
    let mut suffix = b
        .iter()
        .rev()
        .zip(a.iter().rev())
        .take(room)
        .take_while(|(x, y)| x == y)
        .count();
    while !before.is_char_boundary(b.len() - suffix) {
        suffix -= 1;
    }
    let range = TextRange::new(
        TextSize::from(prefix as u32),
        TextSize::from((b.len() - suffix) as u32),
    );
    Some(TextEdit::replace(range, after[prefix..a.len() - suffix].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_edit_keeps_common_text() {
        let edit = minimal_edit("(a  b)", "(a b)").unwrap();
        assert_eq!(edit.range, TextRange::new(3.into(), 4.into()));
        assert_eq!(edit.new_text, "");
        assert!(minimal_edit("(a b)", "(a b)").is_none());
    }

    #[test]
    fn minimal_edit_stops_at_char_boundaries() {
        let edit = minimal_edit("\"相機\"", "\"相片\"").unwrap();
        let mut text = "\"相機\"".to_string();
        kifedit_syntax::apply_edits(&mut text, &[edit]).unwrap();
        assert_eq!(text, "\"相片\"");
    }
}

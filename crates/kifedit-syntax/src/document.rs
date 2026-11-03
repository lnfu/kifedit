use crate::parser::{Parse, parse};
use crate::{EditError, TextEdit, apply_edits};

#[derive(Debug, Clone)]
pub struct Document {
    text: String,
    parse: Parse,
}

impl Document {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let parse = parse(&text);
        Self { text, parse }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn parse(&self) -> &Parse {
        &self.parse
    }

    pub fn apply(&mut self, edits: &[TextEdit]) -> Result<Vec<TextEdit>, EditError> {
        let inverse = apply_edits(&mut self.text, edits)?;
        self.parse = parse(&self.text);
        Ok(inverse)
    }
}

#[cfg(test)]
mod tests {
    use rowan::TextRange;

    use super::*;

    fn replace(start: u32, end: u32, text: &str) -> TextEdit {
        TextEdit::replace(TextRange::new(start.into(), end.into()), text)
    }

    #[test]
    fn apply_updates_text_and_tree() {
        let mut doc = Document::new("(subclass A B)\n");
        doc.apply(&[replace(12, 13, "C")]).unwrap();
        assert_eq!(doc.text(), "(subclass A C)\n");
        assert_eq!(doc.parse().syntax().to_string(), doc.text());
    }

    #[test]
    fn invalid_edit_leaves_document_unchanged() {
        let mut doc = Document::new("(a)\n");
        assert_eq!(
            doc.apply(&[replace(2, 9, "x")]),
            Err(EditError::OutOfBounds)
        );
        assert_eq!(doc.text(), "(a)\n");
        assert_eq!(doc.parse().syntax().to_string(), "(a)\n");
    }

    #[test]
    fn inverse_restores_text_and_errors() {
        let original = "(a b)\n(c d)\n";
        let mut doc = Document::new(original);

        let inverse = doc
            .apply(&[TextEdit::delete(TextRange::new(4.into(), 5.into()))])
            .unwrap();
        assert_eq!(doc.parse().errors.len(), 1);

        doc.apply(&inverse).unwrap();
        assert_eq!(doc.text(), original);
        assert!(doc.parse().errors.is_empty());
    }
}

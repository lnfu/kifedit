use std::path::PathBuf;

use kifedit_sema::{FileId, FormulaId, Term};
use kifedit_syntax::SyntaxKind::*;
use kifedit_syntax::lexer::lex;
use kifedit_syntax::{EditError, SyntaxElement, SyntaxNode, TextEdit, TextRange, TextSize};

use crate::{EditFileError, FileRole, Project, SourceFile};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormValue {
    Symbol(String),
    Str(String), // 不含前後引號
    Number(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormEdit {
    /// G1：插在同一檔案中最後一條以同一詞彙為主詞的公式之後
    AddFact {
        file: FileId,
        relation: String,
        args: Vec<FormValue>,
    },
    /// G2：argument 為 0 表示第 1 個參數
    ChangeArgument {
        formula: FormulaId,
        argument: usize,
        value: FormValue,
    },
    /// G3
    DeleteFact { formula: FormulaId },
    /// G4
    ChangeString {
        formula: FormulaId,
        argument: usize,
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormError {
    #[error("There is no file with this ID.")]
    UnknownFile,

    #[error("There is no formula with this ID.")]
    UnknownFormula,

    #[error("You cannot edit a read-only file: {}", path.display())]
    LibraryFile { path: PathBuf },

    #[error("The formula does not have argument number {}.", .0 + 1)]
    NoArgument(usize),

    #[error("The argument is not a string.")]
    NotAString,

    #[error("The text cannot contain a quotation mark (\").")]
    QuotationMark,

    #[error(
        "The symbol is not correct: {0:?}. A symbol cannot contain spaces, parentheses, quotation marks, or semicolons. It cannot start with ? or @."
    )]
    BadSymbol(String),

    #[error("The number is not correct: {0:?}.")]
    BadNumber(String),

    #[error("The text is not a correct string: {0:?}.")]
    BadString(String),

    #[error("Give a relation and at least one argument.")]
    EmptyFact,

    #[error(transparent)]
    Edit(#[from] EditError),
}

impl From<EditFileError> for FormError {
    fn from(error: EditFileError) -> Self {
        match error {
            EditFileError::UnknownFile => Self::UnknownFile,
            EditFileError::LibraryFile { path } => Self::LibraryFile { path },
            EditFileError::Edit(e) => Self::Edit(e),
        }
    }
}

impl FormValue {
    // 以詞法器驗證：結果必須剛好是一個對應種類的記號
    fn render(&self) -> Result<String, FormError> {
        let (text, kind, error) = match self {
            Self::Symbol(s) => (s.clone(), Word, FormError::BadSymbol(s.clone())),
            Self::Number(s) => (s.clone(), Number, FormError::BadNumber(s.clone())),
            Self::Str(s) => {
                if s.contains('"') {
                    return Err(FormError::QuotationMark);
                }
                (format!("\"{s}\""), Str, FormError::BadString(s.clone()))
            }
        };
        match lex(&text).as_slice() {
            [token] if token.kind == kind => Ok(text),
            _ => Err(error),
        }
    }
}

impl Project {
    /// 產生修改但不套用（§14.2 form_submit 的預覽）
    pub fn preview_form(&self, edit: &FormEdit) -> Result<Vec<TextEdit>, FormError> {
        match edit {
            FormEdit::AddFact {
                file,
                relation,
                args,
            } => self.add_fact(*file, relation, args),
            FormEdit::ChangeArgument {
                formula,
                argument,
                value,
            } => {
                let (_, node) = self.form_node(*formula)?;
                let element = argument_element(&node, *argument)?;
                Ok(vec![TextEdit::replace(element.text_range(), value.render()?)])
            }
            FormEdit::DeleteFact { formula } => {
                let (file, node) = self.form_node(*formula)?;
                Ok(vec![TextEdit::delete(delete_range(
                    file.input.text(&self.db),
                    node.text_range(),
                ))])
            }
            FormEdit::ChangeString {
                formula,
                argument,
                text,
            } => {
                let (_, node) = self.form_node(*formula)?;
                let element = argument_element(&node, *argument)?;
                if element.kind() != Str {
                    return Err(FormError::NotAString);
                }
                let value = FormValue::Str(text.clone()).render()?;
                Ok(vec![TextEdit::replace(element.text_range(), value)])
            }
        }
    }

    /// 套用表單修改，回傳反向修改（用於復原）
    pub fn apply_form(&mut self, edit: &FormEdit) -> Result<Vec<TextEdit>, FormError> {
        let edits = self.preview_form(edit)?;
        let file = match edit {
            FormEdit::AddFact { file, .. } => *file,
            FormEdit::ChangeArgument { formula, .. }
            | FormEdit::DeleteFact { formula }
            | FormEdit::ChangeString { formula, .. } => formula.file,
        };
        Ok(self.edit(file, &edits)?)
    }

    fn editable_file(&self, id: FileId) -> Result<&SourceFile, FormError> {
        let file = self.file(id).ok_or(FormError::UnknownFile)?;
        if file.role == FileRole::Library {
            return Err(FormError::LibraryFile {
                path: file.path.clone(),
            });
        }
        Ok(file)
    }

    fn form_node(&self, id: FormulaId) -> Result<(&SourceFile, SyntaxNode), FormError> {
        let file = self.editable_file(id.file)?;
        let formula = self.index().formula(id).ok_or(FormError::UnknownFormula)?;
        let node = kifedit_db::parse(&self.db, file.input)
            .syntax()
            .children()
            .find(|n| n.text_range() == formula.range)
            .ok_or(FormError::UnknownFormula)?;
        Ok((file, node))
    }

    fn add_fact(&self, id: FileId, relation: &str, args: &[FormValue]) -> Result<Vec<TextEdit>, FormError> {
        let file = self.editable_file(id)?;
        if args.is_empty() {
            return Err(FormError::EmptyFact);
        }
        let mut parts = vec![FormValue::Symbol(relation.to_string()).render()?];
        for arg in args {
            parts.push(arg.render()?);
        }
        let fact = format!("({})", parts.join(" "));

        let text = file.input.text(&self.db);
        let newline = if uses_crlf(text) { "\r\n" } else { "\n" };
        let subject = match args.get(subject_position(relation)) {
            Some(FormValue::Symbol(s)) => self.interner().get(s),
            _ => None,
        };
        let anchor = subject.and_then(|subject| {
            let formulas = self.index().file(id)?.formulas();
            formulas
                .iter()
                .rev()
                .find(|f| subject_of(&f.term, self.interner()) == Some(subject))
                .map(|f| f.range.end())
        });

        let edit = match anchor {
            Some(end) => TextEdit::insert(line_end(text, end), format!("{newline}{fact}")),
            // 檔案中還沒有這個詞彙：加在檔尾，前面空一行
            None => {
                let end = TextSize::of(text);
                let before = if text.is_empty() {
                    String::new()
                } else if text.ends_with('\n') {
                    newline.to_string()
                } else {
                    newline.repeat(2)
                };
                TextEdit::insert(end, format!("{before}{fact}{newline}"))
            }
        };
        Ok(vec![edit])
    }
}

// termFormat 的詞彙是第 2 個參數，其他關係是第 1 個
fn subject_position(relation: &str) -> usize {
    if relation == "termFormat" { 1 } else { 0 }
}

fn subject_of(term: &Term, interner: &kifedit_sema::Interner) -> Option<kifedit_sema::Symbol> {
    let Term::App(head, args) = term else {
        return None;
    };
    let Term::Symbol(relation) = head.as_ref() else {
        return None;
    };
    match args.get(subject_position(interner.resolve(*relation))) {
        Some(Term::Symbol(s)) => Some(*s),
        _ => None,
    }
}

fn significant(node: &SyntaxNode) -> Vec<SyntaxElement> {
    node.children_with_tokens()
        .filter(|e| !matches!(e.kind(), Whitespace | Comment | LParen | RParen))
        .collect()
}

// 元素 0 是關係本身，參數從 1 開始
fn argument_element(node: &SyntaxNode, argument: usize) -> Result<SyntaxElement, FormError> {
    significant(node)
        .into_iter()
        .nth(argument + 1)
        .ok_or(FormError::NoArgument(argument))
}

// 該位置所在行的行尾（換行字元之前）
fn line_end(text: &str, offset: TextSize) -> TextSize {
    let start = usize::from(offset);
    let rest = &text[start..];
    let len = rest.find('\n').unwrap_or(rest.len());
    let len = if rest[..len].ends_with('\r') { len - 1 } else { len };
    TextSize::from((start + len) as u32)
}

// 公式單獨佔據的行整行刪除（含同一行的註解與換行）；否則只刪除公式本身
fn delete_range(text: &str, form: TextRange) -> TextRange {
    let start = usize::from(form.start());
    let end = usize::from(form.end());

    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let alone_before = text[line_start..start].chars().all(|c| c == ' ' || c == '\t');

    let rest = &text[end..];
    let line_len = rest.find('\n').map_or(rest.len(), |i| i + 1);
    let after = rest[..line_len].trim_start_matches([' ', '\t']);
    let alone_after = after.is_empty() || after.starts_with(['\n', '\r', ';']);

    if alone_before && alone_after {
        TextRange::new(
            TextSize::from(line_start as u32),
            TextSize::from((end + line_len) as u32),
        )
    } else {
        form
    }
}

fn uses_crlf(text: &str) -> bool {
    text.find('\n')
        .is_some_and(|i| i > 0 && text.as_bytes()[i - 1] == b'\r')
}

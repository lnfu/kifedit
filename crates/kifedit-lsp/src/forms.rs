use kifedit_project::{FormEdit, FormValue};
use kifedit_sema::{FileId, FormulaId};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum ValueInput {
    Symbol(String),
    String(String),
    Number(String),
}

impl From<ValueInput> for FormValue {
    fn from(value: ValueInput) -> Self {
        match value {
            ValueInput::Symbol(s) => FormValue::Symbol(s),
            ValueInput::String(s) => FormValue::Str(s),
            ValueInput::Number(s) => FormValue::Number(s),
        }
    }
}

#[derive(Deserialize, Clone, Copy)]
pub struct FormulaInput {
    file: u32,
    index: u32,
}

impl From<FormulaInput> for FormulaId {
    fn from(f: FormulaInput) -> Self {
        FormulaId {
            file: FileId(f.file),
            index: f.index,
        }
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FormInput {
    AddFact {
        file: u32,
        relation: String,
        args: Vec<ValueInput>,
    },
    ChangeArgument {
        formula: FormulaInput,
        argument: usize,
        value: ValueInput,
    },
    DeleteFact {
        formula: FormulaInput,
    },
    ChangeString {
        formula: FormulaInput,
        argument: usize,
        text: String,
    },
}

impl FormInput {
    pub fn into_edit(self) -> (u32, FormEdit) {
        match self {
            FormInput::AddFact {
                file,
                relation,
                args,
            } => (
                file,
                FormEdit::AddFact {
                    file: FileId(file),
                    relation,
                    args: args.into_iter().map(Into::into).collect(),
                },
            ),
            FormInput::ChangeArgument {
                formula,
                argument,
                value,
            } => (
                formula.file,
                FormEdit::ChangeArgument {
                    formula: formula.into(),
                    argument,
                    value: value.into(),
                },
            ),
            FormInput::DeleteFact { formula } => (
                formula.file,
                FormEdit::DeleteFact {
                    formula: formula.into(),
                },
            ),
            FormInput::ChangeString {
                formula,
                argument,
                text,
            } => (
                formula.file,
                FormEdit::ChangeString {
                    formula: formula.into(),
                    argument,
                    text,
                },
            ),
        }
    }
}

// 預覽：會變動的行，以共同前綴與後綴比較
#[derive(Serialize)]
pub struct Preview {
    line: usize,
    removed: Vec<String>,
    added: Vec<String>,
}

pub fn line_diff(before: &str, after: &str) -> Preview {
    let b: Vec<&str> = before.split('\n').collect();
    let a: Vec<&str> = after.split('\n').collect();
    let prefix = b.iter().zip(&a).take_while(|(x, y)| x == y).count();
    let room = b.len().min(a.len()) - prefix;
    let suffix = b
        .iter()
        .rev()
        .zip(a.iter().rev())
        .take(room)
        .take_while(|(x, y)| x == y)
        .count();
    let lines = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
    Preview {
        line: prefix + 1,
        removed: lines(&b[prefix..b.len() - suffix]),
        added: lines(&a[prefix..a.len() - suffix]),
    }
}

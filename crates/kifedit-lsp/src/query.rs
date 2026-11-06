use kifedit_project::Project;
use kifedit_reason::QueryResult;
use kifedit_sema::{FileId, Interner, LogicOp, Quantifier, Term};
use kifedit_syntax::TextRange;
use serde::{Deserialize, Serialize};

const ANSWER_LIMIT: usize = 500;

#[derive(Deserialize)]
pub struct QueryParams {
    pub query: String,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum QueryOutcome<L> {
    Answers {
        columns: Vec<String>,
        rows: Vec<Row<L>>,
    },
    NeedsProver {
        message: &'static str,
    },
    Error {
        message: String,
    },
}

#[derive(Serialize)]
pub struct Row<L> {
    pub values: Vec<String>,
    pub support: Vec<Support<L>>,
}

#[derive(Serialize)]
pub struct Support<L> {
    pub text: String, // 公式的第一行
    pub location: L,
}

/// 支持公式的位置先以位元組範圍回傳，由呼叫者換成 LSP Location
pub fn run(project: &Project, text: &str) -> QueryOutcome<(FileId, TextRange)> {
    let answers = match project.query(text, ANSWER_LIMIT) {
        Ok(QueryResult::Answers(answers)) => answers,
        Ok(QueryResult::NeedsProver(reason)) => {
            return QueryOutcome::NeedsProver {
                message: reason.message(),
            };
        }
        Err(e) => {
            return QueryOutcome::Error {
                message: e.to_string(),
            };
        }
    };
    let interner = project.interner();
    let index = project.index();
    let columns = answers.first().map_or(Vec::new(), |a| {
        a.bindings
            .iter()
            .map(|(var, _)| interner.resolve(*var).to_string())
            .collect()
    });
    let rows = answers
        .iter()
        .map(|a| Row {
            values: a
                .bindings
                .iter()
                .map(|(_, t)| term_text(t, interner))
                .collect(),
            support: a
                .support
                .iter()
                .filter_map(|&id| index.formula(id))
                .map(|f| {
                    let source = project.text(f.id.file).unwrap_or_default();
                    let text = source.get(usize::from(f.range.start())..usize::from(f.range.end()));
                    Support {
                        text: text
                            .and_then(|t| t.lines().next())
                            .unwrap_or("")
                            .to_string(),
                        location: (f.id.file, f.range),
                    }
                })
                .collect(),
        })
        .collect();
    QueryOutcome::Answers { columns, rows }
}

fn term_text(term: &Term, interner: &Interner) -> String {
    let list = |head: &str, items: Vec<String>| format!("({head} {})", items.join(" "));
    let all = |terms: &[Term]| terms.iter().map(|t| term_text(t, interner)).collect();
    match term {
        Term::Symbol(s) | Term::Var(s) | Term::RowVar(s) | Term::Num(s) => {
            interner.resolve(*s).to_string()
        }
        Term::Str(s) => format!("\"{}\"", interner.resolve(*s)),
        Term::App(head, args) => list(&term_text(head, interner), all(args)),
        Term::Logic(op, args) => {
            let head = match op {
                LogicOp::And => "and",
                LogicOp::Or => "or",
                LogicOp::Not => "not",
                LogicOp::Implies => "=>",
                LogicOp::Iff => "<=>",
            };
            list(head, all(args))
        }
        Term::Quant(q, vars, body) => {
            let head = match q {
                Quantifier::ForAll => "forall",
                Quantifier::Exists => "exists",
            };
            let vars: Vec<&str> = vars.iter().map(|v| interner.resolve(*v)).collect();
            format!(
                "({head} ({}) {})",
                vars.join(" "),
                term_text(body, interner)
            )
        }
        Term::Nil => "()".to_string(),
    }
}

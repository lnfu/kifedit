use kifedit_project::Project;
use kifedit_reason::QueryResult;
use kifedit_sema::{Interner, LogicOp, Quantifier, Term, search_names, search_text};
use serde::Serialize;

use crate::term_page::{FormulaRef, formula_ref, kind_labels};

const NAME_LIMIT: usize = 50;
const TEXT_LIMIT: usize = 200;
const ANSWER_LIMIT: usize = 500;

#[derive(Serialize)]
pub struct NameHit {
    name: String,
    kinds: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct TextHit {
    term: Option<String>,
    text: String,
    formula: FormulaRef,
}

#[derive(Serialize)]
pub struct Row {
    values: Vec<String>,
    support: Vec<FormulaRef>,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum QueryOutcome {
    Answers { columns: Vec<String>, rows: Vec<Row> },
    NeedsProver { message: &'static str },
}

pub fn names(project: &Project, query: &str) -> Vec<NameHit> {
    let interner = project.interner();
    search_names(project.index(), interner, query, NAME_LIMIT)
        .into_iter()
        .map(|m| NameHit {
            name: interner.resolve(m.symbol).to_string(),
            kinds: kind_labels(project, m.symbol),
        })
        .collect()
}

pub fn text(project: &Project, query: &str) -> Vec<TextHit> {
    let interner = project.interner();
    let index = project.index();
    search_text(index, interner, query, TEXT_LIMIT)
        .into_iter()
        .filter_map(|m| {
            Some(TextHit {
                term: m.term.map(|s| interner.resolve(s).to_string()),
                text: interner.resolve(m.text).to_string(),
                formula: formula_ref(project, index.formula(m.formula)?),
            })
        })
        .collect()
}

pub fn query(project: &Project, text: &str) -> Result<QueryOutcome, String> {
    let answers = match project.query(text, ANSWER_LIMIT).map_err(|e| e.to_string())? {
        QueryResult::Answers(answers) => answers,
        QueryResult::NeedsProver(reason) => {
            return Ok(QueryOutcome::NeedsProver {
                message: reason.message(),
            });
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
            values: a.bindings.iter().map(|(_, t)| term_text(t, interner)).collect(),
            support: a
                .support
                .iter()
                .filter_map(|&id| index.formula(id))
                .map(|f| formula_ref(project, f))
                .collect(),
        })
        .collect();
    Ok(QueryOutcome::Answers { columns, rows })
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
            format!("({head} ({}) {})", vars.join(" "), term_text(body, interner))
        }
        Term::Nil => "()".to_string(),
    }
}

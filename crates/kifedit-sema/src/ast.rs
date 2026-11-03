use kifedit_syntax::TextRange;

use crate::Symbol;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FormulaId {
    pub file: FileId,
    pub index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicOp {
    And,
    Or,
    Not,
    Implies,
    Iff,
}

impl LogicOp {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "and" => Self::And,
            "or" => Self::Or,
            "not" => Self::Not,
            "=>" => Self::Implies,
            "<=>" => Self::Iff,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Quantifier {
    ForAll,
    Exists,
}

impl Quantifier {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "forall" => Some(Self::ForAll),
            "exists" => Some(Self::Exists),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    Symbol(Symbol),
    Var(Symbol),    // ?X
    RowVar(Symbol), // @ROW
    Str(Symbol),    // 不含前後引號
    Num(Symbol),    // 原始文字
    App(Box<Term>, Vec<Term>),
    Logic(LogicOp, Vec<Term>),
    Quant(Quantifier, Vec<Symbol>, Box<Term>),
    Nil, // ()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Formula {
    pub id: FormulaId,
    pub range: TextRange,
    pub term: Term,
    pub hash: u64,
}

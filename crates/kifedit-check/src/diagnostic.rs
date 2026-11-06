use kifedit_sema::FormulaId;
use kifedit_syntax::TextRange;
use serde::Deserialize;

/// 嚴重度；`Off` 只用於 `kifedit.toml` 的 `[checks]`，診斷本身不會是 `Off`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Off,
}

/// 一條診斷；`related` 指向造成問題的公式，例如 domain 宣告（§7.1）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub range: TextRange,
    pub message: String,
    pub related: Vec<FormulaId>,
}

/// 規則表：代碼、預設嚴重度、說明（§7.1）。CLI 與問題面板的規則說明都讀這張表
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    pub code: &'static str,
    pub severity: Severity,
    pub summary: &'static str,
}

pub const RULES: &[Rule] = &[
    rule("E001", Severity::Error, "Syntax error: a parenthesis or a string is not closed."),
    rule("E002", Severity::Error, "The term is not defined."),
    rule("E003", Severity::Error, "The number of arguments is not correct."),
    rule("E004", Severity::Error, "The argument is not of the class that the domain gives."),
    rule("E005", Severity::Error, "The subclass, subrelation, or subAttribute hierarchy has a cycle."),
    rule("E006", Severity::Error, "An individual is an instance of two disjoint classes."),
    rule("W001", Severity::Warning, "The formula is a duplicate."),
    rule("W002", Severity::Warning, "The term has no documentation."),
    rule("W003", Severity::Warning, "A variable occurs only one time in a rule."),
    rule("W004", Severity::Warning, "A variable in the consequent is not in the antecedent."),
    rule("W005", Severity::Warning, "The relation has no domain."),
    rule("I001", Severity::Info, "No other formula uses the term."),
];

const fn rule(code: &'static str, severity: Severity, summary: &'static str) -> Rule {
    Rule { code, severity, summary }
}

pub fn find_rule(code: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.code == code)
}

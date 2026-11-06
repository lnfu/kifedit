//! 靜態檢查（§7）：每條規則都是 salsa 查詢，只重查受影響的部分

mod diagnostic;
mod queries;
mod rules;

pub use diagnostic::{Diagnostic, RULES, Rule, Severity, find_rule};
pub use queries::{
    Constraint, Signature, TermKey, TermTypes, check_file, class_ancestors, declared,
    declared_terms, disjoint_pairs, file_declarations, has_documentation, hierarchy_cycles,
    referenced, signature, term_types,
};

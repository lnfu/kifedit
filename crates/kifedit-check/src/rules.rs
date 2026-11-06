//! E001–E006、W001–W005、I001（§7.1）。需要精確位置的規則直接走 CST

use kifedit_db::{File, ProjectInput};
use kifedit_sema::{SemaDb, Symbol, Term, file_index};
use kifedit_syntax::{SyntaxElement, SyntaxKind, SyntaxNode, TextRange};
use rustc_hash::FxHashSet;

use crate::queries::{
    Constraint, TermKey, class_ancestors, declared, disjoint_pairs, file_declarations,
    has_documentation, hierarchy_cycles, referenced, signature, term_types,
};
use crate::{Diagnostic, find_rule};

const LOGIC: [&str; 7] = ["and", "or", "not", "=>", "<=>", "forall", "exists"];

pub(crate) fn check(db: &dyn SemaDb, project: ProjectInput, file: File, out: &mut Vec<Diagnostic>) {
    let cx = Context { db, project };
    syntax_errors(&cx, file, out);
    undefined_terms(&cx, file, out);
    arguments(&cx, file, out);
    cycles(&cx, file, out);
    disjoint(&cx, file, out);
    duplicates(&cx, file, out);
    declared_terms(&cx, file, out);
    variables(&cx, file, out);
}

struct Context<'db> {
    db: &'db dyn SemaDb,
    project: ProjectInput,
}

impl<'db> Context<'db> {
    fn key(&self, symbol: Symbol) -> TermKey<'db> {
        TermKey::new(self.db, symbol)
    }

    fn name(&self, symbol: Symbol) -> &str {
        self.db.interner().resolve(symbol)
    }

    fn symbol(&self, text: &str) -> Option<Symbol> {
        self.db.interner().get(text)
    }

    /// class 是 sub 本身或 sub 的祖先
    fn is_subclass(&self, sub: Symbol, class: Symbol) -> bool {
        class_ancestors(self.db, self.project, self.key(sub))
            .binary_search(&class)
            .is_ok()
    }
}

/// 嚴重度取規則表的預設值；`kifedit.toml` 的覆寫在讀取時才套用
fn error(code: &'static str, range: TextRange, message: String) -> Diagnostic {
    Diagnostic {
        code,
        severity: find_rule(code).expect("every code is in RULES").severity,
        range,
        message,
        related: Vec::new(),
    }
}

// E001
fn syntax_errors(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    for e in &kifedit_db::parse(cx.db, file).errors {
        out.push(error(e.kind.code(), e.range, e.kind.message().to_string()));
    }
}

fn forms(cx: &Context, file: File) -> impl Iterator<Item = SyntaxNode> {
    kifedit_db::parse(cx.db, file)
        .syntax()
        .children()
        .filter(|n| n.kind() == SyntaxKind::TopLevelForm)
}

// E002
fn undefined_terms(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    let undefined: FxHashSet<Symbol> = file_index(cx.db, file)
        .symbols()
        .filter(|&s| !declared(cx.db, cx.project, cx.key(s)))
        .collect();
    if undefined.is_empty() {
        return;
    }
    for form in forms(cx, file) {
        for token in form
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
        {
            if token.kind() != SyntaxKind::Word {
                continue;
            }
            if cx
                .symbol(token.text())
                .is_some_and(|s| undefined.contains(&s))
            {
                out.push(error(
                    "E002",
                    token.text_range(),
                    format!(
                        "The term {} is not defined. Use it as argument 1 of instance, subclass, subrelation, or subAttribute.",
                        token.text()
                    ),
                ));
            }
        }
    }
}

// E003、E004：每個以關係名稱開頭的串列
fn arguments(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    let variable_arity = cx.symbol("VariableArityRelation");
    for form in forms(cx, file) {
        for list in form
            .descendants()
            .filter(|n| matches!(n.kind(), SyntaxKind::TopLevelForm | SyntaxKind::List))
        {
            let items: Vec<SyntaxElement> = list
                .children_with_tokens()
                .filter(|e| {
                    !matches!(
                        e.kind(),
                        SyntaxKind::Whitespace
                            | SyntaxKind::Comment
                            | SyntaxKind::LParen
                            | SyntaxKind::RParen
                    )
                })
                .collect();
            let Some(SyntaxElement::Token(head)) = items.first() else {
                continue;
            };
            if head.kind() != SyntaxKind::Word || LOGIC.contains(&head.text()) {
                continue;
            }
            let Some(relation) = cx.symbol(head.text()) else {
                continue;
            };
            let sig = signature(cx.db, cx.project, cx.key(relation));
            let args = &items[1..];

            if let Some(arity) = sig.arity {
                let has_row = args.iter().any(|a| a.kind() == SyntaxKind::RowVar);
                let variable = variable_arity.is_some_and(|v| {
                    term_types(cx.db, cx.project, cx.key(relation))
                        .all
                        .contains(&v)
                });
                if !has_row && !variable && args.len() != arity {
                    out.push(Diagnostic {
                        related: sig.domains.iter().map(|c| c.formula).collect(),
                        ..error(
                            "E003",
                            list.text_range(),
                            format!(
                                "{} must have {arity} arguments. It has {}.",
                                head.text(),
                                args.len()
                            ),
                        )
                    });
                }
            }

            for constraint in &sig.domains {
                let Some(arg) = constraint.position.checked_sub(1).and_then(|i| args.get(i)) else {
                    continue;
                };
                if let Some(message) = argument_problem(cx, head.text(), constraint, arg) {
                    out.push(Diagnostic {
                        related: vec![constraint.formula],
                        ..error("E004", arg.text_range(), message)
                    });
                }
            }
        }
    }
}

/// 參數不符合 domain 時回傳說明；無法判斷時（變數、類別未知）不回報
fn argument_problem(
    cx: &Context,
    relation: &str,
    c: &Constraint,
    arg: &SyntaxElement,
) -> Option<String> {
    let class = cx.name(c.class);
    let position = c.position;
    match arg {
        SyntaxElement::Token(token) => match token.kind() {
            SyntaxKind::Word => {
                let symbol = cx.symbol(token.text())?;
                if c.subclass {
                    let ancestors = class_ancestors(cx.db, cx.project, cx.key(symbol));
                    if ancestors.len() > 1 && ancestors.binary_search(&c.class).is_err() {
                        return Some(format!(
                            "Argument {position} of {relation} must be {class} or a subclass of {class}. {} is not.",
                            token.text()
                        ));
                    }
                    return None;
                }
                let types = term_types(cx.db, cx.project, cx.key(symbol));
                if types.all.is_empty() || types.all.binary_search(&c.class).is_ok() {
                    return None;
                }
                let direct: Vec<&str> = types.direct.iter().map(|&s| cx.name(s)).collect();
                Some(format!(
                    "Argument {position} of {relation} must be an instance of {class}. {} is an instance of {}.",
                    token.text(),
                    direct.join(", ")
                ))
            }
            SyntaxKind::Str if !c.subclass => {
                let string = cx.symbol("SymbolicString")?;
                (!cx.is_subclass(string, c.class)).then(|| {
                    format!("Argument {position} of {relation} must be an instance of {class}. A string is not.")
                })
            }
            SyntaxKind::Number if !c.subclass => {
                let accepts = ["RealNumber", "Integer"]
                    .iter()
                    .filter_map(|n| cx.symbol(n))
                    .any(|n| cx.is_subclass(n, c.class))
                    || cx
                        .symbol("Number")
                        .is_some_and(|n| cx.is_subclass(c.class, n));
                (!accepts).then(|| {
                    format!("Argument {position} of {relation} must be an instance of {class}. A number is not.")
                })
            }
            _ => None,
        },
        // 函數項：依函數的 range 判斷
        SyntaxElement::Node(node) if !c.subclass => {
            let head = node
                .children_with_tokens()
                .filter_map(SyntaxElement::into_token)
                .find(|t| !matches!(t.kind(), SyntaxKind::LParen | SyntaxKind::Whitespace))?;
            let function = cx.symbol(head.text())?;
            let range = signature(cx.db, cx.project, cx.key(function)).range?;
            if range.subclass || cx.is_subclass(range.class, c.class) {
                return None;
            }
            Some(format!(
                "Argument {position} of {relation} must be an instance of {class}. {} returns an instance of {}.",
                head.text(),
                cx.name(range.class)
            ))
        }
        _ => None,
    }
}

// E005：本檔中構成循環的階層事實
fn cycles(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    let cycles = hierarchy_cycles(cx.db, cx.project);
    if cycles.is_empty() {
        return;
    }
    for formula in file_index(cx.db, file).formulas() {
        let Term::App(head, args) = &formula.term else {
            continue;
        };
        let (Term::Symbol(relation), [Term::Symbol(a), Term::Symbol(b)]) =
            (head.as_ref(), args.as_slice())
        else {
            continue;
        };
        let relation = cx.name(*relation);
        for (name, members) in cycles {
            if *name == relation && members.contains(a) && members.contains(b) {
                let mut path: Vec<&str> = members.iter().map(|&s| cx.name(s)).collect();
                path.push(path[0]);
                out.push(error(
                    "E005",
                    formula.range,
                    format!("The {name} hierarchy has a cycle: {}.", path.join(", ")),
                ));
            }
        }
    }
}

// E006：個體同時是兩個互斥類別的實例；每個個體在本檔的第一條 instance 事實回報一次
fn disjoint(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    let pairs = disjoint_pairs(cx.db, cx.project);
    let Some(instance) = cx.symbol("instance") else {
        return;
    };
    if pairs.is_empty() {
        return;
    }
    let mut seen = FxHashSet::default();
    for formula in file_index(cx.db, file).formulas() {
        let Term::App(head, args) = &formula.term else {
            continue;
        };
        let (Term::Symbol(relation), [Term::Symbol(individual), _]) =
            (head.as_ref(), args.as_slice())
        else {
            continue;
        };
        if *relation != instance || !seen.insert(*individual) {
            continue;
        }
        let types = &term_types(cx.db, cx.project, cx.key(*individual)).all;
        for (i, &a) in types.iter().enumerate() {
            for &b in &types[i + 1..] {
                if let Some(&source) = pairs.get(&(a.min(b), a.max(b))) {
                    let (a, b) = (cx.name(a), cx.name(b));
                    out.push(Diagnostic {
                        related: vec![source],
                        ..error(
                            "E006",
                            formula.range,
                            format!(
                                "{} is an instance of {a} and {b}, but {a} and {b} are disjoint.",
                                cx.name(*individual)
                            ),
                        )
                    });
                }
            }
        }
    }
}

// W001：同一檔案中正規化雜湊相同的公式；第二條起回報
fn duplicates(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    let mut first = rustc_hash::FxHashMap::default();
    for formula in file_index(cx.db, file).formulas() {
        match first.entry(formula.hash) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(formula.id);
            }
            std::collections::hash_map::Entry::Occupied(e) => out.push(Diagnostic {
                related: vec![*e.get()],
                ..error(
                    "W001",
                    formula.range,
                    "This formula is a duplicate of an earlier formula in this file.".to_string(),
                )
            }),
        }
    }
}

// W002、W005、I001：本檔宣告的詞彙，各在本檔的第一條宣告回報一次
fn declared_terms(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    let terms = file_declarations(cx.db, file);
    if terms.is_empty() {
        return;
    }
    let declaring: Vec<Symbol> = ["instance", "subclass", "subrelation", "subAttribute"]
        .iter()
        .filter_map(|r| cx.symbol(r))
        .collect();
    let relation = cx.symbol("Relation");
    let mut seen = FxHashSet::default();
    for formula in file_index(cx.db, file).formulas() {
        let Term::App(head, args) = &formula.term else {
            continue;
        };
        let (Term::Symbol(r), Some(Term::Symbol(term))) = (head.as_ref(), args.first()) else {
            continue;
        };
        if !declaring.contains(r) || !seen.insert(*term) {
            continue;
        }
        let key = cx.key(*term);
        let name = cx.name(*term);
        if !has_documentation(cx.db, cx.project, key) {
            out.push(error(
                "W002",
                formula.range,
                format!("The term {name} has no documentation."),
            ));
        }
        let is_relation =
            relation.is_some_and(|r| term_types(cx.db, cx.project, key).all.contains(&r));
        if is_relation && signature(cx.db, cx.project, key).domains.is_empty() {
            out.push(error(
                "W005",
                formula.range,
                format!("The relation {name} has no domain."),
            ));
        }
        if !referenced(cx.db, cx.project, key) {
            out.push(error(
                "I001",
                formula.range,
                format!("No other formula uses the term {name}."),
            ));
        }
    }
}

// W003、W004：規則（以 and、or、not、=>、<=>、forall、exists 開頭的頂層公式）中的變數
fn variables(cx: &Context, file: File, out: &mut Vec<Diagnostic>) {
    for form in forms(cx, file) {
        if !head_word(&form).is_some_and(|h| LOGIC.contains(&h.as_str())) {
            continue;
        }
        // W003：量詞的變數列表也算一次，所以「宣告後沒有使用」與「只用一次的自由變數」都會回報
        let mut count: rustc_hash::FxHashMap<String, (usize, TextRange)> =
            rustc_hash::FxHashMap::default();
        for token in form
            .descendants_with_tokens()
            .filter_map(SyntaxElement::into_token)
        {
            if matches!(token.kind(), SyntaxKind::Var | SyntaxKind::RowVar) {
                count
                    .entry(token.text().to_string())
                    .or_insert((0, token.text_range()))
                    .0 += 1;
            }
        }
        let mut once: Vec<(&String, TextRange)> = count
            .iter()
            .filter(|(_, (n, _))| *n == 1)
            .map(|(v, (_, r))| (v, *r))
            .collect();
        once.sort_by_key(|(_, r)| r.start());
        for (var, range) in once {
            out.push(error(
                "W003",
                range,
                format!("The variable {var} occurs only one time in this rule."),
            ));
        }

        // W004：走過頂層的量詞，找到 =>；右側的自由變數必須出現在左側
        let mut node = form.clone();
        let mut bound: Vec<String> = Vec::new();
        loop {
            let items = items(&node);
            match head_word(&node).as_deref() {
                Some("forall" | "exists") => {
                    if let (Some(SyntaxElement::Node(vars)), Some(SyntaxElement::Node(body))) =
                        (items.get(1), items.get(2))
                    {
                        bound.extend(variable_names(vars));
                        node = body.clone();
                        continue;
                    }
                    break;
                }
                Some("=>") => {
                    let (Some(left), Some(right)) = (items.get(1), items.get(2)) else {
                        break;
                    };
                    let left: FxHashSet<String> = element_variables(left).into_iter().collect();
                    let mut free = Vec::new();
                    free_variables(right, &mut bound.clone(), &mut free);
                    let mut reported = FxHashSet::default();
                    for token in free {
                        let name = token.text().to_string();
                        if !left.contains(&name) && reported.insert(name.clone()) {
                            out.push(error(
                                "W004",
                                token.text_range(),
                                format!("The variable {name} is in the consequent of => but not in the antecedent."),
                            ));
                        }
                    }
                    break;
                }
                _ => break,
            }
        }
    }
}

/// 串列中除了括號、空白、註解以外的元素
fn items(node: &SyntaxNode) -> Vec<SyntaxElement> {
    node.children_with_tokens()
        .filter(|e| {
            !matches!(
                e.kind(),
                SyntaxKind::Whitespace
                    | SyntaxKind::Comment
                    | SyntaxKind::LParen
                    | SyntaxKind::RParen
            )
        })
        .collect()
}

fn head_word(node: &SyntaxNode) -> Option<String> {
    match items(node).first() {
        Some(SyntaxElement::Token(t)) if t.kind() == SyntaxKind::Word => Some(t.text().to_string()),
        _ => None,
    }
}

fn variable_names(node: &SyntaxNode) -> Vec<String> {
    node.descendants_with_tokens()
        .filter_map(SyntaxElement::into_token)
        .filter(|t| matches!(t.kind(), SyntaxKind::Var | SyntaxKind::RowVar))
        .map(|t| t.text().to_string())
        .collect()
}

fn element_variables(element: &SyntaxElement) -> Vec<String> {
    match element {
        SyntaxElement::Node(n) => variable_names(n),
        SyntaxElement::Token(t) if matches!(t.kind(), SyntaxKind::Var | SyntaxKind::RowVar) => {
            vec![t.text().to_string()]
        }
        SyntaxElement::Token(_) => Vec::new(),
    }
}

/// 不被內層量詞綁定的變數記號
fn free_variables(
    element: &SyntaxElement,
    bound: &mut Vec<String>,
    out: &mut Vec<kifedit_syntax::SyntaxToken>,
) {
    match element {
        SyntaxElement::Token(t) => {
            if matches!(t.kind(), SyntaxKind::Var | SyntaxKind::RowVar)
                && !bound.iter().any(|b| b == t.text())
            {
                out.push(t.clone());
            }
        }
        SyntaxElement::Node(node) => {
            let items = items(node);
            if let (Some("forall" | "exists"), Some(SyntaxElement::Node(vars))) =
                (head_word(node).as_deref(), items.get(1))
            {
                let before = bound.len();
                bound.extend(variable_names(vars));
                for item in &items[2..] {
                    free_variables(item, bound, out);
                }
                bound.truncate(before);
            } else {
                for item in &items {
                    free_variables(item, bound, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kifedit_sema::{FileId, Interner};

    use super::*;
    use crate::check_file;

    #[salsa::db]
    #[derive(Clone, Default)]
    struct TestDb {
        storage: salsa::Storage<Self>,
        interner: Arc<Interner>,
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl kifedit_db::Db for TestDb {}

    #[salsa::db]
    impl SemaDb for TestDb {
        fn interner(&self) -> &Interner {
            &self.interner
        }
    }

    // 小型函式庫：關係與類別都已宣告，自身沒有任何診斷
    const LIBRARY: &str = r#"(instance instance BinaryPredicate)
(instance subclass BinaryPredicate)
(instance domain TernaryPredicate)
(instance range BinaryPredicate)
(instance disjoint BinaryPredicate)
(instance BinaryPredicate Entity)
(instance TernaryPredicate Entity)
(instance part BinaryPredicate)
(domain part 1 Object)
(domain part 2 Object)
(instance located BinaryPredicate)
(domain located 1 Object)
(domain located 2 Region)
(instance label BinaryPredicate)
(domain label 1 Entity)
(domain label 2 SymbolicString)
(instance WeightFn UnaryFunction)
(instance UnaryFunction Entity)
(domain WeightFn 1 Object)
(range WeightFn Quantity)
(subclass Object Entity)
(subclass Region Object)
(subclass Device Object)
(subclass Quantity Entity)
(subclass SymbolicString Entity)
(subclass Animal Object)
(subclass Plant Object)
(disjoint Animal Plant)
"#;

    /// 只取錯誤（E 開頭）
    fn diagnostics(lab: &str) -> Vec<String> {
        diagnostics_with(lab, "E")
    }

    fn diagnostics_with(lab: &str, prefix: &str) -> Vec<String> {
        let db = TestDb::default();
        let library = File::new(&db, FileId(0), LIBRARY.to_string());
        let lab = File::new(&db, FileId(1), lab.to_string());
        let project = ProjectInput::new(&db, vec![library, lab]);
        let library_errors: Vec<_> = check_file(&db, project, library)
            .iter()
            .filter(|d| d.code.starts_with('E'))
            .collect();
        assert!(library_errors.is_empty(), "{library_errors:?}");
        let text = lab.text(&db).clone();
        check_file(&db, project, lab)
            .iter()
            .filter(|d| d.code.starts_with(prefix))
            .map(|d| format!("{} {} | {}", d.code, &text[d.range], d.message))
            .collect()
    }

    #[test]
    fn e001_syntax_error() {
        assert_eq!(
            diagnostics("(part A\n"),
            ["E001 (part A | The list has no closing parenthesis."]
        );
    }

    #[test]
    fn e003_wrong_number_of_arguments() {
        let lab = "(instance Arm Device)\n(instance Robot Device)\n(part Arm Robot Robot)\n";
        assert_eq!(
            diagnostics(lab),
            ["E003 (part Arm Robot Robot) | part must have 2 arguments. It has 3."]
        );
    }

    #[test]
    fn e004_argument_of_the_wrong_class() {
        let lab = "(instance Arm Device)\n(instance Lab Region)\n(located Lab Arm)\n(label Arm \"arm\")\n(label Arm 3)\n";
        assert_eq!(
            diagnostics(lab),
            [
                "E004 Arm | Argument 2 of located must be an instance of Region. Arm is an instance of Device.",
                "E004 3 | Argument 2 of label must be an instance of SymbolicString. A number is not.",
            ]
        );
    }

    #[test]
    fn e004_function_term_uses_its_range() {
        let lab = "(instance Arm Device)\n(part Arm (WeightFn Arm))\n";
        assert_eq!(
            diagnostics(lab),
            [
                "E004 (WeightFn Arm) | Argument 2 of part must be an instance of Object. WeightFn returns an instance of Quantity."
            ]
        );
    }

    #[test]
    fn e004_ignores_variables_and_unknown_terms() {
        assert!(diagnostics("(=> (instance ?X Device) (part ?X ?Y))\n").is_empty());
    }

    #[test]
    fn e005_cycle() {
        let lab = "(subclass Arm Hand)\n(subclass Hand Arm)\n(instance Hand Entity)\n";
        let found = diagnostics(lab);
        assert_eq!(found.len(), 2);
        assert!(
            found
                .iter()
                .all(|d| d.starts_with("E005") && d.contains("subclass hierarchy has a cycle"))
        );
    }

    #[test]
    fn e006_instance_of_disjoint_classes() {
        let lab = "(instance Rex Animal)\n(instance Rex Plant)\n";
        assert_eq!(
            diagnostics(lab),
            [
                "E006 (instance Rex Animal) | Rex is an instance of Animal and Plant, but Animal and Plant are disjoint."
            ]
        );
    }

    #[test]
    fn w001_duplicate_formula() {
        let lab = "(instance Arm Device)\n(=> (part ?X ?Y) (part ?X ?Y))\n(=> (part ?A ?B) (part ?A ?B))\n";
        assert_eq!(
            diagnostics_with(lab, "W001"),
            [
                "W001 (=> (part ?A ?B) (part ?A ?B)) | This formula is a duplicate of an earlier formula in this file."
            ]
        );
    }

    #[test]
    fn w002_w005_i001_declared_terms() {
        let lab = "(instance grips BinaryPredicate)\n(instance Arm Device)\n(documentation Arm EnglishLanguage \"An arm.\")\n(instance Hand Device)\n(documentation Hand EnglishLanguage \"A hand.\")\n(part Hand Arm)\n";
        assert_eq!(
            diagnostics_with(lab, "W00"),
            ["W002 (instance grips BinaryPredicate) | The term grips has no documentation.",]
        );
        assert_eq!(
            diagnostics_with(lab, "I"),
            [
                "I001 (instance grips BinaryPredicate) | No other formula uses the term grips.",
                "I001 (instance Hand Device) | No other formula uses the term Hand.",
            ]
        );
    }

    #[test]
    fn w005_relation_without_domain() {
        let lab = "(subclass BinaryPredicate Relation)\n(subclass Relation Entity)\n(instance grips BinaryPredicate)\n";
        assert!(
            diagnostics_with(lab, "W005").iter().any(|d| d
                == "W005 (instance grips BinaryPredicate) | The relation grips has no domain.")
        );
    }

    #[test]
    fn w003_variable_used_one_time() {
        let lab = "(=> (part ?X ?Y) (part ?X ?X))\n(exists (?Z) (part ?Z Arm))\n(forall (?U) (part Arm Arm))\n";
        assert_eq!(
            diagnostics_with(lab, "W003"),
            [
                "W003 ?Y | The variable ?Y occurs only one time in this rule.",
                "W003 ?U | The variable ?U occurs only one time in this rule.",
            ]
        );
    }

    #[test]
    fn w004_consequent_variable_not_in_antecedent() {
        let lab = "(=> (part ?X Arm) (part ?X ?Y))\n(=> (part ?X Arm) (exists (?Y) (part ?X ?Y)))\n(forall (?Y) (=> (part ?X Arm) (part ?X ?Y)))\n";
        assert_eq!(
            diagnostics_with(lab, "W004"),
            ["W004 ?Y | The variable ?Y is in the consequent of => but not in the antecedent."]
        );
    }
}

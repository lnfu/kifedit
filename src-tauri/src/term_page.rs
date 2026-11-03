use std::collections::HashSet;

use kifedit_project::{FileRole, Project};
use kifedit_sema::{Formula, Symbol, Term};
use serde::Serialize;

const GROUP_LIMIT: usize = 200;
const EXAMPLE_LIMIT: usize = 5;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FormulaRef {
    file_id: u32,
    index: u32,
    file: String,
    line: usize,
    text: String,
    editable: bool,
    args: Vec<ArgView>,
}

#[derive(Serialize)]
pub struct ArgView {
    kind: &'static str, // symbol、string、number、other
    text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocEntry {
    relation: &'static str,
    language: String,
    text: String,
    formula: FormulaRef,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    items: Vec<FormulaRef>,
    total: usize,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TermPage {
    name: String,
    exists: bool,
    kinds: Vec<&'static str>,
    documentation: Vec<DocEntry>,
    hierarchy: Option<&'static str>,
    parents: Vec<String>,
    ancestors: Vec<String>,
    children: Vec<String>,
    instances: Vec<String>,
    types: Vec<String>,
    signature: Vec<FormulaRef>,
    arity: Option<usize>,
    examples: Vec<FormulaRef>,
    declarations: Group,
    as_argument: Group,
    in_rules: Group,
}

const DECLARATIONS: [&str; 4] = ["instance", "subclass", "subrelation", "subAttribute"];
const SIGNATURE: [&str; 4] = ["domain", "domainSubclass", "range", "rangeSubclass"];

/// 詞彙頁的四個區塊（§8.2）
pub fn build(project: &Project, name: &str) -> TermPage {
    let mut page = TermPage {
        name: name.to_string(),
        ..TermPage::default()
    };
    let interner = project.interner();
    let Some(term) = interner.get(name) else {
        return page;
    };
    let index = project.index();
    let taxonomy = project.taxonomy();
    let names = |symbols: &[Symbol]| -> Vec<String> {
        symbols.iter().map(|&s| interner.resolve(s).to_string()).collect()
    };
    let sorted = |symbols: &[Symbol]| -> Vec<String> {
        let mut out = names(symbols);
        out.sort();
        out
    };
    let relation_name = |f: &Formula| match &f.term {
        Term::App(head, _) => match head.as_ref() {
            Term::Symbol(s) => Some(interner.resolve(*s)),
            _ => None,
        },
        _ => None,
    };

    page.exists = index.mentions(term).next().is_some();
    let kinds = taxonomy.kinds(term);
    for (flag, label) in [
        (kinds.class, "class"),
        (kinds.relation, "relation"),
        (kinds.function, "function"),
        (kinds.attribute, "attribute"),
        (kinds.individual, "individual"),
    ] {
        if flag {
            page.kinds.push(label);
        }
    }

    // Documentation：documentation 的詞彙是第 1 個參數，termFormat 是第 2 個
    let mut shown = HashSet::new();
    for formula in index.mentions(term) {
        let Term::App(_, args) = &formula.term else { continue };
        let entry = match (relation_name(formula), args.as_slice()) {
            (Some("documentation"), [Term::Symbol(t), Term::Symbol(lang), Term::Str(text)])
                if *t == term =>
            {
                Some(("documentation", *lang, *text))
            }
            (Some("termFormat"), [Term::Symbol(lang), Term::Symbol(t), Term::Str(text)])
                if *t == term =>
            {
                Some(("termFormat", *lang, *text))
            }
            _ => None,
        };
        if let Some((relation, lang, text)) = entry {
            shown.insert(formula.id);
            page.documentation.push(DocEntry {
                relation,
                language: interner.resolve(lang).to_string(),
                text: interner.resolve(text).to_string(),
                formula: formula_ref(project, formula),
            });
        }
    }

    // Hierarchy：取第一個包含此詞彙的階層
    let hierarchies = [
        ("subclass", &taxonomy.subclass),
        ("subrelation", &taxonomy.subrelation),
        ("subAttribute", &taxonomy.sub_attribute),
    ];
    if let Some(&(label, hierarchy)) = hierarchies.iter().find(|(_, h)| h.contains(term)) {
        page.hierarchy = Some(label);
        page.parents = names(hierarchy.parents(term));
        page.ancestors = names(hierarchy.ancestors(term));
        page.children = sorted(hierarchy.children(term));
    }
    page.instances = sorted(taxonomy.instances_of(term));
    page.types = names(&taxonomy.types_of(term));

    // Signature
    for relation in SIGNATURE {
        let Some(relation) = interner.get(relation) else { continue };
        for formula in index.facts(relation, term) {
            shown.insert(formula.id);
            if let Term::App(_, args) = &formula.term {
                if let Some(Term::Num(n)) = args.get(1) {
                    if let Ok(n) = interner.resolve(*n).parse::<usize>() {
                        page.arity = page.arity.max(Some(n));
                    }
                }
            }
            page.signature.push(formula_ref(project, formula));
        }
    }
    if page.kinds.contains(&"relation") {
        page.examples = index
            .facts_of(term)
            .take(EXAMPLE_LIMIT)
            .map(|f| formula_ref(project, f))
            .collect();
    }

    // Axioms：已在上面顯示的公式不重複列出
    for formula in index.mentions(term).filter(|f| !shown.contains(&f.id)) {
        let group = match &formula.term {
            Term::Logic(..) | Term::Quant(..) => &mut page.in_rules,
            Term::App(_, args)
                if relation_name(formula).is_some_and(|r| DECLARATIONS.contains(&r))
                    && matches!(args.first(), Some(Term::Symbol(s)) if *s == term) =>
            {
                &mut page.declarations
            }
            _ => &mut page.as_argument,
        };
        group.total += 1;
        if group.items.len() < GROUP_LIMIT {
            group.items.push(formula_ref(project, formula));
        }
    }
    page
}

fn formula_ref(project: &Project, formula: &Formula) -> FormulaRef {
    let interner = project.interner();
    let file = project.file(formula.id.file);
    let text = file.map_or("", |f| f.document.text());
    let start = usize::from(formula.range.start());
    let end = usize::from(formula.range.end());

    // 表單只能修改簡單事實的原子參數（§8.3）
    let args = match &formula.term {
        Term::App(head, args) if matches!(head.as_ref(), Term::Symbol(_)) => args
            .iter()
            .map(|arg| {
                let (kind, value) = match arg {
                    Term::Symbol(s) => ("symbol", interner.resolve(*s)),
                    Term::Str(s) => ("string", interner.resolve(*s)),
                    Term::Num(s) => ("number", interner.resolve(*s)),
                    _ => ("other", ""),
                };
                ArgView {
                    kind,
                    text: value.to_string(),
                }
            })
            .collect(),
        _ => Vec::new(),
    };

    FormulaRef {
        file_id: formula.id.file.0,
        index: formula.id.index,
        file: file.map_or(String::new(), |f| f.path.display().to_string()),
        line: text.get(..start).map_or(0, |t| t.matches('\n').count() + 1),
        text: text.get(start..end).unwrap_or("").to_string(),
        editable: file.is_some_and(|f| f.role == FileRole::Editable),
        args,
    }
}

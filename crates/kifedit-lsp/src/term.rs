use kifedit_project::Project;
use kifedit_sema::{FileId, Formula, Symbol, Term};
use kifedit_syntax::{SyntaxKind, TextRange, TextSize};

const DECLARATIONS: [&str; 4] = ["instance", "subclass", "subrelation", "subAttribute"];

/// 位置上的 Word token：名稱與範圍
pub fn word_at(project: &Project, file: FileId, offset: usize) -> Option<(String, TextRange)> {
    let root = project.parse(file)?.syntax();
    let token = root
        .token_at_offset(TextSize::from(offset as u32))
        .find(|t| t.kind() == SyntaxKind::Word)?;
    Some((token.text().to_string(), token.text_range()))
}

pub fn hover(project: &Project, name: &str) -> Option<String> {
    let interner = project.interner();
    let term = interner.get(name)?;
    let taxonomy = project.taxonomy();
    let names = |symbols: &[Symbol]| {
        symbols
            .iter()
            .map(|&s| format!("`{}`", interner.resolve(s)))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut out = format!("**{name}**");
    let kinds = kind_labels(project, term);
    if !kinds.is_empty() {
        out += &format!(" ({})", kinds.join(", "));
    }
    let types = taxonomy.types_of(term);
    if !types.is_empty() {
        out += &format!("\n\ninstance of {}", names(&types));
    }
    let hierarchies = [
        ("subclass of", &taxonomy.subclass),
        ("subrelation of", &taxonomy.subrelation),
        ("subattribute of", &taxonomy.sub_attribute),
    ];
    for (label, hierarchy) in hierarchies {
        let parents = hierarchy.parents(term);
        if !parents.is_empty() {
            out += &format!("\n\n{label} {}", names(parents));
        }
    }
    for formula in project.index().mentions(term) {
        let Term::App(_, args) = &formula.term else { continue };
        if relation_name(project, formula) != Some("documentation") {
            continue;
        }
        if let [Term::Symbol(t), _, Term::Str(text)] = args.as_slice() {
            if *t == term {
                // SUMO 以 &%Term 標記文件中的詞彙
                out += &format!("\n\n---\n\n{}", interner.resolve(*text).replace("&%", ""));
            }
        }
    }
    Some(out)
}

/// 宣告此詞彙的公式（instance、subclass 等，詞彙為第 1 個參數）；沒有宣告時回傳 documentation
pub fn definitions(project: &Project, name: &str) -> Vec<(FileId, TextRange)> {
    let Some(term) = project.interner().get(name) else { return Vec::new() };
    let first_arg_is_term = |formula: &&Formula| {
        matches!(&formula.term, Term::App(_, args) if matches!(args.first(), Some(Term::Symbol(s)) if *s == term))
    };
    let with_relation = |relations: &[&str]| -> Vec<(FileId, TextRange)> {
        project
            .index()
            .mentions(term)
            .filter(first_arg_is_term)
            .filter(|f| relation_name(project, f).is_some_and(|r| relations.contains(&r)))
            .map(|f| (f.id.file, f.range))
            .collect()
    };
    let declarations = with_relation(&DECLARATIONS);
    if declarations.is_empty() {
        with_relation(&["documentation"])
    } else {
        declarations
    }
}

fn relation_name<'a>(project: &'a Project, formula: &Formula) -> Option<&'a str> {
    match &formula.term {
        Term::App(head, _) => match head.as_ref() {
            Term::Symbol(s) => Some(project.interner().resolve(*s)),
            _ => None,
        },
        _ => None,
    }
}

fn kind_labels(project: &Project, term: Symbol) -> Vec<&'static str> {
    let kinds = project.taxonomy().kinds(term);
    [
        (kinds.class, "class"),
        (kinds.relation, "relation"),
        (kinds.function, "function"),
        (kinds.attribute, "attribute"),
        (kinds.individual, "individual"),
    ]
    .into_iter()
    .filter_map(|(flag, label)| flag.then_some(label))
    .collect()
}

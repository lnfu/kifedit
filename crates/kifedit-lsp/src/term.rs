use kifedit_project::Project;
use kifedit_sema::{FileId, Formula, Symbol, Term, search_names};
use kifedit_syntax::{SyntaxKind, TextRange, TextSize};

const SYMBOL_LIMIT: usize = 100;

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

    let mut out = format!("**{name}** · [Open Term Page]({})", open_page_link(name));
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
        let Term::App(_, args) = &formula.term else {
            continue;
        };
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

// VS Code 的 command URI：參數是以 URL 編碼的 JSON 陣列
fn open_page_link(name: &str) -> String {
    let json = format!("[\"{}\"]", name.replace('\\', "\\\\").replace('"', "\\\""));
    let encoded: String = json
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("command:kifedit.openTermPage?{encoded}")
}

/// 宣告此詞彙的公式（instance、subclass 等，詞彙為第 1 個參數）；沒有宣告時回傳 documentation
pub fn definitions(project: &Project, name: &str) -> Vec<(FileId, TextRange)> {
    let Some(term) = project.interner().get(name) else {
        return Vec::new();
    };
    let first_arg_is_term = |formula: &&Formula| matches!(&formula.term, Term::App(_, args) if matches!(args.first(), Some(Term::Symbol(s)) if *s == term));
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

/// 名稱的模糊搜尋；每個詞彙取第一個定義的位置
pub fn symbols(
    project: &Project,
    query: &str,
) -> Vec<(String, Vec<&'static str>, FileId, TextRange)> {
    let interner = project.interner();
    search_names(project.index(), interner, query, SYMBOL_LIMIT)
        .into_iter()
        .filter_map(|m| {
            let name = interner.resolve(m.symbol);
            let (file, range) = definitions(project, name).into_iter().next().or_else(|| {
                project
                    .index()
                    .mentions(m.symbol)
                    .next()
                    .map(|f| (f.id.file, f.range))
            })?;
            Some((
                name.to_string(),
                kind_labels(project, m.symbol),
                file,
                range,
            ))
        })
        .collect()
}

/// 每個提到此詞彙的 Word token
pub fn references(project: &Project, name: &str) -> Vec<(FileId, TextRange)> {
    let Some(term) = project.interner().get(name) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for formula in project.index().mentions(term) {
        let Some(parse) = project.parse(formula.id.file) else {
            continue;
        };
        let Some(node) = parse.syntax().covering_element(formula.range).into_node() else {
            continue;
        };
        out.extend(
            node.descendants_with_tokens()
                .filter_map(|e| e.into_token())
                .filter(|t| t.kind() == SyntaxKind::Word && t.text() == name)
                .map(|t| (formula.id.file, t.text_range())),
        );
    }
    out.sort_by_key(|(file, range)| (file.0, range.start()));
    out.dedup();
    out
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

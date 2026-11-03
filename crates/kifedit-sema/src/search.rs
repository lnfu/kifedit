use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use crate::{FormulaId, Index, Interner, Symbol, Term};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NameMatch {
    pub symbol: Symbol,
    pub score: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextMatch {
    pub formula: FormulaId,
    pub term: Option<Symbol>,
    pub text: Symbol,
}

struct Named<'a> {
    name: &'a str,
    symbol: Symbol,
}

impl AsRef<str> for Named<'_> {
    fn as_ref(&self) -> &str {
        self.name
    }
}

pub fn search_names(
    index: &Index,
    interner: &Interner,
    query: &str,
    limit: usize,
) -> Vec<NameMatch> {
    let named: Vec<Named> = index
        .terms()
        .into_iter()
        .map(|symbol| Named {
            name: interner.resolve(symbol),
            symbol,
        })
        .collect();
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut matches = pattern.match_list(named, &mut matcher);
    matches.sort_by(|(a, sa), (b, sb)| {
        sb.cmp(sa)
            .then(a.name.len().cmp(&b.name.len()))
            .then(a.name.cmp(b.name))
    });
    matches
        .into_iter()
        .take(limit)
        .map(|(named, score)| NameMatch {
            symbol: named.symbol,
            score,
        })
        .collect()
}

pub fn search_text(
    index: &Index,
    interner: &Interner,
    query: &str,
    limit: usize,
) -> Vec<TextMatch> {
    let query = query.to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let term_format = interner.get("termFormat");
    let mut out = Vec::new();

    for formula in index.formulas() {
        let Term::App(head, args) = &formula.term else {
            continue;
        };
        let Term::Symbol(relation) = head.as_ref() else {
            continue;
        };
        let term_position = if Some(*relation) == term_format { 1 } else { 0 };
        let term = match args.get(term_position) {
            Some(Term::Symbol(s)) => Some(*s),
            _ => None,
        };
        for arg in args {
            if let Term::Str(text) = arg {
                if interner.resolve(*text).to_lowercase().contains(&query) {
                    out.push(TextMatch {
                        formula: formula.id,
                        term,
                        text: *text,
                    });
                    if out.len() == limit {
                        return out;
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use kifedit_syntax::parser::parse;

    use super::*;
    use crate::FileId;

    const SRC: &str = "(subclass Camera Device)
(subclass DepthCamera Camera)
(instance HeadCameraLeft VideoCamera)
(subclass Cabinet Furniture)
(documentation Camera EnglishLanguage \"A device for taking PICTURES.\")
(documentation DepthCamera ChineseTraditionalLanguage \"可輸出深度影像的相機。\")
(termFormat ChineseTraditionalLanguage HeadCameraLeft \"頭部左相機\")
(todoNote HeadCameraLeft \"補上相機的 IP 位址。\")
";

    fn build() -> (Interner, Index) {
        let interner = Interner::default();
        let mut index = Index::default();
        index.set_file(FileId(0), &parse(SRC).syntax(), &interner);
        (interner, index)
    }

    fn names(interner: &Interner, matches: &[NameMatch]) -> Vec<String> {
        matches
            .iter()
            .map(|m| interner.resolve(m.symbol).to_string())
            .collect()
    }

    #[test]
    fn exact_name_ranks_first() {
        let (interner, index) = build();
        let found = names(&interner, &search_names(&index, &interner, "camera", 10));
        assert_eq!(found[0], "Camera");
        assert!(found.contains(&"DepthCamera".to_string()));
        assert!(found.contains(&"HeadCameraLeft".to_string()));
        assert!(!found.contains(&"Cabinet".to_string()));
    }

    #[test]
    fn fuzzy_name_and_limit() {
        let (interner, index) = build();
        let found = names(&interner, &search_names(&index, &interner, "dpcam", 10));
        assert_eq!(found, ["DepthCamera"]);
        assert_eq!(search_names(&index, &interner, "camera", 2).len(), 2);
    }

    #[test]
    fn text_search_ignores_case_and_finds_chinese() {
        let (interner, index) = build();
        let term = |q| {
            search_text(&index, &interner, q, 10)
                .iter()
                .map(|m| interner.resolve(m.term.unwrap()).to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(term("pictures"), ["Camera"]);
        // termFormat 的詞彙是第 2 個參數；todoNote 等其他關係的字串也搜尋
        assert_eq!(
            term("相機"),
            ["DepthCamera", "HeadCameraLeft", "HeadCameraLeft"]
        );
        assert!(term("").is_empty());
    }
}

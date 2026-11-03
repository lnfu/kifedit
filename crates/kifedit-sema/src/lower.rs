use kifedit_syntax::SyntaxKind::*;
use kifedit_syntax::{SyntaxElement, SyntaxNode, SyntaxToken};

use crate::hash::formula_hash;
use crate::{FileId, Formula, FormulaId, Interner, LogicOp, Quantifier, Symbol, Term};

pub fn lower_file(file: FileId, root: &SyntaxNode, interner: &Interner) -> Vec<Formula> {
    root.children()
        .filter(|node| node.kind() == TopLevelForm)
        .enumerate()
        .map(|(index, node)| {
            let term = lower_list(&node, interner);
            let hash = formula_hash(&term, interner);
            Formula {
                id: FormulaId {
                    file,
                    index: index as u32,
                },
                range: node.text_range(),
                term,
                hash,
            }
        })
        .collect()
}

fn lower_list(node: &SyntaxNode, interner: &Interner) -> Term {
    let elements = significant(node);
    let Some(first) = elements.first() else {
        return Term::Nil;
    };
    let args = &elements[1..];

    if let Some(name) = first.as_token().filter(|t| t.kind() == Word).map(|t| t.text()) {
        if let Some(op) = LogicOp::from_name(name) {
            return Term::Logic(op, lower_all(args, interner));
        }
        if let (Some(q), [vars, body]) = (Quantifier::from_name(name), args) {
            if let Some(vars) = variable_list(vars, interner) {
                return Term::Quant(q, vars, Box::new(lower_element(body, interner)));
            }
        }
    }
    // 包含首項是變數的情況，例如 (?REL ?X ?Y)；以及變數列表不合法的量詞，留給檢查器回報
    Term::App(
        Box::new(lower_element(first, interner)),
        lower_all(args, interner),
    )
}

fn significant(node: &SyntaxNode) -> Vec<SyntaxElement> {
    node.children_with_tokens()
        .filter(|e| !matches!(e.kind(), Whitespace | Comment | LParen | RParen))
        .collect()
}

fn variable_list(element: &SyntaxElement, interner: &Interner) -> Option<Vec<Symbol>> {
    let node = element.as_node().filter(|n| n.kind() == List)?;
    significant(node)
        .iter()
        .map(|e| {
            e.as_token()
                .filter(|t| matches!(t.kind(), Var | RowVar))
                .map(|t| interner.intern(t.text()))
        })
        .collect()
}

fn lower_all(elements: &[SyntaxElement], interner: &Interner) -> Vec<Term> {
    elements.iter().map(|e| lower_element(e, interner)).collect()
}

fn lower_element(element: &SyntaxElement, interner: &Interner) -> Term {
    match (element.as_node(), element.as_token()) {
        (Some(node), _) => lower_list(node, interner),
        (_, Some(token)) => lower_token(token, interner),
        _ => unreachable!(),
    }
}

fn lower_token(token: &SyntaxToken, interner: &Interner) -> Term {
    let text = token.text();
    match token.kind() {
        Var => Term::Var(interner.intern(text)),
        RowVar => Term::RowVar(interner.intern(text)),
        Number => Term::Num(interner.intern(text)),
        Str => {
            let content = text
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(text);
            Term::Str(interner.intern(content))
        }
        _ => Term::Symbol(interner.intern(text)),
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use expect_test::{Expect, expect};
    use kifedit_syntax::parser::parse;

    use super::*;

    fn dump(term: &Term, interner: &Interner, depth: usize, out: &mut String) {
        let pad = "  ".repeat(depth);
        let name = |s: &Symbol| interner.resolve(*s).to_string();
        match term {
            Term::Symbol(s) => writeln!(out, "{pad}Symbol {}", name(s)),
            Term::Var(s) => writeln!(out, "{pad}Var {}", name(s)),
            Term::RowVar(s) => writeln!(out, "{pad}RowVar {}", name(s)),
            Term::Str(s) => writeln!(out, "{pad}Str {:?}", name(s)),
            Term::Num(s) => writeln!(out, "{pad}Num {}", name(s)),
            Term::Nil => writeln!(out, "{pad}Nil"),
            Term::App(head, args) => {
                writeln!(out, "{pad}App").unwrap();
                dump(head, interner, depth + 1, out);
                args.iter().for_each(|a| dump(a, interner, depth + 1, out));
                Ok(())
            }
            Term::Logic(op, args) => {
                writeln!(out, "{pad}Logic {op:?}").unwrap();
                args.iter().for_each(|a| dump(a, interner, depth + 1, out));
                Ok(())
            }
            Term::Quant(q, vars, body) => {
                let vars: Vec<_> = vars.iter().map(name).collect();
                writeln!(out, "{pad}Quant {q:?} {}", vars.join(" ")).unwrap();
                dump(body, interner, depth + 1, out);
                Ok(())
            }
        }
        .unwrap();
    }

    fn lower(src: &str, interner: &Interner) -> Vec<Formula> {
        lower_file(FileId(0), &parse(src).syntax(), interner)
    }

    fn check(src: &str, expect: Expect) {
        let interner = Interner::default();
        let mut out = String::new();
        for formula in lower(src, &interner) {
            writeln!(out, "#{} {:?}", formula.id.index, formula.range).unwrap();
            dump(&formula.term, &interner, 1, &mut out);
        }
        expect.assert_eq(&out);
    }

    fn hash_of(src: &str) -> u64 {
        lower(src, &Interner::default())[0].hash
    }

    #[test]
    fn fact() {
        check("(subclass A B)", expect![[r#"
            #0 0..14
              App
                Symbol subclass
                Symbol A
                Symbol B
        "#]]);
    }

    #[test]
    fn rule_with_logic_and_quantifier() {
        check(
            "(=> (and (instance ?X Camera) (exists (?Y) (part ?Y ?X))) (instance ?X Device))",
            expect![[r#"
                #0 0..79
                  Logic Implies
                    Logic And
                      App
                        Symbol instance
                        Var ?X
                        Symbol Camera
                      Quant Exists ?Y
                        App
                          Symbol part
                          Var ?Y
                          Var ?X
                    App
                      Symbol instance
                      Var ?X
                      Symbol Device
            "#]],
        );
    }

    #[test]
    fn strings_numbers_functions_and_row_variables() {
        check(
            "(monetaryValue X (MeasureFn 6370 TaiwanDollar))\n(documentation X EnglishLanguage \"a b\")\n(holds @ROW)",
            expect![[r##"
                #0 0..47
                  App
                    Symbol monetaryValue
                    Symbol X
                    App
                      Symbol MeasureFn
                      Num 6370
                      Symbol TaiwanDollar
                #1 48..87
                  App
                    Symbol documentation
                    Symbol X
                    Symbol EnglishLanguage
                    Str "a b"
                #2 88..100
                  App
                    Symbol holds
                    RowVar @ROW
            "##]],
        );
    }

    #[test]
    fn variable_in_head_position() {
        check("(=> (?REL ?X ?Y) (?REL ?Y ?X))", expect![[r#"
            #0 0..30
              Logic Implies
                App
                  Var ?REL
                  Var ?X
                  Var ?Y
                App
                  Var ?REL
                  Var ?Y
                  Var ?X
        "#]]);
    }

    #[test]
    fn malformed_quantifier_is_an_application() {
        check("(forall ?X (p ?X))", expect![[r#"
            #0 0..18
              App
                Symbol forall
                Var ?X
                App
                  Symbol p
                  Var ?X
        "#]]);
    }

    #[test]
    fn skips_forms_with_syntax_errors_and_comments() {
        check("(a b)\n(c (d\n(e ; note\n  f)\n()", expect![[r#"
            #0 0..5
              App
                Symbol a
                Symbol b
            #1 12..26
              App
                Symbol e
                Symbol f
            #2 27..29
              Nil
        "#]]);
    }

    #[test]
    fn hash_ignores_layout_and_comments() {
        assert_eq!(hash_of("(subclass A B)"), hash_of("(subclass\n  A ; c\n  B)"));
    }

    #[test]
    fn hash_ignores_variable_names() {
        assert_eq!(hash_of("(=> (p ?X) (q ?X))"), hash_of("(=> (p ?Y) (q ?Y))"));
    }

    #[test]
    fn hash_keeps_variable_structure() {
        assert_ne!(hash_of("(p ?X ?Y)"), hash_of("(p ?X ?X)"));
    }

    #[test]
    fn hash_does_not_depend_on_intern_order() {
        let interner = Interner::default();
        interner.intern("zzz");
        interner.intern("B");
        let shuffled = lower("(subclass A B)", &interner)[0].hash;
        assert_eq!(shuffled, hash_of("(subclass A B)"));
    }
}

use kifedit_sema::{FormulaId, Index, Interner, Symbol, Term};
use rustc_hash::FxHashSet;

use crate::{Hierarchy, Taxonomy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub bindings: Vec<(Symbol, Term)>, // 依變數在查詢中第一次出現的順序
    pub support: Vec<FormulaId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryResult {
    Answers(Vec<Answer>),
    NeedsProver(NeedsProver),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NeedsProver {
    LogicalOperators,
    Quantifier,
    VariableRelation,
    NotARelation,
    ComplexArgument,
}

impl NeedsProver {
    pub fn message(self) -> &'static str {
        match self {
            Self::LogicalOperators => "The query contains logical operators. Use Vampire.",
            Self::Quantifier => "The query contains a quantifier. Use Vampire.",
            Self::VariableRelation => "The relation in the query is a variable. Use Vampire.",
            Self::NotARelation => "The query is not a relation with arguments. Use Vampire.",
            Self::ComplexArgument => {
                "An argument in the query is not a symbol, a variable, or a term without variables. Use Vampire."
            }
        }
    }
}

struct Candidate {
    values: Vec<Term>,
    support: Vec<FormulaId>,
}

/// 內建層的結構化查詢（§5.4、§6.1）：單一關係，參數為變數或不含變數的項。
/// instance、subclass、subrelation、subAttribute 套用分類閉包；其他關係包含其所有子關係的事實
pub fn evaluate(
    pattern: &Term,
    index: &Index,
    taxonomy: &Taxonomy,
    interner: &Interner,
    limit: usize,
) -> QueryResult {
    let (relation, args) = match pattern {
        Term::App(head, args) => match head.as_ref() {
            Term::Symbol(relation) => (*relation, args.as_slice()),
            Term::Var(_) | Term::RowVar(_) => return QueryResult::NeedsProver(NeedsProver::VariableRelation),
            _ => return QueryResult::NeedsProver(NeedsProver::NotARelation),
        },
        Term::Logic(..) => return QueryResult::NeedsProver(NeedsProver::LogicalOperators),
        Term::Quant(..) => return QueryResult::NeedsProver(NeedsProver::Quantifier),
        _ => return QueryResult::NeedsProver(NeedsProver::NotARelation),
    };
    if !args.iter().all(|a| matches!(a, Term::Var(_)) || is_ground(a)) {
        return QueryResult::NeedsProver(NeedsProver::ComplexArgument);
    }

    let context = Context { index, taxonomy };
    let candidates = match (interner.resolve(relation), args) {
        ("instance", [x, c]) => context.instance(relation, x, c),
        ("subclass", [a, b]) => context.hierarchy(&taxonomy.subclass, relation, a, b),
        ("subrelation", [a, b]) => context.hierarchy(&taxonomy.subrelation, relation, a, b),
        ("subAttribute", [a, b]) => context.hierarchy(&taxonomy.sub_attribute, relation, a, b),
        _ => context.facts(relation, args),
    };

    let mut seen = FxHashSet::default();
    let answers = candidates
        .into_iter()
        .filter_map(|c| {
            let bindings = unify(args, &c.values)?;
            Some(Answer {
                bindings,
                support: c.support,
            })
        })
        .filter(|answer| seen.insert(answer.bindings.clone()))
        .take(limit)
        .collect();
    QueryResult::Answers(answers)
}

struct Context<'a> {
    index: &'a Index,
    taxonomy: &'a Taxonomy,
}

impl Context<'_> {
    fn instance(&self, relation: Symbol, x: &Term, c: &Term) -> Vec<Candidate> {
        let taxonomy = self.taxonomy;
        let pairs: Vec<(Symbol, Symbol)> = match (x, c) {
            (Term::Symbol(x), _) => taxonomy.types_of(*x).into_iter().map(|c| (*x, c)).collect(),
            (_, Term::Symbol(c)) => std::iter::once(*c)
                .chain(taxonomy.subclass.descendants(*c))
                .flat_map(|class| taxonomy.instances_of(class).iter().map(|&i| (i, *c)))
                .collect(),
            (Term::Var(_), Term::Var(_)) => taxonomy
                .individuals()
                .flat_map(|i| taxonomy.types_of(i).into_iter().map(move |c| (i, c)))
                .collect(),
            _ => Vec::new(),
        };
        // 支持的公式：直接的 instance 事實，其類別就是 c 或是 c 的子孫
        pairs
            .into_iter()
            .map(|(i, c)| Candidate {
                values: vec![Term::Symbol(i), Term::Symbol(c)],
                support: self.direct_facts(relation, i, |k| {
                    k == c || taxonomy.subclass.is_ancestor(c, k)
                }),
            })
            .collect()
    }

    fn hierarchy(&self, hierarchy: &Hierarchy, relation: Symbol, a: &Term, b: &Term) -> Vec<Candidate> {
        let pairs: Vec<(Symbol, Symbol)> = match (a, b) {
            (Term::Symbol(a), _) => hierarchy.ancestors(*a).iter().map(|&b| (*a, b)).collect(),
            (_, Term::Symbol(b)) => hierarchy.descendants(*b).into_iter().map(|a| (a, *b)).collect(),
            (Term::Var(_), Term::Var(_)) => hierarchy
                .nodes()
                .flat_map(|a| hierarchy.ancestors(a).iter().map(move |&b| (a, b)))
                .collect(),
            _ => Vec::new(),
        };
        // 支持的公式：a 的直接事實，其父節點就是 b 或在通往 b 的路徑上
        pairs
            .into_iter()
            .map(|(a, b)| Candidate {
                values: vec![Term::Symbol(a), Term::Symbol(b)],
                support: self.direct_facts(relation, a, |p| p == b || hierarchy.is_ancestor(b, p)),
            })
            .collect()
    }

    fn facts(&self, relation: Symbol, args: &[Term]) -> Vec<Candidate> {
        let relations =
            std::iter::once(relation).chain(self.taxonomy.subrelation.descendants(relation));
        let mut out = Vec::new();
        for relation in relations {
            let formulas: Box<dyn Iterator<Item = _>> = match args.first() {
                Some(Term::Symbol(first)) => Box::new(self.index.facts(relation, *first)),
                _ => Box::new(self.index.facts_of(relation)),
            };
            for formula in formulas {
                if let Term::App(_, values) = &formula.term {
                    out.push(Candidate {
                        values: values.clone(),
                        support: vec![formula.id],
                    });
                }
            }
        }
        out
    }

    fn direct_facts(&self, relation: Symbol, first: Symbol, accept: impl Fn(Symbol) -> bool) -> Vec<FormulaId> {
        self.index
            .facts(relation, first)
            .filter(|f| match &f.term {
                Term::App(_, args) => matches!(args.as_slice(), [_, Term::Symbol(s)] if accept(*s)),
                _ => false,
            })
            .map(|f| f.id)
            .collect()
    }
}

fn is_ground(term: &Term) -> bool {
    match term {
        Term::Var(_) | Term::RowVar(_) => false,
        Term::App(head, args) => is_ground(head) && args.iter().all(is_ground),
        Term::Logic(_, args) => args.iter().all(is_ground),
        Term::Quant(..) => false,
        Term::Symbol(_) | Term::Str(_) | Term::Num(_) | Term::Nil => true,
    }
}

// 同一個變數出現多次時，各位置的值必須相同
fn unify(pattern: &[Term], values: &[Term]) -> Option<Vec<(Symbol, Term)>> {
    if pattern.len() != values.len() {
        return None;
    }
    let mut bindings: Vec<(Symbol, Term)> = Vec::new();
    for (p, v) in pattern.iter().zip(values) {
        match p {
            Term::Var(name) => match bindings.iter().find(|(n, _)| n == name) {
                Some((_, bound)) if bound != v => return None,
                Some(_) => {}
                None => bindings.push((*name, v.clone())),
            },
            ground if ground != v => return None,
            _ => {}
        }
    }
    Some(bindings)
}

#[cfg(test)]
mod tests {
    use kifedit_sema::{FileId, lower_file};
    use kifedit_syntax::parser::parse;

    use super::*;

    const SRC: &str = "
(subclass Device Artifact)
(subclass Camera Device)
(subclass VideoCamera Camera)
(instance HeadCamera VideoCamera)
(instance GripperCamera Camera)
(instance MyRobot Robot)
(subrelation robotSensingPart robotPart)
(robotPart BaseDrive MyRobot)
(robotSensingPart HeadCamera MyRobot)
(rosTopic HeadCamera \"/head\")
(monetaryValue X (MeasureFn 30 Hertz))
";

    struct Kb {
        interner: Interner,
        index: Index,
        taxonomy: Taxonomy,
    }

    impl Kb {
        fn new() -> Self {
            let interner = Interner::default();
            let mut index = Index::default();
            index.set_file(FileId(0), &parse(SRC).syntax(), &interner);
            let taxonomy = Taxonomy::build(&index, &interner);
            Self {
                interner,
                index,
                taxonomy,
            }
        }

        fn run(&self, query: &str, limit: usize) -> QueryResult {
            let formulas = lower_file(FileId(1), &parse(query).syntax(), &self.interner);
            evaluate(&formulas[0].term, &self.index, &self.taxonomy, &self.interner, limit)
        }

        // 每個答案寫成 "?X=HeadCamera" 這種形式，多個綁定以空格分開
        fn answers(&self, query: &str) -> Vec<String> {
            let QueryResult::Answers(answers) = self.run(query, usize::MAX) else {
                panic!("The query needs a prover: {query}");
            };
            answers
                .iter()
                .map(|a| {
                    let parts: Vec<String> = a
                        .bindings
                        .iter()
                        .map(|(v, t)| format!("{}={}", self.interner.resolve(*v), self.show(t)))
                        .collect();
                    parts.join(" ")
                })
                .collect()
        }

        fn show(&self, term: &Term) -> String {
            match term {
                Term::Symbol(s) | Term::Num(s) | Term::Var(s) | Term::RowVar(s) => {
                    self.interner.resolve(*s).to_string()
                }
                Term::Str(s) => format!("{:?}", self.interner.resolve(*s)),
                Term::App(head, args) => {
                    let mut parts = vec![self.show(head)];
                    parts.extend(args.iter().map(|a| self.show(a)));
                    format!("({})", parts.join(" "))
                }
                other => format!("{other:?}"),
            }
        }
    }

    #[test]
    fn instances_include_instances_of_subclasses() {
        let kb = Kb::new();
        assert_eq!(kb.answers("(instance ?X Camera)"), ["?X=GripperCamera", "?X=HeadCamera"]);
    }

    #[test]
    fn types_include_ancestor_classes() {
        let kb = Kb::new();
        assert_eq!(
            kb.answers("(instance HeadCamera ?C)"),
            ["?C=VideoCamera", "?C=Camera", "?C=Device", "?C=Artifact"]
        );
    }

    #[test]
    fn ground_query_answers_yes_or_no() {
        let kb = Kb::new();
        assert_eq!(kb.answers("(instance HeadCamera Device)"), [""]);
        assert!(kb.answers("(instance HeadCamera Robot)").is_empty());
    }

    #[test]
    fn support_points_to_the_direct_fact() {
        let kb = Kb::new();
        let QueryResult::Answers(answers) = kb.run("(instance HeadCamera Device)", 10) else {
            panic!("The query needs a prover.");
        };
        // (instance HeadCamera VideoCamera) 是第 4 條公式（編號 3）
        assert_eq!(answers[0].support, [FormulaId { file: FileId(0), index: 3 }]);
    }

    #[test]
    fn subclass_uses_the_closure() {
        let kb = Kb::new();
        assert_eq!(kb.answers("(subclass ?A Device)"), ["?A=Camera", "?A=VideoCamera"]);
        assert_eq!(
            kb.answers("(subclass VideoCamera ?B)"),
            ["?B=Camera", "?B=Device", "?B=Artifact"]
        );
    }

    #[test]
    fn relation_includes_facts_of_subrelations() {
        let kb = Kb::new();
        assert_eq!(kb.answers("(robotPart ?X MyRobot)"), ["?X=BaseDrive", "?X=HeadCamera"]);
    }

    #[test]
    fn values_can_be_strings_and_function_terms() {
        let kb = Kb::new();
        assert_eq!(kb.answers("(rosTopic HeadCamera ?T)"), ["?T=\"/head\""]);
        assert_eq!(kb.answers("(monetaryValue X ?V)"), ["?V=(MeasureFn 30 Hertz)"]);
    }

    #[test]
    fn limit_stops_the_answers() {
        let kb = Kb::new();
        let QueryResult::Answers(answers) = kb.run("(instance ?X ?C)", 2) else {
            panic!("The query needs a prover.");
        };
        assert_eq!(answers.len(), 2);
    }

    #[test]
    fn other_queries_need_a_prover() {
        let kb = Kb::new();
        let reason = |q| match kb.run(q, 10) {
            QueryResult::NeedsProver(reason) => reason,
            QueryResult::Answers(_) => panic!("The query does not need a prover: {q}"),
        };
        assert_eq!(reason("(and (a) (b))"), NeedsProver::LogicalOperators);
        assert_eq!(reason("(exists (?X) (p ?X))"), NeedsProver::Quantifier);
        assert_eq!(reason("(?R HeadCamera MyRobot)"), NeedsProver::VariableRelation);
        assert_eq!(reason("(monetaryValue X (MeasureFn ?N Hertz))"), NeedsProver::ComplexArgument);
    }
}

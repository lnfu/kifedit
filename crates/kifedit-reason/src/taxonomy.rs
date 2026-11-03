use kifedit_sema::{Index, Interner, Symbol, Term};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::Hierarchy;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Kinds {
    pub class: bool,
    pub relation: bool,
    pub function: bool,
    pub attribute: bool,
    pub individual: bool,
}

// SUMO 頂層的固定類別
#[derive(Debug, Default)]
struct TopClasses {
    set_or_class: Option<Symbol>,
    relation: Option<Symbol>,
    function: Option<Symbol>,
    attribute: Option<Symbol>,
}

#[derive(Debug, Default)]
pub struct Taxonomy {
    pub subclass: Hierarchy,
    pub subrelation: Hierarchy,
    pub sub_attribute: Hierarchy,
    instance_of: FxHashMap<Symbol, Vec<Symbol>>,
    instances: FxHashMap<Symbol, Vec<Symbol>>,
    top: TopClasses,
}

impl Taxonomy {
    pub fn build(index: &Index, interner: &Interner) -> Self {
        let binary_facts = |relation: &str| -> Vec<(Symbol, Symbol)> {
            let Some(relation) = interner.get(relation) else {
                return Vec::new();
            };
            index
                .facts_of(relation)
                .filter_map(|f| symbol_pair(&f.term))
                .collect()
        };

        let mut taxonomy = Self {
            subclass: Hierarchy::from_edges(binary_facts("subclass")),
            subrelation: Hierarchy::from_edges(binary_facts("subrelation")),
            sub_attribute: Hierarchy::from_edges(binary_facts("subAttribute")),
            top: TopClasses {
                set_or_class: interner.get("SetOrClass"),
                relation: interner.get("Relation"),
                function: interner.get("Function"),
                attribute: interner.get("Attribute"),
            },
            ..Self::default()
        };
        for (individual, class) in binary_facts("instance") {
            push_unique(taxonomy.instance_of.entry(individual).or_default(), class);
            push_unique(taxonomy.instances.entry(class).or_default(), individual);
        }
        taxonomy
    }

    pub fn instance_of(&self, individual: Symbol) -> &[Symbol] {
        self.instance_of.get(&individual).map(Vec::as_slice).unwrap_or_default()
    }

    pub fn instances_of(&self, class: Symbol) -> &[Symbol] {
        self.instances.get(&class).map(Vec::as_slice).unwrap_or_default()
    }

    /// 直接的類別，加上這些類別的所有祖先；不重複，近的在前
    pub fn types_of(&self, individual: Symbol) -> Vec<Symbol> {
        let mut seen = FxHashSet::default();
        let mut out = Vec::new();
        for &class in self.instance_of(individual) {
            let up = std::iter::once(class).chain(self.subclass.ancestors(class).iter().copied());
            out.extend(up.filter(|&c| seen.insert(c)));
        }
        out
    }

    pub fn individuals(&self) -> impl Iterator<Item = Symbol> {
        self.instance_of.keys().copied()
    }

    pub fn kinds(&self, term: Symbol) -> Kinds {
        let types = self.types_of(term);
        let has = |class: Option<Symbol>| class.is_some_and(|c| types.contains(&c));
        let class = self.subclass.contains(term) || has(self.top.set_or_class);
        let relation = self.subrelation.contains(term) || has(self.top.relation);
        let attribute = self.sub_attribute.contains(term) || has(self.top.attribute);
        Kinds {
            class,
            relation,
            function: has(self.top.function),
            attribute,
            individual: !types.is_empty() && !class && !relation && !attribute,
        }
    }
}

fn symbol_pair(term: &Term) -> Option<(Symbol, Symbol)> {
    match term {
        Term::App(_, args) => match args.as_slice() {
            [Term::Symbol(a), Term::Symbol(b)] => Some((*a, *b)),
            _ => None,
        },
        _ => None,
    }
}

fn push_unique(list: &mut Vec<Symbol>, symbol: Symbol) {
    if !list.contains(&symbol) {
        list.push(symbol);
    }
}

#[cfg(test)]
mod tests {
    use kifedit_sema::{FileId, Index, Interner};
    use kifedit_syntax::parser::parse;

    use super::*;

    const SRC: &str = "
(subclass Relation Entity)
(subclass BinaryPredicate Relation)
(subclass Function Relation)
(subclass Attribute Entity)
(subclass Camera Device)
(subclass Device Entity)
(subclass ShutterTypeAttribute Attribute)
(instance HeadCamera Camera)
(instance todoNote BinaryPredicate)
(instance MeasureFn Function)
(instance GlobalShutter ShutterTypeAttribute)
(subrelation robotSensingPart robotPart)
";

    fn build() -> (Interner, Taxonomy) {
        let interner = Interner::default();
        let mut index = Index::default();
        index.set_file(FileId(0), &parse(SRC).syntax(), &interner);
        let taxonomy = Taxonomy::build(&index, &interner);
        (interner, taxonomy)
    }

    #[test]
    fn types_include_ancestors_of_direct_classes() {
        let (interner, taxonomy) = build();
        let types: Vec<_> = taxonomy
            .types_of(interner.get("HeadCamera").unwrap())
            .into_iter()
            .map(|s| interner.resolve(s))
            .collect();
        assert_eq!(types, ["Camera", "Device", "Entity"]);
    }

    #[test]
    fn kinds_follow_sumo_top_classes() {
        let (interner, taxonomy) = build();
        let kinds = |name| taxonomy.kinds(interner.get(name).unwrap());

        assert!(kinds("Camera").class);
        assert!(kinds("HeadCamera").individual);
        assert!(kinds("todoNote").relation && !kinds("todoNote").individual);
        assert!(kinds("MeasureFn").function && kinds("MeasureFn").relation);
        assert!(kinds("GlobalShutter").attribute);
        assert!(kinds("robotSensingPart").relation);
    }

    #[test]
    fn instances_of_lists_direct_instances() {
        let (interner, taxonomy) = build();
        let camera = interner.get("Camera").unwrap();
        let head_camera = interner.get("HeadCamera").unwrap();
        assert_eq!(taxonomy.instances_of(camera), [head_camera]);
        assert!(taxonomy.instances_of(interner.get("Device").unwrap()).is_empty());
    }
}

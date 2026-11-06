use std::collections::VecDeque;

use kifedit_sema::Symbol;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Hierarchy {
    parents: FxHashMap<Symbol, Vec<Symbol>>,
    children: FxHashMap<Symbol, Vec<Symbol>>,
    ancestors: FxHashMap<Symbol, Vec<Symbol>>, // BFS 順序：近的在前
    cycles: Vec<Vec<Symbol>>,
}

impl Hierarchy {
    /// edges 為（子, 父）
    pub fn from_edges(edges: impl IntoIterator<Item = (Symbol, Symbol)>) -> Self {
        let mut hierarchy = Self::default();
        for (child, parent) in edges {
            push_unique(hierarchy.parents.entry(child).or_default(), parent);
            push_unique(hierarchy.children.entry(parent).or_default(), child);
        }
        let nodes: Vec<Symbol> = hierarchy.nodes().collect();
        for &node in &nodes {
            let ancestors = bfs(node, &hierarchy.parents);
            hierarchy.ancestors.insert(node, ancestors);
        }
        hierarchy.cycles = Tarjan::cycles(&hierarchy.parents, &nodes);
        hierarchy
    }

    pub fn nodes(&self) -> impl Iterator<Item = Symbol> {
        let only_parents = self
            .children
            .keys()
            .filter(|s| !self.parents.contains_key(s));
        self.parents.keys().chain(only_parents).copied()
    }

    pub fn len(&self) -> usize {
        self.nodes().count()
    }

    pub fn is_empty(&self) -> bool {
        self.parents.is_empty()
    }

    pub fn contains(&self, symbol: Symbol) -> bool {
        self.parents.contains_key(&symbol) || self.children.contains_key(&symbol)
    }

    pub fn parents(&self, symbol: Symbol) -> &[Symbol] {
        self.parents
            .get(&symbol)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn children(&self, symbol: Symbol) -> &[Symbol] {
        self.children
            .get(&symbol)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn ancestors(&self, symbol: Symbol) -> &[Symbol] {
        self.ancestors
            .get(&symbol)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn descendants(&self, symbol: Symbol) -> Vec<Symbol> {
        bfs(symbol, &self.children)
    }

    pub fn ancestor_pairs(&self) -> usize {
        self.ancestors.values().map(Vec::len).sum()
    }

    pub fn is_ancestor(&self, ancestor: Symbol, symbol: Symbol) -> bool {
        self.ancestors(symbol).contains(&ancestor)
    }

    /// 最近共同祖先：共同祖先（含 a、b 本身）中，不是其他共同祖先的祖先者。依 a 的 BFS 順序
    pub fn common_ancestors(&self, a: Symbol, b: Symbol) -> Vec<Symbol> {
        let in_b: FxHashSet<Symbol> = std::iter::once(b)
            .chain(self.ancestors(b).iter().copied())
            .collect();
        let common: Vec<Symbol> = std::iter::once(a)
            .chain(self.ancestors(a).iter().copied())
            .filter(|s| in_b.contains(s))
            .collect();
        common
            .iter()
            .copied()
            .filter(|&c| !common.iter().any(|&d| d != c && self.is_ancestor(c, d)))
            .collect()
    }

    /// 強連通分量中大小大於 1 或有自我迴圈者，留給 E005
    pub fn cycles(&self) -> &[Vec<Symbol>] {
        &self.cycles
    }
}

fn push_unique(list: &mut Vec<Symbol>, symbol: Symbol) {
    if !list.contains(&symbol) {
        list.push(symbol);
    }
}

// seen 集合讓循環不會造成無窮迴圈
fn bfs(start: Symbol, edges: &FxHashMap<Symbol, Vec<Symbol>>) -> Vec<Symbol> {
    let mut seen = FxHashSet::default();
    let mut queue = VecDeque::from([start]);
    let mut out = Vec::new();
    while let Some(node) = queue.pop_front() {
        for &next in edges.get(&node).into_iter().flatten() {
            if next != start && seen.insert(next) {
                out.push(next);
                queue.push_back(next);
            }
        }
    }
    out
}

struct Tarjan<'a> {
    graph: &'a FxHashMap<Symbol, Vec<Symbol>>,
    index: FxHashMap<Symbol, usize>,
    low: FxHashMap<Symbol, usize>,
    stack: Vec<Symbol>,
    on_stack: FxHashSet<Symbol>,
    cycles: Vec<Vec<Symbol>>,
}

impl<'a> Tarjan<'a> {
    fn cycles(graph: &'a FxHashMap<Symbol, Vec<Symbol>>, nodes: &[Symbol]) -> Vec<Vec<Symbol>> {
        let mut tarjan = Self {
            graph,
            index: FxHashMap::default(),
            low: FxHashMap::default(),
            stack: Vec::new(),
            on_stack: FxHashSet::default(),
            cycles: Vec::new(),
        };
        for &node in nodes {
            if !tarjan.index.contains_key(&node) {
                tarjan.visit(node);
            }
        }
        tarjan.cycles
    }

    fn visit(&mut self, v: Symbol) {
        let n = self.index.len();
        self.index.insert(v, n);
        self.low.insert(v, n);
        self.stack.push(v);
        self.on_stack.insert(v);

        let graph = self.graph;
        for &w in graph.get(&v).into_iter().flatten() {
            if !self.index.contains_key(&w) {
                self.visit(w);
                let low = self.low[&v].min(self.low[&w]);
                self.low.insert(v, low);
            } else if self.on_stack.contains(&w) {
                let low = self.low[&v].min(self.index[&w]);
                self.low.insert(v, low);
            }
        }

        if self.low[&v] == self.index[&v] {
            let mut component = Vec::new();
            loop {
                let w = self.stack.pop().unwrap();
                self.on_stack.remove(&w);
                component.push(w);
                if w == v {
                    break;
                }
            }
            let self_loop = graph.get(&v).is_some_and(|ps| ps.contains(&v));
            if component.len() > 1 || self_loop {
                component.reverse();
                self.cycles.push(component);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use kifedit_sema::Interner;

    use super::*;

    struct Fixture {
        interner: Interner,
        hierarchy: Hierarchy,
    }

    impl Fixture {
        fn new(edges: &[(&str, &str)]) -> Self {
            let interner = Interner::default();
            let edges: Vec<_> = edges
                .iter()
                .map(|(c, p)| (interner.intern(c), interner.intern(p)))
                .collect();
            Self {
                hierarchy: Hierarchy::from_edges(edges),
                interner,
            }
        }

        fn sym(&self, name: &str) -> Symbol {
            self.interner.get(name).unwrap()
        }

        fn names(&self, symbols: &[Symbol]) -> Vec<&str> {
            symbols.iter().map(|&s| self.interner.resolve(s)).collect()
        }
    }

    // Camera 有兩個父類別：多重繼承
    const DEVICES: &[(&str, &str)] = &[
        ("Device", "Artifact"),
        ("Artifact", "Entity"),
        ("Camera", "Device"),
        ("Camera", "ElectricDevice"),
        ("ElectricDevice", "Device"),
        ("Lidar", "ElectricDevice"),
    ];

    #[test]
    fn parents_children_and_ancestors() {
        let f = Fixture::new(DEVICES);
        let camera = f.sym("Camera");
        assert_eq!(
            f.names(f.hierarchy.parents(camera)),
            ["Device", "ElectricDevice"]
        );
        assert_eq!(
            f.names(f.hierarchy.children(f.sym("ElectricDevice"))),
            ["Camera", "Lidar"]
        );
        assert_eq!(
            f.names(f.hierarchy.ancestors(camera)),
            ["Device", "ElectricDevice", "Artifact", "Entity"]
        );
        assert!(f.hierarchy.is_ancestor(f.sym("Entity"), camera));
        assert!(!f.hierarchy.is_ancestor(camera, f.sym("Entity")));
    }

    #[test]
    fn descendants_list_each_node_once() {
        let f = Fixture::new(DEVICES);
        assert_eq!(
            f.names(&f.hierarchy.descendants(f.sym("Device"))),
            ["Camera", "ElectricDevice", "Lidar"]
        );
    }

    #[test]
    fn common_ancestors_are_the_nearest_ones() {
        let f = Fixture::new(DEVICES);
        let common = f
            .hierarchy
            .common_ancestors(f.sym("Camera"), f.sym("Lidar"));
        assert_eq!(f.names(&common), ["ElectricDevice"]);
        // 一方是另一方的祖先時，答案就是它本身
        let common = f
            .hierarchy
            .common_ancestors(f.sym("Device"), f.sym("Lidar"));
        assert_eq!(f.names(&common), ["Device"]);
    }

    #[test]
    fn duplicate_edges_are_ignored() {
        let f = Fixture::new(&[("A", "B"), ("A", "B")]);
        assert_eq!(f.names(f.hierarchy.parents(f.sym("A"))), ["B"]);
        assert_eq!(f.hierarchy.len(), 2);
    }

    #[test]
    fn cycles_and_self_loops_are_reported() {
        let f = Fixture::new(&[("A", "B"), ("B", "C"), ("C", "A"), ("D", "D"), ("E", "A")]);
        let mut cycles: Vec<Vec<&str>> = f.hierarchy.cycles().iter().map(|c| f.names(c)).collect();
        cycles.iter_mut().for_each(|c| c.sort());
        cycles.sort();
        assert_eq!(cycles, [vec!["A", "B", "C"], vec!["D"]]);
        // 循環中的節點仍能取得祖先，且不含自己
        assert_eq!(f.names(f.hierarchy.ancestors(f.sym("E"))), ["A", "B", "C"]);
    }
}

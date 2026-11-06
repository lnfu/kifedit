use std::collections::BTreeMap;
use std::sync::Arc;

use kifedit_syntax::SyntaxNode;
use rustc_hash::FxHashMap;

use crate::{FileId, Formula, FormulaId, Interner, Symbol, Term, lower_file};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct FileIndex {
    formulas: Vec<Formula>,
    mentions: FxHashMap<Symbol, Vec<u32>>,
    by_relation: FxHashMap<Symbol, Vec<u32>>,
    by_first_arg: FxHashMap<(Symbol, Symbol), Vec<u32>>,
}

impl FileIndex {
    pub fn build(file: FileId, root: &SyntaxNode, interner: &Interner) -> Self {
        let formulas = lower_file(file, root, interner);
        let mut index = Self::default();
        let mut symbols = Vec::new();

        for (i, formula) in formulas.iter().enumerate() {
            let i = i as u32;
            symbols.clear();
            collect_symbols(&formula.term, &mut symbols);
            symbols.sort_unstable();
            symbols.dedup();
            for &symbol in &symbols {
                index.mentions.entry(symbol).or_default().push(i);
            }

            if let Term::App(head, args) = &formula.term {
                if let Term::Symbol(relation) = head.as_ref() {
                    index.by_relation.entry(*relation).or_default().push(i);
                    if let Some(Term::Symbol(first)) = args.first() {
                        index
                            .by_first_arg
                            .entry((*relation, *first))
                            .or_default()
                            .push(i);
                    }
                }
            }
        }
        index.formulas = formulas;
        index
    }

    pub fn formulas(&self) -> &[Formula] {
        &self.formulas
    }

    /// 檔案中提及的一般符號（不含變數、字串、數字與邏輯運算子）
    pub fn symbols(&self) -> impl Iterator<Item = Symbol> + '_ {
        self.mentions.keys().copied()
    }

    fn select<'a>(&'a self, indices: Option<&'a Vec<u32>>) -> impl Iterator<Item = &'a Formula> {
        indices
            .into_iter()
            .flatten()
            .map(|&i| &self.formulas[i as usize])
    }
}

// 只收集一般符號；變數、字串、數字不算提及詞彙
fn collect_symbols(term: &Term, out: &mut Vec<Symbol>) {
    match term {
        Term::Symbol(s) => out.push(*s),
        Term::App(head, args) => {
            collect_symbols(head, out);
            args.iter().for_each(|a| collect_symbols(a, out));
        }
        Term::Logic(_, args) => args.iter().for_each(|a| collect_symbols(a, out)),
        Term::Quant(_, _, body) => collect_symbols(body, out),
        Term::Var(_) | Term::RowVar(_) | Term::Str(_) | Term::Num(_) | Term::Nil => {}
    }
}

// FileIndex 實作 Eq，Arc 比較時先比指標，未改變的檔案不必逐項比較
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Index {
    files: BTreeMap<FileId, Arc<FileIndex>>,
}

impl Index {
    pub fn from_files(files: impl IntoIterator<Item = (FileId, Arc<FileIndex>)>) -> Self {
        Self {
            files: files.into_iter().collect(),
        }
    }

    pub fn set_file(&mut self, file: FileId, root: &SyntaxNode, interner: &Interner) {
        self.insert_file(file, FileIndex::build(file, root, interner));
    }

    pub fn insert_file(&mut self, file: FileId, index: FileIndex) {
        self.files.insert(file, Arc::new(index));
    }

    pub fn remove_file(&mut self, file: FileId) {
        self.files.remove(&file);
    }

    pub fn file(&self, file: FileId) -> Option<&FileIndex> {
        self.files.get(&file).map(Arc::as_ref)
    }

    pub fn formula(&self, id: FormulaId) -> Option<&Formula> {
        self.files.get(&id.file)?.formulas.get(id.index as usize)
    }

    pub fn formulas(&self) -> impl Iterator<Item = &Formula> {
        self.files.values().flat_map(|f| f.formulas.iter())
    }

    pub fn mentions(&self, symbol: Symbol) -> impl Iterator<Item = &Formula> {
        self.files
            .values()
            .flat_map(move |f| f.select(f.mentions.get(&symbol)))
    }

    pub fn facts_of(&self, relation: Symbol) -> impl Iterator<Item = &Formula> {
        self.files
            .values()
            .flat_map(move |f| f.select(f.by_relation.get(&relation)))
    }

    pub fn facts(&self, relation: Symbol, first_arg: Symbol) -> impl Iterator<Item = &Formula> {
        self.files
            .values()
            .flat_map(move |f| f.select(f.by_first_arg.get(&(relation, first_arg))))
    }

    pub fn terms(&self) -> Vec<Symbol> {
        let mut seen = rustc_hash::FxHashSet::default();
        for file in self.files.values() {
            seen.extend(file.mentions.keys().copied());
        }
        let mut terms: Vec<Symbol> = seen.into_iter().collect();
        terms.sort_unstable();
        terms
    }

    pub fn formula_count(&self) -> usize {
        self.files.values().map(|f| f.formulas.len()).sum()
    }

    pub fn mention_count(&self) -> usize {
        self.files
            .values()
            .flat_map(|f| f.mentions.values())
            .map(Vec::len)
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use kifedit_syntax::parser::parse;

    use super::*;

    const CAMERA: &str = "(subclass Camera Device)
(documentation Camera EnglishLanguage \"A camera.\")
(=> (instance ?X Camera) (exists (?Y) (part ?Y ?X)))
(subclass Camera Camera)
";

    fn index_of(files: &[&str], interner: &Interner) -> Index {
        let mut index = Index::default();
        for (i, src) in files.iter().enumerate() {
            index.set_file(FileId(i as u32), &parse(src).syntax(), interner);
        }
        index
    }

    fn indices<'a>(formulas: impl Iterator<Item = &'a Formula>) -> Vec<(u32, u32)> {
        formulas.map(|f| (f.id.file.0, f.id.index)).collect()
    }

    #[test]
    fn mentions_list_each_formula_once() {
        let interner = Interner::default();
        let index = index_of(&[CAMERA], &interner);
        let camera = interner.get("Camera").unwrap();
        assert_eq!(
            indices(index.mentions(camera)),
            [(0, 0), (0, 1), (0, 2), (0, 3)]
        );
    }

    #[test]
    fn variables_and_strings_are_not_mentions() {
        let interner = Interner::default();
        let index = index_of(&[CAMERA], &interner);
        assert_eq!(index.mentions(interner.get("?X").unwrap()).count(), 0);
        assert_eq!(
            index.mentions(interner.get("A camera.").unwrap()).count(),
            0
        );
    }

    #[test]
    fn facts_by_relation_and_first_argument() {
        let interner = Interner::default();
        let index = index_of(&[CAMERA], &interner);
        let subclass = interner.get("subclass").unwrap();
        let documentation = interner.get("documentation").unwrap();
        let camera = interner.get("Camera").unwrap();

        assert_eq!(indices(index.facts_of(subclass)), [(0, 0), (0, 3)]);
        assert_eq!(indices(index.facts(documentation, camera)), [(0, 1)]);
    }

    #[test]
    fn queries_cover_all_files_in_file_order() {
        let interner = Interner::default();
        let index = index_of(&["(instance A B)", CAMERA, "(part A Camera)"], &interner);
        let camera = interner.get("Camera").unwrap();

        assert_eq!(index.mentions(camera).count(), 5);
        let last = index.mentions(camera).last().unwrap();
        assert_eq!(index.formula(last.id), Some(last));
        assert_eq!(last.id.file, FileId(2));
    }

    #[test]
    fn set_file_replaces_and_remove_file_deletes() {
        let interner = Interner::default();
        let mut index = index_of(&[CAMERA], &interner);
        let camera = interner.get("Camera").unwrap();

        index.set_file(FileId(0), &parse("(instance A Camera)").syntax(), &interner);
        assert_eq!(index.mentions(camera).count(), 1);

        index.remove_file(FileId(0));
        assert_eq!(index.mentions(camera).count(), 0);
        assert_eq!(index.formula_count(), 0);
    }
}

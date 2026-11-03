use lasso::{Spur, ThreadedRodeo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Symbol(Spur);

#[derive(Debug, Default)]
pub struct Interner(ThreadedRodeo);

impl Interner {
    pub fn intern(&self, text: &str) -> Symbol {
        Symbol(self.0.get_or_intern(text))
    }

    pub fn get(&self, text: &str) -> Option<Symbol> {
        self.0.get(text).map(Symbol)
    }

    pub fn resolve(&self, symbol: Symbol) -> &str {
        self.0.resolve(&symbol.0)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

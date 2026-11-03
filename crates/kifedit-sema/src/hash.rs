use std::hash::{Hash, Hasher};

use rustc_hash::{FxHashMap, FxHasher};

use crate::{Interner, Symbol, Term};

// 依結構計算，與排版無關。符號以文字計算（不用駐留編號），每次執行結果相同；
// 變數依第一次出現的順序重新編號，只改變數名稱的公式雜湊相同。
pub(crate) fn formula_hash(term: &Term, interner: &Interner) -> u64 {
    let mut state = State {
        interner,
        vars: FxHashMap::default(),
        hasher: FxHasher::default(),
    };
    state.term(term);
    state.hasher.finish()
}

struct State<'a> {
    interner: &'a Interner,
    vars: FxHashMap<Symbol, u64>,
    hasher: FxHasher,
}

impl State<'_> {
    fn term(&mut self, term: &Term) {
        match term {
            Term::Symbol(s) => self.text(0, *s),
            Term::Var(s) => self.var(1, *s),
            Term::RowVar(s) => self.var(2, *s),
            Term::Str(s) => self.text(3, *s),
            Term::Num(s) => self.text(4, *s),
            Term::App(head, args) => {
                self.tag(5, args.len());
                self.term(head);
                args.iter().for_each(|a| self.term(a));
            }
            Term::Logic(op, args) => {
                self.tag(6, args.len());
                (*op as u8).hash(&mut self.hasher);
                args.iter().for_each(|a| self.term(a));
            }
            Term::Quant(q, vars, body) => {
                self.tag(7, vars.len());
                (*q as u8).hash(&mut self.hasher);
                vars.iter().for_each(|v| self.var(1, *v));
                self.term(body);
            }
            Term::Nil => self.tag(8, 0),
        }
    }

    fn tag(&mut self, tag: u8, len: usize) {
        tag.hash(&mut self.hasher);
        (len as u64).hash(&mut self.hasher);
    }

    fn text(&mut self, tag: u8, symbol: Symbol) {
        tag.hash(&mut self.hasher);
        self.interner.resolve(symbol).hash(&mut self.hasher);
    }

    fn var(&mut self, tag: u8, symbol: Symbol) {
        let next = self.vars.len() as u64;
        let index = *self.vars.entry(symbol).or_insert(next);
        tag.hash(&mut self.hasher);
        index.hash(&mut self.hasher);
    }
}

use kifedit_db::{File, ProjectInput};
use kifedit_reason::project_taxonomy;
use kifedit_sema::{FormulaId, SemaDb, Symbol, Term, file_index, project_index};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::Diagnostic;

// 這些關係的第 1 參數視為已定義的詞彙（§7.1 E002）
const DECLARING: [&str; 4] = ["instance", "subclass", "subrelation", "subAttribute"];

// 這些詞彙不需要宣告：equal 是 Sigma 的內建等號；Entity 是根類別
const ALWAYS_DEFINED: [&str; 2] = ["equal", "Entity"];

/// 查詢的鍵：一個詞彙
#[salsa::interned(debug)]
pub struct TermKey<'db> {
    #[returns(copy)]
    pub symbol: Symbol,
}

/// 投影：檔案中被宣告的詞彙，排序且不含位置
#[salsa::tracked(returns(ref))]
pub fn file_declarations(db: &dyn SemaDb, file: File) -> Vec<Symbol> {
    let declaring: Vec<Symbol> = DECLARING.iter().filter_map(|r| db.interner().get(r)).collect();
    let mut out: Vec<Symbol> = file_index(db, file)
        .formulas()
        .iter()
        .filter_map(|f| match &f.term {
            Term::App(head, args) => match (head.as_ref(), args.first()) {
                (Term::Symbol(r), Some(Term::Symbol(first))) if declaring.contains(r) => Some(*first),
                _ => None,
            },
            _ => None,
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[salsa::tracked(returns(ref))]
pub fn declared_terms(db: &dyn SemaDb, project: ProjectInput) -> FxHashSet<Symbol> {
    project
        .files(db)
        .iter()
        .flat_map(|&file| file_declarations(db, file).iter().copied())
        .collect()
}

/// 依詞彙細分：宣告集合改變時重算，但結果不變時依賴它的檢查不重跑
#[salsa::tracked(returns(copy))]
pub fn declared<'db>(db: &'db dyn SemaDb, project: ProjectInput, term: TermKey<'db>) -> bool {
    let symbol = term.symbol(db);
    declared_terms(db, project).contains(&symbol)
        || ALWAYS_DEFINED.iter().any(|t| db.interner().get(t) == Some(symbol))
}

/// 關係的參數簽名（§7.1 E003、E004）；每項記下來源公式，診斷可以連回去
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Signature {
    pub arity: Option<usize>,
    pub domains: Vec<Constraint>,
    pub range: Option<Constraint>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Constraint {
    pub position: usize, // 參數位置，從 1 開始；range 為 0
    pub class: Symbol,
    pub subclass: bool, // domainSubclass／rangeSubclass：參數是類別本身
    pub formula: FormulaId,
}

#[salsa::tracked(returns(ref))]
pub fn signature<'db>(db: &'db dyn SemaDb, project: ProjectInput, relation: TermKey<'db>) -> Signature {
    let interner = db.interner();
    let index = project_index(db, project);
    let relation = relation.symbol(db);
    let collect = |name: &str, subclass: bool, out: &mut Vec<Constraint>| {
        let Some(rel) = interner.get(name) else { return };
        for f in index.facts(rel, relation) {
            let Term::App(_, args) = &f.term else { continue };
            let (position, class) = match args.as_slice() {
                [_, Term::Num(n), Term::Symbol(class)] => match interner.resolve(*n).parse() {
                    Ok(n) => (n, *class),
                    Err(_) => continue,
                },
                [_, Term::Symbol(class)] => (0, *class),
                _ => continue,
            };
            out.push(Constraint { position, class, subclass, formula: f.id });
        }
    };
    let mut domains = Vec::new();
    collect("domain", false, &mut domains);
    collect("domainSubclass", true, &mut domains);
    domains.sort_unstable();
    let mut range = Vec::new();
    collect("range", false, &mut range);
    collect("rangeSubclass", true, &mut range);
    range.sort_unstable();
    Signature {
        arity: domains.iter().map(|c| c.position).max(),
        domains,
        range: range.first().copied(),
    }
}

/// 一個個體的類別：直接的類別，以及加上祖先後的全部類別（排序）
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TermTypes {
    pub direct: Vec<Symbol>,
    pub all: Vec<Symbol>,
}

#[salsa::tracked(returns(ref))]
pub fn term_types<'db>(db: &'db dyn SemaDb, project: ProjectInput, term: TermKey<'db>) -> TermTypes {
    let taxonomy = project_taxonomy(db, project);
    let symbol = term.symbol(db);
    let mut all = taxonomy.types_of(symbol);
    all.sort_unstable();
    TermTypes {
        direct: taxonomy.instance_of(symbol).to_vec(),
        all,
    }
}

/// 類別本身加上所有 subclass 祖先（排序）
#[salsa::tracked(returns(ref))]
pub fn class_ancestors<'db>(db: &'db dyn SemaDb, project: ProjectInput, class: TermKey<'db>) -> Vec<Symbol> {
    let symbol = class.symbol(db);
    let mut out = project_taxonomy(db, project).subclass.ancestors(symbol).to_vec();
    out.push(symbol);
    out.sort_unstable();
    out
}

/// 三種階層中的循環（§7.1 E005）
#[salsa::tracked(returns(ref))]
pub fn hierarchy_cycles(db: &dyn SemaDb, project: ProjectInput) -> Vec<(&'static str, Vec<Symbol>)> {
    let taxonomy = project_taxonomy(db, project);
    [
        ("subclass", &taxonomy.subclass),
        ("subrelation", &taxonomy.subrelation),
        ("subAttribute", &taxonomy.sub_attribute),
    ]
    .into_iter()
    .flat_map(|(name, h)| h.cycles().iter().map(move |c| (name, c.clone())))
    .collect()
}

/// 投影：檔案中的互斥類別對（disjoint、partition、disjointDecomposition），依位置以外的內容比較
#[salsa::tracked(returns(ref))]
pub fn file_disjoint_pairs(db: &dyn SemaDb, file: File) -> Vec<(Symbol, Symbol, FormulaId)> {
    let interner = db.interner();
    let [disjoint, partition, decomposition] =
        ["disjoint", "partition", "disjointDecomposition"].map(|r| interner.get(r));
    let mut out = Vec::new();
    for f in file_index(db, file).formulas() {
        let Term::App(head, args) = &f.term else { continue };
        let Term::Symbol(relation) = head.as_ref() else { continue };
        let classes: &[Term] = match Some(*relation) {
            r if r == disjoint => args,
            r if r == partition || r == decomposition => args.get(1..).unwrap_or_default(),
            _ => continue,
        };
        let classes: Vec<Symbol> = classes
            .iter()
            .filter_map(|t| match t {
                Term::Symbol(s) => Some(*s),
                _ => None,
            })
            .collect();
        for (i, &a) in classes.iter().enumerate() {
            for &b in &classes[i + 1..] {
                if a != b {
                    out.push((a.min(b), a.max(b), f.id));
                }
            }
        }
    }
    out
}

#[salsa::tracked(returns(ref))]
pub fn disjoint_pairs(db: &dyn SemaDb, project: ProjectInput) -> FxHashMap<(Symbol, Symbol), FormulaId> {
    let mut out = FxHashMap::default();
    for &file in project.files(db) {
        for &(a, b, formula) in file_disjoint_pairs(db, file) {
            out.entry((a, b)).or_insert(formula);
        }
    }
    out
}

/// 詞彙是否有 documentation（§7.1 W002）
#[salsa::tracked(returns(copy))]
pub fn has_documentation<'db>(db: &'db dyn SemaDb, project: ProjectInput, term: TermKey<'db>) -> bool {
    let Some(documentation) = db.interner().get("documentation") else { return false };
    project_index(db, project).facts(documentation, term.symbol(db)).next().is_some()
}

/// 詞彙是否被「不是描述它本身」的公式提及（§7.1 I001）。
/// 描述它本身：它是第 1 參數（termFormat 為第 2 參數）的事實
#[salsa::tracked(returns(copy))]
pub fn referenced<'db>(db: &'db dyn SemaDb, project: ProjectInput, term: TermKey<'db>) -> bool {
    let interner = db.interner();
    let symbol = term.symbol(db);
    project_index(db, project).mentions(symbol).any(|f| match &f.term {
        Term::App(head, args) => {
            let position = match head.as_ref() {
                Term::Symbol(r) if interner.resolve(*r) == "termFormat" => 1,
                _ => 0,
            };
            !matches!(args.get(position), Some(Term::Symbol(s)) if *s == symbol)
        }
        _ => true,
    })
}

/// 一個檔案的所有診斷，依位置排序；只依賴本檔的公式與它用到的詞彙的查詢
#[salsa::tracked(returns(ref))]
pub fn check_file(db: &dyn SemaDb, project: ProjectInput, file: File) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    crate::rules::check(db, project, file, &mut out);
    out.sort_by_key(|d| (d.range.start(), d.code));
    out
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use kifedit_sema::{FileId, Interner};
    use salsa::Setter;

    use super::*;

    #[salsa::db]
    #[derive(Clone)]
    struct TestDb {
        storage: salsa::Storage<Self>,
        interner: Arc<Interner>,
        executed: Arc<Mutex<Vec<String>>>,
    }

    impl Default for TestDb {
        fn default() -> Self {
            let executed = Arc::new(Mutex::new(Vec::new()));
            let log = executed.clone();
            Self {
                storage: salsa::Storage::new(Some(Box::new(move |event: salsa::Event| {
                    if let salsa::EventKind::WillExecute { database_key } = event.kind {
                        log.lock().unwrap().push(format!("{database_key:?}"));
                    }
                }))),
                interner: Arc::default(),
                executed,
            }
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl kifedit_db::Db for TestDb {}

    #[salsa::db]
    impl SemaDb for TestDb {
        fn interner(&self) -> &Interner {
            &self.interner
        }
    }

    impl TestDb {
        /// 上次呼叫之後重新執行的 check_file，以檔案的 salsa ID 表示
        fn checked_files(&self) -> Vec<String> {
            let executed = std::mem::take(&mut *self.executed.lock().unwrap());
            executed.into_iter().filter(|k| k.starts_with("check_file")).collect()
        }
    }

    const CAMERAS: &str = "(instance instance Entity)\n(instance subclass Entity)\n(subclass Camera Device)\n(subclass Device Entity)\n(instance Entity Entity)\n";
    const ROOMS: &str = "(subclass Room Entity)\n(instance Room Entity)\n(located Lab Room)\n";
    const LAB: &str = "(instance HeadCamera Camera)\n(todoNote HeadCamera \"Check the lens.\")\n";

    struct Fixture {
        db: TestDb,
        files: [File; 3],
        project: ProjectInput,
    }

    impl Fixture {
        fn new() -> Self {
            let db = TestDb::default();
            let files = [CAMERAS, ROOMS, LAB]
                .iter()
                .enumerate()
                .map(|(i, text)| File::new(&db, FileId(i as u32), text.to_string()))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            let project = ProjectInput::new(&db, Vec::from(files));
            let fixture = Self { db, files, project };
            fixture.check_all();
            fixture.db.checked_files();
            fixture
        }

        fn check_all(&self) -> Vec<Vec<String>> {
            self.files
                .iter()
                .map(|&f| {
                    check_file(&self.db, self.project, f)
                        .iter()
                        .filter(|d| d.code == "E002")
                        .map(|d| format!("{} {:?}", d.code, d.range))
                        .collect()
                })
                .collect()
        }

        fn set(&mut self, file: usize, text: &str) {
            self.files[file].set_text(&mut self.db).to(text.to_string());
        }
    }

    #[test]
    fn undefined_terms_are_reported_at_each_use() {
        let fixture = Fixture::new();
        let all = fixture.check_all();
        assert!(all[0].is_empty());
        // ROOMS：located 與 Lab 未定義
        assert_eq!(all[1], ["E002 47..54", "E002 55..58"]);
        // LAB：todoNote 未定義；HeadCamera 由本檔宣告
        assert_eq!(all[2], ["E002 30..38"]);
    }

    #[test]
    fn edit_of_a_note_checks_only_the_edited_file() {
        let mut fixture = Fixture::new();
        fixture.set(2, &LAB.replace("Check the lens.", "Check the lens and the cable."));
        fixture.check_all();
        assert_eq!(fixture.db.checked_files().len(), 1);
    }

    #[test]
    fn new_declaration_checks_only_the_files_that_use_the_term() {
        let mut fixture = Fixture::new();
        fixture.set(2, &format!("{LAB}(instance Lab Room)\n"));
        let all = fixture.check_all();
        // LAB 本身與使用 Lab 的 ROOMS 重查；CAMERAS 不重查
        assert_eq!(fixture.db.checked_files().len(), 2);
        assert_eq!(all[1], ["E002 47..54"]);
    }
}

use kifedit_sema::{File, ProjectInput, SemaDb, file_index};

use crate::{Taxonomy, TaxonomyFacts};

/// 投影查詢：每次修改都重算，但結果不變時 project_taxonomy 不會重建
#[salsa::tracked(returns(ref))]
pub fn taxonomy_facts(db: &dyn SemaDb, file: File) -> TaxonomyFacts {
    TaxonomyFacts::collect(file_index(db, file).formulas(), db.interner())
}

#[salsa::tracked(returns(ref))]
pub fn project_taxonomy(db: &dyn SemaDb, project: ProjectInput) -> Taxonomy {
    let facts: Vec<TaxonomyFacts> = project
        .files(db)
        .iter()
        .map(|&file| taxonomy_facts(db, file).clone())
        .collect();
    Taxonomy::from_facts(&facts, db.interner())
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
    impl kifedit_sema::SemaDb for TestDb {
        fn interner(&self) -> &Interner {
            &self.interner
        }
    }

    #[salsa::db]
    impl kifedit_sema::Db for TestDb {}

    impl TestDb {
        fn executed(&self, query: &str) -> bool {
            let executed = std::mem::take(&mut *self.executed.lock().unwrap());
            executed.iter().any(|key| key.starts_with(query))
        }
    }

    const LIBRARY: &str = "(subclass Camera Device)\n";
    const LAB: &str = "(instance HeadCamera Camera)\n(todoNote HeadCamera \"Check the lens.\")\n";

    fn setup() -> (TestDb, File, ProjectInput) {
        let db = TestDb::default();
        let library = File::new(&db, FileId(0), LIBRARY.to_string());
        let lab = File::new(&db, FileId(1), LAB.to_string());
        let project = ProjectInput::new(&db, vec![library, lab]);
        project_taxonomy(&db, project);
        db.executed("");
        (db, lab, project)
    }

    #[test]
    fn edit_outside_taxonomy_facts_does_not_rebuild_the_taxonomy() {
        let (mut db, lab, project) = setup();
        lab.set_text(&mut db)
            .to(LAB.replace("Check the lens.", "Check the lens and the cable."));
        let taxonomy = project_taxonomy(&db, project);
        assert!(!db.executed("project_taxonomy"));

        let interner = db.interner.clone();
        let camera = interner.get("HeadCamera").unwrap();
        assert_eq!(taxonomy.types_of(camera).len(), 2);
    }

    #[test]
    fn edit_of_a_taxonomy_fact_rebuilds_the_taxonomy() {
        let (mut db, lab, project) = setup();
        lab.set_text(&mut db).to(LAB.replace("Camera)", "Device)"));
        let taxonomy = project_taxonomy(&db, project);
        assert!(db.executed("project_taxonomy"));

        let camera = db.interner.get("HeadCamera").unwrap();
        assert_eq!(taxonomy.types_of(camera).len(), 1);
    }
}

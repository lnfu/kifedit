//! salsa 資料庫的基礎層：input 與解析查詢（對應 rust-analyzer 的 base-db）

use kifedit_syntax::parser::Parse;

#[salsa::db]
pub trait Db: salsa::Database {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

/// 一個檔案的文字；路徑與角色屬於專案，不放在這裡
#[salsa::input(debug)]
pub struct File {
    #[returns(copy)]
    pub id: FileId,
    #[returns(ref)]
    pub text: String,
}

/// 專案中的所有檔案，依 FileId 排序
#[salsa::input(debug)]
pub struct ProjectInput {
    #[returns(ref)]
    pub files: Vec<File>,
}

/// 文字改變時整份重新解析（§4.4）
#[salsa::tracked(returns(ref))]
pub fn parse(db: &dyn Db, file: File) -> Parse {
    kifedit_syntax::parser::parse(file.text(db))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use salsa::Setter;

    use super::*;

    #[salsa::db]
    #[derive(Clone)]
    struct TestDb {
        storage: salsa::Storage<Self>,
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
                executed,
            }
        }
    }

    #[salsa::db]
    impl salsa::Database for TestDb {}

    #[salsa::db]
    impl Db for TestDb {}

    impl TestDb {
        fn take_executed(&self) -> Vec<String> {
            std::mem::take(&mut self.executed.lock().unwrap())
        }
    }

    #[test]
    fn parse_runs_again_only_when_the_text_changes() {
        let mut db = TestDb::default();
        let file = File::new(&db, FileId(0), "(subclass A B)\n".to_string());
        assert!(parse(&db, file).errors.is_empty());
        assert_eq!(db.take_executed().len(), 1);

        parse(&db, file);
        assert!(db.take_executed().is_empty());

        file.set_text(&mut db).to("(subclass A\n".to_string());
        assert_eq!(parse(&db, file).errors.len(), 1);
        assert_eq!(db.take_executed().len(), 1);
    }
}

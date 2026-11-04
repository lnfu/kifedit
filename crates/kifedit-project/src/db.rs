use std::fmt;
use std::sync::Arc;

use kifedit_sema::{Interner, SemaDb};

/// 專案的 salsa 資料庫；複本共用同一份儲存與符號表，可交給其他執行緒讀取
#[salsa::db]
#[derive(Clone, Default)]
pub struct RootDatabase {
    storage: salsa::Storage<Self>,
    interner: Arc<Interner>,
}

#[salsa::db]
impl salsa::Database for RootDatabase {}

#[salsa::db]
impl kifedit_db::Db for RootDatabase {}

#[salsa::db]
impl SemaDb for RootDatabase {
    fn interner(&self) -> &Interner {
        &self.interner
    }
}

impl fmt::Debug for RootDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RootDatabase").finish_non_exhaustive()
    }
}

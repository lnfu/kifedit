use std::sync::Arc;

use kifedit_db::{Db, File, ProjectInput};

use crate::{FileIndex, Index, Interner};

/// 語意層的資料庫：在基礎層之上提供符號駐留（rust-analyzer 的分層 db 慣例）
#[salsa::db]
pub trait SemaDb: Db {
    fn interner(&self) -> &Interner;
}

/// 文字改變時重建；結果相同時下游查詢沿用舊值
#[salsa::tracked]
pub fn file_index(db: &dyn SemaDb, file: File) -> Arc<FileIndex> {
    let parse = kifedit_db::parse(db, file);
    Arc::new(FileIndex::build(
        file.id(db),
        &parse.syntax(),
        db.interner(),
    ))
}

/// 整個專案的索引；只組合各檔的 Arc，不複製內容
#[salsa::tracked(returns(ref))]
pub fn project_index(db: &dyn SemaDb, project: ProjectInput) -> Index {
    Index::from_files(
        project
            .files(db)
            .iter()
            .map(|&file| (file.id(db), file_index(db, file).clone())),
    )
}

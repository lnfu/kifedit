use std::path::Path;
use std::time::Instant;

use kifedit_sema::{FileId, Index, Interner, search_names};
use kifedit_syntax::parser::parse;

#[test]
fn search_speed_on_sumo() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/sumo");
    let mut paths: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("Cannot read the directory {}: {e}", root.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "kif"))
        .collect();
    paths.sort();

    let interner = Interner::default();
    let mut index = Index::default();
    for (i, path) in paths.iter().enumerate() {
        let src = std::fs::read_to_string(path).unwrap();
        index.set_file(FileId(i as u32), &parse(&src).syntax(), &interner);
    }

    for query in ["camera", "Cam", "subcls", "zzzzqq"] {
        let start = Instant::now();
        let found = search_names(&index, &interner, query, 50);
        let elapsed = start.elapsed();
        let top: Vec<_> = found
            .iter()
            .take(5)
            .map(|m| interner.resolve(m.symbol))
            .collect();
        eprintln!(
            "Name search {query:?}: {} results in {elapsed:?}. Top: {top:?}",
            found.len()
        );
    }

    let camera = interner.get("Camera").unwrap();
    assert_eq!(
        search_names(&index, &interner, "camera", 1)[0].symbol,
        camera
    );
}

use std::path::Path;

use kifedit_sema::{FileId, Index, Interner};
use kifedit_syntax::SyntaxKind;
use kifedit_syntax::parser::parse;

#[test]
fn index_covers_all_top_level_forms_in_sumo() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/sumo");
    let mut paths: Vec<_> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("Cannot read the directory {}: {e}", root.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "kif"))
        .collect();
    paths.sort();

    let interner = Interner::default();
    let mut index = Index::default();
    let mut top_level_forms = 0;
    for (i, path) in paths.iter().enumerate() {
        let src = std::fs::read_to_string(path).unwrap();
        let tree = parse(&src).syntax();
        top_level_forms += tree
            .children()
            .filter(|n| n.kind() == SyntaxKind::TopLevelForm)
            .count();
        index.set_file(FileId(i as u32), &tree, &interner);
    }

    assert_eq!(index.formula_count(), top_level_forms);
    eprintln!(
        "Files: {}. Formulas: {}. Mention entries: {}. Symbols: {}.",
        paths.len(),
        index.formula_count(),
        index.mention_count(),
        interner.len()
    );
}

mod common;

use kifedit_syntax::{SyntaxKind, parser::parse};

#[test]
fn cst_is_lossless_on_sumo() {
    let root = common::sumo_root();
    let files = common::kif_files(&root);
    let mut top_level_files = 0;
    let mut top_level_forms = 0;

    for path in &files {
        let src = std::fs::read_to_string(path).unwrap();
        let parsed = parse(&src);
        let tree = parsed.syntax();

        assert!(
            tree.to_string() == src,
            "The CST text is not the same as the file text: {}",
            path.display()
        );

        if !parsed.errors.is_empty() {
            eprintln!("{}: {} syntax errors", path.display(), parsed.errors.len());
            for error in parsed.errors.iter().take(3) {
                let start = usize::from(error.range.start());
                let line = src[..start].matches('\n').count() + 1;
                eprintln!("  line {line}: {} {}", error.kind.code(), error.kind.message());
            }
        }

        if path.parent() == Some(root.as_path()) {
            top_level_files += 1;
            top_level_forms += tree
                .children()
                .filter(|n| n.kind() == SyntaxKind::TopLevelForm)
                .count();
        }
    }
    eprintln!("CST round-trip test: {} files, 0 failures.", files.len());
    eprintln!("Found {top_level_forms} top-level forms in {top_level_files} files in data/sumo.");
}

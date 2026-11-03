use kifedit_syntax::{SyntaxKind, lexer::lex};
use std::path::{Path, PathBuf};

fn kif_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            kif_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "kif") {
            out.push(path);
        }
    }
}

#[test]
fn lexer_is_lossless_on_sumo() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/sumo");
    assert!(
        root.is_dir(),
        "Cannot find the directory {}. Clone the SUMO repository into this directory.",
        root.display()
    );

    let mut files = Vec::new();
    kif_files(&root, &mut files);

    for path in &files {
        let src = std::fs::read_to_string(path).unwrap();
        let tokens = lex(&src);
        let rebuilt: String = tokens.iter().map(|t| t.text).collect();

        assert!(
            rebuilt == src,
            "The joined token text is not the same as the file text: {}",
            path.display()
        );

        let errors = tokens
            .iter()
            .filter(|t| t.kind == SyntaxKind::ErrorToken)
            .count();
        if errors > 0 {
            eprintln!("{}: {errors} error tokens", path.display());
        }
    }
    eprintln!("Round-trip test: {} files, 0 failures.", files.len());
}

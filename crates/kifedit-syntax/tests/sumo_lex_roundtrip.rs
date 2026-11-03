mod common;

use kifedit_syntax::{SyntaxKind, lexer::lex};

#[test]
fn lexer_is_lossless_on_sumo() {
    let files = common::kif_files(&common::sumo_root());

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

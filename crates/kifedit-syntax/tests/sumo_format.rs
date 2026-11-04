mod common;

use std::collections::HashMap;
use std::path::Path;

use kifedit_syntax::parser::parse;
use kifedit_syntax::{FormatOptions, SyntaxKind, format_file};

#[derive(Default)]
struct Stats {
    files: usize,
    changed_files: usize,
    changed_lines: usize,
    lone_closing_parens: usize,
}

#[test]
fn formatter_keeps_semantics_on_sumo() {
    let mut stats = Stats::default();
    for path in common::kif_files(&common::sumo_root()) {
        let src = std::fs::read_to_string(&path).unwrap();
        check_file(&path, &src, &mut stats);
    }
    report("SUMO", &stats);
}

#[test]
fn formatter_keeps_semantics_on_mm() {
    let path = common::sumo_root().join("../fixtures/mm.kif");
    let Ok(src) = std::fs::read_to_string(&path) else {
        eprintln!("Skip the test. Cannot find {}.", path.display());
        return;
    };
    let mut stats = Stats::default();
    let out = check_file(&path, &src, &mut stats);
    report("mm.kif", &stats);

    // 印出前 15 行被改動的輸出，方便人工檢查
    for line in new_lines(&src, &out).into_iter().take(15) {
        eprintln!("  + {line}");
    }
}

fn check_file(path: &Path, src: &str, stats: &mut Stats) -> String {
    let options = FormatOptions::default();
    let out = format_file(&parse(src).syntax(), &options);

    let before = significant_tokens(src);
    let after = significant_tokens(&out);
    if let Some(i) = (0..before.len().max(after.len())).find(|&i| before.get(i) != after.get(i)) {
        panic!(
            "The formatter changes the tokens in {}. First difference at token {i}: {:?} -> {:?}.",
            path.display(),
            before.get(i),
            after.get(i)
        );
    }

    let errors_before = parse(src).errors.len();
    let errors_after = parse(&out).errors.len();
    assert_eq!(
        errors_before,
        errors_after,
        "The formatter changes the number of syntax errors in {}.",
        path.display()
    );

    let again = format_file(&parse(&out).syntax(), &options);
    assert!(
        again == out,
        "The formatter output changes when you format it again: {}",
        path.display()
    );

    let changed = new_lines(src, &out).len();
    stats.files += 1;
    stats.changed_files += usize::from(changed > 0);
    stats.changed_lines += changed;
    stats.lone_closing_parens += out.lines().filter(|l| l.trim() == ")").count();
    out
}

// 字串內的 \r\n 視為與 \n 相同（CRLF 檔案的輸出會統一換行風格）；註解的行尾空白不算語意
fn significant_tokens(src: &str) -> Vec<(SyntaxKind, String)> {
    parse(src)
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| t.kind() != SyntaxKind::Whitespace)
        .map(|t| {
            let text = match t.kind() {
                SyntaxKind::Comment => t.text().trim_end(),
                _ => t.text(),
            };
            (t.kind(), text.replace("\r\n", "\n"))
        })
        .collect()
}

// 輸出中不在原文裡的行（以多重集合比較），約等於被改動的行數
fn new_lines<'a>(before: &str, after: &'a str) -> Vec<&'a str> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for line in before.lines() {
        *counts.entry(line).or_default() += 1;
    }
    let mut added = Vec::new();
    for line in after.lines() {
        match counts.get_mut(line) {
            Some(n) if *n > 0 => *n -= 1,
            _ => added.push(line),
        }
    }
    added
}

fn report(name: &str, stats: &Stats) {
    eprintln!(
        "{name}: formatted {} files. {} files change. {} lines change. {} lines contain only a closing parenthesis.",
        stats.files, stats.changed_files, stats.changed_lines, stats.lone_closing_parens
    );
}

mod doc;

use rowan::NodeOrToken;

use crate::SyntaxKind::{self, *};
use crate::{SyntaxElement, SyntaxNode, SyntaxToken};
use doc::{Doc, PrintOptions, concat, group, indent, print, text, verbatim};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatOptions {
    pub line_width: usize,
    pub indent: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            line_width: 100,
            indent: 2,
        }
    }
}

// R1: at most 2 blank lines between top-level forms
const MAX_NEWLINES: usize = 3;

pub fn format_file(file: &SyntaxNode, options: &FormatOptions) -> String {
    let mut docs = Vec::new();
    let mut pending_newlines = 0;
    let mut has_item = false;

    for element in file.children_with_tokens() {
        if element.kind() == Whitespace {
            pending_newlines += count_newlines(&element_text(&element));
            continue;
        }
        let (doc, trailing_newlines) = top_level_doc(&element);
        if has_item {
            if pending_newlines == 0 && element.kind() == Comment {
                docs.push(text(" "));
            } else {
                let n = pending_newlines.clamp(1, MAX_NEWLINES);
                docs.extend(std::iter::repeat_n(Doc::HardLine, n));
            }
        }
        docs.push(doc);
        has_item = true;
        pending_newlines = trailing_newlines;
    }

    let options = PrintOptions {
        line_width: options.line_width,
        indent: options.indent,
    };
    let mut out = print(&concat(docs), options);
    if has_item {
        out.push('\n');
    }
    // 與 rustfmt 的 newline_style = "Auto" 相同：沿用檔案中第一個換行的風格
    if uses_crlf(&file.text().to_string()) {
        out = out.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    out
}

// 回傳（文件, 節點尾端被移出的換行數）
fn top_level_doc(element: &SyntaxElement) -> (Doc, usize) {
    match element {
        NodeOrToken::Node(node) if node.kind() == Error => {
            let text = node.text().to_string();
            match node.last_token() {
                Some(last) if last.kind() == Whitespace => {
                    let kept = &text[..text.len() - last.text().len()];
                    (verbatim(kept), count_newlines(last.text()))
                }
                _ => (verbatim(text), 0),
            }
        }
        _ => (element_doc(element), 0),
    }
}

struct Item {
    kind: SyntaxKind,
    word: Option<String>,
    doc: Doc,
    newline_before: bool,
}

fn list_doc(node: &SyntaxNode) -> Doc {
    let items = collect_items(node);
    if items.is_empty() {
        return text("()");
    }
    let last_is_string = items.len() >= 2 && items[items.len() - 1].kind == Str;
    match items[0].word.as_deref() {
        Some("=>" | "<=>" | "and" | "or") => broken_list(&items, 1),
        Some("forall" | "exists") if items.len() >= 3 && items[1].kind == List => {
            broken_list(&items, 2)
        }
        _ if last_is_string => string_last_list(&items),
        _ => fill_list(&items),
    }
}

// R2: `(head arg ...)`：放不下時，首項之後每個參數一行
// R3: `(forall (vars) body)`：放不下時，首項與變數留在首行，body 換行
fn broken_list(items: &[Item], keep: usize) -> Doc {
    let mut first_line = vec![text("("), items[0].doc.clone()];
    for i in 1..keep {
        first_line.push(separator(items, i, text(" ")));
        first_line.push(items[i].doc.clone());
    }
    let mut body = Vec::new();
    for i in keep..items.len() {
        body.push(separator(items, i, Doc::HardLine));
        body.push(items[i].doc.clone());
    }
    concat(vec![
        concat(first_line),
        indent(concat(body)),
        closing(items),
    ])
}

// R4: if it fits, print on one line; otherwise, each argument after the head is on its own line
fn fill_list(items: &[Item]) -> Doc {
    group(vec![
        text("("),
        items[0].doc.clone(),
        indent(rest(items, 1, items.len())),
        closing(items),
    ])
}

// R4: if it fits, print on one line; otherwise, each argument after the head is on its own line
// R6: if the last argument is a string, it must be on its own line
fn string_last_list(items: &[Item]) -> Doc {
    let last = items.len() - 1;
    let prefix = group(vec![
        text("("),
        items[0].doc.clone(),
        indent(rest(items, 1, last)),
    ]);
    let before_string = if items[last].newline_before {
        Doc::HardLine
    } else {
        separator(items, last, Doc::Line)
    };
    group(vec![
        prefix,
        indent(concat(vec![before_string, items[last].doc.clone()])),
        closing(items),
    ])
}

fn rest(items: &[Item], from: usize, to: usize) -> Doc {
    let mut docs = Vec::new();
    for i in from..to {
        docs.push(separator(items, i, Doc::Line));
        docs.push(items[i].doc.clone());
    }
    concat(docs)
}

// 註解之後一定換行；同一行的註解前面只放一個空格
fn separator(items: &[Item], i: usize, default: Doc) -> Doc {
    if items[i].kind == Comment && !items[i].newline_before {
        text(" ")
    } else if items[i - 1].kind == Comment {
        Doc::HardLine
    } else {
        default
    }
}

// 最後一個項目是註解時，`)` 必須換行，否則會變成註解的一部分
fn closing(items: &[Item]) -> Doc {
    if items.last().is_some_and(|item| item.kind == Comment) {
        concat(vec![Doc::HardLine, text(")")])
    } else {
        text(")")
    }
}

fn collect_items(node: &SyntaxNode) -> Vec<Item> {
    let mut items = Vec::new();
    let mut newline_before = false;
    for element in node.children_with_tokens() {
        match element.kind() {
            LParen | RParen => {}
            Whitespace => newline_before |= element_text(&element).contains('\n'),
            kind => {
                items.push(Item {
                    kind,
                    word: (kind == Word).then(|| element_text(&element)),
                    doc: element_doc(&element),
                    newline_before,
                });
                newline_before = false;
            }
        }
    }
    items
}

fn element_doc(element: &SyntaxElement) -> Doc {
    match element {
        NodeOrToken::Node(node) if matches!(node.kind(), TopLevelForm | List) => list_doc(node),
        NodeOrToken::Node(node) => verbatim(node.text().to_string()),
        NodeOrToken::Token(token) => token_doc(token),
    }
}

fn token_doc(token: &SyntaxToken) -> Doc {
    let s = token.text();
    if token.kind() == Comment {
        // 與 rustfmt、gofmt、Prettier 相同：去掉註解的行尾空白
        text(s.trim_end())
    } else if s.contains('\n') {
        verbatim(s)
    } else {
        text(s)
    }
}

fn element_text(element: &SyntaxElement) -> String {
    match element {
        NodeOrToken::Node(node) => node.text().to_string(),
        NodeOrToken::Token(token) => token.text().to_string(),
    }
}

fn count_newlines(s: &str) -> usize {
    s.bytes().filter(|&b| b == b'\n').count()
}

fn uses_crlf(s: &str) -> bool {
    s.find('\n')
        .is_some_and(|i| i > 0 && s.as_bytes()[i - 1] == b'\r')
}

#[cfg(test)]
mod tests;

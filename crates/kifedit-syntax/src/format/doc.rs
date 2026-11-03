use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Doc {
    Text(String),     // 不含換行的文字
    Verbatim(String), // 可能含換行的文字（多行字串），原樣輸出，不加縮排
    Line,             // 扁平時印成空白，換行時印成換行加縮排
    HardLine,         // 一定換行，並讓所在的 Group 換行
    Indent(Box<Doc>), // 內部的換行多縮排一層
    Concat(Vec<Doc>),
    Group(Vec<Doc>),
}

pub(crate) fn text(s: impl Into<String>) -> Doc {
    let s = s.into();
    debug_assert!(
        !s.contains('\n'),
        "Use verbatim() for text with line breaks."
    );
    Doc::Text(s)
}

pub(crate) fn verbatim(s: impl Into<String>) -> Doc {
    Doc::Verbatim(s.into())
}

pub(crate) fn indent(doc: Doc) -> Doc {
    Doc::Indent(Box::new(doc))
}

pub(crate) fn concat(docs: Vec<Doc>) -> Doc {
    Doc::Concat(docs)
}

pub(crate) fn group(docs: Vec<Doc>) -> Doc {
    Doc::Group(docs)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct PrintOptions {
    pub line_width: usize,
    pub indent: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

// 待處理的指令：（縮排欄數, 模式, 文件）
type Command<'a> = (usize, Mode, &'a Doc);

pub(crate) fn print(doc: &Doc, options: PrintOptions) -> String {
    let mut out = String::new();
    let mut column = 0;
    // 堆疊的最後一個元素是下一個要印的指令
    let mut stack: Vec<Command> = vec![(0, Mode::Break, doc)];

    while let Some((indent_width, mode, doc)) = stack.pop() {
        match doc {
            Doc::Text(s) => {
                out.push_str(s);
                column += s.width();
            }
            Doc::Verbatim(s) => {
                out.push_str(s);
                column = match s.rfind('\n') {
                    Some(i) => s[i + 1..].width(),
                    None => column + s.width(),
                };
            }
            Doc::Line if mode == Mode::Flat => {
                out.push(' ');
                column += 1;
            }
            Doc::Line | Doc::HardLine => {
                newline(&mut out, indent_width);
                column = indent_width;
            }
            Doc::Indent(inner) => stack.push((indent_width + options.indent, mode, inner)),
            Doc::Concat(docs) => {
                stack.extend(docs.iter().rev().map(|d| (indent_width, mode, d)));
            }
            Doc::Group(docs) => {
                let remaining = options.line_width as isize - column as isize;
                let mode = if mode == Mode::Flat || fits(remaining, docs, &stack) {
                    Mode::Flat
                } else {
                    Mode::Break
                };
                stack.extend(docs.iter().rev().map(|d| (indent_width, mode, d)));
            }
        }
    }
    out
}

// 換行前去掉行尾空白，空行因此不會留下縮排空白
fn newline(out: &mut String, indent_width: usize) {
    let trimmed = out.trim_end_matches(' ').len();
    out.truncate(trimmed);
    out.push('\n');
    out.extend(std::iter::repeat_n(' ', indent_width));
}

// 檢查 group 扁平印出後，連同後面直到下一個換行的內容（例如結尾的 `)))`），是否放得進 remaining 欄
fn fits(mut remaining: isize, group: &[Doc], rest: &[Command]) -> bool {
    let mut stack: Vec<(Mode, &Doc)> = group.iter().rev().map(|d| (Mode::Flat, d)).collect();
    let mut rest = rest.iter().rev();

    loop {
        let (mode, doc) = match stack.pop() {
            Some(item) => item,
            None => match rest.next() {
                Some(&(_, mode, doc)) => (mode, doc),
                None => return true,
            },
        };
        match doc {
            Doc::Text(s) => remaining -= s.width() as isize,
            Doc::Verbatim(s) => match s.split_once('\n') {
                None => remaining -= s.width() as isize,
                // 扁平模式不能含換行；後續內容遇到換行則這一行到此結束
                Some(_) if mode == Mode::Flat => return false,
                Some((first_line, _)) => return remaining >= first_line.width() as isize,
            },
            Doc::Line if mode == Mode::Flat => remaining -= 1,
            Doc::Line => return true,
            Doc::HardLine => return mode == Mode::Break,
            Doc::Indent(inner) => stack.push((mode, inner)),
            Doc::Concat(docs) | Doc::Group(docs) => {
                stack.extend(docs.iter().rev().map(|d| (mode, d)));
            }
        }
        if remaining < 0 {
            return false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn print_with(width: usize, doc: Doc) -> String {
        print(
            &doc,
            PrintOptions {
                line_width: width,
                indent: 2,
            },
        )
    }

    // (head arg ...)：放不下時參數各佔一行、縮排 2 格
    fn list(head: &str, args: &[Doc]) -> Doc {
        let mut body = Vec::new();
        for arg in args {
            body.push(Doc::Line);
            body.push(arg.clone());
        }
        group(vec![
            text(format!("({head}")),
            indent(concat(body)),
            text(")"),
        ])
    }

    #[test]
    fn group_that_fits_stays_on_one_line() {
        assert_eq!(
            print_with(80, list("subclass", &[text("A"), text("B")])),
            "(subclass A B)"
        );
    }

    #[test]
    fn group_that_does_not_fit_breaks() {
        assert_eq!(
            print_with(10, list("subclass", &[text("A"), text("B")])),
            "(subclass\n  A\n  B)"
        );
    }

    #[test]
    fn inner_group_stays_flat_when_outer_breaks() {
        let doc = list("=>", &[list("a", &[text("?X")]), list("b", &[text("?X")])]);
        assert_eq!(print_with(12, doc), "(=>\n  (a ?X)\n  (b ?X))");
    }

    #[test]
    fn trailing_text_after_group_counts() {
        // "(a b" 佔 4 欄，加上後面的 ")))" 共 7 欄，超過 6
        let doc = concat(vec![
            group(vec![text("(a"), indent(concat(vec![Doc::Line, text("b")]))]),
            text(")))"),
        ]);
        assert_eq!(print_with(6, doc.clone()), "(a\n  b)))");
        assert_eq!(print_with(7, doc), "(a b)))");
    }

    #[test]
    fn hard_line_forces_break() {
        let doc = group(vec![
            text("(=>"),
            indent(concat(vec![Doc::HardLine, text("(a)")])),
            text(")"),
        ]);
        assert_eq!(print_with(80, doc), "(=>\n  (a))");
    }

    #[test]
    fn verbatim_line_breaks_are_not_indented() {
        let doc = list(
            "documentation",
            &[text("X"), verbatim("\"line 1\nline 2\"")],
        );
        assert_eq!(
            print_with(80, doc),
            "(documentation\n  X\n  \"line 1\nline 2\")"
        );
    }

    #[test]
    fn wide_characters_count_as_two_columns() {
        // "(a 中文)" 是 6 個字元，但佔 8 欄
        let doc = list("a", &[text("中文")]);
        assert_eq!(print_with(8, doc.clone()), "(a 中文)");
        assert_eq!(print_with(7, doc), "(a\n  中文)");
    }

    #[test]
    fn blank_lines_have_no_trailing_spaces() {
        let doc = concat(vec![
            text("a"),
            indent(concat(vec![Doc::HardLine, Doc::HardLine, text("b")])),
        ]);
        assert_eq!(print_with(80, doc), "a\n\n  b");
    }
}

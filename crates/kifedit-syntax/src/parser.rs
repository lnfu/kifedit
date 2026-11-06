use rowan::{GreenNode, GreenNodeBuilder, TextRange, TextSize};

use crate::SyntaxKind::*;
use crate::lexer::{Token, lex};
use crate::{SyntaxError, SyntaxErrorKind, SyntaxNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parse {
    pub green: GreenNode,
    pub errors: Vec<SyntaxError>,
}

impl Parse {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }
}

pub fn parse(src: &str) -> Parse {
    Parser {
        tokens: lex(src),
        pos: 0,
        offset: TextSize::new(0),
        content_end: TextSize::new(0),
        depth: 0,
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
    }
    .file()
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    pos: usize,            // 下一個記號的索引
    offset: TextSize,      // 下一個記號的位元組位置
    content_end: TextSize, // 最後一個非空白記號的結尾
    depth: usize,          // 目前未閉合的串列數
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
}

impl<'a> Parser<'a> {
    fn file(mut self) -> Parse {
        self.builder.start_node(File.into());
        while let Some(token) = self.peek() {
            match token.kind {
                Whitespace | Comment => self.bump(),
                LParen => self.top_level_form(),
                _ => self.stray_token(),
            }
        }
        self.builder.finish_node();
        // 錯誤依出現位置排序。sort_by_key 是穩定排序，起點相同的錯誤保持回報順序
        self.errors.sort_by_key(|e| e.range.start());
        Parse {
            green: self.builder.finish(),
            errors: self.errors,
        }
    }

    /// 頂層公式：解析完才知道要包成 TopLevelForm 還是 Error，所以先記 checkpoint
    fn top_level_form(&mut self) {
        let checkpoint = self.builder.checkpoint();
        let start = self.offset;
        let closed = self.list_body();
        let kind = if closed { TopLevelForm } else { Error };
        self.builder.start_node_at(checkpoint, kind.into());
        self.builder.finish_node();
        // 未閉合的字串會吃到檔尾，串列因此無法閉合；這是連帶錯誤，只回報字串那一個
        if !closed && !self.stopped_by_unclosed_string() {
            self.error(start, SyntaxErrorKind::UnclosedList);
        }
    }

    fn stopped_by_unclosed_string(&self) -> bool {
        self.pos == self.tokens.len() && self.tokens.last().is_some_and(|t| t.kind == ErrorToken)
    }

    fn nested_list(&mut self) -> bool {
        self.builder.start_node(List.into());
        let closed = self.list_body();
        self.builder.finish_node();
        closed
    }

    fn list_body(&mut self) -> bool {
        self.bump(); // (
        self.depth += 1;
        let closed = self.list_items();
        self.depth -= 1;
        closed
    }

    fn list_items(&mut self) -> bool {
        while let Some(token) = self.peek() {
            match token.kind {
                RParen => {
                    self.bump();
                    return true;
                }
                // 行首的 ( 只有在目前的公式之後不會閉合時，才視為新公式的開始
                LParen if self.at_line_start() && !self.form_closes_later() => return false,
                LParen => {
                    if !self.nested_list() {
                        return false;
                    }
                }
                _ => self.bump(),
            }
        }
        false
    }

    fn stray_token(&mut self) {
        let kind = self.tokens[self.pos].kind;
        let start = self.offset;
        self.builder.start_node(Error.into());
        self.bump();
        self.builder.finish_node();
        match kind {
            RParen => self.error(start, SyntaxErrorKind::UnopenedRParen),
            ErrorToken => {} // bump 已經回報
            _ => self.error(start, SyntaxErrorKind::TopLevelAtom),
        }
    }

    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.pos).copied()
    }

    fn bump(&mut self) {
        let token = self.tokens[self.pos];
        self.builder.token(token.kind.into(), token.text);
        let start = self.offset;
        self.offset += TextSize::of(token.text);
        self.pos += 1;
        if token.kind != Whitespace {
            self.content_end = self.offset;
        }
        if token.kind == ErrorToken {
            self.error(start, SyntaxErrorKind::UnclosedString);
        }
    }

    fn at_line_start(&self) -> bool {
        match self.pos.checked_sub(1) {
            None => true,
            Some(prev) => {
                let prev = self.tokens[prev];
                prev.kind == Whitespace && prev.text.ends_with('\n')
            }
        }
    }

    // 從目前的記號往後掃描，看所有未閉合的串列最後是否都會閉合
    fn form_closes_later(&self) -> bool {
        let target = -(self.depth as isize);
        let mut balance = 0isize;
        for token in &self.tokens[self.pos..] {
            match token.kind {
                LParen => balance += 1,
                RParen => {
                    balance -= 1;
                    if balance == target {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    // 錯誤範圍是 start 到最後一個非空白記號的結尾，編輯器的底線不會延伸到行尾之後
    fn error(&mut self, start: TextSize, kind: SyntaxErrorKind) {
        let range = TextRange::new(start, self.content_end.max(start));
        self.errors.push(SyntaxError { range, kind });
    }
}

#[cfg(test)]
mod tests {
    use expect_test::{Expect, expect};

    use super::*;

    // 快照：整棵樹加上錯誤清單。預期結果留空時，執行 UPDATE_EXPECT=1 cargo test 自動填入
    fn check(src: &str, expect: Expect) {
        let parsed = parse(src);
        let mut actual = format!("{:#?}", parsed.syntax());
        for error in &parsed.errors {
            actual += &format!(
                "{} {:?}: {}\n",
                error.kind.code(),
                error.range,
                error.kind.message()
            );
        }
        expect.assert_eq(&actual);
    }

    #[test]
    fn round_trip_keeps_every_byte() {
        let src = "; c\r\n(=> (a ?X)\n  (b ?X))  ; tail\n";
        assert_eq!(parse(src).syntax().to_string(), src);
    }

    #[test]
    fn nested_lists() {
        check(
            "(a (b c))",
            expect![[r#"
            File@0..9
              TopLevelForm@0..9
                LParen@0..1 "("
                Word@1..2 "a"
                Whitespace@2..3 " "
                List@3..8
                  LParen@3..4 "("
                  Word@4..5 "b"
                  Whitespace@5..6 " "
                  Word@6..7 "c"
                  RParen@7..8 ")"
                RParen@8..9 ")"
        "#]],
        );
    }

    #[test]
    fn unclosed_form_stops_at_line_start_paren() {
        check(
            "(a (b\n(c d)\n",
            expect![[r#"
            File@0..12
              Error@0..6
                LParen@0..1 "("
                Word@1..2 "a"
                Whitespace@2..3 " "
                List@3..6
                  LParen@3..4 "("
                  Word@4..5 "b"
                  Whitespace@5..6 "\n"
              TopLevelForm@6..11
                LParen@6..7 "("
                Word@7..8 "c"
                Whitespace@8..9 " "
                Word@9..10 "d"
                RParen@10..11 ")"
              Whitespace@11..12 "\n"
            E001 0..5: The list has no closing parenthesis.
        "#]],
        );
    }

    #[test]
    fn line_start_paren_in_closed_form_does_not_split() {
        // Biography.kif:428 的寫法
        check(
            "(=>\n(a ?P)\n(b ?P))\n(c)\n",
            expect![[r#"
            File@0..23
              TopLevelForm@0..18
                LParen@0..1 "("
                Word@1..3 "=>"
                Whitespace@3..4 "\n"
                List@4..10
                  LParen@4..5 "("
                  Word@5..6 "a"
                  Whitespace@6..7 " "
                  Var@7..9 "?P"
                  RParen@9..10 ")"
                Whitespace@10..11 "\n"
                List@11..17
                  LParen@11..12 "("
                  Word@12..13 "b"
                  Whitespace@13..14 " "
                  Var@14..16 "?P"
                  RParen@16..17 ")"
                RParen@17..18 ")"
              Whitespace@18..19 "\n"
              TopLevelForm@19..22
                LParen@19..20 "("
                Word@20..21 "c"
                RParen@21..22 ")"
              Whitespace@22..23 "\n"
        "#]],
        );
    }

    #[test]
    fn indented_paren_does_not_split_form() {
        check(
            "(a\n  (b c))\n(d)",
            expect![[r#"
            File@0..15
              TopLevelForm@0..11
                LParen@0..1 "("
                Word@1..2 "a"
                Whitespace@2..5 "\n  "
                List@5..10
                  LParen@5..6 "("
                  Word@6..7 "b"
                  Whitespace@7..8 " "
                  Word@8..9 "c"
                  RParen@9..10 ")"
                RParen@10..11 ")"
              Whitespace@11..12 "\n"
              TopLevelForm@12..15
                LParen@12..13 "("
                Word@13..14 "d"
                RParen@14..15 ")"
        "#]],
        );
    }

    #[test]
    fn stray_tokens_at_top_level() {
        check(
            "(a)) b\n",
            expect![[r#"
            File@0..7
              TopLevelForm@0..3
                LParen@0..1 "("
                Word@1..2 "a"
                RParen@2..3 ")"
              Error@3..4
                RParen@3..4 ")"
              Whitespace@4..5 " "
              Error@5..6
                Word@5..6 "b"
              Whitespace@6..7 "\n"
            E001 3..4: This closing parenthesis has no opening parenthesis.
            E001 5..6: A top-level form must start with an opening parenthesis.
        "#]],
        );
    }

    #[test]
    fn unclosed_string() {
        check(
            "(a \"oops",
            expect![[r#"
            File@0..8
              Error@0..8
                LParen@0..1 "("
                Word@1..2 "a"
                Whitespace@2..3 " "
                ErrorToken@3..8 "\"oops"
            E001 3..8: The string has no closing quotation mark.
        "#]],
        );
    }
}

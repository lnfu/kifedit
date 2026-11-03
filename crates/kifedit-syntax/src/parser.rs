use rowan::{GreenNode, GreenNodeBuilder, TextRange, TextSize};

use crate::SyntaxKind::*;
use crate::SyntaxNode;
use crate::lexer::{Token, lex};

pub const UNCLOSED_LIST: &str = "The list has no closing parenthesis.";
pub const UNOPENED_RPAREN: &str = "This closing parenthesis has no opening parenthesis.";
pub const TOP_LEVEL_ATOM: &str = "A top-level form must start with an opening parenthesis.";
pub const UNCLOSED_STRING: &str = "The string has no closing quotation mark.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxError {
    pub range: TextRange, // 位元組範圍，與樹上節點的位置同一種型別
    pub message: &'static str,
}

#[derive(Debug, Clone)]
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
        depth: 0,
        builder: GreenNodeBuilder::new(),
        errors: Vec::new(),
    }
    .file()
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    pos: usize,    // 下一個記號的索引
    offset: TextSize, // 下一個記號的位元組位置
    depth: usize,  // 目前未閉合的串列數
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
        if !closed {
            self.error(start, UNCLOSED_LIST);
        }
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
            RParen => self.error(start, UNOPENED_RPAREN),
            ErrorToken => {} // bump 已經回報
            _ => self.error(start, TOP_LEVEL_ATOM),
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
        if token.kind == ErrorToken {
            self.error(start, UNCLOSED_STRING);
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

    // 錯誤範圍一律是 start 到目前位置
    fn error(&mut self, start: TextSize, message: &'static str) {
        let range = TextRange::new(start, self.offset);
        self.errors.push(SyntaxError { range, message });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SyntaxKind;

    fn top_kinds(src: &str) -> Vec<SyntaxKind> {
        parse(src).syntax().children().map(|n| n.kind()).collect()
    }

    #[test]
    fn round_trip_keeps_every_byte() {
        let src = "; c\r\n(=> (a ?X)\n  (b ?X))  ; tail\n";
        assert_eq!(parse(src).syntax().to_string(), src);
    }

    #[test]
    fn nested_lists() {
        let parsed = parse("(a (b c))");
        let form = parsed.syntax().first_child().unwrap();
        assert_eq!(form.kind(), TopLevelForm);
        assert_eq!(form.first_child().unwrap().kind(), List);
        assert!(parsed.errors.is_empty());
    }

    #[test]
    fn unclosed_form_stops_at_line_start_paren() {
        let src = "(a (b\n(c d)\n";
        assert_eq!(top_kinds(src), [Error, TopLevelForm]);
        let errors = parse(src).errors;
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].message, UNCLOSED_LIST);
        // "(a (b\n" 共 6 個位元組
        assert_eq!(errors[0].range, TextRange::new(0.into(), 6.into()));
    }

    #[test]
    fn line_start_paren_in_closed_form_does_not_split() {
        // Biography.kif:428 的寫法
        let src = "(=>\n(attribute ?P Economist)\n(hasExpertise ?P Economics))\n(instance A B)\n";
        assert_eq!(top_kinds(src), [TopLevelForm, TopLevelForm]);
        assert!(parse(src).errors.is_empty());
    }

    #[test]
    fn indented_paren_does_not_split_form() {
        assert_eq!(top_kinds("(a\n  (b c))\n(d)"), [TopLevelForm, TopLevelForm]);
    }

    #[test]
    fn stray_tokens_at_top_level() {
        let parsed = parse("(a)) b\n");
        assert_eq!(top_kinds("(a)) b\n"), [TopLevelForm, Error, Error]);
        let messages: Vec<_> = parsed.errors.iter().map(|e| e.message).collect();
        assert_eq!(messages, [UNOPENED_RPAREN, TOP_LEVEL_ATOM]);
    }
}

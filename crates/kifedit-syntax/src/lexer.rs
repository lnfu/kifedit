use crate::SyntaxKind::{self, *};
use logos::Logos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    pub kind: SyntaxKind,
    pub text: &'a str,
}

#[derive(Logos, Debug, Clone, Copy, PartialEq, Eq)]
enum Raw {
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[regex(r"[ \t\n\r\f]+")]
    Whitespace,
    // 到行尾；行尾的 \r 留給 Whitespace，行中單獨的 \r 仍算註解
    #[regex(r";([^\r\n]|\r[^\n])*")]
    Comment,
    #[regex(r#""([^"\\]|\\(.|\n))*""#)]
    Str,
    // 沒有結尾引號：吃到檔尾
    #[regex(r#""([^"\\]|\\(.|\n))*"#)]
    UnterminatedStr,
    // 符號、變數、數字先一起切，再由 classify 分類
    #[regex(r#"[^ \t\n\r\f()";]+"#)]
    Atom,
}

pub fn lex(src: &str) -> Vec<Token<'_>> {
    let mut lexer = Raw::lexer(src);
    let mut tokens = Vec::new();
    while let Some(result) = lexer.next() {
        let text = lexer.slice();
        let kind = match result {
            Ok(Raw::LParen) => LParen,
            Ok(Raw::RParen) => RParen,
            Ok(Raw::Whitespace) => Whitespace,
            Ok(Raw::Comment) => Comment,
            Ok(Raw::Str) => Str,
            Ok(Raw::UnterminatedStr) | Err(()) => ErrorToken,
            Ok(Raw::Atom) => classify(text),
        };
        tokens.push(Token { kind, text });
    }
    tokens
}

fn classify(word: &str) -> SyntaxKind {
    match word.as_bytes()[0] {
        b'?' => Var,
        b'@' => RowVar,
        _ if looks_like_number(word) => Number,
        _ => Word,
    }
}

fn looks_like_number(w: &str) -> bool {
    let digits = w.strip_prefix(['-', '+']).unwrap_or(w);
    digits.starts_with(|c: char| c.is_ascii_digit()) && w.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(s: &str) -> Vec<SyntaxKind> {
        lex(s)
            .iter()
            .map(|t| t.kind)
            .filter(|k| *k != Whitespace)
            .collect()
    }

    #[test]
    fn basic_form() {
        assert_eq!(
            kinds("(instance ?X Camera)"),
            [LParen, Word, Var, Word, RParen]
        );
    }

    #[test]
    fn parens_inside_string_are_not_structure() {
        assert_eq!(
            kinds(r#"(documentation A "x (y) z")"#),
            [LParen, Word, Word, Str, RParen]
        );
    }

    #[test]
    fn comment_leaves_crlf_to_whitespace() {
        let t = lex("; hi\r\n(a)");
        assert_eq!(
            t[0],
            Token {
                kind: Comment,
                text: "; hi"
            }
        );
        assert_eq!(
            t[1],
            Token {
                kind: Whitespace,
                text: "\r\n"
            }
        );
    }

    #[test]
    fn numbers_vs_words() {
        assert_eq!(kinds("1 -2.5 3e4 1st"), [Number, Number, Number, Word]);
    }

    #[test]
    fn unterminated_string() {
        assert_eq!(kinds(r#"(a "oops"#), [LParen, Word, ErrorToken]);
    }
}

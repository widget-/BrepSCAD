use crate::syntax::{SyntaxError, SyntaxKind};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Token<'a> {
    pub(crate) kind: SyntaxKind,
    pub(crate) text: &'a str,
    pub(crate) offset: usize,
}

#[derive(Debug)]
pub(crate) struct Lexed<'a> {
    pub(crate) tokens: Vec<Token<'a>>,
    pub(crate) errors: Vec<SyntaxError>,
}

pub(crate) fn lex(input: &str) -> Lexed<'_> {
    let mut lexer = Lexer {
        input,
        pos: 0,
        tokens: Vec::new(),
        errors: Vec::new(),
    };
    lexer.run();
    Lexed {
        tokens: lexer.tokens,
        errors: lexer.errors,
    }
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    tokens: Vec<Token<'a>>,
    errors: Vec<SyntaxError>,
}

impl<'a> Lexer<'a> {
    fn run(&mut self) {
        while self.peek().is_some() {
            self.token();
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn next_is_digit(&self) -> bool {
        self.input[self.pos + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    }

    fn rest(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn advance(&mut self) {
        if let Some(c) = self.peek() {
            self.pos += c.len_utf8();
        }
    }

    fn push(&mut self, kind: SyntaxKind, start: usize) {
        self.tokens.push(Token {
            kind,
            text: &self.input[start..self.pos],
            offset: start,
        });
    }

    fn token(&mut self) {
        let start = self.pos;
        let c = self.peek().expect("run() checks for EOF");

        if c.is_whitespace() {
            while let Some(c) = self.peek() {
                if !c.is_whitespace() {
                    break;
                }
                self.advance();
            }
            self.push(SyntaxKind::WHITESPACE, start);
        } else if self.rest().starts_with("//") {
            self.advance();
            self.advance();
            while let Some(c) = self.peek() {
                if c == '\n' {
                    break;
                }
                self.advance();
            }
            self.push(SyntaxKind::LINE_COMMENT, start);
        } else if self.rest().starts_with("/*") {
            self.advance();
            self.advance();
            let mut closed = false;
            while self.peek().is_some() {
                if self.rest().starts_with("*/") {
                    self.advance();
                    self.advance();
                    closed = true;
                    break;
                }
                self.advance();
            }
            self.push(SyntaxKind::BLOCK_COMMENT, start);
            if !closed {
                self.errors
                    .push(SyntaxError::new("unterminated block comment", start));
            }
        } else if is_ident_start(c) {
            self.advance();
            while let Some(c) = self.peek() {
                if !is_ident_continue(c) {
                    break;
                }
                self.advance();
            }
            self.push(SyntaxKind::IDENT, start);
        } else if c.is_ascii_digit() || (c == '-' && self.next_is_digit()) {
            if c == '-' {
                self.advance();
            }
            self.advance();
            while let Some(c) = self.peek() {
                if !c.is_ascii_digit() {
                    break;
                }
                self.advance();
            }
            if self.peek() == Some('.')
                && self.input[self.pos + 1..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            {
                self.advance();
                while let Some(c) = self.peek() {
                    if !c.is_ascii_digit() {
                        break;
                    }
                    self.advance();
                }
            }
            self.push(SyntaxKind::NUMBER, start);
        } else {
            let kind = match c {
                '[' => Some(SyntaxKind::L_BRACK),
                ']' => Some(SyntaxKind::R_BRACK),
                '(' => Some(SyntaxKind::L_PAREN),
                ')' => Some(SyntaxKind::R_PAREN),
                '{' => Some(SyntaxKind::L_BRACE),
                '}' => Some(SyntaxKind::R_BRACE),
                ',' => Some(SyntaxKind::COMMA),
                ';' => Some(SyntaxKind::SEMICOLON),
                '=' => Some(SyntaxKind::EQ),
                _ => None,
            };
            self.advance();
            match kind {
                Some(kind) => self.push(kind, start),
                None => {
                    self.push(SyntaxKind::ERROR, start);
                    self.errors.push(SyntaxError::new(
                        format!("unexpected character {c:?}"),
                        start,
                    ));
                }
            }
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

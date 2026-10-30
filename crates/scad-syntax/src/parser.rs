use rowan::{GreenNode, GreenNodeBuilder};

use crate::lexer::{Token, lex};
use crate::syntax::{SyntaxError, SyntaxKind, SyntaxNode};

/// A lossless parse result.
#[derive(Debug, Clone)]
pub struct Parse {
    green: GreenNode,
    errors: Vec<SyntaxError>,
}

impl Parse {
    /// Returns the syntax tree. The tree text is the original source.
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    /// Diagnostics collected while lexing and parsing.
    pub fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }

    /// Reconstructs the source from the tree.
    pub fn text(&self) -> String {
        self.syntax().text().to_string()
    }
}

/// Parses the supported subset into a lossless tree.
pub fn parse(input: &str) -> Parse {
    let lexed = lex(input);
    let parser = Parser::new(lexed.tokens, lexed.errors);
    let (green, errors) = parser.parse();
    Parse { green, errors }
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    pos: usize,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
    block_depth: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: Vec<Token<'a>>, errors: Vec<SyntaxError>) -> Self {
        Self {
            tokens,
            pos: 0,
            builder: GreenNodeBuilder::new(),
            errors,
            block_depth: 0,
        }
    }

    fn parse(mut self) -> (GreenNode, Vec<SyntaxError>) {
        self.source_file();
        (self.builder.finish(), self.errors)
    }

    fn source_file(&mut self) {
        self.start(SyntaxKind::SOURCE_FILE);
        loop {
            self.eat_trivia();
            match self.current() {
                SyntaxKind::EOF => break,
                SyntaxKind::IDENT => self.statement(),
                _ => {
                    self.error("expected a statement");
                    self.recover_statement();
                }
            }
        }
        self.eat_trivia();
        self.finish();
    }

    fn statement(&mut self) {
        if self.current() == SyntaxKind::IDENT && self.current_text() == "if" {
            self.if_statement();
            return;
        }
        match self.peek(1) {
            SyntaxKind::EQ => self.assignment(),
            SyntaxKind::L_PAREN => self.module_call(),
            _ => {
                self.error("expected '=' or '(' after identifier");
                self.recover_statement();
            }
        }
    }

    fn if_statement(&mut self) {
        self.start(SyntaxKind::MODULE_CALL);
        self.bump();
        if self.arg_list() {
            self.conditional_body("if");
            if self.at_keyword("else") {
                self.start(SyntaxKind::ELSE);
                self.bump();
                self.conditional_body("else");
                self.finish();
            }
        } else {
            self.recover_statement();
        }
        self.finish();
    }

    fn conditional_body(&mut self, keyword: &str) {
        if self.at(SyntaxKind::L_BRACE) {
            self.block();
        } else if self.at(SyntaxKind::IDENT) {
            self.statement();
        } else {
            self.error(format!("expected a statement after {keyword}"));
            self.recover_statement();
        }
    }

    fn assignment(&mut self) {
        self.start(SyntaxKind::ASSIGNMENT);
        self.bump();
        if !self.expect(SyntaxKind::EQ) || !self.expression() || !self.expect(SyntaxKind::SEMICOLON)
        {
            self.recover_statement();
        }
        self.finish();
    }

    fn module_call(&mut self) {
        self.start(SyntaxKind::MODULE_CALL);
        self.bump();
        if self.arg_list() {
            if self.at(SyntaxKind::L_BRACE) {
                self.block();
            } else if self.at(SyntaxKind::IDENT) {
                self.eat_trivia();
                self.statement();
            } else if !self.expect(SyntaxKind::SEMICOLON) {
                self.recover_statement();
            }
        } else {
            self.recover_statement();
        }
        self.finish();
    }

    fn block(&mut self) {
        self.start(SyntaxKind::BLOCK);
        self.bump();
        self.block_depth += 1;
        loop {
            self.eat_trivia();
            if self.at(SyntaxKind::R_BRACE) {
                self.bump();
                break;
            }
            if self.at(SyntaxKind::EOF) {
                self.error("unterminated block");
                break;
            }
            match self.current() {
                SyntaxKind::IDENT => self.statement(),
                _ => {
                    self.error("expected a statement");
                    self.recover_statement();
                }
            }
        }
        self.block_depth -= 1;
        self.finish();
    }

    fn arg_list(&mut self) -> bool {
        self.start(SyntaxKind::ARG_LIST);
        if !self.expect(SyntaxKind::L_PAREN) {
            self.finish();
            return false;
        }
        if self.at(SyntaxKind::R_PAREN) {
            self.bump();
            self.finish();
            return true;
        }
        loop {
            if self.current() == SyntaxKind::IDENT && self.peek(1) == SyntaxKind::EQ {
                if !self.named_arg() {
                    self.finish();
                    return false;
                }
            } else if !self.expression() {
                self.finish();
                return false;
            }
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                continue;
            }
            if self.at(SyntaxKind::R_PAREN) {
                self.bump();
                self.finish();
                return true;
            }
            self.error("expected ',' or ')'");
            self.finish();
            return false;
        }
    }

    fn named_arg(&mut self) -> bool {
        self.start(SyntaxKind::NAMED_ARG);
        self.bump();
        if !self.expect(SyntaxKind::EQ) || !self.expression() {
            self.finish();
            return false;
        }
        self.finish();
        true
    }

    fn expression(&mut self) -> bool {
        self.binary_expr(0)
    }

    fn binary_expr(&mut self, min_precedence: u8) -> bool {
        let checkpoint = self.builder.checkpoint();
        if !self.unary_expr() {
            return false;
        }
        loop {
            let precedence = match self.current() {
                SyntaxKind::OR_OR => 1,
                SyntaxKind::AND_AND => 2,
                SyntaxKind::EQEQ
                | SyntaxKind::NE
                | SyntaxKind::LT
                | SyntaxKind::LE
                | SyntaxKind::GT
                | SyntaxKind::GE => 3,
                SyntaxKind::PLUS | SyntaxKind::MINUS => 4,
                SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::PERCENT => 5,
                _ => break,
            };
            if precedence < min_precedence {
                break;
            }
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            if !self.binary_expr(precedence + 1) {
                self.finish();
                return false;
            }
            self.finish();
        }
        true
    }

    fn unary_expr(&mut self) -> bool {
        if matches!(
            self.current(),
            SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::BANG
        ) {
            self.start(SyntaxKind::UNARY_EXPR);
            self.bump();
            if !self.unary_expr() {
                self.finish();
                return false;
            }
            self.finish();
            true
        } else {
            self.primary()
        }
    }

    fn primary(&mut self) -> bool {
        match self.current() {
            SyntaxKind::NUMBER | SyntaxKind::IDENT | SyntaxKind::TRUE | SyntaxKind::FALSE => {
                self.bump();
                true
            }
            SyntaxKind::L_BRACK => self.vector(),
            SyntaxKind::L_PAREN => self.group(),
            _ => {
                self.error("expected an expression");
                false
            }
        }
    }

    fn group(&mut self) -> bool {
        self.start(SyntaxKind::GROUP);
        if !self.expect(SyntaxKind::L_PAREN) {
            self.finish();
            return false;
        }
        if !self.expression() {
            self.finish();
            return false;
        }
        if !self.expect(SyntaxKind::R_PAREN) {
            self.finish();
            return false;
        }
        self.finish();
        true
    }

    fn vector(&mut self) -> bool {
        self.start(SyntaxKind::VECTOR);
        if !self.expect(SyntaxKind::L_BRACK) {
            self.finish();
            return false;
        }
        if self.at(SyntaxKind::R_BRACK) {
            self.bump();
            self.finish();
            return true;
        }

        let checkpoint = self.builder.checkpoint();
        if !self.expression() {
            self.finish();
            return false;
        }
        if self.at(SyntaxKind::COLON) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::RANGE.into());
            self.bump();
            if !self.expression() {
                self.finish();
                self.finish();
                return false;
            }
            if self.at(SyntaxKind::COLON) {
                self.bump();
                if !self.expression() {
                    self.finish();
                    self.finish();
                    return false;
                }
            }
            self.finish();
            if !self.expect(SyntaxKind::R_BRACK) {
                self.finish();
                return false;
            }
            self.finish();
            return true;
        }

        loop {
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                if !self.expression() {
                    self.finish();
                    return false;
                }
                continue;
            }
            if self.at(SyntaxKind::R_BRACK) {
                self.bump();
                self.finish();
                return true;
            }
            self.error("expected ',' or ']'");
            self.finish();
            return false;
        }
    }

    fn recover_statement(&mut self) {
        self.start(SyntaxKind::ERROR);
        loop {
            if self.at(SyntaxKind::EOF) {
                break;
            }
            if self.block_depth > 0 && self.at(SyntaxKind::R_BRACE) {
                break;
            }
            if self.at(SyntaxKind::SEMICOLON) {
                self.bump();
                break;
            }
            self.bump();
        }
        self.finish();
    }

    fn at(&self, kind: SyntaxKind) -> bool {
        self.current() == kind
    }

    fn at_keyword(&self, keyword: &str) -> bool {
        self.current_text() == keyword
    }

    fn current_text(&self) -> &'a str {
        self.tokens[self.pos..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map_or("", |token| token.text)
    }

    fn current(&self) -> SyntaxKind {
        self.tokens[self.pos..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map_or(SyntaxKind::EOF, |token| token.kind)
    }

    fn peek(&self, n: usize) -> SyntaxKind {
        self.tokens[self.pos..]
            .iter()
            .filter(|token| !token.kind.is_trivia())
            .nth(n)
            .map_or(SyntaxKind::EOF, |token| token.kind)
    }

    fn bump(&mut self) {
        self.eat_trivia();
        if let Some(token) = self.tokens.get(self.pos).copied() {
            self.builder.token(token.kind.into(), token.text);
            self.pos += 1;
        }
    }

    fn eat_trivia(&mut self) {
        while let Some(token) = self.tokens.get(self.pos).copied() {
            if !token.kind.is_trivia() {
                break;
            }
            self.builder.token(token.kind.into(), token.text);
            self.pos += 1;
        }
    }

    fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            self.error(format!("expected {kind:?}"));
            false
        }
    }

    fn error(&mut self, message: impl Into<String>) {
        let offset = self.tokens[self.pos..]
            .iter()
            .find(|token| !token.kind.is_trivia())
            .map_or_else(
                || {
                    self.tokens
                        .last()
                        .map_or(0, |token| token.offset + token.text.len())
                },
                |token| token.offset,
            );
        self.errors.push(SyntaxError::new(message, offset));
    }

    fn start(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
    }

    fn finish(&mut self) {
        self.builder.finish_node();
    }
}

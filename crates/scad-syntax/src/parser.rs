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
            if self.at(SyntaxKind::EOF) {
                break;
            }
            if self.at(SyntaxKind::SEMICOLON) {
                self.bump();
                continue;
            }
            if self.starts_statement() {
                self.statement();
            } else {
                self.error("expected a statement");
                self.recover_statement();
            }
        }
        self.eat_trivia();
        self.finish();
    }

    fn statement(&mut self) {
        if matches!(
            self.current(),
            SyntaxKind::STAR | SyntaxKind::BANG | SyntaxKind::HASH | SyntaxKind::PERCENT
        ) {
            self.modified_statement();
            return;
        }
        if self.at(SyntaxKind::L_BRACE) {
            self.block();
            return;
        }
        if self.current() == SyntaxKind::IDENT {
            match self.current_text() {
                "if" => {
                    self.if_statement();
                    return;
                }
                "module" if self.peek(1) == SyntaxKind::IDENT => {
                    self.module_declaration();
                    return;
                }
                "function" if self.peek(1) == SyntaxKind::IDENT => {
                    self.function_declaration();
                    return;
                }
                "include" if matches!(self.peek(1), SyntaxKind::STRING | SyntaxKind::LT) => {
                    self.include_statement(SyntaxKind::INCLUDE);
                    return;
                }
                "use" if matches!(self.peek(1), SyntaxKind::STRING | SyntaxKind::LT) => {
                    self.include_statement(SyntaxKind::USE);
                    return;
                }
                _ => {}
            }
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

    fn modified_statement(&mut self) {
        self.start(SyntaxKind::MODIFIER);
        self.bump();
        if self.starts_statement() {
            self.statement();
        } else {
            self.error("expected a statement after a modifier");
            self.recover_statement();
        }
        self.finish();
    }

    fn module_declaration(&mut self) {
        self.start(SyntaxKind::MODULE_DECLARATION);
        self.bump();
        if !self.expect(SyntaxKind::IDENT) {
            self.recover_statement();
            self.finish();
            return;
        }
        if !self.param_list() {
            self.recover_statement();
            self.finish();
            return;
        }
        if self.at(SyntaxKind::L_BRACE) {
            self.block();
        } else if self.starts_statement() {
            self.statement();
        } else {
            self.error("expected a module body");
            self.recover_statement();
        }
        self.finish();
    }

    fn include_statement(&mut self, kind: SyntaxKind) {
        self.start(kind);
        self.bump();
        if self.at(SyntaxKind::STRING) {
            self.bump();
        } else if self.at(SyntaxKind::LT) {
            self.bump();
            while !self.at(SyntaxKind::GT) && !self.at(SyntaxKind::EOF) {
                self.bump();
            }
            if !self.expect(SyntaxKind::GT) {
                self.recover_statement();
                self.finish();
                return;
            }
        } else {
            self.error("expected a quoted or angle-bracket path");
            self.recover_statement();
            self.finish();
            return;
        }
        if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        }
        self.finish();
    }

    fn function_declaration(&mut self) {
        self.start(SyntaxKind::FUNCTION_DECLARATION);
        self.bump();
        if !self.expect(SyntaxKind::IDENT)
            || !self.param_list()
            || !self.expect(SyntaxKind::EQ)
            || !self.expression()
            || !self.expect(SyntaxKind::SEMICOLON)
        {
            self.recover_statement();
        }
        self.finish();
    }

    fn param_list(&mut self) -> bool {
        self.start(SyntaxKind::PARAM_LIST);
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
            if !self.param() {
                self.finish();
                return false;
            }
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                if self.at(SyntaxKind::R_PAREN) {
                    self.bump();
                    self.finish();
                    return true;
                }
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

    fn param(&mut self) -> bool {
        self.start(SyntaxKind::PARAM);
        if !self.expect(SyntaxKind::IDENT) {
            self.finish();
            return false;
        }
        if self.at(SyntaxKind::EQ) {
            self.bump();
            if !self.expression() {
                self.finish();
                return false;
            }
        }
        self.finish();
        true
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
        } else if self.at(SyntaxKind::SEMICOLON) {
            self.bump();
        } else if self.starts_statement() {
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
            } else if self.starts_statement() {
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
            if self.at(SyntaxKind::SEMICOLON) {
                self.bump();
                continue;
            }
            if self.starts_statement() {
                self.statement();
            } else {
                self.error("expected a statement");
                self.recover_statement();
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
                if self.at(SyntaxKind::R_PAREN) {
                    self.bump();
                    self.finish();
                    return true;
                }
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
        let checkpoint = self.builder.checkpoint();
        if !self.binary_expr(0) {
            return false;
        }
        if self.at(SyntaxKind::QUESTION) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::CONDITIONAL_EXPR.into());
            self.bump();
            if !self.expression() {
                self.finish();
                return false;
            }
            if !self.expect(SyntaxKind::COLON) {
                self.finish();
                return false;
            }
            if !self.expression() {
                self.finish();
                return false;
            }
            self.finish();
        }
        true
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
            self.power_expr()
        }
    }

    fn power_expr(&mut self) -> bool {
        let checkpoint = self.builder.checkpoint();
        if !self.postfix() {
            return false;
        }
        if self.at(SyntaxKind::CARET) {
            self.builder
                .start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
            self.bump();
            if !self.unary_expr() {
                self.finish();
                return false;
            }
            self.finish();
        }
        true
    }

    fn postfix(&mut self) -> bool {
        let checkpoint = self.builder.checkpoint();
        if !self.primary() {
            return false;
        }
        loop {
            if self.at(SyntaxKind::L_BRACK) {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::INDEX_EXPR.into());
                self.bump();
                if !self.expression() {
                    self.finish();
                    return false;
                }
                if !self.expect(SyntaxKind::R_BRACK) {
                    self.finish();
                    return false;
                }
                self.finish();
            } else if self.at(SyntaxKind::DOT) && self.peek(1) == SyntaxKind::IDENT {
                self.builder
                    .start_node_at(checkpoint, SyntaxKind::DOT_EXPR.into());
                self.bump();
                self.bump();
                self.finish();
            } else {
                break;
            }
        }
        true
    }

    fn primary(&mut self) -> bool {
        match self.current() {
            SyntaxKind::IDENT => match self.current_text() {
                "let" if self.peek(1) == SyntaxKind::L_PAREN => self.let_expr(),
                "for" if self.peek(1) == SyntaxKind::L_PAREN => self.for_expr(),
                "if" if self.peek(1) == SyntaxKind::L_PAREN => self.if_expr(),
                "echo" if self.peek(1) == SyntaxKind::L_PAREN => {
                    self.prefixed_expr(SyntaxKind::ECHO_EXPR)
                }
                "assert" if self.peek(1) == SyntaxKind::L_PAREN => {
                    self.prefixed_expr(SyntaxKind::ASSERT_EXPR)
                }
                "each" if self.peek_starts_expression(1) => self.each_expr(),
                _ if self.peek(1) == SyntaxKind::L_PAREN => self.call_expr(),
                _ => {
                    self.bump();
                    true
                }
            },
            SyntaxKind::NUMBER
            | SyntaxKind::TRUE
            | SyntaxKind::FALSE
            | SyntaxKind::UNDEF
            | SyntaxKind::STRING => {
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

    fn each_expr(&mut self) -> bool {
        self.start(SyntaxKind::EACH_EXPR);
        self.bump();
        if !self.expression() {
            self.finish();
            return false;
        }
        self.finish();
        true
    }

    fn prefixed_expr(&mut self, kind: SyntaxKind) -> bool {
        self.start(kind);
        self.bump();
        if !self.arg_list() {
            self.finish();
            return false;
        }
        if self.starts_expression() && !self.expression() {
            self.finish();
            return false;
        }
        self.finish();
        true
    }

    fn let_expr(&mut self) -> bool {
        self.start(SyntaxKind::LET_EXPR);
        self.bump();
        if !self.arg_list() || !self.expression() {
            self.finish();
            return false;
        }
        self.finish();
        true
    }

    fn for_expr(&mut self) -> bool {
        if self.for_is_c_style() {
            return self.c_for_expr();
        }
        self.start(SyntaxKind::FOR_EXPR);
        self.bump();
        if !self.arg_list() || !self.expression() {
            self.finish();
            return false;
        }
        self.finish();
        true
    }

    fn for_is_c_style(&self) -> bool {
        let mut depth = 0usize;
        for token in self.tokens[self.pos..]
            .iter()
            .filter(|token| !token.kind.is_trivia())
        {
            match token.kind {
                SyntaxKind::L_PAREN | SyntaxKind::L_BRACK | SyntaxKind::L_BRACE => depth += 1,
                SyntaxKind::R_PAREN | SyntaxKind::R_BRACK | SyntaxKind::R_BRACE => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
                SyntaxKind::SEMICOLON if depth == 1 => return true,
                _ => {}
            }
        }
        false
    }

    fn c_for_expr(&mut self) -> bool {
        self.start(SyntaxKind::C_FOR_EXPR);
        self.bump();
        if !self.expect(SyntaxKind::L_PAREN) {
            self.finish();
            return false;
        }
        if !self.c_for_clause() || !self.expect(SyntaxKind::SEMICOLON) {
            self.finish();
            return false;
        }
        if !self.at(SyntaxKind::SEMICOLON) && !self.expression() {
            self.finish();
            return false;
        }
        if !self.expect(SyntaxKind::SEMICOLON) {
            self.finish();
            return false;
        }
        if !self.c_for_clause() || !self.expect(SyntaxKind::R_PAREN) || !self.expression() {
            self.finish();
            return false;
        }
        self.finish();
        true
    }

    fn c_for_clause(&mut self) -> bool {
        loop {
            if self.at(SyntaxKind::SEMICOLON) || self.at(SyntaxKind::R_PAREN) {
                return true;
            }
            if self.current() == SyntaxKind::IDENT && self.peek(1) == SyntaxKind::EQ {
                if !self.named_arg() {
                    return false;
                }
            } else {
                self.error("expected an assignment");
                return false;
            }
            if self.at(SyntaxKind::COMMA) {
                self.bump();
                continue;
            }
            return true;
        }
    }

    fn if_expr(&mut self) -> bool {
        self.start(SyntaxKind::IF_EXPR);
        self.bump();
        if !self.arg_list() {
            self.finish();
            return false;
        }
        if !self.expression() {
            self.finish();
            return false;
        }
        if self.at_keyword("else") {
            self.start(SyntaxKind::ELSE);
            self.bump();
            if !self.expression() {
                self.finish();
                self.finish();
                return false;
            }
            self.finish();
        }
        self.finish();
        true
    }

    fn call_expr(&mut self) -> bool {
        self.start(SyntaxKind::CALL_EXPR);
        self.bump();
        if !self.arg_list() {
            self.finish();
            return false;
        }
        self.finish();
        true
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
                if self.at(SyntaxKind::R_BRACK) {
                    self.bump();
                    self.finish();
                    return true;
                }
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

    fn starts_statement(&self) -> bool {
        matches!(
            self.current(),
            SyntaxKind::IDENT
                | SyntaxKind::L_BRACE
                | SyntaxKind::STAR
                | SyntaxKind::BANG
                | SyntaxKind::HASH
                | SyntaxKind::PERCENT
        )
    }

    fn starts_expression(&self) -> bool {
        self.peek_starts_expression(0)
    }

    fn peek_starts_expression(&self, n: usize) -> bool {
        matches!(
            self.peek(n),
            SyntaxKind::IDENT
                | SyntaxKind::NUMBER
                | SyntaxKind::TRUE
                | SyntaxKind::FALSE
                | SyntaxKind::UNDEF
                | SyntaxKind::STRING
                | SyntaxKind::L_BRACK
                | SyntaxKind::L_PAREN
                | SyntaxKind::PLUS
                | SyntaxKind::MINUS
                | SyntaxKind::BANG
        )
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

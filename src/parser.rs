use crate::ast::*;
use crate::token::{StrPart, Tok, Token};

pub struct Parser<'a> {
    tokens: Vec<Tok>,
    pos: usize,
    source: &'a str,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub pos: usize,
}

type PResult<T> = Result<T, ParseError>;

impl<'a> Parser<'a> {
    pub fn new(tokens: Vec<Tok>, source: &'a str) -> Self {
        Parser { tokens, pos: 0, source }
    }

    pub fn parse_program(&mut self) -> PResult<Program> {
        let mut items = Vec::new();
        while !self.check(&Token::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

    // ---- token stream helpers ----

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].kind
    }

    fn peek_start(&self) -> usize {
        self.tokens[self.pos].start
    }

    fn check(&self, kind: &Token) -> bool {
        std::mem::discriminant(self.peek()) == std::mem::discriminant(kind)
    }

    fn advance(&mut self) -> Tok {
        let tok = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: &Token) -> PResult<Tok> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error(&format!("expected {:?}, found {:?}", kind, self.peek())))
        }
    }

    fn expect_ident(&mut self) -> PResult<String> {
        match self.peek().clone() {
            Token::Ident(name) => {
                self.advance();
                Ok(name)
            }
            other => Err(self.error(&format!("expected identifier, found {:?}", other))),
        }
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError { message: message.to_string(), pos: self.peek_start() }
    }

    // ---- items ----

    fn parse_item(&mut self) -> PResult<Item> {
        match self.peek() {
            Token::Use => self.parse_use(),
            Token::Ext => self.parse_ext(),
            Token::Typ => self.parse_typ(),
            Token::Iface => self.parse_iface(),
            Token::Enum => self.parse_enum(),
            Token::Def => self.parse_fn(false),
            Token::Link => self.parse_fn(true),
            _ => Ok(Item::Stmt(self.parse_stmt()?)),
        }
    }

    fn parse_use(&mut self) -> PResult<Item> {
        self.expect(&Token::Use)?;
        let name = self.expect_ident()?;
        Ok(Item::Use(name))
    }

    fn parse_ext(&mut self) -> PResult<Item> {
        self.expect(&Token::Ext)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Arrow)?;
        let ret_type = self.parse_type()?;
        self.expect(&Token::Do)?;

        let mut targets = Vec::new();
        while !self.check(&Token::End) {
            match self.peek().clone() {
                Token::ExtTarget(name, raw) => {
                    self.advance();
                    targets.push((name, raw));
                }
                other => return Err(self.error(&format!("expected a target mapping (e.g. 'py: ...'), found {:?}", other))),
            }
        }
        self.expect(&Token::End)?;
        Ok(Item::Ext(ExtDecl { name, params, ret_type, targets }))
    }

    fn parse_typ(&mut self) -> PResult<Item> {
        self.expect(&Token::Typ)?;
        let name = self.expect_ident()?;
        let implements = if self.check(&Token::Is) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        self.expect(&Token::Do)?;
        let mut fields = Vec::new();
        while !self.check(&Token::End) {
            let fname = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            fields.push(Param { name: fname, ty });
        }
        self.expect(&Token::End)?;
        Ok(Item::Typ(TypDecl { name, implements, fields }))
    }

    fn parse_iface(&mut self) -> PResult<Item> {
        self.expect(&Token::Iface)?;
        let name = self.expect_ident()?;
        self.expect(&Token::Do)?;
        let mut methods = Vec::new();
        while !self.check(&Token::End) {
            let mname = self.expect_ident()?;
            self.expect(&Token::LParen)?;
            let params = self.parse_params()?;
            self.expect(&Token::RParen)?;
            self.expect(&Token::Arrow)?;
            let ret_type = self.parse_type()?;
            methods.push(MethodSig { name: mname, params, ret_type });
        }
        self.expect(&Token::End)?;
        Ok(Item::Iface(IfaceDecl { name, methods }))
    }

    /// `enum Name do Variant ... end` — bare variant names, no payload.
    fn parse_enum(&mut self) -> PResult<Item> {
        self.expect(&Token::Enum)?;
        let name = self.expect_ident()?;
        self.expect(&Token::Do)?;
        let mut variants = Vec::new();
        while !self.check(&Token::End) {
            variants.push(self.expect_ident()?);
        }
        self.expect(&Token::End)?;
        Ok(Item::Enum(EnumDecl { name, variants }))
    }

    /// Parses both `def` and `link` — identical shape, except `link` forbids
    /// generics (a wire contract needs a concrete, enumerable type shape) and
    /// sets `is_link` so codegen additionally emits a wire handler + remote
    /// client stub (see ast.rs, SPEC.md §19).
    fn parse_fn(&mut self, is_link: bool) -> PResult<Item> {
        self.expect(if is_link { &Token::Link } else { &Token::Def })?;
        let name = self.expect_ident()?;
        let mut generics = Vec::new();
        if self.check(&Token::Lt) {
            if is_link {
                return Err(self.error("`link` does not support generics — a wire contract needs a concrete type shape"));
            }
            self.advance();
            loop {
                generics.push(self.expect_ident()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(&Token::Gt)?;
        }
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Arrow)?;
        let ret_type = self.parse_type()?;
        let fallible = if self.check(&Token::Question) {
            self.advance();
            true
        } else {
            false
        };
        self.expect(&Token::Do)?;
        let body = self.parse_stmts_until(&[Token::End])?;
        self.expect(&Token::End)?;
        Ok(Item::Def(FnDecl { name, generics, params, ret_type, fallible, body, is_link }))
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            let name = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            params.push(Param { name, ty });
            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> PResult<Type> {
        let name = self.expect_ident()?;
        if self.check(&Token::Lt) {
            self.advance();
            let mut args = Vec::new();
            loop {
                args.push(self.parse_type()?);
                if self.check(&Token::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(&Token::Gt)?;
            Ok(Type::Generic(name, args))
        } else {
            Ok(Type::Named(name))
        }
    }

    // ---- statements ----

    fn stops_at(&self, stops: &[Token]) -> bool {
        stops.iter().any(|s| self.check(s)) || self.check(&Token::Eof)
    }

    fn parse_stmts_until(&mut self, stops: &[Token]) -> PResult<Vec<Stmt>> {
        let mut stmts = Vec::new();
        while !self.stops_at(stops) {
            stmts.push(self.parse_stmt()?);
        }
        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        match self.peek() {
            Token::Let => {
                self.advance();
                let name = self.expect_ident()?;
                let ty = self.parse_optional_type_annotation()?;
                self.expect(&Token::Eq)?;
                let value = self.parse_expr()?;
                Ok(Stmt::Let { name, ty, value })
            }
            Token::Mut => {
                self.advance();
                let name = self.expect_ident()?;
                let ty = self.parse_optional_type_annotation()?;
                self.expect(&Token::Eq)?;
                let value = self.parse_expr()?;
                Ok(Stmt::Mut { name, ty, value })
            }
            Token::Ret => {
                self.advance();
                if self.stops_at(&[Token::End, Token::Els]) {
                    Ok(Stmt::Ret(None))
                } else {
                    Ok(Stmt::Ret(Some(self.parse_expr()?)))
                }
            }
            Token::Fail => {
                self.advance();
                Ok(Stmt::Fail(self.parse_expr()?))
            }
            Token::If => self.parse_if(),
            Token::For => {
                self.advance();
                let first = self.expect_ident()?;
                let second = if self.check(&Token::Comma) {
                    self.advance();
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                self.expect(&Token::In)?;
                let iter = self.parse_expr()?;
                self.expect(&Token::Do)?;
                let body = self.parse_stmts_until(&[Token::End])?;
                self.expect(&Token::End)?;
                Ok(Stmt::For { binding: (first, second), iter, body })
            }
            Token::Whl => {
                self.advance();
                let cond = self.parse_expr()?;
                self.expect(&Token::Do)?;
                let body = self.parse_stmts_until(&[Token::End])?;
                self.expect(&Token::End)?;
                Ok(Stmt::Whl { cond, body })
            }
            Token::Ellipsis => {
                self.advance();
                Ok(Stmt::Todo)
            }
            _ => {
                let expr = self.parse_expr()?;
                if self.check(&Token::Eq) {
                    self.advance();
                    let value = self.parse_expr()?;
                    Ok(Stmt::Assign { target: expr, value })
                } else {
                    Ok(Stmt::ExprStmt(expr))
                }
            }
        }
    }

    fn parse_optional_type_annotation(&mut self) -> PResult<Option<Type>> {
        if self.check(&Token::Colon) {
            self.advance();
            Ok(Some(self.parse_type()?))
        } else {
            Ok(None)
        }
    }

    /// `if cond do ... [els ...] end` — note there's a single closing `end`
    /// for the whole statement; `els` never gets its own `do`/`end`.
    fn parse_if(&mut self) -> PResult<Stmt> {
        self.expect(&Token::If)?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Do)?;
        let then_body = self.parse_stmts_until(&[Token::End, Token::Els])?;
        let else_body = if self.check(&Token::Els) {
            self.advance();
            Some(self.parse_stmts_until(&[Token::End])?)
        } else {
            None
        };
        self.expect(&Token::End)?;
        Ok(Stmt::If { cond, then_body, else_body })
    }

    // ---- expressions (precedence climbing) ----

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_and()?;
        while self.check(&Token::Or) {
            self.advance();
            let rhs = self.parse_and()?;
            lhs = Expr::Binary { op: BinOp::Or, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_equality()?;
        while self.check(&Token::And) {
            self.advance();
            let rhs = self.parse_equality()?;
            lhs = Expr::Binary { op: BinOp::And, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_equality(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinOp::Eq,
                Token::NotEq => BinOp::Ne,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_comparison()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_additive()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::LtEq => BinOp::Le,
                Token::GtEq => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_additive()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_additive(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_multiplicative()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_multiplicative()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_multiplicative(&mut self) -> PResult<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        match self.peek() {
            Token::Not => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr::Unary { op: UnOp::Not, expr: Box::new(e) })
            }
            Token::Minus => {
                self.advance();
                let e = self.parse_unary()?;
                Ok(Expr::Unary { op: UnOp::Neg, expr: Box::new(e) })
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.peek() {
                Token::LParen => {
                    self.advance();
                    let args = self.parse_expr_list(&Token::RParen)?;
                    self.expect(&Token::RParen)?;
                    expr = Expr::Call { callee: Box::new(expr), args };
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index { base: Box::new(expr), index: Box::new(index) };
                }
                Token::Dot => {
                    self.advance();
                    let name = self.expect_ident()?;
                    expr = Expr::Field { base: Box::new(expr), name };
                }
                Token::DQuestion => {
                    self.advance();
                    self.expect(&Token::Do)?;
                    let handler = self.parse_stmts_until(&[Token::End])?;
                    self.expect(&Token::End)?;
                    expr = Expr::Unwrap { expr: Box::new(expr), handler };
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_expr_list(&mut self, end: &Token) -> PResult<Vec<Expr>> {
        let mut items = Vec::new();
        while !self.check(end) {
            items.push(self.parse_expr()?);
            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(items)
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        match self.peek().clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Expr::Int(n))
            }
            Token::Float(f) => {
                self.advance();
                Ok(Expr::Float(f))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Token::NoneLit => {
                self.advance();
                Ok(Expr::NoneLit)
            }
            Token::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            Token::InterpStr(parts) => {
                self.advance();
                let mut out = Vec::new();
                for part in parts {
                    match part {
                        StrPart::Text(t) => out.push(StrPartExpr::Text(t)),
                        StrPart::Expr(toks) => {
                            let mut sub_tokens = toks;
                            sub_tokens.push(Tok { kind: Token::Eof, start: 0, end: 0 });
                            let mut sub_parser = Parser::new(sub_tokens, self.source);
                            let e = sub_parser.parse_expr()?;
                            out.push(StrPartExpr::Expr(e));
                        }
                    }
                }
                Ok(Expr::InterpStr(out))
            }
            Token::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name))
            }
            Token::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            Token::LBracket => {
                self.advance();
                let items = self.parse_expr_list(&Token::RBracket)?;
                self.expect(&Token::RBracket)?;
                Ok(Expr::List(items))
            }
            Token::LBrace => {
                self.advance();
                let mut pairs = Vec::new();
                while !self.check(&Token::RBrace) {
                    let key = self.parse_expr()?;
                    self.expect(&Token::Colon)?;
                    let value = self.parse_expr()?;
                    pairs.push((key, value));
                    if self.check(&Token::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(Expr::Map(pairs))
            }
            other => Err(self.error(&format!("unexpected token in expression: {:?}", other))),
        }
    }
}

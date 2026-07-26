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
        Parser {
            tokens,
            pos: 0,
            source,
        }
    }

    pub fn parse_program(&mut self) -> PResult<Program> {
        let mut items = Vec::new();
        while !self.check(&Token::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

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

    fn expect_ident(&mut self) -> PResult<(String, Span)> {
        match self.peek().clone() {
            Token::Ident(name) => {
                let tok = self.advance();
                Ok((name, Span::new(tok.start, tok.end)))
            }
            other => Err(self.error(&format!("expected identifier, found {:?}", other))),
        }
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            pos: self.peek_start(),
        }
    }

    fn expr(&self, kind: ExprKind, span: Span) -> Expr {
        Expr { kind, span }
    }

    fn stmt(&self, kind: StmtKind, span: Span) -> Stmt {
        Stmt { kind, span }
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
        let (name, _) = self.expect_ident()?;
        Ok(Item::Use(name))
    }

    fn parse_ext(&mut self) -> PResult<Item> {
        self.expect(&Token::Ext)?;
        let (name, name_span) = self.expect_ident()?;
        self.expect(&Token::LParen)?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen)?;
        self.expect(&Token::Arrow)?;
        let ret_type = self.parse_type()?;
        self.expect(&Token::Do)?;

        let mut targets = Vec::new();
        while !self.check(&Token::End) {
            match self.peek().clone() {
                Token::ExtTarget(tname, raw) => {
                    self.advance();
                    targets.push((tname, raw));
                }
                other => {
                    return Err(self.error(&format!(
                        "expected a target mapping (e.g. 'py: ...'), found {:?}",
                        other
                    )))
                }
            }
        }
        self.expect(&Token::End)?;
        Ok(Item::Ext(ExtDecl {
            name,
            name_span,
            params,
            ret_type,
            targets,
        }))
    }

    fn parse_typ(&mut self) -> PResult<Item> {
        self.expect(&Token::Typ)?;
        let (name, name_span) = self.expect_ident()?;
        let implements = if self.check(&Token::Is) {
            self.advance();
            Some(self.expect_ident()?.0)
        } else {
            None
        };
        self.expect(&Token::Do)?;
        let mut fields = Vec::new();
        while !self.check(&Token::End) {
            let (fname, fspan) = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            let end = self.tokens[self.pos.saturating_sub(1)].end;
            fields.push(Param {
                name: fname,
                ty,
                span: Span::new(fspan.start, end),
            });
        }
        self.expect(&Token::End)?;
        Ok(Item::Typ(TypDecl {
            name,
            name_span,
            implements,
            fields,
        }))
    }

    fn parse_iface(&mut self) -> PResult<Item> {
        self.expect(&Token::Iface)?;
        let (name, name_span) = self.expect_ident()?;
        self.expect(&Token::Do)?;
        let mut methods = Vec::new();
        while !self.check(&Token::End) {
            let (mname, _) = self.expect_ident()?;
            self.expect(&Token::LParen)?;
            let params = self.parse_params()?;
            self.expect(&Token::RParen)?;
            self.expect(&Token::Arrow)?;
            let ret_type = self.parse_type()?;
            methods.push(MethodSig {
                name: mname,
                params,
                ret_type,
            });
        }
        self.expect(&Token::End)?;
        Ok(Item::Iface(IfaceDecl {
            name,
            name_span,
            methods,
        }))
    }

    fn parse_enum(&mut self) -> PResult<Item> {
        self.expect(&Token::Enum)?;
        let (name, name_span) = self.expect_ident()?;
        self.expect(&Token::Do)?;
        let mut variants = Vec::new();
        while !self.check(&Token::End) {
            variants.push(self.expect_ident()?.0);
        }
        self.expect(&Token::End)?;
        Ok(Item::Enum(EnumDecl {
            name,
            name_span,
            variants,
        }))
    }

    fn parse_fn(&mut self, is_link: bool) -> PResult<Item> {
        self.expect(if is_link { &Token::Link } else { &Token::Def })?;
        let (name, name_span) = self.expect_ident()?;
        let mut generics = Vec::new();
        if self.check(&Token::Lt) {
            if is_link {
                return Err(self.error(
                    "`link` does not support generics — a wire contract needs a concrete type shape",
                ));
            }
            self.advance();
            loop {
                generics.push(self.expect_ident()?.0);
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
        let end_tok = self.expect(&Token::End)?;
        Ok(Item::Def(FnDecl {
            name,
            name_span: Span::new(name_span.start, end_tok.end),
            generics,
            params,
            ret_type,
            fallible,
            body,
            is_link,
        }))
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        let mut params = Vec::new();
        while !self.check(&Token::RParen) {
            let (name, nspan) = self.expect_ident()?;
            self.expect(&Token::Colon)?;
            let ty = self.parse_type()?;
            let end = self.tokens[self.pos.saturating_sub(1)].end;
            params.push(Param {
                name,
                ty,
                span: Span::new(nspan.start, end),
            });
            if self.check(&Token::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> PResult<Type> {
        let (name, _) = self.expect_ident()?;
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
        let start = self.peek_start();
        match self.peek() {
            Token::Let => {
                self.advance();
                let (name, _) = self.expect_ident()?;
                let ty = self.parse_optional_type_annotation()?;
                self.expect(&Token::Eq)?;
                let value = self.parse_expr()?;
                let span = Span::new(start, value.span.end);
                Ok(self.stmt(StmtKind::Let { name, ty, value }, span))
            }
            Token::Mut => {
                self.advance();
                let (name, _) = self.expect_ident()?;
                let ty = self.parse_optional_type_annotation()?;
                self.expect(&Token::Eq)?;
                let value = self.parse_expr()?;
                let span = Span::new(start, value.span.end);
                Ok(self.stmt(StmtKind::Mut { name, ty, value }, span))
            }
            Token::Ret => {
                self.advance();
                if self.stops_at(&[Token::End, Token::Els]) {
                    Ok(self.stmt(StmtKind::Ret(None), Span::new(start, self.peek_start())))
                } else {
                    let e = self.parse_expr()?;
                    let span = Span::new(start, e.span.end);
                    Ok(self.stmt(StmtKind::Ret(Some(e)), span))
                }
            }
            Token::Fail => {
                self.advance();
                let e = self.parse_expr()?;
                let span = Span::new(start, e.span.end);
                Ok(self.stmt(StmtKind::Fail(e), span))
            }
            Token::If => self.parse_if(),
            Token::For => {
                self.advance();
                let (first, _) = self.expect_ident()?;
                let second = if self.check(&Token::Comma) {
                    self.advance();
                    Some(self.expect_ident()?.0)
                } else {
                    None
                };
                self.expect(&Token::In)?;
                let iter = self.parse_expr()?;
                self.expect(&Token::Do)?;
                let body = self.parse_stmts_until(&[Token::End])?;
                let end_tok = self.expect(&Token::End)?;
                Ok(self.stmt(
                    StmtKind::For {
                        binding: (first, second),
                        iter,
                        body,
                    },
                    Span::new(start, end_tok.end),
                ))
            }
            Token::Whl => {
                self.advance();
                let cond = self.parse_expr()?;
                self.expect(&Token::Do)?;
                let body = self.parse_stmts_until(&[Token::End])?;
                let end_tok = self.expect(&Token::End)?;
                Ok(self.stmt(
                    StmtKind::Whl { cond, body },
                    Span::new(start, end_tok.end),
                ))
            }
            Token::Ellipsis => {
                let tok = self.advance();
                Ok(self.stmt(StmtKind::Todo, Span::new(tok.start, tok.end)))
            }
            _ => {
                let expr = self.parse_expr()?;
                if self.check(&Token::Eq) {
                    self.advance();
                    let value = self.parse_expr()?;
                    let span = Span::new(expr.span.start, value.span.end);
                    Ok(self.stmt(
                        StmtKind::Assign {
                            target: expr,
                            value,
                        },
                        span,
                    ))
                } else {
                    let span = expr.span;
                    Ok(self.stmt(StmtKind::ExprStmt(expr), span))
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

    fn parse_if(&mut self) -> PResult<Stmt> {
        let start = self.peek_start();
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
        let end_tok = self.expect(&Token::End)?;
        Ok(self.stmt(
            StmtKind::If {
                cond,
                then_body,
                else_body,
            },
            Span::new(start, end_tok.end),
        ))
    }

    // ---- expressions ----

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_or()
    }

    fn bin_fold(
        &mut self,
        mut lhs: Expr,
        mut parse_rhs: impl FnMut(&mut Self) -> PResult<Expr>,
        op_of: impl Fn(&Token) -> Option<BinOp>,
    ) -> PResult<Expr> {
        while let Some(op) = op_of(self.peek()) {
            self.advance();
            let rhs = parse_rhs(self)?;
            let span = lhs.span.union(rhs.span);
            lhs = self.expr(
                ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            );
        }
        Ok(lhs)
    }

    fn parse_or(&mut self) -> PResult<Expr> {
        let lhs = self.parse_and()?;
        self.bin_fold(lhs, |p| p.parse_and(), |t| {
            if matches!(t, Token::Or) {
                Some(BinOp::Or)
            } else {
                None
            }
        })
    }

    fn parse_and(&mut self) -> PResult<Expr> {
        let lhs = self.parse_equality()?;
        self.bin_fold(lhs, |p| p.parse_equality(), |t| {
            if matches!(t, Token::And) {
                Some(BinOp::And)
            } else {
                None
            }
        })
    }

    fn parse_equality(&mut self) -> PResult<Expr> {
        let lhs = self.parse_comparison()?;
        self.bin_fold(lhs, |p| p.parse_comparison(), |t| match t {
            Token::EqEq => Some(BinOp::Eq),
            Token::NotEq => Some(BinOp::Ne),
            _ => None,
        })
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let lhs = self.parse_additive()?;
        self.bin_fold(lhs, |p| p.parse_additive(), |t| match t {
            Token::Lt => Some(BinOp::Lt),
            Token::Gt => Some(BinOp::Gt),
            Token::LtEq => Some(BinOp::Le),
            Token::GtEq => Some(BinOp::Ge),
            _ => None,
        })
    }

    fn parse_additive(&mut self) -> PResult<Expr> {
        let lhs = self.parse_multiplicative()?;
        self.bin_fold(lhs, |p| p.parse_multiplicative(), |t| match t {
            Token::Plus => Some(BinOp::Add),
            Token::Minus => Some(BinOp::Sub),
            _ => None,
        })
    }

    fn parse_multiplicative(&mut self) -> PResult<Expr> {
        let lhs = self.parse_unary()?;
        self.bin_fold(lhs, |p| p.parse_unary(), |t| match t {
            Token::Star => Some(BinOp::Mul),
            Token::Slash => Some(BinOp::Div),
            Token::Percent => Some(BinOp::Mod),
            _ => None,
        })
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        match self.peek() {
            Token::Not => {
                let start = self.peek_start();
                self.advance();
                let e = self.parse_unary()?;
                let span = Span::new(start, e.span.end);
                Ok(self.expr(
                    ExprKind::Unary {
                        op: UnOp::Not,
                        expr: Box::new(e),
                    },
                    span,
                ))
            }
            Token::Minus => {
                let start = self.peek_start();
                self.advance();
                let e = self.parse_unary()?;
                let span = Span::new(start, e.span.end);
                Ok(self.expr(
                    ExprKind::Unary {
                        op: UnOp::Neg,
                        expr: Box::new(e),
                    },
                    span,
                ))
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
                    let rparen = self.expect(&Token::RParen)?;
                    let span = Span::new(expr.span.start, rparen.end);
                    expr = self.expr(
                        ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                        span,
                    );
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_expr()?;
                    let rb = self.expect(&Token::RBracket)?;
                    let span = Span::new(expr.span.start, rb.end);
                    expr = self.expr(
                        ExprKind::Index {
                            base: Box::new(expr),
                            index: Box::new(index),
                        },
                        span,
                    );
                }
                Token::Dot => {
                    self.advance();
                    let (name, nspan) = self.expect_ident()?;
                    let span = Span::new(expr.span.start, nspan.end);
                    expr = self.expr(
                        ExprKind::Field {
                            base: Box::new(expr),
                            name,
                        },
                        span,
                    );
                }
                Token::DQuestion => {
                    let start = expr.span.start;
                    self.advance();
                    self.expect(&Token::Do)?;
                    let handler = self.parse_stmts_until(&[Token::End])?;
                    let end_tok = self.expect(&Token::End)?;
                    expr = self.expr(
                        ExprKind::Unwrap {
                            expr: Box::new(expr),
                            handler,
                        },
                        Span::new(start, end_tok.end),
                    );
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
                let tok = self.advance();
                Ok(self.expr(ExprKind::Int(n), Span::new(tok.start, tok.end)))
            }
            Token::Float(f) => {
                let tok = self.advance();
                Ok(self.expr(ExprKind::Float(f), Span::new(tok.start, tok.end)))
            }
            Token::True => {
                let tok = self.advance();
                Ok(self.expr(ExprKind::Bool(true), Span::new(tok.start, tok.end)))
            }
            Token::False => {
                let tok = self.advance();
                Ok(self.expr(ExprKind::Bool(false), Span::new(tok.start, tok.end)))
            }
            Token::NoneLit => {
                let tok = self.advance();
                Ok(self.expr(ExprKind::NoneLit, Span::new(tok.start, tok.end)))
            }
            Token::Str(s) => {
                let tok = self.advance();
                Ok(self.expr(ExprKind::Str(s), Span::new(tok.start, tok.end)))
            }
            Token::InterpStr(parts) => {
                let tok = self.advance();
                let mut out = Vec::new();
                for part in parts {
                    match part {
                        StrPart::Text(t) => out.push(StrPartExpr::Text(t)),
                        StrPart::Expr(toks) => {
                            let mut sub_tokens = toks;
                            sub_tokens.push(Tok {
                                kind: Token::Eof,
                                start: 0,
                                end: 0,
                            });
                            let mut sub_parser = Parser::new(sub_tokens, self.source);
                            let e = sub_parser.parse_expr()?;
                            out.push(StrPartExpr::Expr(e));
                        }
                    }
                }
                Ok(self.expr(ExprKind::InterpStr(out), Span::new(tok.start, tok.end)))
            }
            Token::Ident(name) => {
                let tok = self.advance();
                Ok(self.expr(ExprKind::Ident(name), Span::new(tok.start, tok.end)))
            }
            Token::LParen => {
                self.advance();
                let e = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(e)
            }
            Token::LBracket => {
                let start = self.peek_start();
                self.advance();
                let items = self.parse_expr_list(&Token::RBracket)?;
                let end_tok = self.expect(&Token::RBracket)?;
                Ok(self.expr(ExprKind::List(items), Span::new(start, end_tok.end)))
            }
            Token::LBrace => {
                let start = self.peek_start();
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
                let end_tok = self.expect(&Token::RBrace)?;
                Ok(self.expr(ExprKind::Map(pairs), Span::new(start, end_tok.end)))
            }
            other => Err(self.error(&format!("unexpected token in expression: {:?}", other))),
        }
    }
}

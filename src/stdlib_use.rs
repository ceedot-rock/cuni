//! Wave-1 stdlib usage scan (docs/STDLIB.md).
//!
//! Backends whose imports are compile-checked (Go: unused imports are a
//! compile error) need to know whether a program touches the wave-1 stdlib
//! before emitting. One shared AST walk answers for every backend.

use crate::ast::*;

#[derive(Default, Debug, Clone, Copy)]
pub struct StdlibUse {
    pub json: bool,
    pub time: bool,
    pub strings: bool,
    pub sha: bool,
}

impl StdlibUse {
    pub fn any(&self) -> bool {
        self.json || self.time || self.strings || self.sha
    }
}

pub fn scan(program: &Program) -> StdlibUse {
    let mut u = StdlibUse::default();
    for item in &program.items {
        match item {
            Item::Def(f) => {
                for s in &f.body {
                    scan_stmt(s, &mut u);
                }
            }
            Item::Stmt(s) => scan_stmt(s, &mut u),
            Item::Ext(e) => {
                for (_, body) in &e.targets {
                    // ext bodies are raw target text; a wave-1 name inside
                    // is the author's business, not the stdlib's.
                    let _ = body;
                }
            }
            _ => {}
        }
    }
    u
}

fn scan_stmt(s: &Stmt, u: &mut StdlibUse) {
    match &s.kind {
        StmtKind::Let { value, .. } | StmtKind::Mut { value, .. } => scan_expr(value, u),
        StmtKind::Assign { target, value } => {
            scan_expr(target, u);
            scan_expr(value, u);
        }
        StmtKind::Ret(e) => {
            if let Some(e) = e {
                scan_expr(e, u);
            }
        }
        StmtKind::Fail(e) => scan_expr(e, u),
        StmtKind::If {
            cond,
            then_body,
            else_body,
        } => {
            scan_expr(cond, u);
            for s in then_body {
                scan_stmt(s, u);
            }
            if let Some(b) = else_body {
                for s in b {
                    scan_stmt(s, u);
                }
            }
        }
        StmtKind::For { iter, body, .. } => {
            scan_expr(iter, u);
            for s in body {
                scan_stmt(s, u);
            }
        }
        StmtKind::Whl { cond, body } => {
            scan_expr(cond, u);
            for s in body {
                scan_stmt(s, u);
            }
        }
        StmtKind::ExprStmt(e) => scan_expr(e, u),
        StmtKind::Todo => {}
    }
}

/// The callee of a call can itself contain expressions (field bases).
fn scan_callee(callee: &Expr, u: &mut StdlibUse) {
    match &callee.kind {
        ExprKind::Ident(n) if n == "sha256" => u.sha = true,
        ExprKind::Field { base, name } => {
            if let ExprKind::Ident(ns) = &base.kind {
                if ns == "json" {
                    u.json = true;
                } else if ns == "time" {
                    u.time = true;
                }
            }
            if matches!(name.as_str(), "split" | "join" | "trim" | "contains") {
                u.strings = true;
            }
            scan_expr(base, u);
        }
        _ => {}
    }
}

fn scan_expr(e: &Expr, u: &mut StdlibUse) {
    match &e.kind {
        ExprKind::Call { callee, args } => {
            scan_callee(callee, u);
            for a in args {
                scan_expr(a.expr(), u);
            }
        }
        ExprKind::InterpStr(parts) => {
            for p in parts {
                if let StrPartExpr::Expr(e) = p {
                    scan_expr(e, u);
                }
            }
        }
        ExprKind::List(xs) => {
            for x in xs {
                scan_expr(x, u);
            }
        }
        ExprKind::Map(pairs) => {
            for (k, v) in pairs {
                scan_expr(k, u);
                scan_expr(v, u);
            }
        }
        ExprKind::Index { base, index } => {
            scan_expr(base, u);
            scan_expr(index, u);
        }
        ExprKind::Field { base, .. } => scan_expr(base, u),
        ExprKind::Binary { lhs, rhs, .. } => {
            scan_expr(lhs, u);
            scan_expr(rhs, u);
        }
        ExprKind::Unary { expr, .. } => scan_expr(expr, u),
        ExprKind::Unwrap { expr, handler } => {
            scan_expr(expr, u);
            for s in handler {
                scan_stmt(s, u);
            }
        }
        _ => {}
    }
}

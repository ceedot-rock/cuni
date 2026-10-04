//! Tcl backend — handwritten imperative native seat.
//!
//! Emits real Tcl: `set` bindings, `proc` defs, `while {[expr ...]} {...}}`,
//! `cuni_say` helper (bool → `True`/`False`), `cuni_mod` for Python-floored
//! `%` (Tcl's `%` truncates like C; its `/` already truncates toward zero,
//! which matches CuNi). Expression fragments are bare expr-text words (no
//! spaces outside quoted strings) so they nest safely inside `[expr ...]`
//! without brace-quoting hazards. Refuses everything beyond the core
//! subset (see `crate::codegen_core` docs).

use crate::ast::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    Int,
    Str,
    Bool,
}

fn ty_of(t: &Type) -> Result<Ty, String> {
    match t {
        Type::Named(n) if n == "int" => Ok(Ty::Int),
        Type::Named(n) if n == "str" => Ok(Ty::Str),
        Type::Named(n) if n == "bool" => Ok(Ty::Bool),
        _ => Err("type beyond core subset; refusing".into()),
    }
}

/// Double-quoted Tcl string literal; escapes every substitution hazard.
fn tcl_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            '$' => r.push_str("\\$"),
            '[' => r.push_str("\\["),
            ']' => r.push_str("\\]"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\x{:02x}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

const HEADER: &str = r#"proc cuni_say {x ty} {
    if {$ty eq "bool"} {
        if {$x} {puts "True"} else {puts "False"}
    } else {
        puts $x
    }
}
proc cuni_div {a b} {
    set a [expr $a]
    set b [expr $b]
    set q [expr {$a / $b}]
    if {($a % $b) != 0 && (($a < 0) != ($b < 0))} {
        incr q
    }
    return $q
}
"#;

struct Gen {
    out: String,
    level: usize,
    fns: HashSet<String>,
    fn_ret: HashMap<String, Ty>,
    scope: HashMap<String, Ty>,
    in_def: bool,
}

impl Gen {
    fn line(&mut self, text: &str) {
        if !text.is_empty() {
            self.out.push_str(&"    ".repeat(self.level));
            self.out.push_str(text);
        }
        self.out.push('\n');
    }

    /// Emit an expression as a bare Tcl word (no spaces outside quotes).
    /// int/bool fragments are expr-text; str fragments are quoted words or
    /// `[string cat ...]` / `[name ...]` substitutions.
    fn expr(&mut self, e: &Expr) -> Result<(String, Ty), String> {
        match &e.kind {
            ExprKind::Int(n) => Ok((n.to_string(), Ty::Int)),
            ExprKind::Bool(b) => Ok((if *b { "1" } else { "0" }.to_string(), Ty::Bool)),
            ExprKind::Str(s) => Ok((tcl_str(s), Ty::Str)),
            ExprKind::Float(_) => Err("float: beyond core subset; refusing".into()),
            ExprKind::Dec(_) => Err("dec: beyond core subset; refusing".into()),
            ExprKind::Time(_) => Err("time: beyond core subset; refusing".into()),
            ExprKind::NoneLit => Err("none: beyond core subset; refusing".into()),
            ExprKind::Ident(n) => {
                let t = self
                    .scope
                    .get(n)
                    .copied()
                    .ok_or_else(|| format!("unknown variable `{}`: refusing", n))?;
                Ok((format!("${}", n), t))
            }
            ExprKind::InterpStr(_) => Err("interpolated string: beyond core subset; refusing".into()),
            ExprKind::List(_) => Err("list: beyond core subset; refusing".into()),
            ExprKind::Map(_) => Err("map: beyond core subset; refusing".into()),
            ExprKind::Index { .. } => Err("indexing: beyond core subset; refusing".into()),
            ExprKind::Field { .. } => Err("field access: beyond core subset; refusing".into()),
            ExprKind::Unwrap { .. } => Err("??: beyond core subset; refusing".into()),
            ExprKind::Call { callee, args } => {
                let name = match &callee.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => return Err("indirect call: beyond core subset; refusing".into()),
                };
                if name == "say" {
                    return Err("say: use as a statement, not an expression; refusing".into());
                }
                let ret = self.fn_ret.get(&name).copied().ok_or_else(|| {
                    format!("call to unknown function `{}`: refusing", name)
                })?;
                let mut parts = vec![name];
                for a in args {
                    if a.is_named() {
                        return Err("named arguments: beyond core subset; refusing".into());
                    }
                    let (frag, aty) = self.expr(a.expr())?;
                    // int/bool fragments are raw expr-text: evaluate them
                    // before the call, or the proc would receive the
                    // source text unevaluated. str fragments are already
                    // values (quoted words / substitutions).
                    match aty {
                        Ty::Str => parts.push(frag),
                        _ => parts.push(format!("[expr {}]", frag)),
                    }
                }
                Ok((format!("[{}]", parts.join(" ")), ret))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (l, lt) = self.expr(lhs)?;
                let (r, rt) = self.expr(rhs)?;
                let (frag, ty) = match op {
                    BinOp::Add => {
                        if lt == Ty::Str && rt == Ty::Str {
                            (format!("[string cat {} {}]", l, r), Ty::Str)
                        } else if lt == Ty::Int && rt == Ty::Int {
                            (format!("({}+{})", l, r), Ty::Int)
                        } else {
                            return Err("mixed-type `+`: beyond core subset; refusing".into());
                        }
                    }
                    BinOp::Sub => {
                        Self::ints(lt, rt, "-")?;
                        (format!("({}-{})", l, r), Ty::Int)
                    }
                    BinOp::Mul => {
                        Self::ints(lt, rt, "*")?;
                        (format!("({}*{})", l, r), Ty::Int)
                    }
                    // Tcl integer `/` floors; CuNi wants truncation toward
                    // zero. cuni_div evaluates its expr-text args first (a
                    // proc would otherwise receive them unevaluated).
                    BinOp::Div => {
                        Self::ints(lt, rt, "/")?;
                        (format!("[cuni_div {} {}]", l, r), Ty::Int)
                    }
                    // Tcl `%` truncates like C; CuNi wants Python-floored.
                    // Inlined as one expr so operands evaluate (a proc
                    // would receive the raw expr-text unevaluated).
                    BinOp::Mod => {
                        Self::ints(lt, rt, "%")?;
                        (format!("((({})%({})+({}))%({}))", l, r, r, r), Ty::Int)
                    }
                    BinOp::Eq => (format!("({}{}{})", l, Self::eqop(lt, rt, "==", "eq")?, r), Ty::Bool),
                    BinOp::Ne => (format!("({}{}{})", l, Self::eqop(lt, rt, "!=", "ne")?, r), Ty::Bool),
                    BinOp::Lt => (format!("({}{}{})", l, Self::eqop(lt, rt, "<", "<")?, r), Ty::Bool),
                    BinOp::Gt => (format!("({}{}{})", l, Self::eqop(lt, rt, ">", ">")?, r), Ty::Bool),
                    BinOp::Le => (format!("({}{}{})", l, Self::eqop(lt, rt, "<=", "<=")?, r), Ty::Bool),
                    BinOp::Ge => (format!("({}{}{})", l, Self::eqop(lt, rt, ">=", ">=")?, r), Ty::Bool),
                    BinOp::And => {
                        Self::bools(lt, rt, "and")?;
                        (format!("({}&&{})", l, r), Ty::Bool)
                    }
                    BinOp::Or => {
                        Self::bools(lt, rt, "or")?;
                        (format!("({}||{})", l, r), Ty::Bool)
                    }
                };
                Ok((frag, ty))
            }
            ExprKind::Unary { op, expr } => {
                let (v, t) = self.expr(expr)?;
                match op {
                    UnOp::Not => {
                        if t != Ty::Bool {
                            return Err("`not` on non-bool: beyond core subset; refusing".into());
                        }
                        Ok((format!("(!{})", v), Ty::Bool))
                    }
                    UnOp::Neg => {
                        if t != Ty::Int {
                            return Err("unary `-` on non-int: beyond core subset; refusing".into());
                        }
                        Ok((format!("(-{})", v), Ty::Int))
                    }
                }
            }
        }
    }

    fn ints(l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if l == Ty::Int && r == Ty::Int {
            Ok(())
        } else {
            Err(format!("`{}` on non-int: beyond core subset; refusing", op))
        }
    }

    fn bools(l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if l == Ty::Bool && r == Ty::Bool {
            Ok(())
        } else {
            Err(format!("`{}` on non-bool: beyond core subset; refusing", op))
        }
    }

    /// String operands use Tcl's string operators; mixed str/non-str refused.
    fn eqop(l: Ty, r: Ty, num: &'static str, str_: &'static str) -> Result<&'static str, String> {
        if l == Ty::Str && r == Ty::Str {
            Ok(str_)
        } else if l != Ty::Str && r != Ty::Str {
            Ok(num)
        } else {
            Err("comparison across str/non-str: beyond core subset; refusing".into())
        }
    }

    /// Wrap int/bool fragments for value positions (`set`, `return`, `say`).
    fn value(&self, frag: String, ty: Ty) -> String {
        match ty {
            Ty::Str => frag,
            _ => format!("[expr {}]", frag),
        }
    }

    fn ty_tag(ty: Ty) -> &'static str {
        match ty {
            Ty::Int => "int",
            Ty::Str => "str",
            Ty::Bool => "bool",
        }
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                let is_let = matches!(&s.kind, StmtKind::Let { .. });
                let (v, vt) = self.expr(value)?;
                if let Some(t) = ty {
                    let at = ty_of(t)?;
                    if at != vt {
                        return Err(format!(
                            "{} {}: annotated type disagrees with value; refusing",
                            if is_let { "let" } else { "mut" },
                            name
                        ));
                    }
                }
                self.scope.insert(name.clone(), vt);
                self.line(&format!("set {} {};", name, self.value(v, vt)));
                Ok(())
            }
            StmtKind::Assign { target, value } => {
                let name = match &target.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => return Err("complex assignment target: beyond core subset; refusing".into()),
                };
                if !self.scope.contains_key(&name) {
                    return Err(format!("assignment to unknown variable `{}`: refusing", name));
                }
                let (v, vt) = self.expr(value)?;
                self.scope.insert(name.clone(), vt);
                self.line(&format!("set {} {};", name, self.value(v, vt)));
                Ok(())
            }
            StmtKind::Ret(e) => {
                if !self.in_def {
                    return Err("ret outside def: beyond core subset; refusing".into());
                }
                match e {
                    Some(x) => {
                        let (v, vt) = self.expr(x)?;
                        self.line(&format!("return {};", self.value(v, vt)));
                    }
                    None => self.line("return;"),
                }
                Ok(())
            }
            StmtKind::Fail(_) => Err("fail: beyond core subset; refusing".into()),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let (c, ct) = self.expr(cond)?;
                if ct != Ty::Bool {
                    return Err("if condition must be bool; refusing".into());
                }
                // No braces around the condition: fragments may carry `[...]`
                // substitutions, which braces would suppress.
                self.line(&format!("if [expr {}] {{", c));
                self.level += 1;
                for s in then_body {
                    self.stmt(s)?;
                }
                self.level -= 1;
                if let Some(eb) = else_body {
                    self.line("} else {");
                    self.level += 1;
                    for s in eb {
                        self.stmt(s)?;
                    }
                    self.level -= 1;
                }
                self.line("}");
                Ok(())
            }
            StmtKind::For { .. } => Err("for: beyond core subset; refusing".into()),
            StmtKind::Whl { cond, body } => {
                let (c, ct) = self.expr(cond)?;
                if ct != Ty::Bool {
                    return Err("while condition must be bool; refusing".into());
                }
                // The condition must be re-evaluated every iteration, but
                // `[expr ...]` as the while condition would substitute
                // once. Loop on `1` and break when the condition fails;
                // the set/if use no braces so nested substitutions in the
                // condition still evaluate per iteration.
                self.line("while {1} {");
                self.level += 1;
                self.line(&format!("set __cuni_c [expr {}];", c));
                self.line("if {!$__cuni_c} break;");
                for s in body {
                    self.stmt(s)?;
                }
                self.level -= 1;
                self.line("}");
                Ok(())
            }
            StmtKind::ExprStmt(e) => match &e.kind {
                ExprKind::Call { callee, args } => {
                    if let ExprKind::Ident(n) = &callee.kind {
                        if n == "say" {
                            if args.len() != 1 {
                                return Err("say: exactly one argument in core subset; refusing".into());
                            }
                            if args[0].is_named() {
                                return Err("named arguments: beyond core subset; refusing".into());
                            }
                            let (v, vt) = self.expr(args[0].expr())?;
                            self.line(&format!("cuni_say {} {};", self.value(v, vt), Self::ty_tag(vt)));
                            return Ok(());
                        }
                    }
                    Err("bare call (non-say): beyond core subset; refusing".into())
                }
                _ => Err("bare expression: beyond core subset; refusing".into()),
            },
            StmtKind::Todo => Err("...: beyond core subset; refusing".into()),
        }
    }

    fn def(&mut self, f: &FnDecl) -> Result<(), String> {
        if f.fallible {
            return Err(format!("def {}: fallible functions beyond core subset; refusing", f.name));
        }
        if !f.generics.is_empty() {
            return Err(format!("def {}: generics beyond core subset; refusing", f.name));
        }
        if f.is_link {
            return Err(format!("link {}: beyond core subset; refusing", f.name));
        }
        let ret_ty = ty_of(&f.ret_type)?;
        self.fn_ret.insert(f.name.clone(), ret_ty);
        let outer = std::mem::take(&mut self.scope);
        for p in &f.params {
            self.scope.insert(p.name.clone(), ty_of(&p.ty)?);
        }
        let params: Vec<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
        self.line(&format!("proc {} {{{}}} {{", f.name, params.join(" ")));
        self.level += 1;
        let was = std::mem::replace(&mut self.in_def, true);
        for s in &f.body {
            self.stmt(s)?;
        }
        self.in_def = was;
        self.level -= 1;
        self.line("}");
        self.scope = outer;
        Ok(())
    }
}

pub fn generate(program: &Program) -> Result<String, String> {
    let mut g = Gen {
        out: String::new(),
        level: 0,
        fns: HashSet::new(),
        fn_ret: HashMap::new(),
        scope: HashMap::new(),
        in_def: false,
    };
    g.line("# Generated by the CuNi Tcl backend. Do not hand-edit.");
    g.out.push_str(HEADER);
    // First pass: collect defs, refuse non-core items; defs emit before
    // top-level statements so calls always resolve at runtime.
    let mut script: Vec<&Stmt> = Vec::new();
    for item in &program.items {
        match item {
            Item::Def(f) => {
                g.fns.insert(f.name.clone());
            }
            Item::Stmt(s) => script.push(s),
            Item::Use(u) => return Err(format!("use {}: beyond core subset; refusing", u.name)),
            Item::Enum(_) => return Err("enum: beyond core subset; refusing".into()),
            Item::Typ(_) => return Err("typ: beyond core subset; refusing".into()),
            Item::Iface(_) => return Err("iface: beyond core subset; refusing".into()),
            Item::Ext(_) => return Err("ext: beyond core subset; refusing".into()),
        }
    }
    g.out.push('\n');
    for item in &program.items {
        if let Item::Def(f) = item {
            g.def(f)?;
            g.out.push('\n');
        }
    }
    for s in script {
        g.stmt(s)?;
    }
    Ok(g.out)
}

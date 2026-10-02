//! Shared core-subset emitter for new native seats.
//!
//! Many catalog languages need a real, honest native seat but don't yet
//! warrant a full hand-written backend (cf. codegen_go.rs, codegen_java.rs).
//! This module implements the CuNi **core subset** once, driven by a small
//! per-language [`LangSpec`]. Each `codegen_<id>.rs` is a thin spec (~50
//! lines) over this core.
//!
//! ## Supported (core subset)
//! - `say(x)` for int/str/bool (single argument)
//! - `let`/`mut` bindings for int/str/bool
//S- `def`/`ret` — monomorphic, int/str/bool params and returns
//! - `if`/`else`, `while`
//! - int arithmetic `+ - *`; int `/` (truncate toward zero) and `%`
//!   (Python-floored) via per-language helpers where the native operators
//!   differ
//! - comparisons `== != < > <= >=`, `and`/`or`/`not`, unary `-`/`not`
//! - string literals (escaped) and `+` concatenation
//!
//! ## Refused (honest Err, never approximated)
//! `float`/`dec`/`time` literals and ops, `typ`, `enum`, `iface`, generics,
//! `opt`/`none`, fallible `?` / `fail` / `??`, `link`, `ext`, lists, maps,
//! indexing, field access, `for` loops, `todo`, interpolated strings, and
//! anything else not listed above. A seat that refuses is a real seat; a
//! seat that approximates is a lie.

use crate::ast::*;

/// Per-language customization for the core emitter. All `_fmt` strings use
/// `{name}`, `{expr}`, `{cond}`, `{params}`, `{args}` placeholders.
pub struct LangSpec {
    /// Comment prefix, e.g. `//`, `#`, `--`
    pub comment: &'static str,
    /// Header text: helper definitions (cuni_say, cuni_div/cuni_mod if
    /// needed). Must define `cuni_say(x, ty)` printing with trailing newline,
    /// where `ty` is `"int"`/`"str"`/`"bool"`; booleans render as
    /// `True`/`False`.
    pub header: &'static str,
    /// Wrapping for top-level statements, e.g. `("func main() {", "}")`.
    /// Use `("", "")` for languages that run top-level code directly.
    pub main_open: &'static str,
    pub main_close: &'static str,
    /// e.g. `"let {name} = {expr};"`, `"var {name} = {expr};"`,
    /// `"{name} = {expr}"` (Python-like), `"my ${name} = {expr};"` (PHP-ish
    /// via var_prefix instead — see below).
    pub let_fmt: &'static str,
    pub mut_fmt: &'static str,
    pub assign_fmt: &'static str,
    /// e.g. `"function {name}({params}) {"`, `"def {name}({params}):"`.
    pub fn_open_fmt: &'static str,
    pub fn_close: &'static str,
    /// e.g. `"return {expr};"`. `{expr}` may be empty for bare `ret`.
    pub ret_fmt: &'static str,
    pub if_open_fmt: &'static str,
    pub else_open: &'static str,
    pub block_close: &'static str,
    pub while_open_fmt: &'static str,
    /// e.g. `"cuni_say({expr}, '{ty}');"` where `{ty}` is `int`/`str`/`bool`.
    /// The type tag lets languages without a distinct bool type (Perl, awk)
    /// render `True`/`False` honestly.
    pub say_fmt: &'static str,
    /// e.g. `"{name}({args})"`
    pub call_fmt: &'static str,
    pub true_lit: &'static str,
    pub false_lit: &'static str,
    /// Quote + escape a string literal, e.g. `"a\"b"` or `'a\'b'`.
    pub str_quote: fn(&str) -> String,
    /// If `Some(helper)`, int `/` emits `helper(lhs, rhs)`; if `None`, the
    /// language's native `/` already truncates toward zero (CuNi semantics).
    pub int_div_helper: Option<&'static str>,
    /// If `Some(helper)`, int `%` emits `helper(lhs, rhs)`; if `None`, the
    /// language's native `%` is already Python-floored.
    pub int_mod_helper: Option<&'static str>,
    /// String concatenation format with `{l}` and `{r}` placeholders,
    /// e.g. `"({l} + {r})"` (most), `"({l} . {r})"` (PHP/Perl),
    /// `"[string cat {l} {r}]"` (Tcl). Used for `+` when both operands are
    /// known `str`; otherwise `+` is numeric.
    pub concat_fmt: &'static str,
    /// String comparison operators (used when both operands are `str`).
    /// Most languages use the same as numeric; Perl uses `eq`/`ne`/`lt`/`gt`/`le`/`ge`.
    pub str_eq_op: &'static str,
    pub str_ne_op: &'static str,
    pub str_lt_op: &'static str,
    pub str_gt_op: &'static str,
    pub str_le_op: &'static str,
    pub str_ge_op: &'static str,
    /// Prefix for variable references, e.g. `"$"` (PHP/Perl), `""` otherwise.
    /// Also applied to declarations and parameters via the fmt strings.
    pub var_prefix: &'static str,
    pub indent: &'static str,
}

fn sub(fmt: &str, pairs: &[(&str, &str)]) -> String {
    let mut s = fmt.to_string();
    for (k, v) in pairs {
        s = s.replace(&format!("{{{}}}", k), v);
    }
    s
}

pub struct Emitter<'a> {
    spec: &'a LangSpec,
    out: String,
    level: usize,
    /// Function names defined in this program (for call validation).
    fns: std::collections::HashSet<String>,
    /// Function name -> return kind (for `+` routing).
    fn_ret: std::collections::HashMap<String, Ty>,
    /// Variable name -> kind (for `+` routing).
    scope: std::collections::HashMap<String, Ty>,
}

/// Minimal type for `+` routing (numeric vs string concat).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    Int,
    Str,
    Bool,
}

fn ty_of(ty: &Type) -> Result<Ty, String> {
    match ty {
        Type::Named(n) if n == "int" => Ok(Ty::Int),
        Type::Named(n) if n == "str" => Ok(Ty::Str),
        Type::Named(n) if n == "bool" => Ok(Ty::Bool),
        _ => Err("type beyond core subset; refusing".into()),
    }
}

pub fn generate(program: &Program, spec: &LangSpec) -> Result<String, String> {
    let mut e = Emitter {
        spec,
        out: String::new(),
        level: 0,
        fns: std::collections::HashSet::new(),
        fn_ret: std::collections::HashMap::new(),
        scope: std::collections::HashMap::new(),
    };
    e.gen_program(program)?;
    Ok(e.out)
}

impl<'a> Emitter<'a> {
    fn line(&mut self, text: &str) {
        if !text.is_empty() {
            self.out.push_str(&self.spec.indent.repeat(self.level));
            self.out.push_str(text);
        }
        self.out.push('\n');
    }

    fn var(&self, name: &str) -> String {
        format!("{}{}", self.spec.var_prefix, name)
    }

    fn gen_program(&mut self, program: &Program) -> Result<(), String> {
        self.line(&format!(
            "{} Generated by the CuNi core-subset backend. Do not hand-edit.",
            self.spec.comment
        ));
        self.out.push_str(self.spec.header);
        if !self.spec.header.ends_with('\n') {
            self.out.push('\n');
        }
        // First pass: collect def names, refuse non-core items.
        let mut script_stmts: Vec<&Stmt> = Vec::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    self.check_def(f)?;
                    self.fns.insert(f.name.clone());
                }
                Item::Stmt(s) => script_stmts.push(s),
                Item::Use(u) => return Err(format!("use {}: beyond core subset; refusing", u.name)),
                Item::Enum(_) => return Err("enum: beyond core subset; refusing".into()),
                Item::Typ(_) => return Err("typ: beyond core subset; refusing".into()),
                Item::Iface(_) => return Err("iface: beyond core subset; refusing".into()),
                Item::Ext(_) => return Err("ext: beyond core subset; refusing".into()),
            }
        }
        self.out.push('\n');
        for item in &program.items {
            if let Item::Def(f) = item {
                self.gen_def(f)?;
                self.out.push('\n');
            }
        }
        if !self.spec.main_open.is_empty() {
            self.line(self.spec.main_open);
            self.level += 1;
        }
        for s in script_stmts {
            self.gen_stmt(s)?;
        }
        if !self.spec.main_open.is_empty() {
            self.level -= 1;
            self.line(self.spec.main_close);
        }
        Ok(())
    }

    /// Only int/str/bool types allowed; no generics, no opt, no fallible.
    fn check_ty(&self, ty: &Type) -> Result<Ty, String> {
        ty_of(ty)
    }

    fn check_def(&self, f: &FnDecl) -> Result<(), String> {
        if f.fallible {
            return Err(format!("def {}: fallible functions beyond core subset; refusing", f.name));
        }
        if !f.generics.is_empty() {
            return Err(format!("def {}: generics beyond core subset; refusing", f.name));
        }
        if f.is_link {
            return Err(format!("link {}: beyond core subset; refusing", f.name));
        }
        self.check_ty(&f.ret_type)?;
        for p in &f.params {
            self.check_ty(&p.ty)?;
        }
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let ret_ty = ty_of(&f.ret_type)?;
        self.fn_ret.insert(f.name.clone(), ret_ty);
        // Fresh scope for the function body: params first.
        let outer = std::mem::take(&mut self.scope);
        for p in &f.params {
            self.scope.insert(p.name.clone(), ty_of(&p.ty)?);
        }
        let params: Vec<String> = f.params.iter().map(|p| self.var(&p.name)).collect();
        self.line(&sub(
            self.spec.fn_open_fmt,
            &[("name", &f.name), ("params", &params.join(", "))],
        ));
        self.level += 1;
        for s in &f.body {
            self.gen_stmt(s)?;
        }
        self.level -= 1;
        self.line(self.spec.fn_close);
        self.scope = outer;
        Ok(())
    }

    fn gen_stmt(&mut self, s: &Stmt) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                let (v, vt) = self.gen_expr(value)?;
                if let Some(t) = ty {
                    // Annotated type must agree with the inferred one.
                    let at = self.check_ty(t)?;
                    if at != vt {
                        return Err(format!(
                            "let {}: annotated {:?} disagrees with value; refusing",
                            name,
                            t
                        ));
                    }
                }
                self.scope.insert(name.clone(), vt);
                self.line(&sub(
                    self.spec.let_fmt,
                    &[("name", &self.var(name)), ("expr", &v)],
                ));
                Ok(())
            }
            StmtKind::Mut { name, ty, value } => {
                let (v, vt) = self.gen_expr(value)?;
                if let Some(t) = ty {
                    let at = self.check_ty(t)?;
                    if at != vt {
                        return Err(format!(
                            "mut {}: annotated {:?} disagrees with value; refusing",
                            name,
                            t
                        ));
                    }
                }
                self.scope.insert(name.clone(), vt);
                self.line(&sub(
                    self.spec.mut_fmt,
                    &[("name", &self.var(name)), ("expr", &v)],
                ));
                Ok(())
            }
            StmtKind::Assign { target, value } => {
                let (t, _) = self.gen_expr(target)?;
                let (v, _) = self.gen_expr(value)?;
                // Only plain variable assignment in the core subset.
                if !matches!(target.kind, ExprKind::Ident(_)) {
                    return Err("complex assignment target: beyond core subset; refusing".into());
                }
                self.line(&sub(self.spec.assign_fmt, &[("name", &t), ("expr", &v)]));
                Ok(())
            }
            StmtKind::Ret(e) => {
                let v = match e {
                    Some(x) => self.gen_expr(x)?.0,
                    None => String::new(),
                };
                self.line(&sub(self.spec.ret_fmt, &[("expr", &v)]));
                Ok(())
            }
            StmtKind::Fail(_) => Err("fail: beyond core subset; refusing".into()),
            StmtKind::If { cond, then_body, else_body } => {
                let (c, ct) = self.gen_expr(cond)?;
                if ct != Ty::Bool {
                    return Err("if condition must be bool; refusing".into());
                }
                self.line(&sub(self.spec.if_open_fmt, &[("cond", &c)]));
                self.level += 1;
                for s in then_body {
                    self.gen_stmt(s)?;
                }
                self.level -= 1;
                if let Some(eb) = else_body {
                    self.line(self.spec.else_open);
                    self.level += 1;
                    for s in eb {
                        self.gen_stmt(s)?;
                    }
                    self.level -= 1;
                }
                self.line(self.spec.block_close);
                Ok(())
            }
            StmtKind::For { .. } => Err("for: beyond core subset; refusing".into()),
            StmtKind::Whl { cond, body } => {
                let (c, ct) = self.gen_expr(cond)?;
                if ct != Ty::Bool {
                    return Err("while condition must be bool; refusing".into());
                }
                self.line(&sub(self.spec.while_open_fmt, &[("cond", &c)]));
                self.level += 1;
                for s in body {
                    self.gen_stmt(s)?;
                }
                self.level -= 1;
                self.line(self.spec.block_close);
                Ok(())
            }
            StmtKind::ExprStmt(e) => {
                // Only `say(...)` calls allowed as bare expression statements.
                match &e.kind {
                    ExprKind::Call { callee, args } => {
                        if let ExprKind::Ident(n) = &callee.kind {
                            if n == "say" {
                                if args.len() != 1 {
                                    return Err("say: exactly one argument in core subset; refusing".into());
                                }
                                let (v, vt) = self.gen_expr(args[0].expr())?;
                                let tys = match vt {
                                    Ty::Int => "int",
                                    Ty::Str => "str",
                                    Ty::Bool => "bool",
                                };
                                self.line(&sub(self.spec.say_fmt, &[("expr", &v), ("ty", tys)]));
                                return Ok(());
                            }
                        }
                        Err("bare call (non-say): beyond core subset; refusing".into())
                    }
                    _ => Err("bare expression: beyond core subset; refusing".into()),
                }
            }
            StmtKind::Todo => Err("...: beyond core subset; refusing".into()),
        }
    }

    fn gen_expr(&mut self, e: &Expr) -> Result<(String, Ty), String> {
        match &e.kind {
            ExprKind::Int(n) => Ok((n.to_string(), Ty::Int)),
            ExprKind::Bool(b) => Ok((
                if *b { self.spec.true_lit.into() } else { self.spec.false_lit.into() },
                Ty::Bool,
            )),
            ExprKind::Str(s) => Ok(((self.spec.str_quote)(s), Ty::Str)),
            ExprKind::Float(_) => Err("float: beyond core subset; refusing".into()),
            ExprKind::Dec(_) => Err("dec: beyond core subset; refusing".into()),
            ExprKind::Time(_) => Err("time: beyond core subset; refusing".into()),
            ExprKind::NoneLit => Err("none: beyond core subset; refusing".into()),
            ExprKind::Ident(n) => {
                let t = self.scope.get(n).copied().ok_or_else(|| {
                    format!("unknown variable `{}`: refusing", n)
                })?;
                Ok((self.var(n), t))
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
                let mut as_: Vec<String> = Vec::new();
                for a in args {
                    if a.is_named() {
                        return Err("named arguments: beyond core subset; refusing".into());
                    }
                    as_.push(self.gen_expr(a.expr())?.0);
                }
                Ok((
                    sub(self.spec.call_fmt, &[("name", &name), ("args", &as_.join(", "))]),
                    ret,
                ))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (l, lt) = self.gen_expr(lhs)?;
                let (r, rt) = self.gen_expr(rhs)?;
                let o: String = match op {
                    BinOp::Add => {
                        // String concat vs numeric add, routed by operand types.
                        if lt == Ty::Str && rt == Ty::Str {
                            let l2 = l.clone();
                            let r2 = r.clone();
                            return Ok((
                                sub(self.spec.concat_fmt, &[("l", &l2), ("r", &r2)]),
                                Ty::Str,
                            ));
                        } else if lt == Ty::Int && rt == Ty::Int {
                            "+".to_string()
                        } else {
                            return Err(
                                "mixed-type `+`: beyond core subset; refusing".into()
                            );
                        }
                    }
                    BinOp::Sub => {
                        self.require_ints(lt, rt, "-")?;
                        "-".to_string()
                    }
                    BinOp::Mul => {
                        self.require_ints(lt, rt, "*")?;
                        "*".to_string()
                    }
                    BinOp::Div => {
                        self.require_ints(lt, rt, "/")?;
                        // Integer division truncates toward zero (CuNi).
                        if let Some(h) = self.spec.int_div_helper {
                            return Ok((format!("{}( {}, {} )", h, l, r).replace("( ", "(").replace(" )", ")"), Ty::Int));
                        }
                        "/".to_string()
                    }
                    BinOp::Mod => {
                        self.require_ints(lt, rt, "%")?;
                        // Python-floored modulo (CuNi).
                        if let Some(h) = self.spec.int_mod_helper {
                            return Ok((format!("{}( {}, {} )", h, l, r).replace("( ", "(").replace(" )", ")"), Ty::Int));
                        }
                        "%".to_string()
                    }
                    BinOp::Eq => {
                        if lt == Ty::Str && rt == Ty::Str {
                            self.spec.str_eq_op.to_string()
                        } else {
                            "==".to_string()
                        }
                    }
                    BinOp::Ne => {
                        if lt == Ty::Str && rt == Ty::Str {
                            self.spec.str_ne_op.to_string()
                        } else {
                            "!=".to_string()
                        }
                    }
                    BinOp::Lt => {
                        if lt == Ty::Str && rt == Ty::Str {
                            self.spec.str_lt_op.to_string()
                        } else {
                            "<".to_string()
                        }
                    }
                    BinOp::Gt => {
                        if lt == Ty::Str && rt == Ty::Str {
                            self.spec.str_gt_op.to_string()
                        } else {
                            ">".to_string()
                        }
                    }
                    BinOp::Le => {
                        if lt == Ty::Str && rt == Ty::Str {
                            self.spec.str_le_op.to_string()
                        } else {
                            "<=".to_string()
                        }
                    }
                    BinOp::Ge => {
                        if lt == Ty::Str && rt == Ty::Str {
                            self.spec.str_ge_op.to_string()
                        } else {
                            ">=".to_string()
                        }
                    }
                    BinOp::And => {
                        self.require_bools(lt, rt, "and")?;
                        "&&".to_string()
                    }
                    BinOp::Or => {
                        self.require_bools(lt, rt, "or")?;
                        "||".to_string()
                    }
                };
                let result_ty = match op {
                    BinOp::Add if lt == Ty::Str => Ty::Str,
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => Ty::Int,
                    _ => Ty::Bool,
                };
                Ok((format!("({} {} {})", l, o, r), result_ty))
            }
            ExprKind::Unary { op, expr } => {
                let (v, t) = self.gen_expr(expr)?;
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

    fn require_ints(&self, l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if l == Ty::Int && r == Ty::Int {
            Ok(())
        } else {
            Err(format!("`{}` on non-int: beyond core subset; refusing", op))
        }
    }

    fn require_bools(&self, l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if l == Ty::Bool && r == Ty::Bool {
            Ok(())
        } else {
            Err(format!("`{}` on non-bool: beyond core subset; refusing", op))
        }
    }
}

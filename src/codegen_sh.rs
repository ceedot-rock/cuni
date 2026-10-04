//! Bash backend — custom core-subset native seat (seat id `sh`).
//!
//! Bash has no user functions inside `$(( ))`, so integer arithmetic is
//! emitted as `$(( ))`-ready text at statement level: `say`/`let`/`mut`
//! wrap int expressions in `$(( ))`, conditions use `(( ))`. Division
//! truncates toward zero natively (CuNi semantics); `%` goes through the
//! `cuni_mod` helper (floored). `def` becomes a shell function that echoes
//! its return value; call sites capture with `$( )`. Strings are single
//! shell words (literals single-quoted with `'\''` escaping, variables
//! `"$name"`), concatenated by adjacency. Booleans are 0/1 arithmetic
//! values; string comparisons (which cannot live inside `$(( ))`) are
//! evaluated with `[[ ]]` inside a command substitution. `say` prints via
//! `printf '%s\n'` and routes bools to `True`/`False`.
//!
//! Refuses everything beyond the core subset with the same honest `Err`s as
//! the core.
//!
//! Wiring note: `emit.rs`/`main.rs` dispatch is left to the parent — this file
//! only provides `generate`.

use crate::ast::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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

/// Bash single-quoted word: `'` becomes `'\''`.
fn sh_quote(s: &str) -> String {
    let mut r = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            r.push_str("'\\''");
        } else {
            r.push(c);
        }
    }
    r.push('\'');
    r
}

struct Unit {
    body: String,
    level: usize,
    /// Scope stack: source name -> (type, emitted name).
    frames: Vec<HashMap<String, (Ty, String)>>,
    in_func: bool,
}

impl Unit {
    fn new() -> Self {
        Unit {
            body: String::new(),
            level: 0,
            frames: Vec::new(),
            in_func: false,
        }
    }

    fn bl(&mut self, text: &str) {
        if !text.is_empty() {
            self.body.push_str(&"    ".repeat(self.level));
            self.body.push_str(text);
        }
        self.body.push('\n');
    }
}

struct Emitter {
    out: String,
    /// Function name -> (param types, return type). Fully populated before
    /// any body is generated, so forward references are fine.
    sigs: HashMap<String, (Vec<(String, Ty)>, Ty)>,
    order: Vec<String>,
    counter: u32,
    unit: Unit,
}

pub fn generate(program: &Program) -> Result<String, String> {
    let mut e = Emitter {
        out: String::new(),
        sigs: HashMap::new(),
        order: Vec::new(),
        counter: 0,
        unit: Unit::new(),
    };
    e.gen_program(program)?;
    Ok(e.out)
}

impl Emitter {
    fn lookup(&self, name: &str) -> Option<(Ty, String)> {
        for f in self.unit.frames.iter().rev() {
            if let Some(v) = f.get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    /// Bind `name`; rename (`x__1`, …) when shadowing so a rebinding never
    /// collides with an earlier live binding.
    fn bind(&mut self, name: &str, ty: Ty) -> Result<String, String> {
        let emitted = match self.lookup(name) {
            Some(_) => {
                self.counter += 1;
                format!("{}__{}", name, self.counter)
            }
            None => name.to_string(),
        };
        self.unit
            .frames
            .last_mut()
            .expect("scope frame")
            .insert(name.to_string(), (ty, emitted.clone()));
        Ok(emitted)
    }

    fn bind_param(&mut self, name: &str, ty: Ty) -> Result<String, String> {
        // Same as bind: params live in the function's scope frame.
        self.bind(name, ty)
    }

    fn check_ann(&self, name: &str, ty: &Option<Type>, vt: Ty, kw: &str) -> Result<(), String> {
        if let Some(t) = ty {
            let at = ty_of(t)?;
            if at != vt {
                return Err(format!(
                    "{} {}: annotated type disagrees with value type; refusing",
                    kw, name
                ));
            }
        }
        Ok(())
    }

    fn check_def(&self, f: &FnDecl) -> Result<(), String> {
        if f.fallible {
            return Err(format!(
                "def {}: fallible functions beyond core subset; refusing",
                f.name
            ));
        }
        if !f.generics.is_empty() {
            return Err(format!(
                "def {}: generics beyond core subset; refusing",
                f.name
            ));
        }
        if f.is_link {
            return Err(format!("link {}: beyond core subset; refusing", f.name));
        }
        if f.name.starts_with("cuni_") {
            return Err(format!(
                "def {}: reserved cuni_ prefix (collides with helpers); refusing",
                f.name
            ));
        }
        ty_of(&f.ret_type)?;
        for p in &f.params {
            ty_of(&p.ty)?;
        }
        Ok(())
    }

    fn gen_program(&mut self, program: &Program) -> Result<(), String> {
        // First pass: collect def signatures, refuse non-core items.
        let mut seen: HashSet<String> = HashSet::new();
        let mut script: Vec<&Stmt> = Vec::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    self.check_def(f)?;
                    if !seen.insert(f.name.clone()) {
                        return Err(format!("def {}: duplicate definition; refusing", f.name));
                    }
                    let mut params = Vec::new();
                    for p in &f.params {
                        params.push((p.name.clone(), ty_of(&p.ty)?));
                    }
                    let ret = ty_of(&f.ret_type)?;
                    self.sigs.insert(f.name.clone(), (params, ret));
                    self.order.push(f.name.clone());
                }
                Item::Stmt(s) => script.push(s),
                Item::Use(u) => {
                    return Err(format!("use {}: beyond core subset; refusing", u.name))
                }
                Item::Enum(_) => return Err("enum: beyond core subset; refusing".into()),
                Item::Typ(_) => return Err("typ: beyond core subset; refusing".into()),
                Item::Iface(_) => return Err("iface: beyond core subset; refusing".into()),
                Item::Ext(_) => return Err("ext: beyond core subset; refusing".into()),
            }
        }
        // Second pass: function bodies (any order — signatures are complete).
        let mut fns: Vec<String> = Vec::new();
        for name in self.order.clone() {
            let f = program
                .items
                .iter()
                .find_map(|it| match it {
                    Item::Def(f) if f.name == name => Some(f),
                    _ => None,
                })
                .expect("def present");
            fns.push(self.gen_def(f)?);
        }
        // Top-level statements.
        self.unit = Unit::new();
        self.unit.frames.push(HashMap::new());
        for s in script {
            self.gen_stmt(s)?;
        }
        let top = std::mem::replace(&mut self.unit, Unit::new());

        self.out.push_str("#!/usr/bin/env bash\n");
        self.out.push_str(
            "# Generated by the CuNi core-subset backend. Do not hand-edit.\n",
        );
        self.out
            .push_str("cuni_mod(){ echo $(( (($1 % $2) + $2) % $2 )); }\n");
        self.out.push('\n');
        for code in &fns {
            self.out.push_str(code);
            self.out.push('\n');
        }
        self.out.push_str(&top.body);
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<String, String> {
        let (params, ret) = self
            .sigs
            .get(&f.name)
            .cloned()
            .ok_or_else(|| format!("internal: missing signature for `{}`", f.name))?;
        let saved = std::mem::replace(&mut self.unit, Unit::new());
        self.unit.in_func = true;
        self.unit.frames.push(HashMap::new());
        let mut s = String::new();
        s.push_str(&format!("{}() {{\n", f.name));
        for (i, (n, t)) in params.iter().enumerate() {
            let emitted = self.bind_param(n, *t)?;
            // Keep a marker so the kind is known; the value is positional.
            let _ = t;
            s.push_str(&format!("    local {}=\"${}\"\n", emitted, i + 1));
        }
        for st in &f.body {
            self.gen_stmt(st)?;
        }
        let finished = std::mem::replace(&mut self.unit, saved);
        for line in finished.body.lines() {
            if line.trim().is_empty() {
                s.push('\n');
            } else {
                s.push_str("    ");
                s.push_str(line);
                s.push('\n');
            }
        }
        // A def with no ret would silently echo nothing; the core refuses
        // bare ret, and a missing ret is a loud shell error only if the
        // caller misuses it — keep the body as written.
        let _ = ret;
        s.push_str("}\n");
        Ok(s)
    }

    /// Type of an expression without emitting (for `+` routing).
    fn expr_ty(&self, e: &Expr) -> Result<Ty, String> {
        match &e.kind {
            ExprKind::Int(_) => Ok(Ty::Int),
            ExprKind::Bool(_) => Ok(Ty::Bool),
            ExprKind::Str(_) => Ok(Ty::Str),
            ExprKind::Ident(n) => self
                .lookup(n)
                .map(|(t, _)| t)
                .ok_or_else(|| format!("unknown variable `{n}`: refusing")),
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Add => {
                    let l = self.expr_ty(lhs)?;
                    let r = self.expr_ty(rhs)?;
                    if l == Ty::Str && r == Ty::Str {
                        Ok(Ty::Str)
                    } else if l == Ty::Int && r == Ty::Int {
                        Ok(Ty::Int)
                    } else {
                        Err("mixed-type `+`: beyond core subset; refusing".into())
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => Ok(Ty::Int),
                _ => Ok(Ty::Bool),
            },
            ExprKind::Unary { op: UnOp::Neg, .. } => Ok(Ty::Int),
            ExprKind::Unary { op: UnOp::Not, .. } => Ok(Ty::Bool),
            ExprKind::Call { callee, .. } => {
                let name = match &callee.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => return Err("indirect call: beyond core subset; refusing".into()),
                };
                if name == "say" {
                    return Err("say: use as a statement, not an expression; refusing".into());
                }
                self.sigs
                    .get(&name)
                    .map(|(_, r)| *r)
                    .ok_or_else(|| format!("call to unknown function `{name}`: refusing"))
            }
            _ => Err("unsupported expression: beyond core subset; refusing".into()),
        }
    }

    /// Expression typed as Int/Str/Bool: returns (rendering, type) where the
    /// rendering is an arithmetic text (Int/Bool) or a shell word (Str).
    fn typed(&mut self, e: &Expr) -> Result<(String, Ty), String> {
        match &e.kind {
            ExprKind::Int(_) | ExprKind::Unary { op: UnOp::Neg, .. } => {
                Ok((self.int_expr(e)?, Ty::Int))
            }
            ExprKind::Bool(_) => Ok((self.bool_val(e)?, Ty::Bool)),
            ExprKind::Str(_) => Ok((self.str_word(e)?, Ty::Str)),
            ExprKind::Ident(n) => {
                let (t, emitted) = self
                    .lookup(n)
                    .ok_or_else(|| format!("unknown variable `{}`: refusing", n))?;
                let v = match t {
                    Ty::Int => emitted,
                    Ty::Bool => emitted,
                    Ty::Str => format!("\"${{{}}}\"", emitted),
                };
                Ok((v, t))
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Add => {
                    let lt = self.expr_ty(lhs)?;
                    let rt = self.expr_ty(rhs)?;
                    if lt == Ty::Str && rt == Ty::Str {
                        Ok((self.str_word(e)?, Ty::Str))
                    } else {
                        Ok((self.int_expr(e)?, Ty::Int))
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                    Ok((self.int_expr(e)?, Ty::Int))
                }
                _ => Ok((self.bool_val(e)?, Ty::Bool)),
            },
            ExprKind::Unary { op: UnOp::Not, .. } => Ok((self.bool_val(e)?, Ty::Bool)),
            ExprKind::Call { callee, .. } => {
                let name = match &callee.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => return Err("indirect call: beyond core subset; refusing".into()),
                };
                if name == "say" {
                    return Err("say: use as a statement, not an expression; refusing".into());
                }
                let ret = self
                    .sigs
                    .get(&name)
                    .map(|(_, r)| *r)
                    .ok_or_else(|| format!("call to unknown function `{}`: refusing", name))?;
                let v = match ret {
                    Ty::Int => self.int_expr(e)?,
                    Ty::Bool => self.bool_val(e)?,
                    Ty::Str => self.str_word(e)?,
                };
                Ok((v, ret))
            }
            ExprKind::Float(_) => Err("float: beyond core subset; refusing".into()),
            ExprKind::Dec(_) => Err("dec: beyond core subset; refusing".into()),
            ExprKind::Time(_) => Err("time: beyond core subset; refusing".into()),
            ExprKind::NoneLit => Err("none: beyond core subset; refusing".into()),
            ExprKind::InterpStr(_) => {
                Err("interpolated string: beyond core subset; refusing".into())
            }
            ExprKind::List(_) => Err("list: beyond core subset; refusing".into()),
            ExprKind::Map(_) => Err("map: beyond core subset; refusing".into()),
            ExprKind::Index { .. } => Err("indexing: beyond core subset; refusing".into()),
            ExprKind::Field { .. } => Err("field access: beyond core subset; refusing".into()),
            ExprKind::Unwrap { .. } => Err("??: beyond core subset; refusing".into()),
        }
    }

    /// Bash arithmetic text (valid inside `$(( ))`).
    fn int_expr(&mut self, e: &Expr) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(n.to_string()),
            ExprKind::Ident(n) => {
                let (t, emitted) = self
                    .lookup(n)
                    .ok_or_else(|| format!("unknown variable `{}`: refusing", n))?;
                if t != Ty::Int {
                    return Err(format!(
                        "variable `{}` is not an int: beyond core subset; refusing",
                        n
                    ));
                }
                Ok(emitted)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.int_operand(lhs)?;
                let r = self.int_operand(rhs)?;
                match op {
                    BinOp::Add => Ok(format!("({l} + {r})")),
                    BinOp::Sub => Ok(format!("({l} - {r})")),
                    BinOp::Mul => Ok(format!("({l} * {r})")),
                    // Bash `/` truncates toward zero (CuNi).
                    BinOp::Div => Ok(format!("({l} / {r})")),
                    // Floored modulo via the helper (evaluates operands first
                    // so each arrives as a single word).
                    BinOp::Mod => Ok(format!("$(cuni_mod \"$(({l}))\" \"$(({r}))\")")),
                    _ => Err(format!(
                        "`{}` is not an int operator: beyond core subset; refusing",
                        op_name(op)
                    )),
                }
            }
            ExprKind::Unary { op, expr } => match op {
                UnOp::Neg => Ok(format!("(-({}))", self.int_operand(expr)?)),
                UnOp::Not => Err("`not` on non-bool: beyond core subset; refusing".into()),
            },
            ExprKind::Call { callee, args } => {
                let (name, _, ret) = self.call_head(callee)?;
                if ret != Ty::Int {
                    return Err(format!(
                        "call to `{}` used as int but returns otherwise; refusing",
                        name
                    ));
                }
                let as_ = self.call_args(&name, args)?;
                Ok(format!("$({} {})", name, as_.join(" ")))
            }
            _ => Err("non-int expression in int position: beyond core subset; refusing".into()),
        }
    }

    /// Like int_expr but refuses non-int-typed subexpressions with the core's
    /// wording.
    fn int_operand(&mut self, e: &Expr) -> Result<String, String> {
        let (v, t) = self.typed(e)?;
        match t {
            Ty::Int => Ok(v),
            _ => Err("int operator on non-int: beyond core subset; refusing".into()),
        }
    }

    /// A single shell word evaluating to the string.
    fn str_word(&mut self, e: &Expr) -> Result<String, String> {
        match &e.kind {
            ExprKind::Str(s) => Ok(sh_quote(s)),
            ExprKind::Ident(n) => {
                let (t, emitted) = self
                    .lookup(n)
                    .ok_or_else(|| format!("unknown variable `{}`: refusing", n))?;
                if t != Ty::Str {
                    return Err(format!(
                        "variable `{}` is not a str: beyond core subset; refusing",
                        n
                    ));
                }
                Ok(format!("\"${{{}}}", emitted))
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Add => {
                    let (l, lt) = self.typed(lhs)?;
                    let (r, rt) = self.typed(rhs)?;
                    if lt == Ty::Str && rt == Ty::Str {
                        // Adjacent words concatenate.
                        Ok(format!("{l}{r}"))
                    } else {
                        Err("mixed-type `+`: beyond core subset; refusing".into())
                    }
                }
                _ => Err("non-str operator on strings: beyond core subset; refusing".into()),
            },
            ExprKind::Call { callee, args } => {
                let (name, _, ret) = self.call_head(callee)?;
                if ret != Ty::Str {
                    return Err(format!(
                        "call to `{}` used as str but returns otherwise; refusing",
                        name
                    ));
                }
                let as_ = self.call_args(&name, args)?;
                Ok(format!("\"$({} {})\"", name, as_.join(" ")))
            }
            _ => Err("non-str expression in str position: beyond core subset; refusing".into()),
        }
    }

    /// Bash arithmetic text evaluating to 0/1 (valid inside `$(( ))`).
    fn bool_val(&mut self, e: &Expr) -> Result<String, String> {
        match &e.kind {
            ExprKind::Bool(b) => Ok(if *b { "1".into() } else { "0".into() }),
            ExprKind::Ident(n) => {
                let (t, emitted) = self
                    .lookup(n)
                    .ok_or_else(|| format!("unknown variable `{}`: refusing", n))?;
                if t != Ty::Bool {
                    return Err(format!(
                        "variable `{}` is not a bool: beyond core subset; refusing",
                        n
                    ));
                }
                Ok(emitted)
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::And => Ok(format!("(({}) && ({}))", self.bool_val(lhs)?, self.bool_val(rhs)?)),
                BinOp::Or => Ok(format!("(({}) || ({}))", self.bool_val(lhs)?, self.bool_val(rhs)?)),
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    let (l, lt) = self.typed(lhs)?;
                    let (r, rt) = self.typed(rhs)?;
                    if lt == Ty::Int && rt == Ty::Int {
                        Ok(format!("(({l}) {} ({r}))", cmp_op(op)))
                    } else if lt == Ty::Str && rt == Ty::Str {
                        // String comparison cannot live inside $(( )); evaluate
                        // with [[ ]] in a command substitution yielding 0/1.
                        Ok(format!(
                            "$( [[ {l} {} {r} ]] && echo 1 || echo 0 )",
                            sh_cmp_op(op)
                        ))
                    } else if lt == Ty::Bool && rt == Ty::Bool {
                        Ok(format!("(({l}) {} ({r}))", cmp_op(op)))
                    } else {
                        Err("comparison on mismatched types: beyond core subset; refusing".into())
                    }
                }
                _ => Err("non-bool operator in bool position: beyond core subset; refusing".into()),
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => Ok(format!("(!({}))", self.bool_val(expr)?)),
                UnOp::Neg => Err("unary `-` on non-int: beyond core subset; refusing".into()),
            },
            ExprKind::Call { callee, args } => {
                let (name, _, ret) = self.call_head(callee)?;
                if ret != Ty::Bool {
                    return Err(format!(
                        "call to `{}` used as bool but returns otherwise; refusing",
                        name
                    ));
                }
                let as_ = self.call_args(&name, args)?;
                // Bool-returning functions echo 0/1.
                Ok(format!("$({} {})", name, as_.join(" ")))
            }
            _ => Err("non-bool expression in bool position: beyond core subset; refusing".into()),
        }
    }

    fn call_head(&self, callee: &Expr) -> Result<(String, Vec<(String, Ty)>, Ty), String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("indirect call: beyond core subset; refusing".into()),
        };
        if name == "say" {
            return Err("say: use as a statement, not an expression; refusing".into());
        }
        let (params, ret) = self
            .sigs
            .get(&name)
            .cloned()
            .ok_or_else(|| format!("call to unknown function `{}`: refusing", name))?;
        Ok((name, params, ret))
    }

    /// Call arguments as single shell words, checked against the signature.
    fn call_args(&mut self, name: &str, args: &[CallArg]) -> Result<Vec<String>, String> {
        let (params, _) = self
            .sigs
            .get(name)
            .cloned()
            .ok_or_else(|| format!("call to unknown function `{}`: refusing", name))?;
        if args.len() != params.len() {
            return Err(format!(
                "call to `{}`: arity mismatch ({} given, {} expected); refusing",
                name,
                args.len(),
                params.len()
            ));
        }
        let mut out = Vec::new();
        for (a, (_, pt)) in args.iter().zip(params.iter()) {
            if a.is_named() {
                return Err("named arguments: beyond core subset; refusing".into());
            }
            let (v, vt) = self.typed(a.expr())?;
            if *pt != vt {
                return Err(format!(
                    "call to `{}`: argument type mismatch; refusing",
                    name
                ));
            }
            // Int/Bool args arrive as single words via $(( )); str args are
            // already single words.
            out.push(match vt {
                Ty::Str => v,
                _ => format!("\"$(( {v} ))\""),
            });
        }
        Ok(out)
    }

    fn gen_stmt(&mut self, s: &Stmt) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                let (v, vt) = self.typed(value)?;
                self.check_ann(name, ty, vt, "let")?;
                let emitted = self.bind(name, vt)?;
                self.emit_binding(&emitted, &v, vt, true)?;
                Ok(())
            }
            StmtKind::Mut { name, ty, value } => {
                let (v, vt) = self.typed(value)?;
                self.check_ann(name, ty, vt, "mut")?;
                let emitted = self.bind(name, vt)?;
                self.emit_binding(&emitted, &v, vt, true)?;
                Ok(())
            }
            StmtKind::Assign { target, value } => {
                let tname = match &target.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => {
                        return Err(
                            "complex assignment target: beyond core subset; refusing".into()
                        )
                    }
                };
                let (tt, emitted) = self
                    .lookup(&tname)
                    .ok_or_else(|| format!("unknown variable `{}`: refusing", tname))?;
                let (v, vt) = self.typed(value)?;
                if tt != vt {
                    return Err(format!(
                        "assignment to `{}`: type mismatch; refusing",
                        tname
                    ));
                }
                self.emit_binding(&emitted, &v, vt, false)?;
                Ok(())
            }
            StmtKind::Ret(e) => {
                if !self.unit.in_func {
                    return Err("top-level ret: no return slot outside a def; refusing".into());
                }
                match e {
                    Some(x) => {
                        let (v, vt) = self.typed(x)?;
                        match vt {
                            Ty::Str => self.unit.bl(&format!("printf '%s\\n' {v}")),
                            _ => self.unit.bl(&format!("printf '%s\\n' \"$(( {v} ))\"")),
                        }
                        // The function's stdout is its return value; stop here
                        // so a `ret` inside a branch cannot fall through.
                        self.unit.bl("return 0");
                    }
                    None => return Err("bare ret: beyond core subset; refusing".into()),
                }
                Ok(())
            }
            StmtKind::Fail(_) => Err("fail: beyond core subset; refusing".into()),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let (c, ct) = self.typed(cond)?;
                if ct != Ty::Bool {
                    return Err("if condition must be bool; refusing".into());
                }
                self.unit.bl(&format!("if (( {c} )); then"));
                self.unit.level += 1;
                self.unit.frames.push(HashMap::new());
                for s in then_body {
                    self.gen_stmt(s)?;
                }
                self.unit.frames.pop();
                self.unit.level -= 1;
                if let Some(eb) = else_body {
                    self.unit.bl("else");
                    self.unit.level += 1;
                    self.unit.frames.push(HashMap::new());
                    for s in eb {
                        self.gen_stmt(s)?;
                    }
                    self.unit.frames.pop();
                    self.unit.level -= 1;
                }
                self.unit.bl("fi");
                Ok(())
            }
            StmtKind::For { .. } => Err("for: beyond core subset; refusing".into()),
            StmtKind::Whl { cond, body } => {
                let (c, ct) = self.typed(cond)?;
                if ct != Ty::Bool {
                    return Err("while condition must be bool; refusing".into());
                }
                self.unit.bl(&format!("while (( {c} )); do"));
                self.unit.level += 1;
                self.unit.frames.push(HashMap::new());
                for s in body {
                    self.gen_stmt(s)?;
                }
                self.unit.frames.pop();
                self.unit.level -= 1;
                self.unit.bl("done");
                Ok(())
            }
            StmtKind::ExprStmt(e) => {
                // Only `say(...)` calls allowed as bare expression statements.
                match &e.kind {
                    ExprKind::Call { callee, args } => {
                        if let ExprKind::Ident(n) = &callee.kind {
                            if n == "say" {
                                if args.len() != 1 {
                                    return Err(
                                        "say: exactly one argument in core subset; refusing".into()
                                    );
                                }
                                let (v, vt) = self.typed(args[0].expr())?;
                                match vt {
                                    Ty::Int => {
                                        self.unit.bl(&format!("printf '%s\\n' \"$(( {v} ))\""))
                                    }
                                    Ty::Str => self.unit.bl(&format!("printf '%s\\n' {v}")),
                                    Ty::Bool => {
                                        self.unit.bl(&format!("if (( {v} )); then"));
                                        self.unit.level += 1;
                                        self.unit.bl("printf '%s\\n' 'True'");
                                        self.unit.level -= 1;
                                        self.unit.bl("else");
                                        self.unit.level += 1;
                                        self.unit.bl("printf '%s\\n' 'False'");
                                        self.unit.level -= 1;
                                        self.unit.bl("fi");
                                    }
                                }
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

    fn emit_binding(
        &mut self,
        emitted: &str,
        v: &str,
        vt: Ty,
        is_let: bool,
    ) -> Result<(), String> {
        // `local` is function-scoped in bash, matching the core's flat
        // per-function scope; at top level it would be an error.
        let local = if is_let && self.unit.in_func { "local " } else { "" };
        match vt {
            Ty::Int | Ty::Bool => self.unit.bl(&format!("{local}{emitted}=$(( {v} ))")),
            Ty::Str => self.unit.bl(&format!("{local}{emitted}={v}")),
        }
        Ok(())
    }
}

fn op_name(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

fn cmp_op(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        _ => "==",
    }
}

/// `[[ ]]` string comparison operators.
fn sh_cmp_op(op: &BinOp) -> &'static str {
    match op {
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        _ => "==",
    }
}

//! Vyper contract backend — EVM Vyper writer.
//!
//! "Trust Provable, in all things."
//!
//! Emits a genuine Vyper contract shape from CuNi source: a pure logic core
//! (the part CuNi proves) plus the contract shell (`@external` functions)
//! that wraps it. The EVM-family sibling of the existing Solidity seat.
//!
//! Two artifacts, one law:
//! - `logic_core` — the pure logic in Vyper: `@internal @pure` functions
//!   (one per CuNi `def`, underscore-prefixed per the Vyper docs' own
//!   internal/external pattern). There is deliberately NO driver here: a
//!   pure contract has no stdout, so the CuNi driver (`say` lines) runs only
//!   in the Python reference.
//! - `generate_program` — the full contract: the logic core embedded
//!   verbatim (clearly delimited), plus the contract shell (also clearly
//!   delimited) — one `@external @pure` wrapper per CuNi `def` that returns
//!   the computed value.
//! - `generate_reference` — the standalone runnable Python reference of the
//!   pure logic core with a `main` driver printing the `say` outputs. This
//!   is what the gate proves byte-identical to CuNi gold.
//!
//! Exactness notes:
//! - CuNi `int` is int256 (Vyper has int256 natively). `/` is EVM SDIV
//!   (truncation toward zero) and `%` is SSMOD (sign of the dividend) —
//!   exactly CuNi's `int` semantics, no helpers needed.
//! - CuNi `dec` is a scaled int256 (scale 10^4), truncated toward zero,
//!   canonical rendering per docs/DECIMAL.md section 6. This is deliberately
//!   NOT Vyper's native `decimal` type (scale 10^10): CuNi proves the
//!   semantics it was given, and a different fixed-point scale would be a
//!   different proof. `*` lowers to `trunc(a*b/10000)`, `/` to
//!   `trunc(a*10000/b)`; `%` on `dec` is refused (docs/DECIMAL.md section 3).
//! - `dec` values cross the ABI boundary scaled by 10^4 (the external
//!   wrappers return scaled int256); human rendering happens in the Python
//!   reference via the canonical `_cuni_dec_str`.
//! - `bool` is native; `str` is `String[256]`. String `+` (concat) is
//!   refused in v1 — Vyper's `concat` returns a wider `String` than the
//!   operands, and the backend will not guess at the sizing plumbing.
//!   Interpolated (backtick) strings are driver-side and refused in the
//!   contract for the same reason.
//! - `say` inside a function body is refused in the contract artifact: a
//!   `@pure` function cannot print, so `say` lives only in the reference
//!   driver. Top-level `say` lines are what the reference runs.
//! - `float`, `list`, `map`, `opt`, `??`, structs, enums, `time`, `use`,
//!   `ext`, and `iface` are honestly refused: a Vyper function's verifiable
//!   core is integer/decimal math, and the backend will not guess at
//!   mappings it cannot prove.
//!
//! Honest boundaries: the logic core is gate-proven via `generate_reference`
//! (a Python rendering run with `python3` — no Vyper toolchain on the check
//! machine); the contract shell is NOT compiled here and nothing has
//! executed on-chain. See `docs/ONCHAIN.md` for the full verification matrix.

use crate::ast::{
    BinOp, CallArg, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr, Type, UnOp,
};
use std::collections::{HashMap, HashSet};

/// Vyper-logic kind of a CuNi value, for `say` routing and type inference.
#[derive(Clone, Copy, PartialEq, Eq)]
enum VKind {
    Int,
    Dec,
    Str,
    Bool,
    Other,
}

/// Delimiters marking the two regions of a `--emit-vyper` artifact.
pub const LOGIC_START: &str = "# CUNI-LOGIC-CORE-START";
pub const LOGIC_END: &str = "# CUNI-LOGIC-CORE-END";
pub const SHELL_START: &str = "# CUNI-VYPER-SHELL-START";
pub const SHELL_END: &str = "# CUNI-VYPER-SHELL-END";

pub struct Codegen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, VKind>,
    /// Set while generating the program shell (wrapper docs need it).
    out: String,
}

/// Does this expression contain an interpolated (backtick) string anywhere?
fn expr_has_interp(e: &Expr) -> bool {
    match &e.kind {
        ExprKind::InterpStr(_) => true,
        ExprKind::List(xs) => xs.iter().any(expr_has_interp),
        ExprKind::Map(kvs) => {
            kvs.iter()
                .any(|(k, v)| expr_has_interp(k) || expr_has_interp(v))
        }
        ExprKind::Call { callee, args } => {
            expr_has_interp(callee) || args.iter().any(|a| expr_has_interp(a.expr()))
        }
        ExprKind::Index { base, index } => expr_has_interp(base) || expr_has_interp(index),
        ExprKind::Field { base, .. } => expr_has_interp(base),
        ExprKind::Binary { lhs, rhs, .. } => expr_has_interp(lhs) || expr_has_interp(rhs),
        ExprKind::Unary { expr, .. } => expr_has_interp(expr),
        ExprKind::Unwrap { expr, handler } => {
            expr_has_interp(expr) || handler.iter().any(stmt_has_interp)
        }
        _ => false,
    }
}

/// Does this statement contain an interpolated (backtick) string anywhere?
fn stmt_has_interp(s: &Stmt) -> bool {
    match &s.kind {
        StmtKind::Let { value, .. } | StmtKind::Mut { value, .. } => expr_has_interp(value),
        StmtKind::Assign { target, value } => expr_has_interp(target) || expr_has_interp(value),
        StmtKind::Ret(e) => e.as_ref().is_some_and(expr_has_interp),
        StmtKind::Fail(e) => expr_has_interp(e),
        StmtKind::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_has_interp(cond)
                || then_body.iter().any(stmt_has_interp)
                || else_body
                    .as_ref()
                    .is_some_and(|b| b.iter().any(stmt_has_interp))
        }
        StmtKind::For { iter, body, .. } | StmtKind::Whl { cond: iter, body } => {
            expr_has_interp(iter) || body.iter().any(stmt_has_interp)
        }
        StmtKind::ExprStmt(e) => expr_has_interp(e),
        StmtKind::Todo => false,
    }
}

impl Codegen {
    fn new(program: &Program) -> Self {
        let mut fn_names = HashSet::new();
        let mut fn_ret = HashMap::new();
        for item in &program.items {
            if let Item::Def(f) = item {
                fn_names.insert(f.name.clone());
                fn_ret.insert(f.name.clone(), kind_of_type(&f.ret_type));
            }
        }
        Codegen {
            fn_names,
            fn_ret,
            out: String::new(),
        }
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn comment(&mut self, indent: usize, text: &str) {
        self.line(indent, &format!("# {text}"));
    }

    fn esc(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                '\r' => r.push_str("\\r"),
                c => r.push(c),
            }
        }
        r
    }
}

/// Map a CuNi type to its Vyper type.
///
/// `dec` is deliberately the scaled int256, NOT Vyper's native `decimal`
/// (scale 10^10) — CuNi proves the semantics it was given (docs/DECIMAL.md).
fn vyper_type(ty: &Type) -> Result<String, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok("int256".into()),
            "dec" => Ok("int256".into()),
            "bool" => Ok("bool".into()),
            "str" => Ok("String[256]".into()),
            "float" => Err("Vyper logic core has no float type; refusing float".into()),
            other => Err(format!("type `{other}` has no Vyper-logic mapping; refusing")),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{name}` has no Vyper-logic mapping; refusing"
        )),
    }
}

fn kind_of_type(ty: &Type) -> VKind {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => VKind::Int,
            "dec" => VKind::Dec,
            "str" => VKind::Str,
            "bool" => VKind::Bool,
            _ => VKind::Other,
        },
        _ => VKind::Other,
    }
}

/// The pure logic core in Vyper: `@internal @pure` functions, no driver.
///
/// A pure contract has no stdout, so the CuNi driver (`say` lines) is not
/// part of this artifact — it runs only in the Python reference.
/// `generate_program` embeds this output verbatim.
///
/// Lockstep: the reference is generated first and discarded. The contract
/// emits no driver, but it must not silently swallow a program the
/// reference refuses (e.g. a float inside a `say` line): contract-accept
/// implies reference-accept, so the gate can always prove what the contract
/// states.
pub fn logic_core(program: &Program) -> Result<String, String> {
    let _ = generate_reference(program)?;
    let mut g = Codegen::new(program);
    g.gen_logic_core(program)?;
    Ok(g.out)
}

/// Standalone runnable reference of the pure logic core: Python, run with
/// `python3`; `main` driver prints the `say` outputs. This is what the gate
/// proves byte-identical to CuNi gold.
///
/// `dec` is replicated as scaled Python ints; truncation-toward-zero
/// division is implemented manually (`_cuni_tdiv`) — Python's `//` floors
/// and is never used on a possibly-negative dividend. Canonical dec
/// rendering (`_cuni_dec_str`) follows docs/DECIMAL.md section 6 and is
/// identical to the Cairo emitter's reference.
pub fn generate_reference(program: &Program) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_reference(program)?;
    Ok(g.out)
}

/// Full Vyper contract: the logic core embedded verbatim (clearly delimited),
/// plus the contract shell (also clearly delimited) that requires the Vyper
/// compiler.
pub fn generate_program(program: &Program, _mod_name: &str) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_program(program)?;
    Ok(g.out)
}

impl Codegen {
    /// Shared refusal surface: every item kind the v1 profile cannot prove.
    fn check_items(&self, program: &Program) -> Result<(), String> {
        for item in &program.items {
            match item {
                Item::Stmt(s) => {
                    // The contract has no driver and no stdout: an interpolated
                    // (backtick) driver string has no on-chain form, so the
                    // program is refused even though the Python reference
                    // could render it.
                    if stmt_has_interp(s) {
                        return Err(
                            "interpolated (backtick) strings have no v1 Vyper-logic form; refusing"
                                .into(),
                        );
                    }
                }
                Item::Typ(t) => {
                    return Err(format!(
                        "typ `{}` has no v1 Vyper-logic form; refusing (integer/decimal/fn core only)",
                        t.name
                    ))
                }
                Item::Enum(e) => {
                    return Err(format!(
                        "enum `{}` has no v1 Vyper-logic form; refusing (integer/decimal/fn core only)",
                        e.name
                    ))
                }
                Item::Use(u) => {
                    return Err(format!(
                        "`use {}` imports have no on-chain Vyper form; refusing",
                        u.name
                    ))
                }
                Item::Ext(e) => {
                    return Err(format!(
                        "`ext {}` blocks have no on-chain Vyper form; refusing",
                        e.name
                    ))
                }
                Item::Iface(i) => {
                    return Err(format!(
                        "`iface {}` has no on-chain Vyper form; refusing",
                        i.name
                    ))
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn gen_logic_core(&mut self, program: &Program) -> Result<(), String> {
        self.check_items(program)?;
        for item in &program.items {
            if let Item::Def(f) = item {
                self.gen_def(f)?;
                self.out.push('\n');
            }
        }
        Ok(())
    }

    fn gen_program(&mut self, program: &Program) -> Result<(), String> {
        self.check_items(program)?;
        self.comment(0, "Generated by the CuNi Vyper backend.");
        self.comment(0, "\"Trust Provable, in all things.\"");
        self.comment(0, "");
        self.comment(
            0,
            "STRUCTURE — two delimited regions, one law (same logic or refuse):",
        );
        self.comment(
            0,
            "- Logic core (CUNI-LOGIC-CORE markers): `@internal @pure` Vyper",
        );
        self.comment(
            0,
            "  functions, one per CuNi `def`, underscore-prefixed per the Vyper",
        );
        self.comment(
            0,
            "  docs' own internal/external pattern. `dec` is int256 scaled 10^4",
        );
        self.comment(
            0,
            "  (docs/DECIMAL.md) — deliberately NOT Vyper's native `decimal`",
        );
        self.comment(
            0,
            "  (scale 10^10): CuNi proves the semantics it was given. No driver",
        );
        self.comment(
            0,
            "  here: a pure contract has no stdout; the `say` driver runs only",
        );
        self.comment(0, "  in the Python reference (`--emit-vyper-ref`).");
        self.comment(
            0,
            "- Contract shell (CUNI-VYPER-SHELL markers): one `@external @pure`",
        );
        self.comment(
            0,
            "  wrapper per CuNi `def`, returning the computed value (`dec`",
        );
        self.comment(
            0,
            "  values cross the ABI scaled by 10^4). Requires the Vyper",
        );
        self.comment(
            0,
            "  compiler; NOT compiled by `cuni check` (no Vyper toolchain on",
        );
        self.comment(0, "  the check machine).");
        self.comment(0, "");
        self.comment(
            0,
            "HONEST BOUNDARIES: logic gate-proven via the Python reference;",
        );
        self.comment(0, "shell not compiled here; nothing has executed on-chain.");
        self.out.push('\n');

        // Region 1: the logic core, embedded verbatim.
        self.line(0, LOGIC_START);
        let core = logic_core(program)?;
        self.out.push_str(&core);
        self.line(0, LOGIC_END);
        self.out.push('\n');

        // Region 2: the contract shell.
        self.line(0, SHELL_START);
        self.comment(
            0,
            "Requires: Vyper >= 0.4.0. This shell is NOT compiled by `cuni check` —",
        );
        self.comment(0, "no Vyper toolchain on the check machine.");
        self.out.push('\n');
        let defs: Vec<&FnDecl> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Def(f) => Some(f),
                _ => None,
            })
            .collect();
        if defs.is_empty() {
            self.comment(0, "(no CuNi functions: nothing to expose as external calls)");
        } else {
            for (n, f) in defs.iter().enumerate() {
                if n > 0 {
                    self.out.push('\n');
                }
                self.gen_wrapper(f)?;
            }
        }
        self.line(0, SHELL_END);
        Ok(())
    }

    /// One `@external @pure` wrapper per CuNi function: call the internal
    /// logic and return the computed value. Uniform by design: the shell
    /// does not guess which functions are "validators" — it computes and
    /// returns. Internal/external share the name modulo the underscore
    /// prefix (the Vyper docs' own pattern).
    fn gen_wrapper(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        let mut arg_names = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, vyper_type(&p.ty)?));
            arg_names.push(p.name.clone());
        }
        let ret = vyper_type(&f.ret_type)?;
        let rk = kind_of_type(&f.ret_type);
        self.line(0, "@external");
        self.line(0, "@pure");
        self.line(
            0,
            &format!("def {}({}) -> {}:", f.name, params.join(", "), ret),
        );
        if rk == VKind::Dec {
            self.comment(
                1,
                "CuNi `dec` crosses the ABI scaled by 10^4 (docs/DECIMAL.md).",
            );
        }
        self.line(
            1,
            &format!("return self._{}({})", f.name, arg_names.join(", ")),
        );
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, vyper_type(&p.ty)?));
        }
        let ret = vyper_type(&f.ret_type)?;
        self.line(0, "@internal");
        self.line(0, "@pure");
        self.line(
            0,
            &format!("def _{}({}) -> {}:", f.name, params.join(", "), ret),
        );
        let mut scope: HashMap<String, VKind> = HashMap::new();
        for p in &f.params {
            scope.insert(p.name.clone(), kind_of_type(&p.ty));
        }
        for s in &f.body {
            self.gen_stmt(1, s, &mut scope, false)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn gen_stmt(
        &mut self,
        indent: usize,
        stmt: &Stmt,
        scope: &mut HashMap<String, VKind>,
        in_reference: bool,
    ) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => self.gen_binding(indent, name, ty, value, scope),
            StmtKind::Mut { name, ty, value } => self.gen_binding(indent, name, ty, value, scope),
            StmtKind::Assign { target, value } => {
                let t = self.gen_expr(target, scope)?;
                let v = self.gen_expr(value, scope)?;
                self.line(indent, &format!("{t} = {v}"));
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let text = self.gen_expr(e, scope)?;
                self.line(indent, &format!("return {text}"));
                Ok(())
            }
            StmtKind::Ret(None) => {
                self.line(indent, "return");
                Ok(())
            }
            StmtKind::Fail(e) => match &e.kind {
                ExprKind::Str(s) => {
                    self.line(indent, &format!("raise \"{}\"", Self::esc(s)));
                    Ok(())
                }
                _ => Err("fail with a non-string has no clean Vyper abort; refusing".into()),
            },
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("if {cond_s}:"));
                self.gen_block(indent + 1, then_body, scope, in_reference)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "else:");
                    self.gen_block(indent + 1, else_body, scope, in_reference)?;
                }
                Ok(())
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                if b.is_some() {
                    return Err("two-binding for has no Vyper-logic form; refusing".into());
                }
                let (start_s, end_e) = match &iter.kind {
                    ExprKind::Call { callee, args } => {
                        let is_range = matches!(&callee.kind, ExprKind::Ident(n) if n == "range");
                        if !is_range {
                            return Err(
                                "for over non-range iterables has no Vyper-logic form; refusing"
                                    .into(),
                            );
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => {
                                return Err("range() with step has no Vyper-logic form; refusing".into())
                            }
                        }
                    }
                    _ => {
                        return Err("for over non-range iterables has no Vyper-logic form; refusing"
                            .into())
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr(se, scope)?,
                    None => "0".to_string(),
                };
                let e = self.gen_expr(end_e, scope)?;
                scope.insert(a.clone(), VKind::Int);
                self.line(indent, &format!("for {a}: int256 in range({s}, {e}):"));
                self.gen_block(indent + 1, body, scope, in_reference)?;
                Ok(())
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("while {cond_s}:"));
                self.gen_block(indent + 1, body, scope, in_reference)?;
                Ok(())
            }
            StmtKind::ExprStmt(e) => self.gen_expr_stmt(indent, e, scope, in_reference),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become a Vyper contract; refusing".into())
            }
        }
    }

    fn gen_binding(
        &mut self,
        indent: usize,
        name: &str,
        ty: &Option<Type>,
        value: &Expr,
        scope: &mut HashMap<String, VKind>,
    ) -> Result<(), String> {
        if matches!(value.kind, ExprKind::Unwrap { .. }) {
            return Err("`??` has no Vyper-logic equivalent for internal calls; refusing".into());
        }
        let kind = ty.as_ref().map(kind_of_type).unwrap_or(VKind::Other);
        let kind = if kind == VKind::Other {
            self.expr_kind(value, scope)
        } else {
            kind
        };
        let decl_ty = match ty {
            Some(t) => vyper_type(t)?,
            None => match kind {
                VKind::Int | VKind::Dec => "int256".into(),
                VKind::Str => "String[256]".into(),
                VKind::Bool => "bool".into(),
                VKind::Other => {
                    return Err(format!(
                        "cannot infer a Vyper-logic type for `{name}`; annotate it"
                    ))
                }
            },
        };
        scope.insert(name.to_string(), kind);
        let v = self.gen_expr(value, scope)?;
        self.line(indent, &format!("{name}: {decl_ty} = {v}"));
        Ok(())
    }

    fn gen_block(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, VKind>,
        in_reference: bool,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt(indent, s, scope, in_reference)?;
        }
        Ok(())
    }

    /// Expression statements. In the contract, `say` is refused everywhere:
    /// a `@pure` function cannot print, and the top-level driver is not part
    /// of the contract artifact — `say` runs only in the Python reference.
    fn gen_expr_stmt(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &mut HashMap<String, VKind>,
        in_reference: bool,
    ) -> Result<(), String> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                if in_reference {
                    return self.gen_say_ref(indent, args, scope);
                }
                return Err(
                    "say has no on-chain Vyper form: a `@pure` function cannot print; \
                     the CuNi driver (`say` lines) runs only in the Python reference \
                     (`--emit-vyper-ref`) — refusing"
                        .into(),
                );
            }
        }
        let text = self.gen_expr(e, scope)?;
        self.line(indent, &format!("{text}"));
        Ok(())
    }

    fn gen_expr(&mut self, e: &Expr, scope: &HashMap<String, VKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(n.to_string()),
            // `dec` literals (docs/DECIMAL.md): the parser already scaled to
            // 10^4, so the literal IS the scaled int256.
            ExprKind::Dec(scaled) => Ok(scaled.to_string()),
            ExprKind::Float(_) => Err("float literals have no Vyper-logic form; refusing".into()),
            ExprKind::Time(_) => Err("time literals have no Vyper-logic form (v1); refusing".into()),
            ExprKind::Bool(b) => Ok(if *b { "True".into() } else { "False".into() }),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(_) => Err(
                "interpolated (backtick) strings have no v1 Vyper-logic form; refusing".into(),
            ),
            ExprKind::NoneLit => Err("None has no Vyper-logic form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Vyper-logic form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 Vyper-logic form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Vyper-logic form (structs/enums are refused); refusing"
                    .into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => self.gen_binary(*op, lhs, rhs, scope),
            ExprKind::Unary { op, expr } => {
                let t = self.gen_expr(expr, scope)?;
                match op {
                    UnOp::Not => Ok(format!("(not {t})")),
                    UnOp::Neg => Ok(format!("(-{t})")),
                }
            }
            ExprKind::Unwrap { .. } => {
                Err("`??` has no Vyper-logic equivalent for internal calls; refusing".into())
            }
        }
    }

    /// Binary operators with the dec/int closed world (docs/DECIMAL.md
    /// sections 3-5): both operands dec, both int, or a loud refusal.
    /// EVM SDIV/SSMOD already truncate toward zero / take the dividend's
    /// sign, so int `/` and `%` need no helpers.
    fn gen_binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, VKind>,
    ) -> Result<String, String> {
        let l = self.gen_expr(lhs, scope)?;
        let r = self.gen_expr(rhs, scope)?;
        let lk = self.expr_kind(lhs, scope);
        let rk = self.expr_kind(rhs, scope);
        // Mixed dec/int is a typeck error with a fix-it; the emitter refuses
        // too, in case it is ever driven without the checker.
        if (lk == VKind::Dec) != (rk == VKind::Dec) {
            return Err("cannot mix `dec` and `int` — convert explicitly: \
                        `dec_of_int(n)` or `int_of_dec(d)`; refusing"
                .into());
        }
        if lk == VKind::Dec {
            let o = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                // trunc(a*b/10000) toward zero (docs/DECIMAL.md section 3).
                BinOp::Mul => return Ok(format!("(({l} * {r}) / 10000)")),
                // trunc(a*10000/b) toward zero.
                BinOp::Div => return Ok(format!("(({l} * 10000) / ({r}))")),
                BinOp::Mod => return Err("`%` is not defined on `dec`; refusing".into()),
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
                BinOp::And | BinOp::Or => {
                    return Err("`and`/`or` on `dec` has no Vyper-logic form; refusing".into())
                }
            };
            return Ok(format!("({l} {o} {r})"));
        }
        if lk == VKind::Str && matches!(op, BinOp::Add) {
            return Err(
                "string `+` (concat) has no v1 Vyper-logic form — `concat` widens \
                 `String[256]` beyond its bound and the backend will not guess at the \
                 sizing plumbing; refusing"
                    .into(),
            );
        }
        let o = match op {
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
        };
        Ok(format!("({l} {o} {r})"))
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, VKind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a Vyper-logic form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no Vyper-logic form; refusing".into());
        }
        let vals: Vec<String> = args
            .iter()
            .map(|a| self.gen_expr(a.expr(), scope))
            .collect::<Result<_, _>>()?;
        let arity = |want: usize| {
            if vals.len() != want {
                Err(format!("`{name}` takes {want} argument(s)"))
            } else {
                Ok(())
            }
        };
        // Builtins.
        match name.as_str() {
            "say" => return Err("say is a statement, not an expression; refusing".into()),
            "range" => return Err("range() outside for has no Vyper-logic form; refusing".into()),
            "len" => {
                arity(1)?;
                let k = self.expr_kind(args[0].expr(), scope);
                return match k {
                    // Vyper `len` returns uint256; CuNi `len` returns int.
                    VKind::Str => Ok(format!("convert(len({}), int256)", vals[0])),
                    _ => Err("len() of this value has no Vyper-logic form; refusing".into()),
                };
            }
            // int-only per docs/DECIMAL.md section 5; lowered without
            // builtins so no Vyper-version guesswork is needed.
            "abs" => {
                arity(1)?;
                let a = &vals[0];
                return Ok(format!("(({a} * -1) if ({a} < 0) else ({a}))"));
            }
            "min" => {
                arity(2)?;
                return Ok(format!(
                    "(({a}) if ({a} < {b}) else ({b}))",
                    a = vals[0],
                    b = vals[1]
                ));
            }
            "max" => {
                arity(2)?;
                return Ok(format!(
                    "(({a}) if ({a} > {b}) else ({b}))",
                    a = vals[0],
                    b = vals[1]
                ));
            }
            // Explicit dec/int conversions (docs/DECIMAL.md section 5).
            // SDIV truncates toward zero, so int_of_dec needs no helper.
            "dec_of_int" => {
                arity(1)?;
                return Ok(format!("(({} * 10000))", vals[0]));
            }
            "int_of_dec" => {
                arity(1)?;
                return Ok(format!("(({} / 10000))", vals[0]));
            }
            _ => {}
        }
        if !self.fn_names.contains(&name) {
            return Err(format!("unknown call `{name}`; refusing"));
        }
        // Internal logic functions are underscore-prefixed in the contract.
        Ok(format!("self._{}({})", name, vals.join(", ")))
    }

    /// Best-effort kind of an expression for `say` routing and inference.
    fn expr_kind(&self, e: &Expr, scope: &HashMap<String, VKind>) -> VKind {
        match &e.kind {
            ExprKind::Int(_) => VKind::Int,
            ExprKind::Dec(_) => VKind::Dec,
            ExprKind::Str(_) | ExprKind::InterpStr(_) => VKind::Str,
            ExprKind::Bool(_) => VKind::Bool,
            ExprKind::Ident(n) => scope.get(n).copied().unwrap_or(VKind::Other),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "len" => VKind::Int,
                ExprKind::Ident(n) if n == "abs" || n == "min" || n == "max" => VKind::Int,
                ExprKind::Ident(n) if n == "dec_of_int" => VKind::Dec,
                ExprKind::Ident(n) if n == "int_of_dec" => VKind::Int,
                ExprKind::Ident(n) => self.fn_ret.get(n).copied().unwrap_or(VKind::Other),
                _ => VKind::Other,
            },
            ExprKind::Binary { op, lhs, .. } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => VKind::Bool,
                _ => self.expr_kind(lhs, scope),
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => VKind::Bool,
                UnOp::Neg => self.expr_kind(expr, scope),
            },
            _ => VKind::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// Python reference
// ---------------------------------------------------------------------------

impl Codegen {
    fn gen_reference(&mut self, program: &Program) -> Result<(), String> {
        self.check_items(program)?;
        self.line(0, "#!/usr/bin/env python3");
        self.line(
            0,
            "\"\"\"CuNi Vyper logic-core reference: pure logic + driver, run with python3.",
        );
        self.line(
            0,
            "The `say` driver prints exactly what `cuni run` prints on the same",
        );
        self.line(
            0,
            "source; the gate asserts byte-identical stdout. `dec` is a scaled",
        );
        self.line(
            0,
            "Python int (scale 10^4, docs/DECIMAL.md) — truncation toward zero",
        );
        self.line(
            0,
            "is implemented by hand (`_cuni_tdiv`): Python's `//` floors and is",
        );
        self.line(0, "never used on a possibly-negative dividend.");
        self.line(0, "\"\"\"");
        self.out.push('\n');
        self.line(0, "import sys");
        self.out.push('\n');
        self.line(0, "def _cuni_tdiv(a, b):");
        self.line(
            1,
            "\"\"\"Truncating integer division toward zero (CuNi `int` `/`, and the",
        );
        self.line(1, "core of `dec` `*` and `/`). Python's `//` floors — it is never");
        self.line(1, "used here on a possibly-negative dividend.");
        self.line(1, "\"\"\"");
        self.line(1, "if b == 0:");
        self.line(2, "raise ZeroDivisionError(\"cuni: division by zero\")");
        self.line(1, "q = abs(a) // abs(b)");
        self.line(1, "return -q if (a < 0) != (b < 0) else q");
        self.out.push('\n');
        self.line(0, "def _cuni_tmod(a, b):");
        self.line(1, "\"\"\"Truncated remainder: sign follows the dividend, like CuNi.\"\"\"");
        self.line(1, "return a - _cuni_tdiv(a, b) * b");
        self.out.push('\n');
        self.line(0, "def _cuni_dec_str(v):");
        self.line(1, "\"\"\"Canonical dec rendering (docs/DECIMAL.md section 6).\"\"\"");
        self.line(1, "neg = v < 0");
        self.line(1, "mag = -v if neg else v");
        self.line(1, "ip = mag // 10000");
        self.line(1, "fp = mag % 10000");
        self.line(1, "frac = (\"%04d\" % fp).rstrip(\"0\") or \"0\"");
        self.line(1, "return (\"-\" if neg else \"\") + str(ip) + \".\" + frac");
        self.out.push('\n');
        self.line(0, "def _cuni_bool_str(b):");
        self.line(1, "return \"True\" if b else \"False\"");
        self.out.push('\n');

        for item in &program.items {
            if let Item::Def(f) = item {
                self.gen_def_ref(f)?;
                self.out.push('\n');
            }
        }

        let top: Vec<&Stmt> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Stmt(s) => Some(s),
                _ => None,
            })
            .collect();
        self.line(0, "def main():");
        if top.is_empty() {
            self.line(1, "pass");
        } else {
            let mut scope = HashMap::new();
            for s in top {
                self.gen_stmt_ref(1, s, &mut scope)?;
            }
        }
        self.out.push('\n');
        self.line(0, "main()");
        Ok(())
    }

    fn gen_def_ref(&mut self, f: &FnDecl) -> Result<(), String> {
        let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
        // Validate the declared types even though Python is untyped: a `dec`
        // param is still a scaled int here, and anything unmappable refuses.
        for p in &f.params {
            vyper_type(&p.ty)?;
        }
        vyper_type(&f.ret_type)?;
        self.line(0, &format!("def {}({}):", f.name, params.join(", ")));
        let mut scope: HashMap<String, VKind> = HashMap::new();
        for p in &f.params {
            scope.insert(p.name.clone(), kind_of_type(&p.ty));
        }
        if f.body.is_empty() {
            self.line(1, "pass");
        }
        for s in &f.body {
            self.gen_stmt_ref(1, s, &mut scope)?;
        }
        Ok(())
    }

    fn gen_stmt_ref(
        &mut self,
        indent: usize,
        stmt: &Stmt,
        scope: &mut HashMap<String, VKind>,
    ) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                if let Some(t) = ty {
                    vyper_type(t)?;
                }
                if matches!(value.kind, ExprKind::Unwrap { .. }) {
                    return Err("`??` has no reference form for internal calls; refusing".into());
                }
                let kind = ty.as_ref().map(kind_of_type).unwrap_or(VKind::Other);
                let kind = if kind == VKind::Other {
                    self.expr_kind(value, scope)
                } else {
                    kind
                };
                if kind == VKind::Other {
                    return Err(format!(
                        "cannot infer a reference type for `{name}`; annotate it"
                    ));
                }
                scope.insert(name.to_string(), kind);
                let v = self.gen_expr_ref(value, scope)?;
                self.line(indent, &format!("{name} = {v}"));
                Ok(())
            }
            StmtKind::Assign { target, value } => {
                let t = self.gen_expr_ref(target, scope)?;
                let v = self.gen_expr_ref(value, scope)?;
                self.line(indent, &format!("{t} = {v}"));
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let text = self.gen_expr_ref(e, scope)?;
                self.line(indent, &format!("return {text}"));
                Ok(())
            }
            StmtKind::Ret(None) => {
                self.line(indent, "return");
                Ok(())
            }
            StmtKind::Fail(e) => match &e.kind {
                ExprKind::Str(s) => {
                    self.line(indent, &format!("raise RuntimeError({:?})", s));
                    Ok(())
                }
                _ => Err("fail with a non-string has no clean reference abort; refusing".into()),
            },
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr_ref(cond, scope)?;
                self.line(indent, &format!("if {cond_s}:"));
                self.gen_block_ref(indent + 1, then_body, scope)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "else:");
                    self.gen_block_ref(indent + 1, else_body, scope)?;
                }
                Ok(())
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                if b.is_some() {
                    return Err("two-binding for has no reference form; refusing".into());
                }
                let (start_s, end_e) = match &iter.kind {
                    ExprKind::Call { callee, args } => {
                        let is_range = matches!(&callee.kind, ExprKind::Ident(n) if n == "range");
                        if !is_range {
                            return Err("for over non-range iterables has no reference form; refusing"
                                .into());
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => return Err("range() with step has no reference form; refusing".into()),
                        }
                    }
                    _ => {
                        return Err("for over non-range iterables has no reference form; refusing"
                            .into())
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr_ref(se, scope)?,
                    None => "0".to_string(),
                };
                let e = self.gen_expr_ref(end_e, scope)?;
                scope.insert(a.clone(), VKind::Int);
                self.line(indent, &format!("for {a} in range({s}, {e}):"));
                self.gen_block_ref(indent + 1, body, scope)?;
                Ok(())
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr_ref(cond, scope)?;
                self.line(indent, &format!("while {cond_s}:"));
                self.gen_block_ref(indent + 1, body, scope)?;
                Ok(())
            }
            StmtKind::ExprStmt(e) => {
                if let ExprKind::Call { callee, args } = &e.kind {
                    if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                        return self.gen_say_ref(indent, args, scope);
                    }
                }
                let text = self.gen_expr_ref(e, scope)?;
                self.line(indent, &text);
                Ok(())
            }
            StmtKind::Todo => Err("`...` placeholder body cannot become a reference; refusing".into()),
        }
    }

    fn gen_block_ref(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, VKind>,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt_ref(indent, s, scope)?;
        }
        Ok(())
    }

    /// `say(x)` in the reference driver: print the canonical rendering.
    /// (The contract path calls this only with `in_reference = true` for
    /// top-level statements — but the contract emits no driver, so this is
    /// reference-only in practice.)
    fn gen_say_ref(
        &mut self,
        indent: usize,
        args: &[CallArg],
        scope: &HashMap<String, VKind>,
    ) -> Result<(), String> {
        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
        if vals.len() != 1 {
            return Err("say takes exactly one argument".into());
        }
        let v = vals[0];
        let text = match &v.kind {
            ExprKind::InterpStr(parts) => self.gen_interp_ref(parts, scope)?,
            _ => {
                let kind = self.expr_kind(v, scope);
                let t = self.gen_expr_ref(v, scope)?;
                match kind {
                    VKind::Int => format!("print({t})"),
                    VKind::Dec => format!("print(_cuni_dec_str({t}))"),
                    VKind::Bool => format!("print(_cuni_bool_str({t}))"),
                    VKind::Str => format!("print({t})"),
                    VKind::Other => {
                        return Err("say of this value has no reference form; refusing".into())
                    }
                }
            }
        };
        self.line(indent, &text);
        Ok(())
    }

    fn gen_interp_ref(
        &mut self,
        parts: &[StrPartExpr],
        scope: &HashMap<String, VKind>,
    ) -> Result<String, String> {
        let mut pieces = Vec::new();
        for p in parts {
            match p {
                StrPartExpr::Text(t) => pieces.push(format!("{:?}", t)),
                StrPartExpr::Expr(ie) => {
                    let k = self.expr_kind(ie, scope);
                    let t = self.gen_expr_ref(ie, scope)?;
                    match k {
                        VKind::Int => pieces.push(format!("str({t})")),
                        VKind::Dec => pieces.push(format!("_cuni_dec_str({t})")),
                        VKind::Bool => pieces.push(format!("_cuni_bool_str({t})")),
                        VKind::Str => pieces.push(format!("({t})")),
                        VKind::Other => {
                            return Err(
                                "interpolating this value has no reference form; refusing".into()
                            )
                        }
                    }
                }
            }
        }
        if pieces.is_empty() {
            return Ok("print(\"\")".to_string());
        }
        Ok(format!("print({})", pieces.join(" + ")))
    }

    fn gen_expr_ref(&mut self, e: &Expr, scope: &HashMap<String, VKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(n.to_string()),
            ExprKind::Dec(scaled) => Ok(scaled.to_string()),
            ExprKind::Float(_) => Err("float literals have no reference form; refusing".into()),
            ExprKind::Time(_) => Err("time literals have no reference form (v1); refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("{:?}", s)),
            ExprKind::InterpStr(parts) => {
                // Interpolated string as a value (not via say): build it.
                let mut pieces = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => pieces.push(format!("{:?}", t)),
                        StrPartExpr::Expr(ie) => {
                            let k = self.expr_kind(ie, scope);
                            let t = self.gen_expr_ref(ie, scope)?;
                            match k {
                                VKind::Int => pieces.push(format!("str({t})")),
                                VKind::Dec => pieces.push(format!("_cuni_dec_str({t})")),
                                VKind::Bool => pieces.push(format!("_cuni_bool_str({t})")),
                                VKind::Str => pieces.push(format!("({t})")),
                                VKind::Other => {
                                    return Err(
                                        "interpolating this value has no reference form; refusing"
                                            .into(),
                                    )
                                }
                            }
                        }
                    }
                }
                if pieces.is_empty() {
                    Ok("\"\"".to_string())
                } else {
                    Ok(format!("({})", pieces.join(" + ")))
                }
            }
            ExprKind::NoneLit => Err("None has no reference form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 reference form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call_ref(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 reference form; refusing".into()),
            ExprKind::Field { .. } => Err("field access has no v1 reference form; refusing".into()),
            ExprKind::Binary { op, lhs, rhs } => self.gen_binary_ref(*op, lhs, rhs, scope),
            ExprKind::Unary { op, expr } => {
                let t = self.gen_expr_ref(expr, scope)?;
                match op {
                    UnOp::Not => Ok(format!("(not {t})")),
                    UnOp::Neg => Ok(format!("(-{t})")),
                }
            }
            ExprKind::Unwrap { .. } => Err("`??` has no reference form for internal calls; refusing".into()),
        }
    }

    fn gen_binary_ref(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, VKind>,
    ) -> Result<String, String> {
        let l = self.gen_expr_ref(lhs, scope)?;
        let r = self.gen_expr_ref(rhs, scope)?;
        let lk = self.expr_kind(lhs, scope);
        let rk = self.expr_kind(rhs, scope);
        if (lk == VKind::Dec) != (rk == VKind::Dec) {
            return Err("cannot mix `dec` and `int` — convert explicitly: \
                        `dec_of_int(n)` or `int_of_dec(d)`; refusing"
                .into());
        }
        if lk == VKind::Dec {
            return match op {
                BinOp::Add => Ok(format!("({l} + {r})")),
                BinOp::Sub => Ok(format!("({l} - {r})")),
                BinOp::Mul => Ok(format!("_cuni_tdiv(({l}) * ({r}), 10000)")),
                BinOp::Div => Ok(format!("_cuni_tdiv(({l}) * 10000, ({r}))")),
                BinOp::Mod => Err("`%` is not defined on `dec`; refusing".into()),
                BinOp::Eq => Ok(format!("({l} == {r})")),
                BinOp::Ne => Ok(format!("({l} != {r})")),
                BinOp::Lt => Ok(format!("({l} < {r})")),
                BinOp::Gt => Ok(format!("({l} > {r})")),
                BinOp::Le => Ok(format!("({l} <= {r})")),
                BinOp::Ge => Ok(format!("({l} >= {r})")),
                BinOp::And | BinOp::Or => {
                    Err("`and`/`or` on `dec` has no reference form; refusing".into())
                }
            };
        }
        if lk == VKind::Str && matches!(op, BinOp::Add) {
            return Ok(format!("({l} + {r})"));
        }
        match op {
            BinOp::Add => Ok(format!("({l} + {r})")),
            BinOp::Sub => Ok(format!("({l} - {r})")),
            BinOp::Mul => Ok(format!("({l} * {r})")),
            BinOp::Div => Ok(format!("_cuni_tdiv({l}, {r})")),
            BinOp::Mod => Ok(format!("_cuni_tmod({l}, {r})")),
            BinOp::Eq => Ok(format!("({l} == {r})")),
            BinOp::Ne => Ok(format!("({l} != {r})")),
            BinOp::Lt => Ok(format!("({l} < {r})")),
            BinOp::Gt => Ok(format!("({l} > {r})")),
            BinOp::Le => Ok(format!("({l} <= {r})")),
            BinOp::Ge => Ok(format!("({l} >= {r})")),
            BinOp::And => Ok(format!("({l} and {r})")),
            BinOp::Or => Ok(format!("({l} or {r})")),
        }
    }

    fn gen_call_ref(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, VKind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a reference form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no reference form; refusing".into());
        }
        let vals: Vec<String> = args
            .iter()
            .map(|a| self.gen_expr_ref(a.expr(), scope))
            .collect::<Result<_, _>>()?;
        let arity = |want: usize| {
            if vals.len() != want {
                Err(format!("`{name}` takes {want} argument(s)"))
            } else {
                Ok(())
            }
        };
        match name.as_str() {
            "say" => return Err("say is a statement, not an expression; refusing".into()),
            "range" => return Err("range() outside for has no reference form; refusing".into()),
            "len" => {
                arity(1)?;
                return Ok(format!("len({})", vals[0]));
            }
            "abs" => {
                arity(1)?;
                return Ok(format!("abs({})", vals[0]));
            }
            "min" => {
                arity(2)?;
                return Ok(format!("min({}, {})", vals[0], vals[1]));
            }
            "max" => {
                arity(2)?;
                return Ok(format!("max({}, {})", vals[0], vals[1]));
            }
            "dec_of_int" => {
                arity(1)?;
                return Ok(format!("(({}) * 10000)", vals[0]));
            }
            "int_of_dec" => {
                arity(1)?;
                return Ok(format!("_cuni_tdiv({}, 10000)", vals[0]));
            }
            _ => {}
        }
        if !self.fn_names.contains(&name) {
            return Err(format!("unknown call `{name}`; refusing"));
        }
        Ok(format!("{}({})", name, vals.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn parse_src(src: &str) -> Program {
        let toks = lexer::Lexer::tokenize(src).expect("lex");
        let mut p = parser::Parser::new(toks, src);
        p.parse_program().expect("parse")
    }

    const DEMO: &str = r#"
def fee(amount: dec) -> dec do
    if amount <= 100.00dec do
        ret amount * 0.01dec + 0.25dec
    end
    ret amount * 0.005dec + 0.25dec
end

def double(n: int) -> int do
    ret n * 2
end

say(fee(99.9999dec))
say(double(21))
"#;

    #[test]
    fn logic_core_is_pure_internal_helpers() {
        let core = logic_core(&parse_src(DEMO)).expect("logic core");
        assert!(core.contains("@internal"));
        assert!(core.contains("@pure"));
        assert!(core.contains("def _fee(amount: int256) -> int256:"));
        assert!(core.contains("def _double(n: int256) -> int256:"));
        // dec mul lowers to trunc(a*b/10000) toward zero; no driver here.
        assert!(core.contains("((amount * 100) / 10000)"));
        assert!(!core.contains("say"), "contract logic core must not print");
        assert!(!core.contains("def main"), "no driver in the contract");
    }

    #[test]
    fn program_embeds_logic_core_verbatim() {
        let prog = parse_src(DEMO);
        let core = logic_core(&prog).expect("logic core");
        let program_src = generate_program(&prog, "cuni_demo").expect("program");
        let start = program_src.find(LOGIC_START).expect("logic start marker");
        let end = program_src.find(LOGIC_END).expect("logic end marker");
        assert!(start < end);
        let mut lines = program_src[start..end].lines();
        assert_eq!(lines.next().unwrap(), LOGIC_START);
        let extracted: String = lines.map(|l| format!("{l}\n")).collect();
        assert_eq!(
            extracted, core,
            "program's logic region must be byte-identical to logic_core()"
        );
        // Shell shape.
        for marker in [
            SHELL_START,
            SHELL_END,
            "@external",
            "def fee(amount: int256) -> int256:",
            "return self._fee(amount)",
            "def double(n: int256) -> int256:",
            "return self._double(n)",
        ] {
            assert!(program_src.contains(marker), "shell missing `{marker}`");
        }
    }

    #[test]
    fn refuses_unprovable() {
        for (tag, src) in [
            ("float", "def f() -> int do\n ret 1\nend\nsay(1.5)\n"),
            ("str-concat", "def f(a: str) -> str do\n ret a + \"x\"\nend\nsay(f(\"y\"))\n"),
            (
                "say-in-fn",
                "def f() -> int do\n say(1)\n ret 1\nend\nsay(f())\n",
            ),
            (
                "interp",
                "def f() -> int do\n ret 1\nend\nsay(`v ${f()}`)\n",
            ),
            ("list", "def f() -> int do\n ret 1\nend\nlet xs = [1]\nsay(f())\n"),
            ("time", "def f() -> int do\n ret 1\nend\nsay(\"2026-01-01T00:00:00Z\"t)\n"),
            (
                "typ",
                "typ Point do\n x: int\nend\ndef f() -> int do\n ret 1\nend\nsay(f())\n",
            ),
            (
                "dec-mod",
                "def f(a: dec) -> dec do\n ret a % 0.5dec\nend\nsay(f(1.0dec))\n",
            ),
            (
                "mixed",
                "def f(a: dec) -> dec do\n ret a + 1\nend\nsay(f(1.0dec))\n",
            ),
        ] {
            let err = logic_core(&parse_src(src)).expect_err(&format!("{tag} must refuse"));
            assert!(!err.is_empty(), "{tag}: refusal needs a reason");
        }
    }

    #[test]
    fn reference_renders_dec_canonically() {
        let r = generate_reference(&parse_src(DEMO)).expect("reference");
        assert!(r.contains("def _cuni_tdiv(a, b):"));
        assert!(r.contains("def _cuni_dec_str(v):"));
        assert!(r.contains("def fee(amount):"));
        assert!(r.contains("def main():"));
        assert!(r.contains("main()"));
        // say(fee(...)) prints the canonical dec rendering.
        assert!(r.contains("print(_cuni_dec_str(fee(999999)))"));
        assert!(r.contains("print(double(21))"));
    }
}

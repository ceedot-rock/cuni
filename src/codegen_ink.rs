//! ink! contract backend — Polkadot/Substrate ink! writer.
//!
//! "Trust Provable, in all things."
//!
//! Emits a genuine ink! contract shape from CuNi source: a pure logic core
//! (the part CuNi proves) plus the contract shell (`#[ink::contract]`,
//! storage struct, constructor, messages) that wraps it. Real Rust, like
//! the Solana profile.
//!
//! Two artifacts, one law:
//! - `logic_core` — the standalone logic core: `pub fn` definitions plus a
//!   `fn main` driver. It compiles with plain `rustc` (no dependencies) and
//!   its stdout is what the CuNi gate proves byte-identical across seats.
//! - `generate_program` — the full ink! contract: the logic core embedded
//!   verbatim inside `mod logic` (clearly delimited), plus the contract
//!   shell (also clearly delimited) that requires the ink! toolchain.
//!
//! Exactness notes (dictated by the 0.8.0 Onchain Division spec):
//! - CuNi `int` is i64 (truncated `/` and `%`, Rust semantics) — the logic
//!   core uses i64, matching the Solana profile exactly.
//! - CuNi `dec` is a scaled i128 (scale 10^4). `+`/`-` use checked integer
//!   ops; `*` is `trunc(a*b/10000)` and `/` is `trunc(a*10000/b)`, both
//!   truncation toward zero (i128 `/` already truncates); every true
//!   overflow and every division by zero is a loud panic — the same loud
//!   refusal the interpreter performs, never a silent wrap.
//! - Canonical `dec` rendering for `say` follows docs/DECIMAL.md §6
//!   (implemented once as `cuni_dec_str`, shared by reference and shell).
//! - `float`, `list`, `map`, `opt`, `??`, structs, and enums are honestly
//!   refused: an ink! message's verifiable core is integer/decimal math,
//!   and the backend will not guess at mappings it cannot prove.
//! - `say` becomes `println!` in the reference (stdout is the gate's
//!   comparison surface). `bool` prints canonically as `True`/`False`.
//! - `fail` becomes `panic!`, matching CuNi's abort semantics in the
//!   off-chain logic core. A production shell would map this to a contract
//!   error type.
//!
//! Honest boundaries: the logic core is gate-proven via `generate_reference`
//! (plain `rustc`, no dependencies); the contract shell is NOT compiled here
//! (no ink! toolchain on the check machine) and nothing has executed
//! on-chain. See `docs/ONCHAIN.md` for the full verification matrix.

use crate::ast::{
    BinOp, CallArg, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr, Type, UnOp,
};
use std::collections::{HashMap, HashSet};

/// ink!-logic kind of a CuNi value, for `say` routing and type inference.
#[derive(Clone, Copy, PartialEq, Eq)]
enum InkKind {
    Int,
    Dec,
    Str,
    Bool,
    Other,
}

/// Delimiters marking the two regions of a `--emit-ink` artifact.
pub const LOGIC_START: &str = "// CUNI-LOGIC-CORE-START";
pub const LOGIC_END: &str = "// CUNI-LOGIC-CORE-END";
pub const SHELL_START: &str = "// CUNI-INK-SHELL-START";
pub const SHELL_END: &str = "// CUNI-INK-SHELL-END";

pub struct Codegen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, InkKind>,
    /// Return kind of the function currently being generated (for `ret`
    /// literal adaptation).
    ret_kind: InkKind,
    out: String,
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
            ret_kind: InkKind::Other,
            out: String::new(),
        }
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn esc(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                c => r.push(c),
            }
        }
        r
    }

    /// Escape for `format!` literal text (`{`/`}` would start a placeholder).
    fn esc_fmt(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '{' => r.push_str("{{"),
                '}' => r.push_str("}}"),
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                c => r.push(c),
            }
        }
        r
    }
}

/// Map a CuNi type to its ink!-logic Rust type.
fn ink_type(ty: &Type) -> Result<String, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok("i64".into()),
            "dec" => Ok("i128".into()),
            "str" => Ok("String".into()),
            "bool" => Ok("bool".into()),
            "float" => Err("ink! logic core has no float type; refusing float".into()),
            other => Err(format!(
                "type `{}` has no ink!-logic mapping; refusing",
                other
            )),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{}` has no ink!-logic mapping; refusing",
            name
        )),
    }
}

fn kind_of_type(ty: &Type) -> InkKind {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => InkKind::Int,
            "dec" => InkKind::Dec,
            "str" => InkKind::Str,
            "bool" => InkKind::Bool,
            _ => InkKind::Other,
        },
        _ => InkKind::Other,
    }
}

/// The standalone logic core: helpers + pure functions + driver, no
/// dependencies.
///
/// This is the part CuNi proves. `generate_program` embeds its output
/// verbatim (indented) inside `mod logic`.
pub fn logic_core(program: &Program) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_logic_core(program)?;
    Ok(g.out)
}

/// Standalone runnable reference of the pure logic core: real Rust, compiles
/// with plain `rustc` (no dependencies); `fn main` driver prints the `say`
/// outputs. This is what the gate proves byte-identical to CuNi gold.
pub fn generate_reference(program: &Program) -> Result<String, String> {
    logic_core(program)
}

/// The full ink! contract: delimited logic core + contract shell.
///
/// `mod_name` is the contract module name (already sanitized, e.g.
/// `cuni_fee_schedule`).
pub fn generate_program(program: &Program, mod_name: &str) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_program(program, mod_name)?;
    Ok(g.out)
}

impl Codegen {
    /// Shared body: dec helpers, function definitions, top-level driver as
    /// `fn main`.
    fn gen_logic_core(&mut self, program: &Program) -> Result<(), String> {
        for item in &program.items {
            match item {
                Item::Typ(t) => {
                    return Err(format!(
                        "typ `{}` has no v1 ink!-logic form; refusing (integer/decimal/fn core only)",
                        t.name
                    ))
                }
                Item::Enum(e) => {
                    return Err(format!(
                        "enum `{}` has no v1 ink!-logic form; refusing (integer/decimal/fn core only)",
                        e.name
                    ))
                }
                _ => {}
            }
        }
        self.gen_dec_prelude();
        for item in &program.items {
            if let Item::Def(f) = item {
                self.gen_def(f)?;
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
        if !top.is_empty() {
            self.line(0, "fn main() {");
            let mut scope = HashMap::new();
            for s in top {
                self.gen_stmt(1, s, &mut scope)?;
            }
            self.line(0, "}");
        }
        Ok(())
    }

    /// The `dec` runtime: scaled-i128 arithmetic with CuNi's exact rules
    /// (docs/DECIMAL.md §3 — truncation toward zero, loud refusal on true
    /// overflow and division by zero) plus the canonical renderer (§6).
    /// Emitted once at the top of the logic core so both the standalone
    /// reference and the embedded `mod logic` share one implementation.
    fn gen_dec_prelude(&mut self) {
        self.line(0, "/// CuNi `dec`: scaled i128, scale 10^4 (docs/DECIMAL.md).");
        self.line(0, "const CUNI_DEC_SCALE: i128 = 10000;");
        self.out.push('\n');
        self.line(
            0,
            "/// Loud refusal: dec addition that overflows i128 aborts, never wraps.",
        );
        self.line(0, "fn cuni_dec_add(a: i128, b: i128) -> i128 {");
        self.line(
            1,
            "a.checked_add(b).expect(\"cuni: dec addition overflow \\u{2014} refused\")",
        );
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// Loud refusal: dec subtraction that overflows i128 aborts, never wraps.",
        );
        self.line(0, "fn cuni_dec_sub(a: i128, b: i128) -> i128 {");
        self.line(
            1,
            "a.checked_sub(b).expect(\"cuni: dec subtraction overflow \\u{2014} refused\")",
        );
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// `trunc(a*b/10000)` toward zero (docs/DECIMAL.md §3).",
        );
        self.line(0, "fn cuni_dec_mul(a: i128, b: i128) -> i128 {");
        self.line(
            1,
            "let p = a.checked_mul(b).expect(\"cuni: dec multiplication overflow \\u{2014} refused\");",
        );
        self.line(1, "p / CUNI_DEC_SCALE");
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// `trunc(a*10000/b)` toward zero; division by zero is a loud panic.",
        );
        self.line(0, "fn cuni_dec_div(a: i128, b: i128) -> i128 {");
        self.line(1, "if b == 0 {");
        self.line(2, "panic!(\"cuni: dec division by zero\");");
        self.line(1, "}");
        self.line(
            1,
            "let p = a.checked_mul(CUNI_DEC_SCALE).expect(\"cuni: dec division intermediate overflow \\u{2014} refused\");",
        );
        self.line(1, "p / b");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "fn cuni_dec_neg(a: i128) -> i128 {");
        self.line(
            1,
            "a.checked_neg().expect(\"cuni: dec negation overflow \\u{2014} refused\")",
        );
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "/// `dec_of_int`: exact `n * 10000`; refuses on overflow.");
        self.line(0, "fn cuni_dec_of_int(n: i64) -> i128 {");
        self.line(
            1,
            "(n as i128).checked_mul(CUNI_DEC_SCALE).expect(\"cuni: dec_of_int overflow \\u{2014} refused\")",
        );
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// `int_of_dec`: `trunc(d/10000)` toward zero; refuses on overflow.",
        );
        self.line(0, "fn cuni_int_of_dec(d: i128) -> i64 {");
        self.line(1, "let q = d / CUNI_DEC_SCALE;");
        self.line(1, "if q < i64::MIN as i128 || q > i64::MAX as i128 {");
        self.line(2, "panic!(\"cuni: int_of_dec overflow \\u{2014} refused\");");
        self.line(1, "}");
        self.line(1, "q as i64");
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// Canonical `dec` rendering (docs/DECIMAL.md §6): sign, integer",
        );
        self.line(
            0,
            "/// digits, \".\", fractional digits with trailing zeros stripped",
        );
        self.line(0, "/// (never empty — `1.0000` prints as `1.0`).");
        self.line(0, "fn cuni_dec_str(v: i128) -> String {");
        self.line(1, "let neg = v < 0;");
        self.line(1, "let mag = v.unsigned_abs();");
        self.line(1, "let ip = mag / (CUNI_DEC_SCALE as u128);");
        self.line(
            1,
            "let mut fp = format!(\"{:04}\", mag % (CUNI_DEC_SCALE as u128));",
        );
        self.line(1, "while fp.ends_with('0') {");
        self.line(2, "fp.pop();");
        self.line(1, "}");
        self.line(1, "if fp.is_empty() {");
        self.line(2, "fp.push('0');");
        self.line(1, "}");
        self.line(
            1,
            "format!(\"{}{}.{}\", if neg { \"-\" } else { \"\" }, ip, fp)",
        );
        self.line(0, "}");
        self.out.push('\n');
    }

    fn gen_program(&mut self, program: &Program, mod_name: &str) -> Result<(), String> {
        self.line(0, "//! Generated by the CuNi ink! backend.");
        self.line(0, "//! \"Trust Provable, in all things.\"");
        self.line(0, "//!");
        self.line(
            0,
            "//! STRUCTURE — two delimited regions, one law (same logic or refuse):",
        );
        self.line(
            0,
            "//! - `mod logic` (CUNI-LOGIC-CORE markers): the pure program logic,",
        );
        self.line(
            0,
            "//!   byte-identical to the standalone logic-core emit. The CuNi gate",
        );
        self.line(
            0,
            "//!   proves its stdout byte-identical across real toolchains; it",
        );
        self.line(0, "//!   compiles as plain Rust with no dependencies.");
        self.line(
            0,
            "//! - Contract shell (CUNI-INK-SHELL markers): the `#[ink::contract]`",
        );
        self.line(
            0,
            "//!   wrapper — storage struct, constructor, one message per CuNi",
        );
        self.line(
            0,
            "//!   function. Requires the ink! toolchain (`cargo-contract`); NOT",
        );
        self.line(
            0,
            "//!   compiled by `cuni check` (no ink! toolchain on the check",
        );
        self.line(0, "//!   machine). Full verification recipe: docs/ONCHAIN.md.");
        self.line(0, "//!");
        self.line(
            0,
            "//! HONEST BOUNDARIES: logic gate-proven; shell not compiled here;",
        );
        self.line(0, "//! nothing here has executed on-chain.");
        self.out.push('\n');

        // Region 1: the logic core, embedded verbatim (indented one level).
        //
        // One shell-owned adaptation line: ink! contracts build as no_std,
        // where `String` is not in the prelude, so `mod logic` imports it
        // from `ink::prelude`. The standalone reference compiles the same
        // core against std and never sees this line — the gate's extractor
        // strips exactly it before compiling.
        self.line(0, LOGIC_START);
        let core = logic_core(program)?;
        self.line(0, "mod logic {");
        self.line(1, "use ink::prelude::string::String;");
        for l in core.lines() {
            if l.trim().is_empty() {
                self.out.push('\n');
            } else {
                self.line(1, l);
            }
        }
        self.line(0, "}");
        self.line(0, LOGIC_END);
        self.out.push('\n');

        // Region 2: the ink! contract shell.
        self.line(0, SHELL_START);
        self.line(
            0,
            "// Requires: ink = \"4\" and the cargo-contract toolchain.",
        );
        self.line(
            0,
            "// This shell is NOT compiled by `cuni check` — see docs/ONCHAIN.md.",
        );
        self.line(
            0,
            "// Top-level driver statements (the off-chain `say` sequence) are not",
        );
        self.line(
            0,
            "// messages: each CuNi function is exposed as one message below;",
        );
        self.line(
            0,
            "// invoking them in driver order is the caller's job.",
        );
        self.out.push('\n');
        self.line(0, "#[ink::contract]");
        self.line(0, &format!("mod {} {{", mod_name));
        self.line(1, "use ink::prelude::string::String;");
        self.line(1, "use super::logic;");
        self.out.push('\n');
        self.line(1, "#[ink(storage)]");
        self.line(1, "pub struct Contract {");
        self.line(2, "/// Set once at construction; identifies the deploying account.");
        self.line(2, "owner: AccountId,");
        self.line(1, "}");
        self.out.push('\n');
        self.line(1, "impl Contract {");
        self.line(2, "/// Deploys the contract and records the caller as owner.");
        self.line(2, "#[ink(constructor)]");
        self.line(2, "pub fn new() -> Self {");
        self.line(3, "Self { owner: Self::env().caller() }");
        self.line(2, "}");
        let defs: Vec<&FnDecl> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Def(f) => Some(f),
                _ => None,
            })
            .collect();
        if defs.is_empty() {
            self.out.push('\n');
            self.line(2, "// (no CuNi functions: nothing to expose as messages)");
        } else {
            for f in &defs {
                self.out.push('\n');
                self.gen_message(f)?;
            }
        }
        self.line(1, "}");
        self.line(0, "}");
        self.line(0, SHELL_END);
        Ok(())
    }

    /// One ink! message per CuNi function: call the logic core and return
    /// the result. Uniform by design: the shell does not guess which
    /// functions are "validators" — it computes and returns. Mapping
    /// results to contract error types is a deployment-time decision
    /// (see docs/ONCHAIN.md).
    fn gen_message(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        let mut arg_names = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, ink_type(&p.ty)?));
            arg_names.push(p.name.clone());
        }
        let ret = ink_type(&f.ret_type)?;
        self.line(
            2,
            &format!(
                "/// {}: pure logic lives in `logic::{}`; this message",
                f.name, f.name
            ),
        );
        self.line(2, "/// computes it on-chain and returns the result.");
        self.line(2, "#[ink(message)]");
        self.line(
            2,
            &format!(
                "pub fn {}(&self{}) -> {} {{",
                f.name,
                if params.is_empty() {
                    String::new()
                } else {
                    format!(", {}", params.join(", "))
                },
                ret
            ),
        );
        self.line(
            3,
            &format!("logic::{}({})", f.name, arg_names.join(", ")),
        );
        self.line(2, "}");
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, ink_type(&p.ty)?));
        }
        let ret = ink_type(&f.ret_type)?;
        let saved = self.ret_kind;
        self.ret_kind = kind_of_type(&f.ret_type);
        self.line(
            0,
            &format!("pub fn {}({}) -> {} {{", f.name, params.join(", "), ret),
        );
        let mut scope: HashMap<String, InkKind> = HashMap::new();
        for p in &f.params {
            scope.insert(p.name.clone(), kind_of_type(&p.ty));
        }
        for s in &f.body {
            self.gen_stmt(1, s, &mut scope)?;
        }
        self.line(0, "}");
        self.ret_kind = saved;
        Ok(())
    }

    fn gen_stmt(
        &mut self,
        indent: usize,
        stmt: &Stmt,
        scope: &mut HashMap<String, InkKind>,
    ) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                self.gen_binding(indent, name, ty, value, false, scope)
            }
            StmtKind::Mut { name, ty, value } => {
                self.gen_binding(indent, name, ty, value, true, scope)
            }
            StmtKind::Assign { target, value } => {
                let t = self.gen_expr(target, scope)?;
                let v = self.gen_expr(value, scope)?;
                self.line(indent, &format!("{} = {};", t, v));
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let mut text = self.gen_expr(e, scope)?;
                // A `str` literal needs `.to_string()` against a `String`
                // return type (same rule as `let` bindings).
                if self.ret_kind == InkKind::Str && matches!(e.kind, ExprKind::Str(_)) {
                    text = format!("{}.to_string()", text);
                }
                self.line(indent, &format!("return {};", text));
                Ok(())
            }
            StmtKind::Ret(None) => {
                self.line(indent, "return;");
                Ok(())
            }
            StmtKind::Fail(e) => match &e.kind {
                ExprKind::Str(s) => {
                    self.line(indent, &format!("panic!(\"{}\");", Self::esc(s)));
                    Ok(())
                }
                _ => Err(
                    "fail with a non-string has no clean ink!-logic abort; refusing".into(),
                ),
            },
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("if {} {{", cond_s));
                self.gen_block(indent + 1, then_body, scope)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "} else {");
                    self.gen_block(indent + 1, else_body, scope)?;
                }
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                if b.is_some() {
                    return Err("two-binding for has no ink!-logic form; refusing".into());
                }
                // Only range() iteration is supported.
                let (start_s, end_e) = match &iter.kind {
                    ExprKind::Call { callee, args } => {
                        let is_range = matches!(&callee.kind, ExprKind::Ident(n) if n == "range");
                        if !is_range {
                            return Err(
                                "for over non-range iterables has no ink!-logic form; refusing"
                                    .into(),
                            );
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => {
                                return Err(
                                    "range() with step has no ink!-logic form; refusing".into()
                                )
                            }
                        }
                    }
                    _ => {
                        return Err(
                            "for over non-range iterables has no ink!-logic form; refusing"
                                .into(),
                        )
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr(se, scope)?,
                    None => "0".to_string(),
                };
                let e = self.gen_expr(end_e, scope)?;
                scope.insert(a.clone(), InkKind::Int);
                self.line(indent, &format!("for {} in {}..{} {{", a, s, e));
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("while {} {{", cond_s));
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::ExprStmt(e) => self.gen_expr_stmt(indent, e, scope),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become an ink! contract; refusing".into())
            }
        }
    }

    fn gen_binding(
        &mut self,
        indent: usize,
        name: &str,
        ty: &Option<Type>,
        value: &Expr,
        is_mut: bool,
        scope: &mut HashMap<String, InkKind>,
    ) -> Result<(), String> {
        if matches!(value.kind, ExprKind::Unwrap { .. }) {
            return Err("`??` has no ink!-logic equivalent for internal calls; refusing".into());
        }
        let kind = ty.as_ref().map(kind_of_type).unwrap_or(InkKind::Other);
        let kind = if kind == InkKind::Other {
            self.expr_kind(value, scope)
        } else {
            kind
        };
        let decl_ty = match ty {
            Some(t) => ink_type(t)?,
            None => match kind {
                InkKind::Int => "i64".into(),
                InkKind::Dec => "i128".into(),
                InkKind::Str => "String".into(),
                InkKind::Bool => "bool".into(),
                InkKind::Other => {
                    return Err(format!(
                        "cannot infer an ink!-logic type for `{}`; annotate it",
                        name
                    ))
                }
            },
        };
        scope.insert(name.to_string(), kind);
        let v = self.gen_expr(value, scope)?;
        // A string literal needs `.to_string()` to become a String binding;
        // values already of kind Str (format!, concat) are Strings already.
        let v = if decl_ty == "String" && matches!(value.kind, ExprKind::Str(_)) {
            format!("{}.to_string()", v)
        } else {
            v
        };
        let m = if is_mut { "mut " } else { "" };
        self.line(indent, &format!("let {}{}: {} = {};", m, name, decl_ty, v));
        Ok(())
    }

    fn gen_block(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, InkKind>,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt(indent, s, scope)?;
        }
        Ok(())
    }

    /// Expression statements: `say(...)` prints; anything else is emitted as
    /// a bare call so internal pure calls still run.
    fn gen_expr_stmt(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &mut HashMap<String, InkKind>,
    ) -> Result<(), String> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                if vals.len() != 1 {
                    return Err("say takes exactly one argument".into());
                }
                let v = vals[0];
                let kind = self.expr_kind(v, scope);
                let text = self.gen_expr(v, scope)?;
                match kind {
                    InkKind::Int | InkKind::Str => {
                        self.line(indent, &format!("println!(\"{{}}\", {});", text));
                        return Ok(());
                    }
                    InkKind::Dec => {
                        self.line(
                            indent,
                            &format!("println!(\"{{}}\", cuni_dec_str({}));", text),
                        );
                        return Ok(());
                    }
                    InkKind::Bool => {
                        self.line(
                            indent,
                            &format!(
                                "println!(\"{{}}\", if {} {{ \"True\" }} else {{ \"False\" }});",
                                text
                            ),
                        );
                        return Ok(());
                    }
                    InkKind::Other => {
                        return Err("say of this value has no ink!-logic form; refusing".into())
                    }
                }
            }
        }
        let text = self.gen_expr(e, scope)?;
        self.line(indent, &format!("{};", text));
        Ok(())
    }

    fn gen_expr(&mut self, e: &Expr, scope: &HashMap<String, InkKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(format!("{}i64", n)),
            ExprKind::Float(_) => Err("float literals have no ink!-logic form; refusing".into()),
            // `dec` literals (docs/DECIMAL.md): already scaled i128 in the AST.
            ExprKind::Dec(s) => Ok(format!("{}i128", s)),
            // `time` literals (docs/TIME.md): v1 has no time form in the
            // logic core — refuse rather than fake exactness.
            ExprKind::Time(_) => Err("time literals have no ink!-logic form (v1); refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(parts) => {
                let mut fmt = String::new();
                let mut args = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => fmt.push_str(&Self::esc_fmt(t)),
                        StrPartExpr::Expr(ie) => {
                            let k = self.expr_kind(ie, scope);
                            let t = self.gen_expr(ie, scope)?;
                            match k {
                                InkKind::Int | InkKind::Bool | InkKind::Str => {
                                    fmt.push_str("{}");
                                    args.push(t);
                                }
                                InkKind::Dec => {
                                    fmt.push_str("{}");
                                    args.push(format!("cuni_dec_str({})", t));
                                }
                                InkKind::Other => {
                                    return Err(
                                        "interpolating this value has no ink!-logic form; refusing"
                                            .into(),
                                    )
                                }
                            }
                        }
                    }
                }
                Ok(format!("format!(\"{}\"{})", fmt, if args.is_empty() {
                    String::new()
                } else {
                    format!(", {}", args.join(", "))
                }))
            }
            ExprKind::NoneLit => Err("None has no ink!-logic form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 ink!-logic form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 ink!-logic form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 ink!-logic form (structs/enums are refused); refusing"
                    .into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs, scope)?;
                let r = self.gen_expr(rhs, scope)?;
                let lk = self.expr_kind(lhs, scope);
                let rk = self.expr_kind(rhs, scope);
                // String + is concatenation.
                if matches!(*op, BinOp::Add) && lk == InkKind::Str && rk == InkKind::Str {
                    return Ok(format!("format!(\"{{}}{{}}\", {}, {})", l, r));
                }
                // dec/dec goes through the loud checked helpers.
                if lk == InkKind::Dec && rk == InkKind::Dec {
                    let h = match op {
                        BinOp::Add => "cuni_dec_add",
                        BinOp::Sub => "cuni_dec_sub",
                        BinOp::Mul => "cuni_dec_mul",
                        BinOp::Div => "cuni_dec_div",
                        BinOp::Mod => {
                            return Err("`%` is not defined on `dec` — refusing".into())
                        }
                        BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Gt
                        | BinOp::Le
                        | BinOp::Ge => {
                            let o = match op {
                                BinOp::Eq => "==",
                                BinOp::Ne => "!=",
                                BinOp::Lt => "<",
                                BinOp::Gt => ">",
                                BinOp::Le => "<=",
                                _ => ">=",
                            };
                            return Ok(format!("({} {} {})", l, o, r));
                        }
                        BinOp::And | BinOp::Or => {
                            return Err("`&&`/`||` on `dec` has no ink!-logic form; refusing".into())
                        }
                    };
                    return Ok(format!("{}({}, {})", h, l, r));
                }
                if lk != rk {
                    return Err(format!(
                        "cannot mix `{}` and `{}` with `{}` — convert explicitly: `dec_of_int(n)` / `int_of_dec(d)`",
                        kind_name(lk),
                        kind_name(rk),
                        op_name(op),
                    ));
                }
                if matches!(lk, InkKind::Other) {
                    return Err(format!(
                        "`{}` on this value has no ink!-logic form; refusing",
                        op_name(op)
                    ));
                }
                if matches!(*op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod)
                    && lk != InkKind::Int
                {
                    return Err(format!(
                        "`{}` is not defined on `{}`; refusing",
                        op_name(op),
                        kind_name(lk)
                    ));
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
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                Ok(format!("({} {} {})", l, o, r))
            }
            ExprKind::Unary { op, expr } => {
                let k = self.expr_kind(expr, scope);
                let t = self.gen_expr(expr, scope)?;
                match op {
                    UnOp::Not => {
                        if k != InkKind::Bool {
                            return Err("`not` on a non-bool has no ink!-logic form; refusing".into());
                        }
                        Ok(format!("(!{})", t))
                    }
                    UnOp::Neg => match k {
                        InkKind::Int => Ok(format!("(-{})", t)),
                        InkKind::Dec => Ok(format!("cuni_dec_neg({})", t)),
                        _ => Err("negation of this value has no ink!-logic form; refusing".into()),
                    },
                }
            }
            ExprKind::Unwrap { .. } => {
                Err("`??` has no ink!-logic equivalent for internal calls; refusing".into())
            }
        }
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, InkKind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have an ink!-logic form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no ink!-logic form; refusing".into());
        }
        // Builtins.
        match name.as_str() {
            "say" => return Err("say is a statement, not an expression; refusing".into()),
            "range" => return Err("range() outside for has no ink!-logic form; refusing".into()),
            "len" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("len takes one argument".into());
                }
                let k = self.expr_kind(args[0].expr(), scope);
                return match k {
                    InkKind::Str => Ok(format!("({}.len() as i64)", vals[0])),
                    _ => Err("len() of this value has no ink!-logic form; refusing".into()),
                };
            }
            "dec_of_int" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("dec_of_int takes one argument".into());
                }
                if self.expr_kind(args[0].expr(), scope) != InkKind::Int {
                    return Err("dec_of_int takes an `int`; refusing".into());
                }
                return Ok(format!("cuni_dec_of_int({})", vals[0]));
            }
            "int_of_dec" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("int_of_dec takes one argument".into());
                }
                if self.expr_kind(args[0].expr(), scope) != InkKind::Dec {
                    return Err("int_of_dec takes a `dec`; refusing".into());
                }
                return Ok(format!("cuni_int_of_dec({})", vals[0]));
            }
            "abs" | "min" | "max" => {
                return Err(format!(
                    "builtin `{}` needs a v1 helper; refusing for now",
                    name
                ))
            }
            _ => {}
        }
        if !self.fn_names.contains(&name) {
            return Err(format!("unknown call `{}`; refusing", name));
        }
        let vals: Vec<String> = args
            .iter()
            .map(|a| self.gen_expr(a.expr(), scope))
            .collect::<Result<_, _>>()?;
        Ok(format!("{}({})", name, vals.join(", ")))
    }

    /// Best-effort kind of an expression for `say` routing and inference.
    fn expr_kind(&self, e: &Expr, scope: &HashMap<String, InkKind>) -> InkKind {
        match &e.kind {
            ExprKind::Int(_) => InkKind::Int,
            ExprKind::Dec(_) => InkKind::Dec,
            ExprKind::Str(_) | ExprKind::InterpStr(_) => InkKind::Str,
            ExprKind::Bool(_) => InkKind::Bool,
            ExprKind::Ident(n) => scope.get(n).copied().unwrap_or(InkKind::Other),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "len" => InkKind::Int,
                ExprKind::Ident(n) if n == "dec_of_int" => InkKind::Dec,
                ExprKind::Ident(n) if n == "int_of_dec" => InkKind::Int,
                ExprKind::Ident(n) => self.fn_ret.get(n).copied().unwrap_or(InkKind::Other),
                _ => InkKind::Other,
            },
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => InkKind::Bool,
                BinOp::Add if self.expr_kind(lhs, scope) == InkKind::Str => InkKind::Str,
                _ => {
                    let lk = self.expr_kind(lhs, scope);
                    let rk = self.expr_kind(rhs, scope);
                    if lk == InkKind::Dec || rk == InkKind::Dec {
                        InkKind::Dec
                    } else {
                        InkKind::Int
                    }
                }
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => InkKind::Bool,
                UnOp::Neg => self.expr_kind(expr, scope),
            },
            _ => InkKind::Other,
        }
    }
}

fn kind_name(k: InkKind) -> &'static str {
    match k {
        InkKind::Int => "int",
        InkKind::Dec => "dec",
        InkKind::Str => "str",
        InkKind::Bool => "bool",
        InkKind::Other => "other",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn parse_src(src: &str) -> Program {
        let toks = lexer::Lexer::tokenize(src).expect("lex");
        let mut p = parser::Parser::new(toks, src);
        p.parse_program().expect("parse")
    }

    const FEE: &str = r#"
def fee(amount: dec) -> dec do
    if amount <= 100.00dec do
        ret amount * 0.01dec + 0.25dec
    end
    if amount <= 1000.00dec do
        ret amount * 0.005dec + 0.25dec
    end
    ret amount * 0.0025dec + 0.25dec
end

say(fee(99.9999dec))
say(fee(1000000.00dec))
"#;

    #[test]
    fn logic_core_has_dec_helpers_and_main() {
        let core = logic_core(&parse_src(FEE)).expect("logic core");
        assert!(core.contains("pub fn fee(amount: i128) -> i128"));
        assert!(core.contains("fn main()"));
        assert!(core.contains("fn cuni_dec_mul(a: i128, b: i128) -> i128"));
        assert!(core.contains("fn cuni_dec_str(v: i128) -> String"));
        assert!(core.contains("cuni_dec_mul(amount, 100i128)"));
        assert!(core.contains("println!(\"{}\", cuni_dec_str("));
    }

    #[test]
    fn program_embeds_logic_core_verbatim() {
        let prog = parse_src(FEE);
        let core = logic_core(&prog).expect("logic core");
        let program_src = generate_program(&prog, "cuni_fee").expect("program");
        // Extract the delimited logic block, drop the one shell-owned
        // no_std adaptation import, dedent one level, compare bytes.
        let start = program_src.find(LOGIC_START).expect("logic start marker");
        let end = program_src.find(LOGIC_END).expect("logic end marker");
        assert!(start < end);
        let block = &program_src[start..end];
        let mut lines = block.lines().skip(1).peekable();
        let open = lines.next().expect("empty logic region");
        assert_eq!(open.trim(), "mod logic {", "logic region must open with `mod logic {{`");
        let mut extracted = String::new();
        for line in lines {
            if line == "}" {
                break;
            }
            if line.trim() == "use ink::prelude::string::String;" {
                continue;
            }
            let dedented = line.strip_prefix("    ").unwrap_or(line);
            extracted.push_str(dedented);
            extracted.push('\n');
        }
        let extracted = extracted.trim_end_matches('\n').to_string() + "\n";
        let core_norm = core.trim_end_matches('\n').to_string() + "\n";
        assert_eq!(
            extracted, core_norm,
            "program's mod logic must be byte-identical to the standalone logic core"
        );
    }

    #[test]
    fn program_shell_has_ink_shape() {
        let program_src = generate_program(&parse_src(FEE), "cuni_fee").expect("program");
        for marker in [
            SHELL_START,
            SHELL_END,
            "#[ink::contract]",
            "mod cuni_fee {",
            "#[ink(storage)]",
            "pub struct Contract {",
            "owner: AccountId,",
            "#[ink(constructor)]",
            "pub fn new() -> Self {",
            "#[ink(message)]",
            "pub fn fee(&self, amount: i128) -> i128 {",
            "logic::fee(amount)",
        ] {
            assert!(
                program_src.contains(marker),
                "program shell missing `{}`",
                marker
            );
        }
    }

    #[test]
    fn refuses_float_list_typ_unwrap_time() {
        for (tag, src) in [
            ("float", "def f() -> int do\n ret 1\nend\nsay(1.5)\n"),
            ("list", "def f() -> int do\n ret 1\nend\nlet xs = [1, 2]\nsay(1)\n"),
            (
                "typ",
                "typ Point do\n x: int\nend\ndef f() -> int do\n ret 1\nend\nsay(1)\n",
            ),
            (
                "unwrap",
                "def f() -> int do\n ret 1\nend\nlet x = f() ?? do\n ret 0\nend\nsay(x)\n",
            ),
            (
                "dec-mod",
                "def f(a: dec) -> dec do\n ret a % 2.0dec\nend\nsay(f(1.0dec))\n",
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
    fn string_bool_int_forms() {
        let src = r#"
def shout(s: str, b: bool) -> str do
    if b do
        ret "hi " + s
    end
    ret s
end

say(shout("bob", true))
say(len("hello"))
say(7 / 2)
say(not false)
let greeting: str = "hi"
say(greeting)
"#;
        let core = logic_core(&parse_src(src)).expect("logic core");
        assert!(core.contains("pub fn shout(s: String, b: bool) -> String"));
        assert!(core.contains(".to_string()"));
        assert!(core.contains("as i64"), "len() should cast usize to i64");
        assert!(core.contains("\"True\""), "bool must print canonically");
    }
}

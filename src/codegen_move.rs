//! Move module backend — Aptos/Sui Move writer.
//!
//! "Trust Provable, in all things."
//!
//! Emits a genuine Move module shape from CuNi source: a pure logic core
//! (the part CuNi proves) plus the module shell (`module 0xCUNI::name`,
//! `public entry fun` wrappers) that wraps it.
//!
//! Three renderings, one law:
//! - `move_logic_core` — the pure logic in Move: `fun` definitions plus a
//!   `public entry fun run()` driver. This is the chain's language, as
//!   embedded in the module.
//! - `generate_reference` — the standalone runnable reference of the same
//!   logic in Python (run with `python3`); its stdout is what the CuNi gate
//!   proves byte-identical to CuNi gold. No Move toolchain is needed.
//! - `generate_program` — the full Move module: the logic core embedded
//!   verbatim (clearly delimited), plus the module shell (also clearly
//!   delimited) that requires the Move toolchain (`aptos`/`sui` CLI).
//!
//! Exactness notes (dictated by the 0.8.0 Onchain Division spec):
//! - Move has NO signed integers: CuNi `int` is u64 here, and negative
//!   literals (and unary negation) are honestly refused at emit.
//! - CuNi `dec` is a scaled u128 (scale 10^4); negative decimals are
//!   refused at emit. `*` is `trunc(a*b/10000)`, `/` is `trunc(a*10000/b)`
//!   toward zero (unsigned `/` already truncates). Move aborts loudly on
//!   overflow, underflow, and division by zero — that abort IS this seat's
//!   loud refusal, matching the interpreter's checked-arithmetic posture.
//!   The Python reference enforces the same envelope explicitly (`_cuni_u64`
//!   / `_cuni_u128` raise instead of wrapping), so a loud abort on one
//!   side can never become a silent wrap on the other.
//! - Canonical `dec` rendering for `say` follows docs/DECIMAL.md §6
//!   (`cuni_dec_str` in Move, `_cuni_dec_str` in Python — same law).
//! - `float`, `list`, `map`, `opt`, `??`, structs, and enums are honestly
//!   refused: a Move entry function's verifiable core is integer/decimal
//!   math, and the backend will not guess at mappings it cannot prove.
//! - `bool` prints canonically as `True`/`False`.
//! - `fail` becomes `abort` in Move (abort code 1) and `RuntimeError` in
//!   the Python reference.
//! - `for x in range(a, b)` lowers to a `while` loop in Move (Move's `for`
//!   iterates vectors, not integer ranges); the Python reference uses
//!   `range` directly. Same iteration sequence, stated in the code.
//!
//! Honest boundaries: the logic core is gate-proven via `generate_reference`
//! (a Python rendering run with `python3` — no Move toolchain on the check
//! machine); the Move text itself and the module shell are NOT compiled here
//! and nothing has executed on-chain. See `docs/ONCHAIN.md` for the full
//! verification matrix.

use crate::ast::{
    BinOp, CallArg, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr, Type, UnOp,
};
use std::collections::{HashMap, HashSet};

/// Move-logic kind of a CuNi value, for `say` routing and type inference.
/// Shared by the Move and Python renderings (same kind lattice, same law).
#[derive(Clone, Copy, PartialEq, Eq)]
enum MoveKind {
    Int,
    Dec,
    Str,
    Bool,
    Other,
}

/// Delimiters marking the two regions of a `--emit-move` artifact.
pub const LOGIC_START: &str = "// CUNI-LOGIC-CORE-START";
pub const LOGIC_END: &str = "// CUNI-LOGIC-CORE-END";
pub const SHELL_START: &str = "// CUNI-MOVE-SHELL-START";
pub const SHELL_END: &str = "// CUNI-MOVE-SHELL-END";

fn kind_of_type(ty: &Type) -> MoveKind {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => MoveKind::Int,
            "dec" => MoveKind::Dec,
            "str" => MoveKind::Str,
            "bool" => MoveKind::Bool,
            _ => MoveKind::Other,
        },
        _ => MoveKind::Other,
    }
}

fn kind_name(k: MoveKind) -> &'static str {
    match k {
        MoveKind::Int => "int",
        MoveKind::Dec => "dec",
        MoveKind::Str => "str",
        MoveKind::Bool => "bool",
        MoveKind::Other => "other",
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

/// The Move seat's negativity gate: Move has no signed integers, so a
/// negative `int` literal has no Move form. Checked in both renderings.
fn check_nonneg_int(n: i64) -> Result<u64, String> {
    if n < 0 {
        return Err(
            "negative int literal has no Move form (Move has no signed integers); refusing"
                .into(),
        );
    }
    Ok(n as u64)
}

/// Same gate for scaled `dec` literals.
fn check_nonneg_dec(s: i128) -> Result<u128, String> {
    if s < 0 {
        return Err(
            "negative dec literal has no Move form (scaled u128 cannot hold it); refusing".into(),
        );
    }
    Ok(s as u128)
}

// ---------------------------------------------------------------------------
// Move rendering
// ---------------------------------------------------------------------------

pub struct MoveGen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, MoveKind>,
    out: String,
}

impl MoveGen {
    fn new(program: &Program) -> Self {
        let mut fn_names = HashSet::new();
        let mut fn_ret = HashMap::new();
        for item in &program.items {
            if let Item::Def(f) = item {
                fn_names.insert(f.name.clone());
                fn_ret.insert(f.name.clone(), kind_of_type(&f.ret_type));
            }
        }
        MoveGen {
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

    /// Escape for Move `b"..."` byte-string literals.
    fn esc(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                '\r' => r.push_str("\\r"),
                c if (c < '\u{20}' || c == '\u{7f}') => {
                    r.push_str(&format!("\\x{:02x}", c as u32))
                }
                c => r.push(c),
            }
        }
        r
    }
}

/// Map a CuNi type to its Move type.
fn move_type(ty: &Type) -> Result<String, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok("u64".into()),
            "dec" => Ok("u128".into()),
            "str" => Ok("String".into()),
            "bool" => Ok("bool".into()),
            "float" => Err("Move logic core has no float type; refusing float".into()),
            other => Err(format!(
                "type `{}` has no Move-logic mapping; refusing",
                other
            )),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{}` has no Move-logic mapping; refusing",
            name
        )),
    }
}

/// The pure logic core in Move: helpers + `fun` definitions + the `run`
/// entry driver. Embedded verbatim by `generate_program`.
pub fn move_logic_core(program: &Program) -> Result<String, String> {
    let mut g = MoveGen::new(program);
    g.gen_logic_core(program)?;
    Ok(g.out)
}

/// Standalone runnable reference of the pure logic core: Python, run with
/// `python3`; `main` driver prints the `say` outputs. This is what the gate
/// proves byte-identical to CuNi gold.
pub fn generate_reference(program: &Program) -> Result<String, String> {
    let mut g = PyGen::new(program);
    g.gen_reference(program)?;
    Ok(g.out)
}

/// The full Move module: delimited logic core + module shell.
///
/// `mod_name` is the module name (already sanitized, e.g.
/// `cuni_fee_schedule`); the module lives at the clearly-marked
/// placeholder address `0xCUNI`.
/// Append one indented line to a Move module being built.
fn mline(out: &mut String, indent: usize, text: &str) {
    out.push_str(&"    ".repeat(indent));
    out.push_str(text);
    out.push('\n');
}

pub fn generate_program(program: &Program, mod_name: &str) -> Result<String, String> {
    let mut out = String::new();
    mline(&mut out, 0, "/// Generated by the CuNi Move backend.");
    mline(&mut out, 0, "/// \"Trust Provable, in all things.\"");
    mline(&mut out, 0, "///");
    mline(&mut out, 
        0,
        "/// STRUCTURE — two delimited regions, one law (same logic or refuse):",
    );
    mline(&mut out, 
        0,
        "/// - Logic core (CUNI-LOGIC-CORE markers): the pure program logic as",
    );
    mline(&mut out, 
        0,
        "///   Move `fun` definitions, plus the `run` entry driver. The CuNi gate",
    );
    mline(&mut out, 
        0,
        "///   proves the Python reference byte-identical to CuNi gold; this",
    );
    mline(&mut out, 0, "///   Move text is the same law in the chain's language.");
    mline(&mut out, 
        0,
        "/// - Module shell (CUNI-MOVE-SHELL markers): `public entry fun`",
    );
    mline(&mut out, 
        0,
        "///   wrappers, one per CuNi function. Requires the Move toolchain",
    );
    mline(&mut out, 
        0,
        "///   (`aptos`/`sui` CLI); NOT compiled by `cuni check` (no Move",
    );
    mline(&mut out, 0, "///   toolchain on the check machine).");
    mline(&mut out, 0, "///   Full verification recipe: docs/ONCHAIN.md.");
    mline(&mut out, 0, "///");
    mline(&mut out, 
        0,
        "/// HONEST BOUNDARIES: reference gate-proven; Move text and shell not",
    );
    mline(&mut out, 0, "/// compiled here; nothing here has executed on-chain.");
    mline(&mut out, 0, "///");
    mline(&mut out, 
        0,
        "/// UNSIGNED SEAT: CuNi `int` is u64, `dec` is scaled u128 (10^4).",
    );
    mline(&mut out, 
        0,
        "/// Negative literals are refused at emit; Move aborts loudly on",
    );
    mline(&mut out, 
        0,
        "/// overflow/underflow/division-by-zero — the loud refusal, never a wrap.",
    );
    out.push('\n');

    // TODO: replace 0xCUNI with your real publish address before publishing.
    mline(&mut out, 
        0,
        "// TODO: replace 0xCUNI with your real publish address before publishing.",
    );
    mline(&mut out, 0, &format!("module 0xCUNI::{} {{", mod_name));
    mline(&mut out, 1, "use std::debug;");
    mline(&mut out, 1, "use std::string::{Self, String};");
    mline(&mut out, 1, "use std::string_utils;");
    mline(&mut out, 1, "use std::vector;");
    out.push('\n');

    // Region 1: the logic core, embedded verbatim (indented one level).
    mline(&mut out, 0, LOGIC_START);
    let core = move_logic_core(program)?;
    for l in core.lines() {
        if l.trim().is_empty() {
            out.push('\n');
        } else {
            mline(&mut out, 1, l);
        }
    }
    mline(&mut out, 0, LOGIC_END);
    out.push('\n');

    // Region 2: the module shell — one entry wrapper per CuNi function.
    // Pure-logic modules need no on-chain state struct; the wrappers are
    // the invocation surface.
    mline(&mut out, 0, SHELL_START);
    mline(&mut out, 
        0,
        "// Entry wrappers: one `public entry fun` per CuNi function. The pure",
    );
    mline(&mut out, 
        0,
        "// logic above stays private; these wrappers are the on-chain",
    );
    mline(&mut out, 
        0,
        "// invocation surface. The `_entry` suffix keeps wrapper names from",
    );
    mline(&mut out, 
        0,
        "// colliding with the logic `fun`s. Results print in canonical form;",
    );
    mline(&mut out, 
        0,
        "// production deployments should emit an event instead of debug-printing.",
    );
    let defs: Vec<&FnDecl> = program
        .items
        .iter()
        .filter_map(|i| match i {
            Item::Def(f) => Some(f),
            _ => None,
        })
        .collect();
    if defs.is_empty() {
        out.push('\n');
        mline(&mut out, 0, "// (no CuNi functions: nothing to expose as entry functions)");
    } else {
        for f in &defs {
            out.push('\n');
            gen_entry_wrapper(&mut out, f)?;
        }
    }
    mline(&mut out, 0, SHELL_END);
    mline(&mut out, 0, "}");
    Ok(out)
}

/// One `public entry fun` per CuNi function: call the logic `fun`,
/// debug-print the canonical result.
fn gen_entry_wrapper(out: &mut String, f: &FnDecl) -> Result<(), String> {
    let mut line = |indent: usize, text: &str| {
        out.push_str(&"    ".repeat(indent));
        out.push_str(text);
        out.push('\n');
    };
    let mut params = Vec::new();
    let mut arg_names = Vec::new();
    for p in &f.params {
        params.push(format!("{}: {}", p.name, move_type(&p.ty)?));
        arg_names.push(p.name.clone());
    }
    let ret_kind = kind_of_type(&f.ret_type);
    let print_expr = match ret_kind {
        MoveKind::Int => "std::string_utils::to_string(&result)".to_string(),
        MoveKind::Dec => "cuni_dec_str(result)".to_string(),
        MoveKind::Bool => {
            "(if (result) { std::string::utf8(b\"True\") } else { std::string::utf8(b\"False\") })"
                .to_string()
        }
        MoveKind::Str => "result".to_string(),
        MoveKind::Other => {
            return Err(format!(
                "entry wrapper for `{}`: return type has no Move form; refusing",
                f.name
            ))
        }
    };
    line(
        0,
        &format!(
            "/// {}: pure logic lives in `{}`; this entry computes it on-chain",
            f.name, f.name
        ),
    );
    line(0, "/// and prints the canonical result.");
    line(
        0,
        &format!(
            "public entry fun {}_entry({}) {{",
            f.name,
            params.join(", ")
        ),
    );
    line(
        1,
        &format!("let result = {}({});", f.name, arg_names.join(", ")),
    );
    line(1, &format!("std::debug::print(&{});", print_expr));
    line(0, "}");
    Ok(())
}

impl MoveGen {
    fn gen_logic_core(&mut self, program: &Program) -> Result<(), String> {
        for item in &program.items {
            match item {
                Item::Typ(t) => {
                    return Err(format!(
                        "typ `{}` has no v1 Move-logic form; refusing (integer/decimal/fn core only)",
                        t.name
                    ))
                }
                Item::Enum(e) => {
                    return Err(format!(
                        "enum `{}` has no v1 Move-logic form; refusing (integer/decimal/fn core only)",
                        e.name
                    ))
                }
                _ => {}
            }
        }
        self.gen_helpers();
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
            self.line(
                0,
                "/// Driver: runs the top-level statements in order (the off-chain",
            );
            self.line(0, "/// `say` sequence). Invoke it as an entry function to replay.");
            self.line(0, "public entry fun run() {");
            let mut scope = HashMap::new();
            for s in top {
                self.gen_stmt(1, s, &mut scope)?;
            }
            self.line(0, "}");
        }
        Ok(())
    }

    /// Move `dec` helpers: canonical renderer (§6) and the explicit
    /// conversions. Arithmetic itself is inline — Move's native abort on
    /// overflow/underflow/division-by-zero IS the loud refusal.
    fn gen_helpers(&mut self) {
        self.line(
            0,
            "/// One decimal digit (0-9) as its ASCII byte, for `cuni_dec_str`.",
        );
        self.line(0, "fun cuni_digit(d: u128): u8 {");
        self.line(1, "(48 + (d as u8))");
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// Canonical `dec` rendering (docs/DECIMAL.md §6): scaled u128 ->",
        );
        self.line(
            0,
            "/// \"D.F\" with trailing fractional zeros stripped (never empty).",
        );
        self.line(0, "fun cuni_dec_str(v: u128): String {");
        self.line(1, "let scale: u128 = 10000;");
        self.line(1, "let ip = v / scale;");
        self.line(1, "let r = v % scale;");
        self.line(1, "let frac = std::vector::empty<u8>();");
        self.line(1, "std::vector::push_back(&mut frac, cuni_digit(r / 1000));");
        self.line(
            1,
            "std::vector::push_back(&mut frac, cuni_digit((r / 100) % 10));",
        );
        self.line(
            1,
            "std::vector::push_back(&mut frac, cuni_digit((r / 10) % 10));",
        );
        self.line(1, "std::vector::push_back(&mut frac, cuni_digit(r % 10));");
        self.line(1, "while (std::vector::length(&frac) > 1) {");
        self.line(2, "let last = std::vector::length(&frac) - 1;");
        self.line(2, "if (*std::vector::borrow(&frac, last) != 48) {");
        self.line(3, "break;");
        self.line(2, "};");
        self.line(2, "std::vector::pop_back(&mut frac);");
        self.line(1, "};");
        self.line(1, "let s = std::string_utils::to_string(&ip);");
        self.line(
            1,
            "std::string::append(&mut s, std::string::utf8(b\".\"));",
        );
        self.line(
            1,
            "std::string::append(&mut s, std::string::utf8(frac));",
        );
        self.line(1, "s");
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// `dec_of_int`: exact `n * 10000`; `*` aborts loudly on overflow.",
        );
        self.line(0, "fun cuni_dec_of_int(n: u64): u128 {");
        self.line(1, "(n as u128) * 10000u128");
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "/// `int_of_dec`: `trunc(d/10000)` toward zero; loud abort past u64.",
        );
        self.line(0, "fun cuni_int_of_dec(d: u128): u64 {");
        self.line(1, "let q = d / 10000u128;");
        self.line(
            1,
            "assert!(q <= 18446744073709551615u128, 2); // cuni: int_of_dec overflow — refused",
        );
        self.line(1, "(q as u64)");
        self.line(0, "}");
        self.out.push('\n');
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, move_type(&p.ty)?));
        }
        let ret = move_type(&f.ret_type)?;
        self.line(0, &format!("fun {}({}): {} {{", f.name, params.join(", "), ret));
        let mut scope: HashMap<String, MoveKind> = HashMap::new();
        for p in &f.params {
            scope.insert(p.name.clone(), kind_of_type(&p.ty));
        }
        for s in &f.body {
            self.gen_stmt(1, s, &mut scope)?;
        }
        self.line(0, "}");
        Ok(())
    }

    fn gen_stmt(
        &mut self,
        indent: usize,
        stmt: &Stmt,
        scope: &mut HashMap<String, MoveKind>,
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
                let text = self.gen_expr(e, scope)?;
                self.line(indent, &format!("return {};", text));
                Ok(())
            }
            StmtKind::Ret(None) => {
                self.line(indent, "return;");
                Ok(())
            }
            StmtKind::Fail(e) => match &e.kind {
                ExprKind::Str(s) => {
                    self.line(indent, &format!("// cuni `fail`: \"{}\"", Self::esc(s)));
                    self.line(indent, "abort 1;");
                    Ok(())
                }
                _ => Err("fail with a non-string has no clean Move-logic abort; refusing".into()),
            },
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("if ({}) {{", cond_s));
                self.gen_block(indent + 1, then_body, scope)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "} else {");
                    self.gen_block(indent + 1, else_body, scope)?;
                }
                self.line(indent, "};");
                Ok(())
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                if b.is_some() {
                    return Err("two-binding for has no Move-logic form; refusing".into());
                }
                // Move's `for` iterates vectors, not integer ranges, so
                // `range(a, b)` lowers to an explicit `while` — same sequence.
                let (start_s, end_e) = match &iter.kind {
                    ExprKind::Call { callee, args } => {
                        let is_range = matches!(&callee.kind, ExprKind::Ident(n) if n == "range");
                        if !is_range {
                            return Err(
                                "for over non-range iterables has no Move-logic form; refusing"
                                    .into(),
                            );
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => {
                                return Err(
                                    "range() with step has no Move-logic form; refusing".into()
                                )
                            }
                        }
                    }
                    _ => {
                        return Err(
                            "for over non-range iterables has no Move-logic form; refusing"
                                .into(),
                        )
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr(se, scope)?,
                    None => "0u64".to_string(),
                };
                let e = self.gen_expr(end_e, scope)?;
                scope.insert(a.clone(), MoveKind::Int);
                self.line(indent, "// `for` over `range` lowers to `while` (Move has no int ranges).");
                self.line(indent, &format!("let mut {} = {};", a, s));
                self.line(indent, &format!("while ({} < {}) {{", a, e));
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent + 1, &format!("{} = {} + 1;", a, a));
                self.line(indent, "};");
                Ok(())
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("while ({}) {{", cond_s));
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "};");
                Ok(())
            }
            StmtKind::ExprStmt(e) => self.gen_expr_stmt(indent, e, scope),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become a Move module; refusing".into())
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
        scope: &mut HashMap<String, MoveKind>,
    ) -> Result<(), String> {
        if matches!(value.kind, ExprKind::Unwrap { .. }) {
            return Err("`??` has no Move-logic equivalent for internal calls; refusing".into());
        }
        let kind = ty.as_ref().map(kind_of_type).unwrap_or(MoveKind::Other);
        let kind = if kind == MoveKind::Other {
            self.expr_kind(value, scope)
        } else {
            kind
        };
        let decl_ty = match ty {
            Some(t) => move_type(t)?,
            None => match kind {
                MoveKind::Int => "u64".into(),
                MoveKind::Dec => "u128".into(),
                MoveKind::Str => "String".into(),
                MoveKind::Bool => "bool".into(),
                MoveKind::Other => {
                    return Err(format!(
                        "cannot infer a Move-logic type for `{}`; annotate it",
                        name
                    ))
                }
            },
        };
        scope.insert(name.to_string(), kind);
        // `gen_expr` on a `Str` already yields a `String`
        // (`std::string::utf8(...)`), so no wrapping is needed here —
        // unlike the Rust seats, where `"..."` is a `&str`.
        let v = self.gen_expr(value, scope)?;
        let m = if is_mut { "mut " } else { "" };
        self.line(indent, &format!("let {}{}: {} = {};", m, name, decl_ty, v));
        Ok(())
    }

    fn gen_block(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, MoveKind>,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt(indent, s, scope)?;
        }
        Ok(())
    }

    /// Expression statements: `say(...)` debug-prints the canonical form;
    /// anything else is emitted as a bare call so internal pure calls run.
    fn gen_expr_stmt(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &mut HashMap<String, MoveKind>,
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
                let rendered = match kind {
                    MoveKind::Int => format!("std::string_utils::to_string(&{})", text),
                    MoveKind::Dec => format!("cuni_dec_str({})", text),
                    MoveKind::Bool => format!(
                        "(if ({}) {{ std::string::utf8(b\"True\") }} else {{ std::string::utf8(b\"False\") }})",
                        text
                    ),
                    MoveKind::Str => text,
                    MoveKind::Other => {
                        return Err("say of this value has no Move-logic form; refusing".into())
                    }
                };
                self.line(indent, &format!("std::debug::print(&{});", rendered));
                return Ok(());
            }
        }
        let text = self.gen_expr(e, scope)?;
        self.line(indent, &format!("{};", text));
        Ok(())
    }

    fn gen_expr(&mut self, e: &Expr, scope: &HashMap<String, MoveKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(format!("{}u64", check_nonneg_int(*n)?)),
            ExprKind::Float(_) => Err("float literals have no Move-logic form; refusing".into()),
            // `dec` literals (docs/DECIMAL.md): already scaled in the AST.
            ExprKind::Dec(s) => Ok(format!("{}u128", check_nonneg_dec(*s)?)),
            ExprKind::Time(_) => Err("time literals have no Move-logic form (v1); refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("std::string::utf8(b\"{}\")", Self::esc(s))),
            ExprKind::InterpStr(parts) => self.gen_interp(parts, scope),
            ExprKind::NoneLit => Err("None has no Move-logic form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Move-logic form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 Move-logic form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Move-logic form (structs/enums are refused); refusing"
                    .into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs, scope)?;
                let r = self.gen_expr(rhs, scope)?;
                let lk = self.expr_kind(lhs, scope);
                let rk = self.expr_kind(rhs, scope);
                // String + is concatenation (block: fresh String, append).
                if matches!(*op, BinOp::Add) && lk == MoveKind::Str && rk == MoveKind::Str {
                    return Ok(format!(
                        "{{ let mut t = {}; std::string::append(&mut t, {}); t }}",
                        l, r
                    ));
                }
                // String == compares bytes.
                if matches!(*op, BinOp::Eq | BinOp::Ne)
                    && lk == MoveKind::Str
                    && rk == MoveKind::Str
                {
                    let eq = format!(
                        "(*std::string::bytes(&{}) == *std::string::bytes(&{}))",
                        l, r
                    );
                    return Ok(if matches!(*op, BinOp::Eq) {
                        eq
                    } else {
                        format!("(!{})", eq)
                    });
                }
                // dec/dec: inline ops; Move aborts loudly on overflow or
                // division by zero — the loud refusal, never a wrap.
                if lk == MoveKind::Dec && rk == MoveKind::Dec {
                    let body = match op {
                        BinOp::Add => format!("({} + {})", l, r),
                        BinOp::Sub => format!("({} - {})", l, r),
                        BinOp::Mul => format!("(({} * {}) / 10000u128)", l, r),
                        BinOp::Div => format!("(({} * 10000u128) / {})", l, r),
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
                            return Err("`&&`/`||` on `dec` has no Move-logic form; refusing".into())
                        }
                    };
                    return Ok(body);
                }
                if lk != rk {
                    return Err(format!(
                        "cannot mix `{}` and `{}` with `{}` — convert explicitly: `dec_of_int(n)` / `int_of_dec(d)`",
                        kind_name(lk),
                        kind_name(rk),
                        op_name(op),
                    ));
                }
                if matches!(lk, MoveKind::Other) {
                    return Err(format!(
                        "`{}` on this value has no Move-logic form; refusing",
                        op_name(op)
                    ));
                }
                // int/int: native ops abort loudly on overflow/underflow/div0.
                if matches!(lk, MoveKind::Int) {
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
                        _ => {
                            return Err(format!(
                                "`{}` is not defined on `int`; refusing",
                                op_name(op)
                            ))
                        }
                    };
                    return Ok(format!("({} {} {})", l, o, r));
                }
                // bool/bool: comparisons and logic only.
                if matches!(lk, MoveKind::Bool) {
                    let o = match op {
                        BinOp::Eq => "==",
                        BinOp::Ne => "!=",
                        BinOp::And => "&&",
                        BinOp::Or => "||",
                        _ => {
                            return Err(format!(
                                "`{}` is not defined on `bool`; refusing",
                                op_name(op)
                            ))
                        }
                    };
                    return Ok(format!("({} {} {})", l, o, r));
                }
                Err(format!(
                    "`{}` on `{}` has no Move-logic form; refusing",
                    op_name(op),
                    kind_name(lk)
                ))
            }
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => {
                    if self.expr_kind(expr, scope) != MoveKind::Bool {
                        return Err("`not` on a non-bool has no Move-logic form; refusing".into());
                    }
                    let t = self.gen_expr(expr, scope)?;
                    Ok(format!("(!{})", t))
                }
                // No signed integers on this seat: negation refuses at emit.
                UnOp::Neg => Err(
                    "unary negation has no Move form (Move has no signed integers); refusing"
                        .into(),
                ),
            },
            ExprKind::Unwrap { .. } => {
                Err("`??` has no Move-logic equivalent for internal calls; refusing".into())
            }
        }
    }

    /// Interpolated strings: build with `append` in a block; each part is
    /// bound once so `String` moves are exact.
    fn gen_interp(
        &mut self,
        parts: &[StrPartExpr],
        scope: &HashMap<String, MoveKind>,
    ) -> Result<String, String> {
        let mut out = String::from("{\n");
        out.push_str("let mut t = std::string::utf8(b\"\");\n");
        let mut n = 0usize;
        for p in parts {
            match p {
                StrPartExpr::Text(t) => {
                    out.push_str(&format!(
                        "std::string::append(&mut t, std::string::utf8(b\"{}\"));\n",
                        Self::esc(t)
                    ));
                }
                StrPartExpr::Expr(ie) => {
                    let k = self.expr_kind(ie, scope);
                    let e = self.gen_expr(ie, scope)?;
                    let s = match k {
                        MoveKind::Int => format!("std::string_utils::to_string(&p{})", n),
                        MoveKind::Dec => format!("cuni_dec_str(p{})", n),
                        MoveKind::Bool => format!(
                            "(if (p{}) {{ std::string::utf8(b\"True\") }} else {{ std::string::utf8(b\"False\") }})",
                            n
                        ),
                        MoveKind::Str => format!("p{}", n),
                        MoveKind::Other => {
                            return Err(
                                "interpolating this value has no Move-logic form; refusing".into()
                            )
                        }
                    };
                    out.push_str(&format!("let p{} = {};\n", n, e));
                    out.push_str(&format!("std::string::append(&mut t, {});\n", s));
                    n += 1;
                }
            }
        }
        out.push_str("t }");
        Ok(out)
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, MoveKind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a Move-logic form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no Move-logic form; refusing".into());
        }
        // Builtins.
        match name.as_str() {
            "say" => return Err("say is a statement, not an expression; refusing".into()),
            "range" => return Err("range() outside for has no Move-logic form; refusing".into()),
            "len" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("len takes one argument".into());
                }
                return match self.expr_kind(args[0].expr(), scope) {
                    MoveKind::Str => Ok(format!("std::string::length(&{})", vals[0])),
                    _ => Err("len() of this value has no Move-logic form; refusing".into()),
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
                if self.expr_kind(args[0].expr(), scope) != MoveKind::Int {
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
                if self.expr_kind(args[0].expr(), scope) != MoveKind::Dec {
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
    fn expr_kind(&self, e: &Expr, scope: &HashMap<String, MoveKind>) -> MoveKind {
        match &e.kind {
            ExprKind::Int(_) => MoveKind::Int,
            ExprKind::Dec(_) => MoveKind::Dec,
            ExprKind::Str(_) | ExprKind::InterpStr(_) => MoveKind::Str,
            ExprKind::Bool(_) => MoveKind::Bool,
            ExprKind::Ident(n) => scope.get(n).copied().unwrap_or(MoveKind::Other),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "len" => MoveKind::Int,
                ExprKind::Ident(n) if n == "dec_of_int" => MoveKind::Dec,
                ExprKind::Ident(n) if n == "int_of_dec" => MoveKind::Int,
                ExprKind::Ident(n) => self.fn_ret.get(n).copied().unwrap_or(MoveKind::Other),
                _ => MoveKind::Other,
            },
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => MoveKind::Bool,
                BinOp::Add if self.expr_kind(lhs, scope) == MoveKind::Str => MoveKind::Str,
                _ => {
                    let lk = self.expr_kind(lhs, scope);
                    let rk = self.expr_kind(rhs, scope);
                    if lk == MoveKind::Dec || rk == MoveKind::Dec {
                        MoveKind::Dec
                    } else {
                        MoveKind::Int
                    }
                }
            },
            ExprKind::Unary { op, .. } => match op {
                UnOp::Not => MoveKind::Bool,
                UnOp::Neg => MoveKind::Other,
            },
            _ => MoveKind::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// Python reference rendering
// ---------------------------------------------------------------------------

pub struct PyGen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, MoveKind>,
    out: String,
}

impl PyGen {
    fn new(program: &Program) -> Self {
        let mut fn_names = HashSet::new();
        let mut fn_ret = HashMap::new();
        for item in &program.items {
            if let Item::Def(f) = item {
                fn_names.insert(f.name.clone());
                fn_ret.insert(f.name.clone(), kind_of_type(&f.ret_type));
            }
        }
        PyGen {
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

    /// Escape for Python `"..."` string literals.
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

    fn gen_reference(&mut self, program: &Program) -> Result<(), String> {
        for item in &program.items {
            match item {
                Item::Typ(t) => {
                    return Err(format!(
                        "typ `{}` has no v1 Move-logic form; refusing (integer/decimal/fn core only)",
                        t.name
                    ))
                }
                Item::Enum(e) => {
                    return Err(format!(
                        "enum `{}` has no v1 Move-logic form; refusing (integer/decimal/fn core only)",
                        e.name
                    ))
                }
                _ => {}
            }
        }
        self.line(
            0,
            "\"\"\"CuNi Move backend — standalone logic reference (run with python3).",
        );
        self.line(0, "");
        self.line(
            0,
            "The same law as the Move `fun` core, in executable form: CuNi `int`",
        );
        self.line(
            0,
            "is u64, `dec` is scaled u128 (10^4). The `_cuni_u64` / `_cuni_u128`",
        );
        self.line(
            0,
            "gates raise loudly instead of wrapping — the same loud refusal the",
        );
        self.line(
            0,
            "Move side gets from native abort-on-overflow. Truncation toward zero:",
        );
        self.line(
            0,
            "every operand here is non-negative, so `//` is exact truncation.",
        );
        self.line(0, "\"\"\"");
        self.out.push('\n');
        self.line(0, "CUNI_DEC_SCALE = 10000");
        self.line(0, "_CUNI_U64_MAX = (1 << 64) - 1");
        self.line(0, "_CUNI_U128_MAX = (1 << 128) - 1");
        self.out.push('\n');
        self.line(0, "def _cuni_u64(v):");
        self.line(1, "\"\"\"Loud refusal: ints that leave u64 raise, never wrap.\"\"\"");
        self.line(1, "if not 0 <= v <= _CUNI_U64_MAX:");
        self.line(2, "raise OverflowError(\"cuni: int out of u64 range — refused\")");
        self.line(1, "return v");
        self.out.push('\n');
        self.line(0, "def _cuni_u128(v):");
        self.line(1, "\"\"\"Loud refusal: scaled decs that leave u128 raise, never wrap.\"\"\"");
        self.line(1, "if not 0 <= v <= _CUNI_U128_MAX:");
        self.line(
            2,
            "raise OverflowError(\"cuni: dec scaled value out of u128 range — refused\")",
        );
        self.line(1, "return v");
        self.out.push('\n');
        // int ops (u64 envelope; ZeroDivisionError is the loud div-by-zero).
        self.line(0, "def _cuni_iadd(a, b):");
        self.line(1, "return _cuni_u64(a + b)");
        self.out.push('\n');
        self.line(0, "def _cuni_isub(a, b):");
        self.line(1, "return _cuni_u64(a - b)");
        self.out.push('\n');
        self.line(0, "def _cuni_imul(a, b):");
        self.line(1, "return _cuni_u64(a * b)");
        self.out.push('\n');
        self.line(0, "def _cuni_idiv(a, b):");
        self.line(1, "return _cuni_u64(a // b)");
        self.out.push('\n');
        self.line(0, "def _cuni_imod(a, b):");
        self.line(1, "return _cuni_u64(a % b)");
        self.out.push('\n');
        // dec ops (u128 envelope, truncation toward zero via `//`).
        self.line(0, "def _cuni_dadd(a, b):");
        self.line(1, "return _cuni_u128(a + b)");
        self.out.push('\n');
        self.line(0, "def _cuni_dsub(a, b):");
        self.line(1, "return _cuni_u128(a - b)");
        self.out.push('\n');
        self.line(0, "def _cuni_dmul(a, b):");
        self.line(1, "# trunc(a*b/10000) toward zero (docs/DECIMAL.md §3).");
        self.line(1, "return _cuni_u128((a * b) // CUNI_DEC_SCALE)");
        self.out.push('\n');
        self.line(0, "def _cuni_ddiv(a, b):");
        self.line(1, "# trunc(a*10000/b) toward zero; div-by-zero raises loudly.");
        self.line(1, "if b == 0:");
        self.line(2, "raise ZeroDivisionError(\"cuni: dec division by zero\")");
        self.line(1, "return _cuni_u128((a * CUNI_DEC_SCALE) // b)");
        self.out.push('\n');
        self.line(0, "def _cuni_dec_of_int(n):");
        self.line(1, "return _cuni_u128(n * CUNI_DEC_SCALE)");
        self.out.push('\n');
        self.line(0, "def _cuni_int_of_dec(d):");
        self.line(1, "return _cuni_u64(d // CUNI_DEC_SCALE)");
        self.out.push('\n');
        self.line(0, "def _cuni_dec_str(v):");
        self.line(1, "\"\"\"Canonical `dec` rendering (docs/DECIMAL.md §6).\"\"\"");
        self.line(1, "neg = v < 0");
        self.line(1, "mag = -v if neg else v");
        self.line(1, "ip = mag // CUNI_DEC_SCALE");
        self.line(1, "fp = (\"%04d\" % (mag % CUNI_DEC_SCALE)).rstrip(\"0\") or \"0\"");
        self.line(1, "return (\"-\" if neg else \"\") + str(ip) + \".\" + fp");
        self.out.push('\n');
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
        self.line(0, "def main():");
        if top.is_empty() {
            self.line(1, "pass");
        } else {
            let mut scope = HashMap::new();
            for s in top {
                self.gen_stmt(1, s, &mut scope)?;
            }
        }
        self.out.push('\n');
        self.line(0, "if __name__ == \"__main__\":");
        self.line(1, "main()");
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        // Refuse unmappable types up front so the reference never runs on a lie.
        for p in &f.params {
            move_type(&p.ty)?;
        }
        move_type(&f.ret_type)?;
        let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
        let sig: Vec<String> = f
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, kind_name(kind_of_type(&p.ty))))
            .collect();
        self.line(
            0,
            &format!(
                "def {}({}):  # {} -> {}",
                f.name,
                params.join(", "),
                if sig.is_empty() {
                    "()".to_string()
                } else {
                    format!("({})", sig.join(", "))
                },
                kind_name(kind_of_type(&f.ret_type)),
            ),
        );
        let mut scope: HashMap<String, MoveKind> = HashMap::new();
        for p in &f.params {
            scope.insert(p.name.clone(), kind_of_type(&p.ty));
        }
        if f.body.is_empty() {
            self.line(1, "pass");
        }
        for s in &f.body {
            self.gen_stmt(1, s, &mut scope)?;
        }
        Ok(())
    }

    fn gen_stmt(
        &mut self,
        indent: usize,
        stmt: &Stmt,
        scope: &mut HashMap<String, MoveKind>,
    ) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                self.gen_binding(indent, name, ty, value, scope)
            }
            StmtKind::Assign { target, value } => {
                let t = self.gen_expr(target, scope)?;
                let v = self.gen_expr(value, scope)?;
                self.line(indent, &format!("{} = {}", t, v));
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let text = self.gen_expr(e, scope)?;
                self.line(indent, &format!("return {}", text));
                Ok(())
            }
            StmtKind::Ret(None) => {
                self.line(indent, "return None");
                Ok(())
            }
            StmtKind::Fail(e) => match &e.kind {
                ExprKind::Str(s) => {
                    self.line(indent, &format!("raise RuntimeError(\"{}\")", Self::esc(s)));
                    Ok(())
                }
                _ => Err("fail with a non-string has no clean reference abort; refusing".into()),
            },
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("if {}:", cond_s));
                self.gen_block(indent + 1, then_body, scope)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "else:");
                    self.gen_block(indent + 1, else_body, scope)?;
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
                            return Err(
                                "for over non-range iterables has no reference form; refusing"
                                    .into(),
                            );
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => return Err("range() with step has no reference form; refusing".into()),
                        }
                    }
                    _ => {
                        return Err(
                            "for over non-range iterables has no reference form; refusing".into()
                        )
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr(se, scope)?,
                    None => "0".to_string(),
                };
                let e = self.gen_expr(end_e, scope)?;
                scope.insert(a.clone(), MoveKind::Int);
                self.line(indent, &format!("for {} in range({}, {}):", a, s, e));
                self.gen_block(indent + 1, body, scope)?;
                Ok(())
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("while {}:", cond_s));
                self.gen_block(indent + 1, body, scope)?;
                Ok(())
            }
            StmtKind::ExprStmt(e) => self.gen_expr_stmt(indent, e, scope),
            StmtKind::Todo => Err("`...` placeholder body cannot become a reference; refusing".into()),
        }
    }

    fn gen_binding(
        &mut self,
        indent: usize,
        name: &str,
        ty: &Option<Type>,
        value: &Expr,
        scope: &mut HashMap<String, MoveKind>,
    ) -> Result<(), String> {
        if matches!(value.kind, ExprKind::Unwrap { .. }) {
            return Err("`??` has no reference equivalent for internal calls; refusing".into());
        }
        if let Some(t) = ty {
            move_type(t)?;
        }
        let kind = ty.as_ref().map(kind_of_type).unwrap_or(MoveKind::Other);
        let kind = if kind == MoveKind::Other {
            self.expr_kind(value, scope)
        } else {
            kind
        };
        if kind == MoveKind::Other {
            return Err(format!(
                "cannot infer a reference type for `{}`; annotate it",
                name
            ));
        }
        scope.insert(name.to_string(), kind);
        let v = self.gen_expr(value, scope)?;
        self.line(indent, &format!("{} = {}", name, v));
        Ok(())
    }

    fn gen_block(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, MoveKind>,
    ) -> Result<(), String> {
        if body.is_empty() {
            self.line(indent, "pass");
        }
        for s in body {
            self.gen_stmt(indent, s, scope)?;
        }
        Ok(())
    }

    fn gen_expr_stmt(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &mut HashMap<String, MoveKind>,
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
                    MoveKind::Int | MoveKind::Str => {
                        self.line(indent, &format!("print({})", text));
                        return Ok(());
                    }
                    MoveKind::Dec => {
                        self.line(indent, &format!("print(_cuni_dec_str({}))", text));
                        return Ok(());
                    }
                    MoveKind::Bool => {
                        self.line(
                            indent,
                            &format!("print(\"True\" if {} else \"False\")", text),
                        );
                        return Ok(());
                    }
                    MoveKind::Other => {
                        return Err("say of this value has no reference form; refusing".into())
                    }
                }
            }
        }
        let text = self.gen_expr(e, scope)?;
        self.line(indent, &text);
        Ok(())
    }

    fn gen_expr(&mut self, e: &Expr, scope: &HashMap<String, MoveKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(check_nonneg_int(*n)?.to_string()),
            ExprKind::Float(_) => Err("float literals have no reference form; refusing".into()),
            ExprKind::Dec(s) => Ok(check_nonneg_dec(*s)?.to_string()),
            ExprKind::Time(_) => Err("time literals have no reference form (v1); refusing".into()),
            ExprKind::Bool(b) => Ok(if *b { "True".into() } else { "False".into() }),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(parts) => {
                let mut out = String::from("f\"");
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => {
                            // Inside an f-string, braces must double; quotes
                            // are already escaped by esc().
                            let mut lit = Self::esc(t);
                            lit = lit.replace('{', "{{").replace('}', "}}");
                            out.push_str(&lit);
                        }
                        StrPartExpr::Expr(ie) => {
                            let k = self.expr_kind(ie, scope);
                            let t = self.gen_expr(ie, scope)?;
                            match k {
                                MoveKind::Int | MoveKind::Str => {
                                    out.push_str(&format!("{{{}}}", t));
                                }
                                MoveKind::Dec => {
                                    out.push_str(&format!("{{_cuni_dec_str({})}}", t));
                                }
                                MoveKind::Bool => {
                                    out.push_str(&format!("{{('True' if {} else 'False')}}", t));
                                }
                                MoveKind::Other => {
                                    return Err(
                                        "interpolating this value has no reference form; refusing"
                                            .into(),
                                    )
                                }
                            }
                        }
                    }
                }
                out.push('"');
                Ok(out)
            }
            ExprKind::NoneLit => Err("None has no reference form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 reference form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 reference form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 reference form (structs/enums are refused); refusing".into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs, scope)?;
                let r = self.gen_expr(rhs, scope)?;
                let lk = self.expr_kind(lhs, scope);
                let rk = self.expr_kind(rhs, scope);
                if matches!(*op, BinOp::Add) && lk == MoveKind::Str && rk == MoveKind::Str {
                    return Ok(format!("({} + {})", l, r));
                }
                if lk == MoveKind::Dec && rk == MoveKind::Dec {
                    let h = match op {
                        BinOp::Add => "_cuni_dadd",
                        BinOp::Sub => "_cuni_dsub",
                        BinOp::Mul => "_cuni_dmul",
                        BinOp::Div => "_cuni_ddiv",
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
                            return Err("`and`/`or` on `dec` has no reference form; refusing".into())
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
                if matches!(lk, MoveKind::Other) {
                    return Err(format!(
                        "`{}` on this value has no reference form; refusing",
                        op_name(op)
                    ));
                }
                if matches!(lk, MoveKind::Int) {
                    let h = match op {
                        BinOp::Add => "_cuni_iadd",
                        BinOp::Sub => "_cuni_isub",
                        BinOp::Mul => "_cuni_imul",
                        BinOp::Div => "_cuni_idiv",
                        BinOp::Mod => "_cuni_imod",
                        BinOp::Eq => return Ok(format!("({} == {})", l, r)),
                        BinOp::Ne => return Ok(format!("({} != {})", l, r)),
                        BinOp::Lt => return Ok(format!("({} < {})", l, r)),
                        BinOp::Gt => return Ok(format!("({} > {})", l, r)),
                        BinOp::Le => return Ok(format!("({} <= {})", l, r)),
                        BinOp::Ge => return Ok(format!("({} >= {})", l, r)),
                        _ => {
                            return Err(format!(
                                "`{}` is not defined on `int`; refusing",
                                op_name(op)
                            ))
                        }
                    };
                    return Ok(format!("{}({}, {})", h, l, r));
                }
                if matches!(lk, MoveKind::Bool) {
                    let o = match op {
                        BinOp::Eq => "==",
                        BinOp::Ne => "!=",
                        BinOp::And => "and",
                        BinOp::Or => "or",
                        _ => {
                            return Err(format!(
                                "`{}` is not defined on `bool`; refusing",
                                op_name(op)
                            ))
                        }
                    };
                    return Ok(format!("({} {} {})", l, o, r));
                }
                if matches!(lk, MoveKind::Str) {
                    match op {
                        BinOp::Eq => return Ok(format!("({} == {})", l, r)),
                        BinOp::Ne => return Ok(format!("({} != {})", l, r)),
                        _ => {
                            return Err(format!(
                                "`{}` is not defined on `str`; refusing",
                                op_name(op)
                            ))
                        }
                    }
                }
                Err(format!(
                    "`{}` on `{}` has no reference form; refusing",
                    op_name(op),
                    kind_name(lk)
                ))
            }
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => {
                    if self.expr_kind(expr, scope) != MoveKind::Bool {
                        return Err("`not` on a non-bool has no reference form; refusing".into());
                    }
                    let t = self.gen_expr(expr, scope)?;
                    Ok(format!("(not {})", t))
                }
                UnOp::Neg => Err(
                    "unary negation has no Move form (Move has no signed integers); refusing"
                        .into(),
                ),
            },
            ExprKind::Unwrap { .. } => {
                Err("`??` has no reference equivalent for internal calls; refusing".into())
            }
        }
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, MoveKind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a reference form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no reference form; refusing".into());
        }
        match name.as_str() {
            "say" => return Err("say is a statement, not an expression; refusing".into()),
            "range" => return Err("range() outside for has no reference form; refusing".into()),
            "len" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("len takes one argument".into());
                }
                return match self.expr_kind(args[0].expr(), scope) {
                    MoveKind::Str => Ok(format!("len({})", vals[0])),
                    _ => Err("len() of this value has no reference form; refusing".into()),
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
                if self.expr_kind(args[0].expr(), scope) != MoveKind::Int {
                    return Err("dec_of_int takes an `int`; refusing".into());
                }
                return Ok(format!("_cuni_dec_of_int({})", vals[0]));
            }
            "int_of_dec" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("int_of_dec takes one argument".into());
                }
                if self.expr_kind(args[0].expr(), scope) != MoveKind::Dec {
                    return Err("int_of_dec takes a `dec`; refusing".into());
                }
                return Ok(format!("_cuni_int_of_dec({})", vals[0]));
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

    fn expr_kind(&self, e: &Expr, scope: &HashMap<String, MoveKind>) -> MoveKind {
        match &e.kind {
            ExprKind::Int(_) => MoveKind::Int,
            ExprKind::Dec(_) => MoveKind::Dec,
            ExprKind::Str(_) | ExprKind::InterpStr(_) => MoveKind::Str,
            ExprKind::Bool(_) => MoveKind::Bool,
            ExprKind::Ident(n) => scope.get(n).copied().unwrap_or(MoveKind::Other),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "len" => MoveKind::Int,
                ExprKind::Ident(n) if n == "dec_of_int" => MoveKind::Dec,
                ExprKind::Ident(n) if n == "int_of_dec" => MoveKind::Int,
                ExprKind::Ident(n) => self.fn_ret.get(n).copied().unwrap_or(MoveKind::Other),
                _ => MoveKind::Other,
            },
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => MoveKind::Bool,
                BinOp::Add if self.expr_kind(lhs, scope) == MoveKind::Str => MoveKind::Str,
                _ => {
                    let lk = self.expr_kind(lhs, scope);
                    let rk = self.expr_kind(rhs, scope);
                    if lk == MoveKind::Dec || rk == MoveKind::Dec {
                        MoveKind::Dec
                    } else {
                        MoveKind::Int
                    }
                }
            },
            ExprKind::Unary { op, .. } => match op {
                UnOp::Not => MoveKind::Bool,
                UnOp::Neg => MoveKind::Other,
            },
            _ => MoveKind::Other,
        }
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
    ret amount * 0.0025dec + 0.25dec
end

say(fee(99.9999dec))
say(fee(1000000.00dec))
"#;

    #[test]
    fn move_core_has_unsigned_helpers() {
        let core = move_logic_core(&parse_src(FEE)).expect("move core");
        assert!(core.contains("fun fee(amount: u128): u128"));
        assert!(core.contains("fun cuni_dec_str(v: u128): String"));
        assert!(core.contains("fun cuni_dec_of_int(n: u64): u128"));
        assert!(core.contains("public entry fun run()"));
        // dec mul is trunc(a*b/10000) with an explicit u128 scale literal.
        assert!(core.contains("((amount * 100u128) / 10000u128)") || core.contains("/ 10000u128)"));
        assert!(core.contains("std::debug::print(&cuni_dec_str("));
    }

    #[test]
    fn python_reference_has_checked_envelope() {
        let py = generate_reference(&parse_src(FEE)).expect("python reference");
        assert!(py.contains("def fee(amount):"));
        assert!(py.contains("def _cuni_dmul(a, b):"));
        assert!(py.contains("def _cuni_dec_str(v):"));
        assert!(py.contains("def main():"));
        assert!(py.contains("if __name__ == \"__main__\":"));
        assert!(py.contains("print(_cuni_dec_str("));
    }

    #[test]
    fn program_embeds_move_core_verbatim() {
        let prog = parse_src(FEE);
        let core = move_logic_core(&prog).expect("move core");
        let module_src = generate_program(&prog, "cuni_fee").expect("module");
        let start = module_src.find(LOGIC_START).expect("logic start marker");
        let end = module_src.find(LOGIC_END).expect("logic end marker");
        assert!(start < end);
        let block = &module_src[start..end];
        let mut extracted = String::new();
        for line in block.lines().skip(1) {
            let dedented = line.strip_prefix("    ").unwrap_or(line);
            extracted.push_str(dedented);
            extracted.push('\n');
        }
        let extracted = extracted.trim_end_matches('\n').to_string() + "\n";
        let core_norm = core.trim_end_matches('\n').to_string() + "\n";
        assert_eq!(
            extracted, core_norm,
            "module's logic region must be byte-identical to the standalone Move core"
        );
    }

    #[test]
    fn program_shell_has_move_shape() {
        let module_src = generate_program(&parse_src(FEE), "cuni_fee").expect("module");
        for marker in [
            SHELL_START,
            SHELL_END,
            "module 0xCUNI::cuni_fee {",
            "0xCUNI",
            "public entry fun fee_entry(amount: u128)",
            "let result = fee(amount);",
            "public entry fun run()",
        ] {
            assert!(
                module_src.contains(marker),
                "module shell missing `{}`",
                marker
            );
        }
    }

    #[test]
    fn refuses_negatives_float_list_typ_unwrap() {
        for (tag, src) in [
            (
                "neg-int",
                "def f() -> int do\n ret 1\nend\nsay(-5)\n",
            ),
            (
                "neg-dec",
                "def f() -> dec do\n ret 1.0dec\nend\nsay(-2.5dec)\n",
            ),
            (
                "neg-expr",
                "def f(x: int) -> int do\n ret -x\nend\nsay(f(1))\n",
            ),
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
        ] {
            let err = move_logic_core(&parse_src(src)).expect_err(&format!("{tag} must refuse"));
            assert!(!err.is_empty(), "{tag}: refusal needs a reason");
            let err_py =
                generate_reference(&parse_src(src)).expect_err(&format!("{tag} must refuse (py)"));
            assert!(!err_py.is_empty(), "{tag}: py refusal needs a reason");
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
"#;
        let core = move_logic_core(&parse_src(src)).expect("move core");
        assert!(core.contains("fun shout(s: String, b: bool): String"));
        assert!(core.contains("std::string::length(&"));
        assert!(core.contains("b\"True\""), "bool must print canonically");
        let py = generate_reference(&parse_src(src)).expect("python reference");
        assert!(py.contains("def shout(s, b):"));
        assert!(py.contains("print(\"True\" if"));
    }
}

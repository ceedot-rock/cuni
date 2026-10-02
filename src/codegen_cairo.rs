//! Cairo contract backend — Starknet Cairo writer.
//!
//! "Trust Provable, in all things."
//!
//! Emits a genuine Starknet contract shape from CuNi source: a pure logic
//! core (the part CuNi proves) plus the contract shell (`#[starknet::contract]`,
//! storage, ABI) that wraps it.
//!
//! Two artifacts, one law:
//! - `logic_core` — the pure logic in Cairo: free `fn` definitions, one per
//!   CuNi `def` (underscore-prefixed so they never collide with the ABI
//!   method names in the shell). There is deliberately NO driver here: a
//!   contract has no stdout, so the CuNi driver (`say` lines) runs only in
//!   the Python reference.
//! - `generate_program` — the full contract artifact: the logic core
//!   embedded verbatim (clearly delimited), the `#[starknet::interface]`
//!   trait, and the `#[starknet::contract]` shell (also clearly delimited)
//!   with `#[storage]`, one `#[abi(embed_v0)]` method per CuNi `def`.
//! - `generate_reference` — the standalone runnable Python reference of the
//!   pure logic core with a `main` driver printing the `say` outputs. This
//!   is what the gate proves byte-identical to CuNi gold.
//!
//! Exactness notes:
//! - CuNi `int` is u256 here. Cairo's felt252/u256 world has no negative
//!   integers, so **negative literals are honestly refused at emit** (a
//!   unary `-` on an `int`/`dec` literal, or on any `int`/`dec` expression,
//!   refuses with a clear message). Within the non-negative domain, `/`
//!   truncates exactly like CuNi's truncated division and `%` takes the
//!   dividend's sign (vacuously, since values are non-negative). u256
//!   arithmetic is checked and panics on overflow — the loud refusal
//!   docs/DECIMAL.md section 7 asks for.
//! - CuNi `dec` is a scaled u256 (scale 10^4), truncated toward zero,
//!   canonical rendering per docs/DECIMAL.md section 6. Negative decimals
//!   are refused at emit, same as negative ints. `*` lowers to
//!   `trunc(a*b/10000)`, `/` to `trunc(a*10000/b)`; `%` on `dec` is refused
//!   (docs/DECIMAL.md section 3). `dec` values cross the ABI boundary
//!   scaled by 10^4; human rendering happens in the Python reference via the
//!   canonical `_cuni_dec_str`.
//! - A program whose `int`/`dec` values go negative at RUNTIME will panic
//!   on the checked subtraction — this is a loud failure, never a silent
//!   wrap, and it is documented here rather than hidden.
//! - `bool` is native; `str` is `ByteArray`. String `+` (concat) is refused
//!   in v1 — the backend will not guess at `ByteArray` append plumbing it
//!   cannot prove. Interpolated (backtick) strings are driver-side and
//!   refused in the contract for the same reason.
//! - `say` inside a function body is refused in the contract artifact: a
//!   contract function cannot print, so `say` lives only in the reference
//!   driver. Top-level `say` lines are what the reference runs.
//! - `fail` is refused in v1: Cairo has no message-carrying panic form the
//!   backend can prove, so it refuses rather than dropping the message.
//! - `abs` on this seat is the identity (`u256` values are non-negative by
//!   construction); `min`/`max` lower to `if` expressions.
//! - `float`, `list`, `map`, `opt`, `??`, structs, enums, `time`, `use`,
//!   `ext`, and `iface` are honestly refused: a Cairo function's verifiable
//!   core is integer/decimal math, and the backend will not guess at
//!   mappings it cannot prove.
//!
//! Honest boundaries: the logic core is gate-proven via `generate_reference`
//! (a Python rendering run with `python3` — no Cairo/Scarb toolchain on the
//! check machine); the contract shell is NOT compiled here and nothing has
//! executed on-chain. See `docs/ONCHAIN.md` for the full verification matrix.

use crate::ast::{
    BinOp, CallArg, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr, Type, UnOp,
};
use std::collections::{HashMap, HashSet};

/// Cairo-logic kind of a CuNi value, for `say` routing and type inference.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CKind {
    Int,
    Dec,
    Str,
    Bool,
    Other,
}

/// Delimiters marking the two regions of a `--emit-cairo` artifact.
pub const LOGIC_START: &str = "// CUNI-LOGIC-CORE-START";
pub const LOGIC_END: &str = "// CUNI-LOGIC-CORE-END";
pub const SHELL_START: &str = "// CUNI-CAIRO-SHELL-START";
pub const SHELL_END: &str = "// CUNI-CAIRO-SHELL-END";

pub struct Codegen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, CKind>,
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
                '\r' => r.push_str("\\r"),
                c => r.push(c),
            }
        }
        r
    }
}

/// Map a CuNi type to its Cairo type.
///
/// `int` is u256 (the Starknet-native unsigned word); `dec` is the scaled
/// u256 (scale 10^4, docs/DECIMAL.md). Negative values are refused at emit.
fn cairo_type(ty: &Type) -> Result<String, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok("u256".into()),
            "dec" => Ok("u256".into()),
            "bool" => Ok("bool".into()),
            "str" => Ok("ByteArray".into()),
            "float" => Err("Cairo logic core has no float type; refusing float".into()),
            other => Err(format!("type `{other}` has no Cairo-logic mapping; refusing")),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{name}` has no Cairo-logic mapping; refusing"
        )),
    }
}

fn kind_of_type(ty: &Type) -> CKind {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => CKind::Int,
            "dec" => CKind::Dec,
            "str" => CKind::Str,
            "bool" => CKind::Bool,
            _ => CKind::Other,
        },
        _ => CKind::Other,
    }
}

/// The pure logic core in Cairo: free `fn` definitions, no driver.
///
/// A contract has no stdout, so the CuNi driver (`say` lines) is not part of
/// this artifact — it runs only in the Python reference.
/// `generate_program` embeds this output verbatim.
///
/// Lockstep: the reference is generated first and discarded. The contract
/// emits no driver, but it must not silently swallow a program the
/// reference refuses (e.g. a negative literal inside a `say` line):
/// contract-accept implies reference-accept, so the gate can always prove
/// what the contract states.
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
/// rendering (`_cuni_dec_str`) follows docs/DECIMAL.md section 6, mirroring
/// the Vyper emitter's reference prelude plus a u256-domain guard (negative
/// scaled values raise rather than render — the contract cannot state them).
pub fn generate_reference(program: &Program) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_reference(program)?;
    Ok(g.out)
}

/// Full Cairo contract artifact: the logic core embedded verbatim (clearly
/// delimited), the `#[starknet::interface]` trait, and the
/// `#[starknet::contract]` shell (also clearly delimited) that requires the
/// Cairo/Scarb toolchain.
pub fn generate_program(program: &Program, mod_name: &str) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_program(program, mod_name)?;
    Ok(g.out)
}

impl Codegen {
    /// Shared refusal surface: every item kind the v1 profile cannot prove.
    fn check_items(&self, program: &Program) -> Result<(), String> {
        for item in &program.items {
            match item {
                Item::Typ(t) => {
                    return Err(format!(
                        "typ `{}` has no v1 Cairo-logic form; refusing (integer/decimal/fn core only)",
                        t.name
                    ))
                }
                Item::Enum(e) => {
                    return Err(format!(
                        "enum `{}` has no v1 Cairo-logic form; refusing (integer/decimal/fn core only)",
                        e.name
                    ))
                }
                Item::Use(u) => {
                    return Err(format!(
                        "`use {}` imports have no on-chain Cairo form; refusing",
                        u.name
                    ))
                }
                Item::Ext(e) => {
                    return Err(format!(
                        "`ext {}` blocks have no on-chain Cairo form; refusing",
                        e.name
                    ))
                }
                Item::Iface(i) => {
                    return Err(format!(
                        "`iface {}` has no on-chain Cairo form; refusing",
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

    fn gen_program(&mut self, program: &Program, mod_name: &str) -> Result<(), String> {
        self.check_items(program)?;
        self.line(0, "// Generated by the CuNi Cairo backend.");
        self.line(0, "// \"Trust Provable, in all things.\"");
        self.line(0, "//");
        self.line(
            0,
            "// STRUCTURE — two delimited regions, one law (same logic or refuse):",
        );
        self.line(
            0,
            "// - Logic core (CUNI-LOGIC-CORE markers): pure Cairo `fn`s, one per",
        );
        self.line(
            0,
            "//   CuNi `def`, underscore-prefixed so they never collide with the",
        );
        self.line(
            0,
            "//   ABI method names. CuNi `int` is u256 (negative literals refused",
        );
        self.line(
            0,
            "//   at emit); CuNi `dec` is u256 scaled 10^4 (docs/DECIMAL.md),",
        );
        self.line(
            0,
            "//   truncated toward zero. No driver here: a contract has no",
        );
        self.line(
            0,
            "//   stdout; the `say` driver runs only in the Python reference",
        );
        self.line(0, "//   (`--emit-cairo-ref`).");
        self.line(
            0,
            "// - Contract shell (CUNI-CAIRO-SHELL markers): the",
        );
        self.line(
            0,
            "//   `#[starknet::interface]` trait plus the `#[starknet::contract]`",
        );
        self.line(
            0,
            "//   module with `#[storage]` and one `#[abi(embed_v0)]` method per",
        );
        self.line(
            0,
            "//   CuNi `def` (`dec` values cross the ABI scaled by 10^4).",
        );
        self.line(
            0,
            "//   Requires Cairo 2.x + Scarb; NOT compiled by `cuni check` (no",
        );
        self.line(0, "//   Cairo toolchain on the check machine).");
        self.line(0, "//");
        self.line(
            0,
            "// HONEST BOUNDARIES: logic gate-proven via the Python reference;",
        );
        self.line(0, "// shell not compiled here; nothing has executed on-chain.");
        self.out.push('\n');

        // Region 1: the logic core, embedded verbatim.
        self.line(0, LOGIC_START);
        let core = logic_core(program)?;
        self.out.push_str(&core);
        self.line(0, LOGIC_END);
        self.out.push('\n');

        // Region 2: the contract shell.
        self.line(0, SHELL_START);
        self.line(
            0,
            "// Requires: Cairo 2.x + Scarb. This shell is NOT compiled by `cuni check` —",
        );
        self.line(0, "// no Cairo toolchain on the check machine.");
        self.out.push('\n');

        let defs: Vec<&FnDecl> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Def(f) => Some(f),
                _ => None,
            })
            .collect();

        let trait_name = format!("I{}", to_pascal(mod_name));
        let impl_name = format!("{}Impl", to_pascal(mod_name));

        // The ABI trait: one method per CuNi function, snapshot self (pure).
        self.line(0, "#[starknet::interface]");
        self.line(0, &format!("trait {trait_name}<TContractState> {{"));
        if defs.is_empty() {
            self.line(1, "// (no CuNi functions: nothing to expose)");
        } else {
            for f in &defs {
                let mut params = Vec::new();
                for p in &f.params {
                    params.push(format!("{}: {}", p.name, cairo_type(&p.ty)?));
                }
                let ret = cairo_type(&f.ret_type)?;
                let sig = if params.is_empty() {
                    format!("fn {}(self: @TContractState) -> {ret};", f.name)
                } else {
                    format!(
                        "fn {}(self: @TContractState, {}) -> {ret};",
                        f.name,
                        params.join(", ")
                    )
                };
                self.line(1, &sig);
            }
        }
        self.line(0, "}");
        self.out.push('\n');

        self.line(0, "#[starknet::contract]");
        self.line(0, &format!("mod {mod_name} {{"));
        self.line(1, &format!("use super::{{{trait_name}}};"));
        for f in &defs {
            self.line(1, &format!("use super::{{_{}}};", f.name));
        }
        self.out.push('\n');
        self.line(1, "#[storage]");
        self.line(1, "struct Storage {}");
        self.out.push('\n');
        self.line(1, "#[abi(embed_v0)]");
        self.line(
            1,
            &format!("impl {impl_name} of super::{trait_name}<ContractState> {{"),
        );
        if defs.is_empty() {
            self.line(2, "// (no CuNi functions: nothing to expose)");
        } else {
            for (n, f) in defs.iter().enumerate() {
                if n > 0 {
                    self.out.push('\n');
                }
                self.gen_method(f)?;
            }
        }
        self.line(1, "}");
        self.line(0, "}");
        self.line(0, SHELL_END);
        Ok(())
    }

    /// One `#[abi(embed_v0)]` method per CuNi function: call the pure logic
    /// and return the computed value. Uniform by design: the shell does not
    /// guess which functions are "validators" — it computes and returns.
    fn gen_method(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        let mut arg_names = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, cairo_type(&p.ty)?));
            arg_names.push(p.name.clone());
        }
        let ret = cairo_type(&f.ret_type)?;
        let rk = kind_of_type(&f.ret_type);
        let sig = if params.is_empty() {
            format!("fn {}(self: @ContractState) -> {ret} {{", f.name)
        } else {
            format!(
                "fn {}(self: @ContractState, {}) -> {ret} {{",
                f.name,
                params.join(", ")
            )
        };
        self.line(2, &sig);
        if rk == CKind::Dec {
            self.line(
                3,
                "// CuNi `dec` crosses the ABI scaled by 10^4 (docs/DECIMAL.md).",
            );
        }
        self.line(3, &format!("_{}({})", f.name, arg_names.join(", ")));
        self.line(2, "}");
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, cairo_type(&p.ty)?));
        }
        let ret = cairo_type(&f.ret_type)?;
        self.line(
            0,
            &format!("fn _{}({}) -> {} {{", f.name, params.join(", "), ret),
        );
        let mut scope: HashMap<String, CKind> = HashMap::new();
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
        scope: &mut HashMap<String, CKind>,
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
                self.line(indent, &format!("{t} = {v};"));
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let text = self.gen_expr(e, scope)?;
                self.line(indent, &format!("return {text};"));
                Ok(())
            }
            StmtKind::Ret(None) => {
                self.line(indent, "return;");
                Ok(())
            }
            StmtKind::Fail(_) => Err(
                "fail has no v1 Cairo-logic form: Cairo offers no message-carrying \
                 panic the backend can prove, and it will not drop the message; refusing"
                    .into(),
            ),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("if {cond_s} {{"));
                self.gen_block(indent + 1, then_body, scope)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "} else {");
                    self.gen_block(indent + 1, else_body, scope)?;
                } else {
                    self.line(indent, "}");
                    return Ok(());
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
                    return Err("two-binding for has no Cairo-logic form; refusing".into());
                }
                let (start_s, end_e) = match &iter.kind {
                    ExprKind::Call { callee, args } => {
                        let is_range = matches!(&callee.kind, ExprKind::Ident(n) if n == "range");
                        if !is_range {
                            return Err(
                                "for over non-range iterables has no Cairo-logic form; refusing"
                                    .into(),
                            );
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => {
                                return Err("range() with step has no Cairo-logic form; refusing".into())
                            }
                        }
                    }
                    _ => {
                        return Err("for over non-range iterables has no Cairo-logic form; refusing"
                            .into())
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr(se, scope)?,
                    None => "0_u256".to_string(),
                };
                let e = self.gen_expr(end_e, scope)?;
                // Cairo has no C-style for; lower to an explicit counter loop.
                scope.insert(a.clone(), CKind::Int);
                self.line(indent, &format!("let mut {a}: u256 = {s};"));
                self.line(indent, "loop {");
                self.line(indent + 1, &format!("if {a} >= {e} {{"));
                self.line(indent + 2, "break;");
                self.line(indent + 1, "}");
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent + 1, &format!("{a} += 1_u256;"));
                self.line(indent, "};");
                Ok(())
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, "loop {");
                self.line(indent + 1, &format!("if !({cond_s}) {{"));
                self.line(indent + 2, "break;");
                self.line(indent + 1, "}");
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "};");
                Ok(())
            }
            StmtKind::ExprStmt(e) => self.gen_expr_stmt(indent, e, scope),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become a Cairo contract; refusing".into())
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
        scope: &mut HashMap<String, CKind>,
    ) -> Result<(), String> {
        if matches!(value.kind, ExprKind::Unwrap { .. }) {
            return Err("`??` has no Cairo-logic equivalent for internal calls; refusing".into());
        }
        let kind = ty.as_ref().map(kind_of_type).unwrap_or(CKind::Other);
        let kind = if kind == CKind::Other {
            self.expr_kind(value, scope)
        } else {
            kind
        };
        let decl_ty = match ty {
            Some(t) => cairo_type(t)?,
            None => match kind {
                CKind::Int | CKind::Dec => "u256".into(),
                CKind::Str => "ByteArray".into(),
                CKind::Bool => "bool".into(),
                CKind::Other => {
                    return Err(format!(
                        "cannot infer a Cairo-logic type for `{name}`; annotate it"
                    ))
                }
            },
        };
        scope.insert(name.to_string(), kind);
        let v = self.gen_expr(value, scope)?;
        let m = if is_mut { "mut " } else { "" };
        self.line(indent, &format!("let {m}{name}: {decl_ty} = {v};"));
        Ok(())
    }

    fn gen_block(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, CKind>,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt(indent, s, scope)?;
        }
        Ok(())
    }

    /// Expression statements. In the contract, `say` is refused everywhere:
    /// a contract function cannot print, and the top-level driver is not part
    /// of the contract artifact — `say` runs only in the Python reference.
    fn gen_expr_stmt(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &mut HashMap<String, CKind>,
    ) -> Result<(), String> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                let _ = args;
                return Err(
                    "say has no on-chain Cairo form: a contract function cannot print; \
                     the CuNi driver (`say` lines) runs only in the Python reference \
                     (`--emit-cairo-ref`) — refusing"
                        .into(),
                );
            }
        }
        let text = self.gen_expr(e, scope)?;
        self.line(indent, &format!("{text};"));
        Ok(())
    }

    fn gen_expr(&mut self, e: &Expr, scope: &HashMap<String, CKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => {
                if *n < 0 {
                    return Err(format!(
                        "negative int literal `{n}` has no u256 form; refusing \
                         (Cairo seat: negative literals refused at emit)"
                    ));
                }
                Ok(format!("{n}_u256"))
            }
            // `dec` literals (docs/DECIMAL.md): the parser already scaled to
            // 10^4, so the literal IS the scaled u256. A negative scaled
            // value cannot arise from the parser (negation is a unary op),
            // but the guard stays.
            ExprKind::Dec(scaled) => {
                if *scaled < 0 {
                    return Err(format!(
                        "negative dec literal (scaled `{scaled}`) has no u256 form; refusing"
                    ));
                }
                Ok(format!("{scaled}_u256"))
            }
            ExprKind::Float(_) => Err("float literals have no Cairo-logic form; refusing".into()),
            ExprKind::Time(_) => Err("time literals have no Cairo-logic form (v1); refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(_) => Err(
                "interpolated (backtick) strings have no v1 Cairo-logic form; refusing".into(),
            ),
            ExprKind::NoneLit => Err("None has no Cairo-logic form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Cairo-logic form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 Cairo-logic form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Cairo-logic form (structs/enums are refused); refusing"
                    .into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => self.gen_binary(*op, lhs, rhs, scope),
            ExprKind::Unary { op, expr } => {
                match op {
                    UnOp::Not => {
                        let t = self.gen_expr(expr, scope)?;
                        Ok(format!("(!{t})"))
                    }
                    // The documented refusal: u256 has no negation, so even
                    // `-5` (parsed as Neg(5)) refuses here at emit.
                    UnOp::Neg => Err(
                        "negation has no u256 form on the Cairo seat (negative literals \
                         refused at emit); refusing"
                            .into(),
                    ),
                }
            }
            ExprKind::Unwrap { .. } => {
                Err("`??` has no Cairo-logic equivalent for internal calls; refusing".into())
            }
        }
    }

    /// Binary operators with the dec/int closed world (docs/DECIMAL.md
    /// sections 3-5): both operands dec, both int, or a loud refusal.
    /// u256 `/` truncates (values are non-negative, so this IS truncation
    /// toward zero); `%` keeps the dividend's sign (vacuously).
    fn gen_binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, CKind>,
    ) -> Result<String, String> {
        let l = self.gen_expr(lhs, scope)?;
        let r = self.gen_expr(rhs, scope)?;
        let lk = self.expr_kind(lhs, scope);
        let rk = self.expr_kind(rhs, scope);
        if (lk == CKind::Dec) != (rk == CKind::Dec) {
            return Err("cannot mix `dec` and `int` — convert explicitly: \
                        `dec_of_int(n)` or `int_of_dec(d)`; refusing"
                .into());
        }
        if lk == CKind::Dec {
            let o = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                // trunc(a*b/10000) toward zero (docs/DECIMAL.md section 3).
                BinOp::Mul => return Ok(format!("(({l} * {r}) / 10000_u256)")),
                // trunc(a*10000/b) toward zero.
                BinOp::Div => return Ok(format!("(({l} * 10000_u256) / ({r}))")),
                BinOp::Mod => return Err("`%` is not defined on `dec`; refusing".into()),
                BinOp::Eq => "==",
                BinOp::Ne => "!=",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::Le => "<=",
                BinOp::Ge => ">=",
                BinOp::And | BinOp::Or => {
                    return Err("`and`/`or` on `dec` has no Cairo-logic form; refusing".into())
                }
            };
            return Ok(format!("({l} {o} {r})"));
        }
        if lk == CKind::Str && matches!(op, BinOp::Add) {
            return Err(
                "string `+` (concat) has no v1 Cairo-logic form — the backend will not \
                 guess at `ByteArray` append plumbing it cannot prove; refusing"
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
            BinOp::And => "&&",
            BinOp::Or => "||",
        };
        Ok(format!("({l} {o} {r})"))
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, CKind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a Cairo-logic form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no Cairo-logic form; refusing".into());
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
            "range" => return Err("range() outside for has no Cairo-logic form; refusing".into()),
            "len" => {
                arity(1)?;
                let k = self.expr_kind(args[0].expr(), scope);
                return match k {
                    // ByteArray::len -> u64; CuNi `len` returns int (u256).
                    CKind::Str => Ok(format!("(({}.len()).into())", vals[0])),
                    _ => Err("len() of this value has no Cairo-logic form; refusing".into()),
                };
            }
            // int-only per docs/DECIMAL.md section 5. abs is the identity on
            // this seat: u256 values are non-negative by construction, and
            // negative literals already refused at emit.
            "abs" => {
                arity(1)?;
                return Ok(format!("({})", vals[0]));
            }
            "min" => {
                arity(2)?;
                let (a, b) = (&vals[0], &vals[1]);
                return Ok(format!("(if {a} < {b} {{ {a} }} else {{ {b} }}"));
            }
            "max" => {
                arity(2)?;
                let (a, b) = (&vals[0], &vals[1]);
                return Ok(format!("(if {a} > {b} {{ {a} }} else {{ {b} }}"));
            }
            // Explicit dec/int conversions (docs/DECIMAL.md section 5).
            // u256 division truncates, so int_of_dec needs no helper.
            "dec_of_int" => {
                arity(1)?;
                return Ok(format!("(({} * 10000_u256))", vals[0]));
            }
            "int_of_dec" => {
                arity(1)?;
                return Ok(format!("(({} / 10000_u256))", vals[0]));
            }
            _ => {}
        }
        if !self.fn_names.contains(&name) {
            return Err(format!("unknown call `{name}`; refusing"));
        }
        // Internal logic functions are underscore-prefixed in the artifact.
        Ok(format!("_{}({})", name, vals.join(", ")))
    }

    /// Best-effort kind of an expression for `say` routing and inference.
    fn expr_kind(&self, e: &Expr, scope: &HashMap<String, CKind>) -> CKind {
        match &e.kind {
            ExprKind::Int(_) => CKind::Int,
            ExprKind::Dec(_) => CKind::Dec,
            ExprKind::Str(_) | ExprKind::InterpStr(_) => CKind::Str,
            ExprKind::Bool(_) => CKind::Bool,
            ExprKind::Ident(n) => scope.get(n).copied().unwrap_or(CKind::Other),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "len" => CKind::Int,
                ExprKind::Ident(n) if n == "abs" || n == "min" || n == "max" => CKind::Int,
                ExprKind::Ident(n) if n == "dec_of_int" => CKind::Dec,
                ExprKind::Ident(n) if n == "int_of_dec" => CKind::Int,
                ExprKind::Ident(n) => self.fn_ret.get(n).copied().unwrap_or(CKind::Other),
                _ => CKind::Other,
            },
            ExprKind::Binary { op, lhs, .. } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => CKind::Bool,
                _ => self.expr_kind(lhs, scope),
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => CKind::Bool,
                UnOp::Neg => self.expr_kind(expr, scope),
            },
            _ => CKind::Other,
        }
    }
}

/// `cuni_fee_schedule` -> `CuniFeeSchedule` for trait/impl names.
fn to_pascal(snake: &str) -> String {
    snake
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_ascii_uppercase().to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Python reference
// ---------------------------------------------------------------------------

impl Codegen {
    fn gen_reference(&mut self, program: &Program) -> Result<(), String> {
        self.check_items(program)?;
        // The Cairo seat refuses negative literals at emit; the reference
        // must refuse them too, or the gate would prove a program the
        // contract cannot state. Scan the AST before emitting anything.
        refuse_negatives(program)?;
        self.line(0, "#!/usr/bin/env python3");
        self.line(
            0,
            "\"\"\"CuNi Cairo logic-core reference: pure logic + driver, run with python3.",
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
        self.line(
            0,
            "The prelude below mirrors the Vyper emitter's (`_cuni_tdiv`,",
        );
        self.line(
            0,
            "`_cuni_tmod`, `_cuni_dec_str`): one law, two chains — plus the",
        );
        self.line(
            0,
            "u256-domain guard in `_cuni_dec_str`, since the Cairo seat cannot",
        );
        self.line(0, "state negative values.");
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
        self.line(1, "if v < 0:");
        self.line(
            2,
            "raise ValueError(\"cuni: negative dec value has no Cairo u256 form\")",
        );
        self.line(1, "ip = v // 10000");
        self.line(1, "fp = v % 10000");
        self.line(1, "frac = (\"%04d\" % fp).rstrip(\"0\") or \"0\"");
        self.line(1, "return str(ip) + \".\" + frac");
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
        for p in &f.params {
            cairo_type(&p.ty)?;
        }
        cairo_type(&f.ret_type)?;
        self.line(0, &format!("def {}({}):", f.name, params.join(", ")));
        let mut scope: HashMap<String, CKind> = HashMap::new();
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
        scope: &mut HashMap<String, CKind>,
    ) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                if let Some(t) = ty {
                    cairo_type(t)?;
                }
                if matches!(value.kind, ExprKind::Unwrap { .. }) {
                    return Err("`??` has no reference form for internal calls; refusing".into());
                }
                let kind = ty.as_ref().map(kind_of_type).unwrap_or(CKind::Other);
                let kind = if kind == CKind::Other {
                    self.expr_kind(value, scope)
                } else {
                    kind
                };
                if kind == CKind::Other {
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
            StmtKind::Fail(_) => Err("fail has no v1 Cairo reference form; refusing".into()),
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
                scope.insert(a.clone(), CKind::Int);
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
        scope: &mut HashMap<String, CKind>,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt_ref(indent, s, scope)?;
        }
        Ok(())
    }

    /// `say(x)` in the reference driver: print the canonical rendering.
    fn gen_say_ref(
        &mut self,
        indent: usize,
        args: &[CallArg],
        scope: &HashMap<String, CKind>,
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
                    CKind::Int => format!("print({t})"),
                    CKind::Dec => format!("print(_cuni_dec_str({t}))"),
                    CKind::Bool => format!("print(_cuni_bool_str({t}))"),
                    CKind::Str => format!("print({t})"),
                    CKind::Other => {
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
        scope: &HashMap<String, CKind>,
    ) -> Result<String, String> {
        let mut pieces = Vec::new();
        for p in parts {
            match p {
                StrPartExpr::Text(t) => pieces.push(format!("{:?}", t)),
                StrPartExpr::Expr(ie) => {
                    let k = self.expr_kind(ie, scope);
                    let t = self.gen_expr_ref(ie, scope)?;
                    match k {
                        CKind::Int => pieces.push(format!("str({t})")),
                        CKind::Dec => pieces.push(format!("_cuni_dec_str({t})")),
                        CKind::Bool => pieces.push(format!("_cuni_bool_str({t})")),
                        CKind::Str => pieces.push(format!("({t})")),
                        CKind::Other => {
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

    fn gen_expr_ref(&mut self, e: &Expr, scope: &HashMap<String, CKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => {
                if *n < 0 {
                    return Err(format!(
                        "negative int literal `{n}` has no Cairo reference form; refusing"
                    ));
                }
                Ok(n.to_string())
            }
            ExprKind::Dec(scaled) => {
                if *scaled < 0 {
                    return Err("negative dec literal has no Cairo reference form; refusing".into());
                }
                Ok(scaled.to_string())
            }
            ExprKind::Float(_) => Err("float literals have no reference form; refusing".into()),
            ExprKind::Time(_) => Err("time literals have no reference form (v1); refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("{:?}", s)),
            ExprKind::InterpStr(parts) => {
                let mut pieces = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => pieces.push(format!("{:?}", t)),
                        StrPartExpr::Expr(ie) => {
                            let k = self.expr_kind(ie, scope);
                            let t = self.gen_expr_ref(ie, scope)?;
                            match k {
                                CKind::Int => pieces.push(format!("str({t})")),
                                CKind::Dec => pieces.push(format!("_cuni_dec_str({t})")),
                                CKind::Bool => pieces.push(format!("_cuni_bool_str({t})")),
                                CKind::Str => pieces.push(format!("({t})")),
                                CKind::Other => {
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
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => {
                    let t = self.gen_expr_ref(expr, scope)?;
                    Ok(format!("(not {t})"))
                }
                UnOp::Neg => Err(
                    "negation has no Cairo reference form (negative literals refused at emit); \
                     refusing"
                        .into(),
                ),
            },
            ExprKind::Unwrap { .. } => Err("`??` has no reference form for internal calls; refusing".into()),
        }
    }

    fn gen_binary_ref(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, CKind>,
    ) -> Result<String, String> {
        let l = self.gen_expr_ref(lhs, scope)?;
        let r = self.gen_expr_ref(rhs, scope)?;
        let lk = self.expr_kind(lhs, scope);
        let rk = self.expr_kind(rhs, scope);
        if (lk == CKind::Dec) != (rk == CKind::Dec) {
            return Err("cannot mix `dec` and `int` — convert explicitly: \
                        `dec_of_int(n)` or `int_of_dec(d)`; refusing"
                .into());
        }
        if lk == CKind::Dec {
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
        if lk == CKind::Str && matches!(op, BinOp::Add) {
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
        scope: &HashMap<String, CKind>,
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

/// Refuse any unary negation of an `int`/`dec` value anywhere in the program:
/// the Cairo seat is u256, so the reference must never prove a program the
/// contract refuses. (The contract's `gen_expr` refuses the same nodes; this
/// scan keeps the two artifacts in lockstep before any Python is emitted.)
fn refuse_negatives(program: &Program) -> Result<(), String> {
    fn chk_expr(e: &Expr) -> Result<(), String> {
        match &e.kind {
            ExprKind::Unary { op: UnOp::Neg, .. } => Err(
                "negation has no u256 form on the Cairo seat (negative literals \
                 refused at emit); refusing"
                    .into(),
            ),
            ExprKind::Unary { expr: inner, .. } => chk_expr(inner),
            ExprKind::Binary { lhs, rhs, .. } => {
                chk_expr(lhs)?;
                chk_expr(rhs)
            }
            ExprKind::Call { callee, args } => {
                chk_expr(callee)?;
                for a in args {
                    chk_expr(a.expr())?;
                }
                Ok(())
            }
            ExprKind::InterpStr(parts) => {
                for p in parts {
                    if let StrPartExpr::Expr(ie) = p {
                        chk_expr(ie)?;
                    }
                }
                Ok(())
            }
            ExprKind::List(es) => es.iter().try_for_each(chk_expr),
            ExprKind::Map(kvs) => kvs.iter().try_for_each(|(k, v)| {
                chk_expr(k)?;
                chk_expr(v)
            }),
            ExprKind::Index { base, index } => {
                chk_expr(base)?;
                chk_expr(index)
            }
            ExprKind::Field { base, .. } => chk_expr(base),
            ExprKind::Unwrap { expr: inner, .. } => chk_expr(inner),
            _ => Ok(()),
        }
    }
    fn chk_stmt(s: &Stmt) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { value, .. } | StmtKind::Mut { value, .. } => chk_expr(value),
            StmtKind::Assign { target, value } => {
                chk_expr(target)?;
                chk_expr(value)
            }
            StmtKind::Ret(Some(e)) | StmtKind::Fail(e) => chk_expr(e),
            StmtKind::Ret(None) | StmtKind::Todo => Ok(()),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                chk_expr(cond)?;
                then_body.iter().try_for_each(chk_stmt)?;
                if let Some(eb) = else_body {
                    eb.iter().try_for_each(chk_stmt)?;
                }
                Ok(())
            }
            StmtKind::For { iter, body, .. } => {
                chk_expr(iter)?;
                body.iter().try_for_each(chk_stmt)
            }
            StmtKind::Whl { cond, body } => {
                chk_expr(cond)?;
                body.iter().try_for_each(chk_stmt)
            }
            StmtKind::ExprStmt(e) => chk_expr(e),
        }
    }
    for item in &program.items {
        match item {
            Item::Def(f) => f.body.iter().try_for_each(chk_stmt)?,
            Item::Stmt(s) => chk_stmt(s)?,
            _ => {}
        }
    }
    Ok(())
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
    fn logic_core_is_pure_u256_fns() {
        let core = logic_core(&parse_src(DEMO)).expect("logic core");
        assert!(core.contains("fn _fee(amount: u256) -> u256 {"));
        assert!(core.contains("fn _double(n: u256) -> u256 {"));
        // dec mul lowers to trunc(a*b/10000); literals carry _u256.
        // (Driver `say` lines are not part of the logic core — only the pure
        // fns are — so assert on literals inside the bodies: 100.00dec,
        // 0.01dec, 0.25dec.)
        assert!(core.contains("((amount * 100_u256) / 10000_u256)"));
        assert!(core.contains("1000000_u256"));
        assert!(core.contains("2500_u256"));
        assert!(!core.contains("say"), "contract logic core must not print");
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
        // Shell shape: Starknet contract, storage, ABI.
        for marker in [
            SHELL_START,
            SHELL_END,
            "#[starknet::contract]",
            "mod cuni_demo {",
            "#[storage]",
            "struct Storage {}",
            "#[starknet::interface]",
            "trait ICuniDemo<TContractState>",
            "#[abi(embed_v0)]",
            "impl CuniDemoImpl of super::ICuniDemo<ContractState>",
            "fn fee(self: @ContractState, amount: u256) -> u256 {",
            "_fee(amount)",
        ] {
            assert!(program_src.contains(marker), "shell missing `{marker}`");
        }
    }

    #[test]
    fn refuses_unprovable() {
        for (tag, src) in [
            ("float", "def f() -> int do\n ret 1\nend\nsay(1.5)\n"),
            (
                "negative-int-literal",
                "def f() -> int do\n ret 0\nend\nsay(-1)\n",
            ),
            (
                "negative-dec-literal",
                "def f() -> int do\n ret 0\nend\nsay(-1.5dec)\n",
            ),
            (
                "neg-expr",
                "def f(a: int) -> int do\n ret -a\nend\nsay(f(1))\n",
            ),
            (
                "fail",
                "def f() -> int do\n fail \"boom\"\nend\nsay(f())\n",
            ),
            (
                "str-concat",
                "def f(a: str) -> str do\n ret a + \"x\"\nend\nsay(f(\"y\"))\n",
            ),
            (
                "say-in-fn",
                "def f() -> int do\n say(1)\n ret 1\nend\nsay(f())\n",
            ),
            ("list", "def f() -> int do\n ret 1\nend\nlet xs = [1]\nsay(f())\n"),
            (
                "time",
                "def f() -> int do\n ret 1\nend\nsay(\"2026-01-01T00:00:00Z\"t)\n",
            ),
            (
                "dec-mod",
                "def f(a: dec) -> dec do\n ret a % 0.5dec\nend\nsay(f(1.0dec))\n",
            ),
        ] {
            let err = logic_core(&parse_src(src)).expect_err(&format!("{tag} must refuse"));
            assert!(!err.is_empty(), "{tag}: refusal needs a reason");
        }
    }

    #[test]
    fn reference_refuses_negatives_too() {
        let err = generate_reference(&parse_src("say(-1)\n"))
            .expect_err("reference must refuse the same negatives the contract refuses");
        assert!(!err.is_empty());
    }

    #[test]
    fn reference_renders_dec_canonically() {
        let r = generate_reference(&parse_src(DEMO)).expect("reference");
        assert!(r.contains("def _cuni_tdiv(a, b):"));
        assert!(r.contains("def _cuni_dec_str(v):"));
        assert!(r.contains("def fee(amount):"));
        assert!(r.contains("def main():"));
        assert!(r.contains("main()"));
        assert!(r.contains("print(_cuni_dec_str(fee(999999)))"));
        assert!(r.contains("print(double(21))"));
        // The Cairo reference guards the u256 domain at render time.
        assert!(r.contains("has no Cairo u256 form"));
    }
}

//! Cadence contract backend — Flow Cadence writer.
//!
//! "Trust Provable, in all things."
//!
//! Emits a genuine Cadence contract shape from CuNi source: a pure logic
//! core (the part CuNi proves) plus the contract shell (`access(all)`
//! contract, public functions) that wraps it.
//!
//! Two artifacts, one law:
//! - `generate_reference` — the standalone logic-core reference: Python,
//!   run with `python3`; its `main` driver prints the `say` outputs. This is
//!   what the gate proves byte-identical to CuNi gold.
//! - `generate_program` — the full Cadence contract: the pure logic as
//!   `access(self)` helpers (clearly delimited, nested inside the contract),
//!   plus the `access(all)` public surface (also clearly delimited) that
//!   requires the Flow/Cadence toolchain.
//!
//! Exactness notes:
//! - CuNi `int` is Cadence `Int256`. Cadence integer `/` and `%` truncate
//!   toward zero like CuNi's (re-verify on the Cadence toolchain before
//!   deployment — see honest boundaries); division or modulo by zero is a
//!   loud runtime error — never a value, never silent.
//! - CuNi `dec` is a scaled `Int256` (scale 10^4, docs/DECIMAL.md). This is
//!   deliberately NOT Cadence's native `Fix64` (scale 10^8): CuNi proves the
//!   semantics it was given, and a different fixed-point scale would be a
//!   different proof. `a + b` / `a - b` on the scaled integers;
//!   `a * b` lowers to `((a * b) / 10000)`; `a / b` lowers to
//!   `((a * 10000) / b)` — truncation toward zero throughout; `%` on `dec`
//!   is refused (docs/DECIMAL.md §3). `dec_of_int` / `int_of_dec` lower to
//!   `(n * 10000)` / `(d / 10000)`.
//! - `say` becomes `log(...)` — the on-chain log. For `dec`, `log` shows the
//!   exact *scaled* integer (12499 for 1.2499); the canonical `1.2499`
//!   rendering (docs/DECIMAL.md §6) is proven in the Python reference the
//!   gate runs, not in the contract.
//! - CuNi `snake_case` function names become Cadence `camelCase`
//!   (`transfer_fee` → `transferFee`, logic helper `transferFeeImpl`);
//!   collisions after conversion are refused rather than guessed.
//! - v1 statements: `def`, `let`, `if`/`els`, `ret`, and `say` (plus bare
//!   calls) as statements. `mut`, `=`, `for`, `whl`, `fail`, `...` are
//!   honestly refused.
//! - `float`, `list`, `map`, `opt`, `??`, `len`, structs, enums, `time`,
//!   and unknown calls are honestly refused: a Cadence function's verifiable
//!   core is integer/decimal math, and the backend will not guess at
//!   mappings it cannot prove.
//!
//! Honest boundaries: the logic core is gate-proven via `generate_reference`
//! (a Python rendering run with `python3` — no Cadence/Flow toolchain on the
//! check machine); the contract shell is NOT compiled here and nothing has
//! executed on-chain. See `docs/ONCHAIN.md` for the full verification matrix.

use crate::ast::{
    BinOp, CallArg, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr, Type,
    UnOp, DEC_SCALE,
};
use std::collections::{HashMap, HashSet};

/// Cadence-logic kind of a CuNi value: what a value *is* for codegen.
/// Anything else (float, list, map, opt, time, structs, enums) is refused
/// before a kind is ever assigned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Int,
    Dec,
    Str,
    Bool,
}

/// Delimiters marking the two regions of a `--emit-cadence` artifact.
///
/// The logic region is nested inside the shell region: the contract opens,
/// the pure `access(self)` helpers are delimited, then the public surface
/// follows. Both regions stay contiguous and non-overlapping.
pub const LOGIC_START: &str = "// CUNI-LOGIC-CORE-START";
pub const LOGIC_END: &str = "// CUNI-LOGIC-CORE-END";
pub const SHELL_START: &str = "// CUNI-CADENCE-SHELL-START";
pub const SHELL_END: &str = "// CUNI-CADENCE-SHELL-END";

/// Suffix of the private logic helper behind each public function.
const IMPL_SUFFIX: &str = "Impl";
/// Builtins that cannot be redefined as CuNi functions.
const CADENCE_BUILTINS: &[&str] = &[
    "say", "range", "len", "abs", "min", "max", "dec_of_int", "int_of_dec",
];
/// Public entry point replaying the top-level `say` statements.
const DRIVER_NAME: &str = "cuniDriver";

/// `transfer_fee` → `transferFee` (Cadence convention is camelCase).
/// The first character of each `_`-separated part is case-normalized by
/// position; the rest of each part keeps its case, so `transferFee` (one
/// part) still maps to `transferFee` and collides honestly with
/// `transfer_fee` instead of slipping past the collision check.
fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut first = true;
    for part in name.split('_') {
        if part.is_empty() {
            continue;
        }
        let mut chars = part.chars();
        let head = chars.next().unwrap();
        if first {
            out.push(head.to_ascii_lowercase());
            first = false;
        } else {
            out.push(head.to_ascii_uppercase());
        }
        out.push_str(chars.as_str());
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}

/// `cuni_demo` → `CuniDemo` (Cadence convention is PascalCase contracts).
fn pascal(name: &str) -> String {
    let c = camel(name);
    let mut chars = c.chars();
    match chars.next() {
        None => "_".to_string(),
        Some(h) => {
            let mut s = h.to_ascii_uppercase().to_string();
            s.push_str(chars.as_str());
            s
        }
    }
}

// Cadence words a user identifier must not shadow: `self` (used for helper
// calls), `log` (used for `say`), literals, and keywords. A colliding name
// gets a `cuni` prefix (`self` → `cuniSelf`), applied uniformly at binding,
// use, and call-label sites.
const CADENCE_RESERVED: &[&str] = &[
    "self", "log", "true", "false", "nil", "fun", "let", "var", "if", "else", "return",
    "contract", "struct", "resource", "access", "import", "init",
];

fn cad_ident(name: &str) -> String {
    let c = camel(name);
    if CADENCE_RESERVED.contains(&c.as_str()) {
        format!("cuni{}", pascal(name))
    } else {
        c
    }
}

/// Map a CuNi type to its Cadence kind.
fn kind_of_type(ty: &Type) -> Result<Kind, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok(Kind::Int),
            "dec" => Ok(Kind::Dec),
            "str" => Ok(Kind::Str),
            "bool" => Ok(Kind::Bool),
            "float" => Err("Cadence has no float mapping here; refusing float".into()),
            other => Err(format!("type `{other}` has no Cadence mapping; refusing")),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{name}` (list/map/opt) has no v1 Cadence form; refusing"
        )),
    }
}

fn cadence_ty(k: Kind) -> &'static str {
    match k {
        Kind::Int | Kind::Dec => "Int256",
        Kind::Bool => "Bool",
        Kind::Str => "String",
    }
}

/// Standalone runnable reference of the pure logic core: Python, run with
/// `python3`; `main` driver prints the `say` outputs. This is what the gate
/// proves byte-identical to CuNi gold.
pub fn generate_reference(program: &Program) -> Result<String, String> {
    let mut g = PyGen::new(program)?;
    g.gen_reference(program)?;
    Ok(g.out)
}

/// Full Cadence contract: the logic core as `access(self)` helpers (clearly
/// delimited, nested inside the contract), plus the `access(all)` public
/// surface (also clearly delimited) that requires the Flow/Cadence
/// toolchain.
///
/// `mod_name` is the `cuni_<stem>` module name from the CLI; the Cadence
/// contract is its PascalCase form (`cuni_demo` → `CuniDemo`).
pub fn generate_program(program: &Program, mod_name: &str) -> Result<String, String> {
    let mut g = Codegen::new(program)?;
    g.gen_program(program, mod_name)?;
    Ok(g.out)
}

// ---------------------------------------------------------------------------
// Shared pre-pass
// ---------------------------------------------------------------------------

/// Reject items with no v1 contract form, and collect the function table.
/// Cadence names are the camelCase conversions; collisions after conversion
/// are refused rather than guessed.
fn collect_fns(
    program: &Program,
) -> Result<(HashSet<String>, HashMap<String, Kind>, HashMap<String, Vec<(String, Kind)>>), String>
{
    let mut fn_names = HashSet::new();
    let mut fn_ret = HashMap::new();
    let mut fn_params = HashMap::new();
    let mut taken: HashMap<String, String> = HashMap::new();
    let mut claim = |cuni_name: &str, cadence_name: String| -> Result<(), String> {
        if cadence_name == DRIVER_NAME {
            return Err(format!(
                "function `{cuni_name}` collides with the reserved driver name `{DRIVER_NAME}`; refusing"
            ));
        }
        if let Some(prev) = taken.insert(cadence_name.clone(), cuni_name.to_string()) {
            return Err(format!(
                "functions `{prev}` and `{cuni_name}` collide as `{cadence_name}`; refusing"
            ));
        }
        Ok(())
    };
    for item in &program.items {
        match item {
            Item::Typ(t) => {
                return Err(format!(
                    "typ `{}` has no v1 Cadence form; refusing (integer/decimal core only)",
                    t.name
                ))
            }
            Item::Enum(e) => {
                return Err(format!(
                    "enum `{}` has no v1 Cadence form; refusing (integer/decimal core only)",
                    e.name
                ))
            }
            Item::Use(u) => {
                return Err(format!("`use {}` has no v1 Cadence form; refusing", u.name))
            }
            Item::Ext(e) => {
                return Err(format!("`ext {}` has no v1 Cadence form; refusing", e.name))
            }
            Item::Iface(i) => {
                return Err(format!("iface `{}` has no v1 Cadence form; refusing", i.name))
            }
            Item::Def(f) => {
                if CADENCE_BUILTINS.contains(&f.name.as_str()) {
                    return Err(format!(
                        "`{}` is a builtin and cannot be redefined; refusing",
                        f.name
                    ));
                }
                let c = cad_ident(&f.name);
                claim(&f.name, c.clone())?;
                claim(&f.name, format!("{c}{IMPL_SUFFIX}"))?;
                let mut params = Vec::new();
                for p in &f.params {
                    params.push((p.name.clone(), kind_of_type(&p.ty)?));
                }
                fn_names.insert(f.name.clone());
                fn_ret.insert(f.name.clone(), kind_of_type(&f.ret_type)?);
                fn_params.insert(f.name.clone(), params);
            }
            Item::Stmt(_) => {}
        }
    }
    Ok((fn_names, fn_ret, fn_params))
}

// ---------------------------------------------------------------------------
// Cadence contract emitter
// ---------------------------------------------------------------------------

pub struct Codegen {
    fn_ret: HashMap<String, Kind>,
    fn_params: HashMap<String, Vec<(String, Kind)>>,
    out: String,
}

impl Codegen {
    fn new(program: &Program) -> Result<Self, String> {
        let (_, fn_ret, fn_params) = collect_fns(program)?;
        Ok(Codegen {
            fn_ret,
            fn_params,
            out: String::new(),
        })
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Escape for a Cadence `"..."` string literal.
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

    /// Escape for a Cadence `"...\\(...)"` string template's literal text.
    fn esc_tpl(s: &str) -> String {
        Self::esc(s)
    }

    fn gen_program(&mut self, program: &Program, mod_name: &str) -> Result<(), String> {
        let contract = pascal(mod_name);
        self.line(0, "/// Generated by the CuNi Cadence backend (\"Trust Provable, in all things.\").");
        self.line(0, "///");
        self.line(0, "/// STRUCTURE — two delimited regions, one law (same logic or refuse):");
        self.line(0, "/// - Logic region (CUNI-LOGIC-CORE-START/END), nested inside the contract:");
        self.line(0, "///   the pure program logic as `access(self)` helpers. `dec` is an");
        self.line(0, "///   `Int256` scaled by 10^4 — deliberately NOT Cadence's native");
        self.line(0, "///   `Fix64` (scale 10^8): CuNi proves the semantics it was given.");
        self.line(0, "///   `say` is `log(...)`. A `dec` logs as its exact scaled integer");
        self.line(0, "///   (12499 for 1.2499); the canonical `1.2499` rendering");
        self.line(0, "///   (docs/DECIMAL.md §6) is proven in the Python reference the gate");
        self.line(0, "///   runs (`--emit-cadence-ref`).");
        self.line(0, "/// - Shell region (CUNI-CADENCE-SHELL-START/END): the `access(all)`");
        self.line(0, "///   contract, one public function per CuNi function delegating to its");
        self.line(0, "///   helper, plus the `cuniDriver` entry replaying the top-level `say`s.");
        self.line(0, "///   Requires the Flow/Cadence toolchain; NOT compiled by `cuni check`.");
        self.line(0, "///");
        self.line(0, "/// HONEST BOUNDARIES: the logic core is gate-proven via the Python");
        self.line(0, "/// reference; the contract shell is NOT compiled here and nothing has");
        self.line(0, "/// executed on-chain. Re-verify Int256 `/`-and-`%`-truncate-toward-zero");
        self.line(0, "/// on the Cadence toolchain before deployment.");
        self.line(0, "/// Full verification matrix: docs/ONCHAIN.md.");
        self.out.push('\n');

        // Shell region opens; the logic region nests inside the contract.
        self.line(0, SHELL_START);
        self.line(0, &format!("access(all) contract {contract} {{"));
        self.out.push('\n');
        self.line(1, LOGIC_START);
        self.line(1, "// Pure logic: `access(self)` helpers. CuNi `int` is `Int256`;");
        self.line(1, "// CuNi `dec` is an `Int256` scaled by 10^4 (see module header).");
        let defs: Vec<&FnDecl> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Def(f) => Some(f),
                _ => None,
            })
            .collect();
        if defs.is_empty() {
            self.line(1, "// (no CuNi functions)");
        } else {
            for f in &defs {
                self.out.push('\n');
                self.gen_def(f)?;
            }
        }
        self.out.push('\n');
        self.line(1, LOGIC_END);
        self.out.push('\n');
        self.line(1, "// Public surface: each function delegates to its logic helper.");
        for f in &defs {
            self.out.push('\n');
            self.gen_public(f)?;
        }
        let driver = Self::driver_stmts(program)?;
        if !driver.is_empty() {
            self.out.push('\n');
            self.gen_driver(&driver)?;
        }
        self.out.push('\n');
        self.line(0, "}");
        self.line(0, SHELL_END);
        Ok(())
    }

    /// Top-level statements: only `say(...)` and bare calls may appear.
    fn driver_stmts<'a>(program: &'a Program) -> Result<Vec<&'a Stmt>, String> {
        let mut out = Vec::new();
        for item in &program.items {
            if let Item::Stmt(s) = item {
                match &s.kind {
                    StmtKind::ExprStmt(_) => out.push(s),
                    StmtKind::Let { name, .. } => {
                        return Err(format!(
                            "top-level `let {name}` has no v1 Cadence contract form; move it into a function"
                        ))
                    }
                    _ => {
                        return Err(
                            "only `say(...)` and bare calls may appear at the top level; refusing"
                                .into(),
                        )
                    }
                }
            }
        }
        Ok(out)
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let c = cad_ident(&f.name);
        let params = self.fn_params.get(&f.name).cloned().unwrap_or_default();
        let pstr = params
            .iter()
            .map(|(n, k)| format!("{}: {}", cad_ident(n), cadence_ty(*k)))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = self.fn_ret[&f.name];
        self.line(
            1,
            &format!(
                "access(self) fun {c}{IMPL_SUFFIX}({pstr}): {} {{",
                cadence_ty(ret)
            ),
        );
        let mut scope: HashMap<String, Kind> =
            params.iter().map(|(n, k)| (n.clone(), *k)).collect();
        for s in &f.body {
            self.gen_stmt(2, s, &mut scope, ret)?;
        }
        self.line(1, "}");
        Ok(())
    }

    fn gen_public(&mut self, f: &FnDecl) -> Result<(), String> {
        let c = cad_ident(&f.name);
        let params = self.fn_params.get(&f.name).cloned().unwrap_or_default();
        let pstr = params
            .iter()
            .map(|(n, k)| format!("{}: {}", cad_ident(n), cadence_ty(*k)))
            .collect::<Vec<_>>()
            .join(", ");
        let args = params
            .iter()
            .map(|(n, _)| format!("{n}: {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        let ret = self.fn_ret[&f.name];
        self.line(
            1,
            &format!("access(all) fun {c}({pstr}): {} {{", cadence_ty(ret)),
        );
        self.line(2, &format!("return self.{c}{IMPL_SUFFIX}({args});"));
        self.line(1, "}");
        Ok(())
    }

    fn gen_driver(&mut self, driver: &[&Stmt]) -> Result<(), String> {
        let scope = HashMap::new();
        self.line(1, "// Driver: replays the top-level `say` statements via `log`.");
        self.line(1, &format!("access(all) fun {DRIVER_NAME}(): Bool {{"));
        for s in driver {
            let e = match &s.kind {
                StmtKind::ExprStmt(e) => e,
                _ => return Err("driver holds only expression statements; refusing".into()),
            };
            let eff = self.effect_expr(e, &scope)?;
            self.line(2, &eff);
        }
        self.line(2, "return true;");
        self.line(1, "}");
        Ok(())
    }

    fn gen_stmt(
        &mut self,
        indent: usize,
        s: &Stmt,
        scope: &mut HashMap<String, Kind>,
        ret: Kind,
    ) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                let k = self.binding_kind(name, ty, value, scope)?;
                let v = self.expr(value, scope)?;
                scope.insert(name.clone(), k);
                self.line(indent, &format!("let {}: {} = {v};", cad_ident(name), cadence_ty(k)));
                Ok(())
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let c = self.cond_expr(cond, scope)?;
                self.line(indent, &format!("if {c} {{"));
                {
                    let mut ts = scope.clone();
                    for s in then_body {
                        self.gen_stmt(indent + 1, s, &mut ts, ret)?;
                    }
                }
                if let Some(els) = else_body {
                    self.line(indent, "} else {");
                    let mut es = scope.clone();
                    for s in els {
                        self.gen_stmt(indent + 1, s, &mut es, ret)?;
                    }
                }
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let k = self.kind(e, scope)?;
                if k != ret {
                    return Err(format!(
                        "`ret` yields `{k:?}` but the function returns `{ret:?}`; refusing"
                    ));
                }
                let v = self.expr(e, scope)?;
                self.line(indent, &format!("return {v};"));
                Ok(())
            }
            StmtKind::Ret(None) => Err("bare `ret` has no v1 Cadence form; return a value".into()),
            StmtKind::ExprStmt(e) => {
                let eff = self.effect_expr(e, scope)?;
                self.line(indent, &eff);
                Ok(())
            }
            StmtKind::Mut { .. } | StmtKind::Assign { .. } => Err(
                "`mut`/`=` have no v1 Cadence contract form; refusing (use `let`)".into(),
            ),
            StmtKind::For { .. } => Err(
                "`for` has no v1 Cadence contract form; refusing (unbounded iteration is not provable here)".into(),
            ),
            StmtKind::Whl { .. } => Err(
                "`while` has no v1 Cadence contract form; refusing (unbounded iteration is not provable here)".into(),
            ),
            StmtKind::Fail(_) => Err(
                "`fail` has no v1 Cadence contract form (the shell returns values, not errors); refusing".into(),
            ),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become a Cadence contract; refusing".into())
            }
        }
    }

    /// `say(x)` → `log(x);`; a bare call → `call;`. Anything else refuses.
    fn effect_expr(
        &mut self,
        e: &Expr,
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                if vals.len() != 1 {
                    return Err("say takes exactly one argument".into());
                }
                // Any of the four kinds may log. A `dec` logs as its exact
                // scaled integer (documented in the module header).
                self.kind(vals[0], scope)?;
                let t = self.expr(vals[0], scope)?;
                return Ok(format!("log({t});"));
            }
            let t = self.call(callee, args, scope)?;
            return Ok(format!("{t};"));
        }
        Err("only `say(...)` and bare calls may appear as statements; refusing".into())
    }

    fn binding_kind(
        &self,
        name: &str,
        ty: &Option<Type>,
        value: &Expr,
        scope: &HashMap<String, Kind>,
    ) -> Result<Kind, String> {
        match ty {
            Some(t) => {
                let k = kind_of_type(t)?;
                let vk = self.kind(value, scope)?;
                if vk != k {
                    return Err(format!(
                        "`let {name}` is annotated `{k:?}` but the value is `{vk:?}`; refusing"
                    ));
                }
                Ok(k)
            }
            None => self
                .kind(value, scope)
                .map_err(|e| format!("cannot infer a Cadence kind for `{name}`: {e}")),
        }
    }

    fn cond_expr(
        &mut self,
        cond: &Expr,
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        if self.kind(cond, scope)? != Kind::Bool {
            return Err("`if` condition must be `bool`; refusing".into());
        }
        self.expr(cond, scope)
    }

    fn kind(&self, e: &Expr, scope: &HashMap<String, Kind>) -> Result<Kind, String> {
        match &e.kind {
            ExprKind::Int(_) => Ok(Kind::Int),
            ExprKind::Dec(_) => Ok(Kind::Dec),
            ExprKind::Bool(_) => Ok(Kind::Bool),
            ExprKind::Str(_) | ExprKind::InterpStr(_) => Ok(Kind::Str),
            ExprKind::Float(_) => Err("float has no Cadence form; refusing".into()),
            ExprKind::Time(_) => Err("time has no Cadence form; refusing".into()),
            ExprKind::NoneLit => Err("None has no Cadence form; refusing".into()),
            ExprKind::Ident(n) => scope
                .get(n)
                .copied()
                .ok_or_else(|| format!("unknown variable `{n}`; refusing")),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Cadence form; refusing".into())
            }
            ExprKind::Index { .. } => Err("indexing has no v1 Cadence form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Cadence form (structs/enums are refused); refusing".into(),
            ),
            ExprKind::Unwrap { .. } => Err("`??` has no Cadence form; refusing".into()),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "dec_of_int" => Ok(Kind::Dec),
                ExprKind::Ident(n) if n == "int_of_dec" => Ok(Kind::Int),
                ExprKind::Ident(n) if n == "say" => {
                    Err("say is a statement, not an expression; refusing".into())
                }
                ExprKind::Ident(n) => self
                    .fn_ret
                    .get(n)
                    .copied()
                    .ok_or_else(|| format!("unknown call `{n}`; refusing")),
                _ => Err("only direct function calls have a Cadence form; refusing".into()),
            },
            ExprKind::Binary { op, lhs, rhs } => {
                let lk = self.kind(lhs, scope)?;
                let rk = self.kind(rhs, scope)?;
                match op {
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::Le
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Ok(Kind::Bool),
                    BinOp::Add if lk == Kind::Str && rk == Kind::Str => Ok(Kind::Str),
                    _ if lk == rk => Ok(lk),
                    _ => Err(format!(
                        "cannot mix `{lk:?}` and `{rk:?}` with `{op:?}` in Cadence; refusing"
                    )),
                }
            }
            ExprKind::Unary { op, expr } => {
                let k = self.kind(expr, scope)?;
                match op {
                    UnOp::Not => {
                        if k == Kind::Bool {
                            Ok(Kind::Bool)
                        } else {
                            Err("`not` needs a `bool`; refusing".into())
                        }
                    }
                    UnOp::Neg => match k {
                        Kind::Int | Kind::Dec => Ok(k),
                        _ => Err("negation needs `int` or `dec`; refusing".into()),
                    },
                }
            }
        }
    }

    /// Canonical rendering of a scaled `dec` literal for the `/* ...dec */` comment.
    fn dec_comment(s: i128) -> String {
        let sign = if s < 0 { "-" } else { "" };
        let n = s.unsigned_abs();
        let ip = n / 10_000;
        let fp = n % 10_000;
        let mut frac = format!("{fp:04}");
        while frac.ends_with('0') && frac.len() > 1 {
            frac.pop();
        }
        format!("{sign}{ip}.{frac}")
    }

    fn expr(&mut self, e: &Expr, scope: &HashMap<String, Kind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(n.to_string()),
            // A `dec` literal is already scaled by 10^4 (docs/DECIMAL.md §2).
            ExprKind::Dec(s) => Ok(format!("{s} /* {}dec */", Self::dec_comment(*s))),
            ExprKind::Float(_) => Err("float literals have no Cadence form; refusing".into()),
            ExprKind::Time(_) => Err("time literals have no Cadence form; refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(parts) => self.interp(parts, scope),
            ExprKind::NoneLit => Err("None has no Cadence form; refusing".into()),
            ExprKind::Ident(n) => {
                if scope.contains_key(n) {
                    Ok(cad_ident(n))
                } else {
                    Err(format!("unknown variable `{n}`; refusing"))
                }
            }
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Cadence form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 Cadence form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Cadence form (structs/enums are refused); refusing".into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, scope),
            ExprKind::Unary { op, expr } => {
                let k = self.kind(expr, scope)?;
                let t = self.expr(expr, scope)?;
                match op {
                    UnOp::Not => {
                        if k == Kind::Bool {
                            Ok(format!("(!{t})"))
                        } else {
                            Err("`not` needs a `bool`; refusing".into())
                        }
                    }
                    UnOp::Neg => match k {
                        Kind::Int | Kind::Dec => Ok(format!("(-{t})")),
                        _ => Err("negation needs `int` or `dec`; refusing".into()),
                    },
                }
            }
            ExprKind::Unwrap { .. } => Err("`??` has no Cadence form; refusing".into()),
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        let lk = self.kind(lhs, scope)?;
        let rk = self.kind(rhs, scope)?;
        let l = self.expr(lhs, scope)?;
        let r = self.expr(rhs, scope)?;
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                match (lk, rk) {
                    (Kind::Int, Kind::Int) => {
                        let s = match op {
                            BinOp::Add => "+",
                            BinOp::Sub => "-",
                            BinOp::Mul => "*",
                            BinOp::Div => "/",
                            BinOp::Mod => "%",
                            _ => unreachable!(),
                        };
                        Ok(format!("({l} {s} {r})"))
                    }
                    (Kind::Dec, Kind::Dec) => match op {
                        BinOp::Add => Ok(format!("({l} + {r})")),
                        BinOp::Sub => Ok(format!("({l} - {r})")),
                        // docs/DECIMAL.md §3: trunc(a·b/10000) toward zero.
                        BinOp::Mul => Ok(format!("(({l} * {r}) / {DEC_SCALE})")),
                        // docs/DECIMAL.md §3: trunc(a·10000/b) toward zero.
                        BinOp::Div => Ok(format!("(({l} * {DEC_SCALE}) / {r})")),
                        BinOp::Mod => {
                            Err("`%` is not defined on `dec` (docs/DECIMAL.md §3); refusing".into())
                        }
                        _ => unreachable!(),
                    },
                    (Kind::Str, Kind::Str) if matches!(op, BinOp::Add) => {
                        Ok(format!("{l}.concat({r})"))
                    }
                    _ => Err(format!(
                        "cannot mix `{lk:?}` and `{rk:?}` with `{op:?}` in Cadence; refusing"
                    )),
                }
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if lk != rk {
                    return Err(format!(
                        "cannot compare `{lk:?}` with `{rk:?}` in Cadence; refusing"
                    ));
                }
                // Ordering comparisons only on int/dec; equality on all kinds.
                match op {
                    BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
                        if lk != Kind::Int && lk != Kind::Dec =>
                    {
                        return Err(format!(
                            "ordering comparison needs `int`/`dec` on both sides, found `{lk:?}` and `{rk:?}`; refusing"
                        ))
                    }
                    _ => {}
                }
                let s = match op {
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    BinOp::Ge => ">=",
                    _ => unreachable!(),
                };
                Ok(format!("({l} {s} {r})"))
            }
            BinOp::And | BinOp::Or => {
                if lk == Kind::Bool && rk == Kind::Bool {
                    let s = if matches!(op, BinOp::And) { "&&" } else { "||" };
                    Ok(format!("({l} {s} {r})"))
                } else {
                    Err("`and`/`or` need `bool` on both sides; refusing".into())
                }
            }
        }
    }

    fn interp(
        &mut self,
        parts: &[StrPartExpr],
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        let mut out = String::new();
        for p in parts {
            match p {
                StrPartExpr::Text(t) => out.push_str(&Self::esc_tpl(t)),
                StrPartExpr::Expr(e) => {
                    // A `dec` interpolates as its exact scaled integer
                    // (documented in the module header).
                    self.kind(e, scope)?;
                    let t = self.expr(e, scope)?;
                    out.push_str(&format!("\\({t})"));
                }
            }
        }
        Ok(format!("\"{out}\""))
    }

    fn one_arg(
        &mut self,
        name: &str,
        args: &[CallArg],
        scope: &HashMap<String, Kind>,
        want: Kind,
    ) -> Result<String, String> {
        if args.len() != 1 {
            return Err(format!("`{name}` takes exactly one argument; refusing"));
        }
        let ak = self.kind(args[0].expr(), scope)?;
        if ak != want {
            return Err(format!("`{name}` needs `{want:?}`, found `{ak:?}`; refusing"));
        }
        self.expr(args[0].expr(), scope)
    }

    fn call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a Cadence form; refusing".into()),
        };
        if args.iter().any(|a| matches!(a, CallArg::Named { .. })) {
            return Err("named arguments have no Cadence form; refusing".into());
        }
        match name.as_str() {
            "say" => Err("say is a statement, not an expression; refusing".into()),
            "range" => Err("range() has no Cadence form (`for` is refused); refusing".into()),
            "len" => Err("`len` has no v1 Cadence form; refusing".into()),
            "abs" | "min" | "max" => {
                Err(format!("builtin `{name}` needs a v1 helper; refusing for now"))
            }
            "dec_of_int" => {
                let a = self.one_arg(&name, args, scope, Kind::Int)?;
                Ok(format!("({a} * {DEC_SCALE})"))
            }
            "int_of_dec" => {
                let a = self.one_arg(&name, args, scope, Kind::Dec)?;
                Ok(format!("({a} / {DEC_SCALE})"))
            }
            _ => {
                let params = self
                    .fn_params
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| format!("unknown call `{name}`; refusing"))?;
                if params.len() != args.len() {
                    return Err(format!(
                        "`{name}` takes {} argument(s), found {}; refusing",
                        params.len(),
                        args.len()
                    ));
                }
                let mut vals = Vec::new();
                for (a, (pn, pk)) in args.iter().zip(params.iter()) {
                    let ak = self.kind(a.expr(), scope)?;
                    if ak != *pk {
                        return Err(format!(
                            "`{name}` expects `{pk:?}` for `{pn}`, found `{ak:?}`; refusing"
                        ));
                    }
                    // Cadence calls use argument labels: `f(amount: amount)`.
                    vals.push(format!("{}: {}", cad_ident(pn), self.expr(a.expr(), scope)?));
                }
                Ok(format!(
                    "self.{}{IMPL_SUFFIX}({})",
                    cad_ident(&name),
                    vals.join(", ")
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Python logic-core reference
// ---------------------------------------------------------------------------

/// Names the reference reserves for its helpers and driver.
const PY_RESERVED: &[&str] = &[
    "cuni_div",
    "cuni_mod",
    "render_dec",
    "dec_mul",
    "dec_div",
    "dec_of_int",
    "int_of_dec",
    "main",
    "DEC_SCALE",
];

pub struct PyGen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, Kind>,
    fn_params: HashMap<String, Vec<(String, Kind)>>,
    out: String,
}

impl PyGen {
    fn new(program: &Program) -> Result<Self, String> {
        let (fn_names, fn_ret, fn_params) = collect_fns(program)?;
        for n in fn_names.iter() {
            if PY_RESERVED.contains(&n.as_str()) {
                return Err(format!(
                    "function `{n}` collides with a reserved reference helper name; refusing"
                ));
            }
        }
        Ok(PyGen {
            fn_names,
            fn_ret,
            fn_params,
            out: String::new(),
        })
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Escape for a Python `"..."` literal.
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
        self.line(0, "#!/usr/bin/env python3");
        self.line(0, "\"\"\"Standalone logic-core reference emitted by the CuNi Cadence backend.");
        self.line(0, "");
        self.line(0, "\"Trust Provable, in all things.\"");
        self.line(0, "");
        self.line(
            0,
            "The pure program logic as Python: `int` division and modulo truncate toward",
        );
        self.line(
            0,
            "zero (never Python's floor), `dec` is a scaled `int` (scale 10^4,",
        );
        self.line(
            0,
            "truncation toward zero), and `say` prints canonically (docs/DECIMAL.md",
        );
        self.line(
            0,
            "section 6). Run with `python3`; `main()` prints the `say` outputs. The",
        );
        self.line(0, "gate proves this stdout byte-identical to `cuni run` gold.");
        self.line(0, "\"\"\"");
        self.out.push('\n');
        self.line(0, "DEC_SCALE = 10_000");
        self.out.push('\n');
        self.out.push_str(
            r#"def cuni_div(a, b):
    """CuNi `int` `/` (and scaled `dec` `/`): truncation toward zero."""
    if b == 0:
        raise ZeroDivisionError("cuni: division by zero")
    q = abs(a) // abs(b)
    return q if (a >= 0) == (b >= 0) else -q


def cuni_mod(a, b):
    """CuNi `int` `%`: Python-floored (sign follows the divisor)."""
    if b == 0:
        raise ZeroDivisionError("cuni: modulo by zero")
    return a % b


def render_dec(scaled):
    """Canonical rendering per docs/DECIMAL.md section 6."""
    sign = "-" if scaled < 0 else ""
    n = -scaled if scaled < 0 else scaled
    ip = n // DEC_SCALE
    fp = n % DEC_SCALE
    frac = str(fp).rjust(4, "0").rstrip("0") or "0"
    return "%s%s.%s" % (sign, ip, frac)


def dec_mul(a, b):
    """`dec` `*`: trunc(a*b/10000) toward zero."""
    return cuni_div(a * b, DEC_SCALE)


def dec_div(a, b):
    """`dec` `/`: trunc(a*10000/b) toward zero."""
    return cuni_div(a * DEC_SCALE, b)


def dec_of_int(n):
    """Explicit `int` -> `dec` conversion (docs/DECIMAL.md section 5)."""
    return n * DEC_SCALE


def int_of_dec(d):
    """Explicit `dec` -> `int` conversion, toward zero (docs/DECIMAL.md section 5)."""
    return cuni_div(d, DEC_SCALE)


"#,
        );
        let defs: Vec<&FnDecl> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Def(f) => Some(f),
                _ => None,
            })
            .collect();
        for f in &defs {
            self.gen_def(f)?;
            self.out.push('\n');
        }
        self.line(0, "def main():");
        let mut any = false;
        for item in &program.items {
            if let Item::Stmt(s) = item {
                any = true;
                let scope = HashMap::new();
                self.gen_stmt(1, s, &scope)?;
            }
        }
        if !any {
            self.line(1, "pass");
        }
        self.out.push('\n');
        self.line(0, "if __name__ == \"__main__\":");
        self.line(1, "main()");
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let params = self.fn_params.get(&f.name).cloned().unwrap_or_default();
        let names = params
            .iter()
            .map(|(n, _)| n.clone())
            .collect::<Vec<_>>()
            .join(", ");
        self.line(0, &format!("def {}({}):", f.name, names));
        let mut scope: HashMap<String, Kind> =
            params.iter().map(|(n, k)| (n.clone(), *k)).collect();
        let ret = self.fn_ret[&f.name];
        for s in &f.body {
            self.gen_stmt_full(1, s, &mut scope, ret)?;
        }
        Ok(())
    }

    /// Top-level (driver) statements: only `say(...)` and bare calls.
    fn gen_stmt(
        &mut self,
        indent: usize,
        s: &Stmt,
        scope: &HashMap<String, Kind>,
    ) -> Result<(), String> {
        match &s.kind {
            StmtKind::ExprStmt(e) => self.gen_effect(indent, e, scope),
            _ => Err("only `say(...)` and bare calls may appear at the top level; refusing".into()),
        }
    }

    fn gen_stmt_full(
        &mut self,
        indent: usize,
        s: &Stmt,
        scope: &mut HashMap<String, Kind>,
        ret: Kind,
    ) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                let k = self.binding_kind(name, ty, value, scope)?;
                let v = self.expr(value, scope)?;
                scope.insert(name.clone(), k);
                self.line(indent, &format!("{name} = {v}"));
                Ok(())
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                if self.kind(cond, scope)? != Kind::Bool {
                    return Err("`if` condition must be `bool`; refusing".into());
                }
                let c = self.expr(cond, scope)?;
                self.line(indent, &format!("if {c}:"));
                {
                    let mut ts = scope.clone();
                    for s in then_body {
                        self.gen_stmt_full(indent + 1, s, &mut ts, ret)?;
                    }
                }
                if let Some(els) = else_body {
                    self.line(indent, "else:");
                    let mut es = scope.clone();
                    for s in els {
                        self.gen_stmt_full(indent + 1, s, &mut es, ret)?;
                    }
                }
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let k = self.kind(e, scope)?;
                if k != ret {
                    return Err(format!(
                        "`ret` yields `{k:?}` but the function returns `{ret:?}`; refusing"
                    ));
                }
                let v = self.expr(e, scope)?;
                self.line(indent, &format!("return {v}"));
                Ok(())
            }
            StmtKind::Ret(None) => Err("bare `ret` has no reference form; return a value".into()),
            StmtKind::ExprStmt(e) => self.gen_effect(indent, e, scope),
            StmtKind::Mut { .. } | StmtKind::Assign { .. } => {
                Err("mutation (`mut`/`=`) has no v1 reference form; refusing".into())
            }
            StmtKind::For { .. } => Err("`for` has no v1 reference form; refusing".into()),
            StmtKind::Whl { .. } => Err("`while` has no v1 reference form; refusing".into()),
            StmtKind::Fail(_) => Err("`fail` has no v1 reference form; refusing".into()),
            StmtKind::Todo => Err("`...` placeholder body cannot become a reference; refusing".into()),
        }
    }

    /// `say(x)` → `print` with canonical rendering; a bare call → the call.
    fn gen_effect(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &HashMap<String, Kind>,
    ) -> Result<(), String> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                if vals.len() != 1 {
                    return Err("say takes exactly one argument".into());
                }
                let v = vals[0];
                let k = self.kind(v, scope)?;
                let t = self.expr(v, scope)?;
                // Python prints `True`/`False` for bools, matching `cuni run` gold.
                let out = match k {
                    Kind::Dec => format!("print(render_dec({t}))"),
                    _ => format!("print({t})"),
                };
                self.line(indent, &out);
                return Ok(());
            }
            let t = self.call(callee, args, scope)?;
            self.line(indent, &t);
            return Ok(());
        }
        Err("only `say(...)` and bare calls may appear as statements; refusing".into())
    }

    fn binding_kind(
        &self,
        name: &str,
        ty: &Option<Type>,
        value: &Expr,
        scope: &HashMap<String, Kind>,
    ) -> Result<Kind, String> {
        match ty {
            Some(t) => {
                let k = kind_of_type(t)?;
                let vk = self.kind(value, scope)?;
                if vk != k {
                    return Err(format!(
                        "`let {name}` is annotated `{k:?}` but the value is `{vk:?}`; refusing"
                    ));
                }
                Ok(k)
            }
            None => self
                .kind(value, scope)
                .map_err(|e| format!("cannot infer a reference kind for `{name}`: {e}")),
        }
    }

    fn kind(&self, e: &Expr, scope: &HashMap<String, Kind>) -> Result<Kind, String> {
        match &e.kind {
            ExprKind::Int(_) => Ok(Kind::Int),
            ExprKind::Dec(_) => Ok(Kind::Dec),
            ExprKind::Bool(_) => Ok(Kind::Bool),
            ExprKind::Str(_) | ExprKind::InterpStr(_) => Ok(Kind::Str),
            ExprKind::Float(_) => Err("float has no reference form; refusing".into()),
            ExprKind::Time(_) => Err("time has no reference form; refusing".into()),
            ExprKind::NoneLit => Err("None has no reference form; refusing".into()),
            ExprKind::Ident(n) => scope
                .get(n)
                .copied()
                .ok_or_else(|| format!("unknown variable `{n}`; refusing")),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 reference form; refusing".into())
            }
            ExprKind::Index { .. } => Err("indexing has no v1 reference form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 reference form (structs/enums are refused); refusing".into(),
            ),
            ExprKind::Unwrap { .. } => Err("`??` has no reference form; refusing".into()),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "dec_of_int" => Ok(Kind::Dec),
                ExprKind::Ident(n) if n == "int_of_dec" => Ok(Kind::Int),
                ExprKind::Ident(n) if n == "say" => {
                    Err("say is a statement, not an expression; refusing".into())
                }
                ExprKind::Ident(n) => self
                    .fn_ret
                    .get(n)
                    .copied()
                    .ok_or_else(|| format!("unknown call `{n}`; refusing")),
                _ => Err("only direct function calls have a reference form; refusing".into()),
            },
            ExprKind::Binary { op, lhs, rhs } => {
                let lk = self.kind(lhs, scope)?;
                let rk = self.kind(rhs, scope)?;
                match op {
                    BinOp::Eq
                    | BinOp::Ne
                    | BinOp::Lt
                    | BinOp::Gt
                    | BinOp::Le
                    | BinOp::Ge
                    | BinOp::And
                    | BinOp::Or => Ok(Kind::Bool),
                    BinOp::Add if lk == Kind::Str && rk == Kind::Str => Ok(Kind::Str),
                    _ if lk == rk => Ok(lk),
                    _ => Err(format!(
                        "cannot mix `{lk:?}` and `{rk:?}` with `{op:?}`; refusing"
                    )),
                }
            }
            ExprKind::Unary { op, expr } => {
                let k = self.kind(expr, scope)?;
                match op {
                    UnOp::Not => {
                        if k == Kind::Bool {
                            Ok(Kind::Bool)
                        } else {
                            Err("`not` needs a `bool`; refusing".into())
                        }
                    }
                    UnOp::Neg => match k {
                        Kind::Int | Kind::Dec => Ok(k),
                        _ => Err("negation needs `int` or `dec`; refusing".into()),
                    },
                }
            }
        }
    }


    fn expr(&mut self, e: &Expr, scope: &HashMap<String, Kind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(n.to_string()),
            ExprKind::Dec(s) => Ok(s.to_string()),
            ExprKind::Float(_) => Err("float literals have no reference form; refusing".into()),
            ExprKind::Time(_) => Err("time literals have no reference form; refusing".into()),
            ExprKind::Bool(b) => Ok(if *b { "True".into() } else { "False".into() }),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(parts) => self.interp(parts, scope),
            ExprKind::NoneLit => Err("None has no reference form; refusing".into()),
            ExprKind::Ident(n) => {
                if scope.contains_key(n) {
                    Ok(n.clone())
                } else {
                    Err(format!("unknown variable `{n}`; refusing"))
                }
            }
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 reference form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 reference form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 reference form (structs/enums are refused); refusing".into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, scope),
            ExprKind::Unary { op, expr } => {
                let k = self.kind(expr, scope)?;
                let t = self.expr(expr, scope)?;
                match op {
                    UnOp::Not => {
                        if k == Kind::Bool {
                            Ok(format!("(not {t})"))
                        } else {
                            Err("`not` needs a `bool`; refusing".into())
                        }
                    }
                    UnOp::Neg => match k {
                        Kind::Int | Kind::Dec => Ok(format!("(-{t})")),
                        _ => Err("negation needs `int` or `dec`; refusing".into()),
                    },
                }
            }
            ExprKind::Unwrap { .. } => Err("`??` has no reference form; refusing".into()),
        }
    }

    fn binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        let lk = self.kind(lhs, scope)?;
        let rk = self.kind(rhs, scope)?;
        let l = self.expr(lhs, scope)?;
        let r = self.expr(rhs, scope)?;
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                match (lk, rk) {
                    (Kind::Int, Kind::Int) => {
                        let s = match op {
                            BinOp::Add => "+",
                            BinOp::Sub => "-",
                            BinOp::Mul => "*",
                            BinOp::Div => "cuni_div",
                            BinOp::Mod => "cuni_mod",
                            _ => unreachable!(),
                        };
                        if matches!(op, BinOp::Div | BinOp::Mod) {
                            Ok(format!("{s}({l}, {r})"))
                        } else {
                            Ok(format!("({l} {s} {r})"))
                        }
                    }
                    (Kind::Dec, Kind::Dec) => match op {
                        BinOp::Add => Ok(format!("({l} + {r})")),
                        BinOp::Sub => Ok(format!("({l} - {r})")),
                        BinOp::Mul => Ok(format!("dec_mul({l}, {r})")),
                        BinOp::Div => Ok(format!("dec_div({l}, {r})")),
                        BinOp::Mod => {
                            Err("`%` is not defined on `dec` (docs/DECIMAL.md §3); refusing".into())
                        }
                        _ => unreachable!(),
                    },
                    (Kind::Str, Kind::Str) if matches!(op, BinOp::Add) => {
                        Ok(format!("({l} + {r})"))
                    }
                    _ => Err(format!(
                        "cannot mix `{lk:?}` and `{rk:?}` with `{op:?}`; refusing"
                    )),
                }
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if lk != rk {
                    return Err(format!(
                        "cannot compare `{lk:?}` with `{rk:?}`; refusing"
                    ));
                }
                let s = match op {
                    BinOp::Eq => "==",
                    BinOp::Ne => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    BinOp::Ge => ">=",
                    _ => unreachable!(),
                };
                Ok(format!("({l} {s} {r})"))
            }
            BinOp::And | BinOp::Or => {
                if lk == Kind::Bool && rk == Kind::Bool {
                    let s = if matches!(op, BinOp::And) { "and" } else { "or" };
                    Ok(format!("({l} {s} {r})"))
                } else {
                    Err("`and`/`or` need `bool` on both sides; refusing".into())
                }
            }
        }
    }

    fn interp(
        &mut self,
        parts: &[StrPartExpr],
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        let mut fmt = String::new();
        for p in parts {
            match p {
                StrPartExpr::Text(t) => {
                    fmt.push_str(&Self::esc(t).replace('{', "{{").replace('}', "}}"))
                }
                StrPartExpr::Expr(e) => {
                    let k = self.kind(e, scope)?;
                    let t = self.expr(e, scope)?;
                    match k {
                        Kind::Dec => fmt.push_str(&format!("{{render_dec({t})}}")),
                        _ => fmt.push_str(&format!("{{{t}}}")),
                    }
                }
            }
        }
        Ok(format!("f\"{fmt}\""))
    }

    fn one_arg(
        &mut self,
        name: &str,
        args: &[CallArg],
        scope: &HashMap<String, Kind>,
        want: Kind,
    ) -> Result<String, String> {
        if args.len() != 1 {
            return Err(format!("`{name}` takes exactly one argument; refusing"));
        }
        let ak = self.kind(args[0].expr(), scope)?;
        if ak != want {
            return Err(format!("`{name}` needs `{want:?}`, found `{ak:?}`; refusing"));
        }
        self.expr(args[0].expr(), scope)
    }

    fn call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, Kind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a reference form; refusing".into()),
        };
        if args.iter().any(|a| matches!(a, CallArg::Named { .. })) {
            return Err("named arguments have no reference form; refusing".into());
        }
        match name.as_str() {
            "say" => Err("say is a statement, not an expression; refusing".into()),
            "range" => Err("range() has no reference form (`for` is refused); refusing".into()),
            "len" => Err("`len` has no v1 reference form; refusing".into()),
            "abs" | "min" | "max" => {
                Err(format!("builtin `{name}` needs a v1 helper; refusing for now"))
            }
            "dec_of_int" => {
                let a = self.one_arg(&name, args, scope, Kind::Int)?;
                Ok(format!("dec_of_int({a})"))
            }
            "int_of_dec" => {
                let a = self.one_arg(&name, args, scope, Kind::Dec)?;
                Ok(format!("int_of_dec({a})"))
            }
            _ => {
                if !self.fn_names.contains(&name) {
                    return Err(format!("unknown call `{name}`; refusing"));
                }
                let params = self.fn_params.get(&name).cloned().unwrap_or_default();
                if params.len() != args.len() {
                    return Err(format!(
                        "`{name}` takes {} argument(s), found {}; refusing",
                        params.len(),
                        args.len()
                    ));
                }
                let mut vals = Vec::new();
                for (a, (pn, pk)) in args.iter().zip(params.iter()) {
                    let ak = self.kind(a.expr(), scope)?;
                    if ak != *pk {
                        return Err(format!(
                            "`{name}` expects `{pk:?}` for `{pn}`, found `{ak:?}`; refusing"
                        ));
                    }
                    vals.push(self.expr(a.expr(), scope)?);
                }
                Ok(format!("{}({})", name, vals.join(", ")))
            }
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

    #[test]
    fn dec_div_lowers_to_scaled_form() {
        let src =
            "def f(a: dec, b: dec) -> dec do\n ret a / b\nend\nsay(f(7.0dec, 2.0dec))\n";
        let prog = generate_program(&parse_src(src), "cuni_demo").expect("program");
        assert!(
            prog.contains("((a * 10000) / b)"),
            "dec div must use the scaled form, got:\n{prog}"
        );
        assert!(prog.contains("access(self) fun fImpl(a: Int256, b: Int256): Int256 {"));
        assert!(prog.contains("access(all) fun f(a: Int256, b: Int256): Int256 {"));
        assert!(prog.contains("return self.fImpl(a: a, b: b);"));
        assert!(prog.contains("access(all) contract CuniDemo {"));
        assert!(prog.contains(LOGIC_START));
        assert!(prog.contains(SHELL_START));
    }

    #[test]
    fn camel_case_names() {
        let src = "def transfer_fee(amount: int, fee_bps: int) -> int do\n ret amount * fee_bps / 10000\nend\nsay(transfer_fee(1, 2))\n";
        let prog = generate_program(&parse_src(src), "cuni_demo").expect("program");
        assert!(prog.contains("access(self) fun transferFeeImpl(amount: Int256, feeBps: Int256): Int256 {"));
        assert!(prog.contains("access(all) fun transferFee(amount: Int256, feeBps: Int256): Int256 {"));
    }

    #[test]
    fn camel_collision_refuses() {
        // `transfer_fee` and `transferFee` both become `transferFee`.
        let src = "def transfer_fee(a: int) -> int do\n ret a\nend\ndef transferFee(b: int) -> int do\n ret b\nend\nsay(1)\n";
        let err =
            generate_program(&parse_src(src), "cuni_demo").expect_err("collision must refuse");
        assert!(err.contains("collide"), "unexpected message: {err}");
    }

    #[test]
    fn reference_matches_clarity_rendering() {
        // Both emitters' references must render `dec` identically.
        let src = "def f(d: dec) -> dec do\n ret d * 1.5dec\nend\nsay(f(-2.0dec))\n";
        let r = generate_reference(&parse_src(src)).expect("reference");
        assert!(r.contains("def render_dec(scaled):"));
        assert!(r.contains("print(render_dec(f((-20000))))"));
        assert!(r.contains("dec_mul(d, 15000)"));
    }

    #[test]
    fn refuses_float_unwrap_mut_while() {
        for (tag, src) in [
            ("float", "def f() -> int do\n ret 1\nend\nsay(1.5)\n"),
            (
                "unwrap",
                "def f() -> int do\n ret 1\nend\ndef g() -> int do\n ret f() ?? do\n ret 0\nend\nend\nsay(g())\n",
            ),
            (
                "mut",
                "def f() -> int do\n mut x: int = 1\n ret x\nend\nsay(f())\n",
            ),
            (
                "while",
                "def f() -> int do\n whl true do\n say(1)\n end\n ret 1\nend\nsay(f())\n",
            ),
            (
                "dec-mod",
                "def f(d: dec) -> dec do\n ret d % 2.0dec\nend\nsay(f(1.0dec))\n",
            ),
        ] {
            let err = generate_program(&parse_src(src), "cuni_demo")
                .expect_err(&format!("{tag} must refuse"));
            assert!(!err.is_empty(), "{tag}: refusal needs a reason");
            let rerr = generate_reference(&parse_src(src))
                .expect_err(&format!("{tag} reference must refuse"));
            assert!(!rerr.is_empty(), "{tag}: reference refusal needs a reason");
        }
    }
}

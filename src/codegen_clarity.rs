//! Clarity contract backend — Stacks Clarity writer.
//!
//! "Trust Provable, in all things."
//!
//! Emits a genuine Clarity contract shape from CuNi source: a pure logic
//! core (the part CuNi proves) plus the contract shell (`define-public`
//! functions returning `(ok ...)` responses) that wraps it.
//!
//! Two artifacts, one law:
//! - `generate_reference` — the standalone logic-core reference: Python,
//!   run with `python3`; its `main` driver prints the `say` outputs. This is
//!   what the gate proves byte-identical to CuNi gold.
//! - `generate_program` — the full Clarity contract: the pure logic as
//!   `define-private` helpers (clearly delimited), plus the `define-public`
//!   shell (also clearly delimited) that requires the Clarity toolchain.
//!
//! Exactness notes:
//! - CuNi `int` is Clarity `int` (128-bit signed). Clarity's `/` and `mod`
//!   truncate toward zero like CuNi's; division or modulo by zero aborts
//!   loudly at runtime — never a value, never silent. 128-bit overflow
//!   aborts loudly too. (Re-verify the truncate claim on the Clarity
//!   toolchain before deployment — see honest boundaries.)
//! - CuNi `dec` is a scaled Clarity `int` (scale 10^4, docs/DECIMAL.md):
//!   `a + b` / `a - b` on the scaled integers; `a * b` lowers to
//!   `(/ (* a b) 10000)`; `a / b` lowers to `(/ (* a 10000) b)` —
//!   truncation toward zero throughout; `%` on `dec` is refused
//!   (docs/DECIMAL.md §3). `dec_of_int` / `int_of_dec` lower to
//!   `(* n 10000)` / `(/ d 10000)`.
//! - CuNi `str` is `(string-ascii N)` where N is the widest string literal
//!   in the program (minimum 1). `+` on strings is `(concat a b)` — Clarity
//!   tracks the widened length in the type system, so no silent truncation.
//!   Clarity has no int→string conversion, so interpolating a non-`str`
//!   value refuses at emit.
//! - `say` becomes `(print ...)` — the on-chain print event. For `dec`,
//!   `print` shows the exact *scaled* integer (12499 for 1.2499); the
//!   canonical `1.2499` rendering (docs/DECIMAL.md §6) is proven in the
//!   Python reference the gate runs, not in the contract.
//! - v1 statements: `def`, `let`, `if`/`els`, `ret`, and `say` (plus bare
//!   calls) as expression statements. `mut`, `=`, `for`, `whl`, `fail`,
//!   `...`, top-level `let`/`if`, and statement-position `if`/`els` whose
//!   branches `ret` are honestly refused.
//! - `float`, `list`, `map`, `opt`, `??`, `len`, structs, enums, `time`,
//!   and unknown calls are honestly refused: a Clarity function's verifiable
//!   core is integer/decimal math, and the backend will not guess at
//!   mappings it cannot prove.
//!
//! Honest boundaries: the logic core is gate-proven via `generate_reference`
//! (a Python rendering run with `python3` — no Clarity toolchain on the
//! check machine); the contract shell is NOT compiled here and nothing has
//! executed on-chain. See `docs/ONCHAIN.md` for the full verification matrix.

use crate::ast::{
    BinOp, CallArg, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr, Type,
    UnOp, DEC_SCALE,
};
use std::collections::{HashMap, HashSet};

/// Clarity-logic kind of a CuNi value: what a value *is* for codegen.
/// Anything else (float, list, map, opt, time, structs, enums) is refused
/// before a kind is ever assigned.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Int,
    Dec,
    Str,
    Bool,
}

impl Kind {
    /// `(ok ...)` dummy of this kind, for effect-position `if` branches.
    fn ok_zero(self) -> &'static str {
        match self {
            Kind::Int | Kind::Dec => "(ok 0)",
            Kind::Str => "(ok u\"\")",
            Kind::Bool => "(ok false)",
        }
    }
}

/// Delimiters marking the two regions of a `--emit-clarity` artifact.
pub const LOGIC_START: &str = ";; CUNI-LOGIC-CORE-START";
pub const LOGIC_END: &str = ";; CUNI-LOGIC-CORE-END";
pub const SHELL_START: &str = ";; CUNI-CLARITY-SHELL-START";
pub const SHELL_END: &str = ";; CUNI-CLARITY-SHELL-END";

/// Suffix of the private logic helper behind each public entry point.
/// CuNi identifiers never contain `-`, so `{name}-impl` is injective over
/// CuNi names and can never collide with a user-defined name.
const IMPL_SUFFIX: &str = "-impl";
/// Public entry point replaying the top-level `say` statements.
const DRIVER_NAME: &str = "cuni-driver";
/// Builtins that cannot be redefined as CuNi functions.
const CLARITY_BUILTINS: &[&str] = &[
    "say", "range", "len", "abs", "min", "max", "dec_of_int", "int_of_dec",
];

/// `fee_bps` → `fee-bps` (Clarity convention is kebab-case). Case is
/// preserved and `_` → `-` is injective over CuNi identifiers (which never
/// contain `-`), so this cannot introduce a collision.
fn kebab(name: &str) -> String {
    name.chars()
        .map(|c| if c == '_' { '-' } else { c })
        .collect()
}

/// Clarity words a user identifier must not shadow: response constructors,
/// literals, and every special form this backend emits. A colliding name
/// gets a `cuni-` prefix, applied uniformly at binding and use sites —
/// otherwise e.g. a parameter named `ok` would shadow the `(ok ...)`
/// constructor this backend emits.
const CLARITY_RESERVED: &[&str] = &[
    "ok", "err", "true", "false", "none", "let", "if", "begin", "print", "and", "or", "not",
    "is-eq", "concat", "mod",
];

fn clar_ident(name: &str) -> String {
    let k = kebab(name);
    if CLARITY_RESERVED.contains(&k.as_str()) {
        format!("cuni-{k}")
    } else {
        k
    }
}

/// Indent every non-blank line of `s` by `n` levels (4 spaces each).
fn ind(s: &str, n: usize) -> String {
    let pad = "    ".repeat(n);
    s.lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Map a CuNi type to its Clarity kind.
fn kind_of_type(ty: &Type) -> Result<Kind, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok(Kind::Int),
            "dec" => Ok(Kind::Dec),
            "str" => Ok(Kind::Str),
            "bool" => Ok(Kind::Bool),
            "float" => Err("Clarity has no float type; refusing float".into()),
            other => Err(format!("type `{other}` has no Clarity mapping; refusing")),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{name}` (list/map/opt) has no v1 Clarity form; refusing"
        )),
    }
}

fn clarity_ty(k: Kind, str_width: usize) -> String {
    match k {
        Kind::Int | Kind::Dec => "int".to_string(),
        Kind::Bool => "bool".to_string(),
        Kind::Str => format!("(string-ascii {str_width})"),
    }
}

/// A statement list whose last statement is `ret <expr>` — i.e. the block
/// diverges instead of falling through.
fn ends_in_ret(stmts: &[Stmt]) -> bool {
    matches!(
        stmts.last(),
        Some(s) if matches!(s.kind, StmtKind::Ret(Some(_)))
    )
}

/// Standalone runnable reference of the pure logic core: Python, run with
/// `python3`; `main` driver prints the `say` outputs. This is what the gate
/// proves byte-identical to CuNi gold.
pub fn generate_reference(program: &Program) -> Result<String, String> {
    let mut g = PyGen::new(program)?;
    g.gen_reference(program)?;
    Ok(g.out)
}

/// Full Clarity contract: the logic core as `define-private` helpers
/// (clearly delimited), plus the `define-public` shell (also clearly
/// delimited) that requires the Clarity toolchain.
///
/// `mod_name` is the `cuni_<stem>` module name from the CLI; the Clarity
/// contract name is its kebab-case form (`cuni_demo` → `cuni-demo`).
pub fn generate_program(program: &Program, mod_name: &str) -> Result<String, String> {
    let mut g = Codegen::new(program)?;
    g.gen_program(program, mod_name)?;
    Ok(g.out)
}

// ---------------------------------------------------------------------------
// Shared pre-pass
// ---------------------------------------------------------------------------

/// Reject items with no v1 contract form, and collect the function table.
fn collect_fns(
    program: &Program,
) -> Result<(HashSet<String>, HashMap<String, Kind>, HashMap<String, Vec<(String, Kind)>>), String>
{
    let mut fn_names = HashSet::new();
    let mut fn_ret = HashMap::new();
    let mut fn_params = HashMap::new();
    let mut taken = HashSet::new();
    for item in &program.items {
        match item {
            Item::Typ(t) => {
                return Err(format!(
                    "typ `{}` has no v1 Clarity form; refusing (integer/decimal core only)",
                    t.name
                ))
            }
            Item::Enum(e) => {
                return Err(format!(
                    "enum `{}` has no v1 Clarity form; refusing (integer/decimal core only)",
                    e.name
                ))
            }
            Item::Use(u) => {
                return Err(format!(
                    "`use {}` has no v1 Clarity form; refusing",
                    u.name
                ))
            }
            Item::Ext(e) => {
                return Err(format!(
                    "`ext {}` has no v1 Clarity form; refusing",
                    e.name
                ))
            }
            Item::Iface(i) => {
                return Err(format!(
                    "iface `{}` has no v1 Clarity form; refusing",
                    i.name
                ))
            }
            Item::Def(f) => {
                if CLARITY_BUILTINS.contains(&f.name.as_str()) {
                    return Err(format!(
                        "`{}` is a builtin and cannot be redefined; refusing",
                        f.name
                    ));
                }
                let cn = clar_ident(&f.name);
                if cn == DRIVER_NAME {
                    return Err(format!(
                        "function `{}` collides with the reserved driver name `{DRIVER_NAME}`; refusing",
                        f.name
                    ));
                }
                if !taken.insert(cn.clone()) {
                    return Err(format!(
                        "function `{}` collides with another function as `{cn}`; refusing",
                        f.name
                    ));
                }
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

/// Widest string literal in the program (minimum 1): the `(string-ascii N)`
/// width for `str` params.
fn scan_str_width(program: &Program) -> usize {
    fn expr(e: &Expr, w: &mut usize) {
        match &e.kind {
            ExprKind::Str(s) => *w = (*w).max(s.len().max(1)),
            ExprKind::InterpStr(parts) => {
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => *w = (*w).max(t.len().max(1)),
                        StrPartExpr::Expr(e) => expr(e, w),
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                expr(callee, w);
                for a in args {
                    expr(a.expr(), w);
                }
            }
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, w);
                expr(rhs, w);
            }
            ExprKind::Unary { expr: e, .. } => expr(e, w),
            ExprKind::Unwrap { expr: e, handler } => {
                expr(e, w);
                for s in handler {
                    stmt(s, w);
                }
            }
            ExprKind::Index { base, index } => {
                expr(base, w);
                expr(index, w);
            }
            ExprKind::Field { base, .. } => expr(base, w),
            ExprKind::List(es) => {
                for e in es {
                    expr(e, w);
                }
            }
            ExprKind::Map(kvs) => {
                for (k, v) in kvs {
                    expr(k, w);
                    expr(v, w);
                }
            }
            _ => {}
        }
    }
    fn stmt(s: &Stmt, w: &mut usize) {
        match &s.kind {
            StmtKind::Let { value, .. } | StmtKind::Mut { value, .. } => expr(value, w),
            StmtKind::Assign { target, value } => {
                expr(target, w);
                expr(value, w);
            }
            StmtKind::Ret(Some(e)) | StmtKind::Fail(e) => expr(e, w),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                expr(cond, w);
                for s in then_body {
                    stmt(s, w);
                }
                if let Some(b) = else_body {
                    for s in b {
                        stmt(s, w);
                    }
                }
            }
            StmtKind::For { iter, body, .. } => {
                expr(iter, w);
                for s in body {
                    stmt(s, w);
                }
            }
            StmtKind::Whl { cond, body } => {
                expr(cond, w);
                for s in body {
                    stmt(s, w);
                }
            }
            StmtKind::ExprStmt(e) => expr(e, w),
            StmtKind::Ret(None) | StmtKind::Todo => {}
        }
    }
    let mut w = 1usize;
    for item in &program.items {
        match item {
            Item::Def(f) => {
                for s in &f.body {
                    stmt(s, &mut w);
                }
            }
            Item::Stmt(s) => stmt(s, &mut w),
            _ => {}
        }
    }
    w
}

// ---------------------------------------------------------------------------
// Clarity contract emitter
// ---------------------------------------------------------------------------

pub struct Codegen {
    fn_ret: HashMap<String, Kind>,
    fn_params: HashMap<String, Vec<(String, Kind)>>,
    str_width: usize,
    out: String,
}

impl Codegen {
    fn new(program: &Program) -> Result<Self, String> {
        let (_, fn_ret, fn_params) = collect_fns(program)?;
        Ok(Codegen {
            fn_ret,
            fn_params,
            str_width: scan_str_width(program),
            out: String::new(),
        })
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Escape for a Clarity `u"..."` (string-ascii) literal.
    fn esc(s: &str) -> Result<String, String> {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            if !c.is_ascii() {
                return Err(
                    "non-ASCII characters have no `(string-ascii ...)` form; refusing".into(),
                );
            }
            match c {
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                '\r' => r.push_str("\\r"),
                c => r.push(c),
            }
        }
        Ok(r)
    }

    fn gen_program(&mut self, program: &Program, mod_name: &str) -> Result<(), String> {
        let contract = clar_ident(mod_name);
        self.line(0, ";; Generated by the CuNi Clarity backend (\"Trust Provable, in all things.\").");
        self.line(0, ";;");
        self.line(0, ";; STRUCTURE — two delimited regions, one law (same logic or refuse):");
        self.line(0, ";; - Logic region (CUNI-LOGIC-CORE-START/END): the pure program logic as");
        self.line(0, ";;   `define-private` helpers. `dec` is a Clarity `int` scaled by 10^4;");
        self.line(0, ";;   `say` is `(print ...)` — the on-chain print event. A `dec` prints");
        self.line(0, ";;   as its exact scaled integer (12499 for 1.2499); the canonical");
        self.line(0, ";;   `1.2499` rendering (docs/DECIMAL.md §6) is proven in the Python");
        self.line(0, ";;   reference the gate runs (`--emit-clarity-ref`).");
        self.line(0, ";; - Shell region (CUNI-CLARITY-SHELL-START/END): the `define-public`");
        self.line(0, ";;   surface — one `(ok (name-impl ...))` wrapper per CuNi function,");
        self.line(0, ";;   plus the `cuni-driver` entry replaying the top-level `say`s.");
        self.line(0, ";;   Requires the Clarity toolchain; NOT compiled by `cuni check`.");
        self.line(0, ";;");
        self.line(0, ";; HONEST BOUNDARIES: the logic core is gate-proven via the Python");
        self.line(0, ";; reference; the contract shell is NOT compiled here and nothing has");
        self.line(0, ";; executed on-chain. Re-verify `/`-and-`mod`-truncate-toward-zero on");
        self.line(0, ";; the Clarity toolchain before deployment.");
        self.line(0, ";; Full verification matrix: docs/ONCHAIN.md.");
        self.out.push('\n');

        // Region 1: the pure logic as `define-private` helpers.
        self.line(0, LOGIC_START);
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
        self.line(0, LOGIC_END);
        self.out.push('\n');

        // Region 2: the `define-public` shell.
        self.line(0, SHELL_START);
        self.line(0, &format!(";; Contract: {contract}"));
        self.line(
            0,
            ";; Deploy: save this file as `contracts/<name>.clar` in a Clarinet",
        );
        self.line(0, ";; project and run `clarinet check` before any deployment.");
        if defs.is_empty() {
            self.line(0, ";; (no CuNi functions: nothing to expose)");
        } else {
            for f in &defs {
                self.out.push('\n');
                self.gen_public(f)?;
            }
        }
        let driver = Self::driver_stmts(program)?;
        if !driver.is_empty() {
            self.out.push('\n');
            self.gen_driver(&driver)?;
        }
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
                            "top-level `let {name}` has no v1 Clarity contract form; move it into a function"
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
        // Defensive: two params kebabbing to the same name would collide.
        let params = self.fn_params.get(&f.name).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        for (n, _) in &params {
            if !seen.insert(clar_ident(n)) {
                return Err(format!(
                    "parameter `{n}` collides after kebab-case conversion; refusing"
                ));
            }
        }
        let ret = self.fn_ret[&f.name];
        let pstr = params
            .iter()
            .map(|(n, k)| format!("({} {})", clar_ident(n), clarity_ty(*k, self.str_width)))
            .collect::<Vec<_>>()
            .join(" ");
        let mut scope: HashMap<String, Kind> =
            params.iter().map(|(n, k)| (n.clone(), *k)).collect();
        let body = self.block(&f.body, &mut scope, ret, false)?;
        if pstr.is_empty() {
            self.line(0, &format!("(define-private ({}{})", clar_ident(&f.name), IMPL_SUFFIX));
        } else {
            self.line(
                0,
                &format!("(define-private ({}{} {})", clar_ident(&f.name), IMPL_SUFFIX, pstr),
            );
        }
        self.out.push_str(&ind(&body, 1));
        self.out.push('\n');
        self.line(0, ")");
        Ok(())
    }

    fn gen_public(&mut self, f: &FnDecl) -> Result<(), String> {
        let params = self.fn_params.get(&f.name).cloned().unwrap_or_default();
        let pstr = params
            .iter()
            .map(|(n, k)| format!("({} {})", clar_ident(n), clarity_ty(*k, self.str_width)))
            .collect::<Vec<_>>()
            .join(" ");
        let args = params
            .iter()
            .map(|(n, _)| clar_ident(n))
            .collect::<Vec<_>>()
            .join(" ");
        let call = if args.is_empty() {
            format!("({}{})", clar_ident(&f.name), IMPL_SUFFIX)
        } else {
            format!("({}{} {})", clar_ident(&f.name), IMPL_SUFFIX, args)
        };
        if pstr.is_empty() {
            self.line(0, &format!("(define-public ({})", clar_ident(&f.name)));
        } else {
            self.line(0, &format!("(define-public ({} {})", clar_ident(&f.name), pstr));
        }
        self.line(1, &format!("(ok {call})"));
        self.line(0, ")");
        Ok(())
    }

    fn gen_driver(&mut self, driver: &[&Stmt]) -> Result<(), String> {
        let scope = HashMap::new();
        self.line(0, &format!("(define-public ({DRIVER_NAME})"));
        self.line(1, "(begin");
        for s in driver {
            let e = match &s.kind {
                StmtKind::ExprStmt(e) => e,
                _ => return Err("driver holds only expression statements; refusing".into()),
            };
            let eff = self.effect_expr(e, &scope)?;
            self.line(2, &eff);
        }
        self.line(2, "(ok true)");
        self.line(1, ")");
        self.line(0, ")");
        Ok(())
    }

    /// Compile statements to a Clarity expression in value position.
    /// `effect`: a tail `say`/call is allowed, yielding `(ok <zero>)` —
    /// used for `if` branches in statement position.
    fn block(
        &mut self,
        stmts: &[Stmt],
        scope: &mut HashMap<String, Kind>,
        ret: Kind,
        effect: bool,
    ) -> Result<String, String> {
        match stmts.split_first() {
            None => Err("empty statement block has no Clarity value; refusing".into()),
            Some((first, rest)) if rest.is_empty() => self.stmt_tail(first, scope, ret, effect),
            Some((first, rest)) => self.stmt_head(first, rest, scope, ret, effect),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn stmt_head(
        &mut self,
        s: &Stmt,
        rest: &[Stmt],
        scope: &mut HashMap<String, Kind>,
        ret: Kind,
        effect: bool,
    ) -> Result<String, String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                let k = self.binding_kind(name, ty, value, scope)?;
                let v = self.expr(value, scope)?;
                scope.insert(name.clone(), k);
                let tail = self.block(rest, scope, ret, effect)?;
                Ok(format!("(let (({} {}))\n{})", clar_ident(name), v, ind(&tail, 1)))
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let c = self.cond_expr(cond, scope)?;
                match else_body {
                    None if ends_in_ret(then_body) => {
                        // Guard form: the `if` selects between diverging and falling through.
                        let mut ts = scope.clone();
                        let then = self.block(then_body, &mut ts, ret, false)?;
                        let rest_e = self.block(rest, scope, ret, effect)?;
                        Ok(format!("(if {}\n{}\n{})", c, ind(&then, 1), ind(&rest_e, 1)))
                    }
                    None => {
                        // Effect form: run the branch for effect, then continue.
                        let mut ts = scope.clone();
                        let then = self.block(then_body, &mut ts, ret, true)?;
                        let rest_e = self.block(rest, scope, ret, effect)?;
                        let guard =
                            format!("(if {}\n{}\n{})", c, ind(&then, 1), ind(ret.ok_zero(), 1));
                        Ok(format!("(begin\n{}\n{})", ind(&guard, 1), ind(&rest_e, 1)))
                    }
                    Some(els) => {
                        if ends_in_ret(then_body) || ends_in_ret(els) {
                            return Err("`ret` inside an `if`/`els` in statement position has no v1 Clarity form; use a guard (`if c do ret v end`) or make the `if`/`els` the block's last statement".into());
                        }
                        let mut ts = scope.clone();
                        let then = self.block(then_body, &mut ts, ret, true)?;
                        let mut es = scope.clone();
                        let els_e = self.block(els, &mut es, ret, true)?;
                        let rest_e = self.block(rest, scope, ret, effect)?;
                        let ite = format!("(if {}\n{}\n{})", c, ind(&then, 1), ind(&els_e, 1));
                        Ok(format!("(begin\n{}\n{})", ind(&ite, 1), ind(&rest_e, 1)))
                    }
                }
            }
            StmtKind::ExprStmt(e) => {
                let eff = self.effect_expr(e, scope)?;
                let rest_e = self.block(rest, scope, ret, effect)?;
                Ok(format!("(begin\n{}\n{})", ind(&eff, 1), ind(&rest_e, 1)))
            }
            StmtKind::Ret(_) => Err("`ret` before the end of the block has no v1 Clarity form except as an `if` guard (`if c do ret v end`); refusing".into()),
            StmtKind::Mut { .. } | StmtKind::Assign { .. } => {
                Err("mutation (`mut`/`=`) has no v1 Clarity form (Clarity is immutable); refusing".into())
            }
            StmtKind::For { .. } => {
                Err("`for` has no v1 Clarity form (no loops in Clarity); refusing".into())
            }
            StmtKind::Whl { .. } => {
                Err("`while` has no v1 Clarity form (no loops in Clarity); refusing".into())
            }
            StmtKind::Fail(_) => Err(
                "`fail` has no v1 Clarity form (the shell wraps every call in `(ok ...)`); refusing"
                    .into(),
            ),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become a Clarity contract; refusing".into())
            }
        }
    }

    fn stmt_tail(
        &mut self,
        s: &Stmt,
        scope: &mut HashMap<String, Kind>,
        ret: Kind,
        effect: bool,
    ) -> Result<String, String> {
        match &s.kind {
            StmtKind::Ret(Some(e)) => {
                let k = self.kind(e, scope)?;
                if k != ret {
                    return Err(format!(
                        "`ret` yields `{k:?}` but the function returns `{ret:?}`; refusing"
                    ));
                }
                Ok(format!("(ok {})", self.expr(e, scope)?))
            }
            StmtKind::Ret(None) => Err("bare `ret` has no v1 Clarity form; return a value".into()),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let c = self.cond_expr(cond, scope)?;
                match else_body {
                    Some(els) => {
                        let mut ts = scope.clone();
                        let then = self.block(then_body, &mut ts, ret, effect)?;
                        let mut es = scope.clone();
                        let els_e = self.block(els, &mut es, ret, effect)?;
                        Ok(format!("(if {}\n{}\n{})", c, ind(&then, 1), ind(&els_e, 1)))
                    }
                    None if effect => {
                        let mut ts = scope.clone();
                        let then = self.block(then_body, &mut ts, ret, true)?;
                        Ok(format!("(if {}\n{}\n{})", c, ind(&then, 1), ind(ret.ok_zero(), 1)))
                    }
                    None => Err("tail `if` without `els` has no Clarity value; refusing".into()),
                }
            }
            StmtKind::ExprStmt(e) if effect => {
                let eff = self.effect_expr(e, scope)?;
                Ok(format!("(begin\n{}\n{})", ind(&eff, 1), ind(ret.ok_zero(), 1)))
            }
            StmtKind::ExprStmt(_) => {
                Err("a function body must end with `ret` (or `if`/`els`); refusing".into())
            }
            StmtKind::Let { .. } => {
                Err("block ends with `let`; a value is required (`ret`, or `if`/`els`)".into())
            }
            StmtKind::Mut { .. } | StmtKind::Assign { .. } => {
                Err("mutation (`mut`/`=`) has no v1 Clarity form (Clarity is immutable); refusing".into())
            }
            StmtKind::For { .. } => {
                Err("`for` has no v1 Clarity form (no loops in Clarity); refusing".into())
            }
            StmtKind::Whl { .. } => {
                Err("`while` has no v1 Clarity form (no loops in Clarity); refusing".into())
            }
            StmtKind::Fail(_) => Err(
                "`fail` has no v1 Clarity form (the shell wraps every call in `(ok ...)`); refusing"
                    .into(),
            ),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become a Clarity contract; refusing".into())
            }
        }
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
                .map_err(|e| format!("cannot infer a Clarity kind for `{name}`: {e}")),
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

    /// `say(x)` → `(print x)`; a bare call → the call. Anything else refuses.
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
                // Any of the four kinds may print. A `dec` prints as its
                // exact scaled integer (documented in the module header).
                self.kind(vals[0], scope)?;
                let t = self.expr(vals[0], scope)?;
                return Ok(format!("(print {t})"));
            }
            return self.call(callee, args, scope);
        }
        Err("only `say(...)` and bare calls may appear as statements; refusing".into())
    }

    fn kind(&self, e: &Expr, scope: &HashMap<String, Kind>) -> Result<Kind, String> {
        match &e.kind {
            ExprKind::Int(_) => Ok(Kind::Int),
            ExprKind::Dec(_) => Ok(Kind::Dec),
            ExprKind::Bool(_) => Ok(Kind::Bool),
            ExprKind::Str(_) | ExprKind::InterpStr(_) => Ok(Kind::Str),
            ExprKind::Float(_) => Err("float has no Clarity form; refusing".into()),
            ExprKind::Time(_) => Err("time has no Clarity form; refusing".into()),
            ExprKind::NoneLit => Err("None has no Clarity form; refusing".into()),
            ExprKind::Ident(n) => scope
                .get(n)
                .copied()
                .ok_or_else(|| format!("unknown variable `{n}`; refusing")),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Clarity form; refusing".into())
            }
            ExprKind::Index { .. } => Err("indexing has no v1 Clarity form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Clarity form (structs/enums are refused); refusing".into(),
            ),
            ExprKind::Unwrap { .. } => Err("`??` has no Clarity form; refusing".into()),
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
                _ => Err("only direct function calls have a Clarity form; refusing".into()),
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
                        "cannot mix `{lk:?}` and `{rk:?}` with `{op:?}` in Clarity; refusing"
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
            // A `dec` literal is already scaled by 10^4 (docs/DECIMAL.md §2).
            ExprKind::Dec(s) => Ok(s.to_string()),
            ExprKind::Float(_) => Err("float literals have no Clarity form; refusing".into()),
            ExprKind::Time(_) => Err("time literals have no Clarity form; refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("u\"{}\"", Self::esc(s)?)),
            ExprKind::InterpStr(parts) => self.interp(parts, scope),
            ExprKind::NoneLit => Err("None has no Clarity form; refusing".into()),
            ExprKind::Ident(n) => {
                if scope.contains_key(n) {
                    Ok(clar_ident(n))
                } else {
                    Err(format!("unknown variable `{n}`; refusing"))
                }
            }
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Clarity form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 Clarity form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Clarity form (structs/enums are refused); refusing".into(),
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
                        Kind::Int | Kind::Dec => Ok(format!("(- 0 {t})")),
                        _ => Err("negation needs `int` or `dec`; refusing".into()),
                    },
                }
            }
            ExprKind::Unwrap { .. } => Err("`??` has no Clarity form; refusing".into()),
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
                            BinOp::Mod => "mod",
                            _ => unreachable!(),
                        };
                        Ok(format!("({s} {l} {r})"))
                    }
                    (Kind::Dec, Kind::Dec) => match op {
                        BinOp::Add => Ok(format!("(+ {l} {r})")),
                        BinOp::Sub => Ok(format!("(- {l} {r})")),
                        // docs/DECIMAL.md §3: trunc(a·b/10000) toward zero.
                        BinOp::Mul => Ok(format!("(/ (* {l} {r}) {DEC_SCALE})")),
                        // docs/DECIMAL.md §3: trunc(a·10000/b) toward zero.
                        BinOp::Div => Ok(format!("(/ (* {l} {DEC_SCALE}) {r})")),
                        BinOp::Mod => {
                            Err("`%` is not defined on `dec` (docs/DECIMAL.md §3); refusing".into())
                        }
                        _ => unreachable!(),
                    },
                    (Kind::Str, Kind::Str) if matches!(op, BinOp::Add) => {
                        Ok(format!("(concat {l} {r})"))
                    }
                    _ => Err(format!(
                        "cannot mix `{lk:?}` and `{rk:?}` with `{op:?}` in Clarity; refusing"
                    )),
                }
            }
            BinOp::Eq | BinOp::Ne => {
                if lk != rk {
                    return Err(format!(
                        "cannot compare `{lk:?}` with `{rk:?}` in Clarity; refusing"
                    ));
                }
                if matches!(op, BinOp::Eq) {
                    Ok(format!("(is-eq {l} {r})"))
                } else {
                    Ok(format!("(not (is-eq {l} {r}))"))
                }
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => match (lk, rk) {
                (Kind::Int, Kind::Int) | (Kind::Dec, Kind::Dec) => {
                    let s = match op {
                        BinOp::Lt => "<",
                        BinOp::Gt => ">",
                        BinOp::Le => "<=",
                        BinOp::Ge => ">=",
                        _ => unreachable!(),
                    };
                    Ok(format!("({s} {l} {r})"))
                }
                _ => Err(format!(
                    "ordering comparison needs `int`/`dec` on both sides, found `{lk:?}` and `{rk:?}`; refusing"
                )),
            },
            BinOp::And | BinOp::Or => {
                if lk == Kind::Bool && rk == Kind::Bool {
                    let s = if matches!(op, BinOp::And) { "and" } else { "or" };
                    Ok(format!("({s} {l} {r})"))
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
        let mut acc: Option<String> = None;
        for p in parts {
            let t = match p {
                StrPartExpr::Text(t) => format!("u\"{}\"", Self::esc(t)?),
                StrPartExpr::Expr(e) => {
                    if self.kind(e, scope)? != Kind::Str {
                        return Err("interpolating a non-`str` value has no Clarity form (no int→string conversion); refusing".into());
                    }
                    self.expr(e, scope)?
                }
            };
            acc = Some(match acc {
                None => t,
                Some(a) => format!("(concat {a} {t})"),
            });
        }
        acc.ok_or_else(|| "empty string interpolation; refusing".into())
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
            _ => return Err("only direct function calls have a Clarity form; refusing".into()),
        };
        if args.iter().any(|a| matches!(a, CallArg::Named { .. })) {
            return Err("named arguments have no Clarity form; refusing".into());
        }
        match name.as_str() {
            "say" => Err("say is a statement, not an expression; refusing".into()),
            "range" => Err("range() has no Clarity form (`for` is refused); refusing".into()),
            "len" => Err(
                "`len` returns `uint` in Clarity and there is no uint→int conversion; refusing"
                    .into(),
            ),
            "abs" | "min" | "max" => {
                Err(format!("builtin `{name}` needs a v1 helper; refusing for now"))
            }
            "dec_of_int" => {
                let a = self.one_arg(&name, args, scope, Kind::Int)?;
                Ok(format!("(* {a} {DEC_SCALE})"))
            }
            "int_of_dec" => {
                let a = self.one_arg(&name, args, scope, Kind::Dec)?;
                Ok(format!("(/ {a} {DEC_SCALE})"))
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
                    vals.push(self.expr(a.expr(), scope)?);
                }
                Ok(format!(
                    "({}{} {})",
                    clar_ident(&name),
                    IMPL_SUFFIX,
                    vals.join(" ")
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
        self.line(0, "\"\"\"Standalone logic-core reference emitted by the CuNi Clarity backend.");
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
    """CuNi `int` `%`: sign follows the dividend (truncated division)."""
    if b == 0:
        raise ZeroDivisionError("cuni: modulo by zero")
    return a - cuni_div(a, b) * b


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
    fn dec_mul_lowers_to_scaled_form() {
        let src = "def f(a: dec) -> dec do\n ret a * 2.0dec\nend\nsay(f(1.0dec))\n";
        let prog = generate_program(&parse_src(src), "cuni_demo").expect("program");
        assert!(
            prog.contains("(/ (* a 20000) 10000)"),
            "dec mul must use the scaled form, got:\n{prog}"
        );
        assert!(prog.contains("(define-private (f-impl (a int))"));
        assert!(prog.contains("(define-public (f (a int))"));
        assert!(prog.contains("(ok (f-impl a))"));
        assert!(prog.contains(LOGIC_START));
        assert!(prog.contains(SHELL_START));
    }

    #[test]
    fn guard_if_becomes_nested_if() {
        let src =
            "def f(a: int) -> int do\n if a <= 0 do\n ret 2\n end\n ret 0\nend\nsay(f(1))\n";
        let prog = generate_program(&parse_src(src), "cuni_demo").expect("program");
        assert!(prog.contains("(if (<= a 0)"), "guard if missing:\n{prog}");
        assert!(prog.contains("(ok 2)"));
    }

    #[test]
    fn kebab_case_names() {
        let src =
            "def transfer_fee(amount: int, fee_bps: int) -> int do\n ret amount * fee_bps / 10000\nend\nsay(transfer_fee(1, 2))\n";
        let prog = generate_program(&parse_src(src), "cuni_demo").expect("program");
        assert!(prog.contains("(define-private (transfer-fee-impl (amount int) (fee-bps int))"));
        assert!(prog.contains("(define-public (transfer-fee (amount int) (fee-bps int))"));
    }

    #[test]
    fn reference_uses_truncating_helpers_and_canonical_render() {
        let src = "def f(a: int, d: dec) -> dec do\n ret d * 2.0dec + dec_of_int(a / 3)\nend\nsay(f(-7, 1.5dec))\n";
        let r = generate_reference(&parse_src(src)).expect("reference");
        assert!(r.contains("def render_dec(scaled):"), "missing render_dec");
        assert!(r.contains("cuni_div(a, b)"), "int div must truncate");
        assert!(r.contains("print(render_dec(f((-7), 15000)))"));
        assert!(r.contains("dec_mul(d, 20000)"));
    }

    #[test]
    fn refuses_float_unwrap_mut_for() {
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
                "for",
                "def f() -> int do\n for i in range(3) do\n say(i)\n end\n ret 1\nend\nsay(f())\n",
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

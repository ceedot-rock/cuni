//! SQL backend — lowers pure CuNi computation into SQL expressions.
//!
//! SQLite has no stored procedures, so there is no "obvious" place for
//! statements to live. This backend compiles the program to a *sequence of
//! `SELECT` statements*, one per `say`, in program order; everything else —
//! bindings, calls, branches, loops — is compiled away before a single row
//! is produced:
//!
//! - **Values are compile-time.** Every CuNi value is a SQL scalar
//!   expression plus a statically tracked kind (`VKind`). `list` and `typ`
//!   (struct) values ride *inside* SQL as JSON text (`'[4,8,15]'`,
//!   `'{"x":3,"y":4}'`), so they stay first-class scalars and every
//!   statement remains a flat `SELECT`.
//! - **`def` is inlined.** Calls substitute argument values for parameters
//!   and splice the body inline (generics monomorphize for free — types are
//!   erased at inline time). Recursive functions are refused: SQL has no
//!   honest recursion for this shape.
//! - **`for` over a list is unrolled.** Lists are compile-time-known, so
//!   `for i, x in xs` becomes one copy of the body per element with the
//!   index/element substituted. `while` is refused (unbounded iteration has
//!   no mapping), as is `for` over anything without a statically known
//!   length.
//! - **Control flow is `alive`-threaded.** A SQL boolean expression tracks
//!   whether the current program point is reachable; each `say` becomes
//!   `SELECT <v> WHERE <alive>`, so untaken branches and post-`ret` code
//!   produce no rows. `if`/`else` split `alive` with the condition;
//!   bindings that differ across branches merge with `CASE`.
//! - **`opt<T>` and fallible (`?`) results are SQL `NULL`.** `fail` and
//!   `none` both produce `NULL`; `??` in a `let`/`mut` binding becomes an
//!   `IS NULL` check whose handler runs under `alive AND (v IS NULL)` and
//!   must diverge (`ret`/`fail`) — anything else is refused.
//! - **Integers are i64** (SQLite's integer type, matching CuNi). `+`, `-`,
//!   `*` on integers are exact inside i64 range (documented limit: SQLite
//!   silently widens overflowing integer arithmetic to REAL, so programs
//!   near i64 limits are outside this seat's verified domain — the py and
//!   java seats already disagree with each other there too). Integer `/`
//!   (truncated) and `%` (Python-floored, matching the py seat) go through
//!   `CAST(a / b AS INTEGER)`, which is *provably* exact for |a|,|b| < 2^53
//!   (a correctly-rounded quotient cannot mis-truncate there); a literal
//!   zero divisor or a literal operand outside that domain is refused at
//!   emit time. Float `/` and `%` are supported (`%` mirrors CPython's
//!   `float_rem`); a literal zero divisor is refused at emit, while a
//!   *dynamic* zero divisor yields SQLite's `NULL` (documented gap — the py
//!   seat crashes instead, so the gate still FAILs on divergence).
//! - **`say` prints** ints/floats/strings/bools (`True`/`False`, CuNi's
//!   canonical spelling) and `none` as `None` (matching the py seat).
//!   Printing a composite (list/struct/map) is refused — rendering
//!   Python-format nested values in SQL is a divergence farm; print the
//!   fields instead.
//!
//! Dialects: the `sql` seat runs **SQLite** (verified by the gate).
//! `generate_dialect` additionally emits PostgreSQL and MySQL variants for
//! docs/examples through the same centralized JSON/concat helpers. Those
//! variants are *not* executed by any seat — only SQLite is gate-verified —
//! and the header says so.
//!
//! Honest refusals (Err, never wrong SQL): `while`, recursion, maps, enums,
//! `link`, `ext`, stub (`...`) bodies, `??` outside a `let`/`mut` binding,
//! non-diverging `??` handlers, `say` of composites, unknown
//! functions/methods, dynamic-length `for`, and variables bound in only one
//! `if` branch and live afterwards.

use crate::ast::*;
use std::collections::HashMap;

/// SQL dialect for [`generate_dialect`]. The `sql` seat always uses SQLite;
/// the other two are emitted for docs/examples and are not gate-verified.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)] // PostgreSQL/MySQL are public API for docs/examples; the seat runs SQLite.
pub enum Dialect {
    SQLite,
    PostgreSQL,
    MySQL,
}

/// Default seat entry point: SQLite.
pub fn generate(program: &Program) -> Result<String, String> {
    generate_dialect(program, Dialect::SQLite)
}

pub fn generate_dialect(program: &Program, dialect: Dialect) -> Result<String, String> {
    let mut cg = Codegen::new(dialect);
    cg.gen_program(program)?;
    Ok(cg.out)
}

/// Statically tracked kind of a SQL value. Lists/structs are JSON text at
/// runtime; the kind vector records element/field kinds so `say` (bool
/// spelling), indexing, and unrolling stay exact. `Any` is the merge of two
/// disagreeing branch values — operations that need a static kind refuse on
/// it rather than guess.
#[derive(Clone, Debug, PartialEq, Eq)]
enum VKind {
    Int,
    /// CuNi `dec`: scaled INTEGER, scale 10⁴ (docs/DECIMAL.md). Narrow
    /// seat: |scaled| ≤ i64::MAX; literals are range-checked at emit.
    Dec,
    Float,
    Str,
    Bool,
    List(Vec<VKind>),
    Struct(Vec<(String, VKind)>),
    Null,
    Any,
}

/// A CuNi value: a SQL scalar expression plus its static kind.
#[derive(Clone, Debug)]
struct Val {
    sql: String,
    kind: VKind,
}

impl Val {
    fn scalar(sql: String, kind: VKind) -> Self {
        Val { sql, kind }
    }
}

#[derive(Clone)]
struct FnT<'a> {
    params: Vec<(String, &'a Type)>,
    body: &'a [Stmt],
    fallible: bool,
}

struct Codegen<'a> {
    dialect: Dialect,
    fns: HashMap<String, FnT<'a>>,
    typs: HashMap<String, Vec<(String, &'a Type)>>,
    enum_names: HashMap<String, Vec<String>>,
    env: HashMap<String, Val>,
    alive: String,
    dead: bool,
    selects: Vec<String>,
    inline_stack: Vec<String>,
    ret_val: Option<Val>,
    returned: String,
    in_fn: bool,
    fn_fallible: bool,
    out: String,
}

impl<'a> Codegen<'a> {
    fn new(dialect: Dialect) -> Self {
        Codegen {
            dialect,
            fns: HashMap::new(),
            typs: HashMap::new(),
            enum_names: HashMap::new(),
            env: HashMap::new(),
            alive: "1".into(),
            dead: false,
            selects: Vec::new(),
            inline_stack: Vec::new(),
            ret_val: None,
            returned: "0".into(),
            in_fn: false,
            fn_fallible: false,
            out: String::new(),
        }
    }

    // ------------------------------------------------------------------
    // Dialect helpers: every dialect-sensitive SQL shape goes through here.
    // ------------------------------------------------------------------

    fn concat(&self, parts: &[String]) -> String {
        match self.dialect {
            Dialect::MySQL => format!("CONCAT({})", parts.join(", ")),
            _ => {
                if parts.len() == 1 {
                    parts[0].clone()
                } else {
                    parts.join(" || ")
                }
            }
        }
    }

    /// JSON array element access (0-based), cast to the statically known kind.
    fn jget(&self, json: &str, idx: &str, kind: &VKind) -> String {
        match self.dialect {
            Dialect::SQLite => format!("json_extract({json}, '$[{idx}]')"),
            Dialect::MySQL => format!("JSON_EXTRACT({json}, '$[{idx}]')"),
            Dialect::PostgreSQL => {
                // `->>` yields text; cast back to the static kind.
                let base = format!("(({json})::jsonb->>{idx})");
                match kind {
                    VKind::Int => format!("({base})::bigint"),
                    VKind::Float => format!("({base})::double precision"),
                    VKind::Bool => format!("(({base})::boolean)::int"),
                    _ => base,
                }
            }
        }
    }

    /// JSON object field access, cast to the statically known kind.
    fn jfield(&self, json: &str, field: &str, kind: &VKind) -> String {
        match self.dialect {
            Dialect::SQLite => format!("json_extract({json}, '$.{field}')"),
            Dialect::MySQL => format!("JSON_EXTRACT({json}, '$.{field}')"),
            Dialect::PostgreSQL => {
                let base = format!("(({json})::jsonb->>'{field}')");
                match kind {
                    VKind::Int => format!("({base})::bigint"),
                    VKind::Float => format!("({base})::double precision"),
                    VKind::Bool => format!("(({base})::boolean)::int"),
                    _ => base,
                }
            }
        }
    }

    /// Append one value to a JSON array value.
    fn jappend(&self, json: &str, val: &str) -> String {
        match self.dialect {
            Dialect::SQLite => format!("json_insert({json}, '$[#]', {val})"),
            Dialect::MySQL => format!("JSON_ARRAY_APPEND({json}, '$', {val})"),
            Dialect::PostgreSQL => format!("(({json})::jsonb || to_jsonb({val}))::text"),
        }
    }

    /// Build a JSON array value from SQL value expressions.
    fn jarray(&self, vals: &[String]) -> String {
        match self.dialect {
            Dialect::SQLite => format!("json_array({})", vals.join(", ")),
            Dialect::MySQL => format!("JSON_ARRAY({})", vals.join(", ")),
            Dialect::PostgreSQL => format!("(jsonb_build_array({}))::text", vals.join(", ")),
        }
    }

    /// Build a JSON object value from (field, value-SQL) pairs.
    fn jobject(&self, fields: &[(String, String)]) -> String {
        let pairs: Vec<String> = fields
            .iter()
            .map(|(k, v)| format!("'{k}', {v}"))
            .collect();
        match self.dialect {
            Dialect::SQLite => format!("json_object({})", pairs.join(", ")),
            Dialect::MySQL => format!("JSON_OBJECT({})", pairs.join(", ")),
            Dialect::PostgreSQL => {
                format!("(jsonb_build_object({}))::text", pairs.join(", "))
            }
        }
    }

    /// Set an object field inside a JSON value (for `p.x = v`).
    fn jset_field(&self, json: &str, field: &str, val: &str) -> String {
        match self.dialect {
            Dialect::SQLite => format!("json_set({json}, '$.{field}', {val})"),
            Dialect::MySQL => format!("JSON_SET({json}, '$.{field}', {val})"),
            Dialect::PostgreSQL => {
                format!("(jsonb_set(({json})::jsonb, '{{{field}}}', to_jsonb({val})))::text")
            }
        }
    }

    // ------------------------------------------------------------------
    // Small SQL builders.
    // ------------------------------------------------------------------

    /// SQL string literal with `'` doubling.
    fn lit_str(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }

    /// `(a) AND (b)` with constant folding so statically-dead code stays
    /// syntactically dead (lets the `dead` flag skip emission exactly).
    fn and_sql(a: &str, b: &str) -> String {
        if a == "0" || b == "0" {
            return "0".into();
        }
        if a == "1" {
            return b.to_string();
        }
        if b == "1" {
            return a.to_string();
        }
        format!("({a}) AND ({b})")
    }

    /// Try to constant-fold a binary op on two literal operands. Only safe,
    /// total cases — `/` and `%` are deliberately *not* folded (they carry
    /// zero-divisor guards that must stay dynamic).
    fn fold(op: BinOp, l: &str, r: &str) -> Option<String> {
        match op {
            BinOp::And | BinOp::Or => {
                let lb = Self::bool_lit(l)?;
                let rb = Self::bool_lit(r)?;
                let v = match op {
                    BinOp::And => lb && rb,
                    _ => lb || rb,
                };
                Some(if v { "1".into() } else { "0".into() })
            }
            _ => {
                if let (Ok(a), Ok(b)) = (l.parse::<i64>(), r.parse::<i64>()) {
                    let v: Option<i64> = match op {
                        BinOp::Add => a.checked_add(b),
                        BinOp::Sub => a.checked_sub(b),
                        BinOp::Mul => a.checked_mul(b),
                        BinOp::Eq => return Some(if a == b { "1".into() } else { "0".into() }),
                        BinOp::Ne => return Some(if a != b { "1".into() } else { "0".into() }),
                        BinOp::Lt => return Some(if a < b { "1".into() } else { "0".into() }),
                        BinOp::Gt => return Some(if a > b { "1".into() } else { "0".into() }),
                        BinOp::Le => return Some(if a <= b { "1".into() } else { "0".into() }),
                        BinOp::Ge => return Some(if a >= b { "1".into() } else { "0".into() }),
                        _ => return None,
                    };
                    return v.map(|x| x.to_string());
                }
                if let (Ok(a), Ok(b)) = (l.parse::<f64>(), r.parse::<f64>()) {
                    let v: Option<f64> = match op {
                        BinOp::Add => Some(a + b),
                        BinOp::Sub => Some(a - b),
                        BinOp::Mul => Some(a * b),
                        BinOp::Eq => return Some(if a == b { "1".into() } else { "0".into() }),
                        BinOp::Ne => return Some(if a != b { "1".into() } else { "0".into() }),
                        BinOp::Lt => return Some(if a < b { "1".into() } else { "0".into() }),
                        BinOp::Gt => return Some(if a > b { "1".into() } else { "0".into() }),
                        BinOp::Le => return Some(if a <= b { "1".into() } else { "0".into() }),
                        BinOp::Ge => return Some(if a >= b { "1".into() } else { "0".into() }),
                        _ => return None,
                    };
                    // NaN results (0.0/0.0 can't happen — Div isn't folded;
                    // but 1e308*10 = inf) still round-trip as literals.
                    return v.map(|x| format!("{x}"));
                }
                None
            }
        }
    }

    fn bool_lit(s: &str) -> Option<bool> {
        match s {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }
    }

    fn is_zero_literal(s: &str) -> bool {
        matches!(s.trim(), "0" | "0.0")
    }

    /// Emit-time guard for integer `/` and `%`: a literal zero divisor, or a
    /// literal operand outside the provably-exact |v| < 2^53 domain, is
    /// refused rather than compiled. (Dynamic values outside the domain can
    /// only arise from literals the guard already rejects or from i64
    /// `*`/`+`/`-` overflow — which is outside the exactness domain anyway,
    /// since the py and java seats already disagree with each other there.)
    fn check_int_div_operands(l: &str, r: &str) -> Result<(), String> {
        const LIM: i64 = 9007199254740992; // 2^53
        if Self::is_zero_literal(r) {
            return Err("integer division by zero; refusing".into());
        }
        for (name, s) in [("dividend", l), ("divisor", r)] {
            if let Ok(v) = s.trim().parse::<i64>() {
                if v >= LIM || v <= -LIM {
                    return Err(format!(
                        "integer division {name} `{v}` is outside the SQL seat's provably-exact 2^53 domain; refusing"
                    ));
                }
            }
        }
        Ok(())
    }

    fn is_null_sql(s: &str) -> bool {
        s.trim() == "NULL"
    }

    /// The SQL seat is a narrow (int64) seat (docs/DECIMAL.md §7): a dec
    /// literal whose scaled value falls outside |v| ≤ i64::MAX is refused
    /// at emit, never silently wrapped. `v` is the parser-validated
    /// scaled i128.
    fn check_dec_literal(v: i128) -> Result<(), String> {
        if v > i64::MAX as i128 || v < i64::MIN as i128 {
            return Err(format!(
                "dec literal `{v}` (scaled) is outside the SQL seat's int64 envelope; refusing"
            ));
        }
        Ok(())
    }

    /// Canonical dec rendering as a SQL text expression (docs/DECIMAL.md
    /// §6), via pure string ops — exact for every int64, no REAL division.
    /// `vsql` is a SQL expression evaluating to the scaled INTEGER.
    fn dec_to_text_sql(vsql: &str) -> String {
        // digits = magnitude's decimal digits (no sign, no leading zeros —
        // CAST(int AS TEXT) never emits them).
        let digits = format!(
            "(CASE WHEN ({v}) < 0 THEN substr(CAST(({v}) AS TEXT), 2) ELSE CAST(({v}) AS TEXT) END)",
            v = vsql
        );
        // Last four digits, left-padded to 4: the fractional part.
        let frac4 = format!(
            "(substr('0000' || {d}, length('0000' || {d}) - 3, 4))",
            d = digits
        );
        // Trailing zeros stripped (at least one digit kept).
        let frac = format!(
            "(CASE WHEN rtrim({f}, '0') = '' THEN '0' ELSE rtrim({f}, '0') END)",
            f = frac4
        );
        let intpart = format!(
            "(CASE WHEN length({d}) > 4 THEN substr({d}, 1, length({d}) - 4) ELSE '0' END)",
            d = digits
        );
        format!(
            "((CASE WHEN ({v}) < 0 THEN '-' ELSE '' END) || {i} || '.' || {f})",
            v = vsql,
            i = intpart,
            f = frac
        )
    }

    /// `dec` binary ops: a closed world (docs/DECIMAL.md §3–5). Literal
    /// operands are folded in Rust with checked arithmetic — exact, and a
    /// loud refusal on overflow or division by zero. Dynamic operands use
    /// the CAST(x/y AS INTEGER) truncation trick, provably exact inside
    /// the documented 2^53 domain (same posture as integer `/` above);
    /// values outside it are caught by the cross-seat gate.
    fn eval_dec_binary(&mut self, op: BinOp, l: Val, r: Val) -> Result<Val, String> {
        // The typeck proved both sides dec; this is defense in depth.
        if l.kind != VKind::Dec || r.kind != VKind::Dec {
            return Err(
                "cannot mix dec and non-dec — convert explicitly (`dec_of_int` / `int_of_dec`)".into(),
            );
        }
        // Literal fast path: exact Rust arithmetic, loud refusal.
        if let (Ok(a), Ok(b)) = (l.sql.trim().parse::<i128>(), r.sql.trim().parse::<i128>()) {
            let v: Option<i128> = match op {
                BinOp::Add => a.checked_add(b),
                BinOp::Sub => a.checked_sub(b),
                // trunc(a*b/10000) toward zero.
                BinOp::Mul => a.checked_mul(b).map(|p| p / 10_000),
                // trunc(a*10000/b) toward zero; b == 0 refuses.
                BinOp::Div => {
                    if b == 0 {
                        return Err("dec division by zero; refusing".into());
                    }
                    a.checked_mul(10_000).map(|p| p / b)
                }
                BinOp::Eq => return Ok(Val::scalar(if a == b { "1" } else { "0" }.into(), VKind::Bool)),
                BinOp::Ne => return Ok(Val::scalar(if a != b { "1" } else { "0" }.into(), VKind::Bool)),
                BinOp::Lt => return Ok(Val::scalar(if a < b { "1" } else { "0" }.into(), VKind::Bool)),
                BinOp::Gt => return Ok(Val::scalar(if a > b { "1" } else { "0" }.into(), VKind::Bool)),
                BinOp::Le => return Ok(Val::scalar(if a <= b { "1" } else { "0" }.into(), VKind::Bool)),
                BinOp::Ge => return Ok(Val::scalar(if a >= b { "1" } else { "0" }.into(), VKind::Bool)),
                _ => None,
            };
            match v {
                Some(x) => {
                    if x > i64::MAX as i128 || x < i64::MIN as i128 {
                        return Err("dec arithmetic overflowed the SQL seat's int64 envelope; refusing".into());
                    }
                    return Ok(Val::scalar(x.to_string(), VKind::Dec));
                }
                None => {
                    // checked_* returned None (overflow) on a non-comparison
                    // op, or an unsupported op: refuse loudly.
                    return Err(match op {
                        BinOp::Mod => "`%` is not defined on `dec`; refusing".into(),
                        BinOp::And | BinOp::Or => "`and`/`or` need booleans; refusing".into(),
                        _ => "dec arithmetic overflowed the SQL seat's int64 envelope; refusing".into(),
                    });
                }
            }
        }
        match op {
            BinOp::Add => Ok(Val::scalar(format!("(({}) + ({}))", l.sql, r.sql), VKind::Dec)),
            BinOp::Sub => Ok(Val::scalar(format!("(({}) - ({}))", l.sql, r.sql), VKind::Dec)),
            // trunc(a*b/10000). Documented 2^53 domain (see above).
            BinOp::Mul => Ok(Val::scalar(
                format!("CAST((({}) * ({})) / 10000 AS INTEGER)", l.sql, r.sql),
                VKind::Dec,
            )),
            BinOp::Div => {
                if Self::is_zero_literal(&r.sql) {
                    return Err("dec division by zero; refusing".into());
                }
                // trunc(a*10000/b). Documented 2^53 domain (see above).
                Ok(Val::scalar(
                    format!("CAST((({}) * 10000) / ({}) AS INTEGER)", l.sql, r.sql),
                    VKind::Dec,
                ))
            }
            BinOp::Mod => Err("`%` is not defined on `dec`; refusing".into()),
            BinOp::Eq | BinOp::Ne => {
                let o = if matches!(op, BinOp::Eq) { "=" } else { "<>" };
                Ok(Val::scalar(format!("(({}) {o} ({}))", l.sql, r.sql), VKind::Bool))
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let o = match op {
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    _ => ">=",
                };
                Ok(Val::scalar(format!("(({}) {o} ({}))", l.sql, r.sql), VKind::Bool))
            }
            BinOp::And | BinOp::Or => Err("`and`/`or` need booleans; refusing".into()),
        }
    }

    // ------------------------------------------------------------------
    // Program structure.
    // ------------------------------------------------------------------

    fn gen_program(&mut self, program: &'a Program) -> Result<(), String> {
        let dialect_name = match self.dialect {
            Dialect::SQLite => "SQLite",
            Dialect::PostgreSQL => "PostgreSQL",
            Dialect::MySQL => "MySQL",
        };
        self.out.push_str(&format!(
            "-- Generated by the CuNi SQL backend ({dialect_name} dialect). Do not hand-edit.\n"
        ));
        if self.dialect != Dialect::SQLite {
            self.out.push_str(
                "-- NOTE: only the SQLite dialect is executed by the `sql` seat and\n-- gate-verified; PostgreSQL/MySQL variants are emitted for docs/examples.\n",
            );
        }
        self.out.push_str(
            "-- Each `say` is one SELECT, in program order. Control flow is compiled\n-- into WHERE guards and CASE expressions (compile-or-refuse: unsupported\n-- constructs are rejected at emit time, never silently miscompiled).\n",
        );

        let mut script: Vec<&Stmt> = Vec::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    if f.is_link {
                        return Err(format!(
                            "`link {}` is a network binding; the SQL seat has no HTTP runtime — refusing",
                            f.name
                        ));
                    }
                    if self.fns.contains_key(&f.name) {
                        return Err(format!("duplicate definition of `{}`", f.name));
                    }
                    self.fns.insert(
                        f.name.clone(),
                        FnT {
                            params: f.params.iter().map(|p| (p.name.clone(), &p.ty)).collect(),
                            body: &f.body,
                            fallible: f.fallible,
                        },
                    );
                }
                Item::Typ(t) => {
                    self.typs.insert(
                        t.name.clone(),
                        t.fields.iter().map(|f| (f.name.clone(), &f.ty)).collect(),
                    );
                }
                Item::Enum(e) => {
                    // Registered so `Color.Red` can be refused with a clear
                    // reason at use; an unused enum emits nothing.
                    self.enum_names.insert(
                        e.name.clone(),
                        e.variants.iter().map(|v| v.name.clone()).collect(),
                    );
                    self.out.push_str(&format!(
                        "-- enum {} (payload-free enums have no SQL value mapping)\n",
                        e.name
                    ));
                }
                Item::Iface(i) => {
                    // No runtime meaning on this seat (methods are free
                    // functions in CuNi); a comment, not a lie.
                    self.out.push_str(&format!("-- iface {} (no runtime effect on the SQL seat)\n", i.name));
                }
                Item::Ext(e) => {
                    return Err(format!(
                        "`ext {}` is a per-target native binding; the SQL seat has no `sql:` runtime to call into — refusing",
                        e.name
                    ));
                }
                Item::Use(u) => {
                    // Unreachable: check.rs resolves `use` before codegen.
                    self.out.push_str(&format!("-- use {} (resolved by the front-end)\n", u.name));
                }
                Item::Stmt(s) => script.push(s),
            }
        }

        for s in script {
            self.exec_stmt(s)?;
        }
        for sel in &self.selects {
            self.out.push_str(sel);
            self.out.push('\n');
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Statements.
    // ------------------------------------------------------------------

    fn exec_block(&mut self, stmts: &[Stmt]) -> Result<(), String> {
        for s in stmts {
            if self.dead {
                break;
            }
            self.exec_stmt(s)?;
        }
        Ok(())
    }

    /// `ret`/`fail` inside an inlined function: accumulate
    /// `CASE WHEN <alive-at-ret> AND NOT <already-returned> THEN <v> ELSE
    /// <prev> END` so the *first* dynamic ret wins, then kill `alive`.
    /// Peephole: the first return on the statically-live path needs no CASE.
    fn do_return(&mut self, v: Val) {
        if self.alive == "1" && self.returned == "0" {
            debug_assert!(self.ret_val.is_none());
            self.ret_val = Some(v);
        } else {
            let prev = self
                .ret_val
                .clone()
                .map(|p| p.sql)
                .unwrap_or_else(|| "NULL".into());
            let prev_kind = self.ret_val.clone().map(|p| p.kind);
            let merged = format!(
                "CASE WHEN ({}) AND NOT ({}) THEN {} ELSE {} END",
                self.alive, self.returned, v.sql, prev
            );
            let kind = match prev_kind {
                None => v.kind.clone(),
                Some(pk) => merge_kind(&pk, &v.kind),
            };
            self.ret_val = Some(Val::scalar(merged, kind));
        }
        self.returned = format!("({}) OR ({})", self.returned, self.alive);
        self.alive = "0".into();
        self.dead = true;
    }

    fn exec_stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                if let ExprKind::Unwrap { expr, handler } = &value.kind {
                    self.exec_unwrap(name, ty, expr, handler)?;
                } else {
                    let v = self.eval(value)?;
                    let v = self.coerce_to_decl(v, ty)?;
                    self.env.insert(name.clone(), v);
                }
                Ok(())
            }
            StmtKind::Assign { target, value } => {
                let v = self.eval(value)?;
                match &target.kind {
                    ExprKind::Ident(name) => {
                        self.env.insert(name.clone(), v);
                        Ok(())
                    }
                    ExprKind::Field { base, name } => {
                        // Struct field update: rebuild the JSON object.
                        let b = self.eval(base)?;
                        match &b.kind {
                            VKind::Struct(fields) => {
                                let mut kinds = fields.clone();
                                let fk = v.kind.clone();
                                if let Some((_, k)) = kinds.iter_mut().find(|(n, _)| n == name) {
                                    *k = fk;
                                } else {
                                    return Err(format!(
                                        "assignment to unknown field `{}`; refusing",
                                        name
                                    ));
                                }
                                let sql = self.jset_field(&b.sql, name, &v.sql);
                                self.env.insert(
                                    Self::target_ident(base)?,
                                    Val::scalar(sql, VKind::Struct(kinds)),
                                );
                                Ok(())
                            }
                            _ => Err("field assignment needs a struct target; refusing".into()),
                        }
                    }
                    ExprKind::Index { base, index } => {
                        // List element update via JSON set.
                        let b = self.eval(base)?;
                        let ix = self.eval(index)?;
                        match &b.kind {
                            VKind::List(kinds) => {
                                let sql = match self.dialect {
                                    Dialect::SQLite => {
                                        format!("json_set({}, '$[' || ({}) || ']', {})", b.sql, ix.sql, v.sql)
                                    }
                                    Dialect::MySQL => {
                                        format!("JSON_SET({}, CONCAT('$[', ({}), ']'), {})", b.sql, ix.sql, v.sql)
                                    }
                                    Dialect::PostgreSQL => {
                                        format!(
                                            "(jsonb_set(({})::jsonb, ('{{' || ({}) || '}}')::text[], to_jsonb({})))::text",
                                            b.sql, ix.sql, v.sql
                                        )
                                    }
                                };
                                // Element kind becomes Any unless the index
                                // is a static literal into the known kinds.
                                let mut kinds = kinds.clone();
                                if let Ok(k) = ix.sql.trim().parse::<usize>() {
                                    if k < kinds.len() {
                                        kinds[k] = v.kind.clone();
                                    }
                                } else if !kinds.iter().all(|k| *k == v.kind) {
                                    kinds = kinds.iter().map(|_| VKind::Any).collect();
                                }
                                self.env.insert(
                                    Self::target_ident(base)?,
                                    Val::scalar(sql, VKind::List(kinds)),
                                );
                                Ok(())
                            }
                            _ => Err("index assignment needs a list target; refusing".into()),
                        }
                    }
                    _ => Err("assignment target must be a variable, field, or index; refusing".into()),
                }
            }
            StmtKind::Ret(value) => {
                if !self.in_fn {
                    // Top-level `ret`: the value is evaluated for refusal
                    // checking and discarded (nothing observes main()'s
                    // return on any seat); reachability ends here.
                    if let Some(e) = value {
                        let _ = self.eval(e)?;
                    }
                    self.alive = "0".into();
                    self.dead = true;
                    return Ok(());
                }
                match value {
                    Some(e) => {
                        let v = self.eval(e)?;
                        self.do_return(v);
                    }
                    None => {
                        // Bare `ret`: NULL return (a bare ret in a
                        // value-returning fn is a typeck-level oddity; NULL
                        // is the honest "no value" on this seat).
                        self.do_return(Val::scalar("NULL".into(), VKind::Null));
                    }
                }
                Ok(())
            }
            StmtKind::Fail(e) => {
                let _ = self.eval(e)?; // refusal-checked, message unobservable
                if self.in_fn && self.fn_fallible {
                    self.do_return(Val::scalar("NULL".into(), VKind::Null));
                    Ok(())
                } else {
                    Err("`fail` outside a fallible function has no SQL mapping; refusing".into())
                }
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => self.exec_if(cond, then_body, else_body.as_deref().unwrap_or(&[])),
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => self.exec_for(a, b.as_deref(), iter, body),
            StmtKind::Whl { .. } => Err(
                "unbounded `while` loops have no SQL mapping; refusing (use `for` over a list)"
                    .into(),
            ),
            StmtKind::ExprStmt(e) => {
                // `say(x)` is statement-only (a value use would be py's
                // `None`, which has no honest SQL scalar).
                if let ExprKind::Call { callee, args } = &e.kind {
                    if let ExprKind::Ident(fname) = &callee.kind {
                        if fname == "say" {
                            if args.len() != 1 {
                                return Err("`say` takes exactly one argument; refusing".into());
                            }
                            let v = self.eval(args[0].expr())?;
                            self.emit_say(&v)?;
                            return Ok(());
                        }
                        if fname == "push" {
                            return Err("bare `push` is not a function; refusing (use `xs.push(v)` as a statement)".into());
                        }
                    }
                    // `.push(v)` as a statement: append to the JSON array.
                    if let ExprKind::Field { base, name } = &callee.kind {
                        if name == "push" {
                            return self.exec_push(base, args);
                        }
                    }
                }
                // Any other expression statement: evaluate for
                // refusal-checking, discard the value (CuNi expressions are
                // pure, so this changes nothing observable).
                let _ = self.eval(e)?;
                Ok(())
            }
            StmtKind::Todo => Err(
                "stub (`...`) bodies have no SQL mapping — refusing (write the body or drop the def)"
                    .into(),
            ),
        }
    }

    fn target_ident(base: &Expr) -> Result<String, String> {
        match &base.kind {
            ExprKind::Ident(n) => Ok(n.clone()),
            _ => Err("assignment target must be a plain variable; refusing".into()),
        }
    }

    fn exec_push(&mut self, base: &Expr, args: &[CallArg]) -> Result<(), String> {
        if args.len() != 1 {
            return Err("`.push` takes exactly one argument; refusing".into());
        }
        let name = Self::target_ident(base)?;
        let b = self
            .env
            .get(&name)
            .cloned()
            .ok_or_else(|| format!("unknown variable `{}` in `.push`; refusing", name))?;
        match &b.kind {
            VKind::List(kinds) => {
                let v = self.eval(args[0].expr())?;
                let sql = self.jappend(&b.sql, &v.sql);
                let mut kinds = kinds.clone();
                kinds.push(v.kind.clone());
                self.env.insert(name, Val::scalar(sql, VKind::List(kinds)));
                Ok(())
            }
            _ => Err("`.push` needs a list target; refusing".into()),
        }
    }

    fn exec_if(&mut self, cond: &Expr, then_body: &[Stmt], else_body: &[Stmt]) -> Result<(), String> {
        let c = self.eval(cond)?;
        if !matches!(c.kind, VKind::Bool | VKind::Any) {
            return Err("`if` condition must be a bool; refusing".into());
        }
        // Peepholes: a statically-known condition executes only the live
        // branch — no CASE, no phi-merge, and (importantly) no evaluation of
        // the dead branch, so dead code can never trip a refusal.
        if c.sql == "1" {
            return self.exec_block(then_body);
        }
        if c.sql == "0" {
            return self.exec_block(else_body);
        }
        let outer_alive = self.alive.clone();
        let outer_dead = self.dead;
        let outer_env = self.env.clone();
        // NOTE: ret_val/returned intentionally NOT saved — returns inside
        // branches are real and already condition-tagged by do_return.

        self.alive = Self::and_sql(&outer_alive, &c.sql);
        self.dead = outer_dead || self.alive == "0";
        self.exec_block(then_body)?;
        let then_env = self.env.clone();

        self.env = outer_env.clone();
        self.alive = Self::and_sql(&outer_alive, &format!("(NOT ({}))", c.sql));
        self.dead = outer_dead || self.alive == "0";
        self.exec_block(else_body)?;
        let else_env = self.env.clone();

        // Phi-merge: bindings that differ across branches become CASE.
        let mut merged = outer_env;
        let mut keys: Vec<String> = then_env.keys().chain(else_env.keys()).cloned().collect();
        keys.sort();
        keys.dedup();
        for k in keys {
            let tv = then_env.get(&k);
            let ev = else_env.get(&k);
            let ov = merged.get(&k);
            match (tv, ev) {
                (Some(t), Some(e)) => {
                    if t.sql != e.sql || t.kind != e.kind {
                        let in_outer = ov.is_some();
                        if !in_outer {
                            return Err(format!(
                                "variable `{k}` is bound in only one branch of `if`; the SQL seat needs definite assignment — bind it in both branches or before the `if` (refusing)"
                            ));
                        }
                        let kind = merge_kind(&t.kind, &e.kind);
                        merged.insert(
                            k,
                            Val::scalar(
                                format!("CASE WHEN ({}) THEN {} ELSE {} END", c.sql, t.sql, e.sql),
                                kind,
                            ),
                        );
                    }
                }
                _ => {
                    // Present in exactly one branch and not in outer, or
                    // missing somewhere unexpected — definite-assignment
                    // violation on this seat.
                    if ov.is_none() {
                        return Err(format!(
                            "variable `{k}` is bound in only one branch of `if`; the SQL seat needs definite assignment — bind it in both branches or before the `if` (refusing)"
                        ));
                    }
                }
            }
        }
        self.env = merged;
        self.alive = outer_alive;
        self.dead = outer_dead;
        Ok(())
    }

    fn exec_for(
        &mut self,
        a: &str,
        b: Option<&str>,
        iter: &Expr,
        body: &[Stmt],
    ) -> Result<(), String> {
        let it = self.eval(iter)?;
        let kinds = match &it.kind {
            VKind::List(k) => k.clone(),
            _ => {
                return Err(
                    "`for` needs a list with statically known length on the SQL seat; refusing"
                        .into(),
                )
            }
        };
        for (k, ek) in kinds.iter().enumerate() {
            if self.dead {
                break;
            }
            let elem_sql = self.jget(&it.sql, &k.to_string(), ek);
            match b {
                Some(bname) => {
                    // `for i, x in xs`
                    self.env.insert(a.to_string(), Val::scalar(k.to_string(), VKind::Int));
                    self.env.insert(
                        bname.to_string(),
                        Val::scalar(elem_sql, ek.clone()),
                    );
                }
                None => {
                    self.env
                        .insert(a.to_string(), Val::scalar(elem_sql, ek.clone()));
                }
            }
            self.exec_block(body)?;
        }
        Ok(())
    }

    /// `let name = expr ?? do handler end`. The handler runs under
    /// `alive AND (expr IS NULL)` and must diverge (`ret`/`fail`); the
    /// fallthrough path continues under `alive AND (expr IS NOT NULL)`.
    fn exec_unwrap(
        &mut self,
        name: &str,
        ty: &Option<Type>,
        expr: &Expr,
        handler: &[Stmt],
    ) -> Result<(), String> {
        let e = self.eval(expr)?;
        let outer_alive = self.alive.clone();
        let outer_dead = self.dead;
        let outer_env = self.env.clone();
        // NOTE: ret_val/returned intentionally NOT restored — a `ret` in
        // the handler is a real, condition-tagged return.

        let h_alive = Self::and_sql(&outer_alive, &format!("({} IS NULL)", e.sql));
        self.alive = h_alive;
        self.dead = outer_dead || self.alive == "0";
        self.exec_block(handler)?;
        if !self.dead {
            return Err(format!(
                "the `??` handler for `{name}` must diverge (end in `ret`/`fail`); refusing"
            ));
        }

        self.env = outer_env;
        self.alive = Self::and_sql(&outer_alive, &format!("({} IS NOT NULL)", e.sql));
        self.dead = outer_dead;
        let v = self.coerce_to_decl(e, ty)?;
        self.env.insert(name.to_string(), v);
        Ok(())
    }

    /// Apply a `let`/`mut` annotation's kind when the value's own kind is
    /// less precise (e.g. `NULL` from `none` under `: opt<int>`).
    fn coerce_to_decl(&self, v: Val, ty: &Option<Type>) -> Result<Val, String> {
        let Some(t) = ty else { return Ok(v) };
        let dk = kind_of_type(t)?;
        if v.kind == VKind::Null {
            // `none` under an annotation: NULL is NULL whatever the kind.
            return Ok(Val::scalar("NULL".into(), dk));
        }
        Ok(v)
    }

    fn emit_say(&mut self, v: &Val) -> Result<(), String> {
        let valsql = match &v.kind {
            VKind::Int | VKind::Float | VKind::Str | VKind::Any => v.sql.clone(),
            // A dec renders canonically (docs/DECIMAL.md §6). A static
            // literal folds to its exact spelling at emit; a dynamic value
            // uses pure-SQL string ops (exact for every int64).
            VKind::Dec => match v.sql.trim().parse::<i64>() {
                Ok(n) => format!("'{}'", crate::ast::fmt_dec_scaled(n as i128)),
                Err(_) => Self::dec_to_text_sql(&v.sql),
            },
            // Peephole: a statically-known bool prints its spelling
            // directly — no CASE.
            VKind::Bool if v.sql.trim() == "1" => "'True'".into(),
            VKind::Bool if v.sql.trim() == "0" => "'False'".into(),
            VKind::Bool => format!("CASE WHEN ({}) THEN 'True' ELSE 'False' END", v.sql),
            VKind::Null => "'None'".into(),
            VKind::List(_) | VKind::Struct(_) => {
                return Err("the SQL seat cannot print composite values (list/struct); print their fields instead — refusing".into())
            }
        };
        // `WHERE <alive>` suppresses rows on unreachable paths; when
        // statically live the guard is omitted for readability.
        let stmt = if self.alive == "1" {
            format!("SELECT {valsql};")
        } else {
            format!("SELECT {valsql} WHERE {};", self.alive)
        };
        self.selects.push(stmt);
        Ok(())
    }
}

/// Merge two static kinds (branch phi, return accumulation): identical kinds
/// stay; `NULL` (none/fail) merges into anything; lists/structs merge
/// elementwise; anything else becomes `Any` (operations needing a static
/// kind refuse on it rather than guess).
fn merge_kind(a: &VKind, b: &VKind) -> VKind {
    if a == b {
        return a.clone();
    }
    match (a, b) {
        (VKind::Null, other) | (other, VKind::Null) => other.clone(),
        (VKind::List(x), VKind::List(y)) => {
            let n = x.len().max(y.len());
            VKind::List(
                (0..n)
                    .map(|i| match (x.get(i), y.get(i)) {
                        (Some(p), Some(q)) => merge_kind(p, q),
                        _ => VKind::Any,
                    })
                    .collect(),
            )
        }
        (VKind::Struct(x), VKind::Struct(y))
            if x.len() == y.len() && x.iter().zip(y.iter()).all(|((n, _), (m, _))| n == m) =>
        {
            VKind::Struct(
                x.iter()
                    .zip(y.iter())
                    .map(|((n, p), (_, q))| (n.clone(), merge_kind(p, q)))
                    .collect(),
            )
        }
        _ => VKind::Any,
    }
}

fn kind_of_type(ty: &Type) -> Result<VKind, String> {    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok(VKind::Int),
            // The SQL seat stores dec as a scaled INTEGER (docs/DECIMAL.md
            // §7) — a narrow (int64) seat.
            "dec" => Ok(VKind::Dec),
            "float" => Ok(VKind::Float),
            "str" => Ok(VKind::Str),
            "bool" => Ok(VKind::Bool),
            other => Err(format!("type `{other}` has no SQL mapping; refusing")),
        },
        Type::Generic(n, args) => match n.as_str() {
            "list" => Ok(VKind::List(vec![kind_of_type(&args[0])?])),
            "map" => Err("maps have no SQL mapping; refusing".into()),
            "opt" => kind_of_type(&args[0]),
            other => Err(format!("generic type `{other}` has no SQL mapping; refusing")),
        },
    }
}

impl<'a> Codegen<'a> {
    // ------------------------------------------------------------------
    // Expressions.
    // ------------------------------------------------------------------

    fn eval(&mut self, expr: &Expr) -> Result<Val, String> {
        match &expr.kind {
            ExprKind::Int(n) => Ok(Val::scalar(n.to_string(), VKind::Int)),
            // Scaled dec literal (docs/DECIMAL.md §2). The SQL seat is a
            // narrow (int64) seat: an out-of-range literal is refused at
            // emit, never silently wrapped.
            ExprKind::Dec(s) => {
                Self::check_dec_literal(*s)?;
                Ok(Val::scalar(s.to_string(), VKind::Dec))
            }
            ExprKind::Float(f) => Ok(Val::scalar(format!("{f}"), VKind::Float)),
            ExprKind::Bool(b) => Ok(Val::scalar(
                if *b { "1".into() } else { "0".into() },
                VKind::Bool,
            )),
            ExprKind::Str(s) => Ok(Val::scalar(Self::lit_str(s), VKind::Str)),
            ExprKind::InterpStr(parts) => {
                let mut sql_parts = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => sql_parts.push(Self::lit_str(t)),
                        StrPartExpr::Expr(e) => {
                            let v = self.eval(e)?;
                            sql_parts.push(self.cast_to_text(&v)?);
                        }
                    }
                }
                Ok(Val::scalar(self.concat(&sql_parts), VKind::Str))
            }
            ExprKind::NoneLit => Ok(Val::scalar("NULL".into(), VKind::Null)),
            ExprKind::Ident(name) => {
                if name == "true" {
                    return Ok(Val::scalar("1".into(), VKind::Bool));
                }
                if name == "false" {
                    return Ok(Val::scalar("0".into(), VKind::Bool));
                }
                self.env
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("unknown variable `{name}`; refusing"))
            }
            ExprKind::List(items) => {
                let mut vals = Vec::new();
                let mut kinds = Vec::new();
                for e in items {
                    let v = self.eval(e)?;
                    kinds.push(v.kind.clone());
                    vals.push(v.sql);
                }
                Ok(Val::scalar(self.jarray(&vals), VKind::List(kinds)))
            }
            ExprKind::Map(_) => Err("maps have no SQL mapping; refusing".into()),
            ExprKind::Call { callee, args } => self.eval_call(callee, args),
            ExprKind::Index { base, index } => {
                let b = self.eval(base)?;
                let ix = self.eval(index)?;
                match &b.kind {
                    VKind::List(kinds) => {
                        // Element kind: static literal index into the known
                        // kinds wins; a uniform list degrades gracefully;
                        // anything else is Any (indexing stays dynamic SQL).
                        let ek = ix
                            .sql
                            .trim()
                            .parse::<usize>()
                            .ok()
                            .and_then(|k| kinds.get(k).cloned())
                            .or_else(|| {
                                let mut u = kinds.iter();
                                u.next().filter(|first| kinds.iter().all(|k| k == *first)).cloned()
                            })
                            .unwrap_or(VKind::Any);
                        Ok(Val::scalar(self.jget(&b.sql, &ix.sql, &ek), ek))
                    }
                    VKind::Str => {
                        // py's `"abc"[1]` is `"b"`; substr is 1-based.
                        Ok(Val::scalar(
                            format!("substr({}, ({}), 1)", b.sql, ix.sql),
                            VKind::Str,
                        ))
                    }
                    _ => Err(format!(
                        "indexing needs a list or string target; refusing"
                    )),
                }
            }
            ExprKind::Field { base, name } => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.enum_names.contains_key(base_name) {
                        return Err(format!(
                            "`{base_name}.{name}`: payload-free enums have no SQL value mapping; refusing"
                        ));
                    }
                }
                let b = self.eval(base)?;
                match &b.kind {
                    VKind::Struct(fields) => {
                        let fk = fields
                            .iter()
                            .find(|(n, _)| n == name)
                            .map(|(_, k)| k.clone())
                            .ok_or_else(|| format!("unknown field `{name}`; refusing"))?;
                        Ok(Val::scalar(self.jfield(&b.sql, name, &fk), fk))
                    }
                    _ => Err(format!("field access needs a struct target; refusing")),
                }
            }
            ExprKind::Binary { op, lhs, rhs } => self.eval_binary(*op, lhs, rhs),
            ExprKind::Unary { op, expr } => {
                let v = self.eval(expr)?;
                match op {
                    UnOp::Not => Ok(Val::scalar(format!("(NOT ({}))", v.sql), VKind::Bool)),
                    UnOp::Neg => Ok(Val::scalar(format!("(-({}))", v.sql), v.kind)),
                }
            }
            ExprKind::Unwrap { .. } => Err(
                "`??` outside a `let`/`mut` binding has no SQL shape; refusing (bind it first)"
                    .into(),
            ),
        }
    }

    /// Render any scalar as SQL text for string interpolation. Bool uses
    /// CuNi's `True`/`False` spelling; floats use CAST, which SQLite renders
    /// shortest-round-trip like Python's repr.
    fn cast_to_text(&self, v: &Val) -> Result<String, String> {
        match &v.kind {
            VKind::Int | VKind::Float => Ok(format!("CAST(({}) AS TEXT)", v.sql)),
            VKind::Dec => match v.sql.trim().parse::<i64>() {
                Ok(n) => Ok(format!("'{}'", crate::ast::fmt_dec_scaled(n as i128))),
                Err(_) => Ok(Self::dec_to_text_sql(&v.sql)),
            },
            VKind::Str => Ok(v.sql.clone()),
            VKind::Bool => Ok(format!("CASE WHEN ({}) THEN 'True' ELSE 'False' END", v.sql)),
            VKind::Null => Ok("'None'".into()),
            _ => Err("cannot interpolate a composite value into a string; refusing".into()),
        }
    }

    fn eval_binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Result<Val, String> {
        let l = self.eval(lhs)?;
        let r = self.eval(rhs)?;
        // `dec` is a closed world (docs/DECIMAL.md §3–5): both operands dec,
        // or a loud refusal. Routed before the int/float/str logic below.
        if matches!(l.kind, VKind::Dec) || matches!(r.kind, VKind::Dec) {
            return self.eval_dec_binary(op, l, r);
        }
        let is_num = |k: &VKind| matches!(k, VKind::Int | VKind::Float | VKind::Any);
        let is_str = |k: &VKind| matches!(k, VKind::Str);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                if is_str(&l.kind) || is_str(&r.kind) {
                    if !matches!(op, BinOp::Add) || !is_str(&l.kind) || !is_str(&r.kind) {
                        return Err("only `+` is defined on strings; refusing".into());
                    }
                    return Ok(Val::scalar(self.concat(&[l.sql, r.sql]), VKind::Str));
                }
                if !is_num(&l.kind) || !is_num(&r.kind) {
                    return Err("arithmetic needs numeric operands; refusing".into());
                }
                if let Some(f) = Self::fold(op, &l.sql, &r.sql) {
                    let kind = if l.kind == VKind::Int && r.kind == VKind::Int {
                        VKind::Int
                    } else {
                        VKind::Float
                    };
                    return Ok(Val::scalar(f, kind));
                }
                let o = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    _ => "*",
                };
                // int/int stays INTEGER in SQLite; any float involvement
                // widens to REAL, exactly IEEE double arithmetic.
                let kind = if l.kind == VKind::Int && r.kind == VKind::Int {
                    VKind::Int
                } else if l.kind == VKind::Any || r.kind == VKind::Any {
                    VKind::Any
                } else {
                    VKind::Float
                };
                Ok(Val::scalar(format!("(({}) {o} ({}))", l.sql, r.sql), kind))
            }
            BinOp::Div => {
                if !is_num(&l.kind) || !is_num(&r.kind) {
                    return Err("`/` needs numeric operands; refusing".into());
                }
                if l.kind == VKind::Int && r.kind == VKind::Int {
                    // Truncated integer division via CAST(a / b AS INTEGER),
                    // which is *provably* exact for |a|,|b| < 2^53: a
                    // correctly-rounded double quotient cannot mis-truncate
                    // there (|a - N*b| >= 1 forces distance >= 1/|b| from any
                    // integer). Operands outside that domain, and a literal
                    // zero divisor, are refused at emit time; a *dynamic*
                    // zero divisor yields SQLite's NULL (documented gap —
                    // the py seat crashes instead, so the gate still FAILs
                    // on divergence, just not via a crash).
                    Self::check_int_div_operands(&l.sql, &r.sql)?;
                    return Ok(Val::scalar(
                        format!("CAST(({}) / ({}) AS INTEGER)", l.sql, r.sql),
                        VKind::Int,
                    ));
                }
                if Self::is_zero_literal(&r.sql) {
                    return Err("division by zero; refusing".into());
                }
                Ok(Val::scalar(
                    format!("(({}) / ({}))", l.sql, r.sql),
                    VKind::Float,
                ))
            }
            BinOp::Mod => {
                if !is_num(&l.kind) || !is_num(&r.kind) {
                    return Err("`%` needs numeric operands; refusing".into());
                }
                if l.kind == VKind::Int && r.kind == VKind::Int {
                    // Python-floored `%` (the py seat's operator), via the
                    // truncated quotient: r = a - b*trunc(a/b), then CPython's
                    // float_rem sign adjustment. Same 2^53 exactness domain
                    // and emit-time checks as integer `/` above.
                    Self::check_int_div_operands(&l.sql, &r.sql)?;
                    let q = format!("CAST(({a}) / ({b}) AS INTEGER)", a = l.sql, b = r.sql);
                    let r0 = format!("(({a}) - ({b}) * ({q}))", a = l.sql, b = r.sql, q = q);
                    return Ok(Val::scalar(
                        format!(
                            "(CASE WHEN ({r0}) != 0 AND ((({b}) < 0) != (({r0}) < 0)) THEN (({r0}) + ({b})) ELSE ({r0}) END)",
                            r0 = r0,
                            b = r.sql,
                        ),
                        VKind::Int,
                    ));
                }
                if Self::is_zero_literal(&r.sql) {
                    return Err("modulo by zero; refusing".into());
                }
                // Float `%`: CPython's float_rem shape (truncated remainder,
                // then sign adjustment).
                let q = format!("CAST(({}) / ({}) AS INTEGER)", l.sql, r.sql);
                let r0 = format!("(({}) - ({}) * ({}))", l.sql, r.sql, q);
                Ok(Val::scalar(
                    format!(
                        "(CASE WHEN ({r0}) != 0 AND ((({b}) < 0) != (({r0}) < 0)) THEN (({r0}) + ({b})) ELSE ({r0}) END",
                        r0 = r0,
                        b = r.sql,
                    ),
                    VKind::Float,
                ))
            }
            BinOp::Eq | BinOp::Ne => {
                if matches!(l.kind, VKind::List(_) | VKind::Struct(_))
                    || matches!(r.kind, VKind::List(_) | VKind::Struct(_))
                {
                    return Err("equality on composite values has no SQL mapping; refusing".into());
                }
                // NULL never equals NULL in SQL; the py seat says
                // `none == none` is True, so literal NULLs use IS.
                if Self::is_null_sql(&l.sql) || Self::is_null_sql(&r.sql) {
                    let o = if matches!(op, BinOp::Eq) { "IS" } else { "IS NOT" };
                    return Ok(Val::scalar(format!("(({}) {o} ({}))", l.sql, r.sql), VKind::Bool));
                }
                if let Some(f) = Self::fold(op, &l.sql, &r.sql) {
                    return Ok(Val::scalar(f, VKind::Bool));
                }
                let o = if matches!(op, BinOp::Eq) { "=" } else { "<>" };
                Ok(Val::scalar(format!("(({}) {o} ({}))", l.sql, r.sql), VKind::Bool))
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if let Some(f) = Self::fold(op, &l.sql, &r.sql) {
                    return Ok(Val::scalar(f, VKind::Bool));
                }
                let o = match op {
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    _ => ">=",
                };
                // Strings compare by code point in py; SQLite's BINARY
                // collation orders UTF-8 the same way.
                Ok(Val::scalar(format!("(({}) {o} ({}))", l.sql, r.sql), VKind::Bool))
            }
            BinOp::And | BinOp::Or => {
                if let Some(f) = Self::fold(op, &l.sql, &r.sql) {
                    return Ok(Val::scalar(f, VKind::Bool));
                }
                let o = if matches!(op, BinOp::And) { "AND" } else { "OR" };
                Ok(Val::scalar(format!("(({}) {o} ({}))", l.sql, r.sql), VKind::Bool))
            }
        }
    }

    /// Wave-1 `time.epoch` (docs/STDLIB.md §2) as pure integer SQL.
    /// Howard Hinnant's days_from_civil; every division is on non-negative
    /// operands (valid dates only — the CASE guards the rest), where
    /// SQLite's truncating `/` equals floor division. Invalid dates take the
    /// ELSE branch, which calls json() on a non-JSON string: sqlite3
    /// validates function *names* at prepare time (so a bogus name would
    /// break even valid dates), but json() only fails when *evaluated* —
    /// giving a genuine runtime refusal with a nonzero exit. The engine's
    /// message ("malformed JSON") is generic; the refusal intent is
    /// documented in the argument. Only the SQLite dialect is lowered.
    fn sql_time_epoch(&self, av: &[Val]) -> Result<Val, String> {
        if !matches!(self.dialect, Dialect::SQLite) {
            return Err("`time.epoch` is only lowered for the SQLite dialect; refusing".into());
        }
        let (y, mo, d, h, mi, s) = (
            &av[0].sql, &av[1].sql, &av[2].sql, &av[3].sql, &av[4].sql, &av[5].sql,
        );
        // Days in month, with the Gregorian leap rule.
        let dim = format!(
            "(CASE ({mo}) WHEN 2 THEN (CASE WHEN (({y}) % 4 = 0 AND (({y}) % 100 <> 0 OR ({y}) % 400 = 0)) THEN 29 ELSE 28 END) WHEN 4 THEN 30 WHEN 6 THEN 30 WHEN 9 THEN 30 WHEN 11 THEN 30 ELSE 31 END)",
            y = y, mo = mo
        );
        let valid = format!(
            "(({y}) BETWEEN 1 AND 9999 AND ({mo}) BETWEEN 1 AND 12 AND ({d}) BETWEEN 1 AND {dim} AND ({h}) BETWEEN 0 AND 23 AND ({mi}) BETWEEN 0 AND 59 AND ({s}) BETWEEN 0 AND 59)",
            y = y, mo = mo, d = d, h = h, mi = mi, s = s, dim = dim
        );
        let y0 = format!("(CASE WHEN ({mo}) <= 2 THEN ({y}) - 1 ELSE ({y}) END)", y = y, mo = mo);
        let era = format!("(({y0}) / 400)", y0 = y0);
        let yoe = format!("(({y0}) - ({era}) * 400)", y0 = y0, era = era);
        let mp = format!("((({mo}) + 9) % 12)", mo = mo);
        let doy = format!("((153 * ({mp}) + 2) / 5 + ({d}) - 1)", mp = mp, d = d);
        let doe = format!(
            "(({yoe}) * 365 + ({yoe}) / 4 - ({yoe}) / 100 + ({doy}))",
            yoe = yoe, doy = doy
        );
        let days = format!("(({era}) * 146097 + ({doe}) - 719468)", era = era, doe = doe);
        let epoch = format!(
            "(({days}) * 86400 + ({h}) * 3600 + ({mi}) * 60 + ({s}))",
            days = days, h = h, mi = mi, s = s
        );
        Ok(Val::scalar(
            format!(
                "(CASE WHEN {valid} THEN {epoch} ELSE json('cuni_refusal: time.epoch received an invalid date') END)",
                valid = valid, epoch = epoch
            ),
            VKind::Int,
        ))
    }

    fn eval_call(&mut self, callee: &Expr, args: &[CallArg]) -> Result<Val, String> {
        if let ExprKind::Ident(fname) = &callee.kind {
            match fname.as_str() {
                "say" => {
                    return Err("`say` is a statement, not an expression; refusing".into())
                }
                "range" => {
                    if args.len() != 1 {
                        return Err("`range` takes exactly one argument; refusing".into());
                    }
                    let n = self.eval(args[0].expr())?;
                    // Unrolling needs a compile-time-known length; a
                    // `let`-bound literal still has literal SQL text, so
                    // this covers the realistic cases.
                    let count: i64 = n
                        .sql
                        .trim()
                        .parse()
                        .map_err(|_| "`range()` needs a compile-time-known size on the SQL seat; refusing")?;
                    if count < 0 {
                        return Err("`range()` with a negative size; refusing".into());
                    }
                    let vals: Vec<String> = (0..count).map(|i| i.to_string()).collect();
                    let kinds = vec![VKind::Int; count as usize];
                    return Ok(Val::scalar(self.jarray(&vals), VKind::List(kinds)));
                }
                "abs" => {
                    if args.len() != 1 {
                        return Err("`abs` takes exactly one argument; refusing".into());
                    }
                    let v = self.eval(args[0].expr())?;
                    return Ok(Val::scalar(
                        format!("CASE WHEN ({}) < 0 THEN -({}) ELSE ({}) END", v.sql, v.sql, v.sql),
                        v.kind,
                    ));
                }
                "min" | "max" => {
                    if args.len() != 2 {
                        return Err(format!("`{fname}` takes exactly two arguments; refusing"));
                    }
                    let a = self.eval(args[0].expr())?;
                    let b = self.eval(args[1].expr())?;
                    let o = if fname == "min" { "<=" } else { ">=" };
                    let kind = merge_kind(&a.kind, &b.kind);
                    return Ok(Val::scalar(
                        format!("CASE WHEN (({}) {o} ({})) THEN ({}) ELSE ({}) END", a.sql, b.sql, a.sql, b.sql),
                        kind,
                    ));
                }
                // `dec` explicit conversions (docs/DECIMAL.md §5).
                "dec_of_int" => {
                    if args.len() != 1 {
                        return Err("`dec_of_int` takes exactly one argument; refusing".into());
                    }
                    let v = self.eval(args[0].expr())?;
                    // Literal: exact checked math, loud on overflow.
                    if let Ok(n) = v.sql.trim().parse::<i128>() {
                        let s = n.checked_mul(10_000).ok_or_else(|| {
                            "dec_of_int overflowed the SQL seat's int64 envelope; refusing".to_string()
                        })?;
                        Self::check_dec_literal(s)?;
                        return Ok(Val::scalar(s.to_string(), VKind::Dec));
                    }
                    return Ok(Val::scalar(format!("(({}) * 10000)", v.sql), VKind::Dec));
                }
                "int_of_dec" => {
                    if args.len() != 1 {
                        return Err("`int_of_dec` takes exactly one argument; refusing".into());
                    }
                    let v = self.eval(args[0].expr())?;
                    // Literal: exact (truncates toward zero), loud on overflow.
                    if let Ok(n) = v.sql.trim().parse::<i128>() {
                        let t = n / 10_000;
                        if t > i64::MAX as i128 || t < i64::MIN as i128 {
                            return Err("int_of_dec overflowed the SQL seat's int64 envelope; refusing".into());
                        }
                        return Ok(Val::scalar(t.to_string(), VKind::Int));
                    }
                    // Dynamic: trunc(d/10000) via the CAST trick — exact
                    // inside the documented 2^53 domain (same posture as
                    // integer `/`).
                    return Ok(Val::scalar(
                        format!("CAST(({}) / 10000 AS INTEGER)", v.sql),
                        VKind::Int,
                    ));
                }
                // Wave-1 stdlib (docs/STDLIB.md §4): SQLite core has no
                // SHA-256 (the CLI's sha3() is a different algorithm).
                "sha256" => {
                    return Err(
                        "`sha256` has no SQL form (SQLite core has no SHA-256); refusing".into(),
                    )
                }
                _ => {}
            }
            // Struct constructor: positional or named.
            if let Some(fields) = self.typs.get(fname) {
                let fields = fields.clone();
                let mut pairs: Vec<(String, String)> = Vec::new();
                let mut kinds: Vec<(String, VKind)> = Vec::new();
                if args.iter().all(|a| a.is_named()) && !args.is_empty() {
                    for (fname2, _) in &fields {
                        let found = args.iter().find_map(|a| match a {
                            CallArg::Named { name, value, .. } if name == fname2 => Some(value),
                            _ => None,
                        });
                        match found {
                            Some(v) => {
                                let vv = self.eval(v)?;
                                pairs.push((fname2.clone(), vv.sql.clone()));
                                kinds.push((fname2.clone(), vv.kind.clone()));
                            }
                            None => {
                                return Err(format!(
                                    "named constructor `{fname}` is missing field `{fname2}`; refusing"
                                ))
                            }
                        }
                    }
                } else {
                    if args.len() != fields.len() {
                        return Err(format!(
                            "constructor `{fname}` takes {} fields, got {} arguments; refusing",
                            fields.len(),
                            args.len()
                        ));
                    }
                    for ((fname2, _), a) in fields.iter().zip(args.iter()) {
                        let vv = self.eval(a.expr())?;
                        pairs.push((fname2.clone(), vv.sql.clone()));
                        kinds.push((fname2.clone(), vv.kind.clone()));
                    }
                }
                return Ok(Val::scalar(self.jobject(&pairs), VKind::Struct(kinds)));
            }
            // User function: inline.
            if self.fns.contains_key(fname) {
                let mut avals = Vec::new();
                for a in args {
                    avals.push(self.eval(a.expr())?);
                }
                return self.inline_fn(fname, avals);
            }
            return Err(format!("call of unknown function `{fname}`; refusing"));
        }
        if let ExprKind::Field { base, name } = &callee.kind {
            // Wave-1 stdlib namespaces (docs/STDLIB.md): `json`/`time` are
            // reserved identifiers, so an Ident base here is a namespace.
            if let ExprKind::Ident(ns) = &base.kind {
                if ns == "json" || ns == "time" {
                    match (ns.as_str(), name.as_str()) {
                        ("json", _) => {
                            return Err(
                                "`json` has no SQL form (maps are refused seat-wide); refusing"
                                    .into(),
                            )
                        }
                        ("time", "parts") => {
                            return Err(
                                "`time.parts` returns a map, which has no SQL form; refusing"
                                    .into(),
                            )
                        }
                        ("time", "epoch") => {
                            if args.len() != 6 {
                                return Err(
                                    "`time.epoch` takes exactly six arguments; refusing".into(),
                                );
                            }
                            let av: Vec<Val> = args
                                .iter()
                                .map(|a| self.eval(a.expr()))
                                .collect::<Result<_, _>>()?;
                            return self.sql_time_epoch(&av);
                        }
                        _ => {
                            return Err(format!(
                                "unknown stdlib function `{}.{}`; refusing",
                                ns, name
                            ))
                        }
                    }
                }
            }
            let b = self.eval(base)?;
            match name.as_str() {
                "len" => {
                    if !args.is_empty() {
                        return Err("`.len` takes no arguments; refusing".into());
                    }
                    match &b.kind {
                        VKind::List(kinds) => {
                            return Ok(Val::scalar(kinds.len().to_string(), VKind::Int))
                        }
                        VKind::Str => {
                            return Ok(Val::scalar(format!("length({})", b.sql), VKind::Int))
                        }
                        _ => return Err("`.len` needs a list or string; refusing".into()),
                    }
                }
                "slice" => {
                    if args.len() != 2 {
                        return Err("`.slice` takes exactly two arguments; refusing".into());
                    }
                    let lo = self.eval(args[0].expr())?;
                    let hi = self.eval(args[1].expr())?;
                    match &b.kind {
                        VKind::Str => {
                            // Mirror py's `_cuni_slice` bounds rule, then
                            // substr (1-based).
                            return Ok(Val::scalar(
                                format!(
                                    "CASE WHEN ({lo}) < 0 OR ({hi}) < 0 OR ({lo}) > length({b}) OR ({hi}) > length({b}) OR ({lo}) > ({hi}) THEN '' ELSE substr({b}, ({lo}) + 1, ({hi}) - ({lo})) END",
                                    lo = lo.sql, hi = hi.sql, b = b.sql
                                ),
                                VKind::Str,
                            ));
                        }
                        VKind::List(kinds) => {
                            let a: usize = lo.sql.trim().parse().map_err(|_| {
                                "`.slice` on a list needs compile-time-known bounds on the SQL seat; refusing"
                            })?;
                            let z: usize = hi.sql.trim().parse().map_err(|_| {
                                "`.slice` on a list needs compile-time-known bounds on the SQL seat; refusing"
                            })?;
                            if a > z || z > kinds.len() {
                                return Err("`.slice` bounds out of range; refusing".into());
                            }
                            let vals: Vec<String> = (a..z)
                                .map(|i| self.jget(&b.sql, &i.to_string(), &kinds[i]))
                                .collect();
                            let ks = kinds[a..z].to_vec();
                            return Ok(Val::scalar(self.jarray(&vals), VKind::List(ks)));
                        }
                        _ => return Err("`.slice` needs a list or string; refusing".into()),
                    }
                }
                "push" => {
                    return Err(
                        "`.push` used as an expression has no honest SQL value; refusing — use it as a statement"
                            .into(),
                    )
                }
                // Wave-1 string ops (docs/STDLIB.md §3). Only trim/contains
                // have a static SQL form; split/join build dynamic-length
                // lists, which the seat refuses.
                "split" => {
                    return Err(
                        "`.split` builds a dynamic-length list, which has no static SQL form; refusing"
                            .into(),
                    )
                }
                "join" => {
                    return Err(
                        "`.join` consumes a dynamic-length list, which has no static SQL form; refusing"
                            .into(),
                    )
                }
                "trim" => {
                    if !args.is_empty() {
                        return Err("`.trim` takes no arguments; refusing".into());
                    }
                    if !matches!(b.kind, VKind::Str) {
                        return Err("`.trim` needs a string target; refusing".into());
                    }
                    // ASCII whitespace only (docs/STDLIB.md §3.3): space,
                    // tab, LF, VT, FF, CR — not trim(x)'s space-only default.
                    return Ok(Val::scalar(
                        format!(
                            "trim(({b}), ' ' || char(9, 10, 11, 12, 13))",
                            b = b.sql
                        ),
                        VKind::Str,
                    ));
                }
                "contains" => {
                    if args.len() != 1 {
                        return Err("`.contains` takes exactly one argument; refusing".into());
                    }
                    if !matches!(b.kind, VKind::Str) {
                        return Err("`.contains` needs a string target; refusing".into());
                    }
                    let sub = self.eval(args[0].expr())?;
                    // instr() > 0; instr(s, '') is 1, so contains("") is true.
                    return Ok(Val::scalar(
                        format!("(instr({}, {}) > 0)", b.sql, sub.sql),
                        VKind::Bool,
                    ));
                }
                _ => return Err(format!("unknown method `.{name}()` has no SQL mapping; refusing")),
            }
        }
        Err("computed callee expressions have no SQL mapping; refusing".into())
    }

    /// Inline a `def` body at a call site: bind arguments, execute the body
    /// with fresh reachability, capture the accumulated return.
    fn inline_fn(&mut self, name: &str, args: Vec<Val>) -> Result<Val, String> {
        if self.inline_stack.contains(&name.to_string()) {
            return Err(format!(
                "recursive call of `{name}` has no SQL mapping; refusing"
            ));
        }
        let t = self
            .fns
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("inline_fn of unknown {name}"));
        if args.len() != t.params.len() {
            return Err(format!(
                "`{name}` takes {} arguments, got {}; refusing",
                t.params.len(),
                args.len()
            ));
        }
        // Save caller state.
        let save_env = std::mem::take(&mut self.env);
        let save_alive = std::mem::replace(&mut self.alive, "1".into());
        let save_dead = std::mem::replace(&mut self.dead, false);
        let save_ret = self.ret_val.take();
        let save_returned = std::mem::replace(&mut self.returned, "0".into());
        let save_in_fn = std::mem::replace(&mut self.in_fn, true);
        let save_fallible = std::mem::replace(&mut self.fn_fallible, t.fallible);

        for ((pname, _), aval) in t.params.iter().zip(args.into_iter()) {
            self.env.insert(pname.clone(), aval);
        }
        self.inline_stack.push(name.to_string());
        let r = self.exec_block(t.body);
        self.inline_stack.pop();

        let ret = self.ret_val.take();
        // Restore caller state.
        self.env = save_env;
        self.alive = save_alive;
        self.dead = save_dead;
        self.ret_val = save_ret;
        self.returned = save_returned;
        self.in_fn = save_in_fn;
        self.fn_fallible = save_fallible;

        r?;
        ret.ok_or_else(|| format!("function `{name}` never returns; refusing"))
    }
}

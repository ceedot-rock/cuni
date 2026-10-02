//! Native Rust seat. Tagged `Val` runtime covering the portable core.
//! Compiled with `rustc`.

use crate::ast::*;
use std::collections::HashSet;

pub fn generate(program: &Program) -> String {
    let mut g = Gen {
        out: String::from(
            "#![allow(dead_code, unused_mut, unused_variables, unused_assignments)]\n",
        ) + RT
            + RT_STDLIB,
        fallible: HashSet::new(),
        typs: HashSet::new(),
        enums: Vec::new(),
        in_main: false,
    };
    for item in &program.items {
        match item {
            Item::Def(f) => {
                if f.fallible {
                    g.fallible.insert(f.name.clone());
                }
            }
            Item::Typ(t) => {
                g.typs.insert(t.name.clone());
            }
            Item::Enum(e) => g.enums.push((
                e.name.clone(),
                e.variants.iter().map(|v| v.name.clone()).collect(),
            )),
            _ => {}
        }
    }
    for (name, vars) in &g.enums {
        g.out.push_str(&format!(
            "fn enum_{name}() -> Val {{\n    let mut s = v_struct(\"{name}\");\n"
        ));
        for v in vars {
            g.out.push_str(&format!(
                "    v_set(&mut s, \"{v}\", v_enum(\"{name}\", \"{v}\"));\n"
            ));
        }
        g.out.push_str("    s\n}\n");
    }
    for item in &program.items {
        match item {
            Item::Typ(t) => g.typ(t),
            Item::Def(f) => g.func(f),
            Item::Ext(e) => {
                let ps = e
                    .params
                    .iter()
                    .map(|p| format!("{}: Val", p.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                g.out
                    .push_str(&format!("fn {}({}) -> Val {{ Val::None }}\n", e.name, ps));
            }
            _ => {}
        }
    }
    g.in_main = true;
    g.out.push_str("fn main() {\n");
    for (name, _) in &g.enums {
        g.out
            .push_str(&format!("    let {name} = enum_{name}();\n"));
    }
    for item in &program.items {
        if let Item::Stmt(s) = item {
            g.stmt(s, 1);
        }
    }
    g.out.push_str("}\n");
    g.out
}

struct Gen {
    out: String,
    fallible: HashSet<String>,
    typs: HashSet<String>,
    enums: Vec<(String, Vec<String>)>,
    in_main: bool,
}

impl Gen {
    fn typ(&mut self, t: &TypDecl) {
        let args = t
            .fields
            .iter()
            .map(|f| format!("{}: Val", f.name))
            .collect::<Vec<_>>()
            .join(", ");
        self.out
            .push_str(&format!("fn {}({}) -> Val {{\n", t.name, args));
        self.out
            .push_str(&format!("    let mut s = v_struct(\"{}\");\n", t.name));
        for f in &t.fields {
            self.out
                .push_str(&format!("    v_set(&mut s, \"{}\", {});\n", f.name, f.name));
        }
        self.out.push_str("    s\n}\n");
    }

    fn func(&mut self, f: &FnDecl) {
        let ps = f
            .params
            .iter()
            .map(|p| format!("{}: Val", p.name))
            .collect::<Vec<_>>()
            .join(", ");
        self.out
            .push_str(&format!("fn {}({}) -> Val {{\n", f.name, ps));
        for s in &f.body {
            self.stmt(s, 1);
        }
        self.out.push_str("    Val::None\n}\n");
    }

    fn stmt(&mut self, s: &Stmt, indent: usize) {
        let pad = "    ".repeat(indent);
        match &s.kind {
            StmtKind::Let { name, value, .. } | StmtKind::Mut { name, value, .. } => {
                if let ExprKind::Unwrap { expr, handler } = &value.kind {
                    let inner = self.expr(expr);
                    let fallible = matches!(&expr.kind, ExprKind::Call { callee, .. } if matches!(&callee.kind, ExprKind::Ident(n) if self.fallible.contains(n)));
                    if fallible {
                        self.out
                            .push_str(&format!("{pad}FAILING.store(false, Relaxed);\n"));
                        self.out
                            .push_str(&format!("{pad}let mut {name} = {inner};\n"));
                        self.out
                            .push_str(&format!("{pad}if FAILING.load(Relaxed) {{\n"));
                        self.out
                            .push_str(&format!("{pad}    FAILING.store(false, Relaxed);\n"));
                        for h in handler {
                            self.stmt(h, indent + 1);
                        }
                        self.out.push_str(&format!("{pad}}}\n"));
                    } else {
                        self.out
                            .push_str(&format!("{pad}let mut {name} = {inner};\n"));
                        self.out
                            .push_str(&format!("{pad}if matches!({name}, Val::None) {{\n"));
                        for h in handler {
                            self.stmt(h, indent + 1);
                        }
                        self.out.push_str(&format!("{pad}}}\n"));
                    }
                } else {
                    self.out
                        .push_str(&format!("{pad}let mut {} = {};\n", name, self.expr(value)));
                }
            }
            StmtKind::Assign { target, value } => {
                if let ExprKind::Ident(n) = &target.kind {
                    self.out
                        .push_str(&format!("{pad}{n} = {};\n", self.expr(value)));
                }
            }
            StmtKind::Ret(Some(e)) => {
                if self.in_main {
                    self.out
                        .push_str(&format!("{pad}let _ = {};\n{pad}return;\n", self.expr(e)));
                } else {
                    self.out
                        .push_str(&format!("{pad}return {};\n", self.expr(e)));
                }
            }
            StmtKind::Ret(None) => {
                if self.in_main {
                    self.out.push_str(&format!("{pad}return;\n"));
                } else {
                    self.out.push_str(&format!("{pad}return Val::None;\n"));
                }
            }
            StmtKind::Fail(e) => self
                .out
                .push_str(&format!("{pad}return fail_with({});\n", self.expr(e))),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                self.out
                    .push_str(&format!("{pad}if truthy({}) {{\n", self.expr(cond)));
                for s in then_body {
                    self.stmt(s, indent + 1);
                }
                if let Some(eb) = else_body {
                    self.out.push_str(&format!("{pad}}} else {{\n"));
                    for s in eb {
                        self.stmt(s, indent + 1);
                    }
                }
                self.out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                let it = self.expr(iter);
                if let Some(v) = &binding.1 {
                    self.out.push_str(&format!(
                        "{pad}for ({}, {v}) in enumerate({it}) {{\n",
                        binding.0
                    ));
                } else {
                    self.out
                        .push_str(&format!("{pad}for {} in list_iter({it}) {{\n", binding.0));
                }
                for s in body {
                    self.stmt(s, indent + 1);
                }
                self.out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::Whl { cond, body } => {
                self.out
                    .push_str(&format!("{pad}while truthy({}) {{\n", self.expr(cond)));
                for s in body {
                    self.stmt(s, indent + 1);
                }
                self.out.push_str(&format!("{pad}}}\n"));
            }
            StmtKind::ExprStmt(e) => {
                if let ExprKind::Call { callee, args } = &e.kind {
                    if let ExprKind::Ident(name) = &callee.kind {
                        if name == "say" {
                            let a = if args.is_empty() {
                                "Val::Str(String::new())".into()
                            } else {
                                self.expr(args[0].expr())
                            };
                            self.out.push_str(&format!("{pad}cuni_say({a});\n"));
                            return;
                        }
                    }
                    if let ExprKind::Field { base, name } = &callee.kind {
                        if name == "push" {
                            if let ExprKind::Ident(b) = &base.kind {
                                let x = args
                                    .first()
                                    .map(|a| self.expr(a.expr()))
                                    .unwrap_or_else(|| "Val::None".into());
                                self.out.push_str(&format!("{pad}v_push(&mut {b}, {x});\n"));
                                return;
                            }
                        }
                    }
                }
                self.out
                    .push_str(&format!("{pad}let _ = {};\n", self.expr(e)));
            }
            StmtKind::Todo => self.out.push_str(&format!(
                "{pad}return fail_with(Val::Str(\"...\".into()));\n"
            )),
        }
    }

    fn expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => format!("Val::Int({n})"),
            ExprKind::Dec(s) => format!("Val::Dec({s})"),
            ExprKind::Time(e) => format!("Val::Time({e})"),
            ExprKind::Float(f) => format!("Val::Float({f:?})"),
            ExprKind::Bool(b) => format!("Val::Bool({b})"),
            ExprKind::Str(s) => format!("Val::Str({:?}.into())", s),
            ExprKind::NoneLit => "Val::None".into(),
            ExprKind::Ident(s) => format!("{s}.clone()"),
            ExprKind::List(xs) => {
                let inner = xs
                    .iter()
                    .map(|x| self.expr(x))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Val::List(vec![{inner}])")
            }
            ExprKind::Map(pairs) => {
                // Wave-1: real maps (docs/STDLIB.md). Keys are strings on
                // this seat; anything else refuses at the literal.
                let inner = pairs
                    .iter()
                    .map(|(k, v)| format!("(v_map_key({}), {})", self.expr(k), self.expr(v)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Val::Map(vec![{inner}])")
            }
            ExprKind::Call { callee, args } => {
                if let ExprKind::Field { base, name } = &callee.kind {
                    // Wave-1 stdlib namespaces (docs/STDLIB.md).
                    if let ExprKind::Ident(ns) = &base.kind {
                        if ns == "json" || ns == "time" {
                            let a = args
                                .iter()
                                .map(|x| self.expr(x.expr()))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let f = match (ns.as_str(), name.as_str()) {
                                ("json", "parse") => "v_json_parse",
                                ("json", "emit") => "v_json_emit",
                                ("time", "epoch") => "v_time_epoch",
                                ("time", "parts") => "v_time_parts",
                                _ => "v_stdlib_unknown",
                            };
                            return format!("{f}({a})");
                        }
                    }
                    if name == "len" {
                        return format!("v_len({})", self.expr(base));
                    }
                    if name == "slice" && args.len() == 2 {
                        return format!(
                            "v_slice({}, {}, {})",
                            self.expr(base),
                            self.expr(args[0].expr()),
                            self.expr(args[1].expr())
                        );
                    }
                    // Wave-1 string ops (docs/STDLIB.md §3).
                    if name == "split" && args.len() == 1 {
                        return format!(
                            "v_split({}, {})",
                            self.expr(base),
                            self.expr(args[0].expr())
                        );
                    }
                    if name == "join" && args.len() == 1 {
                        return format!(
                            "v_join({}, {})",
                            self.expr(base),
                            self.expr(args[0].expr())
                        );
                    }
                    if name == "trim" && args.is_empty() {
                        return format!("v_trim({})", self.expr(base));
                    }
                    if name == "contains" && args.len() == 1 {
                        return format!(
                            "v_contains({}, {})",
                            self.expr(base),
                            self.expr(args[0].expr())
                        );
                    }
                    if name == "push" {
                        return format!(
                            "v_push_val({}, {})",
                            self.expr(base),
                            args.iter()
                                .map(|a| self.expr(a.expr()))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                    }
                }
                let a = args
                    .iter()
                    .map(|x| self.expr(x.expr()))
                    .collect::<Vec<_>>()
                    .join(", ");
                if let ExprKind::Ident(n) = &callee.kind {
                    let mapped = match n.as_str() {
                        "range" => "v_range",
                        "abs" => "v_abs",
                        "min" => "v_min",
                        "max" => "v_max",
                        "dec_of_int" => "v_dec_of_int",
                        "int_of_dec" => "v_int_of_dec",
                        "parse_time" => "v_parse_time",
                        "add_seconds" => "v_add_seconds",
                        "days_between" => "v_days_between",
                        // Wave-1 stdlib (docs/STDLIB.md §4).
                        "sha256" => "v_sha256",                        _ => n.as_str(),
                    };
                    return format!("{mapped}({a})");
                }
                let c = self.expr(callee);
                format!("{c}({a})")
            }
            ExprKind::Index { base, index } => {
                format!("v_index({}, {})", self.expr(base), self.expr(index))
            }
            ExprKind::Field { base, name } => {
                format!("v_get(&({}).clone(), \"{name}\")", self.expr(base))
            }
            ExprKind::InterpStr(parts) => {
                let mut acc = "Val::Str(String::new())".to_string();
                for p in parts {
                    let piece = match p {
                        StrPartExpr::Text(t) => format!("Val::Str({:?}.into())", t),
                        StrPartExpr::Expr(ex) => format!("v_to_str({})", self.expr(ex)),
                    };
                    acc = format!("v_concat({acc}, {piece})");
                }
                acc
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs);
                let r = self.expr(rhs);
                match op {
                    BinOp::Add => format!("v_add({l}, {r})"),
                    BinOp::Sub => format!("v_sub({l}, {r})"),
                    BinOp::Mul => format!("v_mul({l}, {r})"),
                    BinOp::Div => format!("v_div({l}, {r})"),
                    BinOp::Mod => format!("v_mod({l}, {r})"),
                    BinOp::Eq => format!("Val::Bool(v_eq({l}, {r}))"),
                    BinOp::Ne => format!("Val::Bool(!v_eq({l}, {r}))"),
                    BinOp::Lt => format!("Val::Bool(v_cmp({l}, {r}) < 0)"),
                    BinOp::Gt => format!("Val::Bool(v_cmp({l}, {r}) > 0)"),
                    BinOp::Le => format!("Val::Bool(v_cmp({l}, {r}) <= 0)"),
                    BinOp::Ge => format!("Val::Bool(v_cmp({l}, {r}) >= 0)"),
                    BinOp::And => format!("Val::Bool(truthy({l}) && truthy({r}))"),
                    BinOp::Or => format!("Val::Bool(truthy({l}) || truthy({r}))"),
                }
            }
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => format!("Val::Bool(!truthy({}))", self.expr(expr)),
                UnOp::Neg => format!("v_neg({})", self.expr(expr)),
            },
            ExprKind::Unwrap { expr, .. } => self.expr(expr),
        }
    }
}

const RT: &str = r#"
use std::convert::TryFrom;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
static FAILING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
enum Val {
    Int(i64),
    /// CuNi `dec`: fixed-point decimal, scale 10^4, exact (docs/DECIMAL.md).
    /// Wide seat: i128; checked ops panic (loud refusal) on true overflow.
    Dec(i128),
    /// CuNi `time`: int64 unix epoch seconds, UTC (docs/TIME.md).
    /// Wide seat: i64; checked ops panic (loud refusal) on true overflow.
    Time(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    List(Vec<Val>),
    // Wave-1 stdlib: real maps (docs/STDLIB.md). Keys are strings; the
    // Vec preserves insertion order (first-seen position on duplicates).
    Map(Vec<(String, Val)>),
    Struct { tag: String, fields: Vec<(String, Val)> },
    Enum { ty: String, variant: String },
}
fn v_struct(tag: &str) -> Val { Val::Struct { tag: tag.into(), fields: vec![] } }
fn v_enum(ty: &str, variant: &str) -> Val { Val::Enum { ty: ty.into(), variant: variant.into() } }
// Wave-1 stdlib: map keys are strings on this seat; anything else refuses
// at the literal (json.emit refuses non-string keys per docs/STDLIB.md §1.3).
fn v_map_key(k: Val) -> String {
    match k { Val::Str(s) => s, _ => panic!("rs seat: map keys must be strings") }
}
fn v_set(s: &mut Val, k: &str, v: Val) {
    if let Val::Struct { fields, .. } = s { fields.push((k.into(), v)); }
}
fn v_get(s: &Val, k: &str) -> Val {
    match s {
        Val::Struct { fields, .. } => fields.iter().find(|(n,_)| n==k).map(|(_,v)| v.clone()).unwrap_or(Val::None),
        Val::Enum { variant, ty } if variant == k => Val::Enum { ty: ty.clone(), variant: variant.clone() },
        _ => Val::None,
    }
}
fn v_index(s: Val, i: Val) -> Val {
    match (s, i) {
        (Val::List(xs), Val::Int(n)) if n >= 0 && (n as usize) < xs.len() => xs[n as usize].clone(),
        _ => Val::None,
    }
}
fn v_len(s: Val) -> Val {
    match s { Val::List(xs) => Val::Int(xs.len() as i64), Val::Str(t) => Val::Int(t.len() as i64), _ => Val::Int(0) }
}
fn v_as_int(s: &Val) -> i64 { match s { Val::Int(n) => *n, _ => 0 } }
fn v_range(n: Val) -> Val {
    let m = v_as_int(&n);
    if m <= 0 { return Val::List(vec![]); }
    Val::List((0..m).map(Val::Int).collect())
}
fn v_abs(n: Val) -> Val { let v = v_as_int(&n); Val::Int(if v < 0 { -v } else { v }) }
fn v_min(a: Val, b: Val) -> Val { let x = v_as_int(&a); let y = v_as_int(&b); Val::Int(if x <= y { x } else { y }) }
fn v_max(a: Val, b: Val) -> Val { let x = v_as_int(&a); let y = v_as_int(&b); Val::Int(if x >= y { x } else { y }) }
fn v_slice(s: Val, a: Val, b: Val) -> Val {
    let ia = v_as_int(&a); let ib = v_as_int(&b);
    match s {
        Val::Str(t) => {
            let n = t.len() as i64;
            if ia < 0 || ib < 0 || ia > n || ib > n || ia > ib { Val::Str(String::new()) }
            else { Val::Str(t.chars().skip(ia as usize).take((ib - ia) as usize).collect()) }
        }
        Val::List(xs) => {
            let n = xs.len() as i64;
            if ia < 0 || ib < 0 || ia > n || ib > n || ia > ib { Val::List(vec![]) }
            else { Val::List(xs[ia as usize..ib as usize].to_vec()) }
        }
        _ => Val::None,
    }
}
fn v_push(s: &mut Val, x: Val) {
    if let Val::List(xs) = s { xs.push(x); }
}
fn v_push_val(mut s: Val, x: Val) -> Val { v_push(&mut s, x); s }
fn enumerate(s: Val) -> Vec<(Val, Val)> {
    match s {
        Val::List(xs) => xs.into_iter().enumerate().map(|(i,v)| (Val::Int(i as i64), v)).collect(),
        _ => vec![],
    }
}
fn list_iter(s: Val) -> Vec<Val> { match s { Val::List(xs) => xs, _ => vec![] } }
fn fail_with(e: Val) -> Val { FAILING.store(true, Relaxed); let _ = e; Val::None }
fn v_eq(a: Val, b: Val) -> bool {
    match (&a,&b) {
        (Val::Dec(x), Val::Dec(y)) => x==y,
        (Val::Time(x), Val::Time(y)) => x==y,
        (Val::Int(x), Val::Int(y)) => x==y,
        (Val::Float(x), Val::Float(y)) => x==y,
        (Val::Bool(x), Val::Bool(y)) => x==y,
        (Val::Str(x), Val::Str(y)) => x==y,
        (Val::None, Val::None) => true,
        (Val::Enum { variant: x, .. }, Val::Enum { variant: y, .. }) => x==y,
        _ => false,
    }
}
fn truthy(a: Val) -> bool {
    match a {
        Val::None => false,
        Val::Bool(b) => b,
        Val::Int(i) => i != 0,
        Val::Dec(d) => d != 0,
        Val::Time(t) => t != 0,
        Val::Float(f) => f != 0.0,
        Val::Str(s) => !s.is_empty(),
        _ => true,
    }
}
fn as_f(a: &Val) -> f64 { match a { Val::Float(f) => *f, Val::Int(i) => *i as f64, _ => 0.0 } }
fn v_cmp(a: Val, b: Val) -> i32 {
    // `time` compares epoch integers directly (docs/TIME.md §3).
    if let (Val::Time(x), Val::Time(y)) = (&a, &b) {
        return if x < y { -1 } else if x > y { 1 } else { 0 };
    }
    if matches!(a, Val::Time(_)) || matches!(b, Val::Time(_)) {
        panic!("cuni: cannot mix time and non-time — durations are plain int seconds (docs/TIME.md §3)");
    }
    // `dec` compares scaled integers directly — never via f64.
    if let (Val::Dec(x), Val::Dec(y)) = (&a, &b) {
        return if x < y { -1 } else if x > y { 1 } else { 0 };
    }
    let x = as_f(&a); let y = as_f(&b);
    if x < y { -1 } else if x > y { 1 } else { 0 }
}
/// `dec` is a closed world (docs/DECIMAL.md §3–5): both operands dec, or a
/// loud refusal. The typeck already rejected mixes; this is defense in depth.
fn cuni_dec_pair(a: &Val, b: &Val) -> (i128, i128) {
    match (a, b) {
        (Val::Dec(x), Val::Dec(y)) => (*x, *y),
        _ => panic!("cuni: cannot mix dec and non-dec — convert explicitly (`dec_of_int` / `int_of_dec`)"),
    }
}
const DEC_SCALE: i128 = 10_000;
fn v_add(a: Val, b: Val) -> Val {
    // `time` is a closed world (docs/TIME.md §3): (time,int)/(int,time) ->
    // time. The typeck proved the shape; this is defense in depth.
    if matches!(a, Val::Time(_)) || matches!(b, Val::Time(_)) {
        match (&a, &b) {
            (Val::Time(t), Val::Int(s)) | (Val::Int(s), Val::Time(t)) => {
                return Val::Time(t.checked_add(*s).expect("cuni: time addition overflow — refused"))
            }
            _ => panic!("cuni: cannot mix time with this operand — durations are plain int seconds (docs/TIME.md §3)"),
        }
    }
    if matches!(a, Val::Dec(_)) || matches!(b, Val::Dec(_)) {
        let (x, y) = cuni_dec_pair(&a, &b);
        return Val::Dec(x.checked_add(y).expect("cuni: dec addition overflow — refused"));
    }
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)+as_f(&b)),
        (Val::Int(x), Val::Int(y)) => Val::Int(x+y),
        (Val::Str(s1), Val::Str(s2)) => Val::Str(format!("{}{}", s1, s2)),
        _ => Val::Int(0),
    }
}
fn v_sub(a: Val, b: Val) -> Val {
    // `time - int -> time`, `time - time -> int` (docs/TIME.md §3).
    if matches!(a, Val::Time(_)) || matches!(b, Val::Time(_)) {
        match (&a, &b) {
            (Val::Time(t), Val::Int(s)) => {
                return Val::Time(t.checked_sub(*s).expect("cuni: time subtraction overflow — refused"))
            }
            (Val::Time(x), Val::Time(y)) => {
                return Val::Int(x.checked_sub(*y).expect("cuni: time difference overflow — refused"))
            }
            _ => panic!("cuni: cannot mix time with this operand (docs/TIME.md §3)"),
        }
    }
    if matches!(a, Val::Dec(_)) || matches!(b, Val::Dec(_)) {
        let (x, y) = cuni_dec_pair(&a, &b);
        return Val::Dec(x.checked_sub(y).expect("cuni: dec subtraction overflow — refused"));
    }
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)-as_f(&b)),
        (Val::Int(x), Val::Int(y)) => Val::Int(x-y),
        _ => Val::Int(0),
    }
}
fn v_mul(a: Val, b: Val) -> Val {
    if matches!(a, Val::Time(_)) || matches!(b, Val::Time(_)) {
        panic!("cuni: `*` is not defined on `time` (docs/TIME.md §3)");
    }
    if matches!(a, Val::Dec(_)) || matches!(b, Val::Dec(_)) {
        // trunc(x*y/10000) toward zero; i128 `/` truncates natively.
        let (x, y) = cuni_dec_pair(&a, &b);
        let p = x.checked_mul(y).expect("cuni: dec multiplication overflow — refused");
        return Val::Dec(p / DEC_SCALE);
    }
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)*as_f(&b)),
        (Val::Int(x), Val::Int(y)) => Val::Int(x*y),
        _ => Val::Int(0),
    }
}
fn v_div(a: Val, b: Val) -> Val {
    if matches!(a, Val::Time(_)) || matches!(b, Val::Time(_)) {
        panic!("cuni: `/` is not defined on `time` (docs/TIME.md §3)");
    }
    if matches!(a, Val::Dec(_)) || matches!(b, Val::Dec(_)) {
        // trunc(x*10000/y) toward zero; division by zero panics loudly.
        let (x, y) = cuni_dec_pair(&a, &b);
        if y == 0 {
            panic!("cuni: dec division by zero");
        }
        let p = x.checked_mul(DEC_SCALE).expect("cuni: dec division intermediate overflow — refused");
        return Val::Dec(p / y);
    }
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)/as_f(&b)),
        (Val::Int(x), Val::Int(y)) if *y != 0 => Val::Int(x/y),
        _ => Val::Int(0),
    }
}
fn v_mod(a: Val, b: Val) -> Val {
    if matches!(a, Val::Time(_)) || matches!(b, Val::Time(_)) {
        panic!("cuni: `%` is not defined on `time` (docs/TIME.md §3)");
    }
    if matches!(a, Val::Dec(_)) || matches!(b, Val::Dec(_)) {
        panic!("cuni: `%` is not defined on `dec` — refusing");
    }
    // Python-floored modulo (CuNi spec); Rust's % truncates.
    match (a,b) {
        (Val::Int(x), Val::Int(y)) if y != 0 => {
            let r = x % y;
            Val::Int(if r != 0 && ((r < 0) != (y < 0)) { r + y } else { r })
        }
        _ => Val::Int(0),
    }
}
fn v_neg(a: Val) -> Val {
    match a {
        Val::Dec(d) => Val::Dec(d.checked_neg().expect("cuni: dec negation overflow — refused")),
        Val::Time(t) => Val::Time(t.checked_neg().expect("cuni: time negation overflow — refused")),
        Val::Float(f) => Val::Float(-f),
        Val::Int(i) => Val::Int(-i),
        x => x,
    }
}
fn v_to_str(a: Val) -> Val {
    Val::Str(match a {
        Val::Int(i) => i.to_string(),
        Val::Dec(d) => cuni_dec_str(d),
        Val::Time(t) => cuni_time_str(t),
        Val::Float(f) => format!("{:.15}", f).trim_end_matches('0').trim_end_matches('.').to_string(),
        Val::Str(s) => s,
        Val::Bool(b) => b.to_string(),
        Val::None => "none".into(),
        _ => String::new(),
    })
}
fn v_concat(a: Val, b: Val) -> Val {
    let Val::Str(x) = v_to_str(a) else { return Val::Str(String::new()) };
    let Val::Str(y) = v_to_str(b) else { return Val::Str(x) };
    Val::Str(x + &y)
}
/// Canonical `dec` rendering of a scaled i128 (docs/DECIMAL.md §6).
fn cuni_dec_str(v: i128) -> String {
    let neg = v < 0;
    let mag = v.unsigned_abs();
    let ip = mag / (DEC_SCALE as u128);
    let mut fp = format!("{:04}", mag % (DEC_SCALE as u128));
    while fp.ends_with('0') {
        fp.pop();
    }
    if fp.is_empty() {
        fp.push('0');
    }
    format!("{}{}.{}", if neg { "-" } else { "" }, ip, fp)
}
/// Canonical ISO-8601 UTC rendering of an int64 epoch (docs/TIME.md §4).
fn cuni_time_str(e: i64) -> String {
    let days = e.div_euclid(86400);
    let sod = e.rem_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let hh = sod / 3600;
    let mi = (sod % 3600) / 60;
    let ss = sod % 60;
    let ys = if y < 0 {
        format!("-{:04}", -y)
    } else {
        format!("{:04}", y)
    };
    format!("{ys}-{m:02}-{d:02}T{hh:02}:{mi:02}:{ss:02}Z")
}
fn cuni_time_days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y0 = if m <= 2 { y - 1 } else { y };
    let era = y0.div_euclid(400);
    let yoe = y0 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
/// Strict ISO-8601 UTC -> time (docs/TIME.md §2, §5): bad input panics loudly.
fn v_parse_time(s: Val) -> Val {
    let t = match s {
        Val::Str(t) => t,
        _ => panic!("cuni: parse_time needs a string"),
    };
    let bad = || panic!("cuni: parse_time: bad ISO-8601 UTC timestamp — refused");
    let b = t.as_bytes();
    if b.len() != 20 {
        bad();
    }
    for (i, expect) in [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':'), (19, b'Z')] {
        if b[i] != expect {
            bad();
        }
    }
    let digits = |lo: usize, hi: usize| -> i64 {
        let mut v: i64 = 0;
        for i in lo..hi {
            let c = b[i];
            if !c.is_ascii_digit() {
                bad();
            }
            v = v * 10 + (c - b'0') as i64;
        }
        v
    };
    let (y, mo, d) = (digits(0, 4), digits(5, 7), digits(8, 10));
    let (h, mi, sec) = (digits(11, 13), digits(14, 16), digits(17, 19));
    if !(1..=9999).contains(&y) || !(1..=12).contains(&mo) {
        bad();
    }
    let dim = match mo {
        4 | 6 | 9 | 11 => 30,
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    };
    if !(1..=dim).contains(&d) || h > 23 || mi > 59 || sec > 59 {
        bad();
    }
    Val::Time(cuni_time_days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec)
}
fn v_add_seconds(t: Val, s: Val) -> Val {
    match (t, s) {
        (Val::Time(t), Val::Int(s)) => {
            Val::Time(t.checked_add(s).expect("cuni: add_seconds overflow — refused"))
        }
        _ => panic!("cuni: add_seconds needs (time, int)"),
    }
}
fn v_days_between(a: Val, b: Val) -> Val {
    match (a, b) {
        (Val::Time(a), Val::Time(b)) => {
            // Truncation toward zero (docs/TIME.md §5); i64 `/` truncates natively.
            Val::Int(a.checked_sub(b).expect("cuni: days_between overflow — refused") / 86400)
        }
        _ => panic!("cuni: days_between needs (time, time)"),
    }
}
fn v_dec_of_int(n: Val) -> Val {
    match n {
        Val::Int(i) => Val::Dec(i as i128 * DEC_SCALE),
        _ => panic!("cuni: dec_of_int needs an int"),
    }
}
fn v_int_of_dec(d: Val) -> Val {
    match d {
        // Truncation toward zero (docs/DECIMAL.md §5).
        Val::Dec(v) => Val::Int(
            i64::try_from(v / DEC_SCALE).expect("cuni: int_of_dec out of int range — refused"),
        ),
        _ => panic!("cuni: int_of_dec needs a dec"),
    }
}
fn cuni_say(v: Val) {
    match v {
        Val::Int(i) => println!("{i}"),
        Val::Dec(d) => println!("{}", cuni_dec_str(d)),
        Val::Time(t) => println!("{}", cuni_time_str(t)),
        Val::Float(f) => {
            let s = format!("{:.15}", f);
            let s = s.trim_end_matches('0').trim_end_matches('.');
            println!("{s}");
        }
        Val::Str(s) => println!("{s}"),
        Val::Bool(b) => println!("{}", if b { "True" } else { "False" }),
        Val::None => println!("None"),
        other => {
            if let Val::Str(s) = v_to_str(other) { println!("{s}"); }
        }
    }
}
"#;

/// Wave-1 stdlib runtime (docs/STDLIB.md). Hand-rolled: the rs seat compiles
/// with `rustc` directly (no cargo, no dependency resolution), so `serde_json`
/// and the `sha2` crate are unavailable — and unnecessary. These implement the
/// spec algorithms exactly, shared with the interpreter seat.
const RT_STDLIB: &str = r#"
// ================= Wave-1 stdlib (docs/STDLIB.md) =================
fn v_json_parse(s: Val) -> Val {
    let t = match s { Val::Str(t) => t, _ => panic!("json.parse needs a str") };
    let mut p = JParser { s: t.as_str(), b: t.as_bytes(), pos: 0 };
    p.ws();
    let v = p.value();
    p.ws();
    if p.pos != p.b.len() { panic!("json.parse: trailing characters"); }
    match v { Val::Map(_) => v, _ => panic!("json.parse: top-level JSON value must be an object") }
}
struct JParser<'a> { s: &'a str, b: &'a [u8], pos: usize }
impl<'a> JParser<'a> {
    fn ws(&mut self) {
        while self.pos < self.b.len() && matches!(self.b[self.pos], b' '|b'\t'|b'\n'|b'\r') { self.pos += 1; }
    }
    fn expect(&mut self, word: &str) {
        if self.s[self.pos..].starts_with(word) { self.pos += word.len(); }
        else { panic!("json.parse: bad literal"); }
    }
    fn value(&mut self) -> Val {
        let c = *self.b.get(self.pos).unwrap_or(&0);
        match c {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Val::Str(self.string()),
            b't' => { self.expect("true"); Val::Bool(true) }
            b'f' => { self.expect("false"); Val::Bool(false) }
            b'n' => { self.expect("null"); Val::None }
            b'-'|b'0'..=b'9' => {
                let start = self.pos;
                while self.pos < self.b.len() && matches!(self.b[self.pos], b'-'|b'+'|b'0'..=b'9'|b'.'|b'e'|b'E') { self.pos += 1; }
                Val::Int(v_json_int(&self.s[start..self.pos]))
            }
            _ => panic!("json.parse: unexpected character"),
        }
    }
    fn object(&mut self) -> Val {
        self.pos += 1;
        let mut pairs: Vec<(String, Val)> = Vec::new();
        self.ws();
        if self.b.get(self.pos) == Some(&b'}') { self.pos += 1; return Val::Map(pairs); }
        loop {
            self.ws();
            if self.b.get(self.pos) != Some(&b'"') { panic!("json.parse: object keys must be strings"); }
            let key = self.string();
            self.ws();
            if self.b.get(self.pos) != Some(&b':') { panic!("json.parse: expected ':'"); }
            self.pos += 1; self.ws();
            let v = self.value();
            if let Some(slot) = pairs.iter_mut().find(|(k, _)| k == &key) { slot.1 = v; }
            else { pairs.push((key, v)); }
            self.ws();
            match self.b.get(self.pos) {
                Some(b',') => { self.pos += 1; }
                Some(b'}') => { self.pos += 1; return Val::Map(pairs); }
                _ => panic!("json.parse: expected ',' or '}'"),
            }
        }
    }
    fn array(&mut self) -> Val {
        self.pos += 1;
        let mut xs = Vec::new();
        self.ws();
        if self.b.get(self.pos) == Some(&b']') { self.pos += 1; return Val::List(xs); }
        loop {
            self.ws();
            xs.push(self.value());
            self.ws();
            match self.b.get(self.pos) {
                Some(b',') => { self.pos += 1; }
                Some(b']') => { self.pos += 1; return Val::List(xs); }
                _ => panic!("json.parse: expected ',' or ']'"),
            }
        }
    }
    fn hex4(&mut self) -> u32 {
        if self.pos + 4 > self.b.len() { panic!("json.parse: bad \\u escape"); }
        let v = u32::from_str_radix(&self.s[self.pos..self.pos+4], 16).unwrap_or_else(|_| panic!("json.parse: bad \\u escape"));
        self.pos += 4;
        v
    }
    fn string(&mut self) -> String {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let rest = &self.s[self.pos..];
            let run = rest.find(|c| c == '"' || c == '\\').unwrap_or(rest.len());
            let chunk = &rest[..run];
            if chunk.bytes().any(|b| b < 0x20) { panic!("json.parse: unescaped control character in string"); }
            out.push_str(chunk);
            self.pos += run;
            match self.b.get(self.pos) {
                None => panic!("json.parse: unterminated string"),
                Some(b'"') => { self.pos += 1; return out; }
                Some(b'\\') => {
                    self.pos += 1;
                    let e = *self.b.get(self.pos).unwrap_or(&0);
                    self.pos += 1;
                    match e {
                        b'"' => out.push('"'), b'\\' => out.push('\\'), b'/' => out.push('/'),
                        b'b' => out.push('\x08'), b'f' => out.push('\x0c'),
                        b'n' => out.push('\n'), b'r' => out.push('\r'), b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4();
                            if (0xD800..0xDC00).contains(&hi) {
                                let is_lo = self.b.get(self.pos) == Some(&b'\\') && self.b.get(self.pos+1) == Some(&b'u');
                                if !is_lo { panic!("json.parse: lone surrogate"); }
                                self.pos += 2;
                                let lo = self.hex4();
                                if !(0xDC00..0xE000).contains(&lo) { panic!("json.parse: lone surrogate"); }
                                let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                out.push(char::from_u32(cp).unwrap_or_else(|| panic!("json.parse: bad code point")));
                            } else if (0xDC00..0xE000).contains(&hi) {
                                panic!("json.parse: lone surrogate");
                            } else {
                                out.push(char::from_u32(hi).unwrap_or_else(|| panic!("json.parse: bad code point")));
                            }
                        }
                        _ => panic!("json.parse: bad escape"),
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
fn v_json_int(tok: &str) -> i64 {
    let t = tok.as_bytes();
    let mut i = 0usize;
    let neg = if t.get(i) == Some(&b'-') { i += 1; true } else { false };
    if t.get(i) == Some(&b'0') { i += 1; }
    else if matches!(t.get(i), Some(b'1'..=b'9')) { while matches!(t.get(i), Some(b'0'..=b'9')) { i += 1; } }
    else { panic!("json.parse: bad number"); }
    let mut frac_len = 0usize;
    if t.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while matches!(t.get(i), Some(b'0'..=b'9')) { i += 1; }
        if i == start { panic!("json.parse: bad number"); }
        frac_len = i - start;
    }
    let mut exp: i64 = 0;
    // Mantissa ends where the exponent begins: only int+frac digits feed `d`.
    let mant_end = i;
    if matches!(t.get(i), Some(b'e')|Some(b'E')) {
        i += 1;
        let eneg = if t.get(i) == Some(&b'-') { i += 1; true } else { if t.get(i) == Some(&b'+') { i += 1; } false };
        let start = i;
        while matches!(t.get(i), Some(b'0'..=b'9')) { i += 1; }
        if i == start { panic!("json.parse: bad number"); }
        let ed: i64 = tok[start..i].parse().unwrap_or_else(|_| panic!("json.parse: bad number"));
        exp = if eneg { -ed } else { ed };
    }
    if i != t.len() { panic!("json.parse: bad number"); }
    // Mantissa only — exponent digits must NOT feed `d`.
    let digits: Vec<u8> = t[..mant_end].iter().filter(|c| c.is_ascii_digit()).copied().collect();
    let sig: &[u8] = match digits.iter().position(|&c| c != b'0') {
        Some(p) => &digits[p..],
        None => return 0,
    };
    if sig.len() > 16 { panic!("json.parse: number is not an integer in ±(2^53−1)"); }
    let mut d: i64 = 0;
    for &c in sig { d = d * 10 + (c - b'0') as i64; }
    let mut f = frac_len as i64;
    while d % 10 == 0 && d != 0 { d /= 10; f -= 1; }
    let k = f - exp;
    let mut v: i64;
    if k <= 0 {
        v = d;
        for _ in 0..(-k) { v = v.checked_mul(10).unwrap_or_else(|| panic!("json.parse: number is not an integer in ±(2^53−1)")); }
    } else {
        if k > 16 { panic!("json.parse: number is not an integer in ±(2^53−1)"); }
        let mut p10: i64 = 1;
        for _ in 0..k { p10 *= 10; }
        if d % p10 != 0 { panic!("json.parse: number is not an integer in ±(2^53−1)"); }
        v = d / p10;
    }
    if neg { v = -v; }
    if v < -9007199254740991 || v > 9007199254740991 { panic!("json.parse: number is not an integer in ±(2^53−1)"); }
    v
}
fn v_json_emit(v: Val) -> Val {
    match v { Val::Map(_) => {}, _ => panic!("json.emit needs a map") }
    let mut out = String::new();
    v_json_write(&v, &mut out);
    Val::Str(out)
}
fn v_json_write(v: &Val, out: &mut String) {
    match v {
        Val::Int(n) => out.push_str(&n.to_string()),
        Val::Str(s) => v_json_write_str(s, out),
        Val::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Val::None => out.push_str("null"),
        Val::List(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() { if i > 0 { out.push(','); } v_json_write(x, out); }
            out.push(']');
        }
        Val::Map(pairs) => {
            let mut ks: Vec<(&str, &Val)> = pairs.iter().map(|(k, v)| (k.as_str(), v)).collect();
            ks.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push('{');
            for (i, (k, val)) in ks.iter().enumerate() {
                if i > 0 { out.push(','); }
                v_json_write_str(k, out);
                out.push(':');
                v_json_write(val, out);
            }
            out.push('}');
        }
        _ => panic!("json.emit: value has no JSON form"),
    }
}
fn v_json_write_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            _ => out.push(c),
        }
    }
    out.push('"');
}
fn v_days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y0 = if m <= 2 { y - 1 } else { y };
    let era = y0 / 400;
    let yoe = y0 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}
fn v_civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
fn v_stdlib_as_int(v: &Val) -> i64 { match v { Val::Int(n) => *n, _ => panic!("expected int") } }
fn v_time_epoch(y: Val, mo: Val, d: Val, h: Val, mi: Val, s: Val) -> Val {
    let (y, mo, d, h, mi, s) = (v_stdlib_as_int(&y), v_stdlib_as_int(&mo), v_stdlib_as_int(&d), v_stdlib_as_int(&h), v_stdlib_as_int(&mi), v_stdlib_as_int(&s));
    if !(1 <= y && y <= 9999) { panic!("time.epoch: year out of range 1..9999"); }
    if !(1 <= mo && mo <= 12) { panic!("time.epoch: month out of range 1..12"); }
    let dim = match mo {
        1|3|5|7|8|10|12 => 31,
        4|6|9|11 => 30,
        2 => if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) { 29 } else { 28 },
        _ => 0,
    };
    if !(1 <= d && d <= dim) { panic!("time.epoch: day out of range for month"); }
    if !(0 <= h && h <= 23) { panic!("time.epoch: hour out of range 0..23"); }
    if !(0 <= mi && mi <= 59) { panic!("time.epoch: minute out of range 0..59"); }
    if !(0 <= s && s <= 59) { panic!("time.epoch: second out of range 0..59"); }
    Val::Int(v_days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s)
}
fn v_time_parts(e: Val) -> Val {
    let e = v_stdlib_as_int(&e);
    let lo = v_days_from_civil(1, 1, 1) * 86400;
    let hi = v_days_from_civil(9999, 12, 31) * 86400 + 86399;
    if e < lo || e > hi { panic!("time.parts: epoch out of range 1..9999"); }
    let days = e.div_euclid(86400);
    let secs = e.rem_euclid(86400);
    let (y, mo, d) = v_civil_from_days(days);
    Val::Map(vec![
        ("year".to_string(), Val::Int(y)),
        ("month".to_string(), Val::Int(mo)),
        ("day".to_string(), Val::Int(d)),
        ("hour".to_string(), Val::Int(secs / 3600)),
        ("min".to_string(), Val::Int((secs % 3600) / 60)),
        ("sec".to_string(), Val::Int(secs % 60)),
    ])
}
fn v_split(s: Val, sep: Val) -> Val {
    let (ss, pp) = match (s, sep) {
        (Val::Str(a), Val::Str(b)) => (a, b),
        _ => panic!(".split needs strings"),
    };
    if pp.is_empty() { panic!(".split: empty separator; refusing"); }
    Val::List(ss.split(pp.as_str()).map(|p| Val::Str(p.to_string())).collect())
}
fn v_join(sep: Val, parts: Val) -> Val {
    let ss = match sep { Val::Str(s) => s, _ => panic!(".join needs a str separator") };
    match parts {
        Val::List(xs) => {
            let mut out = Vec::new();
            for x in xs {
                match x { Val::Str(t) => out.push(t), _ => panic!(".join: all parts must be str") }
            }
            Val::Str(out.join(ss.as_str()))
        }
        _ => panic!(".join needs a list<str>"),
    }
}
fn v_trim(s: Val) -> Val {
    match s {
        Val::Str(t) => Val::Str(t.trim_matches(|c: char| matches!(c, '\u{9}'|'\u{A}'|'\u{B}'|'\u{C}'|'\u{D}'|'\u{20}')).to_string()),
        _ => panic!(".trim needs a str"),
    }
}
fn v_contains(s: Val, sub: Val) -> Val {
    match (s, sub) {
        (Val::Str(a), Val::Str(b)) => Val::Bool(a.contains(b.as_str())),
        _ => panic!(".contains needs strings"),
    }
}
fn v_sha256(s: Val) -> Val {
    let t = match s { Val::Str(t) => t, _ => panic!("sha256 needs a str") };
    let mut msg = t.as_bytes().to_vec();
    let bit_len = (msg.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 { msg.push(0); }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    const K: [u32; 64] = [
        0x428a2f98,0x71374491,0xb5c0fbcf,0xe9b5dba5,0x3956c25b,0x59f111f1,0x923f82a4,0xab1c5ed5,
        0xd807aa98,0x12835b01,0x243185be,0x550c7dc3,0x72be5d74,0x80deb1fe,0x9bdc06a7,0xc19bf174,
        0xe49b69c1,0xefbe4786,0x0fc19dc6,0x240ca1cc,0x2de92c6f,0x4a7484aa,0x5cb0a9dc,0x76f988da,
        0x983e5152,0xa831c66d,0xb00327c8,0xbf597fc7,0xc6e00bf3,0xd5a79147,0x06ca6351,0x14292967,
        0x27b70a85,0x2e1b2138,0x4d2c6dfc,0x53380d13,0x650a7354,0x766a0abb,0x81c2c92e,0x92722c85,
        0xa2bfe8a1,0xa81a664b,0xc24b8b70,0xc76c51a3,0xd192e819,0xd6990624,0xf40e3585,0x106aa070,
        0x19a4c116,0x1e376c08,0x2748774c,0x34b0bcb5,0x391c0cb3,0x4ed8aa4a,0x5b9cca4f,0x682e6ff3,
        0x748f82ee,0x78a5636f,0x84c87814,0x8cc70208,0x90befffa,0xa4506ceb,0xbef9a3f7,0xc67178f2,
    ];
    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4*i], chunk[4*i+1], chunk[4*i+2], chunk[4*i+3]]);
        }
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) = (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g; g = f; f = e; e = d.wrapping_add(t1);
            d = c; c = b; b = a; a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a); h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c); h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e); h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g); h[7] = h[7].wrapping_add(hh);
    }
    let mut out = String::with_capacity(64);
    for x in h { out.push_str(&format!("{:08x}", x)); }
    Val::Str(out)
}
"#;

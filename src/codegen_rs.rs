//! Native Rust seat. Tagged `Val` runtime covering the portable core.
//! Compiled with `rustc`.

use crate::ast::*;
use std::collections::HashSet;

pub fn generate(program: &Program) -> String {
    let mut g = Gen {
        out: String::from("#![allow(dead_code, unused_mut, unused_variables, unused_assignments)]\n") + RT,
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
            Item::Enum(e) => g.enums.push((e.name.clone(), e.variants.clone())),
            _ => {}
        }
    }
    for (name, vars) in &g.enums {
        g.out.push_str(&format!("fn enum_{name}() -> Val {{\n    let mut s = v_struct(\"{name}\");\n"));
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
        self.out.push_str(&format!("fn {}({}) -> Val {{\n", t.name, args));
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
                        self.out.push_str(&format!("{pad}FAILING.store(false, Relaxed);\n"));
                        self.out.push_str(&format!("{pad}let mut {name} = {inner};\n"));
                        self.out.push_str(&format!("{pad}if FAILING.load(Relaxed) {{\n"));
                        self.out.push_str(&format!("{pad}    FAILING.store(false, Relaxed);\n"));
                        for h in handler {
                            self.stmt(h, indent + 1);
                        }
                        self.out.push_str(&format!("{pad}}}\n"));
                    } else {
                        self.out.push_str(&format!("{pad}let mut {name} = {inner};\n"));
                        self.out.push_str(&format!(
                            "{pad}if matches!({name}, Val::None) {{\n"
                        ));
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
            StmtKind::For { binding, iter, body } => {
                let it = self.expr(iter);
                if let Some(v) = &binding.1 {
                    self.out.push_str(&format!(
                        "{pad}for ({}, {v}) in enumerate({it}) {{\n",
                        binding.0
                    ));
                } else {
                    self.out.push_str(&format!(
                        "{pad}for {} in list_iter({it}) {{\n",
                        binding.0
                    ));
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
                self.out.push_str(&format!("{pad}let _ = {};\n", self.expr(e)));
            }
            StmtKind::Todo => self
                .out
                .push_str(&format!("{pad}return fail_with(Val::Str(\"...\".into()));\n")),
        }
    }

    fn expr(&self, e: &Expr) -> String {
        match &e.kind {
            ExprKind::Int(n) => format!("Val::Int({n})"),
            ExprKind::Float(f) => format!("Val::Float({f:?})"),
            ExprKind::Bool(b) => format!("Val::Bool({b})"),
            ExprKind::Str(s) => format!("Val::Str({:?}.into())", s),
            ExprKind::NoneLit => "Val::None".into(),
            ExprKind::Ident(s) => format!("{s}.clone()"),
            ExprKind::List(xs) => {
                let inner = xs.iter().map(|x| self.expr(x)).collect::<Vec<_>>().join(", ");
                format!("Val::List(vec![{inner}])")
            }
            ExprKind::Map(_) => "Val::None".into(),
            ExprKind::Call { callee, args } => {
                if let ExprKind::Field { base, name } = &callee.kind {
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
                        _ => n.as_str(),
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
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
static FAILING: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
enum Val {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    None,
    List(Vec<Val>),
    Struct { tag: String, fields: Vec<(String, Val)> },
    Enum { ty: String, variant: String },
}
fn v_struct(tag: &str) -> Val { Val::Struct { tag: tag.into(), fields: vec![] } }
fn v_enum(ty: &str, variant: &str) -> Val { Val::Enum { ty: ty.into(), variant: variant.into() } }
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
        Val::Float(f) => f != 0.0,
        Val::Str(s) => !s.is_empty(),
        _ => true,
    }
}
fn as_f(a: &Val) -> f64 { match a { Val::Float(f) => *f, Val::Int(i) => *i as f64, _ => 0.0 } }
fn v_cmp(a: Val, b: Val) -> i32 {
    let x = as_f(&a); let y = as_f(&b);
    if x < y { -1 } else if x > y { 1 } else { 0 }
}
fn v_add(a: Val, b: Val) -> Val {
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)+as_f(&b)),
        (Val::Int(x), Val::Int(y)) => Val::Int(x+y),
        _ => Val::Int(0),
    }
}
fn v_sub(a: Val, b: Val) -> Val {
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)-as_f(&b)),
        (Val::Int(x), Val::Int(y)) => Val::Int(x-y),
        _ => Val::Int(0),
    }
}
fn v_mul(a: Val, b: Val) -> Val {
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)*as_f(&b)),
        (Val::Int(x), Val::Int(y)) => Val::Int(x*y),
        _ => Val::Int(0),
    }
}
fn v_div(a: Val, b: Val) -> Val {
    match (&a,&b) {
        (Val::Float(_), _) | (_, Val::Float(_)) => Val::Float(as_f(&a)/as_f(&b)),
        (Val::Int(x), Val::Int(y)) if *y != 0 => Val::Int(x/y),
        _ => Val::Int(0),
    }
}
fn v_mod(a: Val, b: Val) -> Val { match (a,b) { (Val::Int(x), Val::Int(y)) if y != 0 => Val::Int(x%y), _ => Val::Int(0) } }
fn v_neg(a: Val) -> Val { match a { Val::Float(f) => Val::Float(-f), Val::Int(i) => Val::Int(-i), x => x } }
fn v_to_str(a: Val) -> Val {
    Val::Str(match a {
        Val::Int(i) => i.to_string(),
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
fn cuni_say(v: Val) {
    match v {
        Val::Int(i) => println!("{i}"),
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

//! In-process CuNi runner. Smallest path: parse → typeck → evaluate.
//! No emit, no subprocess. `cuni check` remains the exactness proof.

use crate::ast::*;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(i64),
    Float(f64),
    /// `dec`: scaled integer, scale 10⁴ (docs/DECIMAL.md). i128 — the
    /// interpreter is a wide seat; checked ops refuse on true overflow.
    Dec(i128),
    /// `time`: int64 unix epoch seconds, UTC (docs/TIME.md). The
    /// interpreter is a wide seat; checked ops refuse on true overflow.
    Time(i64),
    Bool(bool),
    Str(String),
    None,
    List(Vec<Val>),
    Map(Vec<(Val, Val)>),
    Struct {
        name: String,
        fields: Vec<(String, Val)>,
    },
    Enum {
        ty: String,
        variant: String,
    },
}

enum Flow {
    Next,
    Ret(Val),
    Fail(Val),
}

struct Vm<'a> {
    fns: HashMap<String, &'a FnDecl>,
    typs: HashMap<String, &'a TypDecl>,
    enums: HashMap<String, Vec<String>>,
    out: String,
}

pub fn run(program: &Program) -> Result<String, String> {
    if program.items.iter().any(|i| matches!(i, Item::Ext(_))) {
        return Err("cuni run refuses `ext` (not portable). Proof is `cuni check`; seat emit is `cuni run --lang py`".into());
    }
    let mut vm = Vm {
        fns: HashMap::new(),
        typs: HashMap::new(),
        enums: HashMap::new(),
        out: String::new(),
    };
    for item in &program.items {
        match item {
            Item::Def(f) => {
                vm.fns.insert(f.name.clone(), f);
            }
            Item::Typ(t) => {
                vm.typs.insert(t.name.clone(), t);
            }
            Item::Enum(e) => {
                vm.enums.insert(
                    e.name.clone(),
                    e.variants.iter().map(|v| v.name.clone()).collect(),
                );
            }
            _ => {}
        }
    }
    let mut env: HashMap<String, Val> = HashMap::new();
    for item in &program.items {
        if let Item::Stmt(s) = item {
            match vm.stmt(s, &mut env)? {
                Flow::Next => {}
                Flow::Ret(_) => break,
                Flow::Fail(v) => return Err(format!("fail: {}", vm.fmt(&v))),
            }
        }
    }
    Ok(vm.out)
}

impl<'a> Vm<'a> {
    fn stmt(&mut self, s: &Stmt, env: &mut HashMap<String, Val>) -> Result<Flow, String> {
        match &s.kind {
            StmtKind::Let { name, value, .. } | StmtKind::Mut { name, value, .. } => {
                match self.eval_flow(value, env)? {
                    Ok(v) => {
                        env.insert(name.clone(), v);
                        Ok(Flow::Next)
                    }
                    Err(Flow::Ret(v)) => Ok(Flow::Ret(v)),
                    Err(Flow::Fail(v)) => Ok(Flow::Fail(v)),
                    Err(Flow::Next) => {
                        env.insert(name.clone(), Val::None);
                        Ok(Flow::Next)
                    }
                }
            }
            StmtKind::Assign { target, value } => {
                let v = self.eval(value, env)?;
                match &target.kind {
                    ExprKind::Ident(n) => {
                        env.insert(n.clone(), v);
                    }
                    ExprKind::Index { base, index } => {
                        let i = self.eval(index, env)?.as_int()?;
                        if let ExprKind::Ident(n) = &base.kind {
                            let xs = env.get_mut(n).ok_or_else(|| format!("undefined `{n}`"))?;
                            if let Val::List(items) = xs {
                                let u = as_index(i, items.len())?;
                                items[u] = v;
                            } else {
                                return Err("index assign on non-list".into());
                            }
                        } else {
                            return Err("index assign needs a name".into());
                        }
                    }
                    _ => return Err("assign target must be a name".into()),
                }
                Ok(Flow::Next)
            }
            StmtKind::Ret(None) => Ok(Flow::Ret(Val::None)),
            StmtKind::Ret(Some(e)) => Ok(Flow::Ret(self.eval(e, env)?)),
            StmtKind::Fail(e) => Ok(Flow::Fail(self.eval(e, env)?)),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                if self.eval(cond, env)?.truthy() {
                    self.block(then_body, env)
                } else if let Some(eb) = else_body {
                    self.block(eb, env)
                } else {
                    Ok(Flow::Next)
                }
            }
            StmtKind::Whl { cond, body } => {
                loop {
                    if !self.eval(cond, env)?.truthy() {
                        break;
                    }
                    match self.block(body, env)? {
                        Flow::Next => {}
                        other => return Ok(other),
                    }
                }
                Ok(Flow::Next)
            }
            StmtKind::For {
                binding,
                iter,
                body,
            } => {
                let seq = self.eval(iter, env)?;
                let pairs = seq.iter_pairs()?;
                for (k, v) in pairs {
                    env.insert(binding.0.clone(), k);
                    if let Some(b2) = &binding.1 {
                        env.insert(b2.clone(), v.clone());
                    } else {
                        env.insert(binding.0.clone(), v);
                    }
                    match self.block(body, env)? {
                        Flow::Next => {}
                        other => return Ok(other),
                    }
                }
                Ok(Flow::Next)
            }
            StmtKind::ExprStmt(e) => {
                if let ExprKind::Call { callee, args } = &e.kind {
                    if let ExprKind::Field { base, name } = &callee.kind {
                        if name == "push" {
                            if let ExprKind::Ident(n) = &base.kind {
                                let x = self
                                    .eval(args.first().ok_or("push needs a value")?.expr(), env)?;
                                match env.get_mut(n) {
                                    Some(Val::List(xs)) => {
                                        xs.push(x);
                                        return Ok(Flow::Next);
                                    }
                                    _ => return Err(format!("`.push` on non-list `{n}`")),
                                }
                            }
                        }
                    }
                }
                match self.eval_flow(e, env)? {
                    Ok(_) => Ok(Flow::Next),
                    Err(f) => Ok(f),
                }
            }
            StmtKind::Todo => Err("... (todo)".into()),
        }
    }

    fn block(&mut self, body: &[Stmt], env: &mut HashMap<String, Val>) -> Result<Flow, String> {
        for s in body {
            match self.stmt(s, env)? {
                Flow::Next => {}
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
    }

    fn eval(&mut self, e: &Expr, env: &mut HashMap<String, Val>) -> Result<Val, String> {
        match self.eval_flow(e, env)? {
            Ok(v) => Ok(v),
            Err(Flow::Fail(v)) => Err(format!("fail: {}", self.fmt(&v))),
            Err(Flow::Ret(v)) => Ok(v),
            Err(Flow::Next) => Ok(Val::None),
        }
    }

    fn eval_flow(
        &mut self,
        e: &Expr,
        env: &mut HashMap<String, Val>,
    ) -> Result<Result<Val, Flow>, String> {
        if let ExprKind::Unwrap { expr, handler } = &e.kind {
            let inner = self.eval_flow(expr, env)?;
            let miss = match &inner {
                Ok(Val::None) => true,
                Err(Flow::Fail(_)) => true,
                _ => false,
            };
            if miss {
                return match self.block(handler, env)? {
                    Flow::Next => Ok(Ok(Val::None)),
                    Flow::Ret(v) => Ok(Err(Flow::Ret(v))),
                    Flow::Fail(v) => Ok(Err(Flow::Fail(v))),
                };
            }
            return Ok(inner);
        }
        let v = match &e.kind {
            ExprKind::Int(n) => Val::Int(*n),
            ExprKind::Float(f) => Val::Float(*f),
            ExprKind::Dec(s) => Val::Dec(*s),
            ExprKind::Time(e) => Val::Time(*e),
            ExprKind::Bool(b) => Val::Bool(*b),
            ExprKind::Str(s) => Val::Str(s.clone()),
            ExprKind::NoneLit => Val::None,
            ExprKind::Ident(n) => env
                .get(n)
                .cloned()
                .ok_or_else(|| format!("undefined `{n}`"))?,
            ExprKind::List(xs) => {
                let mut out = Vec::new();
                for x in xs {
                    out.push(self.eval(x, env)?);
                }
                Val::List(out)
            }
            ExprKind::Map(pairs) => {
                let mut out = Vec::new();
                for (k, v) in pairs {
                    out.push((self.eval(k, env)?, self.eval(v, env)?));
                }
                Val::Map(out)
            }
            ExprKind::InterpStr(parts) => {
                let mut s = String::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => s.push_str(t),
                        StrPartExpr::Expr(ex) => {
                            let v = self.eval(ex, env)?;
                            s.push_str(&self.fmt(&v));
                        }
                    }
                }
                Val::Str(s)
            }
            ExprKind::Unary { op, expr } => {
                let v = self.eval(expr, env)?;
                match op {
                    UnOp::Not => Val::Bool(!v.truthy()),
                    UnOp::Neg => match v {
                        Val::Int(n) => Val::Int(-n),
                        Val::Float(f) => Val::Float(-f),
                        Val::Dec(d) => Val::Dec(
                            d.checked_neg()
                                .ok_or("cuni: dec negation overflow — refused")?,
                        ),
                        Val::Time(t) => Val::Time(
                            t.checked_neg()
                                .ok_or("cuni: time negation overflow — refused")?,
                        ),
                        _ => return Err("negation needs a number".into()),
                    },
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                if matches!(op, BinOp::And) {
                    let l = self.eval(lhs, env)?;
                    if !l.truthy() {
                        Val::Bool(false)
                    } else {
                        Val::Bool(self.eval(rhs, env)?.truthy())
                    }
                } else if matches!(op, BinOp::Or) {
                    let l = self.eval(lhs, env)?;
                    if l.truthy() {
                        Val::Bool(true)
                    } else {
                        Val::Bool(self.eval(rhs, env)?.truthy())
                    }
                } else {
                    let l = self.eval(lhs, env)?;
                    let r = self.eval(rhs, env)?;
                    bin(*op, l, r)?
                }
            }
            ExprKind::Index { base, index } => {
                let b = self.eval(base, env)?;
                let i = self.eval(index, env)?;
                index_get(&b, &i)?
            }
            ExprKind::Field { base, name } => {
                if let ExprKind::Ident(en) = &base.kind {
                    if let Some(vars) = self.enums.get(en) {
                        if vars.iter().any(|v| v == name) {
                            return Ok(Ok(Val::Enum {
                                ty: en.clone(),
                                variant: name.clone(),
                            }));
                        }
                    }
                }
                let b = self.eval(base, env)?;
                field_get(&b, name)?
            }
            ExprKind::Call { callee, args } => {
                return self.call(callee, args, env);
            }
            ExprKind::Unwrap { .. } => unreachable!(),
        };
        Ok(Ok(v))
    }

    fn call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        env: &mut HashMap<String, Val>,
    ) -> Result<Result<Val, Flow>, String> {
        if let ExprKind::Field { base, name } = &callee.kind {
            // Wave-1 stdlib namespaces (`json`, `time`): reserved, so the
            // base is never a variable — dispatch without evaluating it.
            if let ExprKind::Ident(ns) = &base.kind {
                if ns == "json" || ns == "time" {
                    let av: Vec<Val> = args
                        .iter()
                        .map(|a| self.eval(a.expr(), env))
                        .collect::<Result<_, _>>()?;
                    return Ok(Ok(stdlib_ns(ns, name, &av)?));
                }
            }
            let b = self.eval(base, env)?;
            let av: Vec<Val> = args
                .iter()
                .map(|a| self.eval(a.expr(), env))
                .collect::<Result<_, _>>()?;
            return Ok(Ok(method(name, b, &av)?));
        }
        if let ExprKind::Ident(fname) = &callee.kind {
            let av: Vec<Val> = args
                .iter()
                .map(|a| self.eval(a.expr(), env))
                .collect::<Result<_, _>>()?;
            match fname.as_str() {
                "say" => {
                    let v = av.first().cloned().unwrap_or(Val::None);
                    self.out.push_str(&self.fmt(&v));
                    self.out.push('\n');
                    return Ok(Ok(Val::None));
                }
                "range" => {
                    let n = av.first().ok_or("range needs n")?.as_int()?;
                    let mut xs = Vec::new();
                    if n > 0 {
                        for i in 0..n {
                            xs.push(Val::Int(i));
                        }
                    }
                    return Ok(Ok(Val::List(xs)));
                }
                "abs" => {
                    let n = av.first().ok_or("abs needs n")?.as_int()?;
                    return Ok(Ok(Val::Int(n.unsigned_abs() as i64)));
                }
                "min" => {
                    let a = av.first().ok_or("min")?.as_int()?;
                    let b = av.get(1).ok_or("min")?.as_int()?;
                    return Ok(Ok(Val::Int(a.min(b))));
                }
                "max" => {
                    let a = av.first().ok_or("max")?.as_int()?;
                    let b = av.get(1).ok_or("max")?.as_int()?;
                    return Ok(Ok(Val::Int(a.max(b))));
                }
                "dec_of_int" => {
                    let n = av.first().ok_or("dec_of_int needs n")?.as_int()?;
                    return Ok(Ok(Val::Dec(n as i128 * crate::ast::DEC_SCALE)));
                }
                "int_of_dec" => {
                    let d = match av.first().ok_or("int_of_dec needs d")? {
                        Val::Dec(d) => *d,
                        _ => return Err("int_of_dec needs a dec".into()),
                    };
                    // Truncation toward zero (docs/DECIMAL.md §5).
                    let q = d / crate::ast::DEC_SCALE;
                    let n: i64 = q.try_into().map_err(|_| {
                        "cuni: int_of_dec result out of int range — refused".to_string()
                    })?;
                    return Ok(Ok(Val::Int(n)));
                }
                // `time` builtins (docs/TIME.md §5).
                "parse_time" => {
                    let s = match av.first().ok_or("parse_time needs s")? {
                        Val::Str(s) => s.clone(),
                        _ => return Err("parse_time needs a string".into()),
                    };
                    // Strict ISO-8601 UTC only: bad input is a loud
                    // refusal, never a silent value.
                    return match crate::ast::parse_time_epoch(&s) {
                        Ok(e) => Ok(Ok(Val::Time(e))),
                        Err(msg) => Err(format!("cuni: parse_time: {msg}")),
                    };
                }
                "add_seconds" => {
                    let (t, s) = match (av.first(), av.get(1)) {
                        (Some(Val::Time(t)), Some(Val::Int(s))) => (*t, *s),
                        _ => return Err("add_seconds needs (time, int)".into()),
                    };
                    return match t.checked_add(s) {
                        Some(e) => Ok(Ok(Val::Time(e))),
                        None => Err("cuni: add_seconds overflow — refused".into()),
                    };
                }
                "days_between" => {
                    let (a, b) = match (av.first(), av.get(1)) {
                        (Some(Val::Time(a)), Some(Val::Time(b))) => (*a, *b),
                        _ => return Err("days_between needs (time, time)".into()),
                    };
                    // Truncation toward zero (docs/TIME.md §5); i64 `/`
                    // truncates natively.
                    return match a.checked_sub(b) {
                        Some(d) => Ok(Ok(Val::Int(d / 86400))),
                        None => Err("cuni: days_between overflow — refused".into()),
                    };
                }
                // Wave-1 stdlib (docs/STDLIB.md §4).
                "sha256" => {
                    let s = av.first().ok_or("sha256 needs a string")?;
                    match s {
                        Val::Str(t) => return Ok(Ok(Val::Str(sha256_hex(t)))),
                        _ => return Err("sha256 needs a str".into()),
                    }
                }
                _ => {}
            }
            if let Some(t) = self.typs.get(fname).copied() {
                let mut fields = Vec::new();
                if args.iter().all(|a| a.is_named()) && !args.is_empty() {
                    for f in &t.fields {
                        let v = args
                            .iter()
                            .find_map(|a| match a {
                                CallArg::Named { name, value, .. } if name == &f.name => {
                                    Some(value)
                                }
                                _ => None,
                            })
                            .ok_or_else(|| format!("missing field {}", f.name))?;
                        fields.push((f.name.clone(), self.eval(v, env)?));
                    }
                } else {
                    if av.len() != t.fields.len() {
                        return Err(format!("`{}` wants {} fields", t.name, t.fields.len()));
                    }
                    for (f, v) in t.fields.iter().zip(av.into_iter()) {
                        fields.push((f.name.clone(), v));
                    }
                }
                return Ok(Ok(Val::Struct {
                    name: t.name.clone(),
                    fields,
                }));
            }
            if let Some(f) = self.fns.get(fname).copied() {
                if av.len() != f.params.len() {
                    return Err(format!("`{}` expects {} args", f.name, f.params.len()));
                }
                let mut local: HashMap<String, Val> = HashMap::new();
                for (p, v) in f.params.iter().zip(av.into_iter()) {
                    local.insert(p.name.clone(), v);
                }
                for s in &f.body {
                    match self.stmt(s, &mut local)? {
                        Flow::Next => {}
                        Flow::Ret(v) => return Ok(Ok(v)),
                        Flow::Fail(v) => return Ok(Err(Flow::Fail(v))),
                    }
                }
                return Ok(Ok(Val::None));
            }
            return Err(format!("undefined function `{fname}`"));
        }
        Err("call of non-name".into())
    }

    fn fmt(&self, v: &Val) -> String {
        match v {
            Val::Int(n) => n.to_string(),
            Val::Dec(d) => crate::ast::fmt_dec_scaled(*d),
            Val::Time(e) => crate::ast::fmt_time_epoch(*e),
            Val::Float(f) => {
                let s = format!("{f}");
                if s.contains('.') || s.contains('e') || s.contains('E') {
                    s
                } else {
                    format!("{s}.0")
                }
            }
            Val::Bool(true) => "True".into(),
            Val::Bool(false) => "False".into(),
            Val::Str(s) => s.clone(),
            Val::None => "None".into(),
            Val::List(xs) => {
                let inner: Vec<String> = xs.iter().map(|x| self.fmt(x)).collect();
                format!("[{}]", inner.join(", "))
            }
            Val::Map(pairs) => {
                let inner: Vec<String> = pairs
                    .iter()
                    .map(|(k, v)| format!("{}: {}", self.fmt(k), self.fmt(v)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            Val::Struct { name, fields } => {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", self.fmt(v)))
                    .collect();
                format!("{name}({})", inner.join(", "))
            }
            Val::Enum { ty, variant } => format!("{ty}.{variant}"),
        }
    }
}

impl Val {
    fn truthy(&self) -> bool {
        match self {
            Val::None => false,
            Val::Bool(b) => *b,
            Val::Int(0) => false,
            Val::Dec(d) if *d == 0 => false,
            Val::Time(t) if *t == 0 => false,
            Val::Float(f) if *f == 0.0 => false,
            Val::Str(s) if s.is_empty() => false,
            Val::List(xs) if xs.is_empty() => false,
            _ => true,
        }
    }

    fn as_int(&self) -> Result<i64, String> {
        match self {
            Val::Int(n) => Ok(*n),
            Val::Float(f) => Ok(*f as i64),
            _ => Err("expected int".into()),
        }
    }

    fn iter_pairs(&self) -> Result<Vec<(Val, Val)>, String> {
        match self {
            Val::List(xs) => Ok(xs
                .iter()
                .enumerate()
                .map(|(i, v)| (Val::Int(i as i64), v.clone()))
                .collect()),
            Val::Map(pairs) => Ok(pairs.clone()),
            _ => Err("for-in needs a list or map".into()),
        }
    }
}

fn as_index(i: i64, n: usize) -> Result<usize, String> {
    if i < 0 || i as usize >= n {
        Err("index out of range".into())
    } else {
        Ok(i as usize)
    }
}

fn index_get(b: &Val, i: &Val) -> Result<Val, String> {
    match b {
        Val::List(xs) => {
            let u = as_index(i.as_int()?, xs.len())?;
            Ok(xs[u].clone())
        }
        Val::Map(pairs) => {
            for (k, v) in pairs {
                if k == i {
                    return Ok(v.clone());
                }
            }
            Ok(Val::None)
        }
        Val::Str(s) => {
            let u = as_index(i.as_int()?, s.len())?;
            Ok(Val::Str(s[u..u + 1].to_string()))
        }
        _ => Err("cannot index".into()),
    }
}

fn field_get(b: &Val, name: &str) -> Result<Val, String> {
    match b {
        Val::Struct { fields, .. } => fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("no field `{name}`")),
        Val::Enum { variant, .. } if variant == name => Ok(b.clone()),
        _ => Err(format!("no field `{name}`")),
    }
}

fn method(name: &str, b: Val, args: &[Val]) -> Result<Val, String> {
    match name {
        "len" => match b {
            Val::List(xs) => Ok(Val::Int(xs.len() as i64)),
            Val::Str(s) => Ok(Val::Int(s.len() as i64)),
            Val::Map(p) => Ok(Val::Int(p.len() as i64)),
            _ => Err(".len on non-collection".into()),
        },
        "push" => Err(".push must be a statement on a mut name".into()),
        // Wave-1 stdlib string ops (docs/STDLIB.md §3): byte-oriented on
        // UTF-8; the empty-separator and non-str-part cases refuse.
        "split" => match b {
            Val::Str(s) => {
                let sep_v = args.first().ok_or(".split needs a separator")?;
                let sep = match sep_v {
                    Val::Str(t) => t,
                    _ => return Err(".split needs a str separator".into()),
                };
                if sep.is_empty() {
                    return Err(".split: empty separator; refusing".into());
                }
                Ok(Val::List(
                    s.split(sep.as_str())
                        .map(|p| Val::Str(p.to_string()))
                        .collect(),
                ))
            }
            _ => Err(".split needs a str".into()),
        },
        "join" => match b {
            Val::Str(sep) => {
                let parts_v = args.first().ok_or(".join needs a list")?;
                match parts_v {
                    Val::List(xs) => {
                        let mut ss = Vec::with_capacity(xs.len());
                        for x in xs {
                            match x {
                                Val::Str(t) => ss.push(t.clone()),
                                _ => return Err(".join: all parts must be str".into()),
                            }
                        }
                        Ok(Val::Str(ss.join(sep.as_str())))
                    }
                    _ => Err(".join needs a list<str>".into()),
                }
            }
            _ => Err(".join needs a str separator".into()),
        },
        "trim" => match b {
            Val::Str(s) => Ok(Val::Str(
                s.trim_matches(|c: char| {
                    matches!(c, '\x09' | '\x0A' | '\x0B' | '\x0C' | '\x0D' | '\x20')
                })
                .to_string(),
            )),
            _ => Err(".trim needs a str".into()),
        },
        "contains" => match b {
            Val::Str(s) => {
                let sub_v = args.first().ok_or(".contains needs a substring")?;
                match sub_v {
                    Val::Str(t) => Ok(Val::Bool(s.contains(t.as_str()))),
                    _ => return Err(".contains needs a str".into()),
                }
            }
            _ => Err(".contains needs a str".into()),
        },
        "slice" => {
            let a = args.first().ok_or("slice")?.as_int()?;
            let b2 = args.get(1).ok_or("slice")?.as_int()?;
            match b {
                Val::Str(s) => {
                    let n = s.len() as i64;
                    if a < 0 || b2 < 0 || a > n || b2 > n || a > b2 {
                        Ok(Val::Str(String::new()))
                    } else {
                        Ok(Val::Str(s[a as usize..b2 as usize].to_string()))
                    }
                }
                Val::List(xs) => {
                    let n = xs.len() as i64;
                    if a < 0 || b2 < 0 || a > n || b2 > n || a > b2 {
                        Ok(Val::List(vec![]))
                    } else {
                        Ok(Val::List(xs[a as usize..b2 as usize].to_vec()))
                    }
                }
                _ => Err(".slice on non-list/str".into()),
            }
        }
        _ => Err(format!("unknown method `.{name}`")),
    }
}

fn bin(op: BinOp, l: Val, r: Val) -> Result<Val, String> {
    // `dec` is a closed world (docs/DECIMAL.md §3–5): both operands dec, or
    // neither. The typeck already refused mixes; this is defense in depth.
    if matches!(l, Val::Dec(_)) || matches!(r, Val::Dec(_)) {
        return dec_bin(op, l, r);
    }
    // `time` is a closed world too (docs/TIME.md §3): the typeck proved the
    // valid shapes; anything else is a loud refusal, never a silent value.
    if matches!(l, Val::Time(_)) || matches!(r, Val::Time(_)) {
        return time_bin(op, l, r);
    }
    match op {
        BinOp::Eq => Ok(Val::Bool(eq(&l, &r))),
        BinOp::Ne => Ok(Val::Bool(!eq(&l, &r))),
        BinOp::Lt => Ok(Val::Bool(cmp(&l, &r)? < 0)),
        BinOp::Gt => Ok(Val::Bool(cmp(&l, &r)? > 0)),
        BinOp::Le => Ok(Val::Bool(cmp(&l, &r)? <= 0)),
        BinOp::Ge => Ok(Val::Bool(cmp(&l, &r)? >= 0)),
        BinOp::Add => match (l, r) {
            (Val::Int(a), Val::Int(b)) => Ok(Val::Int(a.wrapping_add(b))),
            (Val::Float(a), Val::Float(b)) => Ok(Val::Float(a + b)),
            (Val::Int(a), Val::Float(b)) => Ok(Val::Float(a as f64 + b)),
            (Val::Float(a), Val::Int(b)) => Ok(Val::Float(a + b as f64)),
            (Val::Str(a), Val::Str(b)) => Ok(Val::Str(a + &b)),
            _ => Err("+ type mismatch".into()),
        },
        BinOp::Sub => num2(l, r, |a, b| a.wrapping_sub(b), |a, b| a - b),
        BinOp::Mul => num2(l, r, |a, b| a.wrapping_mul(b), |a, b| a * b),
        BinOp::Div => match (l, r) {
            (Val::Int(a), Val::Int(b)) if b != 0 => Ok(Val::Int(a / b)),
            (Val::Float(a), Val::Float(b)) => Ok(Val::Float(a / b)),
            (Val::Int(a), Val::Float(b)) => Ok(Val::Float(a as f64 / b)),
            (Val::Float(a), Val::Int(b)) => Ok(Val::Float(a / b as f64)),
            _ => Err("/ type mismatch or div0".into()),
        },
        BinOp::Mod => match (l, r) {
            // CuNi `%` is Python-floored (not Rust-truncated).
            (Val::Int(a), Val::Int(b)) if b != 0 => {
                let r = a % b;
                let floored = if r != 0 && ((r < 0) != (b < 0)) { r + b } else { r };
                Ok(Val::Int(floored))
            }
            _ => Err("% needs ints".into()),
        },
        BinOp::And | BinOp::Or => unreachable!(),
    }
}

/// Exact `dec` arithmetic on scaled i128 values (docs/DECIMAL.md §3).
/// Division truncates toward zero (i128 `/` already does); every true
/// overflow is a loud refusal, never a wrap.
fn dec_bin(op: BinOp, l: Val, r: Val) -> Result<Val, String> {
    let (Val::Dec(a), Val::Dec(b)) = (l, r) else {
        return Err(
            "cannot mix `dec` with a non-dec value — convert explicitly: `dec_of_int(n)` / `int_of_dec(d)`"
                .into(),
        );
    };
    const S: i128 = crate::ast::DEC_SCALE;
    match op {
        BinOp::Add => Ok(Val::Dec(
            a.checked_add(b)
                .ok_or("cuni: dec addition overflow — refused")?,
        )),
        BinOp::Sub => Ok(Val::Dec(
            a.checked_sub(b)
                .ok_or("cuni: dec subtraction overflow — refused")?,
        )),
        BinOp::Mul => {
            let p = a
                .checked_mul(b)
                .ok_or("cuni: dec multiplication overflow — refused")?;
            Ok(Val::Dec(p / S))
        }
        BinOp::Div => {
            if b == 0 {
                return Err("cuni: dec division by zero".into());
            }
            let p = a
                .checked_mul(S)
                .ok_or("cuni: dec division intermediate overflow — refused")?;
            Ok(Val::Dec(p / b))
        }
        BinOp::Mod => Err("`%` is not defined on `dec` — refusing".into()),
        BinOp::Eq => Ok(Val::Bool(a == b)),
        BinOp::Ne => Ok(Val::Bool(a != b)),
        BinOp::Lt => Ok(Val::Bool(a < b)),
        BinOp::Gt => Ok(Val::Bool(a > b)),
        BinOp::Le => Ok(Val::Bool(a <= b)),
        BinOp::Ge => Ok(Val::Bool(a >= b)),
        BinOp::And | BinOp::Or => unreachable!(),
    }
}

/// Exact `time` arithmetic on int64 epoch seconds (docs/TIME.md §3).
/// `time` is a closed world like `dec`: the typeck proved the valid
/// shapes (`time ± int`, `time - time`, `time` comparisons); anything else
/// is a loud refusal. `duration` is plain `int` seconds — there is no
/// implicit time<->int conversion.
fn time_bin(op: BinOp, l: Val, r: Val) -> Result<Val, String> {
    let mix_err = || {
        "cannot mix `time` with a non-`time`/`int` value — durations are plain `int` seconds; `parse_time(s)` turns an ISO-8601 string into a time (docs/TIME.md §5)"
            .to_string()
    };
    match op {
        BinOp::Add => match (l, r) {
            (Val::Time(t), Val::Int(s)) | (Val::Int(s), Val::Time(t)) => Ok(Val::Time(
                t.checked_add(s)
                    .ok_or("cuni: time addition overflow — refused")?,
            )),
            _ => Err(mix_err()),
        },
        BinOp::Sub => match (l, r) {
            (Val::Time(t), Val::Int(s)) => Ok(Val::Time(
                t.checked_sub(s)
                    .ok_or("cuni: time subtraction overflow — refused")?,
            )),
            (Val::Time(a), Val::Time(b)) => Ok(Val::Int(
                a.checked_sub(b)
                    .ok_or("cuni: time difference overflow — refused")?,
            )),
            _ => Err(mix_err()),
        },
        BinOp::Mul | BinOp::Div | BinOp::Mod => {
            Err("arithmetic `*`/`/`/`%` is not defined on `time` — refusing (docs/TIME.md §3)".into())
        }
        BinOp::Eq => match (l, r) {
            (Val::Time(a), Val::Time(b)) => Ok(Val::Bool(a == b)),
            _ => Err(mix_err()),
        },
        BinOp::Ne => match (l, r) {
            (Val::Time(a), Val::Time(b)) => Ok(Val::Bool(a != b)),
            _ => Err(mix_err()),
        },
        BinOp::Lt => match (l, r) {
            (Val::Time(a), Val::Time(b)) => Ok(Val::Bool(a < b)),
            _ => Err(mix_err()),
        },
        BinOp::Gt => match (l, r) {
            (Val::Time(a), Val::Time(b)) => Ok(Val::Bool(a > b)),
            _ => Err(mix_err()),
        },
        BinOp::Le => match (l, r) {
            (Val::Time(a), Val::Time(b)) => Ok(Val::Bool(a <= b)),
            _ => Err(mix_err()),
        },
        BinOp::Ge => match (l, r) {
            (Val::Time(a), Val::Time(b)) => Ok(Val::Bool(a >= b)),
            _ => Err(mix_err()),
        },
        BinOp::And | BinOp::Or => unreachable!(),
    }
}

fn num2(l: Val, r: Val, i: fn(i64, i64) -> i64, f: fn(f64, f64) -> f64) -> Result<Val, String> {
    match (l, r) {
        (Val::Int(a), Val::Int(b)) => Ok(Val::Int(i(a, b))),
        (Val::Float(a), Val::Float(b)) => Ok(Val::Float(f(a, b))),
        (Val::Int(a), Val::Float(b)) => Ok(Val::Float(f(a as f64, b))),
        (Val::Float(a), Val::Int(b)) => Ok(Val::Float(f(a, b as f64))),
        _ => Err("numeric type mismatch".into()),
    }
}

fn eq(l: &Val, r: &Val) -> bool {
    match (l, r) {
        (Val::Int(a), Val::Int(b)) => a == b,
        (Val::Float(a), Val::Float(b)) => a == b,
        (Val::Bool(a), Val::Bool(b)) => a == b,
        (Val::Str(a), Val::Str(b)) => a == b,
        (Val::None, Val::None) => true,
        (
            Val::Enum {
                ty: t1,
                variant: v1,
            },
            Val::Enum {
                ty: t2,
                variant: v2,
            },
        ) => t1 == t2 && v1 == v2,
        _ => false,
    }
}

fn cmp(l: &Val, r: &Val) -> Result<i32, String> {
    fn ord(o: std::cmp::Ordering) -> i32 {
        match o {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }
    match (l, r) {
        (Val::Int(a), Val::Int(b)) => Ok(ord(a.cmp(b))),
        (Val::Float(a), Val::Float(b)) => {
            Ok(ord(a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)))
        }
        (Val::Str(a), Val::Str(b)) => Ok(ord(a.cmp(b))),
        _ => Err("cannot compare".into()),
    }
}

// ============================================================================
// Wave-1 stdlib (docs/STDLIB.md): JSON, time, SHA-256.
// This interpreter is the reference seat — every native seat implements the
// same algorithms, and the gate requires byte-identical stdout.
// ============================================================================

/// Largest/smallest JSON integer: ±(2^53 − 1), the safe-integer range.
const JSON_INT_MAX: i64 = 9_007_199_254_740_991;
const JSON_INT_MIN: i64 = -9_007_199_254_740_991;

fn stdlib_ns(ns: &str, name: &str, args: &[Val]) -> Result<Val, String> {
    match (ns, name) {
        ("json", "parse") => {
            let s = args.first().ok_or("json.parse needs a string")?;
            match s {
                Val::Str(t) => json_parse(t),
                _ => Err("json.parse needs a str".into()),
            }
        }
        ("json", "emit") => {
            let m = args.first().ok_or("json.emit needs a map")?;
            match m {
                Val::Map(_) => {
                    let mut out = String::new();
                    json_write(m, &mut out)?;
                    Ok(Val::Str(out))
                }
                _ => Err("json.emit needs a map".into()),
            }
        }
        ("time", "epoch") => time_epoch(args),
        ("time", "parts") => time_parts(args),
        _ => Err(format!("unknown stdlib function `{ns}.{name}`")),
    }
}

// ---------------- JSON ----------------

struct JParser<'a> {
    s: &'a str,
    b: &'a [u8],
    pos: usize,
}

impl<'a> JParser<'a> {
    fn ws(&mut self) {
        while matches!(self.b.get(self.pos), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }

    fn value(&mut self) -> Result<Val, String> {
        let c = *self.b.get(self.pos).ok_or("json.parse: unexpected end")?;
        match c {
            b'{' => self.object(),
            b'[' => self.array(),
            b'"' => Ok(Val::Str(self.string()?)),
            b't' => self.lit("true", Val::Bool(true)),
            b'f' => self.lit("false", Val::Bool(false)),
            b'n' => self.lit("null", Val::None),
            b'-' | b'0'..=b'9' => {
                let start = self.pos;
                while matches!(
                    self.b.get(self.pos),
                    Some(b'-' | b'+' | b'0'..=b'9' | b'.' | b'e' | b'E')
                ) {
                    self.pos += 1;
                }
                Ok(Val::Int(json_number(&self.s[start..self.pos])?))
            }
            _ => Err("json.parse: unexpected character".into()),
        }
    }

    fn lit(&mut self, word: &str, v: Val) -> Result<Val, String> {
        if self.s[self.pos..].starts_with(word) {
            self.pos += word.len();
            Ok(v)
        } else {
            Err("json.parse: bad literal".into())
        }
    }

    fn object(&mut self) -> Result<Val, String> {
        self.pos += 1; // {
        let mut pairs: Vec<(Val, Val)> = Vec::new();
        self.ws();
        if self.b.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(Val::Map(pairs));
        }
        loop {
            self.ws();
            if self.b.get(self.pos) != Some(&b'"') {
                return Err("json.parse: object keys must be strings".into());
            }
            let key = Val::Str(self.string()?);
            self.ws();
            if self.b.get(self.pos) != Some(&b':') {
                return Err("json.parse: expected ':'".into());
            }
            self.pos += 1;
            self.ws();
            let v = self.value()?;
            // Duplicate keys: last wins, first position kept.
            if let Some(slot) = pairs.iter_mut().find(|(k, _)| k == &key) {
                slot.1 = v;
            } else {
                pairs.push((key, v));
            }
            self.ws();
            match self.b.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Val::Map(pairs));
                }
                _ => return Err("json.parse: expected ',' or '}'".into()),
            }
        }
    }

    fn array(&mut self) -> Result<Val, String> {
        self.pos += 1; // [
        let mut xs = Vec::new();
        self.ws();
        if self.b.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(Val::List(xs));
        }
        loop {
            self.ws();
            xs.push(self.value()?);
            self.ws();
            match self.b.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Val::List(xs));
                }
                _ => return Err("json.parse: expected ',' or ']'".into()),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.b.len() {
            return Err("json.parse: bad \\u escape".into());
        }
        let h = std::str::from_utf8(&self.b[self.pos..self.pos + 4])
            .map_err(|_| "json.parse: bad \\u escape".to_string())?;
        let v = u32::from_str_radix(h, 16).map_err(|_| "json.parse: bad \\u escape".to_string())?;
        self.pos += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1; // opening "
        let mut out = String::new();
        loop {
            // Copy a maximal run of plain characters (input &str is valid
            // UTF-8, so slicing at the next '"' or '\\' is safe).
            let rest = &self.s[self.pos..];
            let run = rest
                .find(|c| c == '"' || c == '\\')
                .unwrap_or(rest.len());
            let chunk = &rest[..run];
            if chunk.bytes().any(|b| b < 0x20) {
                return Err("json.parse: unescaped control character in string".into());
            }
            out.push_str(chunk);
            self.pos += run;
            match self.b.get(self.pos) {
                None => return Err("json.parse: unterminated string".into()),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let e = *self
                        .b
                        .get(self.pos)
                        .ok_or("json.parse: unterminated escape")?;
                    self.pos += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\x08'),
                        b'f' => out.push('\x0c'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi) {
                                let is_lo = self.b.get(self.pos) == Some(&b'\\')
                                    && self.b.get(self.pos + 1) == Some(&b'u');
                                if !is_lo {
                                    return Err("json.parse: lone surrogate".into());
                                }
                                self.pos += 2;
                                let lo = self.hex4()?;
                                if !(0xDC00..0xE000).contains(&lo) {
                                    return Err("json.parse: lone surrogate".into());
                                }
                                let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                                out.push(
                                    char::from_u32(cp)
                                        .ok_or("json.parse: bad code point")?,
                                );
                            } else if (0xDC00..0xE000).contains(&hi) {
                                return Err("json.parse: lone surrogate".into());
                            } else {
                                out.push(
                                    char::from_u32(hi).ok_or("json.parse: bad code point")?,
                                );
                            }
                        }
                        _ => return Err("json.parse: bad escape".into()),
                    }
                }
                _ => unreachable!("run covered every non-quote non-backslash byte"),
            }
        }
    }
}

/// The spec's value-based integer rule (docs/STDLIB.md §1.1): accept iff the
/// number's exact value is an integer in ±(2^53−1).
fn json_number(tok: &str) -> Result<i64, String> {
    let bad = || "json.parse: bad number".to_string();
    let t = tok.as_bytes();
    let mut i = 0usize;
    let neg = if t.get(i) == Some(&b'-') {
        i += 1;
        true
    } else {
        false
    };
    if t.get(i) == Some(&b'0') {
        i += 1;
    } else if matches!(t.get(i), Some(b'1'..=b'9')) {
        while matches!(t.get(i), Some(b'0'..=b'9')) {
            i += 1;
        }
    } else {
        return Err(bad());
    }
    let mut frac_len = 0usize;
    if t.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while matches!(t.get(i), Some(b'0'..=b'9')) {
            i += 1;
        }
        if i == start {
            return Err(bad());
        }
        frac_len = i - start;
    }
    let mut exp: i64 = 0;
    // Mantissa ends where the exponent begins: only int+frac digits feed `d`.
    let mant_end_before_exp = i;
    if matches!(t.get(i), Some(b'e') | Some(b'E')) {
        i += 1;
        let eneg = if t.get(i) == Some(&b'-') {
            i += 1;
            true
        } else {
            if t.get(i) == Some(&b'+') {
                i += 1;
            }
            false
        };
        let start = i;
        while matches!(t.get(i), Some(b'0'..=b'9')) {
            i += 1;
        }
        if i == start {
            return Err(bad());
        }
        let ed: i64 = tok[start..i].parse().map_err(|_| bad())?;
        exp = if eneg { -ed } else { ed };
    }
    if i != t.len() {
        return Err(bad());
    }
    let refuse = || "json.parse: number is not an integer in ±(2^53−1)".to_string();
    // Significant digits (leading zeros stripped; all-zeros -> 0).
    // Mantissa only — exponent digits must NOT feed `d`.
    let digits: Vec<u8> = t[..mant_end_before_exp]
        .iter()
        .filter(|c| c.is_ascii_digit())
        .copied()
        .collect();
    let nz = digits.iter().position(|&c| c != b'0');
    let sig: &[u8] = match nz {
        Some(p) => &digits[p..],
        None => return Ok(0),
    };
    if sig.len() > 16 {
        return Err(refuse());
    }
    let mut d: i64 = 0;
    for &c in sig {
        d = d * 10 + (c - b'0') as i64;
    }
    let mut f = frac_len as i64;
    while d % 10 == 0 && d != 0 {
        d /= 10;
        f -= 1;
    }
    let k = f - exp;
    let mut v: i64;
    if k <= 0 {
        v = d;
        for _ in 0..(-k) {
            v = v.checked_mul(10).ok_or_else(refuse)?;
        }
    } else {
        if k > 16 {
            return Err(refuse());
        }
        let mut p10: i64 = 1;
        for _ in 0..k {
            p10 *= 10;
        }
        if d % p10 != 0 {
            return Err(refuse());
        }
        v = d / p10;
    }
    if neg {
        v = -v;
    }
    if v < JSON_INT_MIN || v > JSON_INT_MAX {
        return Err(refuse());
    }
    Ok(v)
}

fn json_parse(s: &str) -> Result<Val, String> {
    let mut p = JParser {
        s,
        b: s.as_bytes(),
        pos: 0,
    };
    p.ws();
    let v = p.value()?;
    p.ws();
    if p.pos != p.b.len() {
        return Err("json.parse: trailing characters".into());
    }
    match v {
        Val::Map(_) => Ok(v),
        _ => Err("json.parse: top-level JSON value must be an object".into()),
    }
}

/// Canonical minimal emit (docs/STDLIB.md §1.3): no whitespace, keys sorted
/// in UTF-8 byte order, lowercase-hex \u escapes, UTF-8 passthrough.
fn json_write(v: &Val, out: &mut String) -> Result<(), String> {
    match v {
        Val::Int(n) => out.push_str(&n.to_string()),
        Val::Str(s) => json_write_str(s, out),
        Val::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Val::None => out.push_str("null"),
        Val::List(xs) => {
            out.push('[');
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_write(x, out)?;
            }
            out.push(']');
        }
        Val::Map(pairs) => {
            let mut ks: Vec<(&str, &Val)> = Vec::with_capacity(pairs.len());
            for (k, val) in pairs {
                match k {
                    Val::Str(s) => ks.push((s.as_str(), val)),
                    _ => return Err("json.emit: map keys must be strings".into()),
                }
            }
            ks.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
            out.push('{');
            for (i, (k, val)) in ks.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_write_str(k, out);
                out.push(':');
                json_write(val, out)?;
            }
            out.push('}');
        }
        _ => {
            return Err(
                "json.emit: value has no JSON form (floats, structs and enums refuse)".into(),
            )
        }
    }
    Ok(())
}

fn json_write_str(s: &str, out: &mut String) {
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
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
}

// ---------------- time ----------------

/// Howard Hinnant's days_from_civil; proleptic Gregorian. All divisions are
/// on non-negative operands (y >= 1), so truncating `/` == floor.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y0 = if m <= 2 { y - 1 } else { y };
    let era = y0 / 400;
    let yoe = y0 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468; // >= 306 throughout our domain: non-negative
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

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn time_epoch(args: &[Val]) -> Result<Val, String> {
    let g = |i: usize| -> Result<i64, String> {
        args.get(i)
            .ok_or("time.epoch needs 6 arguments")?
            .as_int()
            .map_err(|_| "time.epoch needs int arguments".to_string())
    };
    let (y, mo, d, h, mi, s) = (g(0)?, g(1)?, g(2)?, g(3)?, g(4)?, g(5)?);
    if !(1 <= y && y <= 9999) {
        return Err("time.epoch: year out of range 1..9999".into());
    }
    if !(1 <= mo && mo <= 12) {
        return Err("time.epoch: month out of range 1..12".into());
    }
    if !(1 <= d && d <= days_in_month(y, mo)) {
        return Err("time.epoch: day out of range for month".into());
    }
    if !(0 <= h && h <= 23) {
        return Err("time.epoch: hour out of range 0..23".into());
    }
    if !(0 <= mi && mi <= 59) {
        return Err("time.epoch: minute out of range 0..59".into());
    }
    if !(0 <= s && s <= 59) {
        return Err("time.epoch: second out of range 0..59".into());
    }
    Ok(Val::Int(
        days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s,
    ))
}

fn time_parts(args: &[Val]) -> Result<Val, String> {
    let e = args
        .first()
        .ok_or("time.parts needs an epoch")?
        .as_int()
        .map_err(|_| "time.parts needs an int epoch".to_string())?;
    let lo = days_from_civil(1, 1, 1) * 86400; // -62135596800
    let hi = days_from_civil(9999, 12, 31) * 86400 + 86399; // 253402300799
    if e < lo || e > hi {
        return Err("time.parts: epoch out of range 1..9999".into());
    }
    let days = e.div_euclid(86400);
    let secs = e.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    let kv = |k: &str, v: i64| (Val::Str(k.to_string()), Val::Int(v));
    Ok(Val::Map(vec![
        kv("year", y),
        kv("month", mo),
        kv("day", d),
        kv("hour", secs / 3600),
        kv("min", (secs % 3600) / 60),
        kv("sec", secs % 60),
    ]))
}

// ---------------- SHA-256 ----------------

/// FIPS 180-4 SHA-256 over the input bytes; lowercase hex.
fn sha256_hex(s: &str) -> String {
    let mut msg = s.as_bytes().to_vec();
    let bit_len = (msg.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    let mut out = String::with_capacity(64);
    for x in h {
        out.push_str(&format!("{:08x}", x));
    }
    out
}

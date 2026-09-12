//! In-process CuNi runner. Smallest path: parse → typeck → evaluate.
//! No emit, no subprocess. `cuni check` remains the exactness proof.

use crate::ast::*;
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    None,
    List(Vec<Val>),
    Map(Vec<(Val, Val)>),
    Struct { name: String, fields: Vec<(String, Val)> },
    Enum { ty: String, variant: String },
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
                vm.enums.insert(e.name.clone(), e.variants.clone());
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
            StmtKind::For { binding, iter, body } => {
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
                                let x = self.eval(
                                    args.first().ok_or("push needs a value")?.expr(),
                                    env,
                                )?;
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
                        return Err(format!(
                            "`{}` wants {} fields",
                            t.name,
                            t.fields.len()
                        ));
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
                    return Err(format!(
                        "`{}` expects {} args",
                        f.name,
                        f.params.len()
                    ));
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
            (Val::Int(a), Val::Int(b)) if b != 0 => Ok(Val::Int(a % b)),
            _ => Err("% needs ints".into()),
        },
        BinOp::And | BinOp::Or => unreachable!(),
    }
}

fn num2(
    l: Val,
    r: Val,
    i: fn(i64, i64) -> i64,
    f: fn(f64, f64) -> f64,
) -> Result<Val, String> {
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
        (Val::Float(a), Val::Float(b)) => Ok(ord(a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))),
        (Val::Str(a), Val::Str(b)) => Ok(ord(a.cmp(b))),
        _ => Err("cannot compare".into()),
    }
}

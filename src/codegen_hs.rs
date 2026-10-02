//! Haskell backend — handwritten native seat (runghc).
//!
//! Emits real Haskell: every `def` becomes a function returning `IO`
//! (so `say` inside bodies is honest), `main :: IO ()`, explicit
//! `do { ...; ... }` braces so layout can't misfire. `div` truncates
//! toward zero (CuNi) and `mod` is already Python-floored, so both map
//! natively; `Integer` is arbitrary precision, matching CuNi. Booleans
//! print as `True`/`False` via `cuniSayBool`. Single-assignment is
//! handled by honest SSA (fresh `v<n>` per binding); `while` + mutated
//! state becomes a tail-recursive local `loop` threading the mutated
//! variables as parameters — an exact desugar, not approximation. A
//! `while` condition with effectful calls is evaluated inside the loop
//! (its call preludes run per iteration). Early `ret` inside a `while`
//! body cannot be threaded through the loop honestly, so it is refused.
//! Refuses everything else beyond the core subset (see
//! `crate::codegen_core` docs).

use crate::ast::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    Int,
    Str,
    Bool,
}

fn ty_of(t: &Type) -> Result<Ty, String> {
    match t {
        Type::Named(n) if n == "int" => Ok(Ty::Int),
        Type::Named(n) if n == "str" => Ok(Ty::Str),
        Type::Named(n) if n == "bool" => Ok(Ty::Bool),
        _ => Err("type beyond core subset; refusing".into()),
    }
}

fn hs_ty(ty: Ty) -> &'static str {
    match ty {
        Ty::Int => "Integer",
        Ty::Str => "String",
        Ty::Bool => "Bool",
    }
}

fn hs_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\x{:x}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

/// Normalized owned statement — same shape as the other seats.
#[derive(Clone)]
enum S<'a> {
    Let {
        name: &'a str,
        ty: Option<Ty>,
        value: &'a Expr,
    },
    Mut {
        name: &'a str,
        ty: Option<Ty>,
        value: &'a Expr,
    },
    Assign {
        name: &'a str,
        value: &'a Expr,
    },
    Ret(Option<&'a Expr>),
    If {
        cond: &'a Expr,
        then_body: Vec<S<'a>>,
        else_body: Vec<S<'a>>,
    },
    Whl {
        cond: &'a Expr,
        body: Vec<S<'a>>,
    },
    Say(&'a Expr),
}

fn diverges(stmts: &[S]) -> bool {
    match stmts.last() {
        Some(S::Ret(_)) => true,
        Some(S::If {
            then_body,
            else_body,
            ..
        }) => diverges(then_body) && diverges(else_body),
        _ => false,
    }
}

fn norm(items: Vec<S>) -> Vec<S> {
    let mut out: Vec<S> = Vec::new();
    let mut it = items.into_iter();
    while let Some(s) = it.next() {
        match s {
            S::If {
                cond,
                then_body,
                else_body,
            } if diverges(&then_body) || diverges(&else_body) => {
                let rest: Vec<S> = norm(it.collect());
                let then2 = if diverges(&then_body) {
                    then_body
                } else {
                    then_body.into_iter().chain(rest.clone()).collect()
                };
                let else2 = if diverges(&else_body) {
                    else_body
                } else {
                    else_body.into_iter().chain(rest).collect()
                };
                out.push(S::If {
                    cond,
                    then_body: then2,
                    else_body: else2,
                });
                return out;
            }
            S::Ret(_) => {
                out.push(s);
                return out;
            }
            _ => out.push(s),
        }
    }
    out
}

fn conv<'a>(stmts: &[&'a Stmt], in_loop: bool, in_def: bool) -> Result<Vec<S<'a>>, String> {
    let mut items = Vec::new();
    for s in stmts {
        match &s.kind {
            StmtKind::Let { name, ty, value } => items.push(S::Let {
                name,
                ty: ty.as_ref().map(ty_of).transpose()?,
                value,
            }),
            StmtKind::Mut { name, ty, value } => items.push(S::Mut {
                name,
                ty: ty.as_ref().map(ty_of).transpose()?,
                value,
            }),
            StmtKind::Assign { target, value } => match &target.kind {
                ExprKind::Ident(n) => items.push(S::Assign { name: n, value }),
                _ => return Err("complex assignment target: beyond core subset; refusing".into()),
            },
            StmtKind::Ret(e) => {
                if in_loop {
                    return Err("ret inside while: cannot thread early return through the loop desugar exactly; refusing".into());
                }
                if !in_def {
                    return Err("ret outside def: beyond core subset; refusing".into());
                }
                items.push(S::Ret(e.as_ref()));
            }
            StmtKind::Fail(_) => return Err("fail: beyond core subset; refusing".into()),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => items.push(S::If {
                cond,
                then_body: norm(conv(&refs(then_body), in_loop, in_def)?),
                else_body: norm(conv(&refs(else_body.as_deref().unwrap_or(&[])), in_loop, in_def)?),
            }),
            StmtKind::For { .. } => return Err("for: beyond core subset; refusing".into()),
            StmtKind::Whl { cond, body } => items.push(S::Whl {
                cond,
                body: norm(conv(&refs(body), true, in_def)?),
            }),
            StmtKind::ExprStmt(e) => match &e.kind {
                ExprKind::Call { callee, args } => {
                    if let ExprKind::Ident(n) = &callee.kind {
                        if n == "say" {
                            if args.len() != 1 || args[0].is_named() {
                                return Err(
                                    "say: exactly one positional argument in core subset; refusing"
                                        .into(),
                                );
                            }
                            items.push(S::Say(args[0].expr()));
                            continue;
                        }
                    }
                    return Err("bare call (non-say): beyond core subset; refusing".into());
                }
                _ => return Err("bare expression: beyond core subset; refusing".into()),
            },
            StmtKind::Todo => return Err("...: beyond core subset; refusing".into()),
        }
    }
    Ok(norm(items))
}

fn refs<'a>(stmts: &'a [Stmt]) -> Vec<&'a Stmt> {
    stmts.iter().collect()
}

fn assigned_outer(stmts: &[S], outer: &HashSet<String>) -> Vec<String> {
    fn walk(stmts: &[S], outer: &HashSet<String>, seen: &mut HashSet<String>, out: &mut Vec<String>) {
        for s in stmts {
            match s {
                S::Let { name, .. } | S::Mut { name, .. } | S::Assign { name, .. } => {
                    if outer.contains(*name) && seen.insert(name.to_string()) {
                        out.push(name.to_string());
                    }
                }
                S::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    walk(then_body, outer, seen, out);
                    walk(else_body, outer, seen, out);
                }
                S::Whl { body, .. } => walk(body, outer, seen, out),
                S::Ret(_) | S::Say(_) => {}
            }
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    walk(stmts, outer, &mut seen, &mut out);
    out
}

const HEADER: &str = r#"-- Generated by the CuNi Haskell backend. Do not hand-edit.
cuniSayInt :: Integer -> IO ()
cuniSayInt x = print x
cuniSayStr :: String -> IO ()
cuniSayStr x = putStrLn x
cuniSayBool :: Bool -> IO ()
cuniSayBool x = putStrLn (if x then "True" else "False")

"#;

type Env = HashMap<String, (String, Ty)>;

fn lookup(env: &Env, name: &str) -> Result<(String, Ty), String> {
    env.get(name)
        .cloned()
        .ok_or_else(|| format!("unknown variable `{}`: refusing", name))
}

struct Gen {
    next: usize,
    fn_ret: HashMap<String, Ty>,
}

impl Gen {
    fn fresh(&mut self) -> String {
        let v = format!("v{}", self.next);
        self.next += 1;
        v
    }

    fn sayfn(ty: Ty) -> &'static str {
        match ty {
            Ty::Int => "cuniSayInt",
            Ty::Str => "cuniSayStr",
            Ty::Bool => "cuniSayBool",
        }
    }

    /// Expression → (call preludes, pure expression, type). Calls are IO
    /// actions, so each binds a fresh temp in the prelude, left to right.
    fn expr(&mut self, e: &Expr, env: &Env) -> Result<(Vec<String>, String, Ty), String> {
        match &e.kind {
            ExprKind::Int(n) => Ok((vec![], n.to_string(), Ty::Int)),
            ExprKind::Bool(b) => Ok((vec![], if *b { "True" } else { "False" }.to_string(), Ty::Bool)),
            ExprKind::Str(s) => Ok((vec![], hs_str(s), Ty::Str)),
            ExprKind::Float(_) => Err("float: beyond core subset; refusing".into()),
            ExprKind::Dec(_) => Err("dec: beyond core subset; refusing".into()),
            ExprKind::Time(_) => Err("time: beyond core subset; refusing".into()),
            ExprKind::NoneLit => Err("none: beyond core subset; refusing".into()),
            ExprKind::Ident(n) => lookup(env, n).map(|(v, t)| (vec![], v, t)),
            ExprKind::InterpStr(_) => Err("interpolated string: beyond core subset; refusing".into()),
            ExprKind::List(_) => Err("list: beyond core subset; refusing".into()),
            ExprKind::Map(_) => Err("map: beyond core subset; refusing".into()),
            ExprKind::Index { .. } => Err("indexing: beyond core subset; refusing".into()),
            ExprKind::Field { .. } => Err("field access: beyond core subset; refusing".into()),
            ExprKind::Unwrap { .. } => Err("??: beyond core subset; refusing".into()),
            ExprKind::Call { callee, args } => {
                let name = match &callee.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => return Err("indirect call: beyond core subset; refusing".into()),
                };
                if name == "say" {
                    return Err("say: use as a statement, not an expression; refusing".into());
                }
                let ret = self.fn_ret.get(&name).copied().ok_or_else(|| {
                    format!("call to unknown function `{}`: refusing", name)
                })?;
                let mut pre = Vec::new();
                let mut parts = Vec::new();
                for a in args {
                    if a.is_named() {
                        return Err("named arguments: beyond core subset; refusing".into());
                    }
                    let (p, v, _) = self.expr(a.expr(), env)?;
                    pre.extend(p);
                    parts.push(v);
                }
                let tmp = self.fresh();
                pre.push(format!("{} <- {} {}", tmp, name, parts.join(" ")));
                Ok((pre, tmp, ret))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (mut pre, l, lt) = self.expr(lhs, env)?;
                let (pr, r, rt) = self.expr(rhs, env)?;
                pre.extend(pr);
                let (frag, ty) = match op {
                    BinOp::Add => {
                        if lt == Ty::Str && rt == Ty::Str {
                            (format!("({} ++ {})", l, r), Ty::Str)
                        } else if lt == Ty::Int && rt == Ty::Int {
                            (format!("({} + {})", l, r), Ty::Int)
                        } else {
                            return Err("mixed-type `+`: beyond core subset; refusing".into());
                        }
                    }
                    BinOp::Sub => {
                        Self::ints(lt, rt, "-")?;
                        (format!("({} - {})", l, r), Ty::Int)
                    }
                    BinOp::Mul => {
                        Self::ints(lt, rt, "*")?;
                        (format!("({} * {})", l, r), Ty::Int)
                    }
                    // `quot` truncates toward zero (CuNi).
                    BinOp::Div => {
                        Self::ints(lt, rt, "/")?;
                        (format!("({} `quot` {})", l, r), Ty::Int)
                    }
                    // `mod` is Python-floored (CuNi).
                    BinOp::Mod => {
                        Self::ints(lt, rt, "%")?;
                        (format!("({} `mod` {})", l, r), Ty::Int)
                    }
                    BinOp::Eq => {
                        Self::same(lt, rt, "==")?;
                        (format!("({} == {})", l, r), Ty::Bool)
                    }
                    BinOp::Ne => {
                        Self::same(lt, rt, "!=")?;
                        (format!("({} /= {})", l, r), Ty::Bool)
                    }
                    BinOp::Lt => {
                        Self::ord(lt, rt, "<")?;
                        (format!("({} < {})", l, r), Ty::Bool)
                    }
                    BinOp::Gt => {
                        Self::ord(lt, rt, ">")?;
                        (format!("({} > {})", l, r), Ty::Bool)
                    }
                    BinOp::Le => {
                        Self::ord(lt, rt, "<=")?;
                        (format!("({} <= {})", l, r), Ty::Bool)
                    }
                    BinOp::Ge => {
                        Self::ord(lt, rt, ">=")?;
                        (format!("({} >= {})", l, r), Ty::Bool)
                    }
                    BinOp::And => {
                        Self::bools(lt, rt, "and")?;
                        (format!("({} && {})", l, r), Ty::Bool)
                    }
                    BinOp::Or => {
                        Self::bools(lt, rt, "or")?;
                        (format!("({} || {})", l, r), Ty::Bool)
                    }
                };
                Ok((pre, frag, ty))
            }
            ExprKind::Unary { op, expr } => {
                let (pre, v, t) = self.expr(expr, env)?;
                match op {
                    UnOp::Not => {
                        if t != Ty::Bool {
                            return Err("`not` on non-bool: beyond core subset; refusing".into());
                        }
                        Ok((pre, format!("(not {})", v), Ty::Bool))
                    }
                    UnOp::Neg => {
                        if t != Ty::Int {
                            return Err("unary `-` on non-int: beyond core subset; refusing".into());
                        }
                        Ok((pre, format!("(negate {})", v), Ty::Int))
                    }
                }
            }
        }
    }

    fn boolexpr(&mut self, e: &Expr, env: &Env) -> Result<(Vec<String>, String), String> {
        let (pre, v, t) = self.expr(e, env)?;
        if t != Ty::Bool {
            return Err("condition must be bool; refusing".into());
        }
        Ok((pre, v))
    }

    fn ints(l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if l == Ty::Int && r == Ty::Int {
            Ok(())
        } else {
            Err(format!("`{}` on non-int: beyond core subset; refusing", op))
        }
    }

    fn bools(l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if l == Ty::Bool && r == Ty::Bool {
            Ok(())
        } else {
            Err(format!("`{}` on non-bool: beyond core subset; refusing", op))
        }
    }

    fn same(l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if l == r {
            Ok(())
        } else {
            Err(format!("`{}` across mismatched types: beyond core subset; refusing", op))
        }
    }

    fn ord(l: Ty, r: Ty, op: &str) -> Result<(), String> {
        if (l == Ty::Int && r == Ty::Int) || (l == Ty::Str && r == Ty::Str) {
            Ok(())
        } else {
            Err(format!("`{}` across mismatched types: beyond core subset; refusing", op))
        }
    }

    fn check_ann(ty: Option<Ty>, vt: Ty, name: &str, kw: &str) -> Result<(), String> {
        if let Some(at) = ty {
            if at != vt {
                return Err(format!("{} {}: annotated type disagrees with value; refusing", kw, name));
            }
        }
        Ok(())
    }

    /// do-block lines for `stmts`; threads SSA env.
    fn lines(&mut self, stmts: &[S], env: &mut Env) -> Result<Vec<String>, String> {
        let mut out = Vec::new();
        for s in stmts {
            match s {
                S::Let { name, ty, value } => {
                    let (pre, v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "let")?;
                    let var = self.fresh();
                    out.extend(pre);
                    out.push(format!("let {{ {} = {} }}", var, v));
                    env.insert(name.to_string(), (var, vt));
                }
                S::Mut { name, ty, value } => {
                    let (pre, v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "mut")?;
                    let var = self.fresh();
                    out.extend(pre);
                    out.push(format!("let {{ {} = {} }}", var, v));
                    env.insert(name.to_string(), (var, vt));
                }
                S::Assign { name, value } => {
                    if !env.contains_key(*name) {
                        return Err(format!("assignment to unknown variable `{}`: refusing", name));
                    }
                    let (pre, v, vt) = self.expr(value, env)?;
                    let var = self.fresh();
                    out.extend(pre);
                    out.push(format!("let {{ {} = {} }}", var, v));
                    env.insert(name.to_string(), (var, vt));
                }
                S::Say(x) => {
                    let (pre, v, vt) = self.expr(x, env)?;
                    out.extend(pre);
                    out.push(format!("{} {}", Self::sayfn(vt), v));
                }
                S::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    let outer: HashSet<String> = env.keys().cloned().collect();
                    let mut merged = assigned_outer(then_body, &outer);
                    for n in assigned_outer(else_body, &outer) {
                        if !merged.contains(&n) {
                            merged.push(n);
                        }
                    }
                    let (pre_c, c) = self.boolexpr(cond, env)?;
                    out.extend(pre_c);
                    if merged.is_empty() {
                        let mut et = env.clone();
                        let t = self.lines(then_body, &mut et)?;
                        let mut ee = env.clone();
                        let e = self.lines(else_body, &mut ee)?;
                        out.push(format!(
                            "if {} then do {{ {} }} else do {{ {} }}",
                            c,
                            Self::do_body(&t),
                            Self::do_body(&e)
                        ));
                    } else {
                        let mut et = env.clone();
                        let t = self.lines(then_body, &mut et)?;
                        let tt = Self::tuple_of(&merged, &et)?;
                        let mut ee = env.clone();
                        let e = self.lines(else_body, &mut ee)?;
                        let te = Self::tuple_of(&merged, &ee)?;
                        let mvars: Vec<String> = merged.iter().map(|_| self.fresh()).collect();
                        for (n, mv) in merged.iter().zip(mvars.iter()) {
                            let ty = lookup(env, n)?.1;
                            env.insert(n.clone(), (mv.clone(), ty));
                        }
                        out.push(format!(
                            "({}) <- if {} then do {{ {}; return {} }} else do {{ {}; return {} }}",
                            mvars.join(", "),
                            c,
                            Self::do_body(&t),
                            tt,
                            Self::do_body(&e),
                            te
                        ));
                    }
                }
                S::Whl { cond, body } => {
                    let outer: HashSet<String> = env.keys().cloned().collect();
                    let carried = assigned_outer(body, &outer);
                    let mut env_l = env.clone();
                    let mut wvars = Vec::new();
                    let mut inits = Vec::new();
                    for n in &carried {
                        let (cv, ty) = lookup(&env_l, n)?;
                        let w = self.fresh();
                        env_l.insert(n.clone(), (w.clone(), ty));
                        wvars.push(w);
                        inits.push(cv);
                    }
                    // Condition (with any effectful call preludes) is
                    // evaluated inside the loop, per iteration.
                    let (pre_c, c) = self.boolexpr(cond, &env_l)?;
                    let mut eb = env_l;
                    let b = self.lines(body, &mut eb)?;
                    let news: Result<Vec<String>, String> =
                        carried.iter().map(|n| lookup(&eb, n).map(|(v, _)| v)).collect();
                    let news = news?;
                    let mut loop_body = pre_c;
                    loop_body.extend(b);
                    let ret_tuple = if wvars.is_empty() {
                        "()".to_string()
                    } else {
                        format!("({})", wvars.join(", "))
                    };
                    let recur_args = if news.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", news.join(" "))
                    };
                    let loop_def = format!(
                        "let loop{} = if {} then do {{ {}; loop{} }} else return {} in loop{}",
                        wvars.iter().map(|w| format!(" {}", w)).collect::<String>(),
                        c,
                        loop_body.join("; "),
                        recur_args,
                        ret_tuple,
                        inits.iter().map(|v| format!(" {}", v)).collect::<String>()
                    );
                    if carried.is_empty() {
                        out.push(loop_def);
                    } else {
                        let mvars: Vec<String> = carried.iter().map(|_| self.fresh()).collect();
                        for (n, mv) in carried.iter().zip(mvars.iter()) {
                            let ty = lookup(env, n)?.1;
                            env.insert(n.clone(), (mv.clone(), ty));
                        }
                        out.push(format!("({}) <- {}", mvars.join(", "), loop_def));
                    }
                }
                S::Ret(_) => return Err("ret not in tail position: internal error; refusing".into()),
            }
        }
        Ok(out)
    }

    fn tuple_of(names: &[String], env: &Env) -> Result<String, String> {
        let vs: Result<Vec<String>, String> =
            names.iter().map(|n| lookup(env, n).map(|(v, _)| v)).collect();
        Ok(format!("({})", vs?.join(", ")))
    }

    /// Join do-block lines; empty body becomes `return ()` (empty do illegal).
    fn do_body(lines: &[String]) -> String {
        if lines.is_empty() {
            "return ()".to_string()
        } else {
            lines.join("; ")
        }
    }

    /// Def-body lines: after norm, ends with Ret or diverging If.
    fn def_value(&mut self, stmts: &[S], env: &mut Env) -> Result<Vec<String>, String> {
        let n = stmts.len();
        let init = if n > 0 { &stmts[..n - 1] } else { &[][..] };
        let mut out = self.lines(init, env)?;
        match stmts.last() {
            Some(S::Ret(e)) => {
                let (pre, v, _) = match e {
                    Some(x) => self.expr(x, env)?,
                    None => (vec![], "()".to_string(), Ty::Int),
                };
                out.extend(pre);
                out.push(format!("return {}", v));
                Ok(out)
            }
            Some(S::If {
                cond,
                then_body,
                else_body,
            }) => {
                if !(diverges(then_body) && diverges(else_body)) {
                    return Err("def body does not return on all paths; refusing".into());
                }
                let (pre_c, c) = self.boolexpr(cond, env)?;
                out.extend(pre_c);
                let mut et = env.clone();
                let t = self.def_value(then_body, &mut et)?;
                let mut ee = env.clone();
                let e = self.def_value(else_body, &mut ee)?;
                out.push(format!(
                    "if {} then do {{ {} }} else do {{ {} }}",
                    c,
                    Self::do_body(&t),
                    Self::do_body(&e)
                ));
                Ok(out)
            }
            _ => Err("def body must end with ret; refusing".into()),
        }
    }
}

pub fn generate(program: &Program) -> Result<String, String> {
    let mut g = Gen {
        next: 0,
        fn_ret: HashMap::new(),
    };
    let mut out = String::new();
    out.push_str(HEADER);
    // First pass: collect defs, refuse non-core items.
    let mut script: Vec<&Stmt> = Vec::new();
    for item in &program.items {
        match item {
            Item::Def(f) => {
                if f.fallible {
                    return Err(format!("def {}: fallible functions beyond core subset; refusing", f.name));
                }
                if !f.generics.is_empty() {
                    return Err(format!("def {}: generics beyond core subset; refusing", f.name));
                }
                if f.is_link {
                    return Err(format!("link {}: beyond core subset; refusing", f.name));
                }
                g.fn_ret.insert(f.name.clone(), ty_of(&f.ret_type)?);
                for p in &f.params {
                    ty_of(&p.ty)?;
                }
            }
            Item::Stmt(s) => script.push(s),
            Item::Use(u) => return Err(format!("use {}: beyond core subset; refusing", u.name)),
            Item::Enum(_) => return Err("enum: beyond core subset; refusing".into()),
            Item::Typ(_) => return Err("typ: beyond core subset; refusing".into()),
            Item::Iface(_) => return Err("iface: beyond core subset; refusing".into()),
            Item::Ext(_) => return Err("ext: beyond core subset; refusing".into()),
        }
    }
    for item in &program.items {
        if let Item::Def(f) = item {
            let mut env: Env = HashMap::new();
            let mut params = Vec::new();
            let mut sigs = Vec::new();
            for p in &f.params {
                let v = g.fresh();
                let ty = ty_of(&p.ty)?;
                params.push(v.clone());
                sigs.push(hs_ty(ty).to_string());
                env.insert(p.name.clone(), (v, ty));
            }
            sigs.push(format!("IO {}", hs_ty(ty_of(&f.ret_type)?)));
            out.push_str(&format!("{} :: {}\n", f.name, sigs.join(" -> ")));
            let body = conv(&refs(&f.body), false, true)?;
            let lines = g.def_value(&body, &mut env)?;
            out.push_str(&format!("{} {} = do {{ {} }}\n\n", f.name, params.join(" "), lines.join("; ")));
        }
    }
    out.push_str("main :: IO ()\n");
    let body = conv(&script, false, false)?;
    let mut env: Env = HashMap::new();
    let mut lines = g.lines(&body, &mut env)?;
    lines.push("return ()".to_string());
    out.push_str(&format!("main = do {{ {} }}\n", lines.join("; ")));
    Ok(out)
}

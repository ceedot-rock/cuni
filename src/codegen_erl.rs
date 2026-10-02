//! Erlang backend — handwritten native seat (escript).
//!
//! Emits real Erlang: `main/1` entry plus one function per `def`,
//! `cuni_say/2` helper (bool → `True`/`False`), `div` for truncating int
//! division (CuNi), `cuni_mod/2` (Erlang's `rem` truncates like C; CuNi
//! wants Python-floored `%`). Erlang integers are arbitrary precision,
//! matching CuNi. Single-assignment is handled by honest SSA (fresh `V<n>`
//! per binding); `while` + mutated state becomes a self-recursive `fun`
//! threading the mutated variables as parameters — an exact desugar, not
//! approximation. Early `ret` inside a `while` body cannot be threaded
//! through the loop honestly, so it is refused. The `%%!` header line is
//! required for `escript` to accept the file in this environment. Refuses
//! everything else beyond the core subset (see `crate::codegen_core`
//! docs).

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

fn erl_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\x{:02x}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

/// Normalized owned statement — same shape as the Clojure/Elixir seats.
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

const HEADER: &str = r#"%%! -noshell
%% Generated by the CuNi Erlang backend. Do not hand-edit.

cuni_say(X, bool) -> io:format("~s~n", [case X of true -> "True"; false -> "False" end]);
cuni_say(X, int) -> io:format("~w~n", [X]);
cuni_say(X, str) -> io:format("~s~n", [X]).

cuni_mod(A, B) -> ((A rem B) + B) rem B.

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
        let v = format!("V{}", self.next);
        self.next += 1;
        v
    }

    fn ind(lv: usize) -> String {
        "    ".repeat(lv)
    }

    fn tyatom(ty: Ty) -> &'static str {
        match ty {
            Ty::Int => "int",
            Ty::Str => "str",
            Ty::Bool => "bool",
        }
    }

    fn expr(&self, e: &Expr, env: &Env) -> Result<(String, Ty), String> {
        match &e.kind {
            ExprKind::Int(n) => Ok((n.to_string(), Ty::Int)),
            ExprKind::Bool(b) => Ok((if *b { "true" } else { "false" }.to_string(), Ty::Bool)),
            ExprKind::Str(s) => Ok((erl_str(s), Ty::Str)),
            ExprKind::Float(_) => Err("float: beyond core subset; refusing".into()),
            ExprKind::Dec(_) => Err("dec: beyond core subset; refusing".into()),
            ExprKind::Time(_) => Err("time: beyond core subset; refusing".into()),
            ExprKind::NoneLit => Err("none: beyond core subset; refusing".into()),
            ExprKind::Ident(n) => lookup(env, n),
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
                let mut parts = Vec::new();
                for a in args {
                    if a.is_named() {
                        return Err("named arguments: beyond core subset; refusing".into());
                    }
                    parts.push(self.expr(a.expr(), env)?.0);
                }
                Ok((format!("{}({})", name, parts.join(", ")), ret))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (l, lt) = self.expr(lhs, env)?;
                let (r, rt) = self.expr(rhs, env)?;
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
                    BinOp::Div => {
                        Self::ints(lt, rt, "/")?;
                        (format!("({} div {})", l, r), Ty::Int)
                    }
                    BinOp::Mod => {
                        Self::ints(lt, rt, "%")?;
                        (format!("cuni_mod({}, {})", l, r), Ty::Int)
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
                        Self::ord(lt, rt, "=<")?;
                        (format!("({} =< {})", l, r), Ty::Bool)
                    }
                    BinOp::Ge => {
                        Self::ord(lt, rt, ">=")?;
                        (format!("({} >= {})", l, r), Ty::Bool)
                    }
                    BinOp::And => {
                        Self::bools(lt, rt, "and")?;
                        (format!("({} andalso {})", l, r), Ty::Bool)
                    }
                    BinOp::Or => {
                        Self::bools(lt, rt, "or")?;
                        (format!("({} orelse {})", l, r), Ty::Bool)
                    }
                };
                Ok((frag, ty))
            }
            ExprKind::Unary { op, expr } => {
                let (v, t) = self.expr(expr, env)?;
                match op {
                    UnOp::Not => {
                        if t != Ty::Bool {
                            return Err("`not` on non-bool: beyond core subset; refusing".into());
                        }
                        Ok((format!("(not {})", v), Ty::Bool))
                    }
                    UnOp::Neg => {
                        if t != Ty::Int {
                            return Err("unary `-` on non-int: beyond core subset; refusing".into());
                        }
                        Ok((format!("(-{})", v), Ty::Int))
                    }
                }
            }
        }
    }

    fn boolexpr(&self, e: &Expr, env: &Env) -> Result<String, String> {
        let (v, t) = self.expr(e, env)?;
        if t != Ty::Bool {
            return Err("condition must be bool; refusing".into());
        }
        Ok(v)
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

    /// Clause elements (comma-joined); threads SSA env.
    fn elements(&mut self, stmts: &[S], env: &mut Env, lv: usize) -> Result<Vec<String>, String> {
        let mut els = Vec::new();
        for s in stmts {
            match s {
                S::Let { name, ty, value } => {
                    let (v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "let")?;
                    let var = self.fresh();
                    env.insert(name.to_string(), (var.clone(), vt));
                    els.push(format!("{}{} = {}", Self::ind(lv), var, v));
                }
                S::Mut { name, ty, value } => {
                    let (v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "mut")?;
                    let var = self.fresh();
                    env.insert(name.to_string(), (var.clone(), vt));
                    els.push(format!("{}{} = {}", Self::ind(lv), var, v));
                }
                S::Assign { name, value } => {
                    if !env.contains_key(*name) {
                        return Err(format!("assignment to unknown variable `{}`: refusing", name));
                    }
                    let (v, vt) = self.expr(value, env)?;
                    let var = self.fresh();
                    env.insert(name.to_string(), (var.clone(), vt));
                    els.push(format!("{}{} = {}", Self::ind(lv), var, v));
                }
                S::Say(x) => {
                    let (v, vt) = self.expr(x, env)?;
                    els.push(format!("{}cuni_say({}, {})", Self::ind(lv), v, Self::tyatom(vt)));
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
                    let c = self.boolexpr(cond, env)?;
                    if merged.is_empty() {
                        let mut et = env.clone();
                        let t = self.elements(then_body, &mut et, lv + 2)?;
                        let mut ee = env.clone();
                        let e = self.elements(else_body, &mut ee, lv + 2)?;
                        let mut s = format!("{}case {} of\n", Self::ind(lv), c);
                        s.push_str(&format!("{}true ->\n", Self::ind(lv + 1)));
                        s.push_str(&self.seq_join(&t, lv + 2, "true"));
                        s.push_str(";\n");
                        s.push_str(&format!("{}false ->\n", Self::ind(lv + 1)));
                        s.push_str(&self.seq_join(&e, lv + 2, "true"));
                        s.push_str(&format!("{}end", Self::ind(lv)));
                        els.push(s);
                    } else {
                        let mut et = env.clone();
                        let t = self.elements(then_body, &mut et, lv + 2)?;
                        let tt = self.tuple_of(&merged, &et)?;
                        let mut ee = env.clone();
                        let e = self.elements(else_body, &mut ee, lv + 2)?;
                        let te = self.tuple_of(&merged, &ee)?;
                        let mvars: Vec<String> = merged.iter().map(|_| self.fresh()).collect();
                        for (n, mv) in merged.iter().zip(mvars.iter()) {
                            let ty = lookup(env, n)?.1;
                            env.insert(n.clone(), (mv.clone(), ty));
                        }
                        let mut s = format!(
                            "{}{{{}}} = case {} of\n",
                            Self::ind(lv),
                            mvars.join(", "),
                            c
                        );
                        s.push_str(&format!("{}true ->\n", Self::ind(lv + 1)));
                        s.push_str(&self.seq_join(&t, lv + 2, &tt));
                        s.push_str(";\n");
                        s.push_str(&format!("{}false ->\n", Self::ind(lv + 1)));
                        s.push_str(&self.seq_join(&e, lv + 2, &te));
                        s.push_str(&format!("{}end", Self::ind(lv)));
                        els.push(s);
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
                    let c = self.boolexpr(cond, &env_l)?;
                    let mut eb = env_l;
                    let b = self.elements(body, &mut eb, lv + 3)?;
                    let news: Result<Vec<String>, String> =
                        carried.iter().map(|n| lookup(&eb, n).map(|(v, _)| v)).collect();
                    let news = news?;
                    let (wlist, ilist) = if wvars.is_empty() {
                        ("".to_string(), "".to_string())
                    } else {
                        (wvars.join(", "), inits.join(", "))
                    };
                    let mut s = String::new();
                    if carried.is_empty() {
                        s.push_str(&format!("{}(fun Loop({}) ->\n", Self::ind(lv), wlist));
                    } else {
                        let mvars: Vec<String> = carried.iter().map(|_| self.fresh()).collect();
                        for (n, mv) in carried.iter().zip(mvars.iter()) {
                            let ty = lookup(env, n)?.1;
                            env.insert(n.clone(), (mv.clone(), ty));
                        }
                        s.push_str(&format!(
                            "{}{{{}}} = (fun Loop({}) ->\n",
                            Self::ind(lv),
                            mvars.join(", "),
                            wlist
                        ));
                    }
                    s.push_str(&format!("{}case {} of\n", Self::ind(lv + 1), c));
                    s.push_str(&format!("{}true ->\n", Self::ind(lv + 2)));
                    let tail = if wvars.is_empty() {
                        "Loop()".to_string()
                    } else {
                        format!("Loop({})", news.join(", "))
                    };
                    s.push_str(&self.seq_join(&b, lv + 3, &tail));
                    s.push_str(";\n");
                    s.push_str(&format!("{}false ->\n", Self::ind(lv + 2)));
                    if wvars.is_empty() {
                        s.push_str(&format!("{}ok", Self::ind(lv + 3)));
                    } else {
                        s.push_str(&format!("{}{{{}}}", Self::ind(lv + 3), wvars.join(", ")));
                    }
                    s.push_str(&format!("\n{}end\n", Self::ind(lv + 1)));
                    s.push_str(&format!("{}end)({})", Self::ind(lv), ilist));
                    els.push(s);
                }
                S::Ret(_) => return Err("ret not in tail position: internal error; refusing".into()),
            }
        }
        Ok(els)
    }

    /// Join clause elements with commas; append `tail` as the final element.
    /// Empty `els` yields just the tail. Result ends with a newline.
    fn seq_join(&self, els: &[String], lv: usize, tail: &str) -> String {
        let mut s = String::new();
        for e in els.iter() {
            s.push_str(e);
            s.push_str(",\n");
        }
        s.push_str(&format!("{}{}\n", Self::ind(lv), tail));
        s
    }

    fn tuple_of(&self, names: &[String], env: &Env) -> Result<String, String> {
        let vs: Result<Vec<String>, String> =
            names.iter().map(|n| lookup(env, n).map(|(v, _)| v)).collect();
        Ok(format!("{{{}}}", vs?.join(", ")))
    }

    /// Def-body elements: after norm, ends with Ret or diverging If; the
    /// final element is the return expression.
    fn def_value(&mut self, stmts: &[S], env: &mut Env, lv: usize) -> Result<Vec<String>, String> {
        let n = stmts.len();
        let init = if n > 0 { &stmts[..n - 1] } else { &[][..] };
        let mut els = self.elements(init, env, lv)?;
        match stmts.last() {
            Some(S::Ret(e)) => {
                let tail = match e {
                    Some(x) => self.expr(x, env)?.0,
                    None => "ok".to_string(),
                };
                els.push(format!("{}{}", Self::ind(lv), tail));
                Ok(els)
            }
            Some(S::If {
                cond,
                then_body,
                else_body,
            }) => {
                if !(diverges(then_body) && diverges(else_body)) {
                    return Err("def body does not return on all paths; refusing".into());
                }
                let c = self.boolexpr(cond, env)?;
                let mut et = env.clone();
                let t = self.def_value(then_body, &mut et, lv + 2)?;
                let mut ee = env.clone();
                let e = self.def_value(else_body, &mut ee, lv + 2)?;
                let mut s = format!("{}case {} of\n", Self::ind(lv), c);
                s.push_str(&format!("{}true ->\n", Self::ind(lv + 1)));
                s.push_str(&self.seq_join_last(&t, lv + 2));
                s.push_str(";\n");
                s.push_str(&format!("{}false ->\n", Self::ind(lv + 1)));
                s.push_str(&self.seq_join_last(&e, lv + 2));
                s.push_str(&format!("{}end", Self::ind(lv)));
                els.push(s);
                Ok(els)
            }
            _ => Err("def body must end with ret; refusing".into()),
        }
    }

    /// Join elements where the LAST element is already the tail (no extra).
    /// Result ends with a newline.
    fn seq_join_last(&self, els: &[String], _lv: usize) -> String {
        let mut s = els.join(",\n");
        s.push('\n');
        s
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
    out.push_str("main(_) ->\n");
    let body = conv(&script, false, false)?;
    let mut env: Env = HashMap::new();
    let mut els = g.elements(&body, &mut env, 1)?;
    els.push("    ok".to_string());
    out.push_str(&els.join(",\n"));
    out.push_str(".\n\n");
    for item in &program.items {
        if let Item::Def(f) = item {
            let mut env: Env = HashMap::new();
            let mut params = Vec::new();
            for p in &f.params {
                let v = g.fresh();
                params.push(v.clone());
                env.insert(p.name.clone(), (v, ty_of(&p.ty)?));
            }
            let body = conv(&refs(&f.body), false, true)?;
            let els = g.def_value(&body, &mut env, 1)?;
            out.push_str(&format!("{}({}) ->\n", f.name, params.join(", ")));
            out.push_str(&els.join(",\n"));
            out.push_str(".\n\n");
        }
    }
    Ok(out)
}

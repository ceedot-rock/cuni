//! Prolog backend — handwritten native seat (SWI-Prolog).
//!
//! Emits real Prolog: each `def` becomes a predicate with an extra result
//! argument (`add(A, B, R)`), `main/0` runs the top-level statements,
//! `cuni_say/2` (bool → `True`/`False`), `//` for truncating int division
//! (CuNi), `mod` (already Python-floored in SWI) for `%`. SWI integers are
//! arbitrary precision, matching CuNi. Single-assignment is handled by
//! honest SSA (fresh `V<n>` per binding); `while` + mutated state becomes
//! a recursive `cuni_while_N` predicate threading every in-scope variable
//! as a parameter and unifying the mutated ones on exit — an exact
//! desugar, not approximation. Boolean expressions are reified to
//! `true`/`false` atoms via `(-> ;)` so they compose as terms. Early
//! `ret` inside a `while` body cannot be threaded through the loop
//! honestly, so it is refused. Refuses everything else beyond the core
//! subset (see `crate::codegen_core` docs).

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

fn pro_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\x{:x}\\", c as u32)),
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

const HEADER: &str = r#"%% Generated by the CuNi Prolog backend. Do not hand-edit.
:- set_prolog_flag(double_quotes, string).
cuni_say(X, int) :- write(X), nl.
cuni_say(X, str) :- write(X), nl.
cuni_say(X, bool) :- ( X == true -> write('True') ; write('False') ), nl.

"#;

type Env = HashMap<String, (String, Ty)>;

fn lookup(env: &Env, name: &str) -> Result<(String, Ty), String> {
    env.get(name)
        .cloned()
        .ok_or_else(|| format!("unknown variable `{}`: refusing", name))
}

struct Gen {
    next: usize,
    next_while: usize,
    fn_ret: HashMap<String, Ty>,
    /// Extra top-level predicates (while loops), emitted after defs.
    extras: Vec<String>,
}

impl Gen {
    fn fresh(&mut self) -> String {
        let v = format!("V{}", self.next);
        self.next += 1;
        v
    }

    fn tyatom(ty: Ty) -> &'static str {
        match ty {
            Ty::Int => "int",
            Ty::Str => "str",
            Ty::Bool => "bool",
        }
    }

    /// Expression → (prelude goals, term, type). Int arithmetic stays as
    /// compound terms for `is`/arithmetic comparisons; strings concat via
    /// `string_concat/3` temps; bools reify to true/false atoms.
    fn expr(&mut self, e: &Expr, env: &Env) -> Result<(Vec<String>, String, Ty), String> {
        match &e.kind {
            ExprKind::Int(n) => Ok((vec![], n.to_string(), Ty::Int)),
            ExprKind::Bool(b) => Ok((vec![], if *b { "true" } else { "false" }.to_string(), Ty::Bool)),
            ExprKind::Str(s) => Ok((vec![], pro_str(s), Ty::Str)),
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
                parts.push(tmp.clone());
                pre.push(format!("{}({})", name, parts.join(", ")));
                Ok((pre, tmp, ret))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (mut pre, l, lt) = self.expr(lhs, env)?;
                let (pr, r, rt) = self.expr(rhs, env)?;
                pre.extend(pr);
                match op {
                    BinOp::Add => {
                        if lt == Ty::Str && rt == Ty::Str {
                            let tmp = self.fresh();
                            pre.push(format!("string_concat({}, {}, {})", l, r, tmp));
                            Ok((pre, tmp, Ty::Str))
                        } else if lt == Ty::Int && rt == Ty::Int {
                            Ok((pre, format!("({} + {})", l, r), Ty::Int))
                        } else {
                            Err("mixed-type `+`: beyond core subset; refusing".into())
                        }
                    }
                    BinOp::Sub => {
                        Self::ints(lt, rt, "-")?;
                        Ok((pre, format!("({} - {})", l, r), Ty::Int))
                    }
                    BinOp::Mul => {
                        Self::ints(lt, rt, "*")?;
                        Ok((pre, format!("({} * {})", l, r), Ty::Int))
                    }
                    // `//` truncates toward zero (CuNi).
                    BinOp::Div => {
                        Self::ints(lt, rt, "/")?;
                        Ok((pre, format!("({} // {})", l, r), Ty::Int))
                    }
                    // SWI `mod` is Python-floored (CuNi).
                    BinOp::Mod => {
                        Self::ints(lt, rt, "%")?;
                        Ok((pre, format!("({} mod {})", l, r), Ty::Int))
                    }
                    _ => {
                        // Comparisons and logic reify to true/false.
                        let goal = self.cmp_goal(op, &l, lt, &r, rt)?;
                        let tmp = self.fresh();
                        pre.push(format!("(( {}) -> {} = true ; {} = false )", goal, tmp, tmp));
                        Ok((pre, tmp, Ty::Bool))
                    }
                }
            }
            ExprKind::Unary { op, expr } => {
                let (pre, v, t) = self.expr(expr, env)?;
                match op {
                    UnOp::Not => {
                        if t != Ty::Bool {
                            return Err("`not` on non-bool: beyond core subset; refusing".into());
                        }
                        let tmp = self.fresh();
                        let mut pre = pre;
                        pre.push(format!("(( {} == true ) -> {} = false ; {} = true )", v, tmp, tmp));
                        Ok((pre, tmp, Ty::Bool))
                    }
                    UnOp::Neg => {
                        if t != Ty::Int {
                            return Err("unary `-` on non-int: beyond core subset; refusing".into());
                        }
                        Ok((pre, format!("(- {})", v), Ty::Int))
                    }
                }
            }
        }
    }

    /// Goal text evaluating to true/false for a comparison or logic op,
    /// over reified/int/str terms.
    fn cmp_goal(&self, op: &BinOp, l: &str, lt: Ty, r: &str, rt: Ty) -> Result<String, String> {
        match op {
            BinOp::Eq => Self::eq_goal(lt, rt, l, r, "=:=", "=="),
            BinOp::Ne => Self::eq_goal(lt, rt, l, r, "=\\=", "\\=="),
            BinOp::Lt => Self::ord_goal(lt, rt, l, r, "<", "@<"),
            BinOp::Gt => Self::ord_goal(lt, rt, l, r, ">", "@>"),
            BinOp::Le => Self::ord_goal(lt, rt, l, r, "=<", "@=<"),
            BinOp::Ge => Self::ord_goal(lt, rt, l, r, ">=", "@>="),
            BinOp::And => {
                Self::bools(lt, rt, "and")?;
                Ok(format!("{} == true, {} == true", l, r))
            }
            BinOp::Or => {
                Self::bools(lt, rt, "or")?;
                Ok(format!("{} == true ; {} == true", l, r))
            }
            _ => Err("not a comparison: internal error; refusing".into()),
        }
    }

    fn eq_goal(lt: Ty, rt: Ty, l: &str, r: &str, int_op: &str, str_op: &str) -> Result<String, String> {
        if lt == Ty::Int && rt == Ty::Int {
            Ok(format!("{} {} {}", l, int_op, r))
        } else if lt == Ty::Str && rt == Ty::Str {
            Ok(format!("{} {} {}", l, str_op, r))
        } else if lt == Ty::Bool && rt == Ty::Bool {
            Ok(format!("{} == {}", l, r))
        } else {
            Err("equality across mismatched types: beyond core subset; refusing".into())
        }
    }

    fn ord_goal(lt: Ty, rt: Ty, l: &str, r: &str, int_op: &str, str_op: &str) -> Result<String, String> {
        if lt == Ty::Int && rt == Ty::Int {
            Ok(format!("{} {} {}", l, int_op, r))
        } else if lt == Ty::Str && rt == Ty::Str {
            Ok(format!("{} {} {}", l, str_op, r))
        } else {
            Err("ordering across mismatched types: beyond core subset; refusing".into())
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

    fn check_ann(ty: Option<Ty>, vt: Ty, name: &str, kw: &str) -> Result<(), String> {
        if let Some(at) = ty {
            if at != vt {
                return Err(format!("{} {}: annotated type disagrees with value; refusing", kw, name));
            }
        }
        Ok(())
    }

    /// Goals for `stmts`; threads SSA env. `ret_var` is Some in def bodies
    /// (the result variable); statements append to `out`.
    fn goals(
        &mut self,
        stmts: &[S],
        env: &mut Env,
        out: &mut Vec<String>,
    ) -> Result<(), String> {
        for s in stmts {
            match s {
                S::Let { name, ty, value } => {
                    let (pre, v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "let")?;
                    let var = self.fresh();
                    out.extend(pre);
                    out.push(self.bind_goal(&var, &v, vt));
                    env.insert(name.to_string(), (var, vt));
                }
                S::Mut { name, ty, value } => {
                    let (pre, v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "mut")?;
                    let var = self.fresh();
                    out.extend(pre);
                    out.push(self.bind_goal(&var, &v, vt));
                    env.insert(name.to_string(), (var, vt));
                }
                S::Assign { name, value } => {
                    if !env.contains_key(*name) {
                        return Err(format!("assignment to unknown variable `{}`: refusing", name));
                    }
                    let (pre, v, vt) = self.expr(value, env)?;
                    let var = self.fresh();
                    out.extend(pre);
                    out.push(self.bind_goal(&var, &v, vt));
                    env.insert(name.to_string(), (var, vt));
                }
                S::Say(x) => {
                    let (mut pre, v, vt) = self.expr(x, env)?;
                    out.append(&mut pre);
                    if vt == Ty::Int {
                        // Normalize compound arithmetic before write/1.
                        let tmp = self.fresh();
                        out.push(format!("{} is {}", tmp, v));
                        out.push(format!("cuni_say({}, int)", tmp));
                    } else {
                        out.push(format!("cuni_say({}, {})", v, Self::tyatom(vt)));
                    }
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
                    let (mut pre_c, c) = self.boolexpr(cond, env)?;
                    out.append(&mut pre_c);
                    if merged.is_empty() {
                        let mut et = env.clone();
                        let mut gt = Vec::new();
                        self.goals(then_body, &mut et, &mut gt)?;
                        let mut ee = env.clone();
                        let mut ge = Vec::new();
                        self.goals(else_body, &mut ee, &mut ge)?;
                        out.push(format!(
                            "( {} == true -> {} ; {} )",
                            c,
                            Self::join_goals(&gt),
                            Self::join_goals(&ge)
                        ));
                    } else {
                        let mut et = env.clone();
                        let mut gt = Vec::new();
                        self.goals(then_body, &mut et, &mut gt)?;
                        let mut ee = env.clone();
                        let mut ge = Vec::new();
                        self.goals(else_body, &mut ee, &mut ge)?;
                        let mvars: Vec<String> = merged.iter().map(|_| self.fresh()).collect();
                        let mut gt_s = gt;
                        let mut ge_s = ge;
                        for (n, mv) in merged.iter().zip(mvars.iter()) {
                            let (tv, ty) = lookup(&et, n)?;
                            gt_s.push(format!("{} = {}", mv, tv));
                            let (ev, _) = lookup(&ee, n)?;
                            ge_s.push(format!("{} = {}", mv, ev));
                            env.insert(n.clone(), (mv.clone(), ty));
                        }
                        out.push(format!(
                            "( {} == true -> {} ; {} )",
                            c,
                            Self::join_goals(&gt_s),
                            Self::join_goals(&ge_s)
                        ));
                    }
                }
                S::Whl { cond, body } => {
                    let outer: HashSet<String> = env.keys().cloned().collect();
                    let carried = assigned_outer(body, &outer);
                    // Thread every in-scope variable (sorted for
                    // determinism) as a predicate parameter; carried ones
                    // get extra result parameters unified on exit.
                    let mut names: Vec<String> = env.keys().cloned().collect();
                    names.sort();
                    let wnum = self.next_while;
                    self.next_while += 1;
                    let pname = format!("cuni_while_{}", wnum);
                    let mut env_l = env.clone();
                    let mut pvars: Vec<String> = Vec::new();
                    let mut inits: Vec<String> = Vec::new();
                    for n in &names {
                        let (cv, ty) = lookup(&env_l, n)?;
                        let w = self.fresh();
                        env_l.insert(n.clone(), (w.clone(), ty));
                        pvars.push(w);
                        inits.push(cv);
                    }
                    let (pre_c, c) = self.boolexpr(cond, &env_l)?;
                    let mut gb: Vec<String> = Vec::new();
                    self.goals(body, &mut env_l, &mut gb)?;
                    // Result params, aligned with `names` ("" = not carried).
                    let mut rvars: Vec<String> = Vec::new();
                    for n in &names {
                        if carried.contains(n) {
                            rvars.push(self.fresh());
                        } else {
                            rvars.push(String::new());
                        }
                    }
                    let mut rec_args: Vec<String> = Vec::new();
                    for n in &names {
                        let (v, _) = lookup(&env_l, n)?;
                        rec_args.push(v);
                    }
                    let mut all_params = pvars.clone();
                    for (n, r) in names.iter().zip(rvars.iter()) {
                        if carried.contains(n) {
                            rec_args.push(r.clone());
                            all_params.push(r.clone());
                        }
                    }
                    let mut body_goals = gb;
                    body_goals.push(format!("{}({})", pname, rec_args.join(", ")));
                    let mut exit: Vec<String> = Vec::new();
                    for ((n, r), p) in names.iter().zip(rvars.iter()).zip(pvars.iter()) {
                        if carried.contains(n) {
                            exit.push(format!("{} = {}", r, p));
                        }
                    }
                    // Condition preludes run first (they bind the reified
                    // condition variable), then the loop decision.
                    let mut head_goals = pre_c;
                    head_goals.push(format!(
                        "( {} == true -> {} ; {} )",
                        c,
                        Self::join_goals(&body_goals),
                        Self::join_goals(&exit)
                    ));
                    self.extras.push(format!(
                        "{}({}) :-\n    {}.",
                        pname,
                        all_params.join(", "),
                        Self::join_goals(&head_goals)
                    ));
                    // Call site: current values in, fresh result vars out.
                    let mut call_args = inits;
                    for (n, r) in names.iter().zip(rvars.iter()) {
                        if carried.contains(n) {
                            let mv = self.fresh();
                            let ty = lookup(env, n)?.1;
                            call_args.push(mv.clone());
                            env.insert(n.clone(), (mv, ty));
                            let _ = r;
                        }
                    }
                    out.push(format!("{}({})", pname, call_args.join(", ")));
                }
                S::Ret(_) => return Err("ret not in tail position: internal error; refusing".into()),
            }
        }
        Ok(())
    }

    fn bind_goal(&self, var: &str, term: &str, ty: Ty) -> String {
        match ty {
            Ty::Int => format!("{} is {}", var, term),
            _ => format!("{} = {}", var, term),
        }
    }

    fn join_goals(goals: &[String]) -> String {
        if goals.is_empty() {
            "true".to_string()
        } else {
            goals.join(", ")
        }
    }

    /// Def-body goals: after norm, ends with Ret or diverging If.
    fn def_goals(
        &mut self,
        stmts: &[S],
        env: &mut Env,
        ret_var: &str,
        out: &mut Vec<String>,
    ) -> Result<(), String> {
        let n = stmts.len();
        let init = if n > 0 { &stmts[..n - 1] } else { &[][..] };
        self.goals(init, env, out)?;
        match stmts.last() {
            Some(S::Ret(e)) => {
                if let Some(x) = e {
                    let (mut pre, v, vt) = self.expr(x, env)?;
                    out.append(&mut pre);
                    out.push(self.bind_goal(ret_var, &v, vt));
                }
                Ok(())
            }
            Some(S::If {
                cond,
                then_body,
                else_body,
            }) => {
                if !(diverges(then_body) && diverges(else_body)) {
                    return Err("def body does not return on all paths; refusing".into());
                }
                let (mut pre_c, c) = self.boolexpr(cond, env)?;
                out.append(&mut pre_c);
                let mut et = env.clone();
                let mut gt = Vec::new();
                self.def_goals(then_body, &mut et, ret_var, &mut gt)?;
                let mut ee = env.clone();
                let mut ge = Vec::new();
                self.def_goals(else_body, &mut ee, ret_var, &mut ge)?;
                out.push(format!(
                    "( {} == true -> {} ; {} )",
                    c,
                    Self::join_goals(&gt),
                    Self::join_goals(&ge)
                ));
                Ok(())
            }
            _ => Err("def body must end with ret; refusing".into()),
        }
    }
}

pub fn generate(program: &Program) -> Result<String, String> {
    let mut g = Gen {
        next: 0,
        next_while: 1,
        fn_ret: HashMap::new(),
        extras: Vec::new(),
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
            for p in &f.params {
                let v = g.fresh();
                params.push(v.clone());
                env.insert(p.name.clone(), (v, ty_of(&p.ty)?));
            }
            params.push("R".to_string());
            let body = conv(&refs(&f.body), false, true)?;
            let mut goals = Vec::new();
            g.def_goals(&body, &mut env, "R", &mut goals)?;
            out.push_str(&format!(
                "{}({}) :-\n    {}.\n\n",
                f.name,
                params.join(", "),
                Gen::join_goals(&goals)
            ));
        }
    }
    // Top-level statements become main/0 (may register while predicates).
    let body = conv(&script, false, false)?;
    let mut env: Env = HashMap::new();
    let mut goals = Vec::new();
    g.goals(&body, &mut env, &mut goals)?;
    for e in &g.extras {
        out.push_str(e);
        out.push_str("\n\n");
    }
    out.push_str(&format!("main :-\n    {}.\n", Gen::join_goals(&goals)));
    Ok(out)
}

//! Clojure backend — handwritten native seat.
//!
//! Emits real Clojure: `defn` defs, `cuni-say` helper (bool → `True`/`False`),
//! `quot` for truncating int division (CuNi), Clojure's `mod` (already
//! Python-floored) for `%`, `+'`/`-'`/`*'` so integer arithmetic
//! auto-promotes instead of wrapping. `mut` rebinding is honest SSA (fresh
//! `v<n>` per binding, nested `let`); `while` + mutated state becomes a
//! tail-recursive `loop`/`recur` threading the mutated variables as loop
//! bindings — an exact desugar, not approximation. Early `ret` inside a
//! `while` body cannot be threaded through the loop honestly, so it is
//! refused. Refuses everything else beyond the core subset (see
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

fn clj_str(s: &str) -> String {
    let mut r = String::from('"');
    for c in s.chars() {
        match c {
            '"' => r.push_str("\\\""),
            '\\' => r.push_str("\\\\"),
            '\n' => r.push_str("\\n"),
            '\t' => r.push_str("\\t"),
            '\r' => r.push_str("\\r"),
            c if (c as u32) < 0x20 => r.push_str(&format!("\\u{:04x}", c as u32)),
            c => r.push(c),
        }
    }
    r.push('"');
    r
}

/// Normalized owned statement. `conv` validates the core subset, rejects
/// `ret` inside `while` / outside `def`, and `norm` hoists trailing code
/// after an if-with-early-return into the non-returning branch (exact).
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
                return out; // dead code after unconditional return
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

/// Names assigned (Assign, or Let/Mut rebinding) inside `stmts` that are
/// already in `outer`, in first-appearance order.
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

const HEADER: &str = r#"(defn cuni-say [x ty] (println (if (= ty :bool) (if x "True" "False") x)))
(defn cuni-mod [a b] (mod a b))
"#;

type Env = HashMap<String, (String, Ty)>;

fn lookup(env: &Env, name: &str) -> Result<(String, Ty), String> {
    env.get(name)
        .cloned()
        .ok_or_else(|| format!("unknown variable `{}`: refusing", name))
}

/// A nesting layer around the block's tail value-expression.
enum Layer {
    Let(String, String),
    Do(String),
    If {
        mvars: Vec<String>,
        cond: String,
        then_code: String,
        else_code: String,
    },
    While {
        mvars: Vec<String>,
        loop_code: String,
    },
}

/// How a value-position block computes its tail from the final env.
enum BlockTail<'x> {
    Tuple(Vec<&'x str>),
    Recur(Vec<&'x str>),
    Nil,
}

struct Gen {
    out: String,
    next: usize,
    fn_ret: HashMap<String, Ty>,
}

impl Gen {
    fn fresh(&mut self) -> String {
        let v = format!("v{}", self.next);
        self.next += 1;
        v
    }

    fn ind(lv: usize) -> String {
        "  ".repeat(lv)
    }

    fn tykw(ty: Ty) -> &'static str {
        match ty {
            Ty::Int => ":int",
            Ty::Str => ":str",
            Ty::Bool => ":bool",
        }
    }

    fn expr(&self, e: &Expr, env: &Env) -> Result<(String, Ty), String> {
        match &e.kind {
            ExprKind::Int(n) => Ok((n.to_string(), Ty::Int)),
            ExprKind::Bool(b) => Ok((if *b { "true" } else { "false" }.to_string(), Ty::Bool)),
            ExprKind::Str(s) => Ok((clj_str(s), Ty::Str)),
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
                let mut parts = vec![name];
                for a in args {
                    if a.is_named() {
                        return Err("named arguments: beyond core subset; refusing".into());
                    }
                    parts.push(self.expr(a.expr(), env)?.0);
                }
                Ok((format!("({})", parts.join(" ")), ret))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (l, lt) = self.expr(lhs, env)?;
                let (r, rt) = self.expr(rhs, env)?;
                let (frag, ty) = match op {
                    BinOp::Add => {
                        if lt == Ty::Str && rt == Ty::Str {
                            (format!("(str {} {})", l, r), Ty::Str)
                        } else if lt == Ty::Int && rt == Ty::Int {
                            (format!("(+' {} {})", l, r), Ty::Int)
                        } else {
                            return Err("mixed-type `+`: beyond core subset; refusing".into());
                        }
                    }
                    BinOp::Sub => {
                        Self::ints(lt, rt, "-")?;
                        (format!("(-' {} {})", l, r), Ty::Int)
                    }
                    BinOp::Mul => {
                        Self::ints(lt, rt, "*")?;
                        (format!("(*' {} {})", l, r), Ty::Int)
                    }
                    BinOp::Div => {
                        Self::ints(lt, rt, "/")?;
                        (format!("(quot {} {})", l, r), Ty::Int)
                    }
                    BinOp::Mod => {
                        Self::ints(lt, rt, "%")?;
                        (format!("(cuni-mod {} {})", l, r), Ty::Int)
                    }
                    BinOp::Eq => {
                        Self::same(lt, rt, "==")?;
                        (format!("(= {} {})", l, r), Ty::Bool)
                    }
                    BinOp::Ne => {
                        Self::same(lt, rt, "!=")?;
                        (format!("(not (= {} {}))", l, r), Ty::Bool)
                    }
                    BinOp::Lt => (Self::ord(lt, rt, &l, &r, "<")?, Ty::Bool),
                    BinOp::Gt => (Self::ord(lt, rt, &l, &r, ">")?, Ty::Bool),
                    BinOp::Le => (Self::ord(lt, rt, &l, &r, "<=")?, Ty::Bool),
                    BinOp::Ge => (Self::ord(lt, rt, &l, &r, ">=")?, Ty::Bool),
                    BinOp::And => {
                        Self::bools(lt, rt, "and")?;
                        (format!("(and {} {})", l, r), Ty::Bool)
                    }
                    BinOp::Or => {
                        Self::bools(lt, rt, "or")?;
                        (format!("(or {} {})", l, r), Ty::Bool)
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
                        Ok((format!("(-' {})", v), Ty::Int))
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

    /// Ordering: numeric for ints, `compare`-based for strings.
    fn ord(l: Ty, r: Ty, ls: &str, rs: &str, num: &str) -> Result<String, String> {
        if l == Ty::Int && r == Ty::Int {
            Ok(format!("({} {} {})", num, ls, rs))
        } else if l == Ty::Str && r == Ty::Str {
            Ok(match num {
                "<" => format!("(neg? (compare {} {}))", ls, rs),
                ">" => format!("(pos? (compare {} {}))", ls, rs),
                "<=" => format!("(not (pos? (compare {} {})))", ls, rs),
                ">=" => format!("(not (neg? (compare {} {})))", ls, rs),
                _ => unreachable!(),
            })
        } else {
            Err("ordering across mismatched types: beyond core subset; refusing".into())
        }
    }

    fn check_ann(ty: Option<Ty>, vt: Ty, name: &str, kw: &str) -> Result<(), String> {
        if let Some(at) = ty {
            if at != vt {
                return Err(format!(
                    "{} {}: annotated type disagrees with value; refusing",
                    kw, name
                ));
            }
        }
        Ok(())
    }

    /// Build the nesting layers for `stmts`, threading `env`.
    fn layers(&mut self, stmts: &[S], env: &mut Env, lv: usize) -> Result<Vec<Layer>, String> {
        let mut layers = Vec::new();
        for s in stmts {
            match s {
                S::Let { name, ty, value } => {
                    let (v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "let")?;
                    let var = self.fresh();
                    env.insert(name.to_string(), (var.clone(), vt));
                    layers.push(Layer::Let(var, v));
                }
                S::Mut { name, ty, value } => {
                    let (v, vt) = self.expr(value, env)?;
                    Self::check_ann(*ty, vt, name, "mut")?;
                    let var = self.fresh();
                    env.insert(name.to_string(), (var.clone(), vt));
                    layers.push(Layer::Let(var, v));
                }
                S::Assign { name, value } => {
                    if !env.contains_key(*name) {
                        return Err(format!("assignment to unknown variable `{}`: refusing", name));
                    }
                    let (v, vt) = self.expr(value, env)?;
                    let var = self.fresh();
                    env.insert(name.to_string(), (var.clone(), vt));
                    layers.push(Layer::Let(var, v));
                }
                S::Say(x) => {
                    let (v, vt) = self.expr(x, env)?;
                    layers.push(Layer::Do(format!("(cuni-say {} {})", v, Self::tykw(vt))));
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
                    let refs: Vec<&str> = merged.iter().map(|s| s.as_str()).collect();
                    if merged.is_empty() {
                        let (t, _) = self.block_value(then_body, env.clone(), lv + 1, BlockTail::Nil)?;
                        let (e, _) = self.block_value(else_body, env.clone(), lv + 1, BlockTail::Nil)?;
                        layers.push(Layer::Do(format!("(if {} {} {})", c, t, e)));
                    } else {
                        let (t, _) =
                            self.block_value(then_body, env.clone(), lv + 1, BlockTail::Tuple(refs.clone()))?;
                        let (e, _) =
                            self.block_value(else_body, env.clone(), lv + 1, BlockTail::Tuple(refs))?;
                        let mvars: Vec<String> = merged.iter().map(|_| self.fresh()).collect();
                        for (n, mv) in merged.iter().zip(mvars.iter()) {
                            let ty = lookup(env, n)?.1;
                            env.insert(n.clone(), (mv.clone(), ty));
                        }
                        layers.push(Layer::If {
                            mvars,
                            cond: c,
                            then_code: t,
                            else_code: e,
                        });
                    }
                }
                S::Whl { cond, body } => {
                    let outer: HashSet<String> = env.keys().cloned().collect();
                    let carried = assigned_outer(body, &outer);
                    let mut env_l = env.clone();
                    let mut wvars = Vec::new();
                    for n in &carried {
                        let (cv, ty) = lookup(&env_l, n)?;
                        let w = self.fresh();
                        env_l.insert(n.clone(), (w.clone(), ty));
                        wvars.push(format!("{} {}", w, cv));
                    }
                    let c = self.boolexpr(cond, &env_l)?;
                    let crefs: Vec<&str> = carried.iter().map(|s| s.as_str()).collect();
                    let (body_code, _) =
                        self.block_value(body, env_l, lv + 2, BlockTail::Recur(crefs))?;
                    let wnames: Vec<String> = wvars
                        .iter()
                        .map(|b| b.split(' ').next().unwrap().to_string())
                        .collect();
                    let loop_code = if wnames.is_empty() {
                        format!("(loop []\n{}(if {}\n{}\n{}nil))", Self::ind(lv + 1), c, body_code, Self::ind(lv + 1))
                    } else {
                        format!(
                            "(loop [{}]\n{}(if {}\n{}\n{}[{}]))",
                            wvars.join(" "),
                            Self::ind(lv + 1),
                            c,
                            body_code,
                            Self::ind(lv + 1),
                            wnames.join(" ")
                        )
                    };
                    let mvars: Vec<String> = carried.iter().map(|_| self.fresh()).collect();
                    for (n, mv) in carried.iter().zip(mvars.iter()) {
                        let ty = lookup(env, n)?.1;
                        env.insert(n.clone(), (mv.clone(), ty));
                    }
                    layers.push(Layer::While {
                        mvars,
                        loop_code,
                    });
                }
                S::Ret(_) => return Err("ret not in tail position: internal error; refusing".into()),
            }
        }
        Ok(layers)
    }

    /// Value-position block: layers for `stmts`, then `tail` from final env.
    fn block_value(
        &mut self,
        stmts: &[S],
        env: Env,
        lv: usize,
        tail: BlockTail,
    ) -> Result<(String, Env), String> {
        let mut env = env;
        let layers = self.layers(stmts, &mut env, lv)?;
        let tail_s = match tail {
            BlockTail::Tuple(names) => {
                let vs: Result<Vec<String>, String> =
                    names.iter().map(|n| lookup(&env, n).map(|(v, _)| v)).collect();
                format!("[{}]", vs?.join(" "))
            }
            BlockTail::Recur(names) => {
                let vs: Result<Vec<String>, String> =
                    names.iter().map(|n| lookup(&env, n).map(|(v, _)| v)).collect();
                let vs = vs?;
                if vs.is_empty() {
                    "(recur)".to_string()
                } else {
                    format!("(recur {})", vs.join(" "))
                }
            }
            BlockTail::Nil => "nil".to_string(),
        };
        Ok((Self::apply_layers(&layers, tail_s, lv), env))
    }

    /// Def-body value: after norm, ends with Ret or diverging If.
    fn def_value(&mut self, stmts: &[S], env: Env, lv: usize) -> Result<(String, Env), String> {
        match stmts.last() {
            Some(S::Ret(e)) => {
                let init = &stmts[..stmts.len() - 1];
                let mut env = env;
                let layers = self.layers(init, &mut env, lv)?;
                let tail = match e {
                    Some(x) => self.expr(x, &env)?.0,
                    None => "nil".to_string(),
                };
                Ok((Self::apply_layers(&layers, tail, lv), env))
            }
            Some(S::If {
                cond,
                then_body,
                else_body,
            }) => {
                if !(diverges(then_body) && diverges(else_body)) {
                    return Err("def body does not return on all paths; refusing".into());
                }
                let init = &stmts[..stmts.len() - 1];
                let mut env = env;
                let layers = self.layers(init, &mut env, lv)?;
                let c = self.boolexpr(cond, &env)?;
                let (t, _) = self.def_value(then_body, env.clone(), lv + 1)?;
                let (e, _) = self.def_value(else_body, env.clone(), lv + 1)?;
                let tail = format!("(if {} {} {})", c, t, e);
                Ok((Self::apply_layers(&layers, tail, lv), env))
            }
            _ => Err("def body must end with ret; refusing".into()),
        }
    }

    fn apply_layers(layers: &[Layer], tail: String, lv: usize) -> String {
        let mut code = tail;
        let n = layers.len();
        for (i, layer) in layers.iter().rev().enumerate() {
            let d = lv + n - 1 - i;
            code = match layer {
                Layer::Let(var, e) => format!("{}(let [{} {}]\n{})", Self::ind(d), var, e, code),
                Layer::Do(c) => format!("{}(do {}\n{})", Self::ind(d), c, code),
                Layer::If {
                    mvars,
                    cond,
                    then_code,
                    else_code,
                } => format!(
                    "{}(let [[{}] (if {}\n{}\n{})]\n{})",
                    Self::ind(d),
                    mvars.join(" "),
                    cond,
                    then_code,
                    else_code,
                    code
                ),
                Layer::While { mvars, loop_code } => {
                    if mvars.is_empty() {
                        format!("{}(do\n{}\n{})", Self::ind(d), loop_code, code)
                    } else {
                        format!(
                            "{}(let [[{}]\n{}]\n{})",
                            Self::ind(d),
                            mvars.join(" "),
                            loop_code,
                            code
                        )
                    }
                }
            };
        }
        code
    }
}

pub fn generate(program: &Program) -> Result<String, String> {
    let mut g = Gen {
        out: String::new(),
        next: 0,
        fn_ret: HashMap::new(),
    };
    g.out.push_str(";; Generated by the CuNi Clojure backend. Do not hand-edit.\n");
    g.out.push_str(HEADER);
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
    g.out.push('\n');
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
            let (code, _) = g.def_value(&body, env, 1)?;
            g.out.push_str(&format!("(defn {} [{}]\n{})\n\n", f.name, params.join(" "), code));
        }
    }
    let body = conv(&script, false, false)?;
    let (code, _) = g.block_value(&body, HashMap::new(), 0, BlockTail::Nil)?;
    g.out.push_str(&code);
    g.out.push('\n');
    Ok(g.out)
}

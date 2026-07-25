use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// CuNi's type/effect checker (SPEC.md §19). Bounded, not full inference:
/// catches the highest-value, lowest-false-positive-risk class of errors
/// (see each `check_*` function), and deliberately does NOT attempt full
/// expression type inference, generic type-parameter substitution/unification,
/// or deep `opt<T>`/`none` contextual typing. Where inference is uncertain,
/// this checker stays silent rather than risk rejecting a valid program.
///
/// **Struct construction:** a call whose callee is a declared `typ` name
/// (e.g. `Circle(1.5)`) is a positional constructor — args match field
/// declaration order. No separate `StructLit` AST node is needed; the call
/// shape is already exact on all three targets once codegen rewrites Go to a
/// composite literal and JS to `new`.
///
/// **Fallible calls:** calling a `-> T ?` function (or a `link`'s always-
/// fallible `*_remote` stub) outside a `??` unwrap is a type error — the
/// result has no honest unwrapped type on its own.
///
/// Also absent: source position tracking. `ast.rs` carries no span/location
/// info on any node (only the lexer/parser's own `Tok` does, and that's
/// discarded once parsing succeeds) — so `TypeError` here can only name the
/// offending construct, not point at a line/column the way lex/parse errors
/// do.
pub struct TypeError {
    pub message: String,
}

fn err<T>(message: impl Into<String>) -> Result<T, TypeError> {
    Err(TypeError { message: message.into() })
}

struct FnSig {
    params: Vec<Type>,
    ret: Type,
    fallible: bool,
    generics: Vec<String>,
}

struct TypInfo {
    fields: HashMap<String, Type>,
    /// Field names in declaration order — positional constructors use this.
    field_order: Vec<String>,
    implements: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mutability {
    Let,
    Mut,
}

struct VarInfo {
    ty: Option<Type>,
    mutability: Mutability,
}

struct Checker<'a> {
    program: &'a Program,
    functions: HashMap<String, FnSig>,
    typs: HashMap<String, TypInfo>,
    ifaces: HashMap<String, &'a IfaceDecl>,
    enums: HashMap<String, HashSet<String>>,
}

pub fn check_program(program: &Program) -> Result<(), TypeError> {
    let checker = Checker::build(program)?;
    checker.check_signatures()?;
    checker.check_conformance()?;
    // Top-level statements share ONE scope across the whole program (SPEC.md
    // §7: "top-level is the implicit entry point," the same single
    // `main()`/`func main()` every codegen backend already wraps them into)
    // — a `let`/`mut` on one top-level statement must be visible to a later
    // one, so this threads a single `top_scope` across the item loop instead
    // of giving each `Item::Stmt` its own fresh scope.
    let mut top_scope: HashMap<String, VarInfo> = HashMap::new();
    let empty_generics = HashSet::new();
    for item in &program.items {
        match item {
            Item::Stmt(s) => checker.check_stmt(s, &mut top_scope, &empty_generics, None, false)?,
            other => checker.check_item(other)?,
        }
    }
    Ok(())
}

impl<'a> Checker<'a> {
    fn build(program: &'a Program) -> Result<Self, TypeError> {
        let mut functions = HashMap::new();
        // `say` (SPEC.md §15) is a stdlib builtin every codegen backend
        // emits unconditionally — it's never declared in CuNi source, so it
        // has no `Item::Def`/`Item::Ext` to build a signature from. `.push`/
        // `.len()` don't need an entry here: they're `Field`-based method
        // calls, handled separately in `check_expr`, never a bare
        // `Call{callee: Ident(...)}`. The bogus-looking `"any"`/`"void"`
        // pseudo-types below deliberately never pass through
        // `validate_type` (which only walks user-written signatures via
        // `check_signatures`) — they only need to satisfy the arg-count
        // check in `check_expr`'s `Call` arm, and `say`'s result is never
        // used as a value in any example, so no inference ever reads `ret`.
        functions.insert("say".to_string(), FnSig { params: vec![Type::Named("any".to_string())], ret: Type::Named("void".to_string()), fallible: false, generics: vec![] });
        let mut typs = HashMap::new();
        let mut ifaces = HashMap::new();
        let mut enums = HashMap::new();

        for item in &program.items {
            match item {
                Item::Def(f) => {
                    functions.insert(
                        f.name.clone(),
                        FnSig { params: f.params.iter().map(|p| p.ty.clone()).collect(), ret: f.ret_type.clone(), fallible: f.fallible, generics: f.generics.clone() },
                    );
                    if f.is_link {
                        // `<name>_remote(base_url: str, ...params)` — always
                        // fallible (network). Same shape every codegen emits.
                        let mut remote_params = vec![Type::Named("str".to_string())];
                        remote_params.extend(f.params.iter().map(|p| p.ty.clone()));
                        functions.insert(
                            format!("{}_remote", f.name),
                            FnSig { params: remote_params, ret: f.ret_type.clone(), fallible: true, generics: vec![] },
                        );
                    }
                }
                Item::Ext(e) => {
                    functions.insert(
                        e.name.clone(),
                        FnSig { params: e.params.iter().map(|p| p.ty.clone()).collect(), ret: e.ret_type.clone(), fallible: false, generics: vec![] },
                    );
                }
                Item::Typ(t) => {
                    // Preserve declaration order — positional constructors
                    // (`Circle(1.5)`) match field order, not HashMap iteration.
                    let fields: HashMap<String, Type> = t.fields.iter().map(|p| (p.name.clone(), p.ty.clone())).collect();
                    typs.insert(t.name.clone(), TypInfo { fields, implements: t.implements.clone(), field_order: t.fields.iter().map(|p| p.name.clone()).collect() });
                }
                Item::Iface(i) => {
                    ifaces.insert(i.name.clone(), i);
                }
                Item::Enum(e) => {
                    enums.insert(e.name.clone(), e.variants.iter().cloned().collect());
                }
                Item::Use(_) | Item::Stmt(_) => {}
            }
        }

        Ok(Checker { program, functions, typs, ifaces, enums })
    }

    fn is_known_type_name(&self, name: &str) -> bool {
        matches!(name, "int" | "float" | "str" | "bool") || self.typs.contains_key(name) || self.enums.contains_key(name)
    }

    /// Validates a `Type` appearing in a signature or field declaration:
    /// `Named` must be a builtin scalar, a declared `typ`/`enum`, or a
    /// generic parameter bound in `generics_in_scope`; `Generic` must be
    /// `list`/`map`/`opt` (the only generic type constructors CuNi has) with
    /// the right arity, each argument recursively validated.
    fn validate_type(&self, ty: &Type, generics_in_scope: &HashSet<String>) -> Result<(), TypeError> {
        match ty {
            Type::Named(name) => {
                if self.is_known_type_name(name) || generics_in_scope.contains(name) {
                    Ok(())
                } else {
                    err(format!("unknown type `{}`", name))
                }
            }
            Type::Generic(name, args) => {
                let expected_arity = match name.as_str() {
                    "list" | "opt" => 1,
                    "map" => 2,
                    other => return err(format!("unknown generic type `{}` (only list<T>/map<K,V>/opt<T> exist)", other)),
                };
                if args.len() != expected_arity {
                    return err(format!("`{}` expects {} type argument(s), found {}", name, expected_arity, args.len()));
                }
                for a in args {
                    self.validate_type(a, generics_in_scope)?;
                }
                Ok(())
            }
        }
    }

    fn check_signatures(&self) -> Result<(), TypeError> {
        for item in &self.program.items {
            match item {
                Item::Def(f) => {
                    let generics: HashSet<String> = f.generics.iter().cloned().collect();
                    for p in &f.params {
                        self.validate_type(&p.ty, &generics)?;
                    }
                    self.validate_type(&f.ret_type, &generics)?;
                }
                Item::Ext(e) => {
                    let empty = HashSet::new();
                    for p in &e.params {
                        self.validate_type(&p.ty, &empty)?;
                    }
                    self.validate_type(&e.ret_type, &empty)?;
                }
                Item::Typ(t) => {
                    let empty = HashSet::new();
                    for f in &t.fields {
                        self.validate_type(&f.ty, &empty)?;
                    }
                    if let Some(iface_name) = &t.implements {
                        if !self.ifaces.contains_key(iface_name) {
                            return err(format!("`typ {} is {}` — `{}` is not a declared iface", t.name, iface_name, iface_name));
                        }
                    }
                }
                Item::Iface(i) => {
                    let empty = HashSet::new();
                    for m in &i.methods {
                        for p in &m.params {
                            self.validate_type(&p.ty, &empty)?;
                        }
                        self.validate_type(&m.ret_type, &empty)?;
                    }
                }
                Item::Use(_) | Item::Enum(_) | Item::Stmt(_) => {}
            }
        }
        Ok(())
    }

    /// `typ X is Y` conformance (SPEC.md §10): every codegen backend's
    /// module docs disclose this is currently unenforced — Go's structural
    /// interfaces don't actually check it, Python's ABC mechanism is
    /// "comparatively toothless," JS has no mechanism at all. The one
    /// well-defined rule implied by the spec's own example (`iface Shape do
    /// area() -> float end` / `def area(c: Circle) -> float`) is: each iface
    /// method is realized as a free function taking the receiver as its
    /// first parameter — not a method with an implicit receiver. This
    /// checker enforces exactly that rule, finally giving `is` a real,
    /// checked meaning instead of a purely documentary one.
    fn check_conformance(&self) -> Result<(), TypeError> {
        for (typ_name, info) in &self.typs {
            let Some(iface_name) = &info.implements else { continue };
            let Some(iface) = self.ifaces.get(iface_name) else { continue }; // already reported by check_signatures
            for m in &iface.methods {
                let Some(sig) = self.functions.get(&m.name) else {
                    return err(format!("`typ {} is {}` requires a function `{}` implementing `{}.{}`, but none is declared", typ_name, iface_name, m.name, iface_name, m.name));
                };
                let expected_params: Vec<Type> = std::iter::once(Type::Named(typ_name.clone())).chain(m.params.iter().map(|p| p.ty.clone())).collect();
                if sig.params.len() != expected_params.len() || !sig.params.iter().zip(&expected_params).all(|(a, b)| types_eq(a, b)) {
                    return err(format!(
                        "`typ {} is {}`: `{}` has the wrong signature to implement `{}.{}` — expected first param `{}`, then {}",
                        typ_name,
                        iface_name,
                        m.name,
                        iface_name,
                        m.name,
                        typ_name,
                        m.params.iter().map(|p| format!("{}: {}", p.name, type_str(&p.ty))).collect::<Vec<_>>().join(", ")
                    ));
                }
                if !types_eq(&sig.ret, &m.ret_type) {
                    return err(format!(
                        "`typ {} is {}`: `{}` returns `{}`, but `{}.{}` declares `{}`",
                        typ_name,
                        iface_name,
                        m.name,
                        type_str(&sig.ret),
                        iface_name,
                        m.name,
                        type_str(&m.ret_type)
                    ));
                }
            }
        }
        Ok(())
    }

    fn check_item(&self, item: &Item) -> Result<(), TypeError> {
        match item {
            Item::Def(f) => {
                let mut scope = HashMap::new();
                for p in &f.params {
                    scope.insert(p.name.clone(), VarInfo { ty: Some(p.ty.clone()), mutability: Mutability::Let });
                }
                let generics: HashSet<String> = f.generics.iter().cloned().collect();
                self.check_block(&f.body, &mut scope, &generics, Some((f.fallible, &f.ret_type)))
            }
            // `Item::Stmt` (top-level code) is handled directly by
            // `check_program`, which threads one shared scope across all
            // top-level statements — see its docs.
            _ => Ok(()),
        }
    }

    fn check_block(&self, stmts: &[Stmt], scope: &mut HashMap<String, VarInfo>, generics: &HashSet<String>, fn_ctx: Option<(bool, &Type)>) -> Result<(), TypeError> {
        for s in stmts {
            self.check_stmt(s, scope, generics, fn_ctx, false)?;
        }
        Ok(())
    }

    fn check_stmt(&self, stmt: &Stmt, scope: &mut HashMap<String, VarInfo>, generics: &HashSet<String>, fn_ctx: Option<(bool, &Type)>, allow_fallible: bool) -> Result<(), TypeError> {
        match stmt {
            Stmt::Let { name, ty, value } => {
                if let Some(t) = ty {
                    self.validate_type(t, generics)?;
                }
                self.check_expr(value, scope, generics, allow_fallible)?;
                let inferred = ty.clone().or_else(|| self.infer_expr(value, scope, generics));
                scope.insert(name.clone(), VarInfo { ty: inferred, mutability: Mutability::Let });
            }
            Stmt::Mut { name, ty, value } => {
                if let Some(t) = ty {
                    self.validate_type(t, generics)?;
                }
                self.check_expr(value, scope, generics, allow_fallible)?;
                let inferred = ty.clone().or_else(|| self.infer_expr(value, scope, generics));
                scope.insert(name.clone(), VarInfo { ty: inferred, mutability: Mutability::Mut });
            }
            Stmt::Assign { target, value } => {
                self.check_expr(value, scope, generics, false)?;
                self.check_expr(target, scope, generics, false)?;
                if let Expr::Ident(name) = target {
                    if let Some(info) = scope.get(name) {
                        if info.mutability != Mutability::Mut {
                            return err(format!("cannot assign to `{}` — it's `let`-bound (immutable); declare it `mut` to allow assignment (SPEC.md §6)", name));
                        }
                    }
                }
            }
            Stmt::Ret(Some(e)) => {
                self.check_expr(e, scope, generics, false)?;
                if let Some((_, ret_ty)) = fn_ctx {
                    self.check_ret_type(e, ret_ty, scope, generics)?;
                }
            }
            Stmt::Ret(None) => {}
            Stmt::Fail(e) => {
                self.check_expr(e, scope, generics, false)?;
                match fn_ctx {
                    Some((true, _)) => {}
                    Some((false, _)) => return err("`fail` used inside a non-fallible function — mark its return type `?` to allow `fail` (SPEC.md §12)".to_string()),
                    None => return err("`fail` used at top level — top-level code isn't a fallible function, so it has no failure channel to signal through (SPEC.md §12)".to_string()),
                }
            }
            Stmt::If { cond, then_body, else_body } => {
                self.check_expr(cond, scope, generics, false)?;
                self.check_block(then_body, scope, generics, fn_ctx)?;
                if let Some(eb) = else_body {
                    self.check_block(eb, scope, generics, fn_ctx)?;
                }
            }
            Stmt::For { binding: (a, b), iter, body } => {
                self.check_expr(iter, scope, generics, false)?;
                let iter_ty = self.infer_expr(iter, scope, generics);
                let (a_ty, b_ty) = match (&iter_ty, b) {
                    (Some(Type::Generic(n, args)), Some(_)) if n == "list" => (Some(Type::Named("int".to_string())), Some(args[0].clone())),
                    (Some(Type::Generic(n, args)), Some(_)) if n == "map" => (Some(args[0].clone()), Some(args[1].clone())),
                    (Some(Type::Generic(n, args)), None) if n == "list" => (Some(args[0].clone()), None),
                    (Some(Type::Generic(n, args)), None) if n == "map" => (Some(args[0].clone()), None),
                    _ => (None, None),
                };
                scope.insert(a.clone(), VarInfo { ty: a_ty, mutability: Mutability::Let });
                if let Some(b) = b {
                    scope.insert(b.clone(), VarInfo { ty: b_ty, mutability: Mutability::Let });
                }
                self.check_block(body, scope, generics, fn_ctx)?;
            }
            Stmt::Whl { cond, body } => {
                self.check_expr(cond, scope, generics, false)?;
                self.check_block(body, scope, generics, fn_ctx)?;
            }
            Stmt::ExprStmt(e) => {
                self.check_expr(e, scope, generics, false)?;
            }
            Stmt::Todo => {}
        }
        Ok(())
    }

    /// Best-effort: only compares when the return expression's type can be
    /// confidently inferred (see module docs) and the declared return type
    /// is a plain, non-generic-parameterized shape — anything involving
    /// `opt<T>`/`none` contextual typing or the function's own generics is
    /// skipped rather than risk a false rejection.
    fn check_ret_type(&self, e: &Expr, ret_ty: &Type, scope: &HashMap<String, VarInfo>, generics: &HashSet<String>) -> Result<(), TypeError> {
        if matches!(e, Expr::NoneLit) || should_skip_ret_check(ret_ty, generics) {
            return Ok(());
        }
        if let Some(actual) = self.infer_expr(e, scope, generics) {
            if !types_eq(&actual, ret_ty) {
                return err(format!("`ret` value has type `{}`, but the function declares `-> {}`", type_str(&actual), type_str(ret_ty)));
            }
        }
        Ok(())
    }

    /// Checks that every identifier/call/field/variant reference in `expr`
    /// resolves, and that `.push`/assignment mutability rules hold.
    /// `allow_fallible` is true only for the direct child of a `??` unwrap —
    /// fallible calls elsewhere must be unwrapped first (SPEC.md §12).
    fn check_expr(&self, expr: &Expr, scope: &HashMap<String, VarInfo>, generics: &HashSet<String>, allow_fallible: bool) -> Result<(), TypeError> {
        match expr {
            Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) | Expr::NoneLit => {}
            Expr::InterpStr(parts) => {
                for p in parts {
                    if let StrPartExpr::Expr(e) = p {
                        self.check_expr(e, scope, generics, false)?;
                    }
                }
            }
            Expr::Ident(name) => {
                if !scope.contains_key(name) {
                    return err(format!("undefined variable `{}`", name));
                }
            }
            Expr::List(items) => {
                for i in items {
                    self.check_expr(i, scope, generics, false)?;
                }
            }
            Expr::Map(pairs) => {
                for (k, v) in pairs {
                    self.check_expr(k, scope, generics, false)?;
                    self.check_expr(v, scope, generics, false)?;
                }
            }
            Expr::Call { callee, args } => {
                for a in args {
                    self.check_expr(a, scope, generics, false)?;
                }
                match callee.as_ref() {
                    Expr::Ident(fname) => {
                        if let Some(sig) = self.functions.get(fname) {
                            if sig.params.len() != args.len() {
                                return err(format!("`{}` expects {} argument(s), found {}", fname, sig.params.len(), args.len()));
                            }
                            if sig.fallible && !allow_fallible {
                                return err(format!("`{}` is fallible — unwrap the result with `??` (SPEC.md §12)", fname));
                            }
                        } else if let Some(info) = self.typs.get(fname) {
                            // Positional typ constructor: `Circle(1.5)`.
                            if args.len() != info.field_order.len() {
                                return err(format!(
                                    "`{}` expects {} field argument(s) ({}), found {}",
                                    fname,
                                    info.field_order.len(),
                                    info.field_order.join(", "),
                                    args.len()
                                ));
                            }
                        } else {
                            return err(format!("undefined function `{}`", fname));
                        }
                    }
                    Expr::Field { base, name } => {
                        // `.push(v)`/`.len()` (SPEC.md §15) — `.push` requires
                        // a `mut`-bound list (§11).
                        self.check_expr(base, scope, generics, false)?;
                        if name == "push" {
                            if let Expr::Ident(var_name) = base.as_ref() {
                                if let Some(info) = scope.get(var_name) {
                                    if info.mutability != Mutability::Mut {
                                        return err(format!("cannot `.push` onto `{}` — it's `let`-bound (immutable); declare it `mut` to allow mutation (SPEC.md §11)", var_name));
                                    }
                                }
                            }
                        }
                    }
                    other => self.check_expr(other, scope, generics, false)?,
                }
            }
            Expr::Index { base, index } => {
                self.check_expr(base, scope, generics, false)?;
                self.check_expr(index, scope, generics, false)?;
            }
            Expr::Field { base, name } => {
                if let Expr::Ident(base_name) = base.as_ref() {
                    if let Some(variants) = self.enums.get(base_name) {
                        if !variants.contains(name) {
                            return err(format!("`{}` has no variant `{}`", base_name, name));
                        }
                        return Ok(());
                    }
                }
                self.check_expr(base, scope, generics, false)?;
                if let Some(Type::Named(typ_name)) = self.infer_expr(base, scope, generics) {
                    if let Some(info) = self.typs.get(&typ_name) {
                        if !info.fields.contains_key(name) {
                            return err(format!("`{}` has no field `{}`", typ_name, name));
                        }
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr(lhs, scope, generics, false)?;
                self.check_expr(rhs, scope, generics, false)?;
            }
            Expr::Unary { expr, .. } => {
                self.check_expr(expr, scope, generics, false)?;
            }
            Expr::Unwrap { expr, handler } => {
                // Only the unwrapped expression may be a bare fallible call.
                self.check_expr(expr, scope, generics, true)?;
                let mut handler_scope: HashMap<String, VarInfo> = scope.iter().map(|(k, v)| (k.clone(), VarInfo { ty: v.ty.clone(), mutability: v.mutability })).collect();
                self.check_block(handler, &mut handler_scope, generics, None)?;
            }
        }
        Ok(())
    }

    /// Best-effort expression type inference — returns `None` whenever the
    /// type can't be confidently determined (generic-influenced values,
    /// unrecognized call shapes, etc.) rather than guess. See module docs.
    fn infer_expr(&self, expr: &Expr, scope: &HashMap<String, VarInfo>, generics: &HashSet<String>) -> Option<Type> {
        match expr {
            Expr::Int(_) => Some(Type::Named("int".to_string())),
            Expr::Float(_) => Some(Type::Named("float".to_string())),
            Expr::Bool(_) => Some(Type::Named("bool".to_string())),
            Expr::Str(_) | Expr::InterpStr(_) => Some(Type::Named("str".to_string())),
            Expr::NoneLit => None,
            Expr::Ident(name) => scope.get(name).and_then(|v| v.ty.clone()),
            Expr::List(items) => {
                let elem = items.first().and_then(|e| self.infer_expr(e, scope, generics))?;
                Some(Type::Generic("list".to_string(), vec![elem]))
            }
            Expr::Map(pairs) => {
                let (k, v) = pairs.first()?;
                let kt = self.infer_expr(k, scope, generics)?;
                let vt = self.infer_expr(v, scope, generics)?;
                Some(Type::Generic("map".to_string(), vec![kt, vt]))
            }
            Expr::Call { callee, .. } => match callee.as_ref() {
                Expr::Ident(fname) => {
                    if let Some(sig) = self.functions.get(fname) {
                        if sig.generics.is_empty() {
                            Some(sig.ret.clone())
                        } else {
                            None
                        }
                    } else if self.typs.contains_key(fname) {
                        Some(Type::Named(fname.clone()))
                    } else {
                        None
                    }
                }
                Expr::Field { name, .. } if name == "len" => Some(Type::Named("int".to_string())),
                _ => None,
            },
            Expr::Index { base, .. } => match self.infer_expr(base, scope, generics)? {
                Type::Generic(n, args) if n == "list" => Some(args[0].clone()),
                Type::Generic(n, args) if n == "map" => Some(args[1].clone()),
                _ => None,
            },
            Expr::Field { base, name } => {
                if let Expr::Ident(base_name) = base.as_ref() {
                    if self.enums.contains_key(base_name) {
                        return Some(Type::Named(base_name.clone()));
                    }
                }
                if let Some(Type::Named(typ_name)) = self.infer_expr(base, scope, generics) {
                    return self.typs.get(&typ_name)?.fields.get(name).cloned();
                }
                None
            }
            Expr::Binary { op, lhs, rhs } => match op {
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge | BinOp::And | BinOp::Or => Some(Type::Named("bool".to_string())),
                _ => self.infer_expr(lhs, scope, generics).or_else(|| self.infer_expr(rhs, scope, generics)),
            },
            Expr::Unary { op, expr } => match op {
                UnOp::Not => Some(Type::Named("bool".to_string())),
                UnOp::Neg => self.infer_expr(expr, scope, generics),
            },
            Expr::Unwrap { expr, .. } => match self.infer_expr(expr, scope, generics) {
                Some(Type::Generic(n, args)) if n == "opt" => Some(args[0].clone()),
                _ => None,
            },
        }
    }
}

fn types_eq(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Named(x), Type::Named(y)) => x == y,
        (Type::Generic(nx, ax), Type::Generic(ny, ay)) => nx == ny && ax.len() == ay.len() && ax.iter().zip(ay).all(|(p, q)| types_eq(p, q)),
        _ => false,
    }
}

fn should_skip_ret_check(ty: &Type, generics: &HashSet<String>) -> bool {
    match ty {
        Type::Named(n) => generics.contains(n),
        Type::Generic(n, args) => n == "opt" || args.iter().any(|a| should_skip_ret_check(a, generics)),
    }
}

fn type_str(ty: &Type) -> String {
    match ty {
        Type::Named(n) => n.clone(),
        Type::Generic(n, args) => format!("{}<{}>", n, args.iter().map(type_str).collect::<Vec<_>>().join(", ")),
    }
}

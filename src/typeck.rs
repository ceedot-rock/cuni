use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// CuNi's type/effect checker (SPEC.md §19). Bounded, not full inference.
///
/// Errors carry a source `Span` so the CLI can print `file:line:col`.
pub struct TypeError {
    pub message: String,
    pub span: Span,
}

fn err_at<T>(span: Span, message: impl Into<String>) -> Result<T, TypeError> {
    Err(TypeError {
        message: message.into(),
        span,
    })
}

struct FnSig {
    params: Vec<Type>,
    ret: Type,
    fallible: bool,
    generics: Vec<String>,
    name_span: Span,
}

struct TypInfo {
    fields: HashMap<String, Type>,
    field_order: Vec<String>,
    implements: Option<String>,
    name_span: Span,
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

/// Wave-1 stdlib namespaces (`json`, `time` — docs/STDLIB.md).
/// Returns (arity, param types, return type) for a namespace function,
/// or None if the namespace has no such function.
fn ns_sig(ns: &str, name: &str) -> Option<(Vec<Type>, Type)> {
    let str_t = || Type::Named("str".to_string());
    let int_t = || Type::Named("int".to_string());
    let any_t = || Type::Named("any".to_string());
    let map_of = |v: Type| Type::Generic("map".to_string(), vec![str_t(), v]);
    match (ns, name) {
        ("json", "parse") => Some((vec![str_t()], map_of(any_t()))),
        ("json", "emit") => Some((vec![map_of(any_t())], str_t())),
        ("time", "epoch") => Some((
            vec![
                int_t(),
                int_t(),
                int_t(),
                int_t(),
                int_t(),
                int_t(),
            ],
            int_t(),
        )),
        ("time", "parts") => Some((vec![int_t()], map_of(int_t()))),
        _ => None,
    }
}

/// Wave-1 stdlib string methods (docs/STDLIB.md §3).
/// Returns (arity, return type); the receiver must be a str.
fn str_method_sig(name: &str) -> Option<(usize, Type)> {
    let str_t = || Type::Named("str".to_string());
    match name {
        "split" => Some((
            1,
            Type::Generic("list".to_string(), vec![str_t()]),
        )),
        "join" => Some((1, str_t())),
        "trim" => Some((0, str_t())),
        "contains" => Some((1, Type::Named("bool".to_string()))),
        _ => None,
    }
}

impl<'a> Checker<'a> {
    fn build(program: &'a Program) -> Result<Self, TypeError> {
        let mut functions = HashMap::new();
        functions.insert(
            "say".to_string(),
            FnSig {
                params: vec![Type::Named("any".to_string())],
                ret: Type::Named("void".to_string()),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        let int_t = Type::Named("int".to_string());
        functions.insert(
            "range".to_string(),
            FnSig {
                params: vec![int_t.clone()],
                ret: Type::Generic("list".to_string(), vec![int_t.clone()]),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        functions.insert(
            "abs".to_string(),
            FnSig {
                params: vec![int_t.clone()],
                ret: int_t.clone(),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        functions.insert(
            "min".to_string(),
            FnSig {
                params: vec![int_t.clone(), int_t.clone()],
                ret: int_t.clone(),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        functions.insert(
            "max".to_string(),
            FnSig {
                params: vec![int_t.clone(), int_t.clone()],
                ret: int_t,
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        // `dec` explicit conversions (docs/DECIMAL.md §5). There is no
        // implicit dec<->int conversion anywhere: mixing is a typeck error.
        let dec_t = Type::Named("dec".to_string());
        functions.insert(
            "dec_of_int".to_string(),
            FnSig {
                params: vec![Type::Named("int".to_string())],
                ret: dec_t.clone(),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        functions.insert(
            "int_of_dec".to_string(),
            FnSig {
                params: vec![dec_t.clone()],
                ret: Type::Named("int".to_string()),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        // `time` builtins (docs/TIME.md §5). Like dec, time never converts
        // implicitly: `parse_time` is the only str->time path, and
        // `add_seconds`/`days_between` are the named arithmetic helpers.
        let time_t = Type::Named("time".to_string());
        functions.insert(
            "parse_time".to_string(),
            FnSig {
                params: vec![Type::Named("str".to_string())],
                ret: time_t.clone(),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        functions.insert(
            "add_seconds".to_string(),
            FnSig {
                params: vec![time_t.clone(), Type::Named("int".to_string())],
                ret: time_t.clone(),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        functions.insert(
            "days_between".to_string(),
            FnSig {
                params: vec![time_t.clone(), time_t.clone()],
                ret: Type::Named("int".to_string()),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        // Wave-1 stdlib: SHA-256 hex digest of the UTF-8 bytes (docs/STDLIB.md §4).
        functions.insert(
            "sha256".to_string(),
            FnSig {
                params: vec![Type::Named("str".to_string())],
                ret: Type::Named("str".to_string()),
                fallible: false,
                generics: vec![],
                name_span: Span::dummy(),
            },
        );
        let mut typs = HashMap::new();
        let mut ifaces = HashMap::new();
        let mut enums = HashMap::new();

        for item in &program.items {
            match item {
                Item::Def(f) => {
                    functions.insert(
                        f.name.clone(),
                        FnSig {
                            params: f.params.iter().map(|p| p.ty.clone()).collect(),
                            ret: f.ret_type.clone(),
                            fallible: f.fallible,
                            generics: f.generics.clone(),
                            name_span: f.name_span,
                        },
                    );
                    if f.is_link {
                        let mut remote_params = vec![Type::Named("str".to_string())];
                        remote_params.extend(f.params.iter().map(|p| p.ty.clone()));
                        functions.insert(
                            format!("{}_remote", f.name),
                            FnSig {
                                params: remote_params,
                                ret: f.ret_type.clone(),
                                fallible: true,
                                generics: vec![],
                                name_span: f.name_span,
                            },
                        );
                    }
                }
                Item::Ext(e) => {
                    functions.insert(
                        e.name.clone(),
                        FnSig {
                            params: e.params.iter().map(|p| p.ty.clone()).collect(),
                            ret: e.ret_type.clone(),
                            fallible: false,
                            generics: vec![],
                            name_span: e.name_span,
                        },
                    );
                }
                Item::Typ(t) => {
                    let fields: HashMap<String, Type> = t
                        .fields
                        .iter()
                        .map(|p| (p.name.clone(), p.ty.clone()))
                        .collect();
                    typs.insert(
                        t.name.clone(),
                        TypInfo {
                            fields,
                            implements: t.implements.clone(),
                            field_order: t.fields.iter().map(|p| p.name.clone()).collect(),
                            name_span: t.name_span,
                        },
                    );
                }
                Item::Iface(i) => {
                    if ifaces.contains_key(&i.name) {
                        return err_at(i.name_span, format!("duplicate iface `{}`", i.name));
                    }
                    ifaces.insert(i.name.clone(), i);
                }
                Item::Enum(e) => {
                    if enums.contains_key(&e.name) {
                        return err_at(e.name_span, format!("duplicate enum `{}`", e.name));
                    }
                    let mut variant_set = HashSet::new();
                    for v in &e.variants {
                        if !variant_set.insert(v.name.clone()) {
                            return err_at(
                                v.name_span,
                                format!("duplicate variant `{}` in enum `{}`", v.name, e.name),
                            );
                        }
                    }
                    enums.insert(e.name.clone(), variant_set);
                }
                Item::Use(_) | Item::Stmt(_) => {}
            }
        }

        Ok(Checker {
            program,
            functions,
            typs,
            ifaces,
            enums,
        })
    }

    fn is_known_type_name(&self, name: &str) -> bool {
        matches!(name, "int" | "float" | "str" | "bool" | "dec" | "time")
            || self.typs.contains_key(name)
            || self.enums.contains_key(name)
    }

    fn validate_type(
        &self,
        ty: &Type,
        generics_in_scope: &HashSet<String>,
        span: Span,
    ) -> Result<(), TypeError> {
        match ty {
            Type::Named(name) => {
                if self.is_known_type_name(name) || generics_in_scope.contains(name) {
                    Ok(())
                } else {
                    err_at(
                        span,
                        format!(
                            "unknown type `{}` — fix-it: use `int`/`str`/`bool`/`float`/`list<T>`/`map<K,V>`/`opt<T>` or a declared `typ`/`enum` (check spelling)",
                            name
                        ),
                    )
                }
            }
            Type::Generic(name, args) => {
                let expected_arity = match name.as_str() {
                    "list" | "opt" => 1,
                    "map" => 2,
                    other => {
                        return err_at(
                            span,
                            format!(
                                "unknown generic type `{}` (only list<T>/map<K,V>/opt<T> exist)",
                                other
                            ),
                        )
                    }
                };
                if args.len() != expected_arity {
                    return err_at(
                        span,
                        format!(
                            "`{}` expects {} type argument(s), found {}",
                            name,
                            expected_arity,
                            args.len()
                        ),
                    );
                }
                for a in args {
                    self.validate_type(a, generics_in_scope, span)?;
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
                        self.validate_type(&p.ty, &generics, p.span)?;
                    }
                    self.validate_type(&f.ret_type, &generics, f.name_span)?;
                }
                Item::Ext(e) => {
                    let empty = HashSet::new();
                    for p in &e.params {
                        self.validate_type(&p.ty, &empty, p.span)?;
                    }
                    self.validate_type(&e.ret_type, &empty, e.name_span)?;
                }
                Item::Typ(t) => {
                    let empty = HashSet::new();
                    for f in &t.fields {
                        self.validate_type(&f.ty, &empty, f.span)?;
                    }
                    if let Some(iface_name) = &t.implements {
                        if !self.ifaces.contains_key(iface_name) {
                            return err_at(
                                t.name_span,
                                format!(
                                    "`typ {} is {}` — `{}` is not a declared iface",
                                    t.name, iface_name, iface_name
                                ),
                            );
                        }
                    }
                }
                Item::Iface(i) => {
                    let empty = HashSet::new();
                    for m in &i.methods {
                        for p in &m.params {
                            self.validate_type(&p.ty, &empty, p.span)?;
                        }
                        self.validate_type(&m.ret_type, &empty, m.name_span)?;
                    }
                }
                Item::Use(_) | Item::Enum(_) | Item::Stmt(_) => {}
            }
        }
        Ok(())
    }

    fn check_conformance(&self) -> Result<(), TypeError> {
        for (typ_name, info) in &self.typs {
            let Some(iface_name) = &info.implements else {
                continue;
            };
            let Some(iface) = self.ifaces.get(iface_name) else {
                continue;
            };
            for m in &iface.methods {
                let Some(sig) = self.functions.get(&m.name) else {
                    return err_at(
                        info.name_span,
                        format!(
                            "`typ {} is {}` requires a function `{}` implementing `{}.{}`, but none is declared",
                            typ_name, iface_name, m.name, iface_name, m.name
                        ),
                    );
                };
                let expected_params: Vec<Type> = std::iter::once(Type::Named(typ_name.clone()))
                    .chain(m.params.iter().map(|p| p.ty.clone()))
                    .collect();
                if sig.params.len() != expected_params.len()
                    || !sig
                        .params
                        .iter()
                        .zip(&expected_params)
                        .all(|(a, b)| types_eq(a, b))
                {
                    return err_at(
                        sig.name_span,
                        format!(
                            "`typ {} is {}`: `{}` has the wrong signature to implement `{}.{}` — expected first param `{}`, then {}",
                            typ_name,
                            iface_name,
                            m.name,
                            iface_name,
                            m.name,
                            typ_name,
                            m.params
                                .iter()
                                .map(|p| format!("{}: {}", p.name, type_str(&p.ty)))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    );
                }
                if !types_eq(&sig.ret, &m.ret_type) {
                    return err_at(
                        sig.name_span,
                        format!(
                            "`typ {} is {}`: `{}` returns `{}`, but `{}.{}` declares `{}`",
                            typ_name,
                            iface_name,
                            m.name,
                            type_str(&sig.ret),
                            iface_name,
                            m.name,
                            type_str(&m.ret_type)
                        ),
                    );
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
                    scope.insert(
                        p.name.clone(),
                        VarInfo {
                            ty: Some(p.ty.clone()),
                            mutability: Mutability::Let,
                        },
                    );
                }
                let generics: HashSet<String> = f.generics.iter().cloned().collect();
                self.check_block(
                    &f.body,
                    &mut scope,
                    &generics,
                    Some((f.fallible, &f.ret_type)),
                )
            }
            _ => Ok(()),
        }
    }

    fn check_block(
        &self,
        stmts: &[Stmt],
        scope: &mut HashMap<String, VarInfo>,
        generics: &HashSet<String>,
        fn_ctx: Option<(bool, &Type)>,
    ) -> Result<(), TypeError> {
        for s in stmts {
            self.check_stmt(s, scope, generics, fn_ctx, false)?;
        }
        Ok(())
    }

    fn check_stmt(
        &self,
        stmt: &Stmt,
        scope: &mut HashMap<String, VarInfo>,
        generics: &HashSet<String>,
        fn_ctx: Option<(bool, &Type)>,
        allow_fallible: bool,
    ) -> Result<(), TypeError> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                if name == "json" || name == "time" {
                    return err_at(
                        stmt.span,
                        format!(
                            "`{}` is a reserved stdlib namespace (docs/STDLIB.md) — rename the binding",
                            name
                        ),
                    );
                }
                if let Some(t) = ty {
                    self.validate_type(t, generics, stmt.span)?;
                }
                self.check_expr(value, scope, generics, allow_fallible, fn_ctx)?;
                let inferred = ty
                    .clone()
                    .or_else(|| self.infer_expr(value, scope, generics));
                scope.insert(
                    name.clone(),
                    VarInfo {
                        ty: inferred,
                        mutability: Mutability::Let,
                    },
                );
            }
            StmtKind::Mut { name, ty, value } => {
                if name == "json" || name == "time" {
                    return err_at(
                        stmt.span,
                        format!(
                            "`{}` is a reserved stdlib namespace (docs/STDLIB.md) — rename the binding",
                            name
                        ),
                    );
                }
                if let Some(t) = ty {
                    self.validate_type(t, generics, stmt.span)?;
                }
                self.check_expr(value, scope, generics, allow_fallible, fn_ctx)?;
                let inferred = ty
                    .clone()
                    .or_else(|| self.infer_expr(value, scope, generics));
                scope.insert(
                    name.clone(),
                    VarInfo {
                        ty: inferred,
                        mutability: Mutability::Mut,
                    },
                );
            }
            StmtKind::Assign { target, value } => {
                self.check_expr(value, scope, generics, false, fn_ctx)?;
                self.check_expr(target, scope, generics, false, fn_ctx)?;
                if let ExprKind::Ident(name) = &target.kind {
                    if let Some(info) = scope.get(name) {
                        if info.mutability != Mutability::Mut {
                            return err_at(
                                target.span,
                                format!(
                                    "cannot assign to `{}` — it's `let`-bound (immutable); declare it `mut` to allow assignment (SPEC.md §6)",
                                    name
                                ),
                            );
                        }
                    }
                }
            }
            StmtKind::Ret(Some(e)) => {
                self.check_expr(e, scope, generics, false, fn_ctx)?;
                if let Some((_, ret_ty)) = fn_ctx {
                    self.check_ret_type(e, ret_ty, scope, generics)?;
                }
            }
            StmtKind::Ret(None) => {}
            StmtKind::Fail(e) => {
                self.check_expr(e, scope, generics, false, fn_ctx)?;
                match fn_ctx {
                    Some((true, _)) => {}
                    Some((false, _)) => {
                        return err_at(
                            stmt.span,
                            "`fail` used inside a non-fallible function — mark its return type `?` to allow `fail` (SPEC.md §12)",
                        )
                    }
                    None => {
                        return err_at(
                            stmt.span,
                            "`fail` used at top level — top-level code isn't a fallible function, so it has no failure channel to signal through (SPEC.md §12)",
                        )
                    }
                }
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                self.check_expr(cond, scope, generics, false, fn_ctx)?;
                self.check_block(then_body, scope, generics, fn_ctx)?;
                if let Some(eb) = else_body {
                    self.check_block(eb, scope, generics, fn_ctx)?;
                }
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                self.check_expr(iter, scope, generics, false, fn_ctx)?;
                let iter_ty = self.infer_expr(iter, scope, generics);
                let (a_ty, b_ty) = match (&iter_ty, b) {
                    (Some(Type::Generic(n, args)), Some(_)) if n == "list" => {
                        (Some(Type::Named("int".to_string())), Some(args[0].clone()))
                    }
                    (Some(Type::Generic(n, args)), Some(_)) if n == "map" => {
                        (Some(args[0].clone()), Some(args[1].clone()))
                    }
                    (Some(Type::Generic(n, args)), None) if n == "list" => {
                        (Some(args[0].clone()), None)
                    }
                    (Some(Type::Generic(n, args)), None) if n == "map" => {
                        (Some(args[0].clone()), None)
                    }
                    _ => (None, None),
                };
                scope.insert(
                    a.clone(),
                    VarInfo {
                        ty: a_ty,
                        mutability: Mutability::Let,
                    },
                );
                if let Some(b) = b {
                    scope.insert(
                        b.clone(),
                        VarInfo {
                            ty: b_ty,
                            mutability: Mutability::Let,
                        },
                    );
                }
                self.check_block(body, scope, generics, fn_ctx)?;
            }
            StmtKind::Whl { cond, body } => {
                self.check_expr(cond, scope, generics, false, fn_ctx)?;
                self.check_block(body, scope, generics, fn_ctx)?;
            }
            StmtKind::ExprStmt(e) => {
                self.check_expr(e, scope, generics, false, fn_ctx)?;
            }
            StmtKind::Todo => {}
        }
        Ok(())
    }

    fn check_ret_type(
        &self,
        e: &Expr,
        ret_ty: &Type,
        scope: &HashMap<String, VarInfo>,
        generics: &HashSet<String>,
    ) -> Result<(), TypeError> {
        if matches!(e.kind, ExprKind::NoneLit) || should_skip_ret_check(ret_ty, generics) {
            return Ok(());
        }
        if let Some(actual) = self.infer_expr(e, scope, generics) {
            if !types_eq(&actual, ret_ty) {
                return err_at(
                    e.span,
                    format!(
                        "`ret` value has type `{}`, but the function declares `-> {}` — fix-it: change the `ret` expression or the `-> T` annotation so they match",
                        type_str(&actual),
                        type_str(ret_ty)
                    ),
                );
            }
        }
        Ok(())
    }

    fn check_expr(
        &self,
        expr: &Expr,
        scope: &HashMap<String, VarInfo>,
        generics: &HashSet<String>,
        allow_fallible: bool,
        fn_ctx: Option<(bool, &Type)>,
    ) -> Result<(), TypeError> {
        match &expr.kind {
            ExprKind::Int(_)
            | ExprKind::Float(_)
            | ExprKind::Dec(_)
            | ExprKind::Time(_)
            | ExprKind::Bool(_)
            | ExprKind::Str(_)
            | ExprKind::NoneLit => {}
            ExprKind::InterpStr(parts) => {
                for p in parts {
                    if let StrPartExpr::Expr(e) = p {
                        self.check_expr(e, scope, generics, false, fn_ctx)?;
                    }
                }
            }
            ExprKind::Ident(name) => {
                if !scope.contains_key(name) {
                    return err_at(
                        expr.span,
                        format!(
                            "undefined variable `{}` — fix-it: declare it with `let {} = …` or `mut {} = …` before use (SPEC.md §6)",
                            name, name, name
                        ),
                    );
                }
            }
            ExprKind::List(items) => {
                for i in items {
                    self.check_expr(i, scope, generics, false, fn_ctx)?;
                }
            }
            ExprKind::Map(pairs) => {
                for (k, v) in pairs {
                    self.check_expr(k, scope, generics, false, fn_ctx)?;
                    self.check_expr(v, scope, generics, false, fn_ctx)?;
                }
            }
            ExprKind::Call { callee, args } => {
                for a in args {
                    self.check_expr(a.expr(), scope, generics, false, fn_ctx)?;
                }
                let any_named = args.iter().any(|a| a.is_named());
                let all_named = !args.is_empty() && args.iter().all(|a| a.is_named());
                if any_named && !all_named {
                    return err_at(
                        expr.span,
                        "cannot mix positional and named arguments in one call",
                    );
                }
                match &callee.kind {
                    ExprKind::Ident(fname) => {
                        if let Some(sig) = self.functions.get(fname) {
                            if any_named {
                                return err_at(
                                    expr.span,
                                    format!(
                                        "named arguments are only allowed for typ constructors, not function `{}`",
                                        fname
                                    ),
                                );
                            }
                            if sig.params.len() != args.len() {
                                return err_at(
                                    expr.span,
                                    format!(
                                        "`{}` expects {} argument(s), found {} — fix-it: pass exactly the declared arity (CuNi refuses silent coercion)",
                                        fname,
                                        sig.params.len(),
                                        args.len()
                                    ),
                                );
                            }
                            // Call-site type checks (generic substitution + concrete match)
                            self.check_call_arg_types(
                                fname, sig, args, scope, generics, expr.span,
                            )?;
                            if sig.fallible && !allow_fallible {
                                return err_at(
                                    expr.span,
                                    format!(
                                        "`{}` is fallible — unwrap the result with `??` (SPEC.md §12)",
                                        fname
                                    ),
                                );
                            }
                        } else if let Some(info) = self.typs.get(fname) {
                            self.check_typ_constructor(fname, info, args, expr.span)?;
                        } else {
                            return err_at(
                            expr.span,
                            format!(
                                "undefined function `{}` — fix-it: define `def {}(...) -> T do … end` above the call, or check spelling",
                                fname, fname
                            ),
                        );
                        }
                    }
                    ExprKind::Field { base, name } => {
                        if any_named {
                            return err_at(
                                expr.span,
                                "named arguments are not allowed on method calls",
                            );
                        }
                        // Wave-1 stdlib namespaces: `json`/`time` are
                        // reserved, so the base is never scope-checked;
                        // validate the call against the namespace table.
                        if let ExprKind::Ident(ns) = &base.kind {
                            if ns == "json" || ns == "time" {
                                match ns_sig(ns, name) {
                                    Some((params, _ret)) => {
                                        if args.len() != params.len() {
                                            return err_at(
                                                expr.span,
                                                format!(
                                                    "`{}.{}` expects {} argument(s), found {}",
                                                    ns,
                                                    name,
                                                    params.len(),
                                                    args.len()
                                                ),
                                            );
                                        }
                                        for (param_ty, arg) in params.iter().zip(args.iter()) {
                                            if let Some(actual) =
                                                self.infer_expr(arg.expr(), scope, generics)
                                            {
                                                if !types_compatible(param_ty, &actual) {
                                                    return err_at(
                                                        arg.span(),
                                                        format!(
                                                            "`{}.{}` expects `{}`, found `{}`",
                                                            ns,
                                                            name,
                                                            type_str(param_ty),
                                                            type_str(&actual)
                                                        ),
                                                    );
                                                }
                                            }
                                            // uncertain — stay silent rather
                                            // than false-reject (same rule
                                            // as check_call_arg_types)
                                        }
                                        return Ok(());
                                    }
                                    None => {
                                        return err_at(
                                            expr.span,
                                            format!(
                                                "unknown stdlib function `{}.{}` — see docs/STDLIB.md",
                                                ns, name
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                        self.check_expr(base, scope, generics, false, fn_ctx)?;
                        // Wave-1 string methods: arity is checked; the
                        // receiver must be a str when its type is known.
                        if let Some((arity, _ret)) = str_method_sig(name) {
                            if args.len() != arity {
                                return err_at(
                                    expr.span,
                                    format!(
                                        "`.{}` expects {} argument(s), found {}",
                                        name,
                                        arity,
                                        args.len()
                                    ),
                                );
                            }
                            if let Some(actual) = self.infer_expr(base, scope, generics) {
                                if !types_compatible(&Type::Named("str".to_string()), &actual) {
                                    return err_at(
                                        expr.span,
                                        format!(
                                            "`.{}` needs a str receiver, found `{}`",
                                            name,
                                            type_str(&actual)
                                        ),
                                    );
                                }
                            }
                        }
                        if name == "push" {
                            if let ExprKind::Ident(var_name) = &base.kind {
                                if let Some(info) = scope.get(var_name) {
                                    if info.mutability != Mutability::Mut {
                                        return err_at(
                                            expr.span,
                                            format!(
                                                "cannot `.push` onto `{}` — it's `let`-bound (immutable); declare it `mut` to allow mutation (SPEC.md §11)",
                                                var_name
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => self.check_expr(callee, scope, generics, false, fn_ctx)?,
                }
            }
            ExprKind::Index { base, index } => {
                self.check_expr(base, scope, generics, false, fn_ctx)?;
                self.check_expr(index, scope, generics, false, fn_ctx)?;
            }
            ExprKind::Field { base, name } => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if let Some(variants) = self.enums.get(base_name) {
                        if !variants.contains(name) {
                            return err_at(
                                expr.span,
                                format!("`{}` has no variant `{}`", base_name, name),
                            );
                        }
                        return Ok(());
                    }
                }
                self.check_expr(base, scope, generics, false, fn_ctx)?;
                if let Some(Type::Named(typ_name)) = self.infer_expr(base, scope, generics) {
                    if let Some(info) = self.typs.get(&typ_name) {
                        if !info.fields.contains_key(name) {
                            return err_at(
                                expr.span,
                                format!("`{}` has no field `{}`", typ_name, name),
                            );
                        }
                    }
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                self.check_expr(lhs, scope, generics, false, fn_ctx)?;
                self.check_expr(rhs, scope, generics, false, fn_ctx)?;
                self.check_dec_binary(*op, lhs, rhs, scope, generics, expr.span)?;
                self.check_time_binary(*op, lhs, rhs, scope, generics, expr.span)?;
            }
            ExprKind::Unary { expr: inner, .. } => {
                self.check_expr(inner, scope, generics, false, fn_ctx)?;
            }
            ExprKind::Unwrap {
                expr: inner,
                handler,
            } => {
                self.check_expr(inner, scope, generics, true, fn_ctx)?;
                let mut handler_scope: HashMap<String, VarInfo> = scope
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.clone(),
                            VarInfo {
                                ty: v.ty.clone(),
                                mutability: v.mutability,
                            },
                        )
                    })
                    .collect();
                self.check_block(handler, &mut handler_scope, generics, fn_ctx)?;
            }
        }
        Ok(())
    }

    /// `dec` operand rules (docs/DECIMAL.md §3–5). Arithmetic (`+ - * /`)
    /// and comparisons need `(dec, dec)`; any mix of `dec` with another type
    /// is refused with an explicit-conversion fix-it; `%` is refused on `dec`
    /// entirely. Operand pairs with no `dec` involved keep the checker's
    /// existing leniency — this only ADDS rejections for dec-involved cases,
    /// never new ones for old programs.
    fn check_dec_binary(
        &self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, VarInfo>,
        generics: &HashSet<String>,
        span: Span,
    ) -> Result<(), TypeError> {
        let is_dec =
            |t: &Option<Type>| matches!(t, Some(Type::Named(n)) if n == "dec");
        let lt = self.infer_expr(lhs, scope, generics);
        let rt = self.infer_expr(rhs, scope, generics);
        let (ld, rd) = (is_dec(&lt), is_dec(&rt));
        if !ld && !rd {
            return Ok(());
        }
        let op_s = match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
        };
        if matches!(op, BinOp::Mod) {
            return err_at(
                span,
                "`%` is not defined on `dec` — fix-it: there is no remainder for exact decimals; restructure the computation (docs/DECIMAL.md §3)",
            );
        }
        if ld != rd {
            let other = if ld { rt } else { lt };
            let other_s = other
                .as_ref()
                .map(type_str)
                .unwrap_or_else(|| "?".to_string());
            return err_at(
                span,
                format!(
                    "cannot mix `dec` and `{other_s}` with `{op_s}` — fix-it: convert explicitly: `dec_of_int(n)` turns an int into a dec, `int_of_dec(d)` turns a dec into an int (truncates toward zero) (docs/DECIMAL.md §5)"
                ),
            );
        }
        Ok(())
    }

    /// `time` operand rules (docs/TIME.md §3). `time` is an int64 unix
    /// epoch, UTC — `duration` is plain `int` seconds, and there is no
    /// implicit time<->int conversion. Valid shapes:
    /// `time + int -> time`, `int + time -> time`, `time - int -> time`,
    /// `time - time -> int` (seconds, trunc toward zero); comparisons need
    /// `(time, time)`. Everything else involving a `time` operand is
    /// refused with a fix-it. Operand pairs with no `time` involved keep
    /// the checker's existing leniency — this only ADDS rejections for
    /// time-involved cases, never new ones for old programs.
    fn check_time_binary(
        &self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        scope: &HashMap<String, VarInfo>,
        generics: &HashSet<String>,
        span: Span,
    ) -> Result<(), TypeError> {
        let is_time =
            |t: &Option<Type>| matches!(t, Some(Type::Named(n)) if n == "time");
        let lt = self.infer_expr(lhs, scope, generics);
        let rt = self.infer_expr(rhs, scope, generics);
        let (ltr, rtr) = (is_time(&lt), is_time(&rt));
        if !ltr && !rtr {
            return Ok(());
        }
        let op_s = match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::Le => "<=",
            BinOp::Ge => ">=",
            BinOp::And => "and",
            BinOp::Or => "or",
        };
        let is_cmp = matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge
        );
        // Arithmetic shape check first.
        let shape_ok = match op {
            BinOp::Add => ltr != rtr, // exactly one side time
            BinOp::Sub => ltr,        // time - int, or time - time
            _ if is_cmp => ltr && rtr,
            _ => false,
        };
        if !shape_ok {
            let why: String = match op {
                BinOp::Add => {
                    "only `time + int` / `int + time` are defined — fix-it: a duration is a plain `int` of seconds; `time + time` has no meaning (docs/TIME.md §3)".to_string()
                }
                BinOp::Sub => {
                    "only `time - int -> time` and `time - time -> int` are defined — fix-it: `int - time` has no meaning; use `add_seconds(t, -s)` for a negative shift (docs/TIME.md §3)".to_string()
                }
                _ if is_cmp => {
                    "time comparisons need `(time, time)` — fix-it: there is no implicit time<->int conversion; compare two times (docs/TIME.md §3)".to_string()
                }
                _ => {
                    format!("`{op_s}` is not defined on `time` — fix-it: scale the `int` seconds first, then `time + s` / `time - s` (docs/TIME.md §3)")
                }
            };
            return err_at(
                span,
                format!("cannot use `{op_s}` here with a `time` operand — {why}"),
            );
        }
        // Mixed with a non-int other side (e.g. time + str): refuse.
        let other_is_int = |t: &Option<Type>| matches!(t, Some(Type::Named(n)) if n == "int");
        let bad_mix = match op {
            BinOp::Add => !(other_is_int(&lt) || other_is_int(&rt)),
            BinOp::Sub if !rtr => !other_is_int(&rt),
            _ => false,
        };
        if bad_mix {
            let other_s = if ltr && !rtr {
                rt.as_ref().map(type_str).unwrap_or_else(|| "?".to_string())
            } else if rtr && !ltr {
                lt.as_ref().map(type_str).unwrap_or_else(|| "?".to_string())
            } else {
                "?".to_string()
            };
            return err_at(
                span,
                format!(
                    "cannot mix `time` and `{other_s}` with `{op_s}` — fix-it: durations are plain `int` seconds; `parse_time(s)` turns an ISO-8601 string into a time (docs/TIME.md §5)"
                ),
            );
        }
        Ok(())
    }

    fn check_typ_constructor(
        &self,
        fname: &str,
        info: &TypInfo,
        args: &[CallArg],
        span: Span,
    ) -> Result<(), TypeError> {
        if args.iter().all(|a| a.is_named()) && !args.is_empty() {
            let mut seen = HashSet::new();
            for a in args {
                let CallArg::Named {
                    name, name_span, ..
                } = a
                else {
                    unreachable!()
                };
                if !info.fields.contains_key(name) {
                    return err_at(*name_span, format!("`{}` has no field `{}`", fname, name));
                }
                if !seen.insert(name.clone()) {
                    return err_at(
                        *name_span,
                        format!("duplicate field `{}` in `{}` constructor", name, fname),
                    );
                }
            }
            for f in &info.field_order {
                if !seen.contains(f) {
                    return err_at(
                        span,
                        format!(
                            "`{}` missing field `{}` (expected: {})",
                            fname,
                            f,
                            info.field_order.join(", ")
                        ),
                    );
                }
            }
            if args.len() != info.field_order.len() {
                return err_at(
                    span,
                    format!(
                        "`{}` expects {} field(s) ({}), found {}",
                        fname,
                        info.field_order.len(),
                        info.field_order.join(", "),
                        args.len()
                    ),
                );
            }
            return Ok(());
        }
        // positional
        if args.iter().any(|a| a.is_named()) {
            return err_at(
                span,
                "cannot mix positional and named arguments in one call",
            );
        }
        if args.len() != info.field_order.len() {
            return err_at(
                span,
                format!(
                    "`{}` expects {} field argument(s) ({}), found {}",
                    fname,
                    info.field_order.len(),
                    info.field_order.join(", "),
                    args.len()
                ),
            );
        }
        Ok(())
    }

    /// Match args against parameter types; bind generic params; refuse conflicts.
    fn check_call_arg_types(
        &self,
        fname: &str,
        sig: &FnSig,
        args: &[CallArg],
        scope: &HashMap<String, VarInfo>,
        generics: &HashSet<String>,
        _span: Span,
    ) -> Result<(), TypeError> {
        let gen_set: HashSet<String> = sig.generics.iter().cloned().collect();
        let mut subst: HashMap<String, Type> = HashMap::new();
        for (i, (param_ty, arg)) in sig.params.iter().zip(args.iter()).enumerate() {
            let Some(actual) = self.infer_expr(arg.expr(), scope, generics) else {
                continue; // uncertain — stay silent rather than false-reject
            };
            if matches!(param_ty, Type::Named(n) if n == "any") {
                continue;
            }
            if let Type::Named(pname) = param_ty {
                if gen_set.contains(pname) {
                    if let Some(prev) = subst.get(pname) {
                        if !types_eq(prev, &actual) {
                            return err_at(
                                arg.span(),
                                format!(
                                    "`{}` generic `{}` bound to both `{}` and `{}`",
                                    fname,
                                    pname,
                                    type_str(prev),
                                    type_str(&actual)
                                ),
                            );
                        }
                    } else {
                        subst.insert(pname.clone(), actual);
                    }
                    continue;
                }
            }
            // list<T> where T is generic: bind element type when actual is list<concrete>
            if let (Type::Generic(pn, pargs), Type::Generic(an, aargs)) = (param_ty, &actual) {
                if pn == "list" && an == "list" && pargs.len() == 1 && aargs.len() == 1 {
                    if let Type::Named(pname) = &pargs[0] {
                        if gen_set.contains(pname) {
                            if let Some(prev) = subst.get(pname) {
                                if !types_eq(prev, &aargs[0]) {
                                    return err_at(
                                        arg.span(),
                                        format!(
                                            "`{}` generic `{}` bound to both `{}` and `{}`",
                                            fname,
                                            pname,
                                            type_str(prev),
                                            type_str(&aargs[0])
                                        ),
                                    );
                                }
                            } else {
                                subst.insert(pname.clone(), aargs[0].clone());
                            }
                            continue;
                        }
                    }
                }
            }
            if !types_eq(&actual, param_ty) {
                // Skip when param is itself a free generic already handled
                if matches!(param_ty, Type::Named(n) if gen_set.contains(n)) {
                    continue;
                }
                return err_at(
                    arg.span(),
                    format!(
                        "`{}` argument {} has type `{}`, expected `{}` — fix-it: pass a `{}` value (no approximate / coerced mode)",
                        fname,
                        i + 1,
                        type_str(&actual),
                        type_str(param_ty),
                        type_str(param_ty)
                    ),
                );
            }
        }
        Ok(())
    }

    fn infer_expr(
        &self,
        expr: &Expr,
        scope: &HashMap<String, VarInfo>,
        generics: &HashSet<String>,
    ) -> Option<Type> {
        match &expr.kind {
            ExprKind::Int(_) => Some(Type::Named("int".to_string())),
            ExprKind::Float(_) => Some(Type::Named("float".to_string())),
            ExprKind::Dec(_) => Some(Type::Named("dec".to_string())),
            ExprKind::Time(_) => Some(Type::Named("time".to_string())),
            ExprKind::Bool(_) => Some(Type::Named("bool".to_string())),
            ExprKind::Str(_) | ExprKind::InterpStr(_) => Some(Type::Named("str".to_string())),
            ExprKind::NoneLit => None,
            ExprKind::Ident(name) => scope.get(name).and_then(|v| v.ty.clone()),
            ExprKind::List(items) => {
                let elem = items
                    .first()
                    .and_then(|e| self.infer_expr(e, scope, generics))?;
                Some(Type::Generic("list".to_string(), vec![elem]))
            }
            ExprKind::Map(pairs) => {
                let (k, v) = pairs.first()?;
                let kt = self.infer_expr(k, scope, generics)?;
                let vt = self.infer_expr(v, scope, generics)?;
                Some(Type::Generic("map".to_string(), vec![kt, vt]))
            }
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(fname) => {
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
                ExprKind::Field { name, .. } if name == "len" => {
                    Some(Type::Named("int".to_string()))
                }
                // Wave-1 stdlib namespaces: `json.parse` etc.
                // String methods: `split`/`join`/`trim`/`contains`.
                // (`slice` keeps its existing receiver-sensitive arm below
                // in spirit — merged here since match arms don't fall through.)
                ExprKind::Field { base, name } => {
                    if let ExprKind::Ident(ns) = &base.kind {
                        if ns == "json" || ns == "time" {
                            return ns_sig(ns, name).map(|(_, ret)| ret);
                        }
                    }
                    if let Some((_, ret)) = str_method_sig(name) {
                        return Some(ret);
                    }
                    if name == "slice" {
                        return match self.infer_expr(base, scope, generics) {
                            Some(Type::Named(n)) if n == "str" => {
                                Some(Type::Named("str".to_string()))
                            }
                            Some(Type::Generic(n, args)) if n == "list" => {
                                Some(Type::Generic(n, args))
                            }
                            _ => None,
                        };
                    }
                    None
                }
                _ => None,
            },
            ExprKind::Index { base, .. } => match self.infer_expr(base, scope, generics)? {
                Type::Generic(n, args) if n == "list" => Some(args[0].clone()),
                Type::Generic(n, args) if n == "map" => Some(args[1].clone()),
                _ => None,
            },
            ExprKind::Field { base, name } => {
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.enums.contains_key(base_name) {
                        return Some(Type::Named(base_name.clone()));
                    }
                }
                if let Some(Type::Named(typ_name)) = self.infer_expr(base, scope, generics) {
                    return self.typs.get(&typ_name)?.fields.get(name).cloned();
                }
                None
            }
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => Some(Type::Named("bool".to_string())),
                // `time - time -> int` (seconds); every other time shape
                // keeps the time side's type.
                BinOp::Sub
                    if matches!(self.infer_expr(lhs, scope, generics), Some(Type::Named(n)) if n == "time")
                        && matches!(self.infer_expr(rhs, scope, generics), Some(Type::Named(n)) if n == "time") =>
                {
                    Some(Type::Named("int".to_string()))
                }
                _ => self
                    .infer_expr(lhs, scope, generics)
                    .or_else(|| self.infer_expr(rhs, scope, generics)),
            },
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => Some(Type::Named("bool".to_string())),
                UnOp::Neg => self.infer_expr(expr, scope, generics),
            },
            ExprKind::Unwrap { expr, .. } => match self.infer_expr(expr, scope, generics) {
                Some(Type::Generic(n, args)) if n == "opt" => Some(args[0].clone()),
                _ => None,
            },
        }
    }
}

fn types_eq(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Named(x), Type::Named(y)) => x == y,
        (Type::Generic(nx, ax), Type::Generic(ny, ay)) => {
            nx == ny && ax.len() == ay.len() && ax.iter().zip(ay).all(|(p, q)| types_eq(p, q))
        }
        _ => false,
    }
}

/// Like [`types_eq`], but `any` matches anything on either side. Used for
/// wave-1 stdlib namespace/method call checks (e.g. `json.emit` takes
/// `map<str, any>` and accepts `map<str, int>`).
fn types_compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (Type::Named(x), _) if x == "any" => true,
        (_, Type::Named(y)) if y == "any" => true,
        (Type::Named(x), Type::Named(y)) => x == y,
        (Type::Generic(nx, ax), Type::Generic(ny, ay)) => {
            nx == ny
                && ax.len() == ay.len()
                && ax.iter().zip(ay).all(|(p, q)| types_compatible(p, q))
        }
        _ => false,
    }
}

fn should_skip_ret_check(ty: &Type, generics: &HashSet<String>) -> bool {
    match ty {
        Type::Named(n) => generics.contains(n),
        Type::Generic(n, args) => {
            n == "opt" || args.iter().any(|a| should_skip_ret_check(a, generics))
        }
    }
}

fn type_str(ty: &Type) -> String {
    match ty {
        Type::Named(n) => n.clone(),
        Type::Generic(n, args) => {
            format!(
                "{}<{}>",
                n,
                args.iter().map(type_str).collect::<Vec<_>>().join(", ")
            )
        }
    }
}

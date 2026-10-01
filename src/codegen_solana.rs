//! Solana program backend — the on-chain Rust writer.
//!
//! "Trust Provable, in all things."
//!
//! Emits a genuine Anchor-shaped Solana program from CuNi source: a pure
//! logic core (the part CuNi proves) plus the program shell (entrypoint,
//! accounts struct, program id) that wraps it. This is real Rust, not a
//! Python lowering.
//!
//! Two artifacts, one law:
//! - `logic_core` — the standalone logic core: `pub fn` definitions plus a
//!   `fn main` driver. It compiles with plain `rustc` (no dependencies) and
//!   its stdout is what the CuNi gate proves byte-identical across seats.
//! - `generate_program` — the full Anchor program: the logic core embedded
//!   verbatim inside `mod logic` (clearly delimited), plus the program shell
//!   (also clearly delimited) that requires `anchor-lang`.
//!
//! Exactness notes:
//! - CuNi `int` is i64 with truncated `/` and `%` (Rust semantics). The logic
//!   core uses i64, so integer arithmetic matches exactly with no helpers.
//!   (On-chain convention would be u64; a u64 port is a separate proof —
//!   CuNi proves the i64 semantics it was given, or refuses.)
//! - `say` becomes `println!` in the logic core (stdout is the gate's
//!   comparison surface) and `msg!` at the program boundary.
//! - `float`, `list`, `map`, `opt`, `??`, structs, and enums are honestly
//!   refused: a Solana program's verifiable core is integer math, and the
//!   backend will not guess at mappings it cannot prove.
//! - `fail` becomes `panic!`, matching CuNi's abort semantics in the
//!   off-chain logic core. A production shell would map this to `Err`.
//!
//! Honest boundaries: the logic core is gate-proven; the program shell is
//! NOT compiled by `cuni check` (no Solana toolchain on the check machine)
//! and nothing here has executed on-chain. See `docs/SOLANA.md` for the
//! full verification recipe.

use crate::ast::{
    BinOp, CallArg, Expr, ExprKind, FnDecl, Item, Program, Stmt, StmtKind, StrPartExpr, Type, UnOp,
};
use std::collections::{HashMap, HashSet};

/// Solana-logic kind of a CuNi value, for `say` routing and type inference.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SolKind {
    Int,
    Str,
    Bool,
    Other,
}

/// Delimiters marking the two regions of a `--emit-solana` artifact.
pub const LOGIC_START: &str = "// CUNI-LOGIC-CORE-START";
pub const LOGIC_END: &str = "// CUNI-LOGIC-CORE-END";
pub const SHELL_START: &str = "// CUNI-SOLANA-SHELL-START";
pub const SHELL_END: &str = "// CUNI-SOLANA-SHELL-END";

pub struct Codegen {
    fn_names: HashSet<String>,
    fn_ret: HashMap<String, SolKind>,
    /// Set while generating the program shell (instruction docs need it).
    out: String,
}

impl Codegen {
    fn new(program: &Program) -> Self {
        let mut fn_names = HashSet::new();
        let mut fn_ret = HashMap::new();
        for item in &program.items {
            if let Item::Def(f) = item {
                fn_names.insert(f.name.clone());
                fn_ret.insert(f.name.clone(), kind_of_type(&f.ret_type));
            }
        }
        Codegen {
            fn_names,
            fn_ret,
            out: String::new(),
        }
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn esc(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                c => r.push(c),
            }
        }
        r
    }

    /// Escape for `format!` literal text (`{`/`}` would start a placeholder).
    fn esc_fmt(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        for c in s.chars() {
            match c {
                '{' => r.push_str("{{"),
                '}' => r.push_str("}}"),
                '"' => r.push_str("\\\""),
                '\\' => r.push_str("\\\\"),
                '\n' => r.push_str("\\n"),
                '\t' => r.push_str("\\t"),
                c => r.push(c),
            }
        }
        r
    }
}

/// Map a CuNi type to its Solana-logic Rust type.
fn solana_type(ty: &Type) -> Result<String, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok("i64".into()),
            "str" => Ok("String".into()),
            "bool" => Ok("bool".into()),
            "float" => Err("Solana logic core has no float type; refusing float".into()),
            other => Err(format!(
                "type `{}` has no Solana-logic mapping; refusing",
                other
            )),
        },
        Type::Generic(name, _) => Err(format!(
            "generic type `{}` has no Solana-logic mapping; refusing",
            name
        )),
    }
}

fn kind_of_type(ty: &Type) -> SolKind {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => SolKind::Int,
            "str" => SolKind::Str,
            "bool" => SolKind::Bool,
            _ => SolKind::Other,
        },
        _ => SolKind::Other,
    }
}

/// The standalone logic core: pure functions + driver, no dependencies.
///
/// This is the part CuNi proves. `generate_program` embeds its output
/// verbatim (indented) inside `mod logic`.
pub fn logic_core(program: &Program) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_logic_core(program)?;
    Ok(g.out)
}

/// The full Anchor-shaped Solana program: delimited logic core + shell.
///
/// `prog_mod` is the `#[program]` module name (already sanitized, e.g.
/// `cuni_escrow`).
pub fn generate_program(program: &Program, prog_mod: &str) -> Result<String, String> {
    let mut g = Codegen::new(program);
    g.gen_program(program, prog_mod)?;
    Ok(g.out)
}

impl Codegen {
    /// Shared body: function definitions + top-level driver as `fn main`.
    fn gen_logic_core(&mut self, program: &Program) -> Result<(), String> {
        for item in &program.items {
            match item {
                Item::Typ(t) => {
                    return Err(format!(
                        "typ `{}` has no v1 Solana-logic form; refusing (integer/fn core only)",
                        t.name
                    ))
                }
                Item::Enum(e) => {
                    return Err(format!(
                        "enum `{}` has no v1 Solana-logic form; refusing (integer/fn core only)",
                        e.name
                    ))
                }
                _ => {}
            }
        }
        for item in &program.items {
            if let Item::Def(f) = item {
                self.gen_def(f)?;
                self.out.push('\n');
            }
        }
        let top: Vec<&Stmt> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Stmt(s) => Some(s),
                _ => None,
            })
            .collect();
        if !top.is_empty() {
            self.line(0, "fn main() {");
            let mut scope = HashMap::new();
            for s in top {
                self.gen_stmt(1, s, &mut scope)?;
            }
            self.line(0, "}");
        }
        Ok(())
    }

    fn gen_program(&mut self, program: &Program, prog_mod: &str) -> Result<(), String> {
        self.line(0, "//! Generated by the CuNi Solana backend.");
        self.line(0, "//! \"Trust Provable, in all things.\"");
        self.line(0, "//!");
        self.line(
            0,
            "//! STRUCTURE — two delimited regions, one law (same logic or refuse):",
        );
        self.line(
            0,
            "//! - `mod logic` (CUNI-LOGIC-CORE markers): the pure program logic,",
        );
        self.line(
            0,
            "//!   byte-identical to the standalone logic-core emit. The CuNi gate",
        );
        self.line(
            0,
            "//!   proves its stdout byte-identical across real toolchains; it",
        );
        self.line(0, "//!   compiles as plain Rust with no dependencies.");
        self.line(
            0,
            "//! - Program shell (CUNI-SOLANA-SHELL markers): the Anchor wrapper —",
        );
        self.line(
            0,
            "//!   entrypoint, accounts struct, program id. Requires `anchor-lang`",
        );
        self.line(
            0,
            "//!   and the Solana toolchain; NOT compiled by `cuni check` (no",
        );
        self.line(
            0,
            "//!   Solana toolchain on the check machine). Full verification recipe:",
        );
        self.line(0, "//!   docs/SOLANA.md.");
        self.line(0, "//!");
        self.line(
            0,
            "//! HONEST BOUNDARIES: logic gate-proven; shell not compiled here;",
        );
        self.line(0, "//! nothing here has executed on-chain.");
        self.out.push('\n');

        // Region 1: the logic core, embedded verbatim (indented one level).
        self.line(0, LOGIC_START);
        let core = logic_core(program)?;
        self.line(0, "mod logic {");
        for l in core.lines() {
            if l.trim().is_empty() {
                self.out.push('\n');
            } else {
                self.line(1, l);
            }
        }
        self.line(0, "}");
        self.line(0, LOGIC_END);
        self.out.push('\n');

        // Region 2: the Anchor program shell.
        self.line(0, SHELL_START);
        self.line(
            0,
            "// Requires: anchor-lang = \"0.31\", solana-program.",
        );
        self.line(
            0,
            "// This shell is NOT compiled by `cuni check` — see docs/SOLANA.md.",
        );
        self.line(0, "use anchor_lang::prelude::*;");
        self.out.push('\n');
        self.line(
            0,
            "// TODO: replace with your program's keypair pubkey before deploying.",
        );
        self.line(0, "declare_id!(\"11111111111111111111111111111111\");");
        self.out.push('\n');
        self.line(0, "#[program]");
        self.line(0, &format!("pub mod {} {{", prog_mod));
        self.line(1, "use super::*;");
        self.out.push('\n');
        let defs: Vec<&FnDecl> = program
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Def(f) => Some(f),
                _ => None,
            })
            .collect();
        if defs.is_empty() {
            self.line(1, "// (no CuNi functions: nothing to expose as instructions)");
        } else {
            for (n, f) in defs.iter().enumerate() {
                if n > 0 {
                    self.out.push('\n');
                }
                self.gen_instruction(f)?;
            }
        }
        self.out.push('\n');
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "#[derive(Accounts)]");
        self.line(0, "pub struct Run<'info> {");
        self.line(1, "/// The signer invoking the instruction.");
        self.line(1, "pub signer: Signer<'info>,");
        self.line(1, "pub system_program: Program<'info, System>,");
        self.line(0, "}");
        self.line(0, SHELL_END);
        Ok(())
    }

    /// One Anchor instruction per CuNi function: call the logic core,
    /// log the result with `msg!`, return Ok.
    ///
    /// Uniform by design: the shell does not guess which functions are
    /// "validators" — it computes and reports. Mapping result codes to
    /// `#[error_code]` is a deployment-time decision (see docs/SOLANA.md).
    fn gen_instruction(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        let mut arg_names = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, solana_type(&p.ty)?));
            arg_names.push(p.name.clone());
        }
        let ret = solana_type(&f.ret_type)?;
        self.line(
            1,
            &format!(
                "/// {}: pure program logic lives in `logic::{}`; this",
                f.name, f.name
            ),
        );
        self.line(1, "/// instruction computes it on-chain and logs the result.");
        self.line(
            1,
            &format!(
                "pub fn {}(ctx: Context<Run>{}) -> Result<()> {{",
                f.name,
                if params.is_empty() {
                    String::new()
                } else {
                    format!(", {}", params.join(", "))
                }
            ),
        );
        self.line(2, "let _ctx = ctx;");
        self.line(
            2,
            &format!(
                "let result: {} = logic::{}({});",
                ret,
                f.name,
                arg_names.join(", ")
            ),
        );
        self.line(2, &format!("msg!(\"{} -> {{}}\", result);", f.name));
        self.line(2, "Ok(())");
        self.line(1, "}");
        Ok(())
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        let mut params = Vec::new();
        for p in &f.params {
            params.push(format!("{}: {}", p.name, solana_type(&p.ty)?));
        }
        let ret = solana_type(&f.ret_type)?;
        self.line(
            0,
            &format!("pub fn {}({}) -> {} {{", f.name, params.join(", "), ret),
        );
        let mut scope: HashMap<String, SolKind> = HashMap::new();
        for p in &f.params {
            scope.insert(p.name.clone(), kind_of_type(&p.ty));
        }
        for s in &f.body {
            self.gen_stmt(1, s, &mut scope)?;
        }
        self.line(0, "}");
        Ok(())
    }

    fn gen_stmt(
        &mut self,
        indent: usize,
        stmt: &Stmt,
        scope: &mut HashMap<String, SolKind>,
    ) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                self.gen_binding(indent, name, ty, value, false, scope)
            }
            StmtKind::Mut { name, ty, value } => {
                self.gen_binding(indent, name, ty, value, true, scope)
            }
            StmtKind::Assign { target, value } => {
                let t = self.gen_expr(target, scope)?;
                let v = self.gen_expr(value, scope)?;
                self.line(indent, &format!("{} = {};", t, v));
                Ok(())
            }
            StmtKind::Ret(Some(e)) => {
                let text = self.gen_expr(e, scope)?;
                self.line(indent, &format!("return {};", text));
                Ok(())
            }
            StmtKind::Ret(None) => {
                self.line(indent, "return;");
                Ok(())
            }
            StmtKind::Fail(e) => match &e.kind {
                ExprKind::Str(s) => {
                    self.line(indent, &format!("panic!(\"{}\");", Self::esc(s)));
                    Ok(())
                }
                _ => Err(
                    "fail with a non-string has no clean Solana-logic abort; refusing".into(),
                ),
            },
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("if {} {{", cond_s));
                self.gen_block(indent + 1, then_body, scope)?;
                if let Some(else_body) = else_body {
                    self.line(indent, "} else {");
                    self.gen_block(indent + 1, else_body, scope)?;
                }
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                if b.is_some() {
                    return Err("two-binding for has no Solana-logic form; refusing".into());
                }
                // Only range() iteration is supported.
                let (start_s, end_e) = match &iter.kind {
                    ExprKind::Call { callee, args } => {
                        let is_range = matches!(&callee.kind, ExprKind::Ident(n) if n == "range");
                        if !is_range {
                            return Err(
                                "for over non-range iterables has no Solana-logic form; refusing"
                                    .into(),
                            );
                        }
                        let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                        match vals.as_slice() {
                            [_] => (None, vals[0]),
                            [s, e] => (Some(*s), *e),
                            _ => {
                                return Err(
                                    "range() with step has no Solana-logic form; refusing".into()
                                )
                            }
                        }
                    }
                    _ => {
                        return Err(
                            "for over non-range iterables has no Solana-logic form; refusing"
                                .into(),
                        )
                    }
                };
                let s = match start_s {
                    Some(se) => self.gen_expr(se, scope)?,
                    None => "0".to_string(),
                };
                let e = self.gen_expr(end_e, scope)?;
                scope.insert(a.clone(), SolKind::Int);
                self.line(indent, &format!("for {} in {}..{} {{", a, s, e));
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::Whl { cond, body } => {
                let cond_s = self.gen_expr(cond, scope)?;
                self.line(indent, &format!("while {} {{", cond_s));
                self.gen_block(indent + 1, body, scope)?;
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::ExprStmt(e) => self.gen_expr_stmt(indent, e, scope),
            StmtKind::Todo => {
                Err("`...` placeholder body cannot become a Solana program; refusing".into())
            }
        }
    }

    fn gen_binding(
        &mut self,
        indent: usize,
        name: &str,
        ty: &Option<Type>,
        value: &Expr,
        is_mut: bool,
        scope: &mut HashMap<String, SolKind>,
    ) -> Result<(), String> {
        if matches!(value.kind, ExprKind::Unwrap { .. }) {
            return Err("`??` has no Solana-logic equivalent for internal calls; refusing".into());
        }
        let kind = ty.as_ref().map(kind_of_type).unwrap_or(SolKind::Other);
        let kind = if kind == SolKind::Other {
            self.expr_kind(value, scope)
        } else {
            kind
        };
        let decl_ty = match ty {
            Some(t) => solana_type(t)?,
            None => match kind {
                SolKind::Int => "i64".into(),
                SolKind::Str => "String".into(),
                SolKind::Bool => "bool".into(),
                SolKind::Other => {
                    return Err(format!(
                        "cannot infer a Solana-logic type for `{}`; annotate it",
                        name
                    ))
                }
            },
        };
        scope.insert(name.to_string(), kind);
        let v = self.gen_expr(value, scope)?;
        // A string literal needs `.to_string()` to become a String binding;
        // values already of kind Str (format!, concat) are Strings already.
        let v = if decl_ty == "String" && matches!(value.kind, ExprKind::Str(_)) {
            format!("{}.to_string()", v)
        } else {
            v
        };
        let m = if is_mut { "mut " } else { "" };
        self.line(indent, &format!("let {}{}: {} = {};", m, name, decl_ty, v));
        Ok(())
    }

    fn gen_block(
        &mut self,
        indent: usize,
        body: &[Stmt],
        scope: &mut HashMap<String, SolKind>,
    ) -> Result<(), String> {
        for s in body {
            self.gen_stmt(indent, s, scope)?;
        }
        Ok(())
    }

    /// Expression statements: `say(...)` prints; anything else is emitted as
    /// a bare call so internal pure calls still run.
    fn gen_expr_stmt(
        &mut self,
        indent: usize,
        e: &Expr,
        scope: &mut HashMap<String, SolKind>,
    ) -> Result<(), String> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "say") {
                let vals: Vec<&Expr> = args.iter().map(|a| a.expr()).collect();
                if vals.len() != 1 {
                    return Err("say takes exactly one argument".into());
                }
                let v = vals[0];
                let kind = self.expr_kind(v, scope);
                let text = self.gen_expr(v, scope)?;
                match kind {
                    SolKind::Int | SolKind::Str | SolKind::Bool => {
                        self.line(indent, &format!("println!(\"{{}}\", {});", text));
                        return Ok(());
                    }
                    SolKind::Other => {
                        return Err("say of this value has no Solana-logic form; refusing".into())
                    }
                }
            }
        }
        let text = self.gen_expr(e, scope)?;
        self.line(indent, &format!("{};", text));
        Ok(())
    }

    fn gen_expr(&mut self, e: &Expr, scope: &HashMap<String, SolKind>) -> Result<String, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(format!("{}i64", n)),
            ExprKind::Float(_) => Err("float literals have no Solana-logic form; refusing".into()),
            ExprKind::Bool(b) => Ok(b.to_string()),
            ExprKind::Str(s) => Ok(format!("\"{}\"", Self::esc(s))),
            ExprKind::InterpStr(parts) => {
                let mut fmt = String::new();
                let mut args = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => fmt.push_str(&Self::esc_fmt(t)),
                        StrPartExpr::Expr(ie) => {
                            let k = self.expr_kind(ie, scope);
                            let t = self.gen_expr(ie, scope)?;
                            match k {
                                SolKind::Int | SolKind::Bool | SolKind::Str => {
                                    fmt.push_str("{}");
                                    args.push(t);
                                }
                                SolKind::Other => {
                                    return Err(
                                        "interpolating this value has no Solana-logic form; refusing"
                                            .into(),
                                    )
                                }
                            }
                        }
                    }
                }
                Ok(format!("format!(\"{}\"{})", fmt, if args.is_empty() {
                    String::new()
                } else {
                    format!(", {}", args.join(", "))
                }))
            }
            ExprKind::NoneLit => Err("None has no Solana-logic form; refusing".into()),
            ExprKind::Ident(n) => Ok(n.clone()),
            ExprKind::List(_) | ExprKind::Map(_) => {
                Err("lists and maps have no v1 Solana-logic form; refusing".into())
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args, scope),
            ExprKind::Index { .. } => Err("indexing has no v1 Solana-logic form; refusing".into()),
            ExprKind::Field { .. } => Err(
                "field access has no v1 Solana-logic form (structs/enums are refused); refusing"
                    .into(),
            ),
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs, scope)?;
                let r = self.gen_expr(rhs, scope)?;
                let lk = self.expr_kind(lhs, scope);
                // String + is concatenation.
                if matches!(*op, BinOp::Add) && lk == SolKind::Str {
                    return Ok(format!("format!(\"{{}}{{}}\", {}, {})", l, r));
                }
                let o = match op {
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
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                Ok(format!("({} {} {})", l, o, r))
            }
            ExprKind::Unary { op, expr } => {
                let t = self.gen_expr(expr, scope)?;
                match op {
                    UnOp::Not => Ok(format!("(!{})", t)),
                    UnOp::Neg => Ok(format!("(-{})", t)),
                }
            }
            ExprKind::Unwrap { .. } => {
                Err("`??` has no Solana-logic equivalent for internal calls; refusing".into())
            }
        }
    }

    fn gen_call(
        &mut self,
        callee: &Expr,
        args: &[CallArg],
        scope: &HashMap<String, SolKind>,
    ) -> Result<String, String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("only direct function calls have a Solana-logic form; refusing".into()),
        };
        if args.iter().any(|a| !matches!(a, CallArg::Pos(_))) {
            return Err("named arguments have no Solana-logic form; refusing".into());
        }
        // Builtins.
        match name.as_str() {
            "say" => return Err("say is a statement, not an expression; refusing".into()),
            "range" => return Err("range() outside for has no Solana-logic form; refusing".into()),
            "len" => {
                let vals: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr(), scope))
                    .collect::<Result<_, _>>()?;
                if vals.len() != 1 {
                    return Err("len takes one argument".into());
                }
                let k = self.expr_kind(args[0].expr(), scope);
                return match k {
                    SolKind::Str => Ok(format!("({}.len() as i64)", vals[0])),
                    _ => Err("len() of this value has no Solana-logic form; refusing".into()),
                };
            }
            "abs" | "min" | "max" => {
                return Err(format!(
                    "builtin `{}` needs a v1 helper; refusing for now",
                    name
                ))
            }
            _ => {}
        }
        if !self.fn_names.contains(&name) {
            return Err(format!("unknown call `{}`; refusing", name));
        }
        let vals: Vec<String> = args
            .iter()
            .map(|a| self.gen_expr(a.expr(), scope))
            .collect::<Result<_, _>>()?;
        Ok(format!("{}({})", name, vals.join(", ")))
    }

    /// Best-effort kind of an expression for `say` routing and inference.
    fn expr_kind(&self, e: &Expr, scope: &HashMap<String, SolKind>) -> SolKind {
        match &e.kind {
            ExprKind::Int(_) => SolKind::Int,
            ExprKind::Str(_) | ExprKind::InterpStr(_) => SolKind::Str,
            ExprKind::Bool(_) => SolKind::Bool,
            ExprKind::Ident(n) => scope.get(n).copied().unwrap_or(SolKind::Other),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(n) if n == "len" => SolKind::Int,
                ExprKind::Ident(n) => self.fn_ret.get(n).copied().unwrap_or(SolKind::Other),
                _ => SolKind::Other,
            },
            ExprKind::Binary { op, lhs, .. } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Gt
                | BinOp::Le
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => SolKind::Bool,
                BinOp::Add if self.expr_kind(lhs, scope) == SolKind::Str => SolKind::Str,
                _ => SolKind::Int,
            },
            ExprKind::Unary { op, .. } => match op {
                UnOp::Not => SolKind::Bool,
                UnOp::Neg => SolKind::Int,
            },
            _ => SolKind::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{lexer, parser};

    fn parse_src(src: &str) -> Program {
        let toks = lexer::Lexer::tokenize(src).expect("lex");
        let mut p = parser::Parser::new(toks, src);
        p.parse_program().expect("parse")
    }

    const ESCROW: &str = r#"
def transfer_fee(amount: int, fee_bps: int) -> int do
    ret amount * fee_bps / 10000
end

def validate_transfer(balance: int, amount: int, fee_bps: int) -> int do
    if amount <= 0 do
        ret 2
    end
    if fee_bps < 0 do
        ret 3
    end
    if fee_bps > 10000 do
        ret 3
    end
    let total: int = amount + transfer_fee(amount, fee_bps)
    if total > balance do
        ret 1
    end
    ret 0
end

say(validate_transfer(10000, 5000, 50))
"#;

    #[test]
    fn logic_core_has_pub_fns_and_main() {
        let core = logic_core(&parse_src(ESCROW)).expect("logic core");
        assert!(core.contains("pub fn transfer_fee(amount: i64, fee_bps: i64) -> i64"));
        assert!(core.contains("pub fn validate_transfer(balance: i64, amount: i64, fee_bps: i64) -> i64"));
        assert!(core.contains("fn main()"));
        assert!(core.contains("println!"));
        // Truncated division preserved exactly (i64 semantics).
        assert!(core.contains("(amount * fee_bps) / 10000i64") || core.contains("/ 10000i64"));
    }

    #[test]
    fn program_embeds_logic_core_verbatim() {
        let prog = parse_src(ESCROW);
        let core = logic_core(&prog).expect("logic core");
        let program_src = generate_program(&prog, "cuni_escrow").expect("program");
        // Extract the delimited logic block, dedent one level, compare bytes.
        let start = program_src.find(LOGIC_START).expect("logic start marker");
        let end = program_src.find(LOGIC_END).expect("logic end marker");
        assert!(start < end);
        let block = &program_src[start..end];
        let mut extracted = String::new();
        let mut in_mod = false;
        for line in block.lines().skip(1) {
            if line.trim() == "mod logic {" {
                in_mod = true;
                continue;
            }
            if line.trim() == "}" && in_mod {
                // The closing brace of `mod logic` is the last `}` line at
                // indent 0 before LOGIC_END.
                if line.starts_with('}') {
                    break;
                }
            }
            if in_mod {
                let dedented = line.strip_prefix("    ").unwrap_or(line);
                extracted.push_str(dedented);
                extracted.push('\n');
            }
        }
        // Drop trailing blank lines the embedder normalized.
        let extracted = extracted.trim_end_matches('\n').to_string() + "\n";
        let core_norm = core.trim_end_matches('\n').to_string() + "\n";
        assert_eq!(
            extracted, core_norm,
            "program's mod logic must be byte-identical to the standalone logic core"
        );
    }

    #[test]
    fn program_shell_has_anchor_shape() {
        let program_src = generate_program(&parse_src(ESCROW), "cuni_escrow").expect("program");
        for marker in [
            SHELL_START,
            SHELL_END,
            "use anchor_lang::prelude::*;",
            "declare_id!",
            "#[program]",
            "pub mod cuni_escrow",
            "pub fn validate_transfer(ctx: Context<Run>",
            "pub fn transfer_fee(ctx: Context<Run>",
            "#[derive(Accounts)]",
            "pub struct Run<'info>",
            "pub signer: Signer<'info>,",
            "logic::validate_transfer(",
            "msg!(\"validate_transfer -> {}\", result);",
        ] {
            assert!(
                program_src.contains(marker),
                "program shell missing `{}`",
                marker
            );
        }
    }

    #[test]
    fn refuses_float_list_typ_unwrap() {
        for (tag, src) in [
            ("float", "def f() -> int do\n ret 1\nend\nsay(1.5)\n"),
            ("list", "def f() -> int do\n ret 1\nend\nlet xs = [1, 2]\nsay(1)\n"),
            (
                "typ",
                "typ Point do\n x: int\nend\ndef f() -> int do\n ret 1\nend\nsay(1)\n",
            ),
            (
                "unwrap",
                "def f() -> int do\n ret 1\nend\nlet x = f() ?? do\n ret 0\nend\nsay(x)\n",
            ),
        ] {
            let err = logic_core(&parse_src(src)).expect_err(&format!("{tag} must refuse"));
            assert!(!err.is_empty(), "{tag}: refusal needs a reason");
        }
    }

    #[test]
    fn string_and_bool_forms() {
        let src = r#"
def shout(s: str, b: bool) -> str do
    if b do
        ret "hi " + s
    end
    ret s
end

say(shout("bob", true))
say(len("hello"))
let greeting: str = "hi"
say(greeting)
"#;
        let core = logic_core(&parse_src(src)).expect("logic core");
        assert!(core.contains("pub fn shout(s: String, b: bool) -> String"));
        assert!(core.contains(".to_string()"));
        assert!(core.contains("as i64"), "len() should cast usize to i64");
    }
}

//! x86-64 assembly backend — a real native Linux seat.
//!
//! Emits Intel-syntax x86-64 assembly for GAS (`as`), linked with `gcc`
//! (the EXEC arm runs `gcc -x assembler -o <bin> <path>`). The emitted
//! program links libc; the "mini-runtime" is five small helpers emitted in
//! the header:
//!
//! - `cuni_say_int` — `printf("%ld\n", x)`
//! - `cuni_say_str` — `printf("%s\n", s)`
//! - `cuni_say_bool` — prints `True`/`False`
//! - `cuni_floormod` — Python-floored `%` (`/` truncates toward zero via
//!   `idiv`, matching CuNi; int add/sub/mul wrap like the interpreter)
//! - `cuni_concat` — malloc-based string `+`
//!
//! Value model: every CuNi value is 8 bytes. `int`/`bool` are int64 values
//! (bool is 0/1); `str` is a `char *` to NUL-terminated bytes. Locals live
//! in fixed `rbp`-relative stack slots; expression evaluation is a stack
//! machine on the hardware stack — every expression leaves its value in
//! `rax`, pushes/pops are always balanced before any `call`, and the stack
//! stays 16-byte aligned at every call site (System V ABI).
//!
//! Supported: the core subset — `say`/`let`/`mut`/assign, `def`/`ret`
//! (monomorphic int/str/bool), `if`/`else`, `while`, int arithmetic
//! `+ - *`, truncating `/`, floored `%`, comparisons, `and`/`or`/`not`
//! (short-circuit, like the interpreter), unary `-`/`not`, string literals
//! and `+` concatenation, string comparisons via `strcmp`.
//!
//! Refused (honest `Err`, never approximated): `float`/`dec`/`time`
//! literals, `typ`, `enum`, `iface`, generics, `opt`/`none`, fallible `?` /
//! `fail` / `??`, `link`, `ext`, lists, maps, indexing, field access, `for`
//! loops, `todo`, interpolated strings, named call arguments, bare
//! non-`say` expression statements, `say` used as an expression, unknown
//! variables/functions, and anything else not listed above.

use crate::ast::*;
use std::collections::HashMap;

/// Minimal type tracked per variable / expression.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ty {
    Int,
    Str,
    Bool,
}

fn ty_of(ty: &Type) -> Result<Ty, String> {
    match ty {
        Type::Named(n) if n == "int" => Ok(Ty::Int),
        Type::Named(n) if n == "str" => Ok(Ty::Str),
        Type::Named(n) if n == "bool" => Ok(Ty::Bool),
        _ => Err("type beyond core subset; refusing".into()),
    }
}

fn ident_ok(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Escape a Rust string as GAS `.asciz` bytes. Non-ASCII bytes (e.g. UTF-8
/// multibyte sequences) become 3-digit octal escapes so `printf("%s")`
/// emits byte-identical output.
fn gas_escape(s: &str) -> String {
    let mut r = String::new();
    for b in s.bytes() {
        match b {
            b'"' => r.push_str("\\\""),
            b'\\' => r.push_str("\\\\"),
            b'\n' => r.push_str("\\n"),
            b'\t' => r.push_str("\\t"),
            b'\r' => r.push_str("\\r"),
            0x20..=0x7e => r.push(b as char),
            _ => r.push_str(&format!("\\{:03o}", b)),
        }
    }
    r
}

const PARAM_REGS: [&str; 6] = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];

const HEADER: &str = "\
.intel_syntax noprefix
.data
.Lfmt_int:
    .asciz \"%ld\\n\"
.Lfmt_str:
    .asciz \"%s\\n\"
.Lcuni_true:
    .asciz \"True\"
.Lcuni_false:
    .asciz \"False\"
";

/// Helpers emitted once in `.text`: the seat's mini-runtime.
const HELPERS: &str = "\
.text
# void cuni_say_int(long x)
cuni_say_int:
    mov rsi, rdi
    lea rdi, .Lfmt_int[rip]
    xor eax, eax
    sub rsp, 8
    call printf
    add rsp, 8
    ret
# void cuni_say_str(char *s)
cuni_say_str:
    mov rsi, rdi
    lea rdi, .Lfmt_str[rip]
    xor eax, eax
    sub rsp, 8
    call printf
    add rsp, 8
    ret
# void cuni_say_bool(long b)  -- b is 0/1, prints True/False
cuni_say_bool:
    lea rsi, .Lcuni_false[rip]
    test rdi, rdi
    jz .Lcuni_sayb_done
    lea rsi, .Lcuni_true[rip]
.Lcuni_sayb_done:
    lea rdi, .Lfmt_str[rip]
    xor eax, eax
    sub rsp, 8
    call printf
    add rsp, 8
    ret
# long cuni_floormod(long a, long b)  -- Python-floored: ((a % b) + b) % b
cuni_floormod:
    mov rax, rdi
    cqo
    idiv rsi
    lea rax, [rdx + rsi]
    cqo
    idiv rsi
    mov rax, rdx
    ret
# char *cuni_concat(char *a, char *b)  -- malloc'd a+b
cuni_concat:
    push rbp
    mov rbp, rsp
    push rbx
    push r12
    push r13
    sub rsp, 8
    mov r12, rdi
    mov r13, rsi
    call strlen
    mov rbx, rax
    mov rdi, r13
    call strlen
    lea rdi, [rbx + rax + 1]
    call malloc
    mov rdi, rax
    mov rsi, r12
    call strcpy
    mov rdi, rax
    mov rsi, r13
    call strcat
    add rsp, 8
    pop r13
    pop r12
    pop rbx
    pop rbp
    ret
";

pub fn generate(program: &Program) -> Result<String, String> {
    let mut e = Emitter {
        data: String::new(),
        text: String::from(HELPERS),
        next_label: 0,
        next_str: 0,
        scope: HashMap::new(),
        fns: HashMap::new(),
        slots: 0,
    };
    e.gen_program(program)?;
    let mut out = String::from(
        "# Generated by the CuNi x86-64 assembly backend. Do not hand-edit.\n",
    );
    out.push_str(HEADER);
    out.push_str(&e.data);
    out.push_str(&e.text);
    Ok(out)
}

struct Emitter {
    data: String,
    text: String,
    next_label: usize,
    next_str: usize,
    /// variable name -> (rbp-relative slot offset in bytes, type)
    scope: HashMap<String, (u32, Ty)>,
    /// function name -> (param types, return type)
    fns: HashMap<String, (Vec<Ty>, Ty)>,
    /// slots allocated in the current function
    slots: u32,
}

impl Emitter {
    fn label(&mut self, tag: &str) -> String {
        let n = self.next_label;
        self.next_label += 1;
        format!(".Lc{}_{}", n, tag)
    }

    fn alloc_slot(&mut self) -> u32 {
        let off = (self.slots + 1) * 8;
        self.slots += 1;
        off
    }

    fn gen_program(&mut self, program: &Program) -> Result<(), String> {
        // First pass: collect defs, refuse non-core items.
        let mut script: Vec<&Stmt> = Vec::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    self.check_def(f)?;
                    if !ident_ok(&f.name) {
                        return Err(format!("def {}: name not encodable; refusing", f.name));
                    }
                    let params: Vec<Ty> =
                        f.params.iter().map(|p| ty_of(&p.ty)).collect::<Result<_, _>>()?;
                    let ret = ty_of(&f.ret_type)?;
                    if self.fns.insert(f.name.clone(), (params, ret)).is_some() {
                        return Err(format!("duplicate def {}; refusing", f.name));
                    }
                }
                Item::Stmt(s) => script.push(s),
                Item::Use(u) => {
                    return Err(format!("use {}: beyond core subset; refusing", u.name))
                }
                Item::Enum(_) => return Err("enum: beyond core subset; refusing".into()),
                Item::Typ(_) => return Err("typ: beyond core subset; refusing".into()),
                Item::Iface(_) => return Err("iface: beyond core subset; refusing".into()),
                Item::Ext(_) => return Err("ext: beyond core subset; refusing".into()),
            }
        }
        // Emit defs before main so every call target exists textually.
        for item in &program.items {
            if let Item::Def(f) = item {
                self.gen_fn(f)?;
            }
        }
        self.gen_main(&script)?;
        Ok(())
    }

    fn check_def(&self, f: &FnDecl) -> Result<(), String> {
        if f.fallible {
            return Err(format!(
                "def {}: fallible functions beyond core subset; refusing",
                f.name
            ));
        }
        if !f.generics.is_empty() {
            return Err(format!(
                "def {}: generics beyond core subset; refusing",
                f.name
            ));
        }
        if f.is_link {
            return Err(format!("link {}: beyond core subset; refusing", f.name));
        }
        ty_of(&f.ret_type)?;
        for p in &f.params {
            ty_of(&p.ty)?;
            if !ident_ok(&p.name) {
                return Err(format!("def {}: param name not encodable; refusing", f.name));
            }
        }
        Ok(())
    }

    /// Emit one function. `is_main` selects the `main` entry point (with
    /// `.globl`); user functions are mangled to `cuni_fn_<name>`.
    fn gen_fn(&mut self, f: &FnDecl) -> Result<(), String> {
        let mangled = format!("cuni_fn_{}", f.name);
        let (param_tys, _) = self.fns.get(&f.name).cloned().unwrap();
        self.scope = HashMap::new();
        self.slots = 0;
        for (p, t) in f.params.iter().zip(param_tys.iter()) {
            let off = self.alloc_slot();
            self.scope.insert(p.name.clone(), (off, *t));
        }
        let ret_label = self.label("ret");
        let mut body = String::new();
        for s in &f.body {
            self.gen_stmt(s, &mut body, &ret_label)?;
        }
        let frame = (self.slots * 8 + 15) & !15;
        let mut code = String::new();
        code.push_str(&format!("{}:\n", mangled));
        code.push_str("    push rbp\n");
        code.push_str("    mov rbp, rsp\n");
        if frame > 0 {
            code.push_str(&format!("    sub rsp, {}\n", frame));
        }
        // Spill params into their slots.
        for (i, p) in f.params.iter().enumerate() {
            let (off, _) = self.scope[&p.name];
            if i < 6 {
                code.push_str(&format!("    mov [rbp - {}], {}\n", off, PARAM_REGS[i]));
            } else {
                code.push_str(&format!("    mov rax, [rbp + {}]\n", 16 + 8 * (i - 6)));
                code.push_str(&format!("    mov [rbp - {}], rax\n", off));
            }
        }
        code.push_str(&body);
        code.push_str(&format!("{}:\n", ret_label));
        code.push_str("    mov rsp, rbp\n");
        code.push_str("    pop rbp\n");
        code.push_str("    ret\n");
        self.text.push_str(&code);
        Ok(())
    }

    fn gen_main(&mut self, script: &[&Stmt]) -> Result<(), String> {
        self.scope = HashMap::new();
        self.slots = 0;
        let ret_label = self.label("ret");
        let mut body = String::new();
        for s in script {
            self.gen_stmt(s, &mut body, &ret_label)?;
        }
        let frame = (self.slots * 8 + 15) & !15;
        let mut code = String::from(".globl main\nmain:\n");
        code.push_str("    push rbp\n");
        code.push_str("    mov rbp, rsp\n");
        if frame > 0 {
            code.push_str(&format!("    sub rsp, {}\n", frame));
        }
        code.push_str(&body);
        code.push_str("    xor eax, eax\n");
        code.push_str(&format!("{}:\n", ret_label));
        code.push_str("    mov rsp, rbp\n");
        code.push_str("    pop rbp\n");
        code.push_str("    ret\n");
        self.text.push_str(&code);
        Ok(())
    }

    fn gen_stmt(&mut self, s: &Stmt, out: &mut String, ret_label: &str) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                if !ident_ok(name) {
                    return Err(format!("binding `{}`: name not encodable; refusing", name));
                }
                let vt = self.gen_expr(value, out)?;
                if let Some(t) = ty {
                    let at = ty_of(t)?;
                    if at != vt {
                        return Err(format!(
                            "binding `{}`: annotated type disagrees with value; refusing",
                            name
                        ));
                    }
                }
                let off = self.alloc_slot();
                self.scope.insert(name.clone(), (off, vt));
                out.push_str(&format!("    mov [rbp - {}], rax\n", off));
                Ok(())
            }
            StmtKind::Assign { target, value } => {
                let slot = match &target.kind {
                    ExprKind::Ident(n) => self
                        .scope
                        .get(n)
                        .copied()
                        .ok_or_else(|| format!("unknown variable `{}`: refusing", n))?,
                    _ => return Err("complex assignment target: beyond core subset; refusing".into()),
                };
                let vt = self.gen_expr(value, out)?;
                if vt != slot.1 {
                    return Err("assignment changes a variable's type; refusing".into());
                }
                out.push_str(&format!("    mov [rbp - {}], rax\n", slot.0));
                Ok(())
            }
            StmtKind::Ret(e) => {
                let e = e.as_ref().ok_or_else(|| "bare `ret` without value; refusing".to_string())?;
                self.gen_expr(e, out)?;
                out.push_str(&format!("    jmp {}\n", ret_label));
                Ok(())
            }
            StmtKind::Fail(_) => Err("fail: beyond core subset; refusing".into()),
            StmtKind::If { cond, then_body, else_body } => {
                let ct = self.gen_expr(cond, out)?;
                if ct != Ty::Bool {
                    return Err("if condition must be bool; refusing".into());
                }
                let lab_else = self.label("else");
                let lab_end = self.label("endif");
                out.push_str("    test rax, rax\n");
                out.push_str(&format!(
                    "    jz {}\n",
                    if else_body.is_some() { &lab_else } else { &lab_end }
                ));
                for st in then_body {
                    self.gen_stmt(st, out, ret_label)?;
                }
                if let Some(eb) = else_body {
                    out.push_str(&format!("    jmp {}\n", lab_end));
                    out.push_str(&format!("{}:\n", lab_else));
                    for st in eb {
                        self.gen_stmt(st, out, ret_label)?;
                    }
                }
                out.push_str(&format!("{}:\n", lab_end));
                Ok(())
            }
            StmtKind::For { .. } => Err("for: beyond core subset; refusing".into()),
            StmtKind::Whl { cond, body } => {
                let lab_top = self.label("wtop");
                let lab_done = self.label("wdone");
                out.push_str(&format!("{}:\n", lab_top));
                let ct = self.gen_expr(cond, out)?;
                if ct != Ty::Bool {
                    return Err("while condition must be bool; refusing".into());
                }
                out.push_str("    test rax, rax\n");
                out.push_str(&format!("    jz {}\n", lab_done));
                for st in body {
                    self.gen_stmt(st, out, ret_label)?;
                }
                out.push_str(&format!("    jmp {}\n", lab_top));
                out.push_str(&format!("{}:\n", lab_done));
                Ok(())
            }
            StmtKind::ExprStmt(e) => match &e.kind {
                ExprKind::Call { callee, args } => {
                    let is_say = matches!(&callee.kind, ExprKind::Ident(n) if n == "say");
                    if !is_say {
                        return Err("bare call (non-say): beyond core subset; refusing".into());
                    }
                    if args.len() != 1 {
                        return Err("say: exactly one argument in core subset; refusing".into());
                    }
                    if args[0].is_named() {
                        return Err("named arguments: beyond core subset; refusing".into());
                    }
                    let at = self.gen_expr(args[0].expr(), out)?;
                    out.push_str("    mov rdi, rax\n");
                    let helper = match at {
                        Ty::Int => "cuni_say_int",
                        Ty::Str => "cuni_say_str",
                        Ty::Bool => "cuni_say_bool",
                    };
                    // rsp is 16-aligned here (all expression pushes are
                    // balanced), so the call needs no adjustment.
                    out.push_str(&format!("    call {}\n", helper));
                    Ok(())
                }
                _ => Err("bare expression: beyond core subset; refusing".into()),
            },
            StmtKind::Todo => Err("...: beyond core subset; refusing".into()),
        }
    }

    /// Append instructions evaluating `e`, leaving the value in `rax`.
    /// Returns the value's type.
    fn gen_expr(&mut self, e: &Expr, out: &mut String) -> Result<Ty, String> {
        match &e.kind {
            ExprKind::Int(n) => {
                out.push_str(&format!("    mov rax, {}\n", n));
                Ok(Ty::Int)
            }
            ExprKind::Bool(b) => {
                out.push_str(&format!("    mov rax, {}\n", if *b { 1 } else { 0 }));
                Ok(Ty::Bool)
            }
            ExprKind::Str(s) => {
                let n = self.next_str;
                self.next_str += 1;
                let lab = format!(".Lstr{}", n);
                self.data
                    .push_str(&format!("{}:\n    .asciz \"{}\"\n", lab, gas_escape(s)));
                out.push_str(&format!("    lea rax, {}[rip]\n", lab));
                Ok(Ty::Str)
            }
            ExprKind::Float(_) => Err("float: beyond core subset; refusing".into()),
            ExprKind::Dec(_) => Err("dec: beyond core subset; refusing".into()),
            ExprKind::Time(_) => Err("time: beyond core subset; refusing".into()),
            ExprKind::NoneLit => Err("none: beyond core subset; refusing".into()),
            ExprKind::Ident(n) => {
                let (off, t) = self
                    .scope
                    .get(n)
                    .copied()
                    .ok_or_else(|| format!("unknown variable `{}`: refusing", n))?;
                out.push_str(&format!("    mov rax, [rbp - {}]\n", off));
                Ok(t)
            }
            ExprKind::InterpStr(_) => Err("interpolated string: beyond core subset; refusing".into()),
            ExprKind::List(_) => Err("list: beyond core subset; refusing".into()),
            ExprKind::Map(_) => Err("map: beyond core subset; refusing".into()),
            ExprKind::Index { .. } => Err("indexing: beyond core subset; refusing".into()),
            ExprKind::Field { .. } => Err("field access: beyond core subset; refusing".into()),
            ExprKind::Unwrap { .. } => Err("??: beyond core subset; refusing".into()),
            ExprKind::Unary { op, expr } => {
                let t = self.gen_expr(expr, out)?;
                match op {
                    UnOp::Not => {
                        if t != Ty::Bool {
                            return Err("`not` on non-bool: beyond core subset; refusing".into());
                        }
                        out.push_str("    xor rax, 1\n");
                        Ok(Ty::Bool)
                    }
                    UnOp::Neg => {
                        if t != Ty::Int {
                            return Err("unary `-` on non-int: beyond core subset; refusing".into());
                        }
                        out.push_str("    neg rax\n");
                        Ok(Ty::Int)
                    }
                }
            }
            ExprKind::Call { callee, args } => {
                let name = match &callee.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => return Err("indirect call: beyond core subset; refusing".into()),
                };
                if name == "say" {
                    return Err("say: use as a statement, not an expression; refusing".into());
                }
                let (param_tys, ret_ty) = self
                    .fns
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| format!("call to unknown function `{}`: refusing", name))?;
                if args.len() != param_tys.len() {
                    return Err(format!(
                        "call to `{}`: arity mismatch; refusing",
                        name
                    ));
                }
                // Evaluate left-to-right, pushing each value; then move the
                // first six into registers, leaving stack args at [rsp].
                for (a, pt) in args.iter().zip(param_tys.iter()) {
                    if a.is_named() {
                        return Err("named arguments: beyond core subset; refusing".into());
                    }
                    let at = self.gen_expr(a.expr(), out)?;
                    if at != *pt {
                        return Err(format!(
                            "call to `{}`: argument type mismatch; refusing",
                            name
                        ));
                    }
                    out.push_str("    push rax\n");
                }
                let n = args.len();
                let in_regs = n.min(6);
                for i in 0..in_regs {
                    // args were pushed in order; arg i sits at [rsp + 8*(n-1-i)].
                    out.push_str(&format!(
                        "    mov {}, [rsp + {}]\n",
                        PARAM_REGS[i],
                        8 * (n - 1 - i)
                    ));
                }
                // Stack args (7th+) must sit at [rsp], [rsp+8], ... with rsp
                // 16-aligned at the call. With an even count they already do
                // after dropping the register slots; with an odd count the
                // block is relocated 40 bytes up (high-to-low, memmove-safe)
                // so the call stays aligned and the callee still sees arg7
                // at [rbp+16].
                let stack_args = n - in_regs;
                if stack_args == 0 {
                    if n > 0 {
                        out.push_str(&format!("    add rsp, {}\n", 8 * n));
                    }
                } else if stack_args % 2 == 0 {
                    out.push_str(&format!("    add rsp, {}\n", 8 * in_regs));
                } else {
                    for j in (0..stack_args).rev() {
                        out.push_str(&format!("    mov rax, [rsp + {}]\n", 8 * j));
                        out.push_str(&format!("    mov [rsp + {}], rax\n", 8 * j + 40));
                    }
                    out.push_str("    add rsp, 40\n");
                }
                out.push_str(&format!("    call cuni_fn_{}\n", name));
                let cleanup = 8 * stack_args + if stack_args % 2 == 1 { 8 } else { 0 };
                if cleanup > 0 {
                    out.push_str(&format!("    add rsp, {}\n", cleanup));
                }
                Ok(ret_ty)
            }
            ExprKind::Binary { op, lhs, rhs } => self.gen_binary(*op, lhs, rhs, out),
        }
    }

    /// Evaluate lhs, push; evaluate rhs into rax; pop lhs into rcx.
    /// On return rax holds the result (except `and`/`or`, which also end in rax).
    fn gen_binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        out: &mut String,
    ) -> Result<Ty, String> {
        // Short-circuiting logical operators evaluate rhs conditionally.
        match op {
            BinOp::And | BinOp::Or => {
                let lt = self.gen_expr(lhs, out)?;
                if lt != Ty::Bool {
                    return Err("`and`/`or` on non-bool: beyond core subset; refusing".into());
                }
                let lab_false = self.label("andf");
                let lab_end = self.label("ande");
                let lab_true = self.label("ort");
                let lab_oend = self.label("ore");
                match op {
                    BinOp::And => {
                        out.push_str("    test rax, rax\n");
                        out.push_str(&format!("    jz {}\n", lab_false));
                        let rt = self.gen_expr(rhs, out)?;
                        if rt != Ty::Bool {
                            return Err("`and` on non-bool: beyond core subset; refusing".into());
                        }
                        out.push_str(&format!("    jmp {}\n", lab_end));
                        out.push_str(&format!("{}:\n", lab_false));
                        out.push_str("    xor eax, eax\n");
                        out.push_str(&format!("{}:\n", lab_end));
                    }
                    _ => {
                        out.push_str("    test rax, rax\n");
                        out.push_str(&format!("    jnz {}\n", lab_true));
                        let rt = self.gen_expr(rhs, out)?;
                        if rt != Ty::Bool {
                            return Err("`or` on non-bool: beyond core subset; refusing".into());
                        }
                        out.push_str(&format!("    jmp {}\n", lab_oend));
                        out.push_str(&format!("{}:\n", lab_true));
                        out.push_str("    mov rax, 1\n");
                        out.push_str(&format!("{}:\n", lab_oend));
                    }
                }
                return Ok(Ty::Bool);
            }
            _ => {}
        }
        let lt = self.gen_expr(lhs, out)?;
        out.push_str("    push rax\n");
        let rt = self.gen_expr(rhs, out)?;
        // rhs value is in rax; move lhs into rcx.
        match op {
            BinOp::Add if lt == Ty::Str && rt == Ty::Str => {
                out.push_str("    mov rsi, rax\n");
                out.push_str("    pop rdi\n");
                out.push_str("    call cuni_concat\n");
                return Ok(Ty::Str);
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                if lt != Ty::Int || rt != Ty::Int {
                    return Err("arithmetic on non-int: beyond core subset; refusing".into());
                }
                match op {
                    BinOp::Add => {
                        out.push_str("    pop rcx\n");
                        out.push_str("    add rax, rcx\n");
                    }
                    BinOp::Sub => {
                        out.push_str("    pop rcx\n");
                        out.push_str("    sub rcx, rax\n");
                        out.push_str("    mov rax, rcx\n");
                    }
                    BinOp::Mul => {
                        out.push_str("    pop rcx\n");
                        out.push_str("    imul rax, rcx\n");
                    }
                    BinOp::Div => {
                        // idiv truncates toward zero: CuNi semantics.
                        out.push_str("    mov rsi, rax\n");
                        out.push_str("    pop rax\n");
                        out.push_str("    cqo\n");
                        out.push_str("    idiv rsi\n");
                    }
                    BinOp::Mod => {
                        out.push_str("    mov rsi, rax\n");
                        out.push_str("    pop rdi\n");
                        out.push_str("    call cuni_floormod\n");
                    }
                    _ => unreachable!(),
                }
                return Ok(Ty::Int);
            }
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                let setcc = match op {
                    BinOp::Eq => "sete",
                    BinOp::Ne => "setne",
                    BinOp::Lt => "setl",
                    BinOp::Gt => "setg",
                    BinOp::Le => "setle",
                    _ => "setge",
                };
                if lt == Ty::Str && rt == Ty::Str {
                    out.push_str("    mov rsi, rax\n");
                    out.push_str("    pop rdi\n");
                    out.push_str("    call strcmp\n");
                    // strcmp returns int: compare eax (32-bit), not rax —
                    // glibc zero-extends the upper half, so a 64-bit
                    // signed compare would misread e.g. -1 as positive.
                    out.push_str("    cmp eax, 0\n");
                } else if lt == Ty::Int && rt == Ty::Int {
                    out.push_str("    pop rcx\n");
                    out.push_str("    cmp rcx, rax\n");
                } else if lt == Ty::Bool && rt == Ty::Bool
                    && matches!(op, BinOp::Eq | BinOp::Ne)
                {
                    out.push_str("    pop rcx\n");
                    out.push_str("    cmp rcx, rax\n");
                } else {
                    return Err("comparison on mismatched types: beyond core subset; refusing"
                        .into());
                }
                out.push_str(&format!("    {} al\n", setcc));
                out.push_str("    movzx rax, al\n");
                return Ok(Ty::Bool);
            }
            BinOp::And | BinOp::Or => unreachable!(),
        }
    }
}

//! COBOL backend — custom core-subset native seat (seat id `cob`).
//!
//! Emits free-format GnuCOBOL (`cobc -x -free`): one `PROGRAM-ID. CUNI-MAIN`
//! whose `def`s become paragraphs sharing an explicit call frame. Paragraph
//! `PERFORM` is re-entrant, but WORKING-STORAGE is static across recursive
//! `CALL`s (verified: a recursive nested program saw shared storage), so
//! every function's params and `let`/`mut` bindings live in `CUNI-FRAME`, an
//! `OCCURS` table indexed by `CUNI-SP` (push on entry, pop on `ret`/exit;
//! `fact(5)` = 120 verified).
//!
//! Types: ints are `PIC S9(19)` (decimal-exact; `DIVIDE ... GIVING`
//! truncates toward zero — CuNi `/` — verified for negative operands, and
//! `%` is lowered to a floored modulo via a truncated `DIVIDE ... REMAINDER`
//! plus a sign adjustment). Strings are `PIC X(4096)` buffers with an
//! explicit length companion; `DISPLAY buf(1:len)` prints exactly (a
//! zero-length reference modification is legal — verified), `STRING ...
//! INTO` concatenates with an explicit overflow refusal, and comparisons use
//! a length-aware template (COBOL pads the shorter operand with spaces,
//! which disagrees with lexicographic order when the longer string's next
//! byte is below `0x20` — the template compares the common prefix, then the
//! longer string wins). Booleans are `PIC 9` 0/1; `say` renders them
//! `True`/`False`; `and`/`or` short-circuit like the interpreter. Integers
//! print through a floating-`-` edited picture plus `TRIM ... LEADING`
//! (a bare `DISPLAY` of `PIC 9(n)` emits leading zeros — verified).
//!
//! Refuses everything beyond the core subset with honest `Err`s (same
//! wording family as the core); never approximates.
//!
//! Wiring note: `emit.rs`/`main.rs` dispatch is left to the parent — this file
//! only provides `generate`.

use crate::ast::*;
use std::collections::HashMap;

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

/// COBOL data names: uppercase, `_` -> `-` (CuNi identifiers never contain
/// `-`, so the mapping is injective). All generated names are prefixed, so
/// COBOL reserved words can never collide.
fn cob_name(s: &str) -> String {
    s.to_ascii_uppercase().replace('_', "-")
}

/// Render a CuNi string as one COBOL literal expression. Printable ASCII
/// stays inline (`"` doubled); every other byte becomes an `X"HH"` chunk
/// joined with `&`. Byte length of the value is `s.len()`.
fn cob_str(s: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    for &b in s.as_bytes() {
        if b == b'"' {
            cur.push_str("\"\"");
        } else if (0x20..=0x7E).contains(&b) {
            cur.push(b as char);
        } else {
            if !cur.is_empty() {
                parts.push(format!("\"{cur}\""));
                cur.clear();
            }
            parts.push(format!("X\"{b:02X}\""));
        }
    }
    if !cur.is_empty() {
        parts.push(format!("\"{cur}\""));
    }
    // Never emit a zero-length literal (GnuCOBOL warns and assumes SPACE);
    // empty strings are materialized as a zero length instead.
    if parts.is_empty() {
        "X\"\"".to_string()
    } else {
        parts.join(" & ")
    }
}

/// A place an int value can live: either a plain data item (literal,
/// variable, temp) or an inline arithmetic expression (only legal inside
/// COMPUTE/conditions — materialize before MOVE/DIVIDE/STRING).
enum IPlace {
    Simple(String),
    Expr(String),
}

/// A place a string value can live.
enum StrPlace {
    /// (buffer item, length item)
    Pair(String, String),
    /// (literal expression, byte length)
    Lit(String, usize),
}

enum Place {
    Int(String),
    Bool(String),
    Str(String, String),
}

impl Place {
    fn ty(&self) -> Ty {
        match self {
            Place::Int(_) => Ty::Int,
            Place::Bool(_) => Ty::Bool,
            Place::Str(_, _) => Ty::Str,
        }
    }
}

const MAX_INT_TEMPS: u32 = 500;
const MAX_BOOL_TEMPS: u32 = 500;
const MAX_STR_TEMPS: u32 = 200;
const STR_BUF: usize = 4096;
const MAX_FRAMES: u32 = 1024;

struct Emitter {
    /// def name -> (params, ret). Fully populated before any body is walked.
    sigs: HashMap<String, (Vec<(String, Ty)>, Ty)>,
    order: Vec<String>,
    /// top-level vars, insertion order.
    globals: HashMap<String, Ty>,
    global_order: Vec<String>,
    /// per-def locals (params first), insertion order.
    flocals: HashMap<String, HashMap<String, Ty>>,
    flocal_order: HashMap<String, Vec<String>>,
    n_int: u32,
    n_bool: u32,
    n_str: u32,
    /// Current def being walked (None = top level).
    cur_fn: Option<String>,
    /// IF/PERFORM nesting depth. A COBOL period terminates the whole
    /// sentence, so inside a scope inner statements must not carry
    /// periods (the scope's outermost END-IF/END-PERFORM keeps its own).
    scope: usize,
    /// Statements of the unit currently being walked.
    stmts: Vec<String>,
    level: usize,
}

impl Emitter {
    fn new() -> Self {
        Emitter {
            sigs: HashMap::new(),
            order: Vec::new(),
            globals: HashMap::new(),
            global_order: Vec::new(),
            flocals: HashMap::new(),
            flocal_order: HashMap::new(),
            n_int: 0,
            n_bool: 0,
            n_str: 0,
            cur_fn: None,
            stmts: Vec::new(),
            level: 0,
            scope: 0,
        }
    }

    fn ln(&mut self, s: &str) {
        // Inside an IF/PERFORM scope a period would terminate the whole
        // COBOL sentence (a following ELSE becomes a syntax error), so the
        // scope's outermost END-IF/END-PERFORM carries the period instead.
        let s = if self.scope > 0 {
            s.strip_suffix('.').unwrap_or(s)
        } else {
            s
        };
        let ind = "    ".repeat(self.level);
        self.stmts.push(format!("{ind}{s}"));
    }

    fn fresh_int(&mut self) -> Result<String, String> {
        self.n_int += 1;
        if self.n_int > MAX_INT_TEMPS {
            return Err("expression too complex for the COBOL seat; refusing".into());
        }
        Ok(format!("CUNI-T{}", self.n_int))
    }

    fn fresh_bool(&mut self) -> Result<String, String> {
        self.n_bool += 1;
        if self.n_bool > MAX_BOOL_TEMPS {
            return Err("expression too complex for the COBOL seat; refusing".into());
        }
        Ok(format!("CUNI-B{}", self.n_bool))
    }

    fn fresh_str(&mut self) -> Result<(String, String), String> {
        self.n_str += 1;
        if self.n_str > MAX_STR_TEMPS {
            return Err("expression too complex for the COBOL seat; refusing".into());
        }
        Ok((format!("CUNI-S{}", self.n_str), format!("CUNI-S{}-LEN", self.n_str)))
    }
}

impl Emitter {
    /// Frame slot reference for a def-local variable.
    fn frame_ref(&self, f: &str, name: &str, is_len: bool) -> String {
        let cf = cob_name(f);
        let cn = cob_name(name);
        let slot = if is_len {
            format!("FF-{cf}-{cn}-LEN")
        } else {
            format!("FF-{cf}-{cn}")
        };
        format!("{slot} OF CUNI-FRAME(CUNI-SP)")
    }

    fn place_of(&self, name: &str) -> Result<Place, String> {
        match &self.cur_fn {
            Some(f) => match self.flocals.get(f).and_then(|m| m.get(name)) {
                Some(Ty::Int) => Ok(Place::Int(self.frame_ref(f, name, false))),
                Some(Ty::Bool) => Ok(Place::Bool(self.frame_ref(f, name, false))),
                Some(Ty::Str) => Ok(Place::Str(
                    self.frame_ref(f, name, false),
                    self.frame_ref(f, name, true),
                )),
                None => Err(format!("unknown variable `{name}`: refusing")),
            },
            None => match self.globals.get(name) {
                Some(Ty::Int) => Ok(Place::Int(format!("V-{}", cob_name(name)))),
                Some(Ty::Bool) => Ok(Place::Bool(format!("V-{}", cob_name(name)))),
                Some(Ty::Str) => Ok(Place::Str(
                    format!("V-{}", cob_name(name)),
                    format!("V-{}-LEN", cob_name(name)),
                )),
                None => Err(format!("unknown variable `{name}`: refusing")),
            },
        }
    }

    /// Bind `name` in the current scope (declaring it on first use).
    fn bind(&mut self, name: &str, ty: Ty) -> Result<Place, String> {
        let key = name.to_ascii_uppercase();
        match self.cur_fn.clone() {
            Some(f) => {
                let m = self.flocals.entry(f.clone()).or_default();
                match m.get(name) {
                    Some(&et) if et == ty => {}
                    Some(_) => {
                        return Err(format!(
                            "`{name}` redefined with a different type: refusing"
                        ))
                    }
                    None => {
                        if m.keys().any(|k| k.to_ascii_uppercase() == key) {
                            return Err(format!(
                                "binding `{name}` collides case-insensitively \
                                 with an existing name (COBOL is case-insensitive); refusing"
                            ));
                        }
                        m.insert(name.to_string(), ty);
                        self.flocal_order.entry(f.clone()).or_default().push(name.to_string());
                    }
                }
            }
            None => match self.globals.get(name) {
                Some(&et) if et == ty => {}
                Some(_) => {
                    return Err(format!(
                        "`{name}` redefined with a different type: refusing"
                    ))
                }
                None => {
                    if self.globals.keys().any(|k| k.to_ascii_uppercase() == key) {
                        return Err(format!(
                            "binding `{name}` collides case-insensitively \
                             with an existing name (COBOL is case-insensitive); refusing"
                        ));
                    }
                    self.globals.insert(name.to_string(), ty);
                    self.global_order.push(name.to_string());
                }
            },
        }
        self.place_of(name)
    }

    fn check_def(&mut self, f: &FnDecl) -> Result<(), String> {
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
        if f.name.starts_with("cuni_") {
            return Err(format!(
                "def {}: reserved cuni_ prefix (collides with seat temporaries); refusing",
                f.name
            ));
        }
        let key = f.name.to_ascii_uppercase();
        if self.order.iter().any(|n| n.to_ascii_uppercase() == key) {
            return Err(format!("def {}: duplicate definition; refusing", f.name));
        }
        ty_of(&f.ret_type)?;
        for p in &f.params {
            ty_of(&p.ty)?;
        }
        Ok(())
    }

    /// Type of an expression, without emitting anything.
    fn infer(&self, e: &Expr) -> Result<Ty, String> {
        match &e.kind {
            ExprKind::Int(_) => Ok(Ty::Int),
            ExprKind::Bool(_) => Ok(Ty::Bool),
            ExprKind::Str(_) => Ok(Ty::Str),
            ExprKind::Ident(n) => Ok(self.place_of(n)?.ty()),
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Add => {
                    let l = self.infer(lhs)?;
                    let r = self.infer(rhs)?;
                    if l == Ty::Str && r == Ty::Str {
                        Ok(Ty::Str)
                    } else if l == Ty::Int && r == Ty::Int {
                        Ok(Ty::Int)
                    } else {
                        Err("mixed-type `+`: beyond core subset; refusing".into())
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                    self.require_ints(lhs, rhs, op_name(op))?;
                    Ok(Ty::Int)
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    let l = self.infer(lhs)?;
                    let r = self.infer(rhs)?;
                    if l == r {
                        Ok(Ty::Bool)
                    } else {
                        Err("comparison on mismatched types: beyond core subset; refusing".into())
                    }
                }
                BinOp::And | BinOp::Or => {
                    self.require_bools(lhs, rhs, op_name(op))?;
                    Ok(Ty::Bool)
                }
            },
            ExprKind::Unary { op: UnOp::Not, expr } => {
                if self.infer(expr)? != Ty::Bool {
                    return Err("`not` on non-bool: beyond core subset; refusing".into());
                }
                Ok(Ty::Bool)
            }
            ExprKind::Unary { op: UnOp::Neg, expr } => {
                if self.infer(expr)? != Ty::Int {
                    return Err("unary `-` on non-int: beyond core subset; refusing".into());
                }
                Ok(Ty::Int)
            }
            ExprKind::Call { callee, .. } => Ok(self.call_head(callee)?.2),
            ExprKind::Float(_) => Err("float: beyond core subset; refusing".into()),
            ExprKind::Dec(_) => Err("dec: beyond core subset; refusing".into()),
            ExprKind::Time(_) => Err("time: beyond core subset; refusing".into()),
            ExprKind::NoneLit => Err("none: beyond core subset; refusing".into()),
            ExprKind::InterpStr(_) => Err("interpolated string: beyond core subset; refusing".into()),
            ExprKind::List(_) => Err("list: beyond core subset; refusing".into()),
            ExprKind::Map(_) => Err("map: beyond core subset; refusing".into()),
            ExprKind::Index { .. } => Err("indexing: beyond core subset; refusing".into()),
            ExprKind::Field { .. } => Err("field access: beyond core subset; refusing".into()),
            ExprKind::Unwrap { .. } => Err("??: beyond core subset; refusing".into()),
        }
    }

    fn require_ints(&self, lhs: &Expr, rhs: &Expr, op: &str) -> Result<(), String> {
        if self.infer(lhs)? == Ty::Int && self.infer(rhs)? == Ty::Int {
            Ok(())
        } else {
            Err(format!("`{op}` on non-int: beyond core subset; refusing"))
        }
    }

    fn require_bools(&self, lhs: &Expr, rhs: &Expr, op: &str) -> Result<(), String> {
        if self.infer(lhs)? == Ty::Bool && self.infer(rhs)? == Ty::Bool {
            Ok(())
        } else {
            Err(format!("`{op}` on non-bool: beyond core subset; refusing"))
        }
    }

    fn call_head(&self, callee: &Expr) -> Result<(String, Vec<(String, Ty)>, Ty), String> {
        let name = match &callee.kind {
            ExprKind::Ident(n) => n.clone(),
            _ => return Err("indirect call: beyond core subset; refusing".into()),
        };
        if name == "say" {
            return Err("say: use as a statement, not an expression; refusing".into());
        }
        let (params, ret) = self
            .sigs
            .get(&name)
            .cloned()
            .ok_or_else(|| format!("call to unknown function `{name}`: refusing"))?;
        Ok((name, params, ret))
    }

    /// Evaluate call arguments into the callee's parameter globals.
    fn call_args(&mut self, name: &str, params: &[(String, Ty)], args: &[CallArg]) -> Result<(), String> {
        if args.len() != params.len() {
            return Err(format!(
                "call to `{name}`: arity mismatch ({} given, {} expected); refusing",
                args.len(),
                params.len()
            ));
        }
        let cf = cob_name(name);
        for (a, (pn, pt)) in args.iter().zip(params.iter()) {
            if a.is_named() {
                return Err("named arguments: beyond core subset; refusing".into());
            }
            let at = self.infer(a.expr())?;
            if at != *pt {
                return Err(format!("call to `{name}`: argument type mismatch; refusing"));
            }
            let pg = format!("CUNI-P-{cf}-{}", cob_name(pn));
            match pt {
                Ty::Int => {
                    let item = self.int_item(a.expr())?;
                    self.ln(&format!("MOVE {item} TO {pg}."));
                }
                Ty::Bool => {
                    self.gen_bool(a.expr(), &pg)?;
                }
                Ty::Str => {
                    let sp = self.str_expr(a.expr())?;
                    let (b, l) = self.str_to_pair(sp)?;
                    self.ln(&format!("MOVE {b} TO {pg}."));
                    self.ln(&format!("MOVE {l} TO {pg}-LEN."));
                }
            }
        }
        Ok(())
    }

    /// Int expression as an IPlace; setup statements are emitted inline.
    fn int_expr(&mut self, e: &Expr) -> Result<IPlace, String> {
        match &e.kind {
            ExprKind::Int(n) => Ok(IPlace::Simple(n.to_string())),
            ExprKind::Ident(n) => match self.place_of(n)? {
                Place::Int(p) => Ok(IPlace::Simple(p)),
                _ => Err(format!("variable `{n}` is not an int: beyond core subset; refusing")),
            },
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul => {
                    let l = self.int_item(lhs)?;
                    let r = self.int_item(rhs)?;
                    let o = match op {
                        BinOp::Add => "+",
                        BinOp::Sub => "-",
                        _ => "*",
                    };
                    Ok(IPlace::Expr(format!("({l} {o} {r})")))
                }
                // Truncating division via DIVIDE (verified: truncates toward
                // zero, matching CuNi `/`).
                BinOp::Div => {
                    let l = self.int_item(lhs)?;
                    let r = self.int_item(rhs)?;
                    self.ln(&format!("MOVE {l} TO CUNI-DA."));
                    self.ln(&format!("MOVE {r} TO CUNI-DB."));
                    self.ln("DIVIDE CUNI-DB INTO CUNI-DA GIVING CUNI-DQ.");
                    let t = self.fresh_int()?;
                    self.ln(&format!("MOVE CUNI-DQ TO {t}."));
                    Ok(IPlace::Simple(t))
                }
                // Floored modulo: truncated DIVIDE ... REMAINDER, then add
                // the divisor when the remainder is non-zero and its sign
                // disagrees with the divisor's.
                BinOp::Mod => {
                    let l = self.int_item(lhs)?;
                    let r = self.int_item(rhs)?;
                    self.ln(&format!("MOVE {l} TO CUNI-MA."));
                    self.ln(&format!("MOVE {r} TO CUNI-MB."));
                    self.ln("DIVIDE CUNI-MB INTO CUNI-MA GIVING CUNI-MQ REMAINDER CUNI-MR.");
                    self.ln("IF CUNI-MR NOT = 0");
                    self.level += 1;
        self.scope += 1;
                    self.ln("IF (CUNI-MA < 0 AND CUNI-MB > 0) OR (CUNI-MA > 0 AND CUNI-MB < 0)");
                    self.level += 1;
        self.scope += 1;
                    self.ln("COMPUTE CUNI-MR = CUNI-MR + CUNI-MB.");
                    self.level -= 1;
        self.scope -= 1;
                    self.ln("END-IF.");
                    self.level -= 1;
        self.scope -= 1;
                    self.ln("END-IF.");
                    let t = self.fresh_int()?;
                    self.ln(&format!("MOVE CUNI-MR TO {t}."));
                    Ok(IPlace::Simple(t))
                }
                _ => Err(format!(
                    "`{}` is not an int operator: beyond core subset; refusing",
                    op_name(op)
                )),
            },
            ExprKind::Unary { op: UnOp::Neg, expr } => {
                let v = self.int_item(expr)?;
                Ok(IPlace::Expr(format!("(0 - {v})")))
            }
            ExprKind::Unary { op: UnOp::Not, .. } => {
                Err("`not` on non-int: beyond core subset; refusing".into())
            }
            ExprKind::Call { callee, args } => {
                let (name, params, ret) = self.call_head(callee)?;
                if ret != Ty::Int {
                    return Err(format!(
                        "call to `{name}` used as int but returns otherwise; refusing"
                    ));
                }
                self.call_args(&name, &params, args)?;
                self.ln(&format!("PERFORM F-{0} THRU F-{0}-EXIT.", cob_name(&name)));
                let t = self.fresh_int()?;
                self.ln(&format!("MOVE CUNI-R-{} TO {t}.", cob_name(&name)));
                Ok(IPlace::Simple(t))
            }
            _ => Err("non-int expression in int position: beyond core subset; refusing".into()),
        }
    }

    /// Int expression reduced to a plain data item (materializing inline
    /// arithmetic through a temp — required before MOVE/DIVIDE/STRING).
    fn int_item(&mut self, e: &Expr) -> Result<String, String> {
        match self.int_expr(e)? {
            IPlace::Simple(s) => Ok(s),
            IPlace::Expr(x) => {
                let t = self.fresh_int()?;
                self.ln(&format!("COMPUTE {t} = {x}."));
                Ok(t)
            }
        }
    }

    /// String expression as a StrPlace; setup statements emitted inline.
    fn str_expr(&mut self, e: &Expr) -> Result<StrPlace, String> {
        match &e.kind {
            ExprKind::Str(s) => Ok(StrPlace::Lit(cob_str(s), s.len())),
            ExprKind::Ident(n) => match self.place_of(n)? {
                Place::Str(b, l) => Ok(StrPlace::Pair(b, l)),
                _ => Err(format!("variable `{n}` is not a str: beyond core subset; refusing")),
            },
            ExprKind::Binary { op: BinOp::Add, lhs, rhs } => {
                if self.infer(lhs)? != Ty::Str || self.infer(rhs)? != Ty::Str {
                    return Err("mixed-type `+`: beyond core subset; refusing".into());
                }
                let l = self.str_expr(lhs)?;
                let r = self.str_expr(rhs)?;
                let (lb, ll) = self.str_operand(l)?;
                let (rb, rl) = self.str_operand(r)?;
                let (tb, tl) = self.fresh_str()?;
                self.ln(&format!("IF {ll} + {rl} > {STR_BUF}"));
                self.level += 1;
        self.scope += 1;
                self.ln("DISPLAY \"cuni: string too long; refusing\".");
                self.ln("STOP RUN RETURNING 1.");
                self.level -= 1;
        self.scope -= 1;
                self.ln("END-IF.");
                self.ln(&format!(
                    "STRING {lb} DELIMITED BY SIZE {rb} DELIMITED BY SIZE INTO {tb}."
                ));
                self.ln(&format!("COMPUTE {tl} = {ll} + {rl}."));
                Ok(StrPlace::Pair(tb, tl))
            }
            ExprKind::Call { callee, args } => {
                let (name, params, ret) = self.call_head(callee)?;
                if ret != Ty::Str {
                    return Err(format!(
                        "call to `{name}` used as str but returns otherwise; refusing"
                    ));
                }
                self.call_args(&name, &params, args)?;
                self.ln(&format!("PERFORM F-{0} THRU F-{0}-EXIT.", cob_name(&name)));
                let (tb, tl) = self.fresh_str()?;
                let cf = cob_name(&name);
                self.ln(&format!("MOVE CUNI-R-{cf} TO {tb}."));
                self.ln(&format!("MOVE CUNI-R-{cf}-LEN TO {tl}."));
                Ok(StrPlace::Pair(tb, tl))
            }
            _ => Err("non-str expression in str position: beyond core subset; refusing".into()),
        }
    }

    /// A string operand as (sending-item, length-item) for STRING/DISPLAY.
    /// Literals go inline; pairs use a (1:len) reference modification.
    /// A reference modification cannot follow a qualified name
    /// (`F OF FRAME(SP)(1:N)` is a syntax error), so frame-local pairs are
    /// bounced through a temp pair first.
    fn str_operand(&mut self, sp: StrPlace) -> Result<(String, String), String> {
        match sp {
            StrPlace::Lit(text, len) => {
                if len == 0 {
                    // A zero-length literal (X"") would DISPLAY as a NUL
                    // byte; bounce through a temp pair instead.
                    let (tb, tl) = self.fresh_str()?;
                    self.ln(&format!("MOVE 0 TO {tl}."));
                    Ok((format!("{tb}(1:{tl})"), tl))
                } else {
                    Ok((text, len.to_string()))
                }
            }
            StrPlace::Pair(b, l) => {
                if b.contains(" OF ") {
                    let (tb, tl) = self.fresh_str()?;
                    self.ln(&format!("MOVE {b} TO {tb}."));
                    self.ln(&format!("MOVE {l} TO {tl}."));
                    Ok((format!("{tb}(1:{tl})"), tl))
                } else {
                    Ok((format!("{b}(1:{l})"), l))
                }
            }
        }
    }

    /// Materialize any StrPlace into a (buffer, length) variable pair.
    fn str_to_pair(&mut self, sp: StrPlace) -> Result<(String, String), String> {
        match sp {
            StrPlace::Pair(b, l) => Ok((b, l)),
            StrPlace::Lit(text, len) => {
                let (tb, tl) = self.fresh_str()?;
                if len > 0 {
                    self.ln(&format!("MOVE {text} TO {tb}."));
                }
                self.ln(&format!("MOVE {len} TO {tl}."));
                Ok((tb, tl))
            }
        }
    }

    /// Boolean expression evaluated into `out` (a `PIC 9` item holding 0/1).
    /// `and`/`or` short-circuit exactly like the interpreter.
    fn gen_bool(&mut self, e: &Expr, out: &str) -> Result<(), String> {
        match &e.kind {
            ExprKind::Bool(b) => {
                self.ln(&format!("MOVE {} TO {out}.", if *b { "1" } else { "0" }));
                Ok(())
            }
            ExprKind::Ident(n) => match self.place_of(n)? {
                Place::Bool(p) => {
                    self.ln(&format!("MOVE {p} TO {out}."));
                    Ok(())
                }
                _ => Err(format!("variable `{n}` is not a bool: beyond core subset; refusing")),
            },
            ExprKind::Binary { op, lhs, rhs } => match op {
                BinOp::And => {
                    let t1 = self.fresh_bool()?;
                    self.gen_bool(lhs, &t1)?;
                    self.ln(&format!("IF {t1} = 1"));
                    self.level += 1;
        self.scope += 1;
                    let t2 = self.fresh_bool()?;
                    self.gen_bool(rhs, &t2)?;
                    self.bool_move(&t2, out)?;
                    self.level -= 1;
        self.scope -= 1;
                    self.ln("ELSE");
                    self.level += 1;
        self.scope += 1;
                    self.ln(&format!("MOVE 0 TO {out}."));
                    self.level -= 1;
        self.scope -= 1;
                    self.ln("END-IF.");
                    Ok(())
                }
                BinOp::Or => {
                    let t1 = self.fresh_bool()?;
                    self.gen_bool(lhs, &t1)?;
                    self.ln(&format!("IF {t1} = 1"));
                    self.level += 1;
        self.scope += 1;
                    self.ln(&format!("MOVE 1 TO {out}."));
                    self.level -= 1;
        self.scope -= 1;
                    self.ln("ELSE");
                    self.level += 1;
        self.scope += 1;
                    let t2 = self.fresh_bool()?;
                    self.gen_bool(rhs, &t2)?;
                    self.bool_move(&t2, out)?;
                    self.level -= 1;
        self.scope -= 1;
                    self.ln("END-IF.");
                    Ok(())
                }
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                    self.gen_compare(op, lhs, rhs, out)
                }
                _ => Err("non-bool operator in bool position: beyond core subset; refusing".into()),
            },
            ExprKind::Unary { op: UnOp::Not, expr } => {
                let t = self.fresh_bool()?;
                self.gen_bool(expr, &t)?;
                self.ln(&format!("COMPUTE {out} = 1 - {t}."));
                Ok(())
            }
            ExprKind::Unary { op: UnOp::Neg, .. } => {
                Err("unary `-` on non-int: beyond core subset; refusing".into())
            }
            ExprKind::Call { callee, args } => {
                let (name, params, ret) = self.call_head(callee)?;
                if ret != Ty::Bool {
                    return Err(format!(
                        "call to `{name}` used as bool but returns otherwise; refusing"
                    ));
                }
                self.call_args(&name, &params, args)?;
                self.ln(&format!("PERFORM F-{0} THRU F-{0}-EXIT.", cob_name(&name)));
                self.ln(&format!("MOVE CUNI-R-{} TO {out}.", cob_name(&name)));
                Ok(())
            }
            _ => Err("non-bool expression in bool position: beyond core subset; refusing".into()),
        }
    }

    /// `MOVE <src> TO <out>.` for a 0/1 bool temp — trivial, but keeps the
    /// short-circuit branches readable.
    fn bool_move(&mut self, src: &str, out: &str) -> Result<(), String> {
        self.ln(&format!("IF {src} = 1"));
        self.level += 1;
        self.scope += 1;
        self.ln(&format!("MOVE 1 TO {out}."));
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln(&format!("MOVE 0 TO {out}."));
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        Ok(())
    }

    /// Comparison of two same-typed operands into `out` (0/1).
    fn gen_compare(&mut self, op: &BinOp, lhs: &Expr, rhs: &Expr, out: &str) -> Result<(), String> {
        let t = self.infer(lhs)?;
        debug_assert_eq!(t, self.infer(rhs)?);
        match t {
            Ty::Int => {
                let l = self.int_item(lhs)?;
                let r = self.int_item(rhs)?;
                let o = match op {
                    BinOp::Eq => "=",
                    BinOp::Ne => "NOT =",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    BinOp::Ge => ">=",
                    _ => unreachable!(),
                };
                self.ln(&format!("IF {l} {o} {r}"));
                self.level += 1;
        self.scope += 1;
                self.ln(&format!("MOVE 1 TO {out}."));
                self.level -= 1;
        self.scope -= 1;
                self.ln("ELSE");
                self.level += 1;
        self.scope += 1;
                self.ln(&format!("MOVE 0 TO {out}."));
                self.level -= 1;
        self.scope -= 1;
                self.ln("END-IF.");
                Ok(())
            }
            Ty::Bool => {
                let t1 = self.fresh_bool()?;
                self.gen_bool(lhs, &t1)?;
                let t2 = self.fresh_bool()?;
                self.gen_bool(rhs, &t2)?;
                let o = match op {
                    BinOp::Eq => "=",
                    BinOp::Ne => "NOT =",
                    _ => {
                        return Err(format!(
                            "`{}` on bools: beyond core subset; refusing",
                            op_name(op)
                        ))
                    }
                };
                self.ln(&format!("IF {t1} {o} {t2}"));
                self.level += 1;
        self.scope += 1;
                self.ln(&format!("MOVE 1 TO {out}."));
                self.level -= 1;
        self.scope -= 1;
                self.ln("ELSE");
                self.level += 1;
        self.scope += 1;
                self.ln(&format!("MOVE 0 TO {out}."));
                self.level -= 1;
        self.scope -= 1;
                self.ln("END-IF.");
                Ok(())
            }
            Ty::Str => {
                let l = self.str_expr(lhs)?;
                let r = self.str_expr(rhs)?;
                let (lb, ll) = self.str_to_pair(l)?;
                let (rb, rl) = self.str_to_pair(r)?;
                self.strcmp(&lb, &ll, &rb, &rl)?;
                // CUNI-CCMP: -1 / 0 / 1
                let o = match op {
                    BinOp::Eq => "= 0",
                    BinOp::Ne => "NOT = 0",
                    BinOp::Lt => "< 0",
                    BinOp::Gt => "> 0",
                    BinOp::Le => "<= 0",
                    BinOp::Ge => ">= 0",
                    _ => unreachable!(),
                };
                self.ln(&format!("IF CUNI-CCMP {o}"));
                self.level += 1;
        self.scope += 1;
                self.ln(&format!("MOVE 1 TO {out}."));
                self.level -= 1;
        self.scope -= 1;
                self.ln("ELSE");
                self.level += 1;
        self.scope += 1;
                self.ln(&format!("MOVE 0 TO {out}."));
                self.level -= 1;
        self.scope -= 1;
                self.ln("END-IF.");
                Ok(())
            }
        }
    }

    /// Length-aware string three-way compare into CUNI-CCMP (-1/0/1).
    /// COBOL pads the shorter operand with spaces, which misorders when the
    /// longer string's next byte is below 0x20 — so compare the common
    /// prefix first, then let the longer string win.
    fn strcmp(&mut self, lb: &str, ll: &str, rb: &str, rl: &str) -> Result<(), String> {
        self.ln(&format!("MOVE {lb} TO CUNI-CA."));
        self.ln(&format!("MOVE {ll} TO CUNI-CA-LEN."));
        self.ln(&format!("MOVE {rb} TO CUNI-CB."));
        self.ln(&format!("MOVE {rl} TO CUNI-CB-LEN."));
        self.ln("IF CUNI-CA-LEN = CUNI-CB-LEN");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA-LEN = 0");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE 0 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA(1:CUNI-CA-LEN) = CUNI-CB(1:CUNI-CB-LEN)");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE 0 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA(1:CUNI-CA-LEN) < CUNI-CB(1:CUNI-CB-LEN)");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE -1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE 1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA-LEN < CUNI-CB-LEN");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE CUNI-CA-LEN TO CUNI-CM.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE CUNI-CB-LEN TO CUNI-CM.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.ln("IF CUNI-CM = 0");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA-LEN < CUNI-CB-LEN");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE -1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE 1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA(1:CUNI-CM) = CUNI-CB(1:CUNI-CM)");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA-LEN < CUNI-CB-LEN");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE -1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE 1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("IF CUNI-CA(1:CUNI-CM) < CUNI-CB(1:CUNI-CM)");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE -1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("ELSE");
        self.level += 1;
        self.scope += 1;
        self.ln("MOVE 1 TO CUNI-CCMP.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        Ok(())
    }
}

fn op_name(op: &BinOp) -> &'static str {
    match op {
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
    }
}

impl Emitter {
    /// Store an int expression into `out` (a data item).
    fn store_int(&mut self, e: &Expr, out: &str) -> Result<(), String> {
        match self.int_expr(e)? {
            IPlace::Simple(s) => self.ln(&format!("MOVE {s} TO {out}.")),
            IPlace::Expr(x) => self.ln(&format!("COMPUTE {out} = {x}.")),
        }
        Ok(())
    }

    /// Store a string expression into the (buffer, length) pair.
    fn store_str(&mut self, e: &Expr, ob: &str, ol: &str) -> Result<(), String> {
        let sp = self.str_expr(e)?;
        let (b, l) = self.str_to_pair(sp)?;
        self.ln(&format!("MOVE {b} TO {ob}."));
        self.ln(&format!("MOVE {l} TO {ol}."));
        Ok(())
    }

    fn check_ann(&self, name: &str, ty: &Option<Type>, vt: Ty, kw: &str) -> Result<(), String> {
        if let Some(t) = ty {
            let at = ty_of(t)?;
            if at != vt {
                return Err(format!(
                    "{kw} {name}: annotated type disagrees with value type; refusing"
                ));
            }
        }
        Ok(())
    }

    /// `say` of one value.
    fn gen_say(&mut self, e: &Expr) -> Result<(), String> {
        match self.infer(e)? {
            Ty::Int => {
                let item = self.int_item(e)?;
                self.ln(&format!("MOVE {item} TO CUNI-INT-EDIT."));
                self.ln("DISPLAY FUNCTION TRIM(CUNI-INT-EDIT LEADING).");
                Ok(())
            }
            Ty::Str => {
                let sp = self.str_expr(e)?;
                match sp {
                    StrPlace::Lit(text, len) if len > 0 => {
                        self.ln(&format!("DISPLAY {text}."));
                        Ok(())
                    }
                    _ => {
                        let (sender, _) = self.str_operand(sp)?;
                        self.ln(&format!("DISPLAY {sender}."));
                        Ok(())
                    }
                }
            }
            Ty::Bool => {
                let t = self.fresh_bool()?;
                self.gen_bool(e, &t)?;
                self.ln(&format!("IF {t} = 1"));
                self.level += 1;
        self.scope += 1;
                self.ln("DISPLAY \"True\".");
                self.level -= 1;
        self.scope -= 1;
                self.ln("ELSE");
                self.level += 1;
        self.scope += 1;
                self.ln("DISPLAY \"False\".");
                self.level -= 1;
        self.scope -= 1;
                self.ln("END-IF.");
                Ok(())
            }
        }
    }

    fn gen_stmt(&mut self, s: &Stmt) -> Result<(), String> {
        match &s.kind {
            StmtKind::Let { name, ty, value } => {
                let vt = self.infer(value)?;
                self.check_ann(name, ty, vt, "let")?;
                let place = self.bind(name, vt)?;
                match place {
                    Place::Int(p) => self.store_int(value, &p)?,
                    Place::Bool(p) => self.gen_bool(value, &p)?,
                    Place::Str(b, l) => self.store_str(value, &b, &l)?,
                }
                Ok(())
            }
            StmtKind::Mut { name, ty, value } => {
                let vt = self.infer(value)?;
                self.check_ann(name, ty, vt, "mut")?;
                let place = self.bind(name, vt)?;
                match place {
                    Place::Int(p) => self.store_int(value, &p)?,
                    Place::Bool(p) => self.gen_bool(value, &p)?,
                    Place::Str(b, l) => self.store_str(value, &b, &l)?,
                }
                Ok(())
            }
            StmtKind::Assign { target, value } => {
                let tname = match &target.kind {
                    ExprKind::Ident(n) => n.clone(),
                    _ => {
                        return Err(
                            "complex assignment target: beyond core subset; refusing".into()
                        )
                    }
                };
                let place = self.place_of(&tname)?;
                let vt = self.infer(value)?;
                if place.ty() != vt {
                    return Err(format!("assignment to `{tname}`: type mismatch; refusing"));
                }
                match place {
                    Place::Int(p) => self.store_int(value, &p)?,
                    Place::Bool(p) => self.gen_bool(value, &p)?,
                    Place::Str(b, l) => self.store_str(value, &b, &l)?,
                }
                Ok(())
            }
            StmtKind::Ret(e) => {
                let f = self.cur_fn.clone().ok_or_else(|| {
                    "top-level ret: no return slot outside a def; refusing".to_string()
                })?;
                let x = e
                    .as_ref()
                    .ok_or_else(|| "bare ret: beyond core subset; refusing".to_string())?;
                let cf = cob_name(&f);
                let rt = self.infer(x)?;
                match rt {
                    Ty::Int => self.store_int(x, &format!("CUNI-R-{cf}"))?,
                    Ty::Bool => self.gen_bool(x, &format!("CUNI-R-{cf}"))?,
                    Ty::Str => self.store_str(
                        x,
                        &format!("CUNI-R-{cf}"),
                        &format!("CUNI-R-{cf}-LEN"),
                    )?,
                }
                // Pop the frame and leave the paragraph; a `ret` inside a
                // branch must not fall through.
                self.ln(&format!("GO TO F-{cf}-EXIT."));
                Ok(())
            }
            StmtKind::Fail(_) => Err("fail: beyond core subset; refusing".into()),
            StmtKind::If { cond, then_body, else_body } => {
                if self.infer(cond)? != Ty::Bool {
                    return Err("if condition must be bool; refusing".into());
                }
                let t = self.fresh_bool()?;
                self.gen_bool(cond, &t)?;
                self.ln(&format!("IF {t} = 1"));
                self.level += 1;
        self.scope += 1;
                for s in then_body {
                    self.gen_stmt(s)?;
                }
                self.level -= 1;
        self.scope -= 1;
                if let Some(eb) = else_body {
                    self.ln("ELSE");
                    self.level += 1;
        self.scope += 1;
                    for s in eb {
                        self.gen_stmt(s)?;
                    }
                    self.level -= 1;
        self.scope -= 1;
                }
                self.ln("END-IF.");
                Ok(())
            }
            StmtKind::For { .. } => Err("for: beyond core subset; refusing".into()),
            StmtKind::Whl { cond, body } => {
                if self.infer(cond)? != Ty::Bool {
                    return Err("while condition must be bool; refusing".into());
                }
                let t = self.fresh_bool()?;
                // The condition is re-evaluated before every iteration
                // (conditions are pure, so re-evaluation is exact).
                self.gen_bool(cond, &t)?;
                self.ln(&format!("PERFORM UNTIL {t} = 0"));
                self.level += 1;
        self.scope += 1;
                for s in body {
                    self.gen_stmt(s)?;
                }
                self.gen_bool(cond, &t)?;
                self.level -= 1;
        self.scope -= 1;
                self.ln("END-PERFORM.");
                Ok(())
            }
            StmtKind::ExprStmt(e) => match &e.kind {
                ExprKind::Call { callee, args } => {
                    if let ExprKind::Ident(n) = &callee.kind {
                        if n == "say" {
                            if args.len() != 1 {
                                return Err(
                                    "say: exactly one argument in core subset; refusing".into()
                                );
                            }
                            if args[0].is_named() {
                                return Err(
                                    "named arguments: beyond core subset; refusing".into()
                                );
                            }
                            return self.gen_say(args[0].expr());
                        }
                    }
                    Err("bare call (non-say): beyond core subset; refusing".into())
                }
                _ => Err("bare expression: beyond core subset; refusing".into()),
            },
            StmtKind::Todo => Err("...: beyond core subset; refusing".into()),
        }
    }

    /// One def as a paragraph pair. Locals live in the call frame; entry
    /// pushes, `ret`/fall-through pops via the shared exit paragraph.
    fn gen_def(&mut self, f: &FnDecl) -> Result<String, String> {
        let cf = cob_name(&f.name);
        let (params, _) = self
            .sigs
            .get(&f.name)
            .cloned()
            .ok_or_else(|| format!("internal: missing signature for `{}`", f.name))?;
        self.cur_fn = Some(f.name.clone());
        let saved = std::mem::take(&mut self.stmts);
        self.level = 0;
        self.ln(&format!("F-{cf}."));
        self.level = 1;
        self.ln("ADD 1 TO CUNI-SP.");
        self.ln(&format!("IF CUNI-SP > {MAX_FRAMES}"));
        self.level += 1;
        self.scope += 1;
        self.ln("DISPLAY \"cuni: call stack overflow; refusing\".");
        self.ln("STOP RUN RETURNING 1.");
        self.level -= 1;
        self.scope -= 1;
        self.ln("END-IF.");
        for (pn, pt) in &params {
            let pg = format!("CUNI-P-{cf}-{}", cob_name(pn));
            let dest = self.place_of(pn)?;
            match (pt, dest) {
                (Ty::Int, Place::Int(d)) => self.ln(&format!("MOVE {pg} TO {d}.")),
                (Ty::Bool, Place::Bool(d)) => self.ln(&format!("MOVE {pg} TO {d}.")),
                (Ty::Str, Place::Str(b, l)) => {
                    self.ln(&format!("MOVE {pg} TO {b}."));
                    self.ln(&format!("MOVE {pg}-LEN TO {l}."));
                }
                _ => return Err(format!("internal: param `{pn}` type mismatch")),
            }
        }
        for s in &f.body {
            self.gen_stmt(s)?;
        }
        self.ln(&format!("GO TO F-{cf}-EXIT."));
        self.level = 0;
        self.ln(&format!("F-{cf}-EXIT."));
        self.level = 1;
        self.ln("SUBTRACT 1 FROM CUNI-SP.");
        self.ln("EXIT PARAGRAPH.");
        let body = std::mem::replace(&mut self.stmts, saved);
        self.level = 0;
        self.cur_fn = None;
        Ok(body.join("\n") + "\n")
    }

    fn pic_of(ty: Ty) -> &'static str {
        match ty {
            Ty::Int => "PIC S9(19)",
            Ty::Bool => "PIC 9",
            Ty::Str => "PIC X(4096)",
        }
    }

    fn generate_program(&mut self, program: &Program) -> Result<String, String> {
        // First pass: def signatures + refuse non-core items.
        let mut script: Vec<&Stmt> = Vec::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    self.check_def(f)?;
                    let mut params = Vec::new();
                    for p in &f.params {
                        params.push((p.name.clone(), ty_of(&p.ty)?));
                    }
                    let ret = ty_of(&f.ret_type)?;
                    self.sigs.insert(f.name.clone(), (params, ret));
                    self.order.push(f.name.clone());
                    // Pre-bind params so the frame layout is declaration
                    // order: params first, then body bindings.
                    self.cur_fn = Some(f.name.clone());
                    for p in &f.params {
                        let pt = ty_of(&p.ty)?;
                        self.bind(&p.name, pt)?;
                    }
                    self.cur_fn = None;
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
        // Top-level statements (binds globals as a side effect).
        self.cur_fn = None;
        self.level = 1;
        for s in script {
            self.gen_stmt(s)?;
        }
        let main_body = std::mem::take(&mut self.stmts);
        // Def bodies (binds frame locals as a side effect).
        let mut defs: Vec<String> = Vec::new();
        for name in self.order.clone() {
            let f = program
                .items
                .iter()
                .find_map(|it| match it {
                    Item::Def(f) if f.name == name => Some(f),
                    _ => None,
                })
                .expect("def present");
            defs.push(self.gen_def(f)?);
        }

        // ---- assembly ----
        let mut o = String::new();
        o.push_str(">>SOURCE FORMAT IS FREE\n");
        o.push_str("IDENTIFICATION DIVISION.\n");
        o.push_str("PROGRAM-ID. CUNI-MAIN.\n");
        o.push_str("DATA DIVISION.\n");
        o.push_str("WORKING-STORAGE SECTION.\n");
        o.push_str(&format!(
            "01 CUNI-INT-EDIT PIC {}9.\n",
            "-".repeat(18)
        ));
        for i in 1..=self.n_int {
            o.push_str(&format!("01 CUNI-T{i} PIC S9(19).\n"));
        }
        for i in 1..=self.n_bool {
            o.push_str(&format!("01 CUNI-B{i} PIC 9.\n"));
        }
        for i in 1..=self.n_str {
            o.push_str(&format!("01 CUNI-S{i} PIC X({STR_BUF}).\n"));
            o.push_str(&format!("01 CUNI-S{i}-LEN PIC 9(9) VALUE 0.\n"));
        }
        o.push_str("01 CUNI-DA PIC S9(19).\n");
        o.push_str("01 CUNI-DB PIC S9(19).\n");
        o.push_str("01 CUNI-DQ PIC S9(19).\n");
        o.push_str("01 CUNI-MA PIC S9(19).\n");
        o.push_str("01 CUNI-MB PIC S9(19).\n");
        o.push_str("01 CUNI-MQ PIC S9(19).\n");
        o.push_str("01 CUNI-MR PIC S9(19).\n");
        o.push_str(&format!("01 CUNI-CA PIC X({STR_BUF}).\n"));
        o.push_str("01 CUNI-CA-LEN PIC 9(9).\n");
        o.push_str(&format!("01 CUNI-CB PIC X({STR_BUF}).\n"));
        o.push_str("01 CUNI-CB-LEN PIC 9(9).\n");
        o.push_str("01 CUNI-CM PIC 9(9).\n");
        o.push_str("01 CUNI-CCMP PIC S9(9).\n");
        o.push_str("01 CUNI-SP PIC 9(9) VALUE 0.\n");
        o.push_str("01 CUNI-FRAMES.\n");
        o.push_str(&format!("    05 CUNI-FRAME OCCURS {MAX_FRAMES} TIMES.\n"));
        o.push_str("        10 FF-PAD PIC X.\n");
        for f in &self.order.clone() {
            let cf = cob_name(f);
            if let Some(names) = self.flocal_order.get(f).cloned() {
                let tys: HashMap<String, Ty> =
                    self.flocals.get(f).cloned().unwrap_or_default();
                for n in &names {
                    let ty = tys[n];
                    let cn = cob_name(n);
                    o.push_str(&format!("        10 FF-{cf}-{cn} {}.", Self::pic_of(ty)));
                    o.push('\n');
                    if ty == Ty::Str {
                        o.push_str(&format!("        10 FF-{cf}-{cn}-LEN PIC 9(9).\n"));
                    }
                }
            }
        }
        for n in &self.global_order.clone() {
            let ty = self.globals[n];
            let cn = cob_name(n);
            o.push_str(&format!("01 V-{cn} {}.", Self::pic_of(ty)));
            o.push('\n');
            if ty == Ty::Str {
                o.push_str(&format!("01 V-{cn}-LEN PIC 9(9) VALUE 0.\n"));
            }
        }
        for f in &self.order.clone() {
            let cf = cob_name(f);
            let (params, ret) = self.sigs[f].clone();
            for (pn, pt) in &params {
                let cn = cob_name(pn);
                o.push_str(&format!("01 CUNI-P-{cf}-{cn} {}.", Self::pic_of(*pt)));
                o.push('\n');
                if *pt == Ty::Str {
                    o.push_str(&format!("01 CUNI-P-{cf}-{cn}-LEN PIC 9(9).\n"));
                }
            }
            o.push_str(&format!("01 CUNI-R-{cf} {}.", Self::pic_of(ret)));
            o.push('\n');
            if ret == Ty::Str {
                o.push_str(&format!("01 CUNI-R-{cf}-LEN PIC 9(9).\n"));
            }
        }
        o.push_str("PROCEDURE DIVISION.\n");
        o.push_str("MAIN-PARA.\n");
        for line in &main_body {
            o.push_str(line);
            o.push('\n');
        }
        o.push_str("    STOP RUN.\n");
        for d in &defs {
            o.push_str(d);
        }
        Ok(o)
    }
}

pub fn generate(program: &Program) -> Result<String, String> {
    Emitter::new().generate_program(program)
}

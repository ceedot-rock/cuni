/// Byte offset range into the originating source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    pub fn union(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    pub fn dummy() -> Self {
        Span { start: 0, end: 0 }
    }
}

#[derive(Debug)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Use(UseDecl),
    Ext(ExtDecl),
    Typ(TypDecl),
    Iface(IfaceDecl),
    Enum(EnumDecl),
    Def(FnDecl),
    Stmt(Stmt),
}

/// `use name` — module import (SPEC.md §9).
#[derive(Debug)]
pub struct UseDecl {
    pub name: String,
    pub name_span: Span,
}

/// One payload-free enum variant name with its source span.
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub name_span: Span,
}

/// Payload-free enum: a closed set of named variants, no attached data.
#[derive(Debug)]
pub struct EnumDecl {
    pub name: String,
    pub name_span: Span,
    pub variants: Vec<EnumVariant>,
}

/// A non-portable, per-target binding: `ext name(...) -> T do py: ... go: ... end`.
#[derive(Debug)]
pub struct ExtDecl {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub ret_type: Type,
    pub targets: Vec<(String, String)>,
}

#[derive(Debug)]
pub struct TypDecl {
    pub name: String,
    pub name_span: Span,
    pub implements: Option<String>,
    pub fields: Vec<Param>,
}

#[derive(Debug)]
pub struct IfaceDecl {
    pub name: String,
    pub name_span: Span,
    pub methods: Vec<MethodSig>,
}

#[derive(Debug)]
pub struct MethodSig {
    pub name: String,
    pub name_span: Span,
    pub params: Vec<Param>,
    pub ret_type: Type,
}

#[derive(Debug)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    /// Span covering `name: type` (best-effort; starts at name).
    pub span: Span,
}

#[derive(Debug)]
pub struct FnDecl {
    pub name: String,
    pub name_span: Span,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub ret_type: Type,
    pub fallible: bool,
    pub body: Vec<Stmt>,
    pub is_link: bool,
}

#[derive(Debug, Clone)]
pub enum Type {
    Named(String),
    Generic(String, Vec<Type>), // list<T>, map<K,V>, opt<T>
}

#[derive(Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub span: Span,
}

#[derive(Debug)]
pub enum StmtKind {
    Let {
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    Mut {
        name: String,
        ty: Option<Type>,
        value: Expr,
    },
    Assign {
        target: Expr,
        value: Expr,
    },
    Ret(Option<Expr>),
    /// `fail expr` — signals failure from a fallible (`-> T ?`) function.
    Fail(Expr),
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    For {
        binding: (String, Option<String>),
        iter: Expr,
        body: Vec<Stmt>,
    },
    Whl {
        cond: Expr,
        body: Vec<Stmt>,
    },
    ExprStmt(Expr),
    /// The literal `...` placeholder used as a stand-in function body.
    Todo,
}

#[derive(Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub span: Span,
}

/// Positional or named call argument. Named args are for typ constructors
/// (`Circle(r: 2.0)`); function calls stay positional in v0.1.x.
#[derive(Debug)]
pub enum CallArg {
    Pos(Expr),
    Named {
        name: String,
        name_span: Span,
        value: Expr,
    },
}

impl CallArg {
    pub fn expr(&self) -> &Expr {
        match self {
            CallArg::Pos(e) | CallArg::Named { value: e, .. } => e,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            CallArg::Pos(e) => e.span,
            CallArg::Named {
                name_span, value, ..
            } => name_span.union(value.span),
        }
    }

    pub fn is_named(&self) -> bool {
        matches!(self, CallArg::Named { .. })
    }
}

#[derive(Debug)]
pub enum ExprKind {
    Int(i64),
    Float(f64),
    /// A `dec` literal, stored already scaled by 10⁴ (docs/DECIMAL.md §1–2).
    /// The parser validates and scales once via `parse_dec_scaled`, so every
    /// seat and the interpreter share one literal semantics.
    Dec(i128),
    Bool(bool),
    Str(String),
    InterpStr(Vec<StrPartExpr>),
    NoneLit,
    Ident(String),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Field {
        base: Box<Expr>,
        name: String,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    /// `expr ?? do ... end`
    Unwrap {
        expr: Box<Expr>,
        handler: Vec<Stmt>,
    },
}

#[derive(Debug)]
pub enum StrPartExpr {
    Text(String),
    Expr(Expr),
}

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy)]
pub enum UnOp {
    Not,
    Neg,
}

/// `dec` fixed scale: 10⁴ (docs/DECIMAL.md §1).
pub const DEC_SCALE: i128 = 10_000;
/// Maximum fractional digits a `dec` literal may carry (§2).
pub const DEC_FRAC_DIGITS: usize = 4;

/// Parse a `dec` literal's raw decimal text (e.g. `"19.99"`, `"100"`) into
/// its scaled i128 value. This is the ONE place literal semantics live:
/// the parser calls it, and codegens may reuse it. Errors are refusal
/// messages, never silent rounding.
pub fn parse_dec_scaled(text: &str) -> Result<i128, String> {
    let (int_part, frac_part) = match text.split_once('.') {
        Some((i, f)) => (i, f),
        None => (text, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        return Err(format!("invalid dec literal `{text}dec`"));
    }
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        return Err(format!("invalid dec literal `{text}dec`"));
    }
    if frac_part.len() > DEC_FRAC_DIGITS {
        return Err(format!(
            "dec literal `{text}dec` has {} fractional digits; scale is {} — refusing (no silent rounding)",
            frac_part.len(),
            DEC_FRAC_DIGITS,
        ));
    }
    let int_val: i128 = if int_part.is_empty() {
        0
    } else {
        int_part.parse::<i128>().map_err(|_| {
            format!("dec literal `{text}dec` is out of range — refusing")
        })?
    };
    let mut frac_val: i128 = if frac_part.is_empty() {
        0
    } else {
        frac_part.parse::<i128>().map_err(|_| {
            format!("dec literal `{text}dec` is out of range — refusing")
        })?
    };
    for _ in frac_part.len()..DEC_FRAC_DIGITS {
        frac_val = frac_val.checked_mul(10).ok_or_else(|| {
            format!("dec literal `{text}dec` is out of range — refusing")
        })?;
    }
    let scaled_int = int_val.checked_mul(DEC_SCALE).ok_or_else(|| {
        format!("dec literal `{text}dec` is out of range — refusing")
    })?;
    scaled_int.checked_add(frac_val).ok_or_else(|| {
        format!("dec literal `{text}dec` is out of range — refusing")
    })
}

/// Canonical `dec` rendering of a scaled i128 (docs/DECIMAL.md §6):
/// sign + integer digits + "." + fractional digits with trailing zeros
/// stripped (never empty). Byte-identical on every seat; seats with their
/// own integer widths reimplement this exact algorithm.
pub fn fmt_dec_scaled(scaled: i128) -> String {
    let neg = scaled < 0;
    // abs() on i128::MIN would overflow; work in unsigned space.
    let mag: u128 = if neg {
        (scaled as u128).wrapping_neg()
    } else {
        scaled as u128
    };
    let int_part = mag / (DEC_SCALE as u128);
    let mut frac = format!("{:04}", mag % (DEC_SCALE as u128));
    while frac.ends_with('0') {
        frac.pop();
    }
    if frac.is_empty() {
        frac.push('0');
    }
    format!("{}{}.{}", if neg { "-" } else { "" }, int_part, frac)
}

/// All `dec` literal scaled values in a program, for per-seat range
/// refusal pre-passes (docs/DECIMAL.md §7). Narrow int64 seats refuse
/// literals whose scaled value doesn't fit; wide seats don't call this.
pub fn check_dec_literals_in_range(
    program: &Program,
    max_abs: i128,
    seat: &str,
) -> Result<(), String> {
    let mut bad: Option<i128> = None;
    fn expr(e: &Expr, bad: &mut Option<i128>, max_abs: i128) {
        if bad.is_some() {
            return;
        }
        match &e.kind {
            ExprKind::Dec(s) => {
                if *s > max_abs || *s < -max_abs {
                    *bad = Some(*s);
                }
            }
            ExprKind::List(xs) => xs.iter().for_each(|x| expr(x, bad, max_abs)),
            ExprKind::Map(pairs) => pairs.iter().for_each(|(k, v)| {
                expr(k, bad, max_abs);
                expr(v, bad, max_abs);
            }),
            ExprKind::Call { callee, args } => {
                expr(callee, bad, max_abs);
                args.iter()
                    .for_each(|a| expr(a.expr(), bad, max_abs));
            }
            ExprKind::Index { base, index } => {
                expr(base, bad, max_abs);
                expr(index, bad, max_abs);
            }
            ExprKind::Field { base, .. } => expr(base, bad, max_abs),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, bad, max_abs);
                expr(rhs, bad, max_abs);
            }
            ExprKind::Unary { expr: x, .. } => expr(x, bad, max_abs),
            ExprKind::Unwrap { expr: x, handler } => {
                expr(x, bad, max_abs);
                handler.iter().for_each(|s| stmt(s, bad, max_abs));
            }
            ExprKind::InterpStr(parts) => parts.iter().for_each(|p| {
                if let StrPartExpr::Expr(x) = p {
                    expr(x, bad, max_abs);
                }
            }),
            _ => {}
        }
    }
    fn stmt(s: &Stmt, bad: &mut Option<i128>, max_abs: i128) {
        if bad.is_some() {
            return;
        }
        match &s.kind {
            StmtKind::Let { value, .. } | StmtKind::Mut { value, .. } => {
                expr(value, bad, max_abs)
            }
            StmtKind::Assign { target, value } => {
                expr(target, bad, max_abs);
                expr(value, bad, max_abs);
            }
            StmtKind::Ret(Some(x)) | StmtKind::Fail(x) | StmtKind::ExprStmt(x) => {
                expr(x, bad, max_abs)
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                expr(cond, bad, max_abs);
                then_body.iter().for_each(|x| stmt(x, bad, max_abs));
                if let Some(eb) = else_body {
                    eb.iter().for_each(|x| stmt(x, bad, max_abs));
                }
            }
            StmtKind::For { iter, body, .. } => {
                expr(iter, bad, max_abs);
                body.iter().for_each(|x| stmt(x, bad, max_abs));
            }
            StmtKind::Whl { cond, body } => {
                expr(cond, bad, max_abs);
                body.iter().for_each(|x| stmt(x, bad, max_abs));
            }
            _ => {}
        }
    }
    for item in &program.items {
        match item {
            Item::Def(f) => f.body.iter().for_each(|s| stmt(s, &mut bad, max_abs)),
            Item::Stmt(s) => stmt(s, &mut bad, max_abs),
            _ => {}
        }
        if bad.is_some() {
            break;
        }
    }
    match bad {
        Some(v) => Err(format!(
            "{seat} seat: dec literal with scaled value {v} is outside the int64 range — refusing (docs/DECIMAL.md §7)"
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dec_literal_scales_exactly() {
        assert_eq!(parse_dec_scaled("19.99").unwrap(), 199900);
        assert_eq!(parse_dec_scaled("100").unwrap(), 1_000_000);
        assert_eq!(parse_dec_scaled("0.0001").unwrap(), 1);
        assert_eq!(parse_dec_scaled("1.50").unwrap(), 15000);
        assert_eq!(parse_dec_scaled("0").unwrap(), 0);
    }

    #[test]
    fn dec_literal_refuses_extra_precision() {
        assert!(parse_dec_scaled("1.23456").is_err());
        assert!(parse_dec_scaled("0.00001").is_err());
    }

    #[test]
    fn dec_literal_refuses_out_of_range() {
        assert!(parse_dec_scaled("170141183460469231731687303715884373758").is_err());
    }

    #[test]
    fn dec_formats_canonically() {
        assert_eq!(fmt_dec_scaled(199900), "19.99");
        assert_eq!(fmt_dec_scaled(1_000_000), "100.0");
        assert_eq!(fmt_dec_scaled(1), "0.0001");
        assert_eq!(fmt_dec_scaled(-5), "-0.0005");
        assert_eq!(fmt_dec_scaled(0), "0.0");
        assert_eq!(fmt_dec_scaled(12300), "1.23");
        assert_eq!(fmt_dec_scaled(-12300), "-1.23");
    }
}

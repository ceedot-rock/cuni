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
    /// A `time` literal, stored as int64 unix epoch seconds, UTC
    /// (docs/TIME.md §1–2). The parser validates the strict ISO-8601 form
    /// and converts once via `parse_time_epoch`, so every seat and the
    /// interpreter share one literal semantics. No timezones, no DST, no
    /// wall-clock `now()` — anything ambiguous refuses at type-check.
    Time(i64),
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

/// `time` = int64 unix epoch seconds, UTC only (docs/TIME.md §1).
/// No timezones, no wall-clock `now()`, no DST — anything ambiguous
/// refuses at type-check.

/// Days since the unix epoch for a proleptic-Gregorian civil date
/// (Howard Hinnant's `days_from_civil`). Floor-correct for the full i64
/// year range; every seat re-implements this exact algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y0 = if m <= 2 { y - 1 } else { y };
    let era = y0.div_euclid(400);
    let yoe = y0 - era * 400; // [0, 399]
    let mp = (m + 9) % 12; // [0, 11]
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146097 + doe - 719468
}

/// Proleptic-Gregorian civil date for days since the unix epoch
/// (Hinnant's `civil_from_days`). Floor-correct for negative inputs —
/// pre-1970 times must round-trip exactly.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse a `time` literal's raw ISO-8601 text (e.g. `"2026-10-01T21:30:25Z"`)
/// into int64 unix epoch seconds. This is the ONE place literal semantics
/// live: the parser calls it, and codegens may reuse it. Strict
/// `YYYY-MM-DDTHH:MM:SSZ` only — offsets, fractional seconds, missing `Z`,
/// non-canonical forms, leap seconds, and years outside 0001..=9999 are
/// all refused with a clear message, never silently normalized.
pub fn parse_time_epoch(text: &str) -> Result<i64, String> {
    let refuse = |why: &str| {
        format!(
            "invalid time literal `\"{text}\"t`: {why} — refusing (strict ISO-8601 UTC `YYYY-MM-DDTHH:MM:SSZ` only, docs/TIME.md §2)"
        )
    };
    let b = text.as_bytes();
    if b.len() != 20 {
        return Err(refuse("must be exactly 20 characters"));
    }
    for (i, expect) in [
        (4, b'-'),
        (7, b'-'),
        (10, b'T'),
        (13, b':'),
        (16, b':'),
        (19, b'Z'),
    ] {
        if b[i] != expect {
            return Err(refuse("bad separator"));
        }
    }
    let digits = |lo: usize, hi: usize| -> Result<i64, String> {
        let mut v: i64 = 0;
        for i in lo..hi {
            let c = b[i];
            if !c.is_ascii_digit() {
                return Err(refuse("non-digit in a numeric field"));
            }
            v = v * 10 + (c - b'0') as i64;
        }
        Ok(v)
    };
    let y = digits(0, 4)?;
    let mo = digits(5, 7)?;
    let d = digits(8, 10)?;
    let h = digits(11, 13)?;
    let mi = digits(14, 16)?;
    let s = digits(17, 19)?;
    if !(1..=9999).contains(&y) {
        return Err(refuse("year out of range 0001..=9999"));
    }
    if !(1..=12).contains(&mo) {
        return Err(refuse("month out of range 01..=12"));
    }
    let dim = match mo {
        4 | 6 | 9 | 11 => 30,
        2 if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    };
    if !(1..=dim).contains(&d) {
        return Err(refuse("day out of range for month"));
    }
    if h > 23 {
        return Err(refuse("hour out of range 00..=23"));
    }
    if mi > 59 {
        return Err(refuse("minute out of range 00..=59"));
    }
    if s > 59 {
        return Err(refuse("second out of range 00..=59 (no leap seconds)"));
    }
    days_from_civil(y, mo, d)
        .checked_mul(86400)
        .and_then(|e| e.checked_add(h * 3600 + mi * 60 + s))
        .ok_or_else(|| refuse("epoch out of int64 range"))
}

/// Canonical `time` rendering of an int64 epoch (docs/TIME.md §4):
/// `YYYY-MM-DDTHH:MM:SSZ`, byte-identical on every seat. The year prints
/// with at least 4 digits (wider if needed, e.g. year 10000); negative
/// years (only reachable via arithmetic underflow) print `-` + zero-padded
/// magnitude. Every seat implements this exact rule.
pub fn fmt_time_epoch(e: i64) -> String {
    let days = e.div_euclid(86400);
    let sod = e.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = sod / 3600;
    let mi = (sod % 3600) / 60;
    let ss = sod % 60;
    let ys = if y < 0 {
        format!("-{:04}", -y)
    } else {
        format!("{:04}", y)
    };
    format!("{ys}-{m:02}-{d:02}T{hh:02}:{mi:02}:{ss:02}Z")
}

/// All `time` literal epochs in a program, for per-seat range refusal
/// pre-passes (docs/TIME.md §7). Epochs always fit int64 by construction;
/// only the sol seat narrows the envelope (uint256: non-negative epochs).
pub fn check_time_literals_in_range(program: &Program, seat: &str) -> Result<(), String> {
    if seat != "sol" {
        return Ok(());
    }
    let mut bad: Option<i64> = None;
    fn expr(e: &Expr, bad: &mut Option<i64>) {
        if bad.is_some() {
            return;
        }
        match &e.kind {
            ExprKind::Time(t) => {
                if *t < 0 {
                    *bad = Some(*t);
                }
            }
            ExprKind::List(xs) => xs.iter().for_each(|x| expr(x, bad)),
            ExprKind::Map(pairs) => pairs.iter().for_each(|(k, v)| {
                expr(k, bad);
                expr(v, bad);
            }),
            ExprKind::Call { callee, args } => {
                expr(callee, bad);
                args.iter().for_each(|a| expr(a.expr(), bad));
            }
            ExprKind::Index { base, index } => {
                expr(base, bad);
                expr(index, bad);
            }
            ExprKind::Field { base, .. } => expr(base, bad),
            ExprKind::Binary { lhs, rhs, .. } => {
                expr(lhs, bad);
                expr(rhs, bad);
            }
            ExprKind::Unary { expr: x, .. } => expr(x, bad),
            ExprKind::Unwrap { expr: x, handler } => {
                expr(x, bad);
                handler.iter().for_each(|s| stmt(s, bad));
            }
            ExprKind::InterpStr(parts) => parts.iter().for_each(|p| {
                if let StrPartExpr::Expr(x) = p {
                    expr(x, bad);
                }
            }),
            _ => {}
        }
    }
    fn stmt(s: &Stmt, bad: &mut Option<i64>) {
        if bad.is_some() {
            return;
        }
        match &s.kind {
            StmtKind::Let { value, .. } | StmtKind::Mut { value, .. } => expr(value, bad),
            StmtKind::Assign { target, value } => {
                expr(target, bad);
                expr(value, bad);
            }
            StmtKind::Ret(Some(x)) | StmtKind::Fail(x) | StmtKind::ExprStmt(x) => expr(x, bad),
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                expr(cond, bad);
                then_body.iter().for_each(|x| stmt(x, bad));
                if let Some(eb) = else_body {
                    eb.iter().for_each(|x| stmt(x, bad));
                }
            }
            StmtKind::For { iter, body, .. } => {
                expr(iter, bad);
                body.iter().for_each(|x| stmt(x, bad));
            }
            StmtKind::Whl { cond, body } => {
                expr(cond, bad);
                body.iter().for_each(|x| stmt(x, bad));
            }
            _ => {}
        }
    }
    for item in &program.items {
        match item {
            Item::Def(f) => f.body.iter().for_each(|s| stmt(s, &mut bad)),
            Item::Stmt(s) => stmt(s, &mut bad),
            _ => {}
        }
        if bad.is_some() {
            break;
        }
    }
    match bad {
        Some(v) => Err(format!(
            "{seat} seat: time literal with negative epoch {v} cannot be a uint256 — refusing (docs/TIME.md §7)"
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

    #[test]
    fn time_literal_parses_to_epoch() {
        assert_eq!(parse_time_epoch("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(parse_time_epoch("2026-10-01T21:30:25Z").unwrap(), 1790890225);
        assert_eq!(parse_time_epoch("1969-12-31T23:59:59Z").unwrap(), -1);
        assert_eq!(parse_time_epoch("2000-02-29T12:00:00Z").unwrap(), 951825600);
        assert_eq!(
            parse_time_epoch("9999-12-31T23:59:59Z").unwrap(),
            253402300799
        );
    }

    #[test]
    fn time_literal_refuses_non_canonical() {
        // Offsets, fractional seconds, missing Z, bad separators, leap
        // seconds, impossible dates, year 0000 — all refuse.
        for bad in [
            "2026-10-01T21:30:25+00:00",
            "2026-10-01T21:30:25.000Z",
            "2026-10-01T21:30:25",
            "2026-10-01 21:30:25Z",
            "2026-10-01T21:30:60Z",
            "2026-02-29T00:00:00Z",
            "2026-13-01T00:00:00Z",
            "2026-10-01T24:00:00Z",
            "0000-01-01T00:00:00Z",
            "26-10-01T21:30:25Z",
        ] {
            assert!(parse_time_epoch(bad).is_err(), "accepted: {bad}");
        }
        // Leap years: 2000 and 2024 have Feb 29; 1900 does not.
        assert!(parse_time_epoch("2000-02-29T00:00:00Z").is_ok());
        assert!(parse_time_epoch("2024-02-29T00:00:00Z").is_ok());
        assert!(parse_time_epoch("1900-02-29T00:00:00Z").is_err());
    }

    #[test]
    fn time_formats_canonically_and_round_trips() {
        assert_eq!(fmt_time_epoch(0), "1970-01-01T00:00:00Z");
        assert_eq!(fmt_time_epoch(1790890225), "2026-10-01T21:30:25Z");
        assert_eq!(fmt_time_epoch(-1), "1969-12-31T23:59:59Z");
        assert_eq!(fmt_time_epoch(253402300799), "9999-12-31T23:59:59Z");
        assert_eq!(fmt_time_epoch(-62167219200), "0000-01-01T00:00:00Z");
        // Round-trip: parse(fmt(e)) == e, incl. pre-1970.
        for e in [0, 1, -1, -86400, 1790890225, 951825600, -2208988800] {
            assert_eq!(parse_time_epoch(&fmt_time_epoch(e)).unwrap(), e);
        }
    }
}

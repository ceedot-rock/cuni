//! Java backend — a real, compilable Java seat (not a lowering).
//!
//! Emits one package-private `class Main` (deliberately *not* `public`, so
//! `javac` accepts the check harness's `<stem>_java.java` artifact filename —
//! only `public` top-level classes must match their filename) with:
//!
//! - CuNi `def`s as `static` methods; top-level statements inside
//!   `public static void main(String[] args)`.
//! - Type mapping: `int` -> `long` (CuNi `int` is i64, SPEC.md §2; Java `int`
//!   is 32-bit so it would be the wrong width), `float` -> `double`,
//!   `str` -> `String`, `bool` -> `boolean`, `list<T>` ->
//!   `java.util.List<boxed T>`, `map<K,V>` -> `java.util.Map<boxed K, boxed
//!   V>`, `opt<T>` -> nullable boxed T (`none` -> `null`), `typ` -> a
//!   `static` nested class with a positional constructor and a
//!   Python-dataclass-shaped `toString` (`Point(x=3, y=4)`, matching the py
//!   seat's `@dataclass` repr exactly), payload-free `enum` -> a Java `enum`
//!   whose `toString` prints `Color.Red` (matching `str(Color.Red)` on the
//!   py seat).
//! - Exactness notes (verified against the py seat, which is the gate's
//!   gold):
//!   - Integer `/` on two `long`s is truncated toward zero, exactly CuNi's
//!     `_cuni_div` semantics (see codegen_py.rs); a zero divisor throws
//!     `ArithmeticException`, crashing the seat the way Python's
//!     `ZeroDivisionError` crashes the gold — both are run failures, never
//!     silent divergence. Non-zero-mixed division is IEEE `double` division;
//!     a zero `double` divisor throws via `cuni_div` to match Python raising
//!     instead of producing `Infinity`.
//!   - `%` is Python-floored, *not* Java-truncated: `-7 % 2` is `1` in CuNi
//!     (the py seat emits a bare `%`, i.e. Python semantics) but `-1` in
//!     Java, so every `%` goes through `cuni_mod`, which emulates the
//!     floored form `((a % b) + b) % b` for both `long` and `double`
//!     (zero-guarded to throw like Python).
//!   - `==`/`!=` on reference types (String, List, Map, structs, enums,
//!     generic type variables, boxed numerics) emit
//!     `java.util.Objects.equals` — Java's `==` would compare references.
//!     Primitive `long`/`double`/`boolean` keep `==` (IEEE semantics match).
//!   - `say` prints booleans as `True`/`False` (CuNi's canonical spelling,
//!     matching py/interp), everything else via `String.valueOf` —
//!     `Double.toString` is shortest-round-trip like Python's `repr`.
//!   - `fail` inside a fallible (`?`) function becomes `return null` (the
//!     message is unobservable — the `??` handler never sees it, on any
//!     seat); `fail` outside one throws `RuntimeException`, mirroring Go's
//!     `panic` posture.
//!   - `opt<T>` / fallible returns are plain nullable references; `??` in a
//!     `let`/`mut` binding becomes a null check whose handler must diverge
//!     (end in `ret`/`fail`) — anything else is refused rather than bound
//!     to a half-meaningful `null`.
//!   - `iface` is emitted as a Java `interface` but, like the py/go seats,
//!     `typ ... is ...` conformance is *not* wired up (CuNi realizes iface
//!     methods as free functions, e.g. `def area(c: Circle)`, not as
//!     methods), so the class does not `implement` it. Documented, not
//!     hidden.
//!
//! Honest refusals (Err, never wrong code): `link`, stub (`...`) bodies,
//! `ext` without a `java:` line, `while`-free — no, `while` IS supported —
//! integer-division-free — no, that IS supported — the actual refuse list:
//! `link` (network), `...` stubs, `ext` with no `java:` mapping, `??`
//! outside a `let`/`mut` binding, `??` handlers that don't diverge,
//! unknown method calls, `say` of more than one argument, indexing a
//! `str` for assignment, and unannotated empty list / bare `none` literals
//! (Java needs a static element type and the AST gives none).

use crate::ast::*;
use std::collections::{HashMap, HashSet};

/// Java-side type of a CuNi value, tracked so `==` vs `Objects.equals`,
/// overload choice, and boxing are decided from types, not guesses.
#[derive(Clone, Debug, PartialEq)]
enum JTy {
    Long,
    /// CuNi `dec`: fixed-point decimal, scale 10⁴, exact (docs/DECIMAL.md).
    /// `java.math.BigInteger` — arbitrary precision, divide() truncates
    /// toward zero natively. Wide seat: no range refusal.
    Dec,
    /// CuNi `time`: int64 unix epoch seconds, UTC (docs/TIME.md).
    /// Primitive `long`; checked helpers (`Math.addExact` etc.) refuse
    /// loudly on overflow (narrow-seat envelope). Tracked distinctly from
    /// `Long` so `say`/interpolation render ISO-8601, not the raw epoch.
    Time,
    Double,
    Bool,
    Str,
    List(Box<JTy>),
    Map(Box<JTy>, Box<JTy>),
    /// A generic type variable, e.g. `T` in `def find<T>`.
    TVar(String),
    /// A CuNi `typ`/`enum` name (emitted as a nested class/enum).
    Named(String),
}

impl JTy {
    /// Declaration type for locals/params/fields/returns: primitives stay
    /// primitive, everything else is a reference type.
    fn decl(&self) -> String {
        match self {
            JTy::Long => "long".into(),
            JTy::Time => "long".into(),
            JTy::Dec => "java.math.BigInteger".into(),
            JTy::Double => "double".into(),
            JTy::Bool => "boolean".into(),
            JTy::Str => "String".into(),
            JTy::List(e) => format!("java.util.List<{}>", e.boxed()),
            JTy::Map(k, v) => format!("java.util.Map<{}, {}>", k.boxed(), v.boxed()),
            JTy::TVar(n) | JTy::Named(n) => n.clone(),
        }
    }
    /// Boxed (reference) form, for generics and nullable positions.
    fn boxed(&self) -> String {
        match self {
            JTy::Long => "Long".into(),
            JTy::Time => "Long".into(),
            JTy::Double => "Double".into(),
            JTy::Bool => "Boolean".into(),
            v => v.decl(),
        }
    }
    fn is_primitive(&self) -> bool {
        matches!(self, JTy::Long | JTy::Time | JTy::Double | JTy::Bool)
    }
    fn zero(&self) -> String {
        match self {
            JTy::Long => "0L".into(),
            JTy::Time => "0L".into(),
            JTy::Dec => "java.math.BigInteger.ZERO".into(),
            JTy::Double => "0.0".into(),
            JTy::Bool => "false".into(),
            _ => "null".into(),
        }
    }
}

/// Map a CuNi type to its Java form. `opt<T>` and named types are reference
/// types; everything else follows `JTy::decl`. Unknown named types are
/// passed through as class names (they must be CuNi `typ`/`enum`s declared
/// in the program — javac will reject anything else, which is the honest
/// failure mode).
fn jty(ty: &Type) -> Result<JTy, String> {
    match ty {
        Type::Named(n) => match n.as_str() {
            "int" => Ok(JTy::Long),
            "time" => Ok(JTy::Time),
            "dec" => Ok(JTy::Dec),
            "float" => Ok(JTy::Double),
            "str" => Ok(JTy::Str),
            "bool" => Ok(JTy::Bool),
            other => Ok(JTy::Named(other.to_string())),
        },
        Type::Generic(n, args) => match n.as_str() {
            "list" => Ok(JTy::List(Box::new(jty(&args[0])?))),
            "map" => Ok(JTy::Map(Box::new(jty(&args[0])?), Box::new(jty(&args[1])?))),
            "opt" => Ok(jty(&args[0])?), // nullable reference; nullability is dynamic
            other => Err(format!("generic type `{}` has no Java mapping; refusing", other)),
        },
    }
}

/// Whether a CuNi type is `opt<...>` — needs a nullable (boxed) Java type
/// at declaration sites, even though `jty` erases `opt` to its inner type.
fn is_opt_ty(ty: &Type) -> bool {
    matches!(ty, Type::Generic(n, _) if n == "opt")
}

/// A generated expression: code plus its Java type (for `==` routing,
/// unboxing, and overload selection).
struct JExpr {
    code: String,
    ty: JTy,
}

struct FnInfo {
    ret: Type,
}

pub struct Codegen {
    fn_info: HashMap<String, FnInfo>,
    typ_fields: HashMap<String, Vec<(String, Type)>>,
    enum_names: HashSet<String>,
    /// Type variables in scope (inside a generic def body).
    tvars: HashSet<String>,
    /// Variable -> Java type, for `==` routing and method selection.
    scope: HashMap<String, JTy>,
    /// Enclosing function's declared return type + fallibility, if any.
    cur_fn: Option<(Type, bool)>,
    tmp_counter: usize,
    out: String,
}

pub fn generate(program: &Program) -> Result<String, String> {
    let mut cg = Codegen::new(program)?;
    cg.gen_program(program)?;
    Ok(cg.out)
}

impl Codegen {
    fn new(program: &Program) -> Result<Self, String> {
        let mut fn_info = HashMap::new();
        let mut typ_fields = HashMap::new();
        let mut enum_names = HashSet::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    fn_info.insert(f.name.clone(), FnInfo { ret: f.ret_type.clone() });
                }
                Item::Typ(t) => {
                    typ_fields.insert(
                        t.name.clone(),
                        t.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect(),
                    );
                }
                Item::Enum(e) => {
                    enum_names.insert(e.name.clone());
                }
                _ => {}
            }
        }
        Ok(Codegen {
            fn_info,
            typ_fields,
            enum_names,
            tvars: HashSet::new(),
            scope: HashMap::new(),
            cur_fn: None,
            tmp_counter: 0,
            out: String::new(),
        })
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn fresh_tmp(&mut self) -> String {
        self.tmp_counter += 1;
        format!("__cuni_tmp{}", self.tmp_counter)
    }

    /// Java string literal with escaping. CuNi strings are UTF-8; Java
    /// string literals are UTF-16 — astral characters survive as surrogate
    /// pairs through `\u` escapes only if encoded that way; emitting the raw
    /// chars and writing the file as UTF-8 (javac's default here) keeps them
    /// intact, so only ASCII-special chars are escaped.
    fn jstr(s: &str) -> String {
        let mut r = String::with_capacity(s.len() + 2);
        r.push('"');
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

    fn gen_program(&mut self, program: &Program) -> Result<(), String> {
        self.line(0, "// Generated by the CuNi Java backend. Do not hand-edit.");
        // Package-private on purpose: only `public` top-level classes must
        // match their filename, so `class Main` compiles from the check
        // harness's `<stem>_java.java` artifact path. See module docs.
        self.line(0, "class Main {");
        self.gen_helpers();
        self.out.push('\n');

        let mut script_stmts: Vec<&Stmt> = Vec::new();
        for item in &program.items {
            if let Item::Stmt(s) = item {
                script_stmts.push(s);
            } else {
                self.gen_item(item)?;
                self.out.push('\n');
            }
        }

        self.line(1, "public static void main(String[] args) {");
        let mut top_scope: HashMap<String, JTy> = HashMap::new();
        std::mem::swap(&mut self.scope, &mut top_scope);
        for s in script_stmts {
            self.gen_stmt(2, s)?;
        }
        std::mem::swap(&mut self.scope, &mut top_scope);
        self.line(1, "}");
        self.line(0, "}");
        Ok(())
    }

    /// Runtime helpers mirroring the py seat's builtins (`say`, `range`,
    /// `abs`, `min`, `max`, `_cuni_slice`, `_cuni_div`'s truncation, and the
    /// floored `%` the py seat gets from Python's own operator).
    fn gen_helpers(&mut self) {
        // UTF-8 stdout explicitly: the JVM's default stdout.encoding can be
        // ASCII (seen in CI), which would mangle non-ASCII the spec requires
        // to pass through raw (docs/STDLIB.md §1.3).
        self.line(
            1,
            "static final java.io.PrintStream OUT = new java.io.PrintStream(System.out, true, java.nio.charset.StandardCharsets.UTF_8);",
        );
        self.line(1, "static void say(Object x) {");
        self.line(2, "if (x == null) {");
        self.line(3, "OUT.println(\"None\");");
        self.line(2, "} else if (x instanceof Boolean) {");
        self.line(3, "OUT.println(((Boolean) x).booleanValue() ? \"True\" : \"False\");");
        // A `dec` is a scaled BigInteger (docs/DECIMAL.md) — it must render
        // canonically, not as its raw scaled integer. No other CuNi value
        // is a BigInteger, so this changes nothing else.
        self.line(2, "} else if (x instanceof java.math.BigInteger) {");
        self.line(3, "OUT.println(cuni_dec_str((java.math.BigInteger) x));");
        self.line(2, "} else {");
        self.line(3, "OUT.println(x);");
        self.line(2, "}");
        self.line(1, "}");
        self.line(1, "static String cuni_str(Object x) {");
        self.line(2, "if (x == null) {");
        self.line(3, "return \"None\";");
        self.line(2, "}");
        self.line(2, "if (x instanceof Boolean) {");
        self.line(3, "return ((Boolean) x).booleanValue() ? \"True\" : \"False\";");
        self.line(2, "}");
        self.line(2, "if (x instanceof java.math.BigInteger) {");
        self.line(3, "return cuni_dec_str((java.math.BigInteger) x);");
        self.line(2, "}");
        self.line(2, "return String.valueOf(x);");
        self.line(1, "}");
        // ---- CuNi `dec` (docs/DECIMAL.md): BigInteger, scale 10^4 ----
        self.line(1, "static final java.math.BigInteger CUNI_DEC_SCALE = new java.math.BigInteger(\"10000\");");
        self.line(1, "static String cuni_dec_str(java.math.BigInteger v) {");
        self.line(2, "// Canonical dec rendering (docs/DECIMAL.md §6).");
        self.line(2, "boolean neg = v.signum() < 0;");
        self.line(2, "java.math.BigInteger mag = neg ? v.negate() : v;");
        self.line(2, "java.math.BigInteger[] dr = mag.divideAndRemainder(CUNI_DEC_SCALE);");
        self.line(2, "String fs = String.format(\"%04d\", dr[1].intValue());");
        self.line(2, "int end = fs.length();");
        self.line(2, "while (end > 1 && fs.charAt(end - 1) == '0') end--;");
        self.line(2, "fs = fs.substring(0, end);");
        self.line(2, "return (neg ? \"-\" : \"\") + dr[0].toString() + \".\" + fs;");
        self.line(1, "}");
        self.line(1, "static java.math.BigInteger cuni_dec_div(java.math.BigInteger a, java.math.BigInteger b) {");
        self.line(2, "// trunc(a*10000/b) toward zero (docs/DECIMAL.md §3); BigInteger.divide truncates natively.");
        self.line(2, "if (b.signum() == 0) throw new ArithmeticException(\"cuni: dec division by zero\");");
        self.line(2, "return a.multiply(CUNI_DEC_SCALE).divide(b);");
        self.line(1, "}");
        // ---- CuNi `time`: int64 unix epoch seconds, UTC (docs/TIME.md) ----
        self.line(1, "static String cuni_time_str(long v) {");
        self.line(2, "// Canonical ISO-8601 UTC rendering (docs/TIME.md §4).");
        self.line(2, "long days = Math.floorDiv(v, 86400);");
        self.line(2, "long sod = Math.floorMod(v, 86400);");
        self.line(2, "long z = days + 719468;");
        self.line(2, "long era = Math.floorDiv(z, 146097);");
        self.line(2, "long doe = z - era * 146097;");
        self.line(2, "long yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;");
        self.line(2, "long y = yoe + era * 400;");
        self.line(2, "long doy = doe - (365 * yoe + yoe / 4 - yoe / 100);");
        self.line(2, "long mp = (5 * doy + 2) / 153;");
        self.line(2, "long d = doy - (153 * mp + 2) / 5 + 1;");
        self.line(2, "long m = mp < 10 ? mp + 3 : mp - 9;");
        self.line(2, "if (m <= 2) y++;");
        self.line(2, "long hh = sod / 3600, mi = (sod % 3600) / 60, ss = sod % 60;");
        self.line(2, "String ys = y < 0 ? \"-\" + String.format(\"%04d\", -y) : String.format(\"%04d\", y);");
        self.line(2, "return ys + \"-\" + String.format(\"%02d\", m) + \"-\" + String.format(\"%02d\", d) + \"T\" + String.format(\"%02d\", hh) + \":\" + String.format(\"%02d\", mi) + \":\" + String.format(\"%02d\", ss) + \"Z\";");
        self.line(1, "}");
        self.line(1, "static long cuni_time_days_from_civil(long y, long m, long d) {");
        self.line(2, "long y0 = m <= 2 ? y - 1 : y;");
        self.line(2, "long era = Math.floorDiv(y0, 400);");
        self.line(2, "long yoe = y0 - era * 400;");
        self.line(2, "long mp = (m + 9) % 12;");
        self.line(2, "long doy = (153 * mp + 2) / 5 + d - 1;");
        self.line(2, "long doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;");
        self.line(2, "return era * 146097 + doe - 719468;");
        self.line(1, "}");
        self.line(1, "static long cuni_parse_time(String s) {");
        self.line(2, "// Strict ISO-8601 UTC -> epoch (docs/TIME.md §2, §5): bad input throws loudly.");
        self.line(2, "String bad = \"cuni: parse_time: bad ISO-8601 UTC timestamp — refused\";");
        self.line(2, "if (s == null || s.length() != 20) throw new IllegalArgumentException(bad);");
        self.line(2, "if (s.charAt(4) != '-' || s.charAt(7) != '-' || s.charAt(10) != 'T' || s.charAt(13) != ':' || s.charAt(16) != ':' || s.charAt(19) != 'Z') throw new IllegalArgumentException(bad);");
        self.line(2, "long[] dg = new long[6];");
        self.line(2, "int[][] pos = {{0,4},{5,7},{8,10},{11,13},{14,16},{17,19}};");
        self.line(2, "for (int k = 0; k < 6; k++) {");
        self.line(3, "long v = 0;");
        self.line(3, "for (int i = pos[k][0]; i < pos[k][1]; i++) {");
        self.line(4, "char c = s.charAt(i);");
        self.line(4, "if (c < '0' || c > '9') throw new IllegalArgumentException(bad);");
        self.line(4, "v = v * 10 + (c - '0');");
        self.line(3, "}");
        self.line(3, "dg[k] = v;");
        self.line(2, "}");
        self.line(2, "long y = dg[0], mo = dg[1], d = dg[2], h = dg[3], mi = dg[4], sec = dg[5];");
        self.line(2, "if (y < 1 || y > 9999 || mo < 1 || mo > 12) throw new IllegalArgumentException(bad);");
        self.line(2, "long dim = 31;");
        self.line(2, "if (mo == 4 || mo == 6 || mo == 9 || mo == 11) dim = 30;");
        self.line(2, "else if (mo == 2) dim = (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)) ? 29 : 28;");
        self.line(2, "if (d < 1 || d > dim || h > 23 || mi > 59 || sec > 59) throw new IllegalArgumentException(bad);");
        self.line(2, "return cuni_time_days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec;");
        self.line(1, "}");
        self.line(1, "static long cuni_add_seconds(long t, long s) {");
        self.line(2, "// Math.addExact throws ArithmeticException on overflow — the loud refusal.");
        self.line(2, "return Math.addExact(t, s);");
        self.line(1, "}");
        self.line(1, "static long cuni_days_between(long a, long b) {");
        self.line(2, "// Truncation toward zero (docs/TIME.md §5); Java's / truncates natively.");
        self.line(2, "return Math.subtractExact(a, b) / 86400;");
        self.line(1, "}");
        self.line(1, "static java.util.List<Long> cuni_range(long n) {");
        self.line(2, "java.util.List<Long> out = new java.util.ArrayList<>();");
        self.line(2, "for (long i = 0; i < n; i++) { out.add(i); }");
        self.line(2, "return out;");
        self.line(1, "}");
        // The py seat coerces via int()/float() before comparing; these
        // overloads reproduce that coercion exactly (see codegen_py.rs).
        self.line(1, "static long cuni_abs(long n) { return n < 0 ? -n : n; }");
        self.line(
            1,
            "static long cuni_abs(double n) { long v = (long) n; return v < 0 ? -v : v; }",
        );
        self.line(1, "static long cuni_min(long a, long b) { return a <= b ? a : b; }");
        self.line(
            1,
            "static long cuni_min(double a, double b) { long x = (long) a; long y = (long) b; return x <= y ? x : y; }",
        );
        self.line(1, "static long cuni_max(long a, long b) { return a >= b ? a : b; }");
        self.line(
            1,
            "static long cuni_max(double a, double b) { long x = (long) a; long y = (long) b; return x >= y ? x : y; }",
        );
        // Python-floored `%`: Java's `%` truncates, so -7 % 2 is -1 in Java
        // but 1 on the py seat. Emulated exactly; zero divisors throw like
        // Python's ZeroDivisionError instead of yielding NaN/Infinity.
        self.line(
            1,
            "static long cuni_mod(long a, long b) { if (b == 0) throw new ArithmeticException(\"mod by zero\"); return ((a % b) + b) % b; }",
        );
        self.line(
            1,
            "static double cuni_mod(double a, double b) { if (b == 0.0) throw new ArithmeticException(\"mod by zero\"); return ((a % b) + b) % b; }",
        );
        // Double division that throws on a zero divisor, matching Python
        // raising ZeroDivisionError where Java would print Infinity.
        self.line(
            1,
            "static double cuni_div(double a, double b) { if (b == 0.0) throw new ArithmeticException(\"/ by zero\"); return a / b; }",
        );
        // Mirrors py's `_cuni_slice` bounds rule exactly.
        self.line(
            1,
            "static <T> java.util.List<T> cuni_slice(java.util.List<T> xs, long a, long b) {",
        );
        self.line(2, "long n = xs.size();");
        self.line(
            2,
            "if (a < 0 || b < 0 || a > n || b > n || a > b) return new java.util.ArrayList<T>();",
        );
        self.line(2, "return new java.util.ArrayList<T>(xs.subList((int) a, (int) b));");
        self.line(1, "}");
        self.line(1, "static String cuni_slice(String xs, long a, long b) {");
        self.line(2, "long n = xs.length();");
        self.line(2, "if (a < 0 || b < 0 || a > n || b > n || a > b) return \"\";");
        self.line(2, "return xs.substring((int) a, (int) b);");
        self.line(1, "}");
        // ---- Wave-1 stdlib (docs/STDLIB.md). Zero external dependencies:
        // JSON is a hand-rolled recursive-descent parser, SHA-256 uses the
        // JDK's java.security.MessageDigest.
        self.line(
            1,
            "static RuntimeException cuniErr(String msg) { return new RuntimeException(\"cuni: \" + msg); }",
        );
        self.line(1, "static final long CUNI_JSON_INT_MAX = 9007199254740991L;");
        self.line(1, "static final java.util.regex.Pattern CUNI_NUM_RE =");
        self.line(
            2,
            "java.util.regex.Pattern.compile(\"-?(0|[1-9][0-9]*)(\\\\.[0-9]+)?([eE][+-]?[0-9]+)?\");",
        );
        self.line(1, "// Value-based integer rule (docs/STDLIB.md §1.1). BigDecimal is exact,");
        self.line(1, "// so the token's mathematical value is decided without float error.");
        self.line(1, "static long cuniJsonNum(String tok) {");
        self.line(
            2,
            "if (!CUNI_NUM_RE.matcher(tok).matches()) throw cuniErr(\"json.parse: bad number\");",
        );
        self.line(2, "java.math.BigDecimal bd;");
        self.line(2, "try { bd = new java.math.BigDecimal(tok); }");
        self.line(2, "catch (NumberFormatException ex) { throw cuniErr(\"json.parse: bad number\"); }");
        self.line(2, "bd = bd.stripTrailingZeros();");
        self.line(
            2,
            "if (bd.scale() > 0) throw cuniErr(\"json.parse: number is not an integer in ±(2^53−1)\");",
        );
        self.line(2, "if (bd.abs().compareTo(java.math.BigDecimal.valueOf(CUNI_JSON_INT_MAX)) > 0)");
        self.line(
            3,
            "throw cuniErr(\"json.parse: number is not an integer in ±(2^53−1)\");",
        );
        self.line(2, "return bd.longValueExact();");
        self.line(1, "}");
        self.line(1, "static void cuniJws(String s, int[] p) {");
        self.line(2, "while (p[0] < s.length()) {");
        self.line(3, "char c = s.charAt(p[0]);");
        self.line(3, "if (c == ' ' || c == '\\t' || c == '\\n' || c == '\\r') p[0]++; else break;");
        self.line(2, "}");
        self.line(1, "}");
        self.line(1, "static int cuniJhex4(String s, int[] p) {");
        self.line(2, "int v = 0;");
        self.line(2, "for (int i = 0; i < 4; i++) {");
        self.line(3, "if (p[0] >= s.length()) throw cuniErr(\"json.parse: bad \\\\u escape\");");
        self.line(3, "int d = Character.digit(s.charAt(p[0]++), 16);");
        self.line(3, "if (d < 0) throw cuniErr(\"json.parse: bad \\\\u escape\");");
        self.line(3, "v = v * 16 + d;");
        self.line(2, "}");
        self.line(2, "return v;");
        self.line(1, "}");
        self.line(1, "static String cuniJstr(String s, int[] p) {");
        self.line(2, "p[0]++; // opening quote");
        self.line(2, "StringBuilder b = new StringBuilder();");
        self.line(2, "while (true) {");
        self.line(3, "int start = p[0];");
        self.line(3, "while (p[0] < s.length()) {");
        self.line(4, "char c = s.charAt(p[0]);");
        self.line(4, "if (c == '\"' || c == '\\\\') break;");
        self.line(
            4,
            "if (c < 0x20) throw cuniErr(\"json.parse: unescaped control character in string\");",
        );
        self.line(4, "p[0]++;");
        self.line(3, "}");
        self.line(3, "b.append(s, start, p[0]);");
        self.line(3, "if (p[0] >= s.length()) throw cuniErr(\"json.parse: unterminated string\");");
        self.line(3, "char c = s.charAt(p[0]);");
        self.line(3, "if (c == '\"') { p[0]++; return b.toString(); }");
        self.line(3, "p[0]++; // backslash");
        self.line(3, "if (p[0] >= s.length()) throw cuniErr(\"json.parse: unterminated string\");");
        self.line(3, "char e = s.charAt(p[0]++);");
        self.line(3, "switch (e) {");
        self.line(4, "case '\"': b.append('\"'); break;");
        self.line(4, "case '\\\\': b.append('\\\\'); break;");
        self.line(4, "case '/': b.append('/'); break;");
        self.line(4, "case 'b': b.append('\\b'); break;");
        self.line(4, "case 'f': b.append('\\f'); break;");
        self.line(4, "case 'n': b.append('\\n'); break;");
        self.line(4, "case 'r': b.append('\\r'); break;");
        self.line(4, "case 't': b.append('\\t'); break;");
        self.line(4, "case 'u': {");
        self.line(5, "int hi = cuniJhex4(s, p);");
        self.line(5, "if (hi >= 0xD800 && hi < 0xDC00) {");
        self.line(
            6,
            "if (p[0] + 1 >= s.length() || s.charAt(p[0]) != '\\\\' || s.charAt(p[0] + 1) != 'u')",
        );
        self.line(7, "throw cuniErr(\"json.parse: lone surrogate\");");
        self.line(6, "p[0] += 2;");
        self.line(6, "int lo = cuniJhex4(s, p);");
        self.line(6, "if (lo < 0xDC00 || lo >= 0xE000) throw cuniErr(\"json.parse: lone surrogate\");");
        self.line(6, "b.appendCodePoint(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00));");
        self.line(5, "} else if (hi >= 0xDC00 && hi < 0xE000) {");
        self.line(6, "throw cuniErr(\"json.parse: lone surrogate\");");
        self.line(5, "} else {");
        self.line(6, "b.appendCodePoint(hi);");
        self.line(5, "}");
        self.line(5, "break;");
        self.line(4, "}");
        self.line(4, "default: throw cuniErr(\"json.parse: bad escape\");");
        self.line(3, "}");
        self.line(2, "}");
        self.line(1, "}");
        self.line(1, "static Object cuniJval(String s, int[] p) {");
        self.line(2, "if (p[0] >= s.length()) throw cuniErr(\"json.parse: unexpected end\");");
        self.line(2, "char c = s.charAt(p[0]);");
        self.line(2, "switch (c) {");
        self.line(3, "case '{': return cuniJobj(s, p);");
        self.line(3, "case '[': return cuniJarr(s, p);");
        self.line(3, "case '\"': return cuniJstr(s, p);");
        self.line(3, "case 't': if (s.startsWith(\"true\", p[0])) { p[0] += 4; return Boolean.TRUE; } break;");
        self.line(
            3,
            "case 'f': if (s.startsWith(\"false\", p[0])) { p[0] += 5; return Boolean.FALSE; } break;",
        );
        self.line(3, "case 'n': if (s.startsWith(\"null\", p[0])) { p[0] += 4; return null; } break;");
        self.line(3, "default: break;");
        self.line(2, "}");
        self.line(2, "if (c == '-' || (c >= '0' && c <= '9')) {");
        self.line(3, "int start = p[0];");
        self.line(3, "while (p[0] < s.length()) {");
        self.line(4, "char d = s.charAt(p[0]);");
        self.line(
            4,
            "if ((d >= '0' && d <= '9') || d == '.' || d == 'e' || d == 'E' || d == '+' || d == '-') p[0]++; else break;",
        );
        self.line(3, "}");
        self.line(3, "return cuniJsonNum(s.substring(start, p[0]));");
        self.line(2, "}");
        self.line(2, "throw cuniErr(\"json.parse: unexpected character\");");
        self.line(1, "}");
        self.line(1, "static java.util.Map<String, Object> cuniJobj(String s, int[] p) {");
        self.line(2, "p[0]++; // {");
        self.line(2, "java.util.Map<String, Object> m = new java.util.HashMap<>();");
        self.line(2, "cuniJws(s, p);");
        self.line(2, "if (p[0] < s.length() && s.charAt(p[0]) == '}') { p[0]++; return m; }");
        self.line(2, "while (true) {");
        self.line(3, "cuniJws(s, p);");
        self.line(
            3,
            "if (p[0] >= s.length() || s.charAt(p[0]) != '\"') throw cuniErr(\"json.parse: object keys must be strings\");",
        );
        self.line(3, "String key = cuniJstr(s, p);");
        self.line(3, "cuniJws(s, p);");
        self.line(
            3,
            "if (p[0] >= s.length() || s.charAt(p[0]) != ':') throw cuniErr(\"json.parse: expected ':'\");",
        );
        self.line(3, "p[0]++; cuniJws(s, p);");
        self.line(3, "m.put(key, cuniJval(s, p)); // duplicate keys: last wins");
        self.line(3, "cuniJws(s, p);");
        self.line(3, "if (p[0] >= s.length()) throw cuniErr(\"json.parse: expected ',' or '}'\");");
        self.line(3, "char d = s.charAt(p[0]);");
        self.line(3, "if (d == ',') { p[0]++; continue; }");
        self.line(3, "if (d == '}') { p[0]++; return m; }");
        self.line(3, "throw cuniErr(\"json.parse: expected ',' or '}'\");");
        self.line(2, "}");
        self.line(1, "}");
        self.line(1, "static java.util.List<Object> cuniJarr(String s, int[] p) {");
        self.line(2, "p[0]++; // [");
        self.line(2, "java.util.List<Object> xs = new java.util.ArrayList<>();");
        self.line(2, "cuniJws(s, p);");
        self.line(2, "if (p[0] < s.length() && s.charAt(p[0]) == ']') { p[0]++; return xs; }");
        self.line(2, "while (true) {");
        self.line(3, "cuniJws(s, p);");
        self.line(3, "xs.add(cuniJval(s, p));");
        self.line(3, "cuniJws(s, p);");
        self.line(3, "if (p[0] >= s.length()) throw cuniErr(\"json.parse: expected ',' or ']'\");");
        self.line(3, "char d = s.charAt(p[0]);");
        self.line(3, "if (d == ',') { p[0]++; continue; }");
        self.line(3, "if (d == ']') { p[0]++; return xs; }");
        self.line(3, "throw cuniErr(\"json.parse: expected ',' or ']'\");");
        self.line(2, "}");
        self.line(1, "}");
        self.line(1, "static java.util.Map<String, Object> cuniJsonParse(Object s) {");
        self.line(2, "if (!(s instanceof String)) throw cuniErr(\"json.parse needs a str\");");
        self.line(2, "String t = (String) s;");
        self.line(2, "int[] p = { 0 };");
        self.line(2, "cuniJws(t, p);");
        self.line(2, "Object v = cuniJval(t, p);");
        self.line(2, "cuniJws(t, p);");
        self.line(2, "if (p[0] != t.length()) throw cuniErr(\"json.parse: trailing characters\");");
        self.line(
            2,
            "if (!(v instanceof java.util.Map)) throw cuniErr(\"json.parse: top-level JSON value must be an object\");",
        );
        self.line(2, "@SuppressWarnings(\"unchecked\")");
        self.line(2, "java.util.Map<String, Object> m = (java.util.Map<String, Object>) v;");
        self.line(2, "return m;");
        self.line(1, "}");
        self.line(1, "// Canonical minimal emit (docs/STDLIB.md §1.3). TreeMap sorts on");
        self.line(1, "// UTF-16 code units, which order exactly like UTF-8 bytes.");
        self.line(1, "static void cuniJesc(String s, StringBuilder b) {");
        self.line(2, "b.append('\"');");
        self.line(2, "for (int i = 0; i < s.length(); ) {");
        self.line(3, "int cp = s.codePointAt(i);");
        self.line(3, "switch (cp) {");
        self.line(4, "case '\"': b.append(\"\\\\\\\"\"); break;");
        self.line(4, "case '\\\\': b.append(\"\\\\\\\\\"); break;");
        self.line(4, "case '\\b': b.append(\"\\\\b\"); break;");
        self.line(4, "case '\\f': b.append(\"\\\\f\"); break;");
        self.line(4, "case '\\n': b.append(\"\\\\n\"); break;");
        self.line(4, "case '\\r': b.append(\"\\\\r\"); break;");
        self.line(4, "case '\\t': b.append(\"\\\\t\"); break;");
        self.line(4, "default:");
        self.line(5, "if (cp < 0x20) b.append(String.format(\"\\\\u%04x\", cp));");
        self.line(5, "else b.appendCodePoint(cp);");
        self.line(3, "}");
        self.line(3, "i += Character.charCount(cp);");
        self.line(2, "}");
        self.line(2, "b.append('\"');");
        self.line(1, "}");
        self.line(1, "static void cuniJwrite(Object v, StringBuilder b) {");
        self.line(2, "if (v == null) { b.append(\"null\"); }");
        self.line(2, "else if (v instanceof String) { cuniJesc((String) v, b); }");
        self.line(2, "else if (v instanceof Boolean) { b.append(((Boolean) v) ? \"true\" : \"false\"); }");
        self.line(2, "else if (v instanceof Long) { b.append(v); }");
        self.line(2, "else if (v instanceof java.util.List) {");
        self.line(3, "b.append('[');");
        self.line(3, "boolean first = true;");
        self.line(3, "for (Object x : (java.util.List<?>) v) {");
        self.line(4, "if (!first) b.append(',');");
        self.line(4, "first = false;");
        self.line(4, "cuniJwrite(x, b);");
        self.line(3, "}");
        self.line(3, "b.append(']');");
        self.line(2, "} else if (v instanceof java.util.Map) {");
        self.line(3, "java.util.TreeMap<String, Object> sorted = new java.util.TreeMap<>();");
        self.line(3, "for (java.util.Map.Entry<?, ?> e : ((java.util.Map<?, ?>) v).entrySet()) {");
        self.line(
            4,
            "if (!(e.getKey() instanceof String)) throw cuniErr(\"json.emit: map keys must be strings\");",
        );
        self.line(4, "sorted.put((String) e.getKey(), e.getValue());");
        self.line(3, "}");
        self.line(3, "b.append('{');");
        self.line(3, "boolean first = true;");
        self.line(3, "for (java.util.Map.Entry<String, Object> e : sorted.entrySet()) {");
        self.line(4, "if (!first) b.append(',');");
        self.line(4, "first = false;");
        self.line(4, "cuniJesc(e.getKey(), b);");
        self.line(4, "b.append(':');");
        self.line(4, "cuniJwrite(e.getValue(), b);");
        self.line(3, "}");
        self.line(3, "b.append('}');");
        self.line(2, "} else {");
        self.line(3, "throw cuniErr(\"json.emit: value has no JSON form\");");
        self.line(2, "}");
        self.line(1, "}");
        self.line(1, "static String cuniJsonEmit(Object m) {");
        self.line(2, "if (!(m instanceof java.util.Map)) throw cuniErr(\"json.emit needs a map\");");
        self.line(2, "StringBuilder b = new StringBuilder();");
        self.line(2, "cuniJwrite(m, b);");
        self.line(2, "return b.toString();");
        self.line(1, "}");
        self.line(1, "// Proleptic Gregorian, Howard Hinnant's algorithms (docs/STDLIB.md §2).");
        self.line(1, "// All divisions below are on non-negative operands.");
        self.line(1, "static long cuniDaysFromCivil(long y, long m, long d) {");
        self.line(2, "long y0 = m <= 2 ? y - 1 : y;");
        self.line(2, "long era = y0 / 400;");
        self.line(2, "long yoe = y0 - era * 400;");
        self.line(2, "long mp = (m + 9) % 12;");
        self.line(2, "long doy = (153 * mp + 2) / 5 + d - 1;");
        self.line(2, "long doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;");
        self.line(2, "return era * 146097 + doe - 719468;");
        self.line(1, "}");
        self.line(1, "static long[] cuniCivilFromDays(long z) {");
        self.line(2, "z += 719468;");
        self.line(2, "long era = z / 146097;");
        self.line(2, "long doe = z - era * 146097;");
        self.line(2, "long yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;");
        self.line(2, "long y = yoe + era * 400;");
        self.line(2, "long doy = doe - (365 * yoe + yoe / 4 - yoe / 100);");
        self.line(2, "long mp = (5 * doy + 2) / 153;");
        self.line(2, "long d = doy - (153 * mp + 2) / 5 + 1;");
        self.line(2, "long mo = mp < 10 ? mp + 3 : mp - 9;");
        self.line(2, "if (mo <= 2) y++;");
        self.line(2, "return new long[] { y, mo, d };");
        self.line(1, "}");
        self.line(1, "static long cuniTimeEpoch(long y, long mo, long d, long h, long mi, long s) {");
        self.line(
            2,
            "if (y < 1 || y > 9999) throw cuniErr(\"time.epoch: year out of range 1..9999\");",
        );
        self.line(
            2,
            "if (mo < 1 || mo > 12) throw cuniErr(\"time.epoch: month out of range 1..12\");",
        );
        self.line(2, "if (h < 0 || h > 23) throw cuniErr(\"time.epoch: hour out of range 0..23\");");
        self.line(
            2,
            "if (mi < 0 || mi > 59) throw cuniErr(\"time.epoch: minute out of range 0..59\");",
        );
        self.line(2, "if (s < 0 || s > 59) throw cuniErr(\"time.epoch: second out of range 0..59\");");
        self.line(2, "boolean leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);");
        self.line(
            2,
            "long dim = mo == 2 ? (leap ? 29 : 28) : (mo == 4 || mo == 6 || mo == 9 || mo == 11 ? 30 : 31);",
        );
        self.line(
            2,
            "if (d < 1 || d > dim) throw cuniErr(\"time.epoch: day out of range for month\");",
        );
        self.line(2, "return cuniDaysFromCivil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s;");
        self.line(1, "}");
        self.line(1, "static java.util.Map<String, Object> cuniTimeParts(long e) {");
        self.line(2, "long lo = cuniDaysFromCivil(1, 1, 1) * 86400;");
        self.line(2, "long hi = cuniDaysFromCivil(9999, 12, 31) * 86400 + 86399;");
        self.line(
            2,
            "if (e < lo || e > hi) throw cuniErr(\"time.parts: epoch out of range 1..9999\");",
        );
        self.line(2, "long days = Math.floorDiv(e, 86400);");
        self.line(2, "long secs = e - days * 86400;");
        self.line(2, "long[] ymd = cuniCivilFromDays(days);");
        self.line(2, "java.util.Map<String, Object> m = new java.util.HashMap<>();");
        self.line(2, "m.put(\"year\", ymd[0]); m.put(\"month\", ymd[1]); m.put(\"day\", ymd[2]);");
        self.line(
            2,
            "m.put(\"hour\", secs / 3600); m.put(\"min\", (secs % 3600) / 60); m.put(\"sec\", secs % 60);",
        );
        self.line(2, "return m;");
        self.line(1, "}");
        self.line(1, "static java.util.List<String> cuniSplit(Object s, Object sep) {");
        self.line(
            2,
            "if (!(s instanceof String) || !(sep instanceof String)) throw cuniErr(\".split needs strings\");",
        );
        self.line(2, "String ss = (String) s, pp = (String) sep;");
        self.line(2, "if (pp.isEmpty()) throw cuniErr(\".split: empty separator; refusing\");");
        self.line(2, "java.util.List<String> out = new java.util.ArrayList<>();");
        self.line(2, "if (ss.isEmpty()) { out.add(\"\"); return out; }");
        self.line(2, "int start = 0, hit;");
        self.line(2, "while ((hit = ss.indexOf(pp, start)) >= 0) {");
        self.line(3, "out.add(ss.substring(start, hit));");
        self.line(3, "start = hit + pp.length();");
        self.line(2, "}");
        self.line(2, "out.add(ss.substring(start));");
        self.line(2, "return out;");
        self.line(1, "}");
        self.line(1, "static String cuniJoin(Object sep, Object parts) {");
        self.line(
            2,
            "if (!(sep instanceof String)) throw cuniErr(\".join needs a str separator\");",
        );
        self.line(
            2,
            "if (!(parts instanceof java.util.List)) throw cuniErr(\".join needs a list<str>\");",
        );
        self.line(2, "StringBuilder b = new StringBuilder();");
        self.line(2, "boolean first = true;");
        self.line(2, "for (Object x : (java.util.List<?>) parts) {");
        self.line(3, "if (!(x instanceof String)) throw cuniErr(\".join: all parts must be str\");");
        self.line(3, "if (!first) b.append((String) sep);");
        self.line(3, "first = false;");
        self.line(3, "b.append((String) x);");
        self.line(2, "}");
        self.line(2, "return b.toString();");
        self.line(1, "}");
        self.line(1, "static boolean cuniIsTrim(char c) {");
        self.line(
            2,
            "return c == ' ' || c == '\\t' || c == '\\n' || c == 0x0B || c == '\\f' || c == '\\r';",
        );
        self.line(1, "}");
        self.line(1, "// ASCII whitespace only (docs/STDLIB.md §3.3) — not String.strip().");
        self.line(1, "static String cuniTrim(Object s) {");
        self.line(2, "if (!(s instanceof String)) throw cuniErr(\".trim needs a str\");");
        self.line(2, "String ss = (String) s;");
        self.line(2, "int a = 0, b = ss.length();");
        self.line(2, "while (a < b && cuniIsTrim(ss.charAt(a))) a++;");
        self.line(2, "while (b > a && cuniIsTrim(ss.charAt(b - 1))) b--;");
        self.line(2, "return ss.substring(a, b);");
        self.line(1, "}");
        self.line(1, "static boolean cuniContains(Object s, Object sub) {");
        self.line(
            2,
            "if (!(s instanceof String) || !(sub instanceof String)) throw cuniErr(\".contains needs strings\");",
        );
        self.line(2, "return ((String) s).contains((String) sub);");
        self.line(1, "}");
        self.line(1, "// SHA-256 (docs/STDLIB.md §4) via the JDK's MessageDigest.");
        self.line(1, "static String cuniSha256(Object s) {");
        self.line(2, "if (!(s instanceof String)) throw cuniErr(\"sha256 needs a str\");");
        self.line(2, "try {");
        self.line(
            3,
            "java.security.MessageDigest md = java.security.MessageDigest.getInstance(\"SHA-256\");",
        );
        self.line(
            3,
            "byte[] h = md.digest(((String) s).getBytes(java.nio.charset.StandardCharsets.UTF_8));",
        );
        self.line(3, "StringBuilder b = new StringBuilder();");
        self.line(3, "for (byte x : h) b.append(String.format(\"%02x\", x & 0xFF));");
        self.line(3, "return b.toString();");
        self.line(2, "} catch (java.security.NoSuchAlgorithmException ex) {");
        self.line(3, "throw cuniErr(\"sha256: SHA-256 unavailable\");");
        self.line(2, "}");
        self.line(1, "}");
    }

    fn gen_item(&mut self, item: &Item) -> Result<(), String> {
        match item {
            Item::Use(u) => {
                // Unreachable: check.rs resolves `use` into the item list
                // before codegen. Kept as a comment for debuggability.
                self.line(1, &format!("// use {} (resolved by the front-end)", u.name));
                Ok(())
            }
            Item::Ext(ext) => {
                let ret = jty(&ext.ret_type)?;
                let params = ext
                    .params
                    .iter()
                    .map(|p| Ok(format!("{} {}", jty(&p.ty)?.decl(), p.name)))
                    .collect::<Result<Vec<_>, String>>()?
                    .join(", ");
                let raw = ext.targets.iter().find(|(t, _)| t == "java").map(|(_, r)| r.clone());
                match raw {
                    Some(raw) => {
                        self.line(1, &format!("static {} {}({}) {{", ret.decl(), ext.name, params));
                        self.line(2, &format!("return {};", raw));
                        self.line(1, "}");
                        Ok(())
                    }
                    None => Err(format!(
                        "`ext {}` has no `java:` mapping; refusing (rename the binding or add one)",
                        ext.name
                    )),
                }
            }
            Item::Typ(t) => {
                // Static nested class, positional constructor in field order,
                // Python-dataclass-shaped toString so `say(Point(3, 4))`
                // prints `Point(x=3, y=4)` exactly like the py seat.
                self.line(1, &format!("static class {} {{", t.name));
                // Borrow the field list once per field: `self.line` needs
                // `&mut self`, so the `&self.typ_fields` borrow can't live
                // across it.
                let nf = self.typ_fields[&t.name].len();
                for i in 0..nf {
                    let (fname, fdecl) = {
                        let (fname, fty) = &self.typ_fields[&t.name][i];
                        (fname.clone(), jty(fty)?.decl())
                    };
                    self.line(2, &format!("{} {};", fdecl, fname));
                }
                let ctor_params = t
                    .fields
                    .iter()
                    .map(|f| Ok(format!("{} {}", jty(&f.ty)?.decl(), f.name)))
                    .collect::<Result<Vec<_>, String>>()?
                    .join(", ");
                self.line(2, &format!("{}( {} ) {{", t.name, ctor_params));
                let cnames: Vec<String> = {
                    self.typ_fields[&t.name]
                        .iter()
                        .map(|(n, _)| n.clone())
                        .collect()
                };
                for fname in &cnames {
                    self.line(3, &format!("this.{0} = {0};", fname));
                }
                self.line(2, "}");
                // toString: `Point(x=3, y=4)` — booleans need the
                // True/False spelling, so route every field through cuni_str.
                let parts = t
                    .fields
                    .iter()
                    .map(|f| format!("\"{0}=\" + cuni_str({0})", f.name))
                    .collect::<Vec<_>>()
                    .join(" + \", \" + ");
                self.line(2, "public String toString() {");
                if parts.is_empty() {
                    self.line(3, &format!("return \"{}()\";", t.name));
                } else {
                    self.line(3, &format!("return \"{}(\" + {} + \")\";", t.name, parts));
                }
                self.line(2, "}");
                self.line(1, "}");
                if let Some(base) = &t.implements {
                    self.line(
                        1,
                        &format!(
                            "// note: `{} is {}` conformance is checked by CuNi typeck, not via `implements` — CuNi realizes iface methods as free functions, see codegen_java.rs docs",
                            t.name, base
                        ),
                    );
                }
                Ok(())
            }
            Item::Iface(i) => {
                self.line(1, &format!("interface {} {{", i.name));
                for m in &i.methods {
                    let ret = jty(&m.ret_type)?;
                    let params = m
                        .params
                        .iter()
                        .map(|p| Ok(format!("{} {}", jty(&p.ty)?.decl(), p.name)))
                        .collect::<Result<Vec<_>, String>>()?
                        .join(", ");
                    self.line(2, &format!("{} {}({});", ret.decl(), m.name, params));
                }
                self.line(1, "}");
                Ok(())
            }
            Item::Enum(e) => {
                self.line(1, &format!("static enum {} {{", e.name));
                if e.variants.is_empty() {
                    self.line(2, ";");
                } else {
                    let vs = e
                        .variants
                        .iter()
                        .map(|v| v.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.line(2, &format!("{};", vs));
                }
                // Match `str(Color.Red)` == "Color.Red" on the py seat.
                self.line(2, "public String toString() {");
                self.line(3, &format!("return \"{}.\" + this.name();", e.name));
                self.line(2, "}");
                self.line(1, "}");
                Ok(())
            }
            Item::Def(f) => self.gen_def(f),
            Item::Stmt(s) => self.gen_stmt(1, s),
        }
    }

    fn gen_def(&mut self, f: &FnDecl) -> Result<(), String> {
        if f.is_link {
            return Err(format!(
                "`link {}` is a network binding; the Java seat has no HTTP runtime — refusing",
                f.name
            ));
        }
        let ret_jt = jty(&f.ret_type)?;
        // Fallible functions return a nullable boxed value (`fail` -> null),
        // exactly the `opt` shape; a declared `opt<T>` return is likewise
        // nullable. Non-fallible, non-opt keeps its declared type.
        let ret_decl = if f.fallible || is_opt_ty(&f.ret_type) {
            ret_jt.boxed()
        } else {
            ret_jt.decl()
        };
        let generics = if f.generics.is_empty() {
            String::new()
        } else {
            format!("<{}> ", f.generics.join(", "))
        };
        let params = f
            .params
            .iter()
            .map(|p| {
                let jt = jty(&p.ty)?;
                // A parameter declared as a bare type variable stays `T`;
                // a declared `opt<T>` parameter is nullable (boxed).
                let decl = if is_opt_ty(&p.ty) { jt.boxed() } else { jt.decl() };
                Ok(format!("{} {}", decl, p.name))
            })
            .collect::<Result<Vec<_>, String>>()?
            .join(", ");
        self.line(
            1,
            &format!("static {}{} {}({}) {{", generics, ret_decl, f.name, params),
        );

        let prev_tvars = std::mem::take(&mut self.tvars);
        for g in &f.generics {
            self.tvars.insert(g.clone());
        }
        let prev_scope = std::mem::take(&mut self.scope);
        for p in &f.params {
            let mut jt = jty(&p.ty)?;
            if let JTy::Named(n) = &jt {
                if self.tvars.contains(n) {
                    jt = JTy::TVar(n.clone());
                }
            }
            self.scope.insert(p.name.clone(), jt);
        }
        let prev_fn = self.cur_fn.replace((f.ret_type.clone(), f.fallible));
        let mut r = Ok(());
        if f.body.is_empty() {
            r = Err(format!(
                "def `{}` has an empty body; the Java seat needs a real body — refusing",
                f.name
            ));
        } else {
            for s in &f.body {
                if let Err(e) = self.gen_stmt(2, s) {
                    r = Err(e);
                    break;
                }
            }
        }
        self.cur_fn = prev_fn;
        self.scope = prev_scope;
        self.tvars = prev_tvars;
        self.line(1, "}");
        r
    }

    fn gen_stmt(&mut self, indent: usize, stmt: &Stmt) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                self.gen_binding(indent, name, ty, value)
            }
            StmtKind::Assign { target, value } => {
                let v = self.gen_expr(value)?;
                match &target.kind {
                    ExprKind::Ident(name) => {
                        self.line(indent, &format!("{} = {};", name, v.code));
                        Ok(())
                    }
                    ExprKind::Field { base, name } => {
                        let b = self.gen_expr(base)?;
                        self.line(indent, &format!("{}.{} = {};", b.code, name, v.code));
                        Ok(())
                    }
                    ExprKind::Index { base, index } => {
                        let b = self.gen_expr(base)?;
                        let ix = self.gen_expr(index)?;
                        match &b.ty {
                            JTy::List(_) => {
                                self.line(
                                    indent,
                                    &format!("{}.set((int) ({}), {});", b.code, ix.code, v.code),
                                );
                                Ok(())
                            }
                            JTy::Map(_, _) => {
                                self.line(
                                    indent,
                                    &format!("{}.put({}, {});", b.code, ix.code, v.code),
                                );
                                Ok(())
                            }
                            _ => Err("index assignment needs a list or map target; refusing".into()),
                        }
                    }
                    _ => Err("assignment target must be a variable, field, or index; refusing".into()),
                }
            }
            StmtKind::Ret(value) => self.gen_ret(indent, value),
            StmtKind::Fail(e) => {
                let v = self.gen_expr(e)?;
                match &self.cur_fn {
                    Some((_, true)) => {
                        // Unobservable message (the `??` handler never sees
                        // it); `fail` is just "return failure".
                        self.line(indent, &format!("return null; // `fail` ({})", v.code));
                        Ok(())
                    }
                    _ => {
                        self.line(
                            indent,
                            &format!(
                                "throw new RuntimeException(cuni_str({})); // `fail` outside a fallible function",
                                v.code
                            ),
                        );
                        Ok(())
                    }
                }
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                let c = self.gen_expr(cond)?;
                let c = self.as_bool(&c)?;
                self.line(indent, &format!("if ({}) {{", c));
                self.gen_block(indent + 1, then_body)?;
                match else_body {
                    Some(eb) => {
                        self.line(indent, "} else {");
                        self.gen_block(indent + 1, eb)?;
                        self.line(indent, "}");
                    }
                    None => self.line(indent, "}"),
                }
                Ok(())
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => self.gen_for(indent, a, b.as_deref(), iter, body),
            StmtKind::Whl { cond, body } => {
                let c = self.gen_expr(cond)?;
                let c = self.as_bool(&c)?;
                self.line(indent, &format!("while ({}) {{", c));
                self.gen_block(indent + 1, body)?;
                self.line(indent, "}");
                Ok(())
            }
            StmtKind::ExprStmt(e) => {
                // `say(x)` is statement-only (see gen_call): emit the call
                // here, where `void` is a legal shape.
                if let ExprKind::Call { callee, args } = &e.kind {
                    if let ExprKind::Ident(fname) = &callee.kind {
                        if fname == "say" {
                            if args.len() != 1 {
                                return Err("`say` takes exactly one argument; refusing".into());
                            }
                            let v = self.gen_expr(args[0].expr())?;
                            // A `time` is a `long` at runtime (docs/TIME.md) —
                            // it must render as ISO-8601, not as its raw
                            // epoch. The codegen knows the static type, so
                            // route here (like dec's BigInteger branch below).
                            if matches!(v.ty, JTy::Time) {
                                self.line(indent, &format!("say(cuni_time_str({}));", v.code));
                            } else {
                                self.line(indent, &format!("say({});", v.code));
                            }
                            return Ok(());
                        }
                    }
                }
                // `.push(x)` as a statement: Java's `add` returns a boolean
                // (unlike py's `append` returning None), so in *expression*
                // position it would be a different value — refused there
                // (see gen_expr); as a statement the value is discarded on
                // every seat, so `.add(x);` is exact.
                if let ExprKind::Call { callee, args } = &e.kind {
                    if let ExprKind::Field { base, name } = &callee.kind {
                        if name == "push" {
                            let b = self.gen_expr(base)?;
                            if !matches!(b.ty, JTy::List(_)) {
                                return Err("`.push` needs a list target; refusing".into());
                            }
                            let av: Vec<String> = args
                                .iter()
                                .map(|x| self.gen_expr(x.expr()).map(|e| e.code))
                                .collect::<Result<_, _>>()?;
                            self.line(indent, &format!("{}.add({});", b.code, av.join(", ")));
                            return Ok(());
                        }
                    }
                }
                let v = self.gen_expr(e)?;
                self.line(indent, &format!("{};", v.code));
                Ok(())
            }
            StmtKind::Todo => Err(
                "stub (`...`) bodies have no Java mapping — refusing (write the body or drop the def)"
                    .into(),
            ),
        }
    }

    fn gen_block(&mut self, indent: usize, stmts: &[Stmt]) -> Result<(), String> {
        for s in stmts {
            self.gen_stmt(indent, s)?;
        }
        Ok(())
    }

    /// `let`/`mut` with optional `??` handling. The handler must diverge
    /// (its last statement is `ret` or `fail`); otherwise the binding would
    /// be half-defined and every seat disagrees about what follows, so this
    /// seat refuses instead of guessing.
    fn gen_binding(
        &mut self,
        indent: usize,
        name: &str,
        ty: &Option<Type>,
        value: &Expr,
    ) -> Result<(), String> {
        if let ExprKind::Unwrap { expr, handler } = &value.kind {
            let last_diverges = matches!(
                handler.last().map(|s| &s.kind),
                Some(StmtKind::Ret(_)) | Some(StmtKind::Fail(_))
            );
            if !last_diverges {
                return Err(format!(
                    "the `??` handler for `{}` must diverge (end in `ret`/`fail`); refusing",
                    name
                ));
            }
            let decl_ty = match ty {
                Some(t) => jty(t)?.boxed(),
                None => self.infer_unwrap_ty(expr)?,
            };
            let inner = self.gen_expr(expr)?;
            let tmp = self.fresh_tmp();
            self.line(indent, &format!("{} {} = {};", decl_ty, tmp, inner.code));
            self.line(indent, &format!("if ({} == null) {{", tmp));
            self.gen_block(indent + 1, handler)?;
            self.line(indent, "}");
            // Definite assignment: the null path diverged (ret/fail), so
            // reaching here means non-null — exactly the py seat's
            // `else: name = _unwrapped` shape.
            self.line(indent, &format!("{} {} = {};", decl_ty, name, tmp));
            self.scope.insert(name.to_string(), self.jty_of_decl(&decl_ty));
            return Ok(());
        }
        let v = self.gen_expr(value)?;
        let jt = match ty {
            Some(t) => {
                let base = jty(t)?;
                // `let x: list<int> = [...]` — declared type wins; the
                // value must already agree (typeck checked it).
                base
            }
            None => v.ty.clone(),
        };
        // A declared `opt<T>` is nullable: box the declaration (`Long`,
        // not `long`) so `= null` compiles.
        let decl = if ty.as_ref().is_some_and(is_opt_ty) {
            jt.boxed()
        } else {
            jt.decl()
        };
        // Re-emit list/map literals against the declared type so empty
        // literals get a concrete element type (see gen_expr_hinted).
        let v = match (&value.kind, &jt) {
            (ExprKind::List(_), JTy::List(_)) | (ExprKind::Map(_), JTy::Map(_, _)) => {
                self.gen_expr_hinted(value, Some(&jt))?
            }
            _ => v,
        };
        // A bare `none` or empty list with no annotation has no Java type.
        if matches!(v.code.as_str(), "null") && ty.is_none() {
            return Err(format!(
                "`let {}` = none needs a type annotation (`: opt<T>`); refusing",
                name
            ));
        }
        self.line(indent, &format!("{} {} = {};", decl, name, v.code));
        self.scope.insert(name.to_string(), jt);
        Ok(())
    }

    /// Best-effort Java type for an unwrapped expression when the binding has
    /// no annotation: calls use the callee's declared return type.
    fn infer_unwrap_ty(&self, expr: &Expr) -> Result<String, String> {
        if let ExprKind::Call { callee, .. } = &expr.kind {
            if let ExprKind::Ident(fname) = &callee.kind {
                if let Some(info) = self.fn_info.get(fname) {
                    return Ok(jty(&info.ret)?.boxed());
                }
            }
        }
        Err("`??` binding needs a type annotation here; refusing".into())
    }

    /// Recover a JTy from a declaration string produced by `boxed()`. Only
    /// used for scope bookkeeping after `??` bindings.
    fn jty_of_decl(&self, decl: &str) -> JTy {
        match decl {
            "Long" => JTy::Long,
            "Double" => JTy::Double,
            "Boolean" => JTy::Bool,
            "String" => JTy::Str,
            d if d.starts_with("java.util.List<") => JTy::List(Box::new(JTy::Named("?".into()))),
            d if d.starts_with("java.util.Map<") => {
                JTy::Map(Box::new(JTy::Named("?".into())), Box::new(JTy::Named("?".into())))
            }
            other => JTy::Named(other.to_string()),
        }
    }

    fn gen_ret(&mut self, indent: usize, value: &Option<Expr>) -> Result<(), String> {
        let cur = match &self.cur_fn {
            None => {
                // Top-level `ret`: main() returns void. The value is pure
                // (CuNi has no side-effecting expressions — `say`/`.push`
                // are statements), so it is discarded entirely, matching
                // the go seat's `_ = expr` without emitting a non-statement
                // like `(-1L);`, which javac would reject.
                if let Some(e) = value {
                    let v = self.gen_expr(e)?;
                    self.line(
                        indent,
                        &format!("// top-level `ret` value discarded (pure): {}", v.code),
                    );
                }
                self.line(indent, "return;");
                return Ok(());
            }
            Some(c) => c.clone(),
        };
        match value {
            Some(e) => {
                let v = self.gen_expr(e)?;
                self.line(indent, &format!("return {};", v.code));
                Ok(())
            }
            None => {
                // Bare `ret`: fill the declared type's zero value (the go
                // seat does the same; py returns None, which no passing
                // program can observe).
                let zero = jty(&cur.0)?.zero();
                self.line(indent, &format!("return {}; // bare `ret` — zero value filled in", zero));
                Ok(())
            }
        }
    }

    fn gen_for(
        &mut self,
        indent: usize,
        a: &str,
        b: Option<&str>,
        iter: &Expr,
        body: &[Stmt],
    ) -> Result<(), String> {
        let it = self.gen_expr(iter)?;
        let elem_ty: JTy = match &it.ty {
            JTy::List(e) => (**e).clone(),
            JTy::Map(k, _) => (**k).clone(),
            JTy::Str => {
                // Iterating a string yields 1-char strings (py semantics).
                JTy::Str
            }
            _ => {
                return Err(format!(
                    "`for` needs a list, map, or string to iterate; refusing (got {})",
                    it.ty.decl()
                ))
            }
        };
        match b {
            Some(b) => match &it.ty {
                JTy::List(_) => {
                    // `for i, x in xs` — index + value, like py's enumerate.
                    // The counter is `long` (CuNi ints are i64); the
                    // `.get` cast is exact for any real list length.
                    self.line(
                        indent,
                        &format!("for (long {0} = 0; {0} < {1}.size(); {0}++) {{", a, it.code),
                    );
                    self.line(
                        indent + 1,
                        &format!("{} {} = {}.get((int) ({}));", elem_ty.decl(), b, it.code, a),
                    );
                    self.scope.insert(a.to_string(), JTy::Long);
                    self.scope.insert(b.to_string(), elem_ty);
                    self.gen_block(indent + 1, body)?;
                    self.line(indent, "}");
                    Ok(())
                }
                JTy::Map(k, v) => {
                    let e = self.fresh_tmp();
                    self.line(
                        indent,
                        &format!(
                            "for (java.util.Map.Entry<{}, {}> {} : {}.entrySet()) {{",
                            k.boxed(),
                            v.boxed(),
                            e,
                            it.code
                        ),
                    );
                    self.line(indent + 1, &format!("{} {} = {}.getKey();", k.decl(), a, e));
                    self.line(indent + 1, &format!("{} {} = {}.getValue();", v.decl(), b, e));
                    self.scope.insert(a.to_string(), (**k).clone());
                    self.scope.insert(b.to_string(), (**v).clone());
                    self.gen_block(indent + 1, body)?;
                    self.line(indent, "}");
                    Ok(())
                }
                _ => Err("two-binding `for` needs a list or map; refusing".into()),
            },
            None => match &it.ty {
                JTy::Map(_, _) => {
                    // py's `for k in m` iterates keys.
                    self.line(indent, &format!("for ({} {} : {}.keySet()) {{", elem_ty.decl(), a, it.code));
                    self.scope.insert(a.to_string(), elem_ty);
                    self.gen_block(indent + 1, body)?;
                    self.line(indent, "}");
                    Ok(())
                }
                JTy::Str => {
                    // Iterate 1-char substrings.
                    let n = self.fresh_tmp();
                    self.line(indent, &format!("for (int {0} = 0; {0} < {1}.length(); {0}++) {{", n, it.code));
                    self.line(
                        indent + 1,
                        &format!(
                            "String {} = {}.substring({}, {} + 1);",
                            a, it.code, n, n
                        ),
                    );
                    self.scope.insert(a.to_string(), JTy::Str);
                    self.gen_block(indent + 1, body)?;
                    self.line(indent, "}");
                    Ok(())
                }
                _ => {
                    self.line(
                        indent,
                        &format!("for ({} {} : {}) {{", elem_ty.decl(), a, it.code),
                    );
                    self.scope.insert(a.to_string(), elem_ty);
                    self.gen_block(indent + 1, body)?;
                    self.line(indent, "}");
                    Ok(())
                }
            },
        }
    }

    /// Coerce a boolean-typed expression for `if`/`while` conditions. CuNi
    /// booleans are always primitive `boolean` here; a boxed `Boolean`
    /// (from a generic or `opt`) is unboxed explicitly so javac never has
    /// to guess.
    fn as_bool(&self, v: &JExpr) -> Result<String, String> {
        match &v.ty {
            JTy::Bool => Ok(v.code.clone()),
            _ => Err("condition must be a bool; refusing".into()),
        }
    }

    fn gen_expr(&mut self, expr: &Expr) -> Result<JExpr, String> {
        self.gen_expr_hinted(expr, None)
    }

    /// Like `gen_expr`, but a `let`/`mut` annotation can supply the expected
    /// Java type so an *empty* list literal (`mut names: list<str> = []`)
    /// still gets a concrete `ArrayList<String>` instead of
    /// `ArrayList<Object>`, which would not compile against the declared
    /// `java.util.List<String>`.
    fn gen_expr_hinted(&mut self, expr: &Expr, hint: Option<&JTy>) -> Result<JExpr, String> {
        self.gen_expr_inner(expr, hint)
    }

    fn gen_expr_inner(&mut self, expr: &Expr, hint: Option<&JTy>) -> Result<JExpr, String> {
        match &expr.kind {
            ExprKind::Int(n) => Ok(JExpr {
                // Always `L`-suffixed: CuNi ints are i64 and a literal past
                // i32 range would not compile as a Java `int` literal.
                code: format!("{}L", n),
                ty: JTy::Long,
            }),
            // Scaled BigInteger literal (docs/DECIMAL.md §2).
            ExprKind::Dec(s) => Ok(JExpr {
                code: format!("new java.math.BigInteger(\"{s}\")"),
                ty: JTy::Dec,
            }),
            // Epoch `long` literal (docs/TIME.md §2): always `L`-suffixed.
            ExprKind::Time(e) => Ok(JExpr {
                code: format!("{e}L"),
                ty: JTy::Time,
            }),
            ExprKind::Float(f) => Ok(JExpr {
                // Same decimal text the py seat emits (`f.to_string()`), so
                // both parse to the identical IEEE double.
                code: format!("{}", f),
                ty: JTy::Double,
            }),
            ExprKind::Bool(b) => Ok(JExpr {
                code: b.to_string(),
                ty: JTy::Bool,
            }),
            ExprKind::Str(s) => Ok(JExpr {
                code: Self::jstr(s),
                ty: JTy::Str,
            }),
            ExprKind::InterpStr(parts) => {
                // `"" + cuni_str(a) + "text" + ...` — cuni_str gives every
                // value (bool -> True/False included) its `say` spelling.
                let mut out = String::from("\"\"");
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => {
                            out.push_str(" + ");
                            out.push_str(&Self::jstr(t));
                        }
                        StrPartExpr::Expr(e) => {
                            let v = self.gen_expr(e)?;
                            // A `time` renders as ISO-8601, not as its raw
                            // epoch `long` (docs/TIME.md §4).
                            if matches!(v.ty, JTy::Time) {
                                out.push_str(&format!(" + cuni_time_str({})", v.code));
                            } else {
                                out.push_str(&format!(" + cuni_str({})", v.code));
                            }
                        }
                    }
                }
                Ok(JExpr {
                    code: out,
                    ty: JTy::Str,
                })
            }
            ExprKind::NoneLit => Ok(JExpr {
                code: "null".to_string(),
                // Untyped null; callers needing a type must supply one
                // (see gen_binding's annotation requirement).
                ty: JTy::Named("Object".into()),
            }),
            ExprKind::Ident(name) => {
                if name == "true" || name == "false" {
                    return Ok(JExpr {
                        code: name.clone(),
                        ty: JTy::Bool,
                    });
                }
                let ty = self
                    .scope
                    .get(name)
                    .cloned()
                    .or_else(|| {
                        // A generic type variable in scope (e.g. comparing
                        // against `T`) is an Object for `==` routing.
                        self.tvars.contains(name).then(|| JTy::TVar(name.clone()))
                    })
                    .unwrap_or(JTy::Named("Object".into()));
                Ok(JExpr {
                    code: name.clone(),
                    ty,
                })
            }
            ExprKind::List(items) => {
                let mut elem_ty: Option<JTy> = None;
                // A declared `list<T>` pins the element type (matters for
                // empty literals, which have no elements to infer from).
                if let Some(JTy::List(e)) = hint {
                    elem_ty = Some((**e).clone());
                }
                let mut codes = Vec::new();
                for e in items {
                    let v = self.gen_expr(e)?;
                    if elem_ty.is_none() {
                        elem_ty = Some(v.ty.clone());
                    }
                    codes.push(v.code);
                }
                let et = elem_ty.unwrap_or(JTy::Named("Object".into()));
                Ok(JExpr {
                    code: format!(
                        "new java.util.ArrayList<>(java.util.Arrays.<{}>asList({}))",
                        et.boxed(),
                        codes.join(", ")
                    ),
                    ty: JTy::List(Box::new(et)),
                })
            }
            ExprKind::Map(pairs) => {
                let tmp = self.fresh_tmp();
                let mut kt: Option<JTy> = None;
                let mut first_vt: Option<JTy> = None;
                let mut hetero = false;
                let mut puts = Vec::new();
                for (k, v) in pairs {
                    let kk = self.gen_expr(k)?;
                    let vv = self.gen_expr(v)?;
                    if kt.is_none() {
                        kt = Some(kk.ty.clone());
                    }
                    match &first_vt {
                        None => first_vt = Some(vv.ty.clone()),
                        Some(t) => {
                            if *t != vv.ty {
                                hetero = true;
                            }
                        }
                    }
                    // Puts must target this literal's own temp, not a
                    // hardcoded name (a user variable could be named `m`).
                    puts.push(format!("{tmp}.put({}, {});", kk.code, vv.code));
                }
                let kt = kt.unwrap_or(JTy::Named("Object".into()));
                // Heterogeneous value types widen to Object: first-pair-only
                // would emit uncompilable puts for e.g. {"z": 1, "a": {...}}.
                let vt = if hetero {
                    JTy::Named("Object".into())
                } else {
                    first_vt.unwrap_or(JTy::Named("Object".into()))
                };
                // Double-brace would work but leaks a class per literal; a
                // block-scoped temp map is plain and honest.
                Ok(JExpr {
                    code: format!(
                        "new java.util.function.Supplier<java.util.Map<{}, {}>>() {{ public java.util.Map<{}, {}> get() {{ java.util.Map<{}, {}> {} = new java.util.HashMap<>(); {} return {}; }} }}.get()",
                        kt.boxed(),
                        vt.boxed(),
                        kt.boxed(),
                        vt.boxed(),
                        kt.boxed(),
                        vt.boxed(),
                        tmp,
                        puts.join(" "),
                        tmp
                    ),
                    ty: JTy::Map(Box::new(kt), Box::new(vt)),
                })
            }
            ExprKind::Call { callee, args } => self.gen_call(callee, args),
            ExprKind::Index { base, index } => {
                let b = self.gen_expr(base)?;
                let ix = self.gen_expr(index)?;
                match &b.ty {
                    JTy::List(e) => Ok(JExpr {
                        code: format!("{}.get((int) ({}))", b.code, ix.code),
                        ty: (**e).clone(),
                    }),
                    JTy::Map(_, v) => Ok(JExpr {
                        code: format!("{}.get({})", b.code, ix.code),
                        ty: (**v).clone(),
                    }),
                    JTy::Str => Ok(JExpr {
                        // py's `"abc"[1]` is `"b"` (a 1-char str), not a
                        // char — wrap charAt so the type stays String.
                        code: format!("String.valueOf({}.charAt((int) ({})))", b.code, ix.code),
                        ty: JTy::Str,
                    }),
                    _ => Err(format!(
                        "indexing needs a list, map, or string target (got {}); refusing",
                        b.ty.decl()
                    )),
                }
            }
            ExprKind::Field { base, name } => {
                // `EnumName.Variant` passes through; everything else is a
                // field access on the base expression.
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.enum_names.contains(base_name) {
                        return Ok(JExpr {
                            code: format!("{}.{}", base_name, name),
                            ty: JTy::Named(base_name.clone()),
                        });
                    }
                }
                let b = self.gen_expr(base)?;
                Ok(JExpr {
                    code: format!("{}.{}", b.code, name),
                    // Field type is unknown without a typ-table lookup by
                    // base type; default to Object (only `==` routing cares,
                    // and Object routes to Objects.equals, which is safe).
                    ty: JTy::Named("Object".into()),
                })
            }
            ExprKind::Binary { op, lhs, rhs } => self.gen_binary(*op, lhs, rhs),
            ExprKind::Unary { op, expr } => {
                let v = self.gen_expr(expr)?;
                match op {
                    UnOp::Not => Ok(JExpr {
                        code: format!("(!{})", v.code),
                        ty: JTy::Bool,
                    }),
                    UnOp::Neg => {
                        if matches!(v.ty, JTy::Dec) {
                            Ok(JExpr {
                                code: format!("({}.negate())", v.code),
                                ty: JTy::Dec,
                            })
                        } else if matches!(v.ty, JTy::Time) {
                            // Math.negateExact throws on overflow — the loud refusal.
                            Ok(JExpr {
                                code: format!("(Math.negateExact({}))", v.code),
                                ty: JTy::Time,
                            })
                        } else {
                            Ok(JExpr {
                                code: format!("(-{})", v.code),
                                ty: v.ty,
                            })
                        }
                    }
                }
            }
            ExprKind::Unwrap { .. } => Err(
                "`??` outside a `let`/`mut` binding has no Java shape; refusing (bind it first)"
                    .into(),
            ),
        }
    }

    fn gen_call(&mut self, callee: &Expr, args: &[CallArg]) -> Result<JExpr, String> {
        // Builtin free functions first.
        if let ExprKind::Ident(fname) = &callee.kind {
            // `dec` explicit conversions (docs/DECIMAL.md §5).
            if fname == "dec_of_int" {
                let a = args
                    .first()
                    .ok_or("dec_of_int needs one argument; refusing")?;
                let v = self.gen_expr(a.expr())?;
                return Ok(JExpr {
                    code: format!(
                        "(java.math.BigInteger.valueOf({}).multiply(CUNI_DEC_SCALE))",
                        v.code
                    ),
                    ty: JTy::Dec,
                });
            }
            if fname == "int_of_dec" {
                let a = args
                    .first()
                    .ok_or("int_of_dec needs one argument; refusing")?;
                let v = self.gen_expr(a.expr())?;
                return Ok(JExpr {
                    // Truncates toward zero; longValueExact throws loudly on
                    // overflow instead of silently wrapping.
                    code: format!("(({}.divide(CUNI_DEC_SCALE)).longValueExact())", v.code),
                    ty: JTy::Long,
                });
            }
            // `time` builtins (docs/TIME.md §5).
            if fname == "parse_time" {
                let a = args
                    .first()
                    .ok_or("parse_time needs one argument; refusing")?;
                let v = self.gen_expr(a.expr())?;
                return Ok(JExpr {
                    code: format!("(cuni_parse_time({}))", v.code),
                    ty: JTy::Time,
                });
            }
            if fname == "add_seconds" {
                let a = args.first().ok_or("add_seconds needs two arguments; refusing")?;
                let b = args.get(1).ok_or("add_seconds needs two arguments; refusing")?;
                let x = self.gen_expr(a.expr())?;
                let y = self.gen_expr(b.expr())?;
                return Ok(JExpr {
                    code: format!("(cuni_add_seconds({}, {}))", x.code, y.code),
                    ty: JTy::Time,
                });
            }
            if fname == "days_between" {
                let a = args.first().ok_or("days_between needs two arguments; refusing")?;
                let b = args.get(1).ok_or("days_between needs two arguments; refusing")?;
                let x = self.gen_expr(a.expr())?;
                let y = self.gen_expr(b.expr())?;
                return Ok(JExpr {
                    code: format!("(cuni_days_between({}, {}))", x.code, y.code),
                    ty: JTy::Long,
                });
            }
            let mapped = match fname.as_str() {
                "range" => Some("cuni_range"),
                "abs" => Some("cuni_abs"),
                "min" => Some("cuni_min"),
                "max" => Some("cuni_max"),
                "sha256" => Some("cuniSha256"),
                "say" => None, // handled below (needs arg-count check)
                _ => None,
            };
            if fname == "say" {
                // `say` is a statement, never a value (py's `say` returns
                // None and no passing program observes it). Statement
                // position is handled in gen_stmt's ExprStmt arm.
                return Err("`say` used as an expression has no Java value; refusing — use it as a statement".into());
            }
            if let Some(m) = mapped {
                let av: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr()).map(|e| e.code))
                    .collect::<Result<_, _>>()?;
                let ty = match fname.as_str() {
                    "range" => JTy::List(Box::new(JTy::Long)),
                    "sha256" => JTy::Str,
                    _ => JTy::Long,
                };
                return Ok(JExpr {
                    code: format!("{}({})", m, av.join(", ")),
                    ty,
                });
            }
            // Typ constructor: positional or named -> `new Point(...)`.
            // Field names are cloned up front: `self.gen_expr` needs
            // `&mut self`, so the `&self.typ_fields` borrow can't live
            // across the argument loop.
            if self.typ_fields.contains_key(fname) {
                let fields: Vec<String> = self.typ_fields[fname]
                    .iter()
                    .map(|(n, _)| n.clone())
                    .collect();
                let mut vals: Vec<String> = Vec::new();
                if args.iter().all(|a| a.is_named()) && !args.is_empty() {
                    for fname2 in &fields {
                        let found = args.iter().find_map(|a| match a {
                            CallArg::Named { name, value, .. } if name == fname2 => Some(value),
                            _ => None,
                        });
                        match found {
                            Some(v) => vals.push(self.gen_expr(v)?.code),
                            None => {
                                return Err(format!(
                                    "named constructor `{}` is missing field `{}`; refusing",
                                    fname, fname2
                                ))
                            }
                        }
                    }
                } else {
                    for a in args {
                        vals.push(self.gen_expr(a.expr())?.code);
                    }
                }
                return Ok(JExpr {
                    code: format!("new {}({})", fname, vals.join(", ")),
                    ty: JTy::Named(fname.clone()),
                });
            }
            // Ordinary function call.
            if self.fn_info.contains_key(fname) {
                let ret_jt = {
                    let info = &self.fn_info[fname];
                    jty(&info.ret)?
                };
                let av: Vec<String> = args
                    .iter()
                    .map(|a| self.gen_expr(a.expr()).map(|e| e.code))
                    .collect::<Result<_, _>>()?;
                return Ok(JExpr {
                    code: format!("{}({})", fname, av.join(", ")),
                    ty: ret_jt,
                });
            }
            return Err(format!("call of unknown function `{}`; refusing", fname));
        }
        // Method calls: only the portable builtins have a Java shape.
        if let ExprKind::Field { base, name } = &callee.kind {
            // Wave-1 stdlib namespaces (docs/STDLIB.md): `json`/`time` are
            // reserved identifiers, so an Ident base here is a namespace.
            if let ExprKind::Ident(ns) = &base.kind {
                if ns == "json" || ns == "time" {
                    let av: Vec<String> = args
                        .iter()
                        .map(|a| self.gen_expr(a.expr()).map(|e| e.code))
                        .collect::<Result<_, _>>()?;
                    let obj_map =
                        || JTy::Map(Box::new(JTy::Str), Box::new(JTy::TVar("Object".into())));
                    let (code, ty) = match (ns.as_str(), name.as_str()) {
                        ("json", "parse") => {
                            (format!("cuniJsonParse({})", av.join(", ")), obj_map())
                        }
                        ("json", "emit") => {
                            (format!("cuniJsonEmit({})", av.join(", ")), JTy::Str)
                        }
                        ("time", "epoch") => {
                            (format!("cuniTimeEpoch({})", av.join(", ")), JTy::Long)
                        }
                        ("time", "parts") => {
                            (format!("cuniTimeParts({})", av.join(", ")), obj_map())
                        }
                        _ => {
                            return Err(format!(
                                "unknown stdlib function `{}.{}`; refusing",
                                ns, name
                            ))
                        }
                    };
                    return Ok(JExpr { code, ty });
                }
            }
            let b = self.gen_expr(base)?;
            let av: Vec<String> = args
                .iter()
                .map(|a| self.gen_expr(a.expr()).map(|e| e.code))
                .collect::<Result<_, _>>()?;
            match name.as_str() {
                "len" => {
                    if !args.is_empty() {
                        return Err("`.len` takes no arguments; refusing".into());
                    }
                    let (code, ty) = match &b.ty {
                        JTy::List(_) | JTy::Map(_, _) => (format!("{}.size()", b.code), JTy::Long),
                        JTy::Str => (format!("{}.length()", b.code), JTy::Long),
                        _ => {
                            return Err(format!(
                                "`.len` needs a list, map, or string (got {}); refusing",
                                b.ty.decl()
                            ))
                        }
                    };
                    // py's len() returns int; keep it long.
                    return Ok(JExpr { code, ty });
                }
                "slice" => {
                    if args.len() != 2 {
                        return Err("`.slice` takes exactly two arguments; refusing".into());
                    }
                    match &b.ty {
                        JTy::List(e) => {
                            let et = (**e).clone();
                            return Ok(JExpr {
                                code: format!(
                                    "cuni_slice({}, {}, {})",
                                    b.code, av[0], av[1]
                                ),
                                ty: JTy::List(Box::new(et)),
                            });
                        }
                        JTy::Str => {
                            return Ok(JExpr {
                                code: format!(
                                    "cuni_slice({}, {}, {})",
                                    b.code, av[0], av[1]
                                ),
                                ty: JTy::Str,
                            })
                        }
                        _ => {
                            return Err(format!(
                                "`.slice` needs a list or string (got {}); refusing",
                                b.ty.decl()
                            ))
                        }
                    }
                }
                "push" => {
                    // `.push` as an *expression* would be Java's boolean
                    // `add` return vs py's `None` — a different value, so
                    // this seat refuses it here; statement position is
                    // rewritten in gen_stmt instead.
                    return Err(
                        "`.push` used as an expression has no honest Java value (Java `add` returns boolean, py returns None); refusing — use it as a statement"
                            .into(),
                    );
                }
                // Wave-1 string ops (docs/STDLIB.md §3).
                "split" => {
                    if args.len() != 1 {
                        return Err("`.split` takes exactly one argument; refusing".into());
                    }
                    if !matches!(b.ty, JTy::Str) {
                        return Err(format!(
                            "`.split` needs a string target (got {}); refusing",
                            b.ty.decl()
                        ));
                    }
                    return Ok(JExpr {
                        code: format!("cuniSplit({}, {})", b.code, av[0]),
                        ty: JTy::List(Box::new(JTy::Str)),
                    });
                }
                "join" => {
                    if args.len() != 1 {
                        return Err("`.join` takes exactly one argument; refusing".into());
                    }
                    if !matches!(b.ty, JTy::Str) {
                        return Err(format!(
                            "`.join` needs a string separator (got {}); refusing",
                            b.ty.decl()
                        ));
                    }
                    return Ok(JExpr {
                        code: format!("cuniJoin({}, {})", b.code, av[0]),
                        ty: JTy::Str,
                    });
                }
                "trim" => {
                    if !args.is_empty() {
                        return Err("`.trim` takes no arguments; refusing".into());
                    }
                    if !matches!(b.ty, JTy::Str) {
                        return Err(format!(
                            "`.trim` needs a string target (got {}); refusing",
                            b.ty.decl()
                        ));
                    }
                    return Ok(JExpr {
                        code: format!("cuniTrim({})", b.code),
                        ty: JTy::Str,
                    });
                }
                "contains" => {
                    if args.len() != 1 {
                        return Err("`.contains` takes exactly one argument; refusing".into());
                    }
                    if !matches!(b.ty, JTy::Str) {
                        return Err(format!(
                            "`.contains` needs a string target (got {}); refusing",
                            b.ty.decl()
                        ));
                    }
                    return Ok(JExpr {
                        code: format!("cuniContains({}, {})", b.code, av[0]),
                        ty: JTy::Bool,
                    });
                }
                _ => {
                    return Err(format!(
                        "unknown method `.{}()` has no Java mapping; refusing",
                        name
                    ))
                }
            }
        }
        Err("computed callee expressions have no Java mapping; refusing".into())
    }

    fn gen_binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Result<JExpr, String> {
        let l = self.gen_expr(lhs)?;
        let r = self.gen_expr(rhs)?;
        // `time` is a closed world (docs/TIME.md §3–5): the typeck proved the
        // valid shapes (`time ± int`, `time − time`, `time` comparisons);
        // anything else is a loud refusal. `Math.addExact`/`subtractExact`/
        // `negateExact` throw ArithmeticException on overflow (narrow-seat
        // envelope); `/` truncates toward zero natively.
        if matches!(l.ty, JTy::Time) || matches!(r.ty, JTy::Time) {
            let is_time = |t: &JTy| matches!(t, JTy::Time);
            let is_long = |t: &JTy| matches!(t, JTy::Long);
            let (code, ty) = match op {
                BinOp::Add
                    if (is_time(&l.ty) && is_long(&r.ty))
                        || (is_long(&l.ty) && is_time(&r.ty)) =>
                {
                    (format!("(Math.addExact({}, {}))", l.code, r.code), JTy::Time)
                }
                BinOp::Sub if is_time(&l.ty) && is_long(&r.ty) => {
                    (format!("(Math.subtractExact({}, {}))", l.code, r.code), JTy::Time)
                }
                BinOp::Sub if is_time(&l.ty) && is_time(&r.ty) => {
                    (format!("(Math.subtractExact({}, {}))", l.code, r.code), JTy::Long)
                }
                BinOp::Eq if is_time(&l.ty) && is_time(&r.ty) => {
                    (format!("({} == {})", l.code, r.code), JTy::Bool)
                }
                BinOp::Ne if is_time(&l.ty) && is_time(&r.ty) => {
                    (format!("({} != {})", l.code, r.code), JTy::Bool)
                }
                BinOp::Lt if is_time(&l.ty) && is_time(&r.ty) => {
                    (format!("({} < {})", l.code, r.code), JTy::Bool)
                }
                BinOp::Gt if is_time(&l.ty) && is_time(&r.ty) => {
                    (format!("({} > {})", l.code, r.code), JTy::Bool)
                }
                BinOp::Le if is_time(&l.ty) && is_time(&r.ty) => {
                    (format!("({} <= {})", l.code, r.code), JTy::Bool)
                }
                BinOp::Ge if is_time(&l.ty) && is_time(&r.ty) => {
                    (format!("({} >= {})", l.code, r.code), JTy::Bool)
                }
                _ => {
                    return Err(
                        "time binary op shape rejected by codegen — the typeck should have refused it first; refusing"
                            .into(),
                    )
                }
            };
            return Ok(JExpr { code, ty });
        }
        // `dec` is a closed world (docs/DECIMAL.md §3–5): both operands dec,
        // or a loud refusal. The typeck already rejected mixes; this is
        // defense in depth. BigInteger ops are exact; divide() truncates
        // toward zero natively.
        if matches!(l.ty, JTy::Dec) || matches!(r.ty, JTy::Dec) {
            if !matches!(l.ty, JTy::Dec) || !matches!(r.ty, JTy::Dec) {
                return Err(
                    "cannot mix dec and non-dec — convert explicitly (`dec_of_int` / `int_of_dec`)"
                        .into(),
                );
            }
            let (code, ty) = match op {
                BinOp::Add => (format!("({}.add({}))", l.code, r.code), JTy::Dec),
                BinOp::Sub => (format!("({}.subtract({}))", l.code, r.code), JTy::Dec),
                BinOp::Mul => (
                    format!("({}.multiply({}).divide(CUNI_DEC_SCALE))", l.code, r.code),
                    JTy::Dec,
                ),
                BinOp::Div => (format!("cuni_dec_div({}, {})", l.code, r.code), JTy::Dec),
                BinOp::Mod => return Err("`%` is not defined on `dec`; refusing".into()),
                BinOp::Eq => (
                    format!("java.util.Objects.equals({}, {})", l.code, r.code),
                    JTy::Bool,
                ),
                BinOp::Ne => (
                    format!("(!java.util.Objects.equals({}, {}))", l.code, r.code),
                    JTy::Bool,
                ),
                BinOp::Lt => (format!("({}.compareTo({}) < 0)", l.code, r.code), JTy::Bool),
                BinOp::Gt => (format!("({}.compareTo({}) > 0)", l.code, r.code), JTy::Bool),
                BinOp::Le => (format!("({}.compareTo({}) <= 0)", l.code, r.code), JTy::Bool),
                BinOp::Ge => (format!("({}.compareTo({}) >= 0)", l.code, r.code), JTy::Bool),
                BinOp::And | BinOp::Or => {
                    return Err("`and`/`or` need booleans; refusing".into())
                }
            };
            return Ok(JExpr { code, ty });
        }
        let is_double = matches!(l.ty, JTy::Double) || matches!(r.ty, JTy::Double);
        let is_str = matches!(l.ty, JTy::Str) || matches!(r.ty, JTy::Str);
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                if is_str && matches!(op, BinOp::Add) {
                    // String concat via + (only when a side is a String).
                    return Ok(JExpr {
                        code: format!("({} + {})", l.code, r.code),
                        ty: JTy::Str,
                    });
                }
                if is_str {
                    return Err("only `+` is defined on strings; refusing".into());
                }
                let ty = if is_double { JTy::Double } else { JTy::Long };
                let o = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    _ => "*",
                };
                Ok(JExpr {
                    code: format!("({} {} {})", l.code, o, r.code),
                    ty,
                })
            }
            BinOp::Div => {
                if is_str {
                    return Err("`/` is not defined on strings; refusing".into());
                }
                if is_double {
                    // Zero-guarded to throw like Python instead of yielding
                    // Infinity.
                    Ok(JExpr {
                        code: format!("cuni_div({}, {})", l.code, r.code),
                        ty: JTy::Double,
                    })
                } else {
                    // long/long: truncated toward zero — exactly CuNi's
                    // `_cuni_div`; zero divisor throws on both seats.
                    Ok(JExpr {
                        code: format!("({} / {})", l.code, r.code),
                        ty: JTy::Long,
                    })
                }
            }
            BinOp::Mod => {
                if is_str {
                    return Err("`%` is not defined on strings; refusing".into());
                }
                let ty = if is_double { JTy::Double } else { JTy::Long };
                Ok(JExpr {
                    code: format!("cuni_mod({}, {})", l.code, r.code),
                    ty,
                })
            }
            BinOp::Eq | BinOp::Ne => {
                let neg = matches!(op, BinOp::Ne);
                // Reference types (incl. type variables and boxed numerics)
                // must not use `==`: it compares references in Java.
                let code = if l.ty.is_primitive() && r.ty.is_primitive() {
                    // Both primitive — but mixed long/double is fine with
                    // ==; bool/bool fine. A primitive vs its boxed form
                    // can't happen here (unboxing handled by javac when one
                    // side is primitive... actually if l is long and r is
                    // Long, `l == r` unboxes r — exact). Keep ==.
                    if neg {
                        format!("({} != {})", l.code, r.code)
                    } else {
                        format!("({} == {})", l.code, r.code)
                    }
                } else {
                    // At least one side is a reference: Objects.equals.
                    // Mixed primitive/object (long vs Long) unboxes fine
                    // under ==, but routing everything non-trivial through
                    // Objects.equals is uniformly exact (it unboxes via
                    // .equals on the boxed value).
                    let eq = format!(
                        "java.util.Objects.equals({}, {})",
                        l.code, r.code
                    );
                    if neg {
                        format!("(!{})", eq)
                    } else {
                        eq
                    }
                };
                Ok(JExpr {
                    code,
                    ty: JTy::Bool,
                })
            }
            BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge => {
                if is_str {
                    // py compares strings lexicographically; compareTo does
                    // the same on UTF-16 units (== code points for BMP).
                    let o = match op {
                        BinOp::Lt => "<",
                        BinOp::Gt => ">",
                        BinOp::Le => "<=",
                        _ => ">=",
                    };
                    return Ok(JExpr {
                        code: format!("({}.compareTo({}) {} 0)", l.code, r.code, o),
                        ty: JTy::Bool,
                    });
                }
                if !l.ty.is_primitive() || !r.ty.is_primitive() {
                    return Err("ordering comparisons need numeric operands; refusing".into());
                }
                let o = match op {
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::Le => "<=",
                    _ => ">=",
                };
                Ok(JExpr {
                    code: format!("({} {} {})", l.code, o, r.code),
                    ty: JTy::Bool,
                })
            }
            BinOp::And | BinOp::Or => {
                let o = if matches!(op, BinOp::And) { "&&" } else { "||" };
                Ok(JExpr {
                    code: format!("({} {} {})", l.code, o, r.code),
                    ty: JTy::Bool,
                })
            }
        }
    }
}

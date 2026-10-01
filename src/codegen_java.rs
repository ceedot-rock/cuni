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
#[derive(Clone, Debug)]
enum JTy {
    Long,
    /// CuNi `dec`: fixed-point decimal, scale 10⁴, exact (docs/DECIMAL.md).
    /// `java.math.BigInteger` — arbitrary precision, divide() truncates
    /// toward zero natively. Wide seat: no range refusal.
    Dec,
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
            JTy::Double => "Double".into(),
            JTy::Bool => "Boolean".into(),
            v => v.decl(),
        }
    }
    fn is_primitive(&self) -> bool {
        matches!(self, JTy::Long | JTy::Double | JTy::Bool)
    }
    fn zero(&self) -> String {
        match self {
            JTy::Long => "0L".into(),
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
        self.line(1, "static void say(Object x) {");
        self.line(2, "if (x == null) {");
        self.line(3, "System.out.println(\"None\");");
        self.line(2, "} else if (x instanceof Boolean) {");
        self.line(3, "System.out.println(((Boolean) x).booleanValue() ? \"True\" : \"False\");");
        // A `dec` is a scaled BigInteger (docs/DECIMAL.md) — it must render
        // canonically, not as its raw scaled integer. No other CuNi value
        // is a BigInteger, so this changes nothing else.
        self.line(2, "} else if (x instanceof java.math.BigInteger) {");
        self.line(3, "System.out.println(cuni_dec_str((java.math.BigInteger) x));");
        self.line(2, "} else {");
        self.line(3, "System.out.println(x);");
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
                            self.line(indent, &format!("say({});", v.code));
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
                            out.push_str(&format!(" + cuni_str({})", v.code));
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
                let mut kty: Option<JTy> = None;
                let mut vty: Option<JTy> = None;
                let mut puts = Vec::new();
                for (k, v) in pairs {
                    let kk = self.gen_expr(k)?;
                    let vv = self.gen_expr(v)?;
                    if kty.is_none() {
                        kty = Some(kk.ty.clone());
                    }
                    if vty.is_none() {
                        vty = Some(vv.ty.clone());
                    }
                    puts.push(format!("m.put({}, {});", kk.code, vv.code));
                }
                let kt = kty.unwrap_or(JTy::Named("Object".into()));
                let vt = vty.unwrap_or(JTy::Named("Object".into()));
                let tmp = self.fresh_tmp();
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
            let mapped = match fname.as_str() {
                "range" => Some("cuni_range"),
                "abs" => Some("cuni_abs"),
                "min" => Some("cuni_min"),
                "max" => Some("cuni_max"),
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

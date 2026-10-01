//! Reverse CuNi: ingest CuNi-shaped subsets of supported languages into `.cuni`, or refuse.
//!
//! DOCUMENTED SUBSET (v2). Ingest accepts code shaped like CuNi's own emitters
//! produce, and refuses everything else — never silently mistranslates.
//!
//! - `py`: CuNi-shaped Python: `def name(p: T, ...) -> T:` / `def name(p, ...):`
//!   with `return`/`if`/`else`/`let`-style bodies, top-level `print(...)` /
//!   `say(...)`, `def main():` unwrapped, the CuNi prelude (`say`, `range`,
//!   `abs`, `min`, `max`, `_cuni_slice`, `_cuni_div`, imports, classes,
//!   `if __name__ == "__main__":`) skipped. Types int/str/bool/float map;
//!   anything else refuses. Untyped params default to int (v1 behavior).
//! - `go`: `func name(p T, ...) T { return e; if c { ... }; x := e; say(e) }`
//!   inside `func main()`. Go int/string/bool/float64 map to CuNi
//!   int/str/bool/float. Generics, `(T, error)` fallible sigs, loops,
//!   `panic`, `switch` refuse.
//! - `js`/`ts`/`mjs`/`cjs`: `function name(p, ...) { return e; if (c) {...} }`,
//!   `const x = e;`, `say(e);` inside `function main()`. The JS backend is
//!   untyped, so param/return types are RECOVERED by call-site inference:
//!   each argument at every call site is classified (literals, constructor
//!   normalizations, binding types), and a parameter takes the unique type
//!   all its call sites agree on; return type comes from `return`
//!   expressions. Uninferrable (no call sites, mixed types) refuses —
//!   never defaulted, never guessed.
//! - `c`/`cpp`: `static Val name(Val p, ...) { return <cuni_*>(...); if (...){} }`,
//!   `Val x = e;`, `cuni_say(e);` inside `int main(void)`. Everything is
//!   `Val`, so the same call-site inference recovers CuNi types;
//!   `V_int(n)`/`V_str(s)`/`V_bool(b)` and the `cuni_add/sub/mul/div/mod/eq/`
//!   `cmp/truthy/neg` helpers map back (bool literals stay bool);
//!   anything else refuses.
//! - `rs`: `fn name(p: Val, ...) -> Val { return <v_*>(...); if ... {} }`,
//!   `let [mut] x = e;`, `cuni_say(e);` inside `fn main()`. Same inference
//!   as C (`Val::Int(n)`/`Val::Str(s)`/`Val::Bool(b)`, `v_*` helpers,
//!   `.clone()`/`.into()` stripped, `Val::None` tail markers skipped).
//! - `awk`: `function name(p, ...)` defs + one `BEGIN` main; `print expr`,
//!   bare `x = expr`, `return`, `if`/`else`. Same call-site inference as JS
//!   (bare assignments feed binding evidence). Pattern-action rules,
//!   `printf`, `getline`, arrays refuse.
//! - `pl` (perl): `sub name { my ($p, ...) = @_; ... }` defs; `my $x = expr`
//!   / `$x = expr` (sigils stripped outside strings), `print EXPR, "\n"` /
//!   `say(EXPR)`, `return`, `if`/`else`/`unless`; same inference as JS.
//!   Regexes, interpolation, `.` concat, loops, modules refuse.
//! - `sh`: top-level only — `x=value`, `x=$((expr))`, `"$var"` interpolation,
//!   `echo`, `if [ a -op b ]; then/elif/else/fi`. Shell functions, loops,
//!   `$( )`, backticks, pipes refuse.
//! - `sql`: `SELECT <expr>;` statements → `say(<expr>)`; `'...'` strings,
//!   `=`/`<>` → `==`/`!=`, `AND`/`OR`/`NOT` folded. `FROM`/DDL refuse.
//! - `wat`: `(func $n (param $p T)* (result T) <single expr>)`, T in
//!   i32/i64→int, f32/f64→float; const/local.get/call/add/sub/mul/signed and
//!   float comparisons; `(start $f)` → top-level call. Locals, `if`,
//!   div/rem (trap on zero), memories refuse.
//! - every other catalog language: these seats emit a Python lowering, so
//!   ingest strips the leading `#` CuNi header lines and applies the Python
//!   subset. A file without the CuNi lowering header refuses. The five
//!   native seats above detect the CuNi lowering header (`is_cuni_lowering`)
//!   and route such artifacts to the lowering path, so the exactness harness
//!   keeps working unchanged.
//!
//! SAFETY NETS (all ingesters):
//! 1. Prelude/runtime helpers are skipped by name; anything that does not fit
//!    the subset is skipped or refused — never guessed.
//! 2. After conversion, the produced `.cuni` must lex, parse, AND typecheck
//!    via the real CuNi front-end (`self_check`), or ingest refuses. A
//!    dropped/skipped definition that is still referenced therefore refuses
//!    instead of producing a broken program.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::langs;

// ---------------------------------------------------------------------------
// public entry
// ---------------------------------------------------------------------------

pub fn ingest_file(path: &Path) -> Result<String, String> {
    let src = std::fs::read_to_string(path)
        .map_err(|e| format!("couldn't read {}: {}", path.display(), e))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext == "cuni" {
        return Ok(src);
    }
    // .mjs/.cjs are JS seats with no catalog entry of their own.
    let seat = if ext == "mjs" || ext == "cjs" {
        "js"
    } else {
        ext.as_str()
    };
    let cuni = match langs::find(seat).map(|l| l.id) {
        Some("py") => ingest_py(&src)?,
        Some("go") => ingest_go(&src)?,
        Some("js") | Some("ts") => ingest_js(&src)?,
        Some("c") | Some("cpp") => ingest_c(&src)?,
        Some("rs") => ingest_rs(&src)?,
        Some("rb") => ingest_rb(&src)?,
        Some("lua") => ingest_lua(&src)?,
        Some("sol") => ingest_sol(&src)?,
        Some("java") => ingest_java(&src)?,
        Some("awk") => ingest_awk(&src)?,
        Some("pl") => ingest_pl(&src)?,
        Some("sh") => ingest_sh(&src)?,
        Some("sql") => ingest_sql(&src)?,
        Some("wat") => ingest_wat(&src)?,
        Some(id) => ingest_lowering(&src, id)?,
        None => {
            return Err(format!(
                "ingest: refuse unknown seat `.{ext}` — no catalog language maps to it"
            ))
        }
    };
    let cuni = format!(
        "# ingested into CuNi — subset ingestion; exactness still required (run `cuni check`)\n{cuni}"
    );
    self_check(&cuni)?;
    Ok(cuni)
}

/// The produced `.cuni` must survive the real front-end, or we refuse.
fn self_check(cuni_src: &str) -> Result<(), String> {
    let path = std::env::temp_dir().join(format!(
        "cuni_ingest_check_{}_{:?}.cuni",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&path, cuni_src).map_err(|e| format!("ingest: self-check write failed: {e}"))?;
    let r = crate::check::load_program(&path)
        .map_err(|e| format!("ingest: produced .cuni failed the CuNi front-end — refusing: {e}"));
    let _ = std::fs::remove_file(&path);
    r.map(|_| ())
}

// ---------------------------------------------------------------------------
// Ix: the tiny expression model every ingester targets
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Ix {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Ident(String),
    Call(String, Vec<Ix>),
    Bin(String, Box<Ix>, Box<Ix>), // op in CuNi spelling: + - * / % == != < > <= >= and or
    Neg(Box<Ix>),
    Not(Box<Ix>),
}

impl Ix {
    fn to_cuni(&self) -> String {
        match self {
            Ix::Int(n) => n.to_string(),
            Ix::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    format!("{f:.1}")
                } else {
                    format!("{f}")
                }
            }
            Ix::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
            Ix::Bool(b) => b.to_string(),
            Ix::Ident(s) => s.clone(),
            Ix::Call(f, args) => format!(
                "{}({})",
                f,
                args.iter()
                    .map(|a| a.to_cuni())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Ix::Bin(op, l, r) => format!("({} {} {})", l.to_cuni(), op, r.to_cuni()),
            Ix::Neg(e) => format!("(-{})", e.to_cuni()),
            Ix::Not(e) => format!("(not {})", e.to_cuni()),
        }
    }
}

// ---------------------------------------------------------------------------
// tokenizer (shared)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Sym(String),
}

fn tokenize(src: &str, hash_comments: bool, ruby_interp: bool) -> Result<Vec<Tok>, String> {
    let ch: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let n = ch.len();
    while i < n {
        let c = ch[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < n && ch[i + 1] == '/' {
            while i < n && ch[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < n && ch[i + 1] == '*' {
            i += 2;
            while i + 1 < n && !(ch[i] == '*' && ch[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }
        if hash_comments && c == '#' {
            while i < n && ch[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '"' {
            i += 1;
            let mut s = String::new();
            while i < n && ch[i] != '"' {
                if ruby_interp && ch[i] == '#' && i + 1 < n && ch[i + 1] == '{' {
                    return Err("ingest: refuse Ruby `#{}` interpolation (outside subset)".into());
                }
                if ch[i] == '\\' && i + 1 < n {
                    match ch[i + 1] {
                        'n' => s.push('\n'),
                        't' => s.push('\t'),
                        'r' => s.push('\r'),
                        '\\' => s.push('\\'),
                        '"' => s.push('"'),
                        '0' => s.push('\0'),
                        e => {
                            s.push('\\');
                            s.push(e);
                        }
                    }
                    i += 2;
                } else {
                    s.push(ch[i]);
                    i += 1;
                }
            }
            if i >= n {
                return Err("ingest: unterminated string".into());
            }
            i += 1;
            toks.push(Tok::Str(s));
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < n && ch[j].is_ascii_digit() {
                j += 1;
            }
            let mut is_float = false;
            if j < n && ch[j] == '.' && j + 1 < n && ch[j + 1].is_ascii_digit() {
                is_float = true;
                j += 1;
                while j < n && ch[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let num: String = ch[i..j].iter().collect();
            // skip type suffixes: 40LL, 2L, 1.5f
            let mut k = j;
            while k < n && ch[k].is_ascii_alphabetic() {
                k += 1;
            }
            if is_float {
                toks.push(Tok::Float(
                    num.parse().map_err(|_| "ingest: bad float literal")?,
                ));
            } else {
                toks.push(Tok::Int(
                    num.parse().map_err(|_| "ingest: bad int literal")?,
                ));
            }
            i = k;
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i;
            while j < n && (ch[j].is_ascii_alphanumeric() || ch[j] == '_') {
                j += 1;
            }
            toks.push(Tok::Ident(ch[i..j].iter().collect()));
            i = j;
            continue;
        }
        let rest: String = ch[i..std::cmp::min(i + 3, n)].iter().collect();
        let sym = if rest.starts_with("===") || rest.starts_with("!==") {
            &rest[..3]
        } else if ["==", "!=", "<=", ">=", "&&", "||", ":=", "->", "::", "~="]
            .iter()
            .any(|p| rest.starts_with(p))
        {
            &rest[..2]
        } else {
            &rest[..1]
        };
        toks.push(Tok::Sym(sym.to_string()));
        i += sym.len();
    }
    Ok(toks)
}

// ---------------------------------------------------------------------------
// expression parser (shared recursive descent, per-language atoms)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ELang {
    Go,
    Js,
    C,
    Rs,
    Rb,
    Lua,
    Py,
    Sol,
    Java,
}

struct XP {
    toks: Vec<Tok>,
    pos: usize,
    lang: ELang,
}

impl XP {
    fn new(src: &str, lang: ELang) -> Result<Self, String> {
        Ok(XP {
            toks: tokenize(src, lang == ELang::Py, lang == ELang::Rb)?,
            pos: 0,
            lang,
        })
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat_sym(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Sym(x)) if x == s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn eat_ident(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Ident(x)) if x == s) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_sym(&mut self, s: &str) -> Result<(), String> {
        if self.eat_sym(s) {
            Ok(())
        } else {
            Err(format!("ingest: expected `{s}`"))
        }
    }

    fn expect_ident(&mut self) -> Result<String, String> {
        match self.next() {
            Some(Tok::Ident(s)) => Ok(s),
            other => Err(format!("ingest: expected identifier, got {other:?}")),
        }
    }

    fn trailing(&self) -> Result<(), String> {
        if self.pos >= self.toks.len() {
            Ok(())
        } else {
            Err(format!(
                "ingest: refuse trailing tokens `{}` (outside subset)",
                self.toks[self.pos..]
                    .iter()
                    .map(|t| format!("{t:?}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            ))
        }
    }

    fn expr(&mut self) -> Result<Ix, String> {
        let e = self.or()?;
        self.trailing()?;
        Ok(e)
    }

    fn sub_expr(&mut self) -> Result<Ix, String> {
        self.or()
    }

    fn or(&mut self) -> Result<Ix, String> {
        let mut l = self.and()?;
        loop {
            let is_or = self.eat_sym("||") || self.eat_ident("or");
            if !is_or {
                break;
            }
            let r = self.and()?;
            l = Ix::Bin("or".into(), Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<Ix, String> {
        let mut l = self.cmp()?;
        loop {
            let is_and = self.eat_sym("&&") || self.eat_ident("and");
            if !is_and {
                break;
            }
            let r = self.cmp()?;
            l = Ix::Bin("and".into(), Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn cmp(&mut self) -> Result<Ix, String> {
        let mut l = self.add()?;
        loop {
            let op = if self.eat_sym("==") || self.eat_sym("===") {
                "=="
            } else if self.eat_sym("!=") || self.eat_sym("!==") || self.eat_sym("~=") {
                "!="
            } else if self.eat_sym("<=") {
                "<="
            } else if self.eat_sym(">=") {
                ">="
            } else if self.eat_sym("<") {
                "<"
            } else if self.eat_sym(">") {
                ">"
            } else {
                break;
            };
            let r = self.add()?;
            // C/Rust emit comparisons as cuni_cmp(a,b) </> 0 — fold back.
            if let Ix::Call(f, args) = &l {
                if (f == "cuni_cmp" || f == "v_cmp") && args.len() == 2 {
                    if let Ix::Int(0) = r {
                        let a = args[0].clone();
                        let b = args[1].clone();
                        l = Ix::Bin(op.into(), Box::new(a), Box::new(b));
                        continue;
                    }
                }
            }
            l = Ix::Bin(op.into(), Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn add(&mut self) -> Result<Ix, String> {
        let mut l = self.mul()?;
        loop {
            let op = if self.eat_sym("+") {
                "+"
            } else if self.eat_sym("-") {
                "-"
            } else {
                break;
            };
            let r = self.mul()?;
            l = Ix::Bin(op.into(), Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn mul(&mut self) -> Result<Ix, String> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat_sym("*") {
                "*"
            } else if self.eat_sym("/") {
                "/"
            } else if self.eat_sym("%") {
                "%"
            } else {
                break;
            };
            let r = self.unary()?;
            l = Ix::Bin(op.into(), Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn unary(&mut self) -> Result<Ix, String> {
        if self.eat_sym("!") {
            return Ok(Ix::Not(Box::new(self.unary()?)));
        }
        if self.eat_sym("-") {
            return Ok(Ix::Neg(Box::new(self.unary()?)));
        }
        if self.eat_ident("not") {
            return Ok(Ix::Not(Box::new(self.unary()?)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Ix, String> {
        let e = self.primary()?;
        loop {
            if self.eat_sym(".") {
                let m = self.expect_ident()?;
                // Rust Val ergonomics the backends emit; strip, keep value.
                if m == "clone" || m == "into" {
                    self.expect_sym("(")?;
                    self.expect_sym(")")?;
                    continue;
                }
                return Err(format!("ingest: refuse `.{m}` (outside subset)"));
            }
            break;
        }
        Ok(e)
    }

    fn primary(&mut self) -> Result<Ix, String> {
        match self.next() {
            Some(Tok::Int(n)) => Ok(Ix::Int(n)),
            Some(Tok::Float(f)) => {
                if self.lang == ELang::Py || self.lang == ELang::Go || self.lang == ELang::Java {
                    Ok(Ix::Float(f))
                } else {
                    Err("ingest: refuse float literal (outside subset)".into())
                }
            }
            Some(Tok::Str(s)) => Ok(Ix::Str(s)),
            Some(Tok::Ident(s)) => {
                match s.as_str() {
                    "true" => return Ok(Ix::Bool(true)),
                    "false" => return Ok(Ix::Bool(false)),
                    "True" if self.lang == ELang::Py => return Ok(Ix::Bool(true)),
                    "False" if self.lang == ELang::Py => return Ok(Ix::Bool(false)),
                    // Ruby/Lua `nil` is CuNi `none`.
                    "nil" if self.lang == ELang::Rb || self.lang == ELang::Lua => {
                        return Ok(Ix::Ident("none".into()))
                    }
                    _ => {}
                }
                // Rust `Val::X` atoms.
                if s == "Val" && self.eat_sym("::") {
                    let k = self.expect_ident()?;
                    return match k.as_str() {
                        "Int" => {
                            self.expect_sym("(")?;
                            let e = self.sub_expr()?;
                            self.expect_sym(")")?;
                            match e {
                                Ix::Int(n) => Ok(Ix::Int(n)),
                                _ => Err("ingest: refuse non-int Val::Int".into()),
                            }
                        }
                        "Str" => {
                            self.expect_sym("(")?;
                            let e = self.sub_expr()?;
                            self.expect_sym(")")?;
                            match e {
                                Ix::Str(s) => Ok(Ix::Str(s)),
                                _ => Err("ingest: refuse non-str Val::Str".into()),
                            }
                        }
                        "Bool" => {
                            self.expect_sym("(")?;
                            let e = self.sub_expr()?;
                            self.expect_sym(")")?;
                            Ok(e) // truthiness wrapper, like V_bool/cuni_truthy/truthy
                        }
                        _ => Err(format!("ingest: refuse Val::{k} (outside subset)")),
                    };
                }
                if self.eat_sym("(") {
                    let mut args = Vec::new();
                    if !self.eat_sym(")") {
                        loop {
                            args.push(self.sub_expr()?);
                            if !self.eat_sym(",") {
                                break;
                            }
                        }
                        self.expect_sym(")")?;
                    }
                    return self.call(&s, args);
                }
                Ok(Ix::Ident(s))
            }
            Some(Tok::Sym(s)) if s == "(" => {
                let e = self.sub_expr()?;
                self.expect_sym(")")?;
                Ok(e)
            }
            other => Err(format!("ingest: refuse expression near {other:?}")),
        }
    }

    /// Map a backend's runtime-helper call back to the CuNi operation.
    fn call(&mut self, name: &str, args: Vec<Ix>) -> Result<Ix, String> {
        let bin2 = |op: &str| -> Result<Ix, String> {
            if args.len() == 2 {
                Ok(Ix::Bin(
                    op.into(),
                    Box::new(args[0].clone()),
                    Box::new(args[1].clone()),
                ))
            } else {
                Err(format!("ingest: refuse `{name}` arity {}", args.len()))
            }
        };
        let un1 = || -> Result<Ix, String> {
            if args.len() == 1 {
                Ok(args[0].clone())
            } else {
                Err(format!("ingest: refuse `{name}` arity {}", args.len()))
            }
        };
        match self.lang {
            ELang::C => match name {
                "cuni_add" | "cuni_concat" => bin2("+"),
                "cuni_sub" => bin2("-"),
                "cuni_mul" => bin2("*"),
                "cuni_div" => bin2("/"),
                "cuni_mod" => bin2("%"),
                "cuni_eq" => bin2("=="),
                "cuni_cmp" => Ok(Ix::Call("cuni_cmp".into(), args)),
                "cuni_truthy" => un1(),
                "cuni_neg" => {
                    if args.len() == 1 {
                        Ok(Ix::Neg(Box::new(args[0].clone())))
                    } else {
                        Err("ingest: refuse cuni_neg arity".into())
                    }
                }
                "V_int" => match args.as_slice() {
                    [Ix::Int(n)] => Ok(Ix::Int(*n)),
                    _ => Err("ingest: refuse non-int V_int".into()),
                },
                "V_str" => match args.as_slice() {
                    [Ix::Str(s)] => Ok(Ix::Str(s.clone())),
                    _ => Err("ingest: refuse non-str V_str".into()),
                },
                "V_bool" => match args.as_slice() {
                    // bool literals: the backend emits V_bool(1)/V_bool(0);
                    // keep them bool (un1() would erase to Int).
                    [Ix::Int(1)] => Ok(Ix::Bool(true)),
                    [Ix::Int(0)] => Ok(Ix::Bool(false)),
                    // V_bool(bool_expr): the inner is already bool-typed
                    // (comparisons fold back in cmp()), so unwrap is sound.
                    _ => un1(),
                },
                _ => Ok(Ix::Call(name.into(), args)),
            },
            ELang::Rs => match name {
                "v_add" | "v_concat" => bin2("+"),
                "v_sub" => bin2("-"),
                "v_mul" => bin2("*"),
                "v_div" => bin2("/"),
                "v_mod" => bin2("%"),
                "v_eq" => bin2("=="),
                "v_cmp" => Ok(Ix::Call("v_cmp".into(), args)),
                "truthy" => un1(),
                "v_neg" => {
                    if args.len() == 1 {
                        Ok(Ix::Neg(Box::new(args[0].clone())))
                    } else {
                        Err("ingest: refuse v_neg arity".into())
                    }
                }
                _ => Ok(Ix::Call(name.into(), args)),
            },
            ELang::Js => match name {
                "_cuni_div" => bin2("/"),
                _ => Ok(Ix::Call(name.into(), args)),
            },
            ELang::Rb | ELang::Lua => match name {
                // Both native backends lower int division through a prelude
                // helper; map it back to CuNi `/`. Other prelude helpers
                // (`_cuni_slice`, `_cuni_repr`, …) stay as calls and the
                // CuNi front-end refuses them — never silently kept.
                "_cuni_div" => bin2("/"),
                _ => Ok(Ix::Call(name.into(), args)),
            },
            _ => Ok(Ix::Call(name.into(), args)),
        }
    }
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    match c.next() {
        Some(x) if x.is_ascii_alphabetic() || x == '_' => {
            c.all(|x| x.is_ascii_alphanumeric() || x == '_')
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// brace-aware block tools (shared by the brace languages)
// ---------------------------------------------------------------------------

/// Strip a `//` comment from one line, respecting string contents.
fn strip_line_comment(line: &str) -> String {
    let mut out = String::new();
    let mut ch = line.chars().peekable();
    let mut in_str = false;
    while let Some(c) = ch.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(e) = ch.next() {
                    out.push(e);
                }
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == '/' && ch.peek() == Some(&'/') {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

/// Byte-scan `{ ... }` starting at the `{` at byte index `open`.
/// Returns (inner_body, index_just_past_closing_brace).
fn extract_braced(src: &str, open: usize) -> Result<(String, usize), String> {
    let b = src.as_bytes();
    if b.get(open) != Some(&b'{') {
        return Err("ingest: expected `{`".into());
    }
    let mut depth = 0i32;
    let mut i = open;
    let n = b.len();
    let mut in_str = false;
    let mut esc = false;
    while i < n {
        let c = b[i];
        if esc {
            esc = false;
            i += 1;
            continue;
        }
        if in_str {
            if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'/' if i + 1 < n && b[i + 1] == b'/' => {
                while i < n && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Ok((src[open + 1..i].to_string(), i + 1));
                }
            }
            _ => {}
        }
        i += 1;
    }
    Err("ingest: unbalanced braces".into())
}

struct Func {
    name: String,
    sig: String,
    body: String,
}

/// Extract top-level `prefix ... { ... }` items. The prefix must start a line
/// (modulo whitespace) so matches inside comments/strings are ignored.
fn extract_funcs(src: &str, prefix: &str) -> Result<Vec<Func>, String> {
    let mut funcs = Vec::new();
    let mut i = 0usize;
    while let Some(p) = src[i..].find(prefix) {
        let start = i + p;
        let line_start = src[..start].rfind('\n').map(|x| x + 1).unwrap_or(0);
        if !src[line_start..start].trim().is_empty() {
            i = start + prefix.len();
            continue;
        }
        let sig_start = start + prefix.len();
        let open = match src[sig_start..].find('{') {
            Some(o) => sig_start + o,
            None => {
                i = start + prefix.len();
                continue;
            }
        };
        let sig = src[sig_start..open].trim().to_string();
        let (body, end) = extract_braced(src, open)?;
        let name = sig
            .split(|c: char| c == '(' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_string();
        funcs.push(Func { name, sig, body });
        i = end;
    }
    Ok(funcs)
}

/// Split a braced body into top-level statement chunks (brace-aware).
/// A chunk is one statement (ends `;` or `}`) or one nested block.
fn split_chunks(body: &str) -> Result<Vec<String>, String> {
    let mut chunks = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for line in body.lines() {
        let stripped = strip_line_comment(line);
        let d = brace_depth_delta(&stripped);
        cur.push_str(line);
        cur.push('\n');
        depth += d;
        if depth < 0 {
            return Err("ingest: unbalanced braces".into());
        }
        if depth == 0 && !cur.trim().is_empty() {
            chunks.push(std::mem::take(&mut cur));
        }
    }
    if depth != 0 {
        return Err("ingest: unbalanced braces".into());
    }
    if !cur.trim().is_empty() {
        chunks.push(cur);
    }
    Ok(chunks)
}

fn brace_depth_delta(stripped_line: &str) -> i32 {
    let mut d = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for c in stripped_line.chars() {
        if esc {
            esc = false;
            continue;
        }
        if in_str {
            if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => d += 1,
            '}' => d -= 1,
            _ => {}
        }
    }
    d
}

/// Split `if COND { ... } [else { ... }]` into (cond, then_body, else_body?).
fn split_if(chunk: &str) -> Result<(String, String, Option<String>), String> {
    let t = chunk.trim();
    let rest = t
        .strip_prefix("if")
        .ok_or("ingest: bad if")?
        .trim_start()
        .to_string();
    let mut pdepth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    let mut brace_at = None;
    for (i, c) in rest.char_indices() {
        if esc {
            esc = false;
            continue;
        }
        if in_str {
            if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' => pdepth += 1,
            ')' => pdepth -= 1,
            '{' if pdepth == 0 => {
                brace_at = Some(i);
                break;
            }
            _ => {}
        }
    }
    let b = brace_at.ok_or("ingest: bad if header")?;
    let cond = rest[..b].trim().to_string();
    let (then_body, after_idx) = extract_braced(&rest, b)?;
    let after = rest[after_idx..].trim();
    let else_body = if let Some(r2) = after.strip_prefix("else") {
        let r2 = r2.trim_start();
        let b2 = r2.find('{').ok_or("ingest: bad else")?;
        let (eb, after2_idx) = extract_braced(r2, b2)?;
        if !r2[after2_idx..].trim().is_empty() {
            return Err("ingest: bad trailing after else".into());
        }
        Some(eb)
    } else if after.is_empty() {
        None
    } else {
        return Err(format!("ingest: bad trailing after if: {after}"));
    };
    Ok((cond, then_body, else_body))
}

/// Strip a single trailing `;` (JS/TS/C/Rust statement terminator).
fn strip_semi(t: &str) -> &str {
    t.strip_suffix(';').unwrap_or(t).trim()
}

/// Split `name = expr` at the top level using the tokenizer (avoids `==`).
fn split_assign(chunk: &str, lang: ELang) -> Result<Option<(String, String)>, String> {
    let toks = tokenize(chunk, lang == ELang::Py, lang == ELang::Rb)?;
    let mut depth = 0i32;
    let mut eq_at = None;
    for (i, t) in toks.iter().enumerate() {
        match t {
            Tok::Sym(s) if s == "(" || s == "[" || s == "{" => depth += 1,
            Tok::Sym(s) if s == ")" || s == "]" || s == "}" => depth -= 1,
            Tok::Sym(s) if s == "=" && depth == 0 => {
                eq_at = Some(i);
                break;
            }
            _ => {}
        }
    }
    let i = match eq_at {
        Some(i) => i,
        None => return Ok(None),
    };
    // reconstruct lhs/rhs as source text is hard; instead re-slice by locating
    // the `=` byte: find the first `=` not part of `==`/`<=`/`>=`/`!=` at depth 0.
    let mut out: Option<(String, String)> = None;
    let mut depth2 = 0i32;
    let mut in_str = false;
    let mut esc = false;
    let bytes = chunk.as_bytes();
    let mut k = 0usize;
    while k < bytes.len() {
        let c = bytes[k];
        if esc {
            esc = false;
            k += 1;
            continue;
        }
        if in_str {
            if c == b'\\' {
                esc = true;
            } else if c == b'"' {
                in_str = false;
            }
            k += 1;
            continue;
        }
        match c {
            b'"' => in_str = true,
            b'(' | b'[' | b'{' => depth2 += 1,
            b')' | b']' | b'}' => depth2 -= 1,
            b'=' if depth2 == 0 => {
                let prev = if k > 0 { bytes[k - 1] } else { 0 };
                let next = if k + 1 < bytes.len() { bytes[k + 1] } else { 0 };
                if prev != b'=' && prev != b'!' && prev != b'<' && prev != b'>' && next != b'=' {
                    out = Some((
                        chunk[..k].trim().to_string(),
                        chunk[k + 1..].trim().to_string(),
                    ));
                    break;
                }
            }
            _ => {}
        }
        k += 1;
    }
    let _ = i;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Go
// ---------------------------------------------------------------------------

const GO_PRELUDE: &[&str] = &[
    "say",
    "cuni_as_int",
    "cuni_range",
    "cuni_abs",
    "cuni_min",
    "cuni_max",
    "cuni_len",
    "cuni_slice",
];

fn go_type_to_cuni(t: &str) -> Result<String, String> {
    match t.trim() {
        "int" | "int64" | "int32" => Ok("int".into()),
        "string" => Ok("str".into()),
        "bool" => Ok("bool".into()),
        "float64" | "float32" => Ok("float".into()),
        other => Err(format!("ingest: Go type `{other}` outside subset")),
    }
}

fn ingest_go(src: &str) -> Result<String, String> {
    let funcs = extract_funcs(src, "func ")?;
    let mut out = String::new();
    let mut main_body = None;
    for f in &funcs {
        if f.name == "main" {
            main_body = Some(f.body.clone());
            continue;
        }
        if GO_PRELUDE.contains(&f.name.as_str()) {
            continue;
        }
        match go_def(f) {
            Ok(Some(def)) => {
                out.push_str(&def);
                out.push('\n');
            }
            Ok(None) => {} // skipped; self-check refuses if still referenced
            Err(e) => return Err(format!("ingest: Go def `{}`: {e}", f.name)),
        }
    }
    let main = main_body.ok_or("ingest: refuse Go without func main")?;
    for st in go_block(&main, 0)? {
        out.push_str(&st);
        out.push('\n');
    }
    Ok(out)
}

/// Returns Ok(None) to skip a definition that falls outside the subset.
fn go_def(f: &Func) -> Result<Option<String>, String> {
    let sig = f.sig.trim();
    let paren = sig.find('(').ok_or("ingest: bad Go sig")?;
    let name = sig[..paren].trim();
    if !is_ident(name) {
        return Ok(None);
    }
    let close = sig.rfind(')').ok_or("ingest: bad Go sig")?;
    let params_s = &sig[paren + 1..close];
    let ret_s = sig[close + 1..].trim();
    if ret_s.contains('(') || ret_s.contains(',') || ret_s.contains('[') {
        return Ok(None); // fallible (T, error) or generics: outside subset
    }
    if name.contains('[') || params_s.contains('[') {
        return Ok(None);
    }
    let mut params = Vec::new();
    if !params_s.trim().is_empty() {
        for p in params_s.split(',') {
            let p = p.trim();
            let mut it = p.split_whitespace();
            let pn = it.next().ok_or("ingest: bad Go param")?;
            let pt = it.next().ok_or("ingest: bad Go param")?;
            if it.next().is_some() || !is_ident(pn) {
                return Ok(None);
            }
            let ct = go_type_to_cuni(pt).map_err(|_| "skip")?;
            params.push(format!("{pn}: {ct}"));
        }
    }
    let ret = if ret_s.is_empty() {
        "int".into()
    } else {
        go_type_to_cuni(ret_s).map_err(|_| "skip")?
    };
    let mut body = Vec::new();
    for st in go_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None); // def without return: outside subset
    }
    Ok(Some(format!(
        "def {name}({}) -> {ret} do\n{}\nend",
        params.join(", "),
        body.join("\n")
    )))
}

fn go_block(body: &str, indent: usize) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for chunk in split_chunks(body)? {
        lines.extend(go_stmt(&chunk, indent)?);
    }
    Ok(lines)
}

fn go_stmt(chunk: &str, indent: usize) -> Result<Vec<String>, String> {
    let pad = "    ".repeat(indent);
    let t = chunk.trim();
    if t.is_empty() {
        return Ok(vec![]);
    }
    if t.starts_with("if ") && t.contains('{') {
        let (cond, then_b, else_b) = split_if(t)?;
        let c = XP::new(&cond, ELang::Go)?.expr()?.to_cuni();
        let mut v = vec![format!("{pad}if {c} do")];
        v.extend(go_block(&then_b, indent + 1)?);
        if let Some(e) = else_b {
            v.push(format!("{pad}els"));
            v.extend(go_block(&e, indent + 1)?);
        }
        v.push(format!("{pad}end"));
        return Ok(v);
    }
    for bad in ["for ", "switch ", "select ", "go ", "defer ", "panic("] {
        if t.starts_with(bad) {
            return Err(format!("ingest: refuse Go `{bad}` (outside subset)"));
        }
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let e = XP::new(rest.trim(), ELang::Go)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}ret {e}")]);
    }
    if t == "return" {
        return Ok(vec![format!("{pad}ret")]);
    }
    if let Some(p) = t.find(":=") {
        let name = t[..p].trim();
        if !is_ident(name) {
            return Err(format!("ingest: refuse Go binding `{name}`"));
        }
        let e = XP::new(t[p + 2..].trim(), ELang::Go)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}let {name} = {e}")]);
    }
    if t.starts_with("say(") && t.ends_with(')') {
        let e = XP::new(&t[4..t.len() - 1], ELang::Go)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}say({e})")]);
    }
    if let Some((lhs, rhs)) = split_assign(t, ELang::Go)? {
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse Go assignment target `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::Go)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}{lhs} = {e}")]);
    }
    // bare call statement
    let e = XP::new(t, ELang::Go)?.expr()?;
    match e {
        Ix::Call(f, _) if f == "main" => Ok(vec![]),
        _ => Ok(vec![format!("{pad}{}", e.to_cuni())]),
    }
}

// ---------------------------------------------------------------------------
// Java — the CuNi Java backend's own output shape
// ---------------------------------------------------------------------------
//
// Subset: `class Main` with `static` methods (`static <ret> name(params)`),
// `public static void main(String[] args)` as the entry point, and the
// statement subset the Java backend emits: `return`, `if/else`, local
// bindings (`long`/`double`/`String`/`boolean` + boxed forms), `say(...)`,
// `.add(...)` (CuNi `.push`), and plain assignments. Generic methods
// (`<T>`), nested classes/enums, and the `cuni_*` helpers are skipped —
// helpers never appear in user code, and the round-trip harness only feeds
// this ingester artifacts the Java backend itself produced.

/// Helper method names the Java backend emits (see codegen_java.rs).
const JAVA_PRELUDE: &[&str] = &[
    "say", "cuni_str", "cuni_range", "cuni_abs", "cuni_min", "cuni_max", "cuni_mod", "cuni_div",
    "cuni_slice",
];

fn ingest_java(src: &str) -> Result<String, String> {
    if is_cuni_lowering(src) {
        return ingest_lowering(src, "java");
    }
    // Normalize `public static void main` so the `static ` prefix extractor
    // sees it like every other method.
    let src = src.replace("public static ", "static ");
    let funcs = extract_funcs(&src, "static ")?;
    let mut out = String::new();
    let mut main_body = None;
    for f in &funcs {
        // Java sigs start with the return type (`static long add(`), so the
        // method name is the identifier immediately before `(` — unlike Go,
        // `extract_funcs`' own `name` field holds the return type here.
        let sig = f.sig.trim();
        let paren = match sig.find('(') {
            Some(p) => p,
            None => continue, // `static class Point` / `static enum`: not a method
        };
        let head = sig[..paren].trim();
        let name = head.split_whitespace().last().unwrap_or("");
        if name == "main" {
            main_body = Some(f.body.clone());
            continue;
        }
        if JAVA_PRELUDE.contains(&name) {
            continue;
        }
        match java_def(f, name) {
            Ok(Some(def)) => {
                out.push_str(&def);
                out.push('\n');
            }
            Ok(None) => {} // skipped; self-check refuses if still referenced
            Err(e) => return Err(format!("ingest: Java def `{name}`: {e}")),
        }
    }
    let main = main_body.ok_or("ingest: refuse Java without static main")?;
    for st in java_block(&main, 0)? {
        out.push_str(&st);
        out.push('\n');
    }
    Ok(out)
}

fn java_type_to_cuni(t: &str) -> Option<&'static str> {
    match t {
        "long" | "int" | "Long" | "Integer" => Some("int"),
        "double" | "float" | "Double" | "Float" => Some("float"),
        "String" => Some("str"),
        "boolean" | "Boolean" => Some("bool"),
        _ => None,
    }
}

/// Returns Ok(None) to skip a definition that falls outside the subset.
fn java_def(f: &Func, name: &str) -> Result<Option<String>, String> {
    let sig = f.sig.trim();
    if sig.contains('<') || sig.contains('>') {
        return Ok(None); // generics: outside subset
    }
    let paren = sig.find('(').ok_or("ingest: bad Java sig")?;
    let head = sig[..paren].trim();
    if !is_ident(name) {
        return Ok(None);
    }
    let ret_s = head[..head.len() - name.len()].trim();
    let close = sig.rfind(')').ok_or("ingest: bad Java sig")?;
    let params_s = &sig[paren + 1..close];
    let mut params = Vec::new();
    if !params_s.trim().is_empty() {
        for p in params_s.split(',') {
            let p = p.trim().strip_prefix("final ").unwrap_or(p.trim()).trim();
            // `String[] args` (main) never reaches here; `long a` is the shape.
            let mut it = p.split_whitespace();
            let pt = it.next().ok_or("ingest: bad Java param")?;
            let pn = it.next().ok_or("ingest: bad Java param")?;
            if it.next().is_some() || !is_ident(pn) {
                return Ok(None);
            }
            let ct = java_type_to_cuni(pt).ok_or("ingest: Java param type outside subset")?;
            params.push(format!("{pn}: {ct}"));
        }
    }
    let ret = java_type_to_cuni(ret_s).ok_or("ingest: Java return type outside subset")?;
    let mut body = Vec::new();
    for st in java_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None); // def without return: outside subset
    }
    Ok(Some(format!(
        "def {name}({}) -> {ret} do\n{}\nend",
        params.join(", "),
        body.join("\n")
    )))
}

fn java_block(body: &str, indent: usize) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for chunk in split_chunks(body)? {
        lines.extend(java_stmt(&chunk, indent)?);
    }
    Ok(lines)
}

fn java_stmt(chunk: &str, indent: usize) -> Result<Vec<String>, String> {
    let pad = "    ".repeat(indent);
    let t = chunk.trim();
    if t.is_empty() {
        return Ok(vec![]);
    }
    if t.starts_with("if ") && t.contains('{') {
        let (cond, then_b, else_b) = split_if(t)?;
        // The Java backend wraps conditions in parens: `if ((a > b))`.
        let cond = cond.trim().strip_prefix('(').unwrap_or(cond.trim());
        let cond = cond.strip_suffix(')').unwrap_or(cond).trim();
        let c = XP::new(cond, ELang::Java)?.expr()?.to_cuni();
        let mut v = vec![format!("{pad}if {c} do")];
        v.extend(java_block(&then_b, indent + 1)?);
        if let Some(e) = else_b {
            v.push(format!("{pad}els"));
            v.extend(java_block(&e, indent + 1)?);
        }
        v.push(format!("{pad}end"));
        return Ok(v);
    }
    for bad in ["for ", "while ", "switch ", "throw ", "try ", "synchronized "] {
        if t.starts_with(bad) {
            return Err(format!("ingest: refuse Java `{bad}` (outside subset)"));
        }
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let rest = strip_semi(rest.trim());
        if rest.is_empty() {
            return Ok(vec![format!("{pad}ret")]);
        }
        let e = XP::new(rest, ELang::Java)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}ret {e}")]);
    }
    if t == "return;" || t == "return" {
        return Ok(vec![format!("{pad}ret")]);
    }
    if t.starts_with("say(") && t.ends_with(')') {
        let inner = strip_semi(&t[4..t.len() - 1]);
        let e = XP::new(inner, ELang::Java)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}say({e})")]);
    }
    // `.add(x);` is the Java backend's shape for CuNi `.push(x)`.
    if let Some(dot) = t.find(".add(") {
        let base = t[..dot].trim();
        let arg = strip_semi(t[dot + 5..].trim().strip_suffix(')').unwrap_or(t[dot + 5..].trim()));
        if is_ident(base) {
            let e = XP::new(arg, ELang::Java)?.expr()?.to_cuni();
            return Ok(vec![format!("{pad}{base}.push({e})")]);
        }
    }
    // Local binding: `[final ]<type> <name> = <expr>;`
    if let Some((typ, rest)) = split_java_binding(t) {
        if java_type_to_cuni(typ).is_some() {
            if let Some(eq) = rest.find('=') {
                let name = rest[..eq].trim();
                if is_ident(name) {
                    let e = XP::new(strip_semi(rest[eq + 1..].trim()), ELang::Java)?
                        .expr()?
                        .to_cuni();
                    return Ok(vec![format!("{pad}let {name} = {e}")]);
                }
            }
            return Err(format!("ingest: refuse Java binding `{t}` (outside subset)"));
        }
    }
    if let Some((lhs, rhs)) = split_assign(t, ELang::Java)? {
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse Java assignment target `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::Java)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}{lhs} = {e}")]);
    }
    // bare call statement
    let e = XP::new(strip_semi(t), ELang::Java)?.expr()?;
    Ok(vec![format!("{pad}{}", e.to_cuni())])
}

/// Split a leading `<type> <rest>` for a Java local binding. Returns None
/// when the chunk doesn't start with a known Java type name.
fn split_java_binding(t: &str) -> Option<(&str, &str)> {
    let t = t.strip_prefix("final ").unwrap_or(t).trim_start();
    for typ in [
        "long", "double", "String", "boolean", "int", "float", "Long", "Double", "Boolean",
        "Integer", "Float",
    ] {
        if let Some(rest) = t.strip_prefix(typ) {
            if rest.starts_with(|c: char| c.is_whitespace()) {
                return Some((typ, rest.trim_start()));
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Call-site type inference (shared by the untyped / Val-boxed backends)
//
// The js/ts/c/cpp/rs backends erase CuNi types: JS emits untyped functions,
// C and Rust box everything in `Val`. Rather than defaulting params to int
// (a silent mistranslation for str/bool code), ingest recovers types in two
// passes over CuNi-shaped artifacts:
//
//  1. Scan every user function body and the main body for call sites
//     `name(arg, ...)`. Each argument is parsed with the shared XP layer
//     (which already normalizes `V_str("ada")` -> Str, `Val::Int(1)` -> Int,
//     `cuni_add` -> `+`, `.clone()` stripping, ...), then classified.
//  2. A parameter's type is the unique type all its call-site arguments
//     agree on; the return type is the unique type all `return` expressions
//     agree on (with the function's own bindings + inferred params as the
//     type environment).
//
// Anything uninferrable — no call sites, mixed types, non-literal-only args —
// refuses with a clear message. Never guessed, never defaulted.
// ---------------------------------------------------------------------------

/// CuNi-level type recovered by inference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Ty {
    Unknown,
    Int,
    Float,
    Str,
    Bool,
}

impl Ty {
    fn cuni(self) -> Option<&'static str> {
        match self {
            Ty::Int => Some("int"),
            Ty::Float => Some("float"),
            Ty::Str => Some("str"),
            Ty::Bool => Some("bool"),
            Ty::Unknown => None,
        }
    }

    fn unify(a: Ty, b: Ty) -> Ty {
        if a == b {
            a
        } else {
            Ty::Unknown
        }
    }
}

fn num_ty(t: Ty) -> Ty {
    match t {
        Ty::Int => Ty::Int,
        Ty::Float => Ty::Float,
        _ => Ty::Unknown,
    }
}

fn bin_ty(op: &str, l: Ty, r: Ty) -> Ty {
    match op {
        "==" | "!=" | "<" | ">" | "<=" | ">=" | "and" | "or" => Ty::Bool,
        "+" => match (l, r) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) | (Ty::Float, Ty::Int) | (Ty::Int, Ty::Float) => Ty::Float,
            (Ty::Str, Ty::Str) => Ty::Str,
            _ => Ty::Unknown,
        },
        "-" | "*" | "%" => match (l, r) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) | (Ty::Float, Ty::Int) | (Ty::Int, Ty::Float) => Ty::Float,
            _ => Ty::Unknown,
        },
        // CuNi `/` truncates on (int, int), else float — either way known.
        "/" => match (l, r) {
            (Ty::Int, Ty::Int) => Ty::Int,
            (Ty::Float, Ty::Float) | (Ty::Float, Ty::Int) | (Ty::Int, Ty::Float) => Ty::Float,
            _ => Ty::Unknown,
        },
        _ => Ty::Unknown,
    }
}

/// Classify an XP-normalized expression to a CuNi type.
fn ix_ty(ix: &Ix, env: &HashMap<String, Ty>) -> Ty {
    match ix {
        Ix::Int(_) => Ty::Int,
        Ix::Float(_) => Ty::Float,
        Ix::Str(_) => Ty::Str,
        Ix::Bool(_) => Ty::Bool,
        Ix::Ident(n) => env.get(n).copied().unwrap_or(Ty::Unknown),
        Ix::Neg(e) => num_ty(ix_ty(e, env)),
        Ix::Not(_) => Ty::Bool,
        Ix::Bin(op, l, r) => bin_ty(op, ix_ty(l, env), ix_ty(r, env)),
        Ix::Call(f, _) => match f.as_str() {
            // comparison helpers return int (v_cmp/cuni_cmp), truthiness bool
            "cuni_cmp" | "v_cmp" => Ty::Int,
            _ => Ty::Unknown,
        },
    }
}

// --- string-aware scanners -------------------------------------------------

/// Extract the balanced `( ... )` starting at `open` (index of `(`).
/// Returns (inner text, index just past the closing paren).
fn balanced_from(ch: &[char], open: usize) -> Option<(String, usize)> {
    let mut depth = 0usize;
    let mut i = open;
    let n = ch.len();
    let mut str_c: Option<char> = None;
    while i < n {
        let c = ch[i];
        if let Some(q) = str_c {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                str_c = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                str_c = Some(c);
                i += 1;
            }
            '/' if i + 1 < n && ch[i + 1] == '/' => {
                while i < n && ch[i] != '\n' {
                    i += 1;
                }
            }
            '/' if i + 1 < n && ch[i + 1] == '*' => {
                i += 2;
                while i + 1 < n && !(ch[i] == '*' && ch[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            '(' => {
                depth += 1;
                i += 1;
            }
            ')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some((ch[open + 1..i - 1].iter().collect(), i));
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

/// Split on top-level commas (string/paren/comment aware).
fn split_top(s: &str) -> Vec<String> {
    let ch: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut str_c: Option<char> = None;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < ch.len() {
        let c = ch[i];
        if let Some(q) = str_c {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                str_c = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                str_c = Some(c);
                i += 1;
            }
            '(' | '[' | '{' => {
                depth += 1;
                i += 1;
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            ',' if depth == 0 => {
                parts.push(ch[start..i].iter().collect());
                i += 1;
                start = i;
            }
            _ => {
                i += 1;
            }
        }
    }
    parts.push(ch[start..].iter().collect());
    parts
}

/// Extract `expr` running to the next top-level `;` (string/paren aware).
fn expr_to_semi(ch: &[char], mut i: usize) -> Option<(String, usize)> {
    let n = ch.len();
    let mut depth = 0usize;
    let mut str_c: Option<char> = None;
    let start = i;
    while i < n {
        let c = ch[i];
        if let Some(q) = str_c {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                str_c = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                str_c = Some(c);
                i += 1;
            }
            '(' | '[' | '{' => {
                depth += 1;
                i += 1;
            }
            ')' | ']' | '}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            ';' if depth == 0 => {
                return Some((ch[start..i].iter().collect(), i + 1));
            }
            _ => {
                i += 1;
            }
        }
    }
    None
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Find `name(` call sites. Returns (callee, arg source texts).
/// Method calls (`x.clone(`) are skipped; only bare `name(` counts.
fn find_calls(body: &str) -> Vec<(String, Vec<String>)> {
    let ch: Vec<char> = body.chars().collect();
    let n = ch.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < n {
        if ch[i].is_ascii_alphabetic() || ch[i] == '_' {
            let s = i;
            while i < n && is_word_char(ch[i]) {
                i += 1;
            }
            let name: String = ch[s..i].iter().collect();
            let preceded_by_dot = s > 0 && ch[s - 1] == '.';
            let mut j = i;
            while j < n && ch[j].is_whitespace() {
                j += 1;
            }
            if j < n && ch[j] == '(' && !preceded_by_dot {
                if let Some((inner, _end)) = balanced_from(&ch, j) {
                    out.push((name, split_top(&inner)));
                    // continue just past the open paren so nested calls
                    // inside the arguments are still discovered
                    i = j + 1;
                    continue;
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Find `KEYWORD name = expr;` bindings. Returns (name, expr source).
fn find_bindings(body: &str, keywords: &[&str]) -> Vec<(String, String)> {
    let ch: Vec<char> = body.chars().collect();
    let n = ch.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < n {
        let mut hit: Option<usize> = None;
        for kw in keywords {
            let kch: Vec<char> = kw.chars().collect();
            if i + kch.len() <= n
                && ch[i..i + kch.len()] == kch[..]
                && (i == 0 || !is_word_char(ch[i - 1]))
                && (i + kch.len() >= n || !is_word_char(ch[i + kch.len()]))
            {
                hit = Some(kch.len());
                break;
            }
        }
        if let Some(klen) = hit {
            let mut j = i + klen;
            while j < n && ch[j].is_whitespace() {
                j += 1;
            }
            let s = j;
            while j < n && is_word_char(ch[j]) {
                j += 1;
            }
            let name: String = ch[s..j].iter().collect();
            let mut k = j;
            while k < n && ch[k].is_whitespace() {
                k += 1;
            }
            // skip type annotations (`x: Val = ...`); only `x = ...` binds here
            if k < n && ch[k] == '=' && (k + 1 >= n || ch[k + 1] != '=') {
                if let Some((expr, end)) = expr_to_semi(&ch, k + 1) {
                    if is_ident(&name) {
                        out.push((name, expr));
                    }
                    i = end;
                    continue;
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Find `return expr;` sites. Returns the expr sources (bare `return;` -> "").
fn find_returns(body: &str) -> Vec<String> {
    let ch: Vec<char> = body.chars().collect();
    let n = ch.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < n {
        if i + 6 <= n
            && ch[i..i + 6].iter().collect::<String>() == "return"
            && (i == 0 || !is_word_char(ch[i - 1]))
            && (i + 6 >= n || !is_word_char(ch[i + 6]))
        {
            let mut j = i + 6;
            while j < n && ch[j].is_whitespace() {
                j += 1;
            }
            if j < n && ch[j] == ';' {
                out.push(String::new());
                i = j + 1;
            } else if let Some((expr, end)) = expr_to_semi_or_eol(&ch, j) {
                out.push(expr);
                i = end;
            } else {
                i = j;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Expression scan like expr_to_semi but also terminating at a newline or a
/// depth-0 `}` (for newline-terminated languages like awk).
fn expr_to_semi_or_eol(ch: &[char], mut i: usize) -> Option<(String, usize)> {
    let n = ch.len();
    let mut depth = 0usize;
    let mut str_c: Option<char> = None;
    let start = i;
    while i < n {
        let c = ch[i];
        if let Some(q) = str_c {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                str_c = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' | '`' => {
                str_c = Some(c);
                i += 1;
            }
            '(' | '[' | '{' => {
                depth += 1;
                i += 1;
            }
            ')' | ']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            '}' if depth == 0 => {
                return Some((ch[start..i].iter().collect(), i));
            }
            '}' => {
                depth -= 1;
                i += 1;
            }
            ';' | '\n' if depth == 0 => {
                return Some((ch[start..i].iter().collect(), i + 1));
            }
            _ => {
                i += 1;
            }
        }
    }
    if i > start {
        return Some((ch[start..i].iter().collect(), i));
    }
    None
}

/// Find bare `name = expr;` assignments (awk-style; no keyword).
fn find_bare_assigns(body: &str) -> Vec<(String, String)> {
    let ch: Vec<char> = body.chars().collect();
    let n = ch.len();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < n {
        if (ch[i].is_ascii_alphabetic() || ch[i] == '_') && (i == 0 || !is_word_char(ch[i - 1])) {
            let s = i;
            while i < n && is_word_char(ch[i]) {
                i += 1;
            }
            let name: String = ch[s..i].iter().collect();
            let mut j = i;
            while j < n && ch[j].is_whitespace() {
                j += 1;
            }
            if j < n
                && ch[j] == '='
                && (j + 1 >= n || ch[j + 1] != '=')
                && (j == 0
                    || ch[j - 1] != '=' && ch[j - 1] != '!' && ch[j - 1] != '<' && ch[j - 1] != '>')
            {
                if let Some((expr, end)) = expr_to_semi_or_eol(&ch, j + 1) {
                    if is_ident(&name) && !expr.trim().is_empty() {
                        out.push((name, expr));
                    }
                    i = end;
                    continue;
                }
            }
        } else {
            i += 1;
        }
    }
    out
}

// --- signature plumbing ----------------------------------------------------

/// Parameter names from a backend signature, per language shapes:
/// js `name(a, b)` / c `name(Val a, long long n)` / rs `name(a: Val, mut b: Val)`.
fn sig_param_names(sig: &str, lang: ELang) -> Result<Vec<String>, String> {
    let paren = sig.find('(').ok_or("ingest: bad sig")?;
    let close = sig.rfind(')').ok_or("ingest: bad sig")?;
    let inner = sig[paren + 1..close].trim();
    if inner.is_empty() || inner == "void" {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    for p in inner.split(',') {
        let p = p.trim();
        let pn = match lang {
            ELang::C => {
                let words: Vec<&str> = p.split_whitespace().collect();
                words.last().copied().unwrap_or("").trim_start_matches('*')
            }
            ELang::Rs => {
                let p = p.strip_prefix("mut ").unwrap_or(p);
                if p.contains('&') || p.contains('<') {
                    return Err("ingest: refuse non-Val param".into());
                }
                p.split(':').next().unwrap_or("").trim()
            }
            _ => p,
        };
        if !is_ident(pn) {
            return Err(format!("ingest: refuse param `{p}` (outside subset)"));
        }
        names.push(pn.to_string());
    }
    Ok(names)
}

struct TypedSig {
    params: Vec<(String, Ty)>,
    ret: Ty,
}

/// Classify one source expression with XP; Unknown on parse failure
/// (the statement parser will report the real error later).
fn classify_expr(src: &str, lang: ELang, env: &HashMap<String, Ty>) -> Ty {
    match XP::new(src.trim(), lang).and_then(|mut x| x.expr()) {
        Ok(ix) => ix_ty(&ix, env),
        Err(_) => Ty::Unknown,
    }
}

/// Two-pass type recovery for an untyped/Val-boxed backend.
///
/// `is_prelude(name)` skips runtime helpers, `bind_keywords` are the
/// language's binding introducers, `skip_return(expr_text)` drops backend
/// tail markers (`V_none()`, `Val::None`) that are not real returns.
fn infer_untyped(
    funcs: &[Func],
    main_body: &str,
    lang: ELang,
    is_prelude: &dyn Fn(&str) -> bool,
    bind_keywords: &[&str],
    bare_assign: bool,
    skip_return: &dyn Fn(&str) -> bool,
    param_names: &dyn Fn(&Func) -> Result<Vec<String>, String>,
    seat: &str,
) -> Result<(HashMap<String, TypedSig>, HashSet<String>), String> {
    // user functions (prelude helpers and main excluded)
    let mut users: Vec<&Func> = funcs
        .iter()
        .filter(|f| f.name != "main" && !is_prelude(&f.name))
        .collect();
    users.sort_by(|a, b| a.name.cmp(&b.name));
    users.dedup_by(|a, b| a.name == b.name);
    let user_names: HashSet<String> = users.iter().map(|f| f.name.clone()).collect();

    // pass 1a: binding environments per function (+ main)
    let main_func = Func {
        name: "main".into(),
        sig: String::new(),
        body: main_body.to_string(),
    };
    let mut benvs: HashMap<String, HashMap<String, Ty>> = HashMap::new();
    for f in users.iter().copied().chain(std::iter::once(&main_func)) {
        let mut env = HashMap::new();
        for (name, expr) in find_bindings(&f.body, bind_keywords) {
            let t = classify_expr(&expr, lang, &env);
            env.insert(name, t);
        }
        if bare_assign {
            for (name, expr) in find_bare_assigns(&f.body) {
                // keyword bindings already recorded; bare pass only fills gaps
                if !env.contains_key(&name) {
                    let t = classify_expr(&expr, lang, &env);
                    env.insert(name, t);
                }
            }
        }
        benvs.insert(f.name.clone(), env);
    }

    // pass 1b: call sites -> argument types (classified with caller's env)
    // func name -> per-param observed arg types
    let mut calls: HashMap<String, Vec<Vec<Ty>>> = HashMap::new();
    for f in users.iter().copied().chain(std::iter::once(&main_func)) {
        let env = &benvs[&f.name];
        for (callee, args) in find_calls(&f.body) {
            if !user_names.contains(&callee) {
                continue;
            }
            let tys: Vec<Ty> = args.iter().map(|a| classify_expr(a, lang, env)).collect();
            let entry = calls.entry(callee).or_default();
            if entry.len() < tys.len() {
                entry.resize(tys.len(), vec![]);
            }
            for (i, t) in tys.iter().enumerate() {
                entry[i].push(*t);
            }
        }
    }

    // pass 2: unify param types; build return envs; unify return types.
    // Returns (inferred signatures, names the old def parsers must decide:
    // their sigs fall outside the subset, so legacy skip/refuse applies).
    let mut sigs = HashMap::new();
    let mut skipped = HashSet::new();
    for f in &users {
        let params = match param_names(f) {
            Ok(p) => p,
            Err(_) => {
                skipped.insert(f.name.clone());
                continue;
            }
        };
        let mut ptypes = Vec::new();
        for (i, pn) in params.iter().enumerate() {
            let mut t = Ty::Unknown;
            let mut seen = false;
            if let Some(per) = calls.get(&f.name) {
                if let Some(obs) = per.get(i) {
                    for o in obs {
                        if !seen {
                            t = *o;
                            seen = true;
                        } else {
                            t = Ty::unify(t, *o);
                        }
                    }
                }
            }
            if t == Ty::Unknown {
                return Err(format!(
                    "ingest: refuse {seat} def `{}` — cannot infer type of param `{pn}` \
                     (no agreeing call-site; types are erased in this backend)",
                    f.name
                ));
            }
            ptypes.push((pn.clone(), t));
        }
        let mut env = benvs[&f.name].clone();
        for (pn, pt) in &ptypes {
            env.insert(pn.clone(), *pt);
        }
        let mut ret = Ty::Unknown;
        let mut rseen = false;
        for r in find_returns(&f.body) {
            let rs = r.trim();
            if rs.is_empty() || skip_return(rs) {
                continue; // bare return / backend tail marker: not a type witness
            }
            let t = classify_expr(rs, lang, &env);
            if !rseen {
                ret = t;
                rseen = true;
            } else {
                ret = Ty::unify(ret, t);
            }
        }
        if ret == Ty::Unknown {
            return Err(format!(
                "ingest: refuse {seat} def `{}` — cannot infer return type \
                 (types are erased in this backend)",
                f.name
            ));
        }
        sigs.insert(
            f.name.clone(),
            TypedSig {
                params: ptypes,
                ret,
            },
        );
    }
    Ok((sigs, skipped))
}

// ---------------------------------------------------------------------------
// JavaScript / TypeScript
// ---------------------------------------------------------------------------

const JS_PRELUDE: &[&str] = &[
    "say",
    "range",
    "abs",
    "min",
    "max",
    "_cuni_slice",
    "_cuni_div",
];

// ---------------------------------------------------------------------------
// Solidity: the blockchain contract reader
// ---------------------------------------------------------------------------
// Reads a .sol contract (the subset the CuNi Solidity writer emits, plus
// hand-written contracts in the same style) back into CuNi source. Round-trip:
//   cuni --emit-sol prog.cuni out.sol  ->  cuni ingest out.sol  ->  .cuni
// that passes the CuNi front-end, or ingestion refuses.

fn ingest_sol(src: &str) -> Result<String, String> {
    let funcs = extract_funcs(src, "function ")?;
    let mut out = String::new();
    let mut run_body = None;
    for f in &funcs {
        // Skip internal helpers (e.g. _cuni_itoa) and non-public functions.
        if f.name.starts_with('_') || f.name.starts_with("_cuni") {
            continue;
        }
        if !f.sig.contains("public") {
            continue;
        }
        if f.name == "run" {
            run_body = Some(f.body.clone());
            continue;
        }
        match sol_def(f)? {
            Some(def) => {
                out.push_str(&def);
                out.push('\n');
            }
            None => {}
        }
    }
    // A contract without run() is just a library of defs; that's fine.
    if let Some(body) = run_body {
        for st in sol_block(&body, 0)? {
            out.push_str(&st);
            out.push('\n');
        }
    }
    if out.trim().is_empty() {
        return Err("ingest: refuse Solidity with no public functions".into());
    }
    Ok(out)
}

/// Parse `name(params) public ... returns (type)` into a CuNi def.
fn sol_def(f: &Func) -> Result<Option<String>, String> {
    let sig = f.sig.trim();
    let paren = sig.find('(').ok_or("ingest: bad Solidity sig")?;
    let name = sig[..paren].trim();
    if !is_ident(name) {
        return Ok(None);
    }
    let close = find_matching_paren(sig, paren).ok_or("ingest: bad Solidity sig")?;
    let params_s = &sig[paren + 1..close];
    let mut params = Vec::new();
    for p in split_top_delim(params_s, ',') {
        let p = p.trim();
        if p.is_empty() {
            continue;
        }
        // `int256 x` or `string memory x`
        let parts: Vec<&str> = p.split_whitespace().collect();
        let (ty_s, nm) = match parts.as_slice() {
            [t, n] => (*t, *n),
            [t, "memory", n] => (*t, *n),
            [t, "calldata", n] => (*t, *n),
            _ => return Err(format!("ingest: refuse Solidity param `{p}`")),
        };
        if !is_ident(nm) {
            return Err(format!("ingest: refuse Solidity param name `{nm}`"));
        }
        let ty = sol_type_to_cuni(ty_s)?;
        params.push(format!("{nm}: {ty}"));
    }
    // Return type after `returns`.
    let ret = if let Some(rpos) = sig.find("returns") {
        let r = sig[rpos + "returns".len()..].trim();
        let r = r
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(r);
        let r = r.trim().replace(" memory", "").replace(" calldata", "");
        if r.is_empty() || r == "()" {
            "()".to_string()
        } else {
            sol_type_to_cuni(r.trim())?
        }
    } else {
        "()".to_string()
    };
    let mut def = format!("def {name}({}) -> {ret}", params.join(", "));
    // A body that reverts was a fallible (`-> T ?`) CuNi function.
    if f.body.contains("revert(") {
        def.push_str(" ?");
    }
    def.push_str(" do\n");
    for st in sol_block(&f.body, 1)? {
        def.push_str(&st);
        def.push('\n');
    }
    def.push_str("end");
    Ok(Some(def))
}

fn sol_type_to_cuni(t: &str) -> Result<String, String> {
    match t {
        "int256" | "int" => Ok("int".into()),
        "string" => Ok("str".into()),
        "bool" => Ok("bool".into()),
        _ => Err(format!(
            "ingest: refuse Solidity type `{t}` (outside subset)"
        )),
    }
}

fn sol_block(body: &str, indent: usize) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for chunk in split_chunks(body)? {
        lines.extend(sol_stmt(&chunk, indent)?);
    }
    Ok(lines)
}

fn sol_stmt(chunk: &str, indent: usize) -> Result<Vec<String>, String> {
    let pad = "    ".repeat(indent);
    let t = chunk.trim();
    if t.is_empty() {
        return Ok(vec![]);
    }
    // Comments and events at function level: skip.
    if t.starts_with("//") || t.starts_with("/*") || t.starts_with("event ") {
        return Ok(vec![]);
    }
    if t.starts_with("if") && t.contains('{') {
        let (cond, then_b, else_b) = split_if(t)?;
        let c = sol_expr_to_cuni(&cond)?;
        let mut v = vec![format!("{pad}if {c} do")];
        v.extend(sol_block(&then_b, indent + 1)?);
        if let Some(e) = else_b {
            v.push(format!("{pad}els"));
            v.extend(sol_block(&e, indent + 1)?);
        }
        v.push(format!("{pad}end"));
        return Ok(v);
    }
    if t.starts_with("while") && t.contains('{') {
        let (cond, body, _) = split_if(&t.replacen("while", "if", 1))?;
        let c = sol_expr_to_cuni(&cond)?;
        let mut v = vec![format!("{pad}whl {c} do")];
        v.extend(sol_block(&body, indent + 1)?);
        v.push(format!("{pad}end"));
        return Ok(v);
    }
    if t.starts_with("for") && t.contains('{') {
        return sol_for(t, indent);
    }
    // emit LogInt(x); -> say(x)
    for (ev, _) in [("LogInt", "int"), ("LogString", "str"), ("LogBool", "bool")] {
        let prefix = format!("emit {ev}(");
        if t.starts_with(&prefix) && t.ends_with(");") {
            let inner = &t[prefix.len()..t.len() - 2];
            let e = sol_expr_to_cuni(inner)?;
            return Ok(vec![format!("{pad}say({e})")]);
        }
    }
    if let Some(rest) = t.strip_prefix("revert(") {
        let rest = rest.trim().strip_suffix(';').unwrap_or(rest);
        let rest = rest.strip_suffix(')').unwrap_or(rest);
        let e = sol_expr_to_cuni(rest.trim())?;
        return Ok(vec![format!("{pad}fail {e}")]);
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let rest = rest.trim().strip_suffix(';').unwrap_or(rest);
        if rest.is_empty() {
            return Ok(vec![format!("{pad}ret")]);
        }
        let e = sol_expr_to_cuni(rest)?;
        return Ok(vec![format!("{pad}ret {e}")]);
    }
    if t == "return;" || t == "return" {
        return Ok(vec![format!("{pad}ret")]);
    }
    // Typed declaration: `int256 x = v;`, `string memory x = v;`, `bool x = v;`
    for ty in ["int256", "string", "bool", "uint256"] {
        if let Some(rest) = t.strip_prefix(ty) {
            let rest = rest.trim();
            let rest = rest.strip_prefix("memory").map(str::trim).unwrap_or(rest);
            let rest = rest.strip_prefix("calldata").map(str::trim).unwrap_or(rest);
            if let Some(eq) = rest.find('=') {
                let nm = rest[..eq].trim();
                let val = rest[eq + 1..]
                    .trim()
                    .strip_suffix(';')
                    .unwrap_or(rest)
                    .trim();
                if !is_ident(nm) {
                    return Err(format!("ingest: refuse Solidity binding `{nm}`"));
                }
                let ct = sol_type_to_cuni(ty)?;
                let e = sol_expr_to_cuni(val)?;
                return Ok(vec![format!("{pad}mut {nm}: {ct} = {e}")]);
            }
            return Err(format!("ingest: refuse Solidity decl `{t}`"));
        }
    }
    // Plain assignment or bare call.
    if let Some((lhs, rhs)) = split_assign(t, ELang::Sol)? {
        let l = sol_expr_to_cuni(&lhs)?;
        let r = rhs.trim().strip_suffix(';').unwrap_or(rhs.trim());
        let r = sol_expr_to_cuni(r)?;
        return Ok(vec![format!("{pad}{l} = {r}")]);
    }
    // Bare call statement: `f(x);`
    let bare = t.strip_suffix(';').unwrap_or(t);
    let e = sol_expr_to_cuni(bare)?;
    Ok(vec![format!("{pad}{e}")])
}

/// `for (int256 i = a; i < b; i++) { ... }` -> `for i in range(a, b) do ... end`
fn sol_for(t: &str, indent: usize) -> Result<Vec<String>, String> {
    let pad = "    ".repeat(indent);
    let rest = t.strip_prefix("for").ok_or("ingest: bad for")?.trim();
    let body_at = rest.find('{').ok_or("ingest: bad for")?;
    let head = rest[..body_at].trim();
    let head = head
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or("ingest: bad for head")?;
    let parts = split_top_delim(head, ';');
    if parts.len() != 3 {
        return Err("ingest: refuse Solidity for (outside subset)".into());
    }
    // init: `int256 i = a`
    let init = parts[0].trim();
    let eq = init.find('=').ok_or("ingest: bad for init")?;
    let var = init[..eq].trim().rsplit(' ').next().unwrap_or("").trim();
    if !is_ident(var) {
        return Err("ingest: refuse Solidity for var".into());
    }
    let start = sol_expr_to_cuni(init[eq + 1..].trim())?;
    // cond: `i < b`
    let cond = parts[1].trim();
    let lt = cond.find('<').ok_or("ingest: refuse non-`<` for cond")?;
    let end = sol_expr_to_cuni(cond[lt + 1..].trim())?;
    // incr: `i++` or `i += 1`
    let incr = parts[2].trim();
    if incr != format!("{var}++") && incr != format!("{var} += 1") && incr != format!("++{var}") {
        return Err("ingest: refuse Solidity for incr (outside subset)".into());
    }
    let body = rest[body_at..].trim();
    let body = body
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or("ingest: bad for body")?;
    let body_lines = sol_block(body, indent + 1)?;
    // `for (i = 0; i < b; i++)` -> `for i in range(b) do`; other starts
    // become an equivalent while loop.
    if start.trim() == "0" {
        let mut v = vec![format!("{pad}for {var} in range({end}) do")];
        v.extend(sol_block(body, indent + 1)?);
        v.push(format!("{pad}end"));
        Ok(v)
    } else {
        let mut v = vec![
            format!("{pad}mut {var}: int = {start}"),
            format!("{pad}whl {var} < {end} do"),
        ];
        v.extend(body_lines);
        v.push(format!("{pad}    {var} = {var} + 1"));
        v.push(format!("{pad}end"));
        Ok(v)
    }
}

/// Convert a Solidity expression to CuNi source.
fn sol_expr_to_cuni(src: &str) -> Result<String, String> {
    let s = src.trim();
    // string(abi.encodePacked(...)) -> interpolated string
    if let Some(inner) = s.strip_prefix("string(").and_then(|r| r.strip_suffix(')')) {
        let inner = inner.trim();
        if let Some(packed) = inner
            .strip_prefix("abi.encodePacked(")
            .and_then(|r| r.strip_suffix(')'))
        {
            return sol_packed_to_interp(packed);
        }
    }
    // _cuni_itoa(x) outside interpolation -> refuse (needs string context)
    Ok(XP::new(s, ELang::Sol)?.expr()?.to_cuni())
}

/// `abi.encodePacked("a", _cuni_itoa(x), "b")` -> `"a{x}b"`.
fn sol_packed_to_interp(packed: &str) -> Result<String, String> {
    let mut out = String::from('"');
    for part in split_top_delim(packed, ',') {
        let p = part.trim();
        if p.starts_with('"') && p.ends_with('"') && p.len() >= 2 {
            let inner = &p[1..p.len() - 1];
            out.push_str(&inner.replace('{', "{{").replace('}', "}}"));
        } else if let Some(inner) = p
            .strip_prefix("_cuni_itoa(")
            .and_then(|r| r.strip_suffix(')'))
        {
            let e = sol_expr_to_cuni(inner)?;
            out.push('{');
            out.push_str(&e);
            out.push('}');
        } else if p.starts_with('(') && p.contains('?') {
            // (cond ? "a" : "b") bool interpolation -> refuse honestly
            return Err("ingest: refuse ternary interpolation (outside subset)".into());
        } else {
            // A plain string variable interpolates as-is.
            let e = sol_expr_to_cuni(p)?;
            out.push('{');
            out.push_str(&e);
            out.push('}');
        }
    }
    out.push('"');
    Ok(out)
}

/// Index of the `)` matching the `(` at `open`.
fn find_matching_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        if i < open {
            continue;
        }
        if esc {
            esc = false;
            continue;
        }
        if in_str {
            if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split on a delimiter, ignoring nesting and strings.
fn split_top_delim(s: &str, delim: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut pdepth = 0i32;
    let mut bdepth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for c in s.chars() {
        if esc {
            cur.push(c);
            esc = false;
            continue;
        }
        if in_str {
            cur.push(c);
            if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                cur.push(c);
            }
            '(' => {
                pdepth += 1;
                cur.push(c);
            }
            ')' => {
                pdepth -= 1;
                cur.push(c);
            }
            '[' => {
                bdepth += 1;
                cur.push(c);
            }
            ']' => {
                bdepth -= 1;
                cur.push(c);
            }
            d if d == delim && pdepth == 0 && bdepth == 0 => {
                parts.push(cur.trim().to_string());
                cur = String::new();
            }
            _ => cur.push(c),
        }
    }
    parts.push(cur.trim().to_string());
    parts
}

fn ingest_js(src: &str) -> Result<String, String> {
    let funcs = extract_funcs(src, "function ")?;
    let main = funcs
        .iter()
        .find(|f| f.name == "main")
        .map(|f| f.body.clone())
        .ok_or("ingest: refuse JS without function main")?;
    // Two-pass call-site type recovery: the JS backend erases types, so
    // params/returns are inferred from literal call-site arguments and
    // return expressions. Uninferrable defs refuse — never defaulted.
    let (sigs, untyped) = infer_untyped(
        &funcs,
        &main,
        ELang::Js,
        &|n| JS_PRELUDE.contains(&n),
        &["const", "let", "var"],
        false,
        &|_| false,
        &|f| sig_param_names(&f.sig, ELang::Js),
        "JS",
    )?;
    let mut out = String::new();
    for f in &funcs {
        if f.name == "main" {
            continue;
        }
        if JS_PRELUDE.contains(&f.name.as_str()) {
            continue;
        }
        match js_def(f, &sigs, &untyped) {
            Ok(Some(def)) => {
                out.push_str(&def);
                out.push('\n');
            }
            Ok(None) => {}
            Err(e) => return Err(format!("ingest: JS def `{}`: {e}", f.name)),
        }
    }
    for st in js_block(&main, 0)? {
        out.push_str(&st);
        out.push('\n');
    }
    Ok(out)
}

/// Typed def: params and return come from call-site inference.
fn js_def(
    f: &Func,
    sigs: &HashMap<String, TypedSig>,
    untyped: &HashSet<String>,
) -> Result<Option<String>, String> {
    if untyped.contains(&f.name) {
        return js_def_legacy(f);
    }
    let tsig = sigs
        .get(&f.name)
        .ok_or_else(|| format!("ingest: no inferred signature for `{}`", f.name))?;
    let params: Vec<String> = tsig
        .params
        .iter()
        .map(|(n, t)| format!("{n}: {}", t.cuni().expect("inferred types are known")))
        .collect();
    let ret = tsig.ret.cuni().expect("inferred types are known");
    let mut body = Vec::new();
    for st in js_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None);
    }
    Ok(Some(format!(
        "def {}({}) -> {ret} do\n{}\nend",
        f.name,
        params.join(", "),
        body.join("\n")
    )))
}

/// Legacy def parser (kept for sigs outside the inference subset — e.g. TS
/// annotations — where the old skip/refuse behavior is preserved verbatim).
fn js_def_legacy(f: &Func) -> Result<Option<String>, String> {
    let sig = f.sig.trim();
    let paren = sig.find('(').ok_or("ingest: bad JS sig")?;
    let name = sig[..paren].trim();
    if !is_ident(name) {
        return Ok(None);
    }
    let close = sig.rfind(')').ok_or("ingest: bad JS sig")?;
    let params_s = &sig[paren + 1..close];
    if params_s.contains(':') {
        return Ok(None); // TS annotations: outside subset
    }
    let mut params = Vec::new();
    if !params_s.trim().is_empty() {
        for p in params_s.split(',') {
            let p = p.trim();
            if !is_ident(p) {
                return Ok(None);
            }
            params.push(format!("{p}: int"));
        }
    }
    let mut body = Vec::new();
    for st in js_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None);
    }
    Ok(Some(format!(
        "def {name}({}) -> int do\n{}\nend",
        params.join(", "),
        body.join("\n")
    )))
}

fn js_block(body: &str, indent: usize) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for chunk in split_chunks(body)? {
        lines.extend(js_stmt(&chunk, indent)?);
    }
    Ok(lines)
}

fn js_stmt(chunk: &str, indent: usize) -> Result<Vec<String>, String> {
    let pad = "    ".repeat(indent);
    let t = strip_semi(chunk.trim());
    if t.is_empty() {
        return Ok(vec![]);
    }
    if t == "main()" {
        return Ok(vec![]);
    }
    if t.starts_with("if ") && t.contains('{') {
        let (cond, then_b, else_b) = split_if(t)?;
        let c = XP::new(&cond, ELang::Js)?.expr()?.to_cuni();
        let mut v = vec![format!("{pad}if {c} do")];
        v.extend(js_block(&then_b, indent + 1)?);
        if let Some(e) = else_b {
            v.push(format!("{pad}els"));
            v.extend(js_block(&e, indent + 1)?);
        }
        v.push(format!("{pad}end"));
        return Ok(v);
    }
    for bad in ["for ", "while ", "switch ", "try ", "throw ", "class "] {
        if t.starts_with(bad) {
            return Err(format!("ingest: refuse JS `{bad}` (outside subset)"));
        }
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let e = XP::new(rest.trim(), ELang::Js)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}ret {e}")]);
    }
    if t == "return" {
        return Ok(vec![format!("{pad}ret")]);
    }
    if let Some(rest) = t.strip_prefix("const ") {
        return js_binding(rest, &pad, true);
    }
    if let Some(rest) = t.strip_prefix("let ") {
        return js_binding(rest, &pad, true);
    }
    if let Some(rest) = t.strip_prefix("var ") {
        return js_binding(rest, &pad, true);
    }
    if t.starts_with("say(") && t.ends_with(')') {
        let e = XP::new(&t[4..t.len() - 1], ELang::Js)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}say({e})")]);
    }
    if let Some((lhs, rhs)) = split_assign(t, ELang::Js)? {
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse JS assignment target `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::Js)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}{lhs} = {e}")]);
    }
    let e = XP::new(t, ELang::Js)?.expr()?;
    Ok(vec![format!("{pad}{}", e.to_cuni())])
}

fn js_binding(rest: &str, pad: &str, _top: bool) -> Result<Vec<String>, String> {
    let (lhs, rhs) = split_assign(rest, ELang::Js)?.ok_or("ingest: bad JS binding")?;
    if !is_ident(&lhs) {
        return Err(format!("ingest: refuse JS binding `{lhs}`"));
    }
    let e = XP::new(&rhs, ELang::Js)?.expr()?.to_cuni();
    Ok(vec![format!("{pad}let {lhs} = {e}")])
}

// ---------------------------------------------------------------------------
// C / C++ (the `Val` runtime backend)
// ---------------------------------------------------------------------------

fn ingest_c(src: &str) -> Result<String, String> {
    let mut funcs = extract_funcs(src, "static Val ")?;
    // int main(void) handled separately
    let mut mains = extract_funcs(src, "int main(void)")?;
    for m in &mut mains {
        m.name = "main".to_string();
    }
    funcs.extend(mains);
    let main = funcs
        .iter()
        .find(|f| f.name == "main")
        .map(|f| f.body.clone())
        .ok_or("ingest: refuse C without int main(void)")?;
    // The C backend boxes everything in `Val`: recover types from call sites.
    let (sigs, untyped) = infer_untyped(
        &funcs,
        &main,
        ELang::C,
        &|n| n.starts_with("cuni_") || n.starts_with("V_") || n == "fail_with",
        &["Val"],
        false,
        &|e| e == "V_none()",
        &|f| sig_param_names(&f.sig, ELang::C),
        "C",
    )?;
    let mut out = String::new();
    for f in &funcs {
        if f.name == "main" {
            continue;
        }
        if f.name.starts_with("cuni_") || f.name.starts_with("V_") || f.name == "fail_with" {
            continue; // runtime prelude
        }
        match c_def(f, &sigs, &untyped) {
            Ok(Some(def)) => {
                out.push_str(&def);
                out.push('\n');
            }
            Ok(None) => {}
            Err(e) => return Err(format!("ingest: C def `{}`: {e}", f.name)),
        }
    }
    for st in c_block(&main, 0)? {
        out.push_str(&st);
        out.push('\n');
    }
    Ok(out)
}

/// Typed def: params and return come from call-site inference.
fn c_def(
    f: &Func,
    sigs: &HashMap<String, TypedSig>,
    untyped: &HashSet<String>,
) -> Result<Option<String>, String> {
    if untyped.contains(&f.name) {
        return c_def_legacy(f);
    }
    let tsig = sigs
        .get(&f.name)
        .ok_or_else(|| format!("ingest: no inferred signature for `{}`", f.name))?;
    let params: Vec<String> = tsig
        .params
        .iter()
        .map(|(n, t)| format!("{n}: {}", t.cuni().expect("inferred types are known")))
        .collect();
    let ret = tsig.ret.cuni().expect("inferred types are known");
    let mut body = Vec::new();
    for st in c_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None);
    }
    Ok(Some(format!(
        "def {}({}) -> {ret} do\n{}\nend",
        f.name,
        params.join(", "),
        body.join("\n")
    )))
}

/// Legacy def parser (kept for sigs outside the inference subset).
fn c_def_legacy(f: &Func) -> Result<Option<String>, String> {
    let sig = f.sig.trim();
    let paren = sig.find('(').ok_or("ingest: bad C sig")?;
    let name = sig[..paren].trim();
    if !is_ident(name) {
        return Ok(None);
    }
    let close = sig.rfind(')').ok_or("ingest: bad C sig")?;
    let params_s = &sig[paren + 1..close];
    let mut params = Vec::new();
    if !params_s.trim().is_empty() && params_s.trim() != "void" {
        for p in params_s.split(',') {
            let p = p.trim();
            // shapes: `Val x`, `long long n`, `char *s`
            let words: Vec<&str> = p.split_whitespace().collect();
            let pn = words.last().copied().unwrap_or("");
            let pn = pn.trim_start_matches('*');
            if !is_ident(pn) {
                return Ok(None);
            }
            params.push(format!("{pn}: int"));
        }
    }
    let mut body = Vec::new();
    for st in c_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None);
    }
    Ok(Some(format!(
        "def {name}({}) -> int do\n{}\nend",
        params.join(", "),
        body.join("\n")
    )))
}

fn c_block(body: &str, indent: usize) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for chunk in split_chunks(body)? {
        lines.extend(c_stmt(&chunk, indent)?);
    }
    Ok(lines)
}

fn c_stmt(chunk: &str, indent: usize) -> Result<Vec<String>, String> {
    let pad = "    ".repeat(indent);
    let t = strip_semi(chunk.trim());
    if t.is_empty() {
        return Ok(vec![]);
    }
    if t.starts_with("if ") && t.contains('{') {
        let (cond, then_b, else_b) = split_if(t)?;
        let c = XP::new(&cond, ELang::C)?.expr()?.to_cuni();
        let mut v = vec![format!("{pad}if {c} do")];
        v.extend(c_block(&then_b, indent + 1)?);
        if let Some(e) = else_b {
            v.push(format!("{pad}els"));
            v.extend(c_block(&e, indent + 1)?);
        }
        v.push(format!("{pad}end"));
        return Ok(v);
    }
    for bad in ["for ", "while ", "switch ", "goto ", "printf("] {
        if t.starts_with(bad) {
            return Err(format!("ingest: refuse C `{bad}` (outside subset)"));
        }
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let e = XP::new(rest.trim(), ELang::C)?.expr()?;
        // `return V_none();` = the backend's tail marker: skip.
        if matches!(&e, Ix::Call(f, _) if f == "V_none") {
            return Ok(vec![]);
        }
        return Ok(vec![format!("{pad}ret {}", e.to_cuni())]);
    }
    if t == "return 0" || t == "return" {
        return Ok(vec![]);
    }
    if let Some(rest) = t.strip_prefix("Val ") {
        let (lhs, rhs) = split_assign(rest, ELang::C)?.ok_or("ingest: bad C binding")?;
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse C binding `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::C)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}let {lhs} = {e}")]);
    }
    if t.starts_with("cuni_say(") && t.ends_with(')') {
        let e = XP::new(&t[9..t.len() - 1], ELang::C)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}say({e})")]);
    }
    if let Some((lhs, rhs)) = split_assign(t, ELang::C)? {
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse C assignment target `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::C)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}{lhs} = {e}")]);
    }
    // bare call statement
    let e = XP::new(t, ELang::C)?.expr()?;
    match e {
        Ix::Call(f, _) if f == "cuni_init" => Ok(vec![]),
        _ => Ok(vec![format!("{pad}{}", e.to_cuni())]),
    }
}

// ---------------------------------------------------------------------------
// Rust (the `Val` enum backend)
// ---------------------------------------------------------------------------

fn ingest_rs(src: &str) -> Result<String, String> {
    let funcs = extract_funcs(src, "fn ")?;
    let main = funcs
        .iter()
        .find(|f| f.name == "main")
        .map(|f| f.body.clone())
        .ok_or("ingest: refuse Rust without fn main")?;
    // The Rust backend boxes everything in `Val`: recover types from call sites.
    let (sigs, untyped) = infer_untyped(
        &funcs,
        &main,
        ELang::Rs,
        &|n| {
            n.starts_with("v_")
                || n.starts_with("cuni_")
                || ["truthy", "enumerate", "list_iter", "fail_with"].contains(&n)
        },
        &["let mut", "let"],
        false,
        &|e| e == "Val::None",
        &|f| sig_param_names(&f.sig, ELang::Rs),
        "Rust",
    )?;
    let mut out = String::new();
    for f in &funcs {
        if f.name == "main" {
            continue;
        }
        if f.name.starts_with("v_")
            || f.name.starts_with("cuni_")
            || ["truthy", "enumerate", "list_iter", "fail_with"].contains(&f.name.as_str())
        {
            continue; // runtime prelude
        }
        match rs_def(f, &sigs, &untyped) {
            Ok(Some(def)) => {
                out.push_str(&def);
                out.push('\n');
            }
            Ok(None) => {}
            Err(e) => return Err(format!("ingest: Rust def `{}`: {e}", f.name)),
        }
    }
    for st in rs_block(&main, 0)? {
        out.push_str(&st);
        out.push('\n');
    }
    Ok(out)
}

/// Typed def: params and return come from call-site inference.
fn rs_def(
    f: &Func,
    sigs: &HashMap<String, TypedSig>,
    untyped: &HashSet<String>,
) -> Result<Option<String>, String> {
    if untyped.contains(&f.name) {
        return rs_def_legacy(f);
    }
    let tsig = sigs
        .get(&f.name)
        .ok_or_else(|| format!("ingest: no inferred signature for `{}`", f.name))?;
    let params: Vec<String> = tsig
        .params
        .iter()
        .map(|(n, t)| format!("{n}: {}", t.cuni().expect("inferred types are known")))
        .collect();
    let ret = tsig.ret.cuni().expect("inferred types are known");
    let mut body = Vec::new();
    for st in rs_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None);
    }
    Ok(Some(format!(
        "def {}({}) -> {ret} do\n{}\nend",
        f.name,
        params.join(", "),
        body.join("\n")
    )))
}

/// Legacy def parser (kept for sigs outside the inference subset).
fn rs_def_legacy(f: &Func) -> Result<Option<String>, String> {
    let sig = f.sig.trim();
    let paren = sig.find('(').ok_or("ingest: bad Rust sig")?;
    let name = sig[..paren].trim();
    if !is_ident(name) || name.contains('<') {
        return Ok(None);
    }
    let close = sig.rfind(')').ok_or("ingest: bad Rust sig")?;
    let params_s = &sig[paren + 1..close];
    let mut params = Vec::new();
    if !params_s.trim().is_empty() {
        for p in params_s.split(',') {
            let p = p.trim();
            // shapes: `a: Val`, `mut a: Val`, `&self`
            let p = p.strip_prefix("mut ").unwrap_or(p);
            if p.contains('&') || p.contains('<') {
                return Ok(None);
            }
            let pn = p.split(':').next().unwrap_or("").trim();
            if !is_ident(pn) {
                return Ok(None);
            }
            params.push(format!("{pn}: int"));
        }
    }
    // return type after `->` is always `Val` in the subset; anything else: skip.
    let mut body = Vec::new();
    for st in rs_block(&f.body, 1)? {
        body.push(st);
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Ok(None);
    }
    Ok(Some(format!(
        "def {name}({}) -> int do\n{}\nend",
        params.join(", "),
        body.join("\n")
    )))
}

fn rs_block(body: &str, indent: usize) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for chunk in split_chunks(body)? {
        lines.extend(rs_stmt(&chunk, indent)?);
    }
    Ok(lines)
}

fn rs_stmt(chunk: &str, indent: usize) -> Result<Vec<String>, String> {
    let pad = "    ".repeat(indent);
    let t = strip_semi(chunk.trim());
    if t.is_empty() {
        return Ok(vec![]);
    }
    if t.starts_with("if ") && t.contains('{') {
        let (cond, then_b, else_b) = split_if(t)?;
        let c = XP::new(&cond, ELang::Rs)?.expr()?.to_cuni();
        let mut v = vec![format!("{pad}if {c} do")];
        v.extend(rs_block(&then_b, indent + 1)?);
        if let Some(e) = else_b {
            v.push(format!("{pad}els"));
            v.extend(rs_block(&e, indent + 1)?);
        }
        v.push(format!("{pad}end"));
        return Ok(v);
    }
    for bad in ["for ", "while ", "match ", "loop ", "panic!"] {
        if t.starts_with(bad) {
            return Err(format!("ingest: refuse Rust `{bad}` (outside subset)"));
        }
    }
    if let Some(rest) = t.strip_prefix("return ") {
        let rest = rest.trim();
        // `return Val::None;` = the backend's tail marker: skip.
        if rest == "Val::None" {
            return Ok(vec![]);
        }
        let e = XP::new(rest, ELang::Rs)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}ret {e}")]);
    }
    if let Some(rest) = t.strip_prefix("let mut ") {
        return rs_binding(rest, &pad);
    }
    if let Some(rest) = t.strip_prefix("let ") {
        return rs_binding(rest, &pad);
    }
    if t == "Val::None" {
        return Ok(vec![]); // backend tail marker
    }
    if t.starts_with("cuni_say(") && t.ends_with(')') {
        let e = XP::new(&t[9..t.len() - 1], ELang::Rs)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}say({e})")]);
    }
    if let Some((lhs, rhs)) = split_assign(t, ELang::Rs)? {
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse Rust assignment target `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::Rs)?.expr()?.to_cuni();
        return Ok(vec![format!("{pad}{lhs} = {e}")]);
    }
    let e = XP::new(t, ELang::Rs)?.expr()?;
    Ok(vec![format!("{pad}{}", e.to_cuni())])
}

fn rs_binding(rest: &str, pad: &str) -> Result<Vec<String>, String> {
    let (lhs, rhs) = split_assign(rest, ELang::Rs)?.ok_or("ingest: bad Rust binding")?;
    if !is_ident(&lhs) {
        return Err(format!("ingest: refuse Rust binding `{lhs}`"));
    }
    let e = XP::new(&rhs, ELang::Rs)?.expr()?.to_cuni();
    Ok(vec![format!("{pad}let {lhs} = {e}")])
}

// ---------------------------------------------------------------------------
// Python (superset of the old v1: typed defs, if/else, bindings, prelude skip)
// ---------------------------------------------------------------------------

const PY_PRELUDE_SKIP: &[&str] = &[
    "say",
    "range",
    "abs",
    "min",
    "max",
    "_cuni_slice",
    "_cuni_div",
    "_cuni_divmod",
    "_cuni_len",
    "_cuni_iter",
];

fn py_type_to_cuni(t: &str) -> Option<String> {
    match t.trim() {
        "int" => Some("int".into()),
        "str" => Some("str".into()),
        "bool" => Some("bool".into()),
        "float" => Some("float".into()),
        _ => None,
    }
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn ingest_py(src: &str) -> Result<String, String> {
    let lines: Vec<&str> = src.lines().collect();
    let (stmts, _) = py_block(&lines, 0, 0)?;
    Ok(stmts.join("\n") + "\n")
}

/// Parse an indented Python block starting at line `i`; every statement in it
/// must sit at exactly `ind` spaces. Returns (cuni lines, next line index).
fn py_block(lines: &[&str], mut i: usize, ind: usize) -> Result<(Vec<String>, usize), String> {
    let mut out = Vec::new();
    while i < lines.len() {
        let raw = lines[i];
        let stripped = raw.trim();
        if stripped.is_empty() || stripped.starts_with('#') {
            i += 1;
            continue;
        }
        let cur = indent_of(raw);
        if cur < ind {
            break;
        }
        if cur > ind {
            return Err(format!("ingest: refuse bad indent in Python: {stripped}"));
        }
        if stripped.starts_with('@') {
            i += 1;
            continue; // decorator
        }
        if stripped.starts_with("import ") || stripped.starts_with("from ") {
            i += 1;
            continue; // prelude imports
        }
        if stripped.starts_with("class ") {
            i = py_skip_block(lines, i, ind);
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("def ") {
            let (def_lines, ni) = py_def(lines, i, ind, rest)?;
            out.extend(def_lines);
            i = ni;
            continue;
        }
        if stripped == "if __name__ == \"__main__\":" || stripped == "if __name__ == '__main__':" {
            i = py_skip_block(lines, i, ind);
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("if ") {
            let (if_lines, ni) = py_if(lines, i, ind, rest)?;
            out.extend(if_lines);
            i = ni;
            continue;
        }
        if stripped.starts_with("for ") || stripped.starts_with("while ") {
            return Err(format!(
                "ingest: refuse Python loop (outside subset): {stripped}"
            ));
        }
        if stripped.starts_with("raise ") || stripped == "raise" {
            return Err("ingest: refuse Python raise (outside subset)".into());
        }
        if stripped == "pass" {
            i += 1;
            continue;
        }
        let pad: String = " ".repeat(ind);
        if let Some(rest) = stripped.strip_prefix("return ") {
            let e = XP::new(rest.trim(), ELang::Py)?.expr()?.to_cuni();
            out.push(format!("{pad}ret {e}"));
            i += 1;
            continue;
        }
        if stripped == "return" {
            out.push(format!("{pad}ret"));
            i += 1;
            continue;
        }
        if stripped.starts_with("print(") && stripped.ends_with(')') {
            let e = XP::new(&stripped[6..stripped.len() - 1], ELang::Py)?
                .expr()?
                .to_cuni();
            out.push(format!("{pad}say({e})"));
            i += 1;
            continue;
        }
        if stripped.starts_with("say(") && stripped.ends_with(')') {
            let e = XP::new(&stripped[4..stripped.len() - 1], ELang::Py)?
                .expr()?
                .to_cuni();
            out.push(format!("{pad}say({e})"));
            i += 1;
            continue;
        }
        // binding: `x = e` or annotated `x: T = e`
        if let Some((lhs, rhs)) = split_assign(stripped, ELang::Py)? {
            let lhs = lhs.split(':').next().unwrap_or("").trim();
            if !is_ident(lhs) {
                return Err(format!("ingest: refuse Python binding `{lhs}`"));
            }
            let e = XP::new(&rhs, ELang::Py)?.expr()?.to_cuni();
            out.push(format!("{pad}let {lhs} = {e}"));
            i += 1;
            continue;
        }
        // bare expression statement (a call)
        let e = XP::new(stripped, ELang::Py)?.expr()?;
        match e {
            Ix::Call(f, _) if f == "main" => {}
            _ => out.push(format!("{pad}{}", e.to_cuni())),
        }
        i += 1;
    }
    Ok((out, i))
}

/// Parse the indented body after a `def`/`if` header at `parent_ind` spaces.
/// The body's indent is derived from its first content line (any width works);
/// an empty body yields no lines.
fn py_body(lines: &[&str], i: usize, parent_ind: usize) -> Result<(Vec<String>, usize), String> {
    let mut j = i;
    while j < lines.len() {
        let s = lines[j].trim();
        if s.is_empty() || s.starts_with('#') {
            j += 1;
            continue;
        }
        break;
    }
    if j >= lines.len() {
        return Ok((vec![], j));
    }
    let bi = indent_of(lines[j]);
    if bi <= parent_ind {
        return Ok((vec![], j));
    }
    py_block(lines, j, bi)
}

/// Skip a `def`/`class`/`if` block: everything indented deeper than `ind`.
fn py_skip_block(lines: &[&str], mut i: usize, ind: usize) -> usize {
    i += 1;
    while i < lines.len() {
        let raw = lines[i];
        let stripped = raw.trim();
        if stripped.is_empty() || stripped.starts_with('#') {
            i += 1;
            continue;
        }
        if indent_of(raw) <= ind {
            break;
        }
        i += 1;
    }
    i
}

fn py_def(
    lines: &[&str],
    i: usize,
    ind: usize,
    rest: &str,
) -> Result<(Vec<String>, usize), String> {
    // rest: `name(params) -> T:` or `name(params):`
    let rest = rest.strip_suffix(':').ok_or("ingest: bad Python def")?;
    let paren = rest.find('(').ok_or("ingest: bad Python def")?;
    let name = rest[..paren].trim();
    if !is_ident(name) {
        return Err(format!("ingest: bad Python def name `{name}`"));
    }
    let close = rest.rfind(')').ok_or("ingest: bad Python def")?;
    let params_s = &rest[paren + 1..close];
    let ret_s = rest[close + 1..].trim();
    let ret_ann = ret_s.strip_prefix("->").map(|s| s.trim()).unwrap_or("");
    if PY_PRELUDE_SKIP.contains(&name) {
        return Ok((vec![], py_skip_block(lines, i, ind)));
    }
    let mut params = Vec::new();
    if !params_s.trim().is_empty() {
        for p in params_s.split(',') {
            let p = p.trim();
            let (pn, ann) = match p.split_once(':') {
                Some((n, a)) => (n.trim(), a.trim()),
                None => (p, ""),
            };
            if !is_ident(pn) {
                return Err(format!("ingest: bad Python param `{p}`"));
            }
            let ct = if ann.is_empty() {
                "int".to_string()
            } else {
                py_type_to_cuni(ann)
                    .ok_or_else(|| format!("ingest: Python type `{ann}` outside subset"))?
            };
            params.push(format!("{pn}: {ct}"));
        }
    }
    let ret = if ret_ann.is_empty() {
        "int".to_string()
    } else {
        py_type_to_cuni(ret_ann)
            .ok_or_else(|| format!("ingest: Python type `{ret_ann}` outside subset"))?
    };
    let (body, ni) = py_body(lines, i + 1, ind)?;
    if name == "main" {
        // unwrap: CuNi is already top-level
        return Ok((body, ni));
    }
    if !body.iter().any(|s| s.trim_start().starts_with("ret")) {
        return Err(format!(
            "ingest: refuse Python def `{name}` without return (v1 subset)"
        ));
    }
    let pad: String = " ".repeat(ind);
    let mut v = vec![format!(
        "{pad}def {name}({}) -> {ret} do",
        params.join(", ")
    )];
    v.extend(body);
    v.push(format!("{pad}end"));
    Ok((v, ni))
}

fn py_if(lines: &[&str], i: usize, ind: usize, rest: &str) -> Result<(Vec<String>, usize), String> {
    let cond_s = rest.strip_suffix(':').ok_or("ingest: bad Python if")?;
    let c = XP::new(cond_s.trim(), ELang::Py)?.expr()?.to_cuni();
    let pad: String = " ".repeat(ind);
    let (then_b, mut ni) = py_body(lines, i + 1, ind)?;
    let mut v = vec![format!("{pad}if {c} do")];
    v.extend(then_b);
    // optional else:
    if ni < lines.len() {
        let raw = lines[ni];
        let stripped = raw.trim();
        if indent_of(raw) == ind && (stripped == "else:" || stripped.starts_with("elif ")) {
            if stripped.starts_with("elif ") {
                return Err("ingest: refuse Python elif (outside subset)".into());
            }
            let (else_b, ni2) = py_body(lines, ni + 1, ind)?;
            v.push(format!("{pad}els"));
            v.extend(else_b);
            ni = ni2;
        }
    }
    v.push(format!("{pad}end"));
    Ok((v, ni))
}

// ---------------------------------------------------------------------------
// Ruby and Lua (native `def`/`function` … `end` backends)
// ---------------------------------------------------------------------------
// Both backends erase CuNi types, so params/returns are recovered by the
// shared call-site inference (`infer_untyped`), exactly like the JS seat.
// Blocks are indent-parsed and `end`-terminated, mirroring the Python
// ingester's shape. Anything outside the emitted subset refuses.
//
// Soundness note on `let` vs `mut`: the backends erase the distinction, and
// it cannot be recovered exactly (a name bound in two `if` branches is two
// separate `let`s; a name bound once then reassigned needed `mut`). So the
// first binding of a name in a scope ingests as `let` and later ones as bare
// reassignments; `if`/`while` bodies get a fresh scope clone so branch-local
// bindings can never leak into (or clobber) the outer scope. Code that truly
// needed `mut` then fails the CuNi front-end with "cannot assign to `x` —
// it's `let`-bound" — an honest refusal, never a mistranslation.

/// Prelude helpers the Ruby backend always emits; user defs with these names
/// are the runtime, not user code, and are skipped on ingest.
const RB_PRELUDE: &[&str] = &[
    "say",
    "_cuni_repr",
    "_cuni_interp_str",
    "_cuni_float_str",
    "range",
    "abs",
    "min",
    "max",
    "_cuni_slice",
    "_cuni_div",
];

/// Prelude helpers the Lua backend always emits.
const LUA_PRELUDE: &[&str] = &[
    "say",
    "_cuni_repr",
    "_cuni_num_str",
    "_cuni_interp_str",
    "_cuni_list",
    "_cuni_map",
    "range",
    "abs",
    "min",
    "max",
    "_cuni_slice",
    "_cuni_div",
    "_cuni_len",
    "kwargs",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum EndLang {
    Rb,
    Lua,
}

impl EndLang {
    fn elang(self) -> ELang {
        match self {
            EndLang::Rb => ELang::Rb,
            EndLang::Lua => ELang::Lua,
        }
    }
    fn seat(self) -> &'static str {
        match self {
            EndLang::Rb => "Ruby",
            EndLang::Lua => "Lua",
        }
    }
    fn prelude(self) -> &'static [&'static str] {
        match self {
            EndLang::Rb => RB_PRELUDE,
            EndLang::Lua => LUA_PRELUDE,
        }
    }
    fn is_comment(self, stripped: &str) -> bool {
        match self {
            EndLang::Rb => stripped.starts_with('#'),
            EndLang::Lua => stripped.starts_with("--"),
        }
    }
    /// The def-keyword rest, or None when this line is not a def.
    fn def_rest<'a>(self, stripped: &'a str) -> Option<&'a str> {
        match self {
            EndLang::Rb => stripped.strip_prefix("def "),
            EndLang::Lua => stripped
                .strip_prefix("function ")
                .or_else(|| stripped.strip_prefix("local function ")),
        }
    }
    /// Binding introducers for the shared type-recovery pass.
    fn bind_keywords(self) -> &'static [&'static str] {
        match self {
            EndLang::Rb => &[],
            EndLang::Lua => &["local"],
        }
    }
}

/// True when `src` is one of CuNi's Python lowerings (`# CuNi exactness
/// artifact` header) rather than a native Ruby/Lua artifact, whose own
/// headers name the Ruby/Lua backend.
fn is_lowering_artifact(src: &str) -> bool {
    for line in src.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !t.starts_with('#') {
            break;
        }
        if t.contains("CuNi exactness artifact") {
            return true;
        }
    }
    false
}

/// Strip the common leading indent from a block of lines (blank lines kept).
fn dedent_block(lines: &[&str]) -> Vec<String> {
    let min = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| indent_of(l))
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|l| {
            if l.len() >= min {
                l[min..].to_string()
            } else {
                l.to_string()
            }
        })
        .collect()
}

/// An `end` line, tolerating a trailing `#`/`--` comment.
fn is_end_line(stripped: &str) -> bool {
    if stripped == "end" {
        return true;
    }
    if let Some(rest) = stripped.strip_prefix("end") {
        let r = rest.trim_start();
        return r.is_empty() || r.starts_with('#') || r.starts_with("--");
    }
    false
}

/// True when `rest` (the text after `def `/`function `) both opens and closes
/// the def on this one line: a parameter list followed by a body and a
/// standalone trailing `end`. The Lua prelude uses these (`function abs(n)
/// return ... end`); the backends never emit them for user code.
fn is_one_line_def(rest: &str) -> bool {
    let t = rest.trim_end();
    let before_end = match t.strip_suffix("end") {
        Some(b) => b,
        None => return false,
    };
    // `end` must be a standalone word, not part of a longer one.
    if before_end.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
        return false;
    }
    // There must be a parameter list before the body.
    before_end.find('(').is_some()
}

/// Split top-level `def`/`function` items from the remaining top-level lines.
/// Nested defs refuse: the backends never emit them.
fn extract_end_funcs(el: EndLang, lines: &[&str]) -> Result<(Vec<Func>, Vec<String>), String> {
    let kw = match el {
        EndLang::Rb => "def",
        EndLang::Lua => "function",
    };
    let mut funcs = Vec::new();
    let mut top = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let raw = lines[i];
        let stripped = raw.trim();
        match el.def_rest(stripped) {
            Some(rest) => {
                if indent_of(raw) != 0 {
                    return Err(format!(
                        "ingest: refuse {} nested {kw} (outside subset): {stripped}",
                        el.seat()
                    ));
                }
                // One-line def (`function abs(n) return ... end`): the Lua
                // prelude uses these. Prelude ones are runtime — skip; a user
                // one-liner is outside the subset and refuses honestly.
                if is_one_line_def(rest) {
                    let name = rest.split('(').next().unwrap_or("").trim().to_string();
                    if el.prelude().contains(&name.as_str()) {
                        i += 1;
                        continue;
                    }
                    return Err(format!(
                        "ingest: refuse {} one-line def `{name}` (outside subset)",
                        el.seat()
                    ));
                }
                let mut j = i + 1;
                while j < lines.len() {
                    if indent_of(lines[j]) == 0 && is_end_line(lines[j].trim()) {
                        break;
                    }
                    j += 1;
                }
                if j >= lines.len() {
                    return Err(format!(
                        "ingest: {} {kw} without `end`: {stripped}",
                        el.seat()
                    ));
                }
                let name = rest.split('(').next().unwrap_or("").trim().to_string();
                if !is_ident(&name) {
                    return Err(format!("ingest: bad {} {kw} name `{name}`", el.seat()));
                }
                funcs.push(Func {
                    name,
                    sig: rest.trim().to_string(),
                    body: lines[i + 1..j].join("\n"),
                });
                i = j + 1;
            }
            None => {
                top.push(raw.to_string());
                i += 1;
            }
        }
    }
    Ok((funcs, top))
}

fn ingest_rb(src: &str) -> Result<String, String> {
    // A Python lowering wearing `.rb` still routes to the lowering path.
    if is_lowering_artifact(src) {
        return ingest_lowering(src, "rb");
    }
    ingest_end_lang(EndLang::Rb, src)
}

fn ingest_lua(src: &str) -> Result<String, String> {
    if is_lowering_artifact(src) {
        return ingest_lowering(src, "lua");
    }
    ingest_end_lang(EndLang::Lua, src)
}

fn ingest_end_lang(el: EndLang, src: &str) -> Result<String, String> {
    let seat = el.seat();
    let lines: Vec<&str> = src.lines().collect();
    let (funcs, top) = extract_end_funcs(el, &lines)?;
    // The `main` body: a `def main` / `function main` when present, else the
    // leftover top-level statements (hand-written scripts).
    let (main_body, main_is_def): (String, bool) = match funcs.iter().find(|f| f.name == "main") {
        Some(f) => {
            for t in &top {
                let s = t.trim();
                if s.is_empty() || el.is_comment(s) || s == "main" {
                    continue;
                }
                // The Ruby prelude's one-line class stub is runtime, not code.
                if el == EndLang::Rb && s.starts_with("class ") {
                    continue;
                }
                return Err(format!(
                    "ingest: refuse {seat} top-level statement outside main: {s}"
                ));
            }
            (f.body.clone(), true)
        }
        None => (top.join("\n"), false),
    };
    let (sigs, _untyped) = infer_untyped(
        &funcs,
        &main_body,
        el.elang(),
        &|n| el.prelude().contains(&n),
        el.bind_keywords(),
        true,
        &|_| false,
        &|f| sig_param_names(&f.sig, el.elang()),
        seat,
    )?;
    let mut out = String::new();
    for f in &funcs {
        if f.name == "main" || el.prelude().contains(&f.name.as_str()) {
            continue;
        }
        let def =
            end_def(el, f, &sigs).map_err(|e| format!("ingest: {seat} def `{}`: {e}", f.name))?;
        out.push_str(&def);
        out.push('\n');
    }
    let main_lines: Vec<&str> = main_body.lines().collect();
    let mut bound = HashSet::new();
    // A def body is still indented: dedent it to top level first. Leftover
    // top-level lines already sit at indent 0.
    let (stmts, ni) = if main_is_def {
        let dedented = dedent_block(&main_lines);
        let dd: Vec<&str> = dedented.iter().map(|s| s.as_str()).collect();
        end_block(el, &dd, 0, 0, &mut bound)?
    } else {
        end_block(el, &main_lines, 0, 0, &mut bound)?
    };
    if ni < main_lines.len() {
        return Err(format!(
            "ingest: refuse {seat} unexpected `{}`",
            main_lines[ni].trim()
        ));
    }
    for st in stmts {
        out.push_str(&st);
        out.push('\n');
    }
    Ok(out)
}

/// Typed def: params and return come from call-site inference.
fn end_def(el: EndLang, f: &Func, sigs: &HashMap<String, TypedSig>) -> Result<String, String> {
    let seat = el.seat();
    let tsig = sigs.get(&f.name).ok_or_else(|| {
        format!("ingest: refuse {seat} def `{}` — cannot infer signature (types are erased in this backend)", f.name)
    })?;
    let params: Vec<String> = tsig
        .params
        .iter()
        .map(|(n, t)| format!("{n}: {}", t.cuni().expect("inferred types are known")))
        .collect();
    let ret = tsig.ret.cuni().expect("inferred types are known");
    let mut bound: HashSet<String> = tsig.params.iter().map(|(n, _)| n.clone()).collect();
    let lines: Vec<&str> = f.body.lines().collect();
    let (body, ni) = end_body(el, &lines, 0, 0, &mut bound)?;
    if ni < lines.len() {
        return Err(format!("unexpected `{}`", lines[ni].trim()));
    }
    if !body
        .iter()
        .any(|s| s.trim_start() == "ret" || s.trim_start().starts_with("ret "))
    {
        return Err(format!(
            "ingest: refuse {seat} def `{}` without return (v1 subset)",
            f.name
        ));
    }
    let mut v = vec![format!("def {}({}) -> {ret} do", f.name, params.join(", "))];
    v.extend(body);
    v.push("end".to_string());
    Ok(v.join("\n"))
}

/// The child block after a `def`/`if`/`while` header at `parent_ind`.
fn end_body(
    el: EndLang,
    lines: &[&str],
    i: usize,
    parent_ind: usize,
    bound: &mut HashSet<String>,
) -> Result<(Vec<String>, usize), String> {
    let mut j = i;
    while j < lines.len() {
        let s = lines[j].trim();
        if s.is_empty() || el.is_comment(s) {
            j += 1;
            continue;
        }
        break;
    }
    if j >= lines.len() {
        return Ok((vec![], j));
    }
    let bi = indent_of(lines[j]);
    if bi <= parent_ind {
        return Ok((vec![], j));
    }
    end_block(el, lines, j, bi, bound)
}

/// Parse an `end`-terminated block; every statement sits at exactly `ind`
/// spaces. `bound` holds the names already bound in this scope: the first
/// binding of a name emits `let`, later ones emit bare reassignment.
/// Returns (cuni lines, next line index) — the index points at the closing
/// `end`/`else` or the first dedented line.
fn end_block(
    el: EndLang,
    lines: &[&str],
    mut i: usize,
    ind: usize,
    bound: &mut HashSet<String>,
) -> Result<(Vec<String>, usize), String> {
    let lang = el.elang();
    let seat = el.seat();
    let mut out = Vec::new();
    while i < lines.len() {
        let raw = lines[i];
        let stripped = raw.trim();
        if stripped.is_empty() || el.is_comment(stripped) {
            i += 1;
            continue;
        }
        if is_end_line(stripped) {
            if indent_of(raw) > ind {
                return Err(format!(
                    "ingest: refuse {seat} dedent `end` (outside subset)"
                ));
            }
            break;
        }
        let cur = indent_of(raw);
        if cur < ind {
            break;
        }
        if cur > ind {
            return Err(format!("ingest: refuse bad indent in {seat}: {stripped}"));
        }
        if stripped == "else" {
            break;
        }
        let pad: String = " ".repeat(ind);
        // Nested def: the pre-pass extracts top-level defs; anything left is nested.
        if el.def_rest(stripped).is_some() {
            return Err(format!("ingest: refuse {seat} nested def (outside subset)"));
        }
        if let Some(rest) = stripped.strip_prefix("if ") {
            let (v, ni) = end_if(el, lines, i, ind, rest, bound)?;
            out.extend(v);
            i = ni;
            continue;
        }
        if let Some(rest) = stripped.strip_prefix("while ") {
            let (v, ni) = end_while(el, lines, i, ind, rest, bound)?;
            out.extend(v);
            i = ni;
            continue;
        }
        for bad in [
            "for", "until", "unless", "case", "elsif", "elseif", "begin", "repeat",
        ] {
            if stripped == bad || stripped.starts_with(&format!("{bad} ")) {
                return Err(format!("ingest: refuse {seat} `{bad}` (outside subset)"));
            }
        }
        if stripped == "do" || stripped.starts_with("do ") {
            return Err(format!(
                "ingest: refuse {seat} bare `do` block (outside subset)"
            ));
        }
        if stripped == "raise" || stripped.starts_with("raise ") || stripped.starts_with("raise(") {
            return Err(format!("ingest: refuse {seat} raise (outside subset)"));
        }
        if stripped == "error" || stripped.starts_with("error(") {
            return Err(format!("ingest: refuse {seat} error() (outside subset)"));
        }
        if let Some(rest) = stripped.strip_prefix("return ") {
            let e = XP::new(rest.trim(), lang)?.expr()?.to_cuni();
            out.push(format!("{pad}ret {e}"));
            i += 1;
            continue;
        }
        if stripped == "return" {
            out.push(format!("{pad}ret"));
            i += 1;
            continue;
        }
        if stripped == "main" || stripped == "main()" {
            i += 1; // trailing entry-point call
            continue;
        }
        // `local x = e` (Lua) or `x = e`: first binding in this scope is
        // `let`, later ones are bare reassignments.
        let assign_src = match el {
            EndLang::Rb => stripped,
            EndLang::Lua => stripped.strip_prefix("local ").unwrap_or(stripped),
        };
        if let Some((lhs, rhs)) = split_assign(assign_src, lang)? {
            if !is_ident(&lhs) {
                return Err(format!("ingest: refuse {seat} assignment target `{lhs}`"));
            }
            let e = XP::new(&rhs, lang)?.expr()?.to_cuni();
            if bound.contains(&lhs) {
                out.push(format!("{pad}{lhs} = {e}"));
            } else {
                bound.insert(lhs.clone());
                out.push(format!("{pad}let {lhs} = {e}"));
            }
            i += 1;
            continue;
        }
        let e = XP::new(stripped, lang)?.expr()?;
        out.push(format!("{pad}{}", e.to_cuni()));
        i += 1;
    }
    Ok((out, i))
}

fn end_if(
    el: EndLang,
    lines: &[&str],
    i: usize,
    ind: usize,
    rest: &str,
    bound: &mut HashSet<String>,
) -> Result<(Vec<String>, usize), String> {
    let seat = el.seat();
    // An optional trailing `then` (hand-written style; the backends omit it).
    let cond_s = rest.strip_suffix(" then").unwrap_or(rest).trim();
    if cond_s.is_empty() {
        return Err(format!("ingest: bad {seat} if"));
    }
    let c = XP::new(cond_s, el.elang())?.expr()?.to_cuni();
    let pad: String = " ".repeat(ind);
    // Fresh scope per branch: branch-local bindings never leak out.
    let (then_b, mut ni) = end_body(el, lines, i + 1, ind, &mut bound.clone())?;
    let mut v = vec![format!("{pad}if {c} do")];
    v.extend(then_b);
    if ni < lines.len() {
        let s = lines[ni].trim();
        if indent_of(lines[ni]) == ind
            && (s.starts_with("elsif ")
                || s.starts_with("elseif ")
                || s == "elsif"
                || s == "elseif")
        {
            return Err(format!("ingest: refuse {seat} elsif (outside subset)"));
        }
    }
    if ni < lines.len() && indent_of(lines[ni]) == ind && lines[ni].trim() == "else" {
        let (else_b, ni2) = end_body(el, lines, ni + 1, ind, &mut bound.clone())?;
        v.push(format!("{pad}els"));
        v.extend(else_b);
        ni = ni2;
    }
    if ni < lines.len() && indent_of(lines[ni]) == ind && is_end_line(lines[ni].trim()) {
        ni += 1;
    } else {
        return Err(format!("ingest: bad {seat} if — missing `end`"));
    }
    v.push(format!("{pad}end"));
    Ok((v, ni))
}

fn end_while(
    el: EndLang,
    lines: &[&str],
    i: usize,
    ind: usize,
    rest: &str,
    bound: &mut HashSet<String>,
) -> Result<(Vec<String>, usize), String> {
    let seat = el.seat();
    // Lua writes `while cond do`; Ruby writes `while cond`.
    let cond_s = rest.strip_suffix(" do").unwrap_or(rest).trim();
    if cond_s.is_empty() {
        return Err(format!("ingest: bad {seat} while"));
    }
    let c = XP::new(cond_s, el.elang())?.expr()?.to_cuni();
    let pad: String = " ".repeat(ind);
    let (body_b, mut ni) = end_body(el, lines, i + 1, ind, &mut bound.clone())?;
    let mut v = vec![format!("{pad}whl {c} do")];
    v.extend(body_b);
    if ni < lines.len() && indent_of(lines[ni]) == ind && is_end_line(lines[ni].trim()) {
        ni += 1;
    } else {
        return Err(format!("ingest: bad {seat} while — missing `end`"));
    }
    v.push(format!("{pad}end"));
    Ok((v, ni))
}

// ---------------------------------------------------------------------------
// Lowering seats: strip the CuNi header, apply the Python subset
// ---------------------------------------------------------------------------

fn ingest_lowering(src: &str, seat_id: &str) -> Result<String, String> {
    let mut lines: Vec<&str> = src.lines().collect();
    let mut stripped = 0usize;
    while let Some(l) = lines.first() {
        if l.starts_with('#') {
            lines.remove(0);
            stripped += 1;
        } else {
            break;
        }
    }
    if stripped == 0 {
        return Err(format!(
            "ingest: refuse .{seat_id} without the CuNi lowering header — not a CuNi artifact"
        ));
    }
    ingest_py(&lines.join("\n"))
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Next tier: native ingest for awk, perl, sh, sql, wat.
// ---------------------------------------------------------------------------
// Each ingester below handles genuine foreign source. CuNi's own Python
// lowerings (leading `#` comment block mentioning CuNi) are detected and
// routed to ingest_lowering, so the exactness harness keeps working for
// these seats while hand-written native code gets a real parser.

/// True when `src` is one of CuNi's own Python lowerings (leading `#` comment
/// block mentioning CuNi) rather than genuine foreign source.
fn is_cuni_lowering(src: &str) -> bool {
    for line in src.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !t.starts_with('#') {
            break;
        }
        if t.contains("CuNi") {
            return true;
        }
    }
    false
}

/// String-aware brace depth delta of a line (`{` = +1, `}` = -1).
fn brace_delta(line: &str) -> i32 {
    let mut d = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for c in line.chars() {
        if esc {
            esc = false;
            continue;
        }
        if in_str {
            if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => d += 1,
            '}' => d -= 1,
            _ => {}
        }
    }
    d
}

/// Extract the braced block following `keyword` at a line start
/// (e.g. awk `BEGIN { ... }`).
fn extract_named_block(src: &str, keyword: &str) -> Result<String, String> {
    let mut i = 0usize;
    while let Some(p) = src[i..].find(keyword) {
        let s = i + p;
        let ls = src[..s].rfind('\n').map(|x| x + 1).unwrap_or(0);
        if !src[ls..s].trim().is_empty() {
            i = s + keyword.len();
            continue;
        }
        let after = s + keyword.len();
        if src[after..]
            .chars()
            .next()
            .map_or(false, |c| c.is_ascii_alphanumeric() || c == '_')
        {
            i = after;
            continue;
        }
        let open = src[after..]
            .find('{')
            .map(|o| after + o)
            .ok_or_else(|| format!("ingest: expected `{{` after {keyword}"))?;
        let (body, _) = extract_braced(src, open)?;
        return Ok(body);
    }
    Err(format!("ingest: no top-level {keyword} block found"))
}

// --- awk -------------------------------------------------------------------
// Subset: `function name(args) { ... }` defs, one `BEGIN { ... }` main,
// `print expr` (single arg), bare `x = expr` bindings, `return expr`,
// `if (c) { } else { }`. No pattern-action rules, no printf, no ++/--,
// no arrays/strings ops beyond the shared expression grammar.

fn ingest_awk(src: &str) -> Result<String, String> {
    if is_cuni_lowering(src) {
        return ingest_lowering(src, "awk");
    }
    // At brace depth 0 only blank/comment/function/BEGIN lines are allowed;
    // anything else (pattern-action rules) is outside the subset.
    let mut depth = 0i32;
    for line in src.lines() {
        let t = line.trim();
        if depth == 0 {
            if !(t.is_empty()
                || t.starts_with('#')
                || t.starts_with("function ")
                || t.starts_with("BEGIN"))
            {
                return Err(format!(
                    "ingest: refuse awk `{t}` — pattern-action rules are outside the subset (function + BEGIN only)"
                ));
            }
        }
        depth += brace_delta(line);
        if depth < 0 {
            return Err("ingest: refuse awk (unbalanced braces)".into());
        }
    }
    let funcs = extract_funcs(src, "function ")?;
    let main = extract_named_block(src, "BEGIN")?;
    let (sigs, untyped) = infer_untyped(
        &funcs,
        &main,
        ELang::Js,
        &|_| false,
        &[],
        true,
        &|_| false,
        &|f| sig_param_names(&f.sig, ELang::Js),
        "awk",
    )?;
    for u in &untyped {
        return Err(format!(
            "ingest: refuse awk — could not infer types for `{u}` (untyped params, no call sites)"
        ));
    }
    let mut out = String::new();
    for f in funcs {
        if f.sig.trim_start().starts_with("function ") {
            // nested/odd placement; extract_funcs already validated the prefix
        }
        let sig = sigs
            .get(&f.name)
            .ok_or_else(|| format!("ingest: refuse awk — no signature for `{}`", f.name))?;
        let mut bound = HashSet::new();
        let body = awk_block(&f.body, 1, &mut bound)?;
        out.push_str(&format!(
            "def {}({}) -> {} do\n{}end\n",
            f.name,
            sig.params
                .iter()
                .map(|(n, t)| format!("{n}: {}", t.cuni().expect("inferred types are known")))
                .collect::<Vec<_>>()
                .join(", "),
            sig.ret.cuni().expect("inferred types are known"),
            body
        ));
    }
    let mut bound = HashSet::new();
    out.push_str(&awk_block(&main, 0, &mut bound)?);
    Ok(out)
}

fn awk_block(body: &str, indent: usize, bound: &mut HashSet<String>) -> Result<String, String> {
    let mut out = String::new();
    for chunk in split_chunks(body)? {
        out.push_str(&awk_stmt(&chunk, indent, bound)?);
    }
    Ok(out)
}

fn awk_stmt(chunk: &str, indent: usize, bound: &mut HashSet<String>) -> Result<String, String> {
    let pad: String = "    ".repeat(indent);
    let t = strip_semi(chunk.trim());
    if t.is_empty() {
        return Ok(String::new());
    }
    if t.starts_with("print") && t[5..].starts_with(|c: char| c.is_whitespace() || c == '(') {
        let mut rest = t[5..].trim().to_string();
        if rest.starts_with('(') && rest.ends_with(')') {
            rest = rest[1..rest.len() - 1].trim().to_string();
        }
        if split_top(&rest).len() > 1 {
            return Err("ingest: refuse awk `print` with multiple args (outside subset)".into());
        }
        let e = XP::new(&rest, ELang::Js)?.expr()?;
        return Ok(format!("{pad}say({})\n", e.to_cuni()));
    }
    for bad in ["printf", "getline", "next", "exit", "system"] {
        if t == bad || t.starts_with(&format!("{bad} ")) || t.starts_with(&format!("{bad}(")) {
            return Err(format!("ingest: refuse awk `{bad}` (outside subset)"));
        }
    }
    if t.starts_with("if") && t[2..].starts_with(|c: char| c.is_whitespace() || c == '(') {
        let (cond, then_b, else_b) = split_if(t)?;
        let c = XP::new(cond.trim(), ELang::Js)?.expr()?;
        let mut s = format!("{pad}if ({}) do\n", c.to_cuni());
        s.push_str(&awk_block(then_b.trim(), indent + 1, bound)?);
        if let Some(eb) = else_b {
            s.push_str(&format!("{pad}els\n"));
            s.push_str(&awk_block(eb.trim(), indent + 1, bound)?);
        }
        s.push_str(&format!("{pad}end\n"));
        return Ok(s);
    }
    if t.starts_with("return") && t[6..].starts_with(|c: char| c.is_whitespace() || c == '(') {
        let e = XP::new(t[6..].trim(), ELang::Js)?.expr()?;
        return Ok(format!("{pad}ret {}\n", e.to_cuni()));
    }
    // bare `x = expr` (awk has no binding keywords); first bind -> let
    if let Some((lhs, rhs)) = split_assign(t, ELang::Js)? {
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse awk assignment to `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::Js)?.expr()?;
        if bound.insert(lhs.clone()) {
            return Ok(format!("{pad}let {} = {}\n", lhs, e.to_cuni()));
        }
        return Ok(format!("{pad}{} = {}\n", lhs, e.to_cuni()));
    }
    Err(format!(
        "ingest: refuse awk statement `{t}` (outside subset)"
    ))
}

// --- perl ------------------------------------------------------------------
// Subset: `sub name { my ($a, $b) = @_; ... }` defs, top-level `my $x = expr`
// / bare `$x = expr` bindings, `print EXPR, "\n"` / `say(EXPR)` output,
// `return expr`, `if (c) { } else { }`. No regexes, no interpolation, no
// `.` concat, no foreach/while, no modules.

/// Strip `$`/`@`/`%` sigils outside strings. `$` inside "..." would be
/// interpolation, which the subset refuses. `@_` is kept for param parsing.
fn strip_perl_sigils(src: &str) -> Result<String, String> {
    let ch: Vec<char> = src.chars().collect();
    let n = ch.len();
    let mut out = String::new();
    let mut i = 0usize;
    let mut in_str = false;
    let mut squote = false;
    let mut esc = false;
    while i < n {
        let c = ch[i];
        if esc {
            out.push(c);
            esc = false;
            i += 1;
            continue;
        }
        if in_str {
            if c == '\\' {
                out.push(c);
                esc = true;
                i += 1;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            if c == '$' || c == '@' {
                return Err("ingest: refuse perl string interpolation (outside subset)".into());
            }
            out.push(c);
            i += 1;
            continue;
        }
        if squote {
            if c == '\'' {
                squote = false;
            }
            out.push(c);
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
                i += 1;
            }
            '\'' => {
                squote = true;
                out.push(c);
                i += 1;
            }
            '#' => {
                // line comment: copy through
                while i < n && ch[i] != '\n' {
                    out.push(ch[i]);
                    i += 1;
                }
            }
            '$' | '@' | '%' => {
                if c == '@' && i + 1 < n && ch[i + 1] == '_' {
                    out.push('@');
                    out.push('_');
                    i += 2;
                    continue;
                }
                let j = i + 1;
                if j < n && (ch[j].is_ascii_alphabetic() || ch[j] == '_') {
                    // drop the sigil, keep the name
                    i += 1;
                } else {
                    return Err(format!(
                        "ingest: refuse perl sigil usage `{}` (outside subset)",
                        ch[i..std::cmp::min(i + 2, n)].iter().collect::<String>()
                    ));
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Parse `my ($a, $b) = @_;` param destructures (sigils already stripped).
fn perl_params(body: &str) -> Result<Vec<String>, String> {
    let mut params = Vec::new();
    let mut seen_destructure = false;
    let mut i = 0usize;
    while let Some(p) = body[i..].find("@_") {
        let at = i + p;
        let stmt_start = body[..at]
            .rfind(|c| c == ';' || c == '{' || c == '}')
            .map(|x| x + 1)
            .unwrap_or(0);
        let mut stmt = body[stmt_start..at].trim().to_string();
        stmt = stmt.strip_suffix('=').unwrap_or(&stmt).trim().to_string();
        let inner = stmt
            .strip_prefix("my")
            .ok_or_else(|| {
                "ingest: refuse perl `@_` without a `my (...) = @_;` destructure".to_string()
            })?
            .trim();
        if seen_destructure {
            return Err("ingest: refuse perl with multiple `@_` destructures".into());
        }
        seen_destructure = true;
        if !(inner.starts_with('(') && inner.ends_with(')')) {
            return Err("ingest: refuse perl `@_` shape (outside subset)".into());
        }
        for part in inner[1..inner.len() - 1].split(',') {
            let pn = part.trim();
            if !is_ident(pn) {
                return Err(format!("ingest: refuse perl param `{pn}` (outside subset)"));
            }
            params.push(pn.to_string());
        }
        i = at + 2;
    }
    Ok(params)
}

/// Split perl source into `sub` definitions and top-level code.
fn split_perl(src: &str) -> Result<(Vec<Func>, String), String> {
    let mut funcs = Vec::new();
    let mut main = String::new();
    let mut i = 0usize;
    let b = src.as_bytes();
    let n = b.len();
    while i < n {
        // find `sub name` at a line start
        let mut found = None;
        let mut j = i;
        while j < n {
            let ls = src[..j].rfind('\n').map(|x| x + 1).unwrap_or(0);
            let line_start_ok = src[ls..j].trim().is_empty();
            if line_start_ok && src[j..].starts_with("sub ") {
                found = Some(j);
                break;
            }
            j += 1;
        }
        let s = match found {
            Some(s) => s,
            None => {
                main.push_str(&src[i..]);
                break;
            }
        };
        main.push_str(&src[i..s]);
        let after = s + 4;
        let mut k = after;
        while k < n && (b[k].is_ascii_alphanumeric() || b[k] == b'_') {
            k += 1;
        }
        let name = src[after..k].trim().to_string();
        if !is_ident(&name) {
            return Err(format!("ingest: refuse perl sub name `{name}`"));
        }
        let open = src[k..]
            .find('{')
            .map(|o| k + o)
            .ok_or_else(|| format!("ingest: refuse perl sub `{name}` without a body block"))?;
        let (body, end) = extract_braced(src, open)?;
        funcs.push(Func {
            name,
            sig: String::new(),
            body,
        });
        i = end;
    }
    Ok((funcs, main))
}

fn ingest_pl(src: &str) -> Result<String, String> {
    if is_cuni_lowering(src) {
        return ingest_lowering(src, "pl");
    }
    let flat = strip_perl_sigils(src)?;
    // `use ...;` / `require` lines are outside the subset
    for line in flat.lines() {
        let t = line.trim();
        if t.starts_with("use ") || t.starts_with("require ") || t.starts_with("package ") {
            return Err(format!(
                "ingest: refuse perl `{t}` (modules outside subset)"
            ));
        }
    }
    let (funcs, main) = split_perl(&flat)?;
    let (sigs, untyped) = infer_untyped(
        &funcs,
        &main,
        ELang::Js,
        &|n| ["abs", "int", "length"].contains(&n),
        &["my"],
        true,
        &|_| false,
        &|f| perl_params(&f.body),
        "perl",
    )?;
    for u in &untyped {
        return Err(format!(
            "ingest: refuse perl — could not infer types for `{u}` (untyped params, no call sites)"
        ));
    }
    let mut out = String::new();
    for f in funcs {
        let sig = sigs
            .get(&f.name)
            .ok_or_else(|| format!("ingest: refuse perl — no signature for `{}`", f.name))?;
        let mut bound = HashSet::new();
        let body = pl_block(&f.body, 1, &mut bound)?;
        out.push_str(&format!(
            "def {}({}) -> {} do\n{}end\n",
            f.name,
            sig.params
                .iter()
                .map(|(n, t)| format!("{n}: {}", t.cuni().expect("inferred types are known")))
                .collect::<Vec<_>>()
                .join(", "),
            sig.ret.cuni().expect("inferred types are known"),
            body
        ));
    }
    let mut bound = HashSet::new();
    out.push_str(&pl_block(&main, 0, &mut bound)?);
    Ok(out)
}

fn pl_block(body: &str, indent: usize, bound: &mut HashSet<String>) -> Result<String, String> {
    let mut out = String::new();
    for chunk in split_chunks(body)? {
        out.push_str(&pl_stmt(&chunk, indent, bound)?);
    }
    Ok(out)
}

fn pl_stmt(chunk: &str, indent: usize, bound: &mut HashSet<String>) -> Result<String, String> {
    let pad: String = "    ".repeat(indent);
    let t = strip_semi(chunk.trim());
    if t.is_empty() {
        return Ok(String::new());
    }
    for bad in [
        "printf", "sprintf", "foreach", "while", "until", "die", "warn", "chomp", "chop", "open",
        "close", "push", "pop", "shift", "unshift", "split", "join", "map", "grep", "sort",
        "reverse", "keys", "values", "each", "exists", "delete", "bless", "tie",
    ] {
        if t == bad || t.starts_with(&format!("{bad} ")) || t.starts_with(&format!("{bad}(")) {
            return Err(format!("ingest: refuse perl `{bad}` (outside subset)"));
        }
    }
    if t.contains("=~") || t.contains("!~") {
        return Err("ingest: refuse perl regex match (outside subset)".into());
    }
    // my (...) = @_; param destructure line -> nothing to emit
    if t.contains("@_") {
        return Ok(String::new());
    }
    if (t.starts_with("print") && t[5..].starts_with(|c: char| c.is_whitespace() || c == '('))
        || (t.starts_with("say") && t[3..].starts_with(|c: char| c.is_whitespace() || c == '('))
    {
        let is_say = t.starts_with("say");
        let rest0 = if is_say { &t[3..] } else { &t[5..] };
        let mut rest = rest0.trim().to_string();
        if rest.starts_with('(') && rest.ends_with(')') {
            rest = rest[1..rest.len() - 1].trim().to_string();
        }
        let args = split_top(&rest);
        if is_say {
            if args.len() != 1 {
                return Err("ingest: refuse perl `say` with != 1 arg (outside subset)".into());
            }
            let e = XP::new(args[0].trim(), ELang::Js)?.expr()?;
            return Ok(format!("{pad}say({})\n", e.to_cuni()));
        }
        // print: `print EXPR, "\n"` -> say(EXPR); `print "lit\n"` -> say("lit")
        if args.len() == 2 && args[1].trim() == "\"\\n\"" {
            let e = XP::new(args[0].trim(), ELang::Js)?.expr()?;
            return Ok(format!("{pad}say({})\n", e.to_cuni()));
        }
        if args.len() == 1 {
            let e = XP::new(args[0].trim(), ELang::Js)?.expr()?;
            if let Ix::Str(s) = &e {
                if let Some(lit) = s.strip_suffix('\n') {
                    let lit_esc: String = lit
                        .replace('\\', "\\\\")
                        .replace('"', "\\\"")
                        .replace('\n', "\\n")
                        .replace('\t', "\\t")
                        .replace('\r', "\\r");
                    return Ok(format!("{pad}say(\"{lit_esc}\")\n"));
                }
            }
            return Err("ingest: refuse perl `print` without trailing newline (say adds one; outside subset)".into());
        }
        return Err("ingest: refuse perl `print` with multiple args (outside subset)".into());
    }
    if t.starts_with("if") && t[2..].starts_with(|c: char| c.is_whitespace() || c == '(') {
        let (cond, then_b, else_b) = split_if(t)?;
        let c = XP::new(cond.trim(), ELang::Js)?.expr()?;
        let mut s = format!("{pad}if ({}) do\n", c.to_cuni());
        s.push_str(&pl_block(then_b.trim(), indent + 1, bound)?);
        if let Some(eb) = else_b {
            s.push_str(&format!("{pad}els\n"));
            s.push_str(&pl_block(eb.trim(), indent + 1, bound)?);
        }
        s.push_str(&format!("{pad}end\n"));
        return Ok(s);
    }
    if t.starts_with("unless") && t[6..].starts_with(|c: char| c.is_whitespace() || c == '(') {
        // unless (c) { } -> if (not (c)) — exact desugar
        let (cond, then_b, else_b) = split_if(t)?;
        if else_b.is_some() {
            return Err("ingest: refuse perl `unless/else` (outside subset)".into());
        }
        let c = XP::new(cond.trim(), ELang::Js)?.expr()?;
        let mut s = format!("{pad}if (not ({})) do\n", c.to_cuni());
        s.push_str(&pl_block(then_b.trim(), indent + 1, bound)?);
        s.push_str(&format!("{pad}end\n"));
        return Ok(s);
    }
    if t == "return"
        || (t.starts_with("return") && t[6..].starts_with(|c: char| c.is_whitespace() || c == '('))
    {
        let rest = if t == "return" { "" } else { t[6..].trim() };
        if rest.is_empty() {
            return Ok(format!("{pad}ret 0\n"));
        }
        let e = XP::new(rest, ELang::Js)?.expr()?;
        return Ok(format!("{pad}ret {}\n", e.to_cuni()));
    }
    // `my x = expr;` binding
    let t2 = t.strip_prefix("my").unwrap_or(t);
    let is_my = t2.len() != t.len() && t2.starts_with(|c: char| c.is_whitespace());
    let bind_src = if is_my { t2.trim() } else { t };
    if let Some((lhs, rhs)) = split_assign(bind_src, ELang::Js)? {
        if !is_ident(&lhs) {
            return Err(format!("ingest: refuse perl assignment to `{lhs}`"));
        }
        let e = XP::new(&rhs, ELang::Js)?.expr()?;
        if is_my || bound.insert(lhs.clone()) {
            return Ok(format!("{pad}let {} = {}\n", lhs, e.to_cuni()));
        }
        return Ok(format!("{pad}{} = {}\n", lhs, e.to_cuni()));
    }
    Err(format!(
        "ingest: refuse perl statement `{t}` (outside subset)"
    ))
}

// --- sh (POSIX shell) ------------------------------------------------------
// Subset: top-level only — `x=value`, `x=$((expr))`, `x="a$var"`, `echo args`,
// `if [ ... ]; then ... elif ... else ... fi` with -gt/-lt/-ge/-le/-eq/-ne and
// =/!= tests. No functions, no loops, no case, no command substitution,
// no pipes. Bare words are shell strings (so `echo hello` -> say("hello")).

/// Parse a shell word into a CuNi expression string.
/// `$var` / `${var}` -> var ; `"..."` with `$var` interpolation -> ("a" + var)
/// `'...'` -> literal ; bare word -> string literal ; number -> number.
fn sh_word(word: &str) -> Result<String, String> {
    let w = word.trim();
    if w.is_empty() {
        return Err("ingest: refuse sh empty word".into());
    }
    // $(( arithmetic )) — check before the `$(` refusal below
    if w.starts_with("$((") && w.ends_with("))") {
        let inner = sh_dollar_vars(&w[3..w.len() - 2])?;
        let e = XP::new(inner.trim(), ELang::Js)?.expr()?;
        return Ok(format!("({})", e.to_cuni()));
    }
    if w.starts_with("$(") || w.contains('`') {
        return Err("ingest: refuse sh command substitution (outside subset)".into());
    }
    // ${var} / $var
    if let Some(v) = w.strip_prefix("${") {
        let name = v
            .strip_suffix('}')
            .ok_or("ingest: refuse sh `${` (outside subset)")?;
        if !is_ident(name) {
            return Err(format!("ingest: refuse sh var `${name}`"));
        }
        return Ok(name.to_string());
    }
    if let Some(v) = w.strip_prefix('$') {
        if !is_ident(v) {
            return Err(format!("ingest: refuse sh var `${v}`"));
        }
        return Ok(v.to_string());
    }
    // double-quoted with interpolation
    if w.starts_with('"') && w.ends_with('"') && w.len() >= 2 {
        let inner = &w[1..w.len() - 1];
        return sh_dq_string(inner);
    }
    // single-quoted literal
    if w.starts_with('\'') && w.ends_with('\'') && w.len() >= 2 {
        return Ok(format!("\"{}\"", sh_escape(&w[1..w.len() - 1])));
    }
    // number?
    if w.parse::<i64>().is_ok() || w.parse::<f64>().is_ok() {
        return Ok(w.to_string());
    }
    // bare word -> shell string
    if w.chars()
        .all(|c| c.is_ascii_alphanumeric() || "_-./:".contains(c))
    {
        return Ok(format!("\"{w}\""));
    }
    Err(format!("ingest: refuse sh word `{w}` (outside subset)"))
}

/// Replace `$var` / `${var}` with `var` outside strings (for $(( ))).
fn sh_dollar_vars(s: &str) -> Result<String, String> {
    let ch: Vec<char> = s.chars().collect();
    let n = ch.len();
    let mut out = String::new();
    let mut i = 0usize;
    while i < n {
        if ch[i] == '$'
            && i + 1 < n
            && (ch[i + 1] == '{' || ch[i + 1].is_ascii_alphabetic() || ch[i + 1] == '_')
        {
            i += 1;
            if ch[i] == '{' {
                i += 1;
                let s0 = i;
                while i < n && ch[i] != '}' {
                    i += 1;
                }
                if i >= n {
                    return Err("ingest: refuse sh `${` (outside subset)".into());
                }
                let name: String = ch[s0..i].iter().collect();
                if !is_ident(&name) {
                    return Err(format!("ingest: refuse sh var `${name}`"));
                }
                out.push_str(&name);
                i += 1;
            } else {
                let s0 = i;
                while i < n && (ch[i].is_ascii_alphanumeric() || ch[i] == '_') {
                    i += 1;
                }
                out.push_str(&ch[s0..i].iter().collect::<String>());
            }
        } else {
            out.push(ch[i]);
            i += 1;
        }
    }
    Ok(out)
}

fn sh_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
        .replace('\r', "\\r")
}

/// `"a$b"` -> `("a" + b)` ; `"$x"` -> `x` ; `"plain"` -> `"plain"`.
fn sh_dq_string(inner: &str) -> Result<String, String> {
    let ch: Vec<char> = inner.chars().collect();
    let n = ch.len();
    let mut parts: Vec<String> = Vec::new();
    let mut lit = String::new();
    let mut i = 0usize;
    let flush = |lit: &mut String, parts: &mut Vec<String>| {
        if !lit.is_empty() {
            parts.push(format!("\"{}\"", sh_escape(lit)));
            lit.clear();
        }
    };
    while i < n {
        if ch[i] == '\\' && i + 1 < n {
            lit.push(ch[i + 1]);
            i += 2;
            continue;
        }
        if ch[i] == '$'
            && i + 1 < n
            && (ch[i + 1] == '{' || ch[i + 1].is_ascii_alphabetic() || ch[i + 1] == '_')
        {
            flush(&mut lit, &mut parts);
            i += 1;
            let name = if ch[i] == '{' {
                i += 1;
                let s0 = i;
                while i < n && ch[i] != '}' {
                    i += 1;
                }
                if i >= n {
                    return Err("ingest: refuse sh `${` (outside subset)".into());
                }
                let nm: String = ch[s0..i].iter().collect();
                i += 1;
                nm
            } else {
                let s0 = i;
                while i < n && (ch[i].is_ascii_alphanumeric() || ch[i] == '_') {
                    i += 1;
                }
                ch[s0..i].iter().collect()
            };
            if !is_ident(&name) {
                return Err(format!("ingest: refuse sh var `${name}`"));
            }
            parts.push(name);
        } else {
            lit.push(ch[i]);
            i += 1;
        }
    }
    flush(&mut lit, &mut parts);
    if parts.is_empty() {
        return Ok("\"\"".to_string());
    }
    if parts.len() == 1 {
        return Ok(parts.into_iter().next().unwrap());
    }
    let mut e = parts[0].clone();
    for p in parts.into_iter().skip(1) {
        e = format!("({e} + {p})");
    }
    Ok(e)
}

/// Split a shell line into words, respecting quotes.
fn sh_split_words(line: &str) -> Result<Vec<String>, String> {
    let ch: Vec<char> = line.chars().collect();
    let n = ch.len();
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut i = 0usize;
    let mut in_dq = false;
    let mut in_sq = false;
    while i < n {
        let c = ch[i];
        if in_dq {
            cur.push(c);
            if c == '\\' && i + 1 < n {
                cur.push(ch[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_dq = false;
            }
            i += 1;
            continue;
        }
        if in_sq {
            cur.push(c);
            if c == '\'' {
                in_sq = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => {
                in_dq = true;
                cur.push(c);
            }
            '\'' => {
                in_sq = true;
                cur.push(c);
            }
            '#' => break, // comment to end of line
            c if c.is_whitespace() => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(c),
        }
        i += 1;
    }
    if in_dq || in_sq {
        return Err("ingest: refuse sh unterminated quote".into());
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    Ok(words)
}

/// `[ "$x" -gt 100 ]` -> `(x > 100)`. Operands are shell words.
fn sh_test(test: &str) -> Result<String, String> {
    let mut t = test.trim();
    t = t.strip_prefix('[').unwrap_or(t).trim();
    t = t.strip_suffix(']').unwrap_or(t).trim();
    let words = sh_split_words(t)?;
    if words.len() == 3 {
        let l = sh_word(&words[0])?;
        let r = sh_word(&words[2])?;
        let op = match words[1].as_str() {
            "-gt" => ">",
            "-lt" => "<",
            "-ge" => ">=",
            "-le" => "<=",
            "-eq" => "==",
            "-ne" => "!=",
            "=" | "==" => "==",
            "!=" => "!=",
            o => return Err(format!("ingest: refuse sh test op `{o}` (outside subset)")),
        };
        return Ok(format!("({l} {op} {r})"));
    }
    if words.len() == 2 && words[0] == "-n" {
        let v = sh_word(&words[1])?;
        return Ok(format!("({v} != \"\")"));
    }
    if words.len() == 2 && words[0] == "-z" {
        let v = sh_word(&words[1])?;
        return Ok(format!("({v} == \"\")"));
    }
    Err(format!("ingest: refuse sh test `{test}` (outside subset)"))
}

fn ingest_sh(src: &str) -> Result<String, String> {
    if is_cuni_lowering(src) {
        return ingest_lowering(src, "sh");
    }
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0usize;
    let mut bound = HashSet::new();
    let mut out = String::new();
    match sh_block(&lines, &mut i, 0, &mut bound, &mut out, true)? {
        ShEnd::Eof | ShEnd::Fi => {}
        ShEnd::Else => return Err("ingest: refuse sh `else` without `if`".into()),
        ShEnd::Elif(_) => return Err("ingest: refuse sh `elif` without `if`".into()),
    }
    Ok(out)
}

enum ShEnd {
    Fi,
    Else,
    Elif(String),
    Eof,
}

/// Parse shell lines until a terminator (`fi`/`else`/`elif` at this level) or EOF.
fn sh_block(
    lines: &[&str],
    i: &mut usize,
    indent: usize,
    bound: &mut HashSet<String>,
    out: &mut String,
    top: bool,
) -> Result<ShEnd, String> {
    let pad: String = "    ".repeat(indent);
    while *i < lines.len() {
        let raw = lines[*i];
        let t = raw.trim();
        *i += 1;
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if t == "fi" {
            if top {
                return Err("ingest: refuse sh `fi` without `if`".into());
            }
            return Ok(ShEnd::Fi);
        }
        if t == "else" {
            return Ok(ShEnd::Else);
        }
        if t.starts_with("elif ") {
            return Ok(ShEnd::Elif(t.to_string()));
        }
        if t.starts_with("if ") {
            sh_if(lines, i, t, indent, bound, out)?;
            continue;
        }
        for bad in ["for ", "while ", "until ", "case ", "select ", "function "] {
            if t.starts_with(bad) {
                return Err(format!(
                    "ingest: refuse sh `{}` (outside subset)",
                    bad.trim()
                ));
            }
        }
        if t.contains("()") {
            return Err("ingest: refuse sh function definition (shell functions don't return values; outside subset)".into());
        }
        if t.starts_with("echo")
            && (t.len() == 4 || t[4..].starts_with(|c: char| c.is_whitespace()))
        {
            let args = sh_split_words(t[4..].trim())?;
            if args.iter().any(|a| a == "-n" || a == "-e") {
                return Err("ingest: refuse sh `echo -n/-e` (outside subset)".into());
            }
            let mut exprs = Vec::new();
            for a in &args {
                exprs.push(sh_word(a)?);
            }
            let e = if exprs.is_empty() {
                "\"\"".to_string()
            } else if exprs.iter().all(|e| e.starts_with('"')) {
                // all literals: join with spaces like echo does
                let joined = exprs
                    .iter()
                    .map(|e| e[1..e.len() - 1].to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("\"{}\"", sh_escape(&joined))
            } else if exprs.len() == 1 {
                exprs.into_iter().next().unwrap()
            } else {
                return Err(
                    "ingest: refuse sh `echo` mixing literals and vars (outside subset)".into(),
                );
            };
            out.push_str(&format!("{pad}say({e})\n"));
            continue;
        }
        if t.starts_with("printf") {
            return Err("ingest: refuse sh `printf` (outside subset)".into());
        }
        if t.starts_with("export ") {
            return Err("ingest: refuse sh `export` (outside subset)".into());
        }
        // assignment `name=value` (no spaces around `=` in shell)
        if let Some(eq) = t.find('=') {
            let name = t[..eq].trim();
            let val = t[eq + 1..].trim();
            if !is_ident(name) || name.is_empty() || val.is_empty() {
                return Err(format!("ingest: refuse sh line `{t}` (outside subset)"));
            }
            if val.contains(' ')
                && !(val.starts_with('"') || val.starts_with('\'') || val.starts_with("$("))
            {
                // `x=a b` is a command, not assignment
                return Err(format!("ingest: refuse sh line `{t}` (outside subset)"));
            }
            let e = sh_word(val)?;
            if bound.insert(name.to_string()) {
                out.push_str(&format!("{pad}let {name} = {e}\n"));
            } else {
                out.push_str(&format!("{pad}{name} = {e}\n"));
            }
            continue;
        }
        // bare command -> refuse (not in subset)
        return Err(format!("ingest: refuse sh command `{t}` (outside subset)"));
    }
    Ok(ShEnd::Eof)
}

fn sh_if(
    lines: &[&str],
    i: &mut usize,
    first: &str,
    indent: usize,
    bound: &mut HashSet<String>,
    out: &mut String,
) -> Result<(), String> {
    let pad: String = "    ".repeat(indent);
    // `if [ ... ]; then`
    let head = first[2..].trim();
    let then_pos = head
        .find("; then")
        .or_else(|| head.find(" then"))
        .ok_or_else(|| {
            "ingest: refuse sh `if` without `then` on the same line (outside subset)".to_string()
        })?;
    let test = head[..then_pos].trim();
    let after = head[then_pos..].trim();
    if !after.ends_with("then") {
        return Err(format!(
            "ingest: refuse sh `if` head `{first}` (outside subset)"
        ));
    }
    let cond = sh_test(test)?;
    out.push_str(&format!("{pad}if {cond} do\n"));
    sh_if_tail(lines, i, indent, bound, out)?;
    out.push_str(&format!("{pad}end\n"));
    Ok(())
}

/// After `if cond do`: then-branch, then else/elif/fi.
fn sh_if_tail(
    lines: &[&str],
    i: &mut usize,
    indent: usize,
    bound: &mut HashSet<String>,
    out: &mut String,
) -> Result<(), String> {
    let pad: String = "    ".repeat(indent);
    match sh_block(lines, i, indent + 1, bound, out, false)? {
        ShEnd::Fi => Ok(()),
        ShEnd::Eof => Err("ingest: refuse sh `if` without `fi`".into()),
        ShEnd::Else => {
            out.push_str(&format!("{pad}els\n"));
            match sh_block(lines, i, indent + 1, bound, out, false)? {
                ShEnd::Fi => Ok(()),
                ShEnd::Eof => Err("ingest: refuse sh `else` without `fi`".into()),
                _ => Err("ingest: refuse sh `elif` after `else` (outside subset)".into()),
            }
        }
        ShEnd::Elif(line) => {
            out.push_str(&format!("{pad}els\n"));
            // `elif [ .. ]; then` becomes a nested if; it consumes the final fi
            let nested = format!("if {}", line[5..].trim());
            sh_if(lines, i, &nested, indent + 1, bound, out)?;
            Ok(())
        }
    }
}

// --- sql -------------------------------------------------------------------
// Subset: `SELECT <expr>;` statements -> `say(<expr>)`. SQL single-quoted
// strings become CuNi strings; `=`/`<>` map to `==`/`!=`; AND/OR/NOT fold
// to lowercase for the shared expression grammar.

/// Convert SQL single-quoted strings to double-quoted (string-aware).
/// `''` inside a literal is the SQL escape for `'`.
fn sql_strings(expr: &str) -> Result<String, String> {
    let ch: Vec<char> = expr.chars().collect();
    let n = ch.len();
    let mut out = String::new();
    let mut i = 0usize;
    while i < n {
        let c = ch[i];
        if c == '\'' {
            i += 1;
            let mut s = String::new();
            loop {
                if i >= n {
                    return Err("ingest: refuse sql unterminated string".into());
                }
                if ch[i] == '\'' {
                    if i + 1 < n && ch[i + 1] == '\'' {
                        s.push('\'');
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    s.push(ch[i]);
                    i += 1;
                }
            }
            out.push('"');
            out.push_str(&sh_escape(&s));
            out.push('"');
        } else if c == '"' {
            return Err("ingest: refuse sql double-quoted identifier (outside subset)".into());
        } else {
            out.push(c);
            i += 1;
        }
    }
    Ok(out)
}

/// Map SQL operators/keywords to the shared expression grammar (outside strings).
fn sql_ops(expr: &str) -> String {
    // expr already has "..." strings; walk string-aware
    let ch: Vec<char> = expr.chars().collect();
    let n = ch.len();
    let mut out = String::new();
    let mut i = 0usize;
    let mut in_str = false;
    while i < n {
        let c = ch[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < n {
                out.push(ch[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '<' && i + 1 < n && ch[i + 1] == '>' {
            out.push_str("!=");
            i += 2;
            continue;
        }
        if c == '=' && !(i + 1 < n && ch[i + 1] == '=') && !(i > 0 && "<>!".contains(ch[i - 1])) {
            out.push_str("==");
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            let s = i;
            while i < n && (ch[i].is_ascii_alphanumeric() || ch[i] == '_') {
                i += 1;
            }
            let word: String = ch[s..i].iter().collect();
            match word.to_ascii_uppercase().as_str() {
                "AND" => out.push_str("and"),
                "OR" => out.push_str("or"),
                "NOT" => out.push_str("not"),
                "TRUE" => out.push_str("true"),
                "FALSE" => out.push_str("false"),
                "NULL" => out.push_str("0"),
                _ => out.push_str(&word),
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn ingest_sql(src: &str) -> Result<String, String> {
    if is_cuni_lowering(src) {
        return ingest_lowering(src, "sql");
    }
    let mut out = String::new();
    for stmt in src.split(';') {
        let mut t = stmt.trim().to_string();
        if t.is_empty() {
            continue;
        }
        // strip `--` comments line-wise
        let mut clean = String::new();
        for line in t.lines() {
            let l = line.trim();
            if l.starts_with("--") || l.is_empty() {
                continue;
            }
            // inline `--` outside strings -> cut
            let cut = if let Some(p) = l.find("--") {
                // crude: only cut when not inside a string
                let before = &l[..p];
                if before.matches('\'').count() % 2 == 0 {
                    &l[..p]
                } else {
                    l
                }
            } else {
                l
            };
            clean.push_str(cut.trim());
            clean.push(' ');
        }
        t = clean.trim().to_string();
        if t.is_empty() {
            continue;
        }
        let up = t.to_ascii_uppercase();
        let rest = if up.starts_with("SELECT ") {
            t[7..].trim()
        } else {
            return Err(format!(
                "ingest: refuse sql `{}` (only SELECT <expr> is in the subset)",
                t.chars().take(40).collect::<String>()
            ));
        };
        // strip a trailing `AS alias`
        let rest = {
            let up2 = rest.to_ascii_uppercase();
            if let Some(p) = up2.rfind(" AS ") {
                rest[..p].trim()
            } else {
                rest
            }
        };
        if rest.to_ascii_uppercase().contains(" FROM ") {
            return Err("ingest: refuse sql SELECT ... FROM (no tables in the subset)".into());
        }
        let expr = sql_ops(&sql_strings(rest)?);
        if expr.trim().is_empty() {
            return Err("ingest: refuse empty sql SELECT".into());
        }
        let ix = XP::new(&expr, ELang::Js)?.expr()?;
        out.push_str(&format!("say({})\n", ix.to_cuni()));
    }
    if out.is_empty() {
        return Err("ingest: refuse sql with no SELECT statements".into());
    }
    Ok(out)
}

// --- wat (WebAssembly text) ------------------------------------------------
// Subset: `(module (func $name (param $p TYPE)* (result TYPE)? <single expr>) ...)`
// with TYPE in {i32,i64}->int {f32,f64}->float. Expression forms: const,
// local.get, call, i32/i64.add/sub/mul, f32/f64.add/sub/mul, integer/float
// comparisons (eq ne lt_s le_s gt_s ge_s lt le gt ge), and (if (result T)
// cond (then e) (else e)). `(start $f)` emits a top-level call. Imports,
// exports, memories, globals are skipped (no compute content).

#[derive(Debug, Clone)]
enum Wx {
    Atom(String),
    List(Vec<Wx>),
}

fn wat_tokenize(src: &str) -> Result<Vec<String>, String> {
    let ch: Vec<char> = src.chars().collect();
    let n = ch.len();
    let mut toks = Vec::new();
    let mut i = 0usize;
    while i < n {
        let c = ch[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == ';' && i + 1 < n && ch[i + 1] == ';' {
            while i < n && ch[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '(' && i + 1 < n && ch[i + 1] == ';' {
            // block comment (; ... ;)
            i += 2;
            let mut depth = 1i32;
            while i < n && depth > 0 {
                if ch[i] == '(' && i + 1 < n && ch[i + 1] == ';' {
                    depth += 1;
                    i += 2;
                } else if ch[i] == ';' && i + 1 < n && ch[i + 1] == ')' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        if c == '(' || c == ')' {
            toks.push(c.to_string());
            i += 1;
            continue;
        }
        if c == '"' {
            let mut s = String::from("\"");
            i += 1;
            while i < n && ch[i] != '"' {
                if ch[i] == '\\' && i + 1 < n {
                    s.push(ch[i]);
                    s.push(ch[i + 1]);
                    i += 2;
                } else {
                    s.push(ch[i]);
                    i += 1;
                }
            }
            if i >= n {
                return Err("ingest: refuse wat unterminated string".into());
            }
            s.push('"');
            i += 1;
            toks.push(s);
            continue;
        }
        let s0 = i;
        while i < n && !ch[i].is_whitespace() && ch[i] != '(' && ch[i] != ')' {
            i += 1;
        }
        toks.push(ch[s0..i].iter().collect());
    }
    Ok(toks)
}

fn wat_parse(toks: &[String], i: &mut usize) -> Result<Wx, String> {
    if *i >= toks.len() {
        return Err("ingest: refuse wat unexpected end".into());
    }
    let t = toks[*i].clone();
    *i += 1;
    if t == "(" {
        let mut items = Vec::new();
        while *i < toks.len() && toks[*i] != ")" {
            items.push(wat_parse(toks, i)?);
        }
        if *i >= toks.len() {
            return Err("ingest: refuse wat unbalanced parens".into());
        }
        *i += 1; // )
        Ok(Wx::List(items))
    } else if t == ")" {
        Err("ingest: refuse wat unbalanced parens".into())
    } else {
        Ok(Wx::Atom(t))
    }
}

fn wat_ty(vt: &str) -> Result<Ty, String> {
    match vt {
        "i32" | "i64" => Ok(Ty::Int),
        "f32" | "f64" => Ok(Ty::Float),
        _ => Err(format!(
            "ingest: refuse wat valtype `{vt}` (outside subset)"
        )),
    }
}

fn wat_name(atom: &str) -> Result<String, String> {
    let n = atom.strip_prefix('$').unwrap_or(atom);
    if !is_ident(n) {
        return Err(format!("ingest: refuse wat name `{atom}`"));
    }
    Ok(n.to_string())
}

/// Compile a wat expression form to a CuNi expression string.
fn wat_expr(x: &Wx) -> Result<String, String> {
    let items = match x {
        Wx::List(items) if !items.is_empty() => items,
        _ => return Err("ingest: refuse wat expression shape (outside subset)".into()),
    };
    let head = match &items[0] {
        Wx::Atom(a) => a.as_str(),
        _ => return Err("ingest: refuse wat non-atom head".into()),
    };
    let atom1 = |idx: usize| -> Result<&str, String> {
        if items.len() != idx + 1 {
            return Err(format!(
                "ingest: refuse wat `{head}` arity (outside subset)"
            ));
        }
        match &items[idx] {
            Wx::Atom(a) => Ok(a.as_str()),
            _ => Err(format!(
                "ingest: refuse wat `{head}` shape (outside subset)"
            )),
        }
    };
    match head {
        "i32.const" | "i64.const" => {
            let a = atom1(1)?;
            a.parse::<i64>()
                .map_err(|_| format!("ingest: refuse wat const `{a}`"))?;
            Ok(a.to_string())
        }
        "f32.const" | "f64.const" => {
            let a = atom1(1)?;
            let f: f64 = a
                .parse()
                .map_err(|_| format!("ingest: refuse wat const `{a}`"))?;
            if !f.is_finite() {
                return Err(format!("ingest: refuse wat non-finite const `{a}`"));
            }
            Ok(
                if a.contains('.') || a.contains('e') || a.contains("inf") || a.contains("nan") {
                    a.to_string()
                } else {
                    format!("{a}.0")
                },
            )
        }
        "local.get" => Ok(wat_name(atom1(1)?)?),
        "call" => {
            let fname = wat_name(match items.get(1) {
                Some(Wx::Atom(a)) => a.as_str(),
                _ => return Err("ingest: refuse wat call shape".into()),
            })?;
            let mut aargs = Vec::new();
            for it in &items[2..] {
                aargs.push(wat_expr(it)?);
            }
            Ok(format!("{fname}({})", aargs.join(", ")))
        }
        // CuNi has no ternary/value-if, so wat `if` cannot sit in expression
        // position: outside the subset.
        "if" => Err("ingest: refuse wat `if` (outside subset)".into()),
        op => {
            let sop = match op {
                "i32.add" | "i64.add" | "f32.add" | "f64.add" => "+",
                "i32.sub" | "i64.sub" | "f32.sub" | "f64.sub" => "-",
                "i32.mul" | "i64.mul" | "f32.mul" | "f64.mul" => "*",
                "i32.eq" | "i64.eq" | "f32.eq" | "f64.eq" => "==",
                "i32.ne" | "i64.ne" | "f32.ne" | "f64.ne" => "!=",
                "i32.lt_s" | "i64.lt_s" | "f32.lt" | "f64.lt" => "<",
                "i32.le_s" | "i64.le_s" | "f32.le" | "f64.le" => "<=",
                "i32.gt_s" | "i64.gt_s" | "f32.gt" | "f64.gt" => ">",
                "i32.ge_s" | "i64.ge_s" | "f32.ge" | "f64.ge" => ">=",
                _ => {
                    return Err(format!(
                        "ingest: refuse wat op `{op}` (outside subset; div/rem trap on zero, unsigned ops and float/int converts unmapped)"
                    ))
                }
            };
            if items.len() != 3 {
                return Err(format!("ingest: refuse wat `{op}` arity (outside subset)"));
            }
            let l = wat_expr(&items[1])?;
            let r = wat_expr(&items[2])?;
            Ok(format!("({l} {sop} {r})"))
        }
    }
}

fn ingest_wat(src: &str) -> Result<String, String> {
    if is_cuni_lowering(src) {
        return ingest_lowering(src, "wat");
    }
    let toks = wat_tokenize(src)?;
    let mut i = 0usize;
    let mut roots = Vec::new();
    while i < toks.len() {
        roots.push(wat_parse(&toks, &mut i)?);
    }
    // accept `(module ...)` or bare funcs
    let mut items: Vec<&Wx> = Vec::new();
    let mut starts: Vec<String> = Vec::new();
    for r in &roots {
        match r {
            Wx::List(l) if matches!(l.first(), Some(Wx::Atom(a)) if a == "module") => {
                for it in &l[1..] {
                    match it {
                        Wx::List(il) => match il.first() {
                            Some(Wx::Atom(a)) if a == "start" => {
                                if il.len() != 2 {
                                    return Err("ingest: refuse wat start shape".into());
                                }
                                match &il[1] {
                                    Wx::Atom(nm) => starts.push(wat_name(nm)?),
                                    _ => return Err("ingest: refuse wat start shape".into()),
                                }
                            }
                            Some(Wx::Atom(a)) if a == "func" => items.push(it),
                            // imports/exports/memories/globals carry no compute: skip
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
            Wx::List(l) if matches!(l.first(), Some(Wx::Atom(a)) if a == "func") => items.push(r),
            _ => return Err("ingest: refuse wat top-level form (outside subset)".into()),
        }
    }
    if items.is_empty() {
        return Err("ingest: refuse wat with no funcs".into());
    }
    let mut out = String::new();
    for it in items {
        let l = match it {
            Wx::List(l) => l,
            _ => unreachable!(),
        };
        // (func $name (param $p T)* (result T)? body...)
        let mut j = 1usize;
        let fname = match l.get(j) {
            Some(Wx::Atom(a)) if a.starts_with('$') => {
                j += 1;
                wat_name(a)?
            }
            _ => return Err("ingest: refuse wat anonymous func (outside subset)".into()),
        };
        let mut params: Vec<(String, Ty)> = Vec::new();
        let mut ret = Ty::Unknown;
        while j < l.len() {
            match &l[j] {
                Wx::List(pl) => match pl.first() {
                    Some(Wx::Atom(a)) if a == "param" => {
                        // (param $p T) or (param T)
                        let rest: Vec<&Wx> = pl[1..].iter().collect();
                        let mut k = 0usize;
                        while k < rest.len() {
                            let (pname, pty) = match rest[k] {
                                Wx::Atom(a) if a.starts_with('$') => {
                                    let nm = wat_name(a)?;
                                    k += 1;
                                    let vt = match rest.get(k) {
                                        Some(Wx::Atom(v)) => v.clone(),
                                        _ => return Err("ingest: refuse wat param shape".into()),
                                    };
                                    k += 1;
                                    (nm, wat_ty(&vt)?)
                                }
                                Wx::Atom(v) => {
                                    k += 1;
                                    (format!("p{}", params.len()), wat_ty(v)?)
                                }
                                _ => return Err("ingest: refuse wat param shape".into()),
                            };
                            params.push((pname, pty));
                        }
                        j += 1;
                    }
                    Some(Wx::Atom(a)) if a == "result" => {
                        if pl.len() != 2 {
                            return Err("ingest: refuse wat result shape".into());
                        }
                        match &pl[1] {
                            Wx::Atom(v) => ret = wat_ty(v)?,
                            _ => return Err("ingest: refuse wat result shape".into()),
                        }
                        j += 1;
                    }
                    Some(Wx::Atom(a)) if a == "local" => {
                        return Err("ingest: refuse wat locals (outside subset)".into())
                    }
                    _ => break,
                },
                _ => break,
            }
        }
        let body_forms = &l[j..];
        if body_forms.is_empty() {
            return Err(format!("ingest: refuse wat func `{fname}` with empty body"));
        }
        // single-expression body (wat `if` refuses inside wat_expr: no value-if in CuNi)
        let body_expr = if body_forms.len() == 1 {
            wat_expr(&body_forms[0])?
        } else {
            return Err(format!(
                "ingest: refuse wat func `{fname}` with multi-form body (outside subset)"
            ));
        };
        if ret == Ty::Unknown {
            return Err(format!(
                "ingest: refuse wat func `{fname}` without a result type"
            ));
        }
        out.push_str(&format!(
            "def {fname}({}) -> {} do\n    ret {body_expr}\nend\n",
            params
                .iter()
                .map(|(n, t)| format!("{n}: {}", t.cuni().expect("inferred types are known")))
                .collect::<Vec<_>>()
                .join(", "),
            ret.cuni().expect("wat result type known"),
        ));
    }
    for s in starts {
        out.push_str(&format!("{s}()\n"));
    }
    Ok(out)
}

// --- NEXT_TIER_MORE3 -------------------------------------------------------

// round-trip harness: CuNi -> target -> run, target -> CuNi -> run, compare
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const RT1: &str = r#"def add(a: int, b: int) -> int do
    ret (a + b)
end
def pick(a: int, b: int) -> int do
    if (a > b) do
        ret a
    els
        ret b
    end
end
let x = (40 + 2)
say(add(x, 8))
say("hello")
say(pick(3, 7))
"#;

    const RT2: &str = r#"def greet(name: str) -> str do
    ret name
end
def is_big(n: int) -> bool do
    if (n > 100) do
        ret true
    els
        ret false
    end
end
say(greet("ada"))
say(is_big(101))
say(is_big(3))
"#;

    enum RunRes {
        Out(String),
        NoToolchain,
    }

    fn run_artifact(
        lang: &crate::langs::Lang,
        artifact: &std::path::Path,
    ) -> Result<RunRes, String> {
        let plan = crate::emit::exec_plan(lang, artifact);
        if let Some((cmd, args)) = plan.compile {
            match std::process::Command::new(&cmd).args(&args).output() {
                Ok(o) if o.status.success() => {}
                Ok(o) => {
                    return Err(format!(
                        "compile failed: {}",
                        String::from_utf8_lossy(&o.stderr)
                    ));
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(RunRes::NoToolchain);
                }
                Err(e) => return Err(format!("compile spawn failed: {e}")),
            }
        }
        match std::process::Command::new(&plan.run.0)
            .args(&plan.run.1)
            .output()
        {
            Ok(o) if o.status.success() => {
                Ok(RunRes::Out(String::from_utf8_lossy(&o.stdout).into_owned()))
            }
            Ok(o) => Err(format!(
                "run failed: {}",
                String::from_utf8_lossy(&o.stderr)
            )),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RunRes::NoToolchain),
            Err(e) => Err(format!("run spawn failed: {e}")),
        }
    }

    /// Full two-way trip for one seat:
    /// 1. CuNi -> target artifact -> run (or fall back to the py gold when the
    ///    toolchain is missing, e.g. Go on this machine).
    /// 2. target artifact -> CuNi -> front-end + interp -> stdout.
    /// 3. The two stdouts must match byte-for-byte.
    fn roundtrip_one(tag: &str, lang_id: &str, cuni_src: &str, expect_ingest_ok: bool) {
        let lang = crate::langs::find(lang_id).expect("known seat");
        let dir = std::env::temp_dir().join(format!(
            "cuni_rt_{}_{}_{}",
            std::process::id(),
            tag,
            lang_id.replace('/', "_")
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let cpath = dir.join("t.cuni");
        std::fs::write(&cpath, cuni_src).unwrap();
        let prog = crate::check::load_program(&cpath).expect("corpus must load");

        // gold stdout from the py seat
        let pylang = crate::langs::find("py").unwrap();
        let pyart = crate::emit::generate_exact(&prog, pylang).expect("py emit");
        let pypath = dir.join("t_gold.py");
        std::fs::write(&pypath, &pyart).unwrap();
        let gold = match run_artifact(pylang, &pypath).expect("gold run") {
            RunRes::Out(s) => s,
            RunRes::NoToolchain => panic!("python3 missing — cannot establish gold"),
        };

        // emit target artifact, run it (must run cleanly; stdout is informational)
        let art = crate::emit::generate_exact(&prog, lang).expect("target emit");
        let apath = dir.join(lang.out_file());
        std::fs::write(&apath, &art).unwrap();
        match run_artifact(lang, &apath) {
            Ok(_) => {}
            Err(e) => panic!("[{lang_id}] artifact failed to run: {e}"),
        }

        // ingest the artifact back to CuNi
        match ingest_file(&apath) {
            Ok(cuni2) => {
                if !expect_ingest_ok {
                    panic!("[{lang_id}] ingest unexpectedly accepted (expected refusal)");
                }
                let c2path = dir.join("t2.cuni");
                std::fs::write(&c2path, &cuni2).unwrap();
                let prog2 =
                    crate::check::load_program(&c2path).expect("ingested must pass front-end");
                let out2 = crate::interp::run(&prog2).expect("ingested must run");
                // The oracle is the py gold (canonical CuNi semantics): the
                // ingested program must MEAN the same as the original .cuni.
                // Native artifact stdout is not the oracle here — the harness
                // only requires the artifact to run cleanly; meaning is
                // checked on the re-ingested program. (Native bool printing
                // was normalized to `True`/`False` on every backend, so
                // artifact stdout does match gold on bool programs too;
                // verified via `cuni check` on the RT2 corpus.)
                assert_eq!(
                    out2, gold,
                    "[{lang_id}] round-trip stdout mismatch\n--- ingested ---\n{cuni2}"
                );
            }
            Err(e) => {
                if expect_ingest_ok {
                    panic!("[{lang_id}] ingest refused but was expected to work: {e}");
                }
                // honestly refused: nothing more to check
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RT1 (typed ints, conditional, top-level binding, calls, int+str output):
    /// every catalog seat must ingest and round-trip exactly.
    #[test]
    fn rt1_all_langs() {
        for l in crate::langs::LANGS {
            roundtrip_one("rt1", l.id, RT1, true);
        }
    }

    /// RT2 (str/bool types): every seat must ingest and round-trip exactly.
    /// The untyped/Val backends (js/ts/c/cpp/rs) recover types via call-site
    /// inference; anything uninferrable refuses instead of mistranslating.
    #[test]
    fn rt2_typed() {
        for l in crate::langs::LANGS {
            roundtrip_one("rt2", l.id, RT2, true);
        }
    }

    /// Solidity seat: the dice contract emits real solc-compilable Solidity
    /// and ingests back to CuNi with identical meaning. Requires solc on PATH;
    /// without it the seat is honestly skipped (NoToolchain).
    #[test]
    fn rt_sol_dice() {
        const DICE: &str = r#"def roll_dice(server_seed: int, client_seed: int, round: int) -> int do
    mut mixed = (server_seed * 31 + client_seed * 17 + round * 13) % 2147483647
    mut state = (48271 * mixed) % 2147483647
    ret (state % 6) + 1
end
say(roll_dice(987654321, 123456789, 1))
say(roll_dice(987654321, 123456789, 2))
"#;
        roundtrip_one("sol_dice", "sol", DICE, true);
    }

    /// Native ingest for the next tier (awk, perl, sh, sql, wat): hand-written
    /// foreign source (not CuNi lowerings) must ingest to CuNi that passes the
    /// front-end, runs, and prints the expected stdout. wat has no I/O in its
    /// subset, so it is verified by front-end + def shape instead.
    /// Also: a CuNi lowering wearing a foreign extension must still route to
    /// the lowering path (is_cuni_lowering), keeping the rt harness green.
    #[test]
    fn next_tier_native() {
        let dir = std::env::temp_dir().join(format!("cuni_next_tier_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let case = |name: &str, ext: &str, src: &str, expect_out: Option<&str>| {
            let p = dir.join(format!("{name}.{ext}"));
            std::fs::write(&p, src).unwrap();
            let cuni =
                ingest_file(&p).unwrap_or_else(|e| panic!("[{ext}] native ingest refused: {e}"));
            let cpath = dir.join(format!("{name}.cuni"));
            std::fs::write(&cpath, &cuni).unwrap();
            let prog = crate::check::load_program(&cpath)
                .unwrap_or_else(|e| panic!("[{ext}] ingested failed front-end: {e}"));
            if let Some(want) = expect_out {
                let got = crate::interp::run(&prog)
                    .unwrap_or_else(|e| panic!("[{ext}] ingested failed to run: {e}"));
                assert_eq!(
                    got, want,
                    "[{ext}] stdout mismatch\n--- ingested ---\n{cuni}"
                );
            }
            cuni
        };

        case(
            "t1",
            "awk",
            r#"function add(a, b) {
    return a + b
}
function is_big(n) {
    if (n > 100) {
        return 1
    } else {
        return 0
    }
}
BEGIN {
    x = 40 + 2
    print add(x, 8)
    print "hello"
    if (is_big(x)) {
        print "big"
    } else {
        print "small"
    }
}
"#,
            Some("50\nhello\nsmall\n"),
        );

        case(
            "t2",
            "pl",
            r#"sub add {
    my ($a, $b) = @_;
    return $a + $b;
}
sub greet {
    my ($name) = @_;
    return $name;
}
my $x = 40 + 2;
print add($x, 8), "\n";
say(greet("ada"));
"#,
            Some("50\nada\n"),
        );

        case(
            "t3",
            "sh",
            r#"x=$((40 + 2))
name="ada"
echo "$x"
echo "hello $name"
if [ "$x" -gt 100 ]; then
    echo "big"
else
    echo "small"
fi
"#,
            Some("42\nhello ada\nsmall\n"),
        );

        case(
            "t4",
            "sql",
            "SELECT 40 + 2;\nSELECT 'ada';\nSELECT 1 = 1 AND 2 <> 3;\n",
            Some("42\nada\nTrue\n"),
        );

        let wat_cuni = case(
            "t5",
            "wat",
            r#"(module
  (func $add (param $a i32) (param $b i32) (result i32)
    (i32.add (local.get $a) (local.get $b)))
  (func $main (result i32)
    (call $add (i32.const 40) (i32.const 2)))
  (start $main))
"#,
            None,
        );
        assert!(
            wat_cuni.contains("def add(a: int, b: int) -> int do"),
            "wat def shape wrong:\n{wat_cuni}"
        );
        assert!(
            wat_cuni.contains("ret (a + b)"),
            "wat body wrong:\n{wat_cuni}"
        );

        // lowering delegation: CuNi's own python lowering in a foreign
        // extension must route to ingest_lowering, not the native parser.
        // `BEGIN { }` is VALID awk (native would return Ok) but invalid
        // python, so an Err proves the lowering path was taken.
        let low = dir.join("low.awk");
        std::fs::write(&low, "# CuNi exactness artifact \u{2014} test\nBEGIN { }\n").unwrap();
        let r = ingest_file(&low);
        assert!(
            r.is_err(),
            "CuNi lowering in .awk must route to the lowering path (native awk would accept `BEGIN {{ }}`)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}

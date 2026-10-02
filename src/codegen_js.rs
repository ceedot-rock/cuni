use crate::ast::*;
use std::collections::HashMap;

/// A toy, best-effort JavaScript emitter for the CuNi AST, following the same
/// spirit as codegen_py.rs: it proves the parse -> emit pipeline for the
/// constructs SPEC.md has fully defined, and is NOT the "compile-or-refuse"
/// compiler SPEC.md describes — it never rejects a program, it does its best
/// and leaves a marker/comment where the mapping is genuinely undefined.
///
/// JS-specific design calls / known gaps:
/// - `let` (CuNi immutable) -> JS `const`; `mut` (CuNi mutable) -> JS `let`.
///   The keyword *names* collide confusingly across the two languages (CuNi
///   `let` is not JS `let`) but the semantics — immutable vs. reassignable
///   binding — map correctly.
/// - `list<T>` -> a plain JS Array. `map<K,V>` -> a JS `Map`, not a plain
///   object, since CuNi map keys aren't restricted to strings; `Map` is the
///   honest mapping. Map literals emit as `new Map([[k, v], ...])`.
/// - `opt<T>`'s `none` -> JS `null`.
/// - `fail expr` (signaling failure from a fallible function body) emits
///   `throw new CuNiError(expr)` — `CuNiError` is emitted once per program
///   (see gen_program), matching the shape `??`'s existing try/catch already
///   expects (its `catch` clause is untyped, so it catches any thrown value).
///   A still-`...` stub body emits a plain `throw new Error("not
///   implemented...")` instead, since "not written yet" is a different
///   concept from "this call failed."
/// - `??` (Unwrap) is only handled when it's the direct value of a `let`/
///   `mut` statement, matching codegen_py.rs's restriction and every example
///   so far. Elsewhere it emits a `null /* UNSUPPORTED */` marker.
///   Fallible-call unwraps become try/catch; opt-value unwraps become an
///   `=== null` check. Each unwrap gets a uniquely-numbered temp
///   (`_u0`, `_u1`, ...) — unlike Python, JS's `const`/`let` throw a
///   SyntaxError on redeclaration in the same scope, so reusing one fixed
///   temp name (as a dynamically-scoped language could) isn't an option here.
/// - `iface` has no JS equivalent (no interfaces/ABCs) and this backend
///   doesn't force-fit one (e.g. via duck-typing checks or mixins) — an
///   `iface` block emits only a comment naming it and its method
///   signatures; conformance (`typ X is Shape`) is likewise just a comment
///   on the generated class, entirely unenforced at runtime. This is a
///   deliberate difference from codegen_py.rs, which does model `iface` as
///   a Python ABC.
/// - `typ` -> a JS `class` with a constructor that assigns each field to
///   `this`.
/// - `enum` (payload-free) -> `const Name = Object.freeze({Variant:
///   "Variant", ...})`. String tags (not integers) are used since JS has no
///   `iota` and strings read better in ad-hoc debugging/logging. `Name.Variant`
///   needs no special-casing in `Field` codegen — it already matches CuNi's
///   own `EnumName.Variant` access syntax verbatim, unlike Go (see
///   codegen_go.rs), where enum constants aren't nested under their type.
/// - Backtick/`${}` interpolation -> JS template literals almost 1:1; text
///   segments are escaped for backslash/backtick/`$` to stay valid inside
///   the emitted template literal.
/// - `.push(...)` needed NO special-casing (unlike codegen_py.rs, which
///   rewrites it to `.append`): JS arrays already have a native `.push`, so
///   the generic call-codegen path handles it as-is.
/// - CuNi type annotations (`int`, `str`, `list<T>`, `map<K,V>`, `opt<T>`,
///   ...) are dropped entirely in emitted JS — vanilla JS has no static type
///   syntax. Only the runtime-relevant "is this a list or a map" shape is
///   still tracked internally (same scope-tracked guess codegen_py.rs uses),
///   purely to decide `for`-loop desugaring.
/// - `for i, x in xs` -> `for (const [i, x] of xs.entries())`; `for k, v in
///   m` -> `for (const [k, v] of m)` (a `Map`'s default iterator already
///   yields `[key, value]` pairs). List vs. map is decided by the same
///   lightweight scope-tracked guess codegen_py.rs uses, not real type
///   inference.
/// - Equality (`==`/`!=`) emits JS's strict operators (`===`/`!==`) rather
///   than the loose ones — idiomatic JS avoids `==`'s coercion surprises, and
///   nothing in CuNi's semantics depends on coercion.
/// - `ext` target bodies for `js:` are spliced in raw. If the raw text
///   contains `await`, the wrapper function is emitted as `async` (a
///   best-effort heuristic, not real analysis) — this mirrors the spec's own
///   `ext fetch` example, whose `js:` line uses `await fetch(...)`. That same
///   example (examples/ext-collision.cuni) originally surfaced a real gap when
///   actually run: an `ext` binding named `fetch` whose `js:` body also calls
///   the global `fetch` emits `function fetch(url) { return await
///   fetch(url)... }`, which recurses into itself instead of the global (a
///   stack overflow at runtime), since a top-level function declaration
///   shadows the global of the same name. This is now caught *before*
///   reaching this backend at all: `src/checks.rs::find_ext_collision`
///   refuses to compile (see `main.rs`) whenever an `ext` name matches a
///   small, hand-picked list of JS globals it plausibly shadows — this
///   backend itself still does no detection of its own, by design (see
///   module docs elsewhere: none of the toy backends do real refusal logic).
///   The Python backend has an analogous latent risk (e.g. `ext len` calling
///   Python's own `len` would hit the same self-recursion, just surfacing as
///   `RecursionError` rather than a silent stack overflow) — `checks.rs`
///   covers Python too. Go has no ambient globals of this kind, so nothing to
///   check there. This is orthogonal to `use math` (and any other imports an
///   `ext` body assumes) never actually being resolved by either toy
///   backend — `ext` bindings are spliced as raw text, not analyzed, and
///   remain the CuNi author's responsibility per §9.
/// - Top-level statements: CuNi allows top-level `ret` (e.g. inside a `??`
///   handler on a top-level `let`), which has no defined target. Like
///   codegen_py.rs, every top-level statement is collected and wrapped in an
///   implicit `function main() { ... }` called at the bottom (a plain named
///   function + call, not an IIFE — mirrors codegen_py.rs's `main()` 1:1 and
///   gives a named frame in stack traces instead of `<anonymous>`).
/// - `Index` (`base[index]`) does not distinguish map- from list-typed
///   bases the way `for`-loop desugaring does, so indexing into a `map<K,V>`
///   emits array-style `base[index]`, which is wrong for a JS `Map` (needs
///   `.get(index)`). None of the example programs index into a map, so this
///   gap is undetected by them; flagged here rather than silently guessed.
pub fn generate(program: &Program) -> String {
    let mut cg = Codegen::new(program);
    cg.gen_program(program);
    cg.out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VarKind {
    List,
    Map,
    /// A `dec` value (BigInt, scaled 10⁴) — see docs/DECIMAL.md.
    Dec,
    /// A `time` value (BigInt, unix epoch seconds, UTC) — see docs/TIME.md.
    /// A distinct tag from `Dec`: both are BigInt at runtime, so `say` and
    /// interpolation must be routed by the codegen, never by `typeof`.
    Time,
    Other,
}

struct FnInfo {
    fallible: bool,
    /// True only for a `link`'s generated `<name>_remote` client stub — it's
    /// `async` (it `await`s `fetch`), so any direct caller needs `await` too
    /// (see `gen_binding`'s fallible-call branch) and must itself become
    /// `async` (see `fn_needs_async`). Multi-hop propagation — a `def` that
    /// calls another `def` that calls `<name>_remote` — is NOT handled; see
    /// module docs.
    is_async: bool,
}

struct Codegen {
    fn_info: HashMap<String, FnInfo>,
    /// Declared `typ` names — constructors must emit `new T(...)` in JS.
    typ_names: std::collections::HashSet<String>,
    /// Field order per typ — for named-arg constructor reordering.
    typ_fields: HashMap<String, Vec<String>>,
    /// `def`s declared `-> dec`: calls to them are dec expressions.
    fn_dec_rets: std::collections::HashSet<String>,
    /// `def`s declared `-> time`: calls to them are time expressions.
    fn_time_rets: std::collections::HashSet<String>,
    out: String,
    unwrap_counter: usize,
}

impl Codegen {
    fn new(program: &Program) -> Self {
        let mut fn_info = HashMap::new();
        let mut typ_names = std::collections::HashSet::new();
        let mut typ_fields = HashMap::new();
        let mut fn_dec_rets = std::collections::HashSet::new();
        let mut fn_time_rets = std::collections::HashSet::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    if matches!(&f.ret_type, Type::Named(n) if n == "dec") {
                        fn_dec_rets.insert(f.name.clone());
                    }
                    if matches!(&f.ret_type, Type::Named(n) if n == "time") {
                        fn_time_rets.insert(f.name.clone());
                    }
                    fn_info.insert(
                        f.name.clone(),
                        FnInfo {
                            fallible: f.fallible,
                            is_async: false,
                        },
                    );
                    if f.is_link {
                        // Always fallible (a network call can fail even when the
                        // local body can't) and always async (it awaits
                        // `fetch`) — see FnInfo's docs.
                        fn_info.insert(
                            format!("{}_remote", f.name),
                            FnInfo {
                                fallible: true,
                                is_async: true,
                            },
                        );
                    }
                }
                Item::Typ(t) => {
                    typ_names.insert(t.name.clone());
                    typ_fields.insert(
                        t.name.clone(),
                        t.fields.iter().map(|f| f.name.clone()).collect(),
                    );
                }
                _ => {}
            }
        }
        Codegen {
            fn_info,
            typ_names,
            typ_fields,
            fn_dec_rets,
            fn_time_rets,
            out: String::new(),
            unwrap_counter: 0,
        }
    }

    /// Whether `stmts` directly contains a `??`-unwrap of a call to an
    /// `is_async` function (only checked at the shallow statement-list level
    /// matching where `??` is already supported, per module docs — recurses
    /// into `if`/`for`/`whl` bodies but not into nested function items).
    /// Determines whether the *enclosing* function/`main()` must itself be
    /// declared `async function` to legally use the `await` `gen_binding`
    /// emits for such a call.
    fn stmts_need_async(&self, stmts: &[Stmt]) -> bool {
        stmts.iter().any(|s| self.stmt_needs_async(s))
    }

    fn stmt_needs_async(&self, s: &Stmt) -> bool {
        match &s.kind {
            StmtKind::Let { value, .. } | StmtKind::Mut { value, .. } => {
                self.expr_needs_async(value)
            }
            StmtKind::If {
                then_body,
                else_body,
                ..
            } => {
                self.stmts_need_async(then_body)
                    || else_body
                        .as_ref()
                        .map_or(false, |b| self.stmts_need_async(b))
            }
            StmtKind::For { body, .. } | StmtKind::Whl { body, .. } => self.stmts_need_async(body),
            _ => false,
        }
    }

    fn expr_needs_async(&self, expr: &Expr) -> bool {
        if let ExprKind::Unwrap { expr, .. } = &expr.kind {
            if let ExprKind::Call { callee, .. } = &expr.kind {
                if let ExprKind::Ident(name) = &callee.kind {
                    return self.fn_info.get(name).map_or(false, |i| i.is_async);
                }
            }
        }
        false
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"    ".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn gen_program(&mut self, program: &Program) {
        self.line(
            0,
            "// Generated by the CuNi toy JavaScript backend. Do not hand-edit.",
        );
        self.out.push('\n');
        self.line(0, "function say(x) {");
        // `String(x)`, not a bare `console.log(x)`: Node's console.log
        // colorizes non-string values (numbers, booleans, ...) with ANSI
        // codes whenever color output is enabled (a real-terminal stdout, or
        // FORCE_COLOR set in the environment) — genuinely discovered via
        // tests/conformance.rs, whose `say(4)` output diverged from Python's
        // `print(4)`/Go's `fmt.Println(4)` specifically under FORCE_COLOR.
        // `String(x)` forces plain-text output unconditionally, matching
        // Python's `str()`- and Go's `%v`-driven `say` output on every value
        // shape this backend emits — except booleans: CuNi's canonical bool
        // spelling is Python's `True`/`False` (py/c/rs backends and the
        // interp all print that), so booleans are normalized here instead
        // of JS's native `true`/`false` — and except BigInts: a `dec` is a
        // scaled BigInt (docs/DECIMAL.md), so it renders via `_cuni_dec_str`
        // instead of `String(x)`, which would print the raw scaled integer.
        // No other CuNi value is a BigInt, so this changes nothing else.
        self.line(
            1,
            "console.log(typeof x === \"boolean\" ? (x ? \"True\" : \"False\") : typeof x === \"bigint\" ? _cuni_dec_str(x) : String(x));",
        );
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function range(n) {");
        self.line(1, "n = Math.trunc(Number(n));");
        self.line(1, "if (!(n > 0)) return [];");
        self.line(1, "const xs = [];");
        self.line(1, "for (let i = 0; i < n; i++) xs.push(i);");
        self.line(1, "return xs;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function abs(n) {");
        self.line(1, "n = Math.trunc(Number(n));");
        self.line(1, "return n < 0 ? -n : n;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function min(a, b) {");
        self.line(1, "a = Math.trunc(Number(a)); b = Math.trunc(Number(b));");
        self.line(1, "return a <= b ? a : b;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function max(a, b) {");
        self.line(1, "a = Math.trunc(Number(a)); b = Math.trunc(Number(b));");
        self.line(1, "return a >= b ? a : b;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_slice(xs, a, b) {");
        self.line(1, "a = Math.trunc(Number(a)); b = Math.trunc(Number(b));");
        self.line(1, "const n = xs.length;");
        self.line(1, "if (a < 0 || b < 0 || a > n || b > n || a > b) return typeof xs === \"string\" ? \"\" : [];");
        self.line(1, "return xs.slice(a, b);");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_div(a, b) {");
        self.line(
            1,
            "if (Number.isInteger(a) && Number.isInteger(b) && b !== 0) return Math.trunc(a / b);",
        );
        self.line(1, "return a / b;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "// CuNi `dec`: fixed-point decimal, scale 10^4, as BigInt (docs/DECIMAL.md).");
        self.line(0, "// A plain JS number is NOT exact (f64) — dec never touches Number.");
        self.line(0, "function _cuni_dec_str(v) {");
        self.line(1, "const neg = v < 0n;");
        self.line(1, "const mag = neg ? -v : v;");
        self.line(1, "const ip = mag / 10000n;");
        self.line(1, "let fp = (mag % 10000n).toString().padStart(4, \"0\").replace(/0+$/, \"\");");
        self.line(1, "if (fp === \"\") fp = \"0\";");
        self.line(1, "return (neg ? \"-\" : \"\") + ip.toString() + \".\" + fp;");
        self.line(0, "}");
        self.line(0, "function _cuni_dec_mul(a, b) {");
        self.line(1, "return (a * b) / 10000n;  // BigInt / truncates toward zero, exact");
        self.line(0, "}");
        self.line(0, "function _cuni_dec_div(a, b) {");
        self.line(1, "if (b === 0n) throw new Error(\"cuni: dec division by zero\");");
        self.line(1, "return (a * 10000n) / b;  // BigInt / truncates toward zero, exact");
        self.line(0, "}");
        self.line(0, "function _cuni_dec_of_int(n) {");
        self.line(1, "return BigInt(n) * 10000n;");
        self.line(0, "}");
        self.line(0, "function _cuni_int_of_dec(d) {");
        self.line(1, "return Number(d / 10000n);  // truncates toward zero, like every seat");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "// CuNi `time`: unix epoch seconds as BigInt, UTC (docs/TIME.md).");
        self.line(0, "// A plain JS number is NOT exact past 2^53 — time never touches Number,");
        self.line(0, "// following dec's BigInt precedent (docs/DECIMAL.md §7).");
        self.line(0, "function _cuni_time_str(v) {");
        self.line(1, "// Canonical ISO-8601 UTC rendering (docs/TIME.md §4), BigInt math.");
        self.line(1, "let days = v / 86400n, sod = v % 86400n;");
        self.line(1, "if (sod < 0n) { days -= 1n; sod += 86400n; }  // floor division");
        self.line(1, "const z = days + 719468n;");
        self.line(1, "const era = z >= 0n ? z / 146097n : -((-z + 146096n) / 146097n);");
        self.line(1, "const doe = z - era * 146097n;");
        self.line(1, "const yoe = (doe - doe / 1460n + doe / 36524n - doe / 146096n) / 365n;");
        self.line(1, "const y = yoe + era * 400n;");
        self.line(1, "const doy = doe - (365n * yoe + yoe / 4n - yoe / 100n);");
        self.line(1, "const mp = (5n * doy + 2n) / 153n;");
        self.line(1, "const d = doy - (153n * mp + 2n) / 5n + 1n;");
        self.line(1, "const m = mp < 10n ? mp + 3n : mp - 9n;");
        self.line(1, "const yy = m <= 2n ? y + 1n : y;");
        self.line(1, "const hh = sod / 3600n, mi = (sod % 3600n) / 60n, ss = sod % 60n;");
        self.line(1, "const ay = yy < 0n ? -yy : yy;");
        self.line(1, "let ys = ay.toString().padStart(4, \"0\");");
        self.line(1, "if (yy < 0n) ys = \"-\" + ys;");
        self.line(1, "const p2 = (n) => n.toString().padStart(2, \"0\");");
        self.line(1, "return ys + \"-\" + p2(m) + \"-\" + p2(d) + \"T\" + p2(hh) + \":\" + p2(mi) + \":\" + p2(ss) + \"Z\";");
        self.line(0, "}");
        self.line(0, "function _cuni_parse_time(s) {");
        self.line(1, "// Strict ISO-8601 UTC -> BigInt epoch (docs/TIME.md §2, §5).");
        self.line(1, "// Bad input throws loudly — never a silent value.");
        self.line(1, "const bad = () => { throw new Error(\"cuni: parse_time: bad ISO-8601 UTC timestamp — refused\"); };");
        self.line(1, "if (typeof s !== \"string\" || s.length !== 20) bad();");
        self.line(1, "if (s[4] !== \"-\" || s[7] !== \"-\" || s[10] !== \"T\" || s[13] !== \":\" || s[16] !== \":\" || s[19] !== \"Z\") bad();");
        self.line(1, "const dig = (i) => { const c = s.charCodeAt(i); if (c < 48 || c > 57) bad(); return BigInt(c - 48); };");
        self.line(1, "const y = dig(0)*1000n + dig(1)*100n + dig(2)*10n + dig(3);");
        self.line(1, "const mo = dig(5)*10n + dig(6), d = dig(8)*10n + dig(9);");
        self.line(1, "const h = dig(11)*10n + dig(12), mi = dig(14)*10n + dig(15), sec = dig(17)*10n + dig(18);");
        self.line(1, "if (y < 1n || y > 9999n || mo < 1n || mo > 12n) bad();");
        self.line(1, "let dim = 31n;");
        self.line(1, "if (mo === 4n || mo === 6n || mo === 9n || mo === 11n) dim = 30n;");
        self.line(1, "else if (mo === 2n) dim = (y % 4n === 0n && (y % 100n !== 0n || y % 400n === 0n)) ? 29n : 28n;");
        self.line(1, "if (d < 1n || d > dim || h > 23n || mi > 59n || sec > 59n) bad();");
        self.line(1, "const y0 = mo <= 2n ? y - 1n : y;");
        self.line(1, "const era = y0 / 400n, yoe = y0 - era * 400n;");
        self.line(1, "const mp = (mo + 9n) % 12n;");
        self.line(1, "const doy = (153n * mp + 2n) / 5n + d - 1n;");
        self.line(1, "const doe = yoe * 365n + yoe / 4n - yoe / 100n + doy;");
        self.line(1, "const days = era * 146097n + doe - 719468n;");
        self.line(1, "return days * 86400n + h * 3600n + mi * 60n + sec;");
        self.line(0, "}");
        self.line(0, "function _cuni_add_seconds(t, s) {");
        self.line(1, "return t + BigInt(s);  // BigInt: no overflow possible on this seat");
        self.line(0, "}");
        self.line(0, "function _cuni_days_between(a, b) {");
        self.line(1, "return Number((a - b) / 86400n);  // BigInt / truncates toward zero");
        self.line(0, "}");
        self.out.push('\n');
        self.out.push('\n');
        // ---- Wave-1 stdlib (docs/STDLIB.md). ----
        self.line(
            0,
            "// JSON: value-based integer rule (docs/STDLIB.md §1.1). JS numbers",
        );
        self.line(
            0,
            "// are f64, so number tokens are validated lexically (exact string",
        );
        self.line(
            0,
            "// arithmetic) BEFORE JSON.parse sees them — JSON.parse would",
        );
        self.line(
            0,
            "// silently round 9007199254740993 to 9007199254740992.",
        );
        self.line(
            0,
            "function _cuni_json_int_value(tok) {",
        );
        self.line(1, "const bad = () => { throw new CuNiError(\"json.parse: number is not an integer in ±(2^53−1)\"); };");
        self.line(1, "let t = tok, neg = false;");
        self.line(1, "if (t[0] === \"-\") { neg = true; t = t.slice(1); }");
        self.line(1, "let mant = t, exp = 0;");
        self.line(1, "const ei = mant.search(/[eE]/);");
        self.line(1, "if (ei >= 0) {");
        self.line(2, "const es = mant.slice(ei + 1);");
        self.line(2, "if (!/^[+-]?\\d+$/.test(es)) bad();");
        self.line(2, "exp = parseInt(es, 10); mant = mant.slice(0, ei);");
        self.line(1, "}");
        self.line(1, "let f = 0, digits = mant;");
        self.line(1, "const di = mant.indexOf(\".\");");
        self.line(1, "if (di >= 0) {");
        self.line(2, "const fp = mant.slice(di + 1);");
        self.line(2, "if (!/^\\d+$/.test(fp) || fp === \"\") bad();");
        self.line(2, "f = fp.length; digits = mant.slice(0, di) + fp;");
        self.line(1, "}");
        self.line(1, "if (!/^\\d+$/.test(digits) || digits === \"\") bad();");
        self.line(1, "digits = digits.replace(/^0+/, \"\");");
        self.line(1, "if (digits === \"\") return 0;");
        self.line(1, "const tz = digits.match(/0+$/);");
        self.line(1, "if (tz) { f -= tz[0].length; digits = digits.slice(0, -tz[0].length); }");
        self.line(1, "const k = f - exp;");
        self.line(1, "if (k > 0) bad(); // no trailing zeros left: can't divide evenly");
        self.line(1, "const total = digits + \"0\".repeat(-k);");
        self.line(1, "if (total.length > 16) bad();");
        self.line(1, "if (total.padStart(16, \"0\") > \"9007199254740991\") bad();");
        self.line(1, "let v = parseInt(total, 10); // <= 2^53-1: exact in f64");
        self.line(1, "if (neg) v = -v;");
        self.line(1, "return v;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "function _cuni_json_parse(s) {",
        );
        self.line(1, "if (typeof s !== \"string\") throw new CuNiError(\"json.parse needs a str\");");
        self.line(1, "// Lexical pre-check: string-aware scan so numbers inside strings");
        self.line(1, "// are skipped; every number token outside strings must satisfy");
        self.line(1, "// the integer rule. JSON.parse remains the syntax authority.");
        self.line(1, "// NOTE: no regex literals containing quotes here — the ingest");
        self.line(1, "// extractor is quote-aware but not regex-aware.");
        self.line(
            1,
            "const _cuni_num_re = /-?(?:0|[1-9]\\d*)(?:\\.\\d+)?(?:[eE][+-]?\\d+)?/g;",
        );
        self.line(1, "let _cuni_pi = 0, _cuni_pm;");
        self.line(1, "while (_cuni_pi < s.length) {");
        self.line(2, "const _cuni_pc = s[_cuni_pi];");
        self.line(2, "if (_cuni_pc === \"\\\"\") {");
        self.line(
            3,
            "_cuni_pi++; while (_cuni_pi < s.length) { const _cuni_pd = s[_cuni_pi]; if (_cuni_pd === \"\\\\\") _cuni_pi += 2; else { _cuni_pi++; if (_cuni_pd === \"\\\"\") break; } }",
        );
        self.line(2, "continue;");
        self.line(2, "}");
        self.line(2, "_cuni_num_re.lastIndex = _cuni_pi;");
        self.line(2, "_cuni_pm = _cuni_num_re.exec(s);");
        self.line(
            2,
            "if (_cuni_pm !== null && _cuni_pm.index === _cuni_pi) { _cuni_json_int_value(_cuni_pm[0]); _cuni_pi += _cuni_pm[0].length; } else { _cuni_pi++; }",
        );
        self.line(1, "}");
        self.line(1, "let v;");
        self.line(1, "try { v = JSON.parse(s); } catch (e) { throw new CuNiError(\"json.parse: invalid JSON\"); }");
        self.line(1, "const w = _cuni_json_walk(v);");
        self.line(1, "if (!(w instanceof Map)) throw new CuNiError(\"json.parse: top-level JSON value must be an object\");");
        self.line(1, "return w;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_json_walk(v) {");
        self.line(1, "if (typeof v === \"number\") {");
        self.line(2, "if (!Number.isInteger(v) || Math.abs(v) > 9007199254740991) throw new CuNiError(\"json.parse: number is not an integer in ±(2^53−1)\");");
        self.line(2, "return v;");
        self.line(1, "}");
        self.line(1, "if (typeof v === \"string\" || typeof v === \"boolean\" || v === null) return v;");
        self.line(1, "if (Array.isArray(v)) return v.map(_cuni_json_walk);");
        self.line(1, "if (typeof v === \"object\") {");
        self.line(2, "const out = new Map();");
        self.line(2, "for (const k of Object.keys(v)) out.set(k, _cuni_json_walk(v[k]));");
        self.line(2, "return out;");
        self.line(1, "}");
        self.line(1, "throw new CuNiError(\"json.parse: unexpected value\");");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_json_set(o, k, v) {");
        self.line(1, "// defineProperty: a plain assignment would route \"__proto__\" to the prototype.");
        self.line(1, "Object.defineProperty(o, k, { value: v, enumerable: true, writable: true, configurable: true });");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_json_norm(v) {");
        self.line(1, "if (v instanceof Map) {");
        self.line(2, "const o = {};");
        self.line(2, "for (const [k, x] of v) {");
        self.line(3, "if (typeof k !== \"string\") throw new CuNiError(\"json.emit: map keys must be strings\");");
        self.line(3, "_cuni_json_set(o, k, _cuni_json_norm(x));");
        self.line(2, "}");
        self.line(2, "return o;");
        self.line(1, "}");
        self.line(1, "if (Array.isArray(v)) return v.map(_cuni_json_norm);");
        self.line(1, "if (typeof v === \"number\") {");
        self.line(2, "if (!Number.isInteger(v) || Math.abs(v) > 9007199254740991) throw new CuNiError(\"json.emit: floats have no JSON integer form\");");
        self.line(2, "return v;");
        self.line(1, "}");
        self.line(1, "if (typeof v === \"string\" || typeof v === \"boolean\" || v === null) return v;");
        self.line(1, "throw new CuNiError(\"json.emit: value has no JSON form\");");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_json_emit(v) {");
        self.line(1, "if (!(v instanceof Map)) throw new CuNiError(\"json.emit needs a map\");");
        self.line(1, "return JSON.stringify(_cuni_json_sort(_cuni_json_norm(v)));");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_json_sort(v) {");
        self.line(1, "if (Array.isArray(v)) return v.map(_cuni_json_sort);");
        self.line(1, "if (v !== null && typeof v === \"object\") {");
        self.line(2, "const o = {};");
        self.line(2, "for (const k of Object.keys(v).sort()) _cuni_json_set(o, k, _cuni_json_sort(v[k]));");
        self.line(2, "return o;");
        self.line(1, "}");
        self.line(1, "return v;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "// Time: proleptic Gregorian, no leap seconds, years 1..9999 (docs/STDLIB.md §2).");
        self.line(0, "function _cuni_days_from_civil(y, m, d) {");
        self.line(1, "const y0 = m <= 2 ? y - 1 : y;");
        self.line(1, "const era = Math.floor(y0 / 400);");
        self.line(1, "const yoe = y0 - era * 400;");
        self.line(1, "const mp = (m + 9) % 12;");
        self.line(1, "const doy = Math.floor((153 * mp + 2) / 5) + d - 1;");
        self.line(1, "const doe = yoe * 365 + Math.floor(yoe / 4) - Math.floor(yoe / 100) + doy;");
        self.line(1, "return era * 146097 + doe - 719468;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_civil_from_days(z) {");
        self.line(1, "z += 719468;");
        self.line(1, "const era = Math.floor(z / 146097);");
        self.line(1, "const doe = z - era * 146097;");
        self.line(1, "const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365);");
        self.line(1, "let y = yoe + era * 400;");
        self.line(1, "const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));");
        self.line(1, "const mp = Math.floor((5 * doy + 2) / 153);");
        self.line(1, "const d = doy - Math.floor((153 * mp + 2) / 5) + 1;");
        self.line(1, "let m = mp < 10 ? mp + 3 : mp - 9;");
        self.line(1, "if (m <= 2) y++;");
        self.line(1, "return [y, m, d];");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_time_epoch(y, mo, d, h, mi, s) {");
        self.line(1, "const chk = (v, lo, hi, nm) => { if (!(v >= lo && v <= hi)) throw new CuNiError(\"time.epoch: \" + nm + \" out of range\"); };");
        self.line(1, "chk(y, 1, 9999, \"year\"); chk(mo, 1, 12, \"month\"); chk(h, 0, 23, \"hour\"); chk(mi, 0, 59, \"minute\"); chk(s, 0, 59, \"second\");");
        self.line(1, "const leap = y % 4 === 0 && (y % 100 !== 0 || y % 400 === 0);");
        self.line(1, "const dim = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][mo - 1];");
        self.line(1, "if (!(d >= 1 && d <= dim)) throw new CuNiError(\"time.epoch: day out of range for month\");");
        self.line(1, "return _cuni_days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s;");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_time_parts(e) {");
        self.line(1, "const lo = _cuni_days_from_civil(1, 1, 1) * 86400;");
        self.line(1, "const hi = _cuni_days_from_civil(9999, 12, 31) * 86400 + 86399;");
        self.line(1, "if (!(e >= lo && e <= hi)) throw new CuNiError(\"time.parts: epoch out of range 1..9999\");");
        self.line(1, "const days = Math.floor(e / 86400);");
        self.line(1, "const secs = e - days * 86400;");
        self.line(1, "const [y, mo, d] = _cuni_civil_from_days(days);");
        self.line(1, "return new Map([[\"year\", y], [\"month\", mo], [\"day\", d], [\"hour\", Math.floor(secs / 3600)], [\"min\", Math.floor((secs % 3600) / 60)], [\"sec\", secs % 60]]);");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "// String ops: byte-oriented on UTF-8 (docs/STDLIB.md §3).");
        self.line(0, "function _cuni_split(s, sep) {");
        self.line(1, "if (typeof s !== \"string\" || typeof sep !== \"string\") throw new CuNiError(\".split needs strings\");");
        self.line(1, "if (sep === \"\") throw new CuNiError(\".split: empty separator; refusing\");");
        self.line(1, "return s.split(sep);");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_join(sep, parts) {");
        self.line(1, "if (typeof sep !== \"string\") throw new CuNiError(\".join needs a str separator\");");
        self.line(1, "if (!Array.isArray(parts)) throw new CuNiError(\".join needs a list<str>\");");
        self.line(1, "for (const x of parts) if (typeof x !== \"string\") throw new CuNiError(\".join: all parts must be str\");");
        self.line(1, "return parts.join(sep);");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_trim(s) {");
        self.line(1, "if (typeof s !== \"string\") throw new CuNiError(\".trim needs a str\");");
        self.line(1, "return s.replace(/^[ \\t\\n\\v\\f\\r]+|[ \\t\\n\\v\\f\\r]+$/g, \"\");");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_contains(s, sub) {");
        self.line(1, "if (typeof s !== \"string\" || typeof sub !== \"string\") throw new CuNiError(\".contains needs strings\");");
        self.line(1, "return s.includes(sub);");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "function _cuni_sha256(s) {");
        self.line(1, "if (typeof s !== \"string\") throw new CuNiError(\"sha256 needs a str\");");
        self.line(1, "return require(\"crypto\").createHash(\"sha256\").update(s, \"utf8\").digest(\"hex\");");        self.line(0, "}");
        self.out.push('\n');
        self.line(
            0,
            "// Raised by `fail` — CuNi's explicit failure-signaling statement.",
        );
        self.line(0, "class CuNiError extends Error {}");
        self.out.push('\n');

        // Top-level `ret` (e.g. inside a `??` handler on a top-level `let`)
        // has no enclosing function in CuNi's source, but `return` is a
        // syntax error outside a function in JS too. So every top-level
        // statement is collected and wrapped in an implicit `main()`, called
        // at the bottom — see module docs.
        let mut script_stmts: Vec<&Stmt> = Vec::new();
        let mut top_scope: HashMap<String, VarKind> = HashMap::new();
        for item in &program.items {
            if let Item::Stmt(s) = item {
                script_stmts.push(s);
            } else {
                self.gen_item(item, &mut top_scope);
                self.out.push('\n');
            }
        }

        // See `stmts_need_async`'s docs: `main()` must be declared `async`
        // if any top-level statement directly `??`-unwraps a call to an
        // async function (currently only ever a `link`'s `<name>_remote`).
        let main_needs_async = script_stmts.iter().any(|s| self.stmt_needs_async(s));
        self.line(
            0,
            if main_needs_async {
                "async function main() {"
            } else {
                "function main() {"
            },
        );
        for s in script_stmts {
            self.gen_stmt(1, s, &mut top_scope);
        }
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "main();");
    }

    fn gen_item(&mut self, item: &Item, scope: &mut HashMap<String, VarKind>) {
        match item {
            Item::Use(u) => {
                self.line(
                    0,
                    &format!(
                        "// use {} — portable CuNi module, not resolved by this toy backend",
                        u.name
                    ),
                );
            }
            Item::Ext(ext) => match ext.targets.iter().find(|(t, _)| t == "js") {
                Some((_, raw)) => {
                    let is_async = raw.contains("await");
                    let async_kw = if is_async { "async " } else { "" };
                    self.line(
                        0,
                        &format!(
                            "{}function {}({}) {{",
                            async_kw,
                            ext.name,
                            params_sig(&ext.params)
                        ),
                    );
                    self.line(1, &format!("return {};", raw));
                    self.line(0, "}");
                }
                None => {
                    self.line(
                        0,
                        &format!("function {}({}) {{", ext.name, params_sig(&ext.params)),
                    );
                    self.line(1, "throw new Error(\"no js: mapping given\");");
                    self.line(0, "}");
                }
            },
            Item::Typ(t) => {
                let impl_comment = t
                    .implements
                    .clone()
                    .map(|b| format!("  // implements {} (unenforced in JS — see iface handling in codegen_js.rs)", b))
                    .unwrap_or_default();
                self.line(0, &format!("class {} {{{}", t.name, impl_comment));
                if t.fields.is_empty() {
                    self.line(1, "constructor() {}");
                } else {
                    let params = t
                        .fields
                        .iter()
                        .map(|f| f.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.line(1, &format!("constructor({}) {{", params));
                    for f in &t.fields {
                        self.line(2, &format!("this.{} = {};", f.name, f.name));
                    }
                    self.line(1, "}");
                }
                self.line(0, "}");
            }
            Item::Iface(i) => {
                self.line(0, &format!("// iface {} — JS has no interfaces/ABCs; conformance is structural/unenforced here", i.name));
                for m in &i.methods {
                    let params = m
                        .params
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.line(0, &format!("//   {}({})", m.name, params));
                }
            }
            Item::Enum(e) => {
                let pairs = e
                    .variants
                    .iter()
                    .map(|v| format!("{}: {:?}", v.name, v.name))
                    .collect::<Vec<_>>()
                    .join(", ");
                self.line(
                    0,
                    &format!("const {} = Object.freeze({{{}}});", e.name, pairs),
                );
            }
            Item::Def(f) => {
                let generics_note = if f.generics.is_empty() {
                    String::new()
                } else {
                    format!("  // generic over {}", f.generics.join(", "))
                };
                let async_kw = if self.stmts_need_async(&f.body) {
                    "async "
                } else {
                    ""
                };
                self.line(
                    0,
                    &format!(
                        "{}function {}({}) {{{}",
                        async_kw,
                        f.name,
                        params_sig(&f.params),
                        generics_note
                    ),
                );
                let mut fn_scope: HashMap<String, VarKind> = HashMap::new();
                for p in &f.params {
                    fn_scope.insert(p.name.clone(), kind_of_type(&p.ty));
                }
                for s in &f.body {
                    self.gen_stmt(1, s, &mut fn_scope);
                }
                self.line(0, "}");
                if f.is_link {
                    self.out.push('\n');
                    self.gen_link_handler(f);
                    self.out.push('\n');
                    self.gen_link_remote(f);
                }
            }
            Item::Stmt(s) => self.gen_stmt(0, s, scope),
        }
    }

    /// `link Name(...) -> T [?] do ... end` (SPEC.md §19) additionally emits
    /// a Node `http`-shaped handler `(req, res) => {...}` — mount it
    /// yourself (e.g. by checking `req.url`/`req.method` in your own
    /// `http.createServer` callback), per the ratified "codegen-only, no
    /// bundled runtime" decision (INTEROP_PROPOSAL.md item 6). Any error
    /// thrown by the local function (including `fail`/stub bodies, which
    /// already `throw`) is caught generically and reported as a JSON error
    /// response, same as the other two backends.
    fn gen_link_handler(&mut self, f: &FnDecl) {
        self.line(0, &format!("function {}_handler(req, res) {{", f.name));
        self.line(1, "let body = \"\";");
        self.line(1, "req.on(\"data\", (chunk) => { body += chunk; });");
        self.line(1, "req.on(\"end\", () => {");
        self.line(2, "try {");
        self.line(3, "const parsed = JSON.parse(body);");
        let args = f
            .params
            .iter()
            .map(|p| js_wire_decode(&format!("parsed.{}", p.name), &p.ty))
            .collect::<Vec<_>>()
            .join(", ");
        self.line(3, &format!("const result = {}({});", f.name, args));
        self.line(
            3,
            "res.writeHead(200, {\"Content-Type\": \"application/json\"});",
        );
        self.line(
            3,
            &format!(
                "res.end(JSON.stringify({{result: {}}}));",
                js_wire_encode("result", &f.ret_type)
            ),
        );
        self.line(2, "} catch (e) {");
        self.line(
            3,
            "res.writeHead(400, {\"Content-Type\": \"application/json\"});",
        );
        self.line(3, "res.end(JSON.stringify({error: e.message}));");
        self.line(2, "}");
        self.line(1, "});");
        self.line(0, "}");
    }

    /// The client side of the same `link`: always `async` (it awaits
    /// `fetch`) and always effectively fallible — a network call can fail
    /// even when the local logic can't — reusing `CuNiError`/`??` rather
    /// than inventing a second error channel for network failures
    /// specifically (see `FnInfo::is_async`'s docs for the async-coloring
    /// caveat this creates for direct callers).
    fn gen_link_remote(&mut self, f: &FnDecl) {
        let params_sig = f
            .params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        self.line(
            0,
            &format!(
                "async function {}_remote(baseUrl, {}) {{",
                f.name, params_sig
            ),
        );
        let fields = f
            .params
            .iter()
            .map(|p| format!("{}: {}", p.name, js_wire_encode(&p.name, &p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        self.line(
            1,
            &format!("const res = await fetch(baseUrl + \"/{}\", {{", f.name),
        );
        self.line(2, "method: \"POST\",");
        self.line(2, "headers: {\"Content-Type\": \"application/json\"},");
        self.line(2, &format!("body: JSON.stringify({{{}}}),", fields));
        self.line(1, "});");
        self.line(1, "const data = await res.json();");
        self.line(1, "if (data.error) {");
        self.line(2, "throw new CuNiError(data.error);");
        self.line(1, "}");
        self.line(
            1,
            &format!("return {};", js_wire_decode("data.result", &f.ret_type)),
        );
        self.line(0, "}");
    }

    fn gen_stmt(&mut self, indent: usize, stmt: &Stmt, scope: &mut HashMap<String, VarKind>) {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } => {
                let mut kind = ty
                    .as_ref()
                    .map(kind_of_type)
                    .or_else(|| kind_of_literal(value))
                    .unwrap_or(VarKind::Other);
                if kind == VarKind::Other && is_dec_expr(value, scope, &self.fn_dec_rets) {
                    kind = VarKind::Dec;
                }
                if kind == VarKind::Other && is_time_expr(value, scope, &self.fn_time_rets) {
                    kind = VarKind::Time;
                }
                scope.insert(name.clone(), kind);
                self.gen_binding(indent, "const", name, value, scope);
            }
            StmtKind::Mut { name, ty, value } => {
                let mut kind = ty
                    .as_ref()
                    .map(kind_of_type)
                    .or_else(|| kind_of_literal(value))
                    .unwrap_or(VarKind::Other);
                if kind == VarKind::Other && is_dec_expr(value, scope, &self.fn_dec_rets) {
                    kind = VarKind::Dec;
                }
                if kind == VarKind::Other && is_time_expr(value, scope, &self.fn_time_rets) {
                    kind = VarKind::Time;
                }
                scope.insert(name.clone(), kind);
                self.gen_binding(indent, "let", name, value, scope);
            }
            StmtKind::Assign { target, value } => {
                self.line(
                    indent,
                    &format!(
                        "{} = {};",
                        self.gen_expr(target, scope),
                        self.gen_expr(value, scope)
                    ),
                );
            }
            StmtKind::Ret(Some(e)) => {
                let text = self.gen_expr(e, scope);
                self.line(indent, &format!("return {};", text));
            }
            StmtKind::Ret(None) => self.line(indent, "return;"),
            StmtKind::Fail(e) => {
                let text = self.gen_expr(e, scope);
                self.line(indent, &format!("throw new CuNiError({});", text));
            }
            StmtKind::If {
                cond,
                then_body,
                else_body,
            } => {
                self.line(indent, &format!("if ({}) {{", self.gen_expr(cond, scope)));
                self.gen_block(indent + 1, then_body, scope);
                match else_body {
                    Some(else_body) => {
                        self.line(indent, "} else {");
                        self.gen_block(indent + 1, else_body, scope);
                        self.line(indent, "}");
                    }
                    None => self.line(indent, "}"),
                }
            }
            StmtKind::For {
                binding: (a, b),
                iter,
                body,
            } => {
                let iter_kind = if let ExprKind::Ident(name) = &iter.kind {
                    scope.get(name).copied()
                } else {
                    None
                };
                let header = match b {
                    Some(b) if iter_kind == Some(VarKind::Map) => {
                        format!(
                            "for (const [{}, {}] of {}) {{",
                            a,
                            b,
                            self.gen_expr(iter, scope)
                        )
                    }
                    Some(b) => format!(
                        "for (const [{}, {}] of {}.entries()) {{",
                        a,
                        b,
                        self.gen_expr(iter, scope)
                    ),
                    None => format!("for (const {} of {}) {{", a, self.gen_expr(iter, scope)),
                };
                self.line(indent, &header);
                self.gen_block(indent + 1, body, scope);
                self.line(indent, "}");
            }
            StmtKind::Whl { cond, body } => {
                self.line(
                    indent,
                    &format!("while ({}) {{", self.gen_expr(cond, scope)),
                );
                self.gen_block(indent + 1, body, scope);
                self.line(indent, "}");
            }
            StmtKind::ExprStmt(e) => {
                let text = self.gen_expr(e, scope);
                self.line(indent, &format!("{};", text));
            }
            StmtKind::Todo => {
                self.line(
                    indent,
                    "throw new Error(\"...\"); // CuNi stub body (`...`) — not yet written",
                );
            }
        }
    }

    fn gen_block(&mut self, indent: usize, stmts: &[Stmt], scope: &mut HashMap<String, VarKind>) {
        for s in stmts {
            self.gen_stmt(indent, s, scope);
        }
    }

    /// `let name = expr ?? do handler end` (or `mut`) is the only Unwrap
    /// position this toy backend supports (see module docs). `name` is only
    /// bound on the success path — the handler is expected to diverge
    /// (`ret`), matching every example program written against the spec so
    /// far. `keyword` is `"const"` for `let` and `"let"` for `mut`.
    fn gen_binding(
        &mut self,
        indent: usize,
        keyword: &str,
        name: &str,
        value: &Expr,
        scope: &mut HashMap<String, VarKind>,
    ) {
        if let ExprKind::Unwrap { expr, handler } = &value.kind {
            let is_fallible_call = matches!(&expr.kind, ExprKind::Call { callee, .. } if matches!(&callee.kind, ExprKind::Ident(fname) if self.fn_info.get(fname).map_or(false, |i| i.fallible))
            );
            // A `link`'s `<name>_remote` is always async (see FnInfo::is_async's
            // docs) — needs `await` here, and the enclosing function/`main()`
            // must itself be declared `async` (handled separately, at each
            // function's own declaration site, by `stmts_need_async`).
            let is_async_call = matches!(&expr.kind, ExprKind::Call { callee, .. } if matches!(&callee.kind, ExprKind::Ident(fname) if self.fn_info.get(fname).map_or(false, |i| i.is_async))
            );
            let inner_raw = self.gen_expr(expr, scope);
            let inner = if is_async_call {
                format!("await {}", inner_raw)
            } else {
                inner_raw
            };
            let tmp = format!("_u{}", self.unwrap_counter);
            self.unwrap_counter += 1;
            if is_fallible_call {
                self.line(indent, &format!("let {};", tmp));
                self.line(indent, "try {");
                self.line(indent + 1, &format!("{} = {};", tmp, inner));
                self.line(indent, "} catch (_e) {");
                self.gen_block(indent + 1, handler, scope);
                self.line(indent, "}");
                self.line(indent, &format!("{} {} = {};", keyword, name, tmp));
            } else {
                self.line(indent, &format!("const {} = {};", tmp, inner));
                self.line(indent, &format!("if ({} === null) {{", tmp));
                self.gen_block(indent + 1, handler, scope);
                self.line(indent, "}");
                self.line(indent, &format!("{} {} = {};", keyword, name, tmp));
            }
        } else {
            self.line(
                indent,
                &format!("{} {} = {};", keyword, name, self.gen_expr(value, scope)),
            );
        }
    }

    fn gen_expr(&self, expr: &Expr, scope: &HashMap<String, VarKind>) -> String {
        match &expr.kind {
            ExprKind::Int(n) => n.to_string(),
            // Scaled BigInt literal — the `n` suffix IS the dec tag, so no
            // f64 `Number` ever touches a dec (docs/DECIMAL.md §7).
            ExprKind::Dec(s) => format!("{s}n"),
            // Epoch BigInt literal — the `n` suffix keeps it off f64 too
            // (docs/TIME.md §7); `say`/interpolation route via _cuni_time_str.
            ExprKind::Time(e) => format!("{e}n"),
            ExprKind::Float(f) => f.to_string(),
            ExprKind::Bool(b) => b.to_string(),
            ExprKind::Str(s) => format!("{:?}", s),
            ExprKind::InterpStr(parts) => {
                let mut s = String::from("`");
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => s.push_str(&escape_template_text(t)),
                        StrPartExpr::Expr(e) => {
                            s.push_str("${");
                            let inner = self.gen_expr(e, scope);
                            // A dec BigInt must render canonically, not as
                            // its raw scaled integer (docs/DECIMAL.md §6).
                            if is_dec_expr(e, scope, &self.fn_dec_rets) {
                                s.push_str(&format!("_cuni_dec_str({inner})"));
                            } else if is_time_expr(e, scope, &self.fn_time_rets) {
                                // A time BigInt renders as ISO-8601, not as
                                // its raw epoch (docs/TIME.md §4).
                                s.push_str(&format!("_cuni_time_str({inner})"));
                            } else {
                                s.push_str(&inner);
                            }
                            s.push('}');
                        }
                    }
                }
                s.push('`');
                s
            }
            ExprKind::NoneLit => "null".to_string(),
            ExprKind::Ident(name) => name.clone(),
            ExprKind::List(items) => format!(
                "[{}]",
                items
                    .iter()
                    .map(|e| self.gen_expr(e, scope))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ExprKind::Map(pairs) => format!(
                "new Map([{}])",
                pairs
                    .iter()
                    .map(|(k, v)| format!(
                        "[{}, {}]",
                        self.gen_expr(k, scope),
                        self.gen_expr(v, scope)
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ExprKind::Call { callee, args } => {
                // A `time` must render canonically via `_cuni_time_str` —
                // never as its raw epoch BigInt, and never through `say`'s
                // BigInt→dec branch (docs/TIME.md §4).
                if let ExprKind::Ident(n) = &callee.kind {
                    if n == "say"
                        && args.len() == 1
                        && is_time_expr(args[0].expr(), scope, &self.fn_time_rets)
                    {
                        return format!(
                            "say(_cuni_time_str({}))",
                            self.gen_expr(args[0].expr(), scope)
                        );
                    }
                }
                // `dec` explicit conversions (docs/DECIMAL.md §5).
                if let ExprKind::Ident(n) = &callee.kind {
                    let one = || {
                        args.first()
                            .map(|a| self.gen_expr(a.expr(), scope))
                            .unwrap_or_else(|| "null".to_string())
                    };
                    let two = || {
                        let a = args
                            .first()
                            .map(|a| self.gen_expr(a.expr(), scope))
                            .unwrap_or_else(|| "null".to_string());
                        let b = args
                            .get(1)
                            .map(|a| self.gen_expr(a.expr(), scope))
                            .unwrap_or_else(|| "null".to_string());
                        format!("{a}, {b}")
                    };
                    match n.as_str() {
                        "dec_of_int" => return format!("_cuni_dec_of_int({})", one()),
                        "int_of_dec" => return format!("_cuni_int_of_dec({})", one()),
                        // `time` builtins (docs/TIME.md §5).
                        "parse_time" => return format!("_cuni_parse_time({})", one()),
                        "add_seconds" => return format!("_cuni_add_seconds({})", two()),
                        "days_between" => return format!("_cuni_days_between({})", two()),
                        _ => {}
                    }
                }
                // `.len()` is a method call in CuNi but a property in JS
                // (`.length`, no parens) — needs its own rewrite, unlike
                // `.push`, which already matches JS's own method shape.
                if let ExprKind::Field { base, name } = &callee.kind {
                    // Wave-1 stdlib namespaces (docs/STDLIB.md).
                    if let ExprKind::Ident(ns) = &base.kind {
                        if ns == "json" || ns == "time" {
                            let a = args
                                .iter()
                                .map(|a| self.gen_expr(a.expr(), scope))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let f = match (ns.as_str(), name.as_str()) {
                                ("json", "parse") => "_cuni_json_parse",
                                ("json", "emit") => "_cuni_json_emit",
                                ("time", "epoch") => "_cuni_time_epoch",
                                ("time", "parts") => "_cuni_time_parts",
                                _ => "_cuni_stdlib_unknown",
                            };
                            return format!("{f}({a})");
                        }
                    }
                    if name == "len" {
                        return format!("{}.length", self.gen_expr(base, scope));
                    }
                    if name == "slice" && args.len() == 2 {
                        return format!(
                            "_cuni_slice({}, {}, {})",
                            self.gen_expr(base, scope),
                            self.gen_expr(args[0].expr(), scope),
                            self.gen_expr(args[1].expr(), scope)
                        );
                    }
                    // Wave-1 string ops (docs/STDLIB.md §3): the native
                    // methods either don't exist (join/contains) or disagree
                    // with the spec (trim's Unicode set; split's empty-sep
                    // behavior), so all four go through checked helpers.
                    if name == "split" && args.len() == 1 {
                        return format!(
                            "_cuni_split({}, {})",
                            self.gen_expr(base, scope),
                            self.gen_expr(args[0].expr(), scope)
                        );
                    }
                    if name == "join" && args.len() == 1 {
                        return format!(
                            "_cuni_join({}, {})",
                            self.gen_expr(base, scope),
                            self.gen_expr(args[0].expr(), scope)
                        );
                    }
                    if name == "trim" && args.is_empty() {
                        return format!("_cuni_trim({})", self.gen_expr(base, scope));
                    }
                    if name == "contains" && args.len() == 1 {
                        return format!(
                            "_cuni_contains({}, {})",
                            self.gen_expr(base, scope),
                            self.gen_expr(args[0].expr(), scope)
                        );
                    }
                }
                // Named typ args: object literal style if we had that; JS classes use
                // positional constructors — reorder is typeck's job only for
                // validation; here emit `new T({field: val, ...})` only if
                // the class is generated to accept a bag. Our codegen uses
                // positional fields, so map named → positional declaration order
                // when we know field names from typ_fields if present.
                let arg_list = if args.iter().all(|a| a.is_named()) && !args.is_empty() {
                    // Keep declaration order if typ_fields known; else name order
                    if let ExprKind::Ident(tname) = &callee.kind {
                        if let Some(fields) = self.typ_fields.get(tname) {
                            fields
                                .iter()
                                .filter_map(|f| {
                                    args.iter().find_map(|a| match a {
                                        CallArg::Named { name, value, .. } if name == f => {
                                            Some(self.gen_expr(value, scope))
                                        }
                                        _ => None,
                                    })
                                })
                                .collect::<Vec<_>>()
                                .join(", ")
                        } else {
                            args.iter()
                                .map(|a| self.gen_expr(a.expr(), scope))
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    } else {
                        args.iter()
                            .map(|a| self.gen_expr(a.expr(), scope))
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                } else {
                    args.iter()
                        .map(|a| self.gen_expr(a.expr(), scope))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                // JS class constructors require `new` — bare `Circle(1.5)` throws.
                if let ExprKind::Ident(tname) = &callee.kind {
                    if self.typ_names.contains(tname) {
                        return format!("new {}({})", tname, arg_list);
                    }
                    // Wave-1 stdlib free function (docs/STDLIB.md §4).
                    if tname == "sha256" {
                        return format!("_cuni_sha256({})", arg_list);
                    }
                }
                format!("{}({})", self.gen_expr(callee, scope), arg_list)
            }
            ExprKind::Index { base, index } => format!(
                "{}[{}]",
                self.gen_expr(base, scope),
                self.gen_expr(index, scope)
            ),
            ExprKind::Field { base, name } => format!("{}.{}", self.gen_expr(base, scope), name),
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.gen_expr(lhs, scope);
                let r = self.gen_expr(rhs, scope);
                // `dec` mul/div rescale (docs/DECIMAL.md §3); add/sub and all
                // comparisons are natively exact on BigInt. The typeck proved
                // both operands dec, so checking one side suffices.
                if is_dec_expr(lhs, scope, &self.fn_dec_rets) {
                    match op {
                        BinOp::Mul => return format!("_cuni_dec_mul({l}, {r})"),
                        BinOp::Div => return format!("_cuni_dec_div({l}, {r})"),
                        _ => {}
                    }
                }
                // `time` arithmetic (docs/TIME.md §3): epoch BigInts; the int
                // (duration) side is lifted with BigInt() — a raw Number would
                // throw on mixed BigInt/Number ops. The typeck proved the
                // valid shapes (time±int, time−time, time comparisons).
                if is_time_expr(lhs, scope, &self.fn_time_rets)
                    || is_time_expr(rhs, scope, &self.fn_time_rets)
                {
                    let lt = is_time_expr(lhs, scope, &self.fn_time_rets);
                    let rt = is_time_expr(rhs, scope, &self.fn_time_rets);
                    match op {
                        BinOp::Add => {
                            // Exactly one side is time (typeck proved it).
                            if lt && !rt {
                                return format!("({l} + BigInt({r}))");
                            } else if rt && !lt {
                                return format!("(BigInt({l}) + {r})");
                            }
                        }
                        BinOp::Sub => {
                            if lt && rt {
                                // time - time -> int (Number, like int_of_dec).
                                return format!("Number({l} - {r})");
                            } else if lt {
                                return format!("({l} - BigInt({r}))");
                            }
                        }
                        BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Gt
                        | BinOp::Le
                        | BinOp::Ge => {
                            if lt && rt {
                                return format!("({} {} {})", l, js_binop(*op), r);
                            }
                        }
                        _ => {}
                    }
                    // Defense in depth only (the typeck proved the valid
                    // shapes): a runtime throw, never a silent value.
                    return "(function(){ throw new CuNiError(\"time binary op shape rejected by codegen; refusing\"); })()".to_string();
                }
                if matches!(op, BinOp::Div) {
                    format!("_cuni_div({}, {})", l, r)
                } else {
                    format!("({} {} {})", l, js_binop(*op), r)
                }
            }
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => format!("(!{})", self.gen_expr(expr, scope)),
                UnOp::Neg => format!("(-{})", self.gen_expr(expr, scope)),
            },
            ExprKind::Unwrap { .. } => {
                "null /* UNSUPPORTED: ?? outside a let/mut binding, see codegen_js.rs docs */"
                    .to_string()
            }
        }
    }
}

fn params_sig(params: &[Param]) -> String {
    params
        .iter()
        .map(|p| p.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

fn kind_of_type(ty: &Type) -> VarKind {
    match ty {
        Type::Named(n) if n == "dec" => VarKind::Dec,
        Type::Named(n) if n == "time" => VarKind::Time,
        Type::Generic(name, _) if name == "list" => VarKind::List,
        Type::Generic(name, _) if name == "map" => VarKind::Map,
        _ => VarKind::Other,
    }
}

fn kind_of_literal(e: &Expr) -> Option<VarKind> {
    match &e.kind {
        ExprKind::List(_) => Some(VarKind::List),
        ExprKind::Map(_) => Some(VarKind::Map),
        ExprKind::Dec(_) => Some(VarKind::Dec),
        ExprKind::Time(_) => Some(VarKind::Time),
        _ => None,
    }
}

/// Best-effort `dec` tracking for the JS backend: BigInt dec values need
/// `_cuni_dec_mul`/`_cuni_dec_div` (plain `*`/`/` would compute the wrong
/// scale), so the codegen must know which expressions are dec. The typeck
/// already proved dec-ness; this just re-derives it from literals,
/// annotations, `dec_of_int`, `-> dec` returns, and dec binops.
fn is_dec_expr(
    expr: &Expr,
    scope: &HashMap<String, VarKind>,
    fn_dec_rets: &std::collections::HashSet<String>,
) -> bool {
    match &expr.kind {
        ExprKind::Dec(_) => true,
        ExprKind::Ident(n) => scope.get(n) == Some(&VarKind::Dec),
        ExprKind::Call { callee, .. } => match &callee.kind {
            ExprKind::Ident(n) => n == "dec_of_int" || fn_dec_rets.contains(n),
            _ => false,
        },
        ExprKind::Binary { op, lhs, .. } => {
            matches!(
                op,
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div
            ) && is_dec_expr(lhs, scope, fn_dec_rets)
        }
        ExprKind::Unary { op, expr } => {
            matches!(op, UnOp::Neg) && is_dec_expr(expr, scope, fn_dec_rets)
        }
        _ => false,
    }
}

/// Best-effort `time` tracking for the JS backend: the int (duration) side
/// of `time ± int` must be lifted with `BigInt()` (a raw Number would throw
/// on mixed BigInt/Number ops), `time − time` must be wrapped in `Number()`
/// to come back as a CuNi int, and `say`/interpolation must route through
/// `_cuni_time_str` (the prelude's `typeof bigint` branch is dec's). The
/// typeck already proved time-ness; this just re-derives it from literals,
/// annotations, the `time` builtins, `-> time` returns, and time binops.
fn is_time_expr(
    expr: &Expr,
    scope: &HashMap<String, VarKind>,
    fn_time_rets: &std::collections::HashSet<String>,
) -> bool {
    match &expr.kind {
        ExprKind::Time(_) => true,
        ExprKind::Ident(n) => scope.get(n) == Some(&VarKind::Time),
        ExprKind::Call { callee, .. } => match &callee.kind {
            ExprKind::Ident(n) => {
                n == "parse_time" || n == "add_seconds" || fn_time_rets.contains(n)
            }
            _ => false,
        },
        ExprKind::Binary { op, lhs, rhs } => match op {
            // time + int -> time (exactly one side time; typeck proved it).
            BinOp::Add => {
                is_time_expr(lhs, scope, fn_time_rets) != is_time_expr(rhs, scope, fn_time_rets)
            }
            // time - int -> time; time - time -> int (not time).
            BinOp::Sub => {
                is_time_expr(lhs, scope, fn_time_rets) && !is_time_expr(rhs, scope, fn_time_rets)
            }
            _ => false,
        },
        ExprKind::Unary { op, expr } => {
            matches!(op, UnOp::Neg) && is_time_expr(expr, scope, fn_time_rets)
        }
        _ => false,
    }
}

fn escape_template_text(t: &str) -> String {
    t.replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace('$', "\\$")
}

/// `link`'s wire codec (SPEC.md §19): an `int` field is wire-encoded as a
/// JSON *string*, not a number — see `codegen_py.rs`'s `py_wire_encode` docs
/// for the full rationale (JSON has no arbitrary-precision integer type).
/// Decoding it back with `Number(...)` is the one place this specific
/// concern bites hardest: JS's own `Number` is a float64 and silently loses
/// precision above 2^53, so a `link` int field wider than that round-trips
/// losslessly onto the wire (as a string) but NOT back into a JS `Number`
/// here. This is a real, disclosed limitation, not a solved problem — using
/// `BigInt` instead would fix it but would make `link`-decoded ints a
/// different runtime representation than every other CuNi `int` in this
/// backend (which are plain `Number`s throughout), and BigInt/Number cannot
/// be mixed in an expression without an explicit conversion. Out of scope
/// for this toy implementation; flagged rather than silently either
/// "solved" or unmentioned.
fn js_wire_encode(expr: &str, ty: &Type) -> String {
    match ty {
        Type::Named(name) if name == "int" => format!("String({})", expr),
        _ => expr.to_string(),
    }
}

fn js_wire_decode(expr: &str, ty: &Type) -> String {
    match ty {
        Type::Named(name) if name == "int" => format!("Number({})", expr),
        _ => expr.to_string(),
    }
}

fn js_binop(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Mod => "%",
        BinOp::Eq => "===",
        BinOp::Ne => "!==",
        BinOp::Lt => "<",
        BinOp::Gt => ">",
        BinOp::Le => "<=",
        BinOp::Ge => ">=",
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

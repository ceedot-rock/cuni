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
///   example (examples/modules.cuni) originally surfaced a real gap when
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
    out: String,
    unwrap_counter: usize,
}

impl Codegen {
    fn new(program: &Program) -> Self {
        let mut fn_info = HashMap::new();
        let mut typ_names = std::collections::HashSet::new();
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    fn_info.insert(f.name.clone(), FnInfo { fallible: f.fallible, is_async: false });
                    if f.is_link {
                        // Always fallible (a network call can fail even when the
                        // local body can't) and always async (it awaits
                        // `fetch`) — see FnInfo's docs.
                        fn_info.insert(format!("{}_remote", f.name), FnInfo { fallible: true, is_async: true });
                    }
                }
                Item::Typ(t) => {
                    typ_names.insert(t.name.clone());
                }
                _ => {}
            }
        }
        Codegen { fn_info, typ_names, out: String::new(), unwrap_counter: 0 }
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
        match s {
            Stmt::Let { value, .. } | Stmt::Mut { value, .. } => self.expr_needs_async(value),
            Stmt::If { then_body, else_body, .. } => {
                self.stmts_need_async(then_body) || else_body.as_ref().map_or(false, |b| self.stmts_need_async(b))
            }
            Stmt::For { body, .. } | Stmt::Whl { body, .. } => self.stmts_need_async(body),
            _ => false,
        }
    }

    fn expr_needs_async(&self, expr: &Expr) -> bool {
        if let Expr::Unwrap { expr, .. } = expr {
            if let Expr::Call { callee, .. } = expr.as_ref() {
                if let Expr::Ident(name) = callee.as_ref() {
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
        self.line(0, "// Generated by the CuNi toy JavaScript backend. Do not hand-edit.");
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
        // shape this backend emits.
        self.line(1, "console.log(String(x));");
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "// Raised by `fail` — CuNi's explicit failure-signaling statement.");
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
        self.line(0, if main_needs_async { "async function main() {" } else { "function main() {" });
        for s in script_stmts {
            self.gen_stmt(1, s, &mut top_scope);
        }
        self.line(0, "}");
        self.out.push('\n');
        self.line(0, "main();");
    }

    fn gen_item(&mut self, item: &Item, scope: &mut HashMap<String, VarKind>) {
        match item {
            Item::Use(name) => {
                self.line(0, &format!("// use {} — portable CuNi module, not resolved by this toy backend", name));
            }
            Item::Ext(ext) => match ext.targets.iter().find(|(t, _)| t == "js") {
                Some((_, raw)) => {
                    let is_async = raw.contains("await");
                    let async_kw = if is_async { "async " } else { "" };
                    self.line(0, &format!("{}function {}({}) {{", async_kw, ext.name, params_sig(&ext.params)));
                    self.line(1, &format!("return {};", raw));
                    self.line(0, "}");
                }
                None => {
                    self.line(0, &format!("function {}({}) {{", ext.name, params_sig(&ext.params)));
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
                    let params = t.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ");
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
                    let params = m.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
                    self.line(0, &format!("//   {}({})", m.name, params));
                }
            }
            Item::Enum(e) => {
                let pairs = e.variants.iter().map(|v| format!("{}: {:?}", v, v)).collect::<Vec<_>>().join(", ");
                self.line(0, &format!("const {} = Object.freeze({{{}}});", e.name, pairs));
            }
            Item::Def(f) => {
                let generics_note = if f.generics.is_empty() {
                    String::new()
                } else {
                    format!("  // generic over {}", f.generics.join(", "))
                };
                let async_kw = if self.stmts_need_async(&f.body) { "async " } else { "" };
                self.line(0, &format!("{}function {}({}) {{{}", async_kw, f.name, params_sig(&f.params), generics_note));
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
        let args = f.params.iter().map(|p| js_wire_decode(&format!("parsed.{}", p.name), &p.ty)).collect::<Vec<_>>().join(", ");
        self.line(3, &format!("const result = {}({});", f.name, args));
        self.line(3, "res.writeHead(200, {\"Content-Type\": \"application/json\"});");
        self.line(3, &format!("res.end(JSON.stringify({{result: {}}}));", js_wire_encode("result", &f.ret_type)));
        self.line(2, "} catch (e) {");
        self.line(3, "res.writeHead(400, {\"Content-Type\": \"application/json\"});");
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
        let params_sig = f.params.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ");
        self.line(0, &format!("async function {}_remote(baseUrl, {}) {{", f.name, params_sig));
        let fields = f.params.iter().map(|p| format!("{}: {}", p.name, js_wire_encode(&p.name, &p.ty))).collect::<Vec<_>>().join(", ");
        self.line(1, &format!("const res = await fetch(baseUrl + \"/{}\", {{", f.name));
        self.line(2, "method: \"POST\",");
        self.line(2, "headers: {\"Content-Type\": \"application/json\"},");
        self.line(2, &format!("body: JSON.stringify({{{}}}),", fields));
        self.line(1, "});");
        self.line(1, "const data = await res.json();");
        self.line(1, "if (data.error) {");
        self.line(2, "throw new CuNiError(data.error);");
        self.line(1, "}");
        self.line(1, &format!("return {};", js_wire_decode("data.result", &f.ret_type)));
        self.line(0, "}");
    }

    fn gen_stmt(&mut self, indent: usize, stmt: &Stmt, scope: &mut HashMap<String, VarKind>) {
        match stmt {
            Stmt::Let { name, ty, value } => {
                let kind = ty.as_ref().map(kind_of_type).or_else(|| kind_of_literal(value)).unwrap_or(VarKind::Other);
                scope.insert(name.clone(), kind);
                self.gen_binding(indent, "const", name, value, scope);
            }
            Stmt::Mut { name, ty, value } => {
                let kind = ty.as_ref().map(kind_of_type).or_else(|| kind_of_literal(value)).unwrap_or(VarKind::Other);
                scope.insert(name.clone(), kind);
                self.gen_binding(indent, "let", name, value, scope);
            }
            Stmt::Assign { target, value } => {
                self.line(indent, &format!("{} = {};", self.gen_expr(target, scope), self.gen_expr(value, scope)));
            }
            Stmt::Ret(Some(e)) => {
                let text = self.gen_expr(e, scope);
                self.line(indent, &format!("return {};", text));
            }
            Stmt::Ret(None) => self.line(indent, "return;"),
            Stmt::Fail(e) => {
                let text = self.gen_expr(e, scope);
                self.line(indent, &format!("throw new CuNiError({});", text));
            }
            Stmt::If { cond, then_body, else_body } => {
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
            Stmt::For { binding: (a, b), iter, body } => {
                let iter_kind = if let Expr::Ident(name) = iter { scope.get(name).copied() } else { None };
                let header = match b {
                    Some(b) if iter_kind == Some(VarKind::Map) => {
                        format!("for (const [{}, {}] of {}) {{", a, b, self.gen_expr(iter, scope))
                    }
                    Some(b) => format!("for (const [{}, {}] of {}.entries()) {{", a, b, self.gen_expr(iter, scope)),
                    None => format!("for (const {} of {}) {{", a, self.gen_expr(iter, scope)),
                };
                self.line(indent, &header);
                self.gen_block(indent + 1, body, scope);
                self.line(indent, "}");
            }
            Stmt::Whl { cond, body } => {
                self.line(indent, &format!("while ({}) {{", self.gen_expr(cond, scope)));
                self.gen_block(indent + 1, body, scope);
                self.line(indent, "}");
            }
            Stmt::ExprStmt(e) => {
                let text = self.gen_expr(e, scope);
                self.line(indent, &format!("{};", text));
            }
            Stmt::Todo => {
                self.line(indent, "throw new Error(\"...\"); // CuNi stub body (`...`) — not yet written");
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
    fn gen_binding(&mut self, indent: usize, keyword: &str, name: &str, value: &Expr, scope: &mut HashMap<String, VarKind>) {
        if let Expr::Unwrap { expr, handler } = value {
            let is_fallible_call = matches!(
                expr.as_ref(),
                Expr::Call { callee, .. } if matches!(callee.as_ref(), Expr::Ident(fname) if self.fn_info.get(fname).map_or(false, |i| i.fallible))
            );
            // A `link`'s `<name>_remote` is always async (see FnInfo::is_async's
            // docs) — needs `await` here, and the enclosing function/`main()`
            // must itself be declared `async` (handled separately, at each
            // function's own declaration site, by `stmts_need_async`).
            let is_async_call = matches!(
                expr.as_ref(),
                Expr::Call { callee, .. } if matches!(callee.as_ref(), Expr::Ident(fname) if self.fn_info.get(fname).map_or(false, |i| i.is_async))
            );
            let inner_raw = self.gen_expr(expr, scope);
            let inner = if is_async_call { format!("await {}", inner_raw) } else { inner_raw };
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
            self.line(indent, &format!("{} {} = {};", keyword, name, self.gen_expr(value, scope)));
        }
    }

    fn gen_expr(&self, expr: &Expr, scope: &HashMap<String, VarKind>) -> String {
        match expr {
            Expr::Int(n) => n.to_string(),
            Expr::Float(f) => f.to_string(),
            Expr::Bool(b) => b.to_string(),
            Expr::Str(s) => format!("{:?}", s),
            Expr::InterpStr(parts) => {
                let mut s = String::from("`");
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => s.push_str(&escape_template_text(t)),
                        StrPartExpr::Expr(e) => {
                            s.push_str("${");
                            s.push_str(&self.gen_expr(e, scope));
                            s.push('}');
                        }
                    }
                }
                s.push('`');
                s
            }
            Expr::NoneLit => "null".to_string(),
            Expr::Ident(name) => name.clone(),
            Expr::List(items) => format!("[{}]", items.iter().map(|e| self.gen_expr(e, scope)).collect::<Vec<_>>().join(", ")),
            Expr::Map(pairs) => format!(
                "new Map([{}])",
                pairs
                    .iter()
                    .map(|(k, v)| format!("[{}, {}]", self.gen_expr(k, scope), self.gen_expr(v, scope)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Expr::Call { callee, args } => {
                // `.len()` is a method call in CuNi but a property in JS
                // (`.length`, no parens) — needs its own rewrite, unlike
                // `.push`, which already matches JS's own method shape.
                if let Expr::Field { base, name } = callee.as_ref() {
                    if name == "len" {
                        return format!("{}.length", self.gen_expr(base, scope));
                    }
                }
                let arg_list = args.iter().map(|e| self.gen_expr(e, scope)).collect::<Vec<_>>().join(", ");
                // JS class constructors require `new` — bare `Circle(1.5)` throws.
                if let Expr::Ident(tname) = callee.as_ref() {
                    if self.typ_names.contains(tname) {
                        return format!("new {}({})", tname, arg_list);
                    }
                }
                format!("{}({})", self.gen_expr(callee, scope), arg_list)
            }
            Expr::Index { base, index } => format!("{}[{}]", self.gen_expr(base, scope), self.gen_expr(index, scope)),
            Expr::Field { base, name } => format!("{}.{}", self.gen_expr(base, scope), name),
            Expr::Binary { op, lhs, rhs } => format!("({} {} {})", self.gen_expr(lhs, scope), js_binop(*op), self.gen_expr(rhs, scope)),
            Expr::Unary { op, expr } => match op {
                UnOp::Not => format!("(!{})", self.gen_expr(expr, scope)),
                UnOp::Neg => format!("(-{})", self.gen_expr(expr, scope)),
            },
            Expr::Unwrap { .. } => "null /* UNSUPPORTED: ?? outside a let/mut binding, see codegen_js.rs docs */".to_string(),
        }
    }
}

fn params_sig(params: &[Param]) -> String {
    params.iter().map(|p| p.name.clone()).collect::<Vec<_>>().join(", ")
}

fn kind_of_type(ty: &Type) -> VarKind {
    match ty {
        Type::Generic(name, _) if name == "list" => VarKind::List,
        Type::Generic(name, _) if name == "map" => VarKind::Map,
        _ => VarKind::Other,
    }
}

fn kind_of_literal(e: &Expr) -> Option<VarKind> {
    match e {
        Expr::List(_) => Some(VarKind::List),
        Expr::Map(_) => Some(VarKind::Map),
        _ => None,
    }
}

fn escape_template_text(t: &str) -> String {
    t.replace('\\', "\\\\").replace('`', "\\`").replace('$', "\\$")
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

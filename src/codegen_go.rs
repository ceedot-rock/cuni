use crate::ast::*;
use std::collections::HashMap;

/// A toy, best-effort Go emitter for the CuNi AST. Like codegen_py.rs, it exists
/// to prove the pipeline (parse -> emit) works end to end for the constructs the
/// spec has fully defined; it is NOT the real "compile-or-refuse" compiler
/// described in SPEC.md — it never rejects a program, it just does its best and
/// leaves a marker where the mapping is genuinely undefined (see NOTES below).
///
/// Go is the odd one out among the three v1 targets: it's the only statically
/// typed one, so several constructs that were free-form in Python needed a real
/// judgment call here. Recorded below so the choices are visible, not buried:
///
/// - `opt<T>` -> Go pointer `*T`, `none` -> `nil`. The alternative was a generic
///   wrapper struct (`Opt[T]{Value T; Ok bool}`, Go 1.18+ generics). Pointer was
///   chosen because it's the idiomatic, zero-ceremony Go shape for "may be
///   absent" (every Go standard-library-adjacent API that models optionality
///   uses `*T`), and because it round-trips through function returns without an
///   extra unwrap step at every call site. The cost: a pointer can never be
///   distinguished from "a mutable heap-boxed value", so if CuNi ever needs
///   *both* optionality and pointer/reference semantics for the same T, this
///   representation collapses them. Not a real problem for anything the spec
///   currently expresses, but the coupling is worth naming.
/// - Fallible (`?`) functions map 1:1 onto Go's own `(T, error)` convention —
///   this is the cleanest fit of any construct in the whole exercise. `fail
///   expr` inside a fallible function becomes `return zeroValue,
///   fmt.Errorf("%v", expr)`, since Go's error channel already *is* a real
///   "this failed" signal. A stub (`...`) body of a fallible function becomes
///   `return zeroValue, fmt.Errorf("not implemented")` (distinct from `fail`:
///   "not written yet" isn't "this call failed"). Either construct used in a
///   *non-fallible* function has no error channel available, so it emits
///   `panic(...)` instead — a real runtime crash, not a graceful error,
///   because Go's static return type leaves no other honest option (a real
///   type checker would reject `fail` outside a fallible function at compile
///   time; this toy backend has none).
/// - `??` (Unwrap) is only handled when it is the direct value of a `let`/`mut`
///   statement, exactly like codegen_py.rs. Two sub-shapes:
///     - unwrapping a call to a function known to be fallible ->
///       `name, err := call(...); if err != nil { handler }` — this is just
///       idiomatic Go error handling, no invention needed.
///     - unwrapping anything else (assumed `opt<T>`, i.e. `*T`) ->
///       `tmp := expr; if tmp == nil { handler }; name := *tmp`.
///   In any other expression position `??` emits an UNSUPPORTED marker. Because
///   Go is statically typed, that marker (`nil` used in an arbitrary
///   expression position) will typically fail *at compile time* rather than
///   silently misbehave at runtime the way Python's `None # UNSUPPORTED` does.
///   That's a genuinely interesting asymmetry: Go's strictness turns an
///   unhandled construct into a hard refusal, which is actually closer to the
///   spec's "compile-or-refuse" ideal than either of the other two targets
///   manages by accident — even though this backend still isn't doing real
///   refusal logic on purpose.
/// - `list<T>` -> Go slice `[]T`; `map<K,V>` -> Go's native `map[K]V`. This part
///   really is close to a gift — almost no impedance mismatch, except that Go
///   slice/map *literals* require an explicit element type (`[]int{...}`, not
///   a bare `{...}`), which Python and JS don't need. That type is taken from
///   the `let`/`mut` annotation when present (`mut names: list<str> = []`) and
///   otherwise guessed from the first literal element — a lightweight
///   heuristic, not real type inference, in the same spirit as codegen_py.rs's
///   disclosed map-vs-list loop guessing (see below).
/// - `.push(x)` on a list is a *statement-level rewrite*, not an expression
///   substitution: Go's `append` returns a new slice rather than mutating in
///   place, so `names.push(x)` cannot become `names.append(x)` (there is no
///   such method) or even `append(names, x)` as a bare expression statement
///   (its result would be silently discarded, which is wrong — the original
///   backing array may or may not have been reused). It must become the
///   assignment `names = append(names, x)`. This is handled in `gen_stmt` for
///   `StmtKind::ExprStmt`, matching on the `.push` call shape before falling back
///   to generic expression codegen — genuinely structurally different from
///   both the Python (`.append` method call) and JS (`.push` method call)
///   mappings, and worth calling out as a real cross-language asymmetry: the
///   *same* CuNi statement is an expression-statement in two targets and an
///   assignment in the third.
/// - Backtick/`${}` interpolation -> `fmt.Sprintf("...%v...", args...)`. Plain
///   text segments have `%` doubled to `%%` so they survive as literal percent
///   signs through Sprintf's own format-verb parsing.
/// - `iface` -> a genuine Go `interface { ... }`. But Go's interface
///   satisfaction is structural and method-based: a type only satisfies an
///   interface by having *methods* with matching receivers, never free
///   functions. CuNi's own spec example (`typ Circle is Shape` /
///   `def area(c: Circle) -> float`) implements the interface's `area` as a
///   free function taking the receiver explicitly, not as a Go method on
///   `Circle`. That means the emitted `Circle` does **not** actually satisfy
///   the emitted `Shape` interface in Go's own type system — the same spec gap
///   codegen_py.rs discloses for Python's ABC mechanism, but sharper here,
///   because Go would *refuse to compile* any code that tried to actually use
///   a `Circle` where a `Shape` interface value is required (Python's ABC
///   check is comparatively toothless — it doesn't require inheriting concrete
///   methods to be an instance of the class already, whereas Go's structural
///   check is unforgiving). This backend does not attempt to reconcile the
///   mismatch (e.g. by synthesizing a method wrapper) — it just emits the
///   struct and the interface as declared and leaves `is` conformance
///   unenforced and unasserted, since nothing in the spec ties the two
///   together yet. A comment is emitted next to any `typ ... is ...` noting
///   this.
/// - `enum` (payload-free) -> `type Name int` + `const (Variant Name = iota;
///   ...)`, the idiomatic Go pattern. The one real wrinkle: Go constants
///   declared this way are *package-level*, not namespaced under `Name` the
///   way Python's `Enum` class or JS's frozen object are — so `Color.Red` in
///   CuNi source must emit as the bare identifier `Red` (see `Field` codegen
///   below), and two different CuNi enums sharing a variant name (e.g. two
///   `Red`s) would collide as a genuine Go compile error, unlike Python/JS
///   where each enum's variants live in their own namespace. Not solved here;
///   flagged since it's a real cross-target asymmetry a real compiler would
///   need to rename around (e.g. `ColorRed`) or refuse to allow.
/// - `typ` -> Go `struct`. Field names are kept exactly as written (lowercase,
///   matching CuNi source) rather than Go-conventionally capitalized/exported
///   — everything lives in one `package main` file here, so export visibility
///   is moot, and keeping names literal keeps the source-to-output mapping
///   easy to eyeball.
/// - Top-level statements: CuNi allows a top-level `ret` (e.g. inside a `??`
///   handler on a top-level `let`), which has no defined target-language
///   meaning of its own. Go actually *requires* a `func main()` for any
///   executable, so wrapping top-level statements in `func main()` is Go's
///   natural shape, not a workaround the way an implicit `def main():` was for
///   Python. The one remaining wrinkle: Go's `main()` returns nothing, so a
///   top-level `ret <value>` (like `ret -1` in full.cuni) has no return slot
///   to put the value in. This backend evaluates the expression for its side
///   effects (`_ = expr`) and then emits a bare `return`, discarding the
///   value — which matches Python's behavior too, since nothing there ever
///   inspects `main()`'s return value either. Still an open spec question,
///   just a *slightly* different flavor of it in Go (a value with nowhere to
///   go, rather than a keyword with nothing to enforce it).
/// - `and`/`or`/`not` -> `&&`/`||`/`!`.
/// - Generics (`def find<T>(...)`) -> real Go 1.18+ type parameters
///   (`func find[T comparable](...)`), unlike Python, where generics could
///   only ever be a `# generic over T` comment (Python has no compile-time
///   generic parameter binding to target). This is a case where Go is *more*
///   expressive than Python for a CuNi construct, not less — but that
///   expressiveness demands a constraint CuNi's own grammar doesn't have a
///   syntax for. `comparable` is used as the default (rather than the weaker
///   `any`) specifically because the spec's own example generic, `find<T>`,
///   compares elements with `==`, which Go rejects for an `any`-constrained
///   type parameter. A CuNi generic that never compares its parameter would
///   be needlessly restricted to comparable types under this default, but
///   nothing in the spec's core distinguishes the two cases yet.
/// - `let` vs `mut`: Go has no notion of a non-constant, runtime-computed,
///   *immutable* local binding (`const` only accepts compile-time constant
///   expressions, and `let`-bound values here are frequently function calls).
///   So both `let` and `mut` compile to a plain `:=`. CuNi's mutability
///   contract for `let` is unenforced Go-side — the same gap codegen_py.rs
///   already discloses for Python (plain `=` there too), just reconfirmed here
///   since Go could in principle have done better and doesn't.
/// - `ext` target lines are raw, un-lexed Go source text, exactly as
///   Python/JS treat their own target lines. This backend does not add
///   `import`s or stub out helper functions/packages referenced inside an
///   `ext` block (e.g. `go: httpGet(url)` assumes `httpGet` already exists
///   somewhere in the target build) — the same limitation Python's backend has
///   for e.g. `py: requests.get(url).text` never importing `requests`. `ext`
///   is documented in the spec as an explicit escape hatch to target-native
///   code; wiring up that code's own dependencies is the CuNi author's
///   problem, not this toy backend's.
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
}

/// Tracks the enclosing Go function's declared return type/fallibility so
/// `ret` and stub (`...`) bodies can be emitted correctly. `None` means we are
/// generating top-level CuNi statements, which live inside `func main()` — a
/// real Go function, but one with no return type at all.
struct CurFn {
    ret_type: Type,
    fallible: bool,
}

struct Codegen {
    fn_info: HashMap<String, FnInfo>,
    /// Names of CuNi `enum` types, so `Field` codegen can tell an enum-variant
    /// access (`Color.Red`) from an ordinary field/method access — Go's `iota`
    /// constants aren't namespaced under their type the way Python's `Enum`
    /// class or JS's frozen object are, so `Color.Red` must emit as bare `Red`.
    enum_names: std::collections::HashSet<String>,
    /// `typ` name -> field names in declaration order. Used to emit
    /// composite literals `T{f0: a0, f1: a1}` for positional constructors
    /// written as `T(a0, a1)` in CuNi.
    typ_fields: HashMap<String, Vec<String>>,
    cur_fn: Option<CurFn>,
    tmp_counter: usize,
    has_link: bool,
    has_link_int_param: bool,
    out: String,
}

impl Codegen {
    fn new(program: &Program) -> Self {
        let mut fn_info = HashMap::new();
        let mut enum_names = std::collections::HashSet::new();
        let mut typ_fields = HashMap::new();
        let mut has_link = false;
        let mut has_link_int_param = false;
        for item in &program.items {
            match item {
                Item::Def(f) => {
                    fn_info.insert(f.name.clone(), FnInfo { fallible: f.fallible });
                    if f.is_link {
                        has_link = true;
                        // The remote client stub is always fallible (a
                        // network call can always fail even if the local
                        // body can't) — registering it here lets the
                        // existing `??`/Unwrap codegen path handle it for
                        // free, exactly like any other fallible function.
                        fn_info.insert(format!("{}_remote", f.name), FnInfo { fallible: true });
                        if f.params.iter().any(|p| matches!(&p.ty, Type::Named(n) if n == "int")) || matches!(&f.ret_type, Type::Named(n) if n == "int") {
                            has_link_int_param = true;
                        }
                    }
                }
                Item::Enum(e) => {
                    enum_names.insert(e.name.clone());
                }
                Item::Typ(t) => {
                    typ_fields.insert(t.name.clone(), t.fields.iter().map(|f| f.name.clone()).collect());
                }
                _ => {}
            }
        }
        Codegen { fn_info, enum_names, typ_fields, cur_fn: None, tmp_counter: 0, has_link, has_link_int_param, out: String::new() }
    }

    fn line(&mut self, indent: usize, text: &str) {
        self.out.push_str(&"\t".repeat(indent));
        self.out.push_str(text);
        self.out.push('\n');
    }

    fn fresh_tmp(&mut self) -> String {
        self.tmp_counter += 1;
        format!("__cuni_tmp{}", self.tmp_counter)
    }

    fn gen_program(&mut self, program: &Program) {
        self.line(0, "// Generated by the CuNi toy Go backend. Do not hand-edit.");
        self.line(0, "package main");
        self.out.push('\n');
        if self.has_link {
            self.line(0, "import (");
            self.line(1, "\"bytes\"");
            self.line(1, "\"encoding/json\"");
            self.line(1, "\"fmt\"");
            self.line(1, "\"net/http\"");
            if self.has_link_int_param {
                self.line(1, "\"strconv\"");
            }
            self.line(0, ")");
        } else {
            self.line(0, "import \"fmt\"");
        }
        self.out.push('\n');
        self.line(0, "func say(x any) {");
        self.line(1, "fmt.Println(x)");
        self.line(0, "}");
        self.out.push('\n');

        // Top-level `ret` (e.g. inside a `??` handler on a top-level `let`) has
        // no enclosing function in CuNi's source. Go, unlike Python, actually
        // *requires* a func main() for any executable — so this wrapping is
        // Go's natural shape, not a workaround. See module docs for the one
        // wrinkle: main() can't return a value, so a top-level `ret <expr>`
        // discards the value (evaluated for side effects only).
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

        self.line(0, "func main() {");
        for s in script_stmts {
            self.gen_stmt(1, s, &mut top_scope);
        }
        self.line(0, "}");
    }

    fn gen_item(&mut self, item: &Item, scope: &mut HashMap<String, VarKind>) {
        match item {
            Item::Use(name) => {
                self.line(0, &format!("// use {} — portable CuNi module, not resolved by this toy backend", name));
            }
            Item::Ext(ext) => {
                self.line(0, &format!("func {}({}) {} {{", ext.name, params_sig(&ext.params), go_type(&ext.ret_type)));
                let go_line = ext.targets.iter().find(|(t, _)| t == "go").map(|(_, raw)| raw.clone());
                match go_line {
                    Some(raw) => self.line(1, &format!("return {}", raw)),
                    None => self.line(1, "panic(\"no go: mapping given\")"),
                }
                self.line(0, "}");
            }
            Item::Typ(t) => {
                self.line(0, &format!("type {} struct {{", t.name));
                for f in &t.fields {
                    self.line(1, &format!("{} {}", f.name, go_type(&f.ty)));
                }
                self.line(0, "}");
                if let Some(base) = &t.implements {
                    self.line(
                        0,
                        &format!(
                            "// note: `{} is {}` is not enforced above — Go interfaces are structural, and {}'s method(s)",
                            t.name, base, base
                        ),
                    );
                    self.line(
                        0,
                        "// are implemented as free functions per CuNi's own example, not as Go methods with a receiver,",
                    );
                    self.line(0, &format!("// so {} does not actually satisfy {} in Go's type system. See codegen_go.rs docs.", t.name, base));
                }
            }
            Item::Iface(i) => {
                self.line(0, &format!("type {} interface {{", i.name));
                for m in &i.methods {
                    self.line(1, &format!("{}({}) {}", m.name, params_sig(&m.params), go_type(&m.ret_type)));
                }
                self.line(0, "}");
            }
            Item::Enum(e) => {
                self.line(0, &format!("type {} int", e.name));
                if !e.variants.is_empty() {
                    self.line(
                        0,
                        "// note: variants are bare, package-level constants — a second enum sharing a variant name would collide, see codegen_go.rs docs",
                    );
                    self.line(0, "const (");
                    for (i, v) in e.variants.iter().enumerate() {
                        if i == 0 {
                            self.line(1, &format!("{} {} = iota", v, e.name));
                        } else {
                            self.line(1, v);
                        }
                    }
                    self.line(0, ")");
                }
            }
            Item::Def(f) => {
                // CuNi's generics have no constraint syntax of their own, so
                // this backend must pick a default Go type-parameter
                // constraint. `comparable` (not the weaker `any`) is used
                // because the one generic function the spec actually defines
                // (`find<T>`) compares elements with `==`, which Go's type
                // checker rejects under `any`. The cost: a CuNi generic
                // function that never compares its type parameter would be
                // needlessly restricted to comparable types in Go, but
                // nothing in the spec's core distinguishes that case yet.
                let generics = if f.generics.is_empty() {
                    String::new()
                } else {
                    format!("[{}]", f.generics.iter().map(|g| format!("{} comparable", g)).collect::<Vec<_>>().join(", "))
                };
                let ret_sig = if f.fallible {
                    format!("({}, error)", go_type(&f.ret_type))
                } else {
                    go_type(&f.ret_type)
                };
                self.line(0, &format!("func {}{}({}) {} {{", f.name, generics, params_sig(&f.params), ret_sig));
                let mut fn_scope: HashMap<String, VarKind> = HashMap::new();
                for p in &f.params {
                    fn_scope.insert(p.name.clone(), kind_of_type(&p.ty));
                }
                let prev_cur_fn = self.cur_fn.take();
                self.cur_fn = Some(CurFn { ret_type: f.ret_type.clone(), fallible: f.fallible });
                if f.body.is_empty() {
                    self.line(1, "panic(\"unreachable: empty function body\")");
                } else {
                    for s in &f.body {
                        self.gen_stmt(1, s, &mut fn_scope);
                    }
                }
                self.cur_fn = prev_cur_fn;
                self.line(0, "}");
                if f.is_link {
                    self.out.push('\n');
                    self.gen_link_types_and_handler(f);
                    self.out.push('\n');
                    self.gen_link_remote(f);
                }
            }
            Item::Stmt(s) => self.gen_stmt(0, s, scope),
        }
    }

    fn gen_stmt(&mut self, indent: usize, stmt: &Stmt, scope: &mut HashMap<String, VarKind>) {
        match &stmt.kind {
            StmtKind::Let { name, ty, value } | StmtKind::Mut { name, ty, value } => {
                let kind = ty.as_ref().map(kind_of_type).or_else(|| kind_of_literal(value)).unwrap_or(VarKind::Other);
                scope.insert(name.clone(), kind);
                self.gen_binding(indent, name, ty, value, scope);
            }
            StmtKind::Assign { target, value } => {
                self.line(indent, &format!("{} = {}", self.gen_expr(target, scope), self.gen_expr(value, scope)));
            }
            StmtKind::Ret(value) => self.gen_ret(indent, value, scope),
            StmtKind::Fail(e) => {
                let text = self.gen_expr(e, scope);
                match &self.cur_fn {
                    Some(cur) if cur.fallible => {
                        let zero = zero_value(&cur.ret_type);
                        self.line(indent, &format!("return {}, fmt.Errorf(\"%v\", {})", zero, text));
                    }
                    _ => {
                        // `fail` outside a fallible function has no Go return
                        // channel to use — a real type checker would reject
                        // this at compile time (see SPEC.md open items); this
                        // toy backend has none, so it panics instead.
                        self.line(indent, &format!("panic(fmt.Sprintf(\"%v\", {})) // `fail` used outside a fallible function", text));
                    }
                }
            }
            StmtKind::If { cond, then_body, else_body } => {
                self.line(indent, &format!("if {} {{", self.gen_expr(cond, scope)));
                self.gen_block(indent + 1, then_body, scope);
                match else_body {
                    Some(eb) => {
                        self.line(indent, "} else {");
                        self.gen_block(indent + 1, eb, scope);
                        self.line(indent, "}");
                    }
                    None => self.line(indent, "}"),
                }
            }
            StmtKind::For { binding: (a, b), iter, body } => {
                let iter_kind = if let ExprKind::Ident(name) = &iter.kind { scope.get(name).copied() } else { None };
                // Two-binding form (`for i, x in xs`) is a genuine gift: Go's
                // `for a, b := range x` gives index+value for slices and
                // key+value for maps with the *same* syntax, no branching
                // needed (unlike Python, which needed `.items()` for maps).
                // The one-binding form isn't as lucky: plain `for x := range
                // xs` gives the *index* for a slice (not the value CuNi
                // intends), so a list needs the index thrown away with `_`.
                // For a map, `for x := range m` already yields the key, which
                // matches CuNi's intended single-binding-over-a-map meaning.
                // Deciding list-vs-map here uses the same lightweight
                // scope-tracked guess codegen_py.rs discloses for its own
                // map-vs-list loop handling — not real type inference.
                let header = match b {
                    Some(b) => format!("for {}, {} := range {} {{", a, b, self.gen_expr(iter, scope)),
                    None => match iter_kind {
                        Some(VarKind::Map) => format!("for {} := range {} {{", a, self.gen_expr(iter, scope)),
                        _ => format!("for _, {} := range {} {{", a, self.gen_expr(iter, scope)),
                    },
                };
                self.line(indent, &header);
                self.gen_block(indent + 1, body, scope);
                self.line(indent, "}");
            }
            StmtKind::Whl { cond, body } => {
                // Go has no `while`; a `for` with just a condition is the idiom.
                self.line(indent, &format!("for {} {{", self.gen_expr(cond, scope)));
                self.gen_block(indent + 1, body, scope);
                self.line(indent, "}");
            }
            StmtKind::ExprStmt(e) => {
                // `.push` is a Stmt-level rewrite, not an expression
                // substitution: Go's `append` returns a *new* slice rather
                // than mutating in place, so `names.push(x)` must become the
                // assignment `names = append(names, x)`, never a bare
                // expression statement (whose result would be silently
                // dropped). See module docs.
                if let ExprKind::Call { callee, args } = &e.kind {
                    if let ExprKind::Field { base, name } = &callee.kind {
                        if name == "push" {
                            let base_text = self.gen_expr(base, scope);
                            let args_text = args.iter().map(|a| self.gen_expr(a, scope)).collect::<Vec<_>>().join(", ");
                            self.line(indent, &format!("{} = append({}, {})", base_text, base_text, args_text));
                            return;
                        }
                    }
                }
                let text = self.gen_expr(e, scope);
                self.line(indent, &text);
            }
            StmtKind::Todo => match &self.cur_fn {
                Some(cur) if cur.fallible => {
                    let zero = zero_value(&cur.ret_type);
                    self.line(
                        indent,
                        &format!(
                            "return {}, fmt.Errorf(\"...\") // CuNi stub body (`...`) — not yet written",
                            zero
                        ),
                    );
                }
                _ => {
                    self.line(indent, "panic(\"...\") // CuNi stub body (`...`) — not yet written");
                }
            },
        }
    }

    fn gen_block(&mut self, indent: usize, stmts: &[Stmt], scope: &mut HashMap<String, VarKind>) {
        for s in stmts {
            self.gen_stmt(indent, s, scope);
        }
    }

    /// `link Name(...) -> T [?] do ... end` (SPEC.md §19) additionally emits
    /// a `NameRequest`/`NameResponse` JSON wire pair and a handler function
    /// — mount it yourself, per the ratified "codegen-only, no bundled
    /// runtime" decision (INTEROP_PROPOSAL.md item 6). Handler/remote
    /// function names are deliberately kept exactly as CuNi source would
    /// call them (`Name_handler`/`Name_remote`, not Go-capitalized), so a
    /// CuNi program can call `Name_remote(...)` identically regardless of
    /// target. Struct *field* names are the one place this backend is
    /// forced to capitalize despite that stylistic preference — unlike
    /// `typ`'s struct codegen (which keeps CuNi's own lowercase field names,
    /// since export visibility is otherwise moot in a single `package main`
    /// file, see that `Item::Typ` arm's docs), `encoding/json` cannot see an
    /// unexported field via reflection at all, so this is a hard Go
    /// requirement, not a style choice.
    fn gen_link_types_and_handler(&mut self, f: &FnDecl) {
        let type_name = go_capitalize(&f.name);
        self.line(0, &format!("type {}Request struct {{", type_name));
        for p in &f.params {
            self.line(1, &format!("{} {} `json:\"{}\"`", go_capitalize(&p.name), go_wire_field_type(&p.ty), p.name));
        }
        self.line(0, "}");
        self.line(0, &format!("type {}Response struct {{", type_name));
        self.line(1, &format!("Result {} `json:\"result,omitempty\"`", go_wire_field_type(&f.ret_type)));
        self.line(1, "Error string `json:\"error,omitempty\"`");
        self.line(0, "}");
        self.out.push('\n');

        self.line(0, &format!("// mount at path \"/{}\" so {}_remote's generated client can find it", f.name, f.name));
        self.line(0, &format!("func {}_handler(w http.ResponseWriter, r *http.Request) {{", f.name));
        self.line(1, &format!("var req {}Request", type_name));
        self.line(1, "if err := json.NewDecoder(r.Body).Decode(&req); err != nil {");
        self.line(2, "w.WriteHeader(400)");
        self.line(2, &format!("json.NewEncoder(w).Encode({}Response{{Error: err.Error()}})", type_name));
        self.line(2, "return");
        self.line(1, "}");
        let mut args = Vec::new();
        for p in &f.params {
            let field = format!("req.{}", go_capitalize(&p.name));
            if matches!(&p.ty, Type::Named(n) if n == "int") {
                self.line(1, &format!("{}, err := strconv.ParseInt({}, 10, 64)", p.name, field));
                self.line(1, "if err != nil {");
                self.line(2, "w.WriteHeader(400)");
                self.line(2, &format!("json.NewEncoder(w).Encode({}Response{{Error: err.Error()}})", type_name));
                self.line(2, "return");
                self.line(1, "}");
                args.push(format!("int({})", p.name));
            } else {
                args.push(field);
            }
        }
        if f.fallible {
            self.line(1, &format!("result, err := {}({})", f.name, args.join(", ")));
            self.line(1, "if err != nil {");
            self.line(2, "w.WriteHeader(400)");
            self.line(2, &format!("json.NewEncoder(w).Encode({}Response{{Error: err.Error()}})", type_name));
            self.line(2, "return");
            self.line(1, "}");
        } else {
            self.line(1, &format!("result := {}({})", f.name, args.join(", ")));
        }
        self.line(1, &format!("json.NewEncoder(w).Encode({}Response{{Result: {}}})", type_name, go_wire_encode("result", &f.ret_type)));
        self.line(0, "}");
    }

    /// The client side of the same `link`: always `(T, error)` regardless of
    /// whether the local function is fallible — a network call can fail even
    /// when the local logic can't, so the remote stub doesn't get to opt out
    /// of Go's error-return convention the way a non-fallible local `def`
    /// would.
    fn gen_link_remote(&mut self, f: &FnDecl) {
        let type_name = go_capitalize(&f.name);
        let params_sig = f.params.iter().map(|p| format!("{} {}", p.name, go_type(&p.ty))).collect::<Vec<_>>().join(", ");
        self.line(0, &format!("func {}_remote(baseUrl string, {}) ({}, error) {{", f.name, params_sig, go_type(&f.ret_type)));
        let fields = f
            .params
            .iter()
            .map(|p| format!("{}: {}", go_capitalize(&p.name), go_wire_encode(&p.name, &p.ty)))
            .collect::<Vec<_>>()
            .join(", ");
        self.line(1, &format!("reqBody, err := json.Marshal({}Request{{{}}})", type_name, fields));
        self.line(1, "if err != nil {");
        self.line(2, &format!("return {}, err", zero_value(&f.ret_type)));
        self.line(1, "}");
        self.line(1, &format!("resp, err := http.Post(baseUrl+\"/{}\", \"application/json\", bytes.NewReader(reqBody))", f.name));
        self.line(1, "if err != nil {");
        self.line(2, &format!("return {}, err", zero_value(&f.ret_type)));
        self.line(1, "}");
        self.line(1, "defer resp.Body.Close()");
        self.line(1, &format!("var out {}Response", type_name));
        self.line(1, "if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {");
        self.line(2, &format!("return {}, err", zero_value(&f.ret_type)));
        self.line(1, "}");
        self.line(1, "if out.Error != \"\" {");
        self.line(2, &format!("return {}, fmt.Errorf(\"%s\", out.Error)", zero_value(&f.ret_type)));
        self.line(1, "}");
        self.line(1, &format!("return {}, nil", go_wire_decode("out.Result", &f.ret_type)));
        self.line(0, "}");
    }

    /// Emits a `ret` statement, whose shape depends entirely on the enclosing
    /// context: top-level (inside `func main()`, no return type at all),
    /// a plain-value function, a fallible function (`(T, error)`), or a
    /// function returning `opt<T>` (`*T`, needing an addressable temp to take
    /// the address of).
    fn gen_ret(&mut self, indent: usize, value: &Option<Expr>, scope: &mut HashMap<String, VarKind>) {
        let cur = match &self.cur_fn {
            None => {
                // Top-level `ret` lives inside func main(), which returns
                // nothing. There is no Go-side slot for a value here; we
                // evaluate it for side effects and discard it, matching the
                // fact that nothing ever inspects Python's implicit main()'s
                // return value either. See module docs.
                match value {
                    Some(e) => {
                        let text = self.gen_expr(e, scope);
                        self.line(indent, &format!("_ = {} // top-level `ret` value has no Go target inside func main(); discarded, see codegen_go.rs docs", text));
                    }
                    None => {}
                }
                self.line(indent, "return");
                return;
            }
            Some(cur) => cur,
        };
        let fallible = cur.fallible;
        let ret_type = cur.ret_type.clone();
        let is_opt = matches!(&ret_type, Type::Generic(n, _) if n == "opt");
        match value {
            Some(e) if is_opt && !matches!(&e.kind, ExprKind::NoneLit) => {
                // opt<T> is `*T`; taking the address of an arbitrary
                // expression isn't legal Go, so the value is first bound to
                // an addressable temp.
                let inner = self.gen_expr(e, scope);
                let tmp = self.fresh_tmp();
                self.line(indent, &format!("{} := {}", tmp, inner));
                if fallible {
                    self.line(indent, &format!("return &{}, nil", tmp));
                } else {
                    self.line(indent, &format!("return &{}", tmp));
                }
            }
            Some(e) => {
                // Either a plain value, or `ret none` (NoneLit already emits
                // the Go literal `nil`, which is exactly the right shape for
                // an opt<T> == *T return, so no temp is needed).
                let text = self.gen_expr(e, scope);
                if fallible {
                    self.line(indent, &format!("return {}, nil", text));
                } else {
                    self.line(indent, &format!("return {}", text));
                }
            }
            None => {
                // Bare `ret` inside a value-returning Go function has no
                // direct source construct (unlike Python, where a bare
                // `return` is always legal regardless of declared type). The
                // zero value of the declared return type is filled in to
                // keep the emitted Go valid.
                let zero = zero_value(&ret_type);
                if fallible {
                    self.line(indent, &format!("return {}, nil // bare `ret` in a value-returning fn — zero value filled in, see codegen_go.rs docs", zero));
                } else {
                    self.line(indent, &format!("return {} // bare `ret` in a value-returning fn — zero value filled in, see codegen_go.rs docs", zero));
                }
            }
        }
    }

    /// `let name = expr ?? do handler end` is the only Unwrap position this
    /// toy backend supports (see module docs), exactly mirroring
    /// codegen_py.rs's restriction. Two distinct Go shapes come out of it:
    /// unwrapping a call known to be fallible uses Go's native `(v, err)`
    /// idiom directly; unwrapping anything else is treated as `opt<T>`
    /// (`*T`), nil-checked and dereferenced.
    fn gen_binding(&mut self, indent: usize, name: &str, ty: &Option<Type>, value: &Expr, scope: &mut HashMap<String, VarKind>) {
        if let ExprKind::Unwrap { expr, handler } = &value.kind {
            let is_fallible_call = matches!(&expr.kind, ExprKind::Call { callee, .. } if matches!(&callee.kind, ExprKind::Ident(fname) if self.fn_info.get(fname).map_or(false, |i| i.fallible))
            );
            let inner = self.gen_expr(expr, scope);
            if is_fallible_call {
                self.line(indent, &format!("{}, err := {}", name, inner));
                self.line(indent, "if err != nil {");
                self.gen_block(indent + 1, handler, scope);
                self.line(indent, "}");
            } else {
                let tmp = self.fresh_tmp();
                self.line(indent, &format!("{} := {}", tmp, inner));
                self.line(indent, &format!("if {} == nil {{", tmp));
                self.gen_block(indent + 1, handler, scope);
                self.line(indent, "}");
                self.line(indent, &format!("{} := *{}", name, tmp));
            }
        } else {
            let text = self.gen_expr_hinted(value, ty.as_ref(), scope);
            self.line(indent, &format!("{} := {}", name, text));
        }
    }

    /// Like `gen_expr`, but for `list`/`map` literals lets the caller supply
    /// the expected `Type` (a `let`/`mut` annotation) so the Go composite
    /// literal's required element type (`[]int{...}`, never a bare `{...}`)
    /// can be taken from the annotation instead of guessed.
    fn gen_expr_hinted(&self, expr: &Expr, hint: Option<&Type>, scope: &HashMap<String, VarKind>) -> String {
        match &expr.kind {
            ExprKind::List(items) => {
                let elem = hint
                    .and_then(|t| match t {
                        Type::Generic(n, args) if n == "list" => Some(go_type(&args[0])),
                        _ => None,
                    })
                    .or_else(|| infer_list_elem_type(items))
                    .unwrap_or_else(|| "any".to_string());
                format!("[]{}{{{}}}", elem, items.iter().map(|e| self.gen_expr(e, scope)).collect::<Vec<_>>().join(", "))
            }
            ExprKind::Map(pairs) => {
                let (k, v) = hint
                    .and_then(|t| match t {
                        Type::Generic(n, args) if n == "map" => Some((go_type(&args[0]), go_type(&args[1]))),
                        _ => None,
                    })
                    .or_else(|| infer_map_kv_type(pairs))
                    .unwrap_or_else(|| ("any".to_string(), "any".to_string()));
                format!(
                    "map[{}]{}{{{}}}",
                    k,
                    v,
                    pairs.iter().map(|(kk, vv)| format!("{}: {}", self.gen_expr(kk, scope), self.gen_expr(vv, scope))).collect::<Vec<_>>().join(", ")
                )
            }
            _ => self.gen_expr(expr, scope),
        }
    }

    fn gen_expr(&self, expr: &Expr, scope: &HashMap<String, VarKind>) -> String {
        match &expr.kind {
            ExprKind::Int(n) => n.to_string(),
            ExprKind::Float(f) => f.to_string(),
            ExprKind::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
            ExprKind::Str(s) => format!("{:?}", s),
            ExprKind::InterpStr(parts) => {
                let mut fmt_str = String::new();
                let mut args = Vec::new();
                for p in parts {
                    match p {
                        StrPartExpr::Text(t) => fmt_str.push_str(&t.replace('%', "%%")),
                        StrPartExpr::Expr(e) => {
                            fmt_str.push_str("%v");
                            args.push(self.gen_expr(e, scope));
                        }
                    }
                }
                let quoted = format!("{:?}", fmt_str);
                if args.is_empty() {
                    quoted
                } else {
                    format!("fmt.Sprintf({}, {})", quoted, args.join(", "))
                }
            }
            ExprKind::NoneLit => "nil".to_string(),
            ExprKind::Ident(name) => name.clone(),
            ExprKind::List(_) | ExprKind::Map(_) => self.gen_expr_hinted(expr, None, scope),
            ExprKind::Call { callee, args } => {
                if let ExprKind::Field { base, name } = &callee.kind {
                    if name == "len" {
                        return format!("len({})", self.gen_expr(base, scope));
                    }
                    if name == "push" {
                        // `.push` only has a valid Go shape as a statement-
                        // level rewrite (`x = append(x, ...)`, see gen_stmt);
                        // as an expression it has no honest single-value
                        // translation (append's result would need to replace
                        // the original binding, which an expression position
                        // can't do). This marker will typically fail to
                        // compile in Go rather than silently misbehave.
                        return "nil /* UNSUPPORTED: .push used as an expression, not a statement — see codegen_go.rs docs */".to_string();
                    }
                }
                // Positional typ constructor `Point(1, 2)` -> `Point{x: 1, y: 2}`
                if let ExprKind::Ident(tname) = &callee.kind {
                    if let Some(fields) = self.typ_fields.get(tname) {
                        let pairs: Vec<String> = fields
                            .iter()
                            .zip(args.iter())
                            .map(|(f, a)| format!("{}: {}", f, self.gen_expr(a, scope)))
                            .collect();
                        return format!("{}{{{}}}", tname, pairs.join(", "));
                    }
                }
                format!("{}({})", self.gen_expr(callee, scope), args.iter().map(|e| self.gen_expr(e, scope)).collect::<Vec<_>>().join(", "))
            }
            ExprKind::Index { base, index } => format!("{}[{}]", self.gen_expr(base, scope), self.gen_expr(index, scope)),
            ExprKind::Field { base, name } => {
                // `EnumName.Variant` -> bare `Variant`: Go's `iota` constants
                // aren't namespaced under their type the way Python's `Enum`
                // class or JS's frozen object are (see gen_item's Item::Enum
                // arm and module docs).
                if let ExprKind::Ident(base_name) = &base.kind {
                    if self.enum_names.contains(base_name) {
                        return name.clone();
                    }
                }
                format!("{}.{}", self.gen_expr(base, scope), name)
            }
            ExprKind::Binary { op, lhs, rhs } => format!("({} {} {})", self.gen_expr(lhs, scope), go_binop(*op), self.gen_expr(rhs, scope)),
            ExprKind::Unary { op, expr } => match op {
                UnOp::Not => format!("(!{})", self.gen_expr(expr, scope)),
                UnOp::Neg => format!("(-{})", self.gen_expr(expr, scope)),
            },
            ExprKind::Unwrap { .. } => {
                "nil /* UNSUPPORTED: ?? outside a let/mut binding, see codegen_go.rs docs */".to_string()
            }
        }
    }
}

fn params_sig(params: &[Param]) -> String {
    params.iter().map(|p| format!("{} {}", p.name, go_type(&p.ty))).collect::<Vec<_>>().join(", ")
}

fn go_type(ty: &Type) -> String {
    match ty {
        Type::Named(name) => match name.as_str() {
            "int" => "int".to_string(),
            "float" => "float64".to_string(),
            "str" => "string".to_string(),
            "bool" => "bool".to_string(),
            other => other.to_string(),
        },
        Type::Generic(name, args) => match name.as_str() {
            "list" => format!("[]{}", go_type(&args[0])),
            "map" => format!("map[{}]{}", go_type(&args[0]), go_type(&args[1])),
            "opt" => format!("*{}", go_type(&args[0])),
            other => format!("{}[{}]", other, args.iter().map(go_type).collect::<Vec<_>>().join(", ")),
        },
    }
}

/// The zero value of a Go type, used to fill in a `return` slot Go requires
/// but CuNi's source didn't supply a value for (a bare `ret`, or a still-`...`
/// fallible function body).
fn zero_value(ty: &Type) -> String {
    match ty {
        Type::Named(name) => match name.as_str() {
            "int" => "0".to_string(),
            "float" => "0.0".to_string(),
            "str" => "\"\"".to_string(),
            "bool" => "false".to_string(),
            other => format!("{}{{}}", other),
        },
        Type::Generic(name, _) => match name.as_str() {
            "opt" => "nil".to_string(),
            "list" => "nil".to_string(),
            "map" => "nil".to_string(),
            _ => "nil".to_string(),
        },
    }
}

fn kind_of_type(ty: &Type) -> VarKind {
    match ty {
        Type::Generic(name, _) if name == "list" => VarKind::List,
        Type::Generic(name, _) if name == "map" => VarKind::Map,
        _ => VarKind::Other,
    }
}

fn kind_of_literal(e: &Expr) -> Option<VarKind> {
    match &e.kind {
        ExprKind::List(_) => Some(VarKind::List),
        ExprKind::Map(_) => Some(VarKind::Map),
        _ => None,
    }
}

/// Best-effort element-type guess for an untyped list literal, from its first
/// element. A lightweight heuristic, not real type inference — good enough
/// for the current examples, in the same spirit as codegen_py.rs's disclosed
/// map-vs-list loop guessing.
fn infer_list_elem_type(items: &[Expr]) -> Option<String> {
    let first = items.first()?;
    Some(match &first.kind {
        ExprKind::Int(_) => "int".to_string(),
        ExprKind::Float(_) => "float64".to_string(),
        ExprKind::Bool(_) => "bool".to_string(),
        ExprKind::Str(_) | ExprKind::InterpStr(_) => "string".to_string(),
        _ => "any".to_string(),
    })
}

fn infer_map_kv_type(pairs: &[(Expr, Expr)]) -> Option<(String, String)> {
    let (k, v) = pairs.first()?;
    let kt = match &k.kind {
        ExprKind::Int(_) => "int".to_string(),
        ExprKind::Float(_) => "float64".to_string(),
        ExprKind::Bool(_) => "bool".to_string(),
        ExprKind::Str(_) | ExprKind::InterpStr(_) => "string".to_string(),
        _ => "any".to_string(),
    };
    let vt = match &v.kind {
        ExprKind::Int(_) => "int".to_string(),
        ExprKind::Float(_) => "float64".to_string(),
        ExprKind::Bool(_) => "bool".to_string(),
        ExprKind::Str(_) | ExprKind::InterpStr(_) => "string".to_string(),
        _ => "any".to_string(),
    };
    Some((kt, vt))
}

/// Capitalizes the first character — Go's `encoding/json` only sees exported
/// (capitalized) struct fields via reflection, so `link`'s wire structs need
/// this regardless of CuNi's own identifier casing (see
/// `gen_link_types_and_handler`'s docs).
fn go_capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The Go type a `link` wire struct field is declared as. An `int` field is
/// `string` on the wire (see `codegen_py.rs`'s `py_wire_encode` docs for why:
/// JSON has no arbitrary-precision integer type and JS's `Number` loses
/// precision above 2^53, so a decimal string is the one lossless
/// representation all three v1 targets share) — `strconv` converts it back
/// to a real `int` after decode (see `gen_link_types_and_handler`).
/// float64/string/bool pass through as their native Go/JSON type.
fn go_wire_field_type(ty: &Type) -> String {
    match ty {
        Type::Named(name) if name == "int" => "string".to_string(),
        _ => go_type(ty),
    }
}

/// Converts a Go expression of CuNi type `ty` into its wire-struct-field
/// value (`strconv.FormatInt` for `int`, pass-through otherwise).
fn go_wire_encode(expr: &str, ty: &Type) -> String {
    match ty {
        Type::Named(name) if name == "int" => format!("strconv.FormatInt(int64({}), 10)", expr),
        _ => expr.to_string(),
    }
}

/// The inverse of `go_wire_encode`, used when decoding a `Response.Result`
/// field back into a real CuNi-typed Go value. Ignores the (already-checked
/// elsewhere, see `gen_link_types_and_handler`) parse error for `int` — this
/// call site is decoding a value *this same codebase* encoded moments
/// earlier over the wire, so a parse failure here would mean the two sides
/// of the same `link` disagree on its own wire format, not a real user input
/// error the way a request-side decode failure is.
fn go_wire_decode(expr: &str, ty: &Type) -> String {
    match ty {
        Type::Named(name) if name == "int" => format!("func() int {{ v, _ := strconv.ParseInt({}, 10, 64); return int(v) }}()", expr),
        _ => expr.to_string(),
    }
}

fn go_binop(op: BinOp) -> &'static str {
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
        BinOp::And => "&&",
        BinOp::Or => "||",
    }
}

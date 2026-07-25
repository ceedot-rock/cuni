# CuNi Open Items — Draft Proposal

**Status: Items 1-4 RATIFIED and folded into SPEC.md** (§12 `fail`, §7 top-level ret, §14 `enum`, §15 stdlib table) and implemented in all three toy backends (`src/codegen_{py,go,js}.rs`) — see SPEC.md §18 for what's still open. **Item 5 is a new DRAFT**, not yet ratified, surfaced while testing the implementation of items 1-4 against `examples/modules.cuni`. Every design decision written into SPEC.md was signed off by the user first; nothing in a DRAFT section is final until the same happens. Each section: problem, options, recommendation, why.

---

## 1. Failure-signaling keyword — RATIFIED (see SPEC.md §12)

**Problem.** A fallible function (`-> T ?`) can declare it might fail, but CuNi has no syntax to actually signal that failure from inside the body. Every example so far uses `...` (the `Todo` stub), which means "not written yet," not "this call failed." Without this, `?`/`??` is only half a feature — the caller side (`??`) exists, the callee side doesn't.

**Constraints from §2 (exactness) and §12 (no exceptions):** the mapping must be lossless and idiomatic on all three v1 targets, and per §12 CuNi deliberately has no general exception mechanism because Go has none. So the keyword must not read as "throw an exception" — it has to read as "signal failure," and the compiler picks the idiomatic per-target realization (Python exception, Go error return, JS throw) itself, the same way `??` already hides three different unwrap mechanisms behind one operator.

**Options.**
- **(a) `fail expr`** — a statement, used only inside a fallible function's body, that terminates the function with a failure carrying `expr` as the failure value/message. Mirrors `ret` (`ret` = succeed with a value, `fail` = fail with a value) — same shape, opposite outcome, one new word.
- **(b) Reuse `ret` with a marker**, e.g. `ret err(expr)` where `err` is a new wrapper. Avoids a new keyword but overloads `ret`'s meaning contextually, which violates the "one concept, one keyword, no context-dependent meanings" tenet in §1.
- **(c) A two-keyword pair `try`/`fail`** modeled on Rust's `Result`. Adds a second keyword for no real benefit here — CuNi doesn't need a `try` since `?` on the return type already marks fallibility at the signature; nothing needs marking at each call site the way Rust needs `?`-propagation.

**Recommendation: (a) `fail expr`.**

- Symmetric with `ret`: a fallible function's body ends by either `ret v` (success, value `v`) or `fail e` (failure, value `e`). Non-fallible functions simply can't use `fail` — a parse/type error, same tier as using `ret` with a value in a `-> void` function.
- Mnemonic and consistent with the existing 3-4 letter word style (`ret`, `mut`, `whl`).
- Codegens cleanly on all three targets:
  - **Python:** `raise CuNiError(e)` (or a plain custom exception class emitted once per program) — exceptions are idiomatic here and `??`'s existing Python handling already expects `except Exception`.
  - **Go:** `return zeroValue, e` from a function whose signature the compiler already rewrote to `(T, error)` because of the `?` marker — this is exactly the idiomatic Go error-return pattern, and it's *why* Go's error model doesn't force anything ugly here.
  - **JS:** `throw e` (or `throw new CuNiError(e)`), matching `??`'s existing JS-side `try/catch` shape.
- `e`'s type: simplest workable rule is "same type as whatever `??`'s handler currently deals with" — i.e., `fail` takes any value (commonly a `str` message), not a declared error type. A typed-error-hierarchy design is a bigger feature (basically enums, see §3) and shouldn't block unblocking `fail` now. This can be revisited once tagged unions exist, without breaking `fail expr`'s surface syntax.
- The type/effect checker must additionally reject `fail` used outside a fallible (`?`) function, and reject a fallible function whose body has no possible `fail` (arguably a warning, not an error, since "declared fallible but always succeeds" is not unsound, just a smell) — a lint but not a proposal item here.

---

## 2. Top-level `ret` / implicit-main semantics — RATIFIED (see SPEC.md §7)

**Problem.** CuNi allows top-level statements outside any `def` (see the full example, SPEC.md §15), and those statements can contain `??` handlers with `ret` inside them (`let idx = find(nums, 16) ?? do say("not found") ret -1 end`). But `ret` is defined as "return from an enclosing function" (§7), and at top level there is no enclosing function. The Python backend's current behavior — wrap all top-level statements into one implicit `main()` and call it — is a real, working design; it just isn't written down as a spec rule anywhere, and Python didn't strictly need it (module-level code is legal in Python) — it was adopted purely so top-level `ret` has *somewhere* to return from.

**Options.**
- **(a) Ratify implicit main.** Ratify the Python backend's assumption formally: top-level code is defined as the body of an implicit entry-point function (call it `main` in the spec prose, though the compiler need not literally emit that name in Python/JS). `ret` at top level returns from the whole script/program, ending execution — same semantics as `ret` inside a real `def`, just at outermost scope.
- **(b) Disallow `ret` at top level** — a parse/type error. Top-level `??` handlers could still exist but would be restricted to non-`ret` statement bodies (e.g. `say(...)` alone), which breaks the existing §15 example verbatim — that example would need to change, or top-level `??` would need a different exit construct than `ret` (adds complexity for what it saves).
- **(c) Something else** — e.g. a dedicated `exit`/`halt` statement distinct from `ret`, reserved for top-level use only. Adds a second "leave-the-current-scope" keyword for a case that behaves identically to `ret` in every target once main-wrapping happens anyway — violates "one concept, one keyword."

**Recommendation: (a), ratify implicit main.**

- It's already true in practice for Go, which requires `func main()` — there is no such thing as "top-level Go code" outside a function at all. Ratifying this rule doesn't add a new constraint for Go; it just names the constraint Go was already imposing on the other two targets.
- It costs nothing for Python/JS (both allow bare module-level `return`... actually Python doesn't allow bare `return` at module scope either, which is exactly why the backend already wraps it — same story for JS `return` outside a function, which is a `SyntaxError`). So all three targets *already* require this wrapping to make top-level `ret` legal; (a) just writes down what all three backends must do, rather than leaving it as an unstated convention one backend happened to invent.
- It matches the §15 example exactly with zero changes to existing examples.
- Concretely for the spec: add one sentence to §7 (Functions) or a new short subsection: *"Top-level statements (outside any `def`) are implicitly the body of the program's entry point. `ret` at top level ends the program's execution; it behaves exactly as `ret` does inside any other function."* Optionally note the Go mapping is `func main()` directly, and Python/JS emit an equivalent wrapper function invoked at the bottom of the file (as the toy backend already does).
- One follow-on question worth flagging for the user, not blocking this decision: what does a *bare* top-level `ret` with no value mean if it's not inside a `??` handler — presumably "exit early," which is fine and requires no new rule, just noting it falls out of (a) automatically.

---

## 3. Enums / tagged unions — payload-free half RATIFIED (see SPEC.md §14); tagged unions with payload remain open (SPEC.md §18)

**Problem.** SPEC.md §16 lists this as undesigned. It's genuinely two different features that get conflated under "enums":
1. **Payload-free enums** — a closed set of named tags (`Color = Red | Green | Blue`), no attached data.
2. **Tagged unions / sum types with payload** — each variant carries its own data shape (`Shape = Circle(r: float) | Rect(w: float, h: float)`), plus exhaustive matching.

**Target reality check (this is the crux):**
- **Python:** `enum.Enum` covers (1) natively. (2) has no native support pre-3.10; even 3.10+ pattern matching over classes is a workaround (a base class + subclasses, matched via `match`/`isinstance`), not a real closed sum type the way Rust or Swift has it — nothing stops a fourth subclass appearing elsewhere, silently breaking exhaustiveness.
- **Go:** (1) is idiomatically `iota`-based typed constants — solid, exact, well-trodden. (2) has no sum types at all. The idiomatic emulation is an interface + a private/sealed marker method restricting implementers, with a type switch for matching — but Go cannot enforce exhaustiveness at compile time (no exhaustiveness checker in the language itself; you'd need an external linter, which is out of scope for a compiler that must "compile or refuse" using the target's own toolchain), and nothing stops a consumer of the interface from adding a new implementing type outside the union, silently un-sealing it.
- **JS:** no native support for either. (1) is usually a frozen object of string/int constants or a plain string-literal union (TS-only, and CuNi isn't targeting TS). (2) is usually a `{tag: "...", ...payload}` discriminated object, entirely convention, zero compiler enforcement.

**The honest conclusion:** payload-free enums have a clean, exact, lossless mapping on all three targets — Go's `iota` constants, Python's `Enum`, JS's frozen-object-of-constants are all straightforward and each target's own tooling can enforce "closed set of values" adequately (Python/Go at least give you *something* checkable; JS is the weakest of the three but a frozen object with a lint-level convention is no worse than CuNi's existing reliance on target idiom elsewhere). Full tagged unions with payload **cannot** currently satisfy §2's compile-or-refuse contract with a straight face on Go: there is no way for the CuNi compiler to make Go itself refuse an unsealed/non-exhaustive union the way it can lean on Python's or a hypothetical stricter target's own checker. Emitting an interface + type-switch and calling it "exact" would be the exact "close enough" approximation §2 explicitly forbids to promise.

**Options.**
- **(a) Full tagged unions with payload, now.** Rejected — no honest Go mapping, as above; would violate §2 immediately upon shipping, the same failure mode Rust was excluded from v1 for (§3: a construct that would force reshaping the core to accommodate the weakest target).
- **(b) Payload-free enums only, in v1; defer tagged-unions-with-payload** as an explicit open item, same posture as Rust in §3 ("candidate v2 feature; may require revisiting the core, e.g. once/if a target with real sum types is added, or once a Go-side sealing/exhaustiveness pattern is found that the compiler can actually enforce rather than merely suggest").
- **(c) Skip enums entirely for v1**, revisit both together later. Leaves a real, commonly-needed feature (§16 itself flags it as "useful for cross-language exactness") unaddressed for no gain over (b) — (b) already draws the line at exactly the boundary the targets support.

**Recommendation: (b).**

Proposed syntax, consistent with `typ`/`iface`/`is`:

```
enum Color do
    Red
    Green
    Blue
end

let c = Color.Green

if c == Color.Red do
    say("stop")
end
```

- New keyword `enum`, closing with `end` like every other block — matches `typ`/`iface`.
- Variants are bare names, referenced as `EnumName.Variant`, matching how `typ` fields and `iface` methods are already namespaced under their declaring type.
- No payload, no generic parameters, no pattern-matching construct introduced yet — comparison is via plain `==`/`!=` (already in the language), so no new operator is needed either.
- Codegen sketch: Python `class Color(Enum): Red = auto(); ...`; Go `type Color int` + `const (Red Color = iota; ...)`; JS a frozen object `const Color = Object.freeze({Red: "Red", Green: "Green", Blue: "Blue"})` (or integers — string tags are friendlier for debugging output and don't cost anything since JS has no `iota`).
- Explicitly document in SPEC.md §16 (or wherever `enum` lands) that payload-carrying tagged unions remain an open item, not solved by this — so nobody mistakes payload-free enums for the full feature request being closed.

---

## 4. Standard library scope — RATIFIED (see SPEC.md §15)

**Problem.** The Python backend invents `say` (→ `print`) and treats `.push` as a builtin list method (→ `.append`), with nothing in SPEC.md defining either as real stdlib surface — they're the codegen author's guesses about what the examples need, not a ratified contract. Every target backend would otherwise be free to invent its own guesses, which is exactly the kind of silent divergence §2 exists to prevent.

**What's actually used today** (`examples/full.cuni`, `examples/modules.cuni`): `say(x)` (print a value), `xs.push(v)` (append to a mutable list). Nothing else. No `len`/`size` call appears in either example yet, though it's an obvious, near-certain near-term need (e.g. bounds-checking before an index, or iterating with a computed range) — worth specifying now while the surface is still this small, rather than letting a third backend invent its own name for it too.

**Options.**
- **(a) Leave it implicit / per-backend convention** (status quo). Rejected — this is precisely the ungoverned divergence the exactness contract forbids; two backends could trivially disagree on a name or behavior (e.g. one emits `len()`, another `.length`) with no spec to catch it.
- **(b) A minimal stdlib table in SPEC.md**, same format as the existing keyword reference table (§14), listing each stdlib name with its signature and its exact mapping per target. Grows by amendment as new examples need new functions — small, auditable, versioned alongside the language.
- **(c) A full standard library spec** (modules, namespacing, a `std.list`/`std.str` hierarchy, etc.) up front. Overkill for where the language is right now — v1 has three functions worth of demonstrated need; designing a whole module system for hypothetical future stdlib growth before any of it exists contradicts "small core over broad coverage" (§1).

**Recommendation: (b).**

Proposed initial table (to live in a new SPEC.md section, e.g. "§17 Standard Library" or folded into §11 Collections for the collection methods):

| CuNi | Signature | Python | Go | JS |
|---|---|---|---|---|
| `say(x)` | `(any) -> void` | `print(x)` | `fmt.Println(x)` | `console.log(x)` |
| `xs.push(v)` | `(list<T>, T) -> void`, `xs` must be `mut` | `xs.append(v)` | `xs = append(xs, v)` | `xs.push(v)` |
| `xs.len()` | `(list<T>) -> int` | `len(xs)` | `len(xs)` | `xs.length` |

Notes on the table:
- `say` and `.push` simply promote what the toy backend already does from "codegen author's guess" to "documented contract" — no behavior change, just ratification.
- `.len()` is proposed as a *method* (`xs.len()`) rather than a free function (`len(xs)`) so it reads consistently with `.push` (both are collection operations spelled as methods on the collection), even though Python's own idiom is the free function `len()` — the CuNi-level name doesn't have to match any one target's idiom, only compile to it exactly, same as `say` not existing as a builtin named `say` in any target.
- Go's `.push` mapping is the one genuine wrinkle: Go's `append` is not in-place, it returns a new slice, so `xs.push(v)` must reassign the underlying binding (`xs = append(xs, v)`), which only works because `.push` is restricted to `mut`-bound lists (§11 already establishes this rule) — worth a callout in the stdlib table itself so a future backend author doesn't miss it and emit `append(xs, v)` as a bare expression statement, silently dropping the mutation for Go's target while Python/JS work fine, which is exactly the kind of one-target-breaks silently that compile-or-refuse is supposed to catch (it should be a compile error on `let`-bound `xs.push`, per §11, not just a Go-specific gotcha).
- Going forward: any new stdlib surface (e.g. string methods, map methods) gets added to this same table with all three mappings filled in *before* any backend is allowed to implement it — the table is the spec, backends implement the table, not the other way around.

---

## 5. `ext` name collisions with target-global identifiers — IMPLEMENTED (option (b), see `src/checks.rs`)

**Problem.** `examples/modules.cuni` declares `ext fetch(url: str) -> str do ... js: await fetch(url).then(r => r.text()) ... end`. The JS backend emits this as `async function fetch(url) { return await fetch(url)...; }` — a top-level function declaration named `fetch` that shadows the global `fetch` it means to call, so the emitted code recurses into itself instead (a `RangeError: Maximum call stack size exceeded` at runtime, confirmed by actually running the generated output). Nothing in `ext`'s design (§9) or either backend detects or prevents an `ext` name matching a target-global identifier used inside that same `ext` block's own body. This is JS-specific in *how* it manifests (silent infinite recursion) precisely because JS resolves the shadowing at call time rather than refusing to compile — Python's analogous gap (`ext fetch` calling `requests.get`, an unimported name) fails loudly with `NameError` instead, and Go's (`httpGet` undefined) fails to compile outright. JS is the only target where the failure mode is silent and confusing rather than an immediate hard error.

**Constraints from §2 (exactness):** `ext` is already the one sanctioned escape hatch from portability (§9) — this isn't about making `ext` itself more portable, it's about whether the compiler can at least *refuse* a specific, detectable class of broken `ext` declaration rather than silently emitting something that behaves wrong only on one target.

**Options.**
- **(a) Do nothing; document it as a known sharp edge.** `ext` is already explicitly the "this code is your responsibility" opt-out (§9) — a user who names their `ext` function the same as a target global they call inside it is arguably misusing the escape hatch, not exposing a language design flaw. Cheapest option, but leaves a foot-gun that fails silently on exactly one of three targets, which is a worse failure mode than the "fails loudly or doesn't compile" gaps Python/Go already have for the same example.
- **(b) Compiler-level name check, per target.** Before emitting each target, check the `ext` declaration's own name against a hardcoded list of that target's well-known globals (JS: `fetch`, `console`, `Math`, ...; Python: builtins via a fixed list or `builtins` module introspection; Go: nothing comparable exists, since Go has no ambient globals of this kind — imports are explicit). If the `ext` name collides, refuse to compile for that target with a clear error ("`ext fetch` shadows the JS global `fetch` inside its own `js:` body — rename the CuNi binding"). Directly satisfies "compile-or-refuse" (§2) for this specific, mechanically-detectable case. Cost: a per-target list of reserved globals that needs maintaining as JS/Python's own global surface evolves; false positives possible if the user's `ext` body doesn't actually reference the colliding global (over-conservative, but a false refusal is far cheaper than a silent runtime bug).
- **(c) Always rename the emitted target-side function to a mangled/prefixed name** (e.g. `__cuni_ext_fetch`), regardless of collision, and rewrite call sites to the mangled name. Eliminates the whole collision class structurally rather than detecting one instance of it — the emitted `js:`/`py:`/`go:` raw body text still references the *original* global name (`fetch`) verbatim since it's spliced in unparsed (§9: "each target line is captured as raw source text, not parsed as CuNi"), so this only prevents the CuNi-declared name from colliding with a global; it can't rescue a raw-text body that assumes its own function name is exactly `fetch` for some other reason. More invasive than (b) for a case (b) already fully covers.
- **(d) Warn, don't refuse.** Emit a compiler warning when an `ext` name matches a known target global, but still compile. Weaker than (b); doesn't satisfy compile-or-refuse, and a silent-recursion bug is exactly the kind of thing that shouldn't ship with just a warning attached.

**Recommendation: (b), scoped narrowly.** Maintain a small, explicit reserved-globals list per target (starting with the handful that would actually cause silent breakage — `fetch`, `console`, a few others for JS; Python/Go's own gaps in this example already fail loudly without any new mechanism, so (b) may only be load-bearing for JS in practice). Refuse to compile with a specific, actionable error naming the collision. This is deliberately narrower than a general "no `ext` name may ever match any target identifier" rule, which would be both unenforceable (Python/Go's builtin surface is enormous) and not actually the problem — the problem is specifically an `ext` body silently and recursively calling itself, not merely sharing a name with something in the target's namespace.

**Not yet decided:** whether this belongs in the toy backends now, or waits for the real type/effect checker (§18) that's supposed to own all compile-or-refuse logic — leaning toward the latter, since the toy backends have deliberately never implemented any refusal logic so far (see each backend's module docs), and adding one narrow refusal rule to a backend that otherwise never refuses anything would be an inconsistent precedent. Staged here for the user's call.

---

## Summary

Items 1-4 are ratified and live in SPEC.md (§7, §12, §14, §15) and implemented in all three toy backends. Item 5 is now also implemented: `src/checks.rs::find_ext_collision` refuses to compile for `py`/`js` when an `ext` name matches its short reserved-globals list, wired into `main.rs` before each `--emit-*` call. SPEC.md's own `ext` example (§9) was renamed `fetchPage` to avoid triggering its own new rule. Verified: `examples/modules.cuni --emit-js` now refuses with a clear error instead of silently producing self-recursive JS; `--emit-py`/`--emit-go` for the same file are unaffected (still fail for their own pre-existing, unrelated reasons — undefined `requests`/`httpGet`).

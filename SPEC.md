# CuNi (Code:uNiTY) — Language Specification v0.1

*See `GRAMMAR.md` for the formal EBNF syntax reference; this document is the prose/rationale reference.*

## 1. Purpose

CuNi is a mnemonic, human-readable programming language designed to compile to **exact, idiomatic source code** in multiple target languages from a single CuNi program. It is not a general-purpose language that happens to have many backends — it is deliberately small, so that every construct in it has a provable, lossless mapping into every supported target.

**Design tenets:**
- Mnemonic over cryptic: every keyword is a short, guessable word (`ret`, `mut`, `iface`), not a symbol or abbreviation that needs to be memorized from scratch.
- One concept, one keyword: no synonyms, no context-dependent meanings for the same token.
- Small core over broad coverage: CuNi supports fewer things than any one of its targets, on purpose. Depth is sacrificed for the guarantee.
- Explicit over inferred, except where inference is unambiguous: mutability, error-handling, and non-portable code are always visible in the source, never guessed at silently.

## 2. The Exactness Contract

CuNi's central promise: **a CuNi program with no `ext` blocks compiles to identical, idiomatic behavior on every supported target — or it fails to compile.** There is no approximate or best-effort mode.

This is enforced by two rules:

1. **Compile-or-refuse.** If a construct in a CuNi program does not have a proven, lossless mapping to a given target, the compiler rejects the program for that target. It never emits a "close enough" approximation.
2. **Single-tier target support.** A language is only listed as a "supported target" if it implements 100% of the CuNi portable core. There are no partial or extended-tier targets. This means the ambition of the core is capped by the least-capable currently-supported target — adding a weaker target later either shrinks the core (a breaking change) or that language doesn't qualify as a target.

The one deliberate exception is the `ext` block (see §9), which is an explicit, visible opt-out of portability for a specific binding — never implicit.

## 3. v1 Target Languages

**Python, JavaScript, Go.**

All three are garbage-collected with reference semantics, which is why they can share one core without an ownership/borrowing model. Rust is deliberately excluded from v1: its ownership rules would force a three-way own/borrow-mut/borrow-immut annotation onto every binding and function signature, reshaping the entire core before it's even validated elsewhere. Rust is a candidate v2 target; adding it may require a breaking change to how bindings and functions are declared.

## 4. Lexical Basics

**Comments** — line comments only, `#` to end of line.

```
# this computes area
```

**Strings** — two literal forms:
- `"..."` — plain string, no interpolation.
- `` `...` `` — backtick string, supports `${expr}` interpolation.

```
let plain = "no interpolation here"
let msg = `radius is ${c.r}`
```

**Booleans** — `true`, `false`.

**Numbers** — `int`, `float`, standard decimal literals.

## 5. Blocks

Every block opens with a keyword (`do`, or the block-introducing keyword itself) and closes with `end`. No braces, no significant whitespace. This is unambiguous to parse and to re-emit exactly regardless of the target's own block syntax (indentation, braces, `end`, etc.).

```
if x > 4 do
    say("big")
els
    say("small")
end
```

## 6. Bindings

`let` — immutable binding. `mut` — mutable binding. Mutability is always declared explicitly at the binding site; it is never inferred, because targets differ in whether and how they enforce it.

```
let x = 5
mut y = 10
y = y + 1        # legal — y is mut
```

## 7. Functions

```
def name(param: type, ...) -> returnType do
    ret expr
end
```

Generics use `<T>` after the function name:

```
def first<T>(xs: list<T>) -> T do
    ret xs[0]
end
```

**Top-level code.** Statements outside any `def` are implicitly the body of the program's entry point. `ret` at top level ends the program's execution — it behaves exactly as `ret` does inside any other function, it just has no enclosing `def` in the source. (Concretely: Go already requires a `func main()` for any executable, so this is simply naming the constraint Go was already imposing; Python and JS backends wrap top-level statements in an equivalent implicit function since bare `return` is illegal at their own module scope.)

## 8. Control Flow

**Branching:**

```
if cond do
    ...
els
    ...
end
```

**Range/collection loop:**

```
for x in xs do
    ...
end

for i, x in xs do   # index + value, or key + value for maps
    ...
end
```

**Conditional loop:**

```
whl cond do
    ...
end
```

**Logical operators** are word-based, matching the rest of the keyword vocabulary: `and`, `or`, `not`. Arithmetic (`+ - * / %`) and comparison (`== != < > <= >=`) stay symbolic — these glyphs are already universal across every target, so there's nothing to disambiguate by spelling them out.

## 9. Modules

Two-tier import system:

- **`use`** — imports another CuNi module. Fully portable; never breaks the exactness contract.
- **`ext`** — the one explicit escape hatch for calling target-native code. Declares a per-target mapping inline. Using `ext` is the only thing that makes a program's output diverge from "identical on every target" — and it's always visible in source, never inferred.

```
use math

ext fetchPage(url: str) -> str do
    py: requests.get(url).text
    go: httpGet(url)
    js: await fetch(url).then(r => r.text())
end
```

A program with zero `ext` blocks gets the full portability guarantee. A program with `ext` blocks is exact except for the specific lines marked non-portable.

Note the `ext` binding above is named `fetchPage`, not `fetch` — an `ext` name that matches a target-global identifier its own body calls (e.g. naming this binding `fetch` while its `js:` line also calls the global `fetch`) shadows that global with the emitted top-level function, turning the intended call into silent self-recursion. The compiler refuses to compile such a collision for the affected target rather than emit it (see `src/checks.rs`, OPEN_ITEMS_PROPOSAL.md item 5).

## 10. Structs and Interfaces

**Structs** (`typ`):

```
typ Point do
    x: int
    y: int
end

let p = Point(3, 4)   # positional constructor — args match field declaration order
say(p.x)
```

Construction is a plain call on the type name: `TypName(field0, field1, ...)`. Arguments are positional in the order fields were declared (no named-field syntax in v1). The type checker enforces arity; each backend emits its idiomatic construction form (Python dataclass call, Go composite literal `Point{x: 3, y: 4}`, JS `new Point(3, 4)`).

**Interfaces** (`iface`) — conformance is always explicit via `is`, never structural/duck-typed. This lets the compiler check conformance before any target sees the code, rather than relying on each target's own (differing) notion of structural typing. Target backends do **not** re-encode `is` as inheritance/ABC satisfaction (that would break free-function method realization and struct construction); CuNi's type checker is the enforcement.

```
iface Shape do
    area() -> float
end

typ Circle is Shape do
    r: float
end
```

## 11. Collections

`list<T>` and `map<K,V>`. Type is inferred from the literal by default; an explicit `<T>` annotation is required only when the type is ambiguous (e.g. an empty collection). Map literals use `{...}` — free to use for this purpose since blocks use `do...end`, not braces.

```
let nums = [1, 2, 3]
mut names: list<str> = []

let scores = {"amy": 90, "cee": 95}
```

A `let`-bound collection is immutable (no `.push`, no mutation methods); a `mut`-bound one is not. This maps directly onto Rust's `Vec` mutability model (relevant if/when Rust becomes a target) and is simply a no-op distinction when emitting to Python.

## 12. Error Handling

No exceptions — they have no honest mapping to Go, which has none. Instead, fallible functions mark their return type with `?`, and the caller unwraps with `??`:

```
def parse(s: str) -> int ? do
    ...
end

let n = parse("5") ?? do
    say("bad input")
    ret 0
end
```

A fallible function's body ends by either `ret v` (succeed with value `v`) or `fail e` (fail with value `e`) — symmetric with `ret`, one new keyword. `fail` is only valid inside a function whose return type is marked `?`; a non-fallible function has no failure channel to signal through. `e` can be any value (commonly a `str` message) — there is no declared error-type hierarchy yet (see §19 Open Items).

```
def parse(s: str) -> int ? do
    if s == "" do
        fail "empty input"
    end
    ret 42
end
```

Each target picks its own idiomatic failure channel for both `?`/`??` and `fail`: Python raises an exception, Go returns `(T, error)`, JS throws. This is exactly the same "one CuNi concept, several idiomatic realizations" pattern `??` already uses to hide three different unwrap mechanisms behind one operator.

## 13. Optionals

No `null`/`nil`/`None`/`undefined` — each target handles absence differently (Python `None`, Go zero-values/pointers, JS `null` and `undefined` both). Instead, optionality is a type: `opt<T>`, with the literal `none`. The `??` unwrap idiom is reused from error handling rather than introducing a second operator — both are "handle the case where this isn't a plain value," so they share one piece of vocabulary instead of two.

```
def find<T>(xs: list<T>, target: T) -> opt<int> do
    for i, x in xs do
        if x == target do
            ret i
        end
    end
    ret none
end
```

## 14. Enums

`enum` declares a closed set of named, payload-free variants:

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

Variants are referenced as `EnumName.Variant`, matching how `typ` fields and `iface` methods are already namespaced under their declaring type. Comparison is plain `==`/`!=` — no new operator or pattern-matching construct is introduced.

This is deliberately the payload-free half of "enums." **Tagged unions with payload** (each variant carrying its own data shape, e.g. `Shape = Circle(r: float) | Rect(w: float, h: float)`, plus exhaustive matching) remain an open item (§19): Go has no honest way for the compiler to make Go itself refuse an unsealed or non-exhaustive union, which the exactness contract (§2) requires before a construct can ship. Payload-free enums don't have this problem — Go's `iota` constants, Python's `Enum`, and a frozen JS object are all straightforward, adequately-closed mappings on all three v1 targets.

## 15. Standard Library

A minimal, explicit table of stdlib surface — every name here has a ratified mapping on all three targets, and any new stdlib addition is added to this table (with all three mappings specified) before any backend is allowed to implement it. This exists specifically to prevent the ungoverned divergence the exactness contract (§2) forbids: without a shared table, two backends could each invent their own name or behavior for the same concept.

| CuNi | Signature | Python | Go | JS |
|---|---|---|---|---|
| `say(x)` | `(any) -> void` | `print(x)` | `fmt.Println(x)` | `console.log(x)` |
| `xs.push(v)` | `(list<T>, T) -> void`, `xs` must be `mut` | `xs.append(v)` | `xs = append(xs, v)` | `xs.push(v)` |
| `xs.len()` | `(list<T>) -> int` | `len(xs)` | `len(xs)` | `xs.length` |

Notes:
- `.len()` is spelled as a method (`xs.len()`), not a free function (`len(xs)`), so it reads consistently with `.push` — both are collection operations spelled as methods on the collection — even though this doesn't match Python's own free-function idiom. The CuNi-level name only has to compile to each target's idiom, not match it.
- Go's `.push` mapping is the one genuine wrinkle: `append` isn't in-place, so `xs.push(v)` must reassign the binding (`xs = append(xs, v)`), which only works because `.push` is restricted to `mut`-bound lists (§11). A conforming Go backend must reject `.push` on a `let`-bound list at compile time, not silently emit a no-op expression statement that drops the mutation.

## 16. Cross-Program Interop (`link`)

`link` declares a typed message contract callable both in-process (like a `def`) and, via a generated wire handler/client pair, by a *separately compiled* CuNi program running as a different target — the missing piece `ext` (§9) doesn't cover, since `ext` only reaches into that same compilation's own target-native code, not into another CuNi program entirely.

```
link Greet(name: str, times: int) -> str do
    ret `hello ${name} x${times}`
end
```

Same shape as `def` — `link Name(params) -> type [?] do ... end` — with two differences:

- **No generics.** A wire contract needs a concrete, enumerable type shape; a type parameter doesn't give it one.
- **v1 scope: params and return type must be scalar** (`int`, `float`, `str`, `bool`) — `list<T>`, `map<K,V>`, and `typ`-defined structs aren't supported yet, since each would need its own recursive wire codec this version doesn't generate. Per §2's compile-or-refuse posture, the compiler refuses a `link` with a non-scalar param/return rather than emit a wrong or partial codec (`src/checks.rs::find_bad_link_type`).

**What a `link` declaration generates**, per target:
1. The plain callable function itself — behaves exactly like a `def`, including `fail`/fallibility rules (§12).
2. A wire handler, `<name>_handler` — decodes a JSON request, calls the local function, and encodes the JSON response (or a `{"error": ...}` response on any failure, including `fail` and stub bodies, since those already raise/throw/error per §12's existing per-target mapping — no extra casework needed).
3. A client stub, `<name>_remote(base_url, ...params) -> type` — **always effectively fallible**, regardless of whether the `link` itself is: a network call can fail even when the local logic can't, so it reuses `??`/`fail`'s existing machinery (Python raises `CuNiError`, Go returns `(T, error)`, JS throws) rather than inventing a second error channel just for network failures.

**Wire substrate: plain HTTP + JSON**, chosen over gRPC/protobuf specifically because it adds no native-dependency toolchain requirement (no `node-gyp`/C-extension build) to any v1 target — see `INTEROP_PROPOSAL.md` for the full design discussion this was decided from. **Numeric exactness across the wire:** JSON has no arbitrary-precision integer type, and JS's own `Number` silently loses precision above 2^53 — so an `int` field is wire-encoded as a decimal *string*, not a JSON number, on all three targets. Decoding that string back into a real integer is lossless in Python (arbitrary precision) and Go (`strconv.ParseInt` into an `int64`), but **not fully solved for JS**, which decodes it with plain `Number(...)` — a `link` int wider than 2^53 will silently lose precision specifically on the JS side. This is a real, disclosed limitation (see `codegen_js.rs`'s `js_wire_decode` docs), not a solved one; a `BigInt`-based fix was considered and deferred, since it would make `link`-decoded ints a different runtime representation than every other CuNi `int` in the JS backend.

**Codegen-only, no bundled runtime.** `link` does not start a server or make an outbound connection on its own — mounting `<name>_handler` (choosing a port, a router, TLS, etc.) is the CuNi program's own responsibility, the same way `func main()`/`def main()` is code the compiler emits, not a process it manages. This also means the same wire contract (name, param names, param/return types) must currently be written once per source file that needs it — there is no cross-file/cross-program sharing mechanism yet (`use`, §9, doesn't resolve across separately-compiled programs either); keeping two independently-maintained `link` declarations in sync is on the CuNi author, not checked by the compiler.

**Also not yet designed:** streaming/long-lived connections (deferred, same posture as tagged unions from `enum` and Rust from v1 — see §19), and the exact grammar is settled here but `GRAMMAR.md` should be checked for the authoritative production.

## 17. Keyword Reference

| Keyword | Meaning |
|---|---|
| `do` / `end` | open / close a block |
| `let` | immutable binding |
| `mut` | mutable binding |
| `def` | function definition |
| `link` | typed message contract, callable in-process and cross-program |
| `ret` | return |
| `fail` | signal failure from a fallible function's body |
| `if` / `els` | branching |
| `for ... in` | range/collection loop |
| `whl` | while loop |
| `typ` | struct definition |
| `iface` | interface definition |
| `is` | interface conformance |
| `enum` | payload-free enum definition |
| `use` | import a CuNi module (portable) |
| `ext` | declare a non-portable, per-target binding |
| `?` (on return type) | function is fallible |
| `opt<T>` | optional value type |
| `none` | the absent-value literal |
| `??` | unwrap-or-handle (fallible results and optionals) |
| `and` / `or` / `not` | logical operators |

## 18. Full Example

```
iface Shape do
    area() -> float
end

typ Circle is Shape do
    r: float
end

def area(c: Circle) -> float do
    ret 3.14159 * c.r * c.r
end

def find<T>(xs: list<T>, target: T) -> opt<int> do
    for i, x in xs do
        if x == target do
            ret i
        end
    end
    ret none
end

def parse(s: str) -> int ? do
    ...
end

let nums = [4, 8, 15, 16, 23]
mut names: list<str> = []

let idx = find(nums, 16) ?? do
    say("not found")
    ret -1
end

# greet whoever we found, if the input parses
let n = parse("42") ?? do
    say("bad input")
    ret 0
end

if idx > 0 and n > 0 do
    names.push(`found at ${idx}, n is ${n}`)
els
    say("nothing to report")
end
```

## 19. Open Items (not yet designed)

### Done for v0.1

- ~~Concrete grammar (formal EBNF)~~ — `GRAMMAR.md`.
- ~~Cross-program interop (sync request/response)~~ — §16 `link` + `INTEROP_PROPOSAL.md`. Streaming/long-lived connections remain deferred.
- ~~Type/effect checker (bounded)~~ — `src/typeck.rs`: undefined names, `let` mutation, `fail` outside fallible, arg counts, unknown types, `typ…is` conformance, **fallible calls without `??`**, **positional typ constructors**, `link` `*_remote` stubs. No full inference/generics unification yet; no AST source spans (errors name the construct, not line:col).
- ~~`use` module resolution~~ — `src/modules.rs` loads `<dir>/<name>.cuni` relative to the importing file (recursive, cycle-safe, missing file refused).
- ~~`ext` name collisions~~ — `src/checks.rs` reserved-globals list for py/js.
- ~~Conformance + typeck suites~~ — `tests/conformance.rs` (full/enums/structs/modules/link) and `tests/typeck.rs` (11 refusal fixtures).

### Deliberately deferred (design, not unfinished code)

- **Tagged unions with payload** + exhaustive match — no honest Go mapping yet (§14, same posture as Rust's v1 deferral).
- **Typed error hierarchy for `fail`** — currently any value; pairs with tagged unions.
- **Streaming `link`** — long-lived connections.
- **Named-field struct construction** — v1 is positional only.
- **Full type inference / generic substitution** — checker stays silent when uncertain rather than false-reject.
- **AST source spans** for type errors with line:col.
- **Rust as a target** — ownership model would reshape the core (§3).

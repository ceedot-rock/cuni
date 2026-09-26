# CuNi is 144 languages

Exactness applies to the whole catalog. A language in `src/langs.rs` is a seat: emit, run, identical stdout — or refuse.

## Native seats (toolchain of that language)

| id | compiler / runtime | backend |
|----|--------------------|---------|
| py | python3 | quality Python |
| go | go run | quality Go |
| js, ts | node | quality JavaScript |
| c | gcc | tagged `Val` C runtime |
| cpp | g++ | same lowering, compiled as C++ |
| rs | rustc | tagged `Val` Rust runtime |
| rb | ruby | quality Ruby |
| lua | lua5.4 | quality Lua |
| sol | solc | Solidity contract writer (compile-verified; events, no stdout) |

`cuni check examples/full.cuni --only py,go,js,c,cpp,rs` is the native gate. It must PASS.

## The rest of the 144

Until a seat has a native backend, its artifact is a Python lowering so the language **still emit+runs** under `cuni check`. The receipt (`--receipt`) marks each id `native` or `lowering`. Lowering is a seat with a shared runtime, not a skip.

Next native seats to grow under the same law: `java`, `cs`, `rb`, `php`, `lua`
(most-used real languages still on lowerings).

## Commands this unlocks

```bash
cuni run file.cuni                      # in-process (no emit)
cuni run file.cuni --lang py            # emit+run one native seat
cuni check file.cuni --receipt          # ledger beside the source
cuni check file.cuni --only c,rs,py     # pin the run to seats
cuni ingest impl.py -o impl.cuni        # reverse: CuNi-shaped subsets → CuNi or refuse
cuni prove file.cuni --against impl.py  # foreign code must match CuNi gold
```

## Two-way: ingest subsets (exact, or refuse)

`cuni ingest` accepts only code shaped like CuNi's own emitters produce, per seat:

| seat | accepted subset |
|------|-----------------|
| py | typed/untyped `def` with `return`, `if`/`else`, `let`-style bindings, top-level `print`/`say`, `def main():` unwrapped; prelude (`say`, `range`, `abs`, …) skipped; int/str/bool/float |
| go | `func name(p T, …) T` with `return`/`if`/`else`/`:=`/`say` inside `func main()`; int/string/bool/float64 map; generics, `(T, error)`, loops, `panic` refuse |
| js/ts | `function name(p, …)` with `return`/`if`/`else`, `const`/`let` bindings, `say(e);` inside `function main()`; erased types are recovered from call sites + return expressions (two-pass inference); unrecoverable → refuse |
| c/cpp | `static Val name(Val p, …)` with `return`, `if`/`else`, `Val x = …`, `cuni_say(…)` inside `int main(void)`; the `cuni_*`/`V_*` helpers map back; erased types recovered by call-site inference; unrecoverable → refuse |
| rs | `fn name(p: Val, …) -> Val` with `return`, `if`/`else`, `let [mut] x = …`, `cuni_say(…)` inside `fn main()`; `Val::Int/Str`, `v_*` helpers, `.clone()`/`.into()` map back; erased types recovered by call-site inference; unrecoverable → refuse |
| sol | `function name(p T, …) public [pure] returns (T)` with `return`, `if`/`else`, typed bindings, `emit LogInt/Str/Bool(e)` inside `run()`; int256/uint256/string/bool map; `revert("…")` → fallible `fail`; `_cuni_itoa` helper skipped; float/list/map/opt/`??` refuse |
| awk | `function name(p, …)` defs + one `BEGIN` main; `print expr` (single arg), bare `x = expr` bindings, `return`, `if`/`else`; types by call-site inference; pattern-action rules, `printf`, arrays refuse |
| pl | `sub name { my ($p, …) = @_; … }` defs; `my $x = expr` / `$x = expr`, `print EXPR, "\n"` / `say(EXPR)`, `return`, `if`/`else`/`unless`; types by call-site inference; regexes, interpolation, `.` concat, loops refuse |
| sh | top-level only: `x=value`, `x=$((expr))`, `"$var"` interpolation, `echo`, `if [ … ]; then/elif/else/fi` with `-gt/-lt/-ge/-le/-eq/-ne`/`=`/`!=`; functions, loops, `$( )`, pipes refuse |
| sql | `SELECT <expr>;` → `say(<expr>)`; `'...'` strings, `=`/`<>`, `AND`/`OR`/`NOT` mapped; `FROM`, joins, DDL refuse |
| wat | `(func $n (param $p T)* (result T) <single expr>)` with `T` in i32/i64/f32/f64; const/local.get/call/add/sub/mul/signed+float comparisons; `(start $f)` → top-level call; locals, `if`, div/rem, memories refuse |
| other 132 | Python lowerings only: the CuNi `#` header is required, then the py subset above; headerless files refuse |

Every ingester ends with a self-check: the produced `.cuni` must lex, parse, and typecheck in the real front-end, or ingest refuses. Skipped prelude helpers that are still referenced therefore refuse instead of producing a broken program. Types erased by a backend (js/ts untyped params, c/cpp/rs `Val` boxing, awk/perl dynamic) are recovered by two-pass call-site inference: parameter types from every call site, return types from return expressions, bindings/literals/comparisons as evidence. When there are no call sites, mixed types, or no evidence at all, ingest refuses with a named reason instead of guessing (the old `untyped → int` default is gone).

`cuni run` is one seat. `cuni check` is the proof.

## Split across seats

`link` is still the process boundary: same function, Go server + Python client. `--only` pins *this check* to a subset of seats. Function-level `on go` syntax is next; until then the whole program is the citizen on every native seat.

## Agents

Law is CuNi. `examples/agent/host/run_agent.py` runs `cuni check` before effect. `--skip-check` is not a citizen.

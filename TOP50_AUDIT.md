# CuNi Top-50 Audit — native vs Python-lowered (2026-10-02)

`--emit-top50` boundary confirmed: `src/main.rs:955` — `langs::LANGS.iter().take(50)`.
Catalog total: 144 entries (`src/langs.rs`).

## Current native seats (12)

Real `codegen_*.rs` emitter + real toolchain + `SeatKind::Native` in `src/emit.rs`:

| # | id | Language | Toolchain |
|---|----|----------|-----------|
| 1 | py | Python | python3 |
| 2 | go | Go | go |
| 3 | js | JavaScript | node |
| 4 | ts | TypeScript | node (via codegen_js) |
| 5 | c | C | gcc |
| 6 | cpp | C++ | g++ |
| 8 | java | Java | javac/java |
| 12 | rs | Rust | rustc |
| 13 | rb | Ruby | ruby |
| 15 | lua | Lua | lua5.4 |
| 38 | sql | SQL | sqlite3 |
| 40 | sol | Solidity | solc (compile-only, no stdout) |

## Top-50 Python-lowered seats → native candidates

| # | id | Language | Toolchain (verify) | Verdict |
|---|----|----------|-------------------|---------|
| 7 | cs | C# | dotnet or mono | CANDIDATE |
| 9 | kt | Kotlin | kotlinc (JVM) | CANDIDATE |
| 10 | scala | Scala | scalac (JVM) | CANDIDATE |
| 11 | swift | Swift | swift (Linux) | CANDIDATE |
| 14 | php | PHP | php | CANDIDATE |
| 16 | pl | Perl | perl | CANDIDATE |
| 17 | r | R | Rscript | CANDIDATE |
| 18 | jl | Julia | julia | CANDIDATE |
| 19 | ex | Elixir | elixir | CANDIDATE |
| 20 | erl | Erlang | erl/escript | CANDIDATE |
| 21 | hs | Haskell | ghc/runghc | CANDIDATE |
| 22 | ml | OCaml | ocaml | CANDIDATE |
| 23 | fs | F# | dotnet fsi | CANDIDATE |
| 24 | lisp | Common Lisp | sbcl | CANDIDATE |
| 25 | clj | Clojure | clojure (JVM) | CANDIDATE |
| 26 | dart | Dart | dart | CANDIDATE |
| 27 | zig | Zig | zig | CANDIDATE |
| 28 | nim | Nim | nim | CANDIDATE |
| 29 | cr | Crystal | crystal | CANDIDATE |
| 30 | d | D | dmd/ldc2/gdc | CANDIDATE |
| 31 | v | V | v (vlang) | CANDIDATE |
| 32 | ada | Ada | gnat | CANDIDATE |
| 33 | pas | Pascal | fpc | CANDIDATE |
| 34 | f90 | Fortran | gfortran | CANDIDATE |
| 35 | cob | COBOL | gnucobol (cobc) | CANDIDATE |
| 36 | m | MATLAB | **none free** (proprietary) | BLOCKED — no free MATLAB toolchain exists; Octave is a separate seat (#89) |
| 37 | pro | Prolog | swipl | CANDIDATE |
| 39 | asm | Assembly | as/gcc (x86-64) | CANDIDATE (emit real asm, non-trivial) |
| 41 | groovy | Groovy | groovy (JVM) | CANDIDATE |
| 42 | m-objc | Objective-C | gcc -x objective-c (GNUstep/Foundation optional) | CANDIDATE |
| 43 | vb | Visual Basic | **none on Linux** (classic VB has no free Linux toolchain) | BLOCKED |
| 44 | sh | Bash | bash | CANDIDATE |
| 45 | ps1 | PowerShell | pwsh (Microsoft repo) | CANDIDATE |
| 46 | awk | Awk | awk/mawk/gawk | CANDIDATE |
| 47 | tcl | Tcl | tclsh | CANDIDATE |
| 48 | st | Smalltalk | gst (GNU Smalltalk) | CANDIDATE |
| 49 | hx | Haxe | haxe --interp | CANDIDATE |
| 50 | hack | Hack | hhvm (heavy) | CANDIDATE (toolchain heavy; may defer) |

**Counts:** 12 native already · 36 candidates with real toolchains · 2 blocked (MATLAB proprietary, VB no Linux toolchain).

## Emitter contract (new native seats)

Each new `src/codegen_<id>.rs` implements the **core subset** and honestly refuses the rest:
- SUPPORTED: `say` (int/str/bool), `let`/`mut` (int/str/bool), `def`/`ret`
  (monomorphic, int/str/bool), `if`/`else`, `while`, int arithmetic `+ - *`,
  int `/` (truncate toward zero) and `%` (Python-floored) via helpers where
  the language differs, comparisons, `and`/`or`/`not`, str literals +
  concatenation, int/str/bool params and returns.
- REFUSED (Err, never approximated): `float` literals/ops (unless the
  language provably prints shortest-round-trip like Python — evaluated per
  seat), `typ`, `enum`, `iface`, generics, `opt`/`none`, fallible `?` /
  `fail` / `??`, `link`, `ext`, `dec`, `time`, lists, maps, and everything else.
- Verification: `cuni check --only <id>` over `tests/fixtures-core/*.cuni`
  must show byte-identical stdout vs the py gold seat.

Rationale: a seat that refuses honestly is a real seat; a seat that
approximates is a lie. The gate already tolerates refusal (`143/144 ok`
semantics in `tests/check_cmd.rs`).

## Build results (2026-10-02)

### Newly native seats — verified 8/8 on core fixtures (byte-identical to py gold)

| # | id | Language | Toolchain | Evidence |
|---|----|----------|-----------|----------|
| 14 | php | PHP | php-cli | 8/8 fixtures pass |
| 17 | r | R | r-base-core (Rscript) | 8/8 fixtures pass |
| 16 | pl | Perl | perl | 8/8 fixtures pass |
| 24 | ml | OCaml | ocaml | 8/8 fixtures pass |
| 25 | lisp | Common Lisp | sbcl | 8/8 fixtures pass |
| 34 | f90 | Fortran | gfortran | 8/8 fixtures pass |
| 33 | pas | Pascal | fpc | 8/8 fixtures pass |

Core fixtures: `tests/fixtures-core/` (01-say, 02-arith, 03-vars, 04-compare,
05-if, 06-while, 07-def, 08-strings). Each seat's stdout compared byte-identical
to Python gold via `diff`.

### Honest refusals / deferred (not native)

| id | Language | Reason |
|----|----------|--------|
| clj | Clojure | Custom emitter has paren-balancing bug in multi-ret functions; deferred |
| ex | Elixir | Honestly refuses: immutable rebinding cannot model CuNi mut/while |
| hs | Haskell | Custom emitter has syntax bug; deferred |
| pro | Prolog | Honestly refuses: single-assignment, no loops/functions |
| tcl | Tcl | Spec emitted C-like syntax; broken, deferred |
| erl | Erlang | Infeasible: single-assignment vs mut/while (subagent verdict) |
| objc | Objective-C | Not distinct from C; keep C-only (subagent verdict) |
| asm | Assembly | Infeasible: would require mini-runtime (subagent verdict) |

### Not attempted (stay Python-lowered)

sh, awk, kt, scala, zig, ada, v, cs, fs, hx, hack, dart, jl, swift, groovy,
cr, nim, d, ps1, cob — emitters not written or toolchains not available.
They remain in the catalog as Python-lowered seats.

### Blocked (no toolchain anywhere)

| id | Language | Reason |
|----|----------|--------|
| m | MATLAB | Proprietary; no free toolchain |
| vb | Visual Basic | No Linux toolchain |

## Final counts

- **Native seats**: 19 (12 original + 7 new)
- **Python-lowered**: 94 (113 total − 19 native)
- **Removed**: 31 (see REMOVED_FAT.md)
- **Catalog total**: 113 (was 144)

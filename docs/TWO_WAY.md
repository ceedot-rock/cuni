# CuNi two-way status — 2026-09-23

"Input any, output any." Every seat below is measured, not claimed.

## Native ingest seats (12)

| seat | emit | ingest | exactness | refusal boundary |
|------|------|--------|-----------|------------------|
| py | native (python3) | native, typed + untyped defs | ✅ `cuni check` PASS | non-CuNi-shaped code refuses |
| go | native (go 1.24.7, installed 2026-09-23) | native, typed `func` | ✅ `cuni check` PASS | generics, `(T, error)`, loops, `panic` refuse |
| js | native (node) | native, call-site type inference | ✅ `cuni check` PASS | uninferrable types refuse (no `int` default) |
| ts | native (node) | native, call-site type inference | ✅ `cuni check` PASS | same as js |
| c | native (gcc, `Val` runtime) | native, call-site type inference | ✅ `cuni check` PASS | uninferrable types refuse |
| cpp | native (g++, `Val` runtime) | native, call-site type inference | ✅ `cuni check` PASS | same as c |
| rs | native (rustc, `Val` runtime) | native, call-site type inference | ✅ `cuni check` PASS | uninferrable types refuse |
| awk | lowering (python3) | native: `function` + `BEGIN`, `print`, bare `x = e`, inference | ✅ rt harness + `cuni check` PASS | pattern-action rules, `printf`, arrays refuse |
| pl | lowering (python3) | native: `sub` + `my ($p) = @_`, `print/say`, inference | ✅ rt harness + `cuni check` PASS | regexes, interpolation, `.` concat, loops refuse |
| sh | lowering (python3) | native: top-level `=`/`$(( ))`/`echo`/`if [ ]` | ✅ rt harness + `cuni check` PASS | functions, loops, `$( )`, pipes refuse |
| sql | lowering (python3) | native: `SELECT <expr>` → `say` | ✅ rt harness + `cuni check` PASS | `FROM`/DDL refuse |
| wat | lowering (python3) | native: typed `(func …)` expr bodies | ✅ front-end + def-shape (no I/O in subset) | locals, `if`, div/rem, memories refuse |

Bool printing is canonical `True`/`False` on **all** native backends (js/go were
normalized 2026-09-23; py/c/cpp/rs already matched). `cuni check` on the
str+bool RT2 corpus: **PASS (7/7 native seats)**.

## Header-gated lowering seats (8)

The remaining 8 seats emit a Python lowering (runs under python3) and ingest
**only** artifacts carrying the CuNi `#` lowering header, via the Python
subset. Headerless foreign files refuse with a named reason. Verified by
`ingest::tests::rt1_all_langs`, which round-trips every seat that has an
ingest parser (seats without one are skipped by name in the test).

Next candidates for native ingest (most-used real languages, still lowering):
`java`, `cs`, `rb`, `php`, `lua`.

## Laws

1. Subset-or-refuse: every ingester documents its accepted subset; anything
   outside refuses with a named reason. No silent guessing (the old
   `untyped → int` default is removed).
2. Every produced `.cuni` must lex, parse, and typecheck in the real CuNi
   front-end (`self_check`), or ingest refuses.
3. Type-erased backends (js/ts/c/cpp/rs/awk/pl) recover types by two-pass
   call-site inference; unrecoverable → refuse, never default.
4. Nothing is committed, pushed, or published until Corey approves release.

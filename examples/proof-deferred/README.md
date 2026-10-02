# Proof: deferred seats

The 0.9.0 changelog briefly listed Clojure, Elixir, Haskell, Prolog,
Erlang, and Tcl as "deferred" (emitter bugs / infeasible / broken spec).
That was wrong — all six were already working native seats. This proof
pins it: `core.cuni` exercises the full core subset every native seat
must honor, and `tests/proof_deferred.rs` runs the exactness gate on
each seat through its own real toolchain:

| Seat | Toolchain |
|------|-----------|
| clj (Clojure) | `clojure` |
| ex (Elixir) | `elixir` |
| hs (Haskell) | `runghc` |
| pro (Prolog) | `swipl` |
| erl (Erlang) | `escript` |
| tcl (Tcl) | `tclsh` |

Covered: `say`/`let`/`mut`, `def`/`ret` (including recursion — `fib`),
`if`/`els`, `whl`, integer `+ - *` with truncating `/` and
Python-floored `%` (including negative operands), comparisons,
`and`/`or`/`not`, unary minus, string concatenation and escapes
(newlines, quotes, `$`, backslashes, tabs), nested calls, nested
conditionals. All seven tests byte-identical to the Python gold.

Anything beyond the core subset (floats, lists, `typ`, `for`, …)
refuses honestly — never approximated.

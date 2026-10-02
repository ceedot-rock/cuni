# examples/onchain — Onchain Division demo fixtures (CuNi 0.8.0)

One law, two chains. Each directory holds the same demo fixture, stated once
in CuNi and emitted per chain:

- `clarity/demo.cuni` → `cuni compile demo.cuni --emit-clarity demo.clar`
  (Stacks Clarity contract: `define-private` logic helpers + `define-public`
  `(ok ...)` shell)
- `cadence/demo.cuni` → `cuni compile demo.cuni --emit-cadence demo.cdc`
  (Flow Cadence: `access(all) contract` with `access(self)` logic helpers)

The fixtures exercise the v1 onchain subset (`int`, `dec`, `bool`, `str`,
`def`/`if`/`ret`, comparisons, arithmetic, `say`). `dec` is a scaled integer
(scale 10^4, truncation toward zero) on both chains — deliberately not
Cadence's native `Fix64`.

Gated by `tests/proof_clarity.rs` and `tests/proof_cadence.rs`: pinned
interpreter verdicts, contract shape, golden snapshots
(`tests/snapshots/`), a Python reference gate (`--emit-clarity-ref` /
`--emit-cadence-ref`, run with `python3`, byte-identical to `cuni run`
gold), the `examples/finance/fee_schedule.cuni` money bridge, refusal
tests, and a toolchain-honesty check (the Clarity/Flow toolchains are not
present on the check machine, so the contract shells are not compiled here
— see `docs/ONCHAIN.md`).

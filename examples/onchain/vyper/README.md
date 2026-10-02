# CuNi → Vyper (EVM) demo

`demo.cuni` is the onchain demo law: `int` + `dec` + `bool` + `str`,
`def`/`if`/`ret`, comparisons, arithmetic, `say` — the exact subset the
Vyper seat proves.

```sh
# Full Vyper contract (logic core + @external shell):
cuni compile examples/onchain/vyper/demo.cuni --emit-vyper demo.vy

# Standalone Python reference of the pure logic core + driver:
cuni compile examples/onchain/vyper/demo.cuni --emit-vyper-ref demo_ref.py
python3 demo_ref.py   # byte-identical to `cuni run demo.cuni`
```

Exactness notes: CuNi `int` is Vyper `int256`; CuNi `dec` is `int256`
scaled 10⁴ (docs/DECIMAL.md) — deliberately NOT Vyper's native `decimal`
(scale 10¹⁰). The logic core is `@internal @pure` functions
(underscore-prefixed, the Vyper docs' own pattern); the shell is one
`@external @pure` wrapper per `def`. The shell is not compiled here (no
Vyper toolchain on the check machine) and nothing has executed on-chain.

Honest refusals: `float`, `list`, `map`, `opt`, `??`, structs, enums,
`time`, string concat, interpolated strings in the contract, and `say`
inside function bodies. See `src/codegen_vyper.rs` and `tests/proof_vyper.rs`.

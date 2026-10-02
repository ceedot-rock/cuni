# CuNi → Cairo (Starknet) demo

`demo.cuni` is the onchain demo law: `int` + `dec` + `bool` + `str`,
`def`/`if`/`ret`, comparisons, arithmetic, `say` — the exact subset the
Cairo seat proves. It is shared verbatim with
`examples/onchain/vyper/demo.cuni`: one law, two chains.

```sh
# Full Cairo contract artifact (logic core + #[starknet::contract] shell):
cuni compile examples/onchain/cairo/demo.cuni --emit-cairo demo.cairo

# Standalone Python reference of the pure logic core + driver:
cuni compile examples/onchain/cairo/demo.cuni --emit-cairo-ref demo_ref.py
python3 demo_ref.py   # byte-identical to `cuni run demo.cuni`
```

Exactness notes: CuNi `int` is Cairo `u256` — **negative literals are
refused at emit** (documented in `src/codegen_cairo.rs`); CuNi `dec` is
`u256` scaled 10⁴ (docs/DECIMAL.md), truncated toward zero, negative
decimals refused. The shell is not compiled here (no Cairo/Scarb toolchain
on the check machine) and nothing has executed on-chain.

Honest refusals: negative literals, `float`, `list`, `map`, `opt`, `??`,
structs, enums, `time`, `fail`, string concat, interpolated strings in the
contract, and `say` inside function bodies. See `src/codegen_cairo.rs` and
`tests/proof_cairo.rs`.

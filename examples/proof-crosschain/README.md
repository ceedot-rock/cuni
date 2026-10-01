# Proof: cross-chain exactness for one escrow validation law

`escrow.cuni` states a transfer-validation rule **once**, in CuNi, and the
exactness gate proves it runs identically everywhere that rule needs to
live: on EVM (Solidity), on a Solana-style Rust program, on a Cosmos-style
Go program, and on Python. No rewrites per chain, no "trust us, it's the
same logic" — the compiler emits each target's code and each target's own
toolchain runs it.

## What the gate proves

- **Behavioral exactness (rs, go, py):** `cuni check --only rs,go,py`
  compiles the fixture with each target's real toolchain (`rustc`, `go run`,
  `python3`) and requires **byte-identical stdout** — exit 0 prints
  `exactness: PASS`. If any seat prints even one byte differently, the gate
  fails instead of shipping.
- **Deployability (sol):** `cuni check --only sol` emits a real Solidity
  contract and compiles it with **solc 0.8.28** to deployable bytecode. A
  contract has no stdout, so successful compilation *is* the verification
  for this seat — the same rule you deploy on-chain.
- **Integer-exact math:** the validator uses only integer arithmetic
  (basis-point fee = `amount * fee_bps / 10000`, truncated division). CuNi's
  `int` divides exactly like Solidity's `int256`, so the fee computation
  cannot drift between chains. No floats, no rounding modes to argue about.

## The validation law (status codes, exact integers)

| Code | Meaning |
|------|---------|
| 0 | OK — transfer valid, fee exact |
| 1 | INSUFFICIENT — `amount + fee` exceeds balance |
| 2 | BAD_AMOUNT — amount must be > 0 |
| 3 | BAD_FEE_RATE — `fee_bps` must be within 0..10000 |

Integer codes, not error strings: strings diverge across chains, codes
don't. The driver prints six fixed cases (valid, fee math, insufficient
balance, zero amount, out-of-range fee rate, exact-boundary balance), so the
verdicts are pinned and any drift is visible.

## Honest scope

- solc proves the contract **compiles to deployable bytecode**; it does not
  execute it on a chain.
- The rs/go/py stdout gate proves **behavioral exactness** of the same
  logic off-chain.
- **Out of scope:** on-chain execution, gas metering, and reentrancy-style
  concerns — this gate is about the *validation law* being provably the
  same law everywhere, not about everything a chain can do to a contract.

Run it: `cargo test --test proof_crosschain`

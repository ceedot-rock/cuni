---
name: cuni
description: Write a contract once; it compiles to identical behavior on every runtime or refuses to run. Use when exact, reproducible computation matters across languages or chains.
version: 1.0.0
metadata:
  author: Slid Phi Labs
  repo: https://github.com/ceedot-rock/cuni
---

# CuNi

A contract language with one rule: the code must behave exactly the same on
every machine it runs on, or it does not run at all. No silent differences
between runtimes, no surprises.

53 catalog seats: 45 native plus 8 lowerings (5 Python: swift, m, vb, st, hack;
3 onchain: vy, move, cairo). Agent Rider uses CuNi `link` contracts as its
standard interop mechanism and requires exactness before a policy can run.

## Install

```bash
cargo add cuni    # 0.8.0 on crates.io
```

## Check exactness

```bash
cuni check path/to/contract.cuni
```

`cuni check` compiles the contract for every catalog seat and compares stdout
byte-for-byte. PASS means identical behavior everywhere. Any divergence is a
refusal, not a warning.

## Minimal example

```cuni
// dice.cuni — deterministic roll, same result on all 53 seats
fn roll(seed: u64) -> u64 {
    let state = (seed * 1103515245 + 12345) % 2147483648;
    (state % 6) + 1
}
```

```bash
cuni check dice.cuni
# PASS on all runnable seats — or it refuses
```

## Links

- Repo: https://github.com/ceedot-rock/cuni
- Crate: https://crates.io/crates/cuni

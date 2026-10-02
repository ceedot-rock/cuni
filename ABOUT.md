# CuNi — Trust Provable, in all things.

## What it is

CuNi is a programming language with one law: **the same output on every
target, or refuse.** Write once in `.cuni`. It runs exact on 113 languages
across 12 native seats — Python, Rust, Go, JavaScript, TypeScript, C, C++,
Ruby, Lua, Java, SQL, Solidity — or it refuses to run at all. No
approximate mode. No silent rounding. No drift.

Money gets exact math (`dec`, fixed scale 10⁴, scaled integers — never
float) and exact time (`time`, int64 unix timestamps, UTC only). The same
source emits contracts for six chains: ink!, Move, Vyper, Cairo, Clarity,
Cadence — plus Solana and Solidity.

## How to use it

```sh
cargo install cuni          # 0.8.0 on crates.io

cuni check law.cuni         # prove it exact across the seats
cuni run law.cuni            # run it
cuni audit law.cuni --against bank_impl.py   # signed receipt: PASS or REFUSE
cuni --emit-vyper law.cuni   # emit for a chain
```

Three divisions, one law:

- **Code** — the machine. Exact types, stdlib, 12 native seats.
- **Financial** — `cuni audit`. Hand it a financial law and an
  implementation, get back a signed receipt. The disagreement itself is
  fileable. Hosted verification at $0.10/check.
- **Onchain** — one law written once, emitted for six chains,
  byte-identical numbers everywhere.

Emission is free. Proof is the product.

## What we're about

Slid Phi Labs builds computation you don't have to trust — because you can
prove it. Every claim we ship is gated: tests green, numbers verified,
refusals loud. We don't do approximate. We don't do mock. What's live is
real, what's unproven is labeled, and what can't be exact refuses.

Forget trust. Prove.

---

Slid Phi Labs accepts donations to keep the lab independent:
https://www.patreon.com/SlidPhiLabs

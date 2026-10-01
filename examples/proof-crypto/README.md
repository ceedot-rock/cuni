# proof-crypto: the crypto conformance profile

**One digest, written once, proven identical in three languages.** That is the
whole pitch: instead of trusting that "the same" cryptographic code behaves
the same in Python, Go, and Rust, this profile *proves* it — every run, on
every machine, byte-for-byte.

## What it is

`digest.cuni` expresses an FNV-1a 32-bit reference digest exactly once, in
CuNi. Running

```
cuni check examples/proof-crypto/digest.cuni --only rs,go,py
```

compiles that one source into three genuinely independent implementations
(Python, Go, Rust), executes each with its own toolchain, and requires
byte-identical stdout. `exactness: PASS` means three separate codebases
agree on every digest output. Anything less is a FAIL, loudly.

The program also includes a toy RSA signature verification
(`powmod(588, 17, 3233) == 65` on the textbook n=3233 key), showing that
modular exponentiation — the core operation of signature verification —
survives the same trip.

## Why this matters

Multi-implementation cryptography is where subtle bugs become
vulnerabilities: one language wrapping an integer where another extends it,
one `%` flooring where another truncates, and suddenly two "identical"
verifiers disagree — and the disagreement is exploitable. This profile turns
that fear into a checked property. The #1 risk, integer semantics, is handled
explicitly in the source rather than assumed:

- CuNi has no bitwise operators in its portable core, so the 32-bit XOR is
  rebuilt from pure arithmetic (`%`, `/`, `*`, `+`, `-`) over non-negative
  integers, where division and modulo mean the same thing on all three seats.
- Every intermediate value stays in `[0, 2^32)` — the largest product is
  `(2^32 − 1) × 16777619 = 72057594037993477`, far below 2^63 − 1 — so Go's
  and Rust's fixed 64-bit ints can never overflow, and Python's
  arbitrary-precision ints agree exactly.
- The 32-bit wraparound is written as an explicit `% 4294967296` mask in the
  source, so the agreement is by construction, not by hope.

Three fixed test vectors carry independently computed expected values
(`python3` one-liner, not the CuNi compiler): `"hello"` → `1335831723`,
`""` → `2166136261`, `"foobar"` → `3214735720`. Each vector is asserted
in-source — a seat computing any other digest prints `-1` and the gate fails.

## Honest scope

- This is a **reference digest for conformance demonstration**, not a
  recommendation to roll your own crypto. FNV-1a is not a cryptographic hash;
  nothing here should protect real data.
- The RSA piece is **verification only**, on a toy 12-bit textbook key with
  no padding. It demonstrates portability of modular exponentiation, not a
  deployable signature scheme.
- Inputs are explicit byte arrays (CuNi's portable core has no
  char-to-codepoint operation) — which is also the honest way to spec a
  digest: no hidden text-encoding step for implementations to disagree about.
- Two real codegen sharp edges were found while building this and worked
  around in the source (not hidden): the Rust seat emits parameter names
  verbatim, so a parameter named `mod` breaks compilation; the Go seat types
  an unannotated `[]` as `[]any`, so the empty vector needs an explicit
  `list<int>` annotation. Both are documented in comments at the site.

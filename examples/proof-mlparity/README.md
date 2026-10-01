# Proof: ML inference parity (`proof-mlparity`)

**What this proves:** one scoring kernel — written once in CuNi — runs
**bit-identical** on three different runtimes (Python, Rust, C). Before you
serve a model, you can catch silent numeric drift between the language you
trained in and the language you deploy in.

**What the kernel does** (`kernel.cuni`): it performs a small version of the
scoring step at the heart of an inference pass — a 4×4 integer matrix
multiply `C = A @ B` (standing in for a logits layer), followed by an
`argmax` over the first row of `C` (standing in for top-1 class selection).
Fixed inputs, integer arithmetic only. The program prints the 4×4 result
matrix and the winning index.

**How to run the gate yourself:**

```
cuni check examples/proof-mlparity/kernel.cuni --only py,rs,c --timeout 120
```

Exit 0 with `exactness: PASS` means Python, Rust, and C all emitted and ran
the same kernel and produced byte-identical stdout. Exit 1 means a real
divergence was found — that is the gate working, not failing.

## The honest scoping note (read this)

This kernel uses **integer arithmetic only**, and that is deliberate. Exact
floating-point results across different languages and toolchains is a
genuinely hard problem: compilers reorder operations, some chips fuse a
multiply-add into one step while others do it in two, and each language's
math library can round edge cases differently. Any of those can change the
last bit of a float result with no warning.

CuNi's exactness gate has no "close enough" mode — it demands byte-identical
output or it refuses. So a float kernel here would be **refused rather than
faked**: we will not ship a proof we cannot honestly guarantee. Integer
arithmetic is fully deterministic everywhere, so the integer kernel carries
the real proof: the same program, the same numbers, every runtime agreeing
down to the byte.

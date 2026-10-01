# CuNi `dec` — exact decimal type (spec v1.0)

> **Trust Provable, in all things.** — CuNi mission line (supersedes "Replace trust with proof" as top-level framing; the 0.4.0 line stays as supporting history).
>
> This document specifies the Code Division's `dec` type: the principled answer to float refusal. CuNi refuses floats on principle (no binary floating point anywhere in the portable core); `dec` is what you use instead when you need fractions — money math, fees, splits, rates.

## 1. What `dec` is

`dec` is a **fixed-point decimal** with **scale 10⁴ (4 fractional digits)**, stored on every seat as a **scaled integer**:

```
stored = trunc(value × 10000)        # at literal scale time, exact
value  = stored / 10000              # exact rational
```

**Why 4, not 18:** an int64 seat (Go, Lua 5.4, SQLite) holding scale-18 decimals tops out at ±$9.22 — unusable for money math, so every money literal would *refuse* on three native seats. At scale 10⁴ an int64 seat holds **±$922,337,203,685,477.5807** (±922 trillion), cents are exact, basis points are native (0.0001 = 1bp), and 4-place FX quotes (1.0854) are exact. Crypto-wei precision (18dp) is a wide-seat affair and stays out of the portable core. The scale is a language constant: every seat implements the same 10⁴, or it refuses.

## 2. Literals

Syntax: `<digits>[.<digits>]dec` — e.g. `19.99dec`, `0.0001dec`, `100dec`, `0dec`.

- At most **4 fractional digits**. More digits → **compile-time refusal**: ``dec literal `1.23456dec` has 5 fractional digits; scale is 4 — refusing``. No silent rounding, ever.
- Trailing zeros are insignificant: `1.50dec == 1.5dec`.
- Negative via unary minus: `-1.23dec` (the `-` is the normal negation operator).
- A literal whose scaled value exceeds the seat's integer range is refused **at emit time** on that seat (see §7).

## 3. Arithmetic — `(dec, dec) -> dec`

| op | definition (exact rational, then) | rule |
|----|-----------------------------------|------|
| `a + b` | `a + b` on scaled integers | refuse on true overflow (narrow seats) |
| `a - b` | `a - b` on scaled integers | refuse on true overflow (narrow seats) |
| `a * b` | `trunc(a·b / 10000)` | **truncation toward zero**; refuse on true overflow |
| `a / b` | `trunc(a·10000 / b)` | **truncation toward zero**; division by zero fails loudly on every seat |
| `a % b` | — | **refused**: `` `%` is not defined on `dec` `` (typeck) |

**Rounding rule (locked): truncation toward zero**, applied to the exact rational result. This is the same rule as CuNi `int` `/` (and Solidity's), so every seat's native signed integer division already does the right thing except Python/Ruby/Lua, which floor — those seats adjust (the existing `_cuni_div` pattern).

**Division by zero** is a loud runtime failure on every seat (panic / exception / revert / non-zero exit) — never a value, never silent.

## 4. Comparisons

`==  !=  <  <=  >  >=` compare the **scaled integers** directly (same scale ⇒ order-preserving). Result is `bool`. Mixed `dec`/`int` comparison is refused — convert explicitly (§5).

## 5. `dec`/`int` interop — explicit or refused

**There is no implicit conversion between `dec` and `int`.** Mixing them in arithmetic or comparison is a **typeck error** with a fix-it:

```
cannot mix `dec` and `int` with `+` — convert explicitly: `dec_of_int(n)` or `int_of_dec(d)`
```

Explicit conversions (builtins, available on every seat):

- `dec_of_int(n: int) -> dec` — exact `n × 10000`; refuses on narrow-seat overflow.
- `int_of_dec(d: dec) -> int` — `trunc(d / 10000)` **toward zero**: `int_of_dec(42.9999dec) == 42`, `int_of_dec(-42.9999dec) == -42`.

`abs`/`min`/`max` stay `int`-only (`dec` argument → the usual arity/type error). This is documented, not hidden.

## 6. Printing (`say`)

Canonical decimal rendering of the scaled integer, byte-identical on all seats:

```
sign + integer digits + "." + fractional digits with trailing zeros stripped
(fractional part never empty — `1.0000` prints as `1.0`)
```

Examples: `1.2300 → "1.23"`, `100.0000 → "100.0"`, `0.0001 → "0.0001"`, `-0.0005 → "-0.0005"`, `0 → "0.0"`.

## 7. Per-seat representation, range, and refusal matrix

"Wide" seats compute the full scaled range; "narrow" (int64) seats share one identical envelope so they behave identically: **|scaled| ≤ 9,223,372,036,854,775,807** (±$922T), add/sub/mul/div/neg refuse on true overflow, literals out of range refuse at emit.

| seat | representation | dec range | overflow / refusal posture |
|------|---------------|-----------|---------------------------|
| py (+132 lowerings) | `CuniDec(int)` subclass — operators overridden, exact bigint | unlimited | n/a (bigint) |
| rs | `Val::Dec(i128)` (tagged runtime) | ±1.7×10³⁴ | `checked_*` → panic `cuni: dec …` (loud refusal) |
| js / ts | `BigInt`, literals `12300n` | unlimited | n/a (BigInt); `/` truncates toward zero natively |
| c / cpp | `Val.d` as `__int128` (tagged runtime) | ±1.7×
...[truncated 4798 chars]
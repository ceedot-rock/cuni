# CuNi `time` — exact timestamp type (spec v1.0)

> **Trust Provable, in all things.** — CuNi mission line.
>
> This document specifies the Code Division's `time` type: the principled
> answer to "what time is it" in a portable exact language. A `time` is an
> **int64 unix epoch in seconds, UTC only** — no timezones, no daylight
> saving, no `now()`. It is the money-types pair with `dec`
> (docs/DECIMAL.md): `dec` prices the world, `time` dates it — settlement,
> vesting, expiries, rate windows.

## 1. What `time` is

`time` is an **exact instant**: seconds since 1970-01-01T00:00:00Z, stored
on every seat as a **signed 64-bit integer** (`i64`):

```
stored = seconds since the unix epoch, UTC     # 2026-10-01T21:30:25Z -> 1790890225
```

**Why int64 seconds, not millis, not a struct:** every native seat already
has an exact int64, so the envelope is identical everywhere —
**±9,223,372,036,854,775,807 seconds** (±292 billion years; the literal
syntax only reaches years 0001–9999, §2). There is no sub-second precision
in v1: fractional seconds are refused at the literal (§2), and durations
are plain `int` seconds (§3). A struct `{y, mo, d, …}` would invite
timezone-shaped bugs; a single integer cannot name a timezone.

**Why no `now()`:** wall-clock reads are non-deterministic — two seats
would print different stdout and the exactness gate would fail by
construction. CuNi programs take their instants as literals, arguments, or
`parse_time` of explicit strings. Determinism is the feature.

## 2. Literals

Syntax: `"YYYY-MM-DDTHH:MM:SSZ"t` — e.g. `"2026-10-01T21:30:25Z"t`.

- Strict ISO-8601 UTC, validated **once** in the parser (front-end): exactly
  20 characters, `-`/`T`/`:`/`Z` separators in place, digit groups, ranges
  (month 01–12, day 01–days-in-month with leap years, hour 00–23, minute and
  second 00–59), years 0001–9999.
- **Refused, loudly, never silently adjusted:** timezone offsets
  (`+02:00`), fractional seconds (`.5`), missing `Z`, leap second `60`,
  impossible dates (`2026-02-30`), year `0000`. Each refusal names the rule
  (docs/TIME.md §2).
- Negative via unary minus is meaningless on a literal (the parser already
  yields the epoch); pre-1970 instants are written directly:
  `"1969-12-31T23:59:59Z"t` is epoch `-1`, exact on every seat except sol
  (§7).
- The Wave-1 `time.epoch` / `time.parts` namespace (component-based,
  returning ints) is unchanged and coexists: the new `time` **type** lives
  in a different syntactic position (annotations, literals, operators).

## 3. Arithmetic — the closed world

| op | types | result | rule |
|----|-------|--------|------|
| `t + s` / `s + t` | (time, int) | time | exact; refuse on true overflow (narrow seats) |
| `t - s` | (time, int) | time | exact; refuse on true underflow (narrow seats) |
| `t - u` | (time, time) | **int** (seconds) | exact; truncates toward zero only via `days_between` |
| `-t` | time | time | negates the epoch |
| `t * s`, `t / s`, `t % s` | — | — | **refused**: `` `*` is not defined on `time` `` (typeck) |
| `t + u` | (time, time) | — | **refused**: a duration is a plain `int` of seconds; `time + time` has no meaning |

**No implicit time↔int conversion.** `t + 1.5`, `t + 1.0dec`, and `2 - t`
are typeck errors with fix-its. A duration is just an `int` — there is no
`duration` type in v1.

**Overflow** refuses loudly on narrow (int64) seats; wide seats (py, js/ts
BigInt) cannot overflow. Overflow is unreachable from literals alone (years
0001–9999 fit comfortably); it takes dynamic arithmetic near ±i64::MAX.

## 4. Comparisons

`==  !=  <  <=  >  >=` compare the **epochs** directly. Result is `bool`.
Comparisons are `(time, time)` only — comparing a time to an int is refused
(compare epochs explicitly if you mean it).

## 5. Builtins

- `parse_time(s: str) -> time` — strict §2 parsing of a **string** at run
  time. Bad input fails **loudly** on every seat (exception / panic /
  revert / non-zero exit) — never a silent value, never a default.
- `add_seconds(t: time, s: int) -> time` — checked `t + s`; refuses on
  overflow like the operator.
- `days_between(a: time, b: time) -> int` — whole days from `b` to `a`,
  **truncation toward zero**: `days_between(expiry, exec) == 91`,
  `days_between(exec, expiry) == -91`.

## 6. Printing (`say`) and interpolation

Canonical ISO-8601 UTC rendering of the epoch, byte-identical on all
seats, via Hinnant's civil-from-days (floor-correct for negative epochs):

```
YYYY-MM-DDTHH:MM:SSZ        # 1790890225 -> "2026-10-01T21:30:25Z"
```

A `time` never prints as its raw epoch integer — `say(t)` and `${t}` both
render the timestamp. Examples: `2000-02-29T12:00:00Z` (leap day),
`1969-12-31T23:59:59Z` (epoch −1), `9999-12-31T23:59:59Z`.

## 7. Per-seat representation, range, and refusal matrix

Every seat stores the int64 epoch; "wide" seats use unbounded integers and
cannot overflow, "narrow" (int64) seats share one identical envelope
(**|epoch| ≤ 9,223,372,036,854,775,807**) and refuse on true overflow.

| seat | representation | overflow / refusal posture |
|------|---------------|---------------------------|
| interp (native `cuni run`) | `Val::Time(i64)` | `checked_*` → loud refusal |
| py (+132 lowerings) | `CuniTime(int)` subclass — operators overridden, tag preserved | n/a (bigint); `int + time` works via `__radd__` |
| js / ts | `BigInt`, literals `1790890225n` | n/a (BigInt); `say`/interpolation route via codegen kind-tracking (both `time` and `dec` are BigInt at runtime) |
| go | `cuniTime int64` | checked add/sub/diff → `panic("cuni: … — refused")` |
| c / cpp | tagged `K_TIME`, `long long` field | checked ops → loud refusal |
| rs | `Val::Time(i64)` (tagged runtime) | `checked_*` → panic (loud refusal) |
| rb | `CuniTime < SimpleDelegator` with explicit `coerce`; `-@` keeps the tag | checked ops raise `TypeError`; `int + time` emits time-first (`Integer#+` cannot dispatch through `coerce`) |
| lua | boxed `CuniTime` metatable | checked helpers → `_cuni_time_refuse` (loud) |
| java | `JTy::Time` → `long` | `Math.addExact`-style checked ops → loud refusal |
| sql | `INTEGER` epoch; literals fold at emit with checked Rust arithmetic | dynamic `+`/`-` follow the seat's int posture (exact inside int64); `parse_time` folds string literals at emit, **dynamic strings refuse at emit** (no loud-refusal SQL form exists — SQLite resolves names at prepare time and never errors on bad values at runtime); `say` renders via `strftime('%Y-%m-%dT%H:%M:%SZ', …, 'unixepoch')` |
| sol | `uint256` epoch | **non-negative only**: negative literals refuse at emit; Solidity 0.8 checked arithmetic **reverts** on overflow (the loud refusal); `say`/interpolation emit `LogTime(_cuni_time_str(…))` |
| solana | — | **refuses**: time literals have no Solana-logic form (v1) |

## 8. Honest boundaries (v1)

- **No timezones, ever.** The type cannot name one; `Z` is the only
  accepted suffix.
- **No sub-second precision.** Fractional seconds are refused, not rounded.
- **No `now()`, no clock reads.** Determinism is load-bearing for the
  exactness gate.
- **No `duration` type.** Durations are plain `int` seconds.
- **`days_between` truncates toward zero** (like CuNi `int` `/`), it does
  not floor: −91, not −92, for a −91.4-day span.
- **sol is non-negative** (uint256); pre-1970 instants refuse at emit.
- **solana refuses** the type entirely in v1.
- **The SQL seat's `parse_time` needs a string literal**; dynamic strings
  are an emit-time refusal, documented above.
- The sol backend does not mangle identifiers: a CuNi variable named
  `days` (a Solidity time-unit keyword) will not compile — pre-existing
  backend behavior, not time-specific.

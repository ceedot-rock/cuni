# CuNi Standard Library — Wave 1

> **Trust Provable, in all things.** The stdlib is where the mission gets
> concrete: every function below is specified byte-for-byte, implemented on
> each seat that can be exact, and *refused* — loudly, with a reason — on
> every seat that can't. A refusal is a tested behavior, not a gap.
>
> (Code Division. "Replace trust with proof" remains the 0.4.0-era
> supporting line for the proof profiles; this document leads with the
> mission line.)

Wave 1 adds four families to the tiny core (`say`, `.push`, `.len`,
`range`, `abs`, `min`, `max`, `.slice`):

| Family | Functions |
|---|---|
| JSON | `json.parse(str) -> map`, `json.emit(map) -> str` |
| Time | `time.epoch(y, mo, d, h, mi, s) -> int`, `time.parts(epoch) -> map` |
| Strings | `s.split(sep) -> list<str>`, `sep.join(parts) -> str`, `s.trim() -> str`, `s.contains(sub) -> bool` |
| Hash | `sha256(s) -> str` |

`json` and `time` are **reserved namespace identifiers**: they cannot be
bound with `let`/`mut` or declared as functions — the compiler rejects the
binding with a clear error. (This keeps `json.parse` unambiguous on every
seat; the same reservation already exists in spirit for `say`/`range`.)

Two refusal kinds exist, and the tests pin both:

- **Emit-time refusal** — the seat cannot be exact (e.g. Solidity has no
  meaningful JSON). The codegen returns `Err`; `cuni check` reports
  `<seat> emit refused: <reason>`. Reason strings are documented in the
  matrix and asserted by the gate tests.
- **Runtime refusal** — the input violates the spec (e.g. `json.parse` of
  `1.5`). Every green seat exits nonzero with an error. The exact message
  text is *not* pinned across seats — only the nonzero exit is. A program
  that triggers a runtime refusal is not portable; the gate fixtures never
  do.

---

## 1. JSON — `json.parse` / `json.emit`

### Value model

JSON values map onto CuNi values exactly one way:

| JSON | CuNi |
|---|---|
| object | `map` (string keys) |
| array | `list` |
| string | `str` |
| integer (see §1.1) | `int` |
| `true` / `false` | `bool` |
| `null` | `none` |

### 1.1 Numbers: integers only, value-based

CuNi has no floats in its JSON surface. A JSON number literal is accepted
**iff its exact mathematical value is an integer** in the safe range

```
-(2^53 - 1) <= v <= 2^53 - 1      (±9007199254740991)
```

Anything else is a **runtime refusal**: `1.5`, `1e999`, `9007199254740993`
(out of range), `0.1`. Integer-valued spellings are accepted and normalize
to the integer: `1e3` → `1000`, `1.0` → `1`, `100e-2` → `1`, `-0` → `0`.

*Why value-based, not lexical:* JavaScript's `JSON.parse` cannot see
whether `1000` was spelled `1e3` — but it *can* see the value is integral.
A lexical rule ("reject any literal containing `.`/`e`") would make the js
seat diverge from the C seat on `{"a": 1e3}`. The value rule keeps all
eleven seats identical.

*Why ±(2^53−1):* it is the largest integer every seat represents exactly
(JS numbers are f64). Anything wider cannot round-trip through the js/ts
seats, so it is refused everywhere rather than silently rounded somewhere.

Reference algorithm (used by the hand-rolled seats; native-parser seats
pre-validate number tokens with the same rule):

```
parse_json_number(token):          # token matches -?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?
    sign = -1 if token[0] == '-' else 1
    int_digits, frac_digits, exp = split token
    digits = (int_digits + frac_digits) with leading zeros stripped
    if digits is empty: return 0            # "0", "0.0", "-0e5" …
    if len(digits) > 16: refuse             # >16 digits can't be in range
    D = int(digits); f = len(frac_digits)
    strip trailing zeros from D, decrementing f per zero stripped
    k = f - exp
    if k <= 0:  v = D * 10^(-k)             # checked multiply; overflow → refuse
    else:
        if k > 16: refuse                    # D < 10^k now, can't divide evenly
        if D % 10^k != 0: refuse             # not an integer value
        v = D // 10^k
    if abs(sign * v) > 9007199254740991: refuse
    return sign * v
```

### 1.2 `json.parse(s: str) -> map`

- `s` must be a JSON text whose **top-level value is an object**.
- Runtime refusals: malformed JSON; top-level array/string/number/bool/null
  (`json.parse("[1,2]")` refuses — the signature promises a map);
  unescaped control characters (< 0x20) inside strings; invalid `\u`
  escapes; **lone surrogates** (`"\uD800"` refuses — CuNi strings hold real
  Unicode, and a lone surrogate is not a character); non-integer or
  out-of-range numbers (§1.1); trailing garbage after the top-level value.
- Surrogate *pairs* (`"\uD834\uDD1E"`) decode to the character. `\/`
  decodes to `/`.
- Duplicate keys: the **last** occurrence wins; the key keeps its **first**
  insertion position. (Matches Python/Ruby/Go/JS native behavior.)
- Returns a CuNi `map` with string keys. Insertion order is first-seen
  order — but note §1.3: portable programs never depend on it.

### 1.3 `json.emit(m: map) -> str` — canonical minimal form

- Every key must be a string; a non-string key is a runtime refusal.
- Every value must be int/str/bool/none/list/map (nested); a float or any
  other value is a runtime refusal.
- Output is **canonical minimal JSON**:
  - No whitespace anywhere: separators are exactly `,` and `:`.
  - Object keys sorted in **UTF-8 byte order** (== Unicode code-point
    order) — *not* insertion order. This is the load-bearing decision: it
    makes `emit` independent of each seat's map iteration order (Go and Lua
    randomize it), so `parse → emit` is deterministic on all eleven seats.
  - Integers print as plain decimal (`-42`, no leading zeros).
  - `true`/`false`/`null` for bool/none. Arrays as `[v,v]`.
  - `{}` for the empty map, `[]` for the empty list.
- String escaping, exactly:
  - `"` → `\"`, `\` → `\\`
  - 0x08 → `\b`, 0x09 → `\t`, 0x0A → `\n`, 0x0C → `\f`, 0x0D → `\r`
  - any other byte < 0x20 → `\u00xx` with **lowercase** hex
  - every other code point passes through as UTF-8, **never** `\u`-escaped
    (so `é` stays `é`; Go's `encoding/json` would emit `\u00e9` and escape
    `<>&` — the Go seat hand-rolls its emitter for exactly this reason).
- Fixed point: for any accepted input, `emit(parse(emit(parse(s)))) ==
  emit(parse(s))`.

### Rendering maps portably

`say` on a bare map is **seat-divergent by design** (Python prints
`{'a': 1}`, the interpreter prints `{a: 1}`, Go randomizes key order).
Portable programs display maps with `json.emit`. The gate fixtures never
`say` a map directly.

---

## 2. Time — `time.epoch` / `time.parts`

Conversions only. There is **no `now()`** — wall-clock reads are
nondeterministic and unprovable, so they are not in the stdlib.

### The law

- **Proleptic Gregorian calendar**, no leap seconds: every day is exactly
  86400 seconds (POSIX/Unix semantics — leap seconds are ignored, the way
  Unix time does).
- Epoch `0` = `1970-01-01T00:00:00Z`. Negative epochs are pre-1970 dates.
- Valid years: **1–9999** (the range every seat's date logic can share;
  Python's `datetime` stops at 9999, so the spec stops there too).

### 2.1 `time.epoch(year, month, day, hour, min, sec) -> int`

Validates, then converts. Any violation is a runtime refusal:

- `1 <= year <= 9999`, `1 <= month <= 12`
- `1 <= day <= days_in(month, year)`; leap years: divisible by 4, except
  centuries not divisible by 400 (so 2000-02-29 is valid, 1900-02-29 and
  2026-02-29 refuse)
- `0 <= hour <= 23`, `0 <= min <= 59`, `0 <= sec <= 59`

Conversion (Howard Hinnant's civil-date algorithm; all divisions are on
non-negative operands, so truncating `/` == floor — identical integer
arithmetic on every seat, including pure-SQL):

```
days_from_civil(y, m, d):
    y0  = y - 1 if m <= 2 else y
    era = y0 // 400
    yoe = y0 - era * 400                 # [0, 399]
    mp  = (m + 9) % 12                  # [0, 11], March = 0
    doy = (153 * mp + 2) // 5 + d - 1   # [0, 365]
    doe = yoe * 365 + yoe // 4 - yoe // 100 + doy
    return era * 146097 + doe - 719468  # days since 1970-01-01

time.epoch(y, mo, d, h, mi, s) = days_from_civil(y, mo, d) * 86400
                               + h * 3600 + mi * 60 + s
```

Pinned values: `epoch(1970,1,1,0,0,0) = 0`,
`epoch(2000,1,1,0,0,0) = 946684800`,
`epoch(1,1,1,0,0,0) = -62135596800`,
`epoch(9999,12,31,23,59,59) = 253402300799`.

### 2.2 `time.parts(epoch) -> map`

The inverse. `epoch` outside
`[epoch(1,1,1,0,0,0), epoch(9999,12,31,23,59,59)]` refuses. Returns a map
with exactly the keys `year`, `month`, `day`, `hour`, `min`, `sec`
(inserted in that order), all ints. Inverse algorithm:

```
civil_from_days(z):                      # z = days since 1970-01-01
    z   += 719468                        # >= 306 in our domain: non-negative
    era  = z // 146097
    doe  = z - era * 146097              # [0, 146096]
    yoe  = (doe - doe//1460 + doe//36524 - doe//146096) // 365
    y    = yoe + era * 400
    doy  = doe - (365*yoe + yoe//4 - yoe//100)
    mp   = (5*doy + 2)//153
    d    = doy - (153*mp+2)//5 + 1
    m    = mp + 3 if mp < 10 else mp - 9
    return (y + (1 if m <= 2 else 0), m, d)

time.parts(e):
    days = floor(e / 86400); secs = e - days * 86400   # floor: secs in [0, 86399]
    (y, mo, d) = civil_from_days(days)
    return {year: y, month: mo, day: d,
            hour: secs // 3600, min: (secs % 3600) // 60, sec: secs % 60}
```

Round trip: `parts(epoch(y,mo,d,h,mi,s))` is the identity on valid inputs.

---

## 3. String ops — `split` / `join` / `trim` / `contains`

All four are **byte-oriented on UTF-8**: they never decode code points, so a
multi-byte character is never split or mis-measured. (Fixtures stay in
ASCII; the byte rule is what keeps C's `memmem` and Python's `str.split`
identical.)

### 3.1 `s.split(sep: str) -> list<str>`

- `sep == ""` is a **runtime refusal** (every seat's native split does
  something different with an empty separator — chars in JS, error in
  Python — so the spec refuses rather than pick a side).
- Otherwise: split on each non-overlapping occurrence of `sep`, keeping
  empty parts, including trailing ones — Python-with-explicit-separator
  semantics: `"a,b,".split(",") == ["a", "b", ""]`,
  `",".split(",") == ["", ""]`, `"abc".split(",") == ["abc"]`,
  `"".split(",") == [""]`.
- Seats whose native split drops trailing empties (Ruby) or takes a regex
  (Java) adjust: Ruby uses `split(sep, -1)`; Java uses
  `split(Pattern.quote(sep), -1)`.

### 3.2 `sep.join(parts: list<str>) -> str`

Method on the separator (Python's shape): `", ".join(["a", "b"]) ==
"a, b"`. Empty list → `""`. Any non-`str` element is a **runtime refusal**
(JS would stringify it, Python would raise — the spec refuses).

### 3.3 `s.trim() -> str`

Strips leading and trailing bytes in exactly this set — **ASCII whitespace
only**:

```
0x09 TAB, 0x0A LF, 0x0B VT, 0x0C FF, 0x0D CR, 0x20 SPACE
```

Unicode whitespace (U+00A0, U+2000…, U+FEFF) is *not* trimmed: Python's
`strip()`, JS's `trim()`, Java's `trim()` and Ruby's `strip` all disagree
there, so every seat hand-rolls the six-byte set (Python:
`s.strip(" \t\n\r\x0b\x0c")`, etc.).

### 3.4 `s.contains(sub: str) -> bool`

Byte-substring search. `s.contains("")` is `true` (every seat agrees).

---

## 4. SHA-256 — `sha256(s: str) -> str`

Deterministic by construction — a fixed algorithm is the most provable
thing in this document. Lowercase hex of SHA-256 over the UTF-8 bytes of
`s` (64 hex chars).

Pinned vectors: `sha256("") ==
e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`,
`sha256("abc") ==
ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad`.

---

## 5. Support matrix

✅ = green (gate-proven) · ❌ = emit-time refusal (tested, reason quoted)

| Function | py | go | js | ts | c | cpp | rs | rb | lua | java | sql | sol |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| `json.parse` | ✅ json module + lexical number pre-check | ✅ encoding/json + UseNumber + validation walk | ✅ JSON.parse + lexical pre-check + walk | ✅ (= js seat) | ✅ hand-rolled | ✅ (= c seat) | ✅ hand-rolled (rustc-direct: no serde_json possible; none added) | ✅ json stdlib + pre-check | ✅ hand-rolled | ✅ hand-rolled | ❌ "maps have no SQL form; refusing" | ❌ "no meaningful on-chain JSON; refusing" |
| `json.emit` | ✅ `dumps(sort_keys, separators, ensure_ascii=False)` | ✅ hand-rolled sorted emitter (encoding/json would `\u`-escape `<>&`, U+2028/29) | ✅ deep-sort + JSON.stringify | ✅ | ✅ hand-rolled | ✅ | ✅ hand-rolled | ✅ deep-sort + JSON.generate | ✅ hand-rolled | ✅ hand-rolled | ❌ | ❌ |
| `time.epoch` | ✅ Hinnant arithmetic | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ pure integer SQL (Hinnant) | ❌ "wall-clock-adjacent conversions have no on-chain form in wave 1; refusing" |
| `time.parts` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ returns a map; maps refused | ❌ |
| `split` | ✅ | ✅ strings.Split + empty-sep guard | ✅ + empty-sep guard | ✅ | ✅ hand-rolled | ✅ | ✅ + empty-pattern guard | ✅ split(sep, -1) + guard | ✅ hand-rolled (plain find) | ✅ Pattern.quote + limit -1 + guard | ❌ "dynamic-length lists have no static SQL form; refusing" | ❌ "wave-1 string ops have no on-chain form; refusing" |
| `join` | ✅ | ✅ | ✅ + all-str guard | ✅ | ✅ hand-rolled | ✅ | ✅ + all-str guard | ✅ + all-str guard | ✅ hand-rolled | ✅ String.join + guard | ❌ | ❌ |
| `trim` | ✅ strip(" \t\n\r\x0b\x0c") | ✅ strings.Trim(cutset) | ✅ hand-rolled regex | ✅ | ✅ hand-rolled | ✅ | ✅ hand-rolled | ✅ hand-rolled regex | ✅ hand-rolled | ✅ hand-rolled | ✅ trim(x, char(9,10,11,12,13,32)) | ❌ |
| `contains` | ✅ `in` | ✅ strings.Contains | ✅ includes | ✅ | ✅ hand-rolled | ✅ | ✅ contains | ✅ include? | ✅ plain find | ✅ contains | ✅ instr(x, y) > 0 | ❌ |
| `sha256` | ✅ hashlib | ✅ crypto/sha256 | ✅ node:crypto | ✅ | ✅ hand-rolled | ✅ | ✅ hand-rolled (no new dep) | ✅ digest stdlib | ✅ hand-rolled (5.4 bitops) | ✅ MessageDigest | ❌ "SQLite core has no SHA-256 (the CLI's sha3() is a different algorithm); refusing" | ❌ "keccak256 ≠ SHA-256; the SHA-256 precompile is unobservable in the compile-only seat; refusing" |

The 132 Python lowerings ride the `py` seat: green wherever `py` is green.

### Why the refusals are honest

- **sol / everything in wave 1:** the sol seat is compile-only — solc
  accepting a contract *is* its verification, and there is no stdout to
  compare. Emitting JSON/string/time/sha helpers that can never be observed
  would be unprovable surface. keccak256 is native but is not SHA-256; the
  SHA-256 precompile (address 0x02) is exact but unwirable to anything the
  gate can check. Refusal, tested, with the reason above.
- **sql / json:** this seat refuses maps outright (values ride as JSON text
  with statically-tracked kinds; a dynamically-keyed object has no exact
  static form). `json.parse` promises a map, so it refuses.
- **sql / split, join:** splitting produces a dynamically-sized list; the
  seat's lists are compile-time-known (unrolled `for`). Refusal.
- **sql / time.parts, sha256:** parts returns a map (refused); SQLite core
  has no SHA-256 function.

---

## 6. What the gate proves

`tests/proof_stdlib.rs`, following `tests/proof_crosschain.rs`:

- Per function family, a fixture under `examples/stdlib-wave1/` is run
  through `cuni check --only <green seats>`; the gate requires byte-identical
  stdout across seats *and* the interpreter, and `exactness: PASS`.
- The interpreter's stdout on each fixture is pinned byte-for-byte
  (`cuni run`), so the law itself can't drift.
- Refusal tests: `cuni check --only <refusing seat>` on the same fixtures
  must fail, with the documented reason in the output.
- Runtime-refusal tests: invalid-input fixtures (`bad-json.cuni`,
  `float-json.cuni`, `empty-sep.cuni`, `bad-date.cuni`) must fail
  (nonzero exit) on every green seat for that function — via
  `cuni check --only <seat>`, which runs the seat's real toolchain — and
  under `cuni run`.

## 7. Open items (not wave 1)

- `json.emit` with sorted keys means `emit` is *not* insertion-ordered;
  if a future use needs insertion order, that's a separate function.
- Pretty-printing (`json.emit_pretty`) — unneeded for the proof.
- `time` beyond year 1–9999; leap-second tables; time zones — refused by
  design (conversions only, UTC, no leap seconds).
- sol on-chain forms if a toll/contract use case lands.

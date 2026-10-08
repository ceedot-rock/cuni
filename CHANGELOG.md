# Changelog

All notable changes to CuNi are documented here.

Protocol v1.1.1 · Install: `cargo install cuni` · Studio: https://cuni-studio.fly.dev

---

## [Unreleased]

- Audited CI checks (version/license coherence, secret scan) — workflow staged; requires a token with `workflow` scope to land

---

## [0.9.0] — 2026-10-07

**Tag implied by `package.json` and `Cargo.toml` update in repo-health pass**

### Changed
- `package.json` license corrected from `GPL-3.0-only` to `AGPL-3.0-or-later` (was contradicting `LICENSE`)
- `SECURITY.md` supported-versions table updated to `0.9.x`
- Issue/PR templates and CI badges added
- Catalog counts corrected on all surfaces: 53 emitters, 45 active seats, 12 native seats

---

## [0.8.0] — 2026-10-02

**Released:** 2026-10-02 · **Tag:** `v0.8.0`

### Added — Onchain Division
- Six smart-contract-chain emitters: ink!, Move, Vyper, Cairo, Clarity, Cadence
- One `.cuni` source emits to all six chains with byte-identical numbers
- Fee law proven end to end across all emitters
- 204 tests green

---

## [0.7.0] — 2026-10-02

**Released:** 2026-10-02 · **Tag:** `v0.7.0`

### Added — Financial Division
- `cuni audit`: accepts a financial law and an implementation; returns a signed PASS or REFUSE receipt
- Three worked laws: tiered fees, interest accrual, withholding
- Boundary-exact arithmetic; the boundary cent is where fee bugs live

---

## [0.6.0] — 2026-10-02

**Released:** 2026-10-02 · **Tag:** `v0.6.0`

### Added — Exact Time
- `time` type: `int64` Unix timestamps, UTC only, strict ISO 8601
- `time + seconds = time`; `time - time = seconds`
- No float dates anywhere

---

## [0.5.0] — 2026-10-02

**Released:** 2026-10-02 · **Tag:** `v0.5.0`

### Added — Code Division
- `dec` type: fixed scale 10^4, scaled integers all the way down; never float
- Division truncates toward zero; division by zero fails loud
- Stdlib wave 1: canonical JSON, Unix timestamps, string ops, SHA-256
- Solana emitter included
- 105 tests green

---

## [0.4.0] — 2026-10-02

**Released:** 2026-10-02 · **Tag:** `v0.4.0`

### Added
- Java and SQL native seats
- Six proof profiles: cross-chain escrow, crypto conformance, integer-only ML inference parity, firmware C/Rust parity, audit-finance Java/Python parity, SQL portability
- 144 emitters; 12 native seats; same output or refuse

---

## [0.3.0 / 0.2.0] — 2026-09-26

**Released:** 2026-09-26 · **Tag:** `v0.2.0` · **Install version reference:** `v0.3.0`

### Added — Solidity Seat
- `cuni --emit-sol`: CuNi source compiles to a deployable, `solc`-verified EVM contract — or refuses
- Solidity reader: `.sol` files ingest back to CuNi; round-trip verified
- Native Ruby and Lua seats (quality backends, no lowering)
- Refusals now fail the emit outright; never a false-pass artifact
- Provably-fair dice example (`examples/casino/`) with 6-seat receipt

---

## [CuNi Bank 0.1.0] — 2026-09-13

**Released:** 2026-09-13 · **Tag:** `cuni-bank-0.1.0`

### Added
- `cuni bank paste <file> --from py --to <id>`: paste N, get X, exactness or refuse
- Gate proven on 10 catalog ids: py, go, js, ts, c, cpp, rs (native); rb, php, pl (catalog lowerings)
- `source_hash = bd4067ac5fd8b550`
- `docs/BANK.md`

### Not in 0.1.0
- Studio `/bank` tab
- `from=rs` / `from=c` ingest
- PCCX deposits

---

## [0.1.10] — 2026-09-12

**Released:** 2026-09-12 · **Tag:** `v0.1.10` · **Binary:** `cuni-0.1.10-x86_64-unknown-linux-gnu.tar.gz`

### Added
- `cuni run`: evaluates in-process; `cuni check` runs it on `ext`-free programs and refuses if stdout diverges from catalog gold
- Compute stdlib: `range`, `abs`, `min`, `max`, `str.len`, `slice` (OOB returns empty)
- Gold algorithms under `examples/compute/` (fib, gcd, sum-range, sort, range)
- Studio Exactness pane: py/go/js seats plus the in-process runner; `source_hash` live on the page
- Protocol `commands` includes `run`
- Available on crates.io: `cargo install cuni`

---

## [0.1.9] — 2026-09-09

**Released:** 2026-09-09 · **Tag:** `v0.1.9` · **Binary:** `cuni-0.1.9-x86_64-unknown-linux-gnu.tar.gz`

### Added
- `cuni check` emit+runs the full catalog (119 languages); native seats: py, go, js, ts, c, cpp, rs
- `--receipt` includes `source_hash` (SHA-256 of the `.cuni` source bytes)
- Agent-Rider refuses `register` if a claimed hash does not match
- `cuni ingest` (Python v1 subset) and `cuni prove --against`
- Lab laws under `examples/laws/` (suite meter, catalog prices, Rider 5% fee, spend cap)
- Integer `/` in py/js truncates toward zero to match Go/C/Rust

---

## [0.1.8] — 2026-09-09

**Released:** 2026-09-09 · **Tag:** `v0.1.8` · **Binary:** `cuni-0.1.8-x86_64-unknown-linux-gnu.tar.gz`

### Added
- `cuni --list-langs`: print the full language catalog
- `cuni <file.cuni> --emit-all DIR`: write one file per language
- Studio language picker and `GET /api/langs`
- Exactness gate unchanged: py, go, js must produce identical stdout or the compiler refuses

---

## [0.1.7] — 2026-07-28

**Released:** 2026-07-28 · **Tag:** `v0.1.7`

### Added
- Studio publish-to-register path live: after a PASS, source auto-registers into the Studio-side Rider stub
- Agent `spend` skill: speech `spend 4 cap 5` drives CheckSpend/can_spend through exactness into py/go/js
- Default example: `spend-control.cuni`
- UI: footer shows registered-contract count; Publish tooltip explains the register path

---

## [0.1.6] — 2026-07-26

**Released:** 2026-07-26 · **Tag:** `v0.1.6`

### Added
- Named type constructors: `Circle(r: 2.0)` (all fields named; no mix with positional)
- Call-site checks: concrete argument types + conflicting generic bindings refuse
- Flagship link-demo GIF in README
- Registry design sketch (`docs/REGISTRY.md`)
- CHANGELOG, CONTRIBUTING, SECURITY files added

---

[Unreleased]: https://github.com/ceedot-rock/cuni/compare/v0.9.0...HEAD
[0.9.0]: https://github.com/ceedot-rock/cuni/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/ceedot-rock/cuni/releases/tag/v0.8.0
[0.7.0]: https://github.com/ceedot-rock/cuni/releases/tag/v0.7.0
[0.6.0]: https://github.com/ceedot-rock/cuni/releases/tag/v0.6.0
[0.5.0]: https://github.com/ceedot-rock/cuni/releases/tag/v0.5.0
[0.4.0]: https://github.com/ceedot-rock/cuni/releases/tag/v0.4.0
[CuNi Bank 0.1.0]: https://github.com/ceedot-rock/cuni/releases/tag/cuni-bank-0.1.0
[0.1.10]: https://github.com/ceedot-rock/cuni/releases/tag/v0.1.10
[0.1.9]: https://github.com/ceedot-rock/cuni/releases/tag/v0.1.9
[0.1.8]: https://github.com/ceedot-rock/cuni/releases/tag/v0.1.8
[0.1.7]: https://github.com/ceedot-rock/cuni/releases/tag/v0.1.7
[0.1.6]: https://github.com/ceedot-rock/cuni/releases/tag/v0.1.6

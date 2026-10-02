# Changelog

## [0.7.0] — 2026-10-01 — "Trust Provable, in all things"
Financial Division, first release. `cuni audit` turns exactness into audit infrastructure: a money law stated once as `.cuni`, a foreign implementation proven byte-identical against it, a signed JSON receipt filed either way.
### Added
- `cuni audit <law.cuni> --against <impl> [--signer <keyfile>] [--out <receipt.json>]` (src/audit.rs, new library module): (a) SHA-256 of law and impl; (b) the gold gate reuses the existing `check::check_file_only` machinery on the money seats (py, rs, go, java, sql — py defines gold stdout), no duplicated runner; (c) the foreign impl runs by extension (.py→python3, .go→`go run`, .rs→rustc+exec, .java→javac+java, .sql→sqlite3 — shared with `cuni prove`, which now accepts the same set) and must print byte-identical stdout to gold; (d) verdict PASS only if the gate passed AND the impl matched, else REFUSE. A failed gate always emits a REFUSE receipt — never a pass. Exit 0 on PASS, nonzero on REFUSE; the receipt is filed either way.
- Signed receipts: `cuni audit --gen-key [name]` mints an Ed25519 keypair (`<name>.key` — 32 raw secret bytes, mode 600, refuses to overwrite; `<name>.pub` — pubkey hex). Receipt fields: `law_sha256`, `impl_sha256`, `gold_stdout_sha256` (null when the gate fails), `seats_run`, `verdict`, `timestamp_utc` (ISO-8601 from `SystemTime` via civil-from-days, no time crate), `cuni_version`, optional `signer_pubkey` + `signature` (Ed25519 over the canonical receipt bytes, which include the pubkey so a swapped key invalidates the signature). `audit::verify_receipt` re-verifies from the JSON bytes alone: Ok(true) valid, Ok(false) unsigned, Err on tampering. Keygen is a runtime op — tests use throwaway keys in temp dirs; no real key is ever committed.
- `examples/finance/`: three worked money laws, each gated byte-identical across the money seats with every driver output pinned in `tests/proof_finance.rs` — `fee_schedule.cuni` (tiered fees, boundary-exact at 99.9999/100.0000/1000.0000/1000.0001/zero/large), `interest_accrual.cuni` (`dec` × `time`: principal × APR × days/365 over a vesting window, leap-day driver case), `withholding.cuni` (marginal brackets 0%/10%/20%, boundary-exact at every bracket edge).
- `docs/FINANCIAL.md`: the division page for bank CTOs and auditors — the three flows (auditor proves the bank's impl; regulator publishes the law; bank self-certifies in CI), what the receipt contains and why it files, honest boundaries (behavioral stdout-equality on stated inputs only — not that the law is good business; driver coverage and key custody are the auditor's; refusals are fileable). Pricing pointer: hosted verification $0.10/check.
- New deps: `ed25519-dalek`, `serde`/`serde_json`, `sha2`, `getrandom` (crates.io sparse index).
- Catalog unchanged: 144 ids, 12 native seats. `audit` is a command, not a seat.

## [0.6.0] — 2026-10-01 — "Trust Provable, in all things"
### Added
- Exact timestamp type `time` (docs/TIME.md): int64 unix epoch seconds, UTC only — no timezones, no `now()`, no DST. Strict ISO-8601 UTC literals (`"2026-10-01T21:30:25Z"t`, validated once in the parser); closed arithmetic world (`time ± int -> time` both orders, `time − time -> int` seconds, comparisons on `(time, time)` only, no implicit time↔int); builtins `parse_time` (strict, loud refusal), `add_seconds`, `days_between` (trunc toward zero); canonical `say` rendering `YYYY-MM-DDTHH:MM:SSZ` byte-identical on every seat via Hinnant's civil-from-days. Per-seat: py `CuniTime`, js/ts BigInt, go `cuniTime`, c/cpp tagged `K_TIME`, rs `Val::Time`, rb `CuniTime` (explicit `coerce`), lua boxed metatable, java `long`, SQL INTEGER (literal folding + `strftime` rendering; `parse_time` folds literals, refuses dynamic strings), Solidity `uint256` via real solc (non-negative only — negative times refuse at emit; 0.8 checked arithmetic reverts), Solana refuses in v1. `cuni check` gates `examples/proof-time/time.cuni` (settlement/vesting/expiry) byte-identical on all 11 native seats + solc-compiled Solidity; `cargo test --test proof_time` pins the 14 verdicts and the refusal battery.
- Catalog unchanged: 144 ids. `time` is a type, not a seat.

## [0.5.0] — 2026-10-01 — "Trust Provable, in all things"
Code Division, first release under the new mission line. ("Replace trust with proof" stays as the 0.4.0-era supporting line.)
### Added
- Exact decimal type `dec` (scale 10⁴, docs/DECIMAL.md): fixed-point money math across all 13 seats — Python int, Rust i128, Go int64 (out-of-range refused), JS/TS BigInt (never f64), C/C++ `__int128` or refuse, Ruby bignum, Lua int64 or refuse, Java scaled `BigInteger`, SQL scaled INTEGER (never REAL), Solidity `uint256` via real solc. Truncation toward zero on division; no implicit dec↔int conversion; `%` refused; `dec_of_int`/`int_of_dec` explicit conversions; canonical printing; division by zero loud everywhere. `cuni check` gates `examples/proof-decimal/money.cuni` byte-identical on all 12 native seats + solc-compiled Solidity.
- Stdlib wave 1 (docs/STDLIB.md): JSON parse/emit (canonical form), unix timestamp conversions (no wall-clock `now()` — nondeterministic), string ops (split/join/trim/contains with exact semantics), SHA-256. Per-seat support/refusal matrix in the spec; Solidity refuses all wave-1 (tested refusals).
- Solana program emitter (`cuni --emit-solana <out.rs>`, `src/codegen_solana.rs`): one `.cuni` source becomes an Anchor-shaped Solana program — a pure logic core (the part CuNi proves) plus the program shell (entrypoint, accounts struct, program id) in two clearly delimited regions. Mirrors the Solidity backend's posture: genuine artifact, real-toolchain verification where possible, honest refusal everywhere else (`float`/`list`/`map`/`opt`/`??`/structs/enums refuse).
- `examples/proof-solana/` + `cargo test --test proof_solana`: the transfer-validation law gate-proven byte-identical on rs/go/py seats AND on the logic module extracted from the emitted program (compiled standalone with plain `rustc`, no dependencies).
- `docs/SOLANA.md`: honest boundaries (logic gate-proven; shell NOT compiled here — no Solana toolchain on the check machine; nothing executed on-chain) and the full verification recipe (Solana CLI + Anchor) for a tooled machine.
- Catalog unchanged: 144 ids, 12 native seats. The Solana work is a proof profile, not a new seat.

## [0.4.0] — 2026-10-01 — "Replace trust with proof"
### Added
- Java real-toolchain seat (`--emit-java`): `javac`-compiled, `long`/`double`/`String`/`boolean`, collections, structs as nested classes. The audit-finance story: the bank's Java and the auditor's Python gate-proven to agree.
- SQL real seat (`--emit-sql`): lowers pure computation to SQL; SQLite dialect verified against real `sqlite3`, PostgreSQL/MySQL dialect emitters included (SQLite gate-verified only, stated in the artifact header).
- Catalog stays 144: 12 native seats (py, go, js, ts, c, cpp, rs, rb, lua, sol, java, sql) + 132 Python lowerings. `java` and `sql` were already catalog ids; promoting them adds no new languages.
- Proof profiles under `examples/proof-*/`, each with a runnable gate test:
  - `proof-crosschain`: escrow transfer-validation law proven across Solidity (solc-compiled, deployable) + Rust + Go + Python.
  - `proof-crypto`: FNV-1a 32-bit reference digest + toy RSA verify, proven across Rust + Python + Go.
  - `proof-mlparity`: integer 4x4 matmul + argmax kernel, proven across Python + Rust + C (integer-only by design — cross-language float exactness can't be guaranteed, so CuNi refuses rather than fakes).
  - `proof-firmware`: thermostat control logic proven across C + Rust, with `promote.sh` — gate fails, nothing ships.
- Native Java ingest (`ingest_java`) so `.java` artifacts round-trip back to CuNi.

## [0.3.0] — 2026-09-29
### Added
- Solidity smart-contract seat: CuNi compiles to deployable EVM contracts
  via solc under the same exactness law.
- 144 language seats (up from 119): 25 new catalog seats, all verified
  emit+run exactness PASS.
- Dual license: AGPL-3.0-or-later OR Slid Phi Labs Commercial License.

## [Unreleased] — 119 → 144 language seats

### Added
- 25 new catalog seats in `src/langs.rs`: Ballerina, Ceylon, Xtend, Gosu, NetRexx (Java family); Frege, Eta, Agda, Curry, Clean, Roc, Koka (Haskell family); ATS, Chapel, Jai, HolyC, Cyclone, C--, Pawn, S-Lang (C family); Carbon, Cpp2 (C++ family); Datalog, Mercury, Oz (Prolog family).
- All 25 verified 25/25 emit+run exactness PASS via `cuni check --only` on `full.cuni`, plus `structs.cuni` + `enums.cuni`; spot-checked artifacts byte-identical to the py gold.
- Docs sweep: every live "119" claim now reads 144. Historical entries below keep their original numbers.

### Honesty
- New seats verify through the repo's Python lowering and stay header-labeled "Seat pending a native toolchain" — catalog seats that emit+run, not native toolchains. Native seats: py, go, js, ts, c, cpp, rs, rb, lua, sol, java, sql.

## [cuni-bank-0.1.0] — 2026-09-13

### Added
- **CuNi Bank** arm: `cuni bank paste IN --from py --to <id>` — ingest → emit → prove, or refuse.
- Studio `POST /api/bank` and `/bank`. Protocol command `bank`.
- 10-lang gate on `examples/bank/add.py` (py go js ts c cpp rs + rb php pl lowerings). `source_hash=bd4067ac5fd8b550`.
- Not 119 ingest parsers. 119 langs remains `cuni check` on the ingested deposit.

## [0.1.10] — 2026-09-12

### Added
- **`cuni run file.cuni`** — in-process evaluator. It is a **seat**: `cuni check` runs it on `ext`-free programs and refuses if stdout diverges from catalog gold. `--lang py|go|js|…` still emit+runs a native toolchain.
- Compute stdlib (SPEC.md §15): `range`, `abs`, `min`, `max`, `str.len`, `slice` (OOB → empty).
- Gold algorithms under `examples/compute/` (fib, gcd, sum-range, sort, range).

### Changed
- C `cuni_len` counts string bytes so `s.len()` matches py/go/js/rs.
- Studio Exactness pane: py/go/js seats + in-process `run` (same stdout). `source_hash` is live on the page.

## [0.1.9] — 2026-09-09

### Added
- **`source_hash` on `--receipt`** — SHA-256 of the `.cuni` bytes. The program is that hash, not the path. Agent-Rider refuses register on mismatch.
- Protocol surfaces: `PROTOCOL.md`, `/.well-known/cuni-protocol.json`.

### Changed
- **`cuni check` emit+runs every catalog language (119).** Native seats: py, go, js, ts, c, cpp, rs. Other ids: Python lowering so the gate still runs.
- Native seats **c / cpp / rs** covering `full.cuni`.
- `cuni ingest` (Python v1 subset) and `cuni prove --against`.
- Lab laws under `examples/laws/` (suite meter, catalog SKUs, Rider fee, spend cap).
- py/js integer `/` truncates toward zero so money laws match Go/C/Rust.

## [Unreleased] — 119-language seat

### Added
- Native seats **c / cpp / rs** (gcc, g++, rustc) covering `full.cuni`.
- **`cuni ingest`** — Python v1 subset → CuNi, or refuse.
- **`cuni prove --against`** — foreign impl must match CuNi gold.
- **`cuni check --only id,id`** and **`--receipt`** (ledger JSON).
- `docs/SEATS.md` — 119 languages is law.
- Lab laws under `examples/laws/`: suite meter, catalog SKUs, Rider fee, spend cap.
- `scripts/prove-lab-laws.sh` proves spl-pay-per-suite quotes against suite-meter gold.
- py/js integer `/` truncates toward zero so money laws match Go/C/Rust.

### Changed
- **`cuni check` emit+runs every catalog language.** Native: py, go, js, ts, c, cpp, rs. Other ids: Python lowering so the gate still runs.
- Agent host timeout 180s; `--skip-check` is not a citizen.

## [0.1.8] — 2026-09-09

### Added
- **`--emit-all DIR`** writes the full language catalog (`src/langs.rs`, 119 printers)
- **`--list-langs`** lists id / name / extension
- Studio language picker + `GET /api/langs`
- Honest split: catalog emit vs exactness (still py/go/js only)
- Studio stays up: `auto_stop_machines = "off"`, `min_machines_running = 1`

## [Unreleased] — Studio host + SEO

### Added
- **Hosted CuNi Studio:** https://cuni-studio.fly.dev/ (Fly.io)
- Studio **Notelog** + **Critic Book** (persist on volume)
- Studio SEO: Open Graph, Twitter cards, JSON-LD, `robots.txt`, `sitemap.xml`
- Primary CTA across README / press / outreach → Studio URL

## [0.1.6] — 2026-07-26

### Added
- **Named typ constructors:** `Circle(r: 2.0)` (all-named only; no mix with positional)
- **Call-site type checks:** concrete arg types + generic parameter binding conflicts
- **GitHub Release** for v0.1.5 platform surface
- **`assets/link-demo.gif`** — flagship link demo animation
- **`docs/REGISTRY.md`** + example `packages/greet-contract/`
- **CHANGELOG**, **CONTRIBUTING**, SEO meta on playground

### Platform (from 0.1.5)
- `cuni check`, line:col errors, playground, Exactness CI, link demo

## [0.1.5] — 2026-07-26

### Added
- `cuni check` exactness gate
- AST spans + `file:line:col` type errors
- Local playground (`playground/`)
- Exactness GitHub workflow + badge
- Flagship `examples/link/demo.sh` + `docs/LINK_TUTORIAL.md`

## [0.1.1] — prior
- CI, demos, install docs

## [0.1.0] — prior
- Initial public compiler

# Changelog

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

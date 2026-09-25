# M1 — Parser fidelity (Universal AST v0)

**Status:** Milestone card (measure + gaps). Not a completeness claim. Partial eng progress: labeled oddity hard-fails for pointers / async / macros / ownership / prototypes (see `tests/oddity_matrix.rs`).  
**Parent design:** [`docs/UNIVERSAL_AST_V0.md`](../UNIVERSAL_AST_V0.md) (merged via [#19](https://github.com/ceedot-rock/cuni/pull/19)).  
**Touches:** `src/parser.rs`, `src/ast.rs`, `src/token.rs`, `src/lexer.rs`  
**Spine:** Translate (CuNi) → Fund (Rider / XPay, **not** PCC) → Execute (Rider)  
**PCC:** lossless compressor product only.

Exactness refuse is sacred. There is **no approximate mode**. Prefer **hard-fail** over soft rewrite.

---

## Honesty (do not soft-claim)

| Tier | Fact |
|------|------|
| Catalog | **119** seat ids in `src/langs.rs` |
| Native | **~7:** `py`, `go`, `js`, `ts`, `c`, `cpp`, `rs` |
| Studio / Publish gate | **`py`, `go`, `js` only** (Fly image) |
| Rest | **Majority lowering** (Python until native); receipt marks `native` vs `lowering` |

Do **not** claim soft “120 languages” 1-for-1.

---

## What “parser fidelity” means here

Parser fidelity is **not** “parse everything every language can say.” It is:

1. **Accept only** the portable CuNi core that can keep one gold stdout under `cuni check`, or refuse.
2. **Surface refuse early** with spans/diagnostics when source leaves that core — not silent rewrite.
3. **Stay honest** about what `src/ast.rs` actually names today vs what the oddity matrix forbids.

The oddity matrix lives in [`UNIVERSAL_AST_V0.md` §3](../UNIVERSAL_AST_V0.md). Columns: **map** | **approximate-forbid** | **hard-fail**.

---

## Measured coverage (today’s front-end)

Inventory against live code (master tip after #19), not aspirational IR:

### Present in AST / parser (portable core surface)

| Area | What’s there | Notes |
|------|----------------|-------|
| Items | `use`, `ext`, `typ`, `iface`, `enum`, `def`, `link`, top-level stmt | `src/parser.rs` `parse_item` |
| Types | `Named`, `Generic` (`list`/`map`/`opt`-shaped) | No pointer / borrow / lifetime types |
| Stmts | `let` / `mut` / assign / `ret` / `fail` / `if`/`els` / `for` / `whl` / expr / `...` | Sync only |
| Exprs | literals, list/map, call (pos + named for typ ctors), index, field, binary/unary, `??` unwrap | No async / await / spawn |
| OO-ish | Closed `typ` (+ optional single `is iface`), `iface` method sigs, payload-free `enum` | Not open prototypes |
| Escape hatch | `ext … do <target>: … end` | Explicit leave-portable-core |

### Oddity matrix → current parser stance

| Oddity | Policy (v0) | Measured parser/AST reality | Gap? |
|--------|-------------|-----------------------------|------|
| **Pointers** | map opaque refs only if identical; else **hard-fail** | No `*` / `&` / address types or ops in `Token` / AST | **By absence = hard-fail** for raw pointers. Gap: no *labeled* refuse diagnostic (“pointer oddity”) if alien text arrives via ingest / bad tokens — today it’s generic parse error. |
| **Prototypes / OO** | map closed `typ`/`iface`; forbid silent prototype-chain | `typ`/`iface` only; no prototype mutation, no dynamic `this`, no multi-inheritance | **Mostly map.** Gap: single `is` iface only; no conformance tests that open-prototype *source patterns* hard-fail with a stable code. |
| **Ownership** | map shared `Val` model; forbid seat-specific move/drop | `let` vs `mut` only — no borrow/move/lifetime syntax | **By absence = hard-fail** for Rust-style ownership in portable core. Gap: fidelity of *emit* ownership stories is emit/runtime work (M2/M3), not parser-proven. |
| **Async** | sync portable core; `link` = process boundary | No async keywords; `link` is sync wire shape | **By absence = hard-fail.** Gap: no corpus asserting “async/await/go/promise-shaped .cuni” refuses with an oddity tag. |
| **Macros** | none in portable core → **hard-fail** | No macro / `#` / template forms in grammar | **By absence = hard-fail.** Gap: `ext` raw lines can contain foreign macros — that’s allowed as leave-core, but receipts must never treat `ext`-divergent programs as full-catalog 1-for-1 citizens. |
| **Floats / formatting** | prefer ints; float print must be proven identical | `Float(f64)` + `Token::Float` accepted | **Known hazard.** Parser accepts floats; exactness refuse is downstream (`cuni check` / refusal examples). Gap: no parse-time warn/refuse tier for float `say` paths. |
| **`ext`** | explicit leave-portable-core | Parsed; targets not re-lexed as CuNi | Correct escape. Gap: ensure Studio/Rider paths never soft-claim `ext` as catalog-wide PASS. |

**Summary:** most dangerous oddities are **out of grammar** (good: prefer hard-fail). That is **not** the same as a finished fidelity suite. Do not read “no syntax” as “matrix enforced in CI.”

---

## Concrete gaps (honest list)

1. **No oddity-matrix test harness** tied to `src/parser.rs` — no checked corpus of “must hard-fail” / “must map” cases per row.
2. **Generic parse errors only** — refuse messages don’t yet name matrix rows (`pointers`, `async`, …).
3. **Span incompleteness** — ~~`Use` name, enum variant names, iface method names lack first-class spans~~ **landed (name spans + diagnostic use for missing-use / dup variant / iface method)**. Remaining: other AST name sites (e.g. `ExprKind::Field` name, `TypDecl.implements` ident) still lack first-class spans — not claimed complete for every identifier.
4. **Float accepted in AST** — fidelity debt: exactness sacred means float formatting divergence must refuse; parser doesn’t help yet.
5. **Ingest / foreign paths** — if non-`.cuni` or rewritten input enters the pipeline, parser fidelity alone doesn’t police oddities; need clear refuse before emit.
6. **Lowering honesty** — parser doesn’t know seats; `src/emit.rs` marks `native` vs `lowering`. M1 must not pretend parser success ⇒ 119 native 1-for-1.
7. **No claim that AST = Universal AST IR** — today’s `src/ast.rs` is the front-end AST. Explicit IR is **M2** and **is not done**.

---

## Done-when (M1 exit — still incremental)

- [x] Oddity matrix rows each have ≥1 **hard-fail** (or documented **map**) fixture under tests. *(landed: pointers / async / macros / ownership / prototypes in `tests/oddity_hardfail/`; floats/`ext` remain documented map-or-downstream — not fake-complete)*
- [x] Refuse diagnostics can cite matrix category where applicable. *(`oddity hard-fail [row]` + fix-it; `src/oddity.rs`)*
- [x] Span coverage on public AST names used in diagnostics is complete enough for Rider/Studio error surfaces. *(Use / enum variant / iface method name spans + refuse locations; lex/parse oddity refuses already file:line:col. Other ident sites may still lack spans — incremental, not fake-total.)*
- [x] Doc + tests still say: **119 catalog / ~7 native / majority lowering; Studio gate py/go/js; IR not done.**

Out of scope for M1: implementing IR, changing emit seat families, SettleHop, PCC payment framing.

---

## Links

| Doc / code | Why |
|------------|-----|
| [`docs/UNIVERSAL_AST_V0.md`](../UNIVERSAL_AST_V0.md) | Parent design; oddity matrix; milestones |
| [`docs/SEATS.md`](../SEATS.md) | Native vs lowering law |
| [`docs/EXACTNESS_REFUSAL_EXAMPLES.md`](../EXACTNESS_REFUSAL_EXAMPLES.md) | Refuse as clearly as PASS |
| `src/parser.rs` / `src/ast.rs` / `src/token.rs` | Measured surface |
| `src/emit.rs` / `src/langs.rs` | Seats + lowering (not parser) |

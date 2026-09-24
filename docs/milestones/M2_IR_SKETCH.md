# M2 — IR sketch (Universal AST v0)

**Status:** Sketch card only. **IR is NOT done.**  
**Parent design:** [`docs/UNIVERSAL_AST_V0.md`](../UNIVERSAL_AST_V0.md) (merged via [#19](https://github.com/ceedot-rock/cuni/pull/19)).  
**Related code today:** `src/ast.rs` → `src/emit.rs` (+ codegen_*) with seat ids in `src/langs.rs`  
**Spine:** Translate (CuNi) → Fund (Rider / XPay, **not** PCC) → Execute (Rider)  
**PCC:** lossless compressor product only — not Fund.

Exactness refuse is sacred. No approximate mode. Prefer hard-fail.

---

## Honesty (say out loud)

> **119 catalog seats; ~7 native; majority lowering.**  
> Studio / Publish gate: **py / go / js** only.  
> Do **not** soft-claim “120 languages” 1-for-1.

Receipts must keep marking `native` vs `lowering`. Lowering ≠ skip; lowering ≠ first-class native.

---

## What this card is / isn’t

| Is | Isn’t |
|----|--------|
| Goals + non-goals for a future portable IR | A shipped IR module |
| How IR should relate to current AST / emit / langs | A rewrite schedule pretending emit already consumes IR |
| Exactness-seat preservation constraints | Permission to approximate when seats diverge |

**Explicit:** Universal AST v0 remains a **constraint design + roadmap**. Pipeline today is still **parse → `src/ast.rs` → `src/emit.rs`**. `src/ir.rs` exists as an **unfinished** M2 stub only — emit does **not** consume IR yet.

---

## Goals (preserve exactness seats)

1. **One portable program → many seats, one gold stdout** — or refuse. IR must name only constructs that can keep that law.
2. **Exactness seats as first-class IR metadata** — IR (or adjacent check artifacts) must carry enough structure for receipts: `source_hash`, `exactness.passed`, per-seat `native` | `lowering`.
3. **Separate “what the program means” from “how a seat spells it”** — portable core in IR; seat spelling stays in emit/codegen. Avoid baking Python-lowering accidents into the portable meaning.
4. **Hard-fail oddities stay hard-fail** — pointers / open prototypes / ownership games / async-in-core / macros: refuse in IR validation, don’t lower into “close enough.”
5. **Incremental adoption** — IR may start as a thin validated view *over* today’s AST (or a lossless lower from AST), then grow; gold stdout on native seats must not regress.
6. **Rider gate compatibility** — Translate produces PASS artifacts Fund can trust; CuNi does not become the payment rail (PCC is compressor, not XPay).

---

## Non-goals

- Shipping a finished optimizing compiler IR in this milestone.
- Claiming 119 (or ~120) first-class native 1-for-1 seats.
- Approximate / best-effort emit when seats diverge.
- Folding Fund into PCC or treating compression as payment.
- Silent macro/pointer/async rewriting to “make emit work.”
- Replacing `ext` with fake portability — `ext` remains explicit leave-core.
- Pretending Studio’s py/go/js gate equals full-catalog native maturity.

---

## Relation to current code

```
.cuni source
  → lexer / parser          (src/lexer.rs, src/parser.rs)
  → front-end AST           (src/ast.rs)          ← today
  → typeck / check          (src/typeck.rs, src/check.rs)
  → emit per seat           (src/emit.rs → codegen_*)
  → catalog ids             (src/langs.rs: 119)
        │
        ▼ (M2 stub — NOT DONE)
  explicit portable IR      (src/ir.rs sketch)
        │
        ▼ (M3)
  emit from IR              (still src/emit.rs family)
```

| Artifact | Role vs IR sketch |
|----------|-------------------|
| **`src/ast.rs`** | Current front-end AST. Likely *source* of IR (lower/validate), not the final IR name forever. |
| **`src/emit.rs`** | SeatKind (`Native` / `Lowering`) + `generate_exact`. IR must not erase that honesty; emit stays the seat spelling layer. |
| **`src/langs.rs`** | 119 catalog ids. IR does not invent seats; it must remain checkable against this catalog law. |
| **codegen_\*** | Native families today (`py`/`go`/`js`/`ts`/`c`/`cpp`/`rs`); rest Python-lower. IR sketch must plan seat-by-seat graduation without soft 120 claims. |

---

## Exactness seats (what IR must preserve)

An “exactness seat” is a catalog language id that either:

- **native:** emits with that language’s toolchain and matches gold, or  
- **lowering:** emits via Python lowering, still emit+runs, receipt marked `lowering`,

and never gets a silent approximate pass.

IR sketch obligations:

- Preserve observability needed for **one gold stdout** comparison.
- Preserve ability to **refuse** (no approximate mode).
- Preserve seat marks through to citizen receipt → Rider `POST /api/v0/contracts` (refuse without PASS).

Studio gate **py/go/js** is hosted-gate alignment, not a claim that other natives are absent or that lowering is native.

---

## Suggested IR shape (sketch — provisional)

Not implemented. For discussion only:

1. **Portable module** — items: functions (`def`), wire (`link`), closed `typ`/`iface`/`enum`, imports; **no** raw pointers/macros/async-in-core.
2. **Typed core ops** — today’s stmt/expr set that already round-trips native gold; floats marked hazardous until proven.
3. **Seat plan attachment (post-IR or sidecar)** — map of lang id → `native` | `lowering` from `src/emit.rs` / `src/langs.rs`, not re-derived ad hoc in codegen.
4. **Refuse table** — oddity matrix rows as validator codes (feeds M1 diagnostics upward).

Anything beyond this is **M3+** (emit-from-IR, native graduation).

---

## Done-when (M2 exit — still a sketch)

- [ ] Written IR goals/non-goals agreed (this card + any small follow-up).
- [x] Named candidate module boundary: `src/ir.rs` (thin stub, wired from `lib` + `main`) — **still unfinished / NOT DONE**; no claim of completion.
- [ ] Explicit statement remains true: **IR is not done**; pipeline still AST→emit.
- [ ] Honesty tiers unchanged in docs: 119 / ~7 native / majority lowering; Studio py/go/js.

**Non-done:** implementing IR, migrating emit, freezing receipt schema (that’s M4), native graduation (M5).

---

## Links

| Doc / code | Why |
|------------|-----|
| [`docs/UNIVERSAL_AST_V0.md`](../UNIVERSAL_AST_V0.md) | Parent design; M0–M5 roadmap |
| [`docs/milestones/M1_PARSER_FIDELITY.md`](M1_PARSER_FIDELITY.md) | Front-end fidelity before IR |
| [`docs/SEATS.md`](../SEATS.md) | Catalog / native / lowering |
| [`docs/RIDER_REGISTRATION_API.md`](../RIDER_REGISTRATION_API.md) | PASS gate for Fund |
| `src/ast.rs`, `src/emit.rs`, `src/langs.rs` | Current Translate spine |

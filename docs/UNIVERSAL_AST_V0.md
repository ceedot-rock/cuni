# Universal AST v0 (Track B — CuNi)

**Status:** Design proposal. IR is **not** done.  
**Owner:** CuNi (Track B)  
**Spine:** Translate (CuNi) → Fund (Rider / XPay, **not** PCC) → Execute (Rider)  
**PCC:** lossless compressor product only — out of scope here.

Exactness is sacred. There is **no approximate mode**. Prefer hard-fail over “almost.”

Canonical law: [`PROTOCOL.md`](../PROTOCOL.md), [`docs/SEATS.md`](SEATS.md), [`docs/EXACTNESS_REFUSAL_EXAMPLES.md`](EXACTNESS_REFUSAL_EXAMPLES.md).

---

## 1. IR goals / non-goals

### Goals (v0)

1. **Portable CuNi core → multi-seat emit with exactness.** One program, one gold stdout across seats under `cuni check`, or refuse.
2. **1-for-1 constraints, not “compile somehow.”** The Universal AST names what the portable core may express so emit stays faithful seat-to-seat.
3. **Stable ledger artifacts.** AST / check output feeds citizen receipts (`source_hash`, `exactness.passed`, per-seat `native` vs `lowering`) that Rider can gate.
4. **Incremental path.** Grow from today’s front-end AST (`src/ast.rs`) and emitters (`src/emit.rs`) toward an explicit IR — do not pretend a finished IR exists.

### Non-goals (v0)

- Approximate / “close enough” emit when seats diverge.
- Claiming 119 (or “~120”) languages as first-class 1-for-1 native seats.
- Replacing PCC or folding Fund into a compressor product.
- Full language interop for every catalog oddity (pointers, macros, etc.) via silent rewriting.
- Shipping SettleHop as a CuNi deliverable in this doc (Rider owns settle; CuNi owns PASS artifacts).

---

## 2. Honesty tiers (measure, don’t soft-claim)

Live Studio health and catalog law (2026-09), aligned with `PROTOCOL.md` / `docs/SEATS.md`:

| Tier | What it means | Count / ids |
|------|----------------|-------------|
| **Catalog seats** | Language ids in `src/langs.rs` — emit+run under `cuni check`, or refuse | **119** |
| **Native seats** | Real maturity: toolchain of that language | **~7:** `py`, `go`, `js`, `ts`, `c`, `cpp`, `rs` |
| **Studio / Publish gate** | Fly image limitation — hosted exactness / publish | **`py`, `go`, `js` only** |
| **Rest of catalog** | Python **lowering** until native — still emit+run; receipt marks `native` vs `lowering` | Majority of 119 |

**Say this out loud:**

> **119 catalog seats; ~7 native; majority lowering.**

Do **not** claim soft “120 languages” 1-for-1.

**Lowering ≠ skip**, but **lowering ≠ first-class 1-for-1 native.** Receipt honesty is the bridge until more seats graduate under the same exactness law.

---

## 3. Language-oddity matrix

Columns are IR / emit policy for the **portable CuNi core** (no `ext`). Prefer **hard-fail** over approximate-forbid when fidelity cannot be proven.

| Oddity | **map** (exact, portable) | **approximate-forbid** (refuse soft rewrite) | **hard-fail** (refuse) |
|--------|---------------------------|-----------------------------------------------|-------------------------|
| **Pointers** | Opaque refs / indices only if every seat has identical observable behavior | “Map `*T` to GC ref and hope” | Raw address arithmetic, pointer provenance, `unsafe` aliasing across seats |
| **Prototypes / OO** | Closed `typ` / `iface` already in CuNi (`src/ast.rs`) with structural field/method semantics that emit identically | Silent prototype-chain / monkey-patch semantics from JS into static seats | Open prototype mutation, dynamic `this` binding games, class inheritance that changes dispatch per seat |
| **Ownership** | Value / shared runtime `Val` model (today’s C/Rust tagged runtime) with identical stdout | “Borrow-check-ish” comments with no enforcement | Seat-specific move/drop/lifetime that changes observable results or silently leaks differently |
| **Async** | Sync portable core only in v0; `link` remains process boundary | Auto-`async`/`await` rewrite that “usually” matches | Promises / goroutines / threads in portable core without a proven identical gold |
| **Macros / metaprogramming** | None in portable core | Hygiene-approximating macro expand that differs by seat | C preprocessor, Rust macros, template metaprogramming in portable AST |
| **Floats / formatting** | Prefer integers; portable `say` paths only when proven identical | Soften float print to “close” strings | Non-portable float formatting that diverges stdout (see exactness refusal examples) |
| **`ext` blocks** | Allowed as **explicit** leave-portable-core — not Universal AST 1-for-1 | Treating `ext` as if it still passed full-catalog exactness | Registering `ext`-divergent programs as citizens without PASS |

**Rule of thumb:** if the oddity cannot be given a single gold observation on native seats, it is **hard-fail** for the portable core — not a best-effort emit.

---

## 4. How the AST ledger feeds Translate → Fund → Execute

```
Translate (CuNi)          Fund (Rider / XPay)         Execute (Rider)
─────────────────         ───────────────────         ────────────────
.cuni source
  → parse / typeck
  → emit per seat
  → cuni check
  → citizen receipt
        │
        ├─ source_hash     (SHA-256 of .cuni bytes)
        ├─ exactness.passed
        └─ per-seat native | lowering
        │
        └──────────────►  gate: refuse register unless PASS
                          POST /api/v0/contracts
                          (Studio stub: POST /api/rider/register)
                          SettleHop later (Rider)
                                              └─► run only funded
                                                   citizens
```

### Artifacts Rider may gate on

| Field | Role |
|-------|------|
| `source_hash` | Program identity — the hash, not the path. Mismatch → refuse. |
| `exactness.passed` | Citizenship. Must be `true` or Rider refuses contract register. |
| Seat marks (`native` / `lowering`) | Honesty in the receipt; not a license to approximate. |

**Law:** CuNi output that Rider refuses without PASS is not a citizen. Speech is not law — models may talk; they may not ship. See `PROTOCOL.md` §2 and `docs/RIDER_REGISTRATION_API.md`.

Studio / Publish today gates `py,go,js` (Fly image). Full catalog exactness remains the CLI / CI law. That split is **gate alignment**, not approximate mode.

---

## 5. Next eng milestones (do not pretend IR is done)

Point at existing code; grow incrementally:

| Milestone | Intent | Touches |
|-----------|--------|---------|
| **M0 — Document** | This file: goals, honesty, oddity matrix, Rider feed | `docs/UNIVERSAL_AST_V0.md` |
| **M1 — Parser fidelity** | Keep front-end AST honest; spans / diagnostics for refuse | `src/ast.rs`, parser |
| **M2 — Explicit IR sketch** | Separate portable IR from seat-specific emit decisions | new IR module (TBD), fed from `src/ast.rs` |
| **M3 — Emit from IR** | Incremental: one seat family at a time; gold unchanged | `src/emit.rs`, `src/langs.rs` |
| **M4 — Receipt schema freeze** | Stable `source_hash` + `exactness.passed` + seat marks for Rider | check / receipt path; Rider `POST /api/v0/contracts` |
| **M5 — Native graduation** | Grow natives under the same law (`sh`, `pl`, `awk`, `sql`, `wat` called out in SEATS) | `src/langs.rs`, emitters |

**Explicit:** Universal AST v0 is a **constraint design + roadmap**, not a claim that IR is finished. Today’s pipeline still centers on `src/ast.rs` → `src/emit.rs` with catalog ids in `src/langs.rs`.

---

## 6. Links (exactness refuse sacred)

| Doc | Why |
|-----|-----|
| [`PROTOCOL.md`](../PROTOCOL.md) | Exactness, receipts, citizen rule |
| [`docs/SEATS.md`](SEATS.md) | Native vs lowering; catalog law |
| [`docs/EXACTNESS_REFUSAL_EXAMPLES.md`](EXACTNESS_REFUSAL_EXAMPLES.md) | Refuse as clearly as PASS; no approximate mode |
| [`docs/RIDER_REGISTRATION_API.md`](RIDER_REGISTRATION_API.md) | `exactness.passed` gate; `POST /api/v0/contracts` |
| `src/ast.rs` | Current front-end AST |
| `src/emit.rs` | Multi-seat emit |
| `src/langs.rs` | 119 catalog seat ids |

---

## Summary for CoS

- Track B design doc only — **no merge assumed** until GREEN.
- Honesty: **119 catalog; ~7 native; Studio gate py/go/js; majority lowering.**
- Oddity matrix prefers **hard-fail**.
- Rider funds/executes only PASS citizens (`source_hash` + `exactness.passed`).
- IR milestones are incremental; **IR is not done.**

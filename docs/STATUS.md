# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-09-24 (ET) · **v0.1.10 tagged** / tree **0.1.11** · `cuni run` is a check seat · Studio → Rider loop live (Fly)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge  
2. Exactness as the trust / verification gate  
3. Shared authoring + progressive deployment (Studio → Rider)

## Live today
- **Studio**: https://cuni-studio.fly.dev/ — Playground + Agent mode; spend-control default; Progress + Publish; free to try  
- **Agent-Rider (live face)**: https://agentrider.fly.dev — Fly-only. The old Vercel edge (`*.vercel.app`) returning HTTP 402 is a **historical dead door**, not the current live path.  
- **Exactness CI**: green on every push (examples + cargo test)  
- **v0.1.9 gate**: `cuni check` emit+runs the **119-language catalog**; `--receipt` records `source_hash` (SHA-256 of `.cuni` bytes) so Rider can refuse a mismatched claim  

- **Studio→Rider citizen-receipt push: LIVE** — Studio publish POSTs explicit `citizen_receipt` (`source_hash` + `exactness.passed`) to Rider `/api/v0/contracts` when `CUNI_RIDER_URL` is set and exactness PASS. Landed via [#23](https://github.com/ceedot-rock/cuni/pull/23). Fund = Rider/XPay — never PCC. See [`PASS_GATE.md`](PASS_GATE.md).
- **Studio `POST /api/pass`: LIVE** — Rider-callable verify door on Fly. Missing/broken source → **REFUSE**; known-good spend-control → **PASS** with receipt. Alias `/api/citizen/pass`.
- **Publish → register**:  
  - Local stub: `/api/rider/register` + `/api/rider/registered` → `{ok, count, contracts[]}`  
  - Remote: `CUNI_RIDER_URL=https://agentrider.fly.dev` → `POST /api/v0/contracts` (exactness-gated, idempotent on `sourceHash`)  
  - Studio UI surfaces contract **count**, recent **id / sourceHash / registeredAt / status**, and a Rider link (prefers `health.rider.remote_url`)  
- **Health**: `/api/health` exposes `rider.register`, `rider.list`, `rider.remote`, `rider.remote_url`, plus `lang_count`  
- **Flagship proofs**: exactness (identical stdout or refuse) + `link` interop + Agent `spend` skill  
- **Packaging**: Homebrew formula + binstall metadata advanced to **v0.1.10** — see [`docs/PACKAGING.md`](PACKAGING.md) + `packaging/homebrew/cuni.rb`  
  Preferred install today: `cargo install cuni` or `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.10`  
  Linux x86_64 release asset ships on the GitHub Release; broader matrix still needs the release workflow.

## Exactness stays sacred
A CuNi program either produces the same behavior on every supported target, or it **refuses**. There is **no approximate mode**. Type and exactness failures now carry concrete fix-its in CLI + Studio (still refuse — never soften the gate).

### Studio hosted gate = py / go / js (choice, 2026-09-11)
- **CLI / CI**: `cuni check` still emit+runs the **119-language catalog** (native + lowering).
- **Studio / Publish / Agent**: `cuni check --only py,go,js` (override with `CUNI_PLAYGROUND_CHECK_ONLY`).
- **Why**: the Fly image ships `python3` / `go` / `node` only. Running the full catalog false-FAILed spend-control on missing `c`/`cpp`/`rs` runners (`os error 2`) even when py/go/js matched — that fought the flagship promise and the publish metadata `targets: ["py","go","js"]`.
- **Not softening**: identical stdout on the gated seats is still required; refuse still refuses. Optional native seats are verified where their toolchains exist (local/CI), not by pretending the Studio VM has gcc/rustc.

## Not live yet / parked (honest)
- **Rider→Studio Execute wire**: Agent-Rider [PR #62](https://github.com/ceedot-rock/Agent-Rider/pull/62) **OPEN** — env-gated (`CUNI_STUDIO_PASS_REQUIRED`), **default OFF**. Not Fly-live required until Cos **GREEN** + Ship. Do not claim soft “always live” while default is off.
- **Attestation 501 / sealed Nitro**: **PARKED** — not a live citizen claim.
- **IR (M2)**: `src/ir.rs` is an **unfinished stub**; **emit does not consume IR**. See [`docs/milestones/M2_IR_SKETCH.md`](milestones/M2_IR_SKETCH.md).
- **PCC**: lossless **compressor-only** — never Fund / never paywall. TNSSRC board figure **43.72M** (do not invent other metrics).

## Known gap (2026-09-11)
- **Remote register re-verify** still pending: Rider-side **GRANT / `service_role`** match for `cuni_contracts` must be confirmed so Studio→Fly register can be re-proven end-to-end after the Vercel 402 dead-door era. Local stub remains healthy; health may already show `rider.remote: true` when the Fly app answers.

## Highest-leverage open
1. Cos GREEN + Ship on Agent-Rider PR #62 (Execute→Studio `/api/pass`, env-gated) — then optional Fly `CUNI_STUDIO_PASS_REQUIRED=true`  
2. Confirm Rider GRANT/`service_role` + re-verify remote `POST /api/v0/contracts` from Studio Publish  
3. Packaging — expand release matrix (darwin/aarch64) + public Homebrew tap once more assets exist  
4. M1 remaining / M2 IR — span fidelity landed incrementally; IR still unfinished  

## Portfolio context
Classic rolling 10-Steps (daily focus) lives in the collaborative work system.  
Primary commercial track is **SlidPhi** freemium + dual human/agent surfaces. CuNi + Agent-Rider remain the exactness + coordination core.

## Paused
- Sunday investor email to Bob unless re-enabled

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

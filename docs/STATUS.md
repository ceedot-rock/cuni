# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-08-12 · **v0.1.7 tagged** · Studio → Rider cutover live 2026-08-07

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge  
2. Exactness as the trust / verification gate  
3. Shared authoring + progressive deployment (Studio → Rider)

## Live today
- **Studio**: https://cuni-studio.fly.dev/ — Progress + Publish; spend-control default; Agent `spend` skill; free to try  
- **Exactness CI**: green on every push (examples + cargo test)  
- **Publish → register**:  
  - Local stub: `/api/rider/register` + `/api/rider/registered`  
  - Remote: `CUNI_RIDER_URL=https://agentrider.vercel.app` → `POST /api/v0/contracts` (exactness-gated, idempotent on sourceHash)  
  - Verified 2026-08-07: contracts `ctr_1ec3e1bdb32541f0` (studio) + smoke  
- **Health**: `/api/health` exposes `rider.register`, `rider.list`, `rider.remote`, `rider.remote_url`  
- **Flagship proofs**: exactness (py/go/js identical stdout) + `link` interop + spend skill  
- **Packaging**: draft only — see [`docs/PACKAGING.md`](PACKAGING.md) + `packaging/homebrew/cuni.rb`  
  Preferred install today: `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.7`

## Known issue (2026-08-12)
- Rider `/api/v0/contracts` currently returns HTTP 402 / DEPLOYMENT_DISABLED from the Vercel edge. Local stub remains healthy. Re-check deployment / billing / x402 gating on Agent-Rider.

## Highest-leverage open
1. **Surface registered contracts in Studio UI** (health footer already has hooks; make count + recent contracts visible + link to Rider) + keep E2E docs current → Step 2 of classic 10  
2. **Packaging** — real Homebrew + cargo-binstall / release assets (one-command install)  
3. **Studio first-impression polish** — guided tour / more flagship examples if needed  
4. **CLI / Studio error polish** — concrete fix-its on type and exactness failures  

## Portfolio context
Classic rolling 10-Steps (daily focus) lives in the collaborative work system.  
Primary commercial track is now **SlidPhi** freemium (free first 100 GB) + dual human/agent surfaces. CuNi + Agent-Rider remain the exactness + coordination core.

## Paused
- Sunday investor email to Bob unless re-enabled

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

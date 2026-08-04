# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-08-04 · **v0.1.7 tagged**

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge  
2. Exactness as the trust / verification gate  
3. Shared authoring + progressive deployment (Studio → Rider)

## Live today
- **Studio**: https://cuni-studio.fly.dev/ — Progress + Publish; spend-control default; Agent `spend` skill  
- **Exactness CI**: green on every push (examples + cargo test)  
- **Publish → register**: Studio-side Rider stub live (`/api/rider/register`, `/api/rider/registered`)  
- **Flagship proofs**: exactness (py/go/js identical stdout) + `link` interop demo  
- **Packaging**: draft only — see [`docs/PACKAGING.md`](PACKAGING.md) + `packaging/homebrew/cuni.rb`  
  Preferred install today: `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.7`

## Highest-leverage open (CuNi S2S focus)
1. **Packaging** — ship real Homebrew formula + cargo-binstall / release assets (one-command install)  
2. **Full Agent-Rider cutover** — replace Studio publish stub with real Rider identity + messaging  
3. **Studio first-impression** — additional flagship examples + short guided tour  
4. **Examples gallery** — clear index of spend-control / link / agent skills  
5. **CLI / Studio error polish** — concrete fix-its on type and exactness failures  
6. **CI matrix expansion** — more targets / benchmarks when release assets exist  

## Portfolio context
Classic rolling 10-Steps (daily focus) and S2S50 (broader) live in the collaborative work system.  
CuNi remains primary with Agent-Rider; SlidPhi elevated for commercial track.

## Paused
- Sunday investor email to Bob unless re-enabled

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

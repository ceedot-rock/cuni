# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28 (afternoon)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## Progress
- Studio live: Progress + Publish, defaults to spend-control
- Agent `spend` skill: speech → exactness → multi-runtime
- `playground/rider_stub.py` **mounted** in `server.py`:
  - `POST /api/rider/register`
  - `GET /api/rider/registered`
  - `/api/publish` auto-registers into the stub after exactness PASS
- **Next**: smoke-test on Fly after redeploy; real Agent-Rider `/api/v0/contracts` later

## What is live / in the repo
- [CuNi Studio](https://cuni-studio.fly.dev/)
- Agent skill `spend` (CheckSpend)
- Publish flow docs + refusal examples + technical candidates
- Rider stub: [`playground/rider_stub.py`](../playground/rider_stub.py) + routes in `server.py`
- API design: [`docs/RIDER_REGISTRATION_API.md`](RIDER_REGISTRATION_API.md)

## Still open
- Smoke-test full publish → register → list on live Studio
- Real Rider service (replace stub with `POST /api/v0/contracts`)
- Multi-step speech polish (optional)

## How we work
Rolling 10 Steps. External feedback paused.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

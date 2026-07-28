# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28 (morning)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## Progress
- Studio live: Progress + Publish, defaults to spend-control
- Agent `spend` skill: speech → exactness → multi-runtime
- `playground/rider_stub.py` implements register + list (ready to mount)
- **Next**: wire rider_stub into server.py (~4 lines) + redeploy

## What is live / in the repo
- [CuNi Studio](https://cuni-studio.fly.dev/)
- Agent skill `spend` (CheckSpend)
- Publish flow docs + refusal examples + technical candidates
- Rider stub module: [`playground/rider_stub.py`](../playground/rider_stub.py)
- Wire-up instructions: [`docs/RIDER_REGISTRATION_API.md`](RIDER_REGISTRATION_API.md)

## Still open
- Mount rider_stub in server.py + redeploy (needs FLY_API_TOKEN or manual flyctl)
- Smoke-test full publish → register → list loop
- Multi-step speech polish (optional)

## How we work
Rolling 10 Steps. External feedback paused.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

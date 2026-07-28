# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28 (morning — register loop live)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## Progress
**Publish → register loop is live and smoke-tested.**

- Studio: Progress + Publish, defaults to spend-control
- Agent `spend` skill: speech → exactness → multi-runtime
- Rider stub mounted and deployed:
  - `POST /api/rider/register`
  - `GET /api/rider/registered` (live count ≥ 1)
  - `/api/publish` auto-registers after exactness PASS

## What is live
- [CuNi Studio](https://cuni-studio.fly.dev/)
- Agent skill `spend` (CheckSpend)
- Rider stub endpoints + publish auto-register
- Publish flow / refusal examples / technical candidates

## Still open (optional / next)
- Multi-step speech polish
- Real Agent-Rider service (`POST /api/v0/contracts`)
- Sunday investor email (draft ready)

## How we work
Rolling 10 Steps. External feedback paused.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

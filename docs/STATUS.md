# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28 (morning)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## Progress
This morning’s focus: Studio defaults to spend-control, Agent-mode `spend` skill (speech → exactness → multi-runtime), publish flow docs, refusal examples. External feedback temporarily paused.

## What is live / in the repo
- [CuNi Studio](https://cuni-studio.fly.dev/) — Progress + Publish live; defaults to spend-control
- Agent skill `spend` (CheckSpend / can_spend) — speech `spend 4 cap 5` works across py/go/js
- Publish prototype + artifact + [PUBLISH_FLOW.md](PUBLISH_FLOW.md)
- [EXACTNESS_REFUSAL_EXAMPLES.md](EXACTNESS_REFUSAL_EXAMPLES.md)
- [TECHNICAL_IMPROVEMENT_CANDIDATES.md](TECHNICAL_IMPROVEMENT_CANDIDATES.md)
- Rider registration design + Studio-side stub path documented

## Still open (highest leverage)
- Live `/api/rider/register` stub in server.py + redeploy
- Registered-contracts list/view
- Multi-step speech edge cases
- Sunday investor outline (in progress)

## How we work
Rolling 10 Steps to Success. External feedback steps paused for now.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

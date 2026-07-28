# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28 (morning — post investor email)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## Progress
**Publish → register loop is live.** Investor update sent to Bob.

- Studio: Progress + Publish; defaults to spend-control
- Agent `spend` skill first in dropdown; speech → exactness → multi-runtime
- Rider **Studio stub only** for now (real Rider deferred until needed)
  - `POST /api/rider/register` · `GET /api/rider/registered`
  - Publish auto-registers after exactness PASS

## Decision (2026-07-28)
Keep the Studio-side Rider stub. Do **not** start a separate Rider service yet. Scaffold remains in `docs/RIDER_SERVICE_SCAFFOLD.md` when the need is real.

## Still open
- Redeploy for: health `rider` field, footer `registered: N`, Publish tooltip, spend-first dropdown
- Tag v0.1.7 when ready (`docs/RELEASE_NOTES_v0.1.7.md`)
- Screenshot/GIF of full path for next investor note

## How we work
Rolling 10 Steps. External feedback paused. CuNi primary; MidPoint subordinate.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

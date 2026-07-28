# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28 (morning)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## What is live / in the repo
- [CuNi Studio](https://cuni-studio.fly.dev/) (hosted playground + Agent mode)
- **Progress link + Publish button are live** on the Studio
- Flagship example: [`examples/spend-control.cuni`](../examples/spend-control.cuni) (already in Studio example dropdown via `/api/examples`)
- Publish prototype: [`scripts/publish-prototype.sh`](../scripts/publish-prototype.sh) + existing `examples/spend-control.publish.json` artifact
- Getting Started: [GETTING_STARTED_AGENT_RIDER.md](GETTING_STARTED_AGENT_RIDER.md)
- Rider registration API design: [RIDER_REGISTRATION_API.md](RIDER_REGISTRATION_API.md)
- Automatic Studio deploy workflow (GitHub Action)

## Still open (highest leverage)
- Collect first real external feedback on the Studio
- Fully wire spend-control into Agent-mode host (speech → exactness → multi-runtime end-to-end; budget skill already covers speech routing)
- Studio first-impression polish (exactness promise in < 30 seconds)
- Minimal working Rider registration endpoint
- Highest-leverage technical improvement inside CuNi

## How we work
We maintain a living 10 Steps to Success list. When 6 are completed, the next 6 are generated. All significant work is logged and feeds the weekly investor update.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

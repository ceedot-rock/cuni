# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## What is live / in the repo
- [CuNi Studio](https://cuni-studio.fly.dev/) (hosted playground + Agent mode)
- Flagship example: [`examples/spend-control.cuni`](../examples/spend-control.cuni)
- Publish prototype: [`scripts/publish-prototype.sh`](../scripts/publish-prototype.sh)
- Getting Started: [GETTING_STARTED_AGENT_RIDER.md](GETTING_STARTED_AGENT_RIDER.md)
- Rider registration API design: [RIDER_REGISTRATION_API.md](RIDER_REGISTRATION_API.md)
- Automatic Studio deploy workflow (GitHub Action)
- Progress link + Publish button already in Studio source (awaiting deploy)

## Still open (highest leverage)
- Redeploy Studio so Progress link and Publish button are live
- Collect first real external feedback
- Fully wire spend-control into Agent-mode host + Studio example dropdown
- Live end-to-end test of the publish prototype
- Studio first-impression polish (exactness promise in < 30 seconds)
- Minimal working Rider registration endpoint

## How we work
We maintain a living 10 Steps to Success list. When 6 are completed, the next 6 are generated. All significant work is logged and feeds the weekly investor update.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

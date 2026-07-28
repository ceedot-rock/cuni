# CuNi + Agent-Rider – Current Status

**Last updated**: 2026-07-28 (morning)

## Vision
Exact multi-runtime agents, coordinated.

## Locked Integration Model
1. CuNi `link` as the primary interop bridge
2. Exactness as the trust / verification gate
3. Shared authoring + progressive deployment (Studio → Rider)

## Progress (10 Steps)
`████████░░` **8 / 10** recently completed in the last two cycles (Studio live, spend-control default, publish flow docs, first-impression, README try path, status page, handoff, candidates note).  
Current open list is maintained in the daily-dev-report skill; next batch generates when 6 of the present 10 are marked done.

## What is live / in the repo
- [CuNi Studio](https://cuni-studio.fly.dev/) (hosted playground + Agent mode)
- **Progress link + Publish button are live**
- Flagship example defaults to [`spend-control.cuni`](../examples/spend-control.cuni)
- Publish prototype + artifact: [`scripts/publish-prototype.sh`](../scripts/publish-prototype.sh), [`examples/spend-control.publish.json`](../examples/spend-control.publish.json)
- Publish flow docs: [PUBLISH_FLOW.md](PUBLISH_FLOW.md)
- Technical candidates: [TECHNICAL_IMPROVEMENT_CANDIDATES.md](TECHNICAL_IMPROVEMENT_CANDIDATES.md)
- Getting Started + Rider registration design docs present
- Automatic Studio deploy workflow (GitHub Action)

## Still open (highest leverage)
- Collect first real external feedback on the Studio
- Fully wire spend-control into Agent-mode host (speech → exactness → multi-runtime)
- Minimal working Rider registration endpoint (or stub)
- Ship the single highest-leverage technical improvement (see candidates doc)
- Tighten Agent speech parsing edge cases

## How we work
We maintain a living 10 Steps to Success list. When 6 are completed, the next 6 are generated. All significant work is logged and feeds the weekly investor update.

---
CuNi + Agent-Rider — exact multi-runtime agents, coordinated.

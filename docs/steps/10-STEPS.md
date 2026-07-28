# 10 Steps to Success — CuNi + Agent-Rider

**Rule:** Always keep **exactly 10 open** steps. When **6 are completed**, generate the next 6.  
**Focus:** CuNi (language + Studio) + Agent-Rider. MidPoint is local/subordinate.

**Updated:** 2026-07-28 (post v0.1.7 + keys available)

## Open (exactly 10)

| # | Step | Status | Notes |
|---|------|--------|-------|
| 1 | **Rider `POST /api/v0/contracts` beyond Studio stub** | open | Real Agent-Rider service (not only Studio in-memory stub) |
| 2 | **spend-control end-to-end: Studio → register → CheckSpend** | open | Prove full loop with live Rider |
| 3 | **Quota env verified on Fly (free/agent caps)** | open | Confirm env + soft/hard limit behavior in production |
| 4 | **Agent Chat polish (errors + quota messaging)** | open | Stable UX under failure |
| 5 | **Publish download / share UX** | open | Download `.publish.json` after PASS |
| 6 | **Getting Started linked from Studio empty-state** | open | One-click path for new visitors |
| 7 | **Organic distribution (Studio CTA)** | open | Human posts only — X @CoreXeroC / Show HN; no tip desks |
| 8 | **daily-dev-report habit** | open | Log under `docs/steps/daily-dev-report/` |
| 9 | **Capture Studio demo GIF / short clip** | open | For README + outreach |
| 10 | **Free-to-try clarity (pay-as-you-go Fly)** | open | Corey direction: make try free; document limits |

## Recently completed (archive)

| Done | Evidence |
|------|----------|
| Narrative CuNi + Agent-Rider | README |
| Link tutorial / Getting Started | docs |
| spend-control example | `examples/spend-control.cuni` |
| Publish prototype + Studio Publish button | `/api/publish` |
| Rider Studio stub (register + list + health) | `rider_stub.py` |
| Publish → auto-register on PASS | Studio |
| FLY_API_TOKEN + auto deploy | GH secrets + workflow |
| **v0.1.7 tagged** | release 2026-07-28 |
| Progress header + spend-first UX | Studio live |

## Notes
- Keys (XAI / OpenWeather / Fly) now available on host — other Grok can use configured env; **never email secrets**.
- Sunday investor email to Bob is **paused** unless Corey explicitly re-enables.
- When 6 of the open 10 are done → roll next 6.

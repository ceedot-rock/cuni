# 10 Steps to Success — CuNi + Agent-Rider

**Rule:** Always keep **exactly 10 open** steps. When **6 are completed**, generate the next 6 logical steps.  
**Auditor:** Check work against this list; prefer finishing these over parallel side quests.  
**Focus:** CuNi (language + Studio) + Agent-Rider (coordination). Other projects = maintenance.

**Updated:** 2026-07-28

## Open (exactly 10)

| # | Step | Status | Notes |
|---|------|--------|-------|
| 1 | **Rider `POST /api/v0/contracts` v0 implement** | open | Design in `RIDER_REGISTRATION_API.md`; implement in Agent-Rider |
| 2 | **Wire publish output → Rider register** | open | Studio/CLI POST metadata to Rider after #1 |
| 3 | **spend-control end-to-end demo** | open | Studio → publish → register → call `CheckSpend` from Rider |
| 4 | **Getting Started linked from Studio UI** | open | Header/empty-state + default spend-control example |
| 5 | **Quota env live on Fly** | open | Confirm free/agent daily caps in Fly env |
| 6 | **Agent Chat polish (stable UX)** | open | Harden errors / quota messaging |
| 7 | **daily-dev-report habit** | open | Keep logging under `docs/steps/daily-dev-report/` |
| 8 | **Sunday investor note to Bob** | open | `tazzski41@gmail.com` — next Sunday morning |
| 9 | **Organic distribution (Studio CTA)** | open | X @CoreXeroC / Show HN — human; no tip desks |
| 10 | **Publish download / share UX** | open | Download `.publish.json` from Studio after PASS |

## Recently completed (archive — not counted in the 10)

| Done | Evidence |
|------|----------|
| Narrative CuNi + Agent-Rider | README section |
| Link tutorial sharpened | `LINK_TUTORIAL.md` |
| Getting Started page | `GETTING_STARTED_AGENT_RIDER.md` |
| spend-control example | `examples/spend-control.cuni` |
| Publish prototype script | `scripts/publish-prototype.sh` |
| Rider registration API design | `RIDER_REGISTRATION_API.md` |
| Progress header link | Studio → `#cuni--agent-rider` |
| Auto Fly deploy workflow | `.github/workflows/deploy-studio.yml` |
| **FLY_API_TOKEN in GH secrets** | set 2026-07-28 |
| **Deploy Studio with Progress live** | fly + GH Actions deploy 2026-07-28 |
| **Studio Publish button live** | `/api/publish` smoke OK 2026-07-28 |

## Open count note

Step 1 just completed. Open list temporarily 9 until next batch — on next `s2s go`, either promote a new step into the 10 or wait until 6 done for a full roll.

## Roll rule

When **6** of the open 10 are done: mark them completed here, then append **6 new** open steps so the open list is again exactly 10.

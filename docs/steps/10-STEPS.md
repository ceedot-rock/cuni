# 10 Steps to Success — CuNi + Agent-Rider

**Rule:** Always keep **exactly 10 open** steps. When **6 are completed**, generate the next 6 logical steps.  
**Auditor:** Check work against this list; prefer finishing these over parallel side quests.  
**Focus:** CuNi (language + Studio) + Agent-Rider (coordination). Other projects = maintenance.

**Updated:** 2026-07-28

## Open (exactly 10)

| # | Step | Status | Notes |
|---|------|--------|-------|
| 1 | **Studio Publish button live** | in progress | `/api/publish` + UI → Rider metadata; deploy with playground paths |
| 2 | **Rider `POST /api/v0/contracts` v0 implement** | open | Design in `RIDER_REGISTRATION_API.md`; implement in Agent-Rider |
| 3 | **Wire publish output → Rider register** | open | After #2: Studio or CLI posts metadata to Rider |
| 4 | **spend-control end-to-end demo** | open | Studio check → publish → register → call `CheckSpend` from Rider host |
| 5 | **Getting Started linked from Studio UI** | open | Header or empty-state link to Getting Started + spend-control example default |
| 6 | **Quota env live on Fly** | open | Confirm `CUNI_PLAYGROUND_FREE_DAILY` / agent daily in Fly env + docs |
| 7 | **Agent Chat polish (stable UX)** | open | Chat panel + voice skeleton shipped; harden errors / quotas messaging |
| 8 | **daily-dev-report habit** | open | Log significant work in `docs/steps/daily-dev-report/` |
| 9 | **Sunday investor note to Bob** | open | `tazzski41@gmail.com` — weekly; next due Sunday morning |
| 10 | **Organic distribution (Studio CTA)** | open | X @CoreXeroC / Show HN — human; no tip desks |

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
| **Deploy Studio with Progress live** | fly deploy 2026-07-28 |

## Roll rule

When **6** of the open 10 are done: mark them completed here, then append **6 new** open steps so the open list is again exactly 10.

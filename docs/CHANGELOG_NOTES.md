# Changelog notes (for next tag)

Accumulate here; fold into formal release notes when tagging.

## Since v0.1.6 (unreleased)

### Studio
- Default example is `spend-control.cuni` (flagship exactness demo)
- First-impression tagline: write once → identical py/go/js **or refuse**
- Publish button: exactness gate → Rider metadata JSON
- Publish **auto-registers** into Studio-side Rider stub when mounted

### Agent mode
- New skill `spend` — speech `spend 4 cap 5` → `can_spend` / CheckSpend → exactness → py/go/js
- Multi-step speech via `plan_steps` (`then` / `;`)

### Rider path
- `playground/rider_stub.py` — `POST /api/rider/register`, `GET /api/rider/registered`
- Mounted in `server.py` and live on https://cuni-studio.fly.dev/
- Design sketch for real Rider: `docs/RIDER_V0_CONTRACTS.md`

### Docs
- `docs/PUBLISH_FLOW.md`, `docs/EXACTNESS_REFUSAL_EXAMPLES.md`
- `docs/TECHNICAL_IMPROVEMENT_CANDIDATES.md`, `docs/RIDER_REGISTRATION_API.md`
- `docs/STATUS.md` current focus

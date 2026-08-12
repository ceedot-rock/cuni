# E2E: Studio exactness → Publish → Agent^Rider

**Status:** cutover live as of 2026-08-07  
**Apps:** https://cuni-studio.fly.dev/ · https://agentrider.vercel.app/

## Flow

1. **Write / open** a CuNi source in Studio.
2. **Exactness** — Studio runs emit + `cuni check` across py/go/js.
3. **PASS only** — if exactness fails, `/api/publish` returns 400 and refuses.
4. **Publish** — `POST /api/publish` with `{ "source": "..." }`:
   - Builds metadata (`sourceHash`, exactness, publisher `studio`)
   - Registers local stub (fallback)
   - When `CUNI_RIDER_URL` is set, calls `register_remote(meta)` →  
     `POST https://agentrider.vercel.app/api/v0/contracts`
5. **Verify** contracts:

```bash
# Local stub (always available)
curl -s https://cuni-studio.fly.dev/api/rider/registered | jq .

# Health (shows remote status)
curl -s https://cuni-studio.fly.dev/api/health | jq .rider

# Remote Rider (may return 402 if deployment gated)
curl -s https://agentrider.vercel.app/api/v0/contracts | jq .
```

## Studio UI surface (open work — Step 2)

Goal: make registered contracts visible without leaving Studio.

Recommended minimal changes:

1. **Footer / health strip**  
   Already polls `/api/health` and `/api/rider/registered`. Surface:
   - `registered: N` (from local stub or remote when available)
   - Small badge: `Rider remote: on/off`
   - Click → expand last 3 contracts (id, sourceHash short, registeredAt)

2. **Publish success toast**  
   After successful publish, show `Registered: <contractId>` + link to Rider contracts page (or local list).

3. **Optional contracts panel**  
   New tab or side drawer listing local + remote contracts (read-only).

4. **Docs link**  
   Keep this file + `RIDER_CUTOVER.md` linked from Progress / README.

Implementation notes:
- Prefer local stub count for the primary badge (always works).
- When remote is healthy, prefer remote list and show both.
- Do not block Publish on remote failure; fall back to stub.

## Smoke (API)

```bash
# Health should show remote=true
curl -s https://cuni-studio.fly.dev/api/health | jq '.rider'

# Local registered list
curl -s https://cuni-studio.fly.dev/api/rider/registered | jq '{ok, count}'

# Publish a known-good example (from repo root)
curl -s -X POST https://cuni-studio.fly.dev/api/publish \
  -H 'Content-Type: application/json' \
  -d "{\"source\": $(python3 -c 'import json;print(json.dumps(open("examples/agent/spend_control.cuni").read()))')}" \
  | jq '{ok, rider}'
```

## Rules

- Rider only accepts `exactness.passed === true` (Studio enforces before POST).
- Idempotent on `sourceHash` on Rider.
- Local stub remains if remote is down.

## Verified

| When | Result |
|------|--------|
| 2026-08-07 | `ctr_1ec3e1bdb32541f0` from Studio publish; contracts count 2 |
| 2026-08-12 | Local stub healthy (`count: 2`). Remote `/api/v0/contracts` returns HTTP 402 (DEPLOYMENT_DISABLED). Re-check Agent-Rider Vercel deployment / billing / x402. |

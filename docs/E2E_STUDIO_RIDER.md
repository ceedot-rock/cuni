# E2E: Studio exactness → Publish → Agent^Rider

**Status:** live as of 2026-08-07  
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
5. **Verify** contracts on Rider:

```bash
curl -s https://agentrider.vercel.app/api/v0/contracts | jq .
# or from Studio:
curl -s https://cuni-studio.fly.dev/api/health | jq .rider
curl -s https://cuni-studio.fly.dev/api/rider/contracts | jq .
```

## Smoke (API)

```bash
# Health should show remote=true and contracts.count >= 1
curl -s https://cuni-studio.fly.dev/api/health | jq '.rider'

# Publish a known-good example
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

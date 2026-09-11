# E2E: Studio exactness → Publish → Agent^Rider

**Status:** cutover live; Studio contracts UI shipped 2026-09-11 (v0.1.9)  
**Apps:** https://cuni-studio.fly.dev/ · https://agentrider.fly.dev/ (Fly-only; Vercel 402 is a historical dead door)

## Flow

1. **Write / open** a CuNi source in Studio.
2. **Exactness** — Studio runs emit + `cuni check` across py/go/js.
3. **PASS only** — if exactness fails, `/api/publish` returns 400 and refuses.
4. **Publish** — `POST /api/publish` with `{ "source": "..." }`:
   - Builds metadata (`sourceHash`, exactness, publisher `studio`)
   - Registers local stub (fallback)
   - When `CUNI_RIDER_URL` is set, calls `register_remote(meta)` →  
     `POST https://agentrider.fly.dev/api/v0/contracts`
5. **Verify** contracts:

```bash
# Local stub (always available)
curl -s https://cuni-studio.fly.dev/api/rider/registered | jq .

# Health (shows remote status)
curl -s https://cuni-studio.fly.dev/api/health | jq .rider

# Remote Rider (Fly-only live face; re-verify pending GRANT/service_role)
curl -s https://agentrider.fly.dev/api/v0/contracts | jq .
```

## Studio UI surface (shipped — Step 2)

Registered contracts are visible in Studio without leaving the page:

1. **Contracts panel** — count + recent rows (`id`, `sourceHash`, `registeredAt`, `status`); empty state when `count=0`
2. **Health strip** — `registered: N` · `rider remote: on/off` · `langs: 119`
3. **Rider link** — prefers `health.rider.remote_url`, falls back to https://agentrider.fly.dev
4. **Docs** — this file + [`STATUS.md`](STATUS.md) + [`RIDER_CUTOVER.md`](RIDER_CUTOVER.md)

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
| 2026-08-12 | Local stub healthy (`count: 2`). Remote Vercel edge returned HTTP 402 (historical dead door). |
| 2026-09-11 | Fly-only Rider face https://agentrider.fly.dev. Studio UI surfaces `/api/rider/registered`. Remote register re-verify still pending GRANT/`service_role` match on Rider. |

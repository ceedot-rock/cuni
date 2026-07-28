# Real Rider service scaffold

**Status**: Scaffold only. Studio stub remains the live stand-in.

## Purpose
Replace `POST /api/rider/register` with a real Agent-Rider service that owns contracts, identity, and invoke paths.

## Suggested layout (new repo or `rider/` package)

```
rider/
  cmd/rider/main.go          # or Python/FastAPI — language flexible
  internal/
    contracts/
      store.go               # sourceHash → contractId, idempotent
      accept.go              # exactness.passed gate
    api/
      v0_contracts.go        # POST/GET /api/v0/contracts
      v0_invoke.go           # POST /api/v0/contracts/{id}/invoke (later)
  docs/
    OPENAPI.yaml             # optional
```

## v0 acceptance rules (copy from Studio stub)
1. Body is publish `meta` (or `{ "meta": … }`).
2. Refuse unless `exactness.passed === true`.
3. Idempotent on `sourceHash`.
4. Return `{ ok, contractId, status, sourceHash }`.

## Env
- `RIDER_DATA` — persistence root
- `RIDER_SHARED_SECRET` — optional v0 auth header
- `CUNI_STUDIO_ORIGIN` — CORS allowlist for Studio

## Cutover
1. Deploy Rider with `/api/v0/contracts`.
2. Point Studio publish auto-POST at Rider base URL (env `CUNI_RIDER_URL`).
3. Keep Studio stub as fallback when `CUNI_RIDER_URL` is unset.

See [RIDER_V0_CONTRACTS.md](RIDER_V0_CONTRACTS.md) and [RIDER_REGISTRATION_API.md](RIDER_REGISTRATION_API.md).

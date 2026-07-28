# Real Rider `/api/v0/contracts` (sketch)

**Status**: Design only. Studio stub (`POST /api/rider/register`) is the stand-in until a real Agent-Rider service exists.

## Goal
Accept a verified CuNi publish payload and make the `link` contract callable across languages inside Rider.

## Endpoint

```
POST /api/v0/contracts
Content-Type: application/json
```

### Request body (same shape as Studio publish `meta`)

```json
{
  "version": "0.1",
  "source": "…",
  "sourceHash": "sha256…",
  "exactness": {
    "passed": true,
    "checkedAt": "2026-07-28T…Z",
    "targets": ["py", "go", "js"],
    "stdoutMatch": true
  },
  "publishedAt": "…",
  "publisher": "studio"
}
```

### Rules
1. Refuse if `exactness.passed` is not `true`.
2. Idempotent on `sourceHash` (return existing contract id).
3. Issue a stable `contractId` and optional remote-client stubs for py/go/js.
4. Store the source + hash for audit.

### Response (success)

```json
{
  "ok": true,
  "contractId": "ctr_…",
  "status": "active",
  "sourceHash": "…",
  "endpoints": {
    "invoke": "/api/v0/contracts/ctr_…/invoke"
  }
}
```

## Migration from Studio stub

| Studio (now) | Real Rider (later) |
|--------------|--------------------|
| `POST /api/rider/register` | `POST /api/v0/contracts` |
| `GET /api/rider/registered` | `GET /api/v0/contracts` |
| Local volume `registered/` | Rider store + identity |

Studio can keep auto-registering into the local stub; a future switch flips the publish `next` / auto-POST target to the real Rider base URL.

## Non-goals (v0)
- Versioning / deprecation policy
- Multi-tenant auth beyond a simple shared secret
- Streaming `link`

See also: [RIDER_REGISTRATION_API.md](RIDER_REGISTRATION_API.md), [PUBLISH_FLOW.md](PUBLISH_FLOW.md).

# Rider Registration API (v0)

**Status**: Design complete. Studio-side stub **implemented and mounted** in `server.py` (`POST /api/rider/register`, `GET /api/rider/registered`; publish auto-registers).

## Purpose
Accept a verified CuNi `link` contract (the metadata produced by the publish prototype) and make it available for cross-language calls inside Agent-Rider.

## Studio stub (ready now)

Module: [`playground/rider_stub.py`](../playground/rider_stub.py)

```
POST /api/rider/register
Content-Type: application/json
```

Body: publish metadata (or `{ "meta": { ... } }`).

Rules:
- `exactness.passed` **must** be `true` → otherwise 400
- Idempotent on `sourceHash`
- Stores under Studio data volume `registered/`

```
GET /api/rider/registered
```

Returns `{ "ok": true, "count": N, "contracts": [ ... ] }`.

### Wire-up (server.py)

```python
from rider_stub import handle_register, handle_list_registered

# do_GET:
if path == "/api/rider/registered":
    return self._json(*handle_list_registered(DATA))

# do_POST:
if path == "/api/rider/register":
    return self._json(*handle_register(data, DATA, append_note))
```

Also update the Publish success `next` string to:
`POST /api/rider/register with the returned meta`

## Conceptual Rider endpoint (future)

```
POST /api/v0/contracts
```

Same metadata shape. Real Rider will issue contract IDs and remote client stubs.

## Notes
Deliberately minimal. No versioning/deprecation yet.

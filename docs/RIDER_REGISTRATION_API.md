# Rider Registration API (v0)

**Status**: Design complete. Ready for implementation.

## Purpose
Accept a verified CuNi `link` contract (the metadata produced by the publish prototype) and make it available for cross-language calls inside Agent-Rider.

## Endpoint (conceptual)

```
POST /api/v0/contracts
Content-Type: application/json
```

### Request body (matches the publish metadata shape)

```json
{
  "version": "0.1",
  "source": "... full .cuni source ...",
  "sourceHash": "sha256...",
  "exactness": {
    "passed": true,
    "checkedAt": "2026-07-28T00:00:00Z",
    "targets": ["py", "go", "js"],
    "stdoutMatch": true
  },
  "publishedAt": "2026-07-28T00:00:00Z",
  "publisher": "studio|cli|manual"
}
```

### Rules
- `exactness.passed` **must** be `true`. Anything else is rejected with 400.
- `sourceHash` is the primary key. Duplicate hashes are idempotent (return existing contract ID).
- Rider stores the source + metadata; it does **not** re-compile in v0.

### Response

```json
{
  "contractId": "cl_...",
  "status": "registered",
  "links": [
    {
      "name": "CheckSpend",
      "remote": {
        "py": "CheckSpend_remote",
        "go": "CheckSpend_remote",
        "js": "CheckSpend_remote"
      }
    }
  ]
}
```

## Client helpers (future)
Rider can later expose generated client stubs or import paths so agents in any of the three languages can call the contract with the existing `*_remote` pattern.

## Notes
This API is deliberately minimal. It does not yet handle versioning, deprecation, or richer types. Those come later without breaking this surface.

# Studio → Rider Publish Flow

**Last updated**: 2026-07-28

## Goal
Exactness-gated path from authoring in CuNi Studio to metadata that Agent-Rider can register and run.

## Live path (Studio UI)

1. Open https://cuni-studio.fly.dev/
2. Source loads with `spend-control.cuni` by default (or pick any example).
3. Click **Run exactness** (or Check).
4. When status shows **exactness PASS**, click **Publish**.
5. Studio runs the same exactness gate again, then emits a metadata JSON blob:
   - `source` + `sourceHash`
   - `exactness.passed`, `checkedAt`, `targets`, `stdoutMatch`
   - `publishedAt`, `publisher: "studio"`
6. The JSON is stored under the Studio data volume and shown in the Python tab for copy/paste.
7. Studio **auto-registers** into the local Rider stub (`POST /api/rider/register` equivalent) and returns `registration.id`.
8. List contracts: `GET /api/rider/registered`. Real Rider later: `POST /api/v0/contracts`.

## CLI prototype (same contract)

```bash
./scripts/publish-prototype.sh examples/spend-control.cuni
# → exactness PASS
# → examples/spend-control.publish.json
```

## Metadata shape (v0.1)

See `docs/RIDER_REGISTRATION_API.md` and the existing `examples/spend-control.publish.json` artifact.

## Why this matters

- Exactness is the citizenship gate: nothing reaches Rider without a proven identical py/go/js result.
- The metadata is the portable contract between Studio (authoring) and Rider (coordination).
- No approximate mode; refuse is the correct outcome when targets diverge.

# Full Agent-Rider Cutover (2026-08-04)

## What shipped

### Agent-Rider (live code)
- `src/lib/cuni-contracts.ts` — register / list / get exactness-gated contracts
- `src/app/api/v0/contracts/route.ts` — `POST` + `GET /api/v0/contracts`
- `supabase/cuni_contracts.sql` — table DDL (apply in Supabase SQL editor)

### CuNi
- `playground/rider_client.py` — POSTs publish meta to real Rider when `CUNI_RIDER_URL` is set
- Integration into `playground/server.py` (see restore steps below if needed)

## Apply schema (required once)

In Supabase SQL editor for the Agent-Rider project, run:

```sql
-- contents of supabase/cuni_contracts.sql
CREATE TABLE IF NOT EXISTS cuni_contracts (
  id            TEXT PRIMARY KEY,
  source_hash   TEXT NOT NULL UNIQUE,
  source        TEXT NOT NULL,
  exactness     JSONB NOT NULL DEFAULT '{}',
  links         JSONB NOT NULL DEFAULT '[]',
  publisher     TEXT NOT NULL DEFAULT 'studio',
  published_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  status        TEXT NOT NULL DEFAULT 'active',
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS cuni_contracts_created ON cuni_contracts(created_at DESC);
GRANT ALL ON TABLE cuni_contracts TO service_role;
```

## Studio wiring

1. Restore full `playground/server.py` if it was overwritten:

```bash
curl -sL https://raw.githubusercontent.com/ceedot-rock/cuni/d36b63652f697c73844844ea538bdec9acd747f8/playground/server.py \
  -o playground/server.py
```

2. Ensure `playground/rider_client.py` is present (already on master).

3. Apply the three integration points in `server.py`:
   - Import: `from rider_client import register_remote`
   - In `/api/publish` success path: call `register_remote(meta)` and include result as `rider` in the JSON response
   - Health: expose `rider.remote` / `rider.remote_url` from `CUNI_RIDER_URL`

4. Set env on Fly (Studio):

```bash
fly secrets set CUNI_RIDER_URL=https://agentrider.vercel.app -a cuni-studio
```

5. Redeploy Studio.

## End-to-end path after cutover

1. Open Studio → Run exactness → Publish
2. Studio posts to `POST https://agentrider.vercel.app/api/v0/contracts`
3. Rider stores contract (idempotent on `sourceHash`), returns `contractId`
4. List: `GET /api/v0/contracts`
5. Local stub still available at `/api/rider/register` as fallback

## Rules (unchanged)
- `exactness.passed` must be `true` or Rider refuses
- Idempotent on `sourceHash`

# Private Studio workspace — design sketch (not built)

**Goal:** first paid surface = private place for policies + proof history.  
**Status:** design only. Public Studio remains free.

## Minimum features (v0)

| Feature | Why |
|---------|-----|
| Account / workspace | Isolation from public demo data |
| Save policy (source + name) | “Our cap rule” lives somewhere stable |
| Exactness run history | PASS/FAIL + timestamp per run |
| Publish / register log | What was filed, when, sourceHash |
| Read-only audit list | For a human or future compliance export |

## Explicit non-goals (v0)
- Full multi-tenant enterprise IAM  
- Real Rider multi-region service  
- Billing UI sophistication  
- Editing other people’s public demos  

## UX sketch
1. Sign in → land on **My policies** (list).  
2. Open or create → Studio editor (same exactness UX as today).  
3. Run exactness → append to **History**.  
4. Publish → append to **Register log** (private stub or future Rider).  

## Data (conceptual)
- `workspace_id`  
- `policies[]`: id, name, source, updated_at  
- `runs[]`: policy_id, exactness, ts, summary  
- `registers[]`: policy_id, sourceHash, ts, result  

## Build order (when funded / demanded)
1. Auth + workspace row  
2. Save/load source  
3. Persist run + register events  
4. Simple list UI  

Public demo path stays unchanged.

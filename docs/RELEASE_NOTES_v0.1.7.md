# CuNi v0.1.7 (draft)

**Tag when ready:**
```bash
git tag -a v0.1.7 -m "v0.1.7: Studio publish→register, Agent spend skill"
git push origin v0.1.7
```

Then create a GitHub Release from the tag using the notes below.

## Highlights

- **Studio → Rider path live**: Publish after exactness PASS; auto-register into Studio-side Rider stub (`POST /api/rider/register`, `GET /api/rider/registered`).
- **Agent `spend` skill**: speech `spend 4 cap 5` → CheckSpend / can_spend → exactness → py/go/js.
- **Default example**: `spend-control.cuni` for an immediate exactness demo.
- **Footer**: shows registered-contract count when the stub is available.

## Docs
- `docs/PUBLISH_FLOW.md`, `docs/RIDER_REGISTRATION_API.md`, `docs/RIDER_V0_CONTRACTS.md`
- `docs/EXACTNESS_REFUSAL_EXAMPLES.md`, `docs/CHANGELOG_NOTES.md`
- `docs/RIDER_SERVICE_SCAFFOLD.md` (future real Rider)

## Not in this tag
- Real Agent-Rider service (stub only)
- Health `rider` field (pending small server edit + redeploy)
- Breaking language changes

Fold from `docs/CHANGELOG_NOTES.md` when tagging.

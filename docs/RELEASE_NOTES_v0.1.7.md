# CuNi v0.1.7

**Tag commands:**
```bash
git pull origin master
git tag -a v0.1.7 -m "v0.1.7: Studio publish→register, Agent spend skill"
git push origin v0.1.7
```

Then create a GitHub Release from the tag using the notes below.

## Highlights

- **Studio → Rider path live**: Publish after exactness PASS; auto-register into Studio-side Rider stub (`POST /api/rider/register`, `GET /api/rider/registered`).
- **Agent `spend` skill**: speech `spend 4 cap 5` → CheckSpend / can_spend → exactness → py/go/js. First in Agent dropdown.
- **Default example**: `spend-control.cuni` for an immediate exactness demo.
- **UI polish**: footer shows registered-contract count; Publish tooltip explains the register path.

## Docs
- `docs/PUBLISH_FLOW.md`, `docs/RIDER_REGISTRATION_API.md`, `docs/RIDER_V0_CONTRACTS.md`
- `docs/EXACTNESS_REFUSAL_EXAMPLES.md`, `docs/CHANGELOG_NOTES.md`
- `docs/RIDER_SERVICE_SCAFFOLD.md` (future real Rider — deferred; stub only for now)

## Not in this tag
- Real Agent-Rider service (Studio stub only)
- Breaking language changes

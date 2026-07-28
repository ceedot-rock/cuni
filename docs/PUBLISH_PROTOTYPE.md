# Publish Prototype (Studio → Rider)

**Status**: Working prototype

This is the minimal path that takes a verified CuNi program and produces the metadata Agent-Rider will accept.

## One-command usage

```bash
# from repo root (after cargo build --release)
./scripts/publish-prototype.sh examples/spend-control.cuni
```

If exactness passes, it writes `examples/spend-control.publish.json` containing:

- full source
- source hash
- exactness result + timestamp
- publisher tag

## What it does
1. Runs `cuni check` on the given file.
2. If exactness **fails**, it refuses to publish.
3. If exactness **passes**, it writes the metadata JSON defined in the publish shape.

## Studio UI

In [CuNi Studio](https://cuni-studio.fly.dev/) (Play mode):

1. Load or write a program (e.g. example **spend-control**).
2. Click **Publish**.
3. Exactness must **PASS** or publish is refused.
4. Metadata is stored on the Studio volume and shown as JSON (copy for Rider).

API: `POST /api/publish` with `{ "source": "..." }`.

## Next
- Point Studio/CLI output at Rider `POST /api/v0/contracts` once implemented.
- Optional: download button for `.publish.json`.

See also: [RIDER_REGISTRATION_API.md](RIDER_REGISTRATION_API.md)

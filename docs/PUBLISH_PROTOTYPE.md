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

## Next
- Wire this into the Studio UI as a “Publish” button.
- Point the output at the Rider registration API once it exists.

See also: [RIDER_REGISTRATION_API.md](RIDER_REGISTRATION_API.md)

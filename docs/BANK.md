# CuNi Bank

Arm of CuNi. Paste N, get X. Exactness or refuse.

**119 languages is `cuni check` / `--emit-all` on the ingested `.cuni`. It is not 119 ingest parsers.** Bank v1 `--from` is `py` or `cuni` only.

Bank is not a second compiler. It is ingest → emit → prove.

## Law

- N is declared (`--from`). Detect-and-guess is not v1.
- X is a catalog id (`--to`).
- Ingest must produce `.cuni` in the Bank subset, or refuse with the line.
- Emit X from that `.cuni` only.
- Prove: X’s stdout must match CuNi gold. Else refuse.
- Receipt names the deposit by `source_hash` of N, not by path.

## v1 subset for N

Python only, same as `cuni ingest`:

- `def name(args):` with a `return` expr
- top-level `print(...)`
- ints, strings, `+ - *`, simple calls

Anything else: refuse.

## CLI

```
cuni bank paste IN --from py --to c [-o OUT]
```

## Fixtures

`examples/bank/`

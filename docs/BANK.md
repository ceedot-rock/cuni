# CuNi Bank

Arm of CuNi. Paste N, get X. Exactness or refuse.

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

Anything else: refuse. Go/JS/Rust/C ingest is dark until a subset lands.

## CLI

```
cuni bank paste IN --from py --to c [-o OUT]
cuni bank paste IN --from py --to py
```

Exit 0 only if ingest + typeck + prove (when the to-seat can run) pass.

Hook in `src/main.rs`: `mod bank;` and `if args[0] == "bank" { return bank::cmd_bank(&args[1..]); }`

## Fixtures

`examples/bank/` — tiny N files.

## Not in v1

- paste any repo
- PCCX deposit
- Studio `/bank` tab
- C/Rust ingest

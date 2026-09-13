# CuNi Bank 0.1.0 — release notes

Arm of CuNi. Paste N, get X. Exactness or refuse.

## Gate (measured)

`cuni bank paste examples/bank/add.py --from py --to <id>`

PASS on ten catalog ids:

py, go, js, ts, c, cpp, rs (native)
rb, php, pl (catalog lowerings, python3 — same as `cuni check`)

`source_hash=bd4067ac5fd8b550`

119 languages remains `cuni check` on the ingested deposit. Bank 0.1 does not add 119 ingest parsers.

## Install (after this lands on the tagged commit with `mod bank` in main.rs)

```
cargo install --git https://github.com/ceedot-rock/cuni --tag cuni-bank-0.1.0
cuni bank paste examples/bank/add.py --from py --to c
```

## Law

`docs/BANK.md`

Subset N: Python v1 ingest (or `.cuni`).
X: catalog id. Prove via `emit::exec_plan`.
Refuse if ingest, typeck, or stdout diverges.

## Not in 0.1.0

- Studio `/bank` tab
- crates.io separate package
- PCCX deposits
- from=rs / from=c ingest

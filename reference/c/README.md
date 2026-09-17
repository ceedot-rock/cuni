# CuNi C99 reference — ScanChunk exact-text

**Issue:** [cuni#17](https://github.com/ceedot-rock/cuni/issues/17)  
**Pairing:** [Agent-Rider#17](https://github.com/ceedot-rock/Agent-Rider/issues/17) / [Agent-Rider#22](https://github.com/ceedot-rock/Agent-Rider/pull/22) (`agent-rider-c/` SoT)

Rust remains the 119-language protocol home. This directory is a **reference only** so Agent-Rider C and the protocol refuse unknown keys the same way. Not a seventh framework. Citizenship (`exactness.passed` before Rider register) is unchanged.

## Wire form

```
CUNI ScanChunk
key=value
```

Banner is exact: `CUNI ScanChunk`. Same shape as Rider’s `agent-rider-c/cuni.c`.

## Error enum + wire (do not collapse)

| Enum (charggri lock) | Value | Wire via `cuni_err_str` | Meaning |
|---|---|---|---|
| **`CUNI_ERR_EXTRA`** | **3** | **`reject.extra`** | Unknown key, or malformed line without `=` |
| `CUNI_ERR_MISSING` | 4 | `reject.missing` | Required field absent |
| `CUNI_ERR_KIND` | 2 | `reject.kind` | Wrong / missing banner |
| `CUNI_ERR_EMPTY` | 1 | `reject.empty` | Empty input / no KV lines |

Spelling lock: the **enum constant** is named `CUNI_ERR_EXTRA`. The **Chamber wire** is `reject.extra`. Both are required; do not rename the enum to match the wire string or print the enum name as the wire token.

## Known keys (Agent-Rider#22 SoT)

| key | required | role |
|---|---|---|
| `url` | yes | scanned URL |
| `etag` | no | optional entity tag |
| `hash` | yes | content / source hash |
| `agent_id` | yes | scout / emitter agent |

Any other key → **`CUNI_ERR_EXTRA`** / wire **`reject.extra`**.  
Malformed line without `=` → same.  
Missing `url` / `hash` / `agent_id` → **`CUNI_ERR_MISSING`** / `reject.missing`.

## Build & golden tests

```sh
cd reference/c
make
make test
```

- `testdata/ok.chunk` → stdout `ok`, exit 0  
- `testdata/extra.chunk` → stdout `reject.extra`, stderr `CUNI_ERR_EXTRA`, exit 1  
- `testdata/malformed.chunk` → same as extra  
- `testdata/missing.chunk` → stdout `reject.missing`, exit 2  
- Enum value check: `(int)CUNI_ERR_EXTRA == 3` and `cuni_err_str` → `reject.extra`

CLI: `./cuni_scan_cli <file>`

## Layout

| file | purpose |
|---|---|
| `cuni.h` | `cuni_err` enum (`CUNI_ERR_EXTRA=3`), `scan_chunk`, API |
| `cuni_scan_chunk.c` | parser + `cuni_err_str` + format (adapted from Rider) |
| `cuni_scan_cli.c` | harness (stdout wire, stderr enum on extra) |
| `Makefile` | build + `make test` |
| `testdata/` | golden chunks |

## Source of truth

ScanChunk portions vendored/adapted from public [Agent-Rider#22](https://github.com/ceedot-rock/Agent-Rider/pull/22) `agent-rider-c/cuni.h` + `cuni.c` (branch `feat/sitescan-c99-rider`).

# CuNi C99 reference — ScanChunk exact-text

**Issue:** [cuni#17](https://github.com/ceedot-rock/cuni/issues/17)  
**Pairing:** [Agent-Rider#17](https://github.com/ceedot-rock/Agent-Rider/issues/17)

Rust remains the 119-language protocol home. This directory is a **reference only** so Agent-Rider C and the protocol refuse unknown keys the same way. Not a seventh framework. Citizenship (`exactness.passed` before Rider register) is unchanged.

## Wire form

```
CUNI ScanChunk
key=value
```

Same shape as Rider’s TypeScript `CUNI SettleHop` parser (`Agent-Rider` `src/lib/settle-hop.ts`): banner line, then `key=value` lines. Extra keys do not bind.

## Error token (locked)

| CuNi token | Rider Chamber wire | Meaning |
|---|---|---|
| **`CUNI_ERR_EXTRA`** | `reject.extra` | Unknown / extra key on exact-text form |

Spelling is exact: `CUNI_ERR_EXTRA` (charggri hop brief + cuni#17). Do not invent variants.

## Assumed known keys (minimal)

Protocol docs (`PROTOCOL.md` / `protocol.json`) do not yet list ScanChunk fields, and the local `agent-rider-c` tree is **not on GitHub** yet (empty scaffold only). Until Rider lands `cuni.c`, this reference assumes a minimal bindable set parallel to SettleHop (`hop_id`, `job_id`, …):

| key | role |
|---|---|
| `chunk_id` | chunk identity |
| `job_id` | open job |
| `agent_id` | scout / emitter agent |
| `hash` | content / source hash |
| `body` | chunk payload (single line) |

Any other key → parse fails with **`CUNI_ERR_EXTRA`**. Amend this table when `agent-rider-c` publishes the SoT key list; do not silently widen acceptance.

## Build & golden tests

```sh
cd reference/c
make
make test
```

- `testdata/ok.chunk` → prints `ok`, exit 0  
- `testdata/extra.chunk` (includes `credits=…`) → prints `CUNI_ERR_EXTRA`, exit 1  

CLI: `./cuni_scan_cli <file>`

## Layout

| file | purpose |
|---|---|
| `cuni.h` | `CUNI_ERR_EXTRA`, ScanChunk struct, API |
| `cuni_scan_chunk.c` | C99 parser |
| `cuni_scan_cli.c` | tiny harness |
| `Makefile` | build + `make test` |
| `testdata/` | golden chunks |

## Blockers noted

- **`agent-rider-c` missing from GitHub** — Agent-Rider#17 describes `cuni.c` / `chamber.c` / `rider.c` in a working tree; public repo has no sources yet (local clone only has an empty `agent-rider-c/tests`). Known keys above are documented assumptions, not Rider SoT.

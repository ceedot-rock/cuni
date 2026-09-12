# CuNi is 119 languages

Exactness applies to the whole catalog. A language in `src/langs.rs` is a seat: emit, run, identical stdout — or refuse.

## Native seats (toolchain of that language)

| id | compiler / runtime | backend |
|----|--------------------|---------|
| py | python3 | quality Python |
| go | go run | quality Go |
| js, ts | node | quality JavaScript |
| c | gcc | tagged `Val` C runtime |
| cpp | g++ | same lowering, compiled as C++ |
| rs | rustc | tagged `Val` Rust runtime |

`cuni check examples/full.cuni --only py,go,js,c,cpp,rs` is the native gate. It must PASS.

## The rest of the 119

Until a seat has a native backend, its artifact is a Python lowering so the language **still emit+runs** under `cuni check`. The receipt (`--receipt`) marks each id `native` or `lowering`. Lowering is a seat with a shared runtime, not a skip.

Next native seats to grow under the same law: `sh`, `pl`, `awk`, `sql`, `wat`.

## Commands this unlocks

```bash
cuni run file.cuni                      # in-process (no emit)
cuni run file.cuni --lang py            # emit+run one native seat
cuni check file.cuni --receipt          # ledger beside the source
cuni check file.cuni --only c,rs,py     # pin the run to seats
cuni ingest impl.py -o impl.cuni        # reverse: Python subset → CuNi or refuse
cuni prove file.cuni --against impl.py  # foreign code must match CuNi gold
```

`cuni run` is one seat. `cuni check` is the proof.

## Split across seats

`link` is still the process boundary: same function, Go server + Python client. `--only` pins *this check* to a subset of seats. Function-level `on go` syntax is next; until then the whole program is the citizen on every native seat.

## Agents

Law is CuNi. `examples/agent/host/run_agent.py` runs `cuni check` before effect. `--skip-check` is not a citizen.

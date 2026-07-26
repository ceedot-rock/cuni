# CuNi Playground (platform step 3)

Local Studio v0: edit CuNi → emit Python / Go / JS → run all three → **exactness PASS/FAIL**.

## Run

```bash
# from repo root
cargo build --release
python3 playground/server.py
# open http://127.0.0.1:8787
```

Env:

| Variable | Default | Meaning |
|----------|---------|---------|
| `CUNI_BIN` | auto `target/release|debug/cuni` | compiler path |
| `CUNI_PLAYGROUND_PORT` | `8787` | bind port |
| `CUNI_PLAYGROUND_HOST` | `127.0.0.1` | bind host |
| `CUNI_PLAYGROUND_TIMEOUT` | `45` | seconds per run |

## API

- `GET /api/health` — cuni + py/go/node availability  
- `GET /api/examples` — loads `examples/*.cuni`  
- `POST /api/run` — `{ "source": "..." }` → emit + check + stdout  

## Requirements

Same as `cuni check`: `python3`, `go`, `node` on `PATH`.

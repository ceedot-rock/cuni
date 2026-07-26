# CuNi Studio (hosted playground)

**Focus freeze:** this is the only product surface we’re building right now.

**Live:** https://cuni-studio.fly.dev/

Hosted Studio:

1. **Emit** — `cuni <file> --emit-py/--emit-go/--emit-js`
2. **Check / Run** — `cuni check` exactness (+ optional per-target stdout)
3. **Notelog** — chronological lab notes (auto on every run + manual)
4. **Critic Book** — structured critiques (auto from typeck/exactness + manual)
5. **Agent mode** — speech → CuNi skill (exactness gate) → py/go/js; propose/adopt law

## Local

```bash
# from repo root
cargo build --release
python3 playground/server.py
# → http://127.0.0.1:8787/  (default bind 0.0.0.0:8787)
```

Local-only bind:

```bash
CUNI_PLAYGROUND_HOST=127.0.0.1 python3 playground/server.py
```

## Env

| Variable | Default | Meaning |
|----------|---------|---------|
| `CUNI_BIN` | auto `target/release\|debug/cuni` | compiler path |
| `CUNI_PLAYGROUND_PORT` | `8787` | port |
| `CUNI_PLAYGROUND_HOST` | `0.0.0.0` | bind (hosted-ready) |
| `CUNI_PLAYGROUND_TIMEOUT` | `45` | seconds per subprocess |
| `CUNI_PLAYGROUND_MAX_SOURCE` | `200000` | max source bytes |
| `CUNI_PLAYGROUND_MAX_CONCURRENT` | `2` | parallel /api/run\|emit\|check |
| `CUNI_PLAYGROUND_DATA` | `playground/data` | Notelog + Critic Book JSON |

## API

| Method | Path | Body | Role |
|--------|------|------|------|
| GET | `/api/health` | — | toolchain + book counts |
| GET | `/api/examples` | — | `examples/*.cuni` |
| POST | `/api/emit` | `{source}` | emit only |
| POST | `/api/check` | `{source}` | emit + `cuni check` |
| POST | `/api/run` | `{source}` | emit + check + stdout |
| GET/POST | `/api/notelog` | `{body}` | read / append note |
| GET/POST | `/api/criticbook` | `{body,severity?,category?}` | read / append critique |

## Docker / Fly

```bash
# from repo root
docker build -f playground/Dockerfile -t cuni-studio .
docker run --rm -p 8787:8787 -v cuni-data:/data cuni-studio

# Fly (create volume first)
cd playground && flyctl launch --no-deploy   # or use fly.toml
flyctl volumes create cuni_studio_data --region iad --size 1
flyctl deploy --config fly.toml --dockerfile Dockerfile
```

Dockerfile context is the **repo root** (`-f playground/Dockerfile .`).

## Security

Runs user code via `python3` / `go run` / `node` on the host (same as `cuni check`).  
For public internet: use the Docker image, low concurrency, short timeout, and prefer a disposable VM.  
Do **not** mount secrets into the container.

## Notelog vs Critic Book

| Book | Purpose |
|------|---------|
| **Notelog** | Lab journal: “what I tried”, auto “run PASS/FAIL” lines |
| **Critic Book** | Structured issues: typeck line:col, exactness divergence, design notes |

Both persist under `CUNI_PLAYGROUND_DATA` (JSON). Clear with `POST /api/notelog/clear` or `/api/criticbook/clear` if needed.

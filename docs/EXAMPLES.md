# CuNi Examples Gallery

Quick index of the runnable examples. All live under [`examples/`](../examples/).

## Flagship (start here)

| File | What it shows |
|------|----------------|
| [`spend-control.cuni`](../examples/spend-control.cuni) | Default Studio example. Exact spend-limit policy. Publish → Rider stub. |
| [`link.cuni`](../examples/link.cuni) + [`link/demo.sh`](../examples/link/demo.sh) | One `link` contract → Go server + Python / JS / Go clients. |
| [`full.cuni`](../examples/full.cuni) | Broader language surface for exactness checks. |

## Language surface

| File | Focus |
|------|--------|
| `structs.cuni` | Named constructors |
| `enums.cuni` | Payload-free enums |
| `named_fields.cuni` | Field access |
| `modules.cuni` + `math.cuni` | `use` / modules |
| `typeck_valid_iface.cuni` | Interfaces |

## Agent

| Path | Focus |
|------|--------|
| [`examples/agent/`](../examples/agent/) | Host that routes speech → CuNi law → multi-runtime (`--check-all`, `--repl`, etc.) |

## How to run

```bash
# Exactness gate (requires python3, go, node on PATH)
cuni check examples/full.cuni
cuni check examples/          # whole directory

# Link interop demo
cargo build --release
./examples/link/demo.sh

# Agent host
python3 examples/agent/host/run_agent.py --check-all
```

Studio loads `spend-control.cuni` by default: https://cuni-studio.fly.dev/

See also: [`LINK_TUTORIAL.md`](LINK_TUTORIAL.md) · [`EXACTNESS_REFUSAL_EXAMPLES.md`](EXACTNESS_REFUSAL_EXAMPLES.md)

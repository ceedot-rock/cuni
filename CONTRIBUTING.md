# Contributing to CuNi

## Setup

```bash
git clone https://github.com/ceedot-rock/cuni.git
cd cuni
cargo build
cargo test
```

Needs **Rust**, **python3**, **go**, and **node** for exactness/conformance.

## Before you PR

```bash
cargo test
./target/debug/cuni check --timeout 120 \
  examples/full.cuni examples/structs.cuni examples/enums.cuni examples/named_fields.cuni
./examples/link/demo.sh   # if you touch link / interop / codegen wire
```

## Design rules (non-negotiable)

1. **Exactness (SPEC §2):** no approximate backends. Prefer **refuse** over “close enough.”
2. **One concept, one keyword** — no silent synonyms.
3. **User-facing errors** should include **`file:line:col`** when a span exists.
4. Spec changes go through **SPEC.md** (and proposals when large); don’t invent dialect in codegen only.

## Project map

| Path | Role |
|------|------|
| `src/` | Compiler |
| `examples/` | Samples + flagship demos |
| `tests/` | Conformance + typeck refusals |
| `playground/` | Local Studio |
| `docs/` | Tutorials, CI, registry design |
| `packages/` | Future registry layout (sketch) |

## Style

- Idiomatic Rust 2021; avoid drive-by refactors unrelated to the PR.
- Keep comments honest about toy-backend limits.

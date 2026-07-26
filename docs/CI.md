# CuNi CI & exactness badge

## This repository

| Workflow | Purpose | Badge |
|----------|---------|--------|
| [`exactness.yml`](../.github/workflows/exactness.yml) | Product gate: `cuni check` on portable examples | **Exactness** |
| [`ci.yml`](../.github/workflows/ci.yml) | Full `cargo test` + same exactness gate | **CI** |

### Badge markdown

```markdown
[![Exactness](https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml/badge.svg)](https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml)
[![CI](https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml/badge.svg)](https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml)
```

### Local equivalent

```bash
cargo build --release
./target/release/cuni check --timeout 120 \
  examples/full.cuni examples/structs.cuni examples/enums.cuni
```

Exit **0** only when every listed program prints **`exactness: PASS (py/go/js)`**.

## What is checked vs not

| Program | In Exactness workflow? | Why |
|---------|------------------------|-----|
| `examples/full.cuni` | yes | flagship exactness |
| `examples/structs.cuni` | yes | portable |
| `examples/enums.cuni` | yes | portable |
| `examples/modules.cuni` | no | intentionally refuses JS (`ext` collision) |
| `examples/link.cuni` | **flagship job** | `examples/link/demo.sh` in Exactness workflow + `cargo test` interop |

## Composite action (this repo)

```yaml
- uses: actions/checkout@v4
- uses: dtolnay/rust-toolchain@stable
- uses: actions/setup-go@v5
  with: { go-version: "1.22" }
- uses: actions/setup-node@v4
  with: { node-version: "20" }
- uses: ./.github/actions/cuni-exactness
  with:
    paths: examples/full.cuni examples/structs.cuni
```

## Other repositories (install CuNi, check your `.cuni` files)

```yaml
name: CuNi exactness
on: [push, pull_request]
jobs:
  exactness:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/setup-go@v5
        with: { go-version: "1.22" }
      - uses: actions/setup-node@v4
        with: { node-version: "20" }

      - name: Install cuni
        run: cargo install --git https://github.com/ceedot-rock/cuni --locked

      - name: Check
        run: cuni check --timeout 120 path/to/your.cuni
```

Pin a tag when you care about stability:

```bash
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.4
```

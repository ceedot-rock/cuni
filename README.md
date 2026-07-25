# CuNi (Code:uNiTY)

<p align="center">
  <img src="assets/logo.png" alt="CuNi — Code uNiTY" width="360" />
</p>

<p align="center">
  <a href="https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml"><img src="https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" /></a>
  <a href="https://github.com/ceedot-rock/cuni/releases/tag/v0.1.1"><img src="https://img.shields.io/badge/version-0.1.1-cyan.svg" alt="v0.1.1" /></a>
</p>

A small, mnemonic programming language that compiles to **exact, idiomatic** Python, JavaScript, and Go from one source file.

> **Exactness contract:** a CuNi program with no `ext` blocks compiles to identical behavior on every supported target — or it **refuses to compile**. No approximate mode.

### 30s demo

<p align="center">
  <a href="assets/demo-30s.mp4"><img src="assets/demo-30s.gif" alt="CuNi 30-second demo" width="640" /></a>
</p>

[Full MP4 (30s)](assets/demo-30s.mp4) · one source → Python / Go / JavaScript with identical stdout

## Install

**Requirements:** Rust (stable), plus `python3`, `go`, and `node` if you want to run the conformance suite.

```bash
# install the cuni binary onto your PATH
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.1

# or clone and build from source
git clone https://github.com/ceedot-rock/cuni.git
cd cuni
cargo build --release
# binary: target/release/cuni
```

## Quick start

```bash
# type-check + dump AST
cuni examples/full.cuni

# emit all three targets
cuni examples/full.cuni \
  --emit-py /tmp/full.py \
  --emit-go /tmp/full.go \
  --emit-js /tmp/full.js

python3 /tmp/full.py
go run /tmp/full.go
node /tmp/full.js

# from a clone: conformance (runs real py/go/js) + typeck suite
cargo test
```

## Language at a glance

| Idea | Syntax |
|------|--------|
| Blocks | `do` … `end` (no braces, no significant whitespace) |
| Bindings | `let` immutable / `mut` mutable |
| Functions | `def name(x: int) -> int do … ret … end` |
| Fallible | `-> T ?` + `fail e` + unwrap with `??` |
| Optionals | `opt<T>`, `none`, same `??` |
| Structs | `typ Point do x: int y: int end` → construct with `Point(3, 4)` |
| Interfaces | `iface Shape do area() -> float end` + `typ Circle is Shape do … end` |
| Enums | payload-free: `enum Color do Red Green Blue end` |
| Modules | `use math` → loads `math.cuni` beside the source file |
| Escape hatch | `ext name(...) -> T do py: … go: … js: … end` |
| Cross-program | `link Greet(name: str) -> str do … end` → handler + `*_remote` client |

Full prose: [`SPEC.md`](SPEC.md). Formal EBNF: [`GRAMMAR.md`](GRAMMAR.md).

## Layout

```
src/
  lexer.rs parser.rs token.rs ast.rs   # frontend
  typeck.rs checks.rs modules.rs       # refuse logic + use resolution
  codegen_{py,go,js}.rs                # three backends
  main.rs                              # CLI
examples/                              # runnable .cuni samples
tests/
  conformance.rs                       # byte-identical stdout across targets
  typeck.rs + typeck_invalid/          # compile-or-refuse fixtures
assets/logo.png                        # brand mark
```

## Status (v0.1.1)

**Shipped:** lexer/parser, three codegens, bounded type checker, `use`, `link` interop, enums, fail/`??`, stdlib (`say`, `.push`, `.len`), conformance tests.

**Not in v0.1 (by design):** tagged unions with payload, Rust target, streaming `link`, full inference, named struct fields — see SPEC.md §19.

## Design tenets

1. Mnemonic over cryptic (`ret`, `mut`, `whl`, `els`)
2. One concept, one keyword
3. Small core over broad coverage
4. Explicit over silently inferred (mutability, fallibility, `ext`)

## License

MIT — see [`LICENSE`](LICENSE).

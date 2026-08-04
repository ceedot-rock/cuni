# CuNi (Code:uNiTY)

<p align="center"><img src="https://raw.githubusercontent.com/ceedot-rock/splabs-brand/main/assets/brand/logos/logo-cuni.jpg" alt="product logo" width="280"/></p>

<p align="center">
  <img src="assets/logo.png" alt="CuNi — Code uNiTY" width="360" />
</p>

<p align="center">
  <a href="https://cuni-studio.fly.dev/"><img src="https://img.shields.io/badge/Studio-try%20in%20browser-5b9dff?style=for-the-badge" alt="Open CuNi Studio" /></a>
</p>

<p align="center">
  <a href="https://cuni-studio.fly.dev/"><img src="https://img.shields.io/badge/playground-live-3dd68c.svg" alt="Playground live" /></a>
  <a href="https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml"><img src="https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml/badge.svg" alt="Exactness" /></a>
  <a href="https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml"><img src="https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml/badge.svg" alt="CI" /></a>
  <a href="https://github.com/ceedot-rock/cuni/releases/tag/v0.1.7"><img src="https://img.shields.io/badge/version-0.1.7-cyan.svg" alt="v0.1.7" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" /></a>
</p>

**One source. Identical Python, JavaScript, and Go — or the compiler refuses.**

CuNi is a small language with a hard exactness contract: a program either produces the same behavior on every supported target, or it does not compile. No approximate mode. Free hosted **[CuNi Studio](https://cuni-studio.fly.dev/)** (Playground + Agent mode). Open source, MIT, v0.1.7.

> **Exactness contract:** a CuNi program with no `ext` blocks compiles to identical behavior on every supported target — or it **refuses to compile**.

**Current focus:** Studio → Rider publish/register loop is live; Agent-mode `spend` skill (speech → exactness → multi-runtime). Status: [`docs/STATUS.md`](docs/STATUS.md).

### Try the flagship example (no install)

1. Open **[CuNi Studio](https://cuni-studio.fly.dev/)** — it loads `spend-control.cuni` by default
2. Hit **Run exactness**
3. See identical Python / Go / JavaScript output (or a clear refusal)
4. Optional: **Publish** — exactness PASS → metadata + Studio-side Rider registration

That is the entire product promise in under 30 seconds.

Flagship source: [`examples/spend-control.cuni`](examples/spend-control.cuni) · Refusal examples: [`docs/EXACTNESS_REFUSAL_EXAMPLES.md`](docs/EXACTNESS_REFUSAL_EXAMPLES.md)

### Agent mind = CuNi

AI **speech** routes (stub or LLM); **law** is CuNi and must pass exactness before it runs on py/go/js:

```bash
python3 examples/agent/host/run_agent.py --check-all
python3 examples/agent/host/run_agent.py --message "spend 4 cap 5" --host go
python3 examples/agent/host/run_agent.py --loop --message "echo hi" --message "score 2 8"
python3 examples/agent/host/run_agent.py --repl
```

In Studio Agent mode, try speech `spend 4 cap 5` — it routes to the flagship CheckSpend law.

Design: [`docs/AI_IS_CUNI.md`](docs/AI_IS_CUNI.md) · pack: [`examples/agent/`](examples/agent/)

### CuNi + Agent-Rider

CuNi provides the exact multi-runtime language and verification.  
Agent-Rider provides the coordination layer (identity, messaging, multi-agent workflows).

### Related projects

| Project | Role |
|---------|------|
| [Agent-Rider](https://github.com/ceedot-rock/Agent-Rider) | Multi-agent coordination · [live](https://agentrider.vercel.app) |
| [quikgater](https://github.com/ceedot-rock/quikgater) | Pay-per-fact fetch for agents (x402 / USDC) |
| [SlidPhi](https://github.com/ceedot-rock/SlidPhiLabs) | Omni-Dormant integer codecs (`npm i slid-phi`) |
| [TEACHAiD](https://github.com/ceedot-rock/teachaid) | Interactive beginner school app |

Packaging draft (Homebrew / cargo-binstall): [`docs/PACKAGING.md`](docs/PACKAGING.md).

**End-to-end path (live today):**

1. Write a policy in [CuNi Studio](https://cuni-studio.fly.dev/) (default: spend-control).
2. **Run exactness** — refuse unless py/go/js match.
3. **Publish** — metadata is stored and auto-registered into the Studio-side Rider stub.
4. Inspect: `GET /api/rider/registered` · design for real Rider: [`docs/RIDER_V0_CONTRACTS.md`](docs/RIDER_V0_CONTRACTS.md)

They fit together like this:

1. Write critical agent policies and skills in CuNi (inside the Studio).
2. Verify them with the exactness checker — the same logic produces the same results on every supported runtime.
3. Deploy into Agent-Rider. Rider uses CuNi `link` contracts as the standard interop mechanism and requires exactness before a policy can run.

Result: agents can be implemented in the language that is most convenient, while the parts that matter for consistency and trust remain portable and verified.

### Two proofs that matter

| Proof | How | What it shows |
|-------|-----|----------------|
| **Exactness** | [Studio](https://cuni-studio.fly.dev/) or `cuni check examples/full.cuni` | One program → py/go/js → **same stdout** |
| **Interop (`link`)** | `./examples/link/demo.sh` | One contract → **Go server** + **Python + JS + Go clients** over HTTP |

### Flagship: one `link`, three languages

```cuni
link Greet(name: str, times: int) -> str do
    ret `hello ${name} x${times}`
end
```

```bash
cargo build --release
./examples/link/demo.sh
# Python / JS / Go clients all print:  hello Cee x3
```

<p align="center">
  <img src="assets/link-demo.gif" alt="CuNi link demo: Go server, Python JS Go clients all print hello Cee x3" width="720" />
</p>

Tutorial: [`docs/LINK_TUTORIAL.md`](docs/LINK_TUTORIAL.md) · source: [`examples/link.cuni`](examples/link.cuni) · [Release notes](https://github.com/ceedot-rock/cuni/releases/tag/v0.1.7)

### 30s demo (exactness)

<p align="center">
  <a href="assets/demo-30s.mp4"><img src="assets/demo-30s.gif" alt="CuNi 30-second demo" width="640" /></a>
</p>

[Full MP4 (30s)](assets/demo-30s.mp4) · one source → Python / Go / JavaScript with identical stdout

## Install

**Requirements:** Rust (stable), plus `python3`, `go`, and `node` if you want to run the conformance suite.

```bash
# install the cuni binary onto your PATH
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.7
# packaging options (draft): docs/PACKAGING.md · packaging/homebrew/cuni.rb

# or clone and build from source
git clone https://github.com/ceedot-rock/cuni.git
cd cuni
cargo build --release
# binary: target/release/cuni
```

## Quick start

```bash
# exactness gate (platform step 1) — emit py/go/js, run each, require identical stdout
cuni check examples/full.cuni
# → exactness: PASS (py/go/js)   exit 0
# → exactness: FAIL — …         exit 1

cuni check examples/          # all .cuni under a directory
cuni check examples/full.cuni --verbose --timeout 120

# type errors include file:line:col (platform step 2)
# → tests/typeck_invalid/undefined_var.cuni:1:9: type error: undefined variable `y`

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

### Exactness CI (platform step 4)

Workflows (run on every push/PR):

| Workflow | Badge | What it runs |
|----------|--------|----------------|
| **Exactness** | [![Exactness](https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml/badge.svg)](https://github.com/ceedot-rock/cuni/actions/workflows/exactness.yml) | `cuni check` on portable examples |
| **CI** | [![CI](https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml/badge.svg)](https://github.com/ceedot-rock/cuni/actions/workflows/ci.yml) | `cargo test` + exactness |

Local:

```bash
cargo build --release
./target/release/cuni check --timeout 120 \
  examples/full.cuni examples/structs.cuni examples/enums.cuni
```

Composite action (this repo): `.github/actions/cuni-exactness`  
Docs for other repos: [`docs/CI.md`](docs/CI.md)

(`cuni check` needs `python3`, `go`, and `node` on PATH.)

### Studio (hosted playground — current focus)

```bash
cargo build --release
python3 playground/server.py
# open http://127.0.0.1:8787  (binds 0.0.0.0 by default)
```

**Live:** https://cuni-studio.fly.dev/  

**Emit** · **Check/Run** · **Publish** (exactness → Rider stub) · **Notelog** · **Critic Book**.  
Details: [`playground/README.md`](playground/README.md) · freeze: [`docs/FREEZE.md`](docs/FREEZE.md).  
Redeploy: `flyctl deploy --config fly.toml --remote-only` (repo root).

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
**`link` interop tutorial:** [`docs/LINK_TUTORIAL.md`](docs/LINK_TUTORIAL.md).  
**CI / badges:** [`docs/CI.md`](docs/CI.md).

## Layout

```
src/
  lexer.rs parser.rs token.rs ast.rs   # frontend
  typeck.rs checks.rs modules.rs       # refuse logic + use resolution
  codegen_{py,go,js}.rs                # three backends
  main.rs                              # CLI
examples/                              # runnable .cuni samples
examples/link/demo.sh                  # flagship Go server ← py/js/go clients
playground/                            # hosted Studio: emit + check + Notelog + Critic Book
docs/LINK_TUTORIAL.md                  # interop walkthrough
docs/CI.md                             # badges + exactness CI
tests/
  conformance.rs                       # byte-identical stdout + link interop
  check_cmd.rs                         # cuni check CLI
  typeck.rs + typeck_invalid/          # compile-or-refuse fixtures
assets/logo.png                        # brand mark
```

## Status (v0.1.7)

**Shipped:** lexer/parser, three codegens, bounded type checker with **line:col** errors, **named typ constructors**, call-site generic binding checks, `use`, `link` interop, enums, fail/`??`, stdlib (`say`, `.push`, `.len`), `cuni check`, **hosted Studio** ([cuni-studio.fly.dev](https://cuni-studio.fly.dev/)) with Notelog + Critic Book, Exactness **CI + badge**, flagship **link demo**, Studio → Rider publish/register stub, Agent `spend` skill.

**Not in v0.1 (by design):** tagged unions with payload, Rust target, streaming `link`, full inference — see SPEC.md §19.

## Design tenets

1. Mnemonic over cryptic (`ret`, `mut`, `whl`, `els`)
2. One concept, one keyword
3. Small core over broad coverage
4. Explicit over silently inferred (mutability, fallibility, `ext`)

## License

MIT — see [`LICENSE`](LICENSE).

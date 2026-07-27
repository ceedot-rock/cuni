# Flagship tutorial: one `link`, three languages

**Try exactness first (no install):** [CuNi Studio](https://cuni-studio.fly.dev/) → example `full` → **Run exactness**.

**Story in one line:** write a typed contract once in CuNi; run the **server in Go** and call it from **Python** and **JavaScript** — same JSON, same path, same answer. Or the compiler refuses.

```
examples/link.cuni
        │
        ▼  cuni --emit-go / --emit-py / --emit-js
   ┌────┴────┬────────────┐
   Go        Python       JS
   Greet     Greet        Greet
   Greet_handler          …
   Greet_remote …
        │
        ▼  you mount the handler (codegen-only runtime)
   Go HTTP :8947  ──POST /Greet──►  py/js/go clients
```

This is **cross-program interop** (SPEC.md §16), not just “same file runs on three backends.”  
For **same-file exactness** (stdout matches), use [Studio](https://cuni-studio.fly.dev/) or `cuni check examples/full.cuni`.

**Why this matters for multi-agent systems**  
`link` is the concrete mechanism that lets agents written in different languages call each other with guaranteed identical contract behavior. It is the technical foundation for treating CuNi as the interop standard (one source of truth, multiple runtimes).

---

## Prerequisites

- Built compiler: `cargo build --release` → `target/release/cuni`
- Toolchain: `go`, `python3`, `node` (18+ recommended for `fetch`)

---

## 60-second demo

```bash
# from repo root
cargo build --release
./examples/link/demo.sh
```

You should see:

```text
==> Python client → Go server
    hello Cee x3
==> JavaScript client → Go server
    hello Cee x3
==> Go client → Go server (same binary symbols, remote call)
    hello Cee x3

FLAGSHIP LINK DEMO — PASS
```

Artifacts land in `examples/link/out/` (gitignored).

---

## The contract (entire CuNi program)

```cuni
link Greet(name: str, times: int) -> str do
    ret `hello ${name} x${times}`
end
```

File: [`examples/link.cuni`](../examples/link.cuni)

What each backend generates:

| Symbol | Role |
|--------|------|
| `Greet(...)` | In-process implementation |
| `Greet_handler` | HTTP handler for `POST /Greet` |
| `Greet_remote(base_url, ...)` | Client (always fallible — network can fail) |

Wire format: **HTTP + JSON**. Integers are JSON **strings** on the wire so all three targets round-trip without float surprises (SPEC §16).

---

## Manual walkthrough

### 1. Emit

```bash
mkdir -p /tmp/cuni-link
./target/release/cuni examples/link.cuni \
  --emit-go /tmp/cuni-link/link.go \
  --emit-py /tmp/cuni-link/link.py \
  --emit-js /tmp/cuni-link/link.js
```

### 2. Mount a Go server

Generated Go ends with an empty `main()`. Replace it (or use `examples/link/demo.sh`, which does this for you):

```go
func main() {
	http.HandleFunc("/Greet", Greet_handler)
	http.ListenAndServe("127.0.0.1:8947", nil)
}
```

```bash
go run /tmp/cuni-link/link.go   # after editing main
```

### 3. Call from Python

```python
from link import Greet_remote
print(Greet_remote("http://127.0.0.1:8947", "Cee", 3))
# → hello Cee x3
```

### 4. Call from JavaScript (Node)

Load the generated file, then:

```js
const result = await Greet_remote("http://127.0.0.1:8947", "Cee", 3);
console.log(result);
// → hello Cee x3
```

### 5. Call from Go

```go
result, err := Greet_remote("http://127.0.0.1:8947", "Cee", 3)
// result == "hello Cee x3"
```

---

## Why this is the product story

| Claim | Proof |
|-------|--------|
| One source of truth | Single `link` in `.cuni` |
| Real multi-runtime | Separate processes, real TCP |
| No “close enough” codec | Scalar-only v1; non-scalars **refuse** |
| No framework lock-in | You choose port, process model, TLS |
| Tested in CI | `cargo test` → `link_interop_go_server_python_client` |

Pair with **exactness** for portable logic:

```bash
cuni check examples/full.cuni   # same program, three local runs, same stdout
./examples/link/demo.sh         # one contract, three languages over the wire
```

---

## Limits (honest and deliberate)

- v1 `link` params/returns are **scalars only** (`int` / `float` / `str` / `bool`).
- JS `Number` still bounds very large ints after decode (disclosed in SPEC §16).
- You maintain the same `link` text in each program that needs the contract (no shared registry yet).
- Sync request/response only — not streaming.

These limits keep the exactness contract honest today and leave clear, non-breaking room for future expansion (shared registry, richer types, streaming) without undermining the core guarantee.

---

## Next platform steps after this tutorial

- Publish a short cast/gif of `demo.sh` output next to the 30s exactness demo.
- Hosted Studio is for exactness/emit; keep `./examples/link/demo.sh` as the source of truth for `link` interop.

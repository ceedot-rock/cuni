# FOR IMMEDIATE RELEASE

**CuNi Opens Free Hosted Studio: Try Exact Multi-Target Compile in the Browser**

*Write once → identical Python, Go, and JavaScript — or the compiler refuses. No install required.*

**[Web]** — July 26, 2026 — **CuNi (Code:uNiTY)** today opened **[CuNi Studio](https://cuni-studio.fly.dev/)**, a free hosted playground for its open-source multi-target language. Visitors can edit a CuNi program, **emit** Python, Go, and JavaScript, and run the official **`cuni check`** exactness gate in the browser. Sessions keep a **Notelog** (lab journal) and **Critic Book** (structured critiques of type and exactness failures).

### The problem

Teams and AI coding agents routinely generate logic that must run in more than one language. Hand ports drift. Transpilers often paper over differences. The result is subtle production bugs that no single-language test suite catches.

### The approach

CuNi keeps a deliberately small language surface so every construct has a proven mapping to Python, Go, and JS. Product surface now includes:

- **CuNi Studio** — https://cuni-studio.fly.dev/ — free hosted playground  
- **`cuni check`** — emit and run all three targets; require **byte-identical stdout**; exit non-zero on divergence  
- **`file:line:col` diagnostics** for type and compile errors  
- **`link`** — typed HTTP+JSON contracts so a **Go server** and **Python/JS clients** share one interface definition  
- Exactness CI badge and MIT license  

### Availability

- **Try now (no install):** https://cuni-studio.fly.dev/  
- **Source & docs:** https://github.com/ceedot-rock/cuni  
- **Release notes:** https://github.com/ceedot-rock/cuni/releases/tag/v0.1.6  
- **Interop tutorial:** https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md  
- **Install CLI:**  
  `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6`  

**Quick proof:**

1. Open https://cuni-studio.fly.dev/ → pick example **full** → **Run exactness**  
2. Or CLI: `cuni check examples/full.cuni` → `exactness: PASS (py/go/js)`  
3. Interop: `./examples/link/demo.sh` → Go server ← Python + JS + Go clients  

### About CuNi

CuNi is developed in the open under the MIT license by Corey Tasz and contributors. It targets polyglot engineering teams and agent-assisted development workflows that need a **hard gate** on multi-runtime correctness—not another approximate transpiler.

### Media contact

Corey Tasz  
Email: ceedotrock@gmail.com  
GitHub: https://github.com/ceedot-rock  
Studio: https://cuni-studio.fly.dev/  
Web: https://agentrider.xyz  

### ###

# FOR IMMEDIATE RELEASE

**CuNi 0.1.6 Ships Exact Multi-Target Compilation: One Source → Identical Python, Go, and JavaScript**

*Open-source language enforces “exactness”: same behavior on all three runtimes—or the compiler refuses to emit.*

**[Location / Web]** — July 26, 2026 — **CuNi (Code:uNiTY)** today announced **version 0.1.6**, an open-source programming language that compiles a single source program to **Python, Go, and JavaScript** under a strict **exactness contract**: portable programs must produce **identical runtime behavior** on every supported target, or the compiler **refuses** rather than emitting “close enough” code.

### The problem

Teams and AI coding agents routinely generate logic that must run in more than one language. Hand ports drift. Transpilers often paper over differences. The result is subtle production bugs that no single-language test suite catches.

### The approach

CuNi keeps a deliberately small language surface so every construct has a proven mapping to Python, Go, and JS. The product surface includes:

- **`cuni check`** — emit and run all three targets; require **byte-identical stdout**; exit non-zero on divergence  
- **`file:line:col` diagnostics** for type and compile errors  
- **`link`** — typed HTTP+JSON contracts so a **Go server** and **Python/JS clients** share one interface definition  
- **Local playground**, Exactness CI badge, and MIT license  

### Availability

- **Source & docs:** https://github.com/ceedot-rock/cuni  
- **Release notes:** https://github.com/ceedot-rock/cuni/releases/tag/v0.1.6  
- **Interop tutorial:** https://github.com/ceedot-rock/cuni/blob/master/docs/LINK_TUTORIAL.md  
- **Install:**  
  `cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.6`  

**Quick proof:**

```bash
cuni check examples/full.cuni          # exactness: PASS (py/go/js)
./examples/link/demo.sh                # Go server ← Python + JS + Go clients
```

### About CuNi

CuNi is developed in the open under the MIT license by Corey Tasz and contributors. It targets polyglot engineering teams and agent-assisted development workflows that need a **hard gate** on multi-runtime correctness—not another approximate transpiler.

### Media contact

Corey Tasz  
Email: ceedotrock@gmail.com  
GitHub: https://github.com/ceedot-rock  
Web: https://agentrider.xyz  

### ###

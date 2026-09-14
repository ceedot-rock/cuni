# Press release

**FOR IMMEDIATE RELEASE**

**Slid Phi Labs launches CuNi Bank — paste a program, get another language, or the compiler refuses**

Cherry Hill, N.J. — September 14, 2026 — Slid Phi Labs today made **CuNi Bank 0.1.0** public as an arm of **CuNi** (Code:uNiTY), the lab’s exactness compiler: one source, many languages, identical behavior or no build.

CuNi already ships a 119-language emit catalog. Native seats are Python, Go, JavaScript, TypeScript, C, C++, and Rust. Every other catalog id is a Python lowering so the gate still runs. `cuni check` emit+runs those seats and **refuses** if stdout diverges.

Bank is the reverse direction the lab would sell to agents: **paste N, get X**.

```
cargo install --git https://github.com/ceedot-rock/cuni --tag cuni-bank-0.1.0
cuni bank paste examples/bank/add.py --from py --to c
```

On the measured gate, `examples/bank/add.py` passed **ten** catalog seats: py, go, js, ts, c, cpp, rs (native) and rb, php, pl (catalog lowerings). Receipt name is `source_hash`, not a file path.

Bank v1 ingest is a **Python subset** (or existing `.cuni`). It is not “any GitHub repo in, any language out.” One hundred nineteen languages remains `cuni check` on the ingested deposit — not 119 ingest parsers.

**CuNi Studio** (free exactness in the browser): https://cuni-studio.fly.dev/  
**Source and release:** https://github.com/ceedot-rock/cuni/releases/tag/cuni-bank-0.1.0  
**Agent catalog (check / translate / squeeze):** https://spl-lab-agent.fly.dev/

License is dual: **AGPL-3.0-or-later** or a written commercial grant (COMMERCIAL.md). Hosted Studio is $0 exactness, not a grant to ship CuNi inside a closed product.

Slid Phi Labs is a computation lab in Cherry Hill. Contact: corey@slidphilabs.com · https://www.slidphilabs.com/

###

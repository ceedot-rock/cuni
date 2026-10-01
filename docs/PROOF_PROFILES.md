# Replace trust with proof — the 0.4.0 proof profiles

Corey's law for this release: everywhere the industry *trusts* two implementations
match, CuNi *proves* it. Same stdout, or refuse. Each profile below is a working
example with a runnable gate — not a slide.

| Profile | What it proves | Seats gated | Run it |
|---|---|---|---|
| `examples/proof-crosschain/` | One escrow transfer-validation law, identical on every chain target before anything deploys | sol (solc compile = deployable) + rs, go, py (stdout) | `cargo test --test proof_crosschain` |
| `examples/proof-crypto/` | One reference digest (FNV-1a 32-bit) + toy RSA verify, three independent implementations in agreement | rs, go, py | `cargo test --test proof_crypto` |
| `examples/proof-mlparity/` | One scoring kernel (int 4x4 matmul + argmax) bit-identical on every runtime, catching silent numeric drift before serving | py, rs, c | `cargo test --test proof_mlparity` |
| `examples/proof-firmware/` | One thermostat control law identical on both targets; fail the proof and nothing ships | c, rs (+ `promote.sh` refuse-to-promote) | `cargo test --test proof_firmware` |

Two more 0.4.0 pieces make the framing real:

- **Java seat** (`--emit-java`, `javac`-compiled): the audit-finance story — the bank's
  Java and the auditor's Python, gate-proven to agree.
- **SQL seat** (`--emit-sql`, verified against real `sqlite3`): one query logic,
  SQLite/PostgreSQL/MySQL dialects, proven instead of trusted.

## Honest boundaries (read before citing)

- The Solidity seat proves *compilability to deployable bytecode*, not on-chain
  execution. Behavioral exactness is proven on the rs/go/py seats.
- `proof-mlparity` is integer arithmetic by design. Cross-language float exactness
  (op ordering, fused multiply-add, libm differences) cannot be guaranteed, so a
  float kernel would be refused rather than faked.
- `proof-crypto` is a reference digest for conformance demonstration, not a
  recommendation to roll your own crypto. RSA side is verification-only (n=3233).
- The SQL seat's PostgreSQL/MySQL dialect emitters ship for documentation; only
  the SQLite dialect is gate-verified (stated in the artifact header).
- `proof-firmware` proves behavioral agreement on a fixed trace, not machine-code
  formal verification.

## Catalog honesty

144 catalog ids, unchanged: **12 native seats**
(py, go, js, ts, c, cpp, rs, rb, lua, sol, java, sql) + **132 Python lowerings**.
`java` and `sql` were already catalog ids; promoting them adds no new languages.
`cuni check` still emit+runs the whole catalog; `sol` keeps its honest refusal on
floats/structs (no Solidity mapping), so the pinned full-catalog count on those
examples remains 143/144.

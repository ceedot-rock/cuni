# proof-solana — the Solana program profile

"Trust Provable, in all things."

`escrow.cuni` states one transfer-validation law once. `cuni --emit-solana`
compiles it into an Anchor-shaped Solana program: a pure logic core (the part
CuNi proves) plus the program shell (entrypoint, accounts struct, program id).

The gate (`cargo test --test proof_solana`) proves:

1. **Behavioral exactness** — `cuni check --only rs,go,py` emits the fixture
   and runs each artifact with that target's own toolchain (`rustc`, `go run`,
   `python3`), requiring byte-identical stdout and `exactness: PASS` on exit 0.
   The six driver verdicts are pinned: `0, 25, 1, 2, 3, 0`.
2. **Program shape** — `--emit-solana` emits a real Anchor-shaped program:
   `declare_id!`, `#[program]`, `#[derive(Accounts)]` accounts struct, one
   instruction per CuNi function, and the two delimited regions
   (`CUNI-LOGIC-CORE-*` / `CUNI-SOLANA-SHELL-*`).
3. **Logic-core chain** — the delimited logic module is extracted from the
   emitted program, compiled standalone with `rustc` (no dependencies), run,
   and its stdout is asserted byte-identical to the pinned verdicts. The exact
   code that would ship inside the on-chain program is the code the gate ran.

## Honest boundaries

- The logic core is gate-proven. The program shell is **not** compiled here:
  no Solana toolchain exists on the check machine, and `anchor-lang` is not
  vendored. Nothing has executed on-chain.
- `fail` in CuNi becomes `panic!` in the logic core (abort semantics for the
  off-chain proof); a production shell maps it to `Err`. See `docs/SOLANA.md`
  for the full verification recipe (Solana CLI + Anchor install, `anchor
  build`, `anchor test`).
- On-chain convention would be `u64` amounts; CuNi proves the `i64` semantics
  it was given (truncated division). A `u64` port is a separate proof.

## Run it

```bash
cuni examples/proof-solana/escrow.cuni --emit-solana /tmp/escrow_program.rs
cargo test --test proof_solana
```

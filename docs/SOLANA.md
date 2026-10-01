# CuNi → Solana programs

**"Trust Provable, in all things."**

This is the Code Division's Solana emitter: one `.cuni` source becomes an
Anchor-shaped Solana program. It sits under CuNi's mission line alongside the
Financial and Onchain divisions' proof profiles (`docs/PROOF_PROFILES.md`).

## The two artifacts, one law

`cuni <file.cuni> --emit-solana <out.rs>` emits a single Rust file with two
clearly delimited regions:

1. **Logic core** (`// CUNI-LOGIC-CORE-START` … `// CUNI-LOGIC-CORE-END`):
   the pure program logic inside `mod logic` — byte-identical to the
   standalone logic-core emit. This is the part CuNi **proves**: the gate
   compiles it with plain `rustc` (no dependencies) and asserts its stdout is
   byte-identical to the pinned verdicts (see `examples/proof-solana/` and
   `cargo test --test proof_solana`).
2. **Program shell** (`// CUNI-SOLANA-SHELL-START` … `// CUNI-SOLANA-SHELL-END`):
   the Anchor wrapper — `declare_id!`, `#[program]` entrypoint module, one
   instruction per CuNi function, and a `#[derive(Accounts)]` accounts struct.
   Requires `anchor-lang` and the Solana toolchain.

Same logic or refuse: the shell calls `logic::*` for every instruction, so
the code that would execute on-chain is exactly the code the gate ran.

## Usage

```bash
cuni examples/proof-solana/escrow.cuni --emit-solana /tmp/escrow_program.rs
cargo test --test proof_solana   # the gate: exactness + shape + standalone logic core
```

The backend is `src/codegen_solana.rs` (unit tests inline: embedding is
byte-identical, shell has the Anchor shape, refusals are honest). It mirrors
`src/codegen_sol.rs`'s structure: genuine artifact, real-toolchain
verification where possible, refusal everywhere else.

## Honest boundaries (read before citing)

- **Logic gate-proven; shell NOT compiled here.** There is no Solana
  toolchain on the check machine and `anchor-lang` is not vendored, so the
  program shell has never been compiled — by anyone, anywhere in this repo.
  What the gate proves is the logic core: extracted from the emitted program,
  compiled standalone with `rustc`, run, stdout byte-identical to the
  `rs`/`go`/`py` seats.
- **Nothing has executed on-chain.** No deployment, no devnet, no test
  validator run exists for these artifacts.
- **`fail` becomes `panic!`** in the logic core (abort semantics for the
  off-chain proof). A production shell maps it to `Err` — that mapping is a
  deployment-time decision, not something CuNi guesses.
- **v1 refusal set.** `float`, `list`, `map`, `opt`, `??`, structs, enums,
  indexing, and field access are honestly refused. A Solana program's
  verifiable core is integer math; the backend will not guess at mappings it
  cannot prove. (Solidity's seat is the model: same posture, `solc`'s
  compile-to-bytecode in place of our `rustc` standalone check.)
- **Integers are `i64`.** On-chain convention would be `u64` amounts; CuNi
  proves the `i64` semantics it was given (truncated `/` and `%`, exactly
  like CuNi's `int`). A `u64` port is a separate proof.
- **The shell logs, it doesn't adjudicate.** Each instruction computes its
  logic function and reports the result with `msg!`. Mapping result codes to
  `#[error_code]` is a deployment-time decision.
- **ink!** (Polkadot) is the next profile in the emitter queue, not this one.

## Full verification recipe (documented — not run here)

On a machine with the Solana toolchain, the shell graduates from documented
to verified. Recipe (check versions — they move):

```bash
# 1. Solana CLI
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
solana --version

# 2. Anchor via AVM (Anchor Version Manager)
cargo install --git https://github.com/coral-xyz/anchor avm --force
avm install latest
avm use latest
anchor --version

# 3. Scaffold and drop in the CuNi program
anchor init cuni-escrow
cp /tmp/escrow_program.rs cuni-escrow/programs/cuni-escrow/src/lib.rs
#    replace declare_id!("111...") with the keypair from `anchor keys list`

# 4. Build + test (requires a local validator for `anchor test`)
anchor build
anchor test
```

The logic core itself needs nothing beyond a recent stable `rustc` (verified
on 1.98.1 here). Until `anchor build` succeeds on a tooled machine, any claim
stronger than "logic gate-proven, shell as-documented" is out of bounds.

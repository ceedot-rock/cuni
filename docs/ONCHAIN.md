# CuNi Onchain Division — Trust Provable, in all things

The Onchain Division takes CuNi's exactness guarantee to the chains. The
problem it solves: a financial law is proven exact off-chain (Code Division
built the machine, Financial Division sold the proof to money), and then it
has to *live* on-chain — re-implemented per chain, per VM, per numeric
model, with every port a new chance to drift. CuNi emits the law for each
chain from the same source, and proves the emitted logic byte-identical to
CuNi gold, or refuses.

## The architecture: one law, two regions

Every `--emit-X` artifact has the same shape, inherited from the 0.5.0
Solana profile:

- **Logic core** (`CUNI-LOGIC-CORE-START/END`) — the pure functions, the
  part CuNi proves. No chain imports, no stdout in the contract form.
- **Chain shell** (`CUNI-<CHAIN>-SHELL-START/END`) — entry points, storage,
  ABI wiring in the chain's idiom. Clearly delimited, never confused with
  the proven core.

A third artifact, `--emit-X-ref`, is the standalone runnable reference of
the logic core (Rust for ink!, Python for the rest) with a driver `main`
that prints the `say` outputs. The gate runs it and requires stdout
byte-identical to CuNi gold. The exact code the gate ran is the logic the
contract embeds.

## The six emitters (Corey's order)

| # | Target | Flag | Contract shape | `int` | `dec` (scale 10⁴) |
|---|--------|------|---------------|-------|-------------------|
| 1 | ink! (Polkadot/Substrate) | `--emit-ink` | `#[ink::contract]`, storage struct, constructor, `#[ink(message)]` fns | i64 | i128 scaled, checked ops |
| 2 | Move (Aptos/Sui) | `--emit-move` | `module 0xCUNI::name`, `public entry fun` | u64 (negatives refused) | u128 scaled (negatives refused) |
| 3 | Vyper (EVM) | `--emit-vyper` | `@external @pure` fns over `@internal @pure` helpers | int256 | int256 scaled (NOT native `decimal`) |
| 4 | Cairo (Starknet) | `--emit-cairo` | `#[starknet::contract]`, `#[storage]`, `#[abi(embed_v0)]` | u256 (negatives refused) | u256 scaled (negatives refused) |
| 5 | Clarity (Stacks) | `--emit-clarity` | `define-private` helpers + `define-public` `(ok ...)` wrappers | int (128-bit) | int scaled |
| 6 | Cadence (Flow) | `--emit-cadence` | `access(all) contract`, public fns, private helpers | Int256 | Int256 scaled (NOT native Fix64) |

Deliberate non-uses of native decimal types (Vyper `decimal` at 10¹⁰,
Cadence `Fix64` at 10⁸): CuNi proves the semantics it was given — scale
10⁴, truncation toward zero, canonical rendering per `docs/DECIMAL.md`
section 6. A different fixed-point scale would be a different proof.

Move, Cairo: no signed integers on these VMs, so negative `int`/`dec`
literals refuse at emit and runtime underflow aborts loudly — never silent.

## Verification matrix (0.8.0, this machine)

"Proven" means: reference compiled/run with a real toolchain present on
the check machine, stdout byte-identical to `cuni run` gold. "Shape" means:
emitter output asserted on contract markers + full golden snapshot.

| Target | Logic core proven | How | Contract shell | Money bridge (`fee_schedule`) |
|--------|------------------|-----|----------------|-------------------------------|
| ink! | ✅ | plain `rustc`, run, byte-identical (demo 15 lines + fee law 6 lines) | shape + snapshot — **not compiled: no `cargo-contract`** | ✅ byte-identical |
| Move | ✅ | `python3`, byte-identical (demo + fee law) | shape + snapshot — **not compiled: no `aptos`/`sui` CLI** | ✅ byte-identical |
| Vyper | ✅ | `python3`, byte-identical (demo + fee law + negative-dec edges) | shape + snapshot — **not compiled: no `vyper`** | ✅ byte-identical |
| Cairo | ✅ | `python3`, byte-identical (demo + fee law) | shape + snapshot — **not compiled: no `scarb`/`starkli`** | ✅ byte-identical |
| Clarity | ✅ | `python3`, byte-identical (demo 11 lines + fee law + stress fixture) | shape + snapshot — **not compiled: no `clarity-cli`** | ✅ byte-identical |
| Cadence | ✅ | `python3`, byte-identical (demo 11 lines + fee law) | shape + snapshot — **not compiled: no `flow` CLI** | ✅ byte-identical |
| Solana (0.5.0) | ✅ | plain `rustc`, byte-identical | shape + snapshot — **not compiled: no Solana toolchain** | n/a (predates the law) |

No chain toolchain was installed for this release. No mocked toolchains,
no skipped asserts: where the toolchain is absent, the tests assert the
absence (`which` probe) and the docs say so here.

## The money bridge

`examples/finance/fee_schedule.cuni` — the Financial Division's tiered-fee
law — compiles through all six emitters, and every reference prints the
same six driver lines, byte-identical:

```
1.2499 / 1.25 / 5.25 / 2.75 / 0.25 / 2500.25
```

The same financial law, stated once, proven exact, emittable for six
chains. That is the three divisions tied together: the Code Division's
machine, the Financial Division's law, the Onchain Division's reach.

## Honest boundaries

- **Nothing has been deployed.** No mainnet, no testnet, no local devnet.
  No chain was touched: no faucet, no account, no transaction, no spend.
  This release is compilation and local verification only.
- **Contract shells are not compiled here.** The logic cores are proven;
  the shells are shape-asserted and snapshot-pinned. Real-toolchain
  compilation (and any deployment) is future work, on machines with the
  toolchains, under Corey's explicit go-ahead per deployment.
- **Unsigned VMs stay unsigned.** Where a VM has no signed integers
  (Move, Cairo), negativity refuses loudly — at emit for literals, by
  abort at runtime. A port that silently wrapped would be a different,
  unproven semantics.
- **The audit proves behavior on the driver inputs**, exactly as in the
  Financial Division: the gate pins whatever the law's driver cases cover.
  `cuni audit` works against the references the same way it works against
  any seat implementation.
- **Profiles, not seats.** The six emitters are compilation profiles, like
  `--emit-solana`. The catalog is unchanged: 53 entries, 45 native
  seats, 8 lowerings.

## For the implementer

One emitter = one `src/codegen_X.rs` with `generate_program(program,
mod_name)` and `generate_reference(program)`, wired in `src/main.rs`
through the shared `emit_profile_artifact` / `emit_profile_reference`
helpers (`--emit-X` / `--emit-X-ref`). New profiles follow the same
contract: pure-logic core delimited from chain shell, honest refusals for
everything unprovable, a runnable reference the gate can execute, and a
`tests/proof_X.rs` suite in the shape of `tests/proof_solana.rs`.

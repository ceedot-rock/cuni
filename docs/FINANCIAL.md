# CuNi Financial Division — Trust Provable, in all things

The Financial Division turns CuNi's exactness guarantee into audit
infrastructure. The problem it solves is old and expensive: a financial law
is stated once (in a spec, a spreadsheet, a regulator's PDF), then
re-implemented N times — the bank's Java, the auditor's Python, the
regulator's reference — and everyone *trusts* they match. CuNi *proves* it:
same stdout, or refuse.

## The three flows

**1. The auditor proves the bank's implementation matches the law.**
The law is stated once as `.cuni` — tiered fees, interest accrual,
withholding brackets. The bank hands over its Java; the auditor runs
`cuni audit` against it. PASS means the Java printed byte-identical output
to the proven CuNi gold on every driver case. REFUSE means it didn't, and
the refusal itself is a signed, fileable receipt.

**2. The regulator publishes the law; everyone proves against it.**
One canonical `.cuni` file, published by the regulator. Every filer,
vendor, and counterparty proves their own implementation against the same
law and files their own receipt. Disagreement is no longer a meeting —
it's a REFUSE receipt with hashes.

**3. The bank self-certifies before shipping.**
Run the audit in CI. A pricing-engine change that moves one boundary cent
fails the gate before it reaches production, because the gate pins every
driver output byte-for-byte. `cuni audit` exits 0 on PASS and nonzero on
REFUSE, so it drops straight into a pipeline.

## How an audit runs

```sh
# The auditor mints a receipt-signing keypair (once; guard the .key).
cuni audit --gen-key auditor-name
#   writes auditor-name.key (32 raw bytes, mode 600) + auditor-name.pub (hex)

# The audit: law once, foreign impl, signed receipt out.
cuni audit examples/finance/fee_schedule.cuni \
  --against bank_fee.py \
  --signer auditor-name.key \
  --out receipt.json
# exit 0 on PASS, nonzero on REFUSE — the receipt is filed either way.
```

What happens inside, in order:

1. **Hash.** SHA-256 of the law file and of the foreign implementation.
2. **Gold gate.** The law itself must be exact across the money seats
   (Python, Rust, Go, Java, SQL — each run under its own real toolchain)
   via the same `check` machinery as everything else in CuNi. If the law
   isn't provably exact, there is no gold to compare against: the receipt
   says REFUSE. A failed gate *always* emits a REFUSE receipt — never a pass.
3. **Foreign run.** The implementation runs by extension — `.py` under
   python3, `.go` via `go run`, `.rs` via rustc, `.java` via javac+java,
   `.sql` via sqlite3 — and its stdout must be byte-identical to gold.
4. **Verdict + signature.** PASS only if the gate passed *and* the impl
   matched. The receipt is signed with the auditor's Ed25519 key
   (signature covers the receipt bytes including the signer pubkey, so a
   swapped key invalidates it).

## What the receipt contains, and why it files

```json
{
  "law_sha256": "958ee66f…",
  "impl_sha256": "a8455398…",
  "gold_stdout_sha256": "5550c980…",
  "seats_run": ["py", "go", "java", "rs", "sql", "tsql"],
  "verdict": "PASS",
  "timestamp_utc": "2026-10-02T02:36:17Z",
  "cuni_version": "0.7.0",
  "signer_pubkey": "e04bd916…",
  "signature": "dffb58d5…"
}
```

- **Hashes** bind the receipt to the exact bytes audited: the law, the
  implementation, and the gold output they were compared on. Re-run the
  audit on different bytes and the hashes won't match — the receipt can't
  be transplanted onto a different program.
- **seats_run** names the money seats whose agreement constitutes "gold".
- **verdict** is `PASS` or a `REFUSE` with the reason inline
  (`REFUSE: impl stdout diverged from gold`,
  `REFUSE: gold gate failed: …`). Refusals are loud and fileable too —
  a REFUSE receipt is still signed, still hash-bound, still evidence.
- **signature** makes the receipt third-party verifiable: anyone with the
  auditor's `.pub` can re-verify with `cuni::audit::verify_receipt`,
  no CuNi installation required. Unsigned receipts are supported
  (`verify_receipt` returns `Ok(false)` for "unsigned, nothing to check")
  — the hashes stand on their own.

## The worked laws

`examples/finance/` states three laws, each gated byte-identical across
the money seats and pinned byte-for-byte in `tests/proof_finance.rs`:

- **`fee_schedule.cuni`** — tiered fees on exact `dec`: ≤100.00 →
  1.0%+0.25; ≤1000.00 → 0.5%+0.25; above → 0.25%+0.25. Boundary-exact:
  99.9999, 100.0000, 1000.0000, 1000.0001, zero, and a large amount are
  all driver cases, because the boundary cent is where fee bugs live.
- **`interest_accrual.cuni`** — `dec` × `time`: principal × APR × days/365
  over a vesting window, whole days from `days_between`, dec truncation at
  every step. A leap-day window is a driver case, because date math is
  where interest bugs live.
- **`withholding.cuni`** — marginal brackets (0% ≤1000, 10% on 1000–5000,
  20% above), boundary-exact at every bracket edge.

Money math runs on CuNi's `dec` type (fixed-point, scale 10⁴, truncation
toward zero — never silent rounding; see `docs/DECIMAL.md`) and `time`
type (int64 unix epoch, UTC only, no timezones, no `now()` — see
`docs/TIME.md`). Both are the reason a `.cuni` law can serve as financial
ground truth: the types refuse to be approximate.

## Honest boundaries

- **The audit proves behavioral stdout-equality on the stated inputs.**
  It proves the implementation prints what the law prints, on the driver
  cases in the law file. It does **not** prove the law itself is good
  business — a law that undercharges everyone passes the audit exactly as
  loudly as a correct one. Law review is still a human job.
- **Driver coverage is the auditor's responsibility.** The gate pins
  whatever the law's driver cases cover. Boundary cases belong in the law
  file — the three worked laws put them there on purpose.
- **Key custody is the auditor's responsibility.** The `.key` file signs
  receipts in the auditor's name; CuNi writes it mode 600 and refuses to
  overwrite an existing key, but guarding it, rotating it, and deciding
  whose pubkey counts as authoritative are organizational controls, not
  compiler features. Never commit a real key to a repo — key generation
  is a runtime operation.
- **Refusals are the product working.** A REFUSE receipt is not a failed
  audit run; it *is* the audit output when implementations disagree.
  File it.

## Pricing

Hosted verification: **$0.10/check** — submit a law and an implementation,
get back the signed receipt without running toolchains yourself. The
`cuni` CLI itself is open source (AGPL-3.0-or-later) and the audit command
runs fully locally.

## For the implementer

`src/audit.rs` is a library module: `audit_law` (hash → gold gate →
foreign run → receipt), `gen_keypair`, `load_signing_key`,
`sign_receipt`, `verify_receipt`, `run_foreign_impl`, `sha256_hex`,
`utc_timestamp`. `cuni prove` shares the gold-gate helper and the
foreign-impl runner with `cuni audit` — one runner, one behavior —
and additionally accepts `.js`. Catalog unchanged: 53 entries, no new
seats in this phase; `audit` is a command, not a seat.

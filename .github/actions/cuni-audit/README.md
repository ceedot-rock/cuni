# `cuni-audit` GitHub Action

Forget trust. Prove.

Drops `cuni audit` into any CI pipeline: the implementation must print
byte-identical output to the `.cuni` law on every driver case, or the gate
refuses and the step fails. A pricing-engine PR that moves one boundary
cent never merges.

## Usage

```yaml
- uses: ceedot-rock/cuni/.github/actions/cuni-audit@master
  with:
    law: examples/finance/fee_schedule.cuni
    against: bank/fee.py
```

## Inputs

| input     | required | default | description                                              |
|-----------|----------|---------|----------------------------------------------------------|
| `law`     | yes      | —       | Path to the `.cuni` law file                             |
| `against` | yes      | —       | Implementation to audit (`.py`, `.go`, `.rs`, `.java`, `.sql`) |
| `version` | no       | `0.9.0` | `cuni` version to install from crates.io                 |

Exit 0 on PASS, nonzero on REFUSE — a REFUSE still emits its signed
receipt. See `docs/FINANCIAL.md` for the full audit story.

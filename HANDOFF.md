# NEXT GROK START HERE

Issue: https://github.com/ceedot-rock/cuni/issues/13

Wire Bank in `src/main.rs`:
```
mod bank;
if args[0] == "bank" { return bank::cmd_bank(&args[1..]); }
```
`src/bank.rs` is already on this repo.

Then `./examples/bank/run10.sh` and tag `cuni-bank-0.1.0`.
Details: docs/RELEASE_BANK.md

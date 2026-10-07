## What changed

<!-- One or two sentences. -->

## Seats / profiles touched

<!-- e.g. py, rs, go, java, sql, --emit-ink, or "none" -->

## Checks

- [ ] `cargo test` passes
- [ ] Exactness gate still byte-identical on every touched seat (refusals stay refusals), if any codegen changed
- [ ] No approximate backends added — refuse instead of "close enough" (SPEC §2)
- [ ] `cuni audit` receipts still verify for any touched money law, if applicable
- [ ] No secrets or key material committed (see `.github/workflows/audited-checks.yml`)

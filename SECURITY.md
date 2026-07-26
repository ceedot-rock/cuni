# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| 0.1.x   | yes |

## Reporting a vulnerability

Please open a **private** security advisory on GitHub, or email the maintainer via the address on the GitHub profile, with:

- CuNi version / commit
- Minimal reproducing program (`.cuni` if possible)
- Impact (e.g. incorrect emit that passes exactness, sandbox escape in playground)

Do **not** open a public issue for active exploits.

## Scope notes

- CuNi is a **source-to-source compiler**. Generated code inherits the security of the target language and host.
- `ext` blocks embed **untrusted target-language snippets** by design — treat them like any foreign code review.
- The local playground runs user code via `python3` / `go` / `node` on the host — **do not expose it to the public internet** without a sandbox.

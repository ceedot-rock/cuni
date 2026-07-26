# CuNi Package Registry (design sketch)

**Status:** design only — not shipped. Product step toward shared `use` modules and `link` contracts across teams.

## Problem

- `use math` only resolves sibling `.cuni` files on disk.
- The same `link Greet(...)` text must be copy-pasted into every program that needs the contract.
- There is no versioned, discoverable home for portable CuNi packages.

## Goals

1. **Publish** a package: name, version, source tree of `.cuni` files, optional README.
2. **Depend** via `use @scope/name` or `use name@version` (final syntax TBD).
3. **Exactness CI** on every published package (refuse if py/go/js diverge).
4. **Link contract packages**: a package may export only `link` declarations for interop without runtime code.

## Non-goals (v1 registry)

- Full npm/crates.io feature set
- Binary hosting of emitted py/go/js (clients always re-emit from CuNi)
- Private enterprise feeds (later)

## Layout (proposed)

```
packages/
  greet-contract/
    cuni.toml          # name, version, exports
    src/greet.cuni     # link Greet(...) ...
  math-extra/
    cuni.toml
    src/lib.cuni
```

### `cuni.toml` (draft)

```toml
name = "greet-contract"
version = "0.1.0"
description = "Shared Greet link contract"
license = "MIT"

[exports]
# path relative to package root
greet = "src/greet.cuni"
```

## Client workflow (future CLI)

```bash
cuni registry login
cuni publish packages/greet-contract
cuni add greet-contract@0.1.0   # writes to cuni.lock + vendor/
cuni check .                    # exactness over project + deps
```

## Trust

- Every published version runs **Exactness** (same gate as this repo’s CI).
- Optional: signed releases (TUF-lite) later.
- Malicious `ext` bodies are **not** portable — packages that require `ext` are marked **target-bound** and excluded from the default exactness badge.

## Hosting options

| Option | Pros | Cons |
|--------|------|------|
| Git + tags (v0) | zero infra | no search, manual |
| Static index on Pages + tarballs on GH Releases | cheap SEO | weak search |
| Small API (Cloudflare Workers + R2) | real registry | ops cost |

**Near-term:** document packages as git subtrees / monorepo `packages/` until demand justifies API hosting.

## Relation to playground

Playground loads **local** examples only. A later mode can “Open from registry” once publish exists.

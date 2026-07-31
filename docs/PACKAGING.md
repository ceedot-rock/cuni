# CuNi packaging draft (v0.1.7)

Goal: installable `cuni` binary without cloning the full repo.  
**Status:** draft only — formulas/scripts not published to Homebrew core or crates.io yet.

## Already works

```bash
# From source (documented in README)
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.7

# Local release binary
cargo build --release
# → target/release/cuni
```

## cargo-binstall (draft)

When GitHub Releases ship prebuilt tarballs (`cuni-{version}-{target}.tar.gz`):

```toml
# Cargo.toml metadata (future)
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/v{ version }/cuni-{ version }-{ target }.tar.gz"
bin-dir = "{ bin }"
pkg-fmt = "tgz"
```

```bash
cargo binstall cuni   # after crate publish + release assets
# or:
cargo binstall --git https://github.com/ceedot-rock/cuni
```

### Release asset naming (proposed)

| Target triple | Asset name |
|---------------|------------|
| `x86_64-unknown-linux-gnu` | `cuni-0.1.7-x86_64-unknown-linux-gnu.tar.gz` |
| `aarch64-unknown-linux-gnu` | `cuni-0.1.7-aarch64-unknown-linux-gnu.tar.gz` |
| `x86_64-apple-darwin` | `cuni-0.1.7-x86_64-apple-darwin.tar.gz` |
| `aarch64-apple-darwin` | `cuni-0.1.7-aarch64-apple-darwin.tar.gz` |

Each tarball contains a single `cuni` binary at the root.

## Homebrew (draft formula)

Tap path (proposed): `ceedot-rock/homebrew-cuni` → `Formula/cuni.rb`

See skeleton: [`packaging/homebrew/cuni.rb`](../packaging/homebrew/cuni.rb).

```bash
# After tap exists:
brew install ceedot-rock/cuni/cuni
```

Until then:

```bash
brew install --build-from-source ./packaging/homebrew/cuni.rb
# or cargo install as above
```

## CI hooks (future)

1. On tag `v*`: GHA matrix builds release binaries + checksums.
2. Upload to GitHub Release.
3. Bump Homebrew formula `url` + `sha256`.
4. Optional: publish crate to crates.io for `cargo install cuni`.

## Not in this draft

- Snap / Flatpak / Windows MSI  
- Docker image for the compiler (Studio is separate Fly deploy)  
- Registry packages under `packages/` (see [`REGISTRY.md`](REGISTRY.md)) — different concern

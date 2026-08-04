# CuNi packaging (v0.1.7 draft → ship)

**Goal**: installable `cuni` binary without cloning the full repo.  
**Status**: draft advanced 2026-08-04. Preferred path today is cargo install from tag. Homebrew formula and binstall metadata are ready for the next release that ships binary assets.

## One-command install (works today)

```bash
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.7
```

Requires a Rust toolchain. After install, `cuni --help` and `cuni check examples/full.cuni` work if the examples are present (or clone just for the suite).

## Local release binary

```bash
git clone https://github.com/ceedot-rock/cuni.git && cd cuni
cargo build --release
# → target/release/cuni
```

## cargo-binstall (ready for next tagged release with assets)

When GitHub Releases ship prebuilt tarballs:

```toml
# Add to Cargo.toml
[package.metadata.binstall]
pkg-url = "{ repo }/releases/download/v{ version }/cuni-{ version }-{ target }.tar.gz"
bin-dir = "{ bin }"
pkg-fmt = "tgz"
```

```bash
cargo binstall cuni
# or
cargo binstall --git https://github.com/ceedot-rock/cuni
```

### Proposed release asset naming

| Target triple | Asset name |
|---------------|------------|
| `x86_64-unknown-linux-gnu` | `cuni-0.1.7-x86_64-unknown-linux-gnu.tar.gz` |
| `aarch64-unknown-linux-gnu` | `cuni-0.1.7-aarch64-unknown-linux-gnu.tar.gz` |
| `x86_64-apple-darwin` | `cuni-0.1.7-x86_64-apple-darwin.tar.gz` |
| `aarch64-apple-darwin` | `cuni-0.1.7-aarch64-apple-darwin.tar.gz` |

Each tarball contains a single `cuni` binary at the root.

## Homebrew

Skeleton formula: [`packaging/homebrew/cuni.rb`](../packaging/homebrew/cuni.rb)

```bash
# Local test (no tap required)
brew install --build-from-source ./packaging/homebrew/cuni.rb
```

After a public tap (`ceedot-rock/homebrew-cuni`) and real sha256:

```bash
brew install ceedot-rock/cuni/cuni
```

**Next packaging actions (to close this S2S item)**  
1. On next tag: GHA matrix builds the four target binaries + checksums and uploads to the GitHub Release.  
2. Compute sha256 of the source tarball and of each binary asset; update the formula.  
3. Create the homebrew-cuni tap and push the formula.  
4. Optional: publish the crate to crates.io so `cargo install cuni` works without `--git`.

## Not in scope yet

- Snap / Flatpak / Windows MSI  
- Docker image for the compiler (Studio is the Fly deploy)  
- Registry packages under `packages/` (see [`REGISTRY.md`](REGISTRY.md))

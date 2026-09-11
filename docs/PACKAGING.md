# CuNi packaging (v0.1.9)

**Goal**: installable `cuni` binary without cloning the full repo.  
**Status**: advanced 2026-09-11 for **v0.1.9**. Preferred path today is cargo install from tag. Homebrew formula and cargo-binstall metadata match the tagged release; Linux x86_64 assets are on the GitHub Release. Broader target matrix still needs the release workflow.

## One-command install (works today)

```bash
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.9
```

Requires a Rust toolchain. After install, `cuni --help` and `cuni check examples/full.cuni` work if the examples are present (or clone just for the suite).

## Prebuilt Linux binary (v0.1.9)

GitHub Release [v0.1.9](https://github.com/ceedot-rock/cuni/releases/tag/v0.1.9) ships:

| Asset | Notes |
|-------|--------|
| `cuni-0.1.9-x86_64-unknown-linux-gnu.tar.gz` | single `cuni` binary at tarball root |
| `cuni-0.1.9-x86_64-unknown-linux-gnu.tar.gz.sha256` | checksum |

```bash
curl -sL https://github.com/ceedot-rock/cuni/releases/download/v0.1.9/cuni-0.1.9-x86_64-unknown-linux-gnu.tar.gz   | tar -xz && ./cuni --help
```

## Local release binary

```bash
git clone https://github.com/ceedot-rock/cuni.git && cd cuni
cargo build --release
# → target/release/cuni
```

Helper: [`packaging/scripts/build-release-assets.sh`](../packaging/scripts/build-release-assets.sh) `0.1.9`

## cargo-binstall

`Cargo.toml` includes `[package.metadata.binstall]` aimed at:

```text
{repo}/releases/download/v{version}/cuni-{version}-{target}.tar.gz
```

```bash
cargo binstall --git https://github.com/ceedot-rock/cuni
# once more targets + crates.io exist:
# cargo binstall cuni
```

Today only **linux-gnu x86_64** is uploaded; other triples need the release matrix.

### Release asset naming

| Target triple | Asset name |
|---------------|------------|
| `x86_64-unknown-linux-gnu` | `cuni-0.1.9-x86_64-unknown-linux-gnu.tar.gz` ✅ shipped |
| `aarch64-unknown-linux-gnu` | `cuni-0.1.9-aarch64-unknown-linux-gnu.tar.gz` (pending workflow) |
| `x86_64-apple-darwin` | `cuni-0.1.9-x86_64-apple-darwin.tar.gz` (pending workflow) |
| `aarch64-apple-darwin` | `cuni-0.1.9-aarch64-apple-darwin.tar.gz` (pending workflow) |

Each tarball contains a single `cuni` binary at the root.

## Homebrew

Formula: [`packaging/homebrew/cuni.rb`](../packaging/homebrew/cuni.rb) — points at the **v0.1.9** source tarball with a real sha256.

```bash
# Local test (no tap required)
brew install --build-from-source ./packaging/homebrew/cuni.rb
```

After a public tap (`ceedot-rock/homebrew-cuni`):

```bash
brew install ceedot-rock/cuni/cuni
```

**Next packaging actions**  
1. GHA matrix for the remaining three target binaries + checksums on each tag.  
2. Create the homebrew-cuni tap and push the formula.  
3. Optional: publish the crate to crates.io so `cargo install cuni` / `cargo binstall cuni` work without `--git`.

## Not in scope yet

- Snap / Flatpak / Windows MSI  
- Docker image for the compiler (Studio is the Fly deploy)  
- Registry packages under `packages/` (see [`REGISTRY.md`](REGISTRY.md))

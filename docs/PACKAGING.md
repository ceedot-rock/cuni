# CuNi packaging (v0.1.10)

**Goal**: installable `cuni` binary without cloning the full repo.  
**Status**: **v0.1.10**. Preferred path: `cargo install cuni`. Git tag and Linux x86_64 GitHub Release assets match. Broader target matrix still needs the release workflow.

## One-command install

```bash
cargo install cuni
# or from git:
cargo install --git https://github.com/ceedot-rock/cuni --tag v0.1.10
```

Requires a Rust toolchain. After install, `cuni --help` and `cuni check examples/full.cuni` work if the examples are present (or clone just for the suite).

## Prebuilt Linux binary (v0.1.10)

GitHub Release [v0.1.10](https://github.com/ceedot-rock/cuni/releases/tag/v0.1.10) ships:

| Asset | Notes |
|-------|--------|
| `cuni-0.1.10-x86_64-unknown-linux-gnu.tar.gz` | single `cuni` binary at tarball root |
| `cuni-0.1.10-x86_64-unknown-linux-gnu.tar.gz.sha256` | checksum |

```bash
curl -sL https://github.com/ceedot-rock/cuni/releases/download/v0.1.10/cuni-0.1.10-x86_64-unknown-linux-gnu.tar.gz \
  | tar -xz && ./cuni --help
```

## Local release binary

```bash
git clone https://github.com/ceedot-rock/cuni.git && cd cuni
cargo build --release
# → target/release/cuni
```

Helper: [`packaging/scripts/build-release-assets.sh`](../packaging/scripts/build-release-assets.sh) `0.1.10`

## cargo-binstall

`Cargo.toml` includes `[package.metadata.binstall]` aimed at:

```text
{repo}/releases/download/v{version}/cuni-{version}-{target}.tar.gz
```

```bash
cargo binstall cuni
```

Today only **linux-gnu x86_64** is uploaded; other triples need the release matrix.

### Release asset naming

| Target triple | Asset name |
|---------------|------------|
| `x86_64-unknown-linux-gnu` | `cuni-0.1.10-x86_64-unknown-linux-gnu.tar.gz` |
| `aarch64-unknown-linux-gnu` | `cuni-0.1.10-aarch64-unknown-linux-gnu.tar.gz` (pending workflow) |
| `x86_64-apple-darwin` | `cuni-0.1.10-x86_64-apple-darwin.tar.gz` (pending workflow) |
| `aarch64-apple-darwin` | `cuni-0.1.10-aarch64-apple-darwin.tar.gz` (pending workflow) |

Each tarball contains a single `cuni` binary at the root.

## Homebrew

Formula: [`packaging/homebrew/cuni.rb`](../packaging/homebrew/cuni.rb) — points at the **v0.1.10** source tarball.

```bash
brew install --build-from-source ./packaging/homebrew/cuni.rb
```

**Next packaging actions**  
1. GHA matrix for the remaining three target binaries + checksums on each tag.

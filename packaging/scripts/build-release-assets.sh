#!/usr/bin/env bash
# Build release tarballs for cargo-binstall / GitHub Releases.
# Usage: ./packaging/scripts/build-release-assets.sh 0.1.7
set -euo pipefail
VER="${1:-0.1.7}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="$ROOT/dist/release"
mkdir -p "$OUT"
cd "$ROOT"

TARGETS=(
  "x86_64-unknown-linux-gnu"
)
# Local single-target build (expand matrix in GHA)
cargo build --release
BIN="$ROOT/target/release/cuni"
test -x "$BIN"
STAGE="$OUT/stage"
rm -rf "$STAGE" && mkdir -p "$STAGE"
cp "$BIN" "$STAGE/cuni"
TAR="cuni-${VER}-$(rustc -vV | sed -n 's/^host: //p').tar.gz"
# normalize host triple
HOST=$(rustc -vV | sed -n 's/^host: //p')
TAR="cuni-${VER}-${HOST}.tar.gz"
tar -C "$STAGE" -czf "$OUT/$TAR" cuni
( cd "$OUT" && sha256sum "$TAR" > "${TAR}.sha256" )
echo "Wrote $OUT/$TAR"
cat "$OUT/${TAR}.sha256"
echo "Next: upload to GitHub Release v${VER}, update packaging/homebrew/cuni.rb sha256"

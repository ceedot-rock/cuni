#!/usr/bin/env bash
# CuNi 30-second live demo — one program, native seats, same stdout.
# From repo root after `cargo build --release`:
#   ./examples/demo-30s.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ -x "${CUNI_BIN:-}" ]]; then
  CUNI="$CUNI_BIN"
elif [[ -x target/release/cuni ]]; then
  CUNI=target/release/cuni
elif [[ -x target/debug/cuni ]]; then
  CUNI=target/debug/cuni
elif command -v cuni >/dev/null 2>&1; then
  CUNI="$(command -v cuni)"
else
  echo "error: cuni not found — cargo build --release" >&2
  exit 1
fi

SRC=examples/few.cuni
export PATH="/usr/bin:/usr/lib/go/bin:${HOME}/.cargo/bin:${PATH}"

echo
echo "CuNi — 119 languages. One program. Same stdout, or refuse."
echo
echo "── source ──────────────────────────────────────────"
cat "$SRC"
echo "────────────────────────────────────────────────────"
echo
echo "cuni check --only py,go,js,c,cpp,rs"
echo
"$CUNI" check "$SRC" --only py,go,js,c,cpp,rs --timeout 90
echo
echo "Studio: https://cuni-studio.fly.dev/"
echo

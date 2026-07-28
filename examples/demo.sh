#!/usr/bin/env bash
# CuNi product demo — exactness + Agent spend (CLI)
# Usage: from repo root, after cargo build --release
#   ./examples/demo.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

CUNI="${CUNI_BIN:-}"
if [[ -z "$CUNI" ]]; then
  if [[ -x target/release/cuni ]]; then
    CUNI=target/release/cuni
  elif [[ -x target/debug/cuni ]]; then
    CUNI=target/debug/cuni
  elif command -v cuni >/dev/null 2>&1; then
    CUNI="$(command -v cuni)"
  else
    echo "error: cuni binary not found — run: cargo build --release" >&2
    exit 1
  fi
fi

echo "=== CuNi demo ==="
echo "binary: $CUNI"
echo

echo "--- 1. Flagship policy: spend-control (exactness) ---"
"$CUNI" check examples/spend-control.cuni
echo

echo "--- 2. Portable sample: full.cuni (exactness + stdout story) ---"
"$CUNI" check examples/full.cuni
echo

if [[ -f examples/agent/host/run_agent.py ]]; then
  echo "--- 3. Agent speech → spend law (host=go) ---"
  python3 examples/agent/host/run_agent.py --message "spend 4 cap 5" --host go || {
    echo "(agent step skipped or failed — exactness steps above are the core demo)"
  }
  echo
fi

echo "=== Done ==="
echo "Studio (no install): https://cuni-studio.fly.dev/"
echo "  → Run exactness on spend-control · optional Publish · Agent: spend 4 cap 5"
echo "Docs: docs/DEMO.md"

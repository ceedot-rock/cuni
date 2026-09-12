#!/usr/bin/env bash
# Exactness + run lab compressor laws.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CUNI="${CUNI_BIN:-$ROOT/target/release/cuni}"
if [[ ! -x "$CUNI" ]]; then
  CUNI="$ROOT/dist/release/stage/cuni"
fi
if [[ ! -x "$CUNI" ]]; then
  echo "need a cuni binary (cargo build --release)" >&2
  exit 1
fi

echo "== exactness (119 langs) =="
"$CUNI" check "$ROOT/examples/compressors" --timeout 180

echo "== run (python emit) =="
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
for f in "$ROOT"/examples/compressors/*.cuni; do
  name="$(basename "$f" .cuni)"
  echo "-- $name --"
  "$CUNI" "$f" --emit-py "$work/$name.py"
  python3 "$work/$name.py"
done
echo OK

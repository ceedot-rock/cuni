#!/usr/bin/env bash
# Prove live product implementations against CuNi lab laws.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CUNI="${CUNI_BIN:-$ROOT/target/release/cuni}"
if [[ ! -x "$CUNI" ]]; then
  echo "need a cuni binary at $CUNI" >&2
  exit 1
fi

echo "== laws exactness (native seats) =="
for f in "$ROOT"/examples/laws/*.cuni; do
  "$CUNI" check "$f" --only py,go,js,c,cpp,rs --timeout 90
done

echo "== compressor laws exactness (native seats) =="
for f in "$ROOT"/examples/compressors/*.cuni; do
  "$CUNI" check "$f" --only py,go,js,c,cpp,rs --timeout 90
done

PPS="$ROOT/../SlidPhiLabs/packages/spl-pay-per-suite/test/print-suite-meter.mjs"
if [[ -f "$PPS" ]]; then
  echo "== prove suite-meter vs spl-pay-per-suite =="
  "$CUNI" prove "$ROOT/examples/laws/suite-meter.cuni" --against "$PPS"
fi

echo "OK"

#!/bin/sh
# Golden tests for ScanChunk exact-text refuse (cuni#17)
set -eu
cd "$(dirname "$0")"
fail=0

out=$(./cuni_scan_cli testdata/ok.chunk)
if [ "$out" = "ok" ]; then
  echo "PASS ok.chunk → ok"
else
  echo "FAIL ok.chunk expected ok got: $out"
  fail=1
fi

out=$(./cuni_scan_cli testdata/extra.chunk || true)
if [ "$out" = "CUNI_ERR_EXTRA" ]; then
  echo "PASS extra.chunk → CUNI_ERR_EXTRA"
else
  echo "FAIL extra.chunk expected CUNI_ERR_EXTRA got: $out"
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "golden: FAIL"
  exit 1
fi
echo "golden: PASS"

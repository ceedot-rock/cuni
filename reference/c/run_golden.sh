#!/bin/sh
# Golden tests for ScanChunk exact-text refuse (cuni#17 / Agent-Rider#22 SoT)
# Extra keys: enum CUNI_ERR_EXTRA + wire reject.extra (do not collapse).
set -eu
cd "$(dirname "$0")"
fail=0

# ok.chunk → wire "ok", exit 0
out=$(./cuni_scan_cli testdata/ok.chunk 2>/dev/null)
if [ "$out" = "ok" ]; then
  echo "PASS ok.chunk → ok"
else
  echo "FAIL ok.chunk expected ok got: $out"
  fail=1
fi

# extra.chunk → wire reject.extra on stdout, CUNI_ERR_EXTRA on stderr, exit 1
set +e
out=$(./cuni_scan_cli testdata/extra.chunk 2>testdata/_extra.err)
rc=$?
set -e
err=$(cat testdata/_extra.err)
rm -f testdata/_extra.err
if [ "$out" = "reject.extra" ] && [ "$err" = "CUNI_ERR_EXTRA" ] && [ "$rc" -eq 1 ]; then
  echo "PASS extra.chunk → enum CUNI_ERR_EXTRA + wire reject.extra"
else
  echo "FAIL extra.chunk expected reject.extra/CUNI_ERR_EXTRA/1 got wire=$out enum=$err rc=$rc"
  fail=1
fi

# malformed line without '=' → same as extra (CUNI_ERR_EXTRA / reject.extra)
set +e
out=$(./cuni_scan_cli testdata/malformed.chunk 2>testdata/_mal.err)
rc=$?
set -e
err=$(cat testdata/_mal.err)
rm -f testdata/_mal.err
if [ "$out" = "reject.extra" ] && [ "$err" = "CUNI_ERR_EXTRA" ] && [ "$rc" -eq 1 ]; then
  echo "PASS malformed.chunk → enum CUNI_ERR_EXTRA + wire reject.extra"
else
  echo "FAIL malformed.chunk expected reject.extra/CUNI_ERR_EXTRA/1 got wire=$out enum=$err rc=$rc"
  fail=1
fi

# missing required hash → reject.missing
set +e
out=$(./cuni_scan_cli testdata/missing.chunk 2>/dev/null)
rc=$?
set -e
if [ "$out" = "reject.missing" ] && [ "$rc" -eq 2 ]; then
  echo "PASS missing.chunk → reject.missing"
else
  echo "FAIL missing.chunk expected reject.missing/2 got wire=$out rc=$rc"
  fail=1
fi

# Compile-time / value check: CUNI_ERR_EXTRA must be 3
cat > testdata/_enum_check.c << 'CEOF'
#include "../cuni.h"
#include <stdio.h>
#include <string.h>
int main(void) {
  if ((int)CUNI_ERR_EXTRA != 3) { fprintf(stderr, "CUNI_ERR_EXTRA value %d != 3\n", (int)CUNI_ERR_EXTRA); return 1; }
  if (strcmp(cuni_err_str(CUNI_ERR_EXTRA), "reject.extra") != 0) {
    fprintf(stderr, "cuni_err_str(CUNI_ERR_EXTRA) != reject.extra\n"); return 1;
  }
  puts("ok");
  return 0;
}
CEOF
cc -std=c99 -Wall -Wextra -o testdata/_enum_check testdata/_enum_check.c cuni_scan_chunk.o
out=$(./testdata/_enum_check)
rm -f testdata/_enum_check testdata/_enum_check.c
if [ "$out" = "ok" ]; then
  echo "PASS CUNI_ERR_EXTRA enum value 3 + wire reject.extra"
else
  echo "FAIL enum/wire lock check"
  fail=1
fi

if [ "$fail" -ne 0 ]; then
  echo "golden: FAIL"
  exit 1
fi
echo "golden: PASS"

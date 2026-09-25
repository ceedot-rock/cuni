#!/bin/bash
# run_tests.sh — build the C driver with cc and exercise it.
# Writes emitted artifacts to /tmp only. Never touches the tree's tracked files.
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
CD="$(cd "$SCRIPT_DIR/.." && pwd)"
TREE="$(cd "$CD/.." && pwd)"
export CUNI_TREE="$TREE"

mkdir -p "$CD/build" /tmp/cuni_c_test

echo "== build =="
cc -std=c99 -Wall -Wextra -O2 -I"$CD/include" \
   -o "$CD/build/test_cuni_driver" \
   "$CD/src/cuni_driver.c" "$CD/tests/test_cuni_driver.c"

echo "== driver test =="
"$CD/build/test_cuni_driver" "$TREE/examples/full.cuni" /tmp/cuni_c_test

echo
echo "== native harness cross-check (same seats, same source) =="
BIN="$TREE/target/release/cuni"
for s in py js ts c cpp; do
    if "$BIN" check "$TREE/examples/full.cuni" --only "$s" >/tmp/native_$s.log 2>&1; then
        echo "native $s: PASS"
    else
        echo "native $s: FAIL"
        tail -2 "/tmp/native_$s.log"
    fi
done

echo
echo "== emitted artifacts =="
ls -la /tmp/cuni_c_test

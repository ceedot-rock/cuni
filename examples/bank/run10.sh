#!/bin/sh
set -e
CUNI="${CUNI:-./target/release/cuni}"
IN="${1:-examples/bank/add.py}"
for to in py go js ts c cpp rs rb php pl; do
  echo "== $to =="
  "$CUNI" bank paste "$IN" --from py --to "$to"
done
echo "bank 10: PASS"

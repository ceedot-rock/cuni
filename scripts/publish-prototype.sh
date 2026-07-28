#!/usr/bin/env bash
# Minimal Studio → Rider publish prototype
# Usage: ./scripts/publish-prototype.sh path/to/file.cuni

set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <file.cuni>"
  exit 1
fi

SRC="$1"
if [ ! -f "$SRC" ]; then
  echo "File not found: $SRC"
  exit 1
fi

CUNI_BIN="${CUNI_BIN:-./target/release/cuni}"
if [ ! -x "$CUNI_BIN" ]; then
  echo "cuni binary not found at $CUNI_BIN – build first or set CUNI_BIN"
  exit 1
fi

echo "==> Running exactness check..."
if ! "$CUNI_BIN" check "$SRC"; then
  echo "Exactness FAILED – refusing to publish"
  exit 1
fi

echo "==> Exactness PASSED"
SOURCE_HASH=$(sha256sum "$SRC" | cut -d' ' -f1)
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
OUT="${SRC%.cuni}.publish.json"

# Minimal metadata (matches the shape we defined)
python3 - "$SRC" "$SOURCE_HASH" "$TIMESTAMP" "$OUT" << 'PY'
import json, sys
src, source_hash, timestamp, out = sys.argv[1:]
with open(src) as f:
    source = f.read()
meta = {
    "version": "0.1",
    "source": source,
    "sourceHash": source_hash,
    "exactness": {
        "passed": True,
        "checkedAt": timestamp,
        "targets": ["py", "go", "js"],
        "stdoutMatch": True
    },
    "publishedAt": timestamp,
    "publisher": "prototype-script"
}
with open(out, "w") as f:
    json.dump(meta, f, indent=2)
print(f"==> Published metadata written to {out}")
print("Ready for Rider to consume.")
PY

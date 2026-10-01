#!/usr/bin/env bash
# proof-firmware/promote.sh — refuse-to-promote gate for the firmware profile.
#
# Replace trust with proof: the same controller.cuni source is compiled to C
# and Rust, and binaries are released ONLY if the exactness gate proves both
# targets behave identically. Anything else -> REFUSED, nothing ships.
#
# Steps:
#   1. `cuni check --only c,rs --timeout 120` on the source. Exit != 0 ->
#      print REFUSED with the reason, touch nothing, exit non-zero.
#   2. Emit C and Rust, compile each with its native toolchain (gcc, rustc).
#   3. Belt-and-braces proof: run both compiled binaries, require identical
#      stdout. Diverge -> REFUSED, exit non-zero.
#   4. Only then copy both binaries into <PROMOTE_DIR>/released/.
#
# Usage: promote.sh [source.cuni]
#
# Env:
#   CUNI_BIN     path to the cuni binary (default: $HOME/.cargo/bin/cuni)
#   PROMOTE_DIR  directory holding the .cuni source; released/ lands here
#                (default: this script's directory). Integration tests point
#                this at a temp copy so examples/ is never polluted.
#   RUSTC_DIR    directory containing rustc (default: the stable toolchain
#                dir under ~/.rustup). Prepended to PATH along with
#                ~/.cargo/bin so child commands (cuni check's seat runners,
#                rustc) resolve.
#
# Exits: 0 on PROMOTED, 1 on REFUSED, 2 on environment/usage problems.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROMOTE_DIR="${PROMOTE_DIR:-$SCRIPT_DIR}"
CUNI_BIN="${CUNI_BIN:-$HOME/.cargo/bin/cuni}"
RUSTC_DIR="${RUSTC_DIR:-$HOME/.rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin}"

export PATH="$RUSTC_DIR:$HOME/.cargo/bin:/usr/bin:/bin:$PATH"

SRC="${1:-$PROMOTE_DIR/controller.cuni}"
SRC_NAME="$(basename "$SRC" .cuni)"
RELEASED="$PROMOTE_DIR/released"
BUILD="$PROMOTE_DIR/.promote-build"

refuse() {
    # $1 = reason. Prints the refusal, guarantees released/ is untouched,
    # exits non-zero. Callers pass 1 (gate/compiler refusal) or 2 (env).
    local code="${2:-1}"
    echo "REFUSED — $1; binary NOT promoted" >&2
    rm -rf "$BUILD"
    exit "$code"
}

[ -x "$CUNI_BIN" ] || refuse "cuni binary not executable at $CUNI_BIN" 2
[ -f "$SRC" ] || refuse "source not found: $SRC" 2
command -v gcc >/dev/null 2>&1 || refuse "gcc not on PATH" 2
command -v rustc >/dev/null 2>&1 || refuse "rustc not on PATH" 2

# --- Step 1: the exactness gate ------------------------------------------
echo "gate: cuni check --only c,rs --timeout 120 $SRC"
CHECK_OUT="$("$CUNI_BIN" check "$SRC" --only c,rs --timeout 120 2>&1)"
STATUS=$?
printf '%s\n' "$CHECK_OUT"
if [ "$STATUS" -ne 0 ]; then
    REASON="$(printf '%s\n' "$CHECK_OUT" \
        | grep -E 'exactness: FAIL|front-end  FAIL|error' \
        | head -3 | tr '\n' '; ' | sed 's/; $//')"
    [ -z "$REASON" ] && REASON="cuni check exited $STATUS with no message"
    refuse "$REASON" 1
fi
printf '%s\n' "$CHECK_OUT" | grep -q 'exactness: PASS' \
    || refuse "gate exited 0 but no 'exactness: PASS' line in output" 1

# --- Step 2: emit + native compile ----------------------------------------
rm -rf "$BUILD"
mkdir -p "$BUILD"
"$CUNI_BIN" "$SRC" --emit c "$BUILD/$SRC_NAME.c" >/dev/null 2>&1 \
    || refuse "emit to C failed" 1
"$CUNI_BIN" "$SRC" --emit rs "$BUILD/$SRC_NAME.rs" >/dev/null 2>&1 \
    || refuse "emit to Rust failed" 1
gcc -O2 -o "$BUILD/${SRC_NAME}-c" "$BUILD/$SRC_NAME.c" \
    || refuse "gcc compile of the C target failed" 1
rustc -O -o "$BUILD/${SRC_NAME}-rs" "$BUILD/$SRC_NAME.rs" \
    || refuse "rustc compile of the Rust target failed" 1

# --- Step 3: post-compile proof — the shipped binaries must agree ---------
OUT_C="$("$BUILD/${SRC_NAME}-c")" \
    || refuse "compiled C binary exited non-zero" 1
OUT_RS="$("$BUILD/${SRC_NAME}-rs")" \
    || refuse "compiled Rust binary exited non-zero" 1
[ "$OUT_C" = "$OUT_RS" ] \
    || refuse "compiled C and Rust binaries diverged on stdout" 1

# --- Step 4: release ------------------------------------------------------
mkdir -p "$RELEASED"
cp "$BUILD/${SRC_NAME}-c" "$RELEASED/"
cp "$BUILD/${SRC_NAME}-rs" "$RELEASED/"
rm -rf "$BUILD"

echo "PROMOTED — ${SRC_NAME}-c and ${SRC_NAME}-rs print byte-identical stdout; released/"
exit 0

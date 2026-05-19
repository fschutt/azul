#!/usr/bin/env bash
# Sprint J — golden-PNG snapshot harness for the headless backend.
#
# Builds + runs an example with AZ_BACKEND=headless and
# AZ_HEADLESS_SNAPSHOT_PATH=<actual>, then diffs the produced PNG
# against scripts/mobile/golden/<name>.png. On mismatch the script
# exits 1 and leaves both `actual.png` and a `diff.png` (if
# imagemagick is available) next to the golden for inspection.
#
# Usage:
#   bash scripts/mobile-snapshot.sh <example-name> [<golden-name>]
#
# Modes:
#   AZ_SNAPSHOT_UPDATE=1   Treat the actual.png as authoritative and
#                          overwrite the golden. Useful when changing
#                          the example deliberately. Default off so CI
#                          stays strict.
#
# Tooling priority for diff:
#   1. `compare` (imagemagick)   — produces side-by-side diff.png
#   2. `cmp -s` (POSIX)          — byte-equal only, no diff visual
#   3. manual stat               — pixel-count mismatch warning

set -euo pipefail

EXAMPLE="${1:-}"
GOLDEN_NAME="${2:-$EXAMPLE}"
if [[ -z "$EXAMPLE" ]]; then
    echo "usage: $0 <example-name> [<golden-name>]" >&2
    exit 2
fi

WORKSPACE_ROOT=$(cd "$(dirname "$0")/.." && pwd)
GOLDEN_DIR="$WORKSPACE_ROOT/scripts/mobile/golden"
mkdir -p "$GOLDEN_DIR"
GOLDEN="$GOLDEN_DIR/$GOLDEN_NAME.png"
ACTUAL="$GOLDEN_DIR/$GOLDEN_NAME.actual.png"
DIFF="$GOLDEN_DIR/$GOLDEN_NAME.diff.png"

export AZ_BACKEND=headless
export AZ_HEADLESS_SNAPSHOT_PATH="$ACTUAL"

# Use the rustup-managed cargo (not the Homebrew one) so the cross-compile
# rustup targets are visible. mobile-check-all.sh does the same.
export PATH="$HOME/.cargo/bin:$PATH"

rm -f "$ACTUAL" "$DIFF"

echo "==> cargo run --release -p $EXAMPLE  (AZ_BACKEND=headless)"
(cd "$WORKSPACE_ROOT" && cargo run --release -p "$EXAMPLE")

if [[ ! -f "$ACTUAL" ]]; then
    echo "FAIL: example exited without writing $ACTUAL" >&2
    echo "       (did the layout callback return an empty DOM? \
the AZ_HEADLESS_SNAPSHOT_PATH hook only writes after the first frame)" >&2
    exit 1
fi

if [[ "${AZ_SNAPSHOT_UPDATE:-}" == "1" ]]; then
    cp "$ACTUAL" "$GOLDEN"
    echo "==> golden updated: $GOLDEN ($(wc -c <"$GOLDEN") bytes)"
    exit 0
fi

if [[ ! -f "$GOLDEN" ]]; then
    echo "FAIL: no golden at $GOLDEN — initialize with AZ_SNAPSHOT_UPDATE=1" >&2
    exit 1
fi

# 1) imagemagick (pixel-perfect with a small AE allowance for AA jitter)
if command -v compare >/dev/null 2>&1; then
    if compare -metric AE -fuzz 1% "$GOLDEN" "$ACTUAL" "$DIFF" 2>/dev/null; then
        echo "PASS: $GOLDEN_NAME (imagemagick AE=0)"
        rm -f "$DIFF"
        exit 0
    else
        echo "FAIL: $GOLDEN_NAME differs — see $DIFF" >&2
        exit 1
    fi
fi

# 2) fallback: byte-equal check
if cmp -s "$GOLDEN" "$ACTUAL"; then
    echo "PASS: $GOLDEN_NAME (byte-equal)"
    exit 0
fi

echo "FAIL: $GOLDEN_NAME PNG bytes differ (imagemagick not available — \
install with 'brew install imagemagick' for a visual diff)" >&2
echo "       golden: $(wc -c <"$GOLDEN") B"
echo "       actual: $(wc -c <"$ACTUAL") B"
exit 1

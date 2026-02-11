#!/usr/bin/env bash
# verify-system-style-macos.sh
#
# Runs the C test binary and compares detected SystemStyle values
# against macOS system utility queries.
#
# Usage:
#   ./scripts/verify-system-style-macos.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
BINARY="$ROOT_DIR/target/c-examples/test-system-style"
SOURCE="$ROOT_DIR/examples/c/test-system-style.c"

# ── Build if needed ─────────────────────────────────────────────────────
if [[ ! -f "$BINARY" ]] || [[ "$SOURCE" -nt "$BINARY" ]]; then
    echo "Building test-system-style..."
    mkdir -p "$ROOT_DIR/target/c-examples"
    cc -o "$BINARY" "$SOURCE" \
        -I"$ROOT_DIR/target/codegen/v2" \
        -framework Cocoa -framework OpenGL -framework IOKit \
        -framework CoreFoundation -framework CoreGraphics \
        -L"$ROOT_DIR/target/release" -lazul \
        -Wl,-rpath,"$ROOT_DIR/target/release"
    echo "  → built OK"
fi

# ── Run and capture ─────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  AzSystemStyle_detect() output"
echo "═══════════════════════════════════════════════════════════════"
DETECTED=$("$BINARY")
echo "$DETECTED"

# ── Gather expected values from macOS ───────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  macOS system utilities (ground truth)"
echo "═══════════════════════════════════════════════════════════════"

PASS=0
FAIL=0
SKIP=0

check() {
    local label="$1" expected="$2" pattern="$3"
    if echo "$DETECTED" | grep -qi "$pattern"; then
        echo "  ✅ $label: $expected"
        PASS=$((PASS + 1))
    else
        echo "  ❌ $label: expected '$expected', pattern '$pattern' not found"
        FAIL=$((FAIL + 1))
    fi
}

skip() {
    local label="$1" reason="$2"
    echo "  ⏭  $label: $reason"
    SKIP=$((SKIP + 1))
}

# --- Theme (Dark / Light) ---
APPEARANCE=$(defaults read -g AppleInterfaceStyle 2>/dev/null || echo "Light")
if [[ "$APPEARANCE" == "Dark" ]]; then
    check "Theme" "Dark" "Dark"
else
    check "Theme" "Light" "Light"
fi

# --- OS version ---
OS_VER=$(sw_vers -productVersion)   # e.g. "15.3"
echo "  ℹ️  OS version: macOS $OS_VER"

# --- Platform ---
check "Platform" "macOS" "Mac"

# --- Language ---
LANG_OS=$(defaults read -g AppleLanguages 2>/dev/null | head -2 | tail -1 | tr -d ' ",()' || echo "unknown")
echo "  ℹ️  Primary language: $LANG_OS"

# --- Reduce motion ---
REDUCE_MOTION=$(defaults read com.apple.universalaccess reduceMotion 2>/dev/null || echo "0")
if [[ "$REDUCE_MOTION" == "1" ]]; then
    check "Reduce motion" "true" "prefers_reduced_motion.*true\|ReduceMotion.*true\|reduce_motion.*true"
else
    check "Reduce motion" "false" "prefers_reduced_motion.*false\|ReduceMotion.*false\|Inherit"
fi

# --- High contrast ---
INCREASE_CONTRAST=$(defaults read com.apple.universalaccess increaseContrast 2>/dev/null || echo "0")
if [[ "$INCREASE_CONTRAST" == "1" ]]; then
    check "High contrast" "true" "prefers_high_contrast.*true\|HighContrast.*true\|increase_contrast.*true"
else
    check "High contrast" "false" "prefers_high_contrast.*false\|HighContrast.*false\|Inherit"
fi

# --- Accent color ---
ACCENT_COLOR=$(defaults read -g AppleAccentColor 2>/dev/null || echo "not set (= blue)")
echo "  ℹ️  Accent color raw value: $ACCENT_COLOR"

# --- Font smoothing ---
FONT_SMOOTH=$(defaults read -g AppleFontSmoothing 2>/dev/null || echo "unset")
echo "  ℹ️  Font smoothing: $FONT_SMOOTH"

# --- Scrollbar visibility ---
SCROLLBAR=$(defaults read -g AppleShowScrollBars 2>/dev/null || echo "Automatic")
echo "  ℹ️  Scrollbar visibility: $SCROLLBAR"

# --- Double click interval ---
DBLCLICK=$(defaults read -g com.apple.mouse.doubleClickThreshold 2>/dev/null || echo "unset (default ~0.5s)")
echo "  ℹ️  Double-click threshold: $DBLCLICK"

# --- Reduce transparency ---
REDUCE_TRANS=$(defaults read com.apple.universalaccess reduceTransparency 2>/dev/null || echo "0")
echo "  ℹ️  Reduce transparency: $REDUCE_TRANS"

# --- Bold text ---
UIAccessibilityIsBoldTextEnabled=$(defaults read com.apple.universalaccess boldText 2>/dev/null || echo "0")
echo "  ℹ️  Bold text: $UIAccessibilityIsBoldTextEnabled"

# ── Summary ─────────────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "  Results: $PASS passed, $FAIL failed, $SKIP skipped"
echo "═══════════════════════════════════════════════════════════════"
exit $FAIL

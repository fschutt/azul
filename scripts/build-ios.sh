#!/usr/bin/env bash
# Build + bundle (+ sign + deploy) an Azul cdylib as a .app for iOS,
# entirely from the command line. No Xcode project required.
#
# Required tools (validated by check-prereqs-mobile.sh):
#   Full Xcode (or Xcode CLT with the iOS SDK separately installed) — Xcode
#       CLT alone (xcrun -p == /Library/Developer/CommandLineTools) does NOT
#       include the iphoneos / iphonesimulator SDKs.
#   xcrun simctl   for simulator targets
#   codesign       for device targets (optional unless IOS_SIGNING_IDENTITY set)
#   ios-deploy     to push to a physical iPhone (optional)
#
# Usage:
#   bash scripts/build-ios.sh aarch64-apple-ios-sim       # Apple-Silicon simulator
#   bash scripts/build-ios.sh aarch64-apple-ios           # device (requires signing)
#   bash scripts/build-ios.sh x86_64-apple-ios            # Intel simulator
#
# Env knobs:
#   AZ_IOS_DRYRUN=1        do everything up to and including bundle, but
#                          skip simctl install / launch / codesign / deploy.
#   AZ_IOS_FEATURES=...    cargo --features list. Default: 'std,logging,link-static,a11y'.
#   IOS_SIGNING_IDENTITY   codesign -s value for device deploy.
#   APP_NAME, BUNDLE_ID, DISPLAY_NAME, VERSION, BUILD, MIN_OS — Info.plist overrides.

set -euo pipefail

TARGET="${1:-aarch64-apple-ios-sim}"
case "$TARGET" in
    aarch64-apple-ios-sim|x86_64-apple-ios)
        IS_SIM=1
        ;;
    aarch64-apple-ios)
        IS_SIM=0
        ;;
    *)
        echo "unknown iOS target: $TARGET" >&2; exit 2 ;;
esac

APP_NAME="${APP_NAME:-AzulExample}"
BUNDLE_ID="${BUNDLE_ID:-com.azul.example}"
DISPLAY_NAME="${DISPLAY_NAME:-$APP_NAME}"
VERSION="${VERSION:-1.0}"
BUILD="${BUILD:-1}"
MIN_OS="${MIN_OS:-16.0}"
FEATURES="${AZ_IOS_FEATURES:-std,logging,link-static,a11y}"

if ! xcrun -p >/dev/null 2>&1; then
    echo "xcode-select not configured. Run 'xcode-select --install'." >&2
    exit 3
fi
SDK_SHORT=$([[ $IS_SIM -eq 1 ]] && echo iphonesimulator || echo iphoneos)
if ! xcrun --sdk "$SDK_SHORT" --show-sdk-path >/dev/null 2>&1; then
    echo "iOS SDK '$SDK_SHORT' is not installed." >&2
    echo "Install full Xcode (App Store) or 'xcodes install --latest'." >&2
    exit 3
fi

WORKSPACE_ROOT=$(cd "$(dirname "$0")/.." && pwd)

echo "==> cargo build --target $TARGET --release -p azul-dll --no-default-features --features '$FEATURES'"
( cd "$WORKSPACE_ROOT" \
  && cargo build --target "$TARGET" --release -p azul-dll \
       --no-default-features --features "$FEATURES" )

# `cargo build` with `crate-type = ["cdylib","staticlib","rlib"]` and an iOS
# target produces target/<triple>/release/libazul.dylib for sim and an
# .a static lib. To get an actual executable Mach-O we need a thin C
# main() that links against libazul.a — placeholder for that is the
# `examples/ios/main.c` (TODO: real example). For now the script bundles
# the dylib as a sanity check.

ARTIFACT="$WORKSPACE_ROOT/target/$TARGET/release/libazul.dylib"
if [[ ! -f "$ARTIFACT" ]]; then
    ARTIFACT="$WORKSPACE_ROOT/target/$TARGET/release/libazul.a"
fi
[[ -f "$ARTIFACT" ]] || { echo "missing $ARTIFACT — cargo did not produce a library" >&2; exit 4; }

BUNDLE_DIR="$WORKSPACE_ROOT/target/ios-bundle/${APP_NAME}-${TARGET}.app"
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR"

cp "$ARTIFACT" "$BUNDLE_DIR/$APP_NAME"
chmod +x "$BUNDLE_DIR/$APP_NAME" || true

# Render Info.plist from the template.
PLIST_TEMPLATE="$WORKSPACE_ROOT/scripts/ios/Info.plist"
sed \
    -e "s|@EXECUTABLE@|$APP_NAME|g" \
    -e "s|@BUNDLE_ID@|$BUNDLE_ID|g" \
    -e "s|@DISPLAY_NAME@|$DISPLAY_NAME|g" \
    -e "s|@VERSION@|$VERSION|g" \
    -e "s|@BUILD@|$BUILD|g" \
    -e "s|@MIN_OS@|$MIN_OS|g" \
    "$PLIST_TEMPLATE" > "$BUNDLE_DIR/Info.plist"

# PlistBuddy converts XML → binary plist (smaller; iOS prefers binary).
if command -v /usr/libexec/PlistBuddy >/dev/null 2>&1; then
    plutil -convert binary1 "$BUNDLE_DIR/Info.plist" 2>/dev/null || true
fi

echo "==> bundled: $BUNDLE_DIR"

if [[ "${AZ_IOS_DRYRUN:-}" == "1" ]]; then
    echo "AZ_IOS_DRYRUN=1 — stopping after bundle."
    exit 0
fi

if (( IS_SIM == 1 )); then
    # Simulator deploy — no signing.
    if xcrun simctl list -j devices booted 2>/dev/null | grep -q '"state" : "Booted"'; then
        echo "==> xcrun simctl install + launch"
        xcrun simctl install booted "$BUNDLE_DIR"
        xcrun simctl launch --console booted "$BUNDLE_ID" || true
    else
        echo "no booted simulator — APK at $BUNDLE_DIR; boot one with 'open -a Simulator'."
    fi
else
    # Device deploy — needs codesign + ios-deploy.
    if [[ -n "${IOS_SIGNING_IDENTITY:-}" ]]; then
        echo "==> codesign with '$IOS_SIGNING_IDENTITY'"
        codesign --force --timestamp=none \
            --sign "$IOS_SIGNING_IDENTITY" \
            --entitlements "$WORKSPACE_ROOT/scripts/ios/entitlements.xcent" \
            "$BUNDLE_DIR"
    else
        echo "IOS_SIGNING_IDENTITY not set — bundle unsigned at $BUNDLE_DIR"
    fi
    if command -v ios-deploy >/dev/null 2>&1; then
        ios-deploy --bundle "$BUNDLE_DIR" --justlaunch
    else
        echo "ios-deploy not on PATH — deploy manually."
    fi
fi

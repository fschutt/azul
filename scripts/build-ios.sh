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
#   AZ_IOS_LOCAL_NETWORK="<why the app uses the local network>"
#                          an app that uses IrohEndpoint (local-network discovery is ON by default):
#                          writes NSLocalNetworkUsageDescription (the text of the Local Network
#                          prompt) and NSBonjourServices = [_irohv1._udp] (the service the system's
#                          Bonjour is asked for on iOS). Without them the system refuses the lookup
#                          (kDNSServiceErr_PolicyDenied) and local_discovery_status says so.
#   AZ_IOS_USAGE="NSCameraUsageDescription=...<newline>NSMicrophoneUsageDescription=..."
#                          more usage descriptions, one Key=text per line (only NS...UsageDescription).
#   AZ_IOS_BONJOUR_SERVICES="_other._tcp,_more._udp"  more NSBonjourServices entries.
#   AZ_IOS_LINK_LIB=...    the prebuilt libazul.a of this target (scripts/demo_lib.sh) with its
#                          libazul.a.native-libs next to it: the app is built with link-dynamic against
#                          it (AZ_LINK_PATH; no engine crate compiles) and only its own bin is built
#   AZ_IOS_STRINGS_DIR=... a folder of <lang>.lproj/InfoPlist.strings copied into the bundle
#   AZ_IOS_BUILD_JSON=...  where cargo's JSON log goes (read by scripts/demo_gate.py)
#   AZ_IOS_SYMBOL_JSON=... where scripts/ci/ios_symbol_gate.py writes its numbers (default: nowhere); the gate fails the
#                          build when the binary imports a symbol newer than MIN_OS or not on iOS

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

# CRATE is the package to build. A normal example/demo is a BIN whose main()
# runs App::run → UIApplicationMain, i.e. it IS the iOS app executable. When
# CRATE is the library (azul-dll) we fall back to bundling the dylib (sanity
# check only). Pass via 2nd arg or AZ_IOS_CRATE.
CRATE="${2:-${AZ_IOS_CRATE:-azul-dll}}"
APP_NAME="${APP_NAME:-$CRATE}"
BUNDLE_ID="${BUNDLE_ID:-com.azul.${CRATE//-/_}}"
DISPLAY_NAME="${DISPLAY_NAME:-$APP_NAME}"
VERSION="${VERSION:-1.0}"
BUILD="${BUILD:-1}"
MIN_OS="${MIN_OS:-16.0}"
FEATURES="${AZ_IOS_FEATURES:-std,logging,link-static,a11y}"

# `xcrun -p` is NOT a valid invocation — xcrun exits 64 with
# "unrecognized option: -p" on every machine, Xcode or not. This probe was
# therefore ALWAYS true, so every iOS bundle attempt bailed before doing any
# work and the demo .app/.ipa artifacts were never produced once. The real
# capability test is whether the iOS SDK resolves.
if ! xcrun --sdk iphoneos --show-sdk-path >/dev/null 2>&1; then
    # Say WHAT is wrong. "xcode-select not configured" was the whole message,
    # printed ten times in a row in CI, and it does not distinguish "xcrun is
    # not on PATH at all" from "xcode-select points at a directory that is not
    # there" — which is the difference between an unfixable runner and a
    # one-line `sudo xcode-select -s`.
    {
        echo "cannot resolve an Xcode toolchain (\`xcrun -p\` failed)."
        if ! command -v xcrun >/dev/null 2>&1; then
            echo "  xcrun is not on PATH — no Xcode and no Command Line Tools are installed."
        else
            echo "  xcrun is at $(command -v xcrun)"
            echo "  xcode-select -p says: $(xcode-select -p 2>&1 || true)"
            echo "  DEVELOPER_DIR=${DEVELOPER_DIR:-<unset>}"
            echo "  /Applications/Xcode*.app: $(ls -d /Applications/Xcode*.app 2>/dev/null | tr '\n' ' ')"
        fi
        echo "  fix: sudo xcode-select -s /Applications/Xcode.app  (or 'xcode-select --install')"
    } >&2
    exit 3
fi
SDK_SHORT=$([[ $IS_SIM -eq 1 ]] && echo iphonesimulator || echo iphoneos)
if ! xcrun --sdk "$SDK_SHORT" --show-sdk-path >/dev/null 2>&1; then
    echo "iOS SDK '$SDK_SHORT' is not installed." >&2
    echo "Install full Xcode (App Store) or 'xcodes install --latest'." >&2
    exit 3
fi

# The one libazul of this target, built once (scripts/demo_lib.sh): the app links it over the C ABI.
LINK_LIB="${AZ_IOS_LINK_LIB:-}"
if [[ -n "$LINK_LIB" ]]; then
    [[ -f "$LINK_LIB" ]] || { echo "AZ_IOS_LINK_LIB=$LINK_LIB is not a file" >&2; exit 8; }
    LINK_LIB="$(cd "$(dirname "$LINK_LIB")" && pwd)/$(basename "$LINK_LIB")"
    export AZ_LINK_PATH="$LINK_LIB"
fi

# The workspace to BUILD. Normally this script's own parent (running from a
# checkout), but azul-doc embeds this file and materializes it into a cache
# directory — from there `dirname $0/..` is the cache, not a crate. So the
# caller may name the workspace explicitly.
WORKSPACE_ROOT="${AZ_WORKSPACE_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
# Where the templates + helper scripts live (android/*.java, cargo_bin_path.py).
# Same directory as this script when in-tree; the cache dir when materialized.
ASSET_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# azul-dll takes its features explicitly; a demo/example crate already pins its
# own azul features (link-static) in its Cargo.toml, so build it with defaults.
if [[ "$CRATE" == "azul-dll" ]]; then
    FEATURE_ARGS=(--no-default-features --features "$FEATURES")
else
    FEATURE_ARGS=()
    # Only the app's executable: the package's library is also a cdylib (the Android entry point), and an
    # iPhone has no use for a second copy of the app linked as a dynamic library.
    [[ -n "$LINK_LIB" ]] && FEATURE_ARGS=(--bin "$CRATE")
fi
# `${ARR[@]}` on an EMPTY array is an "unbound variable" error under `set -u`
# in bash 3.2, which is what macOS ships and therefore what every CI runner
# uses. A demo crate takes no feature args, so FEATURE_ARGS is empty for all
# five of them and this line aborted every single iOS bundle. Use the
# empty-safe expansion.
# the same iOS the library was built for (scripts/demo_lib.sh), and the one the Info.plist says
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-$MIN_OS}"
echo "==> cargo build --target $TARGET --release -p $CRATE ${FEATURE_ARGS[*]:-}"
CARGO_LOG="${AZ_IOS_BUILD_JSON:-$WORKSPACE_ROOT/target/ios-build-${CRATE}-${TARGET}.json}"
mkdir -p "$WORKSPACE_ROOT/target"
( cd "$WORKSPACE_ROOT" \
  && cargo build --target "$TARGET" --release -p "$CRATE" ${FEATURE_ARGS[@]+"${FEATURE_ARGS[@]}"} \
       --message-format=json-render-diagnostics > "$CARGO_LOG" )

# A bin crate produces an executable Mach-O whose main() runs App::run →
# UIApplicationMain, so it IS the app. Ask cargo where it put it rather than
# assuming the file is named after the package: `examples/azul-writer`
# declares `[[bin]] name = "azwriter"`, so the assumed
# target/<triple>/release/azul-writer does not exist and this fell through to
# the azul-dll fallback below — i.e. it would have bundled the LIBRARY as if it
# were the app. If CRATE really is the library (azul-dll) there is no bin and
# the dylib/.a fallback is correct.
ARTIFACT="$(python3 "$ASSET_ROOT/scripts/cargo_bin_path.py" "$CARGO_LOG" "$CRATE" 2>/dev/null || true)"
if [[ -z "$ARTIFACT" || ! -f "$ARTIFACT" ]]; then
    ARTIFACT="$WORKSPACE_ROOT/target/$TARGET/release/$CRATE"
fi
if [[ ! -f "$ARTIFACT" ]]; then
    ARTIFACT="$WORKSPACE_ROOT/target/$TARGET/release/libazul.dylib"
    [[ -f "$ARTIFACT" ]] || ARTIFACT="$WORKSPACE_ROOT/target/$TARGET/release/libazul.a"
fi
[[ -f "$ARTIFACT" ]] || { echo "missing $ARTIFACT — cargo did not produce a binary/library" >&2; exit 4; }

# <target>/<App>.app: the folder a zip of it carries is the app's own name, not the name of the build
BUNDLE_DIR="$WORKSPACE_ROOT/target/ios-bundle/${TARGET}/${APP_NAME}.app"
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR"

cp "$ARTIFACT" "$BUNDLE_DIR/$APP_NAME"
chmod +x "$BUNDLE_DIR/$APP_NAME" || true

# The extra Info.plist keys: usage descriptions and Bonjour service types from the environment, as
# XML (the Android script does the same for AZ_ANDROID_PERMISSIONS). Only `NS...UsageDescription`
# keys are taken from AZ_IOS_USAGE, so it cannot overwrite the bundle's own keys; text and service
# names are XML-escaped.
PLIST_EXTRA_FILE="$(mktemp)"
python3 - "$PLIST_EXTRA_FILE" <<'PY'
import os, re, sys
from xml.sax.saxutils import escape

entries = []          # (key, text)
for line in os.environ.get("AZ_IOS_USAGE", "").splitlines():
    key, sep, text = line.partition("=")
    key, text = key.strip(), text.strip()
    if sep and text and re.fullmatch(r"NS[A-Za-z]+UsageDescription", key):
        entries.append((key, text))
services = []
why = os.environ.get("AZ_IOS_LOCAL_NETWORK", "").strip()
if why:
    entries.append(("NSLocalNetworkUsageDescription", why))
    services.append("_irohv1._udp")
for name in os.environ.get("AZ_IOS_BONJOUR_SERVICES", "").split(","):
    name = name.strip()
    if re.fullmatch(r"_[A-Za-z0-9-]{1,15}\._(tcp|udp)", name) and name not in services:
        services.append(name)
out = []
seen = set()
for key, text in entries:
    if key not in seen:
        seen.add(key)
        out.append(f"    <key>{key}</key>\n    <string>{escape(text)}</string>")
if services:
    out.append("    <key>NSBonjourServices</key>\n    <array>\n"
               + "\n".join(f"        <string>{escape(s)}</string>" for s in services)
               + "\n    </array>")
open(sys.argv[1], "w").write("\n".join(out) + ("\n" if out else ""))
PY

# Render Info.plist from the template.
PLIST_TEMPLATE="$ASSET_ROOT/scripts/ios/Info.plist"
sed \
    -e "s|@EXECUTABLE@|$APP_NAME|g" \
    -e "s|@BUNDLE_ID@|$BUNDLE_ID|g" \
    -e "s|@DISPLAY_NAME@|$DISPLAY_NAME|g" \
    -e "s|@VERSION@|$VERSION|g" \
    -e "s|@BUILD@|$BUILD|g" \
    -e "s|@MIN_OS@|$MIN_OS|g" \
    -e "/@EXTRA_PLIST@/r $PLIST_EXTRA_FILE" \
    -e "/@EXTRA_PLIST@/d" \
    "$PLIST_TEMPLATE" > "$BUNDLE_DIR/Info.plist"
rm -f "$PLIST_EXTRA_FILE"

# What the template cannot say: which platform the bundle is for, and a launch screen (without one iOS
# runs the app letterboxed in a small window). plistlib, so the file stays well-formed whatever the
# environment put in it.
# A home-screen icon (scripts/demo_icon.py draws one from the app's name; without the script the app has none).
HAVE_ICON=0
if [[ -f "$ASSET_ROOT/scripts/demo_icon.py" && "${AZ_IOS_NO_ICON:-}" != "1" ]]; then
    python3 "$ASSET_ROOT/scripts/demo_icon.py" ios "$APP_NAME" "$BUNDLE_DIR" && HAVE_ICON=1
fi
python3 - "$BUNDLE_DIR/Info.plist" "$IS_SIM" "$HAVE_ICON" <<'PY'
import plistlib, sys
path, is_sim, icon = sys.argv[1], sys.argv[2] == "1", sys.argv[3] == "1"
with open(path, "rb") as f:
    plist = plistlib.load(f)
plist["CFBundleSupportedPlatforms"] = ["iPhoneSimulator" if is_sim else "iPhoneOS"]
plist["UILaunchScreen"] = {}
plist.pop("UILaunchStoryboardName", None)
if icon:
    plist["CFBundleIcons"] = {"CFBundlePrimaryIcon": {"CFBundleIconFiles": ["AppIcon60x60"]}}
    plist["CFBundleIcons~ipad"] = {"CFBundlePrimaryIcon": {"CFBundleIconFiles": ["AppIcon60x60", "AppIcon76x76", "AppIcon83.5x83.5"]}}
with open(path, "wb") as f:
    plistlib.dump(plist, f, fmt=plistlib.FMT_XML)
PY
# The localized texts of the usage descriptions: <lang>.lproj/InfoPlist.strings (the app's own folder of them).
if [[ -n "${AZ_IOS_STRINGS_DIR:-}" && -d "${AZ_IOS_STRINGS_DIR}" ]]; then
    for lproj in "$AZ_IOS_STRINGS_DIR"/*.lproj; do
        [[ -d "$lproj" ]] && cp -R "$lproj" "$BUNDLE_DIR/"
    done
fi
# The executable without its symbols (the release profile keeps line tables for profiling).
strip -S -x "$BUNDLE_DIR/$APP_NAME" 2>/dev/null || true

# A binary that imports a symbol which the oldest iOS it claims (MIN_OS) does not have dies at launch with "Symbol not found", and
# the Simulator, newer than MIN_OS, never shows it. scripts/ci/ios_symbol_gate.py reads the availability the SDK's headers declare
# for every strong undefined import (xcrun nm -m -u) and fails on a symbol newer than MIN_OS or not on iOS at all. Only when the
# gate is there (the cache azul-doc materializes this script into does not carry it); AZ_IOS_SYMBOL_JSON names where its numbers go.
if [[ -f "$ASSET_ROOT/scripts/ci/ios_symbol_gate.py" ]]; then
    python3 "$ASSET_ROOT/scripts/ci/ios_symbol_gate.py" --binary "$BUNDLE_DIR/$APP_NAME" --min-os "$MIN_OS" \
        --sdk "$(xcrun --sdk "$SDK_SHORT" --show-sdk-path)" --json "${AZ_IOS_SYMBOL_JSON:-/dev/null}"
fi

# PlistBuddy converts XML → binary plist (smaller; iOS prefers binary).
if command -v /usr/libexec/PlistBuddy >/dev/null 2>&1; then
    plutil -convert binary1 "$BUNDLE_DIR/Info.plist" 2>/dev/null || true
fi

# A Simulator app runs as an arm64 process of the Mac, and the kernel kills an arm64 executable whose
# signature is gone (strip above removed the linker's). Ad-hoc signed, last, after the last change to the
# bundle; the DEVICE bundle stays unsigned on purpose (AzReview signs it for the phone it is installed on).
if (( IS_SIM == 1 )) && command -v codesign >/dev/null 2>&1; then
    codesign --force --sign - --timestamp=none "$BUNDLE_DIR"
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
            --entitlements "$ASSET_ROOT/scripts/ios/entitlements.xcent" \
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

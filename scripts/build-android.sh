#!/usr/bin/env bash
# Build + package + sign + (optional) deploy an Azul cdylib as an APK,
# entirely from the command line. No Gradle, no Android Studio.
#
# Required tools (validated by check-prereqs-mobile.sh):
#   sdkmanager + build-tools 34.0.0 (aapt2, zipalign, apksigner)
#   platform-tools (adb)
#   NDK 27 (linker config in workspace .cargo/config.toml)
#   JDK 17 (apksigner / keytool)
#
# Usage:
#   bash scripts/build-android.sh aarch64-linux-android [APP_NAME] [PACKAGE]
#
# Env knobs:
#   AZ_ANDROID_NO_DEPLOY=1   skip adb install + am start
#   AZ_ANDROID_FEATURES=...  override cargo --features list (default: same as Sprint F gate)

set -euo pipefail

TARGET="${1:-aarch64-linux-android}"
APP_NAME="${2:-azul-example}"
PACKAGE="${3:-com.azul.example}"
# CRATE is the package built as the NativeActivity cdylib. A demo opts into
# Android by declaring `crate-type=["cdylib","rlib"]` + an `android_main` shim
# (see guide/mobile.md); a plain bin produces no .so and is skipped here.
CRATE="${4:-${AZ_ANDROID_CRATE:-azul-dll}}"
LIB_NAME="${CRATE//-/_}"
VERSION_CODE="${VERSION_CODE:-1}"
VERSION_NAME="${VERSION_NAME:-1.0}"
LABEL="${LABEL:-${APP_NAME}}"

case "$TARGET" in
    aarch64-linux-android)    ABI=arm64-v8a ;;
    armv7-linux-androideabi)  ABI=armeabi-v7a ;;
    x86_64-linux-android)     ABI=x86_64 ;;
    i686-linux-android)       ABI=x86 ;;
    *) echo "unknown Android target: $TARGET" >&2; exit 2 ;;
esac

: "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
: "${ANDROID_NDK_HOME:=$ANDROID_HOME/ndk/27.0.12077973}"
export ANDROID_HOME ANDROID_NDK_HOME

if [[ -z "${JAVA_HOME:-}" ]] && [[ -d /opt/homebrew/opt/openjdk@17 ]]; then
    export JAVA_HOME="/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home"
fi

BT="$ANDROID_HOME/build-tools/34.0.0"
PLATFORM="$ANDROID_HOME/platforms/android-34"

for need in "$BT/aapt2" "$BT/zipalign" "$BT/apksigner" "$PLATFORM/android.jar"; do
    [[ -e "$need" ]] || { echo "missing $need — run sdkmanager 'build-tools;34.0.0' 'platforms;android-34'" >&2; exit 3; }
done

WORKSPACE_ROOT=$(cd "$(dirname "$0")/.." && pwd)
BUILD_DIR="$WORKSPACE_ROOT/target/android-bundle/${APP_NAME}-${ABI}"
mkdir -p "$BUILD_DIR/lib/$ABI"

FEATURES="${AZ_ANDROID_FEATURES:-std,logging,link-static,a11y}"

# azul-dll takes its features explicitly; a demo/example crate pins its own azul
# features (link-static) in Cargo.toml, so build it with its defaults.
if [[ "$CRATE" == "azul-dll" ]]; then
    FEATURE_ARGS=(--no-default-features --features "$FEATURES")
else
    FEATURE_ARGS=()
fi
# --lib: Android loads the cdylib (.so) via NativeActivity; build ONLY the
# library target. A demo's `[[bin]]` is the desktop/iOS entry point and links as
# an executable (no undefined symbols allowed), which fails on the NDK media
# symbols — and we don't ship it in the APK anyway.
echo "==> cargo build --lib --target $TARGET --release -p $CRATE ${FEATURE_ARGS[*]}"
(cd "$WORKSPACE_ROOT" \
  && cargo build --lib --target "$TARGET" --release -p "$CRATE" "${FEATURE_ARGS[@]}")

SRC_SO="$WORKSPACE_ROOT/target/$TARGET/release/lib${LIB_NAME}.so"
[[ -f "$SRC_SO" ]] || { echo "missing $SRC_SO — '$CRATE' produced no cdylib (.so). A demo must declare crate-type=cdylib + android_main to ship as an APK; skipping." >&2; exit 4; }
cp "$SRC_SO" "$BUILD_DIR/lib/$ABI/lib${LIB_NAME}.so"

# Java sources for the optional native-gesture bridge. If present, we
# compile them with javac, dex with d8, and ship classes.dex inside the
# APK (which forces android:hasCode="true" in the manifest below).
# AZ_ANDROID_NO_JAVA=1 skips the dex even when sources exist.
JAVA_SRC_DIR="$WORKSPACE_ROOT/scripts/android"
JAVA_SOURCES=()
HAS_JAVA=0
if [[ "${AZ_ANDROID_NO_JAVA:-}" != "1" ]]; then
    while IFS= read -r f; do
        JAVA_SOURCES+=("$f")
        HAS_JAVA=1
    done < <(find "$JAVA_SRC_DIR" -maxdepth 2 -name '*.java' -print 2>/dev/null)
fi

DEX_FILE=""
if (( HAS_JAVA == 1 )); then
    : "${JAVA_HOME:=$(brew --prefix openjdk@17 2>/dev/null)/libexec/openjdk.jdk/Contents/Home}"
    export JAVA_HOME
    [[ -x "$JAVA_HOME/bin/javac" ]] \
        || { echo "javac not found at $JAVA_HOME/bin — AZ_ANDROID_NO_JAVA=1 to skip" >&2; exit 5; }
    echo "==> javac ${#JAVA_SOURCES[@]} Java source(s)"
    rm -rf "$BUILD_DIR/classes" "$BUILD_DIR/dex"
    mkdir -p "$BUILD_DIR/classes" "$BUILD_DIR/dex"
    "$JAVA_HOME/bin/javac" -source 11 -target 11 \
        -classpath "$PLATFORM/android.jar" \
        -d "$BUILD_DIR/classes" \
        "${JAVA_SOURCES[@]}"
    echo "==> d8 classes -> classes.dex"
    "$BT/d8" \
        --output "$BUILD_DIR/dex" \
        $(find "$BUILD_DIR/classes" -name '*.class')
    DEX_FILE="$BUILD_DIR/dex/classes.dex"
    [[ -f "$DEX_FILE" ]] || { echo "d8 did not produce classes.dex" >&2; exit 6; }
fi

# Manifest: substitute placeholders into the template, and flip
# android:hasCode="true" when we're shipping a .dex.
MANIFEST_TEMPLATE="$WORKSPACE_ROOT/scripts/android/AndroidManifest.xml"
MANIFEST_OUT="$BUILD_DIR/AndroidManifest.xml"
HAS_CODE_VALUE="false"
if (( HAS_JAVA == 1 )); then
    HAS_CODE_VALUE="true"
fi
sed \
    -e "s|@PACKAGE@|$PACKAGE|g" \
    -e "s|@LABEL@|$LABEL|g" \
    -e "s|@LIB_NAME@|${LIB_NAME}|g" \
    -e "s|@VERSION_CODE@|$VERSION_CODE|g" \
    -e "s|@VERSION_NAME@|$VERSION_NAME|g" \
    -e "s|android:hasCode=\"false\"|android:hasCode=\"$HAS_CODE_VALUE\"|g" \
    "$MANIFEST_TEMPLATE" > "$MANIFEST_OUT"

cd "$BUILD_DIR"

echo "==> aapt2 link (compile manifest)"
"$BT/aapt2" link \
    --manifest AndroidManifest.xml \
    -I "$PLATFORM/android.jar" \
    -o base.apk

echo "==> add lib/$ABI/libazul.so to APK"
( cd lib && zip -r ../base.apk "$ABI/" >/dev/null )

# Ship classes.dex inside the APK (at the root, where Android expects it).
if [[ -n "$DEX_FILE" ]]; then
    echo "==> add classes.dex to APK"
    ( cd dex && zip -r ../base.apk classes.dex >/dev/null )
fi

echo "==> zipalign"
"$BT/zipalign" -f 4 base.apk aligned.apk

# Debug keystore — generate once if absent. apksigner is happy with it.
KS="$WORKSPACE_ROOT/scripts/android/debug.keystore"
if [[ ! -f "$KS" ]]; then
    echo "==> creating debug keystore at $KS"
    keytool -genkeypair \
        -keystore "$KS" -alias androiddebugkey \
        -keyalg RSA -keysize 2048 -validity 10000 \
        -storepass android -keypass android \
        -dname "CN=Android Debug,O=Android,C=US"
fi

echo "==> apksigner sign"
"$BT/apksigner" sign \
    --ks "$KS" --ks-key-alias androiddebugkey \
    --ks-pass pass:android \
    aligned.apk

echo "==> built: $BUILD_DIR/aligned.apk"

if [[ "${AZ_ANDROID_NO_DEPLOY:-}" == "1" ]]; then
    echo "AZ_ANDROID_NO_DEPLOY=1 — skipping adb install"
    exit 0
fi

if ! command -v adb >/dev/null 2>&1; then
    echo "adb not on PATH — APK is at aligned.apk, deploy manually."
    exit 0
fi

echo "==> adb install -r aligned.apk"
adb install -r aligned.apk || { echo "no connected device — APK at $BUILD_DIR/aligned.apk" >&2; exit 0; }
adb shell am start -n "$PACKAGE/android.app.NativeActivity"

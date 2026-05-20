#!/usr/bin/env bash
# Sprint J / CI gate: cargo check across every mobile target.
#
# Targets:
#   aarch64-apple-ios          (device)
#   aarch64-apple-ios-sim      (Apple-silicon simulator)
#   x86_64-apple-ios           (Intel simulator)
#   aarch64-linux-android      (ARM64 device)
#   x86_64-linux-android       (x86_64 emulator)
#
# iOS targets that don't have the iOS SDK installed are SKIPPED with an
# INFO line, not FAIL — link-level breakage is the SDK's concern and
# this script only validates source-level correctness. cargo check
# doesn't link, so it surfaces every type / trait / import error before
# the SDK enters the picture.
#
# Exit code: 0 iff every non-skipped target checks clean.

set -u

WORKSPACE_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$WORKSPACE_ROOT"

: "${ANDROID_HOME:=/opt/homebrew/share/android-commandlinetools}"
: "${ANDROID_NDK_HOME:=$ANDROID_HOME/ndk/27.0.12077973}"
export ANDROID_HOME ANDROID_NDK_HOME
if [[ -z "${JAVA_HOME:-}" ]] && [[ -d /opt/homebrew/opt/openjdk@17 ]]; then
    export JAVA_HOME="/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home"
fi
export PATH="$HOME/.cargo/bin:${ANDROID_HOME}/build-tools/34.0.0:${ANDROID_HOME}/platform-tools:${JAVA_HOME}/bin:$PATH"

FEATURES='std,logging,link-static,a11y'
FLAGS=(-p azul-dll --release --no-default-features --features "$FEATURES")

red()   { printf '\033[31m%s\033[0m' "$*"; }
green() { printf '\033[32m%s\033[0m' "$*"; }
yellow(){ printf '\033[33m%s\033[0m' "$*"; }

has_ios_sdk() {
    xcrun --sdk iphonesimulator --show-sdk-path >/dev/null 2>&1
}

ANY_FAIL=0
ANY_SKIP=0
# macOS ships bash 3.2 — no associative arrays. Use a single Summary buffer.
SUMMARY=""

check_target() {
    triple=$1
    # `cargo check` validates types only — no linker, so no iOS SDK
    # needed. The SDK gate matters for `cargo build`; this script's
    # purpose is to catch source-level regressions before the SDK
    # enters the picture, so we always attempt the check.
    started=$(date +%s)
    log=$(mktemp)
    if cargo check --target "$triple" "${FLAGS[@]}" >"$log" 2>&1; then
        elapsed=$(( $(date +%s) - started ))
        printf '  %s   %s  (%ss)\n' "$(green '[ok]')" "$triple" "$elapsed"
        SUMMARY="$SUMMARY"$'\n'"  $triple   ok (${elapsed}s)"
    else
        printf '  %s   %s\n' "$(red '[fail]')" "$triple"
        tail -25 "$log"
        SUMMARY="$SUMMARY"$'\n'"  $triple   FAIL"
        ANY_FAIL=1
    fi
    rm -f "$log"
}

printf '==> cargo check across mobile targets (features: %s)\n' "$FEATURES"
for triple in \
    aarch64-apple-ios \
    aarch64-apple-ios-sim \
    x86_64-apple-ios \
    aarch64-linux-android \
    x86_64-linux-android
do
    check_target "$triple"
done

echo
echo "==> Summary"
printf '%s\n' "$SUMMARY"

if [ "$ANY_FAIL" = "1" ]; then
    echo
    red "FAIL — see [fail] lines above."; echo
    exit 1
fi
if [ "$ANY_SKIP" = "1" ]; then
    echo
    yellow "PASS (with skipped iOS targets — install Xcode to lift the gate)"; echo
fi
exit 0

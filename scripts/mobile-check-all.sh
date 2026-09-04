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
# EVERY target is checked, including iOS on a machine with no iOS SDK.
# `cargo check` does not link, so it needs only the Rust `std` for the
# target (`rustup target add <triple>`) — no NDK, no iOS SDK, no cross C
# toolchain. That is the whole value of this gate: platform shells are
# #[cfg]-gated, so nothing under shell2/{android,ios,windows,linux} is
# compiled by a normal build on your machine, and an import or a struct
# literal that only breaks on one target stays invisible until here.
#
# Exit code: 0 iff every target checks clean.

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

ANY_FAIL=0
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

printf '==> cargo check across mobile + desktop targets (features: %s)\n' "$FEATURES"
for triple in \
    aarch64-apple-ios \
    aarch64-apple-ios-sim \
    x86_64-apple-ios \
    aarch64-linux-android \
    x86_64-linux-android \
    x86_64-unknown-linux-gnu \
    x86_64-pc-windows-gnu \
    x86_64-pc-windows-msvc
do
    check_target "$triple"
done

# The E2E-on-device path (shell2/run.rs ANDROID_DEBUG_CHANNEL + the
# register_debug_timer call in android_main) is behind
# `any(debug-server, e2e-scripting)`, which the feature set above does NOT
# include — so the loop over targets compiles right past it. One extra check so
# that code cannot rot unnoticed; it is the configuration
# `azul-doc mobile run --e2e` actually builds.
echo
printf '==> android + debug-server (the on-device E2E configuration)\n'
started=$(date +%s)
log=$(mktemp)
if cargo check --target aarch64-linux-android -p azul-dll --release \
       --no-default-features --features "$FEATURES,debug-server" >"$log" 2>&1; then
    elapsed=$(( $(date +%s) - started ))
    printf '  %s   %s  (%ss)\n' "$(green '[ok]')" "aarch64-linux-android+debug-server" "$elapsed"
    SUMMARY="$SUMMARY"$'\n'"  aarch64-linux-android+debug-server   ok (${elapsed}s)"
else
    printf '  %s   %s\n' "$(red '[fail]')" "aarch64-linux-android+debug-server"
    tail -25 "$log"
    SUMMARY="$SUMMARY"$'\n'"  aarch64-linux-android+debug-server   FAIL"
    ANY_FAIL=1
fi
rm -f "$log"

echo
echo "==> Summary"
printf '%s\n' "$SUMMARY"

if [ "$ANY_FAIL" = "1" ]; then
    echo
    red "FAIL — see [fail] lines above."; echo
    exit 1
fi
exit 0

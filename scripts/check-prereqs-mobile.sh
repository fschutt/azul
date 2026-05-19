#!/usr/bin/env bash
# Verify host has everything needed to cross-compile Azul for iOS + Android.
# Exits 0 if Android is ready (iOS gaps are warnings until full Xcode is installed).

set -u

red()   { printf '\033[31m%s\033[0m\n' "$*"; }
green() { printf '\033[32m%s\033[0m\n' "$*"; }
yellow(){ printf '\033[33m%s\033[0m\n' "$*"; }
bold()  { printf '\033[1m%s\033[0m\n' "$*"; }

ANDROID_OK=1
IOS_OK=1

bold "==> Rust targets"
INSTALLED=$(rustup target list --installed)
for t in aarch64-apple-ios aarch64-apple-ios-sim aarch64-linux-android x86_64-linux-android; do
  if echo "$INSTALLED" | grep -q "^${t}$"; then
    green   "  [ok]   ${t}"
  else
    yellow  "  [miss] ${t}  (run: rustup target add ${t})"
    case "$t" in *android*) ANDROID_OK=0;; *ios*) IOS_OK=0;; esac
  fi
done

bold "==> Xcode / iOS SDK"
if [[ -x /Applications/Xcode.app/Contents/Developer/usr/bin/xcrun ]] \
   || xcrun --sdk iphonesimulator --show-sdk-path >/dev/null 2>&1; then
  green "  [ok]   iOS SDK detected"
else
  yellow "  [miss] iOS SDK not installed."
  yellow "         Install Xcode from the App Store (free, ~13 GB),"
  yellow "         or use 'xcodes' to install a specific Xcode version:"
  yellow "           brew install xcodes && xcodes install --latest"
  IOS_OK=0
fi
if command -v ios-deploy >/dev/null 2>&1; then
  green "  [ok]   ios-deploy"
else
  yellow "  [opt]  ios-deploy (only needed for physical-device deploy: brew install ios-deploy)"
fi

bold "==> Android SDK / NDK"
ANDROID_HOME=${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}
export ANDROID_HOME
if [[ -d "$ANDROID_HOME" ]]; then
  green "  [ok]   ANDROID_HOME=$ANDROID_HOME"
else
  red   "  [err]  ANDROID_HOME not found ($ANDROID_HOME)"; ANDROID_OK=0
fi
for tool in sdkmanager adb aapt2 apksigner zipalign; do
  if command -v "$tool" >/dev/null 2>&1 \
     || [[ -x "$ANDROID_HOME/build-tools/34.0.0/$tool" ]]; then
    green "  [ok]   $tool"
  else
    yellow "  [miss] $tool  (run sdkmanager 'build-tools;34.0.0')"
    ANDROID_OK=0
  fi
done

NDK_DIR=$(ls -d "$ANDROID_HOME/ndk/"*/ 2>/dev/null | head -1)
if [[ -n "${NDK_DIR:-}" && -d "$NDK_DIR" ]]; then
  green "  [ok]   NDK at $NDK_DIR"
else
  yellow "  [miss] No NDK (run: sdkmanager 'ndk;27.0.12077973')"
  ANDROID_OK=0
fi

bold "==> Java"
JAVA_HOME=${JAVA_HOME:-$(/usr/libexec/java_home 2>/dev/null || true)}
if [[ -z "${JAVA_HOME:-}" ]] && [[ -d /opt/homebrew/opt/openjdk@17 ]]; then
  JAVA_HOME="/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home"
fi
export JAVA_HOME
if [[ -n "${JAVA_HOME:-}" && -x "$JAVA_HOME/bin/java" ]]; then
  green "  [ok]   JAVA_HOME=$JAVA_HOME"
else
  yellow "  [miss] No JDK (brew install openjdk@17)"
  ANDROID_OK=0
fi

bold "==> cargo-ndk"
if command -v cargo-ndk >/dev/null 2>&1; then
  green "  [ok]   cargo-ndk"
else
  yellow "  [miss] cargo-ndk (cargo install cargo-ndk)"
  ANDROID_OK=0
fi

bold "==> Summary"
if (( IOS_OK == 1 )); then
  green "  iOS  : ready"
else
  yellow "  iOS  : need full Xcode + iOS SDK"
fi
if (( ANDROID_OK == 1 )); then
  green "  Android: ready"
  exit 0
else
  red "  Android: missing prerequisites above"
  exit 2
fi

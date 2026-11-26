#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): one app linked against a prebuilt libazul.so on Android.
#   bash scripts/ci/exp_android.sh <rust-target> <smoke|nosmoke>
# Builds the shared lib once (scripts/demo_lib.sh), links AzWidgets against it (link-dynamic, no engine
# crate), packs the APK with the lib beside the app's library, checks the dynamic section, and with
# `smoke` boots it on an emulator. Everything lands in demo-out/.
set -euxo pipefail
target="${1:?target}"; smoke="${2:-nosmoke}"
sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
df -h / | tail -n 1
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-$JAVA_HOME}"
rustup target add "$target"
case "$target" in aarch64-linux-android) abi=arm64-v8a ;; x86_64-linux-android) abi=x86_64 ;; esac
FEATURES="std,logging,link-static,a11y,android-activity,map-tiles,pdf"
mkdir -p demo-out
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$FEATURES" "demo-out/lib-$abi"
echo "LIB SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
AZ_ANDROID_NO_DEPLOY=1 AZ_ANDROID_LINK_LIB="demo-out/lib-$abi/libazul.so" AZ_ANDROID_BUILD_JSON="demo-out/AzWidgets-$abi.json" \
  bash scripts/build-android.sh "$target" AzWidgets com.azul.AzWidgets AzWidgets
echo "APP SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_gate.py "demo-out/AzWidgets-$abi.json" --json "demo-out/gate-$abi.json"
cp "target/android-bundle/AzWidgets-$abi/aligned.apk" "demo-out/AzWidgets-$abi.apk"
unzip -l "demo-out/AzWidgets-$abi.apk" | tee "demo-out/apk-$abi.txt"
readelf -d "target/android-bundle/AzWidgets-$abi/lib/$abi/libazul_widgets.so" | tee "demo-out/dynamic-$abi.txt" | grep -E 'NEEDED|SONAME' || true
mkdir -p demo-out/codegen && cp target/codegen/dll_api_external.rs target/codegen/reexports.rs demo-out/codegen/ || true
rm -f "demo-out/AzWidgets-$abi.json.bak"
if [ "$smoke" = "smoke" ]; then
  bash scripts/ci/android_smoke.sh "demo-out/smoke-$abi" 15 "demo-out/AzWidgets-$abi.apk" || echo "SMOKE FAILED"
fi

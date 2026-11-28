#!/usr/bin/env bash
# android_codec_proof.sh: the H.264 round trip of AzSelfTest's `system.video_codec` row on the x86_64 emulator, in ONE job on ONE
# Linux runner: the QA library with iroh (mobile-e2e-iroh) built from THIS tree, AzSelfTest against it, the QA's emulator
# (scripts/ci/android_probe.sh), the self-test run unattended, the codec's own log lines (logcat tag azul-video) kept.
#
#   bash scripts/ci/android_codec_proof.sh
#
# Lands in demo-out/: apps/AzSelfTest-e2e-android-x86_64.apk, probe/ (marker.txt: the AZ_SELFTEST_REPORT line, start-1.log: the whole
# logcat, probe.md), report-line.txt, azul-video-logcat.txt (the codec's own lines). The row system.video_codec holds the numbers:
# PSNR, frame count, first-frame latency, milliseconds a frame.
#
# On a runner: scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_codec_proof.sh' --codegen --artifacts 'demo-out/probe/*
# demo-out/azul-video-logcat.txt
# demo-out/video-row.txt
# demo-out/report-line.txt
# demo-out/results/*.json
# demo-out/libs/*/libazul.info.json'
set -uxo pipefail
target=x86_64-linux-android
col=android-x86_64
variant=mobile-e2e-iroh

if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  python3 -m pip install --quiet --user tomli || true
fi

export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
rustup target add "$target"
mkdir -p demo-out/libs demo-out/apps demo-out/results

# 1. the library and the app, built from this tree
features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
echo "== $variant: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant" || { echo "LIB FAILED"; exit 1; }
echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_build.py android --target "$target" --apps AzSelfTest --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant "$variant" || { echo "APP FAILED"; exit 1; }
apk="demo-out/apps/AzSelfTest-e2e-$col.apk"
ls -la demo-out/apps
[ -f "$apk" ] || { echo "no $apk"; exit 1; }

# 2. the emulator and the run: scripts/ci/android_probe.sh boots the QA's AOSP emulator (API 34, x86_64), installs the APK, sets
# debug.az.selftest=1 (the unattended run) and waits for the AZ_SELFTEST_REPORT line; the whole logcat is probe/start-1.log
bash scripts/ci/android_probe.sh demo-out/probe "$apk" --prop debug.az.selftest=1 --prop debug.az.selftest_rows=system.video_codec --marker AZ_SELFTEST_REPORT --seconds 150
code=$?
echo "probe exit: $code"
grep -a "azul-video" demo-out/probe/start-1.log > demo-out/azul-video-logcat.txt 2>/dev/null || true
grep -a "AZ_SELFTEST_ROW" demo-out/probe/start-1.log > demo-out/video-row.txt 2>/dev/null || true
cat demo-out/video-row.txt | cut -c1-1500 || true
grep -a "AZ_SELFTEST_REPORT" demo-out/probe/start-1.log | head -c 40000 > demo-out/report-line.txt 2>/dev/null || true
grep -a -o '"id":"system.video_codec"[^}]*}' demo-out/report-line.txt | head -c 4000 || true
exit 0

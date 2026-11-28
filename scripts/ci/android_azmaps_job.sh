#!/usr/bin/env bash
# android_azmaps_job.sh <build run id>: ONE runner job: AzMaps of the tree it runs in (the plain phone library, the app against it) and AzMaps of an earlier
# build run (its APK, named -BEFORE-), each started on the same emulator and looked at twice (scripts/ci/android_smoke.sh): do the map's tiles show on the
# CPU renderer? Built for E62 (the Android and iOS shells never drained the VirtualView updates before a CPU frame: an empty map).
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/android_azmaps_job.sh 46089143227' --codegen --artifacts 'demo-out/qa-azmaps/**
#   demo-out/results/*'
# demo-out/qa-azmaps/<apk name>/screenshot.png and screenshot-2.png are the pictures (60 s after the start, and later).
set -euxo pipefail
before_run="${1:?the build run whose AzMaps APK is the BEFORE}"
target=x86_64-linux-android
col=android-x86_64
if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  sudo rm -rf /usr/share/dotnet /opt/ghc /usr/local/.ghcup /opt/hostedtoolcache/CodeQL 2>/dev/null || true
  python3 -m pip install --quiet --user tomli || true
fi
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
rustup target add "$target"
mkdir -p demo-out/libs demo-out/apps demo-out/results before
gh run download "$before_run" -R "$GITHUB_REPOSITORY" -n remote-Linux -D before
cp before/demo-out/apps/AzMaps-android-x86_64.apk demo-out/apps/AzMaps-BEFORE-android-x86_64.apk
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
features="$(python3 scripts/demo_apps.py features "$col" mobile)"
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile"
python3 scripts/demo_build.py android --target "$target" --apps AzMaps --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant ""
ls -la demo-out/apps
set +e
bash scripts/ci/android_smoke.sh demo-out/qa-azmaps 60 demo-out/apps/AzMaps-BEFORE-android-x86_64.apk demo-out/apps/AzMaps-android-x86_64.apk
exit 0

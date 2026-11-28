#!/usr/bin/env bash
# android_tabs_job.sh: ONE runner job: the tab strip on a Pixel 8 emulator with the punch-hole camera (examples/azul-widgets/e2e/tab_touch*.json).
# It builds the QA variant of AzWidgets (the `mobile-e2e` phone library: debug server and e2e scripting) from the tree it runs in, starts a Pixel 8
# emulator, turns the system's punch-hole cutout on (cmd overlay enable com.android.internal.display.cutout.emulation.hole) and runs
# scripts/ci/android_qa.sh's `tabs` session: portrait, then the phone turned to landscape (the hole at the side); per orientation the device's own
# pictures of a plain start (light, dark), and per scenario file a recording and the engine's frames (demo-out/qa-tabs/<mode>/tabs-<orientation>/).
#
#   scripts/remote.sh run <full sha | ref> ubuntu-22.04 'bash scripts/ci/android_tabs_job.sh' --codegen --artifacts 'demo-out/qa-tabs/**
#   demo-out/results/*
#   demo-out/plan.txt'
set -uxo pipefail
target=x86_64-linux-android
col=android-x86_64
variant=mobile-e2e
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
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
echo "FEATURES $variant: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant" || { echo "LIB $variant FAILED"; exit 3; }
echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_build.py android --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "$variant" \
  || echo "demo_build.py returned non-zero"
ls -la demo-out/apps
apk="demo-out/apps/AzWidgets-e2e-$col.apk"
[ -e "$apk" ] || { echo "no $apk"; exit 3; }

# the emulator: Pixel 8 (its profile), the hole overlay; portrait and landscape inside the session
AZ_ANDROID_DEVICE=pixel_8 AZ_ANDROID_OVERLAYS=com.android.internal.display.cutout.emulation.hole \
  timeout 4500 bash scripts/ci/android_qa.sh demo-out/qa-tabs "$apk" swiftshader_indirect "AzWidgets tabs" tabs
echo "SESSION tabs: exit $?"
exit 0

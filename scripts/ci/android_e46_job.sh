#!/usr/bin/env bash
# android_e46_job.sh [<App> ...]  (default: AzWidgets AzWriter): ONE runner job that re-measures E46 / E47 / E60, the black frames of the Android cold start,
# per app, on the tree it runs in (any SHA or branch that has this file and the scripts/ci/ harness).
#
#   scripts/remote.sh run <full sha | ref> ubuntu-22.04 'AZ_FLICKER_STARTUP=1 AZ_E46_CONTROL=1 bash scripts/ci/android_e46_job.sh AzWidgets AzWriter' --codegen \
#       --artifacts 'demo-out/qa-e46-*/**
#   demo-out/results/*'
#
# It builds the PLAIN phone library (`mobile`) and the plain apps here, then per app the flash audit on its own emulator boot (API 34 AOSP, swiftshader_indirect,
# the Pixel 6 profile): scripts/ci/qa_android_flicker.sh <APK> = a screen recording at 30 fps of cold start, rotation there and back, dark mode on and off,
# the keyboard, home and back, cut into frames (ffmpeg); a stretch of flat frames is classified (the system's starting window = the first flat run, light, TOLD;
# any black run over 100 ms, any later flat run: FAIL). With AZ_E46_CONTROL=1 the control is measured too: the FLOOR APK (scripts/floor-apk: an empty Java
# Activity, no Azul code), so that what the system does by itself is on the same emulator and the same recording.
#
# Per app, in demo-out/qa-e46-<App>/swiftshader_indirect/flicker/ :
#   flicker.md      the verdicts in words (the frame numbers of every flat run, 'told' for the starting window)
#   flicker.json    flash.flagged[]: per flat run {index, start_s, frames, ms, black, splash, fail, rgb: the mean colour}; flash.marks[]: the scenario's marks in recording seconds
#   flicker.mp4     the recording itself (30 fps, the emulator's own pixels)
#   cold-start-logcat.txt   the whole log from the cold start to 2.5 s after the app is in front (system + app, threadtime): the timeline. With AZ_FLICKER_STARTUP=1
#                   the engine prints its `[startup]` table at the first present (the engine's clock, first_present, layout passes) into it.
#   strips/flagged-frames.png   one row per flag: the frame before, first, middle, last of the run and the one after.
# The numbers of an emulator on a shared runner are indicative: compare two builds in the same job (run it on both SHAs) and read the frames, not the milliseconds.
set -uxo pipefail
apps="${*:-AzWidgets AzWriter}"
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
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e | tee demo-out/plan.txt
features="$(python3 scripts/demo_apps.py features "$col" mobile)"
echo "FEATURES mobile: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-mobile" || { echo "LIB mobile FAILED"; exit 3; }
echo "LIB mobile SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_build.py android --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" \
  || echo "demo_build.py returned non-zero: apps that did not build are skipped below"
ls -la demo-out/apps

for app in $apps; do
  apk="demo-out/apps/$app-$col.apk"
  [ -e "$apk" ] || { echo "no $apk: $app did not build"; continue; }
  AZ_QA_OUT="demo-out/qa-e46-$app" timeout 1500 bash scripts/ci/qa_android_flicker.sh "$apk"
  echo "SESSION e46 $app: exit $?"
done

if [ "${AZ_E46_CONTROL:-}" = 1 ]; then
  mkdir -p demo-out/floor
  if JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}" bash scripts/floor-apk/build.sh "$PWD/demo-out/floor/AzFloor-android.apk" >demo-out/floor/build.log 2>&1; then
    AZ_QA_OUT="demo-out/qa-e46-AzFloor" timeout 1500 bash scripts/ci/qa_android_flicker.sh "demo-out/floor/AzFloor-android.apk"
    echo "SESSION e46 control (the floor APK): exit $?"
  else
    echo "the floor APK did not build (demo-out/floor/build.log): no control"
  fi
fi
exit 0

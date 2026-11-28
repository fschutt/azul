#!/usr/bin/env bash
# qa_android_flicker.sh <dir of the APKs of an android_build_all.sh run | one APK>: the flicker and jitter audit on the Android emulator (scripts/ci/flicker_audit.py):
# idle redraws of the plain AzWidgets, flash frames in a screen recording (ffmpeg cuts it into frames), the pan jitter of AzWidgets (its e2e APK) and AzMaps.
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <build run id> -n remote-Linux -D apks && bash scripts/ci/qa_android_flicker.sh apks/demo-out/apps' --artifacts 'demo-out/qa-flicker/**'
set -uo pipefail
arg="${1:?directory of the APKs, or ONE APK (the flash audit only: the cold start and the resume of any app)}"
command -v ffmpeg >/dev/null 2>&1 || { sudo apt-get install -y ffmpeg >/dev/null 2>&1 || echo "ffmpeg could not be installed: the recording will not be cut into frames"; }
if [ -f "$arg" ]; then
  # one APK, any app: the starting window, the first frame and the resume do not depend on the app's page (E46 and E47 of the engine ledger)
  export AZ_QA_FLICKER_FLASH_ONLY=1
  bash "$(dirname "$0")/android_qa.sh" demo-out/qa-flicker "$arg" swiftshader_indirect "flicker" flicker
  exit $?
fi
dir="$arg"
export AZ_QA_FLICKER_E2E="$dir/AzWidgets-e2e-android-x86_64.apk"
[ -e "$dir/AzMaps-android-x86_64.apk" ] && export AZ_QA_FLICKER_MAPS="$dir/AzMaps-android-x86_64.apk"
bash "$(dirname "$0")/android_qa.sh" demo-out/qa-flicker "$dir/AzWidgets-android-x86_64.apk" swiftshader_indirect "flicker" flicker

#!/usr/bin/env bash
# qa_android_everything.sh <dir of the APKs of an android_build_all.sh run>: every Android QA of the AFTER build in ONE runner job (one slot, one
# queue wait), the most valuable first and each with a time limit, so that a job that is cut keeps what ran (every session writes its results
# as it goes):
#   1. matrix   every e2e scenario file against its host app (scripts/ci/qa_android_matrix.sh)         demo-out/qa-matrix
#   2. bars35   AzPaint's phone-fit audit on an API 35 emulator (the user's phone: the status bar over the first row)  demo-out/qa-bars35
#   3. flicker  idle, flash frames, pan jitter (scripts/ci/qa_android_flicker.sh)                        demo-out/qa-flicker
#   4. audit    the phone-fit audit of the apps (scripts/ci/qa_android_audit.sh)                         demo-out/qa-audit
#   5. all      the lifecycle of AzWidgets (cold start with the permission-dialog check, render modes cpu/gpu/auto, dark mode, rotation,
#               background and back, ...) and a first look at the other apps (scripts/ci/qa_android_all.sh), the swiftshader_indirect
#               GL mode only (the guest mode is the GPU work's own run)                                  demo-out/qa-all
#   6. startup  ten cold starts of the app and of the floor APK (scripts/ci/qa_android_startup.sh)       demo-out/qa-startup
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <build run id> -n remote-Linux -D apks && bash scripts/ci/qa_android_everything.sh apks/demo-out/apps' \
#       --artifacts 'demo-out/qa*/**'
# The last lines are `SESSION <name>: exit <n> after <s> s` (124 = the session's time limit ended it). The exit status of the script is 0.
set -uo pipefail
dir="${1:?directory of the APKs}"
here="$(cd "$(dirname "$0")" && pwd)"

session() {   # session <name> <seconds> <command...>
  local name="$1" limit="$2"
  shift 2
  echo "=== session $name (limit $limit s) $(date '+%H:%M:%S')"
  local start rc
  start=$(date +%s)
  timeout "$limit" "$@"
  rc=$?
  echo "SESSION $name: exit $rc after $(( $(date +%s) - start )) s"
  adb emu kill >/dev/null 2>&1 || true      # an emulator a cut session left behind would hold the next one's port
  sleep 5
}

session matrix 1200 bash "$here/qa_android_matrix.sh" "$dir"
# AzPaint on an API 35 emulator (Android 15, the user's phone: the status bar and the apps' first row): its phone-fit audit, light and dark
mkdir -p bars35 && cp "$dir/AzPaint-e2e-android-x86_64.apk" bars35/ 2>/dev/null
AZ_ANDROID_IMAGE="system-images;android-35;google_apis;x86_64" AZ_QA_OUT=demo-out/qa-bars35 session bars35 780 bash "$here/qa_android_audit.sh" bars35
session flicker 900 bash "$here/qa_android_flicker.sh" "$dir"
AZ_QA_OUT=demo-out/qa-audit session audit 1320 bash "$here/qa_android_audit.sh" "$dir"
AZ_QA_OUT=demo-out/qa-all AZ_QA_GLMODES=swiftshader_indirect session all 1500 bash "$here/qa_android_all.sh" "$dir"
AZ_QA_OUT=demo-out/qa-startup session startup 720 bash "$here/qa_android_startup.sh" "$dir"
exit 0

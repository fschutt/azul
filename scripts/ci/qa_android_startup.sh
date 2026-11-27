#!/usr/bin/env bash
# qa_android_startup.sh <apk>: the startup question on ONE emulator boot (swiftshader_indirect): ten cold starts of the app and of the floor APK
# (an empty Java Activity), each with the engine's own startup timeline on (`setprop debug.az.startup 1`, master from d35a2a4b6f on: the APK
# must be built from such a master), the logcat of the whole series kept in one file, the median start's `[startup]` table and one more cold
# start with `debug.az.profile cpu` for the layout passes' own tables, all in qa.md and logs/.
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <build run id> -n remote-Linux -D apks && bash scripts/ci/qa_android_startup.sh apks/demo-out/apps' --artifacts 'demo-out/qa/**'
set -uo pipefail
arg="${1:?an APK, or the directory of the APKs}"
if [ -d "$arg" ]; then apk="${arg%/}/AzWidgets-android-x86_64.apk"; else apk="$arg"; fi
export AZ_QA_SERIES=10
export AZ_FLOOR=1
export AZ_QA_STARTUP=1
export AZ_QA_ONLY=03d
bash "$(dirname "$0")/android_qa.sh" demo-out/qa "$apk" swiftshader_indirect AzWidgets qa

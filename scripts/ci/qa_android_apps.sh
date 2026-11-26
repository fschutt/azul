#!/usr/bin/env bash
# qa_android_apps.sh: a first look at every Az app APK on ONE emulator boot, in one short command for the remote runner.
# The primary APK gets the cold start step only; every other APK of quick/ gets the generic steps (cold start, rotate, home and
# resume, dark mode, font scale) with a folder of its own: demo-out/qa/<mode>/apps/<apk name>/.
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/qa_android_apps.sh' --artifacts 'demo-out/qa/**'
set -uo pipefail
export AZ_QA_ONLY=03
export AZ_QA_ONLY_EXTRA=03,08,09,12,13
export AZ_QA_TAPS="AzCompanion-android-x86_64=Try it without a Mac"
export AZ_QA_EXTRA_APKS="quick/AzMaps-android-x86_64.apk quick/AzMeet-android-x86_64.apk quick/AzPaint-android-x86_64.apk quick/AzWriter-android-x86_64.apk quick/AzCompanion-android-x86_64.apk"
bash "$(dirname "$0")/android_qa.sh" demo-out/qa quick/AzWidgets-android-x86_64.apk swiftshader_indirect AzWidgets qa

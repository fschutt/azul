#!/usr/bin/env bash
# qa_android_all.sh <primary mobile-e2e apk>: EVERYTHING of the Android QA in one runner job and one short command:
#   the lifecycle of the primary APK on two emulator GL modes (swiftshader_indirect, guest), the 10-start series against the floor
#   APK, the e2e session with the AzWidgets scenarios, and a first look at the other apps of quick/ (AzMaps, AzMeet, AzPaint,
#   AzWriter, AzCompanion, with the press of AzCompanion's 'Try it without a Mac') on the same boot.
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/qa_android_all.sh quick/<AzWidgets e2e apk>' --artifacts 'demo-out/qa/**'
set -uo pipefail
apk="${1:?primary apk}"
export AZ_QA_SERIES=10
export AZ_FLOOR=1
export AZ_QA_STARTUP=1   # the engine's own startup timeline of every start of the series (master from d35a2a4b6f on; harmless on an older build)
export AZ_QA_SCENARIOS="examples/azul-widgets/e2e/building_blocks.json examples/azul-widgets/e2e/every_input_form.json examples/azul-widgets/e2e/tab_looks.json"
export AZ_QA_TAPS="AzCompanion-android-x86_64=Try it without a Mac"
export AZ_QA_ONLY_EXTRA=03,08,09,12,13
export AZ_QA_EXTRA_APKS="quick/AzMaps-android-x86_64.apk quick/AzMeet-android-x86_64.apk quick/AzPaint-android-x86_64.apk quick/AzWriter-android-x86_64.apk quick/AzCompanion-android-x86_64.apk"
bash "$(dirname "$0")/android_qa.sh" demo-out/qa "$apk" swiftshader_indirect,guest AzWidgets-e2e both

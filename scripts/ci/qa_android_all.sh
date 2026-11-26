#!/usr/bin/env bash
# qa_android_all.sh <directory of the APKs | primary mobile-e2e apk>: EVERYTHING of the Android QA in one runner job and one short command:
#   the lifecycle of the primary APK on two emulator GL modes (swiftshader_indirect, guest), the 10-start series against the floor
#   APK, the e2e session with the AzWidgets scenarios, and a first look at the other apps (AzMaps, AzMeet, AzPaint, AzWriter,
#   AzCompanion, with the press of AzCompanion's 'Try it without a Mac') on the same boot.
# With a DIRECTORY the APKs are the ones scripts/ci/android_build_all.sh made (AzWidgets-e2e-android-x86_64.apk is the primary, the
# plain <App>-android-x86_64.apk files the first look): the job fetches them from the build run's artifact itself,
#   scripts/remote.sh run <ref> ubuntu-22.04 'gh run download <build run id> -n remote-Linux -D apks && bash scripts/ci/qa_android_all.sh apks/demo-out/apps' --artifacts 'demo-out/qa/**'
# With one APK the others are looked for in quick/ (the old way: APKs committed to a throwaway ref).
set -uo pipefail
arg="${1:?directory of the APKs, or the primary apk}"
if [ -d "$arg" ]; then
  dir="${arg%/}"; apk="$dir/AzWidgets-e2e-android-x86_64.apk"
else
  dir="quick"; apk="$arg"
fi
export AZ_QA_SERIES=10
export AZ_FLOOR=1
export AZ_QA_STARTUP=1   # the engine's own startup timeline of every start of the series (master from d35a2a4b6f on; harmless on an older build)
export AZ_QA_SCENARIOS="examples/azul-widgets/e2e/building_blocks.json examples/azul-widgets/e2e/every_input_form.json examples/azul-widgets/e2e/tab_looks.json"
export AZ_QA_TAPS="AzCompanion-android-x86_64=Try it without a Mac"
export AZ_QA_ONLY_EXTRA=03,08,09,12,13
export AZ_QA_EXTRA_APKS="$dir/AzMaps-android-x86_64.apk $dir/AzMeet-android-x86_64.apk $dir/AzPaint-android-x86_64.apk $dir/AzWriter-android-x86_64.apk $dir/AzCompanion-android-x86_64.apk"
bash "$(dirname "$0")/android_qa.sh" demo-out/qa "$apk" swiftshader_indirect,guest AzWidgets-e2e both

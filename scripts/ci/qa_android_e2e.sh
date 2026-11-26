#!/usr/bin/env bash
# qa_android_e2e.sh <apk>: the Android QA of a mobile-e2e APK in ONE short command for the remote runner:
# the lifecycle on two emulator GL modes (swiftshader_indirect, guest), ten timed cold starts of the app and of the floor
# APK (an empty Java Activity), and the e2e session with the AzWidgets scenarios of examples/azul-widgets/e2e.
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/qa_android_e2e.sh quick/<apk>' --artifacts 'demo-out/qa/**'
set -uo pipefail
apk="${1:?apk}"
export AZ_QA_SERIES=10
export AZ_FLOOR=1
export AZ_QA_SCENARIOS="examples/azul-widgets/e2e/building_blocks.json examples/azul-widgets/e2e/every_input_form.json examples/azul-widgets/e2e/tab_looks.json"
bash "$(dirname "$0")/android_qa.sh" demo-out/qa "$apk" swiftshader_indirect,guest AzWidgets-e2e both

#!/usr/bin/env bash
# android_landscape_probe.sh <build run id> <keeper APK run id>: ONE emulator job, NO build (about 15 minutes): does a scenario file's app come up LANDSCAPE when an app
# of ours is already in front and turned the display (the keeper), instead of starting from the launcher, which keeps the display portrait?
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/android_landscape_probe.sh 46089143227 46103694882' --artifacts 'demo-out/qa-probe/**'
# <build run id> is a run of android_build_all.sh (its artifact holds AzWidgets-e2e-android-x86_64.apk), <keeper APK run id> any run that kept AzMail-android-x86_64.apk (the
# demos agent's android_build_smoke.sh AzMail). The session is scripts/ci/android_qa.sh `tabs` with two self-mounting scenario files that take engine pictures
# (tests/e2e/css23_proof.json, tests/e2e/layout23_proof.json): the engine frames' sizes and the window each run was made with (`InitWindow physical=WxH` in the log)
# say whether the landscape pass was landscape. Printed at the end: those lines per pass.
set -uxo pipefail
build="${1:?the run id of an android_build_all.sh run}"
keeper_run="${2:?the run id of a run that kept AzMail-android-x86_64.apk}"
mkdir -p apks keeper-apk
gh run download "$build" -R "$GITHUB_REPOSITORY" -n remote-Linux -D apks || { echo "no artifact of run $build"; exit 3; }
gh run download "$keeper_run" -R "$GITHUB_REPOSITORY" -n remote-Linux -D keeper-apk || { echo "no artifact of run $keeper_run"; exit 3; }
apk="apks/demo-out/apps/AzWidgets-e2e-android-x86_64.apk"
keeper="keeper-apk/demo-out/apps/AzMail-android-x86_64.apk"
[ -e "$apk" ] || { echo "no $apk"; ls apks/demo-out/apps; exit 3; }
[ -e "$keeper" ] || { echo "no $keeper"; ls keeper-apk/demo-out/apps; exit 3; }
AZ_ANDROID_DEVICE=pixel_8 AZ_ANDROID_OVERLAYS="com.android.internal.emulation.pixel_8 com.android.systemui.emulation.pixel_8" \
  AZ_QA_KEEPER_APK="$keeper" AZ_QA_TABS_FILES="css23_proof layout23_proof" \
  timeout 2400 bash scripts/ci/android_qa.sh demo-out/qa-probe "$apk" swiftshader_indirect "landscape probe" tabs
echo "SESSION probe: exit $?"
echo "=== the window each run was made with"
for f in demo-out/qa-probe/swiftshader_indirect/tabs-*/matrix-*/raw/android/*.txt; do
  echo "$f: $(grep -m1 'InitWindow' "$f" | cut -c1-160)"
done
echo "=== the engine frames' sizes"
for f in demo-out/qa-probe/swiftshader_indirect/tabs-*/shots/*/*.png; do
  [ -e "$f" ] && echo "$f: $(python3 -c "import struct,sys; d=open(sys.argv[1],'rb').read(24); print(struct.unpack('>II', d[16:24]))" "$f")"
done
echo "=== the rotation log"
cat demo-out/qa-probe/swiftshader_indirect/tabs-landscape/rotation.log 2>/dev/null | cut -c1-260
exit 0

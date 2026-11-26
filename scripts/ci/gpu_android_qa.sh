#!/usr/bin/env bash
# gpu_android_qa.sh: the Android GPU matrix in ONE runner job: the QA variant of AzWidgets (the `mobile-e2e` libazul: debug server,
# e2e scripting and the test hooks, an ARTIFACT never shipped) built from this ref for x86_64, the size of its libazul.so, and the
# phone QA (scripts/ci/mobile_qa.py through scripts/ci/android_qa.sh) on both emulator GL modes, once per renderer.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/gpu_android_qa.sh' --codegen --artifacts 'demo-out/gpu/**
#   demo-out/libs/*/libazul.info.json'
#
# One run: lib ~10 min cold, app ~5, then per renderer and per emulator mode a boot (~30 s) and the steps below. What each step shows
# is in scripts/ci/mobile_qa.py. The renderer's own folder is demo-out/gpu/<emulator-mode>-<renderer>/ (qa.md, qa.json, shots/, logs/).
#   AZ_GPU_QA_BACKENDS  the renderers to run, default "gpu cpu" (gpu first: its failure is what the run is for)
#   AZ_GPU_QA_MODES     the emulator GL modes, default "swiftshader_indirect,guest"
#   AZ_GPU_QA_STEPS     the QA steps (besides 00-02), default 03,03b,03c,05,08,09,10,11,14,15,16,17
set -uo pipefail

target=x86_64-linux-android
col=android-x86_64
variant=mobile-e2e

if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
  python3 -m pip install --quiet --user tomli || true   # the runner's Python 3.10 has no tomllib
fi
export ANDROID_HOME="${ANDROID_HOME:-/usr/local/lib/android/sdk}"
SDKM="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"
yes | "$SDKM" --licenses >/dev/null 2>&1 || true
"$SDKM" --install "ndk;27.0.12077973" "build-tools;34.0.0" "platforms;android-34" >/dev/null
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
export JAVA_HOME="${JAVA_HOME_17_X64:-${JAVA_HOME:-}}"
rustup target add "$target"

mkdir -p demo-out/libs demo-out/apps demo-out/results demo-out/gpu
features="$(python3 scripts/demo_apps.py features "$col" "$variant")"
echo "== $variant: $features"
start=$(date +%s)
bash scripts/demo_lib.sh android "$target" "$features" "demo-out/libs/demolib-$col-$variant"
status=$?
echo "LIB $variant exit $status SECONDS: $(( $(date +%s) - start ))"
[ "$status" -eq 0 ] || exit "$status"
ls -l demo-out/libs/*/libazul.so
cat demo-out/libs/*/libazul.info.json

python3 scripts/demo_build.py android --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant "$variant"
status=$?
echo "APP exit $status"
ls -l demo-out/apps
apk="$(ls demo-out/apps/AzWidgets-e2e-android-x86_64.apk 2>/dev/null | head -n 1)"
[ -n "$apk" ] || { echo "no APK was built"; exit 1; }

modes="${AZ_GPU_QA_MODES:-swiftshader_indirect,guest}"
steps="${AZ_GPU_QA_STEPS:-03,03b,03c,05,08,09,10,11,14,15,16,17}"
overall=0
for backend in ${AZ_GPU_QA_BACKENDS:-gpu cpu}; do
  echo "================ renderer: $backend ================"
  if [ "$backend" = gpu ]; then expect_gpu=1; else expect_gpu=""; fi
  AZ_QA_BACKEND="$backend" AZ_QA_EXPECT_GPU="$expect_gpu" AZ_QA_EXPECT_HOOKS=1 AZ_QA_ONLY="$steps" \
    bash scripts/ci/android_qa.sh demo-out/gpu "$apk" "$modes" "AzWidgets-e2e" qa
  status=$?
  echo "renderer $backend: android_qa.sh exit $status"
  if [ "$status" -ne 0 ]; then overall=$status; fi
done
echo "gpu android qa: overall exit $overall"
exit "$overall"

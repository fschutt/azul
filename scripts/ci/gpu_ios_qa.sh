#!/usr/bin/env bash
# gpu_ios_qa.sh: the iOS GPU matrix in ONE macOS job (the GPU renderer on the Simulator's OpenGL ES, which is Apple's software renderer):
#
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/gpu_ios_qa.sh' --codegen --artifacts 'demo-out/results/*
#   demo-out/libs/*/libazul.info.json
#   demo-out/qa*/**'
#
# Builds the mobile-e2e libazul (the debug server and the test hooks of scripts/demo_apps.toml) and AzWidgets once, as
# scripts/ci/qa_ios_build.sh does, then runs scripts/ci/ios_qa.sh three times on it, the most valuable first (the job has 120 minutes
# and every qa.json is written after each step, so a job that is cut keeps what ran):
#   1. qa-modes     AZ_BACKEND=cpu / gpu / auto (which renderer ran, first frame, the frame times of a scripted swipe, the idle window,
#                   memory, the cpu picture against the gpu picture pixel by pixel) and AZ_GPU_FAIL for every stage of the bring-up;
#   2. qa-gpu       the lifecycle with the app on the GPU (dark mode, background 10 s with no GL command, rotation and memory
#                   warning through the Simulator's menu (AZ_QA_GUI=1: the runner may refuse osascript, the step says so),
#                   a second cold start, the whole log);
#   3. qa-gpu-e2e   the AzWidgets scenario files through the debug server with the app on the GPU.
# The verdicts are in <dir>/qa.json (a failing app is a RESULT of this job); this script prints them at the end. Exit 0 when the
# library and the app were built, 3 when not.
set -uo pipefail

target=aarch64-apple-ios-sim
mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt
features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile-e2e ios-sim)"
echo "FEATURES: $features"
start=$(date +%s)
bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-mobile-e2e" || { echo "LIB FAILED"; exit 3; }
echo "LIB SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
python3 scripts/demo_build.py ios --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results \
  --variant mobile-e2e || { echo "APP FAILED"; exit 3; }
echo "APP SECONDS: $(( $(date +%s) - start ))"
zip="$(ls demo-out/apps/*.zip | head -n 1)"
ls -la "$zip" demo-out/libs/*/libazul.info.json 2>/dev/null
echo "QA app: $zip"

echo "=== 1. render modes and failure injection"
AZ_QA_EXPECT_GPU=1 AZ_QA_HOOKS=1 AZ_QA_ONLY=03,04 bash scripts/ci/ios_qa.sh demo-out/qa-modes "$zip" AzWidgets-modes qa
echo "=== 2. the lifecycle on the GPU"
AZ_QA_GUI=1 AZ_QA_BACKEND=gpu AZ_QA_EXPECT_GPU=1 AZ_QA_HOOKS=1 AZ_QA_ONLY=05,07,08,09,10,11 bash scripts/ci/ios_qa.sh demo-out/qa-gpu "$zip" AzWidgets-gpu qa
echo "=== 3. the e2e scenarios on the GPU"
SIMCTL_CHILD_AZ_BACKEND=gpu AZ_QA_E2E_ARGS="--first-frame-timeout 240 --server-timeout 300" \
  AZ_QA_SCENARIOS="examples/azul-widgets/e2e/building_blocks.json examples/azul-widgets/e2e/every_input_form.json examples/azul-widgets/e2e/tab_looks.json" \
  bash scripts/ci/ios_qa.sh demo-out/qa-gpu-e2e "$zip" AzWidgets-gpu-e2e e2e

echo "=== verdicts"
python3 - <<'PY'
import json, pathlib
for qa in sorted(pathlib.Path("demo-out").glob("qa*/qa.json")) + sorted(pathlib.Path("demo-out").glob("qa*/e2e/qa.json")):
    doc = json.loads(qa.read_text())
    print(f"--- {qa.parent}: {'PASS' if doc.get('ok') else 'FAIL'}  {doc.get('meta', {})}")
    for step in doc.get("steps", []):
        bad = [c for c in step.get("checks", []) if not c["ok"] and c["required"]]
        print(f"    {'PASS' if step['ok'] else 'FAIL'} {step['name']} ({step['seconds']} s)" + "".join(f"\n        - {c['what']}: {c['detail']}"[:300] for c in bad))
PY
exit 0

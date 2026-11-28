#!/usr/bin/env bash
# qa_ios_all.sh: ONE macOS job: the six phone apps on the iOS Simulator, each with the CPU renderer and with the GPU path (pictures), and the
# life of AzWidgets with the debug server. Most valuable first, so a job that is cut keeps what ran (every qa.json is written after each step).
#
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/qa_ios_all.sh' --codegen --artifacts 'demo-out/results/*
#   demo-out/plan.txt
#   demo-out/libs/*/libazul.info.json
#   demo-out/qa*/**/qa.md
#   demo-out/qa*/**/qa.json
#   demo-out/qa*/**/*.log
#   demo-out/qa*/**/shots/*
#   demo-out/qa*/**/console-*
#   demo-out/qa*/**/log-app.txt
#   demo-out/qa*/e2e-*.json
#   demo-out/qa*/raw/**
#   demo-out/qa*/**/DiagnosticReports-ls.txt'
#
#   1. the plain phone libraries (`mobile`, and `mobile-iroh` for AzCompanion) and the six apps (AzWidgets AzMaps AzPaint AzMeet AzWriter
#      AzCompanion) of the demos pipeline's iOS Simulator column;
#   2. per app (demo-out/qa-apps/<App>/): scripts/ci/ios_qa.sh with the steps 03 (cold start), 04 (AZ_BACKEND cpu, gpu, auto: which renderer
#      ran, a picture each), 05 (the system's dark appearance), 08 (rotation to landscape through the Simulator's menu);
#   3. the `mobile-e2e` library and the e2e apps AzWidgets and AzWriter: every e2e scenario file against its host app, the table of
#      scripts/ci/e2e_matrix.py (demo-out/qa-matrix), then the whole lifecycle of AzWidgets-e2e (demo-out/qa) and its debug-server session with
#      the three AzWidgets scenario files (demo-out/qa/e2e).
# The GPU matrix of AzWidgets (failure injection at every stage, the lifecycle on the GPU, the scenarios on the GPU) is scripts/ci/gpu_ios_qa.sh.
# Exit 0 when the first stage built, 3 when not.
set -uo pipefail

target=aarch64-apple-ios-sim
mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt

build_lib() {   # build_lib <variant>: demo-out/libs/demolib-ios-sim-<variant>
  local variant="$1" features start
  features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios "$variant" ios-sim)" || return 1
  echo "FEATURES $variant: $features"
  start=$(date +%s)
  bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-$variant" || return 1
  echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
}

echo "=== 1. libraries and the six apps"
build_lib mobile || { echo "LIB mobile FAILED"; exit 3; }
iroh=1
build_lib mobile-iroh || { echo "LIB mobile-iroh FAILED: AzCompanion is skipped"; iroh=0; }
apps="AzWidgets AzMaps AzPaint AzMeet AzWriter"
[ "$iroh" = 1 ] && apps="$apps AzCompanion"
start=$(date +%s)
python3 scripts/demo_build.py ios --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" \
  || echo "demo_build.py ios returned non-zero: apps that did not build are skipped below"
echo "APPS SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps 2>/dev/null

# a gpu request that ran the CPU renderer FAILS only when this tree has an iOS GPU path at all
expect_gpu=""
grep -rq "iOS GPU" dll/src/desktop/shell2/ios 2>/dev/null && expect_gpu=1
echo "the tree has an iOS GPU path: ${expect_gpu:-no}"

echo "=== 2. the apps, CPU renderer and GPU path"
for app in $apps; do
  zip="$(ls demo-out/apps/"$app"-ios-sim*.zip 2>/dev/null | grep -v -- '-e2e-' | head -n 1)"
  if [ -z "$zip" ]; then echo "no iOS Simulator app for $app (see demo-out/results)"; continue; fi
  echo "--- $app: $zip"
  AZ_QA_GUI=1 AZ_QA_EXPECT_GPU="$expect_gpu" AZ_QA_ONLY=03,04,05,08 bash scripts/ci/ios_qa.sh "demo-out/qa-apps/$app" "$zip" "$app" qa
done

echo "=== 3. the e2e library and apps: the e2e matrix, then the life of AzWidgets with the debug server"
if build_lib mobile-e2e; then
  python3 scripts/demo_build.py ios --target "$target" --apps "AzWidgets AzWriter" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e \
    || echo "an e2e app did not build"
  e2e_zip="$(ls demo-out/apps/AzWidgets-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
  writer_zip="$(ls demo-out/apps/AzWriter-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
  if [ -n "$e2e_zip" ]; then
    echo "--- 3a. every e2e scenario file against its host app (scripts/ci/e2e_matrix.py)"
    AZ_QA_MATRIX_APPS="$e2e_zip $writer_zip" bash scripts/ci/ios_qa.sh demo-out/qa-matrix "$e2e_zip" e2e-matrix matrix
    echo "--- 3b. the life of AzWidgets-e2e: the lifecycle and the debug-server session"
    AZ_QA_GUI=1 AZ_QA_EXPECT_GPU="$expect_gpu" AZ_QA_ONLY=03,04,05,06,07,08,09,10,11 \
      AZ_QA_SCENARIOS="examples/azul-widgets/e2e/building_blocks.json examples/azul-widgets/e2e/every_input_form.json examples/azul-widgets/e2e/tab_looks.json" \
      bash scripts/ci/ios_qa.sh demo-out/qa "$e2e_zip" AzWidgets-e2e both
  fi
else
  echo "LIB mobile-e2e FAILED"
fi

echo "=== verdicts"
python3 - <<'PY'
import json, pathlib
for qa in sorted(pathlib.Path("demo-out").glob("qa*/**/qa.json")):
    doc = json.loads(qa.read_text())
    print(f"--- {qa.parent}: {'PASS' if doc.get('ok') else 'FAIL'}  {doc.get('meta', {}).get('render_modes', '')}")
    for step in doc.get("steps", []):
        bad = [c for c in step.get("checks", []) if not c["ok"] and c["required"]]
        print(f"    {'PASS' if step['ok'] else 'FAIL'} {step['name']} ({step['seconds']} s)" + "".join(f"\n        - {c['what']}: {c['detail']}"[:300] for c in bad))
PY
exit 0

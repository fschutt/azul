#!/usr/bin/env bash
# qa_ios_flicker.sh: ONE macOS run: the plain phone library, AzWidgets against it, and the flicker audit of that app on the iOS Simulator with the
# CPU renderer and then with the GPU path (scripts/ci/ios_qa.sh session `flicker`: idle pictures, flash frames of a recording).
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/qa_ios_flicker.sh' --codegen --artifacts 'demo-out/qa-flicker*/**'
# AZ_QA_GUI=1 turns the Simulator's menu on for the rotation (the runner may refuse the accessibility permission: the report says so).
set -uo pipefail
target=aarch64-apple-ios-sim
rustup target add "$target"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt
features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile ios-sim)" || exit 3
echo "FEATURES: $features"
bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-mobile" || { echo "LIB FAILED"; exit 3; }
python3 scripts/demo_build.py ios --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" || echo "the app did not build"
zip="$(ls demo-out/apps/AzWidgets-ios-sim*.zip 2>/dev/null | grep -v -- '-e2e-' | head -n 1)"
[ -n "$zip" ] || { echo "no AzWidgets app"; exit 3; }
for backend in cpu gpu; do
  echo "=== flicker audit, $backend renderer"
  AZ_QA_GUI=1 AZ_QA_BACKEND="$backend" bash scripts/ci/ios_qa.sh "demo-out/qa-flicker-$backend" "$zip" AzWidgets flicker
done
exit 0

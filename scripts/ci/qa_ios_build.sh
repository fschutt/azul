#!/usr/bin/env bash
# qa_ios_build.sh: ONE macOS run that builds the iOS Simulator QA app of the tree it runs in and tests it:
#   1. the mobile-e2e libazul of aarch64-apple-ios-sim (the lib variant of scripts/demo_apps.toml with the engine's debug server),
#   2. AzWidgets linked against it (the same recipe as the demos workflow: scripts/demo_build.py ios --variant mobile-e2e),
#   3. scripts/ci/ios_qa.sh on that app: the lifecycle with simctl (qa) and the debug-server session (e2e), on the newest iOS runtime.
# It is a build + link + launch gate of the tree at the same time: a failing step says which.
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/qa_ios_build.sh' --codegen --artifacts 'demo-out/results/*
#   demo-out/plan.txt
#   demo-out/qa/qa.*
#   demo-out/qa/*.log
#   demo-out/qa/shots/*
#   demo-out/qa/logs/console-*
#   demo-out/qa/logs/log-app.txt
#   demo-out/qa/logs/DiagnosticReports-ls.txt
#   demo-out/qa/e2e/**'
# (not the libazul archive of ~140 MB, not the app, not logs/log-all.txt of the whole Simulator log (200-370 MB): what a person reads.)
set -euo pipefail
target=aarch64-apple-ios-sim
rustup target add "$target"
mkdir -p demo-out/libs demo-out/apps demo-out/results
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt
features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile-e2e ios-sim)"
echo "FEATURES: $features"
start=$(date +%s)
bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-mobile-e2e"
echo "LIB SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
python3 scripts/demo_build.py ios --target "$target" --apps AzWidgets --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e
echo "APP SECONDS: $(( $(date +%s) - start ))"
ls -la demo-out/apps demo-out/results
cat demo-out/results/*.json
zip="$(ls demo-out/apps/*.zip | head -n 1)"
echo "QA app: $zip"
set +e
export AZ_QA_SCENARIOS="examples/azul-widgets/e2e/building_blocks.json examples/azul-widgets/e2e/every_input_form.json examples/azul-widgets/e2e/tab_looks.json"
bash scripts/ci/ios_qa.sh demo-out/qa "$zip" AzWidgets-e2e both
exit 0

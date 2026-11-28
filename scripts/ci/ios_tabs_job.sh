#!/usr/bin/env bash
# ios_tabs_job.sh: ONE macOS job: the tab strip on the iOS Simulator (examples/azul-widgets/e2e/tab_touch*.json) on an iPhone 16 Pro (the Dynamic
# Island) and on an iPhone SE (no cutout, the home button's size), each portrait and then turned to landscape. It builds the QA variant of AzWidgets
# (the `mobile-e2e` phone library) from the tree it runs in and runs scripts/ci/ios_qa.sh's `tabs` session per device: the engine's frames of every
# file, the device's pictures of a plain start (light, dark) and a recording of each run (demo-out/qa-tabs/<device>/tabs/<orientation>/).
#
#   scripts/remote.sh run <full sha | ref> macos-15 'bash scripts/ci/ios_tabs_job.sh' --codegen --artifacts 'demo-out/qa-tabs/**
#   demo-out/results/*
#   demo-out/plan.txt'
set -uxo pipefail
target=aarch64-apple-ios-sim
mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt
features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios mobile-e2e ios-sim)" || { echo "no features for mobile-e2e"; exit 3; }
echo "FEATURES mobile-e2e: $features"
start=$(date +%s)
bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-mobile-e2e" || { echo "LIB mobile-e2e FAILED"; exit 3; }
echo "LIB mobile-e2e SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_build.py ios --target "$target" --apps "AzWidgets" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e \
  || echo "demo_build.py returned non-zero"
ls -la demo-out/apps
e2e_zip="$(ls demo-out/apps/AzWidgets-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
[ -n "$e2e_zip" ] || { echo "no AzWidgets e2e Simulator app"; exit 3; }
# the two phones; a device the runner's Xcode does not have fails its own session (a line says so) and the next one runs
for dev in "iPhone 16 Pro" "iPhone SE (3rd generation)"; do
  tag="$(printf '%s' "$dev" | tr -c 'A-Za-z0-9' '-')"
  AZ_QA_GUI=1 AZ_QA_IOS_DEVICE="$dev" timeout 2400 bash scripts/ci/ios_qa.sh "demo-out/qa-tabs/$tag" "$e2e_zip" "tabs $dev" tabs
  echo "SESSION tabs $dev: exit $?"
done
exit 0

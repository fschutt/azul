#!/usr/bin/env bash
# ios_tabs_job.sh: ONE macOS job: the tab strip on the iOS Simulator (examples/azul-widgets/e2e/tab_touch*.json) on an iPhone 16 Pro (the Dynamic
# Island) and on an iPhone SE (no cutout, the home button's size), each portrait and then turned to landscape. It builds the QA variant of AzWidgets
# (the `mobile-e2e` phone library) from the tree it runs in and runs scripts/ci/ios_qa.sh's `tabs` session per device: the engine's frames of every
# file, the device's pictures of a plain start (light, dark) and a recording of each run (demo-out/qa-tabs/<device>/tabs/<orientation>/).
#
#   scripts/remote.sh run <full sha | ref> macos-15 'bash scripts/ci/ios_tabs_job.sh' --codegen --artifacts 'demo-out/qa-tabs/**
#   demo-out/results/*
#   demo-out/plan.txt'
# AZ_TABS_FLICKER=1 adds, AFTER the two phones, the flicker and jitter audit of the plain app (scripts/ci/qa_ios_flicker.sh: its own library build, the
# CPU renderer and the GPU path; add 'demo-out/qa-flicker*/**' and 'demo-out/qa-startup/**' to the artifacts): the audit of the iOS Simulator that never ran before the app was
# installed by it (flicker_audit.py, 7c4e132056).
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
python3 scripts/demo_build.py ios --target "$target" --apps "AzWidgets AzWriter" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e \
  || echo "demo_build.py returned non-zero"
ls -la demo-out/apps
e2e_zip="$(ls demo-out/apps/AzWidgets-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
[ -n "$e2e_zip" ] || { echo "no AzWidgets e2e Simulator app"; exit 3; }
# the AzWriter ribbon: its QA build gets a first look on the same phones
writer_zip="$(ls demo-out/apps/AzWriter-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
[ -n "$writer_zip" ] && export AZ_QA_TABS_EXTRA_APPS="$writer_zip"
# the two phones; a device the runner's Xcode does not have fails its own session (a line says so) and the next one runs
for dev in "iPhone 16 Pro" "iPhone SE (3rd generation)"; do
  tag="$(printf '%s' "$dev" | tr -c 'A-Za-z0-9' '-')"
  AZ_QA_GUI=1 AZ_QA_IOS_DEVICE="$dev" timeout 2400 bash scripts/ci/ios_qa.sh "demo-out/qa-tabs/$tag" "$e2e_zip" "tabs $dev" tabs
  echo "SESSION tabs $dev: exit $?"
done
if [ "${AZ_TABS_FLICKER:-}" = 1 ]; then
  timeout 3000 bash scripts/ci/qa_ios_flicker.sh
  echo "SESSION flicker: exit $?"
  # the cold starts with the startup timeline on (first_present, the layout passes, the font cache rows) of the same plain app
  plain_zip="$(ls demo-out/apps/AzWidgets-ios-sim*.zip 2>/dev/null | grep -v -- '-e2e-' | head -n 1)"
  if [ -n "$plain_zip" ]; then
    timeout 900 bash scripts/ci/ios_qa.sh demo-out/qa-startup "$plain_zip" AzWidgets startup
    echo "SESSION startup: exit $?"
  fi
fi
exit 0

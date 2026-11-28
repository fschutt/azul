#!/usr/bin/env bash
# ios_overlays_job.sh: ONE macOS job: the popups, dialogs and menus of a window with ONE surface on the iOS Simulator (an iPhone 16 Pro), portrait and
# then turned to landscape: the scenario examples/azul-widgets/e2e/overlays_phone.json against the QA variant of AzWidgets (the `mobile-e2e` phone
# library, built here from the tree the job runs in), the engine's frames of it, the device's pictures of a plain start and a recording of the run
# (demo-out/qa-tabs/<device>/tabs/<orientation>/). It is also the compile of the iOS shell on a real toolchain.
#
#   scripts/remote.sh run <full sha | ref> macos-15 'bash scripts/ci/ios_overlays_job.sh' --codegen --artifacts 'demo-out/qa-tabs/**
#   demo-out/results/*
#   demo-out/plan.txt'
set -uxo pipefail
# a macOS runner has no GNU `timeout` (the first run of this job: "timeout: command not found", exit 127, no session ran): a time limit in plain bash
limit() {   # limit <seconds> <command...>: the command's exit status, 124 when it was stopped at the limit
  local secs="$1"; shift
  "$@" &
  local pid=$!
  ( sleep "$secs"; kill -TERM "$pid" 2>/dev/null; sleep 5; kill -KILL "$pid" 2>/dev/null ) >/dev/null 2>&1 &
  local watcher=$!
  wait "$pid" 2>/dev/null
  local rc=$?
  kill "$watcher" 2>/dev/null; wait "$watcher" 2>/dev/null
  # stopped by the watcher's TERM or KILL: 124, as GNU timeout says
  if [ "$rc" -eq 143 ] || [ "$rc" -eq 137 ]; then rc=124; fi
  return "$rc"
}
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
# the phone: an iPhone 16 Pro (the Dynamic Island), portrait and then turned to landscape; a device the runner's Xcode does not have fails its own
# session (a line says so)
for dev in "iPhone 16 Pro"; do
  tag="$(printf '%s' "$dev" | tr -c 'A-Za-z0-9' '-')"
  AZ_QA_TABS_FILES="${AZ_QA_TABS_FILES:-overlays_phone}" AZ_QA_GUI=1 AZ_QA_IOS_DEVICE="$dev" limit 2400 bash scripts/ci/ios_qa.sh "demo-out/qa-tabs/$tag" "$e2e_zip" "overlays $dev" tabs
  echo "SESSION overlays $dev: exit $?"
done
exit 0

#!/usr/bin/env bash
# wp_e2_ios.sh: ONE macOS job: the iOS Simulator pictures of the apps of WP-E2 on the two phones of the brief (an iPhone 16 Pro with the Dynamic
# Island, an iPhone SE with the home button's size): the plain phone library (`mobile`) and the apps against it (scripts/demo_build.py ios), then per
# device scripts/ci/ios_qa.sh's `qa` session: cold start, the CPU renderer, the GPU path, the system's dark appearance, rotation to landscape.
# With WP_E2_IOS_E2E=1 the QA variant (`mobile-e2e`: the debug server and the scenario executor) is built too and the scenario files named in
# WP_E2_SCENARIOS (space separated, repo paths) run on each device through scripts/ci/ios_qa.sh's `e2e` session; their pictures are the engine's frames
# (AZ_E2E_SHOT_DIR) and the device's.
#
#   scripts/remote.sh run <full sha> macos-15 'bash scripts/ci/wp_e2_ios.sh' --codegen --artifacts 'demo-out/wp-e2-ios/**
#   demo-out/results/*
#   demo-out/plan.txt'
#
# WP_E2_IOS_APPS: the apps (default AzMaps). WP_E2_IOS_DEVICES: device types separated by `|` (default the two phones above).
set -uxo pipefail
# a macOS runner has no GNU `timeout`: a time limit in plain bash (124 when the command was stopped at the limit)
limit() {
  local secs="$1"; shift
  "$@" &
  local pid=$!
  ( sleep "$secs"; kill -TERM "$pid" 2>/dev/null; sleep 5; kill -KILL "$pid" 2>/dev/null ) >/dev/null 2>&1 &
  local watcher=$!
  wait "$pid" 2>/dev/null
  local rc=$?
  kill "$watcher" 2>/dev/null; wait "$watcher" 2>/dev/null
  if [ "$rc" -eq 143 ] || [ "$rc" -eq 137 ]; then rc=124; fi
  return "$rc"
}
apps="${WP_E2_IOS_APPS:-AzMaps}"
devices="${WP_E2_IOS_DEVICES:-iPhone 16 Pro|iPhone SE (3rd generation)}"
target=aarch64-apple-ios-sim
mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt

build_lib() {   # build_lib <variant>
  local variant="$1" features start
  features="$(python3 scripts/ci/plan_lib_features.py demo-out/plan.txt libs_ios "$variant" ios-sim)" || return 1
  echo "FEATURES $variant: $features"
  start=$(date +%s)
  bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-ios-sim-$variant" || return 1
  echo "LIB $variant SECONDS: $(( $(date +%s) - start ))"
}

build_lib mobile || { echo "LIB mobile FAILED"; exit 3; }
python3 scripts/demo_build.py ios --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant "" \
  || echo "demo_build.py returned non-zero"
ls -la demo-out/apps

if [ "${WP_E2_IOS_E2E:-}" = 1 ]; then
  build_lib mobile-e2e || { echo "LIB mobile-e2e FAILED"; }
  python3 scripts/demo_build.py ios --target "$target" --apps "$apps" --libs demo-out/libs --out demo-out/apps --results demo-out/results --variant mobile-e2e \
    || echo "demo_build.py (e2e) returned non-zero"
  ls -la demo-out/apps
fi

IFS='|' read -r -a device_list <<<"$devices"
for app in $apps; do
  zip="$(ls demo-out/apps/"$app"-ios-sim*.zip 2>/dev/null | grep -v -- '-e2e-' | head -n 1)"
  [ -n "$zip" ] || { echo "no Simulator app of $app"; continue; }
  for dev in "${device_list[@]}"; do
    tag="$(printf '%s' "$dev" | tr -c 'A-Za-z0-9' '-')"
    AZ_QA_GUI=1 AZ_QA_IOS_DEVICE="$dev" limit 1500 bash scripts/ci/ios_qa.sh "demo-out/wp-e2-ios/$app/$tag" "$zip" "$app $dev" qa
    echo "SESSION qa $app $dev: exit $?"
  done
  if [ "${WP_E2_IOS_E2E:-}" = 1 ] && [ -n "${WP_E2_SCENARIOS:-}" ]; then
    ezip="$(ls demo-out/apps/"$app"-e2e-ios-sim*.zip 2>/dev/null | head -n 1)"
    [ -n "$ezip" ] || { echo "no e2e Simulator app of $app"; continue; }
    for dev in "${device_list[@]}"; do
      tag="$(printf '%s' "$dev" | tr -c 'A-Za-z0-9' '-')"
      AZ_QA_GUI=1 AZ_QA_IOS_DEVICE="$dev" AZ_QA_SCENARIOS="$WP_E2_SCENARIOS" limit 2400 bash scripts/ci/ios_qa.sh "demo-out/wp-e2-ios/$app-e2e/$tag" "$ezip" "$app $dev" e2e
      echo "SESSION e2e $app $dev: exit $?"
    done
  fi
done
exit 0

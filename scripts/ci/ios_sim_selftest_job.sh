#!/usr/bin/env bash
# ios_sim_selftest_job.sh: the ONE macOS job of the iOS Simulator self-test: build the QA library and the AzSelfTest app for
# the Simulator, then run scripts/ci/ios_sim_selftest.py on an iPhone with a Dynamic Island and on an iPhone SE.
#
#   bash scripts/ci/ios_sim_selftest_job.sh [<App>]          (default AzSelfTest)
#
# On a runner, from any ref that has the app in the manifest (`qa_only = true`) and this file:
#   scripts/remote.sh run <ref> macos-15 'bash scripts/ci/ios_sim_selftest_job.sh' --codegen --artifacts 'demo-out/ios-sim-selftest-*/**
#   demo-out/results/*
#   demo-out/libs/*/libazul.info.json'
#
# The build is what scripts/ci/ios_build_qa.sh does for the `mobile-e2e` variant, without its boot test and lifecycle QA
# (those are QA's own runs; this job is about the rows of the self-test): the library of the app's e2e_lib for
# aarch64-apple-ios-sim, the .app linked against it. ~15 to 25 minutes cold; the two Simulator runs ~5 minutes each.
#
# The exit code is the sum of what failed: the build, and a run with a finding. Every result lands in demo-out/ios-sim-selftest-*/
# (console, report and screenshot of each run: fresh install, location granted, location revoked, per device).
set -uo pipefail

app="${1:-AzSelfTest}"
target=aarch64-apple-ios-sim
col=ios-sim
status=0

if ! python3 scripts/demo_apps.py qa-only "$app"; then
  echo "$app is not a qa_only app of the manifest: this job builds only the QA library variant of one" >&2
  exit 2
fi

mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
python3 scripts/demo_apps.py plan --e2e > demo-out/plan.txt

lib="$(python3 scripts/demo_apps.py e2e-lib "$app")"
features="$(python3 scripts/demo_apps.py features "$col" "$lib")"
echo "== build $app on $lib: $features"
start=$(date +%s)
bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-$col-$lib" || { echo "BUILD_EXIT=lib"; exit 1; }
echo "LIB $lib SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
python3 scripts/demo_build.py ios --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant "$lib" || { echo "BUILD_EXIT=app"; exit 1; }
echo "APP SECONDS: $(( $(date +%s) - start ))"

zip="demo-out/apps/$app-e2e-$col.app.zip"
ls -la "$zip" || { echo "BUILD_EXIT=nozip"; exit 1; }
mkdir -p demo-out/selftest-app
unzip -q -o "$zip" -d demo-out/selftest-app
bundle="$(find demo-out/selftest-app -maxdepth 2 -name '*.app' -type d | head -n 1)"
[ -n "$bundle" ] || { echo "the zip holds no .app"; exit 1; }
echo "== the app: $bundle"

xcrun simctl list devicetypes | grep -i "iphone" | head -30 || true
xcrun simctl list runtimes | head -10 || true

for device in "iPhone 16 Pro" "iPhone SE (3rd generation)"; do
  slug="$(printf '%s' "$device" | tr -c 'A-Za-z0-9' '-' | sed 's/-*$//')"
  echo "== self-test on $device"
  python3 scripts/ci/ios_sim_selftest.py --app "$bundle" --device "$device" --out "demo-out/ios-sim-selftest-$slug" || status=$((status + 1))
done

echo "SELFTEST_STATUS=$status"
exit "$status"

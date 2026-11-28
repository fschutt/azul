#!/usr/bin/env bash
# ios_files_job.sh: the iOS Simulator proof of the share / open-with work (ledger WP-F) in ONE macOS job: the QA library and the
# AzSelfTest app built for the Simulator from THIS tree (what scripts/ci/ios_sim_selftest_job.sh builds), then
# scripts/ci/ios_files_proof.py drives the rows and the links with pictures.
#
#   bash scripts/ci/ios_files_job.sh
#
# On a runner (ONE of the two jobs a work package may have at once):
#   scripts/remote.sh run <ref> macos-15 'bash scripts/ci/ios_files_job.sh' --codegen --artifacts 'demo-out/ios-files/**
#   demo-out/results/*
#   demo-out/ios-files/pictures/*'
# ~15 to 25 minutes cold (the library), the proof ~5 minutes. The exit code is the proof's (or 1: the build).
set -uo pipefail
app=AzSelfTest
target=aarch64-apple-ios-sim
col=ios-sim

if ! python3 scripts/demo_apps.py qa-only "$app"; then
  echo "$app is not a qa_only app of the manifest" >&2
  exit 2
fi
mkdir -p demo-out/libs demo-out/apps demo-out/results
rustup target add "$target"
lib="$(python3 scripts/demo_apps.py e2e-lib "$app")"
features="$(python3 scripts/demo_apps.py features "$col" "$lib")"
echo "== build $app on $lib: $features"
bash scripts/demo_lib.sh ios "$target" "$features" "demo-out/libs/demolib-$col-$lib" || { echo "BUILD_EXIT=lib"; exit 1; }
python3 scripts/demo_build.py ios --target "$target" --apps "$app" --libs demo-out/libs --out demo-out/apps \
  --results demo-out/results --variant "$lib" || { echo "BUILD_EXIT=app"; exit 1; }
zip="demo-out/apps/$app-e2e-$col.app.zip"
ls -la "$zip" || { echo "BUILD_EXIT=nozip"; exit 1; }
mkdir -p demo-out/files-app
unzip -q -o "$zip" -d demo-out/files-app
bundle="$(find demo-out/files-app -maxdepth 2 -name '*.app' -type d | head -n 1)"
[ -n "$bundle" ] || { echo "the zip holds no .app"; exit 1; }
echo "== the app: $bundle"
python3 scripts/ci/ios_files_proof.py --app "$bundle" --device "iPhone 16 Pro" --out demo-out/ios-files
code=$?
echo "ios files proof exit: $code"
cat demo-out/ios-files/files-proof.md 2>/dev/null | head -80 || true
exit "$code"

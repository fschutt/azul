#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): one app linked against a prebuilt libazul.a on the iOS Simulator.
#   bash scripts/ci/exp_ios.sh
# Builds the static lib once (scripts/demo_lib.sh; rustc says which frameworks it needs), links AzWidgets
# against it (link-dynamic, no engine crate), bundles the .app and boots it in a simulator.
set -euxo pipefail
target=aarch64-apple-ios-sim
rustup target add "$target"
FEATURES="std,logging,link-static,a11y,map-tiles,pdf"
mkdir -p demo-out
start=$(date +%s)
bash scripts/demo_lib.sh ios "$target" "$FEATURES" demo-out/lib
echo "LIB SECONDS: $(( $(date +%s) - start ))"
start=$(date +%s)
AZ_IOS_DRYRUN=1 AZ_IOS_LINK_LIB=demo-out/lib/libazul.a AZ_IOS_BUILD_JSON=demo-out/AzWidgets.json \
  APP_NAME=AzWidgets BUNDLE_ID=com.azul.AzWidgets bash scripts/build-ios.sh "$target" AzWidgets
echo "APP SECONDS: $(( $(date +%s) - start ))"
python3 scripts/demo_gate.py demo-out/AzWidgets.json --json demo-out/gate.json
( cd "target/ios-bundle/$target" && zip -qr "$OLDPWD/demo-out/AzWidgets-ios-sim.app.zip" AzWidgets.app )
ls -la demo-out
bash scripts/ci/ios_smoke.sh demo-out/smoke 12 demo-out/AzWidgets-ios-sim.app.zip || echo "SMOKE FAILED"

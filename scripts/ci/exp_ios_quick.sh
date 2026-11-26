#!/usr/bin/env bash
# EXPERIMENT (throwaway, not for master): the iOS Simulator boot test of an app built earlier (quick/*.app.zip).
set -euxo pipefail
mkdir -p demo-out
bash scripts/ci/ios_smoke.sh demo-out/smoke 12 quick/AzWidgets-ios-sim.app.zip || echo "SMOKE FAILED"

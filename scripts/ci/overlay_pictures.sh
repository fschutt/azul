#!/usr/bin/env bash
# overlay_pictures.sh: the host pictures of the popups of a window with ONE surface in one job: the bindings of this tree, the desktop QA
# build of AzWidgets (the shipped library plus the debug server and the e2e executor, scripts/ci/desktop_e2e_build.sh) and the scenario
# examples/azul-widgets/e2e/overlays_phone.json with take_screenshot (a CPU render of the window at rest) on a Pixel 8 display: the modal
# Dialog, the DropDown open and picked, the context menu, the Popover - each asked `list_popups` for HOW it is shown.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/overlay_pictures.sh' --codegen --artifacts 'demo-out/overlay-shots/**
#   demo-out/e2e-overlays/**'
#
# Steps do not stop each other; the last lines are one `STEP <name> <exit>` each, the exit code is 0 only when all passed.
set -uo pipefail
names=()
codes=()
step() {
  local name="$1"
  shift
  echo "=== STEP $name: $* ==="
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code ==="
  return 0
}

# 1. the bindings of this api.json (the QA library does not build without them)
step codegen cargo run -r -p azul-doc -- codegen all
# 2. the QA build of AzWidgets: lib.desktop-e2e and AzWidgets-e2e-linux
step build bash scripts/ci/desktop_e2e_build.sh linux AzWidgets
# 3. the scenario, headless (AZ_BACKEND=headless is the matrix's own), every take_screenshot copied to AZ_E2E_SHOT_DIR
mkdir -p demo-out/overlay-shots demo-out/e2e-overlays
matrix="python3 scripts/ci/e2e_matrix.py desktop --repo . --bin AzWidgets=demo-out/apps/AzWidgets-e2e-linux"
step scenario env AZ_E2E_SHOT_DIR="$PWD/demo-out/overlay-shots" $matrix --only overlays_phone --out demo-out/e2e-overlays
# the matrix exits 0 on a FAIL verdict: read them (the reader of the tab strip's job reads any directory of verdicts)
step verdicts python3 scripts/ci/tabs_verdicts.py demo-out/e2e-overlays

echo
bad=0
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
ls -la demo-out/overlay-shots 2>/dev/null | head -50
exit "$bad"

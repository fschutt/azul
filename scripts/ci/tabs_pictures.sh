#!/usr/bin/env bash
# tabs_pictures.sh: the host pictures of the tab strip in one job: api.json by autofix (the new API), the desktop QA build of AzWidgets
# (the shipped library plus the debug server and the e2e executor, scripts/ci/desktop_e2e_build.sh) over the new bindings, and the
# scenarios of this work with take_screenshot (a CPU render of the window at rest) in a Pixel 8 / iPhone band.
#
#   scripts/remote.sh run <ref> ubuntu-22.04 'bash scripts/ci/tabs_pictures.sh' --artifacts 'api.json
#   api.diff
#   demo-out/tabs-shots/**
#   demo-out/e2e-tabs/**
#   demo-out/results/*.json'
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

# 1. api.json for the new API: the autofix step of the gate (twice, the second must change nothing)
step autofix env TABS_ONLY=autofix bash scripts/ci/tabs_gate.sh
# 2. azul-doc's own checks of that api.json (the double-drop invariant, the guide's links and example calls)
step doc_check cargo run -r -p azul-doc -- check
step doc_check_layout cargo run -r -p azul-doc -- check layout
# 3. the bindings of that api.json (the demo library does not build without them)
step codegen cargo run -r -p azul-doc -- codegen all
# 4. the QA build of AzWidgets over them: lib.desktop-e2e and AzWidgets-e2e-linux
step build bash scripts/ci/desktop_e2e_build.sh linux AzWidgets
# 5. the scenarios, headless (AZ_BACKEND=headless is the matrix's own), every take_screenshot copied to AZ_E2E_SHOT_DIR
mkdir -p demo-out/tabs-shots demo-out/e2e-tabs
export AZ_E2E_SHOT_DIR="$PWD/demo-out/tabs-shots"
step scenarios python3 scripts/ci/e2e_matrix.py desktop --repo . --bin AzWidgets=demo-out/apps/AzWidgets-e2e-linux --only tab_touch --out demo-out/e2e-tabs
# 6. the source gates of the dll (the shells read the window flags this work added)
step srcgate cargo test -p azul-srcgate --no-fail-fast

echo
bad=0
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
ls -la demo-out/tabs-shots 2>/dev/null | head -30
exit "$bad"

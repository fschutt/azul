#!/usr/bin/env bash
# sidebar_pictures.sh: the host pictures and the scenarios of the sidebar family in one job: the bindings of the api.json as it stands
# (committed), the desktop QA build of AzWidgets (the shipped library plus the debug server and the e2e executor,
# scripts/ci/desktop_e2e_build.sh), the scenarios of this work (examples/azul-widgets/e2e/sidebar.json) and the pictures
# (sidebar_pictures.json: take_screenshot, a CPU render of the window at rest, numbered in capture order).
#
#   BOX_BY=sidebar python3 box.py run <ref> 'bash scripts/ci/sidebar_pictures.sh' --codegen --artifacts 'demo-out/sidebar-shots/**
#   demo-out/e2e-sidebar/**
#   demo-out/results/*.json'
#
# Steps do not stop each other; the last lines are one `STEP <name> <exit>` each, the exit code is 0 only when all passed.
set -uo pipefail
names=()
codes=()
step() {
  local name="$1"
  shift
  echo "=== STEP $name: $*"
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code"
  return 0
}

# 1. azul-doc's own checks of the committed api.json (the double-drop invariant, the guide's links and example calls)
step doc_check cargo run -r -p azul-doc -- check
step doc_check_layout cargo run -r -p azul-doc -- check layout
# 2. the QA build of AzWidgets over the bindings: lib.desktop-e2e and AzWidgets-e2e-linux
step build bash scripts/ci/desktop_e2e_build.sh linux AzWidgets
# 3. the scenarios, headless (AZ_BACKEND=headless is the matrix's own), every take_screenshot copied to AZ_E2E_SHOT_DIR
mkdir -p demo-out/sidebar-shots demo-out/e2e-sidebar
matrix="python3 scripts/ci/e2e_matrix.py desktop --repo . --bin AzWidgets=demo-out/apps/AzWidgets-e2e-linux"
step scenarios $matrix --only sidebar.json --out demo-out/e2e-sidebar/plain
step pictures env AZ_E2E_SHOT_DIR="$PWD/demo-out/sidebar-shots" $matrix --only sidebar_pictures --out demo-out/e2e-sidebar/pictures

echo
bad=0
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
ls -la demo-out/sidebar-shots 2>/dev/null | head -50
exit "$bad"

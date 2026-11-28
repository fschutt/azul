#!/usr/bin/env bash
# redesign_gate.sh: AzDrive on the sidebar family, every step with its REAL exit code, on one runner (or the box).
#
#   BOX_BY=sidebar-drive python3 box.py run <ref> 'AZDRIVE_ONLY=autofix,check bash examples/azul-drive/scripts/redesign_gate.sh' \
#     --artifacts 'api.json api.diff demo-out/azdrive-shots/** demo-out/azdrive-e2e/** azdrive-check.txt azdrive-tests.txt'
#
# Steps (AZDRIVE_ONLY names them; none named = all of them, in this order):
#   autofix     api.json through azul-doc as scripts/ci/sidebar_gate.sh does it (`SIDEBAR_ONLY=autofix`): only while the committed
#               api.json lacks the types of the work (the SidebarRow's glyph and drop fields, the family of widgets)
#   check       codegen of the bindings from the api.json as it stands, then `cargo check -p AzDrive --lib --tests`
#   build       the desktop QA build of AzDrive (scripts/ci/desktop_e2e_build.sh linux AzDrive): the shipped libazul with the debug
#               server, the e2e executor and the test hooks, and the app linked against it (demo-out/apps/AzDrive-e2e-linux)
#   tests       the tests of the redesign's modules in AzDrive's lib, linked against that libazul
#   scenarios   examples/azul-drive/e2e/redesign.json and redesign_phone.json (examples/azul-drive/scripts/e2e_redesign.sh), the
#               pictures in demo-out/azdrive-shots
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the script is
# 0 only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${AZDRIVE_ONLY:-}"

run() {
  local name="$1"
  shift
  case ",$only," in
    ",,") ;;
    *",$name,"*) ;;
    *) return 0 ;;
  esac
  echo "=== STEP $name: $*"
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code"
}

autofix() {
  SIDEBAR_ONLY=autofix bash scripts/ci/sidebar_gate.sh 2>&1 | tail -60
  return "${PIPESTATUS[0]}"
}
run autofix autofix

check() {
  cargo run -r -p azul-doc -- codegen all 2>&1 | tail -3
  cargo check -p AzDrive --lib --tests --message-format short > azdrive-check.txt 2>&1
  local code=$?
  echo "AZDRIVE check exit $code: $(grep -c 'error' azdrive-check.txt) lines with error, $(grep -c 'warning' azdrive-check.txt) with warning"
  grep -E 'error(\[|:)' azdrive-check.txt | sort -u | head -300
  tail -3 azdrive-check.txt
  return "$code"
}
run check check

build() {
  cargo run -r -p azul-doc -- codegen all 2>&1 | tail -3
  bash scripts/ci/desktop_e2e_build.sh linux AzDrive 2>&1 | tail -60
  return "${PIPESTATUS[0]}"
}
run build build

# the tests of the redesign's modules (and what they sit beside), over the QA library
tests() {
  local lib
  lib=$(ls -d "$PWD"/demo-out/libs/demolib-linux-desktop-e2e 2>/dev/null | head -1)
  if [ -z "$lib" ]; then
    echo "no QA library in demo-out/libs (the build step has not run here)"
    return 2
  fi
  AZ_LINK_PATH="$lib/libazul.so" LD_LIBRARY_PATH="$lib" \
    cargo test -p AzDrive --lib --no-fail-fast -- \
    ui_sidebar ui_pies ui_sheet ui_location ui_preview ui_more ui_status ui_frame ui_ribbon \
    command_line model_sidebar args_form args:: model:: look:: l10n 2>&1 | tee azdrive-tests.txt | tail -150
  return "${PIPESTATUS[0]}"
}
run tests tests

scenarios() {
  bash examples/azul-drive/scripts/e2e_redesign.sh demo-out/apps/AzDrive-e2e-linux
}
run scenarios scenarios

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: AZDRIVE_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

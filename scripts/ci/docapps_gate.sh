#!/usr/bin/env bash
# docapps_gate.sh: the gate of the three document apps (AzWriter, AzSheets, AzShow) and of what they share (azul-appkit: the open
# documents, the flow editor) on one Ubuntu runner, every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'DOCAPPS_ONLY=appkit,check bash scripts/ci/docapps_gate.sh' --codegen
#
# DOCAPPS_APPS names the apps (default: AzWriter); DOCAPPS_ONLY names the steps (none named = all of them):
#   appkit    azul-appkit's plain unit tests (no libazul, no codegen: docs, flow_rules, the localization check)
#   check     the apps over the generated bindings: `cargo check --lib --tests` (a stub libazul: a check links nothing) and
#             azul-appkit with its `azul` feature (docs_ui, flow)
#   tests     the real libazul (`build-dll`) and the apps' unit tests, azul-appkit's tests that need the engine
#   clippy    `cargo clippy --all-targets --no-deps -D warnings` of the apps and of azul-appkit
#   srcgate   the repository's source rules (cargo test -p azul-srcgate): no libazul, no codegen
#   demo      scripts/demo_apps.py check + scripts/test_demo_apps.py (the manifest of the phone entries)
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the script is 0
# only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${DOCAPPS_ONLY:-}"
apps="${DOCAPPS_APPS:-AzWriter}"

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

packages=()
for app in $apps; do packages+=(-p "$app"); done

appkit() {
  cargo test -p azul-appkit --lib --no-fail-fast 2>&1 | tee appkit-tests.txt
  local code="${PIPESTATUS[0]}"
  ./scripts/zero_test_guard.sh --strict appkit-tests.txt || code=1
  return "$code"
}
run appkit appkit

# azul-dll's build script refuses a libazul that lies in the cargo profile directory, and a check links nothing: an empty stub file in a
# folder of its own satisfies it
check() {
  local link code=0
  link="$(mktemp -d)"
  : > "$link/libazul.so"
  export AZ_LINK_PATH="$link"
  cargo check "${packages[@]}" --lib --tests > check.txt 2>&1 || code=1
  # the generated code's own warnings come first; what is wrong with the apps follows
  awk '/^error/ {p=1} p' check.txt | head -400
  echo "=== warnings that point into the apps or the kit"
  grep -B2 -A8 -E '^\s+--> examples/azul-(writer|sheets|show|appkit)/' check.txt | head -200
  cargo check -p azul-appkit --features azul --lib --tests > check-appkit.txt 2>&1 || code=1
  awk '/^error/ {p=1} p' check-appkit.txt | head -200
  return "$code"
}
run check check

tests() {
  local code=0
  cargo build -p azul-dll --features build-dll 2>&1 | tail -3
  local link
  link="$(mktemp -d)"
  cp target/debug/libazul.so "$link/"
  nm -D "$link/libazul.so" | grep -q " T AzU8Vec_delete" && echo CABI_OK
  export AZ_LINK_PATH="$link"
  export LD_LIBRARY_PATH="$link${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
  cargo test "${packages[@]}" --lib --no-fail-fast 2>&1 | tee apps-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  ./scripts/zero_test_guard.sh --strict apps-tests.txt || code=1
  cargo test -p azul-appkit --features azul --lib --no-fail-fast 2>&1 | tee appkit-azul-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  return "$code"
}
run tests tests

clippy() {
  local link code=0
  link="$(mktemp -d)"
  : > "$link/libazul.so"
  export AZ_LINK_PATH="$link"
  # --no-deps: the path dependencies of the apps (azul-storage, azcloud-kit, the dll's build scripts) are not these apps' to lint
  cargo clippy "${packages[@]}" --all-targets --no-deps -- -D warnings 2>&1 | tee apps-clippy.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  cargo clippy -p azul-appkit --features azul --all-targets --no-deps -- -D warnings 2>&1 | tee appkit-clippy.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  return "$code"
}
run clippy clippy

srcgate() {
  cargo test -p azul-srcgate --no-fail-fast 2>&1 | tee srcgate.txt
  return "${PIPESTATUS[0]}"
}
run srcgate srcgate

demo() {
  local code=0
  python3 scripts/demo_apps.py check || code=1
  python3 scripts/test_demo_apps.py || code=1
  return "$code"
}
run demo demo

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: DOCAPPS_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

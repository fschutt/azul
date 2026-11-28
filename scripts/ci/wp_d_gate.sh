#!/usr/bin/env bash
# wp_d_gate.sh: the gate of AzERP (examples/azul-erp) on one Ubuntu runner (or the box), every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'WPD_GATE=apps,apps_clippy bash scripts/ci/wp_d_gate.sh' --codegen --artifacts '*.txt'
#
# Steps (WPD_GATE names them, comma separated; none named = all of them):
#   srcgate      the repository's source rules alone (cargo test -p azul-srcgate): no libazul
#   demo_apps    scripts/demo_apps.py check and scripts/test_demo_apps.py (python only)
#   check        codegen, then `cargo check --lib --tests` and clippy -D warnings of the app over the bindings: no libazul is built, so this is the
#                fast loop (the step `apps` below links the app's tests against a built libazul and runs them)
#   apps         the app over the bindings: cargo check --lib --tests and the app's own unit tests; libazul is built once in a
#                folder of its own (the apps link it over the C ABI, as the CI's lib tests do)
#   apps_clippy  clippy -D warnings of the app alone (--no-deps: the path dependencies are not this app's to lint)
#   autofix      api.json through azul-doc (only when this branch adds API): `autofix`, patches until none, normalize, a second round
#                that must change nothing; the result is api.json (upload it) and api.diff
#   doc_check    azul-doc check
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the
# script is 0 only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${WPD_GATE:-}"
apps="${WPD_APPS:-AzERP}"

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

srcgate() {
  cargo test -p azul-srcgate --no-fail-fast 2>&1 | tee srcgate.txt
  return "${PIPESTATUS[0]}"
}
run srcgate srcgate

demo_apps() {
  local code=0
  python3 scripts/demo_apps.py check || code=1
  python3 scripts/test_demo_apps.py > demo-apps-tests.txt 2>&1 || code=1
  tail -5 demo-apps-tests.txt
  return "$code"
}
run demo_apps demo_apps

check() {
  local code=0 p packages=()
  cargo run -r -p azul-doc -- codegen all || return 1
  for p in $apps; do packages+=(-p "$p"); done
  cargo check "${packages[@]}" --lib --tests 2>&1 | tee check.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  cargo clippy "${packages[@]}" --all-targets --no-deps -- -D warnings 2>&1 | tee check-clippy.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  return "$code"
}
run check check

apps() {
  local code=0 p packages=()
  cargo run -r -p azul-doc -- codegen all || return 1
  # the apps link libazul over the C ABI: its debug build in a folder of its own, as the CI's lib tests have it
  cargo build -p azul-dll --features build-dll || return 1
  local cabi
  cabi="$(mktemp -d)"
  cp target/debug/libazul.so "$cabi/"
  export AZ_LINK_PATH="$cabi"
  for p in $apps; do packages+=(-p "$p"); done
  cargo check "${packages[@]}" --lib --tests || code=1
  for p in $apps; do
    cargo test -p "$p" --lib --no-fail-fast 2>&1 | tee "$p-tests.txt"
    [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
    ./scripts/zero_test_guard.sh --strict "$p-tests.txt" || code=1
  done
  return "$code"
}
run apps apps

apps_clippy() {
  cargo run -r -p azul-doc -- codegen all || return 1
  local packages=() p
  for p in $apps; do packages+=(-p "$p"); done
  cargo clippy "${packages[@]}" --all-targets --no-deps -- -D warnings 2>&1 | tee apps-clippy.txt
  return "${PIPESTATUS[0]}"
}
run apps_clippy apps_clippy

autofix_round() {
  local out code=0
  out=$(cargo run -r -p azul-doc -- autofix 2>&1) || code=$?
  echo "$out" | grep -E "Types to add|Type modifications|Path fixes|Generated|^  [+~] " | head -40
  if [ "$code" -ne 0 ]; then
    echo "$out" | tail -8
  fi
  if echo "$out" | grep -q "Generated 0 patches"; then
    return "$code"
  fi
  cargo run -r -p azul-doc -- autofix explain >/dev/null 2>&1
  cargo run -r -p azul-doc -- patch safe target/autofix/patches || return 1
  cargo run -r -p azul-doc -- patch target/autofix/patches || return 1
  return 0
}
autofix() {
  cargo run -r -p azul-doc -- autofix apply || return 1
  local i
  for i in 1 2 3 4 5 6; do
    autofix_round || return 1
  done
  cargo run -r -p azul-doc -- normalize || return 1
  git diff api.json > api.diff
  cp api.json api.round1.json
  autofix_round || return 1
  cargo run -r -p azul-doc -- normalize || return 1
  if ! cmp -s api.json api.round1.json; then
    echo "autofix round 2 changed api.json"
    diff api.round1.json api.json | head -40
    return 1
  fi
  echo "autofix round 2 changed nothing"
  return 0
}
run autofix autofix

doc_check() {
  local code=0
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo run -r -p azul-doc -- check || code=1
  return "$code"
}
run doc_check doc_check

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: WPD_GATE=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

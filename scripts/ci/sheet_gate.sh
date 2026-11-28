#!/usr/bin/env bash
# sheet_gate.sh: the gate of the BottomSheet widget (layout/src/widgets/bottom_sheet.rs) and of the apps that use it (AzMaps, AzCode) on one
# Ubuntu runner (or the box), every step with its REAL exit code.
#
#   BOX_BY=wp-e2 python3 box.py run <ref> 'SHEET_ONLY=<step>,<step> bash scripts/ci/sheet_gate.sh' [--codegen] --artifacts 'api.json api.diff'
#
# Steps (SHEET_ONLY names them; none named = all of them):
#   tests      the widget's tests by name, in azul-layout, with the CI's feature list
#   e2e_all    every test of azul-layout's e2e module (the runner's pointer capture)
#   phone_tests  the integration tests of the phone layouts of AzMaps (layout/tests/azul_maps_*): the floating layout and the banner
#   clippy     the CI's clippy line of azul-layout, the test targets included
#   autofix    api.json through azul-doc: `autofix add` for the new functions, patches until there are none, normalize; the result is
#              api.json (upload it) and api.diff; then a SECOND round that must change nothing
#   doc_check  azul-doc check (the double-drop invariant, the guide links) and azul-doc check layout
#   dll_check  codegen of the bindings from the api.json as it stands, cargo check of azul-dll, the srcgate tests
#   apps       the two apps over the bindings: cargo check --lib --tests and the apps' own unit tests (no engine crate builds in them)
#   apps_clippy  clippy -D warnings of the two apps
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the script is 0
# only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${SHEET_ONLY:-}"

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

features="azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater"

tests() {
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-layout --lib --features "$features" --no-fail-fast -- bottom_sheet detents 2>&1 | tee sheet-tests.txt
  return "${PIPESTATUS[0]}"
}
run tests tests

# every scenario of the e2e module of azul-layout: the runner applies pointer capture now (as the shells do), which must not move another one
e2e_all() {
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-layout --lib --features "$features" --no-fail-fast -- e2e:: 2>&1 | tee e2e-all.txt
  return "${PIPESTATUS[0]}"
}
run e2e_all e2e_all

# the integration tests of the phone layouts of AzMaps (they read the app's source: ui.rs, lib.rs, flow.rs, drive_ui.rs)
phone_tests() {
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-layout --features "$features" --no-fail-fast \
    --test all -- azul_maps_floating azul_maps_drive 2>&1 | tee phone-tests.txt
  return "${PIPESTATUS[0]}"
}
run phone_tests phone_tests

clippy() {
  cargo clippy -p azul-layout --all-targets --features "$features" -- -D warnings 2>&1 | tee clippy.txt
  return "${PIPESTATUS[0]}"
}
run clippy clippy

# api.json: `autofix add` stages the functions (a plain `autofix` syncs TYPES only), the patches are applied until the scan finds none,
# and the tree is normalized. The functions are the ones the work added to the API.
api_functions=(
  BottomSheet.create
  BottomSheet.set_label
  BottomSheet.with_label
  BottomSheet.set_grip_label
  BottomSheet.with_grip_label
  BottomSheet.set_detent
  BottomSheet.with_detent
  BottomSheet.set_lowest
  BottomSheet.with_lowest
  BottomSheet.set_highest
  BottomSheet.with_highest
  BottomSheet.set_container_height
  BottomSheet.with_container_height
  BottomSheet.set_peek_height
  BottomSheet.with_peek_height
  BottomSheet.set_collapsed_height
  BottomSheet.with_collapsed_height
  BottomSheet.set_top_margin
  BottomSheet.with_top_margin
  BottomSheet.set_bottom_inset
  BottomSheet.with_bottom_inset
  BottomSheet.set_safe_bottom
  BottomSheet.with_safe_bottom
  BottomSheet.set_grab_height
  BottomSheet.with_grab_height
  BottomSheet.set_on_event
  BottomSheet.with_on_event
  BottomSheet.height_of
  BottomSheet.dom
  BottomSheet.swap_with_default
  SheetDetent.higher
  SheetDetent.lower
)
autofix_round() {
  local out code=0
  out=$(cargo run -r -p azul-doc -- autofix 2>&1) || code=$?
  echo "$out" | grep -E "Types to add|Type modifications|Path fixes|Generated|^  [+~] " | head -40
  if [ "$code" -ne 0 ]; then
    echo "$out" | grep -A 40 "critical FFI safety errors" | head -80
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
  if ! cargo run -r -p azul-doc -- autofix add "${api_functions[@]}"; then
    echo "autofix add of the whole list failed: one function at a time"
    local f
    for f in "${api_functions[@]}"; do
      cargo run -r -p azul-doc -- autofix add "$f" || echo "ADD FAILED: $f"
    done
  fi
  cargo run -r -p azul-doc -- autofix apply || return 1
  local i
  for i in 1 2 3 4 5 6; do
    autofix_round || return 1
  done
  cargo run -r -p azul-doc -- normalize || return 1
  git diff api.json > api.diff
  echo "api.json: $(git diff --stat api.json | tail -1)"
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
  cargo run -r -p azul-doc -- check layout || code=1
  return "$code"
}
run doc_check doc_check

dll_check() {
  local code=0
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo check -p azul-dll || code=1
  cargo test -p azul-srcgate --no-fail-fast || code=1
  return "$code"
}
run dll_check dll_check

apps() {
  local code=0
  cargo run -r -p azul-doc -- codegen all || return 1
  # the apps link libazul over the C ABI: its debug build in a folder of its own, as the CI's lib tests have it
  cargo build -p azul-dll --features build-dll || return 1
  local cabi
  cabi="$(mktemp -d)"
  cp target/debug/libazul.so "$cabi/"
  export AZ_LINK_PATH="$cabi"
  cargo check -p AzMaps -p AzCode --lib --tests || code=1
  cargo test -p AzMaps --lib --no-fail-fast 2>&1 | tee azmaps-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  ./scripts/zero_test_guard.sh --strict azmaps-tests.txt || code=1
  cargo test -p AzCode --lib --no-fail-fast 2>&1 | tee azcode-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  ./scripts/zero_test_guard.sh --strict azcode-tests.txt || code=1
  return "$code"
}
run apps apps

apps_clippy() {
  cargo run -r -p azul-doc -- codegen all || return 1
  # --no-deps: the path dependencies of the apps (azul-routing, azul-search, the dll's build scripts) are not these apps' to lint.
  # SHEET_CLIPPY_APPS names the apps (default both): a branch that has only one of them clean lints that one.
  local packages=() app
  for app in ${SHEET_CLIPPY_APPS:-AzMaps AzCode}; do packages+=(-p "$app"); done
  cargo clippy "${packages[@]}" --all-targets --no-deps -- -D warnings 2>&1 | tee apps-clippy.txt
  return "${PIPESTATUS[0]}"
}
run apps_clippy apps_clippy

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: SHEET_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

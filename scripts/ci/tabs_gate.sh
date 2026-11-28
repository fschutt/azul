#!/usr/bin/env bash
# tabs_gate.sh: the gate of the cutout / system bar / tab strip work on one Ubuntu runner, every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha or ci-wip ref> ubuntu-22.04 'TABS_ONLY=<step>,<step> bash scripts/ci/tabs_gate.sh' [--codegen] \
#     --artifacts 'api.json
# api.diff'
#
# Steps (TABS_ONLY names them; none named = all of them):
#   tests      the tests of the work by name, in azul-core and azul-layout, with the CI's feature list
#   workspace  the CI's workspace test command of css, core, layout and webrender in full (the gate before a push)
#   clippy     the CI's clippy line of the three crates, the test targets included
#   autofix    api.json through azul-doc: `autofix add` for the new functions, patches until there are none, normalize; the
#              result is api.json (upload it) and api.diff (what it changed); then a SECOND round that must change nothing
#   doc_check  azul-doc check (the double-drop invariant, the guide links) and azul-doc check layout
#   widgets_check  the AzWidgets example type-checks over the C ABI (after the autofix step's api.json)
#   dll_check  codegen of the bindings from the api.json as it stands, cargo check of azul-dll, the srcgate tests (no --codegen needed)
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the
# script is 0 only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${TABS_ONLY:-}"

run() {
  local name="$1"
  shift
  case ",$only," in
    ",,") ;;
    *",$name,"*) ;;
    *) return 0 ;;
  esac
  echo "=== STEP $name: $* ==="
  "$@"
  local code=$?
  names+=("$name")
  codes+=("$code")
  echo "=== STEP $name: exit $code ==="
}

features="azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater"

tests() {
  local code=0
  # the tests of this work by name: the module names carry them
  cargo test -p azul-css -p azul-core -p azul-layout --lib --features "$features" --no-fail-fast -- \
    display_features display_ops tab_strip tab_pills tab_flip ribbon_touch 2>&1 | tee tabs-tests.txt
  code=${PIPESTATUS[0]}
  return "$code"
}
run tests tests

workspace() {
  sudo apt-get update -qq || true
  sudo apt-get install -y fonts-liberation fonts-dejavu-core || true
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-css -p azul-core -p azul-layout -p webrender --lib --tests \
    --features "$features" --no-fail-fast 2>&1 | tee workspace-tests.txt
  local code=${PIPESTATUS[0]}
  ./scripts/zero_test_guard.sh workspace-tests.txt || code=1
  return "$code"
}
run workspace workspace

clippy() {
  cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features "$features" -- -D warnings 2>&1 | tee clippy.txt
  return "${PIPESTATUS[0]}"
}
run clippy clippy

# api.json: `autofix add` stages the functions (a plain `autofix` syncs TYPES only), the patches are applied until the scan finds
# none, and the tree is normalized. The functions are the ones the work added to the API.
api_functions=(
  CallbackInfo.get_display_features
  LayoutCallbackInfo.get_display_features
  StatusBarBand.from_display
  TabHeader.set_status_bar
  TabHeader.with_status_bar
  TabHeader.set_flip
  TabHeader.with_flip
  TabContent.set_flip_of
  TabContent.with_flip_of
  Ribbon.set_status_bar
  Ribbon.with_status_bar
  Ribbon.set_flip
  Ribbon.with_flip
)
autofix_round() {
  local out
  out=$(cargo run -r -p azul-doc -- autofix 2>&1) || { echo "$out" | tail -30; return 1; }
  echo "$out" | grep -E "Types to add|Type modifications|Path fixes|Generated|^  [+~] " | head -40
  if echo "$out" | grep -q "Generated 0 patches"; then
    return 0
  fi
  cargo run -r -p azul-doc -- autofix explain >/dev/null 2>&1
  cargo run -r -p azul-doc -- patch safe target/autofix/patches || return 1
  cargo run -r -p azul-doc -- patch target/autofix/patches || return 1
  return 0
}
autofix() {
  cargo run -r -p azul-doc -- autofix add "${api_functions[@]}" || return 1
  cargo run -r -p azul-doc -- autofix apply || return 1
  local i
  for i in 1 2 3 4 5 6; do
    autofix_round || return 1
  done
  cargo run -r -p azul-doc -- normalize || return 1
  git diff api.json > api.diff
  echo "api.json: $(git diff --stat api.json | tail -1)"
  cp api.json api.round1.json
  # round 2 on the result: nothing may change
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
  cargo run -r -p azul-doc -- check || code=1
  cargo run -r -p azul-doc -- check layout || code=1
  return "$code"
}
run doc_check doc_check

dll_check() {
  local code=0
  # the bindings of the api.json as it stands NOW (after the autofix step): azul-dll does not build without them, and a struct
  # that grew (WindowFlags) must be the same size on both sides
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo check -p azul-dll || code=1
  cargo test -p azul-srcgate --no-fail-fast || code=1
  return "$code"
}
run dll_check dll_check

# AzWidgets over the C ABI: the page of this work type-checks against the bindings of the api.json as it stands
widgets_check() {
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo check -p AzWidgets --lib --tests
}
run widgets_check widgets_check

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: TABS_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

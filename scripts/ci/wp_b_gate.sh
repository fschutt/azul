#!/usr/bin/env bash
# wp_b_gate.sh: the gate of WP-B (AzNotes, AzContacts, AzTasks and what they share: azul-appkit's `form` / `follow` / `list`, azul-pim,
# azul-contacts-core) on one Ubuntu runner (or the box), every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'WPB_ONLY=<step>,<step> bash scripts/ci/wp_b_gate.sh' --codegen
#   BOX_BY=wp-b python3 box.py run <ref> 'WPB_ONLY=<step>,<step> bash scripts/ci/wp_b_gate.sh' --codegen
#
# Steps (WPB_ONLY names them; none named = all of them):
#   demo_apps    the manifest's own check and its python tests (no libazul)
#   srcgate      the repository's source rules alone (cargo test -p azul-srcgate): no libazul, no codegen
#   plain        the plain crates: azul-pim, azul-contacts-core, azul-appkit without the azul feature (their tests need no libazul)
#   apps         the three apps over the bindings: cargo check --lib --tests, their unit tests and azul-appkit's own (feature azul)
#                with the zero-test guard
#   apps_clippy  clippy -D warnings (--no-deps: the path dependencies are not these apps' to lint) of the three apps and azul-appkit
#   layout       the widget family's tests in azul-layout that the apps lean on (location_bar, preview, sidebar, app_frame, ribbon)
#   autofix      api.json through azul-doc for the functions this work added (LocationBar's (?) buttons), a second round that must change
#                nothing: upload api.json and api.diff
#   doc_check    azul-doc check (the double-drop invariant, the guide links) and azul-doc check layout
#   scenarios    the apps' own scenario files (examples/azul-*/e2e/*.json) through the engine's executor on the QA builds, one test per
#                file, the pictures of their take_screenshot steps kept in demo-out/wp-b/scenarios/<app>/
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the script is 0
# only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${WPB_ONLY:-}"

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

demo_apps() {
  local code=0
  python3 scripts/demo_apps.py check || code=1
  python3 scripts/test_demo_apps.py || code=1
  return "$code"
}
run demo_apps demo_apps

srcgate() {
  cargo test -p azul-srcgate --no-fail-fast 2>&1 | tee srcgate.txt
  return "${PIPESTATUS[0]}"
}
run srcgate srcgate

plain() {
  local code=0
  cargo test -p azul-pim --no-fail-fast 2>&1 | tee pim-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  cargo test -p azul-contacts-core --no-fail-fast 2>&1 | tee contacts-core-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  ./scripts/zero_test_guard.sh --strict contacts-core-tests.txt || code=1
  cargo test -p azul-appkit --no-fail-fast 2>&1 | tee appkit-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  ./scripts/zero_test_guard.sh --strict appkit-tests.txt || code=1
  return "$code"
}
run plain plain

# the libazul the apps link over the C ABI: its debug build in a folder of its own, as the CI's lib tests have it
libazul() {
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo build -p azul-dll --features build-dll || return 1
  local cabi
  cabi="$(mktemp -d)"
  cp target/debug/libazul.so "$cabi/"
  export AZ_LINK_PATH="$cabi"
  echo "AZ_LINK_PATH=$cabi" > .wp_b_link
}

apps() {
  local code=0
  libazul || return 1
  # shellcheck disable=SC1091
  . ./.wp_b_link
  export AZ_LINK_PATH
  # one cargo call per crate: a crate that does not compile does not hide the errors of the others
  cargo check -p azul-appkit --features azul --lib --tests 2>&1 | tee appkit-check.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  local app
  for app in AzNotes AzContacts AzTasks; do
    cargo check -p "$app" --lib --tests 2>&1 | tee "$app-check.txt"
    [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  done
  for app in AzNotes AzContacts AzTasks; do
    cargo test -p "$app" --lib --no-fail-fast 2>&1 | tee "$app-tests.txt"
    [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
    ./scripts/zero_test_guard.sh --strict "$app-tests.txt" || code=1
  done
  cargo test -p azul-appkit --features azul --lib --no-fail-fast 2>&1 | tee appkit-azul-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  ./scripts/zero_test_guard.sh --strict appkit-azul-tests.txt || code=1
  return "$code"
}
run apps apps

apps_clippy() {
  cargo run -r -p azul-doc -- codegen all || return 1
  # --no-deps: the path dependencies of the apps (azul-routing, azul-search, the dll's build scripts) are not these apps' to lint.
  local code=0
  cargo clippy -p azul-appkit --features azul --all-targets --no-deps -- -D warnings 2>&1 | tee appkit-clippy.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  local app
  for app in AzNotes AzContacts AzTasks; do
    cargo clippy -p "$app" --all-targets --no-deps -- -D warnings 2>&1 | tee "$app-clippy.txt"
    [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  done
  return "$code"
}
run apps_clippy apps_clippy

layout() {
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-layout --lib --features "$features" --no-fail-fast -- \
    location_bar preview sidebar app_frame action_sheet 2>&1 | tee layout-tests.txt
  return "${PIPESTATUS[0]}"
}
run layout layout

# api.json: `autofix add` stages the functions (a plain `autofix` syncs TYPES only), the patches are applied until the scan finds none, and
# the tree is normalized. The functions are the ones this work added to the API (LocationBar's (?) buttons); upload api.json and api.diff
# (--artifacts 'api.json
# api.diff') and commit api.json as it comes back (the stamp lines untouched).
api_functions=(
  LocationBar.add_detail
  LocationBar.with_detail
  LocationBar.set_details
  LocationBar.with_details
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

# the scenario files of the three apps on the desktop QA builds: each file is ONE test (a file plays all its tests in one process)
scenarios() {
  local code=0 out=demo-out/wp-b/scenarios
  mkdir -p "$out"
  bash scripts/ci/desktop_e2e_build.sh linux AzNotes AzContacts AzTasks || return 1
  local pair app dir
  for pair in AzNotes:azul-notes AzContacts:azul-contacts AzTasks:azul-tasks; do
    app="${pair%%:*}"
    dir="examples/${pair##*:}/e2e"
    [ -d "$dir" ] || { echo "no $dir"; continue; }
    local file name data
    for file in "$dir"/*.json; do
      [ -f "$file" ] || continue
      name="$(basename "$file" .json)"
      mkdir -p "$out/$app/$name"
      data="$(mktemp -d)"
      # AzTasks names its data folder `--data`, the kit's apps `--data-dir`
      local dataflag=--data-dir
      [ "$app" = AzTasks ] && dataflag=--data
      AZ_E2E="$PWD/$file" AZ_E2E_SHOT_DIR="$PWD/$out/$app/$name" AZ_BACKEND=headless AZ_LOG=off AZLIN_DATA="$data" AZLIN_CONFIG=off \
        timeout 600 "demo-out/apps/$app-e2e-linux" --sample "$dataflag" "$data" --size 1280x800 --mode light --language en --theme flora \
        > "$out/$app/$name.txt" 2>&1
      local c=$?
      echo "=== scenario $app/$name exit $c"
      grep -E "^test |^---- |test result|step [0-9]+: " "$out/$app/$name.txt" | head -60
      [ "$c" -eq 0 ] || code=1
    done
  done
  return "$code"
}
run scenarios scenarios

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: WPB_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

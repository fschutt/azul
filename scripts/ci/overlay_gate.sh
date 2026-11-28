#!/usr/bin/env bash
# overlay_gate.sh: the gate of the popups-on-a-phone work (a window with one surface shows its popups, dialogs and windows
# inside itself) on one Ubuntu runner, every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha or ci-wip ref> ubuntu-22.04 'OVERLAY_ONLY=<step>,<step> bash scripts/ci/overlay_gate.sh' [--codegen]
#
# Steps (OVERLAY_ONLY names them; none named = all of them):
#   tests      the tests of the work by name, in azul-core and azul-layout, with the CI's feature list
#   teeth      the tests of the capture and override-dirt fixes must be red when the fix is taken out (a scratch copy)
#   workspace  the CI's workspace test command of css, core, layout and webrender in full (the gate before a push)
#   clippy     the CI's two clippy lines of the three crates, the test targets included (the second without the 36 findings
#              of master's `unused_qualifications`, which are listed by file instead)
#   autofix    api.json through azul-doc: patches until there are none, normalize; a SECOND round that must change nothing
#   doc_check  azul-doc check (the double-drop invariant, the guide links) and azul-doc check layout
#   dll_check  codegen of the bindings, cargo check of azul-dll, the srcgate tests
#   dll_tests  the dll's tests of the work (the headless shell on a phone display) and of the popups it touched
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the
# script is 0 only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${OVERLAY_ONLY:-}"

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
    transient event_boundary menu_overlay input_capture override_dirt pointer_capture_ops 2>&1 | tee overlay-tests.txt
  local first=${PIPESTATUS[0]}
  cargo test -p azul-layout --features "$features" --test all --no-fail-fast -- \
    a_window_with_one_surface a_menu_of_a_window a_popover_fits a_screen_reader_reaches 2>&1 | tee overlay-tests-integration.txt
  local second=${PIPESTATUS[0]}
  [ "$first" -eq 0 ] && [ "$second" -eq 0 ] || code=1
  return "$code"
}
run tests tests

# The tests of the fixes that came out of a review (the runner's capture and override-dirt tests) must be RED without the fix:
# each fix is taken out of a scratch copy of the source (nothing is committed), the tests run, and the source is put back.
teeth() {
  local code=0
  # 1. an override write that needs layout stages no css dirt
  python3 - <<'PY'
p = "layout/src/window.rs"
s = open(p).read()
old = "if override_only && needs_relayout {\n            self.stage_css_dirty("
assert old in s, "the staging of the override dirt moved: update the teeth step"
open(p, "w").write(s.replace(old, "if false && override_only && needs_relayout {\n            self.stage_css_dirty(", 1))
PY
  cargo test -p azul-css -p azul-core -p azul-layout --lib --features "$features" -- override_dirt 2>&1 | tee teeth-override.txt
  if [ "${PIPESTATUS[0]}" -eq 0 ]; then
    echo "TEETH override: the tests PASS without the fix - they prove nothing"
    code=1
  else
    echo "TEETH override: red without the fix, as it must be"
  fi
  git checkout -- layout/src/window.rs
  # 2. the explicit capture runs AFTER the press-target capture (the order the shells and the runner had)
  python3 - <<'PY'
p = "layout/src/input_capture.rs"
s = open(p).read()
a = """    if let Some(held) = *capture {
        if apply_pointer_capture(events, held) {
            *capture = None;
        }
    }
    hover.apply_press_target_capture(events, in_release_path);
"""
b = """    hover.apply_press_target_capture(events, in_release_path);
    if let Some(held) = *capture {
        if apply_pointer_capture(events, held) {
            *capture = None;
        }
    }
"""
assert a in s, "the capture order moved: update the teeth step"
open(p, "w").write(s.replace(a, b, 1))
PY
  cargo test -p azul-css -p azul-core -p azul-layout --lib --features "$features" -- input_capture pointer_capture_ops 2>&1 | tee teeth-capture.txt
  if [ "${PIPESTATUS[0]}" -eq 0 ]; then
    echo "TEETH capture: the tests PASS in the old order - they prove nothing"
    code=1
  else
    echo "TEETH capture: red in the old order, as it must be"
  fi
  git checkout -- layout/src/input_capture.rs
  return "$code"
}
run teeth teeth

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
  local code=0 kind found
  # The CI's two clippy runs of the three crates (the default features; the feature set the tests compile), the test targets
  # included. The CI runs them with `-D warnings`, and origin/master carries 36 `unnecessary qualification` findings in
  # layout/src/window.rs (the paths were written before `CssProperty` was imported there), so that run is red on master; and the
  # lint cannot be allowed from the command line, because the crate WARNS it in its own source (`#![warn(unused_qualifications)]`
  # beats `-A`). So the runs are made without `-D warnings`, one line per diagnostic, and every diagnostic that is not that
  # one finding fails the step: what is checked is the same set of lints, the 36 are listed by file instead.
  for kind in default features; do
    if [ "$kind" = features ]; then
      cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features "$features" --message-format=short 2>&1 | tee "clippy-$kind.txt"
    else
      cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --message-format=short 2>&1 | tee "clippy-$kind.txt"
    fi
    [ "${PIPESTATUS[0]}" -eq 0 ] || { echo "clippy ($kind): the build failed"; code=1; }
    found=$(grep -E '\.rs:[0-9]+:[0-9]+: (warning|error)' "clippy-$kind.txt" | grep -v 'unnecessary qualification' || true)
    if [ -n "$found" ]; then
      echo "clippy ($kind): findings that are not master's unnecessary qualification:"
      echo "$found" | head -80
      code=1
    fi
    echo "--- unnecessary qualifications by file ($kind; origin/master: layout/src/window.rs 36, nothing else) ---"
    grep -E '\.rs:[0-9]+:[0-9]+: (warning|error): unnecessary qualification' "clippy-$kind.txt" | sed -E 's/:[0-9]+:[0-9]+:.*//' | sort | uniq -c
  done
  return "$code"
}
run clippy clippy

# api.json: the work adds no function to the C API (its types are not repr(C)): the rounds below must find nothing to change.
autofix_round() {
  # a critical FFI error does not stop the round: the patches were written anyway ("fix the errors before proceeding with
  # codegen"), so they are applied, the error is printed in full and the exit code of the LAST round carries it
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
  local i
  for i in 1 2 3 4 5 6; do
    autofix_round || return 1
  done
  cargo run -r -p azul-doc -- normalize || return 1
  git diff api.json > api.diff
  echo "api.json: $(git diff --stat api.json | tail -1)"
  if ! git diff --quiet api.json; then
    echo "autofix changed api.json: the work moved the API, or the tree drifted (api.diff)"
    head -60 api.diff
    return 1
  fi
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
  # the examples lint reads target/codegen: the bindings of THIS api.json, not the cache's older ones
  cargo run -r -p azul-doc -- codegen all || return 1
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

dll_tests() {
  local code=0
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo test -p azul-dll --features build-dll --lib --no-fail-fast -- transient the_layout_dirt_of_one_dom 2>&1 | tee dll-lib-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  cargo test -p azul-dll --features build-dll --no-fail-fast \
    --test a_one_surface_window_shows_its_popups_inside_itself --test a_menu_on_a_phone_opens_inside_the_window \
    --test a_second_window_of_a_phone_is_a_view_that_fills_it --test blur_releases_mouse_buttons \
    --test transient_window_layout 2>&1 | tee dll-tests.txt
  [ "${PIPESTATUS[0]}" -eq 0 ] || code=1
  return "$code"
}
run dll_tests dll_tests

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: OVERLAY_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

#!/usr/bin/env bash
# mobile_apps_gate.sh: the gate of the mobile apps work on one Ubuntu runner, every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'bash scripts/ci/mobile_apps_gate.sh' --codegen --artifacts 'autofix.diff'
#
# Needs the bindings (remote.sh --codegen). A step that fails does not stop the ones after it, so one run says
# everything that is wrong; the last lines are one `STEP <name> <exit>` each and the exit code of the script is 0 only
# when every step that has to pass passed AND the RED step failed (the red step takes the guard of the safe-area
# fix out again and runs its test: it must fail, which proves the test sees the bug).
set -uo pipefail
declare -A exit_of
order=()

run() {
  local name="$1"
  shift
  order+=("$name")
  echo "=== STEP $name: $* ==="
  "$@"
  exit_of[$name]=$?
  echo "=== STEP $name: exit ${exit_of[$name]} ==="
}

features="azul-core/serde-json,azul-core/url,azul-layout/json,azul-layout/e2e-server,azul-layout/updater"

# 1. the pure parts: the language tags, the info's localizer slot, the layout tests of the phone work
run css_language cargo test -p azul-css --lib system_language
run core_callbacks cargo test -p azul-core --lib callbacks_test
run layout_lib cargo test -p azul-layout --lib -- fluent widgets::button widgets::themes
run layout_tests cargo test -p azul-layout --test all -- safe_area_inset azul_widgets_video azul_maps_sidebar azul_paint_toolbar a_button_is_a_touch azul_widgets_toolbar

# 2. the engine through the headless shell: RED (the guard out), then GREEN
file=dll/src/desktop/shell2/common/layout.rs
sed -i 's/let safe_area_moved_precheck = layout_window.safe_area_moved_since_layout(current_window_state);/let safe_area_moved_precheck = false;/' "$file"
if git diff --quiet -- "$file"; then
  echo "the RED step could not take the guard out of $file: the line it looks for is gone"
  red_setup=1
else
  red_setup=0
fi
run red_safe_area_without_the_guard cargo test -p azul-dll --features build-dll --test headless_safe_area_moved
git checkout -q -- "$file"
run green_dll_tests cargo test -p azul-dll --features build-dll --test headless_safe_area_moved --test localization_headless --test android_language_contract

# 3. lints of the three engine crates, as the CI's second clippy step runs them
run clippy cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features "$features" -- -D warnings

# 4. the apps over the C ABI, against the library of this tree (a copy of it in a folder of its own, as the CI's
#    lib tests do: they link-dynamic against that file)
run cabi_build cargo build -p azul-dll --features build-dll
cabi_dir="$(mktemp -d)"
cp target/debug/libazul.so "$cabi_dir/"
export AZ_LINK_PATH="$cabi_dir"
run cabi_exports bash -c 'nm -D "$AZ_LINK_PATH/libazul.so" | grep -q " T AzU8Vec_delete"'
run appkit_tests cargo test -p azul-appkit --features azul --no-fail-fast
run azpaint_tests cargo test -p AzPaint --lib --no-fail-fast
run azmaps_tests cargo test -p AzMaps --lib --no-fail-fast
run azmeet_tests cargo test -p AzMeet --lib --no-fail-fast
run azwriter_tests cargo test -p AzWriter --lib --no-fail-fast
run azwidgets_tests cargo test -p AzWidgets --lib --no-fail-fast

# 5. the Android shell compiles for a phone
run android_check bash scripts/ci/android_check.sh

# 6. api.json against the source: autofix must find nothing more (the file the CI's autofix_check runs)
autofix() {
  cargo run -r -p azul-doc -- autofix &&
    cargo run -r -p azul-doc -- autofix explain &&
    cargo run -r -p azul-doc -- patch safe target/autofix/patches &&
    cargo run -r -p azul-doc -- patch target/autofix/patches &&
    cargo run -r -p azul-doc -- normalize &&
    git diff --quiet
}
run autofix autofix
git diff > autofix.diff || true

echo
bad=0
for name in "${order[@]}"; do
  code="${exit_of[$name]}"
  echo "STEP $name $code"
  if [ "$name" = "red_safe_area_without_the_guard" ]; then
    [ "$red_setup" -eq 0 ] || { echo "the RED step ran on an unchanged file"; bad=1; }
    [ "$code" -ne 0 ] || { echo "the RED step passed: the test does not see the bug"; bad=1; }
  else
    [ "$code" -eq 0 ] || bad=1
  fi
done
exit "$bad"

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
run layout_tests cargo test -p azul-layout --test all -- safe_area_inset azul_widgets_video azul_maps_sidebar azul_maps_drive azul_paint_toolbar azul_writer_phone a_button_is_a_touch azul_widgets_toolbar

# 1b. RED of the layout tests of the phone work: with the themes of the engine and the files of the apps as they were on the
#     base commit (MOBILE_APPS_BASE, a commit of master before the fixes) the tests that see the bugs must FAIL, each by name
#     (the touch floor and the video card by their assertions; the sheet, the drive banner and the toolbar because the files
#     of the base have no such style at all). The files are put back from HEAD after.
red_layout() {
  local base="${MOBILE_APPS_BASE:-}"
  [ -n "$base" ] || { echo "MOBILE_APPS_BASE is not set: no base commit to take the previous files from"; return 2; }
  git fetch -q --depth=1 origin "$base" || { echo "cannot fetch $base"; return 2; }
  local files=(layout/src/widgets/themes/decl.rs layout/src/widgets/themes/flat.rs layout/src/widgets/themes/flora.rs
               examples/azul-maps/src/lib.rs examples/azul-maps/src/drive_ui.rs examples/azul-widgets/src/video.rs
               examples/azul-paint/src/lib.rs examples/azul-writer/src/ribbon.rs)
  local must_fail=(
    a_button_is_a_touch_target_on_a_phone::a_button_is_48_dp_in_each_direction_on_android_in_every_theme
    a_button_is_a_touch_target_on_a_phone::a_button_is_44_pt_in_each_direction_on_ios_in_every_theme
    a_button_is_a_touch_target_on_a_phone::a_text_field_is_as_tall_as_a_finger_on_a_phone_and_a_desktop_field_is_not
    azul_widgets_video_card_fits_a_phone::no_node_of_the_video_card_crosses_the_right_edge_of_a_phone
    azul_widgets_video_card_fits_a_phone::the_stage_is_as_wide_as_the_page_leaves_it_up_to_the_clips_480_and_keeps_16_to_9
    azul_maps_sidebar_is_a_sheet_on_a_phone::on_a_portrait_phone_the_sheet_leaves_the_map_visible_below_it
    azul_maps_sidebar_is_a_sheet_on_a_phone::a_short_sidebar_is_as_tall_as_its_content_on_a_phone
    azul_maps_drive_banner_fits_a_phone::on_a_phone_the_banner_and_the_bar_are_the_screen_less_8_px_each_side_and_no_node_crosses_an_edge
    azul_maps_drive_banner_fits_a_phone::the_end_key_of_the_bar_is_on_the_screen_and_a_finger_wide
    azul_paint_toolbar_fits_a_phone::the_six_keys_fit_the_narrowest_phone_and_are_48_dp_each
    azul_writer_phone_bar_fits_a_phone::the_bar_is_the_width_of_the_screen_and_every_key_is_48_dp
    azul_writer_phone_bar_fits_a_phone::the_first_keys_are_on_the_screen_without_scrolling
  )
  git checkout -q "$base" -- "${files[@]}" || { echo "cannot take the previous files from $base"; return 2; }
  local out
  out="$(mktemp)"
  cargo test -p azul-layout --test all -- a_button_is_a_touch azul_widgets_video azul_maps_sidebar azul_maps_drive azul_paint_toolbar azul_writer_phone > "$out" 2>&1
  git checkout -q HEAD -- "${files[@]}"
  grep -E '^test .* \.\.\. (ok|FAILED)$' "$out" || true
  local missing=0 name
  for name in "${must_fail[@]}"; do
    if grep -q -E "^test ${name} \.\.\. FAILED$" "$out"; then
      echo "RED ok: ${name} fails on the files of the base"
    else
      echo "RED MISSING: ${name} did not fail on the files of the base"
      missing=1
    fi
  done
  return "$missing"
}
run red_layout_on_the_previous_files red_layout

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

# 5. the phone shells compile: Android (cargo check for the target) and iOS (the same without an Apple SDK)
run android_check bash scripts/ci/android_check.sh
run ios_cross_check bash scripts/ci/ios_cross_check.sh

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

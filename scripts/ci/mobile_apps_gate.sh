#!/usr/bin/env bash
# mobile_apps_gate.sh: the gate of the mobile apps work on one Ubuntu runner, every step with its REAL exit code.
#
#   scripts/remote.sh run <full sha> ubuntu-22.04 'MOBILE_APPS_BASE=<sha of master before the fixes> bash scripts/ci/mobile_apps_gate.sh' \
#     --codegen --artifacts 'autofix.diff'
#   MOBILE_APPS_ONLY=<step>,<step> in front of the command runs those steps only (the RED of the layout tests builds no azul-dll:
#   no --codegen; the checks of the dll, the apps and the two phone shells are the quick first run after a change).
#
# A runner job ends after 120 minutes and the whole gate is longer than that on a cold cache: two runs, in this order,
#   G1 (the crates' tests, the REDs, the lints, the type-checks of dll, apps and both phone shells):
#     MOBILE_APPS_ONLY=css_lib,core_lib,layout_lib,layout_tests,layout_fluent,phone_tests,red_layout_on_the_previous_files,red_ribbon_without_the_file_row,clippy,dll_check,apps_check,android_check,ios_cross_check
#   G2 (what builds the C ABI library: the headless-shell tests with the RED of the safe-area guard, the apps over the library, autofix):
#     MOBILE_APPS_ONLY=red_safe_area_without_the_guard,green_dll_tests,cabi_build,cabi_exports,appkit_tests,azpaint_tests,azmaps_tests,azmeet_tests,azwriter_tests,azwidgets_tests,autofix
# Needs the bindings (remote.sh --codegen) for every step that builds azul-dll. A step that fails does not stop the ones after
# it, so one run says everything that is wrong; the last lines are one `STEP <name> <exit>` each and the exit code of the script
# is 0 only when every step passed. The two RED steps take the fix away again (the guard of the safe-area fix; the files the
# layout tests of the apps and the themes read, as they were on the base) and must make their tests FAIL, by name: that proves
# the tests see the bugs. They pass (exit 0) when the tests failed as they must.
set -uo pipefail
names=()
codes=()
only="${MOBILE_APPS_ONLY:-}"

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

# The tests that must FAIL in `out` (a cargo test log), each by its path; returns the number of those that did not.
must_fail() {
  local out="$1"
  shift
  local missing=0 name
  grep -E '^test .* \.\.\. (ok|FAILED)$' "$out" || true
  for name in "$@"; do
    if grep -q -E "^test ${name} \.\.\. FAILED$" "$out"; then
      echo "RED ok: ${name} fails"
    else
      echo "RED MISSING: ${name} did not fail"
      missing=$((missing + 1))
    fi
  done
  return "$missing"
}

# 1. the engine crates' own tests IN FULL (the safe area, the touch floor, the localizer slot and the ribbon touch every
#    widget and every window: a filtered run would not see what they broke), then the tests of the phone work by name
run css_lib cargo test -p azul-css --lib
run core_lib cargo test -p azul-core --lib
run layout_lib cargo test -p azul-layout --lib
run layout_tests cargo test -p azul-layout --test all
run layout_fluent cargo test -p azul-layout --lib --features fluent -- fluent
run phone_tests cargo test -p azul-layout --test all -- safe_area_inset azul_widgets_video azul_widgets_mail azul_maps_sidebar azul_maps_drive azul_paint_toolbar azul_writer_phone a_button_is_a_touch azul_widgets_toolbar

# 1b. RED of the layout tests of the phone work: with the themes of the engine and the files of the apps as they were on the
#     base commit (MOBILE_APPS_BASE, a commit of master before the fixes) the tests that see the bugs must FAIL (the touch floor
#     and the video card by their assertions; the sheet, the drive banner, the toolbar and the format bar because the files of
#     the base have no such style at all). The files are put back from HEAD after.
red_layout() {
  local base="${MOBILE_APPS_BASE:-}"
  [ -n "$base" ] || { echo "MOBILE_APPS_BASE is not set: no base commit to take the previous files from"; return 2; }
  git fetch -q --depth=1 origin "$base" || { echo "cannot fetch $base"; return 2; }
  local files=(layout/src/widgets/themes/decl.rs layout/src/widgets/themes/flat.rs layout/src/widgets/themes/flora.rs
               examples/azul-maps/src/lib.rs examples/azul-maps/src/drive_ui.rs examples/azul-widgets/src/video.rs
               examples/azul-widgets/src/mail.rs examples/azul-paint/src/lib.rs examples/azul-writer/src/ribbon.rs)
  git checkout -q "$base" -- "${files[@]}" || { echo "cannot take the previous files from $base"; return 2; }
  local out
  out="$(mktemp)"
  cargo test -p azul-layout --test all -- a_button_is_a_touch azul_widgets_video azul_widgets_mail azul_maps_sidebar azul_maps_drive azul_paint_toolbar azul_writer_phone > "$out" 2>&1
  git checkout -q HEAD -- "${files[@]}"
  must_fail "$out" \
    a_button_is_a_touch_target_on_a_phone::a_button_is_48_dp_in_each_direction_on_android_in_every_theme \
    a_button_is_a_touch_target_on_a_phone::a_button_is_44_pt_in_each_direction_on_ios_in_every_theme \
    a_button_is_a_touch_target_on_a_phone::a_text_field_is_as_tall_as_a_finger_on_a_phone_and_a_desktop_field_is_not \
    azul_widgets_video_card_fits_a_phone::no_node_of_the_video_card_crosses_the_right_edge_of_a_phone \
    azul_widgets_video_card_fits_a_phone::the_stage_is_as_wide_as_the_page_leaves_it_up_to_the_clips_480_and_keeps_16_to_9 \
    azul_widgets_mail_window_fits_a_phone::on_a_phone_the_three_panes_stand_one_over_the_other_inside_the_card \
    azul_maps_sidebar_is_a_sheet_on_a_phone::on_a_portrait_phone_the_sheet_leaves_the_map_visible_below_it \
    azul_maps_sidebar_is_a_sheet_on_a_phone::a_short_sidebar_is_as_tall_as_its_content_on_a_phone \
    azul_maps_drive_banner_fits_a_phone::on_a_phone_the_banner_and_the_bar_are_the_screen_less_8_px_each_side_and_no_node_crosses_an_edge \
    azul_maps_drive_banner_fits_a_phone::the_end_key_of_the_bar_is_on_the_screen_and_a_finger_wide \
    azul_paint_toolbar_fits_a_phone::the_six_keys_fit_the_narrowest_phone_and_are_48_dp_each \
    azul_writer_phone_bar_fits_a_phone::the_bar_is_the_width_of_the_screen_and_every_key_is_48_dp \
    azul_writer_phone_bar_fits_a_phone::the_first_keys_are_on_the_screen_without_scrolling
}
run red_layout_on_the_previous_files red_layout

# 1c. everything that compiles, type-checked without building a library (minutes, not an hour): the dll with its tests, the
#     apps with theirs (they link nothing in a check)
run dll_check cargo check -p azul-dll --features build-dll --lib --test headless_safe_area_moved --test localization_headless --test android_language_contract
run apps_check cargo check -p azul-appkit -p AzPaint -p AzMaps -p AzMeet -p AzWriter -p AzWidgets --lib --profile test --features azul-appkit/azul

# 1d. RED of the ribbon's tests: the picker without the application button's row and the group list's rows 44 px tall, as they were
red_ribbon() {
  local file=layout/src/widgets/ribbon.rs
  python3 - "$file" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
old_row = """            RibbonChromeMode::Adaptive | RibbonChromeMode::Mobile => app_button
                .clone()
                .into_option()
                .map(|ab| mobile_file_entry(ab, &style)),
"""
old_height = """        Cond::simple(P::const_height(LayoutHeight::const_px(48))),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(15))),
"""
assert text.count(old_row) == 1 and text.count(old_height) == 1, "the lines the RED step edits are gone"
text = text.replace(old_row, "            RibbonChromeMode::Adaptive | RibbonChromeMode::Mobile => None,\n")
text = text.replace(old_height, old_height.replace("const_px(48)", "const_px(44)"))
open(path, "w").write(text)
PY
  if git diff --quiet -- "$file"; then
    echo "the RED step could not edit $file"
    return 2
  fi
  local out
  out="$(mktemp)"
  cargo test -p azul-layout --lib -- ribbon_mobile_test > "$out" 2>&1
  git checkout -q -- "$file"
  must_fail "$out" \
    widgets::ribbon::ribbon_mobile_test::the_picker_lists_the_application_button_first_with_its_click \
    widgets::ribbon::ribbon_mobile_test::the_adaptive_ribbon_has_the_row_too \
    widgets::ribbon::ribbon_mobile_test::a_button_that_only_opens_a_menu_hangs_the_menu_on_its_row \
    widgets::ribbon::ribbon_mobile_test::the_rows_of_the_picker_and_of_the_group_list_are_48_px_tall
}
run red_ribbon_without_the_file_row red_ribbon

# 2. the engine through the headless shell: RED (the guard out: the precheck is always false, as it was), then GREEN
red_safe_area() {
  local file=dll/src/desktop/shell2/common/layout.rs
  python3 - "$file" <<'PY'
import sys
path = sys.argv[1]
text = open(path).read()
old = "let safe_area_moved_precheck = layout_window.safe_area_moved_since_layout(current_window_state);"
open(path, "w").write(text.replace(old, "let safe_area_moved_precheck = false;"))
PY
  if git diff --quiet -- "$file"; then
    echo "the RED step could not take the guard out of $file: the line it looks for is gone"
    return 2
  fi
  local out
  out="$(mktemp)"
  cargo test -p azul-dll --features build-dll --test headless_safe_area_moved > "$out" 2>&1
  git checkout -q -- "$file"
  must_fail "$out" \
    the_bars_that_arrive_after_the_first_frame_move_the_root_of_a_ui_that_never_changes \
    insets_that_moved_with_a_rotation_take_the_root_with_them
}
run red_safe_area_without_the_guard red_safe_area
run green_dll_tests cargo test -p azul-dll --features build-dll --test headless_safe_area_moved --test localization_headless --test android_language_contract

# 3. lints of the three engine crates, as the CI's second clippy step runs them
run clippy cargo clippy -p azul-core -p azul-css -p azul-layout --all-targets --features "$features" -- -D warnings

# 4. the apps over the C ABI, against the library of this tree (a copy of it in a folder of its own, as the CI's
#    lib tests do: they link-dynamic against that file)
run cabi_build cargo build -p azul-dll --features build-dll
if [ -z "$only" ] || [ "${only#*cabi_build}" != "$only" ]; then
  cabi_dir="$(mktemp -d)"
  cp target/debug/libazul.so "$cabi_dir/" 2>/dev/null
  export AZ_LINK_PATH="$cabi_dir"
fi
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
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: MOBILE_APPS_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

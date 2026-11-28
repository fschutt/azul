#!/usr/bin/env bash
# sidebar_gate.sh: the gate of the sidebar family (Sidebar, Inspector, ActionSheet, LocationBar, AppFrame, the pie marker's size) on one
# Ubuntu runner (or the box), every step with its REAL exit code.
#
#   BOX_BY=sidebar python3 box.py run <ref> 'SIDEBAR_ONLY=<step>,<step> bash scripts/ci/sidebar_gate.sh' [--codegen] \
#     --artifacts 'api.json api.diff'
#
# Steps (SIDEBAR_ONLY names them; none named = all of them):
#   tests      the tests of the family by name, in azul-layout, with the CI's feature list
#   clippy     the CI's clippy line of azul-layout, the test targets included
#   autofix    api.json through azul-doc: `autofix add` for the new functions, patches until there are none, normalize; the result is
#              api.json (upload it) and api.diff; then a SECOND round that must change nothing
#   doc_check  azul-doc check (the double-drop invariant, the guide links) and azul-doc check layout
#   dll_check  codegen of the bindings from the api.json as it stands, cargo check of azul-dll, the srcgate tests
#   widgets_check  the AzWidgets example type-checks over the C ABI (after the autofix step's api.json)
# A step that fails does not stop the ones after it; the last lines are one `STEP <name> <exit>` each, and the exit code of the script is 0
# only when every step that ran passed.
set -uo pipefail
names=()
codes=()
only="${SIDEBAR_ONLY:-}"

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
  AZ_REQUIRE_TEST_FONTS=1 cargo test -p azul-layout --lib --features "$features" --no-fail-fast -- \
    sidebar inspector location_bar action_sheet app_frame pie_marker ribbon tab_ tabs:: 2>&1 | tee sidebar-tests.txt
  return "${PIPESTATUS[0]}"
}
run tests tests

clippy() {
  cargo clippy -p azul-layout --all-targets --features "$features" -- -D warnings 2>&1 | tee clippy.txt
  return "${PIPESTATUS[0]}"
}
run clippy clippy

# api.json: `autofix add` stages the functions (a plain `autofix` syncs TYPES only), the patches are applied until the scan finds none,
# and the tree is normalized. The functions are the ones the work added to the API.
api_functions=(
  Sidebar.create
  Sidebar.add_section
  Sidebar.with_section
  Sidebar.set_sections
  Sidebar.with_sections
  Sidebar.add_status
  Sidebar.with_status
  Sidebar.set_state
  Sidebar.with_state
  Sidebar.set_surface
  Sidebar.with_surface
  Sidebar.set_form
  Sidebar.with_form
  Sidebar.set_label
  Sidebar.with_label
  Sidebar.set_header
  Sidebar.with_header
  Sidebar.set_help
  Sidebar.with_help
  Sidebar.set_expanded_width
  Sidebar.with_expanded_width
  Sidebar.set_window_controls
  Sidebar.with_window_controls
  Sidebar.set_handle_in_cycle
  Sidebar.with_handle_in_cycle
  Sidebar.set_theme
  Sidebar.with_theme
  Sidebar.set_on_event
  Sidebar.with_on_event
  Sidebar.swap_with_default
  Sidebar.column
  Sidebar.toggled_state
  Sidebar.dom
  SidebarSection.create
  SidebarSection.add_row
  SidebarSection.with_row
  SidebarSection.set_rows
  SidebarSection.with_rows
  SidebarSection.set_collapsible
  SidebarSection.with_collapsible
  SidebarRow.create
  SidebarRow.set_icon
  SidebarRow.with_icon
  SidebarRow.set_tint
  SidebarRow.with_tint
  SidebarRow.set_badge
  SidebarRow.with_badge
  SidebarRow.set_tag
  SidebarRow.with_tag
  SidebarRow.set_depth
  SidebarRow.with_depth
  SidebarRow.set_disclosure
  SidebarRow.with_disclosure
  SidebarRow.set_selected
  SidebarRow.with_selected
  SidebarRow.set_mark
  SidebarRow.with_mark
  SidebarRow.set_in_rail
  SidebarRow.with_in_rail
  SidebarRow.set_tooltip
  SidebarRow.with_tooltip
  SidebarRow.is_rail_button
  SidebarStatus.create
  SidebarStatus.set_text
  SidebarStatus.with_text
  SidebarStatus.set_detail
  SidebarStatus.with_detail
  SidebarStatus.set_popover
  SidebarStatus.with_popover
  FormFactor.is_phone
  FormFactor.resolved
  SidebarState.toggled
  SidebarState.from_handle
  InspectorDock.resolved
  SidebarColumnWidth.in_frame
  Inspector.create
  Inspector.set_title
  Inspector.with_title
  Inspector.set_header_end
  Inspector.with_header_end
  Inspector.set_dock
  Inspector.with_dock
  Inspector.set_form
  Inspector.with_form
  Inspector.set_size
  Inspector.with_size
  Inspector.set_open
  Inspector.with_open
  Inspector.set_surface
  Inspector.with_surface
  Inspector.set_theme
  Inspector.with_theme
  Inspector.set_on_event
  Inspector.with_on_event
  Inspector.resolved_dock
  Inspector.built_size
  Inspector.dom
  LocationBar.create
  LocationBar.add_parent
  LocationBar.with_parent
  LocationBar.set_parents
  LocationBar.with_parents
  LocationBar.set_editable
  LocationBar.with_editable
  LocationBar.set_editing
  LocationBar.with_editing
  LocationBar.set_placeholder
  LocationBar.with_placeholder
  LocationBar.set_separator
  LocationBar.with_separator
  LocationBar.set_form
  LocationBar.with_form
  LocationBar.set_theme
  LocationBar.with_theme
  LocationBar.set_on_event
  LocationBar.with_on_event
  LocationBar.dom
  LocationCrumb.create
  ActionSheet.create
  ActionSheet.set_title
  ActionSheet.with_title
  ActionSheet.add_item
  ActionSheet.with_item
  ActionSheet.set_open
  ActionSheet.with_open
  ActionSheet.set_form
  ActionSheet.with_form
  ActionSheet.set_surface
  ActionSheet.with_surface
  ActionSheet.set_theme
  ActionSheet.with_theme
  ActionSheet.set_on_event
  ActionSheet.with_on_event
  ActionSheet.shows
  ActionSheet.dom
  ActionSheetItem.create
  ActionSheetItem.set_danger
  ActionSheetItem.with_danger
  ActionSheetItem.set_disabled_reason
  ActionSheetItem.with_disabled_reason
  AppFrame.create
  AppFrame.set_sidebar
  AppFrame.with_sidebar
  AppFrame.set_inspector
  AppFrame.with_inspector
  AppFrame.set_footer
  AppFrame.with_footer
  AppFrame.set_sheet
  AppFrame.with_sheet
  AppFrame.set_form
  AppFrame.with_form
  AppFrame.dom
  PieMarker.set_size
  PieMarker.with_size
  SidebarRow.set_pie
  SidebarRow.with_pie
  SidebarRow.set_collab
  SidebarRow.with_collab
  SidebarRow.has_pie
  TabHeader.set_labels
  TabHeader.with_labels
  TabHeader.set_icons
  TabHeader.with_icons
  Ribbon.set_labels
  Ribbon.with_labels
  Ribbon.set_button_face
  Ribbon.with_button_face
  RibbonTab.set_location
  RibbonTab.with_location
  RibbonTab.set_icon
  RibbonTab.with_icon
  RibbonAppButton.set_icon
  RibbonAppButton.with_icon
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

widgets_check() {
  cargo run -r -p azul-doc -- codegen all || return 1
  cargo check -p AzWidgets --lib --tests
}
run widgets_check widgets_check

echo
bad=0
if [ "${#names[@]}" -eq 0 ]; then
  echo "no step ran: SIDEBAR_ONLY=$only names none"
  exit 1
fi
for i in "${!names[@]}"; do
  echo "STEP ${names[$i]} ${codes[$i]}"
  [ "${codes[$i]}" -eq 0 ] || bad=1
done
exit "$bad"

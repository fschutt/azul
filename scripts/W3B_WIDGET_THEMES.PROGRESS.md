# W3b widget themes - progress (checkpoint file)

Branch `wt/w3b-widget-themes` (cut from `fix/input-bugs-2026-09-19` @ 9c6065b05),
worktree `.claude/worktrees/agent-a89d60761e1015fb9`. No compilation (parent integrates).

Widgets: modal, dialog, number_input, pagination, popover, radio_group, segmented,
split_pane, stepper, time_picker, toast, tooltip, video; then decide camera,
microphone, screencap, map, node_graph.

## Approach (decided)
- Widget keeps DOM structure, callbacks and a11y; its builder takes a "skin" (the
  per-part styles) that `themes::flat` / `themes::flora` supply. `dom()` dispatches
  `Flora => flora::<widget>(self)`, `Flat | None => flat::<widget>(self)`.
- Flat = the widget's established look (unchanged light values, system-palette dark
  twins) plus whatever was missing (dark twins, focus ring). Flora = flora.css tokens
  (`flora::LIGHT_*` + `flora::DARK_*` twins), raised-paper faces, sunken accent stone
  for the selected item, 3px house radius, accent focus ring (DARK: glow).
- Root carries `__azul-theme-flat` / `__azul-theme-flora` (as Button does); click
  handlers that live-restyle read it back (`style_kit::theme_of_classes`) to pick colours.
- Shared: `themes/style_kit.rs` (pair builders, marker), test helpers
  `themes/theme_checks.rs` (`#[cfg(test)]`). Theme field always APPENDED LAST.

## DONE
- dialog + modal + popover (shared builder): RED 113b016c4, impl 898b93cf6.
  API: Dialog.theme / Modal.theme / Popover.theme (OptionUiTheme, appended LAST),
  set_theme / with_theme on all three.
- number_input: RED 92e74f261, impl 9a8a33c0c. API: NumberInput.theme (last),
  set_theme / with_theme.
- pagination: RED e8ef3e2e7, impl 817a22b6a. API: Pagination.theme (last),
  set_theme / with_theme. Click restyle reads the marker (PaginationSkin.restyle).
- radio_group: RED eb21dd0d1, impl be8d8ae0c. API: RadioGroup.theme (last),
  set_theme / with_theme.
- segmented: RED 3271883b5, impl ec82375ee. API: Segmented.theme (last),
  set_theme / with_theme. Selection restyle reads the marker.
- radio_group class pin fix: 0e8f49695 (root classes now include the marker).
- split_pane: RED ae8c11e8a, impl 7a1b01231. API: SplitPane.theme (last),
  set_theme / with_theme.
- stepper: RED 65b9dd8da, impl d345a46c9. API: Stepper.theme (last),
  set_theme / with_theme. Click restyle reads the marker.
- time_picker: RED 6b6097192, impl e76ea579e. API: TimePicker.theme (last),
  set_theme / with_theme.

- toast: RED d95b9bdb7, impl dc4e75e66. API: Toast.theme (last),
  set_theme / with_theme.

- tooltip: RED 89d72092d, impl e686346fd. API: Tooltip.theme (last),
  set_theme / with_theme.

## IN PROGRESS
- video

## NEXT
video -> decisions (camera/mic/screencap/map/node_graph) -> report

## Open questions
(none yet)

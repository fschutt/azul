# WIDGETS9A progress - IconGrid, Toolbar, TokenInput (wave 9, 2026-10-03)

Branch `wt/widgets9a` from `e537ddbe2`. Nothing compiled (house rule). Report: `scripts/WIDGETS9A_2026_10_03.md`.

## DONE (commits)
- (this file)

## IN PROGRESS
- Toolbar: `layout/src/widgets/toolbar.rs`

## NEXT
1. Toolbar: skeleton (types, builder, callback triple) -> RED tests -> GREEN (fit, build, handlers) -> theme appends
   (`toolbar_look` in flat.rs / flora.rs under `// ==== toolbar ====`) -> manifest (mod.rs `pub mod toolbar;`,
   `every_widget_dom` push, theme_contrast CHROME) -> commit each.
2. TokenInput: `layout/src/widgets/token_input.rs`, same steps; manifest group INPUTS.
3. IconGrid: `layout/src/widgets/icon_grid.rs`, same steps; manifest CHROME + wheel_ownership list.
4. Report with the api.json list.

## Design decisions (unattended)
- Pattern for all three: the newest widgets' (ThumbnailStrip / DataTable / Chart): a `XxxLook` of parts, each part =
  base (structure, in the widget file) + skin (theme file); `look_for(theme)` merges the two looks part by part with
  `theme_blocks::follow_props` (DOM built once); one `on_event` callback triple (`impl_widget_callback!` +
  `impl_managed_callback!`), events carry what the app stores.
- Toolbar: each tool is the existing `Button` widget with the toolbar's parts handed in as its styles (like the
  ribbon's `styled_button`; that helper pins a `UiTheme`, the toolbar may follow - near twin, noted). Items:
  Button, Toggle, MenuButton (drop-down of choices), Separator, Spacer, Custom (an app Dom, e.g. a search field,
  with a declared width). Overflow: the app passes `available_width` (like DataTable's viewport); a pure `fit()`
  estimates item widths (icon 20 px, label chars x font x 0.6, padding) and moves trailing items (never
  `never_overflow` ones) into a "more" (`more_horiz`) button's menu. APG toolbar keys: one Tab stop (roving),
  Left / Right (wrap) / Home / End; Enter / Space are the Button's activation; Down opens a menu button. Custom
  items keep their own Tab stops (outside the roving group). Icon-only tools: name via Button `alt`, tooltip on
  hover through button.rs's tooltip handlers (made `pub(crate)`).
- TokenInput: chips = the `Chip` widget (removable), entry = the `TextInput` widget (its on_text_input /
  on_virtual_key_down hooks), suggestions = an in-DOM list under the field (absolute), filtered at build from
  `suggestions` by the typed text. App-owned state `TokenInputState { tokens, text, active }`, every event carries
  the next state (DataTable's view pattern); the app stores it and rebuilds. Separators `,` `;` (and Enter / Tab)
  commit; a paste with a separator commits every part; Backspace on an empty entry removes the last chip;
  validation by an optional `on_validate` callback (accept with a normalised token / refuse with a reason).
- IconGrid: DataTable's scroll window (whole rows, `top_row`, own scroll bar, the wheel moves rows), items from a
  DATA callback (only the items in view are asked), selection = `ListSelection` over item indices, rubber band on
  empty space (pointer capture), Explorer press rules (a press on a selected item keeps the selection until the
  release, so the selection can be dragged), drag out through the engine's DnD (`DragStart` on an item: the grid
  sets its own MIME with the indices and reports `DragStart` so the app adds its payload), double-click / Enter =
  Activate, right click / Shift+F10 / Menu key = ContextMenu, async thumbnails = the item's `image` the data
  callback answers once the app has it (the app rebuilds).

## Open questions
- none yet

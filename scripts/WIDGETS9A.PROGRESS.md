# WIDGETS9A progress - IconGrid, Toolbar, TokenInput (wave 9, 2026-10-03)

Branch `wt/widgets9a` from `e537ddbe2`. Nothing compiled (house rule). Report: `scripts/WIDGETS9A_2026_10_03.md`.
Parse check: `sh /tmp/w9a_parse.sh <files>` (rustfmt --emit stdout; recreate the 6-line script if /tmp was wiped).
Commit messages via `printf ... > /tmp/w9a_msg.txt && git commit -F /tmp/w9a_msg.txt` (heredocs are refused).

## DONE (commits)
- 75c86029c progress file
- Toolbar (`layout/src/widgets/toolbar.rs`): 288e31797 skeleton, 49bf1cad7 RED tests, 7a8badc52 GREEN fit (+ button.rs
  tooltip handlers `show_disabled_reason` / `hide_disabled_reason` made pub(crate)), b433a4229 GREEN build / handlers
  / menus, 8e0794816 flat/flora looks, ab1b3a910 manifest (`toolbar`, `toolbar (overflow)`: every_widget_dom +
  theme_contrast CHROME)
- TokenInput (`layout/src/widgets/token_input.rs`): abd8bf85c skeleton, b8bbd71c2 RED, 87ee03dbb rules GREEN,
  98dc8aeb1 build + handlers GREEN, c7723ab15 flat/flora looks, 454feb519 manifest (`token_input`: every_widget_dom +
  theme_contrast INPUTS)

## IN PROGRESS
- IconGrid: a DONE (20a1edf3d skeleton), b DONE (2bb488537 RED), c PARTLY: db91378c9 geometry / hit_test / item_rect
  / marquee_keys / scroll_by GREEN (+ data_table `thumb()` pub(crate)), ff81da335 press / drag_move / drag_end /
  grid_key GREEN, b23d7f92c build + handlers + look_for GREEN, 1e0cc4a13 looks (d), 75c5ce86f manifest (e: every_widget_dom,
  CHROME, wheel_ownership). NEXT: f - the report `scripts/WIDGETS9A_2026_10_03.md` (template CHART7's), then a
  read-through of the three files for compile errors.

## NEXT (exact) - IconGrid in `layout/src/widgets/icon_grid.rs` (model: `data_table.rs`, `thumbnail_strip.rs`)
a. Skeleton + `pub mod icon_grid;` APPENDED after `pub mod token_input;` in widgets/mod.rs. Types (all repr C):
   - `IconGridItem { label, name (a11y, empty = label), icon (glyph name), badge (glyph), image: OptionImageRef }`
     `create(label, icon)`, `with_image`, `with_badge`, `with_name`; `impl HostOut` (unwritten = empty item).
   - DATA callback `IconGridDataSourceCallbackType = extern "C" fn(RefAny, usize) -> IconGridItem` (impl_widget_callback
     + impl_managed_callback Form 4 with `ctx_field: ctx, data: data: RefAny, args: [index: usize]` exactly like
     `DataTableDataSource`).
   - `IconGridDragKind { None, Pending, Marquee, Thumb }`, `IconGridDrag { start_x, start_y, x, y, start_top: f32,
     index: usize, kind }` (Copy), `IconGridView { selection: ListSelection, drag: IconGridDrag, top_row: usize }`.
   - `IconGridEventKind { Select, Scroll, Activate, ContextMenu, DragStart, Drag }`, `IconGridEvent { view, index:
     OptionUsize, x, y: f32, kind, shift, ctrl }`; on_event triple.
   - `IconGrid { view, data_source, on_event, accessibility_name, id (node id, default "icon-grid"), count: usize,
     viewport_width, viewport_height, cell_width (96), cell_height (104), icon_size (48), theme }`.
b. RED tests: geometry (columns = floor(body / cell), rows, page_rows, max_top, the scroll bar only when rows overflow),
   hit_test (item / empty / track / thumb), press (plain / ctrl / shift; a press on a selected item -> Pending, the
   release collapses), marquee (keys of the cells the rect crosses, ctrl adds), keys (arrows by 1 / by a row, Page,
   Home / End, Shift extends, Ctrl+A, Enter -> Activate, Apps / Shift+F10 -> ContextMenu, Escape clears; the focused
   row revealed by top_row), wheel rows (cell_grid::take_wheel), DOM (only the items in view are asked for and built,
   item classes selected / focused, role List + Multiselectable, items ListItem + Selected, the image or the icon
   glyph, draggable items, DragStart sets the MIME `application/x-azul-icon-grid` with the indices), follows the
   app theme.
c. GREEN: pure fns `geometry`, `hit_test`, `item_rect`, `press`, `drag_move`, `drag_end`, `grid_key`, `scroll_by`,
   `marquee_keys`; build (grid div: relative, overflow hidden, viewport size, ONE Tab stop, handlers on the grid node
   like DataTable's `table_callbacks`: VirtualKeyDown, LeftMouseDown, MouseMove, MouseUp, DoubleClick, RightMouseUp,
   Scroll; items absolute at their cells with DragStart; the marquee rect; the scroll bar track + thumb - reuse
   `data_table::ScrollBar` + make data_table's `thumb()` pub(crate) instead of a twin).
d. Theme appends `// ==== icon_grid ====` (`icon_grid_look()`: grid fill, item hover / selected / focused ring,
   label ink, icon ink, marquee wash + edge, track / thumb) in flat.rs / flora.rs.
e. Manifest: `icon_grid::fixtures::sample()` in every_widget_dom, theme_contrast CHROME, and APPEND "icon_grid" to
   the `wheel_ownership` expected list (it scrolls by whole rows like the data table).
f. Report `scripts/WIDGETS9A_2026_10_03.md` (template: scripts/CHART7_2026_10_03.md): built, commits, api.json list
   for all three widgets, least-sure-to-compile, test commands, engine gap, left.

## Engine gap seen (for the report)
- `:focus-within` is parsed (PseudoStateType::FocusWithin) but never raised: `StyledNodeState::focus_within` is
  never set (no restyle on focus change, not in prop_cache's tiers, not in apply_runtime_states_before_layout).
  The token field therefore rings its ENTRY on focus (look.entry), not the whole field.

## Design decisions (unattended)
- Pattern for all three: the newest widgets' (ThumbnailStrip / DataTable / Chart): a `XxxLook` of parts, each part =
  base (structure, in the widget file) + skin (theme file); `look_for(theme)` merges the two looks part by part with
  `theme_blocks::follow_props` (DOM built once); one `on_event` callback triple (`impl_widget_callback!` +
  `impl_managed_callback!`), events carry what the app stores.
- Toolbar: each tool is the existing `Button` widget with the toolbar's parts handed in as its styles (like the
  ribbon's `styled_button`; that helper pins a `UiTheme`, the toolbar may follow - near twin, noted). Items:
  Button, Toggle, MenuButton (drop-down of choices), Separator, Spacer, Custom (an app Dom, e.g. a search field,
  with a declared width). Overflow: the app passes `available_width` (like DataTable's viewport); a pure `fit()`
  estimates item widths (icon 20 px, label chars x 13 px x 0.6, padding) and moves trailing items (never
  `never_overflow` ones) into a "more" (`more_horiz`) button's menu. APG toolbar keys: one Tab stop (roving),
  Left / Right (wrap) / Home / End; Enter / Space are the Button's activation; Down opens a menu button. Custom
  items keep their own Tab stops (outside the roving group). Icon-only tools: name via Button `alt`, tooltip on
  hover through button.rs's tooltip handlers (made `pub(crate)`). Menus open under the tool
  (`open_menu_for_hit_node`, falling back to `open_menu` when the tool has no rect).
- TokenInput: chips = the `Chip` widget (removable; label opens -> `Open`), entry = the `TextInput` widget (its
  on_text_input / on_virtual_key_down hooks), suggestions = an in-DOM list floating under the field (absolute,
  top 100%, z-index 10), filtered at build. App-owned state `TokenInputState { tokens, text, active }`, every event
  carries the next state computed from the BUILT state (no state is stored between events: the app rebuilds after
  each). Separators `,` `;` tab / newline (and Enter / Tab) commit; a paste with a separator commits every part;
  Backspace on an empty entry removes the last chip; chips are out of the Tab order (NoKeyboardFocus), Left from
  an empty entry walks them; validation by `on_validate` (accept, optionally normalised / refuse with a reason:
  the refused text stays typed, `Refuse` event).
- IconGrid: DataTable's scroll window (see NEXT).

## Open questions
- none

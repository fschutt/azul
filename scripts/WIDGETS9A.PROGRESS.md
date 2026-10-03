# WIDGETS9A progress - IconGrid, Toolbar, TokenInput (wave 9, 2026-10-03)

Branch `wt/widgets9a` from `e537ddbe2`. Nothing compiled (house rule). Report: `scripts/WIDGETS9A_2026_10_03.md`.

## DONE (commits)
- 75c86029c progress file
- Toolbar (`layout/src/widgets/toolbar.rs`): 288e31797 skeleton, 49bf1cad7 RED tests, 7a8badc52 GREEN fit (+ button.rs
  tooltip handlers pub(crate)), b433a4229 GREEN build/handlers/menus, 8e0794816 flat/flora looks, ab1b3a910 manifest
  (`toolbar`, `toolbar (overflow)` in every_widget_dom + theme_contrast CHROME)

## IN PROGRESS
- TokenInput: `layout/src/widgets/token_input.rs` - skeleton abd8bf85c, RED tests b8bbd71c2, rules GREEN 87ee03dbb.
  build + handlers GREEN 98dc8aeb1. Steps 2a and 2b DONE; continue at 2c (theme appends `token_input_look()` in
  flat.rs / flora.rs - `look_for` already calls them), then 2d (fixtures + manifest).

## NEXT (exact)
2. TokenInput, in `layout/src/widgets/token_input.rs`:
   a. RED: append `#[cfg(test)] mod token_input_tests` (model on `toolbar.rs`'s tests): split_tokens ("a, b; c" ->
      ["a","b"] + "c"), add_tokens (trim, case-folded dedupe, allow_duplicates), remove_token, matching_suggestions
      (prefix first, then contains, no existing token, cap, empty text -> none), step_active (wraps), entry_key table
      (Enter -> CommitSuggestion(active) / CommitText, Tab with text -> CommitText, Back on empty -> RemoveLast,
      Down/Up -> Navigate, Escape -> Dismiss, Left on empty -> ToChips, modified -> Pass); DOM: root class + Grouping
      + name, field holds one chip (CHIP_CLASS) per token whose "x" is NoKeyboardFocus, the entry (ENTRY_CLASS) the one
      Tab stop, list (LIST_CLASS, role List) only when text matches, active option OPTION_ACTIVE_CLASS + Selected;
      chip "x" click -> Remove event; suggestion click -> Add; key Back on the entry (empty) -> Remove last;
      follows the app theme. Commit RED.
   b. GREEN: implement the stubs; `build`: root (position relative, column) > [field (flex row wrap) > chips (Chip
      removable, on_remove/on_click -> TokenData{index, shared}; x set NoKeyboardFocus + VirtualKeyDown handler) +
      entry (TextInput::create().with_text(state.text).with_placeholder.with_accessibility_name + container style =
      TextInput's default + overrides (no border, transparent, flex-grow 1, min-width 80) + look.entry; hooks
      `with_on_text_input(shared, on_entry_text)` (separator typed/pasted -> validate -> Add/Refuse, veto with
      valid No + TextInput::set_text_in) and `with_on_virtual_key_down(shared, on_entry_key)` (entry_key)), list
      (absolute, top 100%, z-index) > options (Click -> commit, focus back to the entry)]. Shared:
      `TokenShared { on_event, on_validate, state, shown: Vec<AzString>, allow_duplicates }`.
   c. Theme appends `// ==== token_input ====` in flat.rs / flora.rs: `token_input_look()` (flat field LIGHT_FLD +
      system_palette::DARK_CONTROL_BACKGROUND, flora field LIGHT_FLD / DARK_SUR - the entry's dark fills).
   d. Manifest: `token_input::fixtures::sample()` (tokens + text "al" + suggestions) in every_widget_dom, group INPUTS.
3. IconGrid (see design below), then the report `scripts/WIDGETS9A_2026_10_03.md`.

## Engine gap seen (for the report)
- `:focus-within` is parsed (PseudoStateType::FocusWithin) but never raised: `StyledNodeState::focus_within` is
  never set (no restyle on focus change, not in prop_cache's tiers, not in apply_runtime_states_before_layout).
  The token field therefore rings its ENTRY on focus (look.entry), not the whole field.
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

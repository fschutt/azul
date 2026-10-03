# R5-D: widgets become a BASE plus per-theme SKINS - report (2026-09-29)

Branch `wt/r5d-base-and-skins`, from `d240a1b1d`. Widgets: statusbar, stepper, switch, tabs, text_area,
text_input, time_picker, titlebar, toast, tooltip, tree_view, video. Nothing was compiled (house rule);
the parent compiles and runs the suites.

## What was built

- One RED test per widget, `a_<widget>_declares_its_structure_once_for_every_theme`: the widget built
  WITHOUT a pin under each app theme (`under(t, ..)` for `t in BOTH`), over its main states, through
  `theme_checks::assert_structure_is_shared`.
- Each widget's structure (the `STRUCTURE_PROPERTIES`: display, position, box-sizing, flex-*, align-*,
  overflow, cursor, user-select, white-space) is now declared ONCE, as a base in the widget's own file.
  Both theme builders lay it FIRST and their skin after it, so the merge shares it: it is declared outside
  every `@theme` block and holds under flat, flora and any future theme. `flat.rs` / `flora.rs` keep only
  the skin (paint and metrics) of these widgets; no structure declaration of an R5-D widget appears in both
  theme files any more.
- Where the widget's placement or behaviour is the same in every theme and tied to the structure, it went
  into the base as well: the toast's insets (bottom / right `TOAST_INSET` with its `position: absolute`),
  the tooltip tip's insets and its hidden start (`opacity: 0`, the value the leave handler writes back).
- One new helper, `themes::flat::on_base(base, skin)` (private, APPENDED at the end of flat.rs under
  `// ==== R5-D: a flat part is the widget's base, then flat's skin ====`): the concatenation flat uses
  to lay a widget's base under its const skins (tabs, time_picker, toast, tooltip, tree_view). Flora's
  builders start from `BASE.to_vec()` directly. Nothing was added to `decl.rs` / `style_kit.rs`.
- `stack_parts` (the parent's new helper, not in this base): R5-D stacks no MERGED parts. Every base +
  skin composition happens inside one theme's builder, on plain (unthemed) declarations, before
  `follow_props` / `follow_dom` makes the `@theme` blocks, so the cascade's rank of a themed block over an
  unthemed declaration never meets a stacked part here. statusbar's `merged_style` was not touched.

## Per widget

| widget | structure moved to base | accidental differences unified (old flat / flora -> new) | allowed real differences |
|---|---|---|---|
| statusbar | none needed: flora repaints the flat part's geometry (`chrome_part` keeps every non-paint declaration, in flat's order), so the structure is already authored once, in statusbar.rs | - | - |
| stepper | `CIRCLE_BASE` (display flex, row, justify/align center, flex-grow 0, box-sizing border-box, user-select none, cursor pointer), `CONNECTOR_BASE` (flex-grow 1), `LABEL_BASE` (user-select none, cursor pointer); the cell already had one (`STEPPER_STEP_STYLE`) | circle `box-sizing`: none / border-box -> border-box | - |
| switch | none needed: flat's and flora's switch are one look (`build_track_style` / `build_knob_style`) | - | - |
| tabs | `HEADER_BASE` (display flex, row), `AFTER_BASE` (flex-grow 1), `TAB_BASE` for every tab (box-sizing content-box, align-items center, cursor pointer), `PANEL_BASE` (flex-grow 1) | tab `cursor`: none / pointer -> pointer; tab `align-items`: center / none -> center; tab `box-sizing`: content-box on flat's ACTIVE tab only / none -> content-box on every tab | `.__azul-native-tabs-header` `align-items`: flora stands its tabs on the strip's rule (`end`), flat's native tabs hang from the top of the bar. `.__azul-native-tabs-before-tabs` `flex-grow`: flat's leading spacer grows (1), flora's tabs start a fixed 8px in (0) |
| text_area | none needed: both builders start from the widget's resolvers (`resolved_container_style`, `TEXT_AREA_LABEL_PROPS`) and add paint only | - | - |
| text_input | `SEARCH_FIELD_BASE` (the search row: display flex, row, align-items center, flex-grow 1 - flat's and flora's `search_field` were twins), `search_clear_base(visible)` (display `SEARCH_CLEAR_SHOWN` / none, flex-grow 0, cursor pointer, justify/align center) | clear button shown `display`: block / flex -> flex; clear button `justify-content` / `align-items`: none / center -> center; the live show on the first character (`sync_live_looks`): block in every theme -> `SEARCH_CLEAR_SHOWN` (flex) | - |
| time_picker | `CONTAINER_BASE` (display flex, row, align-items center, align-self start, flex-grow 0), `CLICKABLE_BASE` for the arrows and AM/PM (cursor pointer, user-select none), `READOUT_BASE` for the value and the `:` (user-select none); the spinner column's `SPINNER_STYLE` is its whole style in both themes | none (flora restated flat's structure alike) | - |
| titlebar | none needed: the bar's and the title's metrics are the widget's (`container_style_painted` / `title_style_painted`), the looks give paint only | - | - |
| toast | `TOAST_CARD_BASE` (display flex, row, align-items start, flex-grow 0, position absolute, bottom / right inset), `TOAST_CLOSE_BASE` (flex-grow 0, cursor pointer, user-select none); the message's `TOAST_MESSAGE_STYLE` is its whole style in both | none | - |
| tooltip | `TIP_BASE` (position absolute, top `TIP_OFFSET_Y`, left 0, white-space nowrap, opacity 0); the wrapper's `TOOLTIP_WRAPPER_STYLE` is its whole style in both | none | - |
| tree_view | `TREE_CONTAINER_BASE` (overflow-y auto, display flex, column), `ROW_BASE` (display flex, row, align-items center, cursor pointer), `CHILDREN_BASE` (display flex, column), `ICON_BASE` (flex-grow 0), `LABEL_BASE` (flex-grow 1); the leaf spacer's `LEAF_SPACER_STYLE` is its whole style in both | none | - |
| video | none needed: neither the widget's nodes nor the poster declare structure in a theme block | - | - |

Every widget in the list follows the app theme (none was skipped).

## Visible changes (none intended; what unifying an accidental difference shows)

- Flat's TABS now show the pointer cursor, like flora's (a tab is clicked).
- Flora's SEARCH clear badge keeps its centred cross after the first keystroke: the live show used to
  write `display: block` in every theme, which dropped the badge's flex centring until the next rebuild.
  Flat's clear glyph is now a flex box (`display: flex`, centred) instead of a block: one glyph with 6px
  air either side, so it lays out the same.
- No box changes size: flat's stepper circle takes `box-sizing: border-box` (it has no border or
  padding), flora's tabs take `align-items: center` (a block `<p>` - no effect) and every tab takes
  `box-sizing: content-box` (the default).
- Everywhere else, pinned widgets resolve to the same values in every mode and state; only the
  declaration ORDER moved (the base first).

## Commits

| commit | what |
|---|---|
| c19e2ef1d | RED: the twelve `..._declares_its_structure_once_for_every_theme` tests |
| cfd4b8836 | GREEN stepper |
| d70ed0c15 | GREEN tabs (+ the two allowed differences) |
| 55902f260 | RED text_input: `the_clear_button_shows_with_the_display_a_filled_field_builds_it_with` |
| 59a6d3ada | GREEN text_input |
| 889b90705 | GREEN time_picker (+ `flat::on_base`) |
| 92b516d0b | GREEN toast |
| e2c27bddf | GREEN tooltip |
| daa25f33a | GREEN tree_view |
| 5c415982c, 2d8b75e8d, 9e434c52d, 652b4d3a9, ba3b359cc, be71db790, dcf8801a1, 628b8d45f | progress checkpoints |

## Tests changed because they asserted an order or a const incidentally

- stepper: `circle_style_declares_the_same_property_set_for_both_states` (18 -> 19 declarations).
- tabs: `dom_at_usize_max_gives_every_tab_the_plain_inactive_style`,
  `dom_is_a_header_div_wrapping_spacer_tabs_spacer`, `dom_pairs_every_tab_style_with_the_classes_it_advertises`
  (via `style_for_classes`), `content_dom_picks_the_style_vec_the_padding_flag_asks_for`: compare with
  `flat::tab_header_look()` / `flat::tab_content_look()` instead of the bare `CSS_MATCH_*` const.
- time_picker: `create_uses_the_shared_const_container_style` (`CONTAINER_BASE` then `CONTAINER_STYLE`).
- toast: `dom_children_carry_exactly_the_static_child_styles` (`TOAST_CLOSE_BASE` then `TOAST_CLOSE_STYLE`).
- tooltip: `new_uses_the_static_style_tables`, `tip_style_starts_hidden_with_exactly_one_opacity_declaration`,
  `tip_style_is_absolutely_positioned_and_does_not_wrap` (reads `TIP_BASE`),
  `neither_style_table_declares_a_property_type_twice`, `both_style_tables_apply_unconditionally`,
  `dom_builds_a_wrapper_with_the_anchor_then_the_tip`, `leave_restores_the_opacity_declared_in_the_static_tip_style`
  (reads `TIP_BASE`).
- tree_view: the `style_is` checks of `dom_root_carries_the_container_class_and_style`,
  `dom_leaf_renders_a_spacer_and_no_icon`, `dom_expanded_parent_uses_expand_more_and_emits_a_container`,
  `dom_selected_rows_use_the_selected_style` (base + static via `flat_part`; `style_is` takes `&[..]`).

The behaviours those tests pin (hidden start, one opacity, no duplicate property, the seam styles, the
padding flag, the 32-declaration toast card) are unchanged.

## api.json

No public API change: every new item (`*_BASE` statics, `search_clear_base`, `SEARCH_CLEAR_SHOWN`,
`flat::on_base`) is `pub(crate)` or private. Nothing crosses the FFI.

## Least sure to compile

- `layout/src/widgets/themes/flat.rs` `tab_header_look` / `tab_content_look`: `on_base(.., t::CSS_MATCH_x.as_slice())`
  borrows a temporary of a `const CssPropertyWithConditionsVec` for the call (should be fine: the result
  is owned).
- `v.extend([..])` with array literals (by-value `IntoIterator` for arrays) in stepper.rs, toast.rs,
  flora.rs, flat.rs.
- The glob imports of tabs.rs / text_input.rs resolving `StyleCursor`, `LayoutBoxSizing`,
  `LayoutJustifyContent` (all already used in those files).
- tooltip tests: the `[("wrapper", TOOLTIP_WRAPPER_STYLE), ("tip", flat_tip.as_slice())]` array unifies a
  `&'static [T]` and a local `&[T]`.
- `type P` aliases removed where they became unused (flora `tab_content_look`, `tooltip_skin`); flora
  functions that still use `P` keep it.

## Twins found (reported, not merged)

- `themes::flat::switch` and `themes::flora::switch` are the same builder, line for line.
- `themes::flat::text_input` / `flora::text_input` and `flat::text_area` / `flora::text_area` are the same
  builders except for the dark paint they append.
- `flat::search_field` and `flora::search_field` are now one-liners over `text_input::SEARCH_FIELD_BASE`;
  the pair could become one builder in text_input.rs.

## Noticed, not changed

- Flat's tab bar CENTRES its tabs: both spacers grow (`before-tabs` `flex-grow: 1` next to `after-tabs`
  `flex-grow: 1`). Kept as a real difference (allowed); it may be a bug of the generated flat CSS.
- Flat's active tab const re-declares `height: 21px` after `23px` (last wins); already pinned by
  `tab_styles_redeclare_properties_and_therefore_depend_on_declaration_order`.
- `theme_checks.rs` needed no change.

## Test commands for the parent

```
cargo test --release -p azul-layout --lib declares_its_structure_once_for_every_theme
cargo test --release -p azul-layout --lib the_clear_button_shows_with_the_display_a_filled_field_builds_it_with
cargo test --release -p azul-layout --lib widgets::stepper
cargo test --release -p azul-layout --lib widgets::tabs
cargo test --release -p azul-layout --lib widgets::text_input
cargo test --release -p azul-layout --lib widgets::time_picker
cargo test --release -p azul-layout --lib widgets::toast
cargo test --release -p azul-layout --lib widgets::tooltip
cargo test --release -p azul-layout --lib widgets::tree_view
cargo test --release -p azul-layout --lib widgets::statusbar
cargo test --release -p azul-layout --lib widgets::switch
cargo test --release -p azul-layout --lib widgets::text_area
cargo test --release -p azul-layout --lib widgets::titlebar
cargo test --release -p azul-layout --lib widgets::video
cargo test --release -p azul-layout --lib widgets::themes
cargo test --release -p azul-layout --test all widgets_follow_the_app_theme
cargo test --release -p azul-layout --test all flat_and_flora_widgets_follow_the_light_and_dark_theme
cargo test --release -p azul-layout --test all a_clicked_control_takes_the_new_mode_after_a_scheme_switch
cargo test --release -p azul-layout --test all a_replaced_inline_style_follows_the_mode
cargo test --release -p azul-layout --test all form_controls_become_widgets
cargo test --release -p azul-layout --test all static_opacity_paints
```

## What is left

- Nothing of R5-D's scope. If the parent wants the two allowed tab differences gone, that is a design
  call (flat's centred tabs, flora's tabs on the rule), not an accident.
- Merge note: flat.rs gains one appended section (`// ==== R5-D ... ====`) at its end; the other R5 agents
  may append theirs at the same spot.

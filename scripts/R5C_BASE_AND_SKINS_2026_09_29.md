# R5-C: widgets become a BASE plus per-theme SKINS (report, 2026-09-29)

Branch `wt/r5c-base-and-skins`, base `d240a1b1d`. Task `R5C_BASE_AND_SKINS`.
Widgets: number_input, pagination, popover, progressbar, quick_access, radio_group, ribbon,
segmented, slider, spinner, split_pane. All eleven follow the app theme (none skipped).

Nothing was compiled. The parent compiles once and runs the commands at the end.

## What was built

1. **RED** (`9131a2c67`): one test per widget, a sentence named
   `a_<widget>_declares_its_structure_once_for_every_theme`. Each builds the widget WITHOUT a
   pin under both app themes (`theme_blocks::checks::{under, BOTH}`), over its main states, and
   calls `theme_checks::assert_structure_is_shared(.., &[])`. The tests live in a new
   `base_and_skin_tests` module at the end of each widget file; ribbon's sits at the end of its
   `flora_tests` module (it reuses that module's private `fixture()`).
2. **Audit + GREEN**: every structure declaration both theme files spelled out is now ONE base
   in the widget's file; both looks start with it (base first, skin after). No `allowed`
   exceptions were needed: every test passes `&[]`.

| widget | structure moved to base | accidental differences unified (old flat / flora -> new) | allowed real differences |
|---|---|---|---|
| pagination | `PAGINATION_BUTTON_BASE` (display flex, row, justify/align center, flex-grow 0, border-box, cursor pointer, user-select none); flat `build_button_style` + `flora::pagination_button` start with it | `cursor` / `user-select` declared alike but in CROSSING orders (flat: after padding and borders; flora: before), so the merge wrote them per theme -> both in the base, first | none |
| segmented | `SEGMENT_BASE` (the same minus box-sizing, which neither look declares); flat `build_segment_style` + `flora::segmented_segment` | the same crossing of `cursor` / `user-select` -> base | none |
| split_pane | `divider_base(dir)` (flex-grow 0, flex-shrink 0, border-box, the axis' resize cursor, position relative) + `divider_thickness(dir)`; flat `divider_style` + `flora::split_pane_skin` | `box-sizing`: flat none / flora `border-box` -> `border-box` in the base (a theme's hairlines must sit inside the thickness the drag arithmetic subtracts) | none |
| radio_group | `RADIO_GROUP_CIRCLE_BASE` (display flex, row, centred, flex-grow 0, `NO_SHRINK`), `RADIO_GROUP_DOT_BASE` (flex-grow 0, `NO_SHRINK`); flat's `RADIO_GROUP_CIRCLE_STYLE` / `RADIO_GROUP_DOT_STYLE_*` are skins now, laid on the base by `flat::radio_group_skin`; flora starts with the bases. Row (`build_row_style`) and label (`RADIO_GROUP_LABEL_STYLE`) were already one base | none (alike, same order - it passed before) | none |
| popover | `POPOVER_PANEL_BASE` (position relative); flat `build_panel_style` + `flora::popover_panel_style` | none | none |
| progressbar | `BAR_CONTAINER_BASE` (display flex, row) at the start of both `progressbar_render_bar_impl` containers; the `VirtualView` wrapper (height, 100% width, overflow-x/y hidden) was `flat::progressbar_mount` + a verbatim copy in `flora::progressbar` -> `progressbar::mount(bar, render)` | none | none |
| number_input | nothing to move: the wrapped TextInput's `TEXT_INPUT_CONTAINER_PROPS` / `TEXT_INPUT_LABEL_PROPS` already lead both looks | none | none |
| slider | nothing to move: `SLIDER_TRACK_STYLE` / `build_thumb_style` are shared by both builders | none | none |
| spinner | nothing to move: `build_container_style` and `part()` are the one builder both looks call | none | none |
| ribbon | nothing to move: the flora look is the flat part's geometry (`flora::chrome_geometry`) plus paint, so the structure is authored once in `ribbon.rs`'s `theme_*` part builders | none | none |
| quick_access | the same as ribbon (`flora::quick_access_style`) | none | none |

Expected RED -> GREEN: pagination, segmented, split_pane. The other eight were expected green
already (read off the builders); their commits only remove the duplicated authoring.

## Visible changes

None intended, one declared value changes:
- **flat's split-pane divider now declares `box-sizing: border-box`**. It has no border and no
  padding, so its box does not move; flora's divider already had it.

Every other pinned widget resolves to the same values in every mode and state: each property
is still declared once, unconditionally; only the ORDER in which the flat / flora lists declare
the structure changed (it now leads).

## Tests changed incidentally

- `split_pane::autotest_generated::divider_style_never_grows_or_shrinks_and_is_visible`: the
  exact declaration count 7 -> 8 (the new `box-sizing`), comment updated.
- No other count / order test moved: pagination's 26 and segmented's 24 declarations are the
  same set; radio's const-table tests still see width, height, radius and opacity in the flat
  skins.

## Overlaps with other tasks

- **popover / dialog (R5-B)**: the popover's skin is `dialog_skin` with the panel swapped in. I
  changed only the popover's own part (`build_panel_style`, `flora::popover_panel_style`, the
  new `POPOVER_PANEL_BASE`); `dialog_skin` is untouched. A popover renders only the panel and
  the content part (`DIALOG_CONTENT_STYLE`, the same in both looks): no title, no close.
- **ribbon / quick_access embedded widgets**: the ribbon test fixture embeds no ComboBox /
  DropDown / CheckBox. Built in the ribbon's look, those carry their OWN widget's structure
  (other R5 parts); the ribbon's buttons carry the ribbon's parts only (both Button builders
  take injected styles verbatim).
- **`theme_blocks::stack_parts`** (parent, not in my base): I stack no parts after a merge. My
  bases are prepended INSIDE each look before `follow_props` / `follow_dom` merge it, so a
  property is either wholly shared or wholly themed. The ribbon / quick_access `merged_style`
  bodies are untouched.
- pagination.rs / segmented.rs `set_node_inline_style` calls (V2) are untouched.

## NO DUPLICATION notes (existing twins, not fixed here)

- `themes/decl.rs` and `themes/style_kit.rs` are twins for `padding`, `radius`, `ink`,
  `fill`/`bg`, `layers`, `themed_ink`, `themed_layers`, `hover_layers`, `active_layers`,
  `hover_ink`, and differ subtly in `border` (widths+styles only vs. with colours),
  `focus_ring` (interleaved twins vs. lights then darks) and `focus_halo` (bottom vs. left
  shadow slot).
- `flat::progressbar_render_bar_impl` and `flora::progressbar_render_bar_impl` are near
  verbatim copies apart from colours and shadows (the structure is shared now; the rest is
  skin, left as is).
- ribbon / quick_access keep their base INSIDE flat's part builders (`theme_*`, geometry and
  flat paint interleaved); flora strips the paint (`chrome_geometry`). Authored once, but not
  a separate `base_*` function.

## Lint helper (not edited)

`theme_checks::themed_structure` matches an inline `allowed` entry by the node's CLASS, so a
classless node (the dialog's close-row div, `<no class>`) can never be allowed. Not needed by
part C; a path- or selector-based entry would cover it.

## Commits

- `9131a2c67` test(widgets): R5-C widgets declare their structure once for every theme (RED)
- `0d05c508d` fix(widgets): pagination, segmented, split_pane - one base, a skin per theme
- `6cdb62d18` refactor(widgets): radio_group - the indicator's structure is one base
- `073ac7820` refactor(widgets): popover, progressbar - the structure is one base
- checkpoints: `0a8c147c7`, `0da3e5744`, `a56929f0b`, and the one with this report

## api.json

No public API change. New items are `pub(crate)`: `pagination::PAGINATION_BUTTON_BASE`,
`segmented::SEGMENT_BASE`, `split_pane::{divider_base, divider_thickness}`,
`radio_group::{RADIO_GROUP_CIRCLE_BASE, RADIO_GROUP_DOT_BASE}`,
`popover::POPOVER_PANEL_BASE`, `progressbar::{BAR_CONTAINER_BASE, mount}`. Removed:
`themes::flat::progressbar_mount` (`pub(crate)`). `themes::flat::progressbar` /
`themes::flora::progressbar` keep their signatures.

## Least sure to compile

1. `v.extend([ .. ])` with long array literals of `CssPropertyWithConditions` (pagination,
   segmented, popover, flora radio / pagination) - by-value array iteration, edition 2021.
2. `flat::radio_group_skin`'s closure `|base: &[P], skin: &[P]| ..([base, skin].concat())`.
3. `progressbar::mount` uses `alloc::vec!`, `RefAny`, `LayoutHeightValue`,
   `LayoutOverflowValue` through the file's glob imports (the same ones flat.rs used).
4. `split_pane.rs` now imports `LayoutBoxSizing` explicitly; its test modules use `super::*`.
5. The new tests' closures: `under(t, || [(&str, Dom); 3])` in ribbon, a by-value
   `[(&str, QuickAccessBar); 3]` loop in quick_access, `super::follow_bar` (private) from
   progressbar's child test module.

## Commands for the parent

```
cargo test --release -p azul-layout --lib declares_its_structure_once_for_every_theme
cargo test --release -p azul-layout --lib widgets::pagination
cargo test --release -p azul-layout --lib widgets::segmented
cargo test --release -p azul-layout --lib widgets::split_pane
cargo test --release -p azul-layout --lib widgets::radio_group
cargo test --release -p azul-layout --lib widgets::popover
cargo test --release -p azul-layout --lib widgets::progressbar
cargo test --release -p azul-layout --lib widgets::ribbon
cargo test --release -p azul-layout --lib widgets::quick_access
cargo test --release -p azul-layout --test all widgets_follow_the_app_theme
cargo test --release -p azul-layout --test all a_clicked_control_takes_the_new_mode
cargo test --release -p azul-layout --test all radio_group_geometry
cargo test --release -p azul-layout --test all pagination_fits_its_card
```

The eleven new test names: `a_number_input_...`, `a_pagination_...`, `a_popover_...`,
`a_progress_bar_...`, `a_quick_access_band_...`, `a_radio_group_...`, `a_ribbon_...`,
`a_segmented_control_...`, `a_slider_...`, `a_spinner_...`, `a_split_pane_...`, each
`..._declares_its_structure_once_for_every_theme`.

## Left

Nothing in part C's list. If a RED test I expected green fails, the message names the node and
property; the audit table above says where its base lives.

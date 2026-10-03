# R5-B: widgets become a BASE plus per-theme SKINS (2026-09-29)

Branch `wt/r5b-base-and-skins`, base `d240a1b1d`. TASK `R5B_BASE_AND_SKINS`.
Widgets: combobox, date_picker, datetime_local, dialog, divider, drop_down,
file_input, form, frame, label, menubar. All 11 follow the app theme, so none
was skipped. Nothing was compiled.

## What was built

1. **RED lint test per widget** (`a_<widget>_declares_its_structure_once_for_every_theme`).
   Each builds the widget with no pin under both structure themes
   (`checks::under(t, ..)` for `t in BOTH`), covers its states, and calls
   `theme_checks::assert_structure_is_shared(.., &[])`. No widget needed an
   allowed exception.
2. **Audit.** Only two widgets actually failed: combobox and menubar. The
   other nine already declared their structure alike and in the same order
   in both looks, so the merge shared it.
3. **One base per widget.** Where the structure was written twice (once
   in each look), it now lives once in the widget's own file. Both
   `flat.rs` and `flora.rs` start from that base and add only their skin.
   Four widgets already had a single source and were left unchanged.

## Table

| widget | structure moved to base | accidental differences unified (old flat / flora -> new) | allowed real differences |
|---|---|---|---|
| combobox | `COMBOBOX_FIELD_BASE` (flex row, align-items center, flex-grow 0, cursor text), `COMBOBOX_LIST_BASE` (display block), `COMBOBOX_OPTION_BASE` (display block, cursor pointer, user-select none), all in combobox.rs | option row, ORDER only. Flat put cursor and user-select after the four paddings. Flora put them before. The orders crossed, so both properties went into each theme's block. Now: base first in both. | none |
| menubar | `menubar::base_bar()` (flex row, align-items stretch) and `menubar::base_item()` (flex row, align-items center, cursor pointer), both as inline style | Flat kept its structure inside its `with_css` component sheet and flora kept it inline, so neither side had a twin. Flat now carries the base inline and its sheet (`MENUBAR_CSS`, `MENUBAR_ITEM_CSS`) holds only skin. The values are unchanged. | none |
| divider | `divider::DIVIDER_BASE` (block, align-self stretch, flex-grow 0) | none. Flat's static put `height` second; now it comes after the base (same value). | none |
| drop_down | `DROPDOWN_WRAPPER_BASE` (inline-flex row, flex-grow 0, align-items center, cursor pointer), `DROPDOWN_LABEL_BASE` (flex-grow 1), `DROPDOWN_ARROW_BASE` (flex-grow 0), all in drop_down.rs | none. The arrow's flex-grow now comes before its font-size in both looks. | none |
| dialog | `DIALOG_PANEL_BASE` (relative, flex column, flex-grow 0), `DIALOG_TITLE_BASE` (flex-grow 0, user-select none), `DIALOG_CLOSE_BASE` (absolute, cursor pointer, user-select none), all in dialog.rs. `DIALOG_*_STYLE` are now flat's skins. | none (reorder only in flat) | none |
| form | `form::base_form()` (flex column) | none | none |
| datetime_local | `datetime_local::base_row()` (flex row, align-items center, align-self start) | none | none |
| date_picker | already one source: `DatePickerLook::established()`, which both looks extend | none | none |
| frame | already one source: `FRAME_*_STYLE`, which both looks extend | none | none |
| label | already one source: `Label::resolved_label_style()`, used by both looks | none | none |
| file_input | already one source: it renders as a `Button`, so its structure is `build_button_container_style` (the button's, owned by another R5 part) | none | none |

## Visible changes

None intended, and none expected:
- **Pinned flora:** every changed builder produces the same declarations in the same order as before.
- **Pinned flat:** a few declarations are reordered to put the base first:
  - the combobox option's cursor and user-select;
  - the divider's height and width;
  - the drop_down arrow's flex-grow;
  - the dialog title's user-select;
  - the dialog close button's cursor and user-select.

  Each of these properties is declared once per node, so the resolved values do not change.
- **Flat menubar:** display, flex-direction, align-items and cursor now come from the node's inline style instead of its `with_css` sheet. The values are the same, and both are INLINE-priority and node-only.
- **Unpinned widgets:** the structure is now shared (outside every `@theme` block), so it also applies under an app theme that no widget knows. As a side effect, the divider's `height`, which is equal in both looks, is now shared too.

## Commits

- `c4fecd00d` docs(r5b): progress checkpoint
- `ace6a34ed` test(widgets): R5-B widgets declare their structure once for every theme (RED)
- `759b7f551` docs(r5b): checkpoint after RED
- `1f2fc5e8f` fix(widgets): combobox and menubar declare their structure once (R5-B GREEN)
- `c105c964e` refactor(widgets): divider, drop_down, dialog, form, datetime_local author their structure once (R5-B GREEN)
- (this report and the final checkpoint)

## Tests updated incidentally

- combobox `theme_tests::the_style_resolvers_answer_for_the_theme` expected flat's field to be exactly `COMBOBOX_INPUT_STYLE`. It now expects `[COMBOBOX_FIELD_BASE, COMBOBOX_INPUT_STYLE].concat()`.
- divider `a_hand_built_divider_cannot_contradict_its_own_orientation` and `dom_renders_the_style_field_and_ignores_the_orientation_field` used the removed `DIVIDER_STYLE_HORIZONTAL` as "the horizontal style". They now use `Divider::create().resolved_divider_style()`. What they check is unchanged, and `DECL_COUNT` is still 8.

## api.json

No public API change. Every new item is `pub(crate)`:
- the `*_BASE` statics;
- `menubar::base_bar` / `base_item`, `form::base_form`, `datetime_local::base_row`;
- `menubar`'s private `style_flat_bar` / `style_flat_item`.

The renamed divider statics were private, and the `MENUBAR_*_CSS` strings are private consts.

## `stack_parts` (the parent's new cascade fact)

My changes do not add any place where parts are stacked onto a merged style. Every base + skin concatenation happens inside ONE look's builder, before the merge. No skin declares a property that is also in its base, so `stack_parts` is not called anywhere in this branch.

I found one existing place in my widgets that stacks onto a merged part and should use it. The parent should look at it:
- `ComboBox::list_style_on` (combobox.rs) appends the caller's `list_style` extras to the merged panel (`skin.list`) and says the extras win "last-wins". Under the ranking rule, an extra that sets a property the two themes draw differently loses to the live theme's block. Examples: the background, the border colours, the bottom radii. The fix is `theme_blocks::stack_parts(&base, extra)` in the `Some(extra)` arm.
- I did not change it: the helper does not exist in my base, and `theme_checks::resolve` does not model the ranking, so I cannot write a correct RED test for it.

Other stacking sites I checked are safe:
- frame content: `flex-grow` is prepended, and `look.content` never declares it.
- date_picker `cell_faces`: built per look, before the merge.
- dialog, form and datetime_local `container_style` / `panel_style`: these replace the whole style, not stack onto it.

## Least sure to compile

- `layout/src/widgets/date_picker.rs` test: `let pickers: [(&str, fn() -> DatePicker); 3] = [("date", || ..), ..]` relies on non-capturing closures coercing to `fn` pointers inside tuple and array literals.
- `layout/src/widgets/drop_down.rs`: the three `*_BASE` statics use `LayoutDisplay`, `LayoutFlexDirection`, `LayoutFlexGrow`, `LayoutAlignItems` and `StyleCursor`. The file had not used these names before; they come in through its existing `layout::*` / `style::*` globs, the same import pattern label.rs and frame.rs use.
- `layout/src/widgets/menubar.rs`:
  - a new top-level `use azul_css::{..}` (menubar had no azul_css import);
  - `build(menu, None, style_flat_bar, style_flat_item)` passes fn items where `impl Fn(Dom) -> Dom` is expected;
  - the test helper passes `&style_flat_item` as `&impl Fn`.
- `flat.rs` / `flora.rs` `drop_down`: a `use crate::widgets::drop_down::{..}` item placed in the function body after a `let`.
- `Vec::extend([..])` with arrays of up to 17 elements (flora `datetime_local`), which needs by-value array `IntoIterator` (edition 2021).

A `rustfmt --edition 2021 --check` parse pass on every changed file reported no errors.

## Test commands for the parent

```
cargo test --release -p azul-layout --lib declares_its_structure_once_for_every_theme
```

That filter runs the 11 new tests:
- `widgets::combobox::theme_tests::a_combobox_declares_its_structure_once_for_every_theme`
- `widgets::menubar::app_theme_tests::a_menubar_declares_its_structure_once_for_every_theme`
- `widgets::dialog::theme_tests::a_dialog_declares_its_structure_once_for_every_theme`
- `widgets::date_picker::app_theme_tests::a_date_picker_declares_its_structure_once_for_every_theme`
- `widgets::datetime_local::app_theme_tests::a_datetime_local_declares_its_structure_once_for_every_theme`
- `widgets::divider::app_theme_tests::a_divider_declares_its_structure_once_for_every_theme`
- `widgets::drop_down::app_theme_tests::a_drop_down_declares_its_structure_once_for_every_theme`
- `widgets::file_input::theme_tests::a_file_input_declares_its_structure_once_for_every_theme` (needs the `std` feature)
- `widgets::form::app_theme_tests::a_form_declares_its_structure_once_for_every_theme`
- `widgets::frame::app_theme_tests::a_frame_declares_its_structure_once_for_every_theme`
- `widgets::label::app_theme_tests::a_label_declares_its_structure_once_for_every_theme`

Regression suites for the touched widgets (whole modules):

```
cargo test --release -p azul-layout --lib widgets::combobox
cargo test --release -p azul-layout --lib widgets::menubar
cargo test --release -p azul-layout --lib widgets::dialog
cargo test --release -p azul-layout --lib widgets::modal
cargo test --release -p azul-layout --lib widgets::popover
cargo test --release -p azul-layout --lib widgets::divider
cargo test --release -p azul-layout --lib widgets::drop_down
cargo test --release -p azul-layout --lib widgets::form
cargo test --release -p azul-layout --lib widgets::datetime_local
cargo test --release -p azul-layout --lib widgets::themes
cargo test --release -p azul-layout --test all menubar
```

Run `widgets::themes` because flora.rs's tests read `FLORA_DROPDOWN_WRAPPER_STYLE`, which is now a skin with the same backgrounds. Run the integration `menubar` filter because `menubar_item_clip.rs` and `demo_layout_regressions.rs` lay out `build_menubar_dom`, which now has its structure inline.

## What is left / notes

- file_input's result depends on the Button's two looks (another R5 part). Today the Button's structure is shared, so the test is expected to pass.
- `dialog_skin` is shared with modal (the same skin) and popover (the same skin with a different panel). I changed only the panel, title and close structure. If the modal or popover owner also edits `dialog_skin`, expect a textual conflict in `flat.rs` / `flora.rs` `dialog_skin`.
- The datetime_local test lints the row only. Its date part is covered by the date_picker test, and its time part by the time_picker owner's test.
- No edit to `theme_checks.rs`, `theme_blocks.rs`, `decl.rs` or `style_kit.rs`, and no new helper: every base + skin concatenation uses `[base, skin].concat()` or `base.to_vec()` + `extend`.

# R5-A: widgets become a BASE plus per-theme SKINS - report (2026-09-29)

Branch `wt/r5A-base-and-skins`, base `d240a1b1d`. Widgets: accordion, alert, avatar, backstage,
badge, breadcrumb, button, card, check_box, chip, color_input. All eleven follow the app theme
(`follow_app_theme`, or `follow_props` for backstage), so none was skipped.

Nothing was compiled (house rule). Every touched file passes `rustfmt --check` as a parse check.

## What was built

1. One RED test per widget (`<widget>_declares_its_structure_once_for_every_theme`): the widget
   built WITHOUT a pin under each structure theme (`checks::under(t, ..)` for `t` in
   `checks::BOTH`), over its main states, through `theme_checks::assert_structure_is_shared` with
   nothing allowed. No test allows anything at the end either.
2. The three widgets whose structure really sat in `@theme` blocks got a base in their own file:
   accordion, breadcrumb, color_input.
3. The four whose structure merged out already but was written twice (the widget file for flat,
   flora.rs for flora) now author it once: alert, badge, card, chip.
4. avatar, backstage, button, check_box: already one base in the widget file; test only.
5. `decl::on_base(base, skin)` (APPENDED to `themes/decl.rs` under
   `// ==== R5-A: a part's base, then its skin ====`): the one helper that lays a part's base
   before a theme's skin, in one list, before the merge.

After this no structure declaration of these widgets appears in `flat.rs` or `flora.rs` (scanned
section by section: accordion, alert, badge, breadcrumb, card, chip, color_input - 0 lines; the
avatar / button / check_box builders and flora's `backstage_style` declare none).

## Per widget

| widget | structure moved to the base (widget file) | accidental differences unified (old flat / flora -> new) | allowed real differences |
|---|---|---|---|
| accordion | `ACCORDION_CONTAINER_BASE` display, flex-direction, flex-grow, overflow-x/-y; `ACCORDION_SECTION_BASE` display, flex-direction, flex-grow; `ACCORDION_HEADER_BASE` display, flex-direction, align-items, flex-grow, cursor, user-select; `ACCORDION_TITLE_BASE` flex-grow, text-align (was `ACCORDION_TITLE_STYLE`); `ACCORDION_CHEVRON_BASE` display, align-items, justify-content, flex-grow, flex-shrink (out of `chevron_box`, which is now the size only). `accordion::build` lays each base first. | panel overflow-x/-y: flat after the radius / flora right after the font (the orders crossed) -> base, after flex-grow. Header cursor + user-select: flat after the padding / flora before it -> base. Same values. | none |
| alert | `ALERT_CONTAINER_BASE` display, flex-direction, align-items, align-self, flex-grow (`build_alert_style` and flora's banner start with it); `ALERT_MESSAGE_BASE` flex-grow, text-align (was `ALERT_MESSAGE_STYLE`); `ALERT_CLOSE_BASE` flex-grow, cursor, user-select (`alert::build` lays both). | none | none |
| avatar | none: `build_avatar_style` / `build_image_style` in avatar.rs are the whole style of both looks | none | none |
| backstage | none: flora's parts are `chrome_geometry(flat part)` + paint, so the structure is backstage.rs's `theme_*` parts, once | none | none |
| badge | `BADGE_BASE` display, flex-direction, justify-content, align-items, align-self, flex-grow (`build_badge_style` and `flora_badge_style` start with it) | none | none |
| breadcrumb | `BREADCRUMB_ITEM_BASE` flex-grow, cursor, user-select; `BREADCRUMB_LABEL_BASE` flex-grow, user-select (current page, separator). `breadcrumb::build` lays them; flora's `quiet` closure is gone. | link cursor: flat flex-grow, cursor, user-select / flora flex-grow, user-select, ink, cursor (crossed) -> base, before user-select | none |
| button | none: `build_button_container_style` (button.rs) is both looks' base; the theme builders add dark twins and states only | none | none |
| card | `CARD_BASE` display, flex-direction (`card::build` lays it after the card's own flex-grow) | none | none |
| check_box | none: both builders start from `resolved_container_style` / `resolved_content_style` | none | none |
| chip | `CHIP_CONTAINER_BASE` display, flex-direction, align-items, align-self, flex-grow (`build_chip_style` and `flora_chip_container` start with it); `CHIP_LABEL_STYLE` (flex-grow, text-align, user-select) is the label's base; `CHIP_REMOVE_BASE` flex-grow, cursor, user-select. `chip::build` lays both. | none | none |
| color_input | `PICKER_PANEL_BASE_CSS` display, flex-direction; `PICKER_PREVIEW_BASE_CSS` position, overflow; `PICKER_EYEDROPPER_BASE_CSS` display, align-items, justify-content, cursor - each a sheet of its own that `picker_panel` attaches before the skin sheet | none: both looks wrote the same layout, but in ONE CSS rule with the paint, and a rule is never split | none |

## Visible changes

None intended, none known. Every unified difference kept its value; only the position of a
structure declaration in a part's list changed (structure ahead of paint), and no property has
declarations in both a base and a skin, so a pinned widget resolves every property as before in
every mode and state. `build_alert_style`, `build_badge_style`, `build_chip_style` return exactly
what they returned.

## Tests

New (the parent runs them; each should be green):

```
cargo test --release -p azul-layout --lib declares_its_structure_once_for_every_theme
```

matches all eleven:

- `widgets::accordion::app_theme_tests::an_accordion_declares_its_structure_once_for_every_theme`
- `widgets::alert::app_theme_tests::an_alert_declares_its_structure_once_for_every_theme`
- `widgets::avatar::app_theme_tests::an_avatar_declares_its_structure_once_for_every_theme`
- `widgets::backstage::flora_tests::a_backstage_declares_its_structure_once_for_every_theme`
- `widgets::badge::app_theme_tests::a_badge_declares_its_structure_once_for_every_theme`
- `widgets::breadcrumb::app_theme_tests::a_breadcrumb_declares_its_structure_once_for_every_theme`
- `widgets::button::app_theme_tests::a_button_declares_its_structure_once_for_every_theme`
- `widgets::card::app_theme_tests::a_card_declares_its_structure_once_for_every_theme`
- `widgets::check_box::app_theme_tests::a_check_box_declares_its_structure_once_for_every_theme`
- `widgets::chip::app_theme_tests::a_chip_declares_its_structure_once_for_every_theme`
- `widgets::color_input::app_theme_tests::a_color_input_declares_its_structure_once_for_every_theme`

Regression (the widgets' own suites, the follow contract, the merge):

```
cargo test --release -p azul-layout --lib widgets::accordion
cargo test --release -p azul-layout --lib widgets::alert
cargo test --release -p azul-layout --lib widgets::badge
cargo test --release -p azul-layout --lib widgets::breadcrumb
cargo test --release -p azul-layout --lib widgets::card
cargo test --release -p azul-layout --lib widgets::chip
cargo test --release -p azul-layout --lib widgets::color_input
cargo test --release -p azul-layout --lib widgets::backstage
cargo test --release -p azul-layout --lib follows_the_app_theme
cargo test --release -p azul-layout --lib widgets::
```

Tests that asserted a declaration layout incidentally, updated (same assertions):

- breadcrumb `only_the_clickable_crumb_style_declares_a_pointer_cursor`,
  `every_crumb_style_disables_text_selection_and_flex_growth`: read a crumb as `build` lays it,
  base + flat skin, instead of the flat static alone.
- card `dom_prepends_the_flex_grow_declaration_to_the_static_card_style`: the count is
  flex-grow + `CARD_BASE` + `CARD_STYLE`.
- chip `label_and_remove_static_styles_are_finite_unconditional_and_non_growing`,
  `the_remove_affordance_is_styled_as_a_clickable_target`: read the "x" as `CHIP_REMOVE_BASE` +
  `CHIP_REMOVE_STYLE`.

The colour input's test prunes the widgets its picker nests (subtrees with the classes
`__azul-native-text-input-container` and `__azul-native-label`, i.e. the hex field, the channel
fields and their labels): they follow the app theme on their own and answer in their own tests. If
the parent would rather lint them here too once R5-B/C/D land, drop `own_nodes`.

## Commits

- `1a501bee3` test(widgets): R5-A - every widget declares its structure once for every theme (RED)
- `cd89505ca` fix(accordion): one base for its structure, the themes keep only their skin
- `692fff6ac` fix(breadcrumb): one base for a crumb's structure, the themes keep only their skin
- `501adb32b` fix(color_input): the picker's layout is a base sheet, the themes keep only their skin
- `08da0680c` refactor(alert, badge, card, chip): structure authored once in the widget, the themes keep only their skin
- `6060a3b59` refactor(widgets): one helper lays a part's base before its skin (decl::on_base)
- progress checkpoints `bfc79c49e`, `7795f0268`, `378be313b`, `60dde0f3b`, `fb3588719`, and this
  report.

The RED commit lists the expected failures with the lint's Debug names (`OverflowX`, ...); in the
main checkout the lint prints CSS names (`overflow-x`) - the parent's fix, not on this branch.

## api.json

No change. Every new or renamed item is `pub(crate)` (`ACCORDION_*_BASE`, `ALERT_*_BASE`,
`BADGE_BASE`, `BREADCRUMB_*_BASE`, `CARD_BASE`, `CHIP_CONTAINER_BASE`, `CHIP_REMOVE_BASE`,
`PICKER_*_BASE_CSS`, `decl::on_base`); the removed `ACCORDION_TITLE_STYLE` and
`ALERT_MESSAGE_STYLE` were `pub(crate)`. No public struct, field or function changed.

## Least sure to compile

1. `accordion.rs` `ACCORDION_CHEVRON_BASE`: a `static` slice building
   `azul_css::props::layout::LayoutFlexShrink { inner: azul_css::props::basic::length::FloatValue::const_new(0) }`
   and `CssProperty::const_justify_content(azul_css::props::layout::LayoutJustifyContent::Center)`
   by full path (both `const`; the paths are the ones `chevron_box` imported).
2. The `part` closures in `accordion::build`, `alert::build`, `breadcrumb::build`: annotated
   `|base: &[CssPropertyWithConditions], skin: &[CssPropertyWithConditions]|`, called with a
   `&'static [_]` static and `look.x.as_slice()`.
3. `color_input.rs` test `own_nodes`: `core::mem::take(&mut dom.children)` (`DomVec: Default`, as
   `theme_blocks::follow_node` does), `.collect::<Vec<Dom>>().into()` for the `DomVec`, and
   `crate::widgets::text_input::TEXT_INPUT_CONTAINER_CLASS` (a `pub const &str`).
4. `avatar.rs` test: `ImageRef::null_image(2, 2, RawImageFormat::RGBA8, Vec::new())` with
   `azul_core::resources::RawImageFormat` imported in the test (copied from the autotest module).
5. `button.rs` test: an array of `(&str, Button)` iterated by value, each `Button` moved into the
   `FnOnce` given to `checks::under`.
6. `Vec::new()` for the empty skins in the look structs (`AccordionLook::title`,
   `AlertLook::message`, `ChipLook::label`) - the field types fix the element type.

## What is left / for the parent

- Twins, reported not fixed: `flat::avatar` and `flora::avatar` are identical (both call
  avatar.rs's builders); `flat::button` / `flora::button` and `flat::check_box` /
  `flora::check_box` are near-twins (they differ only in the dark twins and states). Deduping them
  means editing the middle of the shared theme files next to other agents' widgets, so I left them.
- `decl::on_base` is new; if R5-B/C/D add a base-then-skin helper of their own, the parent keeps
  one.
- `theme_blocks::stack_parts` (main only): not copied, backstage's `merged_style` untouched as
  asked. Nothing on this branch stacks two merged parts: every base + skin is one list before the
  merge, and a base never shares a property with its skin, so the rule that a themed declaration
  outranks an unthemed one does not bite. After the parent integrates `stack_parts`, the backstage
  lint stays green: the active and gap parts carry no structure (a background, a margin), so
  `stack_parts` copies none into a theme block.

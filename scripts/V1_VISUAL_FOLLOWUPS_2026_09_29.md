# V1 - visual follow-ups of the widget-theme wave (2026-09-29)

Branch `wt/v1-visual-followups`, cut from `0a326afe5`. Nothing was compiled (house rule); every
changed Rust file was parse-checked with `rustfmt --check` (syntax only, no writes). Every
behaviour change is a RED test commit followed by its fix.

## Audit (at 0a326afe5)

| # | Item | Found | Now |
|---|---|---|---|
| 1 | 4-sided box-shadow draws four stacked shadows | STILL OPEN - an ENGINE bug | fixed at the painter |
| 2 | Spinner ring does not grow and shrink | STILL OPEN - fixed 135deg arc | nested clip paths, 2 s sweep |
| 3 | Accordion chevron | STILL OPEN - no indicator at all (TODO2) | indicator that turns, tweened |
| 4 | Flora dark focus ring #2F4A85 | PARTIAL - W3a/W3b widgets used `DARK_GLOW`; the shared `FOCUS_BORDER_*_DARK` (buttons, text fields) still `DARK_ACC` | `DARK_GLOW`, contrast-tested |
| 5 | segmented / stepper / pagination / date_picker stale colours | STILL OPEN | cascade-resolved restyle, all four twins deleted |

## What was built

### 1. Box-shadow (engine root cause)

The engine keeps a shadow in four per-side slots (`-azul-box-shadow-{left,right,top,bottom}`) and
the `box-shadow` shorthand writes the SAME shadow into all four; `display_list.rs` pushed one full
`DisplayListItem::BoxShadow` per slot, and no renderer gives an item a side - so every
`box-shadow` was painted four times (50% black ring -> ~94% black). Not the widget CSS (W3a's
`decl::shadow` had worked round it with the bottom slot alone).

Fix: new `solver3::getters::get_box_shadows(styled_dom, node, state) -> Vec<StyleBoxShadow>` - every
DISTINCT shadow of the four slots, each once, in slot order; the painter paints that list. A node
declaring different shadows in different slots still paints each (a guard test pins it). The
parser was left alone: a one-slot shorthand would stop `box-shadow: none` clearing an earlier
per-side declaration.

Pixel test (headless cpurender): `layout/tests/a_box_shadow_paints_once.rs` - one DL item, the
ring pixel of a 50% black spread ring over white is mid grey (100..=160), two different side
shadows are two items.

### 2. Spinner arc sweep (`spinner.rs`)

The Windows 11 ProgressRing (reference section 3.2): 2 s loop, the arc grows from 0 to 180deg at
its head, then shrinks from its tail, the ring turning 450deg/s. Built only from the existing
clip-path machinery and `rotate` tracks (no shape changes over time):

```text
arc     spinning frame, no paint              rotate 0 -> 900deg / 2 s, linear
  window  clip: right half of the box         turns with the TAIL
    body  ink, clip: half-annulus [-45,135]   turns with the HEAD (inside the window)
  cap   ink, round, at the head               turns with the head
  cap   ink, round, at the tail               turns with the tail
```

Nested, the window [tail, tail+180] and body [head-180, head] intersect in [tail, head]. Tracks
are TURNS from the rest picture, so under reduced motion (no animation declared, as before) the
ring is exactly the old round-capped 135deg arc from 12 o'clock; the loop is seamless in world
angles. Each half is ease-in-out (the Lottie curves were not in the reference). `part()` now takes
optional ink / motion / shape; `rotate_track` + `ring_tracks` replace `spin_track`; `arc_shape`
became `arc_body_shape`, `ring_window_shape`, `cap_shape`. Spokes unchanged.

Contract changes (on purpose, in the RED commit): the ring's spin is 450deg/s over the 2 s loop
(was 800 ms per turn); the ink sits on body + caps, frame and window paint nothing.

### 3. Accordion disclosure indicator (`accordion.rs`, CSS transform tween)

Every header ends in the theme's indicator: flat `expand_more` (16px) turned 180deg when open
(Windows 11 expander / Bootstrap), flora `add` (18px) turned 45deg into a cross (flora.css FAQ).
It is a BOX (class `__azul-native-accordion-chevron`) around `Dom::create_icon`: icon resolution
replaces the icon node and drops its classes, and the click handler finds the indicator by class.
The click writes the new `transform: rotate(..)` on the same terms as the body's height (full
channel, or `initial` with no tween and a rebuilding host); the box declares a `transform` tween
on the body's 220 ms beat, only under `prefers-reduced-motion: no-preference`. One `tween()` helper
now declares both animations. Decoration: no tab stop, no callback, ink = the header's (turns
brass with a flora header under the pointer).

Engine piece (css): `CssProperty::interpolate` had no `transform` arm - a seeded transition held
its value and jumped half way. New `transform::interpolate_transform_lists` (CSS Transforms 1 s.9):
same-function lists tween per function (angles UNFOLDED, so two turns stay two turns; translate /
scale / skew per argument), an empty list stands for the other side's identity, mismatched lists
keep the half-way switch.

### 4. Flora night focus ring (`flora.rs`)

`FOCUS_BORDER_{TOP,RIGHT,BOTTOM,LEFT}_DARK` now use `DARK_GLOW` #7A93C6 (flora.css's night
`--focus-color`, the token every W3a/W3b flora widget already rings in): >= 3.4:1 on all 14 night
surface tokens (5.1:1 on `--fl-sur`); the stone #2F4A85 stood 1.8:1. Test module
`night_focus_ring_tests` (appended at the end of flora.rs): each const against every night surface
(`ColorU::contrast_ratio >= 3.0`), the token identity, and a focused flora `Button` at night.
Affects flora buttons and (via `FIELD_BORDER_STATES`) flora text fields / areas.

### 5. Stale restyle colours - engine API + four widgets

Root cause: `set_css_property` writes a USER OVERRIDE, which outranks every declaration - the
colour baked for the mode of the moment survives a restyle-only mode switch, and outranks the
node's `:hover` / `:focus` rules too (the W3b "click restyles outrank hover/pressed" note). No
existing mechanism could re-condition a node at run time (`:checked` is never set by the engine,
class changes do not re-run the cascade, inline conditions cannot test a class).

New engine primitive: `CallbackInfo::set_node_inline_style(node, CssPropertyWithConditionsVec)` ->
`CallbackChange::SetNodeInlineStyle` -> `ContentChange::NodeStyle` through the one content
chokepoint (`LayoutWindow::apply_content_change`; dll host + e2e runner delegate like NodeCss).
`apply_node_style_change` replaces the node's inline style, computes which property types' own
declarations changed (tier + overrides to clear), clears overrides of those types (`initial`
through `restyle_user_property`), re-runs the cascade tail a theme flip runs
(`recascade_ua_inheritance_and_compact`) under a new cascade epoch, then rebuilds the DL (or
relayouts in place when a layout property changed). Nothing is pinned.

Widgets: each restyle writes the style a BUILD in the new state gives the node, from the same
skin function - so a clicked control IS the control built in its new state, in every mode:
segmented `(skin.segment)(sel, first, last)`, pagination `(skin.button)(face, first, last)`,
stepper `(skin.circle)(reached)` / `(skin.connector)(fill)` / `(skin.label)(reached)`, date picker
`CellFaces` (the picked / other faces, built by `DatePickerLook::cell_faces`, which the day and
month builders ALSO render with - one source). Blank calendar cells are no longer touched. The
theme still comes from the marker class; for an unpinned widget that is the structure theme's
pinned face, which equals the followed face under the live theme (a theme switch rebuilds).

Deleted (every mode twin and every baked palette): `segmented::window_is_dark`,
`stepper::renders_dark`, `pagination::renders_dark`, `date_picker::window_is_dark`,
`segment_colours`, `established_colours`, `established_{circle_colours,connector_fill,label_ink}`,
`ConnFill::dark_bg`, `day_cell_colours`, `DayPalette` / `DatePickerLook::day_palette`, flora's
`segmented_colours`, `pagination_colours`, `stepper_{circle_colours,connector_fill,label_ink}` and
its hand-written date palette, and the `restyle` fields of `SegmentedSkin` / `PaginationSkin` /
`StepperSkin`. `text_input::paint_invalid_ring` borrowed the date picker's twin; it now asks
`CallbackInfo::get_resolved_color_scheme()` (the one decision, I1).

Tests: `layout/tests/a_replaced_inline_style_follows_the_mode.rs` (engine: light face at once,
its OWN dark twin after a restyle-only switch and back, geometry relayouts, a no-op is Unchanged);
`layout/tests/a_clicked_control_takes_the_new_mode_after_a_scheme_switch.rs` (per widget x
{flat, flora, unpinned}: click via `invoke_single_callback_at` + the chokepoint, twin BUILT in the
clicked state, every part's resolved background + ink equal by day, after a switch to dark, and
back). Widget unit harnesses now read `SetNodeInlineStyle` (their colour pins read the written
style's light resting face, unchanged in value); flora click tests assert the written styles equal
the flora build's, resolving by day and night.

## Commits

| Item | RED | Fix |
|---|---|---|
| audit | ddc63fbb4 | |
| 1 box-shadow | 09b31b32d | f042df374 |
| 4 focus ring | 7f001d54a | 4f299a1e8 |
| 3 transform tween (css) | f84365212 | dfb18ac7b |
| 3 accordion indicator | 686adb878 | 8efedd119 |
| 2 spinner sweep | a9ad59994 | 46c9edf67 |
| 5 engine | b618e6bae (plumbing), dc5751a52 | e22113da6 |
| 5 widgets | e68033b80 | cf7ef5e18 segmented, bb0f49454 pagination, 2f48d441d stepper, 6ecad1cdf date_picker |

Progress checkpoints: 4ff6fda84, 542be7cb9, 595171716, f85b1b566, 7528d71e1, 1f42426fb, 5e3fa7b89.

## API for api.json (autofix)

One new method on `CallbackInfo` (class of `set_css_property`):

- `set_node_inline_style`: fn_args `[{"self": "refmut"}, {"node_id": "DomNodeId"}, {"style":
  "CssPropertyWithConditionsVec"}]`, no return, fn_body `object.set_node_inline_style(node_id,
  style)`. Doc (ASCII): "Replace a node's whole inline style - every declaration and its
  conditions - with `style` (applied after the callback returns). The live restyle for a state a
  widget owns: write the node's style for the new state exactly as a rebuild would build it, dark
  twins and :hover / :focus rules included. `set_css_property` pins one value (a user override
  outranks every declaration, so a colour baked for light mode stays light after a switch to
  dark); this pins nothing: the cascade re-resolves the new declarations on every mode switch and
  state change. A node_id without a node is ignored."

Rust-only (not FFI): `CallbackChange::SetNodeInlineStyle { dom_id, node_id, style }`,
`overlay::ContentChange::NodeStyle { dom_id, node_id, style }`,
`solver3::getters::get_box_shadows`, `css::props::style::transform::interpolate_transform_lists`,
`accordion::chevron_box` (pub(crate)). No struct layout of an FFI type changed. If the web/other
backends mirror `CallbackChange` somewhere I did not find, they need the arm too (I found only the
dll `event.rs` and the e2e `runner.rs` matches).

## Least sure to compile

1. `layout/src/window.rs::apply_node_style_change`: borrows - `node_data.as_container()` bound
   in a block, then `as_container_mut()[node_id].set_style(..)`; `restyle_user_property` on the
   same `layout_result`; then `self.relayout_root_dom_in_place()` after the last use of
   `layout_result` (the same shape as `apply_node_css_change`). `get_css_property_cache_mut()`
   field access `cascade_epoch` (pub).
2. `const fn follow_skin` in segmented / pagination / stepper: fn-item -> fn-pointer coercion in a
   const fn (stable since 1.61). Drop `const` if the toolchain objects.
3. `spinner.rs` tests: `ring_pose(&dom, &|n| turn_at(&dom, n, t))` relies on closure signature
   inference from `&dyn Fn(&Dom) -> f32`; `cap_turns.sort_by(f32::total_cmp)`.
4. `accordion.rs`: the new import items `StyleTransform, StyleTransformVec` in the
   `props::style::{..}` list and `angle::AngleValue`; `chevron_box`'s function-local `use` of
   `props::layout::{LayoutFlexShrink, LayoutJustifyContent, LayoutMarginLeft, LayoutWidth}` and
   `basic::length::FloatValue`.
5. `css/src/props/property.rs` Transform arm: `start.get_property()` -> `&StyleTransformVec`
   -> `.as_ref()` as `&[StyleTransform]` (annotated); `interpolate_transform_lists` reached through
   the `style::transform::*` glob.
6. Integration tests: `Segmented::with_theme` etc. as `fn(W, UiTheme) -> W` (const fn items);
   `sd.get_css_property_cache().get_property(&node_data[node], &node, &state, &ty)`.
7. `stepper.rs` test harness imports `CssPropertyType` locally (`azul_css::props::property`), the
   file's own import list does not carry it.

## Tests for the parent

```sh
cargo test -p azul-layout --release --test all a_box_shadow_paints_once
cargo test -p azul-layout --release --test all a_replaced_inline_style_follows_the_mode
cargo test -p azul-layout --release --test all a_clicked_control_takes_the_new_mode_after_a_scheme_switch
cargo test -p azul-layout --release --lib widgets::spinner
cargo test -p azul-layout --release --lib widgets::accordion
cargo test -p azul-layout --release --lib widgets::segmented
cargo test -p azul-layout --release --lib widgets::pagination
cargo test -p azul-layout --release --lib widgets::stepper
cargo test -p azul-layout --release --lib widgets::date_picker
cargo test -p azul-layout --release --lib widgets::themes::flora::night_focus_ring_tests
cargo test -p azul-css --release --lib props::property::transform_tween_tests
# regression sweep of what the changes touch
cargo test -p azul-layout --release --test all widgets_follow_the_app_theme flat_and_flora_widgets_follow accordion_animation system_colours_in_every_colour_property app_color_scheme_override
cargo test -p azul-layout --release --lib widgets::text_input widgets::button
cargo check -p azul-dll   # the new CallbackChange arm in shell2/common/event.rs
```

(The spinner tests need the `text_layout` feature for `compile_keyframes_track`, as before.)

## Left / follow-ups

- Spinner ENGINE GAP (W3a follow-up 1) is untouched: `-azul-animation-in` tracks are started only
  by the e2e reconciliation path and only for a mounted subtree's root, so on desktop no spinner
  part moves yet. The sweep is declarative and tested through the engine's own sampler; it moves
  as soon as tracks start for every mounted node. The nested reference frames + nested image-mask
  clips (frame > window > body) are new territory for the renderers - worth one real-window look.
- `text_input`'s invalid ring (`paint_invalid_ring`) is still a baked OVERRIDE (light or dark at
  the moment of the edit) - the same staleness as item 5; `set_node_inline_style` of the field's
  style + the ring would fix it, but the container style is assembled from several theme parts
  (a text_input task).
- Stepper cells' `accessibility_value` ("step N of M") is not updated by a click (pre-existing).
- Cost: each `set_node_inline_style` re-runs the cascade tail and rebuilds the DL, once per node
  (a date-picker pick writes ~30). The old path paid a compact-cache rebuild + DL rebuild per
  PROPERTY (2 per node), so this is not worse, but a batched variant (one recascade per callback)
  is the obvious next step if a profile shows it.
- `themes/decl.rs::shadow` still writes the bottom slot only (W3a's workaround); now harmless, and
  the shorthand would paint once too.
- Flora text fields still HOVER to `DARK_ACC` at night (`HOVER_BORDER_*_DARK`, 1.8:1); hover is
  not a focus indicator, but it is as faint. Not changed here.
- Files outside the listed set, touched minimally: `layout/src/callbacks.rs`,
  `layout/src/overlay.rs`, `layout/src/window.rs`, `layout/src/e2e/runner.rs`,
  `dll/src/desktop/shell2/common/event.rs` (the new change arm, one delegation each),
  `layout/src/solver3/getters.rs` (item 1), `css/src/props/{property.rs,style/transform.rs}`
  (item 3's tween), `layout/src/widgets/text_input.rs` (one line: the twin call), and in-place edits
  in `themes/flat.rs` / `themes/flora.rs` (accordion look fields, removed `restyle` skin fields and
  flora's baked palettes, the four focus consts). Expect one-line conflicts with U1 in
  `accordion.rs` / `date_picker.rs` `dom()` and with R0's `get_resolved_color_scheme` rename in
  `text_input.rs`.

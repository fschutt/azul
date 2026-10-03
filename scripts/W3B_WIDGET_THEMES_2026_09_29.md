# W3b - flat / flora themes for the second half of the widgets (2026-09-29)

Branch `wt/w3b-widget-themes`, cut from `fix/input-bugs-2026-09-19` @ 9c6065b05. Nothing was
compiled (house rule); every behaviour change is a RED test commit followed by its
implementation commit. api.json was NOT touched - the list for autofix is below.

## What was built

13 widgets got a `theme: OptionUiTheme` field, `set_theme` / `with_theme`, and a `dom()` that
dispatches to `themes::flat::<widget>` / `themes::flora::<widget>` (the `Button` pattern):
dialog + modal + popover (one shared builder, themed once), number_input, pagination,
radio_group, segmented, split_pane, stepper, time_picker, toast, tooltip, video.

How the split works (same shape for every widget):

- The widget file keeps the DOM structure, callbacks and accessibility. Its old `dom()` body is
  now `pub(crate) fn build(self, skin: <Widget>Skin) -> Dom`; the skin is the per-part styling
  (a struct of style vecs, or of fn pointers where the style depends on state, e.g.
  `PaginationSkin { button: fn(PageFace, first, last), restyle: fn(PageFace, dark) }`).
- `themes::flat` builds the skin from the widget's ESTABLISHED statics (light values unchanged,
  system-palette dark twins) and adds only what was missing: focus rings (all four
  theme x mode combinations) and hover / pressed states. `themes::flora` builds its own skin from
  `doc/templates/flora.css` tokens, every colour paired with its night value.
- The widget root carries the theme marker class `__azul-theme-flat` / `__azul-theme-flora`
  (as `Button` does). Click handlers that live-restyle colours (pagination, segmented, stepper)
  read it back (`style_kit::theme_of_classes`) so a flora widget is not repainted in flat's
  colours on the first click.
- A caller's own style (`panel_style`, `container_style`, `tip_style`, ...) still wins over
  either theme; resolvers (`Modal::resolved_backdrop_style`, `Popover::resolved_content_style`,
  `TimePicker::resolved_container_style`, `Toast::resolved_container_style`,
  `Tooltip::resolved_tip_style`) answer for the theme via the same skin the render uses.

Shared, new files:

- `layout/src/widgets/themes/style_kit.rs` (`pub mod`): light+dark pair builders
  (`themed_bg`, `themed_ink`, `themed_layers`, `border(Edges, width, light, dark)`,
  `drop_shadow`, `inset_shadow`), state pairs (`hover_*`, `active_*`, `focus_ring` = border
  ring, `focus_shadow_ring` = inset 2px ring for joined-bar items, `focus_halo` = outset 2px
  ring for bare glyph buttons, `ring_slot` = transparent 1px border), layout helpers
  (`padding`, `radius`, `radius_corners`, `fill`, `font_size`, `weight`, `face`), the marker
  (`FLAT_CLASS`, `FLORA_CLASS`, `marker`, `theme_of_classes`).
- `layout/src/widgets/themes/theme_checks.rs` (`#[cfg(test)]`): resolves a node's inline style
  last-match-wins per mode and pseudo-state, and checks whole trees for the invariants every
  theme owes: dark twins after their light half, no interactive state shadowed by a later
  resting declaration, every Tab stop ringed by day and by night, identical a11y outline.
- `themes/mod.rs` gained two lines (`pub mod style_kit;`, `#[cfg(test)] pub(crate) mod
  theme_checks;`) under a `// ==== W3b ... ====` banner.

All flat / flora code is APPENDED at the end of `flat.rs` / `flora.rs` under
`// ==== <widget> ====` banners; nothing existing was reordered. Expect trivial textual
conflicts with W3a's appends (keep both).

## Per widget

Flora vocabulary used below: leaf = `--fl-sur` panel; BD/BD2/BD3 = `--fl-bd*` hairlines;
raised paper = `.btn-secondary` face (RAISED_FACE gradients, HOVER_FACE / PRESSED_FACE on
hover / press); sunken stone = `--fl-gem-sunken` under the sunken rig (`flora::selected_stone`,
new, shared); ink panel = `--fl-code-bg/-fg/-bd`; brass = `--fl-qt`; focus ring = `LIGHT_ACC`
by day, `DARK_GLOW` by night (flora.css `--focus-color`).

| widget | flat (established + what it gained) | flora | RED / impl |
|---|---|---|---|
| dialog / modal / popover | white panel, #ccc hairline, 8px radius, system window surface + separator by night, 50% black backdrop; the "x" gained a transparent ring slot, FIELD_RING / DARK_ACC focus ring, ink hover | leaf (SUR, BD2, 5px, shadow-3 layer), title ruled off in SEP, brass-ink close (QT -> QT2 over the quiet wash), warm `.nav-overlay` backdrop rgba(20,19,16,.45); popover = small leaf (3px, shadow-2) | 113b016c4 / 898b93cf6 |
| number_input | the wrapped TextInput's flat field, unchanged | flora field paper (FLD) under ink, BD2 hairline, 3px, `--fl-well` inset; night field / border / ink are `flora::text_input`'s; ring ACC / GLOW appended after the field's own states | 92e74f261 / 9a8a33c0c |
| pagination | white / accent bar, system face by night; gained HT / PT hover + press on neutral pages and an inset 2px focus ring on every button (white on the accent page) | raised paper pages (BD2 joined hairline, 3px outer corners), current page = sunken stone (ON_ACC), unreachable end = DISBG / DISTX, HOVER / PRESSED faces, inset ring ACC / GLOW; click restyle writes flora colours | e8ef3e2e7 / 817a22b6a |
| radio_group | #9b9b9b ring + accent dot, unchanged; rows gained ring slot, 1px/4px inset, FIELD_RING / DARK_ACC ring | indicator = well of field paper (FLD / DARK_FLD, BD3, `--fl-well`) holding an accent stone dot (ACC + orb gloss, both modes); labels ink; rows wash to `--fl-hov` and ring ACC / GLOW. Both keep the 16px, `flex-shrink: 0` indicator | eb21dd0d1 / be8d8ae0c (+ pin 0e8f49695) |
| segmented | white segments / accent choice, system face + accent by night; gained HT / PT hover + press and an inset focus ring (white on the choice) | raised paper segments, choice = sunken stone, HOVER / PRESSED faces, inset ring; selection restyle writes flora colours | 3271883b5 / ec82375ee |
| split_pane | #adb5bd 6px bar, system separator by night; on focus the whole bar lights FIELD_RING / DARK_ACC plus inset ring | channel: STRIP / DARK_STRIP between two BD hairlines on the long sides, border-box (thickness unchanged for the drag math), HB hover, PT while dragged, inset ring ACC / GLOW | ae8c11e8a / 7a1b01231 |
| stepper | accent / #e9ecef circles, system highlight by night; cells gained an inset focus ring (`:focus` only - resting cell unchanged) | reached = raised accent stones (stone_face + streak, ON_ACC) on an accent line; upcoming = raised paper, SOFT1 numbers, BD line; BD2 hairline inside the 28px circle; labels INK / SOFT1; cells wash `--fl-hov` and ring ACC / GLOW; click restyle writes flora colours | 65b9dd8da / d345a46c9 |
| time_picker | #ced4da frame, grey arrows, accent AM/PM; arrows gained HT / PT hover + press, arrows and toggle an inset focus ring | well of field paper (FLD / DARK_FLD, BD2, `--fl-well`), ink readouts, soft-ink `:`, icon-ink arrows turning ink on a raised hover face, raised paper AM/PM toggle, rings ACC / GLOW. Arrows keep the 40x16 hit box in both | 6b6097192 / e76ea579e (+ 537b7742b) |
| toast | kind's alert palette by day, deep tint by night, unchanged; the "x" gained a 2px focus halo (`:focus` only) | every kind a leaf (SUR, INK, BD2, 3px, shadow-2) with the kind as a 3px thread in the left margin in flora's house hues: accent (glow by night), leaf #44684F/#7FA98C, brass #9A8B5F/#C4B58E, clay #7E4A42/#B3837A; brass-ink close ringed ACC / GLOW | d95b9bdb7 / dc4e75e66 |
| tooltip | translucent #333 chip, white text - its own colour in both modes (no focusable) | ink panel (code-bg / fg / bd, day and night), 3px, shadow-1. Both keep placement and start at opacity 0, so enter / leave are unchanged | 89d72092d / e686346fd |
| video | "no signal" poster = #2a2a30 screen, #44444c hairline, both modes | poster = ink panel (code-bg / bd, day and night). The theme travels in `VideoWidgetState.theme` (adopted by `merge_video_state`), so the VirtualView render callback draws the poster in it | 3b8d94b79 / 677ecc4c9 |

Other commits: progress checkpoints (808ba08aa ... f5889dc16), theme_checks loop fix
(fde9aea17).

Existing pins adjusted in the impl commits (the behaviour they pinned moved on purpose):
pagination "dark twins follow the light face" now allows state rules after the twins;
segmented / split_pane rendered-style pins compare the RESTING declarations; toast close pin
compares resting declarations; split_pane / time_picker / radio_group exact root-class pins
include `__azul-theme-flat`; time_picker's flattened-node class check ignores the marker.

## Decisions: camera, microphone, screencap, map, node_graph - no theme option

- **camera, screencap**: `dom()` is a single `<img>` fed by the capture thread (a null
  placeholder image until the first frame). No chrome, nothing focusable; the app builds and
  names the controls. A theme field would be API that changes nothing.
- **microphone**: an invisible node. Nothing to theme.
- **map**: its look IS its cartography, already themed by `MapTheme` / `map_themes.rs` (MapCSS
  sheets, light and dark presets) - a UiTheme next to it would be a second, confusing theme
  knob. Its only widget chrome is the loading-tile placeholder (`#e7e9ec` / `#d0d4d9`, glyph
  `#888`), which is LIGHT-ONLY: follow-up - derive it from the active MapTheme's `canvas`
  colour so a Dark map loads dark tiles (no UiTheme needed).
- **node_graph**: a theme makes sense in principle (canvas, node cards, ports, wires; it hosts
  themed field widgets), but not as a UiTheme bolted on now: its card chrome is TRANSPILED CSS
  (`render_node`'s hashed `STYLE_*` consts, ~46 colour sites), it is a dark Blender-style editor
  in both modes, and it already has its own knob, `NodeGraphStyle` (repr(C) enum, one variant,
  "to be extended"). Route: add `NodeGraphStyle::{Flat, Flora}` variants that pick a
  regenerated card stylesheet each, and forward the matching UiTheme to the field widgets
  (CheckBox / TextInput / NumberInput / ColorInput / FileInput already take themes). One
  enum-variant api.json change instead of a new field.

## API for autofix (all repr(C) structs; docs ASCII only)

`theme: OptionUiTheme` is APPENDED as the LAST field of each struct below. OptionUiTheme is
8 bytes, 4-aligned; every struct here is 8-aligned with a size that is a multiple of 8, so each
grows by exactly 8 bytes and no padding is introduced (existing field offsets unchanged). Field
doc for every one: "The widget theme, or `None` for the default (`UiTheme::Flat`). ..."

| struct (api.json class) | fields after the change, in order |
|---|---|
| Dialog | dialog_state, title, content, invoker, show_close_button, anchor, panel_style, backdrop_style, **theme** |
| Modal | modal_state, title, content, show_close_button, backdrop_style, **theme** |
| Popover | popover_state, anchor, content, wrapper_style, content_style, **theme** |
| NumberInput | number_input_state, text_input, style, accessibility_name, **theme** |
| Pagination | pagination_state, container_style, **theme** |
| RadioGroup | radio_group_state, options, container_style, accessibility_name, **theme** |
| Segmented | segmented_state, labels, container_style, **theme** |
| SplitPane | split_pane_state, first, second, container_style, **theme** |
| Stepper | stepper_state, labels, container_style, **theme** |
| TimePicker | state, container_style, accessibility_name, **theme** |
| Toast | toast_state, message, kind, dismissible, container_style, **theme** |
| Tooltip | anchor, text, wrapper_style, tip_style, **theme** |
| VideoWidget | config, on_frame, frames, on_mount, on_status, **theme** |

Methods, on each of the 13 (same shape as Button's entries):

- `set_theme`: fn_args `[{"self": "refmut"}, {"theme": "UiTheme"}]`, fn_body
  `object.set_theme(theme)`, doc "Pick the widget theme. Unset (`None`), the <widget> renders in
  the default theme (`UiTheme::default()`, flat)." (`const fn` in Rust.)
- `with_theme`: fn_args `[{"self": "value"}, {"theme": "UiTheme"}]`, returns the struct, fn_body
  `object.with_theme(theme)`, doc "[`Self::set_theme`] for the builder chain."

Rust-only (not in api.json): `VideoWidgetState.theme: UiTheme` (new last field; the struct is
not repr(C)); new `pub mod themes::style_kit`; `pub` fns `flat::{dialog, modal, popover,
popover_panel_style, number_input, pagination, radio_group, segmented, split_pane, stepper,
time_picker, toast, tooltip, video}` and the same in `flora`, plus `flora::selected_stone`,
`flora::SELECTED_STONE_GEM`, `flora::DIALOG_BACKDROP`. Behaviour notes for the docs:
`Modal::resolved_backdrop_style`, `Popover::resolved_content_style`,
`TimePicker::resolved_container_style`, `Toast::resolved_container_style` and
`Tooltip::resolved_tip_style` now answer for the widget's theme (unchanged for flat / None).

## Least sure to compile

1. `style_kit`'s `const fn`s returning arrays of `CssPropertyWithConditions`
   (`padding`, `radius`, `focus_ring`, `hover_border`, `themed_ink`, `ink`, `font_size`,
   `weight`) and `marker` (a `match` inside `AzString::from_const_str`) - modelled on
   `flat::focus_border_both`, which is const too.
2. Glob-imported names used for the FIRST time in `flat.rs` / `flora.rs`: `StyleTextAlign`,
   `StyleUserSelect`, `StyleOpacity`, `StyleWhiteSpace` (+ `property::StyleWhiteSpaceValue`),
   `LayoutMarginBottom/Left`, `LayoutInsetBottom`, `LayoutRight/Top/Left`, `LayoutPosition`,
   `LayoutBoxSizing`, `LayoutMin/MaxWidth`, `LayoutFlexShrink`, `LayoutAlignSelf`,
   `LayoutJustifyContent`, `FloatValue`, `StyleFontWeight`. Each is defined once in the globbed
   modules, but an ambiguity between two globs would only show at compile time.
3. `theme_checks`: `v.get_property()?.as_ref()` on `BoxOrStatic<StyleBoxShadow>` (inherent
   `as_ref`), `.number.get()` on `FloatValue`, closure patterns over
   `iter_inline_properties()` items.
4. fn-pointer skins: `const fn` items coerced to fn pointers
   (`stepper::established_connector_fill`, `flora::stepper_label_ink`); `#[derive(Debug,
   Clone, Copy)]` on `PaginationSkin` / `SegmentedSkin` with fn-pointer fields.
5. `video_widget_render`: the poster closure reads `s.theme` from the downcast guard inside
   `map_or_else` (nested closure borrow).
6. `number_input` theme fns: `Dom::add_class` / `Dom::add_css_property` on the TextInput's
   returned Dom; `TEXT_INPUT_CONTAINER_PROPS` / `TEXT_INPUT_LABEL_PROPS` are per-OS
   `pub(crate) static`s (all three cfg variants exist).
7. `pub(crate)` skin structs with `pub` fields returned by `pub(crate)` fns in `pub` modules
   (same shape as the existing `DialogParts`).

## What is left / follow-ups

- Wire the API list above through `azul-doc autofix` (13 fields + 26 methods); the dll's
  generated mirrors change size by 8 bytes per struct.
- Flora's pre-existing shared `FOCUS_BORDER_*_DARK` (buttons, text fields) is `DARK_ACC`
  (#2F4A85), barely visible on flora's #232323 ground; my widgets use `DARK_GLOW` as flora.css's
  `--focus-color` says. Aligning the shared consts touches the 19 older widgets - not done here.
- A flora number field's NIGHT face is `flora::text_input`'s (DARK_SUR), not DARK_FLD.
- The pagination / segmented / stepper click restyles write plain overrides, which outrank
  hover / pressed rules on the restyled nodes afterwards (pre-existing, both themes).
- Flora's two-layer shadows are reproduced with their first layer only (one shadow slot per
  node); a popover's shadow can be clipped at its popup window's edge.
- Map placeholder tiles are light-only (see decisions). node_graph: the `NodeGraphStyle` route
  above.
- The AzWidgets demo (`examples/azul-widgets/`) was not touched (not allowed); it can now show
  the flora variants via `.with_theme(UiTheme::Flora)`.

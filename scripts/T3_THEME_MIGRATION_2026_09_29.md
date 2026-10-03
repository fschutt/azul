# T3 - the `@theme` migration, second half of the widgets (2026-09-29)

Branch `wt/t3-theme-migration`, cut from `fix/input-bugs-2026-09-19` @ 36ce2f698 (step 0 done).
Recipe: scripts/T1_APP_THEME_2026_09_29.md §4. Nothing was compiled (house rule); every behaviour
change is a RED test commit followed by its implementation commit. api.json was NOT touched - the
list for autofix is in §5.

## 1. What an app gets

Every widget below, built with no `with_theme`, now follows the APP theme
(`AppConfig::with_theme`, `CallbackInfo::set_theme`): it is built in the STRUCTURE of the theme
its DOM is built for (`UiTheme::current()`, so its root carries that theme's marker class) and
every node carries the declarations of BOTH themes, each theme's block conditioned
`@theme(flat)` / `@theme(flora)`. The cascade keeps the live theme's block. A widget pinned with
`with_theme(..)` is exactly today's path (no theme condition anywhere). `ComboBox` and
`FileInput` gained the theme option they lacked. `TextArea`, `Slider` and `ProgressBar` used to
default to `Some(Flat)`; they now default to `None` like every other widget (and an explicitly
unpinned `TextArea` / `Slider` no longer renders an empty div).

## 2. Design

### 2.1 One merge, two shapes (`themes/flat.rs`, new section `// ==== follow the app theme (T3) ====`)

- `follow_props(flat, flora)` merges one part's declarations; `follow_dom(structure, flat, flora)`
  merges two built DOMs node by node (the structure theme's tree, classes, callbacks, datasets
  and a11y are kept; only the inline styles are merged); `follow_app_theme(w, flat_fn, flora_fn)`
  builds a two-builder widget both ways under `UiTheme::current()` and merges.
- The merge decides PER PROPERTY: a property both themes declare alike (same declarations,
  conditions included, same order) stays unconditional, once; any other property is written per
  theme - flat's declarations of it in `@theme(flat)`, then flora's in `@theme(flora)`, each in
  its own order. Inline declarations resolve last-match-wins per property, so under either theme a
  followed node resolves exactly as that theme's own build (proof: for every property, the
  declarations that apply are that theme's, in that theme's order). State rules stay after their
  resting rules; dark twins stay in the same theme block as their light half, so the step-0
  `theme_pairs` lint (pairs per theme name) holds. The theme name goes first in every condition list
  (`in_theme` for props; `follow_rule_in_theme` for rule blocks). Multi-declaration rule blocks are
  split per declaration first (same path / conditions / priority; the cascade sorts stably).
- Children are paired by position only where both trees have the same number of children; a
  subtree only the structure theme builds keeps its styles as they are (the DOM is rebuilt on every
  theme switch, so it never answers for another theme) - e.g. a `SpinnerStyle::Auto` spinner is
  flat's ring or flora's spokes.
- Under a theme no widget knows (`"monokai"`), only the shared (unconditional) declarations apply -
  T1's documented sharp edge, unchanged.

### 2.2 Skin merge vs DOM merge

- Widgets built from a W3b SKIN merge the skin part by part (`follow_skin(structure)`): dialog /
  modal / popover, tooltip, split_pane, radio_group, time_picker, toast, pagination, segmented,
  stepper, combobox. The DOM is built ONCE; a container's caller content (a dialog's body, the
  panes of a split pane, a tooltip's anchor) is never cloned or walked. fn-pointer skins get merged
  fns (`follow_button`, `follow_segment`, `follow_cell/_circle/_connector/_label`,
  `follow_container`).
- Widgets whose flat and flora looks are two builders use the DOM merge (`follow_app_theme`):
  number_input, text_input, text_area, slider, switch, spinner, file_input. progressbar merges its
  two render cores inside the VirtualView callback (`render_virtual_view_following`, under
  `UiTheme::current()` of the pass that lays it out); video records `follows_app_theme` in its
  state and its render callback merges the two posters.
- Live restyles (pagination, segmented, stepper) keep reading the marker class
  (`style_kit::theme_of_classes`): the marker is the structure theme's, so a bar built for flora
  repaints in flora's colours (new tests pin this). The text input's invalid ring reads its marker
  the same way.
- Style resolvers that "answer from the skin the render uses" (`Modal::resolved_backdrop_style`,
  `Popover::resolved_content_style`, `Tooltip::resolved_tip_style`,
  `TimePicker::resolved_container_style`, the new combobox resolvers) answer with the follow skin
  when unpinned; `Toast::resolved_container_style` unpinned is the card the render carries (pinned
  flat stays the light face alone, as before).

### 2.3 Deviations from the recipe (decide)

1. **Runtime merge, not const statics.** Every skin these widgets had is built at runtime
   (`style_kit` returns `Vec`s), so the blocks are produced by `in_theme` / the merge (the
   recipe's runtime shape), not rewritten as `theme_conditions!` statics. The output shape is the
   recipe's (shared first, flat block, flora block). Rewriting all W3b skins as const statics is
   a mechanical follow-up if the ledger still wants it; the tests would not change.
2. **A caller's style in a DOM-merged widget** (TextInput's `container_style`, ...) is decorated by
   each theme with different dark twins, so for those properties it appears in both theme blocks
   (conditioned) rather than once unconditionally. Correct under flat and flora; under an unknown
   theme it is inert. Hoisting the common prefix would split light halves from their twins and
   trip the step-0 `theme_pairs` lint (pairs per theme name). Skin-merged widgets are unaffected
   (a caller style replaces the skin part and stays unconditional).
3. **DOM-merged widgets build twice**: an unnamed slider / switch emits its a11y warning
   (`warn_widget_needs_a_name`) twice per build. Fix outside my files: a thread-local "building
   the other theme's twin" flag in `widgets/mod.rs` that the warning checks.

## 3. Per widget

| widget | done | shape | notes |
|---|---|---|---|
| dialog / modal / popover | yes | skin (`dialog::follow_skin/_skins/skin_of`, `popover::follow_popover_skin`) | modal/popover default-resolver pins now pin flat |
| tooltip | yes | skin | 2 autotest pins -> flat |
| split_pane | yes | skin (`flat/flora::split_pane_skin(direction)` extracted) | 1 pin -> flat |
| radio_group | yes | skin (`flat/flora::radio_group_skin(horizontal)` extracted) | no pin needed |
| time_picker | yes | skin | 1 pin -> flat |
| toast | yes | skin (+ `follow_container`) | 8 pins -> flat |
| pagination | yes | skin (`follow_button`) | 7 pins; new: unpinned bar built for flora restyles in flora |
| segmented | yes | skin (`follow_segment`) | 6 pins; same restyle test |
| stepper | yes | skin (`follow_part` + 4 fns) | 11 pins; same restyle test |
| number_input | yes | DOM merge | no pin needed |
| text_input | yes | DOM merge (`dom_in(theme)`) | 2 pins; every kind incl. search / constrained |
| text_area | yes | DOM merge; default `None` | 1 pin |
| slider | yes | DOM merge; default `None` | 1 pin |
| switch | yes | DOM merge | the two looks are one today -> merged DOM unchanged; guard test |
| spinner | yes | DOM merge | structure follows too (ring / spokes) |
| progressbar | yes | VirtualView merge; default `None` | `flat::progressbar_mount` split out; 13 `render_bar` pins |
| video | yes | state flag + poster merge | `VideoWidgetState.follows_app_theme` (Rust-only) |
| combobox | yes | NEW theme option + skin | flat = established statics + inset focus ring on option rows; flora = field paper over a leaf; wrapper carries the marker |
| file_input | yes | NEW theme option; DOM merge of two pinned Buttons | follows even before T2 migrates Button |
| ribbon, quick_access, statusbar, tabs, titlebar, tree_view | no - nothing to migrate | - | NO `UiTheme` and NO flora look (the ledger's "HAVE" list counted their palette structs). A guard pins that they render the same under every app theme; each needs a flora look + theme option first (wave-2 gap) |

Tests: `layout/tests/widgets_follow_the_app_theme.rs` (+ `all.rs`): one generic check per widget
(`assert_follows_the_app_theme`) - unpinned and built for T it resolves like `with_theme(T)` on
every node, light and dark, at rest / hover / active / focus, same classes and a11y; it carries
both themes' blocks (unless the looks are one); a pinned widget carries no theme condition and does
not change with the app theme; the a11y tree (accessibility declarations) is the same under both;
plus a self-test that a widget stuck on flat fails the check. Unit tests: the merge
(`flat::follow_tests`), a renamed "without a theme follows the app theme, flat by default" test in
every W3b widget, the three restyle tests, progressbar's VirtualView, video's poster, combobox and
file_input theme suites.

## 4. Commits

f8fdaae5e plan · 26e1fe2e4 / f4cfefa2a merge helper · a413d272d / 575039720 dialog family ·
086a479be / a666b6f6e tooltip · 7ad8c09a1 / a23bdbe50 split_pane · 887b4dd6b / 4940304d8
radio_group · 9764b7244 / ff0ec96e2 time_picker · 887996171 / 26e03508e toast · eae92e842 /
d96fcca29 pagination · d141d7ebd / 986aeeac1 segmented · 0805ce5a5 / 37966b85d stepper ·
617f4cd10 / e5a3191b9 number_input · dec340a25 / 52ab897ab text_input · 5f4de8513 / 9fbc50759
text_area · 223e3c5d6 / d8dde2fae slider · 421fe7c4d / 1c0ace865 switch · 0348d8c3e / 49fe625f4
spinner · f07cd8fc7 check compares the a11y tree · 375e4141f / e0d855885 progressbar · 96a6ed0db /
7572be02f video · 10396f6d7 / ece968e41 combobox · 82dc350f3 / b02077edd file_input · 535919153
single-look guard · 2c137a878 style · da28d0906 test helper · + this report.

## 5. API for autofix (api.json)

Both structs are 8-aligned with a size that is a multiple of 8; `OptionUiTheme` is 8 bytes /
4-aligned, appended LAST, so each grows by exactly 8 bytes with no padding (offsets unchanged).

| class | item | spelling |
|---|---|---|
| ComboBox | struct field (LAST, after `accessibility_name`) | `theme: OptionUiTheme`, doc "The widget theme, or `None` to follow the app theme (`AppConfig::with_theme`). ..." |
| ComboBox | fn `set_theme` | `[{"self": "refmut"}, {"theme": "UiTheme"}]`, body `object.set_theme(theme)`, doc "Pick the widget theme. Unset (`None`), the combobox follows the app theme (`AppConfig::with_theme`, flat by default)." |
| ComboBox | fn `with_theme` | `[{"self": "value"}, {"theme": "UiTheme"}]` -> `ComboBox`, body `object.with_theme(theme)`, doc "[`Self::set_theme`] for the builder chain." |
| FileInput | struct field (LAST, after `image_style`) | `theme: OptionUiTheme` |
| FileInput | fns `set_theme` / `with_theme` | as ComboBox's (returns `FileInput`) |

Docs changed (autofix syncs them): the `theme` field / `set_theme` docs now say "follows the app
theme" on Dialog, Modal, Popover, NumberInput, Pagination, RadioGroup, Segmented, SplitPane,
Stepper, TimePicker, Toast, Tooltip, VideoWidget, Spinner, Switch (field doc was the track style's),
TextInput, TextArea, Slider, ProgressBar. Behaviour for the docs: `TextArea::create` /
`Slider::create` / `ProgressBar::create` default to `theme: None`.

Rust-only: `VideoWidgetState.follows_app_theme: bool` (appended); `pub(crate)`:
`themes::flat::{follow_props, follow_dom, follow_app_theme, progressbar_mount, split_pane_skin,
radio_group_skin, combobox_skin}`, `themes::flora::{split_pane_skin, radio_group_skin,
combobox_skin}`, `combobox::{ComboBoxSkin, skin_for, follow_skin, skin_of, MIN_WIDTH,
COMBOBOX_*_STYLE, build_list_style}`, the per-widget `follow_skin` / `skin_of` fns.

## 6. Least sure

Compile (in order of doubt):
1. `flat::follow_alike<T: PartialEq>(.., ty: impl Fn(&T) -> CssPropertyType)` with
   `.filter(|x| ty(*x) == t).eq(..)`, and `follow_rules` passing `&Vec<CssRuleBlock>` to it.
2. `follow_node`: `let mine: &mut [Dom] = built.children.as_mut();` (DomVec's `AsMut`).
3. `flat::progressbar_mount(bar, render: azul_core::callbacks::VirtualViewCallbackType)` and the
   private `extern "C" fn render_virtual_view_following` passed to it.
4. `state_poster_style(&s)` with `s` the `downcast_ref` guard (`Ref<'_, _>` deref coercion) inside
   the nested closure in `video_widget_render`.
5. `pinned(widget, t, Dialog::with_theme)` etc.: `const fn` items coerced to `fn(W, UiTheme) -> W`
   in the integration test.
6. `ComboBox::build` moving the skin's fields out (`skin.wrapper` ...) after reading `skin.theme`.
7. `stepper::follow_part(|s| (s.cell)())` - calling fn-pointer fields of a `Copy` skin.

Behaviour:
1. **theme_checks.rs** resolves only Light/Dark/pseudo conditions (`applies` returns false for a
   `Theme(Custom)`), so a `tc::*` probe on an UNPINNED widget sees only its shared declarations. My
   widgets' tc tests all pin a theme; T2's widgets that EMBED my widgets unpinned (ColorInput's
   hex TextInput and R/G/B/A NumberInputs, node_graph's fields, datetime_local pins its parts) may
   have tests that probe those children - they now carry follow-mode blocks.
2. A StyledDom created without a window context (unit tests using `StyledDom::create_from_dom`
   + the property cache) sees only unconditional declarations of a followed widget - the engine's
   documented rule. Layout tests through `LayoutWindow` get the app theme (default flat) and are
   fine; I found no layout test of my widgets that skips the window.
3. The autotest pins I changed are the ones I could see by reading; a test that reads a default
   widget's unconditional face somewhere I missed would fail under flat by missing declarations,
   not by wrong values - the fix is the same `.with_theme(UiTheme::Flat)` pin.

## 7. Left / open

- **The six single-look widgets** (ribbon, quick_access, statusbar, tabs, titlebar, tree_view)
  need a flora look and a theme option before they can follow; until then they look flat under a
  flora app theme - exactly the "out of place" case the ruling wants gone. Suggest a W3c agent.
- Const statics (§2.3.1), the caller-style repetition (§2.3.2), the doubled a11y warning (§2.3.3).
- The AzWidgets demo still pins every widget with `.with_theme(state.theme)` (recipe item 8):
  switching to `info.set_theme(..)` needs T2's half too.
- Merge with T2: both append sections at the end of `flat.rs` / `flora.rs` (keep both) and an
  `all.rs` line; T2 may add its own merge helper - dedup onto one (`style_kit` is the natural home).

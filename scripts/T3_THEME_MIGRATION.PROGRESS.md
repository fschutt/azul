# T3 - @theme migration, second half of the widgets (progress)

Branch `wt/t3-theme-migration`, cut from `fix/input-bugs-2026-09-19` @ 36ce2f698 (step 0 done).
Recipe: scripts/T1_APP_THEME_2026_09_29.md §4. Nothing is compiled here (house rule).

## Plan

- One shared merge in `themes/flat.rs` (new section at the END, `// ==== follow the app theme (T3) ====`):
  `follow_props(flat, flora)` (a part's declarations, for skins) and `follow_dom(structure, flat, flora)`
  (a finished DOM, for widgets whose flat / flora builders are separate functions). Per PROPERTY:
  identical in both themes -> unconditional once; otherwise flat's block `@theme(flat)` then flora's
  `@theme(flora)`. Under either theme a node resolves exactly as that theme's pinned build.
- Container widgets (dialog / modal / popover, tooltip, split_pane) merge their SKIN (the caller's
  content is never cloned or walked); the rest merge the two built DOMs.
- Tests: `layout/tests/widgets_follow_the_app_theme.rs` (+ `all.rs`), one generic check per widget:
  unpinned under app theme T resolves like `with_theme(T)` on every node, in light and dark, at rest
  and in every state; the DOM carries both blocks; a pinned widget ignores the app theme; the a11y
  tree is the same under both.

## DONE

- plan f8fdaae5e
- helper: RED 26e1fe2e4, impl f4cfefa2a (`flat::{follow_props, follow_dom, follow_app_theme}`)
- dialog / modal / popover: RED a413d272d (+ the integration file), impl: see git log
  (`dialog::{follow_skin, follow_skins, skin_of}`, `popover::follow_popover_skin`; the modal /
  popover resolvers answer the follow skin when unpinned; their two "default resolver = flat"
  pins now pin `with_theme(Flat)`)

- tooltip: RED 086a479be, impl: see git log (`tooltip::{follow_skin, skin_of}`; two autotest
  pins that compare against flat's const tables now pin `with_theme(Flat)`)

- split_pane: RED 7ad8c09a1, impl: see git log (`flat::split_pane_skin` / `flora::split_pane_skin`
  extracted inside their sections; `split_pane::{skin_for, follow_skin}`; the flat-divider pin
  now pins `with_theme(Flat)`)

- radio_group: RED 887b4dd6b, impl: see git log (`flat/flora::radio_group_skin(horizontal)`
  extracted; `radio_group::{skin_for, follow_skin}`; no existing pin needed changing - the
  default-widget tests read properties both looks declare alike)

- time_picker: RED 9764b7244, impl: see git log (`time_picker::{follow_skin, skin_of}`;
  `resolved_container_style` answers the follow skin; the const-container pin pins
  `with_theme(Flat)`)

- toast: RED 887996171, impl: see git log (`toast::{follow_skin, follow_container}`; unpinned
  `resolved_container_style` = the card the render carries; 8 autotest pins on the flat card /
  flat child styles now pin `with_theme(Flat)`)

- pagination: RED eae92e842, impl: see git log (`pagination::{follow_skin, follow_button}`; the
  restyle stays the structure theme's (marker); 7 autotest DOM-style pins now pin
  `with_theme(Flat)`; new: an unpinned bar built for flora restyles in flora's colours)

- segmented: RED d141d7ebd, impl: see git log (`segmented::{follow_skin, follow_segment}`; the
  restyle stays the structure theme's; 6 autotest DOM-style pins now pin `with_theme(Flat)`;
  new: an unpinned control built for flora restyles in flora's colours)

- stepper: RED 0805ce5a5, impl: see git log (`stepper::{follow_skin, follow_part, follow_cell,
  follow_circle, follow_connector, follow_label}`; restyle colours stay the structure theme's; 11
  autotest DOM-style pins now pin `with_theme(Flat)`; new: an unpinned stepper built for flora
  restyles in flora's colours)

- number_input: RED 617f4cd10, impl: see git log (DOM merge: `flat::follow_app_theme(self,
  flat::number_input, flora::number_input)`; no pin needed changing. NOTE: ColorInput's R/G/B/A
  fields and node_graph's fields build unpinned NumberInputs - they now follow the app theme)

- text_input: RED dec340a25, impl: see git log (`dom()` = `dom_in(theme)` pinned, else DOM
  merge of `dom_flat` / `dom_flora`; constrained fields keep the structure theme's marker for
  the invalid ring; 2 pins: the configured-styles test and the search clear-button display test
  pin flat. NOTE: a caller's container/label style is repeated in both theme blocks for the
  properties the two themes twin differently (dark bg/ink/borders) - see report)

- text_area: RED 5f4de8513, impl: see git log (`Default` theme `Some(Flat)` -> `None`; `dom()`
  None = DOM merge instead of an EMPTY div (a pre-existing bug: an explicitly unpinned area
  rendered nothing); 1 pin: the border-states test pins flat)

- slider: RED 223e3c5d6, impl: see git log (`create` theme `Some(Flat)` -> `None`; `dom()` None
  = DOM merge instead of an empty div; 1 pin: the verbatim-styles test pins flat)

- switch: guard 421fe7c4d (green before: the two looks are one), impl: see git log (DOM merge;
  merges to the flat DOM unchanged today)

- spinner: RED 0348d8c3e, impl: see git log (DOM merge; `Auto` differs in STRUCTURE (flat ring /
  flora spokes) - the structure theme's subtree is kept, its own component sheet (@keyframes) too;
  no pin needed changing)

- progressbar: RED 375e4141f, impl: see git log (`create` theme `Some(Flat)` -> `None`; unpinned
  `dom()` mounts `flat::progressbar_mount(bar, render_virtual_view_following)` - the VirtualView
  renders both themes' bars and merges them under `UiTheme::current()`; `render_bar()` merges
  the same way (`follow_bar`); 13 autotest `render_bar` mechanics pins now pin `FLAT`)

- video: RED 96a6ed0db, impl: see git log (`VideoWidgetState.follows_app_theme: bool` appended
  (Rust-only struct), adopted by `merge_video_state`; unpinned `dom()` = `build_in(current(),
  true)`; the render callback's poster = `follow_props(flat, flora)` when following)

- combobox: RED 10396f6d7, impl: see git log (NEW `theme: OptionUiTheme` LAST in the struct +
  `set_theme` / `with_theme`; `ComboBoxSkin` + `skin_for` / `follow_skin` / `skin_of`; flat skin =
  the established statics (now `pub(crate)`) + an inset focus ring on the option rows; flora skin
  = field paper over a leaf (new `// ==== combobox ====` sections at the END of flat.rs /
  flora.rs); the wrapper now carries the theme marker; resolvers answer for the theme)

- file_input: RED 82dc350f3, impl: see git log (NEW `theme: OptionUiTheme` LAST + `set_theme` /
  `with_theme`; the looks are the Button's (flat / flora, light + dark): pinned = a Button pinned
  to that theme; unpinned = DOM merge of the two pinned Buttons (so it follows even before T2
  migrates Button); the resolvers ask a Button in the input's theme)

- ribbon / quick_access / statusbar / tabs / titlebar / tree_view: NO UiTheme and no flora look -
  nothing to condition. Guard test (see git log): they render the same under every app theme and
  carry no theme blocks. Left for a follow-up: a flora look + theme option each.

## IN PROGRESS

- review pass over every commit, then the report

## NEXT

 stepper, number_input, progressbar, slider, spinner, switch, text_area,
text_input, video, combobox (+ theme option + flora look), file_input (same), then the six
single-look widgets (ribbon, quick_access, statusbar, tabs, titlebar, tree_view).

## Open questions

- ribbon / quick_access / statusbar / tabs / titlebar / tree_view have NO UiTheme and no flora
  look (the ledger's "HAVE" list counted their palette structs). Nothing to condition until a flora
  look exists.

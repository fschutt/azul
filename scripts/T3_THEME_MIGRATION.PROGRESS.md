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

## IN PROGRESS

- stepper

## NEXT

 stepper, number_input, progressbar, slider, spinner, switch, text_area,
text_input, video, combobox (+ theme option + flora look), file_input (same), then the six
single-look widgets (ribbon, quick_access, statusbar, tabs, titlebar, tree_view).

## Open questions

- ribbon / quick_access / statusbar / tabs / titlebar / tree_view have NO UiTheme and no flora
  look (the ledger's "HAVE" list counted their palette structs). Nothing to condition until a flora
  look exists.

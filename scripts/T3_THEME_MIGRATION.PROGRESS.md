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

## IN PROGRESS

- split_pane

## NEXT

split_pane, radio_group, time_picker, toast,
pagination, segmented, stepper, number_input, progressbar, slider, spinner, switch, text_area,
text_input, video, combobox (+ theme option + flora look), file_input (same), then the six
single-look widgets (ribbon, quick_access, statusbar, tabs, titlebar, tree_view).

## Open questions

- ribbon / quick_access / statusbar / tabs / titlebar / tree_view have NO UiTheme and no flora
  look (the ledger's "HAVE" list counted their palette structs). Nothing to condition until a flora
  look exists.

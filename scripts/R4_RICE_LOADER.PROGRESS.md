# R4_RICE_LOADER progress (branch wt/r4-rice-loader, base 0a326afe5)

Design: scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md (main checkout).
Report: scripts/R4_RICE_LOADER_2026_09_29.md.

## Decisions
- New module `css/src/rice.rs`: header parse, version gate, hardening, discovery over the
  theme chain (`~/.azul/css/<seg>/<seg>/*.css`), priority stamping + `@theme(n)` wrap,
  per-app precedence, status listing, process-global loaded rice + watch polling.
- Loaded rice is PROCESS-global (like `azul_core::app_theme`), not a `SystemStyle` field:
  a watch reload must reach every window, and the chain head is the app's theme.
- Applied in `LayoutWindow::style_user_dom_in_scope` through a new core entry point
  `StyledDom::create_from_dom_with_user_sheets` (unscoped, appended after the DOM's sheets).
- Legacy `styles/<app>.css` keeps loading, as a per-app, unthemed file. The three shell
  twins `load_app_specific_stylesheet` are deleted; `SystemStyle.app_specific_stylesheet`
  stays for ABI, no longer filled.
- Palette slot = 35 (same as widgets) so rank decides between a spin-off and its base.
- Watch: thread polls, wakes the loop, app-event collector runs the SetTheme rebuild
  (extracted into `PlatformWindow::rebuild_all_windows_for_app_theme`), and
  `regenerate_layout` upgrades a lagging window to `AppThemeChange` by rice generation.

## DONE
- 5b2670ce7 RED tests (css/tests/rice_loader.rs, layout/tests/rice_styles_the_window.rs)
- 51cbb1f07 css: rice loader
- bed897f0f core+layout: user-origin sheets, the hook, rice_generation
- 6762b0e12 dll: install, watch, shell loaders removed, SetTheme tail extracted
- 8c278fd2c docs: ricing guide page
- 7f6fd5724 rustfmt rice.rs
- report scripts/R4_RICE_LOADER_2026_09_29.md

## IN PROGRESS
- (none)

## NEXT
- Parent: merge R3 (css crate needs `theme_chain::expand_chain`), then run the test
  commands in the report.

## Open questions
- R3 `expand_chain` signature assumed: `names`/`warnings` iterable of String-likes.

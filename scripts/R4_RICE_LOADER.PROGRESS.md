# R4_RICE_LOADER progress (branch wt/r4-rice-loader, base 0a326afe5)

Design: scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md (main checkout).

## Decisions
- New module `css/src/rice.rs`: header parse, version gate, hardening, discovery over the
  theme chain (`~/.azul/css/<seg>/<seg>/*.css`), priority stamping + `@theme(n)` wrap,
  per-app precedence, status listing, process-global loaded rice + watch polling.
- Loaded rice is PROCESS-global (like `azul_core::app_theme`), not a `SystemStyle` field:
  a watch reload must reach every window, and the chain head is the app's theme.
- Applied in `LayoutWindow::style_user_dom_in_scope` through a new core entry point
  `StyledDom::create_from_dom_with_user_sheets` (unscoped, appended after the DOM's sheets;
  pushing it onto the root `Dom` would scope a `* {}` rule at >= INLINE to the root node).
- Legacy `styles/<app>.css` keeps loading, as a per-app, unthemed file. The three shell
  twins `load_app_specific_stylesheet` are deleted; `SystemStyle.app_specific_stylesheet`
  stays for ABI, no longer filled.
- Palette slot = 35 (same as widgets) so rank decides between a spin-off and its base.
- Watch: a thread polls a fingerprint, wakes the loop (`loop_waker::wake`), the app-event
  collector runs the SetTheme rebuild (extracted into one PlatformWindow method), and
  `regenerate_layout` upgrades a lagging window to `AppThemeChange` by rice generation.

## DONE
(none yet)

## IN PROGRESS
- RED tests: css/tests/rice_loader.rs, layout/tests/rice_styles_the_window.rs

## NEXT
- rice.rs implementation, core entry point, layout hook, dll wiring, docs, report.

## Open questions
- R3 `expand_chain` signature assumed: `names`/`warnings` iterable of String-likes.

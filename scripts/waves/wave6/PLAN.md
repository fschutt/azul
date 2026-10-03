# Wave 6 - "finish off all apps now that we have all the building blocks" (2026-10-03)

User, 2026-10-02: "start another wave of subagents to finish all follow-ups and get closer to Chrome parity for mail.
The next wave should then properly finish off all apps now that we have all the building blocks and breaking engine
fixes fixed." Wave 5 (building blocks + engine fixes) is integrated and pushed (aa59b2d84; see ../wave5/STATUS.md).

Every app report so far ends with "the look is written, not seen": the apps were written blind. Since aa59b2d84 the
dylib and ALL 20 apps build, so wave 6 starts from what the apps really do: each app agent first runs its app headless
(scripts/waves/tools/run_capped.sh, the debug server's `screenshot` op), LOOKS at the screenshots (flat + flora,
light + dark), writes down what is broken, then fixes it - RED first, root causes in the engine where they live.

## The eleven tasks and who owns what (parallel; stay in your files, append-only in shared lists)

| Task | Owns | Goal |
|---|---|---|
| INFRA6 | examples/azul-storage, examples/azul-appkit, layout/src/window_state.rs, layout/src/widgets/close_guard.rs, the close paths in dll/src/desktop/shell2 | `.azlin/cache` manifest in the drive; one data root + legacy-folder migration; one id mint; CLOSE: remove `close_callback`, every backend dispatches `CloseRequested` and honours `prevent_window_close` |
| MAIL6 | examples/azul-mail | AzMail like Outlook 2010 (the plan's goal), To-Do bar on the shared task store, close via the event, `__azmail_` prefixes, appkit, the sanitizer's class prefix |
| MEETDRIVE6 | examples/azul-meet, examples/azul-drive | AzMeet and AzDrive fully working |
| SHEETSHOW6 | examples/azul-sheets, examples/azul-show | AzSheets and AzShow finished (S3 blockers, export into the tree, missing dialogs/features) |
| MEDIA6 | examples/azul-photo, examples/azul-videocut, examples/azul-paint (+ an engine text-raster API) | AzPhoto, AzVideoCut, AzPaint finished |
| WRITER6 | examples/azul-writer, examples/azul-notes, layout/src/widgets/rich_text*, core/src/events.rs Undo/Redo shortcut block | AzWriter on the RichTextEditor, formats from the engine, Ctrl+Z reaches an app's own history |
| PIM6 | examples/azul-calendar, examples/azul-tasks, examples/azul-contacts, examples/azul-pim, a new RecurrenceEditor widget | the PIM apps finished, Calendar on the Drive + a Thread |
| SMALL6 | examples/azul-{calculator,setup,builder,review,maps,shells,widgets} | the small apps finished |
| MAILENG6 | layout/src/solver3, layout/src/text3, css (not widgets, not page_breaks.rs) | mail HTML at Chrome parity: the engine leftovers of MAILHTML / TABLES, measured on the corpus |
| AUTOFIX6 | doc/src/autofix, doc/src/patch | the six autofix gaps of the wave-5 integration, RED unit tests each |
| HEADLESS6 | layout/src/e2e, the headless backend dll/src/desktop/shell2/headless, dll/src/desktop/shell2/run.rs | the /e2e corpus (22 of 62 failing), the exit segfault, menus in headless, a window id for layout callbacks |

## Contracts between tasks (so nobody waits for anybody)
- STORAGE: apps write durable data - exports included - through the azul-storage `Drive` into the data tree; INFRA6 makes
  the `LocalDrive` keep `<root>/.azlin/cache` itself. Apps call nothing new for it.
- DATA ROOT: apps take their root from azul-appkit (`azul_appkit::data::data_root` through appkit's startup config, as
  AzCalculator and AzContacts do); INFRA6 hooks the one-time legacy-root migration (`azul/`, `Azul/` -> `Azlin/`) into
  that same startup path without changing its signature.
- CLOSE: apps veto in a `WindowEventFilter::CloseRequested` callback with `info.prevent_window_close()` (exists) or use
  the `CloseGuard` widget (exists). INFRA6 makes every backend honour it and removes `close_callback`.
- PIM: `azul_pim`'s existing public API (task store included) stays; PIM6 only adds. MAIL6 builds AzMail's To-Do bar on it.
- ENGINE: MAILENG6 = solver3/text3/css layout; HEADLESS6 = e2e/headless/run.rs/events plumbing; WRITER6 = the rich_text
  widget + the Undo/Redo shortcut block in core/src/events.rs; MEDIA6 = a text-raster API on images. A bug found in
  another task's area goes into your report for its owner (and the ledger), not into a parallel fix.
- PREFIX: each app defines its class/id constants once (`const X: AzString = AzString::from_const_str("__azmail_x")`).

## Parent-side (not agents)
Verification of wave 5 (suites, calculator E2E, AzMail send test, mail corpus re-measure vs 625); the integration of
wave 6 (merge each branch, autofix rounds, build, suites, E2E, push); the PGO run (scripts/pgo/chain.sh, after the
dump_profile fix); the memory follow-up (about half of each app's ~100 MB footprint is malloc "reclaimable" after
startup - find the startup peak).

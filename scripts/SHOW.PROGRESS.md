# SHOW progress (AzShow, the PowerPoint clone) - branch `wt/show` from `16d19442c`

Brief: scratchpad `SHOW_go.md`; house rules: scratchpad `wave4_common.md`. Report: `scripts/SHOW_2026_10_01.md`.

## Decisions (taken unattended, noted here)

- Name: package `AzShow`, lib `azshow`, bin `AzShow` (the user's name; the ledger's A9 "AzSlides" is the same app).
- Shell: S1 `DocumentShell` (ribbon / backstage / navigation = slide rail / document = slide canvas + notes /
  side pane = format pane / status bar), `ShellThemeScope` root, `Titlebar` under `WindowDecorations::NoTitle`.
- Text boxes reuse AzWriter's rich text IR (`azwriter::ir`, a path dependency on the `azwriter` lib) for
  in-place editing (to_content_dom, set_run_text / sync_block_text, apply_operation, toggle_format_range);
  no second rich-text model. Promote `ir` into azul as the RichTextEditor model later (report).
- The slide is rendered at the view's scale by the renderer (every px multiplied), not with
  `transform: scale` (hit-testing and carets stay in plain px); rotation is `transform: rotate`.
- Canvas interaction is model-based (bento's way): the new azul widget `SelectionAdorner` hit-tests the
  frames it is given, draws handles / guides / marquee and reports select / transform / commit / marquee /
  activate / nudge / delete events in slide units. The app owns the model.
- The slide rail and the sorter are the new azul widget `ThumbnailStrip` (column / grid, sections,
  multi-select, keyboard, drag reorder through the DnD events).
- Data: `<data dir>/azul/show/<uuid>/deck.json` + `media/` through azul-storage `LocalDrive` from a Thread
  (`AZSHOW_DATA` overrides the root).

## DONE

- b704d7531 test(azshow): the deck model (RED) + crate registration (workspace, test members, CI step)
- 5c4aeb8b8 feat(azshow): the deck model (GREEN) - type-checked standalone with rustc --emit=metadata
  (serde stripped; no codegen, no target dir)
- SelectionAdorner: 24d2b2676 (types), a07776b1c (RED geometry/drag), b35b13cda + ba99af95d (GREEN),
  487b2ffbb (RED DOM + registration in widgets/mod.rs: module, manifest, CHROME), 2d7c9f040 (GREEN DOM +
  callbacks), bcf09c2a9 (flat + flora looks).

- ThumbnailStrip: 302f45ee7 (types), 04138ad8b (RED + registration), 5dc922ace (GREEN), 3a4987b49 (looks).

## IN PROGRESS

- The app UI (`examples/azul-show/src/`). DONE: text.rs 0e128010c, render.rs 1441e2df3, storage.rs
  c75706697, args.rs 44196ab6d, editor.rs a4e73ff3b (pure session + tests), themes.rs 57a7d5570.
  (scratchpad tc_pure.py type-checks model + editor + themes standalone; gen.py lists generated API.)
  app.rs ea1a81bd9, commands.rs 9267cb03e, ribbon.rs 383775a66, backstage.rs b3581f940.
  NEXT STEP, one commit each: `views.rs` (normal: rail ThumbnailStrip + SelectionAdorner canvas with
  `SLIDE_ID` + notes TextArea + format pane + status bar; sorter; outline; notes page; the adorner /
  strip / notes / outline / text-edit extern callbacks), `show.rs` (show screen + presenter layout,
  keys, play tick, presenter timer), `lib.rs` (mod decls, main layout, presenter_layout,
  on_presenter_created, on_play_tick, focus_text_soon, spawn_storage + storage thread + writeback,
  window keys, start). lib.rs is referenced by commands.rs (crate::presenter_layout,
  crate::on_presenter_created, crate::on_play_tick, crate::focus_text_soon, crate::spawn_storage)
  and views::SLIDE_ID.

## NEXT

5. scripts/azshow_e2e.py, the report (`scripts/SHOW_2026_10_01.md`)

## Open questions

(none)

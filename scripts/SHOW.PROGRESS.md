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

- The app UI (`examples/azul-show/src/`). NEXT STEP, in this order, one commit each:
  1. `text.rs`: TextBody <-> azwriter::ir::IrDocument, the text box DOM (ir::to_content_dom + p/li per
     paragraph with ids `tb<element id>-<i>`), sync of the engine's text edits, structural edits, formats.
  2. `render.rs`: slide -> Dom at a scale (absolute boxes; shapes; images; tables; placeholders; the
     theme background; build visibility for the show).
  3. `storage.rs`: data root, LocalDrive, save / load / list decks + media from an azul Thread.
  4. `args.rs` (like AzWriter's), `themes.rs` (deck themes from ShellThemeAccent), `app.rs` state +
     undo, `ribbon.rs`, `backstage.rs`, `views.rs` (normal / sorter / outline / notes), `show.rs`
     (slide show + presenter window, timers), `lib.rs` (layout, callbacks, start).

## NEXT

5. scripts/azshow_e2e.py, the report (`scripts/SHOW_2026_10_01.md`)

## Open questions

(none)

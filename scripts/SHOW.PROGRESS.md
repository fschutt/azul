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

## IN PROGRESS

- SelectionAdorner widget (`layout/src/widgets/selection_adorner.rs`)

## NEXT

2. SelectionAdorner RED + GREEN (+ flat / flora looks, manifest)
3. ThumbnailStrip RED + GREEN
4. the app UI (ribbon, backstage, normal / sorter / outline / notes views, show + presenter window, storage, PDF)
5. registrations (workspace, test members, CI), E2E script, report

## Open questions

(none)

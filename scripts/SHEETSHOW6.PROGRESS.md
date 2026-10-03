# SHEETSHOW6 progress (wave 6, 2026-10-03)

Branch `wt/sheetshow6` from `25d78e309`. Brief: scripts/waves/wave6/SHEETSHOW6.md.
Look tools (not committed): target/sheetshow6-tools/{look.py,dbg.py,tree.py,parse.sh,*_steps*.py};
screenshots + hierarchy dumps under target/sheetshow6-shots/.

## DONE
- eaf508b38 progress file
- 5e5b20bed RED / 9207ff00f GREEN: a ragged block of inputs reaches the sheet (the Budget sample showed only A1:
  IronCalc's paste reader drops a record of another length); one TSV encoder `model::tsv_of` (D18 / N5).
- ff114f1ac AzSheets zoom slider hook (brief item).
- 2a11b594a AzSheets body margin 0 / height 100% (status bar was off-screen, backstage 464 px tall), name box in a
  fixed box (it took half the formula bar), "functions" icon.

- 58675fc89 RED (cell_grid): `every_cell_is_named_by_its_place_and_carries_its_text_as_its_value`,
  `a_text_too_wide_for_its_cell_spills_over_the_empty_cells_after_it` + stub `spill_spans` (returns all 1).

- 285061c31 GREEN 1/2 (cells named), 4a7cedcfd GREEN 2/2 (spill_spans + build draws a spill as one wide cell).
- 098f2be5b RED / fbefcd404 GREEN: StatusBarZoom names its slider "Zoom".

- 3e7010614 RED / 8d03d7653 GREEN: RibbonButton.alt (icon-only buttons named; launcher + gallery spinners named).
- c66fec68c AzSheets HOME = Excel 2010 icon rows (~960 px; needs api.json RibbonButton.alt / set_alt / with_alt).
- f2388e8ef RED / bb1fbb5fd GREEN: AzShow File > Open reads every page (ops::list_all + deck_id_of).

- AzSheets on appkit: d265c5ccb (id mint, dep), 30a45f3d4 (args over AppArgs; --data-dir replaces AZSHEETS_DATA),
  8c473405c (facts ABOUT / SHORTCUTS, AppState { drive, kit, asking_close, close_after_save }), 3236a60c2 (every
  Post / Job through the shared drive; exports -> sheets/exports/<name>.<ext>), 484743331 (start / startup on the
  kit), 480913b53 (Options = kit::settings_page, About = AboutDialog, VIEW > Look removed), 797557784 (Mod+, / F1).

## IN PROGRESS
- (power warning 2026-10-03: battery ~16 %; NO headless app runs until told otherwise. The AzShow look run was
  killed before it produced anything; AzShow is NOT looked at yet.)

## NEXT (exact)
1-3. (done)
4. AzSheets: S-A7 close guard (layout(): wrap the shell in CloseGuard::create(content, title).with_dirty(doc.dirty)
   .with_asking(asking_close).with_on_event(app, on_close_guard); Ask -> asking_close = true; Save -> save with
   close_after_save = true, apply_reply's Pending::Saved closes the window (info.close_window()); Discard -> dirty
   false; Cancel -> asking_close false); S-A8 `__azsheets_` id / class consts (an `ids` module, AzString consts).
   Then the same for AzShow (+ exports PDF / PNG into show/exports/ through the drive instead of FileDialog).
5. When power allows: LOOK at AzShow (target/sheetshow6-tools/show_steps1.py, fixed ready check) and re-look at Sheets.

## Broken (seen, 2026-10-03, prebuilt aa59b2d84, headless 1280x800)
AzSheets:
- [FIXED 9207ff00f] sample data invisible below A1 (ragged TSV).
- [FIXED 2a11b594a] status bar off-screen / backstage half height (body margin + no height).
- [FIXED 2a11b594a] name box 600 px wide; "function" icon missing.
- HOME ribbon wider than 1280: Cells + Editing groups off-screen -> "Sort A to Z", "Filter", "Find" unclickable.
- A1's title clipped at the cell edge (Excel spills text over empty neighbours).
- every GridCell anonymous (a11y-shape warning per cell per frame); the zoom Slider unnamed (a11y-widget).
- ENGINE (owner HEADLESS6 / MAILENG6): the headless screenshot does not show the current layout: the hierarchy dump's
  rects are right (ribbon groups adjacent, grid columns aligned, backstage nav on the left) but the PNG shows stale
  geometry - ghost text drawn twice (ribbon labels garbled after a tab switch / theme switch), grid lines misaligned
  per row, the backstage nav painted on the RIGHT at x=1137 with a second title strip. Evidence:
  target/sheetshow6-shots/look1/{07-flat-dark,12-editing-suggestions,13-bs-info}.png vs the .json dumps beside them.

## Decisions
- LOOK sessions are scripted (one bounded run_capped call each), not interactive: the machine-wide lock had a queue
  of ~10 runs; an interactive 15-minute hold would starve the other agents.

## Open questions

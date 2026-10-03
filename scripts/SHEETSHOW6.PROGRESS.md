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

- 182c53ded AzSheets close guard (CloseGuard; Save closes after the write; failed save keeps the window).
- 240901643 AzSheets `__azsheets_` ids (src/ids.rs).
- 3dfb544fb scripts/azsheets_e2e.py rewritten on azlin_e2e (D29) with the wave-6 checks (NOT run: power).

- cf5d4b1ce both apps mint ids with azul_storage::ids::new_uuid (INFRA6's one mint; builds after wt/infra6 merges).
- AzShow on appkit: aa5521bf4 (args over AppArgs + --slide / --no-presenter), cde847864 (AppState drive / kit /
  close flags; About page; theme commands gone), 4e6a111fc RED / b7430bc99 GREEN (Job::Export into show/exports/),
  30df953eb (start on the kit, jobs on the drive, close guard), 732beed52 (exports through the drive, no
  FileDialog::save_bytes), cf3f7a6fa (Options = kit page, About = AboutDialog), 0772d3897 (`__azshow_` ids),
  bcf486aab (azshow_e2e.py on azlin_e2e, NOT run: power).

- 2884a05fe RED / 379115fe7 GREEN / f59a06873: ops::find_match (case / whole word / backwards), replace_text,
  replace_all (one undo step); 26c4ddd8f worker Command::Find opts / Replace / ReplaceAll; b0d33c710 the side
  panel's Find is the standard FindReplaceDialog (Mod+F / Mod+H, HOME > Replace).
- Resumed after the power loss (coordinator, 2026-10-03): power back; the uuid note is done (d265c5ccb dropped the
  crate, cf5d4b1ce new_uuid).

- b2358810a RED / 7c75b824e GREEN: RibbonGallery.visible (the row of the selected cell; More shows all);
  fa94952ef AzShow galleries show 3-6 cells (every tab fits 1280 px; api.json RibbonGallery.visible / set_visible /
  with_visible); b02c045e8 AzShow icon-only buttons named (icon_button + with_alt).

- d335b8d7c AzShow New's theme cards show the accents.
- 0bd51d09f RED / e51203add GREEN: refs::cycle_reference (F4); 3e7a1e1b3 F4 wired in on_window_key (editing).
- c0fff7291 + 6780b41c7 RED / e005cb20d GREEN: CellGrid point mode (click / drag / arrows insert references;
  CellGridDragKind::Point - api.json). AzSheets needs nothing more (EditText events store the view).

## IN PROGRESS
- (none)

## NEXT (exact)
1-3. (done)
4. (done)
5. NEXT: Sheets Format Cells dialog, merge cells, conditional formatting, tab strip; Show (drop indicator, multi-select
   rotate, tables in place, picture contain / cover, find / replace, presenter on a chosen monitor).
6. Check both apps for `ctrl || meta` and duplicated helpers (checklist).

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

AzShow (looked 2026-10-03, prebuilt aa59b2d84, --sample --no-presenter, 1280x800, target/sheetshow6-shots/show1/):
- [FIXED fa94952ef] HOME is 1666 px wide (the Layout gallery alone 863 px: all 7 layouts inline): Font is cut at the window's edge,
  Paragraph / Drawing / Editing are off-screen (02-normal.json group rects). PowerPoint has "Layout" as a dropdown
  in the Slides group.
- [FIXED cf3f7a6fa] VIEW > Window had Flat / Flora / Light / Dark / System buttons (gone in cf3f7a6fa: Options).
- New: the theme previews all look the same (beige ground, tiny title): the accent does not show.
- ENGINE (HEADLESS6): the same stale / doubled paint as in Sheets - bold ghost labels after a theme switch
  (14-flora-light-full), sorter thumbnails painted overlapping while their rects are adjacent 232 px tiles
  (20-view-slidesorter-full.png vs .json), the show's text drawn twice (38-show-full), INSERT's groups garbled after
  the tab switch (03-tab-insert).
- Normal view, rail, task pane, selection handles, notes, status bar, backstage New look right.

## Decisions
- LOOK sessions are scripted (one bounded run_capped call each), not interactive: the machine-wide lock had a queue
  of ~10 runs; an interactive 15-minute hold would starve the other agents.

## Open questions

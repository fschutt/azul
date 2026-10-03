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

## IN PROGRESS
- LOOK at AzShow (show_steps1.py, with a resize-toggle "-full" shot after each, to tell stale paint from bad layout).

## NEXT
- Sheets: compact HOME ribbon so it fits 1280 px (Cells / Editing are off-screen: Sort / Filter / Find unclickable -
  the old E2E fails at "the sort"); text overflow into empty neighbours (CellGrid); GridCell a11y names; zoom slider
  a11y name (StatusBarZoom).
- Then S3 blockers (one shared drive, Show page.next, exports into the tree), appkit, close guard, prefixes, then the
  brief's feature list.

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

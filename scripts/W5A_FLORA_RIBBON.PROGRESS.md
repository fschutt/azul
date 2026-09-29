# W5A_FLORA_RIBBON - progress (branch wt/w5a-flora-ribbon, from 0a326afe5)

Task: flora look + theme option for ribbon, quick_access, statusbar; unpinned
follows the app theme through `theme_blocks::{follow_app_theme, follow_dom}`
(U1 moves T3's helpers there in parallel - the build waits for U1).

## Design (decided)
- Flat look = the established palette parts in the widget files, untouched.
- Flora look = `themes::flora::{ribbon,statusbar,quick_access}_style(style)`:
  fills every part the caller left `None` with the established part's
  GEOMETRY (paint, dark twins and states filtered out) + flora paint.
  Caller's `Some(..)` parts win in both looks.
- Widget: `theme: OptionUiTheme` LAST, `set_theme` / `with_theme`;
  `build_in(theme)` pins every internal Button / Slider and every embedded
  unpinned Combo/Drop/Check to `theme`, root carries the theme marker.
- Pin pass-down done HERE (U1's pass-down would collide with the rewrite).

## DONE
- 380e204f4 plumbing: theme option + pin pass-down + marker, all three;
  style-reading unit tests pinned to Flat
- RED (this commit): widget `flora_tests` modules (ribbon/statusbar/
  quick_access), `flora::{CHROME_METRICS, chrome_metric_findings}` test
  helpers, integration follow tests + flora-look-of-its-own guard (single-
  look guard lost the three), light/dark harness entry

## IN PROGRESS
- flora looks in flora.rs (`// ==== chrome ... ====` section + per widget)

## NEXT
- report scripts/W5A_FLORA_RIBBON_2026_09_29.md

## Open questions
- none

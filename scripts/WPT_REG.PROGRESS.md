# WPT_REG progress (branch wt/wpt-reg, base 05ef3a8f4)

Gate run (built from 889dccf30): 131 pass / 79 fail; 60 expected, 19 regressions, 44 unexpected passes.
Snapshot of the run's results/diffs: /tmp/wpt/gate_snapshot (copied from target/wpt).
Tools: /tmp/wpt/tools (mk.py builds an E2E mount scenario from WPT pages, run_e2e.sh runs it on
AzPaint headless through run_capped, resp.py prints display list / layout tree).

KEY FINDING: tests/wpt/reftest_expectations.txt was SEEDED from a debug-server sweep, never
blessed from this runner; 12 of the 19 "regressions" were removed from it by agents PREDICTING
their fix (8fba0bcdf, 8a8d980ea, 978441b69, 4958a31e7, 03ba7769c) - never verified.

## Root-cause groups (19)
- A abspos static position never computed in block flow: height-table-cell-001, height-width-table-001
- B inside marker rides a nested first line (116feca4d): list-style-position-023
- C marker of an empty item 4px off (own-line vs riding marker): list-style-type-applies-to-009
- D font-size keywords (smaller/larger/x-small..) not parsed: local/ua/small-is-smaller
- E needs `display: inline flow-root list-item` (expectation): inline-block-list
- F needs list-style-type <string> + ::before (expectation): list-style-type-string-001a
- G table cell vertical-align uses item bounds not line-box extent: table-cell-nowrap-with-fixed-width
- H text clipped to a shrink-wrapped box's advance edge (glyph AA spill): anonymous-table-ws-001, table-width-s
- tables (TODO analyse): collapsing-border-model-003/009, border-collapse-offset-002,
  border-collapse-empty-row, calc-percent-plus-0px-auto, col-definite-size-001, th-text-align,
  table-cell-width-s
- gradient seams: background-gradient-subpixel-fills-area

## DONE
- 98187c0ac the 44 unexpected passes leave tests/wpt/reftest_expectations.txt
- 264f41dba RED / 879830f93 GREEN (A) abspos static position (LayoutOutput::static_positions)
- ce4fc965c RED / 640999161 GREEN (B) inside marker: own line host only, in flow otherwise
- 20f62e7d2 RED / f3312a48d GREEN (C) a marker's space is never stripped as trailing white space
- 888b7fac7 RED / c766ce0c0 GREEN (D) font-size keywords (css), one table (ua_css, core xml legacy)
- 320874a95 RED / d1f8fbeee GREEN (G) middle-aligned cell measured by ifc_extent
- a48f2d8d2 RED / 3bd138be7 GREEN :first-child/:last-child parse (table-cell-width-s test side;
  its REF still broken: float clearance across sibling blocks -> expectation)
- a2dbb0014 RED / 680501c46 GREEN compact-cache border width sentinel (1in/em) in collapsed
  tables + painter (collapsing-border-model-003/009)
- 6a7a6a495 RED / f7c45f50a GREEN inline-table paints cells + collapsed borders
  (border-collapse-empty-row; its REF needs :not() -> expectation)
- ebf70029d RED / 14e8e18f1 GREEN calc() cell width with % (calc-percent-plus-0px-auto)

## NEXT
- col-definite-size-001, th-text-align (likely expectation), border-collapse-offset-002 (caption
  inside table box -> expectation), H text clip, gradient seams; expectations for E, F and every
  gap left unfixed (table-cell-width-s, border-collapse-empty-row, ...)

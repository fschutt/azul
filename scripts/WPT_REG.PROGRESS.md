# WPT_REG progress (branch wt/wpt-reg, base 05ef3a8f4) - DONE

Gate run (built from 889dccf30): 131 pass / 79 fail; 60 expected, 19 regressions, 44 unexpected passes.
Report: scripts/WPT_REG_2026_10_06.md (root causes, commits, risks, commands).

KEY FINDING: tests/wpt/reftest_expectations.txt was SEEDED from a debug-server sweep, never
blessed from this runner; most of the 19 "regressions" were removed from it by agents PREDICTING
their fix (8fba0bcdf, 8a8d980ea, 978441b69, 4958a31e7, 03ba7769c), or never passed.

## DONE (RED / GREEN, all unverified - nothing compiled)
- 98187c0ac the 44 unexpected passes leave the expectations
- 264f41dba / 879830f93 (A) abspos static position (LayoutOutput::static_positions)
- ce4fc965c / 640999161 (B) inside marker: own line host only, in flow otherwise
- 20f62e7d2 / f3312a48d (C) a marker's space is never stripped as trailing white space
- 888b7fac7 / c766ce0c0 (D) font-size keywords (css), one table (ua_css, core xml legacy)
- 320874a95 / d1f8fbeee (G) middle-aligned cell measured by ifc_extent
- a48f2d8d2 / 3bd138be7 :first-child / :last-child parse
- a2dbb0014 / 680501c46 (I) compact-cache border width sentinel (1in / em)
- 6a7a6a495 / f7c45f50a (J) inline-table paints cells + collapsed borders
- ebf70029d / 14e8e18f1 (K) calc() cell width with %
- b697b68ef / 1f775ae35 (L) definite <col> columns without cells
- 2df492101 / 3b1f814cd (H) text clip on a visible x axis widened by 1em
- 9e70454bb / 9770ecc33 (N) pixel-snapped gradient fills
- 99050156a / 80de97ac1 (E2) hanging outside markers take no line room
- ba3e7811d six feature gaps back into the expectations
- report commits 1178c5a81 .. (this one)

## NEXT (parent)
- compile, run the commands of the report sec. 5, then the WPT gate; bless what flipped after
  reading the diffs (sec. 2 lists the expected flips).

# ANIMFRAME8 progress (wave-8 follow-up, branch wt/animframe8, base 5745afee6)

Task (PLAN.md "Wave-8 follow-ups" / ANIMFRAME8): an animation frame must cost ~1-2 ms. A transform /
opacity animation: no layout, no display-list rebuild (GPU property channel). A paint-only transition:
patch the display list. A layout-property transition: relayout only the dirty subtree. Answer "do we have
duplicated paths?" with a path map. Then, RED test per item: (1) a tick that changes no DOM node does not
reconcile; (2) style-only changes take the DL patch path; (3) css_transition_tick lightweight; (4)
VirtualView callbacks not re-run on a relayout whose host node is unchanged. Never compile; never touch
page_breaks.rs. Not the a11y code (A11YPATCH8).

## STATUS: IN PROGRESS

## DONE (oldest first)
- (this file)

## IN PROGRESS
- Measuring: baseline from /Users/fschutt/Development/azul-work/lp8/tick3.log (profiled) + tick4.log
  (unprofiled) on the current build; reading the frame paths in layout/src/window.rs + dll event.rs.

## NEXT
- Path map + per-stage costs into the report skeleton scripts/ANIMFRAME8_2026_10_03.md.

## DECISIONS

## OPEN QUESTIONS

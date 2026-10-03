# RULINGS8 progress (wave 8, branch wt/rulings8, base 72d0d6639)

Brief: scripts/waves/wave8/PLAN.md section "RULINGS8".
1. FOCUS: a click on non-focusable content inside a VirtualView focuses the nearest focusable ancestor, across the
   VirtualView boundary into the host DOM.
2. INLINE-BLOCK LINE HEIGHT: a line holding only an inline-block includes the strut (CSS 2.1 s10.8), as Chrome; then
   adjust the widgets' icon CSS so they keep their look.

## DONE
- (none yet)

## IN PROGRESS
- reading the focus path (core/src/events.rs, dll/src/desktop/shell2/common/event.rs, layout/src/e2e/runner.rs)

## NEXT
- RED test for focus across the VirtualView boundary

## Decisions / open questions

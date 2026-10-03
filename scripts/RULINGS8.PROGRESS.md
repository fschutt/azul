# RULINGS8 progress (wave 8, branch wt/rulings8, base 72d0d6639)

Brief: scripts/waves/wave8/PLAN.md section "RULINGS8".
1. FOCUS: a click on non-focusable content inside a VirtualView focuses the nearest focusable ancestor, across the
   VirtualView boundary into the host DOM.
2. INLINE-BLOCK LINE HEIGHT: a line holding only an inline-block includes the strut (CSS 2.1 s10.8), as Chrome; then
   adjust the widgets' icon CSS so they keep their look.

## DONE
- 80a1c3ccf progress file
- cffdaeffa FOCUS RED: layout/src/e2e/focus_across_virtual_view_tests.rs (child module of runner.rs, appended at
  its end; needs run_e2e_test_keeping_runner which is private, so not in layout/tests/)
- 91928b72d FOCUS GREEN: managers::hover::focusable_under_pointer walks core::events::get_event_path (4th closure
  host_of); dll event.rs + runner.rs pass virtual_view_manager.host_of_nested_dom; hover.rs unit tests updated.

## IN PROGRESS
- INLINE-BLOCK LINE HEIGHT: read text3 line-box metrics for atomic inlines, find LAYOUT7's note

## NEXT
- Chrome probe for a 10px inline-block in a 16px / normal parent; RED test layout/tests/<sentence>.rs

## Decisions / open questions
- FOCUS test location: inside the crate (layout/src/e2e/), because the e2e runner's keep-the-runner entry point is
  crate-private; the click-to-focus rule is a pure function also unit-tested in managers/hover.rs.

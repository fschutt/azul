# R3-E2E progress (wave 9 round 3) - branch wt/r3-e2e, base 7393e3582

## DONE
- Bug 1 (Modal button does not rebuild the main window):
  - reproduced: azerp_e2e.py against the prebuilt AzERP fails at step 3 ("does not read checked out").
  - root cause: `PlatformWindow::process_timers_and_threads` (dll/src/desktop/shell2/common/event.rs)
    fanned out only a callback's `Update::RefreshDomAllWindows`, never a change pass's
    `ShouldRegenerateDomAllWindows` (a popup pass upgrades CurrentWindow -> AllWindows; the debug
    click is a timer change). Real desktop clicks fan out through the event arms already.
  - RED bada0e9f9 (dll/src/desktop/shell2/headless/tests/e2e_host.rs
    `a_modal_button_that_changes_app_state_rebuilds_its_parent_window`)
  - GREEN d0b97e739 (event.rs: one fan-out after every answer is in)
- Bug 2 (AzNews 22,598 px layer / AzCode inf layer):
  - reproduced with /tmp probes (get_all_nodes_layout): the whole OfficeShell chain has an infinite
    height in both apps; bisected live with set_node_css_override: the panes' `height: 100%` inside
    the split halves (`display: block`) is the trigger.
  - root cause: solver3/fc.rs `layout_flex_grid` -> `resolve_explicit_dimension_*` resolved a
    percentage against `available_size` = INFINITY (a measurement pass) -> inf known height.
  - RED acb8578c7 (layout/src/solver3/fc.rs window_layout_tests
    `a_percentage_height_measured_against_an_indefinite_height_is_auto`)
  - GREEN 09de0bfc7 (fc.rs `definite_or_auto`)

## IN PROGRESS
- final report scripts/R3_E2E_2026_10_05.md

## NEXT
- (none after the report)

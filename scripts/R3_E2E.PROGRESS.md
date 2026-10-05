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

## IN PROGRESS
- Bug 2 (AzNews 22,598 px layer / AzCode inf layer): reproduce.

## NEXT
- Bug 2: aznews_e2e.py step 3, azcode_e2e.py folder run; find the tall node; app CSS vs engine.
- Final report scripts/R3_E2E_2026_10_05.md.

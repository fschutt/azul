# R3-E2E progress (wave 9 round 3) - branch wt/r3-e2e, base 7393e3582

## DONE
- (none yet)

## IN PROGRESS
- Bug 1 (Modal button does not rebuild the main window): root cause candidate found -
  `PlatformWindow::process_timers_and_threads` (dll/src/desktop/shell2/common/event.rs) only fans out
  `Update::RefreshDomAllWindows` returned by a timer/thread, never a `ShouldRegenerateDomAllWindows`
  CHANGE result (the popup pass upgrades CurrentWindow -> AllWindows; the debug click is a timer change).

## NEXT
- reproduce with azerp_e2e.py steps 3/4/4b; RED test in dll/src/desktop/shell2/headless/tests; fix.
- Bug 2 (AzNews 22,598 px / AzCode inf layer).

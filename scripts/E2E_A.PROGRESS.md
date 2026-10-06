# E2E-A progress (branch wt/e2e-a, base 05ef3a8f4)

Apps: AzCalculator, AzCalendar, AzClock, AzCode, AzContacts, AzDashboard, AzDrive, AzERP, AzKeys, AzMail.
Runs: `/Users/fschutt/Development/azul/scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 600
--log /tmp/e2e-a/<app>.log -- python3 scripts/<app>_e2e.py --bin /Users/fschutt/Development/azul/target/release/<App> --keep`

## AzCalculator
- status: FAIL at "Escape closes the About box first: expected 'closed', last 'open'".
- root cause (ENGINE, dll headless): the About box is a Modal = a `<transient-window>` popup, a
  `Menu`-type window. Headless `is_menu_window` counted it as a window-based menu, so the owner's
  Escape went to `dismiss_menu_windows`, which closed the popup silently (no Dismissed, no
  on_close) and consumed the key. Second half: headless `deliver_forwarded_keys` is a no-op, so a
  key forwarded to a keyboard-owning popup is never replayed.
- commits: RED (tests in dll/src/desktop/shell2/headless/mod.rs child_window_tests).
- NEXT: GREEN for is_menu_window + deliver_forwarded_keys in headless/mod.rs.

## Others
- not started.

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
- commits: c0a7ef37f RED (dll/src/desktop/shell2/headless/mod.rs child_window_tests), 49d34f0b0 GREEN
  (is_menu_window excludes mailboxes; headless deliver_forwarded_keys). Unverified (needs a rebuild).
- proof on today's binary: a probe copy that sends the Escape to the modal's own window
  (`window_id="azul-transient"`, what macOS does - the modal is the key window) closes the box
  (`AZCALCULATOR_ABOUT closed`) and the whole script then PASSES (Ctrl+C copied 42).
- the script itself is unchanged (its expectation is right).
- DONE pending rebuild.

## Others
- not started.

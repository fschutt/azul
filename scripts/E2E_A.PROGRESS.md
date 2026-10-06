# E2E-A progress (branch wt/e2e-a, base 05ef3a8f4)

Apps: AzCalculator, AzCalendar, AzClock, AzCode, AzContacts, AzDashboard, AzDrive, AzERP, AzKeys, AzMail.
Runs: `/Users/fschutt/Development/azul/scripts/waves/tools/run_capped.sh --cap-mb 1500 --seconds 600
--log /tmp/e2e-a/<app>.log -- python3 scripts/<app>_e2e.py --bin /Users/fschutt/Development/azul/target/release/<App> --keep`

## AzCalculator - DONE pending rebuild
- FAIL "Escape closes the About box first: expected 'closed', last 'open'".
- root cause (ENGINE, dll headless): the About box is a Modal = a `<transient-window>` popup, a
  `Menu`-type window. Headless `is_menu_window` counted it as a window-based menu, so the owner's
  Escape went to `dismiss_menu_windows`, which closed the popup silently (no Dismissed, no
  on_close) and consumed the key. Second half: headless `deliver_forwarded_keys` was a no-op, so a
  key forwarded to a keyboard-owning popup was never replayed.
- commits: c0a7ef37f RED, 49d34f0b0 GREEN (dll/src/desktop/shell2/headless/mod.rs).
- proof on today's binary: a probe sending the Escape to the modal's own window (`azul-transient`)
  closes the box and the whole script then PASSES. Script unchanged.

## AzCalendar - DONE pending rebuild
- editor: "timed out waiting for AZCAL_EDITOR closed". ENGINE (layout hover.rs): Save & Close is
  half under the fold; the press on its label focuses the button, the focus scrolls it 12 px into
  view, the release lands on the button's padding -> the engine clicked only press==release node.
  Fix: W3C nearest-common-ancestor click in `HoverManager::apply_press_target_capture`.
  commits 2c5292337 RED, e42a91049 GREEN.
- close: SCRIPT (clicked "Don't Save" in the editor window; the question is a Modal window of its
  own -> `reach_modal`, 7a26433ee) + ENGINE (CloseGuard's Discard `close_window()` ran in the
  modal's popup and closed the popup, not the editor): `close_owner` mailbox protocol,
  commits 410c86966 RED, 94dc5c544 GREEN.
- contrast: 9 false findings in flora light (text on gradients). ENGINE (debug server listed
  gradients as `unknown` without bounds) 0ac74f7ca RED, 2aa707695 GREEN; script reads gradients as
  fills 0fc182265.
- proof on today's binary: with a scroll_into_view before Save, import/editor/repeat/occurrence PASS.

## AzClock - DONE pending rebuild
- "the two alarms that are on schedule at least two notifications, got 0" (AZCLOCK_SCHEDULED 0 26):
  ENGINE - AzClock probes PlatformCapability::notifications() before App::run; the headless switch
  is set by the run loop -> macOS probe, unavailable. Fix: AzBackend::headless_selected() in
  notifications::probe. ce6845526 RED, 2daacb34b GREEN.
- editor / city search / ringing overlay are Modals (own windows): SCRIPT -> shared
  e2e.InWindow / modal_window / laid_out in azlin_e2e.py (moved from azerp_e2e.py) 1e439d6b5;
  clock script da968559e. Also click-by-text is "contains": "1 min" hit "+1 min" -> click_exact.
- first click after a closed modal eaten: ENGINE, the modal window lingers in today's libazul
  (built 03:14, before 30fb9f06b) and headless dismiss_menu_windows ate the press (49d34f0b0).
- proof: with the schedule check relaxed and a second click after each modal, the script PASSES.

## Others
- NEXT: AzCode.

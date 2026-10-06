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

## Others
- NEXT: AzClock.

# FIX9-INPUT progress (PKG 3 ENGINE-INPUT-IO + DLL + TOOLING, wave 9, 2026-10-05)

Branch wt/fix9-input, base b454da215. Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 3" + the selection.rs suite failure.
Commit messages via /tmp/fix9i_msg (Write tool), git add explicit paths.

## DONE
- S.1 selection.rs suite failure: TEST was wrong (code escapes = safe). ed0cf0f09
- G.1 (coordinator add-on) gene2e: dump_profile unclassified + assert_no_unmocked_requests false alarm.
  RED 1460f3df6, GREEN 25a0f001b
- 3.1 macOS Cmd+letter key-up: RED c5bbd8b16, GREEN 4185ed217 (macos/events.rs flags_changed_keyboard_state)
- 3.2 focus by id delegates: GREEN 41ab21644 (RED pre-existing 2d0afec0f)

## IN PROGRESS
- 3.3 Shift+Insert / Ctrl+Insert (core/src/events.rs KeyboardShortcut::from_key)

## NEXT
- 3.4 .. 3.16 in order

## Open questions
(none)

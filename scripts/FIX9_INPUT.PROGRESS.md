# FIX9-INPUT progress (PKG 3 ENGINE-INPUT-IO + DLL + TOOLING, wave 9, 2026-10-05)

Branch wt/fix9-input, base b454da215. Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 3" + the selection.rs suite failure.
Commit messages via /tmp/fix9i_msg.

## DONE
- S.1 selection.rs suite failure: TEST was wrong (code escapes = safe). ed0cf0f09
- G.1 (coordinator add-on) gene2e: dump_profile unclassified + assert_no_unmocked_requests false alarm.
  RED 1460f3df6, GREEN 25a0f001b

## IN PROGRESS
- 3.1 macOS Cmd+letter key-up (dll/src/desktop/shell2/macos/events.rs handle_flags_changed)

## NEXT
- 3.2 .. 3.16 in order

## Open questions
(none)

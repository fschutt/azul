# FIX9-INPUT progress (PKG 3 ENGINE-INPUT-IO + DLL + TOOLING, wave 9, 2026-10-05)

Branch wt/fix9-input, base b454da215. Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 3" + the selection.rs suite failure.
Commit messages via /tmp/fix9i_msg (Write tool), git add explicit paths.

## DONE
- S.1 selection.rs suite failure: TEST was wrong (code escapes = safe). ed0cf0f09
- G.1 (coordinator add-on) gene2e: dump_profile unclassified + assert_no_unmocked_requests false alarm.
  RED 1460f3df6, GREEN 25a0f001b
- 3.1 macOS Cmd+letter key-up: RED c5bbd8b16, GREEN 4185ed217 (macos/events.rs flags_changed_keyboard_state)
- 3.2 focus by id delegates: GREEN 41ab21644 (RED pre-existing 2d0afec0f)
- 3.3 Shift/Ctrl+Insert: RED 7e655e985, GREEN e2c1d1667 (KeyboardShortcut::from_key_event)
- 3.4 http off-thread: RED 156ae0192, GREEN 4fd4c5af6 (http.rs resume_without_blocking)
  report note: the request queue is process-wide; tests draining it (dialogs.rs, http.rs) can steal entries.

- 3.5 Url::open: RED dec04a08a, GREEN 04121d6b6 (opener_command; Url::open_path new -> api.json)
- 3.6 final_url: RED f66ad9d5a, GREEN 3f34d7033 (HttpResponse.final_url last field -> api.json)

- 3.7 runner outside press: RED e1264e068, GREEN 1c1e16713

- 3.8 paste op: NOT doable in PKG 3 files (needs CallbackChange in callbacks.rs + dll event.rs arm).
  RED committed #[ignore] d501b146b; design -> report round-2 note.

- 3.9 scheduled_notifications: RED 6beeb4898, GREEN a62d6096d (-> api.json)
- 3.10 wasm AudioSink stub: c55d35d37 (no RED, cfg wasm32)

## IN PROGRESS
- 3.11 type index: desktop::extra::<m> types re-exported by unified/<m>.rs take the facade path
  (doc/src/autofix/type_index.rs)

## NEXT
- 3.12 .. 3.16 in order

## Open questions
(none)

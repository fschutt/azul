# INFRA6 progress (wave 6, 2026-10-03)

Branch `wt/infra6` from 25d78e309. Brief: scripts/waves/wave6/INFRA6.md. Report: scripts/INFRA6_2026_10_03.md.
Commit messages go through `<scratchpad>/infra6/msg.txt` (the scratchpad root is shared with other agents).

## DONE (commits)
- b4e8352ec progress file
- 4e79da6bc ids RED, e3ee94a56 ids GREEN (azul_storage::ids::new_uuid / uuid_from_words / is_uuid; appkit re-exports)
- 433a82a16 manifest RED (src/tests/manifest.rs), fb7253e27 manifest GREEN (src/manifest.rs, LocalDrive hooks,
  without_manifest, config DriveEntry::open, sigv4 uri_decode + sha256_hex_of)
- c9b579c04 sigv4 helper tests (+ progress d301b6153)
- 9c5e32262 migration RED (appkit migrate.rs, stub + 8 tests), a639fc30a migration GREEN + hook in ui::create_kit
- b39b4d731 CLOSE RED (dll/tests/close_requested_headless.rs, 7 tests)
- 76395525c A (common/event.rs), 7d20fc589 B (headless), 0b084638e C (linux), 04bf7ba63 D (macos),
  1fa94d591 E (windows)
- LAST COMMIT: see `git log -1`; next step = CLOSE step F (remove close_callback), then the report.

## CLOSE GREEN plan (exact)
A. common/event.rs: CommonWindowState gets `close_unconfirmed: bool` (init false in `new`) + `pub fn
   close_unconfirmed(&self)`, `pub fn take_close_unconfirmed(&mut self)`, `pub fn rebuild_owed(&self)`.
   apply_user_change: CloseWindow arm raises the flag AND sets the marker only on false->true;
   ModifyWindowState arm: old false -> new true sets the marker.
   `request_window_close`: clears the marker; a flag already up is lowered + `discard_input_delta` (instead
   of the snapshot) so the pass sees false->true. New trait methods: `run_close_protocol(site)` = build an
   owed DOM (rebuild_owed -> regen_epoch / regenerate_layout / clear_regeneration_unless_reraised) then
   request_window_close; `confirm_app_close(site) -> Option<WindowCloseOutcome>` = if take marker ->
   Some(run_close_protocol), and raise a regeneration when the pass asked for one.
B. headless: HeadlessEvent::Close -> run_close_protocol (close if confirmed, else route result);
   phase 2b: confirm_app_close first, then the old `if flag -> close`.
C. Linux run.rs (~2502): `window.confirm_app_close();` before the `close_requested()` check; LinuxWindow
   wrapper method in linux/mod.rs.
D. macOS: process_close_event -> run_close_protocol; sync_window_state close branch skips when
   `close_unconfirmed()` (drain_loop_work runs the protocol).
E. Windows: route_main_window_result tail + Win32 sync_window_state start + poll_event_internal: if
   take_close_unconfirmed -> self.close() (posts WM_CLOSE); WM_CLOSE -> run_close_protocol.
F. remove FullWindowState::close_callback (window_state.rs 125/384/570/652, macos 5901, wayland 2135/9510,
   x11 4243, windows 633, event.rs 1596), core/src/window.rs:1592 comment, doc/guide windowing.md.

## IN PROGRESS
- 4. CLOSE

## NEXT
2. CLOSE: RED headless tests (`dll/tests/close_requested_headless.rs`); fix = `request_window_close` works for an
   app-raised flag (lower + rebaseline, then raise + pass), backends run the protocol for an app-raised flag
   instead of closing directly (headless phase 2b + HeadlessEvent::Close, Linux run.rs, macOS sync_window_state,
   Windows poll_event); remove `FullWindowState::close_callback` (+ backends' copies, test, docs).
3. Report.

## Decisions
- Manifest: `LocalDrive::new` keeps it (the data tree; apps call nothing new). A plain folder that is
  NOT the data tree (AzDrive's Home / Downloads / dropped files, the user's own local drives in
  `drives.json`) uses `LocalDrive::without_manifest` - otherwise every browsed folder grows a `.azlin/`.
  `config::DriveEntry::open` (storage crate) switched; AzDrive's two app call sites (actions.rs 1482, 1544)
  are its owner's (MEETDRIVE6) - in the report.
- Manifest errors never fail the drive call (the data write succeeded; the manifest is a cache the
  diff repairs).
- Hash = lowercase hex SHA-256 (`sigv4::sha256_hex`): the same hash SigV4 already sends as
  `x-amz-content-sha256` for a PUT, so the sync can compare without reading files.
- Migration only when the root is the DEFAULT one (`<OS data dir>/Azlin`), only the starting app's folder
  (an app not yet on appkit keeps its data where it reads it); `azul/` also holds azul's own config and
  styles - never touched.
- CLOSE: deferred, not nested - an app-raised close (close_window, the titlebar's button) is run through
  the protocol by the backend loop AFTER the frame, so CloseRequested reaches the DOM the app's last
  callback asked for (CloseGuard's Save-on-a-thread flow: dirty=false + close_window in one writeback).

## Open questions
- (none)

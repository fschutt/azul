# INFRA6 progress (wave 6, 2026-10-03)

Branch `wt/infra6` from 25d78e309. Brief: scripts/waves/wave6/INFRA6.md. Report: scripts/INFRA6_2026_10_03.md.
Commit messages go through `<scratchpad>/infra6/msg.txt` (the scratchpad root is shared with other agents).

## DONE (commits)
- b4e8352ec progress file
- 4e79da6bc ids RED, e3ee94a56 ids GREEN (azul_storage::ids::new_uuid / uuid_from_words / is_uuid; appkit re-exports)
- 433a82a16 manifest RED (src/tests/manifest.rs), fb7253e27 manifest GREEN (src/manifest.rs, LocalDrive hooks,
  without_manifest, config DriveEntry::open, sigv4 uri_decode + sha256_hex_of)
- (this commit) sigv4 helper tests

## IN PROGRESS
- 2. migration (appkit)

## NEXT
1. migration: `examples/azul-appkit/src/migrate.rs` (RED tests on a temp folder), GREEN, hook in `ui::create_kit`.
   Design: per APP (only the starting app's own folder `<os data>/<legacy>/<app_folder>/` moves, legacy =
   `azul`, `Azul`, `AzNotes`), only when the root is the default `<os data>/Azlin`; never overwrite;
   note file `MOVED-TO-AZLIN.txt` appended in the legacy folder when something moved.
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

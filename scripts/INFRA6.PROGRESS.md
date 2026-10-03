# INFRA6 progress (wave 6, 2026-10-03)

Branch `wt/infra6` from 25d78e309. Brief: scripts/waves/wave6/INFRA6.md. Report: scripts/INFRA6_2026_10_03.md.

## DONE (commits)
- (none yet)

## IN PROGRESS
- 3. ONE ID MINT (azul_storage::ids::new_uuid / uuid_from_words / is_uuid; appkit delegates)

## NEXT
1. ids: RED test in azul-storage ids.rs, GREEN, appkit data.rs delegates (keep its pub names as re-exports).
2. manifest: new `examples/azul-storage/src/manifest.rs` + `src/tests/manifest.rs` (RED), then LocalDrive hooks (GREEN).
3. migration: `examples/azul-appkit/src/migrate.rs` (RED tests on a temp folder), GREEN, hook in `ui::create_kit`.
4. CLOSE: RED headless tests (`dll/tests/close_requested_headless.rs`), fix in `common/event.rs`
   `apply_user_change` (app-requested close runs the protocol pass), headless `HeadlessEvent::Close`
   through `request_window_close`; remove `FullWindowState::close_callback` (+ backends' copies, test, docs).
5. Report.

## Decisions
- Manifest: `LocalDrive::new` keeps it (the data tree; apps call nothing new). A plain folder that is
  NOT the data tree (AzDrive's Home / Downloads / dropped files, the user's own local drives in
  `drives.json`) uses `LocalDrive::without_manifest` - otherwise every browsed folder grows a `.azlin/`.
  `config::DriveEntry::open` (storage crate) switches; AzDrive's two app call sites are its owner's.
- Manifest errors never fail the drive call (the data write succeeded; the manifest is a cache the
  diff repairs).
- Hash = lowercase hex SHA-256 (`sigv4::sha256_hex`): the same hash SigV4 already sends as
  `x-amz-content-sha256` for a PUT, so the sync can compare without reading files.
- Migration only when the root is the DEFAULT one (`<OS data dir>/Azlin`), only the known app
  folders of the legacy roots (`azul/` holds azul's own config and styles too - never touched).

## Open questions
- (none)

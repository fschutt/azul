# INFRA6 progress (wave 6, 2026-10-03)

Branch `wt/infra6` from 25d78e309. Brief: scripts/waves/wave6/INFRA6.md. Report: scripts/INFRA6_2026_10_03.md.
Commit messages go through `<scratchpad>/infra6/msg.txt` (the scratchpad root is shared with other agents).

## DONE (commits) - ALL FOUR PARTS + REPORT
- ids: 4e79da6bc RED, e3ee94a56 GREEN
- manifest: 433a82a16 RED, fb7253e27 GREEN, c9b579c04 sigv4 tests, 6a70374f2 sha256_hex_of generic
- migration: 9c5e32262 RED, a639fc30a GREEN + hook in ui::create_kit
- CLOSE: b39b4d731 RED; 76395525c common, 7d20fc589 headless, 0b084638e linux, 04bf7ba63 macos,
  1fa94d591 windows, 9fbb69005 x11 WM_DELETE_WINDOW; a669d42eb close_callback removed; 33d685fb7 CloseGuard doc
- report scripts/INFRA6_2026_10_03.md (committed with this file)

## IN PROGRESS
- (none)

## NEXT
- Nothing left for INFRA6. If resumed: re-read the report, answer the parent's compile errors.

## Decisions
- Manifest: `LocalDrive::new` keeps it (the data tree; apps call nothing new). A plain folder that is
  NOT the data tree uses `LocalDrive::without_manifest`; `config::DriveEntry::open` switched; AzDrive's two
  app call sites (actions.rs 1482, 1544) are MEETDRIVE6's - in the report.
- Manifest errors never fail the drive call (the manifest is a cache the diff repairs).
- Hash = lowercase hex SHA-256 (`sigv4::sha256_hex`), the SigV4 payload hash.
- Migration only for the DEFAULT root, only the starting app's own folder, legacy dirs azul / Azul / AzNotes.
- CLOSE: deferred to the backend loop (confirm_app_close), not nested in the callback pass; the DOM a
  pending RefreshDom asked for is built before CloseRequested (run_close_protocol).

## Open questions
- (none)

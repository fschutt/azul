# DRIVE2 progress

Branch `wt/drive2` from `39092feee`. Report: `scripts/DRIVE2_2026_10_01.md`.
Task: AzDrive that works like Windows 10 File Explorer with the Ribbon (File / Home / Share / View),
the four+ layouts, grouping, the three panes, Explorer's keyboard, file operations on every drive
through the `Drive` trait on azul Threads (transfer queue, conflicts, InfoBar errors), preview and
details panes, a Properties dialog, Settings on ShellSettingsLayout, and a headless E2E.

## DONE
- `ae0d6783c` RED / `a7d539da6` GREEN: azul-storage folder operations (`ops`: list_all, exists,
  folder_exists; `Drive::{create_folder, rename, delete_folder, local_path, metadata}` with defaults,
  LocalDrive native, S3 metadata, ScopedDrive forwards), `transfer::copy_object` with progress.
- `ab49b5e29` RED / `3fee6fab7` GREEN: RibbonButton disabled + reason (tooltip, Unavailable),
  AddressBar recent chevron + dimmed arrows, BrowserShell `tree_visible`.

## IN PROGRESS
- `908e4b0f5` RED / `ddb4f6312` GREEN: the AzDrive model (browse, model, fileops, preview, keys).
- `d2a0b20f6` WIP: new lib.rs (state, navigation, keyring, on_job_done, layout, start) + jobs.rs.
  Still to write (lib.rs already declares them): `args.rs` (Args::parse, Screen, write_sample),
  `actions.rs` (on_key_down, on_dropped_file, on_resized, run_command, open_drive_form,
  request_preview, transfer_planned, transfer_ran, enqueue/pump of the queue, every ribbon
  command), `ui_ribbon.rs`, `ui_view.rs` (content, on_column_drag_move/end), `ui_panes.rs`
  (address_bar, navigation_pane, status_bar, preview_pane, details_pane), `ui_dialogs.rs`
  (popup_parts, inline_sheet, on_dialog_closed, backstage).

## NEXT
- the UI modules above; parse-check; commit "feat(azul-drive): ..." (GREEN of the app).
- scripts/azdrive_e2e.py (headless), update examples/azul-drive/scripts/browse.py.
- report scripts/DRIVE2_2026_10_01.md (api.json list, least-sure spots, test commands).

## Decisions (unattended run)
- Details view: ListView is single-select with fixed column widths and the table code belongs to
  another agent tonight, so the multi-select, resizable-column details view is composed in the app
  (examples/azul-drive/src/view.rs) from plain nodes; reported as a widget candidate.
- Navigation pane: ShellNavigationPane groups "Quick access" and "This PC" (each a TreeView whose root
  is the place itself, as AzShells does with "Favorites").

## Open questions
- (none yet)

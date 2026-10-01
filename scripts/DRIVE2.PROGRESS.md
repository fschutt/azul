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
- Last commit: `fe60cb477` (actions.rs parts 1-3 done: Action table, keyboard, selection, clipboard,
  transfer queue, conflicts, upload/download/drop, delete/rename/new/undo, properties, previews,
  share, pins, view settings, context menu, drives). Earlier: `ae7f45a45` args.rs GREEN,
  `d2a0b20f6` lib.rs core + jobs.rs.
- NEXT STEP: write `ui_panes.rs` (address_bar, navigation_pane [ShellNavigationPane: groups Quick
  access / This PC], status_bar, preview_pane, details_pane), then `ui_view.rs` (content: This PC,
  Quick access, folder layouts, Details header with resizable columns, groups, items with click /
  double-click / right-click / drag / drop / rename field, on_column_drag_move/end), then
  `ui_ribbon.rs` (ribbon(s, app)), then `ui_dialogs.rs` (popup_parts, inline_sheet, on_dialog_closed,
  backstage with ShellSettingsLayout). Each file in pieces, committed.

## NEXT
- review pass (unused imports, names against api_new.json), rustfmt parse check of the crate.
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

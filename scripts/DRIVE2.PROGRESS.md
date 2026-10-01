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
- AzDrive model (pure modules) - RED tests.

## NEXT
- AzDrive model (browse.rs and new pure modules: selection, type-ahead, layouts, grouping, columns,
  conflicts, trash, transfer queue, preview kinds) - RED unit tests, then GREEN.
- AzDrive UI rebuilt on BrowserShell + ShellNavigationPane + Ribbon + StatusBar + InfoBar.
- scripts/azdrive_e2e.py; report.

## Decisions (unattended run)
- Details view: ListView is single-select with fixed column widths and the table code belongs to
  another agent tonight, so the multi-select, resizable-column details view is composed in the app
  (examples/azul-drive/src/view.rs) from plain nodes; reported as a widget candidate.
- Navigation pane: ShellNavigationPane groups "Quick access" and "This PC" (each a TreeView whose root
  is the place itself, as AzShells does with "Favorites").

## Open questions
- (none yet)

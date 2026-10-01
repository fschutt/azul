# DRIVE2 progress

Branch `wt/drive2` from `39092feee`. Report: `scripts/DRIVE2_2026_10_01.md`.
Task: AzDrive that works like Windows 10 File Explorer with the Ribbon (File / Home / Share / View),
the four+ layouts, grouping, the three panes, Explorer's keyboard, file operations on every drive
through the `Drive` trait on azul Threads (transfer queue, conflicts, InfoBar errors), preview and
details panes, a Properties dialog, Settings on ShellSettingsLayout, and a headless E2E.

## DONE
- (none yet)

## IN PROGRESS
- azul-storage: folder operations (create / rename / delete a folder, list_all, exists), drive-to-drive
  `transfer::copy_object` with progress, `Drive::local_path`, `Drive::metadata` (S3 headers) - RED.

## NEXT
- GREEN of the storage operations.
- Widgets in azul: RibbonButton disabled + reason; BrowserShell navigation pane toggle; AddressBar
  recent-locations chevron (RED, then GREEN).
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

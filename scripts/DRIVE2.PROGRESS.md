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
- Last commit: `1d6992947`. Done since: `23362a4d5`/`5af413502` Drive::copy (storage),
  `211f455dd`/`2cc494582` AzDrive same-drive copies; report updated.
- NOW: a third read-only review agent checks the delta `a25c10912..HEAD` (thumbnails, PDF, WAV,
  copies, tree drop, image_scale) for compile errors.
- NEXT STEP: apply its findings; final report touch (commit list), final progress.

## NEXT
- done after that.

## Decisions (unattended run)
- Details view: ListView is single-select with fixed column widths and the table code belongs to
  another agent tonight, so the multi-select, resizable-column details view is composed in the app
  (examples/azul-drive/src/view.rs) from plain nodes; reported as a widget candidate.
- Navigation pane: ShellNavigationPane groups "Quick access" and "This PC" (each a TreeView whose root
  is the place itself, as AzShells does with "Favorites").

## Open questions
- (none yet)

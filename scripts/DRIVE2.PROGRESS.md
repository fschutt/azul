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
- Last commit: `2ff3dae8b` (`afd1bafca` RED / `2ff3dae8b` PDF first-page preview; api: Pdf::to_svg_pages).
- Before: `958da8cd9`. Since the last checkpoint: `92492e8a9` RED / `11df978f5` GREEN
  image_scale::fit_within + thumbnail (api: RawImage::thumbnail -> OptionRawImage);
  `ea18d5e7a` AzDrive thumbnails in Medium/Large/XL icons; `958da8cd9` E2E thumbnail step.
- WAITING: two read-only review agents (core and UI).
- NEXT STEP: apply their findings ("fix(azul-drive): ..."), then finish the report sections 5 and 7
  (api.json adds: TreeView.on_node_drop + set/with_on_node_drop; ShellNavigationPaneEventKind::
  NodeDropped; RawImage::thumbnail; Pdf::to_svg_pages) and commit.

## NEXT
- report; final progress update.

## Decisions (unattended run)
- Details view: ListView is single-select with fixed column widths and the table code belongs to
  another agent tonight, so the multi-select, resizable-column details view is composed in the app
  (examples/azul-drive/src/view.rs) from plain nodes; reported as a widget candidate.
- Navigation pane: ShellNavigationPane groups "Quick access" and "This PC" (each a TreeView whose root
  is the place itself, as AzShells does with "Favorites").

## Open questions
- (none yet)

# FB2_AZDRIVE progress

Branch `wt/fb2-azdrive-explorer` from `d1dd0b783`. Report: `scripts/FB2_AZDRIVE_2026_09_30.md`.
Task: AzDrive as a Windows-Explorer-like app on the Ribbon; reusable widgets; engine bugs.

## DONE
- (nothing yet)

## IN PROGRESS
- disk space API (`azul_layout::file::disk_space`, `FilePath::get_disk_space`)

## NEXT
1. accordion `Groups` variant + section count (Explorer group headers)
2. tree_view: node icons, lazy children, chevron click / double-click toggle
3. list_view: column widths, dark header separators
4. new widgets: tile (drive / file tile with capacity bar), address_bar, details_pane
5. AzDrive rebuilt on them (ribbon, address bar + history, nav tree, This PC groups, tiles,
   details list, details pane, search)
6. browse.py e2e: tree walk, Home drive, Back / Forward
7. engine bugs found on the way: RED layout tests + fixes
8. report

## Open questions
- none yet

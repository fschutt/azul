# FB2_AZDRIVE progress

Branch `wt/fb2-azdrive-explorer` from `d1dd0b783`. Report: `scripts/FB2_AZDRIVE_2026_09_30.md`.
Task: AzDrive as a Windows-Explorer-like app on the Ribbon; reusable widgets; engine bugs.

## DONE
- `1c44ac86e` RED / `75135142c` GREEN: `file::disk_space` + `FilePath::disk_space` (statfs /
  statvfs / GetDiskFreeSpaceExW); the dll sqlite `free_bytes_at` twin now calls it.
- `21ec8e748` RED: layout/tests/flex_items_keep_the_size_their_container_gave_them.rs (engine
  bugs A, B, C found by mounting the Explorer layout into the released engine).
- `2b044c710` fix A: a taffy measure leaves the node as its last final layout left it.
- `5ac7bea4c` fix B: a definite-width box lays its content out at that width in every query.
- `6be87f2d6` fix C: a layout-cache hit on a flex/grid container positions its items.

## IN PROGRESS
- widgets

## NEXT
1. accordion `Groups` variant + section count (Explorer group headers)
2. tree_view: node icons, lazy children, chevron click / double-click toggle
3. list_view: column widths, dark header separators
4. new widgets: tile (drive / file tile with capacity bar), address_bar, details_pane
5. AzDrive rebuilt on them
6. browse.py e2e: tree walk, Home drive, Back / Forward
7. report

## Probe tools (scratchpad, not committed)
- `scratchpad/fb2/rects.py <bin> <port> <mount.json> ENV=.. -- '#sel' ..`: mounts HTML+CSS into a
  headless app over the debug server and prints node rects (AZ_TAFFY_DEBUG=1 / AZ_PROFILE=cpu).

## Open questions
- Measure-time clobbering of a measured block's DESCENDANTS (their used sizes / IFC layouts
  written by the measure's BFC run) is not fixed; see report.

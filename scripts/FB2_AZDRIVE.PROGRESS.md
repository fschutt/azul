# FB2_AZDRIVE progress

Branch `wt/fb2-azdrive-explorer` from `d1dd0b783`. Report: `scripts/FB2_AZDRIVE_2026_09_30.md`.
Task: AzDrive as a Windows-Explorer-like app on the Ribbon; reusable widgets; engine bugs.
Resumed 2026-09-30 21:15 after the kernel panic (scratchpad wiped; helper scripts recreated).

## DONE
- `1c44ac86e` RED / `75135142c` GREEN: `file::disk_space` + `FilePath::disk_space` (statfs /
  statvfs / GetDiskFreeSpaceExW); the dll sqlite `free_bytes_at` twin now calls it.
- `21ec8e748` RED: layout/tests/flex_items_keep_the_size_their_container_gave_them.rs (engine
  bugs A, B, C found by mounting the Explorer layout into the released engine).
- `2b044c710` fix A: a taffy measure leaves the node as its last final layout left it.
- `5ac7bea4c` fix B: a definite-width box lays its content out at that width in every query.
- `6be87f2d6` fix C: a layout-cache hit on a flex/grid container positions its items.
- `7c4c7e647` RED / `83ecdde79` GREEN: accordion `Groups` variant + `AccordionSection::count`.
- `a27724b78` RED / `977de65ae` GREEN: tree_view lazy children, node icon, arrow click target,
  double-click toggles; `roving::test_support::fire`.
- `782d0f155` RED / `f487ccab0` GREEN: tile, address_bar, details_pane widgets (flat + flora),
  breadcrumb `on_segment_menu`.
- `afa332733` RED / (next) GREEN: AzDrive's places / history / search / typed path (browse.rs),
  lib.rs rebuilt on the widgets, browse.py extended (tree, Back / Forward / Up).

## IN PROGRESS
- the report

## NEXT
1. report `scripts/FB2_AZDRIVE_2026_09_30.md` (api.json list, least-sure spots, test commands)

## Probe tools (scratchpad, not committed; recreated after the panic)
- `scratchpad/fb2/rects.py <bin> <port> <script.json> ENV=.. -- '#sel' ..`: runs ONE app through
  `run_capped.sh` (1.5 GB cap), mounts HTML+CSS over the debug server, prints node rects, kills it.
- `scratchpad/fb2/pc.sh <files>`: rustfmt parse check.

## Open questions
- Measure-time clobbering of a measured block's DESCENDANTS (their used sizes / IFC layouts
  written by the measure's BFC run) is not fixed; see report.
- AzDrive's `browse::format_size` and the tile widget's `format_bytes` are twins in concept (the
  app's has no azul dependency so its tests run without libazul); reported.
- The AzDrive rebuild needs the new widget API in api.json (autofix) before it compiles.

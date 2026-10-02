# PHOTO progress (branch `wt/photo`, base `16d19442c`)

Task: the initial AzPhoto (examples/azul-photo, package AzPhoto) on CanvasShell, with its own
tiled raster core behind a `RasterEngine` trait; the engine piece it needs (partial image
updates by dirty rect); tests; `scripts/azphoto_e2e.py`; report `scripts/PHOTO_2026_10_01.md`.

## DONE
- 829b9a1a9 test(image): partial image uploads (RED): core union/clipped_to, overlay dirty,
  layout/tests/a_partial_image_change_leaves_its_rect_for_the_renderer.rs, dll planner tests.
- 8a344ccfd GREEN 1: core `ImageDirtyRect` methods, overlay `image_dirty` arm,
  `apply_image_change(.., dirty_rect)` in window.rs.
- 279bae778 GREEN 2: `CallbackChange::ChangeNodeImage { dirty_rect }`, `change_node_image_rect`.
- GREEN 3: wr_translate2 planner item (slot, image, pending), `OverlayImageUpload.dirty`,
  `translate_dirty_rect` (shared with translate_update_image), clear after upload.

## IN PROGRESS
- the app crate (see NEXT)

## NEXT
- DONE (app): view 63795c3db, storage 84d7673b7, args 1be2d10a3, state 051a51d95+69ca8c842,
  codec 45e77d5b3, lib+jobs d1c39ff03, canvas 1e24ff93f, commands 97f537f08,
  ui e75b54765..15ec67f64 (menus, options, tools, panels, rulers, status, start, sheets, layout).
- IN PROGRESS: scratch type check of the whole app against the REAL generated bindings
  (scratchpad/photo/tc/azul_real.rs includes target/codegen/{dll_api_external,reexports}.rs;
  build_azul_meta.sh under run_capped) - next: add `extern crate alloc;`, then a
  check_app.sh that type-checks src/ with azul_storage + serde stubbed; fix what it finds.
- then: registration (root Cargo.toml member, workspace_test_members.txt, rust.yml step),
  scripts/azphoto_e2e.py, report scripts/PHOTO_2026_10_01.md.

## Decisions (made unattended, noted here)
- The canvas is ONE image node: a `RenderImageCallback` renders the VIEWPORT (the visible part
  of the document at the current zoom, checkerboard, marching ants) at its physical size; a
  view change (zoom, pan, resize) re-renders it whole; a stroke updates only the dirty view
  rect through the new `change_node_image_rect` (the renderer re-uploads that rect only).
- The data root (the local stand-in for the per-user S3 bucket): `AZPHOTO_DATA`, else
  `<user data dir>/Azul`; documents under `photo/<uuid>/` (doc.json + layers/<layer>/<tx>_<ty>.png).
- Dab profile: generated with azul's own `RawImage::paint_dot` (one stamp per radius), so the
  brush falloff stays single-sourced in azul (`brush_dab_coverage`) - no twin in the app.
- Text tool: azul has no text-to-pixels API; see the report (engine gap).

## Open questions
- (none yet)

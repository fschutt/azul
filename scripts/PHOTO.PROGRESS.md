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
- DONE: engine piece (829b9a1a9 RED; 8a344ccfd, 279bae778, 383672264 GREEN); skeleton
  af9a50285; raster RED 58e0bffeb..772ee1b48; raster GREEN 33943cbe5..38a2028d8 (geom, tile,
  blend, adjust, layer, selection, filter, transform, brush, history, document, engine);
  b21469443 test slips. The raster core type-checks and its 53 tests PASS in the scratchpad
  (scratchpad/photo/tc/run_raster_tests.sh: rustc against a stub azul, no cargo).
- next: src/view.rs (viewport: zoom/pan mapping, checkerboard, nearest/box sampling of the
  composite, marching ants, render a view rect into a BGRA buffer) + tests -> commit;
  src/storage.rs (doc.json model with serde, tile keys photo/<uuid>/layers/<id>/<tx>_<ty>.png,
  save/load through azul_storage::Drive, encode via azul RawImage) + tests -> commit;
  src/args.rs -> commit; then lib.rs UI in pieces (state, layout, canvas callbacks, tools,
  panels, menus, threads) committing each.

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

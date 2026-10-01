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
- DONE so far: engine piece (829b9a1a9 RED; 8a344ccfd, 279bae778, 383672264 GREEN);
  skeleton af9a50285; raster RED tests 58e0bffeb, f3f41aeee, 0cee88238, 772ee1b48
  (examples/azul-photo/src/raster/tests.rs - the API it fixes is in raster/mod.rs re-exports).
- next: raster GREEN, one module per commit, in this order:
  geom.rs (IRect) -> tile.rs (Tile, TileGrid) -> blend.rs -> adjust.rs (Adjustment) ->
  layer.rs (Layer, LayerContent, tree helpers find/find_mut/remove/insert/flatten) ->
  selection.rs (Mask, Shape, SelectMode, magic_wand, feather) -> brush.rs (stamps via
  azul RawImage::paint_dot, Stroke) -> filter.rs -> transform.rs -> history.rs ->
  document.rs (Document, Composite w/ worker threads) -> engine.rs (Op, RasterEngine,
  TileEngine; history labels "Open", "New Layer", "Brush", "Opacity" coalesced).
- then: the app (view.rs canvas viewport, ui, tools, files/storage, --sample),
  registration, scripts/azphoto_e2e.py, report scripts/PHOTO_2026_10_01.md.

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

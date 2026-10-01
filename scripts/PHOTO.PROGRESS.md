# PHOTO progress (branch `wt/photo`, base `16d19442c`)

Task: the initial AzPhoto (examples/azul-photo, package AzPhoto) on CanvasShell, with its own
tiled raster core behind a `RasterEngine` trait; the engine piece it needs (partial image
updates by dirty rect); tests; `scripts/azphoto_e2e.py`; report `scripts/PHOTO_2026_10_01.md`.

## DONE
- 829b9a1a9 test(image): partial image uploads (RED): core union/clipped_to, overlay dirty,
  layout/tests/a_partial_image_change_leaves_its_rect_for_the_renderer.rs, dll planner tests.
- GREEN part 1 (this commit): core `ImageDirtyRect` methods, overlay `image_dirty` arm,
  `apply_image_change(.., dirty_rect)` in window.rs.

## IN PROGRESS
- Engine: `CallbackInfo::change_node_image_rect` -> the overlay keeps the not-yet-uploaded
  dirty region per image node -> WebRender `update_image` with `DirtyRect::Partial`.

## NEXT
0. GREEN part 2: callbacks.rs `CallbackChange::ChangeNodeImage { dirty_rect }` +
   `change_node_image_rect`; e2e/runner.rs + common/event.rs pass it to
   `ContentChange::Image`; capture_common.rs test pattern `dirty_rect: _`; headless/mod.rs
   test literal `dirty_rect: None`; wr_translate2.rs planner (item = (slot, &ImageRef,
   ImageDirtyRect), `OverlayImageUpload.dirty`, `translate_dirty_rect`, clear after upload).
1. (done) Engine RED (core `ImageDirtyRect::union/clipped_to`, overlay dirty bookkeeping,
   `change_node_image_rect`, the WR upload planner) -> GREEN.
2. Raster core RED (tests) -> GREEN (tiles, blend, brush, selection, adjustments, filters,
   transform, crop, history, the `RasterEngine` trait).
3. The app: CanvasShell layout, tools, panels, canvas view (one image node, dirty rects),
   files (open / doc.json + layer PNG tiles via LocalDrive / export), --sample.
4. Registration (workspace, test members, CI step), `scripts/azphoto_e2e.py`, report.

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

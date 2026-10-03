# PHOTO progress (branch `wt/photo`, base `16d19442c`)

Task: the initial AzPhoto (examples/azul-photo, package AzPhoto) on CanvasShell, with its own
tiled raster core behind a `RasterEngine` trait; the engine piece it needs (partial image
updates by dirty rect); tests; `scripts/azphoto_e2e.py`; report `scripts/PHOTO_2026_10_01.md`.

## DONE
- Everything in the brief; see scripts/PHOTO_2026_10_01.md (the report: commits, api.json
  entry, least-sure spots, test commands, what is left).
- Engine: 829b9a1a9 (RED), 8a344ccfd, 279bae778, 383672264 (GREEN).
- App: af9a50285 .. 3bb842788; E2E 80552f353; registration 15999013c.
- Scratch checks: 78/78 raster+view+state tests run; the whole crate (lib + tests)
  type-checks against the real generated bindings (0 errors).

## IN PROGRESS
- (none)

## NEXT (for the parent)
- autofix the api.json entry `CallbackInfo::change_node_image_rect`, build, run the suites and
  `scripts/azphoto_e2e.py` (commands in the report); look at the screenshots.

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
- Text tool: an azul text-to-pixels API (engine work) - shown disabled with the reason.
- CPU backends still damage the whole image item on a partial change (follow-up).

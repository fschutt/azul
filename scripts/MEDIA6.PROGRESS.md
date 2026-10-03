# MEDIA6 progress (wave 6, 2026-10-03)

Branch `wt/media6` from `25d78e309`. Brief: `scripts/waves/wave6/MEDIA6.md`.

## DONE
- LOOK of all three apps (list below).
- `e24841ffb` RED: layout/tests/a_text_rasterises_into_a_raw_image.rs (+ all.rs).
- `6d5b2748f` `b09fd8448` `f6d634f31` GREEN: layout/src/cpurender/text_raster.rs (TextRasterStyle,
  rasterize_text_at, text_image(_with), draw_text(_with), unpremultiply_rgba); raster.rs
  `render_text` pub(super).
- AzPhoto: `1660ad26c` RED (state tests: text tool, live move, tool availability); GREEN
  `5ae307c52` (engine live edits), `fba92ddca` (state text/move), `49ea43c0a` (commands:
  azul_text on RawImage::from_text, on_text, Field::Text*), `aba432ca6` (ui Text bar, tool rail
  with_toggled/with_disabled; canvas Enter/Escape).
- AzVideoCut V1/V3: `e73d06036` RED (E2E fills-the-window), `239cf5e5e` GREEN.
- AzPhoto exports into the data tree: `64a35523b` RED, `88589a482` GREEN.
- AzPhoto on appkit: `c958a70d5` args, `4fa9ff60f` start/kit/ABOUT/SHORTCUTS, `0234096e4` settings page +
  Dialog sheets + AboutDialog, `6571f82fd` CloseGuard, `f111607bd` __azphoto_ ids + E2E (move, text).
- AzVideoCut: `59a7eaf68` RED / `3a6a1aa30` GREEN fit_within + fit_to on RawImage::fit_within /
  thumbnail (needs api.json `RawImage.fit_within` -> PhysicalSizeU32, report); `0c6d624e4`
  ProgressDialog + AboutDialog; `a5999b54d` args on appkit; `fa2145ef1` start on the kit, settings
  page, ABOUT/SHORTCUTS, store::data_root gone, E2E --data-dir; `d1687bc41` __azvideocut_ markers.
- AzPaint: `6449be8a0` RED E2E scripts/azpaint_e2e.py (live raster, undo/redo, exports in tree);
  `09e246bd0` E2E on pixels (azlin_e2e read_png / dark_pixels); `94478e917` GREEN 1 (MouseMove,
  poke node raw-1, __azpaint_ ids, body margin, AZPAINT_RASTER/STROKES).
- ENGINE `4192aa092` RED layout/tests/an_image_patched_in_place_survives_a_cached_relayout.rs,
  `a27ded13e` GREEN window.rs apply_image_change: patched DL handed to layout_cache.cached_display_list.
- AzPhoto `796d2da1a` canvas on MouseMove (same W3C MouseOver bug).

## IN PROGRESS
- (between units)

## NEXT (in order)
4. AzPaint: B UndoHistory<Vec<Stroke>> + menu accelerators (Undo LWin+Z, Redo LWin+LShift+Z;
   the engine dispatches menu accelerators, headless too - NOT in the key handler, double);
   C appkit (Cargo azul-appkit features=["azul"] - link features already coexist on android;
   args SPEC, kit, settings page, kit::handle_key in a window VirtualKeyDown handler,
   on_window_created, AboutDialog from Help); D exports: CPU raster in the menu callback ->
   encode_png -> kit::spawn_file_jobs Put paint/exports/canvas-<secs>.png / strokes-<secs>.svg,
   AZPAINT_EXPORTED <key> <bytes> in on_files_done (pending map key->len); drop export_path /
   export_png / std::fs::write; --sample strokes.
5. AzPhoto leftovers: P1 private Palette (D16); P3 History undo/redo row cut off.
6. Verify brief items: UndoHistory in Photo (engine.rs history) and VideoCut; VideoCut CloseGuard?
   (project autosaved?); ids from Uuid::from_seed(random_seed()) in all three.
7. Report scripts/MEDIA6_2026_10_03.md (api.json: TextRasterStyle + RawImage.from_text /
   draw_text, RawImage.fit_within; engine fix; AzReview ui.rs:547 still MouseOver - owner).

## Seen broken (LOOK)
Screenshots: `target/media6/{photo,vc,paint}/*.png` (not committed). LOOK harness:
`<scratchpad>/media6/look.py <App> <port> <out> <flow> [args]` through run_capped.sh.

AzPhoto (`--sample`, 1400x900): the editor renders and is usable.
- P1 flora looks like flat: the root `ShellThemeScope` pins `ShellThemeAccent::Blue` and the
  chrome is the app's private `Palette` (DEDUP D16) - menu buttons, slider thumbs, selection
  rows stay blue in flora. (Titlebar is flora brown.)
- P2 tool rail: the selected tool has only an app-painted background, no toggled state; the
  Text tool is drawn at opacity .45 instead of `Button::with_disabled`.
- P3 the History panel's undo / redo row is cut off at the bottom of the panels column.
- P4 sheets are a hand-made frame (`sheet_frame`, D12) - not dialog_kit; About is hand-made.
- P5 export writes a file with std::fs (`--export-dir`) / a save dialog - not into the data tree.
- P6 not on appkit (own args `--data`, data root `<data>/Azul`, settings not remembered).
- P7 Text tool is a stub (engine gap: no text raster API).
- P8 Move tool: no live preview (guide line only, applies on release).

AzVideoCut (`--sample`): opens after ~2 s (sample encoded through VideoToolbox).
- V1 THE EDITOR IS COLLAPSED: only the menu row + status bar show; bin, monitors, effect
  controls and timeline are 0 px tall. Root cause (app): the body / theme scope are not
  stretched to the window (`body` has no `height: 100%`, the `ShellThemeScope` dom no
  `flex-grow`), the body keeps its default 8 px margin; with `min-height: 0` panes the shell
  collapses to its content. AzPhoto sets both and fills the window.
- V2 dark mode: everything outside the (collapsed) root is the white window canvas.
- V3 the loading screen has no title row (Titlebar only in the empty state / editor).
- V4 theme / mode are two toolbar toggle buttons (no System, not remembered - D13).
- V5 Cmd+E did not open the export dialog in the LOOK run (to verify after V1).
- V6 About / export progress hand-made (D12); `fit_within` / `scale_to` twins of core (D15).

AzPaint (prebuilt, AZ_PAINT_DEBUG, scratchpad diag): after a mouse stroke "1 strokes" over an
empty canvas. (1) no on_pointer_move during the drag: MouseOver is ENTRY since 2a7712f66, movement
is MouseMove; (2) poke_canvas NodeId{inner: raw} (1-based) pokes the next node; (3) ENGINE: the
canvas re-rasterised on the pointer-up RefreshDom frame but the window kept the old picture until
a resize - apply_image_change's make_mut leaves the solver's cached_display_list with the
pre-patch Arc, the next cache-hit relayout serves it back. (4) 8 px body margin.

## Decisions
- AzPaint keeps its android/ios targets; appkit with `azul` is added unconditionally (azul-dll is
  already listed with link-dynamic AND link-static there, so no new feature conflict).
- AzPaint undo/redo/export keys live on the menu items' accelerators (engine dispatch), the window
  key handler only forwards the kit's keys.

## Open questions

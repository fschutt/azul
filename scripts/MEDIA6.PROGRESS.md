# MEDIA6 progress (wave 6, 2026-10-03)

Branch `wt/media6` from `25d78e309`. Brief: `scripts/waves/wave6/MEDIA6.md`.

## DONE
- LOOK of all three apps (list below).
- `e24841ffb` RED: layout/tests/a_text_rasterises_into_a_raw_image.rs (+ all.rs).

## IN PROGRESS
- GREEN text raster: new file `layout/src/cpurender/text_raster.rs` (`mod text_raster; pub use
  text_raster::*;` in cpurender/mod.rs); make `fn render_text` in cpurender/raster.rs
  `pub(super)`. Contents: `TextRasterStyle {font_family: AzString, size_px, line_height,
  color: ColorU, bold, italic}` repr(C) + `create/with_bold/with_italic/with_line_height`;
  `shared_font_cache()` (OnceLock<FcFontCache>, build_font_cache); `resolve_font(fc, style)`
  (query_with_fallback: family+bold/italic, then family, then sans-serif); shape each '\n' line
  with `text3::default::shape_text_for_parsed_font`, GlyphInstance at baseline (point = pen,
  baseline), line box = (ascent - descent + max(line_gap,0)) * scale, step = box * line_height;
  render with `super::raster::render_text(.., force_grayscale = true)` into a transparent
  AzulPixmap (premultiplied); `text_image_with` un-premultiplies -> RawImage RGBA8 straight;
  `draw_text_with(fc, img, text, style, x, y)` composites (RGBA8/BGRA8, straight or
  premultiplied target), false for other formats; `text_image(AzString, TextRasterStyle) ->
  OptionRawImage` and `draw_text(&mut RawImage, AzString, TextRasterStyle, f32, f32) -> bool`
  on the shared cache (the api.json fn_body targets).

## NEXT (in order)
1. GREEN text raster (above), commit.
2. AzPhoto: text tool on it; move-tool live preview (engine begin_move/move_to/end_move);
   tool rail Button with_toggled / with_disabled; sheets -> Dialog/dialog_kit, About ->
   AboutDialog; export into the data tree `photo/<uuid>/exports/` via Drive; appkit; prefixes.
3. AzVideoCut: V1 (body height 100%, margin 0, scope flex-grow) + E2E check; fit_within ->
   export core `image_scale::fit_within` as `RawImage::fit_size` or use `thumbnail`;
   ProgressDialog for export; AboutDialog; appkit; prefixes.
4. AzPaint: stroke not shown (poke_canvas passes `NodeId{inner: raw}` - off by one vs Photo /
   VideoCut `raw - 1`; verify); appkit, `__azpaint_` prefixes (markers const, not Uuid::short),
   UndoHistory, exports into the data tree, E2E script.
POWER: Mac on battery (coordinator 14:xx) - no long headless runs, commit every unit.

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

AzPaint: (run pending - the disk was full at 14:xx)

## Decisions

## Open questions

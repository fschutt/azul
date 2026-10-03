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

## IN PROGRESS
- (between units)

## NEXT (in order)
2. AzPhoto: export into the data tree `photo/<uuid>/exports/` via Drive (storage::export_key,
   RED in storage tests); close guard (CloseRequested + prevent_window_close when modified);
   sheets -> Dialog, About -> AboutDialog; appkit (args/data root/settings/shortcuts); prefixes;
   E2E (text tool + live move steps).
3. AzVideoCut: fit_within ->
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

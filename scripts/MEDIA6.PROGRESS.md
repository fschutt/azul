# MEDIA6 progress (wave 6, 2026-10-03)

Branch `wt/media6` from `25d78e309`. Brief: `scripts/waves/wave6/MEDIA6.md`.

## DONE
- (none yet)

## IN PROGRESS
- LOOK: run AzPhoto / AzVideoCut / AzPaint headless (prebuilt aa59b2d84), screenshots in target/media6/ (not committed)

## NEXT
- write the broken list below, then fix RED first

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

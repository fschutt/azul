# AZTERM11 progress (worktree agent-ad3cc20b4f35f44e0, no cargo)

Task: AzTerm - the last tab closes the window; the tab strip is the title bar
(TabsInTitlebar offsets, drag region); equal-width tabs that scroll
horizontally; smooth (pixel) scrollback; follow mode with a follow button;
E2E for all of it.

## Design (decided)
- Scroll position = the engine's display offset `scroll` (whole lines) slid UP
  by `scroll_fraction` of a line (app-owned, per tab): `scroll - fraction`
  lines up from the output. Any position off the output keeps the engine's
  offset >= 1, so alacritty keeps the content in view while output streams.
  The view draws the rows of `scroll` plus `TerminalScreen.line_below`, all in
  one rows container whose `top` is `-fraction * line_height`.
- Scroll events are applied by the app RELATIVE to the screen the view last
  showed (the engine moved its offset since, when output came in while
  scrolled up); a target of 0 is the output (follow).
- `TerminalScreen.new_lines`: the app counts display-offset rises (output that
  came in below a scrolled-up view); the view's follow button shows it.
- AzTerm re-renders a scrolled-up view at most every 15 ticks.
- The strip: `#__azterm_tabs` (drag region, TabsInTitlebar paddings) holding
  `#__azterm_tab-scroller` (overflow-x auto, scrollbar-width none) with the
  tabs (width 180px, min 110px, no-drag) and the "+" after it.

## DONE
- 39ef40483 test(terminal_view): RED - pixel scrolling, the slide, line_below, the follow button
- 7ba839928 feat(terminal_view): GREEN - the same

- 84734bbf6 test(azterm): RED - last tab / strip / still view
- 54d35ac96 feat(azterm): closing the last tab closes the window
- d1091783a feat(azterm): the strip of tabs is the title bar
- 9d7ffd151 feat(azterm): smooth scrollback and follow mode (+ sample stream)
- 8c62cb1fc test(e2e): azterm_e2e.py - strip, flood + follow button, many tabs, last tab
- 7d016b4e7 test(terminal_view): RED - an app that keeps whole lines still scrolls both ways
- c7576a14b fix(terminal_view): GREEN - the same (wheel_base / TerminalShared::asked)

## IN PROGRESS
- nothing: DONE, report sent (nothing compiled - the lead builds)

## NEXT (lead)
- azul-doc autofix drift sync (TerminalScreen + TerminalViewEvent fields),
  codegen, build dll + AzTerm, layout --lib terminal_view tests, AzTerm
  tests, scripts/azterm_e2e.py, screenshots

## Open questions / for the lead
- api.json: TerminalScreen gains line_below / scroll_fraction / new_lines,
  TerminalViewEvent gains scroll_fraction (autofix drift sync, then codegen)
  - AzTerm only compiles against the regenerated API.

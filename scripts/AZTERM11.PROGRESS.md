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

## IN PROGRESS
- AzTerm RED tests (lib.rs / sample.rs / session.rs tests)

## NEXT
- AzTerm: last tab closes the window; the strip as the title bar (scroller,
  equal widths, reveal); scroll.rs ViewScroll (relative scroll events, new
  lines, follow); sample stream (`seq N`, `yes | head -n N`); tick throttle
- E2E: scripts/azterm_e2e.py

## Open questions / for the lead
- api.json: TerminalScreen gains line_below / scroll_fraction / new_lines,
  TerminalViewEvent gains scroll_fraction (autofix drift sync, then codegen)
  - AzTerm only compiles against the regenerated API.

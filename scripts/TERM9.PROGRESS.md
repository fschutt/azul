# TERM9 progress - AzTerm (PTY + VT engine + TerminalView with virtualized scrollback)

Branch: wt/term9 (base e537ddbe2). Worktree: .claude/worktrees/agent-aeeb57dd4eb500ff0
Crate sources read from /tmp/term9_src (alacritty_terminal-0.26.0, vte-0.15.0, downloaded with curl, not compiled).

## DONE
- 2b1cc1059 progress file; 84b4f7566 decisions
- 814855418 widget data types + builder (layout/src/widgets/terminal_view.rs, registered in widgets/mod.rs)
- aa7d17d51 RED pure-logic tests (encoding_tests, palette_tests, view_tests)
- 3577bdf53 GREEN pure logic + theme palettes (flat.rs / flora.rs APPENDED) + data_table::thumb pub(crate)

## IN PROGRESS
- the DOM build (VirtualView host) + handlers in terminal_view.rs (marker "TERM9-NEXT: the build, the handlers.")

## NEXT
1. (done) widget types + pure logic.
2. DOM build (VirtualView host, rows, cursor, selection, scroll bar), handlers, theme APPENDs (flat/flora), manifest.
3. examples/azul-term (AzTerm): Cargo.toml, lib.rs, vt.rs (alacritty Term -> TerminalLine mapping, RED tests first),
   session.rs (PTY + EventLoop), ui, args, settings, sample recordings, registration, E2E script.
4. Report scripts/TERM9_2026_10_03.md.

## Decisions
- VT ENGINE + PTY: `alacritty_terminal` 0.26.0 (one crate for both). Why: the xterm-compatible state machine Alacritty
  and Zed's embedded terminal run on (grid + scrollback ring indexed directly by Line(-n) = no mutation to read a
  window, modes as TermMode flags for the key / mouse / paste encoders, Selection that rotates with output,
  display_offset kept stationary when output arrives while scrolled up, damage), AND its `tty` module is the PTY:
  openpty/forkpty-equivalent on Unix (rustix-openpty, TIOCSWINSZ on resize), ConPTY on Windows, plus an EventLoop
  thread that parses PTY output into the Term. Rejected: `vt100` (reading scrollback needs `set_scrollback` mutation,
  no selection / damage), `termwiz` (heavy; its terminal model wezterm-term is not on crates.io), `vte` alone (a parser:
  we would write the terminal), `portable-pty` (a second crate + nix 0.28 + filedescriptor + serial2 when alacritty's
  tty already does it). New deps in Cargo.lock: alacritty_terminal, vte 0.15, rustix-openpty, signal-hook 0.4, miow,
  windows-sys 0.59 (win only); the rest (base64, bitflags, home, parking_lot, polling, regex-automata, piper) exist.
- SCROLLBACK: VirtualView as the HOST (like the MapWidget: outer div with the handlers + dataset, a VirtualView child
  rendering the rows) - so the view gets its bounds (-> columns x rows, resize -> TIOCSWINSZ) and PTY output re-renders
  ONLY the terminal (`trigger_all_virtual_view_rerender`, no window DOM rebuild). Scrolling inside it is the
  scroll-window pattern in WHOLE LINES (cell_grid / data_table): the engine's display offset (lines up from the
  bottom) is the scroll position, the wheel / scroll bar / Shift+PageUp move it - no px extent, so 100k or 10M lines
  cost the same and an f32 never has to address them. Only the rows in view are ever built.
- The widget is GENERIC OVER ITS DATA: a data-source callback gives the rows in view (TerminalScreen) for the grid size
  the view has room for; events (Input bytes, Scroll, Select*, Copy, Resize) go to the app. Reusable for a log viewer.
- Key -> bytes, paste (bracketed), mouse reports (X10 / SGR / UTF-8), focus reports, the 256-colour palette: pure
  functions in the widget module (engine independent), tested there.

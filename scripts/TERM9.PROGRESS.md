# TERM9 progress - AzTerm (PTY + VT engine + TerminalView with virtualized scrollback)

Branch: wt/term9 (base e537ddbe2). Worktree: .claude/worktrees/agent-aeeb57dd4eb500ff0
Crate sources read from /tmp/term9_src (alacritty_terminal-0.26.0, vte-0.15.0, downloaded with curl, not compiled;
re-download with `curl -sSL -H "User-Agent: azul-build-agent (https://github.com/fschutt/azul)" https://static.crates.io/crates/<name>/<name>-<ver>.crate | tar xz`
if /tmp was wiped).

## DONE
- 2b1cc1059 progress file; 84b4f7566 decisions
- 814855418 widget data types + builder (layout/src/widgets/terminal_view.rs, registered in widgets/mod.rs)
- aa7d17d51 RED pure-logic tests (encoding_tests, palette_tests, view_tests)
- 3577bdf53 GREEN pure logic + theme palettes (flat.rs / flora.rs APPENDED) + data_table::thumb pub(crate)
- 09d398619 manifest; 685641fd0 + 272d4eb5e engine: a focus that listens for paste gets the paste chord
- 980e5fd76 widget build + handlers (VirtualView host, render_terminal, merge, key/text/paste/focus/mouse/wheel),
  fixtures::sample, build_tests
- 219d37e10 RED + efa08418d GREEN: examples/azul-term crate begun (Cargo.toml, main.rs, stub lib.rs) and src/vt.rs
  (`vt::screen(&Term, alt_sends_escape) -> azul::widgets::TerminalScreen`, `vt::GridSize` implements Dimensions)
- 200eaef7c src/session.rs: Session::spawn (tty::new + EventLoop + Notifier), Session::replay (recording, echo),
  write / resize / is_live / size, Signals {dirty, title, bell, exited} polled by the UI (take_dirty / take_title /
  take_bell); Listener answers PtyWrite through the EventLoopSender
- 1f69d558f src/ids.rs (const AzString __azterm_ ids) + src/sample.rs (build_session, log_session(n), PROMPT)

## IN PROGRESS
- (none - between units)

## NEXT
1. (done) Manifest. 2. (done) Engine paste fix (focus_hears_paste; see commits above). The widget's key handler
   leaves the paste chord alone (KeyAction::Paste -> DoNothing): the engine's Paste event brings the text to
   on_terminal_paste. Shift+Insert has no engine paste yet (limitation for the report).
3. examples/azul-term (AzTerm) - NEXT FILE: src/lib.rs (replace the
   stub start(); the window), then registration + scripts/azterm_e2e.py:
   - src/lib.rs (start; layout: ShellThemeScope::body + appkit title row + a tab strip + the
     TerminalView pane (a `position: relative; flex-grow: 1` container) + StatusBar; CloseGuard asks when a tab
     runs a command; About; settings page). TerminalView data source = an extern "C" fn(RefAny, TerminalGridSize)
     -> TerminalScreen that locks the session's FairMutex<Term>, resizes it (and the PTY) when the grid differs,
     returns vt::screen. on_event: Input -> notifier.notify(bytes) + scroll_display(Bottom); Scroll ->
     term.scroll_display(Scroll::Delta(new - old)); SelectStart/Extend/End/Clear -> alacritty Selection with
     term::viewport_to_point(display_offset, Point<usize>); Copy -> term.selection_to_string() ->
     info.set_clipboard_content(...) (check api.json for ClipboardContent / a text setter).
   - src/session.rs: alacritty_terminal::tty::new(&Options, WindowSize, id) + EventLoop::new(term, listener, pty,
     false, false).spawn() + Notifier(loop.channel()); the Listener sets an AtomicBool on Wakeup / Title / Bell /
     ChildExit; a 16 ms azul Timer calls trigger_all_virtual_view_rerender when dirty (RefreshDom for title / exit).
     Resize: the data callback sees a new TerminalGridSize -> term.resize + notifier.on_resize (TIOCSWINSZ).
   - src/profiles.rs: profiles + settings json in <data root>/term/ through the Drive from a Thread (appkit jobs).
   - src/sample.rs: --sample replays recorded sessions (cargo build, git status, a TUI frame) into a Term with no
     PTY - deterministic for the headless E2E.
   - Register: root Cargo.toml members, scripts/workspace_test_members.txt, .github/workflows/rust.yml dll_tests.
   - scripts/azterm_e2e.py against the debug server.
4. Report scripts/TERM9_<date>.md (api.json list: see "API" below).

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
  the view has room for; events (Input bytes, Scroll, Select*, Copy) go to the app. Reusable for a log viewer.
- Key -> bytes, paste (bracketed), mouse reports (X10 / SGR / UTF-8), focus reports, the 256-colour palette: pure
  functions in the widget module (engine independent), tested there.
- Light/dark colour pairs reuse `ChartColor` (twins: `ChartColor` in widgets/chart.rs and `IconModeColors` in
  core/src/icon.rs are the same concept - say so in the report).
- The palette is not theme-block merged per run: the DOM is rebuilt on an app-theme switch (UiTheme::current), each
  colour carries its light / dark pair as conditional props.
- The build tests (build_tests) were committed with the build (980e5fd76), not RED first - note in the report.

## API (for the report, api.json terms)
- types: TerminalColor (enum C,u8: Foreground, Background, Indexed(u8), Rgb(ColorU)), TerminalStyle, TerminalRun(+Vec,
  Option), TerminalLine(+Vec, Option), TerminalPoint, TerminalSelection (+Option), TerminalCursorShape, TerminalCursor,
  TerminalMouseMode, TerminalMouseEncoding, TerminalModes, TerminalMouseButton, TerminalMouseAction, TerminalPalette
  (+Option), TerminalStyleColors, TerminalGridSize, TerminalScreen, TerminalSelectionKind, TerminalViewEventKind,
  TerminalViewEvent, TerminalView; callbacks TerminalViewOnEvent(Callback) (RefAny, CallbackInfo, TerminalViewEvent)
  -> Update and TerminalViewDataSource(Callback) (RefAny, TerminalGridSize) -> TerminalScreen; fn xterm_256_color.

## Open questions
- (none)

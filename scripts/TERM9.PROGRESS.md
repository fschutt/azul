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
- f9bad78f0 RED + 7f5f1a588 GREEN: key_action leaves Ctrl+Shift+letter to the window off macOS
- 48776e635 src/lib.rs: the window (DeveloperShell: activity bar, side bar of tabs, the TerminalView pane, status
  bar), AppState (tabs, open / close / select / cycle), the data callback (resize + vt::screen), on_terminal
  (write / scroll / select / copy), the 16 ms output timer, window keys, About, settings; tests
- f208e30fd registration (root Cargo.toml member, workspace_test_members.txt, rust.yml dll_tests step)

- 59264ae05 scripts/azterm_e2e.py; 52ffe8488 wheel_ownership list; 975d682a9 Rust-only helpers pub(crate)
- REPORT: scripts/TERM9_2026_10_03.md (committed with this progress update)

## IN PROGRESS
- (none) - the task is complete as far as it can go without compiling.

## NEXT
- Only if resumed with more work: the "Left" items (in the report and below), starting with profiles / settings
  files through the Drive (appkit settings page sections), then FindBar-based find in the scrollback.
- Left for later (in the report): profiles / settings files through the Drive (settings page sections:
   font size, Alt as Meta, scrollback, shell command), split panes, find in scrollback (FindBar), tab tear-off,
   URL Ctrl+click, the bell flashing the tab, Shift+Insert paste (no engine paste for it), triple-click line select,
   motion reports with no button held (1003), cursor blink, IME preedit drawing, a CloseGuard for running commands.

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

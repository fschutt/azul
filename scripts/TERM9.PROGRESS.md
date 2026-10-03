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
- 980e5fd76 widget build + handlers (VirtualView host, render_terminal, merge, key/text/paste/focus/mouse/wheel),
  fixtures::sample, build_tests

## IN PROGRESS
- (none - between units)

## NEXT
1. (done 09d398619) Manifest.
   `all.push(("terminal_view", super::terminal_view::fixtures::sample().dom()));` and check the theme_contrast
   groups (~line 2775 in mod.rs) to see whether a new widget must be listed there.
2. Engine (paste): core/src/events.rs `handle_key_down` returns None for Cmd/Ctrl+V on a non-editable focus without
   a selection, and CallbackInfo cannot READ the clipboard -> a terminal never gets a Paste event. Fix: a new
   `InputInterpreterState` field `focus_hears_paste` (the focused node has a FocusEventFilter::Paste callback), set in
   dll/src/desktop/shell2/common/event.rs (~11995) and layout/src/e2e/runner.rs (~1207), carried through the ctx
   (core/src/events.rs ~5201 / 5234 / 5283), and in handle_key_down: Paste is the engine's (AddAndSkip -> the
   deferred Paste event) when focus_is_editable || has_selection || focus_hears_paste; Copy / Cut / SelectAll
   unchanged (Ctrl+C / Ctrl+A must reach the terminal's key handler). Update the struct literals in
   core/src/events_test.rs (~3678 / 3750 / 3786 / 3831) and core/src/events.rs ~5214. RED test first in
   core/src/events_test.rs ("a paste chord on a node that listens for paste becomes a paste event").
3. examples/azul-term (AzTerm):
   - Cargo.toml (alacritty_terminal = "0.26", azul-appkit, azul-storage, serde, serde_json) like azul-drive's.
   - src/main.rs (thin), src/lib.rs (start; layout: ShellThemeScope::body + appkit title row + a tab strip + the
     TerminalView pane (a `position: relative; flex-grow: 1` container) + StatusBar; CloseGuard asks when a tab
     runs a command; About; settings page).
   - src/vt.rs: Term<Listener> + the mapping grid -> TerminalScreen (runs split at style AND at wide-char changes,
     colours Named/Indexed/Spec -> TerminalColor, Flags -> TerminalStyle, cursor -> view rows through
     display_offset (alacritty term::point_to_viewport), the selection range -> view rows clipped, TermMode ->
     TerminalModes). RED tests first: feed bytes with `vte::ansi::Processor::<vte::ansi::StdSyncHandler>::new()`
     `.advance(&mut term, bytes)`.
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

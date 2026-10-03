# SMALL6 progress (wave 6, 2026-10-03)

Branch `wt/small6` from `25d78e309`. Apps: AzCalculator, AzSetup, AzBuilder, AzReview, AzMaps, AzShells, AzWidgets.
Screenshots: target/small6-shots/ (not committed). Look driver: target/small6/look.py, look_all.py.

## DONE
- 5e82d2094 progress file
- 1c4571e3c / 182655027 codegen: `AzString::from_const_str` in the Rust bindings (RED / GREEN) - the prefix
  ruling's `const X: AzString = AzString::from_const_str(..)` did not exist in link-dynamic builds.
- f07100715 / 14d55866e shells: `ShellThemeScope::body()` fills the window (RED unit + layout test / GREEN).

- AzCalculator: 2cecbf0ba body(); 3da564e80 / 8f473b78c panel results wrap (RED/GREEN); ee29d9bb4 ids module;
  d749c308d ui on ids; a21dc5a2b + 84b97862e + 0e2b94d78 E2E (prefixed ids, Ctrl+C last, About box).
- appkit (INFRA6's crate, minimal): 5c8d03540 / 64bad139b About box = standard AboutDialog in a Modal from the
  settings' About section (Kit.about_open, about_open/set_about_open, `<APP>_ABOUT open|closed` stdout).

## IN PROGRESS
- POWER WARNING (coordinator, 16% battery): commit after every small unit; NO long headless runs until told.

- AzSetup: 3637b7921 body(); ef93a054c / 0e07e1e9a split_switches + open_on (RED/GREEN); a5b8a8ad3 on appkit
  (own Args parser deleted); 878f3092c / a37cabfd0 settings remembered (RED/GREEN); 24237e413 Escape = Cancel;
  7c4867de1 AZSETUP_BOXES stdout; dcf5e50c0 E2E rewritten on azlin_e2e (Escape/F1, buttons in window,
  remembered dark mode). AzSetup defines no DOM ids/classes of its own (widgets' only) - prefix rule n/a.

- AzShells: e9b384a0e body(); fc86b135a appkit choose_theme/choose_mode (INFRA6's crate, refactor);
  2e0536f2c on appkit + ids.rs (__azshells_); 3a5fde5f2 E2E on azlin_e2e.

- AzWidgets: 1c44c30e8 one crate::keep (DEDUP); 5a4e91e15 Building blocks section (blocks.rs); 0eba1189e
  Flat/Flora = the app theme (get_theme/set_theme); 84ddb8ad2 e2e/building_blocks.json; e22863a7a Titlebar.

- AzMaps: 4d18cda71 / 2e0f66938 model.rs (pins file, kept viewport, pan math; RED/GREEN); a8759df48 WIP part 1
  of the lib.rs rewrite (header doc, imports, SPEC/ABOUT/SHORTCUTS, MapState + methods, ids.rs). lib.rs is
  INCOMPLETE until part 2: the old layout / callbacks / start / engine_feature_tests / android ctor are in
  commit 2e0f66938, examples/azul-maps/src/lib.rs (port from there).

## NEXT (exact)
- AzMaps part 2 (examples/azul-maps/src/lib.rs, append after `// ==== LAYOUT (next commit) ====`): layout()
  (BrowserShell: address bar = toolbar ids::PAN_*/ZOOM_*/RECENTRE/LOCATE/CLEAR_PINS/SETTINGS + coords; tree =
  pins column; content = map area with MapWidget + compass + location dot; details = DetailsPane of the centre;
  ShellThemeScope::body(); kit::settings_page when open), callbacks (buttons, on_key via kit::handle_key first,
  on_viewport_changed, on_pin_tap -> save pins, the sensor timer also keeps the viewport after
  VIEW_SAVE_IDLE_TICKS), on_window_created (kit + FileJob::Get pins), on_files_done, start() (AppArgs, kit,
  parse_view(VIEW_KEY)), keep engine_feature_tests + android ctor. Then part 3: scripts/azmaps_e2e.py.
- (old plan) AzMaps (examples/azul-maps/src/lib.rs, 634 lines, hand-rolled header): on BrowserShell (S5) - address bar
  slot = search/coords + zoom/pan Toolbar buttons, tree = pins list (TreeView), content = the MapWidget,
  details = selected pin (DetailsPane), status bar = centre/zoom; ShellThemeScope::body(); appkit (args,
  settings: last viewport; pins saved as maps/pins.json through kit::spawn_file_jobs); ids `__azmaps_`;
  E2E scripts/azmaps_e2e.py (pan / zoom buttons -> AZMAPS_VIEW lines; pin list; restart remembers).
- then AzReview (DocumentShell), AzBuilder (minimal).
- AzShells: body(), appkit, prefixes, About, E2E.
- AzWidgets: the new widgets; app theme; LOOK (short, it has a <video>).
- AzMaps, AzReview: onto shells + themes + appkit + Drive; AzBuilder: checklist minimal.

## Broken (seen in the LOOK, prebuilt aa59b2d84)
- ALL shell apps (AzSetup, AzShells, AzCalculator, AzReview, AzMaps): the UA body margin (8px) and an auto-height
  body: the shell in a band at the top, white canvas below (dark mode too), panes collapsed (S5: no tree / content
  height), AzSetup's button row clipped / pushed out of the window. FIXED in the engine: ShellThemeScope::body().
- AzCalculator E2E fails at "Ctrl+C to copy": Cmd/Ctrl+C with a focused NON-editable node (a keypad Button after a
  mouse click) is claimed by core/src/events.rs handle_key_down's shortcut block (`AddAndSkip(CopyToClipboard)`):
  neither the window VirtualKeyDown callback nor the Copy event reaches the app. WRITER6 owns that block (its
  Undo/Redo item is the same root cause) -> report. The E2E step is moved to the end so the rest is checked.
- AzCalculator history panel: a long result (sqrt(2), 32 digits) overflows the 260px panel; it overflows to the
  LEFT (start digits cut) - CSS says an overflowing line is start-aligned (overflows the end edge). Engine
  (text3 / solver3, MAILENG6) -> report. App: results wrap (`overflow-wrap: anywhere`).
- Headless screenshots: the FIRST screenshot after `set_theme` (same theme) + `set_mode` shows text from two layouts
  (ghost / doubled glyphs, stale clipping: "Reading pane" smeared in S4, wizard labels at old positions); the next
  frame is clean. Engine (HEADLESS6: screenshot / display list after a rebuild) -> report.
- AzSetup Options page: the PATH checkbox row drawn over the radio set (may be the same stale-frame issue; re-check
  after body()).
- AzReview: hard-coded light colours (no dark mode, no flora), hand-rolled chrome; sessions written to the temp dir
  from callbacks (std::fs) - not the Drive.
- AzMaps: hand-rolled header (#2b2b2b, blue buttons), no themes, tiles never load headless (no network).
- AzBuilder: the window is an empty white body (dark mode too); the builder UI is the debug server's web page.
- AzShells: S11's bottom tabs overlap ("Contacts" label below the bar) - re-check after body().

## Decisions
- `from_const_str` added to the codegen (doc/src/codegen/v2/lang_rust.rs) rather than per-app helpers: every app
  agent follows the prefix ruling; one generator change serves all.
- The window-filling body is ONE helper on the shells (`ShellThemeScope::body()`), not CSS strings per app.

## Open questions
- (none)

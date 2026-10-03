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
  of the lib.rs rewrite; 4d8a9006b part 2 (BrowserShell layout, callbacks, pins file + kept view, appkit);
  0bb82399f scripts/azmaps_e2e.py. (Resumed after the power loss from 05f87019f.)

- AzReview: ecfc1143c / d42a39f6c archive_name (RED/GREEN); 6a21926ef sessions via appkit file jobs into
  review/ (session::archive builds bytes; one writer), appkit switches/window, Mod+S, 1-9, stdout lines;
  12f01aa07 DocumentShell + ShellThemeScope::body() + system colours + Buttons + settings page; 17944f4b0
  scripts/azreview_e2e.py.

- AzBuilder: 3fd127f98 documented, not changed (its empty body is the builder's canvas).
- Report: scripts/SMALL6_2026_10_03.md (this commit).

## NEXT (exact)
- DONE. Nothing left on this branch; the parent integrates (api.json ShellThemeScope.body, codegen, build,
  suites, E2Es) - see the report's "Left".
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

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

## NEXT (exact)
- AzSetup (examples/azul-setup/src/lib.rs): (1) window_root (~line 470) -> ShellThemeScope::body(); commit.
  (2) ids module `__azsetup_`; (3) appkit: args (--screen/--theme/--mode/--data-dir), settings dialog values
  remembered in settings.json (kit::set_value on Apply/OK), shortcuts table; (4) E2E scripts/azsetup_e2e.py.
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

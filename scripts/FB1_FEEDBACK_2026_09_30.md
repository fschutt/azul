# FB1_FEEDBACK - the user's Mac feedback on AzMeet, AzCalendar, AzBuilder (2026-09-30)

Branch `wt/fb1-user-feedback` from `66db869f1`. Nothing compiled here (house rules); every
claim marked "run" was run against the parent's prebuilt binaries (`target/release/Az*`,
`target/azul-lib/libazul.dylib`, headless, debug server, all processes killed) or node / python.

## Commits (in order)

| commit | what |
|---|---|
| e9fbed1ed | test(azcalendar): the page chrome and its text are system colours in light and dark (RED) |
| 0dc871134 | fix(azcalendar): the whole page follows the window's light / dark mode |
| 1f49e67cb | docs(fb1): progress |
| d6658bd14 | test(debug-server): get_mode / set_mode and get_theme / set_theme, and the builder page follows the app (RED) |
| 98a488f8d | feat(debug-server): get_mode / set_mode and get_theme / set_theme; the builder page IS the app's mode |
| 89abfdfe8 | docs(fb1): progress |
| 2fa21e71e | test(headless): a resize takes the fast path the desktop shells take (RED) |
| 1023db90c | fix(headless): a latched resize takes IncrementalRelayout::Resize, as every desktop shell does |
| a6f611432 | test(azmeet): a resize probe over the debug server, with references recorded on the restyle path |
| 8e56c4400 | test(layout): the resize fast path paints what a relayout paints - AzMeet's lobby and statistics |
| (last) | docs(fb1): report + progress |

## Item 3 - AzCalendar buttons in light / dark: ROOT CAUSE IN THE APP (fixed)

Run: `AZ_MODE=dark` / `AZ_MODE=light` headless AzCalendar (`AZCAL_DATA` in the scratchpad),
`take_screenshot`. Dark: "Previous week / This week / Next week" INVISIBLE (pixel (160,54) =
255,255,255), the titlebar title nearly invisible. Light: fine.

Why: the Buttons are right. In dark mode the flat Default button paints the desktop's
`system:button-face` (macOS `controlColor` = a translucent white) with white `system:button-text`
- grey on a dark surface - but AzCalendar hard-coded a LIGHT page (BODY #f4f5f8, TOOLBAR /
FOOTER / days / popover / sheet #ffffff, text #1d2330) and filled its title row white
(`Titlebar::with_background(ColorU::rgb(0xff,0xff,0xff))`), so face and ink sat on white.
Button variants checked: flat.rs gives only the Neutral (Default) surface the dark face,
OwnColour (Primary, Secondary, Success, Danger, Warning, Info) keep their colour in both modes,
Link takes `system:link` in dark; run: AzWidgets dark shows Default grey, Primary blue, Danger
red, all readable. No engine bug, no engine change.

Fix (examples/azul-calendar/src/lib.rs, colours / CSS constants / `title_row()` ONLY - CAL2 owns
the week grid and popover logic):
- BODY `color: system:text; background: system:window-background`; TOOLBAR / FOOTER / side sheet
  / POPOVER card `system:window-background`; days and the day header
  `system:control-background` (`DAY_PAINT`); every rule `system:separator` (`LINE`); hour labels,
  event / draft times, form hints `system:secondary-text` (`SECONDARY`, `LABEL`); today's header
  ink `system:accent`.
- The app's own colours carry a dark twin under `@media (prefers-color-scheme: dark)`:
  `TODAY_PAINT` (#f7faff / #1b2433), `EVENT_PAINT` (#dbe7ff + #2f6db0 edge / #233a5e + #6ea8ff),
  `DRAFT_PAINT` (#eef4ff dashed #2f6db0 / #1a2c4d dashed #6ea8ff), `DRAFT_TITLE` (#2f6db0 /
  #8dbbff), `NOTICE` (#2c4a7a on #e6eefc / #c4d7ff on #1f2d45), `ERROR` (#b3261e / #f2b8b5). The
  "now" line stays #d93025 in both.
- `title_row()`: `Titlebar::create("AzCalendar").without_border_bottom().dom()` - no fill, so the
  page's `system:window-background` (= the toolbar's) shows through in both modes; the title's
  default ink already has a dark twin.
- Edited lines (for merging with CAL2): the constants block after `WEEK_SCROLL_ID`, `title_row()`,
  in `week_grid` the header `background` and the hour-label colour, `day_header`'s today colour,
  `day_column`'s `let paint` + `{paint}` and the hour-line `{LINE}`, `event_block`'s
  `{EVENT_PAINT}` + time `SECONDARY`, `draft_block`'s `{DRAFT_PAINT}` + `DRAFT_TITLE` +
  `SECONDARY`, `popover_panel` / `form_panel` / `meet_toggle` colour literals, two error lines
  -> `ERROR`, and a `mode_tests` module appended at the end of the file.
- Palette checked by running: the same CSS mounted (`mount` op) into headless AzCalendar in both
  modes: dark page (50,50,50), days (30,30,30), today (27,36,51), event (35,58,94), separators
  (70,70,70); light page / days white, today (247,250,255), event (219,231,255).
- Test: `mode_tests` in examples/azul-calendar/src/lib.rs (RED at e9fbed1ed: BODY / TOOLBAR /
  FOOTER / LINE / LABEL / POPOVER carry fixed colours; the fix adds a second test that every
  app-own colour has a dark twin).

## Item 4 - AzBuilder mode not synchronized: NO CHANNEL (fixed)

Root cause: the builder page kept its own Auto / Light / Dark (localStorage) and the debug server
had no op to read or switch the APP's mode, so page and window disagreed.

- Ops (layout/src/e2e/full.rs `DebugEvent` + `ResponseData::Mode` / `::Theme`), through the same
  `CallbackInfo` calls an app's toggle makes (`set_mode`: restyle of every window; `set_theme`:
  every window's DOM rebuilt), no `needs_update`:
  - `get_mode` -> `{"mode":"system"|"light"|"dark","resolved":"light"|"dark"}`
  - `set_mode {"mode":"light"|"dark"|"system"}` (anything else refused, naming the three)
  - `get_theme` -> `{"theme":"flat"|"flora"|...}`, `set_theme {"theme":"flora"}` (empty refused)
  - curl: `curl -s -X POST localhost:8765/ -d '{"op":"set_mode","mode":"dark"}'`
  - classified in doc/src/gene2e.rs `OP_POLICY` (get_* observation, set_* app-callback API).
- Page (debugger.js `app.mode`): once connected it reads `get_mode` at once and every 1.5 s while
  visible (quiet fetch, outside the request log), adopts the app's choice (system = Auto) and on
  Auto shows the app's `resolved` mode, not the browser's desktop; the toggle calls `set_mode`
  and reads back 150 ms later; a read that started before a local choice is dropped (no flip-
  back). Without the ops (older server, mock) the page keeps its own choice. The four ops are in
  the page's command schema.
- Docs: doc/guide/en/debugging.md (command vocabulary + "Light / dark mode and the app theme"
  with curl examples); the enum variants carry curl lines too.
- Tests: layout/src/e2e/mode_ops_tests.rs (4 scenarios through the real dispatcher: round trip
  system -> dark -> light -> system; "purple" refused and the mode kept; theme flat -> flora ->
  flat; empty theme refused). builder-mode-smoke.mjs: its mock gains the app mode; run: 21/27 on
  the old page (the 6 new checks red), 27/27 after. Other builder smokes unchanged (run): dnd
  26/26, dnd-indicator 19/19, export 50/50, extras 40/40, project 42/42, `node --test` 4/4.

## Items 1 and 2 - AzMeet "input forgets to stretch" / "statistics break lines on resize"

Not reproduced; the harness could not reproduce them, and that is fixed.

What was run (headless AzMeet, debug server, `get_node_layout` / `get_display_list` /
`take_screenshot` after each op, `wait_frame` x2-3):
- Lobby: resize 1100 -> 900 -> 700 -> 560 -> 500 -> 450 -> 400 -> 350 -> ... -> 1100, 300 ->
  1100, 580 / 575 / 570; `text_input` and `key_down` with `text` (86 characters into the join
  field, "/x/y/z" into the server field); `focus_node`; hover; clicks; Enter / blur -> status line
  "Asking ..." -> "... is unreachable"; `dpi_changed` 192 / 144 / 96. The server input stayed
  520 x 22 and the join input 461.4 x 22 whenever the card was 576 wide; below that they follow
  the card (e.g. card 350 -> inputs 294 / 235.4) and stretch back exactly.
- Call view (`AZMEET_AUTOCREATE=1`, local Worker at :8787): resize 1100 -> 900 -> 700 -> 560 ->
  700 -> 900 -> 1100 -> 600 -> 1100: the columns shrink, clamp at their min-content (Speakers
  54.9 at 560), re-wrap ("(none detected)" 1 -> 2 lines, 15.5 -> 31.0) and un-wrap to exactly
  the numbers of the first 1100 frame.

Why headless could not see it (ROOT CAUSE of the gap): a resize that crosses no breakpoint
latches the resize FAST path. macOS (`build_atomic_txn`), X11, Wayland and Windows answer it with
`IncrementalRelayout::Resize` (`resize_only_hint`: the retained tree, its warm per-node caches -
taffy's measure + final-layout memo, the pure-measure cache - and a PATCHED display list).
Headless's `service_frame` consumed the latch and ran the RESTYLE relayout (full reconcile whose
clone drops every measurement, deac0bebb; no patch), so every headless resize took a path no
desktop window takes. Fixed in 1023db90c (RED 2fa21e71e: `last_reconcile_was_skipped` after a
latched resize).

Engine suspects (not proven): (a) the taffy bridge memoises a FINAL layout by its inputs, but
`compute_non_flex_layout` also writes the subtree's IFC line layout, children's offsets and used
sizes, which a measure pass in the same frame can overwrite - a final-layout HIT then keeps the
measure's state (the memo-with-side-effect shape of ci_green_and_layout_cache_2026_09_07; only
reachable when taffy caches survive a frame, i.e. the fast path; deac0bebb cleared them on the
clone for a bug of this class, 4d0aa30c5 then added the fast path without the clone). Tracing
AzMeet's lobby and devices panel through taffy's cache keys I could not find a sequence that
triggers it in steady state (the keys change together with the final's), so no speculative fix
is committed. (b) the display-list patch (`PatchState`), which only resize-skip passes use.
Candidate fix for (a) if (B) below shows it: a per-node "final layout current" bit (NodeCache),
cleared by any measure computation of the node, set after its final computation, and a
PerformLayout `cache_get` that misses unless the bit is set.

Tools for the parent after the rebuild:
- (B) layout/tests/the_resize_fast_path_paints_what_a_relayout_paints.rs: two LayoutWindows, one
  history, one resizing like a desktop shell and one like the restyle relayout, AzMeet's lobby
  (REAL flat TextInputs + Buttons) and devices panel (5 columns, counters ticking, rebuilds
  between resizes); after every step the display lists must be the same items (geometry rounded
  to 0.01). NOT run here; RED if the Mac bugs come from the fast path (it names step and item),
  GREEN otherwise.
- scripts/fb1/azmeet_resize_probe.py + reference_lobby.json / reference_call.json: recorded on
  the current build (restyle path), deterministic (run: 18/18 and 19/19 replay). After the
  rebuild, `--compare` reports every step where the fast path paints differently.

## api.json

No changes. The new ops and response structs are layout-internal (`DebugEvent`,
`ResponseData::Mode` / `::Theme`, `ModeResponse`, `ThemeResponse`); they use the existing
`CallbackInfo::get_mode` / `get_resolved_mode` / `set_mode` / `get_theme` / `set_theme`.

## Least sure to compile

1. layout/src/e2e/full.rs: the `GetMode` / `SetMode` / `GetTheme` / `SetTheme` arms (types of
   `callback_info.get_mode()` = `azul_core::window::OptionDarkLightMode`, `get_resolved_mode()`,
   `set_theme(name.to_string().into())` into `AzString`); `ModeResponse { mode: &'static str,
   resolved: Option<&'static str> }` under `#[cfg(feature = "std")]` like its siblings.
2. dll/src/desktop/shell2/headless/mod.rs: `relayout_existing_dom(kind: event::IncrementalRelayout)`;
   the new test calls the private `service_frame` from the child `tests` module and imports
   `PlatformWindow` locally for `snapshot_window_state_baseline`.
3. layout/tests/the_resize_fast_path_paints_what_a_relayout_paints.rs: `FcFontCache::clone`
   (rust-fontconfig 5.0.0 implements it), `TextInput::create().with_text("..".into())`,
   `Button::create("..".into()).dom()`, `layout_cache.cached_display_list` 6-tuple.
4. examples/azul-calendar: format strings `{DAY_PAINT}` / `{SECONDARY}` / `{EVENT_PAINT}` /
   `{DRAFT_PAINT}` capture consts (as `{LINE}` already did); `ColorU` no longer used (it came
   from `prelude::*`, no warning).

## Test commands for the parent

```
cargo test --release -p azul-layout --lib e2e::mode_ops_tests
cargo test --release -p azul-layout --test all the_resize_fast_path_paints_what_a_relayout_paints
cargo test --release -p azul-dll --lib --features build-dll a_resize_takes_the_fast_path_the_desktop_shells_take
cargo test --release -p AzCalendar --lib mode_tests
cargo test --release -p azul-doc every_real_op_is_classified     # gene2e OP_POLICY
node scripts/debugger-ui/builder-mode-smoke.mjs                   # 27/27
# after rebuilding the dylib + AzMeet:
python3 scripts/fb1/azmeet_resize_probe.py --view lobby --compare scripts/fb1/reference_lobby.json
python3 scripts/fb1/azmeet_resize_probe.py --view call  --compare scripts/fb1/reference_call.json   # needs the Worker dev server on :8787
```
Combined RED pass: e9fbed1ed (AzCalendar), d6658bd14 (ops + smoke), 2fa21e71e (headless fast
path) are genuine REDs (the smoke RED was seen: 21/27). 8e56c4400 is a differential pin whose
state on the base is unknown.

Watch: after 1023db90c every headless / AZ_E2E `resize` takes the fast path (op-resize-*.json,
bug-textinput-resize-select-visual.json, ...). A scenario that turns red there has found a
desktop resize bug that was hidden before.

## Left

- Items 1 / 2: run (B) and the probe on the rebuilt tree; if red, the memo fix above (or the
  patch) with (B) as its RED.
- AzCalendar in both modes on the rebuilt binary (screenshots): not seen yet with the new CSS
  (the palette was checked through `mount`).

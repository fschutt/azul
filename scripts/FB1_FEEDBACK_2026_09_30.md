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
| ba8f50102 | docs(fb1): report and progress |
| 43c226479 | fix(layout): a memoised final layout is served only while its subtree holds what it wrote (the ENGINE fix for items 1 and 2; RED = 8e56c4400) |
| 293d7d4b7 | docs(fb1): report + progress (follow-up) |
| 9b2939de8 | test(layout): the fast-path comparison tolerates float noise (0.02 px) and GPU key ids, nothing else |
| 5b51d6d79 | fix(layout): a placeholder prompt is attributed to its own node on a patched display list |
| (last) | docs(fb1): report + progress (round 3) |

Round 3 (after 43c226479 fixed the widths): the lobby's item 41, the join field's placeholder
prompt, was attributed to NodeId(2) on the fast path vs NodeId(20) (its value `<p>`) on a
relayout. `maybe_paint_placeholder_prompt` pushed its runs under whatever
`DisplayListBuilder::current_node` was left; on a patched build the node's background run is
spliced (`try_copy_cached_run` restores the node current before it - the centring div, re-emitted
because it resized), so the prompt inherited the wrong owner. It now names its own node
(5b51d6d79). The other two differences were the test's: glyph float noise (53.29 vs 53.28) and
GPU key allocation ids (the fast path keeps its keys) - now 0.02 px tolerance and key ids
normalised, owners / words / colours still exact (9b2939de8).

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

ROOT CAUSE IN THE ENGINE (fixed in 43c226479). Not reproducible headless until 1023db90c; the
differential test 8e56c4400 reproduced both on the parent's build (all 3 red): after the first
fast-path resize the lobby's stretched "AzMeet" span (NodeId 4) painted 93.89 px wide instead of
520 at every step, the Microphones column's "(none detected)" (NodeId 13) 54.90 instead of 75.86,
and the viewport's horizontal thumb differed.

The mechanism (a memo with side effects): taffy memoises a FINAL layout (`PerformLayout`) by its
inputs, but in the taffy bridge a MEASURE of the same subtree writes the state that final layout
produced - `compute_child_layout` stores every answer (hypothetical-cross and min-content probes
too, even cache hits) as the node's `used_size`, `compute_non_flex_layout` re-flows the inline
content and re-places the children at the measure's constraints. On the fast path the taffy
caches survive the pass. D's hypothetical-cross slot for the card alternates between
(576, MaxContent) and (576, Definite(h)), so every pass MEASURES the card; its children answer
their hypothetical cross size - the span's content width 93.89 - into `used_size`; the card's
final layout (same inputs as last frame) came from the memo, and nobody wrote the stretched 520
back. The Microphones column, clamped at its min-content 75.86, is measured, "(none detected)"
answers its widest line 54.90, and the column's final is a memo hit. The restyle relayout never
showed it: its reconcile clones start with empty taffy caches (deac0bebb); the fast path
(4d0aa30c5) took the retained tree without that clone.

Fix (layout/src/solver3/cache.rs, taffy_bridge.rs): `NodeCache::final_layout_current`. Every
computation of a node in the bridge clears it before it runs, a final computation sets it when
done, and `TaffyBridge::cache_get` serves a `PerformLayout` entry only while it is set; otherwise
the final layout runs again, re-lays the children and writes their sizes and offsets back. A
child is clobbered only inside a computation of its parent (and so on up to the layout root, whose
final always runs), so a final that may still be served had nothing under it rewritten. Measures
stay memoised; the finals under an untouched subtree still hit. General: no AzMeet-specific code.

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

(My first trace through taffy's keys missed it because I assumed a measure HIT leaves the
node alone; the bridge's tail writes `used_size` on hits too - that is the 93.89.)

Tools for the parent after the rebuild:
- (B) layout/tests/the_resize_fast_path_paints_what_a_relayout_paints.rs: two LayoutWindows, one
  history, one resizing like a desktop shell and one like the restyle relayout, AzMeet's lobby
  (REAL flat TextInputs + Buttons) and devices panel (5 columns, counters ticking, rebuilds
  between resizes); after every step the display lists must be the same items (geometry rounded
  to 0.01). RED on the parent's build (3/3), expected GREEN with 43c226479.
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
4. layout/src/solver3/taffy_bridge.rs (43c226479): `self.ctx.cache_map.entries.get(..)` from
   the `&self` `cache_get` (the CacheTree impl) through the `&mut LayoutContext` field; the
   closure passed to `compute_cached_layout` uses `inputs.run_mode` after handing `inputs` by
   value to the compute function (`LayoutInput` is `Copy`).
5. examples/azul-calendar: format strings `{DAY_PAINT}` / `{SECONDARY}` / `{EVENT_PAINT}` /
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
path) are genuine REDs (the smoke RED was seen: 21/27); 8e56c4400 is red on the parent's build
(3/3) and is the RED of 43c226479. The headless test's settle needs the parent's
`let _ = window.common.take_regeneration();` (on fix/input-bugs-2026-09-19; not duplicated here,
to keep the pick clean).

Watch: after 1023db90c every headless / AZ_E2E `resize` takes the fast path (op-resize-*.json,
bug-textinput-resize-select-visual.json, ...). A scenario that turns red there has found a
desktop resize bug that was hidden before.

## Left

- Items 1 / 2: build 43c226479, run the 3 differential tests (expect green) and the probe
  `--compare` on the rebuilt AzMeet; then the resize perf numbers (frame_perf resize) - a final
  after a measure computation now runs again where it used to be served.
- AzCalendar in both modes on the rebuilt binary (screenshots): not seen yet with the new CSS
  (the palette was checked through `mount`).

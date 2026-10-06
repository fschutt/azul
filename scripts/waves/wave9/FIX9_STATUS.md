# FIX9 - the wave-9 small-fix wave (started 2026-10-05, base b454da215)

Plan: scripts/waves/wave9/SMALL_FIXES.md (86 items in 6 packages, no shared files; + the suite failures).
Resume: SendMessage the agent id ("continue from your progress file"); in a NEW conversation start a new agent per task in
the EXISTING worktree (MONDAY_RESUME_2026_10_05.md "How to resume"). Worktrees: .claude/worktrees/agent-<id>.

| Task | Package | Agent id | Branch | Progress / report | State |
|---|---|---|---|---|---|
| FIX9-LAYOUT | PKG 1 ENGINE-LAYOUT-TEXT | aed040f92949d6540 | wt/fix9-layout | scripts/FIX9_LAYOUT_2026_10_05.md | DONE (12/12; merged; sub/super pins in text3_baseline_exact.rs / text3_regression_metrics.rs move on purpose; round 2: window.rs paint-only classification of font/text changes, paged layout + fast paths keep old tree, LAYOUTPERF8 bug C) |
| FIX9-PAINT | PKG 2 ENGINE-PAINT-FRAME-A11Y | a88eb9585f405aa19 | wt/fix9-paint | scripts/FIX9_PAINT_2026_10_05.md | DONE (14/14; merged; inline-flex suite failure diagnosed, not fixed - solver3 reuse of cached measurements, bisect recipe in report; round 2: text3/paged_layout *_in_viewport, compact border-radius %, inline zoom) |
| FIX9-INPUT | PKG 3 INPUT-IO + DLL + TOOLING | a99cddf42c4359914 | wt/fix9-input | scripts/FIX9_INPUT_2026_10_05.md | DONE (15/16 + selection test + gene2e; merged; 3.8 e2e paste op needs callbacks.rs + event.rs (round 2); verify 3.1 Cmd key-up on the Mac) |
| FIX9-WIDGETS | PKG 4 WIDGETS | a82bcf5fe8377717a | wt/fix9-widgets | scripts/FIX9_WIDGETS_2026_10_05.md | DONE (17/18; 4.17 dialog-button disabled model needs a user decision; api: ReferencePickerEventKind::Clear; the 4 widget suite failures fixed) |
| FIX9-APPSA | PKG 5 APPS-A | acbf919509a8261aa | wt/fix9-appsa | scripts/FIX9_APPSA_2026_10_05.md | DONE (12 full + 5.13/5.14 partial; merged; AzContacts/AzReview toolbars and AzMail To/Cc / AzCalendar attendees TokenInput left) |
| FIX9-APPSB | PKG 6 APPS-B | a8beb815bbe185f30 | wt/fix9-appsb | scripts/FIX9_APPSB_2026_10_05.md | DONE (9/12; skipped AzReader IconGrid + links (files outside), ERP filter bar; integration: Toolbar items need their id as DOM id (R-1, toolbar.rs tool()), azerp_e2e.py:83 selector + "2,400.00" (R-2)) |

Integration: merge_one.sh per branch, then the PARENT list in SMALL_FIXES.md (api.json from the reports, register new
test files in all.rs, the 9 external path fixes, the AudioSink doc refresh), codegen, dylib + 35 apps, suites.

2026-10-05 evening: ALL 6 MERGED (clean), api.json converged (887495ddd), dylib + 35 apps build, css goldens blessed,
pushed fbdb57b4d. Integration extras: Toolbar items carry their id as DOM id (RED 47fadd43c / GREEN 6d5dae76f),
azerp_e2e amount step, wasm scheduled_notifications stub, RawImage::rgba_to_nv12 C wrapper. Suites running.

## Round 2 (2026-10-05 evening; plan + user decisions D1-D4: scripts/waves/wave9/ROUND2.md, base 440991077)
| Task | Agent id | Branch | Report | State |
|---|---|---|---|---|
| R2-INPUT (PKG R2-INPUT-IO-TOOLING + D2) | a794e0551d23a8ba8 | wt/r2-input | scripts/R2_INPUT_2026_10_05.md | running |
| R2-WIDGETS (PKG R2-WIDGETS + D1 + D3) | add7791a3c9f3a36e | wt/r2-widgets | scripts/R2_WIDGETS_2026_10_05.md | running |
| R2-APPS (PKG R2-APPS) | a1a907b790f2a870c | wt/r2-apps | scripts/R2_APPS_2026_10_05.md | running |
| R2-ENGINE | the coordinator | - | - | with the slider regression (FIX9 1.6 confirmed by revert) and the RED tests that did not turn green |

## Round 3 (2026-10-05 night): the 42 failing layout tests (scripts/waves/wave9/ROUND3_FAILURES.md, base 6a39b7f1a)
Round 2 merged + pushed (3db9fce91). Suites on it: core / css / css_codegen / dll lib / dll tests / dylib / apps build
GREEN; layout lib 4 + layout all 38 failing (-> round 3); doc 8 failing (bug_classes - the other session's codegen WIP in
this checkout, not ours); apps lib: AzMonitor + AzNotes test helpers fixed (bc0b00177, AzNotes), rerun pending.
| Task | Agent id | Branch | Report | State |
|---|---|---|---|---|
| R3-PAINT (MERGED) | a2b50af7b06081acb | wt/r3-paint | scripts/R3_PAINT_2026_10_05.md | running |
| R3-TEXT (MERGED) | aa29d62945e549b29 | wt/r3-text | scripts/R3_TEXT_2026_10_05.md | running |
| R3-WIDGETS (MERGED) | aab70e6966f347921 | wt/r3-widgets | scripts/R3_WIDGETS_2026_10_05.md | running |
| R3-FRAME (MERGED) | ae324df22d3899ae3 | wt/r3-frame | scripts/R3_FRAME_2026_10_05.md | running |
| R3-E2E MERGED (Modal button does not rebuild the main window; AzNews / AzCode infinite-height box) | a280d19d436d15d59 | wt/r3-e2e | scripts/R3_E2E_2026_10_05.md | running |
| R3-APPS MERGED (10 failing app lib tests) | a1902e84164760a94 | wt/r3-apps | scripts/R3_APPS_2026_10_05.md | running |

## pdfocr engine issues (2026-10-05 night; /Users/fschutt/Development/pdfocr/results/engine-issues/README.md + repro.zip)
| Issue | Owner | Branch / worktree | Report |
|---|---|---|---|
| 1 sup / sub / vertical-align shrink but do not move (run takes vertical-align from the text node, not its inline ancestors) | R3-TEXT (aa29d62945e549b29, told) | wt/r3-text | scripts/R3_TEXT_2026_10_05.md "pdfocr issue 1" |
| 2 position: relative on an inline span; 3 hyphens: auto ignores lang | PDFOCR-AZUL aa0807006e05d6a08 | wt/pdfocr-azul | scripts/PDFOCR_AZUL_2026_10_05.md |
| 4 printpdf from_html_with_cache decodes every image for every page | PRINTPDF-IMAGES acbd8c219bb2cf6ce | ../printpdf-lazy-images, branch fix/html-images-decoded-per-page (from origin/azul-codegen-api 84dce8c) | PRINTPDF_IMAGES_REPORT.md there |

2026-10-05 late night: ALL round-3 branches + PDFOCR-AZUL merged (2961792fa; one hand-resolved conflict in
css/src/props/property.rs - both typed-`none` additions kept). printpdf fix on ../printpdf-lazy-images branch
fix/html-images-decoded-per-page (NOT pushed): its 4 new tests + 55 lib tests pass against the local azul.
Still open from round 3: an_inline_date_picker_fits_its_pane (flex min-content of a fixed-width item - needs a
suite run), a_short_list_in_a_shell_pane_fills_its_pane_from_the_top (the UA `html { height: 100% }` rule is
commented out in core/src/ua_css.rs; Chrome has no such rule -> give the shells' roots + the test height: 100%,
per the "Chrome is the reference" ruling), R3-TEXT's leftovers (vh fast path on a height-only resize, an <img> in
a shifted span), R3-PAINT's (the styled-DOM loader draws no inline SVG, a stroke without stroke-width draws nothing).

## Verification round (2026-10-06): E2E + WPT triage (base 05ef3a8f4)
Suites on 889dccf30: everything green except one dll test (the Modal close - fixed 30fb9f06b). Mail corpus: 20
mails, 0 mismatched boxes (target/refci/mail-wave9). WPT gate: 19 regressions, 44 unexpected passes. E2E: the
shared click now settles animations first (05ef3a8f4: AzCalculator's Scientific keys slide in, clicks missed).
| Task | Agent id | Branch | Report |
|---|---|---|---|
| E2E-A (Calculator, Calendar, Clock, Code, Contacts, Dashboard, Drive, ERP, Keys, Mail) | a90e1e2428f99a3a2 | wt/e2e-a | scripts/E2E_A_2026_10_06.md |
| E2E-B (Maps, Meet, Monitor, Music, News, Notes, Paint, Pdf, Photo, Player) | a153b86fed8549d05 | wt/e2e-b | scripts/E2E_B_2026_10_06.md |
| E2E-C (Reader, Review, Setup, Sheets, Show, Tasks, Term, VideoCut, Writer) | a668f2e176ca88325 | wt/e2e-c | scripts/E2E_C_2026_10_06.md |
| WPT-REG (19 regressions + 44 unexpected passes) | ad8fe53ce300e017b | wt/wpt-reg | scripts/WPT_REG_2026_10_06.md |
WAYLAND8 Linux checks: not possible on this Mac (no running Linux VM; a live KDE session needed) - open.

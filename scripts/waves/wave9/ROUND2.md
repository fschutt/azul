# FIX9 round 2 - the follow-ups of the six FIX9 reports (planned 2026-10-05, HEAD b005ce479)

Sources: scripts/FIX9_{LAYOUT,PAINT,INPUT,WIDGETS,APPSA,APPSB}_2026_10_05.md (round-2 / outside-my-files /
skipped / least-sure sections) and scripts/waves/wave9/SMALL_FIXES.md "Round 2" + "PARENT". Every item below was
re-checked against HEAD by grep/read: all are still open. Already done at integration and left out: Toolbar item id
as DOM id, the azerp_e2e amount step, the wasm `scheduled_notifications` stub, `RawImage::rgba_to_nv12`, the
api.json drift (final_url, ReferencePickerEventKind::Clear, unified externals, AudioSink docs), the zoom test
registration.

House rules for every package: RED first (a test named as a sentence), no cargo by agents, WIP branch per package,
never force-push. The other session's files must never be staged: layout/src/solver3/page_breaks.rs,
layout/tests/all.rs, layout/tests/a_padded_table_cell_stays_in_its_row.rs, run_autofix.sh. Because of all.rs, every
new test goes inline (`#[cfg(test)]` in the touched file), so no package registers a new layout/tests/*.rs.

---------------------------------------------------------------------------------------------------------------

## 0. DO FIRST (coordinator, 2 lines, not in any report): the layout lib tests do not compile

- Found in azul-work/logs/suite_layout_lib.log (the 18:03 run on HEAD): `layout_lib exit=101`, E0599
  `Dom::create_text` does not exist. This is layout/src/window.rs:28284 and :28310, in PAINT 2.3's two RED tests
  (`text_sized_in_viewport_units_is_drawn_by_its_own_font`, `..._keeps_its_own_font_after_a_resize`).
- Fix: `Dom::create_text("Hi")` -> `Dom::create_text_do_not_use_without_block_level_wrapper("Hi")`. That is the
  name the other 30 window.rs tests use.
- Until this lands, the whole `cargo test -p azul-layout --lib` suite is unmeasured, and so is the PAINT suite
  failure below (N.3). Fix it before the packages branch, because window.rs is also in PKG R2-ENGINE.

---------------------------------------------------------------------------------------------------------------

## Tests that will fail ON PURPOSE (coordinator updates them; they are not code bugs)

FIX9-LAYOUT 1.8 (sub / super = LayoutNG `font_size/5+1` down, `font_size/3+1` up, and the line grows with the
shift). These tests still pin the old 0.3 / 0.4 x line-ascent rule (checked at HEAD):
- layout/tests/text3_baseline_exact.rs:222-230 `vertical_align_super_raises_glyph`: 6.4 -> 6.3333 (16/3+1).
- layout/tests/text3_baseline_exact.rs:234-241 `vertical_align_sub_lowers_glyph`: 4.8 -> 4.2 (16/5+1).
- layout/tests/text3_regression_metrics.rs:277-289 `vertical_align_super_raises_cluster`: y -6.4 -> 0.0. The line
  box grows by the shift, so the raised box's top is the line top.
- layout/tests/text3_regression_metrics.rs:293-305 `vertical_align_sub_lowers_cluster`: y 4.8 -> 1.0. The line
  ascent stays the strut's 12.8, so baseline + 4.2 - 16.
- Also fix the comments next to the old rule ("line_ascent * 0.4 / 0.3"). Both files are already registered. Their
  only link to all.rs is that registration, so editing them is safe. Confirm the four new numbers with one run before
  committing (they come from the report's arithmetic, not from a run).
- P.6: re-measure the mail corpus afterwards (every `<sup>` / `<sub>` moves).

Possible on-purpose movers in round 2 (none found by grep, so watch the suite):
- R2-ENGINE E1: a test that pins a font-only restyle as "no relayout".
- E3: pixel goldens with an empty padded inline that has a background.
- E5: a golden with a `%` / `em` border radius, which was painted square until now.

---------------------------------------------------------------------------------------------------------------

## SMALL - 4 packages, disjoint files (4 agents, inside the "3-4 agents max" rule)

### PKG R2-ENGINE (7 items, about 450 lines including tests)
Files: layout/src/window.rs, layout/src/solver3/fc.rs (tests only), layout/src/solver3/sizing.rs,
layout/src/solver3/display_list.rs, layout/src/solver3/getters.rs, core/src/compact.rs, layout/src/text3/cache.rs,
layout/src/solver3/paged_layout.rs, layout/src/image.rs. (Optional: the 2 test files of the "fail on purpose"
list.) Run `git status` before editing paged_layout.rs: it neighbours the other session's page_breaks.rs.

**E1 A stylesheet / theme change of only font or text properties never re-lays out.**
Source: FIX9-LAYOUT R2.1 (found while writing 1.2).
- Files: layout/src/window.rs:14088-14091 (`begin_reconciliation`'s CSS diff: `worst.max(ty.relayout_scope(false))`)
  and :29776 (`CssTransition::declared`: `scope: prop_type.relayout_scope(false)`).
- Root cause: css/src/props/property.rs:1904-1921 answers `RelayoutScope::None` for every font/text property
  (FontSize, LineHeight, LetterSpacing, TextIndent, WhiteSpace, ...) when `node_is_ifc_member` is false. So the
  restyle is paint-only, and cache.rs:1431 keeps the node out of `css_relayout` (it is not rebuilt fresh, and its
  old style is kept). The comment there says that `false` is the conservative reading. It is the opposite.
- The same flag on a transition makes a font-size tween "patchable" / paint-only (window.rs:15300, :15349).
- Change: pass `true` at both sites (IfcOnly) and fix the comment.
- RED, in fc.rs `window_layout_tests` (it has the `restyled(..)` helper):
  `a_stylesheet_only_font_size_change_alone_relays_out_its_paragraph`. This is 1.2's test without the padding
  crutch: the text's line height / width follows the new size.

**E2 The marker of an absolutely positioned list item counts as out of flow.**
Source: FIX9-LAYOUT R2.5.
- File: layout/src/solver3/sizing.rs:356-363.
- Root cause: the marker node carries its li's DOM id. `get_position_type(.., dom_node_id)` therefore reads the
  LI's `position: absolute`, and the marker returns 0 to its li (whose intrinsic size is `max(content, marker)`
  since 1.4).
- Change: `!fc::is_marker_box(tree, node_index) && matches!(get_position_type(..), Absolute | Fixed) ||
  is_marker_on_a_line(..)`.
- RED, in sizing.rs `anonymous_ifc_intrinsic_tests`:
  `an_absolutely_positioned_list_item_without_a_line_is_as_tall_as_its_marker` (an empty
  `<li style="position:absolute">`; the marker needs a font: `FcFontCache::build()` as in 1.1's min-content test).

**E3 An empty padded inline paints its background only over its strut.**
Source: FIX9-LAYOUT R2.3 (a consequence of 1.7, GREEN 258df98cd).
- Files: layout/src/solver3/display_list.rs:8990-9060 `paint_inline_shape`. The shape is built in fc.rs:11589-11650
  (read only).
- Root cause: since 1.7 the empty inline's `InlineShape` is only its strut, which is line-height tall around the
  baseline. `paint_inline_shape` paints that box, minus vertical margins, from `box_props`. Chrome paints the
  content area plus vertical padding and borders (`<span style="padding:4px;background:red"></span>` is
  16 + 8 tall).
- Change: when the shape's node is `display: inline` (not inline-block / atomic), take the strut box as the content
  area, grow it by padding-top + border-top above and padding-bottom + border-bottom below, and ignore vertical
  margins. For `line-height: normal` this matches A+D. With an explicit line-height it is off by the half-leading,
  which is acceptable.
- RED, inline in display_list.rs: `an_empty_inline_with_padding_paints_its_background_over_its_padding` (the
  background item's rect height = strut + 8, top = strut top - 4).

**E4 DEDUP: the SVG clip-mask rasteriser builds its agg path inline.**
Source: FIX9-PAINT round 2 ("svg_mask_memo vs pixmap helpers").
- File: layout/src/solver3/display_list.rs:12224-12261 (in `rasterize_svg_clip_to_r8`, cfg cpurender).
- Change: replace the 38-line ring loop with `crate::cpurender::pixmap::svg_path_to_agg(svg_clip, &mx, &my)`.
  Keep this function's own `mx`/`my`: the no-viewBox case maps window coordinates, which `svg_user_space_mapping`
  does not do. The stroke twin (:12009-12012) already does exactly this.
- No RED (same output). Verify with the svg clip tests, `svg_mask_memo_tests`, a_chart_paints_its_series_through_the_svg_path.

**E5 A `%` / `em` border radius reads as 0 in the compact fast path (`border-radius: 50%` paints a square).**
Source: FIX9-PAINT round 2 ("Compact cache").
- Files: core/src/compact.rs:1855-1885 (the four radius arms encode only `SizeMetric::Px`; anything else leaves
  I16_SENTINEL). layout/src/solver3/getters.rs:2161-2190 `get_style_border_radius` and :2253-2280
  `get_border_radius`: both fast paths decode `raw >= I16_SENTINEL_THRESHOLD` as 0 px.
- Change: encode a non-px radius (and `inherit` / `initial`) as a distinct marker (I16_AUTO is unused for radii;
  do not reuse I16_INHERIT, which the inheritance builder may act on). In both getters, if any corner holds the
  marker, fall through to the slow cascade path, which resolves `%` against `element_size` and `em` against the
  font.
- RED, in getters.rs: `a_fifty_percent_border_radius_rounds_a_square_box_into_a_circle` (100x100 -> 50 on every
  corner), and `an_em_border_radius_follows_the_font_size`.

**E6 Paged layout and the text3 pre-resolver collect font chains against a zero viewport.**
Source: FIX9-PAINT R.1 = SMALL_FIXES Round 2 R.1.
- Files: layout/src/solver3/paged_layout.rs:340 and :2624 (`collect_and_resolve_font_chains_with_registration` ->
  `_in_viewport`, with the page size, `viewport.size`, as the viewport). The skip signature at :2603-2619 / the one
  before :340 folds only `prev_font_hashes`: fold in the viewport when any node's font size is vw/vh/vmin/vmax.
  Share the check window.rs gained in 307df77e7 as one getters helper, e.g.
  `has_viewport_sized_font(styled_dom)`, used by window.rs and by both paged sites.
- layout/src/text3/cache.rs:1099-1120: add `pre_resolve_chains_for_dom_in_viewport(.., viewport)`. Keep the old
  name as the zero-viewport wrapper: its only callers are the doc/src/reftest warmups, which do not need a
  viewport.
- RED, in paged_layout.rs tests: `paged_text_sized_in_viewport_units_is_collected_at_its_used_size` (5vw on an
  800px page -> a chain with optical size 40, as PAINT 2.3's window test does).

**E7 `encode_png` (and the other encoders) write premultiplied pixels as straight alpha.**
Source: FIX9-PAINT 2.5 / round 2.
- File: layout/src/image.rs:655-700 (`encode_func!`: bmp / tga / tiff / gif / pnm), :713-755 `encode_png`, :765
  `encode_jpeg`. None of them reads `image.premultiplied_alpha`. `svg_render` now (correctly) returns
  `premultiplied_alpha: true`, so a translucent SVG export comes out dark.
- Change: one helper, run before `bgr_to_rgb_swap` in all three paths: un-premultiply RGBA8 / BGRA8 when the flag
  is set (`c * 255 / a`, with a = 0 giving 0).
- RED, in image.rs tests: `encode_png_writes_a_premultiplied_pixel_with_straight_alpha` ((64,0,0,128) premultiplied
  -> decoded (128,0,0,128)).

### PKG R2-INPUT-IO-TOOLING (5 items, about 350 lines)
Files: layout/src/callbacks.rs, layout/src/e2e/full.rs, layout/src/e2e/runner.rs, doc/src/gene2e.rs,
dll/src/desktop/shell2/common/event.rs, layout/src/request.rs, layout/src/http.rs (tests),
layout/src/desktop/dialogs.rs (tests), core/src/xml.rs, layout/src/telemetry/crash_mail.rs,
doc/src/codegen/v2/{mod.rs, bug_classes.rs, conformance/mod.rs, lang_haskell/mod.rs, lang_kotlin/managed.rs,
lang_java/managed.rs, lang_csharp/managed.rs, lang_node/managed.rs}, doc/src/autotest/mod.rs.

**I1 The e2e protocol has no paste op (borderline SMALL, about 180 lines; the design and the RED exist).**
Source: FIX9-INPUT 3.8 / Round 2 item 1.
- RED already committed, ignored: layout/src/e2e/runner.rs:7209-7212
  `a_scenarios_paste_op_pastes_bold_html_into_the_focused_editor`. Remove the `#[ignore]`.
- callbacks.rs: `CallbackChange::Paste { content: ClipboardContent }`, plus a test-facing pusher. CallbackChange is
  not in api.json (checked).
- full.rs: `DebugEvent::Paste { text: String, #[serde(default)] html: Option<String> }`. Its arm builds a
  `ClipboardContent { plain_text, html, styled_runs: empty }` and pushes the change.
- gene2e.rs: an OP_POLICY row for "paste" (gen_e2e_cases.py reads it; `every_real_op_is_classified` enforces it).
- Both hosts' `apply_user_change` (runner.rs:2046, dll event.rs:5808) get the same arm:
  1. `clipboard_manager.set_paste_content(content)` (managers/clipboard.rs:59);
  2. dispatch `EventType::Paste` at the focus. The model is the dll's deferred clipboard block around event.rs:8396
     (`SystemChange::PasteFromClipboard`);
  3. unless a callback calls `prevent_default`, run `LayoutWindow::paste_clipboard_content(&content)`
     (window.rs:24454);
  4. clear the paste content and map `PasteOutcome` as the dll arm does.

**I2 `set_copy_content`'s doc says it applies only "if preventDefault() was not called"; the shell applies it
regardless.**
Source: FIX9-WIDGETS round 2 item 6 (found at 4.9).
- Files: layout/src/callbacks.rs:7045-7050 (the doc). The behaviour is in dll event.rs:7309-7320. Rewrite the doc to
  match: the content is written to the clipboard after the callback returns, whatever `prevent_default` says.
- PARENT afterwards: api.json:14629-14635 carries the stale doc too, and autofix never refreshes an existing doc
  (`autofix remove` + `add` of CallbackInfo.set_copy_content, or the P.1 doc-drift route). No RED (a doc).

**I3 Tests drain each other's completions from the process-wide request queue.**
Source: FIX9-INPUT 3.4 risk / Round 2 item 7.
- Files: layout/src/request.rs:95-117 (`queue::with_queue`, one static Mutex) and :176 `take_completed()`. The
  drainers are the http.rs tests (:2244, :2475), the e2e runner's pump (runner.rs:807, which runs inside many
  parallel runner tests) and request.rs's own tests (:940-966).
- `take_completed_for(id)` alone does not fix it: a parallel runner test pumps everything and swallows an http
  test's entry. Two options:
  - (a) Recommended, about 15 lines: under `cfg(test)`, make the queue thread-local. Every completion is issued on
    the calling thread (`complete`), or polled on the pumping thread (`defer`), so each test sees only its own
    requests. The dll and product builds keep the process-wide queue.
  - (b) A `cfg(test)` lock taken by every test that issues or pumps requests, which means touching many runner
    tests.
- RED, in request.rs: `a_tests_completion_is_not_drained_by_a_test_running_beside_it` (thread A completes, thread
  B drains, A still finds its entry).

**I4 `MimeTypeHint::from_extension` has no json / txt.**
Source: FIX9-INPUT Round 2 item 4.
- Files: core/src/xml.rs:233-268 (add `"json" => "application/json"`, `"txt" => "text/plain"`) and
  layout/src/telemetry/crash_mail.rs:306-319 (`attachment_type` becomes the table alone).
- RED, in core xml tests: `a_json_or_txt_extension_has_its_own_media_type`. xml_test.rs:813-837 pins only "", an
  emoji and a long input, so it is unaffected.

**I5 DEDUP: 7 copies of `lower_first`, and autotest's third Rust-file walker.**
Source: FIX9-INPUT Round 2 item 5. The report named 2 copies; there are 7.
- Add one `pub fn lower_first` next to `upper_first` (doc/src/codegen/v2/mod.rs:133). The copies:
  bug_classes.rs:92, conformance/mod.rs:387, lang_haskell/mod.rs:1195, lang_kotlin/managed.rs:958 (pub(super),
  also used by lang_kotlin/wrappers.rs), lang_java/managed.rs:393, lang_csharp/managed.rs:800,
  lang_node/managed.rs:753.
- ASCII vs Unicode lowercase is identical for the ASCII names fed to them. Leave haskell's `lower_first_word`
  alone (different semantics).
- doc/src/autotest/mod.rs:238-279 `collect_rust_files` + its `should_exclude_path` "mirror" ->
  `autofix::type_index::rust_files_under(dir, &|p| module_map::should_exclude_path(p) || p has "/codegen/" or
  "/generated/", out)`. Keep the crate-name pairing at the call site.
- No RED (goldens + `cargo test -p azul-doc` cover it).

### PKG R2-WIDGETS (6 items, about 220 lines)
Files: layout/src/widgets/{date_picker.rs, date_range_picker.rs, gauge.rs, terminal_view.rs, icon_grid.rs,
cell_grid.rs, data_table.rs}, layout/src/dom_lint.rs.

**W1 The DatePicker's today cell and lit range hide the focus halo / hover face.**
Source: FIX9-WIDGETS round 2 item 1 (the same bug as the date_range_picker suite failure, a7dc9e502).
- Files: date_picker.rs:2096-2107 `ringed` / `washed` (they append the resting mark after the face's `:focus` /
  `:hover` declarations); call sites :2137, :2141, :2430, :2436. date_range_picker.rs:711-733 `marked` (the fix)
  and :691/:695.
- Change: move `marked` into date_picker.rs as `pub(crate)`. `ringed(face, faces) = marked(face, &faces.today)`,
  `washed(..) = marked(face, &faces.in_range)`. date_range_picker::day_face imports it.
- RED, in date_picker.rs: `a_focused_today_in_the_date_picker_shows_its_focus_halo_over_its_ring` (in the today
  cell's props, the last `-azul-box-shadow-bottom` matching Focus comes after the resting ring; light and dark).

**W2 dom_lint reports text beside an absolutely positioned sibling.**
Source: FIX9-WIDGETS suite-failure note + round 2 item 2.
- File: layout/src/dom_lint.rs, :309-325 (the `has_block_child` sibling loop calls only `is_block_level(display_of(..))`).
- Change: skip a sibling whose position is absolute / fixed (it splits no line). Keep floats counted: azul does
  not place floats inside an IFC yet (NOT SMALL list).
- RED, in dom_lint tests: `text_next_to_an_absolutely_positioned_sibling_is_not_reported`.

**W3 DEDUP: gauge's `class_list` and date_range_picker's `one_class` duplicate `decl::classes`.**
Source: FIX9-WIDGETS 4.14 / round 2 item 4.
- gauge.rs:637-644 (6 uses: :669, :679, :844, :908, :922, :932) -> `themes::decl::classes` (decl.rs:991).
  date_range_picker.rs:740 `one_class(n)` -> `decl::classes(&[n])`. No RED.

**W4 DEDUP: one VirtualKeyCode -> character helper.**
Source: FIX9-WIDGETS 4.4 / round 2 item 5.
- terminal_view.rs:580-610 `us_char(key, shift) -> Option<u8>` becomes `pub(crate)`. icon_grid.rs:965-974
  `typed_letter` becomes `us_char(key, false).filter(u8::is_ascii_alphanumeric).map(char::from)`. No RED (the
  existing type-ahead and terminal tests).

**W5 DEDUP: cell_grid's `typed()` re-implements data_table's `insert_at`.**
Source: FIX9-WIDGETS 4.10.
- data_table.rs:2886-2895 `insert_at` becomes `pub(crate)`. cell_grid.rs:3172-3184 calls it. No RED.

**W6 IconGrid items carry no DOM id.**
Source: FIX9-APPSB R-3 / 6.4. It blocks AzReader's adoption and azreader_e2e's `#__azreader_book-0`. It is the
same gap the Toolbar had (R-1).
- File: icon_grid.rs, about :1296-1320 (the item `Dom::create_div()`).
- Change: when `grid.id` is not empty, `.with_id(format!("{grid.id}-{index}"))`. No api change.
- RED, in icon_grid.rs tests: `an_icon_grid_item_carries_its_grids_id_and_its_index_as_its_dom_id`.

### PKG R2-APPS (9 items, about 300 lines, mostly deletions + E2E)
Files: examples/azul-news/src/ui.rs, examples/azul-keys/{Cargo.toml, src/ui.rs, src/ui_item.rs, src/ids.rs},
examples/azul-reader/{Cargo.toml, src/jobs.rs}, examples/azul-appkit/src/files.rs,
examples/azul-calendar/src/chrome.rs, examples/azul-contacts/src/ui.rs, examples/azul-erp/src/ui/form.rs,
examples/azul-code/src/lib.rs, scripts/{aznews,azcode,azerp}_e2e.py. (Cargo.lock: drop "csv" from AzKeys' list,
or let the integration build rewrite it.)

- **A1 AzNews DOM helpers -> `azul_appkit::pieces`.** Source: SMALL_FIXES R.2 / FIX9-APPSA. File:
  azul-news/src/ui.rs:512-551. `strs`, `text`, `block`, `column`, `button`, `primary` are verbatim twins; its
  `row` is `pieces::flex_row`. `input` (:553) stays.
- **A2 AzKeys leftovers.** Source: FIX9-APPSA + FIX9-APPSB R-5.
  - Cargo.toml:51: drop `csv = "1.4"` (nothing calls csv:: directly any more).
  - ui.rs:48: drop the `flex_row as row` alias. ui_item.rs:33-36 imports `flex_row` instead, and its 10 `row(`
    calls follow.
  - ids.rs:162: drop the unused `edit_tag_chip`.
- **A3 AzReader's own TempDir.** Source: FIX9-APPSA (5.6 left). Cargo.toml gets a new
  `[dev-dependencies] azul-storage = { path = "../azul-storage", features = ["testing"] }`. jobs.rs:357-375's copy
  -> `azul_storage::testing::TempDir`.
- **A4 appkit `open_external` opens files / folders through `Url::open_path`.** Source: FIX9-APPSA 5.7 + FIX9-INPUT
  Round 2 item 3.
  - azul-appkit/src/files.rs:191-230: with feature `azul`, a non-web target -> `azul::url::Url::open_path(..)`
    (api.json has it since 887495ddd). The `cmd /C start` fallback breaks on `&`.
  - Keep the system command only without the feature. Update the doc comment.
- **A5 AzCalendar chrome.rs pieces.** Source: FIX9-APPSA. chrome.rs:442-465: `line` ->
  `flex_row("margin-top: 8px;", ..)`; `button` / `primary` -> `pieces::button(..)` / `pieces::primary(..)` +
  `.with_css("margin-right: 8px;")`. This is the same `.with_css` on the button DOM it does today.
- **A6 AzContacts adopts Toolbar (5.13, unblocked by the Toolbar-id fix).** Source: FIX9-APPSA 5.13.
  - ui.rs:1248-1264: five `ToolbarItem::create_button(ids::TOOLBAR_*, label, icon)` with `with_show_label(true)`,
    a spacer before Settings, and one `on_toolbar` that matches `event.id` and calls the existing `on_new` /
    `on_import_open` / ... (the AzPdf ui.rs:197-260 pattern).
  - The E2E keeps working: the items' ids are the same `__azcontacts_toolbar-*` DOM ids (azcontacts_e2e.py:134,
    174, 225).
- **A7 AzERP `on_reference` has no Clear arm.** Source: FIX9-APPSB R-6. ui/form.rs:386-418 (`on_reference`): add
  `ReferencePickerEventKind::Clear => with_erp(.., |s, _| { s.reference_query = None; s.state.set_value(&name, "");
  })`. Today it falls into `_ => DoNothing`, so the x leaves the record set.
- **A8 AzCode's F1 list lacks Mod+O.** Source: FIX9-APPSB R-4. lib.rs:79: `[Shortcut; 14]` -> 15, add
  `Shortcut::new("File", "Mod+O", "Open a folder")`. An `OPEN_FOLDER` id is not needed: the E2E can click
  `#__azcode_welcome .__azul-native-empty-state-action` or press Mod+O.
- **A9 E2E steps for the new behaviour.** Source: FIX9-APPSB R-7. None exists today (grep).
  - aznews_e2e.py: rename a feed -> close the window -> wait for `AZNEWS_SAVED news/subscriptions` -> relaunch on the
    same data dir -> the new title is shown.
  - azcode_e2e.py: `{"op":"mock","set":{"file_open":{"path":<tmp dir>}}}`, then Mod+O -> `AZCODE_FOLDER <dir>`, and
    the explorer lists it.
  - azerp_e2e.py: click Delete -> `AZERP_ASK_DELETE <id>` -> the question's Delete -> the record is gone; Cancel
    keeps it.
  - Run them one at a time through scripts/waves/tools/run_capped.sh.

---------------------------------------------------------------------------------------------------------------

## NEEDS A USER DECISION

- **D1 FIX9-WIDGETS 4.17: one disabled model for dialog buttons.** dialog_kit.rs:264 `row_button` fakes disabled:
  manual Unavailable + the `held` skin, no click, no Tab stop. `Button::with_disabled(reason)` (button.rs:552) means
  "has a reason": `""` enables, and a disabled Button keeps its Tab stop and answers click / hover with its reason.
  wizard_layout.rs:1204 pins "a held Next is inert". Options:
  - (a) Button gets a reasonless disabled state + "no Tab stop when disabled". This is a new repr(C) field, so
    api.json changes. Then row_button uses it, and the wizard test stays.
  - (b) Dialog buttons always carry a reason (invent texts for standard_dialogs / settings Apply). They keep their
    Tab stop and answer with the reason. The wizard test changes to "a click shows the reason".
  - (c) Keep row_button's own fake (status quo) and document why.
  - With (a) or (b): the 50% `held` skin over Button's 40% dimming gives 20%, so drop one of the two.
- **D2 FIX9-INPUT 3.2: the debug server's `focus_node` strictness.** layout/src/e2e/full.rs:14718-14757 refuses an
  unfocusable node ("cannot hold focus"). The engine's `FocusTarget::Id` now delegates to the first focusable
  descendant. Options:
  - (a) Keep strict. Scenarios must name the focusable node, as APPSB R-2 did with
    `.__azul-native-text-input-container`.
  - (b) Delegate the same way and answer with the node that actually took the focus (`FocusNodeResponse.node_id`).
    Error only when the subtree has no focusable node.
  - Recommended: (b), so the engine and the debug op agree. If (b): it sits in full.rs, which PKG R2-INPUT touches
    anyway.
- **D3 FIX9-APPSB 6.4: AzReader's library as an IconGrid.** It needs W6 first, plus `AppState.library_view:
  IconGridView` in app.rs and the grid in ui_library.rs (plan in the report). The visible cost: each tile loses its
  author / progress lines and the coloured no-cover tile, which become a label and a glyph. Adopt, or keep the
  custom covers grid?
- **D4 SMALL_FIXES P.4: `ProgressBar.with_container_style`.** It is a misnamed duplicate of
  `with_container_background`: the same fn_body, in api.json at widgets/ProgressBar. No app calls it (grep).
  Remove it with `autofix remove` (an API break for outside users), or keep it?

---------------------------------------------------------------------------------------------------------------

## NOT SMALL (still open; one line each, with the source)

- N.1 Paged layout keeps the diff-less reconcile. The resize-only / overrides-only skip paths of `layout_document`
  keep the retained tree, so a css-dirty BLOCK there has stale box props (an override-animated block margin does
  not move). Also LAYOUTPERF8 bug C: `a_parent_grows_with_its_restyled_child.json` stays xfail, and needs the
  relayout-boundary design. (FIX9-LAYOUT R2.4)
- N.2 sub / super use the block container's font size. Chrome uses the parent INLINE box's (`<sup>` in `<small>`),
  which needs the run style to carry the parent's size (text3 style + hashing). (FIX9-LAYOUT R2.6)
- N.3 Suite failure `window_theme_context::a_changed_inline_flex_box_behind_a_block_sibling_keeps_its_slot_and_widens`
  (76 vs 60). The root cause is not located: the suspects are LAYOUTPERF8's carried measurements (39f090082,
  b8a5c8cb1) and cea6b0840. After item 0, re-run it first: LAYOUT 1.6's fresh rebuild of restyled nodes may have
  moved it. Bisect recipe: FIX9_PAINT report, "SUITE FAILURE". (FIX9-PAINT)
- N.4 Inline boxes: `getters::get_inline_border_info` reads border widths raw and padding / margins against
  DEFAULT_FONT_SIZE, so they get no zoom and no em (text3 work). (FIX9-PAINT round 2)
- N.5 Toolbar overflow measured. This covers AzNotes passing `with_available_width` (the pane width) and AzPdf's
  bar (R-8): reading the window width in layout does not rerun on resize. (FIX9-APPSA, FIX9-APPSB R-8; already on
  the wave-9 NOT SMALL list)
- N.6 AzReview adopts Toolbar (5.13). The bar is mostly custom content (semantic swatches with dataset + MouseUp
  callbacks, the tool nib, the meter), so it is low value; leaving it is suggested. (FIX9-APPSA 5.13)
- N.7 AzMail To / Cc and AzCalendar attendees as TokenInput: a validator over azul-pim `mail_address`, commit /
  draft semantics, and azmail_e2e's `type_into('__azmail_compose_to', ..)`. (FIX9-APPSA 5.14)
- N.8 AzERP register filter bar: a predicate filter in views/rows.rs, the bar's state in `Erp`, and a test.
  (FIX9-APPSB 6.6e)
- N.9 AzReader internal links + exact anchors across content.rs / ui_reader.rs / commands.rs / app.rs / a
  post-layout hook (4-step plan in the FIX9_APPSB report). (FIX9-APPSB 6.10)
- N.10 AzMusic covers: the library keeps no covers (extract `AudioFileInfo::cover`, decode off the UI thread, cache
  per album). (FIX9-APPSB 6.4)
- N.11 Deferred, optional: core/src/gl.rs a pub `&self` accessor on `*VecRefMut` would shorten 3.13's generated
  bodies. It moves api.json fn_bodies, for little gain. (FIX9-INPUT Round 2 item 6)
- N.12 A doc-drift check in the autofix scan. It would also cover I2's api.json doc. (SMALL_FIXES P.1; on the
  NOT SMALL list)

---------------------------------------------------------------------------------------------------------------

## PARENT after round 2 (api.json through azul-doc autofix)
- I2: refresh CallbackInfo.set_copy_content's doc (remove + add, or by hand through the tool's route).
- D4 if "remove".
- D1 (a) only: the new Button field, then codegen + memtest sizes.
- No other round-2 item changes the API: E5/E7 are behaviour only, CallbackChange is not exported, and W6 adds no
  API.

## Verification owed (not code; from the reports)
- 3.1 Cmd+letter key-up on this Mac.
- 3.3 Shift/Ctrl+Insert on Linux / Windows.
- 3.5 `Url::open` with `&` and `open_path` on Windows.
- 4.6 MoneyInput caret / scroll live.
- 4.8 the x's position in flat / flora.
- 4.11 a double-click on a header edge leaves no drag.
- 2.14 VoiceOver / TalkBack on a device.
- 5.11 `azmail_e2e.py --phase submission`.
- The E2E scripts of 6.2 / 6.5 / 6.6 after the merge.
- The TextInput 11px change (4.1) in the E2E screenshots.
- P.6: the mail corpus + WPT re-measure after the sub/sup move.

## Seen while checking, not in any report (optional)
- examples/azul-calculator/src/num.rs:411 `group_thousands` (+ datecalc.rs:188) is a fourth thousands-grouping
  twin. `money_input::group_digits` is pub(crate), so the calculator would need `MoneyInput::format_amount` or a
  new export. Low priority.

---------------------------------------------------------------------------------------------------------------------

# USER DECISIONS (2026-10-05) - binding for round 2
- D1 dialog buttons: "Always a reason" + "show reason on HOVER". Every disabled dialog button (dialog_kit row_button,
  standard_dialogs, settings Apply, the wizard's held Next) carries a short reason text; a disabled Button keeps its
  Tab stop and shows its reason on hover (and on keyboard focus for keyboard users); no fake disabled skin - drop the
  doubled dimming (the `held` 50 % skin over Button's 40 %). wizard_layout.rs's "a held Next is inert" pin changes to
  "a held Next shows why on hover and does not advance". -> PKG R2-WIDGETS (+ files dialog_kit.rs,
  standard_dialogs.rs, wizard_layout.rs, wizard_pages.rs, button.rs).
- D2 focus_node: delegate like the engine (first focusable descendant; answer with the node that took focus; error only
  when the subtree has none). -> PKG R2-INPUT-IO-TOOLING (full.rs).
- D3 AzReader library: EXTEND IconGrid first, the additions OPTIONAL: an IconGridItem may carry extra text lines
  (e.g. author, progress) and a placeholder colour for an item without an image; items without them look exactly as
  today. -> PKG R2-WIDGETS (icon_grid.rs; list the api.json additions). AzReader adopts it in a later round.
- D4 ProgressBar.with_container_style: remove (parent, `autofix remove`).

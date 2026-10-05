# Wave 9: the remaining small fixes (triage 2026-10-05, base 83d5c2716)

Sources: the PR #476 ledger (pause notes, WAVE 7 A-I, FOLLOW-UPS, Engine backlog), the wave-9 / wave-8-follow-up
reports in scripts/*_2026_10_03.md, and today's integration follow-ups. Every item below was checked against the
code at 83d5c2716 (grep / read, no build).

Counts: SMALL open = 86 items in 6 packages, + 2 round-2 items + 7 parent (api.json / run) actions.
NOT SMALL open = 85 lines (some lines group related items of one report), + 6 lines of verification runs owed.
DONE = 58 evidence lines (many lines close several ledger items).
Package sizes: PKG1 12, PKG2 14, PKG3 16, PKG4 18, PKG5 14, PKG6 12. No two packages share a file.

## Rules for every package (in addition to scripts/waves/house_rules.md)
- RED first (the test is named as a sentence), then the fix. No cargo in agents. Root-cause in the engine.
- api.json: agents never edit it. Each package lists its api changes in its report, and the parent runs
  `azul-doc autofix add/remove` (see "PARENT" below).
- layout/tests/all.rs: agents do NOT edit it. Prefer inline `#[cfg(test)]` tests in the touched file. If a
  layout/tests/*.rs file is needed, name it in the report and the parent appends the `#[path]` line at integration.
- Off limits (another session, uncommitted): layout/src/solver3/page_breaks.rs, its line in layout/tests/all.rs,
  layout/tests/a_padded_table_cell_stays_in_its_row.rs, run_autofix.sh.
- Each package touches ONLY the files listed in its "Files" line. Anything else goes to the report as a round-2 note.

---------------------------------------------------------------------------------------------------------------------

# SMALL - open

## PKG 1 ENGINE-LAYOUT-TEXT (12 items)
Files: layout/src/solver3/{fc.rs, sizing.rs, taffy_bridge.rs, cache.rs, mod.rs}, layout/src/text3/{cache.rs,
knuth_plass.rs}, core/src/{xml.rs, xml_attributes.rs}

1.1 The intrinsic width of an anonymous block after a nested block still counts the text-indent.
    sizing.rs:1054 adds the indent for every IFC. Use the same "first in-flow child" gate that fc `layout_ifc` uses
    (`first_in_flow_child`). 64d3cb633 fixed only the layout half.
    RED an_anonymous_block_after_a_nested_block_adds_no_indent_to_its_max_content. Source: TEXT7 found (a).
1.2 The inline-content cache key may miss a font-size / line-height change that comes only from the stylesheet.
    The fingerprint at fc.rs:4230-4237 hashes `tier1_enums` plus `tier2b_text.font_family_hash` only. Hash the
    whole `CompactTextProps` (line-height, letter/word spacing, indent, colour) and the `tier2_dims` font-size.
    The symptom is unverified: write the RED first, and drop the item if it passes.
    RED a_stylesheet_only_font_size_change_relays_out_its_text. Source: TEXT7 found (c).
1.3 text-wrap: balance indents a continuation fragment.
    text3/cache.rs:11400 drains the cursor into `kp_layout` without saying whether the paragraph starts there.
    Compute `cursor.next_item_index == 0 && partial_remainder.is_empty()` (as at cache.rs:11520) and pass it to
    knuth_plass.rs:478/579, so the indent applies only when `line == 0 && starts_paragraph`.
    RED a_balanced_continuation_fragment_is_not_indented. Source: TEXT7 found (d).
1.4 A list item whose first child is a block is 20px too tall.
    fc.rs:12203 `marker_line_host` returns None for `<li><div style=height:50px>`, so the marker gets a line of its
    own. Lay out a marker that has no line box out of flow at the item's start, with no block-size contribution.
    RED a_list_item_without_a_line_box_is_as_tall_as_its_block. Source: LAYOUT7 left (ledger F).
1.5 A stretched flex item loses its min-height.
    taffy_bridge.rs:1501-1511 (`should_suppress_cross_intrinsic`) sets `min_size.width/height = 0` for every
    stretched auto item. Keep a declared CSS min-size; zero it only when min-size is auto.
    RED exists: layout/tests/a_stretched_flex_container_keeps_its_min_height.rs (f246c017c, registered, failing).
    Source: OFFICE7 found.
1.6 A block whose own margin changes does not move on relayout (LAYOUTPERF8 bug B).
    Step 1.15 (solver3/mod.rs:926-972) only marks the node dirty, and the cloned node keeps its stale box props.
    Pass the css-dirty set into `reconcile_and_invalidate` (solver3/cache.rs:1358) and rebuild those nodes fresh,
    as `DirtyFlag::Layout` nodes are; lift to the parent for scope Full. Flip
    scripts/layoutperf8_e2e/a_block_moves_with_its_own_margin.json to "pass".
    RED a_block_moves_with_its_own_margin_after_a_restyle. Source: LAYOUTPERF8 found, ANIMFRAME8 s8.
1.7 An empty inline with padding is 27.2px tall (Chrome: 18).
    fc.rs:11589-11632 pushes it as an `InlineShape` of line-height plus vertical padding/border, sitting on the
    baseline. Make it line-height tall, straddling the baseline like the strut (`baseline_offset`); vertical
    padding and borders do not count in the line box.
    RED an_empty_inline_with_padding_is_as_tall_as_its_strut. Source: RULINGS8 LEFT.
1.8 sub/super still use the old shift.
    `baseline_shift` returns None for Sub/Super (text3/cache.rs:11186); placement uses `line_ascent * ..RATIO`
    (:13014/13018). Put Sub/Super into `baseline_shift` as Chrome does: the parent font-size / 5 + 1 down,
    / 3 + 1 up. This needs a `strut_font_size` on `UnifiedConstraints` (text3/cache.rs:2345, built in fc.rs:5214
    and :5966). This moves every `<sup>` in the mail corpus: re-measure it afterwards.
    RED a_superscript_rises_a_third_of_its_parents_font_size_and_grows_the_line. Source: RULINGS8 LEFT.
1.9 Remove the wasm-lift diagnostic tree clone.
    solver3/mod.rs:1277 `cache.tree = Some((*new_tree).clone())`, plus the marks at :1278-1288 and :1340-1346.
    The real store is at :2021, and nothing in between reads `cache.tree`. Delete the clone; on the two `?` error
    returns in between, set the tree to None. Perf only, no RED. Source: ANIMFRAME8 s8 (LAYOUTPERF8 twins).
1.10 The XML tree loader drops the `<html>` element's attributes.
    core/src/xml.rs:6012-6018 (`str_to_dom_unstyled`) and :7014 (`render_dom_from_body_node_fast`) create a bare
    `Html` node. Build it from `get_html_node`'s attributes with the attribute conversion the other elements use.
    RED the_html_elements_style_attribute_reaches_the_root. Source: WPT8 found (c).
1.11 `Xml::scan_external_resources`: reverse order, and `url(` in prose becomes a resource (SCANORDER).
    core/src/xml.rs:378-399 uses a LIFO stack (children come out reversed), and the Text arm (:384-387) scans every
    text node as CSS, so a `<style>` is scanned twice (once more by the style arm ~:599-606). Push the children
    `.rev()` and drop the Text arm (scan only `<style>` text). AzMail's own de-duplication can stay.
    RED scan_external_resources_lists_resources_in_document_order_once_and_ignores_url_in_prose.
    Source: MAIL9 engine gaps.
1.12 A `data:...;base64,` URL in a style attribute is cut at its `;`.
    core/src/xml_attributes.rs:526 uses `style.split(';')`. Use the public
    `azul_css::props::basic::parse::split_top_level(style, |b| b == b';')`.
    RED a_data_url_in_a_style_attribute_keeps_its_base64_payload. Source: SYSUI8 s8.

## PKG 2 ENGINE-PAINT-FRAME-A11Y (14 items)
Files: layout/src/solver3/{getters.rs, display_list.rs}, layout/src/cpurender/{svg.rs, raster.rs},
layout/src/xml/svg.rs, layout/src/window.rs, layout/src/dom_lint.rs, core/src/gpu.rs, core/src/diagnostics.rs,
dll/src/desktop/shell2/common/event.rs, dll/src/desktop/shell2/ios/mod.rs, dll/src/desktop/shell2/android/mod.rs

2.1 CSS zoom does not reach border-radius, box-shadow or outline.
    Apply `getters::zoomed_length` / `get_effective_zoom` in `get_border_radius` (getters.rs:2087-2200), in the
    box-shadow resolution in display_list `paint_box_decorations`, and to the outline width / offset. Background
    sizes wait for background-size support (NOT SMALL).
    RED css_zoom_scales_border_radius_shadows_and_outlines. Source: LAYOUT7 left, MAIL6.
2.2 DEDUP: display_list `live_color` duplicates `get_used_text_color`.
    display_list.rs:8688-8771 adds an ancestor user-override walk that the getter (getters.rs:2512) lacks, so a
    currentcolor border misses colour animations. Move the walk and the UA fallback into the getter, and call it
    from both places.
    RED a_currentcolor_border_follows_an_animated_colour_on_its_parent. Source: WPT8 found (f).
2.3 vw / vh font sizes are resolved with a zero viewport when fonts are collected.
    `collect_font_stacks_from_styled_dom` (getters.rs:4628) keys the optical size from `get_element_font_size`
    (getters.rs:96, a zero viewport). Add a variant that takes the viewport and switch getters' own callers
    (:5710, :6032, :6042) to it. Keep the old signature: text3/cache.rs:1109 (PKG 1's file) switches in round 2.
    RED text_sized_in_viewport_units_is_drawn_by_its_own_font. Source: SYSUI8 s8.
2.4 The CPU SVG renderer ignores stroke-linecap, linejoin and dasharray.
    cpurender/svg.rs:574-577 hard-codes Round / Round. Read linecap / linejoin / miterlimit (the SVG defaults are
    butt / miter), and wrap the path in `agg_rust::conv_dash::ConvDash` for dasharray / dashoffset (raster.rs:4800
    already uses it). RED an_svg_stroke_takes_its_linecap_linejoin_and_dasharray. Source: PDF9 seen broken.
2.5 `svg_render` renders to PNG and decodes the PNG back.
    layout/src/xml/svg.rs:2472-2497 (TODO at :2480). Add a cpurender function that returns a `RawImage` from the
    private `rasterize_svg` (cpurender/svg.rs:151) with `premultiplied_alpha: true`.
    RED svg_render_returns_the_rasterised_pixels_without_a_png_round_trip. Source: PDF9 seen broken.
2.6 Every display-list build re-rasterises every SVG clip and stroke mask.
    display_list.rs:5804 (`rasterize_svg_stroke_to_r8`, :11837) and :5925 (`rasterize_svg_clip_to_r8`, :12025).
    Add a bounded memo of `ImageRef` keyed by (path hash, paint-rect size, viewBox bits, stroke width), held on the
    LayoutWindow or as a thread-local LRU. RED a_rebuilt_display_list_reuses_its_unchanged_svg_masks.
    Source: CHART7.
2.7 The tile path keeps its own i32 clip for LCD text (a twin of `text_run_clip`).
    raster.rs:4165 `render_text_prerendered_lcd` (~:4214-4243) next to `text_run_clip` (:3785). Derive it from
    `text_run_clip`, snapping outward.
    RED a_fractional_clip_cuts_tiled_lcd_text_where_it_cuts_grayscale_text. Source: Engine backlog 5.
2.8 CSS `opacity` tweens rebuild the display list every frame.
    display_list.rs:5154-5166 binds only `anim_opacity_keys`. The CSS `opacity_keys` are synced (gpu.rs:580-654)
    but never bound, and they are missing from `dl_emission_fingerprint` (gpu.rs:164). Bind them in PushOpacity,
    add them to the fingerprint, and add an Opacity arm next to the Transform values-only arm of
    `css_transition_tick` (window.rs:15159) that also patches the compact-cache opacity. Borderline SMALL: the
    transform arm is the template. RED a_css_opacity_tween_frame_after_the_first_is_values_only.
    Source: ANIMFRAME8 s8.
2.9 Timer.node_id is not remapped when the DOM is rebuilt.
    `remap_node_ids` (window.rs:25266) only drops the timers of unmounted nodes (~:25545). For each timer whose
    `node_id.dom == dom`, set `node_id = map.resolve_dom_node_id(dom, id)`, or None if the node is gone
    (`Timer.node_id`: layout/src/timer.rs:134, read by `get_node_size` at :405).
    RED a_timer_reads_its_own_nodes_size_after_a_node_is_inserted_before_it. Source: THREADS8.
2.10 The GpuValueCache is cloned on every layout pass.
    window.rs:7221-7225 `get_or_create_cache(dom_id).clone()`. Borrow `self.gpu_state_manager` disjointly at the
    `layout_document` call (the call already splits borrows at :7309-7317), or `mem::take` the cache and put it
    back. Perf only. Source: ANIMFRAME8 s8.
2.11 The four DOM lints run on every relayout of an unchanged DOM.
    window.rs:3034-3041. Skip doms whose DOM has not changed since the last lint (a per-dom generation or
    fingerprint). RED a_relayout_of_an_unchanged_dom_runs_no_dom_lint. Source: ANIMFRAME8 s8.
2.12 `[a11y-shape]` and two other lints print their findings on every pass.
    `warn_a11y_shape` (dom_lint.rs:1298), `warn_div_used_as_text_container` (:805) and
    `warn_interactive_without_accessibility` (:892) emit up to 8 lines each per pass through `diagnostics::emit`,
    which does no dedupe (core/src/diagnostics.rs:140). Route them through `dedup_key` / `EMITTED`, as the
    bare-text lint does (dom_lint.rs:57/427). RED the_same_a11y_shape_finding_is_printed_once_per_process.
    Source: ANIMFRAME8 s8.
2.13 The dll runs a redundant second a11y pass on every regenerate.
    `refill_a11y_tree_after_regeneration` (common/event.rs:4874; called at :4799 and :4828) runs after the layout
    tail already ran `update_a11y_tree` (window.rs:2953-2956). Keep it only for a regenerate that ran no layout
    pass. RED regenerate_layout_builds_the_a11y_tree_once. Source: A11YPATCH8 left 1.
2.14 iOS and Android rebuild their whole A11ySnapshot after every regenerate.
    ios/mod.rs:1913 (caller :2025) and android/mod.rs:317 (caller :528). Return early unless
    `lw.a11y_manager.last_pass.published` (managers/a11y.rs:535). Mobile only, no headless RED.
    Source: A11YPATCH8 left 3.

## PKG 3 ENGINE-INPUT-IO + DLL + TOOLING (16 items)
Files: core/src/events.rs, core/src/url.rs, layout/src/managers/focus_cursor.rs, layout/src/http.rs,
layout/src/e2e/{runner.rs, full.rs}, layout/src/telemetry/crash_mail.rs, doc/src/gene2e.rs,
scripts/gen_e2e_cases.py, dll/src/desktop/shell2/macos/{events.rs, mod.rs}, dll/src/desktop/extra/capability.rs,
dll/src/unified/audio.rs, doc/src/autofix/{type_index.rs, module_map.rs, mod.rs, function_diff.rs},
doc/src/codegen/v2/{mod.rs, lang_haskell/mod.rs, lang_swift/wrappers.rs, lang_d/wrappers.rs, lang_go/types.rs}

3.1 macOS never sends key-up after Cmd+letter when no menu item takes it (the letter stays held).
    macos/mod.rs:7955 ignores KeyUp, and `handle_flags_changed` (macos/events.rs:884) only updates the modifier.
    When Cmd goes up, release every non-modifier key still in `pressed_virtual_keycodes` (one state-diff pass).
    Verify on this Mac. RED a_letter_pressed_with_cmd_is_released_when_cmd_comes_up. Source: EVENTS7 found.
3.2 Focusing a node by id lands on an unfocusable wrapper (Cmd+F in AzTasks, Ctrl+F in AzMonitor, Mod+F in AzKeys).
    The `FocusTarget::Id` arm (managers/focus_cursor.rs:863-875) resolves to any existing node. If the node cannot
    hold focus, resolve to its first focusable descendant (like delegatesFocus). The unit test
    `resolve_focus_target_id_accepts_valid_but_unfocusable_node` (:2167) changes with it. The RED already exists:
    layout/tests/focusing_a_search_field_by_its_id_focuses_its_text.rs (2d0afec0f, failing).
    Source: PIMDRIVE7 found, KEYS9 / MONITOR9 left.
3.3 Shift+Insert (and Ctrl+Insert) are not shortcuts.
    `KeyboardShortcut::from_key` (core/src/events.rs:5096-5114) returns None without primary, and
    `handle_key_down_for` matches only inside `if primary` (:5513). Off macOS, map Shift+Insert to Paste and
    Ctrl+Insert to Copy; the dll's existing `focus_hears_paste` input applies unchanged.
    RED shift_insert_on_a_node_that_listens_for_paste_becomes_the_engines_paste. Source: TERM9 found.
3.4 `http_get` / `download_bytes` block the calling UI callback on desktop.
    http.rs:457-465 and :606-614 call the blocking request, then `request::complete`. Spawn a thread and use
    `request::defer` with a polling closure (request.rs:153; desktop/dialogs.rs:587 is the template). Check any
    test that expects the synchronous result.
    RED http_get_from_a_callback_returns_before_the_response_arrives. Source: MAIL9 engine gaps.
3.5 `Url::open`: the Windows command breaks on `&`, and there is no file-path variant.
    core/src/url.rs:135 shells out with `cmd /C start <url>` (an `&` splits the command, and the first quoted
    argument becomes the window title). Quote it properly (or use ShellExecuteW / NSWorkspace), and add a path
    variant (api.json: parent). appkit's `open_external` switches to it in PKG 5.
    RED the_windows_open_command_keeps_an_ampersand_url_whole. Source: NEWS9 left, pause note.
3.6 HttpResponse does not report the final URL after redirects.
    `HttpResponse` (http.rs:713) has no final URL. Add `final_url` (from ureq `get_uri`) at construction (:1150);
    the api.json field is the parent's. Telling a 301 from a 302 is NOT SMALL.
    RED a_followed_redirect_reports_the_final_url. Source: NEWS9 left.
3.7 The in-crate scenario runner has no outside-press popup dismissal.
    `dismiss_popups_on_escape` (runner.rs:881) handles Escape only; the real path also closes a popup on an
    outside press. Add the press arm. RED a_press_outside_a_popup_in_a_scenario_dismisses_it.
    Source: Engine backlog 11.
3.8 The e2e protocol has no paste op.
    `DebugEvent` (e2e/full.rs:2354). Add `paste {text, html}`: build a `ClipboardContent`, run the Paste callbacks,
    then the existing `paste_clipboard_content`. Update OP_POLICY in doc/src/gene2e.rs and
    scripts/gen_e2e_cases.py. RED a_scenarios_paste_op_pastes_bold_html_into_the_focused_editor.
    Source: Engine backlog 10.
3.9 `PlatformCapability::scheduled_notifications()` is missing.
    dll/src/desktop/extra/capability.rs has only `notifications()` (:439). Return true for UN / Apple and Windows
    toasts; false for freedesktop, portal, Android and balloon (per notifications/mod.rs:38-47). The api.json
    entry is the parent's. RED scheduled_notifications_capability_says_whether_the_backend_can_schedule.
    Source: CLOCK9 left.
3.10 The wasm32 AudioSink stub lacks the 7 methods api.json now calls.
    dll/src/unified/audio.rs:57-80 has only open / is_open / play / frames_played / error_message / close. Add
    `try_play -> false`, `queued_frames -> 0`, `samples_played -> 0`, `pause -> false`, `resume`,
    `clear -> false`, and `config -> AudioConfig::default()`, mirroring dll/src/desktop/extra/audio/mod.rs:332-433.
    No RED (cfg wasm32). Source: integration 2026-10-05 (MEDIA9 P6).
3.11 9 api.json externals point at `azul_dll::desktop::...` instead of the `azul_dll::unified::...` facade
    (wasm32 cannot build them).
    AudioFileDecoder, AudioPlayer, AudioDecoder, AudioEncoder, EchoCanceller (-> unified::audio), ParsedPdf,
    PdfPageSize (-> unified::pdf), Mp4Muxer, Mp4Demuxer (-> unified::video_codec). The facade re-exports all of
    them off-wasm (desktop/extra/audio/mod.rs:48-66, pdf/mod.rs:23, video_codec/mod.rs:39) and has wasm stubs.
    Root cause: the type index skips the facade's wasm32 stubs and its `pub use` items, and TOOLS7 left azul_dll
    out of the re-export logic. Rule: a type under `azul_dll::desktop::extra::<m>::` that
    `dll/src/unified/<m>.rs` re-exports gets the path `azul_dll::unified::<m>::<Name>`. The parent's next scan
    then emits the 9 path fixes; also check PdfPageSize's module (css today, pdf expected).
    RED a_desktop_extra_type_reexported_by_the_unified_facade_takes_the_facade_path. Source: integration 2026-10-05.
3.12 The type index attaches cross-file `impl` methods by NAME to every candidate.
    type_index.rs:1093-1105 / `attach_methods_to_type` (:1119). Phase 2 parses every file, including the file that
    defines a module-private type of the same name. Attach only to candidates in the impl's own crate (inherent
    impls must live there), and skip impls whose self type is defined in the same file (phase 1 already attached
    those, at :1660-1666). RED a_private_types_impl_methods_stay_off_a_public_type_of_the_same_name.
    Source: integration 2026-10-05.
3.13 `autofix add`: `&mut [T]` and `&Vec<u8>` arguments get no correct conversion.
    The body passes `&[u8]` where the method wants `&Vec<u8>`. Extend `slice_arg_type` / the accessor templates
    (function_diff.rs). RED a_vec_ref_argument_is_passed_as_a_vec_and_a_mut_slice_as_a_mut_slice.
    Source: TOOLS7 seen, not fixed.
3.14 DEDUP: `upper_first` exists 4 times.
    lang_haskell/mod.rs:1236, lang_swift/wrappers.rs:3824, lang_d/wrappers.rs:2909, lang_go/types.rs:600. Make
    one in codegen/v2/mod.rs. No RED (the goldens cover it). Source: TOOLS7 seen, not fixed.
3.15 DEDUP: `collect_rs_files` (autofix/mod.rs:111) and `collect_rust_files` (type_index.rs:1387) walk the same
    trees with different filters. Make one walker with a filter argument. No RED. Source: TOOLS7 seen, not fixed.
3.16 DEDUP: crash_mail builds its own MIME and base64 (a twin of micromail's MessageBuilder).
    layout/src/telemetry/crash_mail.rs:279 (`build_mime_body`) and :306 (`base64_encode`). Switch to micromail
    0.2's builder with `send_raw`, map the outcomes, and restate the MIME tests. Borderline SMALL.
    Source: HYGIENE (wave-6 notes).

## PKG 4 WIDGETS (18 items)
Files: layout/src/widgets/{text_input.rs, color_input.rs, token_input.rs, icon_grid.rs, ribbon.rs, toolbar.rs,
button.rs, money_input.rs, date_range_picker.rs, reference_picker.rs, cell_grid.rs, data_table.rs, timeline.rs,
chart.rs, thumbnail_strip.rs, dialog_kit.rs, todo_bar.rs, summary_list.rs, reading_pane.rs, close_guard.rs,
rich_text_editor.rs, rich_text/html.rs, rich_text/doc.rs}, layout/src/widgets/themes/{flat.rs, flora.rs}

4.1 TextInput ignores the font size its app gives the field (the AzNotes title).
    11px is pinned on the value `<p>` in `TEXT_INPUT_LABEL_PROPS` (text_input.rs:437 win, :476 linux, :515 mac /
    mobile). Move `const_font_size(11px)` onto the Windows (:89-196) and mac (:301-~400) CONTAINER props and
    delete the three label lines. Also check the other users of these statics: color_input.rs:821,
    flora.rs:3053/3068. The RED exists: layout/tests/a_text_field_takes_the_font_size_its_app_gives_it.rs
    (4afd055db), with the guard a_text_field_without_a_font_size_keeps_the_ui_size. Source: OFFICE7 / WRITER6.
4.2 AzShow's slide-rail thumbnails misalign when a slide has a badge.
    Give `THUMBNAIL_NUMBER_BASE` (thumbnail_strip.rs:533) a fixed width or min-width in column mode. The RED
    exists: layout/tests/a_slide_rails_thumbnails_line_up_with_and_without_a_badge.rs (4809ad3ed).
    Source: OFFICE7 found.
4.3 TokenInput: a refused token keeps its text with no invalid look.
    token_input.rs:1039-1073. On Refuse, restyle the entry with `text_input_invalid_ring` (flat.rs:3972,
    flora.rs:3962) through `set_css_property`. RED a_refused_token_rings_the_entry_as_invalid. Source: WIDGETS9A left.
4.4 IconGrid has no type-ahead.
    `grid_key` (icon_grid.rs:979-1011) has no letter keys. A letter moves the focus to the next item whose label
    starts with it. RED typing_a_letter_moves_the_focus_to_the_next_item_named_with_it. Source: WIDGETS9A left.
4.5 DEDUP: ribbon.rs `styled_button` (:3783-3809) is a twin of toolbar.rs `tool()` (:971-1026).
    Move it into button.rs taking `theme: OptionUiTheme`; the ribbon's 4 calls pass Some. Existing tests cover it.
    Source: WIDGETS9A s10.
4.6 MoneyInput digits are left-aligned.
    The TextInput container pins `text-align: left` (text_input.rs:273, :374). In money_input.rs `build`
    (~:1288-1316), push `text-align: right` onto `resolved_container_style()`; no API change. Check the caret and
    scrolling live. RED a_money_input_aligns_its_digits_to_the_right. Source: WIDGETS9B left.
4.7 DateRangePicker: the presets are separate Tab stops, and there is no year jump.
    date_range_picker.rs:987 (presets), :1178 `plain_key` drops Shift, :1192 PageUp / PageDown. Make the presets
    one roving group (`roving::item_tab_index` + `move_stop`), and Shift+PageUp / PageDown = +-12 months.
    RED the_presets_are_one_tab_stop_and_arrows_walk_them, shift_page_down_turns_a_year. Source: WIDGETS9B left.
4.8 ReferencePicker has no clear button.
    `ReferencePickerEventKind` (reference_picker.rs:156-164) is Query / Pick / Create. Add `Clear` (the api.json
    enum variant is the parent's) and an x button shown while a record is picked.
    RED a_picked_reference_clears_with_its_x_button. Source: WIDGETS9B left.
4.9 CellGrid: Ctrl+C and Ctrl+X are dead (paste works).
    core/src/events.rs:5520-5534 drops Copy / Cut unless the focus is contenteditable or text is selected, so the
    grid's `Focus(Copy/Cut)` handlers (cell_grid.rs:2833-2834) never run. Do as the DataTable does
    (data_table.rs:3728-3731): in `on_grid_key` (cell_grid.rs:3141-3170), when not editing, Ctrl+C / Ctrl+X ->
    `prevent_default` + `copy_selection(..)`. RED ctrl_c_on_the_focused_grid_puts_the_range_on_the_clipboard.
    Source: DATATABLE7.
4.10 DEDUP: CellGrid's edit-key caret code (cell_grid.rs:3080-3096) duplicates `data_table::line_edit`
    (data_table.rs:2858-2884). Add a fallback arm that calls `line_edit(..)?`; the enter-mode and pointing arms
    stay above it. No RED. Source: DATATABLE7.
4.11 DataTable: a double-click on a header edge does not auto-fit the column.
    `double_click` (data_table.rs:3533) handles only `Hit::Cell`; `Hit::HeaderEdge` (:1995) is drag-only. Fit to
    the widest text of the header plus the rows in view, using the cell_grid character-width estimate
    (cell_grid.rs:2165). RED double_clicking_a_header_edge_fits_the_column_to_its_widest_text_in_view.
    Source: DATATABLE7.
4.12 DEDUP: timeline.rs `tick_label` (:1709-1720) re-implements `SeekBar::media_time`.
    `if step < 1.0 && t < 3600.0 { &timecode[3..] } else { seek_bar::media_time(t.max(0.0)) }` (seek_bar.rs:86).
    No RED. Source: MEDIA9 s8.
4.13 DEDUP: thousands grouping in the layout crate (3 loops).
    data_table.rs:2351, chart.rs:1124, money_input.rs:766. Split `format_money`'s loop into a
    `pub(crate) group_digits` and call it from data_table and chart. (The apps switch in PKG 5 / 6.)
    RED group_digits_groups_by_three_with_the_locales_separator. Source: ERP9 twins, DEDUP.
4.14 DEDUP: two `classes(&[&str])` copies (timeline.rs:1697, chart.rs:2519). Use one in decl.rs.
    Source: DEDUP_WIDGETS_API F (left over after 2dd113562).
4.15 DEDUP: three private `hook()` builders of an `OptionXOnEvent` (todo_bar.rs:515, summary_list.rs:925,
    reading_pane.rs:502). Use `CoreCallbackData::create` / the callback macro's `create`.
    Source: DEDUP_WIDGETS_API F21.
4.16 DEDUP: rich_text/html.rs `escape_html` (:115-127) duplicates `Xml::encode_text` / `encode_attribute`
    (core/src/xml_html.rs:212/226). Switch the text calls (:141, :259, :279) to encode_text and the attribute
    calls (:159, :255, :265, :266) to encode_attribute.
    RED a_control_character_in_a_paragraph_does_not_reach_the_html. Source: XML8 found, DEDUP F31.
4.17 dialog_kit `row_button` fakes the disabled state (manual Unavailable + "held" skin; dialog_kit.rs:264-310).
    Use `Button::with_disabled`. The disabled look changes: look at both themes.
    RED a_disabled_dialog_button_is_a_disabled_button. Source: APIEXPORT wave-6 list.
4.18 Hand imports of `impl_option_inner` left after 0a98cab8f ($crate:: in the macros).
    rich_text/doc.rs:26, rich_text_editor.rs:60, close_guard.rs:61: drop them. page_breaks.rs:22 belongs to the
    other session: leave it. Source: FOLLOW-UPS (impl_option! hygiene), DEDUP_WIDGETS_API.

## PKG 5 APPS-A: office / PIM apps + the shared crates (14 items)
Files: examples/azul-appkit/** (+ its Cargo.toml), examples/azul-storage/**, examples/azul-pim/**,
examples/azul-contacts/**, examples/azul-mail/**, examples/azul-notes/**, examples/azul-review/**,
examples/azul-tasks/**, examples/azul-calendar/**, examples/azul-writer/**, examples/azul-show/**,
examples/azul-sheets/**, examples/azul-drive/**, examples/azul-photo/**, examples/azul-videocut/**;
in the PKG-6 crates ONLY: azul-keys/src/{ui.rs, import.rs}, azul-erp/src/csv_io.rs, azul-reader/src/{ribbon.rs,
jobs.rs}, azul-code/src/storage.rs; scripts/azmail_e2e.py; Cargo.lock (csv for appkit)

5.1 DEDUP: one CSV reader.
    AzContacts' own RFC 4180 parser (azul-contacts/src/csv.rs:110-182) versus the csv crate, and the separator
    guess written 3 times (contacts csv.rs:116-127, azul-keys/src/import.rs:228, azul-erp/src/csv_io.rs:405-413).
    Add `azul_appkit::csv::{separator, read_table}` on the csv crate and switch all three. The csv crate does not
    report an unclosed quote, which a contacts test expects: restate that test.
    Source: KEYS9 / ERP9 DEDUP.
5.2 DEDUP: one set of DOM helpers.
    strs / text / block / column / row / button / primary are copied in azul-contacts/src/ui.rs:328-360 and
    azul-keys/src/ui.rs:47-110. Add an `azul_appkit` module for them (appkit's `ui::row` has another signature:
    pick a new name). AzNews' copy (azul-news/src/ui.rs:502-540) switches in round 2. Source: KEYS9 DEDUP.
5.3 DEDUP: AzMail's style-sheet sanitizer mechanics duplicate `azul_appkit::css`.
    azul-mail/src/html.rs:850-1096 (`sanitize_rules`, `block_body` :946, `parse_style` :1033, `safe_style_value`
    :1074, comment stripping) versus appkit css.rs:32/50/70/120/160. Keep only the policy (paper scoping, class
    prefixes). The existing sanitizer tests stay green; add one for `content: ";"`. Source: READER9 twins.
5.4 DEDUP: one ribbon-helper builder.
    azul-reader/src/ribbon.rs:22-53 = azul-writer/src/ribbon.rs:25-57, with more copies in show ribbon.rs:41-67,
    sheets lib.rs:1282-1310, drive ui_ribbon.rs:60-108, calendar chrome.rs:94, tasks chrome.rs:299. One appkit
    builder taking (RefAny, callback). Source: READER9 twins.
5.5 DEDUP: the paged drive-listing loops.
    Hand-written in appkit files.rs:67 (a twin of `ops::list_all`), notes store.rs:~130, pim task_store.rs:43,
    photo storage.rs:250/305 and videocut store.rs:80 (all -> `azul_storage::ops::list_all`); and in
    code/storage.rs:85, mail store.rs:262 and sheets storage.rs:~120 (folder listings -> a new
    `ops::list_folder_all`). RED list_folder_all_follows_every_page. Source: DEDUP_OFFICE (S3 blockers).
5.6 DEDUP: one TempDir for tests.
    azul-mail/src/testutil.rs:9, azul-drive/src/fileops.rs:848, azul-reader/src/jobs.rs:357, azul-appkit/src/
    files.rs:225 (TestDir) and azul-storage/src/tests/mod.rs:22 all twin `azul_pim::testing::TempDir`. Move it
    into azul-storage behind a `testing` feature (storage cannot depend on pim); pim re-exports it. Test code
    only. Source: PIM 2026-10-02 wave-6 list.
5.7 Open with the OS through the engine.
    azul-review/src/lib.rs:572-577 still hand-rolls open / xdg-open / explorer: call
    `azul_appkit::files::open_external` and show the error in `s.status`. Make appkit's `open_external` call
    `azul::Url::open` (+ the path variant). Merge after PKG 3's 3.5. Source: NEWS9 twins.
5.8 AzPhoto's tool rail drops its a11y states.
    azul-photo/src/ui.rs:514 `.with_accessibility_info(..)` replaces the info and loses the Unavailable / checked
    states set by `with_toggled` / `with_disabled` (:503-507). Use `.with_accessibility_assign(..)`
    (core/src/a11y.rs:103). Test: the tool's a11y node keeps checked. Source: APIEXPORT wave-6 list.
5.9 DEDUP: AzVideoCut `rgba_to_i420` (export.rs:118, used at :173/:263) twins core `rgba_to_nv12`
    (core/src/resources.rs:1818). After the parent exports rgba_to_nv12: convert to NV12 and split the UV plane.
    Expect +-1 rounding in export_tests.rs:8. Source: VIDEO8.
5.10 DEDUP: AzMail compose formats an attachment's size by hand (azul-mail/src/ui_compose.rs:630, "({} KB)",
    rounded up). Use `DiskSpace::format_bytes`. Source: DEDUP_WIDGETS_API F (byte sizes 4 ways).
5.11 azmail_e2e.py: a phase for the Sending page's third choice (signed-in submission) and DKIM.
    The sink already supports `--auth` (azmail_smtp_sink.py:27): choose, Save, send to the sink. Source: MAIL9 left 2.
5.12 DEDUP: AzSheets' thousands grouping (sheets model.rs:145, fake_engine.rs:244) -> `MoneyInput::format_amount`
    with an empty symbol. Source: ERP9 twins.
5.13 Adopt Toolbar: AzContacts `toolbar()` (azul-contacts/src/ui.rs:1285), the AzNotes format toolbar
    (azul-notes/src/ui.rs ~779-900), AzReview `toolbar()` (azul-review/src/ui.rs:255). Source: WIDGETS9A s10
    (DEDUP F30), pause note.
5.14 Adopt TokenInput: AzTasks tags (azul-tasks/src/detail.rs:514, which says "no TokenInput widget yet");
    borderline: AzMail To / Cc and AzCalendar attendees (address parsing through azul-pim's mail_address).
    Source: pause note WIDGETS9A, PIMDRIVE7.

## PKG 6 APPS-B: the wave-9 apps (12 items)
Files: examples/azul-dashboard/**, examples/azul-pdf/**, examples/azul-news/**, examples/azul-monitor/**,
examples/azul-clock/**, examples/azul-music/**, examples/azul-widgets/src/video.rs; azul-code/src/{ui.rs,
commands.rs}; azul-reader/src/{ui_library.rs, content.rs, commands.rs, paginate.rs, position.rs};
azul-keys/src/ui_item.rs; azul-erp/** except csv_io.rs

6.1 AzDashboard uses two id prefixes.
    chart.rs:34/36 define CHARTS / CHARTS_CAPTION with `__azdashboard_` (used at :313, :322); ids.rs uses
    `__azdash_`. Add CHARTS_ROW_CLASS / CHARTS_CAPTION to the `names!` block in ids.rs, delete chart.rs:33-36, and
    use `ids::...`. Test: no `__azdashboard_` left in src. Source: MONITOR9 seen broken.
6.2 Adopt Toolbar: AzPdf (azul-pdf/src/ui.rs:203-212), AzNews (azul-news/src/ui.rs:1304), and AzCode's find bar
    (azul-code/src/ui.rs:342). Source: TODO(WIDGETS9A) markers.
6.3 Adopt TokenInput: AzKeys tags (azul-keys/src/ui_item.rs:791). Source: TODO(WIDGETS9A).
6.4 Adopt IconGrid: AzReader's covers grid (azul-reader/src/ui_library.rs:5). Optional (borderline): AzMusic album
    covers (azul-music/src/ui.rs:330, lib.rs:5; the covers must become `IconGridItem.image`).
    Source: TODO(WIDGETS9A).
6.5 Adopt Gauge in place of ProgressBar: AzMonitor per-core (azul-monitor/src/ui.rs:493), AzKeys TOTP ring
    (azul-keys/src/ui_item.rs:221), AzClock timer ring (azul-clock/src/ui/views.rs:424, ids.rs:63).
    Source: TODO(WIDGETS9B).
6.6 AzERP:
    (a) MoneyInput for the amount fields (erp money.rs:10, ui/form.rs:10/190, `FieldKind::Decimal`);
    (b) ReferencePicker for category / location (ui/form.rs:181, views/spec.rs:96);
    (c) a delete confirmation: ui/mod.rs:473-476 deletes at once. Set `confirm_delete`, render a
        `MessageBox(Question)` in a `Modal` (as azul-monitor/src/ui.rs:243-270 does), and only Yes calls
        `delete_asset`. Test: "delete" queues no `Write::Delete` until confirmed;
    (d) money.rs:20 `format_amount` -> `MoneyInput::format_amount`;
    (e) borderline: the register's filter bar (DateRangePicker + a status DropDown).
    Source: ERP9 left, TODO(WIDGETS9B), ERP9 twins.
6.7 DEDUP: AzDashboard's grouping (table.rs:80, data.rs:592) -> `MoneyInput::format_amount` (minor_digits 0 for
    counts). Source: ERP9 twins.
6.8 AzNews:
    (a) the rename / folder of a feed are lost when the window is closed on the feed page (`on_feed_title` /
        `on_feed_folder` ui.rs:2461-2496 only set `list_dirty`; the write happens in `leave_form` :1657). Add a
        `Window(CloseRequested)` callback: if dirty, `save_list`, `prevent_window_close`, and close when the write
        lands (as azul-calendar/src/lib.rs:587-627 does);
    (b) "Refresh every" takes effect only at the next start (the timer is made once in `on_window_created`
        :1679-1685). Keep `refresh_timer: Option<TimerId>`, and re-arm it in `on_set_refresh_every` (:2638-2649).
    Test: E2E rename -> close on the page -> restart -> the title is kept. Source: NEWS9 left.
6.9 AzCode: no folder picker. `FileDialog::open_directory` exists (layout/src/desktop/dialogs.rs:616). Add
    "Open Folder..." on the welcome screen and in the menu -> `commands::open_workspace(.., folder_root(&path))`
    (commands.rs:38/50). Test: E2E with a mocked file-open answer. Source: CODE9 left.
6.10 AzReader: in-book links do nothing, and TOC jumps are approximate.
    (a) The sanitizer drops `href` (content.rs:16-17). Keep relative `<a href>`, record a per-chapter link table
        under a prefixed id, and add one click callback on the column (paginate.rs:76) that resolves (chapter,
        fragment) with `resolve()` and calls `commands::go_to(.., Target::Anchor)`;
    (b) exact anchors: after layout, use `get_node_id_by_id_attribute` + `get_node_position` and
        `PageMap::page_of_y` (position.rs:135). Book ids are unprefixed: prefix them.
    Borderline (~200 lines). Test: an EPUB fixture whose footnote link lands on the footnote's page.
    Source: READER9 left.
6.11 AzClock: add a city context menu (Move up / Move down / Remove; azul-clock/src/ui/views.rs:280) and a new
    `Action::CityDown`. Test: CityDown swaps. Drag reorder is NOT SMALL. Source: CLOCK9 left.
6.12 The AzWidgets video demo: its own seek bar (video.rs:190-216, :314-337) and `clock()` (:279-287, m:ss with no
    hours) -> `SeekBar` + `SeekBar::media_time`. RED a_video_past_an_hour_shows_its_hours. Source: MEDIA9 s8.

## Round 2 (after the packages merge: they cross two packages' files)
R.1 text3/cache.rs:1109 `pre_resolve_chains_for_dom` -> the viewport-taking collector from 2.3 (PKG 1 + 2).
R.2 AzNews' DOM-helper copy (azul-news/src/ui.rs:502-540) -> the appkit module from 5.2 (PKG 5 + 6).

## PARENT (api.json through azul-doc autofix; not agent work)
P.1 AudioSink.open / play / frames_played still say "stub" in api.json (the Rust docs in desktop/extra/audio/
    mod.rs are right; autofix never refreshes the doc of an existing entry): `autofix remove` + `autofix add` of
    the three. A doc-drift check in the scan would stop this recurring (NOT SMALL: a tool feature).
P.2 After 3.11: run the scan, and apply the 9 external path fixes (+ PdfPageSize css -> pdf if the rule says so).
P.3 Exports the packages need: Url path variant (3.5), HttpResponse.final_url (3.6),
    PlatformCapability.scheduled_notifications (3.9), ReferencePickerEventKind::Clear (4.8), core rgba_to_nv12
    (5.9). Exports needed by NOT SMALL items stay out.
P.4 `ProgressBar.with_container_style` is a misnamed duplicate of `with_container_background` (same fn_body;
    TOOLS7 seen). Check that no app calls it, then `autofix remove` it, or keep it.
P.5 Register every new layout/tests/*.rs that a package names (all.rs; never stage the other session's line).
P.6 Re-measure after the merge: the mail corpus (1.8 moves every `<sup>`), the WPT run, and the suites.
P.7 The wave-9 api list after the merge: one `autofix add` round from the packages' reports, the drift loop to
    0 / 0 critical, codegen, then build the dylib + 35 apps.

---------------------------------------------------------------------------------------------------------------------

# NOT SMALL - open (one line each; source)

## Engine: layout / text / CSS
- `::before` / `::after` generated content (the selector parser drops them; parser2.rs:459): design in WPT8 s7. (WPT8)
- `display: inline list-item` (a new LayoutDisplay variant + marker in inline layout). (WPT8)
- Per-element inline fragments: nested-span IFC (002/006), split-inline borders, and a split inline's own
  background / border beside the block. (WPT8, LAYOUT7 left)
- background-size / background-position / background-origin are not painted at all; zoom for background sizes
  depends on it. (WPT8, LAYOUT7 left)
- Floats split inline runs (no in-line float placement in the IFC). (LAYOUT7 left)
- RichTextEditor::page_doms + AzWriter start pages only at whole blocks: page starts must become (block, run, byte)
  via the break_line_* accessors (FFI signature + split-block rendering). (LAYOUT7 left, WRITER6)
- A growing block child does not grow its auto-height parent (LAYOUTPERF8 bug C; relayout-boundary design).
  (LAYOUTPERF8)
- An inline-block of two block children is 17.81px wide (Chrome 8.91): root cause not located. (RULINGS8 LEFT)
- 300 Avatar rows grow past 1.5 GB in the first layout: RED exists, root cause not located (needs a profiling
  build). (PIMDRIVE7)
- A one-contact list sits at the bottom of a row that does not fill the pane: RED exists, cause not located.
  (PIMDRIVE7)
- A form section laid out as a row beside a photo contact: no repro, no RED. (PIMDRIVE7)
- `:focus-within` is never raised and does not even parse in a stylesheet (new selector variant in a repr(C) enum,
  cascade tiers, restyle, dll runtime states; ~300+ lines). (WIDGETS9A)
- SVG `<text>` / `<image>` in the CPU renderer (PDF pages show only shapes). (PDF9)
- Runtime font registration by family name reachable from CallbackInfo, kept across `replace_fc_cache` (FONTREG;
  API shape). (MAIL9)
- A disk cache of baked SF instances; optical sizing for app-registered variable fonts; ruby annotations keep the
  base size's instance; GPOS variation kerning of baked instances; Windows / Linux system-ui untested. (SYSUI8)
- Text raster (cpurender text_raster) is LTR only, with no wrapping or alignment. (MEDIA6)
- PDFFIX limits (vertical modes, floats, orphans / widows, max-height, split box paints the first fragment only) and
  page_breaks.rs sub-pixel last page: the other session's file. (FOLLOW-UPS)
- Threaded text frames for AzPdfMaker (fragmentainer chains). (ROADMAP)

## Engine: paint / frame / perf / a11y
- The click that starts a glide regenerates the whole 3472-node AzWidgets DOM (0.5-0.9 s headless): the next perf
  target. (MONDAY_RESUME)
- No-op relayout still reconciles (~10 ms; reconcile 4.6 ms). (ANIMFRAME8)
- The tree clone into DomLayoutResult (window.rs:7951-7955, :7394): Arc ownership refactor. (ANIMFRAME8)
- One whole-DOM compact rebuild per layout-tween frame (window.rs:15272): per-node compact patch in core
  compact.rs. (ANIMFRAME8)
- A colour patch deep-copies the whole display list (window.rs:15246): shared item runs. (ANIMFRAME8)
- A per-node content epoch so the a11y pass hashes only geometry (the last ~1 ms; many writers). (A11YPATCH8)
- A VirtualView has no scroll blit (`collect_scroll_shifts`, compositor.rs:2786). (Engine backlog 5)
- Not checked, platform-specific: AzReview's per-frame RenderImageCallback ImageRefs keep it non-idle; the Linux
  multi-window poll cap; Android / iOS pace at the 60 Hz fallback. (Engine backlog 5)
- Zero-copy IOSurface video; Opus off Apple (a user decision); VIDEO8's unverified leftovers (decode/encode
  threading). (Engine backlog 8, VIDEO8)
- WAYLAND8 left: the 8-bit-KWin swizzle, pool reuse during resize drags, a third slot, fd-passing MIT-SHM (needs a
  Linux run). (WAYLAND8)

## Engine: input / platform / io
- macOS maps keys by physical position (AZERTY Cmd+Z arrives as Cmd+W). The proper fix needs TIS + UCKeyTranslate;
  a Latin-only `charactersIgnoringModifiers` fix is small. (EVENTS7)
- A "concealed" clipboard flag (API + the external rich-clipboard crate + 4 backends). (KEYS9)
- Reading the clipboard outside a paste (an async read request; Wayland). (KEYS9)
- A screen-lock / sleep / session event (NSWorkspace, WTSRegisterSessionNotification, logind). (KEYS9)
- Keyring secrets and text-field passwords cannot be zeroized. (KEYS9)
- Exporting `download_bytes_blocking` / `http_get_blocking`: they are documented "not public (cannot exist on
  web)", so this needs a ruling. (MAIL9 HTTPBLOCK)
- Telling a 301 from a 302 (redirect history or follow_redirects false). (NEWS9)
- A `Delivered` notification event (repr(C) variant, macOS willPresent, an in-process mirror for Windows).
  (CLOCK9)
- CLOCK9 platform alarms: iOS AlarmKit, Android setAlarmClock, Windows alarm toasts + Task Scheduler, Linux systemd
  timers, a tray icon. (CLOCK9, Engine backlog 14)
- Engine backlog 10: the deferred selection items N3 / N4 / N5 / N8 and "D typed rects" (no spec in the ledger).
- Engine backlog 12 AzBuilder (B5 extras, file viewer, code exports, HTML tree to component, L3 previews).
- Engine backlog 13 the widgets / theming program (every HTML input type, @theme, native theme, localization review).
- Engine backlog 15 codegen held items (compile_fn per language, ComponentCodegen::Call, theme->mode sweep). LAST.
- Engine backlog 3 ABI guard left: Lua / PHP compare nothing; C# / Ruby / Node / Go / Java / Swift declare nothing;
  the Rust e2e is not in CI. (ABI8)

## Widgets
- Toolbar overflow measured instead of estimated (a resize hook; where measured widths live). (WIDGETS9A)
- TokenInput suggestions in a transient popup (an overflow: hidden ancestor clips them today). (WIDGETS9A)
- IconGrid: autoscroll during the rubber band, F2 rename, group headers, two-line labels with an ellipsis, focus
  outline only when the grid has focus (needs :focus-within). (WIDGETS9A)
- MoneyInput Indian lakh grouping (a new field on a repr(C) struct). (WIDGETS9B)
- TimePicker minute -> hour carry (decision first: native fields wrap without carrying) and typing digits.
  (CLOCK9, W13)
- DataTable re-sort after an edit (by design the app calls start_query: a decision). (DATATABLE7)
- DataTable icon cells, dragging rows out, a right-click event -> then AzDrive Details on DataTable. (PIMDRIVE7)
- One WAV writer exported in the API (AzMusic, AzReview, AzDrive x2, the dll fixture). (MEDIA9)
- MediaControls compact / large / overlay sizes; SeekBar hover time + thumbnails + chapters; Waveform recolours
  played bars in place. (MEDIA9)
- ChartColor / IconModeColors -> one ModeColor (ABI rename; 41 + 5 api.json uses). (TERM9)
- Rename the general widgets out of `shells` (22 Shell* types, 32 crates). (DEDUP_WIDGETS_API)
- 124 hand-written `CoreCallbackData { .. }` literals in 35 files (mechanical; split per file). (HYGIENE)
- Slider native look (track with filled segment). (Engine backlog 11)
- AzSheets dark header row white on light: unverified, repro first. (OFFICE7)

## Apps
- AzSheets merges do not shift with inserted rows and are not undo steps (outside IronCalc's model). (OFFICE7)
- AzCalendar backstage text very faint in light mode: cause unknown, needs a screenshot. (PIMDRIVE7)
- AzCalendar / AzTasks data roots outside Azlin (a data move); Calendar / Tasks / Meet on appkit's CLI (flags,
  env vars). (DEDUP_OFFICE)
- AzVideoCut: no CloseGuard (it autosaves; a close during a save job is not waited for); meters from
  AudioFileDecoder (per-clip audio decode). (DEDUP_OFFICE, MEDIA9)
- GlobalHotkey::matches adoption (a CommandTable design). (APIEXPORT)
- AzNotes / AzTasks date sections vs azul-pim DateGroup: different buckets (a product decision). (NEWS9)
- AzMail: fetch the listed fonts (needs FONTREG), remote style sheets, an Outbox view, OAuth token refresh. (MAIL9)
- AzNews: a virtualized list, full-text extraction, podcast playback, magazine mode (IconGrid cards). (NEWS9)
- AzCode: targeted updates instead of RefreshDom per key (a CodeView API), per-tab close (a DocumentTabs widget),
  workspace search, IME preedit, CJK width, file watching / LSP / git / folding / palette / terminal. (CODE9)
- AzReader: page-turn cost on long chapters, embedded fonts, search, highlights, mobile layout. (READER9)
- AzTerm: profiles / settings, split panes, find in scrollback, URL click, bell, triple-click, 1003 motion, cursor
  blink, IME, OSC 52, close-with-running-command prompt. (TERM9)
- AzMusic / AzPlayer: cover art, full-screen view, lyrics, podcasts, playlist UI, folder watch, subtitles, track
  menus, PiP, audio as the master clock; engine rubato / ReplayGain / EQ / device choice / AAudio queue / gapless
  isomp4 / tag writing. (MEDIA9)
- AzKeys: PGP, QR scan, KDBX, age backup, several vaults, zxcvbn, autofill. (KEYS9)
- AzClock: drag reorder (ListView v2), custom sounds, sync, phone layout. (CLOCK9)
- AzMonitor: process details, suspend / resume, priority, tree rows, column chooser, Startup / Services / Users
  tabs, per-process net / GPU, compact / tray mode. (MONITOR9)
- AzERP: ledger posting, PDF register, barcodes, non-January fiscal year, the other ERP sections. (ERP9)
- AzPdf: tiles for deep zoom, a text layer / annotations / forms / print / passwords. (PDF9)
- A shared ReaderView widget (AzMail sanitizer + AzReader content.rs + AzNews reader). (READER9)

## Other repos / user decisions / tooling
- printpdf (the user's crate): cubic curves written as Q + L, the `cm` transform offset, colour spaces / inline
  images / shadings skipped, /Rotate ignored, a parse panic aborts the app (fuzz it). (PDF9)
- micromail: `format_dkim_dns_record` writes PKCS#1 instead of SPKI; a structured "unreachable" status. (MAIL9)
- CI: push only master + a draft gate (waiting for the user's yes). (CI cost)
- mini-mail-auth 0.1.1 unpublished (AzMail does not need it). (FOLLOW-UPS)
- Uuid::v4 is a deterministic marker mint under a "v4" name: rename it or keep it (an API naming decision; no app
  calls it any more). (DEDUP_OFFICE D7)
- A doc-drift check in the autofix scan (refresh api.json docs from the source). (integration 2026-10-05)
- autofix re-export resolution does not follow renames, and reads a pub(crate) item behind a glob as public (no
  case today). (TOOLS7)
- The lint cleanup wave (unused_qualifications / trivial_casts / unreachable_pub in parser2.rs, layout, dll).
  (FOLLOW-UPS)
- PGO + order file run (scripts/pgo/chain.sh); BOLT for the Linux .so in CI; give back allocator slack after
  startup. (H)
- Recommendability: a release, the "dashboard over a big spreadsheet" tutorial. (I)
- ../azul-apps: 5 local planning commits, push only if the user asks. (MONDAY_RESUME)

## Verification runs owed (not code)
- The suites (scripts/waves/tools/suites.sh), every E2E script (wait_settled before screenshots), the mail corpus,
  the WPT bless (and whether whitespace-001 / anonymous-table-ws-001 pass), pdf_chrome_probe.py, WAYLAND8 on Linux
  (KDE 6.7, X11, ssh -X), the SYSUI8 look compare (record + compare, then fix the widget CSS it flags). (A, MONDAY)
- LOOK at every app in flat / flora x light / dark (PIM6, SHEETSHOW6, MEDIA6, AzContacts, AzSetup checkbox row,
  AzShells S11 tab labels). (A)
- VERIFY items fixed blind: Ctrl+C after a calculator key, Cmd/Ctrl+A/C/X/V on a non-editable focus (azdrive step
  16), the too-wide line overflows at its end. (A)
- MAILENG6: the PANE_BASE / FILL_COLUMN_BASE / split-pane percentage heights still fill their panes. (B)
- macOS device checks: Edit-menu Cmd+Z through key handlers (EVENTS7), non-key popups (S2_FOCUS_LEFTOVERS), the
  Graphite accent in the Dark segment. The instrumented AZ_E2E host was not re-run after the exit fix. (various)
- ReferencePicker focus restore after the app's rebuild (live). (WIDGETS9B)

---------------------------------------------------------------------------------------------------------------------

# DONE (evidence)

## Ledger A / F (integration, apps)
- The first compile of waves 6-9: the dylib + all 35 apps build (34047f48f, 83d5c2716).
- WRITER6 ids as const AzString: 606addf10 (writer), 5ae4b1a87 (notes).
- AzReview MouseOver -> MouseMove: 6c823ec53 (RED 1799a0c56); no MouseOver left in examples/.
- PIM6 prefixes `__azcal_` / `__aztasks_` / `__azcontacts_`: the `names!` const AzString in each ids.rs (PIMDRIVE7).
- TextRasterStyle in module image: 5c2d874a6 (api.json `image`).
- AzMail mail files on one Drive at the data root, ScopedDrive per account: 1dd27a351, 92631d148 (store.rs:16-25).
- AzWriter import on a Thread: 59bdedafa; Update::max_self used, commands::merge gone: 21547860e.
- AzSheets Replace inside the grid's edit: ef0292aa6; Format Cells = one undo step: dd6ab7d69.
- AzShow drop line: 3cb72aa9b; tables edited in place: dc308942f; presenter window on a chosen monitor: a1a67ea16.
- AzTasks month grid + board, AzCalendar start through the Drive, AzMeet chat on rejoin, E2E on azlin_e2e.py:
  PIMDRIVE7 (merged).
- AzTasks blank-on-click: did not reproduce (PIMDRIVE7). Backstage TextInput garble: did not reproduce (WIDGETS7).
- Leftover paint after a runtime theme switch: 53f1b748b `release_undriven_animation_values` (PAINT7).
- AzMail close flag -> prevent_window_close: 7b777cfcb; AzNotes: 880dc5984; FullWindowState::close_callback
  removed: a669d42eb.
- Writer / Sheets / Show / Photo / Calendar editor / Code ask "save?" on close (CloseGuard: writer lib.rs:216,
  sheets lib.rs:2148, show lib.rs:188, photo commands.rs:168, calendar editor_ui.rs:204).
- AzShow listing past 1000 keys: bb1fbb5fd (`ops::list_all`, storage.rs:151).
- Show / VideoCut / Photo data root through the kit (Azlin).
- Dialogs: VideoCut AboutDialog / ProgressDialog, Drive MessageBox / ProgressDialog, Sheets FindReplaceDialog,
  Photo Dialog + AboutDialog.
- AzWriter + AzNotes on RichTextEditor (writer pages.rs:69, notes editor.rs:1); AzMail editor.rs (sync_text) gone.
- PIM wave-6 list: the AzMail To-Do bar on the task store (todo.rs), VTODO import / export (tasks vtodo.rs),
  AzCalendar writes on a Thread (writes.rs), AzMeet on azul_pim::initials, AzNotes on shared search / tags, appkit
  re-exports the id mint.
- APIEXPORT wave-6 list: the AzSheets zoom slider hook (lib.rs:1830), VideoCut fit_within -> RawImage::fit_within.
- AzMail's own char-ref decoder gone: 1ff01f689. AzMail unfetched <img> hole: MAILHTML. Sent mail -> stale/:
  a9e67753b + 6420d63ab. Ctrl+B with no selection reported: TEXTENG runs + EVENTS7 TypingStyleChanged.
- AzMail mail classes behind a per-message prefix (html.rs:29, `class_prefix`).
- Uuid data loss (D7): no Uuid::v4 call left in any app (d86ef7c6f, azul_storage::ids::random_seed + from_seed).

## Ledger B (engine text / layout)
- text-indent narrows the first line: b5728f613, f805f5b0a, 64d6ee9c5 (a_text_indent_narrows_the_first_line.rs).
- A block inside an inline: 3995cb27f (a_block_inside_an_inline_splits_the_inline_around_it.rs).
- A run shaped before its font loads: ca05d694c (+ SYSUI8 c4ebf6301 next-face fallback).
- bolder / lighter relative: 0a0093605. dll font-index off-by-one: 397eb4432 (layout.rs:1572).
- min-width:100% border-box inline-block: d6016967a. width: fit-content: cc44b5c48. display:table keeps a <p>'s
  margins: dd3412ea4.
- AzMail wizard stops at page 2 (RED cc7040ae5): 3ccc3c8c5 (core diff.rs container_identities).
- An absolutely positioned child in-flow + ::marker with list-style none: 92877ff2e, fbef5dcd7, 87cc1c6ae,
  116feca4d.
- A block taller than a page is split (engine): 035ee5157 (the app side is NOT SMALL above).
- overflow:hidden span clips glyphs: 2b6ff6269 (also the scrolled week-view titles).
- inline-block in an inline span sized by its CSS: measure_atomic_inline (fc.rs:11756/11776).
- line-height 19.55 / rem / vw / vh / 20px stack: TEXTENG 4c4ea42d9 (pinned by TEXT7).
- Text after a nested block not indented (layout part): 64d3cb633. vertical-align in vw / vh: 5ede221e7.
- TABLES font bug (a run beside bold / italic dropped): 2b5bae827, un-ignored 291873c42. RTL collapsed borders:
  4b9f52dcf.
- a_narrow_table_wraps_its_cells_to_fit restored (2 baselines per cell): f120ecc14.
- Helvetica / Times / Courier taller lines by a family list: by design (Blink / WebKit do the same, pinned against
  Chrome by a_normal_line_is_as_tall_as_chromes.rs).
- COMPONENTS: ol start / type / reversed, li value: e6f767003, d85dbf66f; list-style-position: inside fc.rs:12286;
  rowspan height distribution fc.rs:9502-9540.
- rgba() space-splitting in box-shadow: 080901e26. :backdrop in stylesheet rules: c8d76b9f9.
- Inline-block line height as Chrome + VirtualView focus = nearest focusable ancestor: RULINGS8 (cffdaeffa,
  91928b72d).
- Mail corpus = Chrome (0 mismatched): MAILREF8 + XML8 (3428223f1). system-ui = the OS UI font: SYSUI8 2ad6a861a.
- Engine backlog 6 LENIENT-XML: XML8 (rule tables). Engine backlog 4 threads: 7c1c12389 + THREADS8.

## Ledger C / D (paint, events)
- Paint order (positioned vs earlier transformed): 0fae6b3d2. Hit test follows the animation transform:
  a3f14f4fb. CSS-id image registration: 54b1200b3. CPU compositor sheet under the scroll layer: 9376e3075.
  Backstage pieces: 53f1b748b. AzTasks duplicated bold text: fea2f1dd1.
- VirtualView events bubble to the parent DOM: 971c2fa9a, 5e56a6547. macOS Edit menu through key handlers:
  76bafe411, b47d4f120. Runner undo / redo arms: 4561f90d6. Headless menus close on outside press / Escape:
  4708e715e.
- Ctrl/Cmd+Z reaches app callbacks: 2ba518aeb. Formatted (HTML) paste: 55ce2dc21, 251ac83f1. The paste parser is
  the lenient one: e354e552d.

## Ledger E (widgets, WIDGETS7)
- CloseGuard asks at close time: b14047275. Placeholder ink: 1a78b80f4. DatePicker fits its pane: fae2d1084.
  ToDoBar week start: 85fe5221f. Slider dark fills (also the flora-dark zoom slider): 5cdb53b49. system: colours in
  the HTML dump (also the check-box colour): cbd4265ee. MessageList -> SummaryList: bf12be9ff.
- AzNotes title font size on the app side: 4ed56d37a (the widget side is 4.1).

## Engine backlog
- 5 idle: FLIP springs settle (ab5c41500); the debug server is event-driven and encodes PNG off the UI thread
  (dcc854f30, 9dae67586, ea1d8434c, 6f36a69de); a monitor change re-paces the frames (b4635a23e).
- 9: the AzWidgets "Dark" segment: 16e5c8f91 (readable_accent_ink).
- 11: the stepper as one spin-button stop is correct (WAI-ARIA; TimePicker a275c42b6); spinner native look
  9531b8726; combobox active-descendant highlight (combobox.rs:1603-1609); non-key popups on macOS / Win32
  (S2_FOCUS_LEFTOVERS); spatnav into hidden scroll containers (already right, focus_cursor.rs:1218);
  get_selection_state affinity c81877d74; headless step() scroll / text input 0813655e1; e2e Dismissed on Escape
  5fd9a69ac; the switch-toggle lag (ANIM8 order-independent hash).
- 14 scheduled notifications: CLOCK9 (Notification.deliver_at).
- PGO: the instrumented-host exit crash's root cause fixed (b872bfa3a, 8e6c5bfb2, 78169f857) + dump_profile
  4a3e172cb.
- ANIM8 perf (each knob frame re-laid out the page): LAYOUTPERF8 / 8B / ANIMFRAME8 (0 layout passes, 0 DL rebuilds
  per tick).

## Ledger G (tooling)
- The type index skips module-private types: 4298fea50 (RED 72d346d4a). One method per new type: f044740e7.
  `--fn` for free functions: cd2b046b9. A bare `object` is critical: e5de1ad90. One rule places a new type:
  5c2d874a6. Private module paths: 2d2e48396, 1b4de30bd. AUTOFIX6's list (borrowed returns, Option / slice
  arguments, accessor templates, is_empty with removals, remove-then-add in one round): 9defb80c5, 981555c7f,
  442116648, e8004ed34, d868a9bfa. Wave-5 gaps 1-6: AUTOFIX6.
- EchoCanceller in module audio (api.json). impl_option! / impl_result! via $crate: 0a98cab8f. One "is a Vec" rule
  (is_vec_family). Only micromail 0.2.0 in Cargo.lock.

## DEDUP / FOLLOW-UPS
- ShellThemeAccent::colors, KeyModifiers / KeyboardState::primary_down, ColorU to_hex / parse_hex,
  RawImage create_rgba8 / resized, DiskSpace::format_bytes, Button disabled / toggled, NodeData get_attribute(s),
  TextAreaState::get_text, GlobalHotkey::matches, DatePicker::with_week_start, StatusBarZoom::create,
  RibbonTab::with_groups / RibbonGroup::with_items, Xml::encode_text: all in api.json.
- No app builds ButtonOnClick by hand any more. ModuleSwitcher merged: 24a191049. decl vs style_kit: c0467267f.
  timeline / cell_grid on decl helpers: 2dd113562. Shared selection: ListSelection (BLOCKS). One undo stack:
  UndoHistory (BLOCKS). Preset-shell setters: BLOCKS. The AzMail HTML tokenizer: MAILHTML (-360 lines).
  Calendar / Tasks one task store: PIM. Quoted attendee: PIM. Ctrl+B/I/U over a selection, Bold toggles: RTE.

---------------------------------------------------------------------------------------------------------------------

# SUITE FAILURES (2026-10-05, `cargo test -p azul-layout --features e2e-server --lib` on 09c9c626f) - assigned
Each: find out whether the TEST or the CODE is wrong (root cause; a widget pin may have moved on purpose, e.g. SYSUI8
made system-ui text 10-14 % wider on macOS); fix that side; say which in the report.
- PKG 2 (window.rs): `window::window_theme_context::a_changed_inline_flex_box_behind_a_block_sibling_keeps_its_slot_and_widens`
  - "one line, as before": height 76 vs 60 (window.rs:31952).
- PKG 3 (+ layout/src/managers/selection.rs): `managers::selection::autotest_generated::to_html_font_family_is_interpolated_raw_into_the_style_attribute`
  - "escaping behaviour changed, re-check the injection note": the font-family is now HTML-escaped
  (`&quot;&gt;&lt;img onerror=x&gt;`) - the escaping is the safe behaviour; update the test + its note (selection.rs:477).
- PKG 4 (widgets):
  - `dom_lint::autotest_generated::every_widget_dom_is_warning_free`: rich_text_editor's text node "Send the invite"
    sits next to block-level siblings (wrap it - the fix is in rich_text_editor.rs, not dom_lint.rs).
  - `widgets::button::autotest_generated::dom_carries_the_container_style_on_the_root_and_the_label_style_on_the_child`
    (button.rs:2102, "Default: the root inline style is not the container style").
  - `widgets::code_view::code_view_tests::the_wheel_scrolls_whole_lines_and_never_past_the_last_line`
    (code_view_tests.rs:693: Some(5) vs Some(99)) - PKG 4 may touch layout/src/widgets/{code_view.rs, code_view_tests.rs}.
  - `widgets::date_range_picker::dom_tests::a_pinned_date_range_picker_keeps_its_theme_invariants`: root/1/0/0/2/1/2
    `-azul-box-shadow-bottom` Focus (light + dark) shadowed by a later resting declaration.

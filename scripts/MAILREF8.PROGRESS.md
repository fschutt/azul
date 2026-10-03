# MAILREF8 progress (wave 8, 2026-10-03)

Branch `wt/mailref8` from `45c6bf98b` (wave 7 integrated). Brief: scripts/waves/wave8/PLAN.md "MAILREF8".
Never compile; never touch layout/src/solver3/page_breaks.rs (nor display_list.rs: WPT8 owns it).

## DONE
- step 1: BEFORE = 531 mismatched boxes (18 mails, 0 errors; Chrome 154, AzPaint + azmail-sanitize of
  45c6bf98b's build, 760x1100). Output /Users/fschutt/Development/azul/target/refci/mail-wave8-base.
  Command (from the worktree): AZUL_APP=<main>/target/release/AzPaint AZUL_LIB_DIR=<main>/target/azul-lib
  AZMAIL_SANITIZE=<main>/target/release/azmail-sanitize run_capped.sh --cap-mb 3500 --seconds 900 --log ..
  -- python3 scripts/refci/mail_boxes.py --out <main>/target/refci/mail-wave8-base
  Per mail: cerberus fluid 78, hybrid 165, responsive 154; 03_outlook 18; 04_receipt 16; leemunroe 1;
  mailgun billing 26; postmark invoice 32, receipt 41; all others 0.

## Groups (largest first; first guesses, to be confirmed by probes)
- A (~397, cerberus x3): AzMail's paper is `display:inline-block; height:100%`. Chrome puts it 14px
  low (y 22): its baseline is the preheader's (max-height:0; overflow:hidden -> bottom margin edge)
  because tables are SKIPPED when an inline-block looks for its last line box (Blink
  LayoutTable::InlineBlockBaseline = -1 / LayoutNG). azul: y 8. Also the paper's height (azul 682,
  Chrome 1054 = content): percentage height of an inline-block in an auto-height body = auto.
- B (~73, postmark invoice + receipt): one inner table (azr-54 / azr-44): tbody 183 wide, rows'
  cells stacked vertically.
- C (26, mailgun billing): a row 412 tall (Chrome 323) -> +90 below.
- D (18, 03_outlook_reply): <p> 17 tall vs 16 (font / line-height of the reply's paragraphs).
- E (16, 04_receipt): rows 31 vs 34, <hr> 1 vs 2 tall.
- F (1, leemunroe): an inline-block a x +4.

- group A RED: ff0a40de8 (layout/tests/an_inline_blocks_baseline_is_its_last_line_box.rs,
  layout/tests/a_percentage_height_inline_block_in_an_auto_height_body_is_as_tall_as_its_content.rs).
- group A GREEN part 1 (WIP, committed after ff0a40de8): ONE helper
  `sizing::percentage_height_computes_to_auto(Option<&LayoutHeight>, cb_definite)`; `MultiValue::as_exact`
  (getters.rs); sizing `calculate_used_size_for_node` maps such a % height to Auto; cache.rs Phase 2.5 and
  `prepare_layout_context` `height_is_auto` use it (old `cache::is_percentage_height` deleted); fc.rs
  `layout_bfc` children_containing_block_size `height_is_auto` uses it with
  `constraints.containing_block_size.height.is_finite()`.

- group A GREEN part 2: 6813ac111 (steps 1+2 below DONE: `sizing::height_is_auto_for_children` used by
  cache.rs prepare_layout_context + fc.rs layout_bfc; measure_atomic_inline % -> content height).

## NEXT (exact)
1. DONE - Table guard (decided: table boxes keep their used height for their children, CSS 2.2 17.5.3): in
   cache.rs `prepare_layout_context` and fc.rs `layout_bfc` only apply the % -> auto rule when
   `node.formatting_context` is NOT Table / TableRowGroup / TableRow / TableCell / TableColumnGroup /
   TableCaption. Do it with ONE helper next to `percentage_height_computes_to_auto` in sizing.rs
   (e.g. `height_is_auto_for_children(fc, height, cb_definite)`), used by both sites.
2. DONE - fc.rs `measure_atomic_inline`: `final_height` match - a % height with
   `atomic_inline_containing_block(constraints).height` not finite goes the `Auto` (content) arm.
3. DONE 5bc033816 - Baseline: text3 cache.rs new `UnifiedLayout::last_line_baseline()` (positioned: items of the max
   line_index; prefer baseline-aligned (`get_item_vertical_align` None/Baseline); cluster with glyphs ->
   position.y + get_item_vertical_metrics_approx().0; Object/CombinedBlock -> position.y + (bounds.height -
   baseline_offset)). fc.rs new `inline_block_baseline(index, tree, depth) -> Option<f32>` next to
   `first_line_baseline` (from the BORDER-box top): IFC root -> content_top + last_line_baseline; else
   children in REVERSE, skip out-of-flow (abspos/fixed/float via warm.computed_style), skip
   FormattingContext::Table, overflow_x/y != Visible -> content_top + child_top + used height + margin
   bottom, Flex/Grid -> first_line_baseline(child), else recurse. measure_atomic_inline: for FC not
   Table/Flex/Grid use `inline_block_baseline(child_index, tree, 0).map(|b| b - (padding.top +
   border.top))` instead of `layout_result.output.baseline`.
4. DONE (no engine fix) - group B ROOT CAUSE = HTML foster parenting, not layout: the templates'
   `{{#each receipt_details}}` / `{{/each}}` text sits between `<tr>`s. Chrome's HTML parser fosters it
   BEFORE the table (one 18px line above the inner table in the td); azul's `Xml::create_from_html`
   (core/src/xml_html.rs, header: "foster parenting ... stays where it is") keeps it in the tbody, where
   the layout correctly wraps it in an anonymous row + cell (Chrome lays the same DISPLAY-based table out
   exactly like azul: probe target/mailref8/b1.json css_table_stray_text). Owner: XML8 (its brief lists
   table foster parenting). Expected effect when XML8 lands it: ~73 boxes (invoice 32, receipt 41).
   (The mail_boxes 'tbody 183 wide / stacked cells' rows were the measure pairing azul's anonymous boxes.)
5. DONE group C: RED 8257b045a (layout/tests/a_cells_row_is_as_tall_as_its_content_at_the_column_width.rs),
   GREEN 531e3a19e (fc.rs layout_cell_for_height block branch: content height = the final layout's
   overflow_content_size only; the `measured` (min-content measurement used_size) term dropped).
   Expected: mailgun billing 26 -> ~0.
6. DONE (no engine fix, DECIDED) group D: ROOT CAUSE = the generic `sans-serif` on macOS. Chrome maps it
   to Helvetica (Blink's Mac default), azul to Helvetica Neue: rust-fontconfig 5.0.0 (crates.io)
   `FcFallbackConfig::os_defaults` MacOS lists ["Helvetica Neue", "Helvetica", "Lucida Grande"]. Calibri
   is not installed, so "Calibri",sans-serif at 11pt = Helvetica 16px lines in Chrome, Helvetica Neue
   17px in azul (probe target/mailref8/d1.json: widths 125.52 vs 126.83 identify the faces). Not changed:
   it is the external crate's per-OS table and a global look change for every app's `sans-serif`; the
   fix is one line there (Helvetica first on macOS) or `FcFontCache::set_fallback_config` at startup -
   listed for the user in the report. Also seen: a family that is missing with NO generic falls back to
   sans-serif in azul, to the standard font (Times) in Chrome.
7. group E: E1 (IFC height = glyph lines' line boxes incl. the strut): RED 7c0619af8
   (layout/tests/a_line_of_small_text_is_as_tall_as_its_line_box.rs), GREEN 7c9224764 (text3 cache.rs
   perform_fragment_layout: `glyph_line_box_top` + `line_box_extent` for lines holding glyphs; atomic-only
   lines unchanged). E2 DECIDED not changed: `<hr>` is 1px (border-top only, width 100%) in core/src/
   ua_css.rs - a deliberate azul UA choice pinned by ua_css_test `hr_line_comes_from_the_border_not_from_
   height`; HTML 15.3.11 has a 1px inset border on all four sides (2px tall, width auto). 1px per hr is
   inside the measure's tolerance; listed for the user.
8. DONE group F: RED 37094ca16 (layout/tests/spaces_at_a_lines_edges_do_not_widen_its_max_content.rs),
   GREEN cca5b355e (text3 measure_intrinsic_widths: collapsing modes skip leading collapsible spaces and
   measure a line to its last non-space item).
9. DONE group G (residual of cerberus hybrid after A, ~45 boxes): atomic inlines in an RTL line were not
   reordered (L2 reversed clusters only). RED abab33812 (layout/tests/inline_blocks_in_a_right_to_left_
   line_run_from_the_right.rs), GREEN d63d0025d (text3 apply_l2_visual_reversal(line_items, base):
   Objects resolve N1/N2 from neighbour clusters / base). Hybrid's other residual (+5 from azr-113: a
   font-size:0 cell 10px too tall, content centred) is the group C cause (measured term) - covered by
   531e3a19e (probe target/mailref8/h1.json: cell 185 vs 175).
10. DONE: report scripts/MAILREF8_2026_10_03.md.
11. NEXT (optional, only if resumed with time): nothing required; candidates listed in the report's
    section 10. Old note: write the report scripts/MAILREF8_2026_10_03.md (before 531; groups A-F with owners, expected
   effects; commits; api.json none; least-sure spots; test commands). Then, if time: atomic-only line
   strut (look pass) is NOT for this wave; maybe re-check other mismatch sources in cerberus after A
   (hybrid had 6 boxes not in the -14 pattern?).
- Group A expected effect (after the parent's build): cerberus x3 ~390 boxes y -14 -> 0, azr-1/azr-2
  heights fixed (paper = content). Left in A: an IFC's height is its items' bounds, not its line boxes
  (strut descent below an inline-block: t1 wrap 46 vs Chrome 60, t3 30 vs 34) - text3 cache.rs
  ~11660 (LAYOUT7 'seen broken', needs a LOOK pass); not compared by mail_boxes (no azr box).

## Decisions
- (none yet)

## Open questions
- (none yet)

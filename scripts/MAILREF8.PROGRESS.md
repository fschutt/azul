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

## NEXT (exact)
1. Table guard (decided: table boxes keep their used height for their children, CSS 2.2 17.5.3): in
   cache.rs `prepare_layout_context` and fc.rs `layout_bfc` only apply the % -> auto rule when
   `node.formatting_context` is NOT Table / TableRowGroup / TableRow / TableCell / TableColumnGroup /
   TableCaption. Do it with ONE helper next to `percentage_height_computes_to_auto` in sizing.rs
   (e.g. `height_is_auto_for_children(fc, height, cb_definite)`), used by both sites.
2. fc.rs `measure_atomic_inline`: `final_height` match - a % height with
   `atomic_inline_containing_block(constraints).height` not finite goes the `Auto` (content) arm.
3. Baseline: text3 cache.rs new `UnifiedLayout::last_line_baseline()` (positioned: items of the max
   line_index; prefer baseline-aligned (`get_item_vertical_align` None/Baseline); cluster with glyphs ->
   position.y + get_item_vertical_metrics_approx().0; Object/CombinedBlock -> position.y + (bounds.height -
   baseline_offset)). fc.rs new `inline_block_baseline(index, tree, depth) -> Option<f32>` next to
   `first_line_baseline` (from the BORDER-box top): IFC root -> content_top + last_line_baseline; else
   children in REVERSE, skip out-of-flow (abspos/fixed/float via warm.computed_style), skip
   FormattingContext::Table, overflow_x/y != Visible -> content_top + child_top + used height + margin
   bottom, Flex/Grid -> first_line_baseline(child), else recurse. measure_atomic_inline: for FC not
   Table/Flex/Grid use `inline_block_baseline(child_index, tree, 0).map(|b| b - (padding.top +
   border.top))` instead of `layout_result.output.baseline`.
4. rustfmt --check the edited files, commit GREEN, then group B (postmark inner table azr-54/44).

## Decisions
- (none yet)

## Open questions
- (none yet)

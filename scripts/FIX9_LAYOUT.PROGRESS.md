# FIX9-LAYOUT progress (wave 9, PKG 1 ENGINE-LAYOUT-TEXT)

Branch: wt/fix9-layout (fast-forwarded to b454da215). Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 1".
Files: layout/src/solver3/{fc.rs, sizing.rs, taffy_bridge.rs, cache.rs, mod.rs}, layout/src/text3/{cache.rs,
knuth_plass.rs}, core/src/{xml.rs, xml_attributes.rs}. No cargo.

## DONE
- 1.1 RED e5a290e7d, GREEN b7b4187ce (sizing.rs mod anonymous_ifc_intrinsic_tests; fc helpers
  ifc_root_style_dom_id / anonymous_block_holds_the_first_line). Note: triage said "sizing adds the indent
  for every IFC" - actually it added NONE for any anonymous block (no dom id) and measured white-space normal;
  the "after a nested block" test is a pin, the RED is the first anonymous block + nowrap.

- 1.2 RED f3134cb66 (+ test fix cfb0916b0: span font-size + padding, since window's begin_reconciliation
  classifies a font-size-only change as paint-only), GREEN df8f3b7bd (fc.rs mod window_layout_tests;
  fc::hash_resolved_style).

- 1.3 RED 07ef25280, GREEN 2f1e76da4 (knuth_plass tests; starts_paragraph param on kp_layout /
  find_optimal_breakpoints / position_lines_from_breaks).

- 1.4 RED 471d463ab, GREEN 1b580afc3 (fc.rs window_layout_tests; layout_bfc marker branch before the
  position/float checks, content = max(content, marker) like Chrome - NOT "no contribution" as the brief
  said: an empty li keeps its marker height; sizing block intrinsic max; cache.rs skips every marker).

- 1.5 GREEN 4283694b6 (RED existed: layout/tests/a_stretched_flex_container_keeps_its_min_height.rs).

- 1.6 RED aaaa87196, GREEN 97b2a05c9 + 0edd94e79 (reconcile_and_invalidate_restyled + css_relayout;
  Step 1.15 Full -> parent for block flow; outermost_layout_roots; e2e json expect pass).

## IN PROGRESS
1.7

## NEXT
1.7 RED an_empty_inline_with_padding_is_as_tall_as_its_strut (fc.rs ~11589 empty inline InlineShape)

## Open questions
(none)

# FIX9-LAYOUT progress (wave 9, PKG 1 ENGINE-LAYOUT-TEXT)

Branch: wt/fix9-layout (fast-forwarded to b454da215). Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 1".
Files: layout/src/solver3/{fc.rs, sizing.rs, taffy_bridge.rs, cache.rs, mod.rs}, layout/src/text3/{cache.rs,
knuth_plass.rs}, core/src/{xml.rs, xml_attributes.rs}. No cargo.

## DONE
- 1.1 RED e5a290e7d, GREEN b7b4187ce (sizing.rs mod anonymous_ifc_intrinsic_tests; fc helpers
  ifc_root_style_dom_id / anonymous_block_holds_the_first_line). Note: triage said "sizing adds the indent
  for every IFC" - actually it added NONE for any anonymous block (no dom id) and measured white-space normal;
  the "after a nested block" test is a pin, the RED is the first anonymous block + nowrap.

- 1.2 RED f3134cb66, GREEN df8f3b7bd (fc.rs mod inline_collection_cache_tests; fc::hash_resolved_style).
  Unverified symptom: parent runs the RED at f3134cb66; if it passes there, revert df8f3b7bd.

- 1.3 RED 07ef25280, GREEN 2f1e76da4 (knuth_plass tests; starts_paragraph param on kp_layout /
  find_optimal_breakpoints / position_lines_from_breaks).

- 1.4 RED 471d463ab, GREEN 1b580afc3 (fc.rs window_layout_tests; layout_bfc marker branch before the
  position/float checks, content = max(content, marker) like Chrome - NOT "no contribution" as the brief
  said: an empty li keeps its marker height; sizing block intrinsic max; cache.rs skips every marker).

## IN PROGRESS
1.5

## NEXT
1.5 GREEN for layout/tests/a_stretched_flex_container_keeps_its_min_height.rs (taffy_bridge.rs ~1501)

## Open questions
(none)

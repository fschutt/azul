# FIX9-LAYOUT progress (wave 9, PKG 1 ENGINE-LAYOUT-TEXT)

Branch: wt/fix9-layout (fast-forwarded to b454da215). Brief: scripts/waves/wave9/SMALL_FIXES.md "PKG 1".
Files: layout/src/solver3/{fc.rs, sizing.rs, taffy_bridge.rs, cache.rs, mod.rs}, layout/src/text3/{cache.rs,
knuth_plass.rs}, core/src/{xml.rs, xml_attributes.rs}. No cargo.

## DONE
- 1.1 RED e5a290e7d, GREEN b7b4187ce (sizing.rs mod anonymous_ifc_intrinsic_tests; fc helpers
  ifc_root_style_dom_id / anonymous_block_holds_the_first_line). Note: triage said "sizing adds the indent
  for every IFC" - actually it added NONE for any anonymous block (no dom id) and measured white-space normal;
  the "after a nested block" test is a pin, the RED is the first anonymous block + nowrap.

## IN PROGRESS
1.2

## NEXT
1.2 RED a_stylesheet_only_font_size_change_relays_out_its_text (fc.rs fingerprint ~4210)

## Open questions
(none)

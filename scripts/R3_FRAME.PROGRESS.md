# R3-FRAME progress (wave 9 round 3, branch wt/r3-frame, base 6a39b7f1a)

## DONE
- (none yet)

## IN PROGRESS
- diagnosis done for all six; committing fixes one per test

## NEXT
1. struct_sizes: LayoutNodeCold 288 -> 296 (UnresolvedBoxProps zoom + root_zoom, aa4a3f9e1) - keep, update pin
2. struct_sizes: StyleProperties 248 -> 256 (InlineBorderInfo margin_left / margin_right, 753dbc393) - keep, update pin
3. text_after_a_block_is_carried_over_by_the_next_layout: the Step 1.1 display-list cache hit leaves
   last_intrinsic_dirty from an earlier pass (the cold pass's 7) - CODE, solver3/mod.rs
4. a_knob_frame_costs_the_same_on_a_page_twice_as_long: the layout pass drops the probe buffer an explicit
   consumer (set_recording(true): tests, telemetry) drains - CODE, probe.rs + window.rs
5. tweens_that_restyle_share_one_cascade_refresh_per_frame: opacity tweens are GPU values since FIX9 2.8 -
   TEST, use two width tweens
6. every_published_patch_leaves_the_screen_reader_holding_the_fresh_tree: the a11y pass runs before
   register_scroll_nodes (one pass stale scroll state) - CODE, window.rs funnel order
7. report scripts/R3_FRAME_2026_10_05.md

## Open questions
- none

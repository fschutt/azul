# R3-FRAME progress (wave 9 round 3, branch wt/r3-frame, base 6a39b7f1a)

## DONE
- 81a7a4396 struct_sizes::layout_tree_node_struct_sizes_are_pinned - TEST pin: LayoutNodeCold 296 (CSS zoom, aa4a3f9e1)
- 9f26ca1ac struct_sizes::inline_pipeline_struct_sizes_are_pinned - TEST pin: StyleProperties 256 (inline margins, 753dbc393)
- 985535765 text_after_a_block_is_carried_over_by_the_next_layout - CODE: the Step 1.1 DL cache hit sets
  last_intrinsic_dirty = 0 (solver3/mod.rs)
- 7b621fdc0 a_knob_frame_costs_the_same_on_a_page_twice_as_long - CODE: a layout pass leaves the probe
  buffer to a caller that set_recording(true) (probe.rs, window.rs)
- fe90b5b1c tweens_that_restyle_share_one_cascade_refresh_per_frame - TEST: width tweens (opacity is a GPU
  value since FIX9 2.8)
- 21aa81ada every_published_patch_leaves_the_screen_reader_holding_the_fresh_tree - CODE: update_a11y_tree
  after register_scroll_nodes + the caret reveal (window.rs)

## IN PROGRESS
- (none)

## NEXT
- done: report scripts/R3_FRAME_2026_10_05.md committed

## Open questions
- none (unverified without cargo: the knob test's count bounds once its spans are visible)

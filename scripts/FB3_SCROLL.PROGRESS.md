# FB3_SCROLL progress

Branch `wt/fb3-scroll-damage` from `5bab5dfa2`. Never compiles; runs prebuilt apps only through
`scratchpad/run_capped.sh` (one at a time, killed when done). Report: `scripts/FB3_SCROLL_2026_09_30.md`.

## DONE
- b42dcdc04 progress checkpoint (findings before the reboot)
- 7358f2c49 RED item 1: `layout/tests/a_scrolled_virtual_view_is_repainted_where_its_content_moved.rs`
  (item comparison + backends recipe over 11 steps vs a full render; S1 `Raster` gains
  `full_state`/`repaint_state`) and dll headless
  `a_virtual_view_scrolled_on_the_lightweight_path_paints_where_its_content_moved` (real ScrollTo
  through apply_user_change + service_frame, 10 steps; `full_repaint_of` split out of
  `incremental_vs_full`, `pixel_of` out of `sample_px`).
- 6df297b21 FIX item 1: `DisplayListItem::is_visually_equal` compares `VirtualView::content_offset`.
- 7cafda46f progress
- 84f4b61dd RED item 3(b): raster test `a_runs_ink_outside_its_own_clip_rect_is_cut_on_every_paint_path`.
- 52d6d6211 FIX item 3(b): one `text_run_clip` (clip_rect cut to the stack clip) for the sweep and
  grayscale paths in `render_text`.
- 17b01320c RED item 3(a): gpu_state unit test
  `remap_node_ids_moves_the_animation_channel_with_its_node_and_drops_the_unmounted` + headless
  `a_node_that_inherits_the_id_of_an_animated_node_is_not_painted_through_its_transform`.
- b1bd7d9d5 FIX item 3(a): `GpuStateManager::remap_node_ids` remaps the four `anim_*` maps.
- 58f2b0eee report + progress.
- 2af1e228d FIX item 3(b), second half (parent's suite: the tile path's pass-2a sweep got the stack
  clip alone; now the combined `run_clip`). VirtualView scroll and anim-map tests passed on the
  parent's run.
- Item 2 (viewport leftovers): all five are S1's, integrated in the base (their tests are registered
  in all.rs, none ignored). Verified by reading; nothing to change. See the report.

## IN PROGRESS
- (none)

## NEXT
- Report committed; parent compiles + runs the commands in the report.

## Open questions
- AzReview in target/release (20:58) predates libazul.dylib (21:31): relink before further probes.

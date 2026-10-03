# PAINT7 progress (wave 7, 2026-10-03)

Branch `wt/paint7` from `2e55eef06`. Brief: scripts/waves/wave7/PAINT7.md.
Commit messages are written to scripts/.paint7_msg.txt (untracked, never staged).

## DONE
- a96250829 progress file
- 8428d1fca RED item 1: layout/tests/a_positioned_box_paints_in_tree_order_with_stacking_contexts.rs
  (DL order + layered-compositor pixels)
- 0fae6b3d2 GREEN item 1 (display list): positioned z-auto boxes = `StackingContext::positioned_box`
  entries at step 8 in tree order (`file_into_context`, `node_paints_as_positioned_box`)

- ffc7b4f5d RED flat raster vs compositor for transformed boxes
  (layout/tests/the_incremental_raster_paints_a_transformed_box_where_the_compositor_does.rs)
- 57e91a451 GREEN raster.rs: in-place reference frames (`MaskEntry::Transform`, `ReferenceFrameGroup`,
  `TransformGroup`; translation -> scroll-offset stack; else isolated + composited through the matrix)
- 8939e7d11 RED layout/tests/a_box_painted_after_a_layer_shows_over_it_in_the_cpu_compositor.rs
- 9376e3075 GREEN compositor.rs: `painted_over_later` - a group painted over later is not promoted
  (in place); `translation_2d` + pub(crate) `is_identity_2d` shared with raster; layer_soup spaced

## IN PROGRESS (old notes)
- item 1/4a CPU side. Findings:
  - the layered compositor composites every child layer AFTER all of its parent's own items
    (render_layers skips child ranges, composite_layer_recursive blits parent then children):
    anything the list paints after a layer, over it, comes out UNDER it (MEETDRIVE6 4a).
  - the FLAT raster (render_single_item; the incremental / damaged path of every CPU backend)
    keeps a transform_stack nothing reads: transformed content is repainted at its LAYOUT place
    in every damage rect (mid-slide ghosts, permanent ghosts for a lasting transform).
  - plan: (a) RED: flat raster paints a transformed box moved (translate + rotate) = what the
    compositor paints; GREEN: in-place reference frames in raster.rs (translation -> scroll-offset
    stack; other affine -> isolated group composited through the matrix, clipped to its bounds
    like the layer). (b) compositor: a layer something later in its parent paints over is painted
    IN PLACE (scroll / opacity / transform; blur stays a layer - note in report).

## NEXT
- item 2 (RED: a node mid-slide is hit where it is painted), then items 2 (hit test anim channel: one helper GpuValueCache::reference_frame_of), 3 (ImageById
  rebuilds the list in window.rs), 4d, 5, 6 (run prebuilt AzTasks/AzCalendar with wait_settled),
  7 (overflow:hidden span clip)

## Decisions / open questions
- D1 paint_in_flow_child's absolute/fixed scroll-chain detour is now unreachable (abs boxes are
  positioned entries, fixed are stacking contexts); left in place.

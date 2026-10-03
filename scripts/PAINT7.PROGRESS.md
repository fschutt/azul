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

- 222cccbec RED item 2: layout/tests/a_node_mid_slide_is_hit_where_it_is_painted.rs
- a3f14f4fb GREEN item 2: GpuValueCache::reference_frame_of / painted_transform_of (core/src/gpu.rs),
  GpuStateManager::painted_transform_of; hit tester chains + all resolve_tf closures + DL use it;
  LayoutWindow::css_transform_of renamed painted_transform_of

- 0a61c5fad RED item 3: layout/tests/a_css_id_image_registration_rebuilds_the_display_list_itself.rs
- 54b1200b3 GREEN item 3: apply_content_change(ImageById) regenerates every dom's display list itself
  (tier RebuildDisplayList = "rebuilt, send it" - the dll/X11 convention)

- 94487d554 RED item 7: layout/tests/a_clipped_box_inside_a_scrolled_frame_shows_all_of_its_lines.rs
  (root cause seen on prebuilt AzCalendar: DL right, compositor's static_clip mixed clip spaces)
- 2b6ff6269 GREEN item 7: Layer::wrapping_clips (clips opened inside the parent layer + their in-place
  frames), static_clip resolved in render_layers, composite uses it as is
- dea8bb74d RED item 6/5: layout/tests/a_sliding_box_moves_its_sliding_children_once.rs (prebuilt AzTasks
  Cmd+2: settled frame right; mid-slide 70 of 87 sliding nodes inside another sliding node, nested frames
  compose -> moved 2x/3x)
- fea2f1dd1 GREEN: LayoutWindow::published_flips - each slide relative to the enclosing sliding frame

- 60e1f3a0d RED 4d: layout/tests/the_animation_channel_holds_only_what_an_animation_drives.rs (prebuilt
  AzDrive: theme switch mid backstage exit -> 15 stale anim values with 0 animations; back button in the
  corner, search box gone)
- 53f1b748b GREEN: LayoutWindow::release_undriven_animation_values (tick, idle early return,
  finish_reconciliation after drop_unplaced)
- WIDGETS7 finding (2) "three one-hour events show only their colour" = item 7, fixed by 2b6ff6269.
- WIDGETS7 finding (1) AzCalendar backstage dark: NOT paint, NOT layout: the DL paints the backstage
  content WHITE (#ffffff rects for nodes 9/10 `__azul-native-backstage __azul-theme-flat` and 33/34
  `-right`/`-content`) and the 28px page heading "Calendars" in #ffffffd8 -> white on white. Widget
  styling (BackstageTheme content_bg WHITE wins over the dark mode); note for the widgets owner.

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
- 4d: AzDrive Options -> Escape -> set_theme flora + set_mode dark -> wait_settled -> shot (probe in
  /tmp/paint7/probe.py); 5: AzShow slide sorter / tab switch settled shots; AzCalendar backstage nav
  item before the gap. Then the report. (run prebuilt AzTasks/AzCalendar with wait_settled),
  7 (overflow:hidden span clip)

## Decisions / open questions
- D1 paint_in_flow_child's absolute/fixed scroll-chain detour is now unreachable (abs boxes are
  positioned entries, fixed are stacking contexts); left in place.

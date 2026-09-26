# Viewport scrollbar painting (2026-09-26)

Worktree branch `wt/viewport-scrollbar` (worktree
`.claude/worktrees/agent-ad7cf34be0a4bfeec`, starts at 97f211f96),
**uncompiled**:

```
9a0f13b4c fix(scroll): the viewport's scrollbar is one bar, sized from the layout it paints
c57383fab test(scroll): the viewport's scrollbar is painted, and is one bar for every consumer
```

Context: 443def41f made the viewport scroll what the root overflows;
e0dedb898 stopped painting the viewport's bar because
`real_ribbon_resize_sweep` diverged by 6 px at width 705 (first pixel 685,257).

## Mechanism (from reading, not measured)

The damage path was never the cause. Every sweep step shrinks the window,
and a shrink repaints everything (dll/.../headless/mod.rs:693, :761).

- **Stale input:** `paint_scrollbars` sized the thumb from `scroll_offsets`
  (display_list.rs:7376-7387), a ScrollManager snapshot taken BEFORE layout
  (window.rs:5955); this pass's `register_scroll_nodes` publishes afterwards
  (window.rs:2543).
- For the root, registration publishes content grown to the root's margin box
  (scroll_registration.rs:112, :248); the DL fallback is the bare content, so
  the incremental window sized the thumb for content at the previous width
  while a fresh window's first pass had no snapshot.
- 705 is the first width reading the snapshot published at 718 (ribbon
  breakpoint 720). Bar at 705 - 8 - 12 = 685; the shorter thumb's rounded end
  = 2x3 antialiased pixels (inferred).

Other disagreements about the same bar: painted along the root's box (page
tall, with buttons) but hit-tested at the window edge without buttons
(scroll_state.rs `calculate_scrollbar_state_from_geometry`); the GPU thumb
updater compared the root box with its own content and never moved the thumb
(gpu_state.rs:312); scrollbar items carry no layout tag
(display_list.rs:5898/6674) so patched-build damage (solver3/mod.rs:1821)
never included a bar; presses were hit-tested in reverse node order so the
viewport bar (node 0) was tried last.

## RED commit (expected failures, 12 px classic bar)

`layout/tests/viewport_scrollbar.rs` (registered in all.rs) + the early return removed:
- `a_relayout_paints_the_viewport_bar_a_fresh_window_paints`: thumb ~561 vs 576 px (content 616 vs 600).
- `the_viewport_bar_is_painted_where_the_pointer_finds_it`: painted 12x600 at (380,8) vs hit-tested 12x300 at (388,0).
- `the_viewport_thumb_follows_the_page_to_the_bottom`: "painted at y=20 but the pointer finds it at y~153.9".
- `a_press_on_the_viewport_bar_is_not_taken_by_a_bar_under_it`: hits (ROOT,1) not (ROOT,0).
- `a_patched_pass_damages_the_viewport_bar_it_changed`: "its damage [] leaves the old bar ... stale" (retries 3x: `dl_patch_golden` flips a process-wide switch).
- dll: `REAL ribbon diverges from fresh render at width 705: 6 px, first Some((685, 257))`.

## FIX commit

Shared helpers in scrollbar.rs (`is_viewport_scroller`,
`viewport_scroll_extent`) + `LayoutTree::scroll_extent`, used by the
scrollbar decision (cache.rs), registration, painting and the GPU updater.
The viewport bar is painted along the viewport edge as an overlay without
buttons, sized from this layout. Registration uses the viewport the layout was
solved in (not `current_window_state`, stale during a resize) and stops
counting the root's borders twice. Patched builds damage every scrollbar
whose drawing changed (`changed_scrollbar_damage`). `hit_test_scrollbars`
tries the viewport's bar first. Touches `layout/src/managers/gpu_state.rs`
outside the briefed dirs.

Unsure to compile: `pub const fn viewport_scroll_extent` (const `f32::max`,
already used at geometry.rs:849); nested `fn bars(..) -> impl Iterator<Item =
&ScrollbarDrawInfo> + '_` with `Some(&**info)`; closure patterns in
`hit_test_scrollbars`. Risk: pixel tests of pages taller than the window now
show a 12 px bar at the window's right edge - run the dll suite.

## Gaps found, NOT fixed

1. **The page content does not scroll.** The root never gets a scroll id or
   scroll frame: `compute_scroll_ids` (window.rs:12193) and `push_node_clips`
   read the root's own `overflow: visible` (the viewport rule of CSS Overflow
   3 §3.3 is applied only at the scrollport decision). The wheel / a thumb
   drag moves the scroll offset and the thumb, not the page.
2. **Classic bars overshoot:** `update_scrollbar_transforms` decides the
   vertical bar's buttons from `scrollbar_height` (looks swapped); a
   vertical-only classic bar can overshoot into its bottom button by up to
   24*(1-ratio) px at full scroll. All classic bars.
3. Other scroll containers still size their thumb from the pre-layout snapshot
   (one pass lag); left alone because the caret gutter only exists in the
   published size.
4. Theoretical blit risk: the move-blit could drag bar pixels over a mover
   moving by a different amount.

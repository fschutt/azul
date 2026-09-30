# FB3_SCROLL progress

Branch `wt/fb3-scroll-damage` from `5bab5dfa2`. Never compiles; runs prebuilt apps only through
`scratchpad/run_capped.sh` (one at a time, killed when done).

## DONE
- b42dcdc04 progress checkpoint (findings before the reboot)
- 7358f2c49 RED item 1: `layout/tests/a_scrolled_virtual_view_is_repainted_where_its_content_moved.rs`
  (item comparison + backends recipe over 11 steps vs a full render; S1 `Raster` gains
  `full_state`/`repaint_state`) and dll headless
  `a_virtual_view_scrolled_on_the_lightweight_path_paints_where_its_content_moved` (real ScrollTo
  through apply_user_change + service_frame, 10 steps; `full_repaint_of` split out of
  `incremental_vs_full`, `pixel_of` out of `sample_px`).
- 6df297b21 FIX item 1: `DisplayListItem::is_visually_equal` compares `VirtualView::content_offset`.

## FINDINGS SO FAR (2026-09-30, before the 18:15 reboot)
- AzReview headless (1400x900): the sheet strip is a `VirtualView` (node 424, horizontal,
  content 189440 px). Wheel `delta_x` NEGATIVE scrolls right (delta_x +300 was clamped at 0).
- After `wheel delta_x=-300` + 5 `wait_frame`s, each dumped incremental frame (`AZ_DUMP_FRAME_DIR`)
  differed from the `take_screenshot` render in the WHOLE sheet text area: the last one by ~20.9k px
  (bbox 287,54 - 1392,862). `frame_020_inc.png` shows stale glyph fragments inside the code text
  ("trans_ifine:nAffine", "a[r . + 2b6 + c]"), i.e. the incremental path leaves pixels of the old
  offset behind. The toolbar digits also differ (probably raster-path noise: screenshot = plain
  raster, frame = compositor; verify with a full `render_frame` instead).
- Suspect: `DisplayListItem::is_visually_equal` for `VirtualView` ignores `content_offset`
  (display_list.rs ~2247), so the item diff reports "unchanged" when only the view's scroll moved.
  `patch_virtual_view_content_offset` (window.rs ~3434) is the lightweight scroll path for views
  (`Arc::make_mut` on the live list). Whatever damage moved the content came from elsewhere
  (child re-materialisation / gpu thumb damage) and does not cover the view.
- Bug 3(a) reproduced with `scratchpad/mv/e2e/first1.json` (760x400, AzWidgets, `mount` two `<p>`):
  the first line "guard line" is not painted, the DL has both Text items (dl_004: item 3 is a
  `PushReferenceFrame` (Discriminant 23) owned by node 4 = the first `<p>`, closed by a
  `PopReferenceFrame` (24) at index 7 AFTER the first text). At 760x800 both lines paint.
  Hypothesis: a stale GPU transform for that reference frame's key (AzWidgets' previous DOM had 497
  reference frames; a key collision keeps an old transform) or the reference frame is emitted for
  an animation the mounted node never had.

## IN PROGRESS
- Item 3(b): `render_text` (sweep + grayscale) clips to the STACK clip only; the pre-tiled LCD path
  clips to `clip_rect` ∩ stack. Fix: one `text_run_clip` helper in `render_text`. RED test in
  raster.rs next to `a_clip_thinner_than_a_pixel_paints_nothing`.
- Item 3(a): AzWidgets at rest holds 476 FLIP moves (`get_animations` active=476) whose GPU values
  never change over 40 frames (tx-57.17 constant) while its 21 enter tracks do converge; a `mount`
  matches the new `<p>` (node 4) to an old node by structural key and the stale FLIP puts it
  off-screen at 760x400 (at 760x800 no reference frame). Reading tick_animations to see why FLIPs
  do not advance.

## NEXT
1. RED test for item 1 (headless `dll/src/desktop/shell2/headless/mod.rs` tests or a
   `layout/tests/` cpurender test): a VirtualView scrolled through the lightweight path must equal a
   full render; then the fix (`is_visually_equal` compares `content_offset`, or the view's damage
   comes from `compute_virtual_view_damage` when the offset changed).
2. Item 3(a): why the first `<p>` gets a reference frame at 760x400 and why it is not painted.
3. Item 3(b): text clipping uniform across `raster.rs` paint paths.
4. Item 2 leftovers (all marked FIXED by S1 already; verify what is still open).
5. Report `scripts/FB3_SCROLL_2026_09_30.md`.

## Open questions
- none yet

# A2-cpu: sign, offset, coordinate and rounding audit of the CPU rendering path

Area agent A2-cpu of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A2-cpu` (azul and m4p). Base: `audit/base` (`f48bcc7b4`). Line numbers are those of the base commit.

## Claims

Functions this area may restructure (owner per RULES.md section 3), all in `layout/src/window.rs`:

- `window_space_offset_of_dom`
- `virtual_view_content_offset`
- `virtual_view_frame_scroll_offsets`
- `patch_virtual_view_content_offset`

Everything else this area touches is a minimal patch in another area's file (a few changed lines inside an existing
function, plus a test directly below it), per RULES.md section 4:

- A1's `layout/src/cpurender/{raster,compositor,pixmap}.rs`: only on top of the tag `audit-A1-structure`, never inside a
  function A1 claims; findings there go to `## For area A1` below.
- A7-editor's `mapedit3/src/ui/main/map.rs` (m4p): minimal patches, bigger things to `## For area A7-editor

**A2-cpu-23, the zoom's anchor** (`write_picture_transform`). What must hold: the picture point under the pointer is
drawn where the pointer is, with the scale the engine DRAWS. All in view px (logical), per axis.

- exact, from the model (as today): `k = s / s0`, `t = s (pan - pan0)` (`picture_transform`); a picture point `P` is at
  `k P + t`.
- the scale the engine draws: three decimals. Quantise in the editor, so that it does not depend on whether the engine
  cuts or rounds (A2-cpu-24): `n = round(k x 1000)`, `kq = n / 1000`, and hand over `FloatValue::create((n + 0.25) /
  1000.0)` (x 1000 in the engine is `n + 0.25`: `n` cut, `n` rounded). `snap_transform`'s `|k - 1| < 1e-3 -> 1` stays
  (`kq = 1`, no `Scale` written).
- the anchor: the picture point under the pointer of the zoom step, `Pc = (c - t) / k` with `c` = `(cx, cy)` of
  `zoom_view` (equal to `(at + pan0) x s0`, `at = to_map(cx, cy)`: it does not change while the pointer's map point is
  kept). Keep `Pc` in `ViewMotion` (new field, cleared by a render); a pan during the zoom keeps it (the point moves
  with the picture); with no step yet, the picture point at the view's middle, `(size / 2 - t) / k`.
- the translation to write: **`tq = t + (k - kq) x Pc`** (= `c - kq x Pc`). Then `kq Pc + tq = k Pc + t = c`: the anchor is
  exact. Any other point is off by `(k - kq) x (P - Pc)`: at most `0.0005 x` its distance from the anchor.
- `m.zoom = Some((kq, tqx, tqy))`; `picture_covers` with `kq`, `tq`.

Example: view 2560 x 1440, render scale 6, the picture unshifted, the pointer at `c = (2000, 1200)`, one wheel notch
(`2^(60 / 240) = 1.1892071`). `k = 1.1892071`, `t = c (1 - k) = (-378.414, -227.049)`, `Pc = (2000, 1200)`.
Today: the engine draws `1.189`, the anchor lands at `1.189 x 2000 - 378.414 = 1999.586` and `1.189 x 1200 - 227.049 =
1199.751`: 0.41 px left, 0.25 px up. Corrected: `n = 1189`, `kq = 1.189`, `tq = (2000 - 2378.0, 1200 - 1426.8) =`
**`(-378.000, -226.800)`** (before: `(-378.414, -227.049)`), the anchor at (2000, 1200). Three notches: `k = 1.6817928`,
today drawn at 1.681: 1.59 px of slip at x 2000; corrected `kq = 1.682`, `tqx = 2000 - 3364.0 = -1364.000` (before
`-1363.586`).

Smaller, by reading (not signs): the first frame of a zoom after a pan sets the view's offset back to zero and writes
the transform in one frame: the engine moves the view's pixels back (memmove) and then repaints the whole view for the
value change; a zoom back to exactly the render's scale and pan keeps `motion.zoom = Some((1, 0, 0))`, so the settle
renders an identical picture once more.

## For area A11

**A2-cpu-23, the assertion of `e2e/zoom_labels.json`** (`e2e.rs` `view_picture`). Redefined, not relaxed: the point under
the pointer stays where it is ON SCREEN, measured with what the engine holds.

- `view_picture` reads `(kr, txr, tyr)` from the engine (`read_transform`, as today) and has the model's exact
  `(k, tx, ty)` (`picture_transform`) and the editor's anchor `Pc` (the new `ViewMotion` field; without one, the view's
  middle as above).
- new field **`error.anchor_px`** = `max over x, y of |(kr x Pc + tr) - (k x Pc + t)| x dpi`: the drawn position of the
  anchor against the model's, in device pixels. Assert **`at_most: 0.02`** (the translation keeps three decimals, cut:
  up to 0.001 px; f32 at coordinates of a few thousand: about 0.001 px; x dpi 2).
- `error.k` stays (`|kr - k|`, at most 0.001 with the editor's rounding; the scenarios allow 0.003).
- `error.tx` / `error.ty` (`<= 0.05` against the EXACT model's translation) go: with the fix `txr = tx + (k - kq) Pc`
  on purpose (0.414 px in the example above). The absolute ranges of `result.read.tx` / `.ty` hold where `k` has three
  decimals (the pinch of 1.5: `kq = k`, `tq = t`) and move by `(k - kq) x Pc` elsewhere (1.414 steps: `0.0002135 x Pc`,
  0.13 px at `Pc = 600`): recompute them with the formula or assert `anchor_px` instead.
- optional second number, to see the cost of three decimals: `error.far_px` = the same expression at the view corner
  farthest from the anchor; bound `0.0005 x` the view's diagonal in device px (1.5 px at 2560 x 1440, dpi 1).
- same example as for A7-editor: before the fix `anchor_px = 0.414` (fails), after it `0.000`.

## For area A10`

### A2-cpu-14 the layered compositor rounded a reference frame's pivot to a device pixel; the flat walk and WebRender do not
- file:line `compositor.rs:1325-1328` (base) - class: rounding - severity: low at whole device positions, visible at
  fractional ones (a full repaint and a damage repaint of one frame half a pixel apart). Lead from A2-gpu (A2-gpu-05).
- evidence: dpi 1, frame box (20.5, 30.5), `scale(2)`: flat walk `x' = 2 g + 19.5`, layer `x' = round(20.5) + 2 (g - 0.5)
  = 2 g + 20`.
- test: `a2_cpu_14_a_scaled_frame_between_pixels_is_composited_where_the_damage_repaint_puts_it` (the two paths' frames
  are equal, every pixel)
- status: fixed `e14f80eb3`

### A2-cpu-15 a turned frame at the window's level: the full repaint (a layer) and the damage repaint (a group) are not the same pixels when layers nest inside it
- file:line `compositor.rs:1496-1504` (`child_base`), the tests `lane_az_1_*` (tolerances of 40 and 400 pixels)
- class: wrong-output - severity: low (edges only; the editor's turned labels are inside a view, which both paths paint
  with the flat walk)
- evidence: a layer inside a promoted frame is rebased with `round(bounds.origin dpi)` and composited through the
  frame's matrix on its own; the flat walk paints it into the frame's surface first and resamples once. The existing
  tests accept the difference. After A2-cpu-14 a frame WITHOUT inner layers is exact.
- status: open (A1: the retained-surface work decides whether window-level frames go through one path)

### A2-cpu-16 a snapped offset is `n / s`, and `(x + n / s) x s` is `x s + n` only to 1e-5
- file:line `window.rs` `virtual_view_content_offset` (the snap), `raster.rs` `VirtualView` arm (`acc - origin -
  content_offset`), `GroupGeometry::of` (`o = bounds.origin * dpi`) - class: rounding - severity: low (scales 1.25, 1.5, ...
  only: an edge on a half device pixel can round the other way after a pan, a resampled group can differ by a level)
- evidence: at 1.5 an odd logical edge is `k + 0.5` device px; after a pan by `n` device pixels its position is computed
  as `(x + 48 + n / 1.5) x 1.5 = k + 0.5 + n (1 +- 6e-8)`: which way it rounds depends on the noise, so the moved pixels
  (rounded at the old offset) and a fresh render (rounded at the new one) can differ by a pixel along that edge. At 1 and
  2 every term is exact. The tests of this area therefore stand on multiples of 4 px and allow two levels at 1.25 / 1.5.
- fix: carry the view's offset in whole device pixels (an integer) through the raster and add it AFTER the scale.
- status: open (A1): see `## For area A1`

### A2-cpu-17 a solid border thinner than one device pixel painted nothing (or two rows, one outside the box)
- file:line `raster.rs` `render_border`, `let swi = sw as i32` - class: rounding - severity: low (hairline borders). Lead from A2-gpu.
- test: `a2_cpu_17_a_border_thinner_than_a_device_pixel_is_one_pixel_inside_its_box`
- status: fixed `c5f25d10d` (`(sw as i32).max(1)`, as WebRender)

### A2-cpu-18 the damaged walk culled a text under a shadow by its box, without the shadow's reach
- file:line `raster.rs` `walk_items`, the `WalkCull::Damage` block (base: `render_display_list_damaged` 2743-2771)
- class: wrong-output (missed repaint) - severity: low-medium (window-level text with an offset / wide shadow; the labels of
  a view go through `outside_active_clip`, which had the reach)
- test: `a2_cpu_18_a_damage_rect_on_a_text_shadow_alone_repaints_the_shadow` (`raster.rs`, `text_shadow_tests`)
- status: fixed `6836b9c89` (`text_shadow_reach`, used by both culls)

### A2-cpu-19 `execute_virtual_view_shift` counts every item after the view as an overlay, also the ones that paint nothing
- file:line `compositor.rs:3755-3780` - class: waste - severity: a whole-view repaint per pan frame when it happens
- evidence: the loop damages `it.visual_bounds().or_else(|| it.bounds())` of every non-state item after the view that
  overlaps its clip. `HitTestArea`, `TextLayout` and `WebView` have bounds and paint nothing: a hit-test area emitted
  after the view over the view's box makes every pan repaint the box. (`overlay_rects_after_frame_in` has the same rule
  for scroll frames.) Not seen in the editor's measurements (13 px strips), so its list has none there today.
- status: open (A1 claims the function): skip `HitTestArea | TextLayout | WebView` in both loops

### A2-cpu-20 the value damage and the item diff are placed with the REAL scroll offsets, an incremental frame paints at the BASELINE offsets
- file:line `cpu_backend.rs:501-507`, `:598-608`, `:823-828` against `:1200-1204` - class: sign/offset - severity: very low
- evidence: a frame whose scroll delta is under half a device pixel keeps its previous baseline (`next_scroll_baseline`)
  and is painted there; `window_value_damage`, `compute_display_list_damage_live` and `collect_virtual_view_shifts` get
  `scroll_offsets`. The rects are up to half a device pixel from the pixels; after the outward snap a one-pixel column
  of a changed item inside such a frame can stay stale. (`compute_virtual_view_damage_live` gets the baseline: right.)
- status: open (A1 claims `render_frame`): pass `next_scroll_baseline` to all three

### A2-cpu-21 a plain layer is placed on a whole pixel and its content rounded relative to it; the flat walk rounds each item's own position
- file:line `compositor.rs` `composite_layer_recursive` (`layout_offset_device_px`), `render_layers` (`offset_x`) against
  `raster.rs` `render_rect` - class: rounding - severity: low (fractional scales, content inside scroll-frame layers)
- evidence: dpi 1.25, scroll frame clip origin x = 10 (12.5 device: the layer is placed at 13), an opaque rect at x = 20,
  w = 30 inside. Flat walk: `[round(25), round(62.5)) = [25, 63)`. Layer: local `[round(12.5), round(50)) = [13, 50)`, plus
  13: `[26, 63)`. Column 25 differs between a full repaint and a damage repaint of the same list.
- status: open (A1): either the flat walk snaps a scroll frame's origin the way the layer does (WebRender's order:
  offsets first, then edges, A2-gpu-05), or full repaints stop using layers for plain scroll frames

### A2-cpu-22 `node_groups_on_screen` subtracts the whole scroll AFTER the frames
- file:line `compositor.rs:3199-3208` - class: sign/offset - severity: very low (a culler that errs to "visible" otherwise)
- evidence: `affine_rect_about(m, bounds.origin, content)` with the unscrolled origin, then `moved_by(r, stack.scrolled())`.
  Right for scroll frames outside every frame; a scroll frame INSIDE a scaling frame is off by `(M - I) s`, and the group
  can be judged off screen while it shows (its animation would be culled).
- status: open (not claimed; `ScrollStack::on_screen` with a `FrameStack` is the walk that is right)

### A2-cpu-23 the editor's zoom keeps the pointer's map point only for the scale it MEANT, not for the scale the engine draws
- file:line m4p `mapedit3/src/ui/main/map.rs:564-593` (`write_picture_transform`), `:518` (`picture_transform`)
- class: rounding - severity: medium-low (every zoom step, 30 / s: up to 2.5 px of slip at the far edge of a 2560 px view,
  gone at the settle render: labels visibly jump when the zoom settles)
- evidence: the editor writes `translate(t) scale(k)` with `t = s (pan - pan0)`, exact for `k = s / s0`. The engine keeps
  three decimals of `k`, cut off (A2-cpu-02): it draws `k_q <= k`, `k - k_q < 0.001`. A picture point `P` is shown at
  `k_q P + t = (k P + t) - (k - k_q) P`. Example: one wheel notch, `k = 1.189207 -> 1.189`: the point under a pointer at
  picture x 2000 is drawn 0.41 px left of the pointer; `k = 1.6817928 -> 1.681`: 1.6 px; at the settle it is back.
  Always to the left / up (the cut only shrinks).
- status: open (A7-editor): see `## For area A7-editor`

### A2-cpu-24 `FloatValue::new` truncates
- file:line `css/src/props/basic/length.rs:223`, `css/src/cast.rs:40` - class: rounding - severity: low, but it is the root
  of A2-cpu-02, -13 and -23
- evidence: `FloatValue::new(0.29)`: `0.29f32 = 0.28999999`, `x 1000 = 289.99999 -> 289`: the value is 0.289.
  `scale(0.9999999)` is 0.999; `translate(-0.0007px)` is 0; `get_hidpi_factor()` at dpi 128 is 1.333.
- fix: `(v * 1000.0).round() as isize` in `FloatValue::new` (and the same in `PixelValue` if it has its own).
- status: open (css is A2-gpu's / A3-core's; A2-gpu has finished: for the coordinator to route)

### A2-cpu-25 `is_identity_2d` does not look at the perspective terms
- file:line `compositor.rs:77-84` - class: wrong-output - severity: very low (`matrix3d()` only)
- evidence: it tests `m00, m01, m10, m11, m30, m31`. A matrix with those at the identity and `m03` / `m13` / `m33` not is
  painted in place, untilted; `translation_2d` does test them.
- status: open (one more conjunct; not patched: no test without a perspective fixture)

### A2-cpu-26 an opaque rect rounds both edges, a translucent one truncates them
- file:line `raster.rs` `render_rect`: `round_edge` against `rect.x as i32` (`blend_bar`) - class: rounding - severity: low
- evidence: a rect at x = 62.8, w = 10.4: opaque `[63, 73)`, at alpha 254 `[62, 73)`: a fade moves the box's left edge.
- status: open (part of the one-rule decision of A2-cpu-06)

### A2-cpu-27 the view shift's "covered" test reads the child's opaque fills at their layout place
- file:line `compositor.rs:3704-3729` (`FrameIndex::opaque_fills`) - class: sign/offset - severity: very low today
- evidence: a ground inside the picture's frame is taken at its L bounds; with the frame scaled to 0.5 the painted
  ground no longer covers the view, `covered` still says yes and the memmove runs over whatever shows around the
  picture. Harmless over a flat backdrop (what the editor has, and it pans through the transform while zoomed).
- status: open (A1): count only fills outside every non-identity frame of the child

### A2-cpu-28 the windowed diff damages a changed frame's box, not what the frame paints
- file:line `compositor.rs:4898-4980` (`collect_window`) - class: wrong-output (missed damage) - severity: low
- evidence: a `PushReferenceFrame` inside the changed window contributes its `bounds` through the frames. Its content can
  lie in the visually-equal SUFFIX (the window ends before it): content that reaches past the frame's box (a shadow, an
  overflowing child) is then not damaged although its matrix changed. The paired path uses the frame's extent.
- status: open (not claimed): in `collect_window`, a `PushReferenceFrame` adds `reference_frame_extent_walked` instead of
  its box

### checked: fine
- A2-cpu-08 `GroupGeometry::of` (`T(o) . Lm . T((x, y) - o)`) against `affine_rect_about` / `FrameStack::map` (`o + M (p - o)`)
  and the layer path (`place . layer_h . from_pivot`): one mapping; the perspective row is divided by dpi and the
  translation multiplied in both.
- A2-cpu-09 `group_part_within`: the inverse is the inverse of `x' = a x + b y + tx, y' = c x + d y + ty`; corners are pixel
  edges in both spaces; non-invertible (`|det| < 1e-9`), non-finite and perspective matrices keep the whole surface;
  nested groups compose (each maps into the surface below it).
- A2-cpu-10 `blit_scale::axis` / `lerp` / the bands: the general path's sampling points, weights 0..=255, no lane
  overflow, weight 0 is a copy, the last row / column fades against a transparent tap, negative scale and translation
  through the same formula, disjoint rows per band.
- A2-cpu-11 the movers, all eight directions: section Shift.
- `ScrollStack` (`on_screen`, `placement`, `step`): scroll frames outside and inside frames, nested frames (worked through
  three nestings against the raster): right.
- `view_damage_in_list` (`lift`), `list_value_damage`: `+ origin + content_offset`, cut to the box, then the frames around;
  the old extent of a value inside a view is lifted with the NEW content offset, which is where the view's shift has
  moved the old pixels to: right.
- `compute_display_list_damage_impl`: the offset stack (old and new), `map_through_frames`, the symmetric differences
  (`rect_minus`), the 1 px text fringe: right. `coalesce_damage_rects`, `disjoint_damage`, `box_minus`: only ever
  replace rects by a cover of them.
- `snap_out` (floor / ceil, clamped before any unsigned cast), `fill_rect`, `backdrop_under`, `blit_*` loops: no negative
  value reaches an unsigned cast unclamped.
- `execute_scroll_shift` (ghost at `g - delta`), `compute_resize_damage` (logical strips from physical sizes / dpi).
- `virtual_view_frame_scroll_offsets` (`-content_offset`: what WebRender scrolls the pipeline by, A2-gpu-04 agrees),
  `patch_virtual_view_content_offset` (writes the snapped value; its `!=` compares two results of one function),
  `window_space_offset_of_dom` (delegates: A2-cpu-03).
- the editor: `picture_transform` (`k p + t` about the picture's top left, `transform-origin: 0 0`, `Translate` listed
  before `Scale` = scale first), `set_scale` (the map point under the pointer is kept in the model), the pan's
  telescoping round (`round(shift dpi) / dpi` of the accumulated shift), `picture_covers` (1 px of slack, hidden by the
  ground having the page's colour), `OVERSCAN_PX - 16` against the ground's 256, `MIN_PICTURE_SCALE` (never the binding
  bound: `picture_covers` gives up near 0.7-0.83 for real view sizes, so bilinear minification below 0.5 is not reached).

## For area A1

1. **A2-cpu-04, frame extent.** `FrameIndex` keeps, per frame, its extent WITHOUT its inner frames (`own`) and the list
   of its direct inner frames; `reference_frame_extent(list, idx, transforms)` = `own` united with
   `affine_rect_about(live_or_initial(inner), inner.bounds.origin, extent(inner))` for each inner frame, recursively
   (`None` -> unbounded for a perspective matrix). Callers that have the values pass them (`raster.rs`: `render_state.
   transforms`; `gpu_value_damage`: old for the old extent, new for the new; `allocate_layers`: `live_transforms`;
   `compute_display_list_damage_impl`: `values`). And `gpu_value_damage` then also has to damage an OUTER frame's extent
   when an INNER frame's value changes only through the inner frame (it does: the inner frame's own rects go through
   the frames around it). Test to un-ignore: `a2_cpu_04_...`.
2. **A2-cpu-06, one clip rule.** `AzRect::pixel_box()` (pixmap.rs) is the fills' rule. Use it at: `render_box_shadow`'s
   `blit_clipped` (far edge `ceil` today), the `PushOpacity` arm (`rect.width as u32`: take `pixel_box` of the region and
   subtract), `text_shadow_boxes` and `FilterGroup::begin` (`floor` / `ceil`), `composite_layer_recursive`'s `static_clip`
   (`round`), and `scroll_shift_region_impl`'s region (`floor` / `ceil`: with a far edge at 1600.5 the memmove takes the
   neighbour's column 1600 along). DECIDED by the coordinator: the fills' rule is the one rule of the CPU path.
   (WebRender's order - offsets snapped first, then every edge `round`, A2-gpu-05 - stays the alternative for a
   session that can compare pictures: then `pixel_box` and the five `as i32` clip sites change together.)
   **Negative coordinates.** `pixel_box` truncates (`as i32`): towards ZERO, so below zero it is `ceil`, not `floor`:
   an edge at -0.5 becomes 0 where `floor` gives -1, an edge at -3.7 becomes -3 where `floor` gives -4. A negative
   edge CAN reach it: the damaged walk seeds the clip stack with the damage rect (clamped to the pixmap, never
   negative), but the full walks (`render_display_list_with_state`, a layer's `render_display_list_range`) seed it
   with `None`, so the first `PushClip` or a view's box is taken as it is: a view at logical x -20.4 .. 99.6 at 1.25 is
   the clip -25.5 .. 124.5, `pixel_box` = (-25, .., 124, ..), `floor` would say -26. The picture is the same under both,
   because pixel -1 exists on no surface and every user of the box intersects it with the surface before it writes
   (`blit_pixmap_affine_clipped`: `x0.max(cx0)` with `x0 >= 0`; `group_part_within`: `x0.max(0)`; agg's `clip_box_i`
   against the buffer; `fill_rect`; `snapshot_region` / `write_region` skip `sx < 0`): near edge -0.5: `[0, ..)` and
   `[-1, ..)` are both `[0, ..)` on the surface; far edge -0.5: `[.., 0)` and `[.., -1)` are both empty there. So the
   rule holds as long as the box is used as a CLIP (intersect, clamp, write). It must NOT be used as an origin to place
   content from, or to size a buffer by: there the two differ by a pixel below zero (`GroupGeometry::of` takes `floor`
   for the surface's corner for that reason; an opacity region taken from `pixel_box` must snapshot `x1 - x0` columns
   FROM `x0`, not `trunc(width)` columns, and may start below zero: `snapshot_region` skips those).
3. **A2-cpu-07, the view shift's gate.** Gate on both OFFSETS being whole device pixels
   (`|off dpi - round(off dpi)| < 1e-3` for `shift.offset` and `shift.offset - shift.delta`), not on the delta; then
   `round(new) - round(prev)` IS the delta. Otherwise `whole`. (The producer snaps, so nothing is lost.) Test to
   un-ignore: `a2_cpu_07_...`.
4. **A2-cpu-16.** `VirtualView` arm: keep `content_offset` out of `acc` and apply it as whole device pixels after the
   scale (an `(i32, i32)` device shift on the walk state), so a pan by `n` pixels moves every rounding by exactly `n`.
5. **A2-cpu-19 / -20 / -21 / -27**: as their blocks say.
6. **For the retained group surface (A1b)**, from the space table:
   - a surface is the picture of `(extent, frac((bounds.origin - acc) dpi), dpi, child list, values of the frames INSIDE
     it)`: the content is rasterised at `(p - acc) dpi - (x, y)`, so a surface kept at one pan offset is the same picture
     at another only when the two offsets differ by whole device pixels (they do, since A2-cpu-13); key the sub-pixel
     phase or refuse the hit;
   - the walk paints only `group_part_within(.., 64)` of a surface: a kept surface must have been painted whole (or keep
     the painted box and repaint what a later frame needs outside it);
   - the composite's edge rule (taps outside the surface are transparent) fades the outermost texel: a surface cut
     tighter than the extent shows a fade where the full surface had content;
   - a damage rect inside the surface maps back with the same inverse as `group_part_within` (no margin for the CLEAR,
     the margin for what is painted).
7. **For the bands (A1a)**: a band is a damage rect; cut bands on device rows of the `SnappedRect`; a group that
   crosses bands is opened once per band with `outer_clip` = the band's clip: same pixels, since the clip only limits
   where the composite writes and `group_part_within` is computed per band.

## For area A6

**A2-cpu-03.** `resolve_chain` must apply the links in order, each frame about its own origin, instead of one summed
scroll and one composed matrix. With the chain outermost first:

- forward (local to screen), from the INNERMOST link out: a scroll link: `q -= s`; a frame link: `q = o + M (q - o)`,
  where `o` is the frame's box origin in LIST space (unscrolled: `PushReferenceFrame.bounds.origin`, the node's paint
  rect origin) and `M` the painted matrix. (An outer scroll frame is subtracted after the inner frame's matrix, which is
  `(o - s_out) + M (p - s_in - o)`: the raster's rule.)
- inverse (screen to local), from the OUTERMOST link in: a scroll link: `q += s`; a frame link: `q = o + M^-1 (q - o)`.
- `HitChainLink::Transform` therefore needs the frame's origin (or the layout node to read it from), not only the node.
- `nested_dom_window_origin`: the child's (0, 0) is the host-L point `bounds.origin + content_offset`; run the forward
  mapping of the host chain on it. For a view inside a view, map through the inner host's chain into the outer child's
  L, add the outer view's `bounds.origin + content_offset`, then the outer chain (adding the offsets first and mapping
  once is right only without frames).
- Example to test with: the label of A2-cpu-03 (box (300, 200) 100 x 12, `rotate(90deg)` about (0, 12)): the screen point
  (306, 262) is the local point (50, 6) of the box, i.e. L (350, 206).
- `window_space_offset_of_dom` (mine) stays a delegate of `nested_dom_window_origin`.

## For area A7-editor

**A2-cpu-23, the zoom's anchor.** In `write_picture_transform`, after `snap_transform_of`: the engine draws
`kq = floor(k x 1000) / 1000` (until A2-cpu-24 is fixed; `round` after). Write the translation for THAT scale, so that a
chosen point stays put: with `c` the anchor in view px (the pointer of the step, else the view's middle) and
`P = (c - t) / k` its picture point, `t_q = c - kq P` per axis. The slip left is `|k - kq| x` the distance from the anchor
(under 0.3 px over half a 2560 px view with rounding) instead of `x` the distance from the picture's corner (2.5 px).
`zoom_view` has the pointer; `pan_view` (a pan during a zoom) has none: keep the last step's. The scenarios of
`e2e/zoom_labels.json` assert `result.error.tx <= 0.05` against `picture_transform` (`e2e.rs` `view_picture`): compare
against the corrected translation there (A11). Not patched here: it changes what the scenarios read.

Smaller, by reading (not signs): the first frame of a zoom after a pan sets the view's offset back to zero and writes
the transform in one frame: the engine moves the view's pixels back (memmove) and then repaints the whole view for the
value change; a zoom back to exactly the render's scale and pan keeps `motion.zoom = Some((1, 0, 0))`, so the settle
renders an identical picture once more.

## For area A10

- `CpuBackend::render_frame`'s `dpi_factor` should be `layout_window.frame_dpi_factor()` in every shell (headless, X11,
  macOS and Windows compute the same `ws.size.dpi as f32 / 96.0` today; the Wayland popup path has a `dpi_factor` of its
  own, `wayland/mod.rs` about 10200-10310): a view's offset is snapped on that grid (A2-cpu-13), and a frame rendered
  at another factor repaints the view on every pan step.
- `WindowSize::get_hidpi_factor()` is cut to three decimals (A2-cpu-24): do not size or place a frame with it.
- `pixel_w = ceil(width x dpi)`: at a fractional product the frame is a fraction of a pixel larger than the window; the
  last row / column holds whatever the root items' own rounding puts there.

## api.json

None.

## Not read

`raster.rs`: the bodies of the text functions (`render_text*`, `render_glyphs_lcd`, glyph pen placement: A8),
gradients, `render_box_shadow` beyond its clip, `render_image` / `blit_sampled_image` / `visible_dst_window`, masks,
`render_border_sides`, the component-preview and page renderers; `svg.rs`, `hvif.rs`, `shape.rs`, `blend.rs`,
`text_halo.rs`, `text_raster.rs`, `embolden.rs`. `compositor.rs`: `apply_layer_filters`, `composite_with_source`,
`scroll_fast_path_eligible_in` and `execute_translate_blit` were read for signs only, not for their heuristics;
`scroll_layer` / `compute_exposed_rects` / `shift_pixbuf` are dead code (the comment of `scroll_shift_region` says so).
`dll/.../headless/mod.rs`: `paint_cpu_frame` and the test helpers only (the present side is A10's).
`managers/virtual_view.rs`: the origin / size stores only. `window.rs`: my four functions,
`apply_virtual_view_geometry`, `painted_transform_of`. m4p `map.rs`: the view-motion, zoom and pan functions
(190-660, 822-850, 1400-1565, 2128-2225 and `zoom_tests`); not the label / symbol builders.

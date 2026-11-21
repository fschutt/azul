# A2-gpu: where matrices and offsets are made, how the GPU translation consumes them, CPU vs GPU cross-check

Area agent A2-gpu of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A2-gpu` (azul only). Base: `audit/base` (`f48bcc7b4`). Line numbers are those of the base commit.

## Claims

Functions this area may restructure (owner per RULES.md section 3):

- `core/src/transform.rs`: `ComputedTransform3D::from_style_transform_vec`, `from_style_transform`, the matrix
  constructors (`new_translation`, `new_scale`, `new_rotation`, `new_skew`, `new_perspective`, `make_rotation`),
  `then`, `inverse`, `transform_point2d`, `scale_for_dpi`, `to_device_px` (new).
- `core/src/gpu.rs`: `GpuValueCache::synchronize` / `synchronize_with_sizes` (and its transform / opacity key maps),
  `reference_frame_of`, `reference_frame_at`, `for_each_bound_value`.
- `layout/src/managers/gpu_state.rs`: `GpuStateManager::update_scrollbar_transforms`, `update_sticky_transform`,
  `update_scrollbar_transform_key`, `painted_transform_of`.

Touched so far: `core/src/transform.rs` only (one new function and its tests). Nothing in another area's file was
changed: both fixes this area found on the GPU side are inside functions A4 claims (`translate_displaylist_to_wr`,
`synchronize_gpu_values`), so they are under `## For area A4` with the exact change (RULES.md section 4).

Tests: `dll/src/desktop/audit_tests/a2_gpu.rs`, `layout/src/audit_tests/a2_gpu.rs`, `core/src/transform.rs`
(`audit_a2_gpu_tests`, at the end of the file).

## Spaces

The CPU-side spaces are A2-cpu's (`git show audit/A2-cpu:audit/A2-cpu.md`, `## Spaces`): **L** list space (logical px,
origin = the list's DOM), **P** painted space, **D** device px of the frame, **G** group surface px, **Y** layer px.
The GPU path adds four. All are y-down except F.

| # | space | origin | unit | what is stored in it | converts to |
|---|---|---|---|---|---|
| W | **WebRender layout space of a pipeline** (one pipeline per DOM: `PipelineId(dom_id, document_id)`) | the pipeline's root scroll node: the window's top-left for the root DOM; for a `VirtualView` child the iframe's `bounds.min` (snapped by WebRender to whole device px) | **device px**: `L x dpi`. The fork's frame builder pins `global_device_pixel_scale = 1.0` (`webrender/core/src/frame_builder.rs:535`), so WebRender's "layout px" ARE device px | everything `compositor2::translate_displaylist_to_wr` emits: rects, clip rects, glyph points, frame origins, gradient points, shadow offsets, radii, border widths | S(n) of the node it is given in |
| S(n) | **local space of spatial node n** | reference frame: its `origin` in the parent after its matrix; scroll frame: the parent's origin (a scroll frame makes NO new origin) | device px | an item is given in the space of its `common.spatial_id`; a clip in the space of the node it was defined on | parent: reference frame `M(p) + origin` (`spatial_node.rs:435`, plus the accumulated scroll of the scroll frames between the two frames); scroll frame `p - snap(offset)` (`:585`), `offset` = what `set_scroll_offsets` got (stored negated, `:327`) |
| WD | **WebRender world = device space** | window top-left | device px, integer rects | dirty rects (`PartialPresentDamage`), `WorldPoint`s | equals D of the CPU path pixel for pixel |
| F | **GL window coordinates** | bottom-left, y-UP | device px | `glScissor`, `eglSwapBuffersWithDamage` rects | `y_F = fb_h - (y_WD + h)`: done inside WebRender's renderer for drawing and by the EGL presenters for damage rects (A10: `shell2/linux/wayland/gl.rs`); `wr_translate2::device_rects_to_present_rects` stays top-left |

### L -> W (the translation, `compositor2.rs`)

- rect / clip / iframe bounds: `x_W = x_L * dpi` (`resolve_rect`, `scale_bounds_to_layout_rect`). The `offset_stack`
  of the translation is dead: it is an immutable `vec![(0.0, 0.0)]`, `current_offset!()` is always (0, 0), every
  `- offset.0` is a no-op (the comment at `:1047` says why: a scroll frame makes no new origin).
- glyph: `point_W = point_L * dpi` (`push_text`), font instance at `size * dpi` (`translate_add_font_instance`): once each.
- border: widths `floor(css * dpi)`, at least 1 device px for a visible side; radii resolved in CSS px then `* dpi`
  (`wr_translate2::get_webrender_border`).
- lengths of shadows, blurs, gradients' lines: `* dpi`.
- scroll offset: `current_offset * dpi` (`scroll_all_nodes`), positive = scrolled down / right; WebRender negates it.
- reference frame `PushReferenceFrame { bounds, initial_transform M, transform_key }`: TWO frames, the outer at
  `corner = bounds.origin * dpi` with the bound matrix `M_wr`, the inner at `-corner` with the identity; items stay in
  window coordinates. A point `p_W` is drawn at `corner + M_wr (p_W - corner)`. The origin is in the frames, not in
  the matrix, so the live value `synchronize_gpu_values` sends replaces the outer frame's matrix as it is.
  `M_wr` = `M` with the translation row `* dpi` (and, missing today, the perspective column `/ dpi`: A2-gpu-01).
- stacking context, opacity group, filter group, image-mask context: opened at `LayoutPoint::zero()` (WebRender
  moves a context's content by its origin; the items are already in window coordinates).
- scrollbar thumb: one frame at zero with the translation `* dpi`.
- `VirtualView { bounds, clip_rect, content_offset }`: `push_iframe(bounds * dpi, clip_rect * dpi, child pipeline)`;
  `content_offset` is NOT translated. The child's list is its own pipeline in its own L x dpi, and
  `scroll_all_nodes` scrolls that pipeline's root scroll frame (`ExternalScrollId(0, child pipeline)`) by
  `virtual_view_frame_scroll_offsets() * dpi = -content_offset * dpi`: a child point `c` is drawn at
  `snap(bounds.origin * dpi) + c * dpi + content_offset * dpi`.

### The matrix convention (made in `core/src/transform.rs`, `core/src/gpu.rs`)

`ComputedTransform3D.m` is row-major for ROW vectors: `[x' y' z' w'] = [x y z 1] * m`, translation in `m[3][0..3]`,
perspective column `m[0..3][3]`. `a.then(b)` = `a * b` = "a first, then b". For `transform: t1 t2 ... tn` the loop of
`from_style_transform_vec` computes `matrix = component.then(&matrix)` starting with `t1`, so the result is
`tn * ... * t2 * t1` and a point `p * tn * ... * t1` goes through `tn` FIRST and `t1` LAST: the CSS order (the last
listed function is applied first). The whole product is then wrapped once: `T(-o) * M * T(o)` with `o` =
`transform-origin` resolved against the node's own border box, so the stored matrix acts in the BOX's own space
(origin = the box's top-left corner); the consumers move it to the box (`o_d + M_d (p_d - o_d)`). Checked term by term:

| function | matrix (row vectors) | CSS Transforms | ok |
|---|---|---|---|
| `rotate(a)` / `rotateZ` | `x' = x cos a - y sin a`, `y' = x sin a + y cos a` (clockwise on a y-down screen): `make_rotation` negates the angle for `ForWebRender` and then takes `2 pi - (-a)`, the two cancel | same | yes |
| `rotateX(a)` | `y' = y cos a - z sin a`, `z' = y sin a + z cos a` | same | yes |
| `rotateY(a)` | `x' = x cos a + z sin a`, `z' = -x sin a + z cos a` | same | yes |
| `scale`, `translate` (percent of the own box) | diagonal / row 3 | same | yes |
| `skew(ax, ay)` | `x' = x + y tan ax`, `y' = y + x tan ay` | same | yes |
| `matrix(a, b, c, d, tx, ty)` | `x' = a x + c y + tx`, `y' = b x + d y + ty` | same | yes |
| `matrix3d` | the 16 arguments in order = rows of the row-vector matrix = columns of the CSS one | same | yes |
| `perspective(d)` | `m[2][3] = -1/d`: `w = 1 - z/d` | same | yes (d = 0 gives -inf, see A2-gpu-11) |
| `RotationMode::ForHitTesting` | the inverse rotation | - | no caller in `core/`, `layout/`, `dll/` |

### Numeric example of the area (used by the tests of A2-gpu-03)

A 100 x 40 box at L (300, 200), `transform-origin: 50% 50%`, `scale(2)`, inside a scroll frame scrolled by (0, 50), dpi 2.

- made: `o = (50, 20)`, `M: p -> 2 p - (50, 20)` (`m[0][0] = m[1][1] = 2`, `m[3] = (-50, -20, 0, 1)`).
- by hand: the corner, box-local (0, 0), goes to (-50, -20), in L (250, 180), scrolled (250, 130), device **(500, 260)**;
  the far corner to device (900, 420).
- CPU (`raster.rs`): `acc = (0, 50)`; frame origin `o_d = (300, 150) * 2 = (600, 300)`; group surface at
  `floor(...) = (600, 300)`, 200 x 80 px; `layer_h` translation `(-50, -20) * 2`; group pixel (0, 0) ->
  `(600, 300) + (-100, -40)` = **(500, 260)**.
- GPU (`compositor2.rs`): rect at W (600, 400); inner frame `+(-600, -400)` -> (0, 0); outer frame
  `M_wr(0, 0) + (600, 400) = (-100, -40) + (600, 400) = (500, 360)`; scroll frame `- (0, 50 * 2)` -> **(500, 260)**.

## Cross-check: one sample point per item type, CPU against GPU

`p` = a point of the item in L, `acc` = the accumulated scroll (CPU) = the sum of the scroll offsets of the scroll
frames around it, `dpi` = the hidpi factor. "equal" = the same real number before rounding.

| item | CPU rasteriser (`raster.rs` flat walk) | GPU (`compositor2.rs` -> WebRender) | verdict |
|---|---|---|---|
| Rect | `(p - acc) * dpi`; opaque square fill: both edges `round`; translucent: `trunc` (A2-cpu-06) | `p * dpi` in its node; WebRender snaps the rect's edges to whole device px after the node's (snapped) scroll offset | equal; edges differ by at most 1 px where `(p - acc) * dpi` is fractional (A2-gpu-05) |
| Border | box `(p - acc) * dpi`, edges `trunc`; width `trunc(w * dpi)` px, so a width under one device px paints NOTHING (`raster.rs:6093`) | box `p * dpi`; width `floor(w * dpi)`, at least 1 px (`wr_translate2.rs:2595`) | positions equal; a hairline (`0.5px` at dpi 1) is drawn by the GPU and not by the CPU (A2-gpu-05, for A2-cpu) |
| Text | pen `(g.point - acc) * dpi`, em `font_size * dpi` (`raster.rs:5298`) | `g.point * dpi`, font instance at `size * dpi` | equal; the sub-pixel quantisation of each side was not compared (A8's glyph cells; WebRender rounds the baseline to whole device px on an axis-aligned node) |
| Image | `scroll_rect(bounds) * dpi` | `bounds * dpi` | equal |
| Box shadow | box `scroll_rect(bounds) * dpi`, offset / blur / spread `* dpi` | box `* dpi`, offset / blur / spread `* dpi` | equal |
| Text shadow (halo) | A5 / A1 | kept halo image at `halo.origin` (device px) or WebRender's shadow with offset `* dpi` | not compared here (A5) |
| Clip (`PushClip`) | `scroll_rect(bounds) * dpi`, intersected with the clip around it, kept as f32 and used with `trunc` | `bounds * dpi` on the current spatial node (moves with the frames it was pushed in, as items do), snapped by WebRender | equal up to rounding |
| Scroll frame | `acc += s`; the clip is the `PushClip` in front of it | scroll node, content at `- snap(s * dpi)`; its own clip = `clip_bounds * dpi` on the PARENT node (plus the `PushClip`, same rect: one redundant clip node) | equal up to the snap of `s * dpi` (A2-gpu-05). Same source on both sides: `ScrollManager.states[..].current_offset` (`build_scroll_offset_map` / `get_scroll_states_for_dom().children_rect.origin`) |
| Viewport scroll frame (root) | no clip (`viewport_scroll_frame` pushes none) | clip = the canvas rect | equal on screen (the canvas is the window) |
| Reference frame | `o_d + M_d (p_d - o_d)`, `o_d = (bounds.origin - acc) * dpi`, `M_d`: translation `* dpi`, perspective row `/ dpi` (`raster.rs:1233`, `compositor.rs:1306`); identity and pure translations painted in place | `corner + M_wr (p_W - corner)`, `M_wr`: translation `* dpi`, perspective column unchanged | equal for every affine matrix; **differ under `perspective()` when dpi != 1: A2-gpu-01** |
| Stacking context | nothing (list order is paint order) | context at origin zero | equal |
| Opacity group | the group is the node's BOX inside the open clip (`opacity_group_region`); what the node paints outside its box is painted at full opacity | a stacking context with an opacity filter around all of the node's items | positions equal; coverage differs outside the box (A2-gpu-06, for A1) |
| `VirtualView` | child point `c` at `(bounds.origin + content_offset + c - acc) * dpi`, clipped to `bounds` | `snap(bounds.origin * dpi) + c * dpi - frame_scroll * dpi`, `frame_scroll = -content_offset`, clipped to `clip_rect` (= `bounds`) | equal (A2-gpu-04); `content_offset` is on the device grid on both; a box at a fractional device position is snapped by WebRender only (A2-gpu-05) |
| Scrollbar thumb | `thumb_bounds + (m[3][0], m[3][1])` of the live key value | a frame at zero with that translation `* dpi` | equal; one quantisation (`quantize_thumb_offset`) feeds both the baked and the live value |
| Scrollbar fade | flat walk: drawn opaque while the key's value is above 0.001, not at all below | opacity filter bound to the key | the GPU fades, the CPU flat walk switches (A2-gpu-07, for A1) |

Values behind keys: both backends read `GpuValueCache::for_each_bound_value` for every DOM of the window
(`WindowGpuValues::of_doms` on the CPU, `synchronize_gpu_values` on the GPU). Key ids come from one process-wide
counter (`PROPERTY_KEY_COUNTER`, `core/src/resources.rs:3174`), so the merge across DOMs cannot collide, and
transforms, floats and colours are separate maps on both sides.

## Findings

### A2-gpu-01
- file:line: `dll/src/desktop/compositor2.rs:1502-1519` (`PushReferenceFrame`), `dll/src/desktop/wr_translate2.rs:2149-2166`
  (`synchronize_gpu_values`); the CPU side that is right: `layout/src/cpurender/raster.rs:1233-1243`,
  `compositor.rs:1306-1316`.
- class: sign/offset (matrix x dpi), wrong-output
- evidence: the GPU translation works in device px (`L x dpi`) and scales only the translation row of a frame's matrix.
  The conversion of a matrix from logical to device px is `S^-1 M S`, `S = scale(dpi)`: translation row `* dpi` AND
  perspective column `/ dpi`. Without the division `w = m33 + dpi * (x m03 + y m13)`: `perspective()` is `dpi` times
  too strong on a HiDPI window. The CPU divides, so the backends disagree.
  Example: 100 x 40 box, origin 50% 50%, `perspective(400px) rotateY(60deg)`, dpi 2. `m[0][0] = 0.608253`,
  `m[0][3] = 0.00216506`, `m[3] = (19.58735, -2.16506, 43.30127, 0.891747)`. Top-right corner, logical (100, 0):
  `x' = 80.41265`, `w = 1.108253` -> (72.558, 1.954). Device point (200, 0): as coded `w = 0.891747 + 200 * 0.00216506
  = 1.324759` -> (121.400, 3.269); right: `w = 0.891747 + 200 * 0.00108253 = 1.108253` -> (145.116, 3.907) = 2 x logical.
  23.7 device px off across a 200 px box. Affine matrices (the editor's zoom, rotated labels) are not affected.
- severity: every frame of every perspective frame on a HiDPI GPU window (the map widget's 3D tilt); 0 ms.
- fix: `ComputedTransform3D::to_device_px` (the conversion, with tests) in this branch; the two call sites are for A4.
- test: core `transform::audit_a2_gpu_tests::*`; layout `audit_tests::a2_gpu::a_box_under_perspective_is_painted_where_the_gpu_values_put_it_at_two_device_px`
  (CPU pixels = converted matrix); dll `audit_tests::a2_gpu::a_box_under_perspective_reaches_webrender_where_the_cpu_paints_it_at_two_device_px`
  (fails until the call sites are changed: commit `3aff8a911`, revert it alone if the change is deferred).
- status: fixed `729911211` (the conversion), open at the call sites: `## For area A4`.

### A2-gpu-02
- file:line: `dll/src/desktop/wr_translate2.rs:2257` (`txn.append_dynamic_properties(properties)`), consumed by
  `webrender/core/src/scene.rs:55-61` (`SceneProperties::add_properties`) and `:80-110` (`flush_pending_updates`).
- class: leak + waste (per-frame work that grows with the session)
- evidence: `append_dynamic_properties` APPENDS to WebRender's pending list (`pending_properties.extend(properties)`);
  the list is emptied only by `reset_dynamic_properties` (`render_api.rs:392`), which azul never calls
  (`git grep reset_dynamic_properties` finds only its definition). `synchronize_gpu_values` sends the COMPLETE set of
  bound values of every DOM each time the set changed (and unconditionally after `invalidate_submitted_digests`, i.e. in
  every full frame). So after N such transactions the pending list holds N copies of the set. `flush_pending_updates`
  then, for each of them: compares the lists, clears the three maps, inserts ALL N copies again (the last one wins, so
  the values are right), and clones the whole list into `current_properties`.
  Cost model with the samples' n: the Tuebingen view binds 2,118 reference frames (one transform key per label and
  symbol), 72 bytes per `PropertyValue<LayoutTransform>`: **153 KB appended per zoom step**, twice (pending + current) =
  0.3 MB per step, 9 MB/s at 30 steps/s, never freed. Work on the render backend thread per frame after N steps:
  N x 2,118 map inserts + an N x 153 KB clone: about 40 us x N, i.e. **~40 ms per frame after 1,000 value frames**
  (33 s of zooming, or 1,000 animation ticks / full frames), 0.4 s per frame after 10,000. Every frame that changes a
  value pays it: zoom step, transform / opacity tween, scrollbar thumb move and fade, colour fade, every full frame.
- severity: the largest item of this area. ms x frequency grows without bound: 40 us x N x 30/s; memory 0.3 MB x N.
- fix: one line, inside a function A4 claims: `## For area A4`.
- test: needs a seam, described there (the transaction's `frame_ops` are private: nothing in azul-dll can see what a
  transaction carries).
- status: open: for A4.

### A2-gpu-03
- file:line: `compositor2.rs:1486-1582` (reference frame), `:959-1167` (scroll frame), `raster.rs:1181-1295`, `:3511-3539`
- class: sign/offset
- evidence: the algebra of "Numeric example of the area" above: origin folded once (outer frame at the corner, inner at
  minus the corner, nothing in the matrix), translation `* dpi` once, scroll offset subtracted once on each side.
- fix: none needed.
- test: layout `a_scaled_box_in_a_scrolled_frame_is_painted_where_the_gpu_values_put_it`, dll
  `a_scaled_box_in_a_scrolled_frame_reaches_webrender_where_the_cpu_paints_it` (`a1706ad18`).
- status: checked: fine.

### A2-gpu-04
- file:line: `compositor2.rs:1584-1644`, `wr_translate2.rs:2072-2096`, `window.rs:4338-4393`, `raster.rs:3547-3643`,
  `webrender/core/src/scene_building.rs:1167-1255`
- class: sign/offset (the Z1 class on the GPU side)
- evidence: CPU `bounds.origin + content_offset + c`; GPU iframe at `bounds.origin`, root scroll frame of the child's
  pipeline at `-(frame_scroll)`, `frame_scroll = -content_offset` (`virtual_view_frame_scroll_offsets`), `* dpi` on
  every term; WebRender does not clamp `set_scroll_offsets` (`spatial_node.rs:315-336`), so a negative scroll (content
  offset positive) is drawn as asked. View at x 200, pan -37, stripe at child x 40: 203 on both; dpi 2: 406; pan -37.3
  at dpi 2 snaps to -37.5: 405 on both.
- fix: none needed.
- test: layout `a_panned_views_content_is_painted_where_the_gpu_scroll_offset_puts_it`, dll
  `a_panned_views_content_reaches_webrender_where_the_cpu_paints_it` (`a1706ad18`).
- status: checked: fine.

### A2-gpu-05
- file:line: `webrender/core/src/scene_building.rs:1202` (`snap_rect` of an iframe's bounds), `spatial_node.rs:574-592`
  (`snap_offset` of scroll offsets), `raster.rs:4121-4131`, `:6091-6101`, `compositor.rs:1325-1328`
- class: rounding
- evidence: WebRender snaps (a) an iframe's bounds, (b) every accumulated scroll offset, (c) every axis-aligned rect
  and clip, each to whole device px, in that order. The CPU rounds per item: `(p - acc) * dpi` with nothing snapped
  before. Example: view box at L x = 250.4, child item at 10.3, dpi 1: GPU `round(250.4) + 10.3 = 260.3 -> 260`, CPU
  `260.7 -> 261`. Scroll 0.2 px, rect at 10.6: GPU `10.6 - round(0.2) -> 11`, CPU `10.4 -> 10`. At most one device px,
  only for fractional device positions (dpi 1.25 / 1.5, fractional layout, a glide between whole pixels). Inside the CPU
  path the same class: the layered compositor rounds a frame's pivot (`layout_offset_device_px`, `compositor.rs:1326`)
  where the flat walk (`raster.rs:1229`) and the GPU use the unrounded corner: a frame at a fractional device position
  is composited up to half a pixel apart by the two CPU paths.
- fix: none on the GPU side (WebRender's snapping is its design). For the CPU side see `## For area A2-cpu`.
- test: none (documented difference).
- status: checked: a known difference of at most 1 device px; no change.

### A2-gpu-06
- file:line: `layout/src/cpurender/raster.rs:3726-3776`, `:2557-2568` (`opacity_group_region`)
- class: wrong-output (CPU flat walk)
- evidence: the CPU's opacity group is the node's box (`bounds`) inside the open clip; the GPU's is a stacking context
  around everything the node paints. A child positioned outside the box, a shadow, overflowing text: painted at the
  group's opacity by WebRender and at full opacity by the flat walk (it paints outside the snapshot the pop blends).
- fix: for A1 (the group's region should be what the node paints, as `reference_frame_extent` is for a frame).
- status: open: `## For area A1`.

### A2-gpu-07
- file:line: `layout/src/cpurender/raster.rs:3351-3434` (`ScrollBarStyled`)
- class: wrong-output (CPU flat walk)
- evidence: the opacity read from the key only gates the arm (`if scrollbar_opacity > 0.001`); track, buttons and thumb
  are painted in their own colours. A fade at 0.5 is opaque on the flat walk and half transparent on the GPU. Not
  checked against the layered compositor (full repaints), which may give the bar a layer of its own.
- status: open: `## For area A1`.

### A2-gpu-08
- file:line: `core/src/gpu.rs:707-724` (`compute_transform_events`), `:811-828` (opacity), `window.rs:8727`, `:8917`,
  `display_list.rs:6683`, `:7331`
- class: waste
- evidence: `(Some(old), Some(new)) => Changed` is emitted for EVERY node with a transform on every sync, also when
  `old == new`; with 2,118 transformed labels that is a 2,118-element Vec (about 300 KB) per layout pass, merged into
  `pending_changes`, which nothing drains but the next pass. Each pass also builds every CSS matrix three times
  (`synchronize_with_sizes` before layout, `refresh_transform_values` after it, `reference_frame_at` in the display
  list builder). Per settle render: 3 x 2,118 cascade lookups and matrix products, well under 1 ms.
- fix: `Changed` only when the value differs (one comparison). NOT done blind: the E2E manager fingerprint prints
  `pending=<number of events>` (`layout/src/e2e/full.rs:12552`) and the two-sided manager assertions compare it, so a
  pass that now emits 0 events instead of N can flip a scenario; it wants the E2E suite at hand.
- status: open: low (under 1 ms per settle), needs the verification pass.

### A2-gpu-09
- class: checked: fine (one line each)
- matrix construction order, rotation direction, skew, `matrix()` / `matrix3d()`, perspective sign, the origin wrapped
  once per list (table above): fine.
- percentages (`transform-origin`, `translate(%)`) resolve against the same size everywhere: `used_size` of the node's
  first layout box (`synchronize_with_sizes`, `refresh_transform_values`, `bound_node_size`, the tween at
  `window.rs:18656`) and the paint rect's size in the builder (`get_paint_rect` = `calculated_positions` + `used_size`): fine.
- dpi multiplies once: the fork ignores the `DevicePixelScale` handed to `set_document_view` (`frame_builder.rs:497`
  takes it, `:535` uses 1.0), so `compositor2`'s `* dpi` is the only one. The comments at `wr_translate2.rs:1971` and
  `:2804` ("WebRender's device_pixel_scale handles the conversion") say the opposite of what happens: fine, misleading.
- a display list is re-translated when the dpi changes (`translation_env` holds the scale): fine.
- scrollbar thumb: baked value and live value through one geometry function and one quantisation; the list's keys are
  registered into the cache after the build (`window.rs:9508-9604`) and reused by the next build: fine.
- scroll offset: one source, one sign, `* dpi` once: fine.
- scroll ids are a hash of (dom, node), transform keys a counter: a `SpatialTreeItemKey` collision between a scroll
  frame `(scroll_id, n)` and a transform frame `(key, 0)` needs a 64-bit hash to equal a small integer: fine.
- colour keys are not remapped by `GpuStateManager::remap_node_ids` but pruned before every root build
  (`retain_running_color_keys`): fine.
- border colour defaults differ (`raster.rs:3055` transparent, `wr_translate2.rs:2547` opaque black) but
  `getters::get_border_info` gives every painting side a colour (currentcolor or the declared one, explicit
  `transparent` included); the other producers of `Border` items (`display_list.rs:4438`, `:9179`) were not checked.
- `WindowGpuValues::of_doms`: merge of per-DOM maps by key id, unique ids: fine.

### A2-gpu-10
- file:line: `core/src/gpu.rs:164-179` (`reference_frame_of`)
- class: wrong-output (both backends, equally)
- evidence: a node gets ONE frame, bound to the first channel that has a value: CSS, else animation, else sticky. A
  node with a CSS `transform` that is also dragged (`update_drag_visual` writes the animation channel) or slid by a
  FLIP animation shows only its CSS matrix: the drag / slide is not drawn. Same for a sticky box with a transform
  (documented there). The fix is a composed value under one key (`css.then(anim)`), in the cache.
- status: open: low; a design gap, no difference between the backends.

### A2-gpu-11
- file:line: `core/src/transform.rs:128-133`, `:671-690`, `:762-776`
- class: wrong-output (degenerate inputs)
- evidence: `perspective(0)` makes `m[2][3] = -inf` (CSS Transforms 2: a depth under 1px is treated as 1px); pinned by
  `transform_test.rs:390` and `:1446`, so not changed blind. `inverse()` returns the IDENTITY when `|det| < f32::EPSILON`
  (a 2D `scale(0.0003)` has det 9e-8: its inverse would be the identity, not `scale(3333)`); `transform_point2d` lets
  `w = +0` through (division by zero). Neither has a caller outside tests in `core/`, `layout/`, `dll/`.
- status: open: low (no production caller / degenerate CSS).

### A2-gpu-12
- file:line: `compositor2.rs:1529`, `:1567`; `display_list.rs:6681-6703`
- class: wrong-output (suspected, not verified)
- evidence: the spatial key of a transformed frame is `(transform_key.id, 0)`. A node painted twice in one list (a box
  broken across regions: `generate_for_stacking_context` runs once per part; a `position: fixed` box replicated per
  page) opens two frames with the same key, and WebRender requires keys to be unique per pipeline
  (`compositor2.rs:270-277` says so for scroll frames, which count their pushes for this reason).
- status: open: not verified (needs a region / paged list on the GPU path); the scroll frames' push counter is the fix.

## For area A4

### A2-gpu-02 (first: it is the one that costs frames)

`dll/src/desktop/wr_translate2.rs`, `synchronize_gpu_values`, in front of line 2257:

```rust
    // The set is complete (every bound value of every DOM), so it REPLACES what the renderer
    // holds: `append` alone adds to WebRender's pending list, which only a reset empties
    // (`SceneProperties::add_properties` extends it), and every later frame then inserted and
    // cloned every set this window ever sent.
    txn.reset_dynamic_properties();
    txn.append_dynamic_properties(properties);
```

`Transaction::reset_dynamic_properties` exists in the fork (`webrender/core/src/render_api.rs:392`); frame ops run in
order, so the reset empties the pending list and the append fills it with this set. The early returns above it
(nothing collected; digest unchanged) send nothing, and WebRender keeps the last set: unchanged behaviour.
Test (needs a seam, the transaction is opaque): either (a) in `webrender/core/src/scene.rs` a `#[cfg(test)]` module
below `flush_pending_updates`: `add_properties` of the same 3 transforms twice, flush, `current_properties.transforms.len()
== 6`; with `reset_properties()` before each add: 3 (pins the semantics the azul side relies on); or (b) a
`#[cfg(test)]` accessor on `Transaction` for the names of its `frame_ops`, and in azul-dll: a window with one
transformed node, two `synchronize_gpu_values` with a changed value, each transaction holds
`[ResetDynamicProperties, AppendDynamicProperties]`. A counter for the frame recorder: properties sent per frame.

### A2-gpu-01

`dll/src/desktop/compositor2.rs`, `PushReferenceFrame` arm (lines 1506, 1510, 1514):

```rust
-                    initial_transform.m[0][3],
+                    initial_transform.m[0][3] / dpi_scale,
...
-                    initial_transform.m[1][3],
+                    initial_transform.m[1][3] / dpi_scale,
...
-                    initial_transform.m[2][3],
+                    initial_transform.m[2][3] / dpi_scale,
```

and the same three lines in `dll/src/desktop/wr_translate2.rs`, `synchronize_gpu_values` (lines 2153, 2157, 2161:
`transform.m[0][3] / dpi_scale`, `transform.m[1][3] / dpi_scale`, `transform.m[2][3] / dpi_scale`). `dpi_scale` is
never 0 (`WindowSize::get_hidpi_factor` falls back to 96 dpi). Equivalent and shorter: build both from
`let m = transform.to_device_px(dpi_scale).m;` (`azul_core::transform::ComputedTransform3D::to_device_px`, commit
`729911211`) and pass the 16 entries as they are. The scrollbar thumb's frame (`compositor2.rs:721-738`) is a
translation: nothing to change. The test is already in this branch (`3aff8a911`).

### Smaller (no behaviour change)

- `compositor2.rs:284-310`: `offset_stack` is never pushed; `current_offset!()`, `apply_offset`, the `offset` /
  `scroll_offset` parameters of `resolve_rect`, `push_text`, `push_text_decoration_rect` are constant (0, 0).
- `compositor2.rs:1067`: a scroll frame defines a clip with the rect the `PushClip` in front of it just defined (the
  builder always emits both, `display_list.rs:7864-7888`): one clip node and one chain per scroll frame for nothing.
  The viewport's frame (`:7785`, no `PushClip`) needs the one at `:1067`.
- `wr_translate2.rs:1971-1973`, `:2804-2805`: the comment and the `DevicePixelScale::new(hidpi)` argument suggest
  WebRender scales; the fork uses 1.0 (`frame_builder.rs:535`). A reader who "fixes" either side doubles the scale.

## For area A2-cpu

- (A2-gpu-05) `compositor.rs:1325-1328`: the layered compositor rounds a frame's pivot to whole device px
  (`layout_offset_device_px`); `raster.rs:1229-1232` (flat walk) and the GPU use the unrounded corner. For a frame at
  a fractional device position the full repaint and the damaged repaint composite the same group up to half a pixel
  apart: a candidate for "damaged render != full render" with fractional positions.
- (A2-gpu-05) `raster.rs:6091-6101`: a solid border's device width is `sw as i32`; a width under one device px
  (`0.5px` at dpi 1, `0.3px` at dpi 2) paints nothing, the GPU paints one device px (`get_webrender_border`,
  `device_width`). The box edges there are `trunc`, the GPU's `round`.
- WebRender's rule, to align the CPU with if one rule is chosen (your A2-cpu-06): scroll offsets are snapped to whole
  device px first (`round`), then every axis-aligned rect and clip edge is rounded to the nearest device px; an
  iframe's (view's) box is rounded before its content is placed.

## For area A1

- (A2-gpu-06) `raster.rs:3744-3748`: the opacity group's region is the node's box; content of the group outside the box
  is painted at full opacity (the GPU fades it). Proposed: the region = the union of the visual bounds of the items
  from the push to its pop (what `reference_frame_extent` computes for a frame), inside the open clip.
- (A2-gpu-07) `raster.rs:3359-3364`: `ScrollBarStyled` ignores the value of its opacity key except as an on / off
  gate; the GPU fades the bar. Proposed: paint track, buttons and thumb with their alpha times the key's value.
- (cross-check, Border; not a position) `raster.rs:3125-3146` resolves a percentage `border-radius` of a `Border` item
  against the box's WIDTH for both axes of every corner; the GPU resolves the horizontal radius against the width and
  the vertical one against the height (`wr_translate2::wr_translate_border_radius`, CSS Backgrounds 3 s5.1).
  `border-radius: 50%` on a bordered 100 x 40 box: an ellipse of 50 x 20 on the GPU, a 50 px arc (then clamped) on
  the CPU. Not verified beyond reading the two resolvers.

## What this area did not read

- `layout/src/cpurender/compositor.rs` beyond `is_identity_2d`, `translation_2d`, `composite_layer_recursive`'s matrix,
  `affine_rect_about`, `layout_offset_device_px` (A2-cpu audits the rest); `pixmap.rs` blits (A1).
- hit-testing on either path (`headless.rs`, `AsyncHitTester`: A6), present rects and their y flip (A10), the halo
  images' placement (A5), glyph-cell quantisation (A8).
- `compositor2.rs`: the gradient, filter and image-mask arms were read for their coordinates only; the test modules not.
- `wr_translate2.rs`: resources, fonts, images, overlay uploads (lines 660-1860) not read.
- the WebRender fork: only `spatial_node.rs` (update of frames and scroll frames), `scene_building.rs::push_iframe`,
  `scene.rs::SceneProperties`, `render_api.rs::Transaction`, the device-pixel-scale uses in `frame_builder.rs`.
  WebRender's own snapping (`snap_rect`, glyph snapping) is quoted from the names and upstream behaviour, not re-read.
- `layout/src/solver3/scrollbar.rs`: the geometry functions read, the tests not. `window.rs`: only
  `virtual_view_content_offset` .. `patch_virtual_view_content_offset`, `refresh_scrollbar_transforms`,
  `synchronize_scrollbar_opacity`, `compute_scroll_ids`, the GPU sync in the layout pass (8700-8930, 9500-9610),
  `write_bound_gpu_properties`, `publish_gpu_only_restyle`, `sync_css_gpu_values`.
- sticky frames' offsets (`StickyConstraint::shift_at`) and the sheen layer's values: the frames are ordinary
  reference frames on both backends; the values themselves were not checked.

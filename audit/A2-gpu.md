# A2-gpu: where matrices and offsets are made, how the GPU translation consumes them, CPU vs GPU cross-check

Area agent A2-gpu of the 2026-11-21 audit. Blind: nothing here was compiled or run.
Branch `audit/A2-gpu` (azul only), append-only. Base: `audit/base` (`f48bcc7b4`). Line numbers are those of the base
commit unless a section names another tree. Round 2 (RULES.md section 8) is `30065ea07` and later.

## Claims

Functions this area may restructure (owner per RULES.md section 3):

- `core/src/transform.rs`: `ComputedTransform3D::from_style_transform_vec`, `from_style_transform`, the matrix
  constructors (`new_translation`, `new_scale`, `new_rotation`, `new_skew`, `new_perspective`, `make_rotation`),
  `then`, `inverse`, `try_inverse` (new), `transform_point2d`, `scale_for_dpi`, `to_device_px` (new).
- `core/src/gpu.rs`: `GpuValueCache::synchronize` / `synchronize_with_sizes`, `compute_transform_events`,
  `compute_opacity_events`, `apply_transform_events`, `refresh_transform_values`, `refresh_transform_value_of`,
  `sync_sticky_keys`, `reference_frame_of`, `frame_of_channels` (new), `reference_frame_at`, `for_each_bound_value`,
  and the write methods of A2-gpu-13 (`TransformChannel`, `OpacityChannel`, `RekeyNodes`, `set_*` / `key_*` /
  `ensure_*` / `register_*` / `remove_*` / `retain_*` / `clear*`, `rekey_nodes`). On `audit/integration` the twelve
  transform maps are A6's `StampedMap` (its A6-05); the methods are the same.
- `layout/src/managers/gpu_state.rs`: `GpuStateManager::update_scrollbar_transforms`,
  `refresh_scroll_driven_transforms` (new), `update_scroll_driven_box` (new), `update_sticky_transform`,
  `update_scrollbar_transform_key`, `painted_transform_of`, `value_generation` (new), `remap_node_ids`.

Changed in files of other areas (minimal, none inside a claimed function):
- `layout/src/managers/scroll_state.rs` (A6): one new accessor, `ScrollManager::nodes_with_a_bar` (A2-gpu-14).
- `layout/src/window.rs` `refresh_scrollbar_gpu_cache_for_cpu_frame` (17006 at base, A6's range, not in its claims):
  the thumbs of a CPU frame are refreshed by the frame form (2 lines changed, 9 added; A2-gpu-14).
- `core/src/gpu.rs` belongs to A3-core by the table and to this area by its transform / value functions; A3-core's
  `b37653684` (the write generation) is cherry-picked here as `44a2e6e42`.
Everything else for another area's file is under `## For area <owner>`, `## Review of A4` or `## Integration`.

Tests: `dll/src/desktop/audit_tests/a2_gpu.rs`, `layout/src/audit_tests/a2_gpu.rs`, and `audit_a2_gpu_tests` at the end
of `core/src/transform.rs`, `core/src/gpu.rs` and `layout/src/managers/gpu_state.rs`. Seven guards of
`core/src/transform_test.rs` were changed with A2-gpu-11 (they pinned the values that finding corrects).

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
- cost: every frame of every perspective frame on a HiDPI GPU window (the map widget's 3D tilt); 0 ms.
- severity: medium
- fix: `ComputedTransform3D::to_device_px` (`729911211`: the conversion, with tests). The two call sites were changed by
  A4 as three inline divisions each (`d6e9a9b0f`); equal to `to_device_px` entry by entry (`## Review of A4`).
- test: core `transform::audit_a2_gpu_tests::*`; layout `audit_tests::a2_gpu::a_box_under_perspective_is_painted_where_the_gpu_values_put_it_at_two_device_px`
  (CPU pixels = converted matrix); dll `audit_tests::a2_gpu::a_box_under_perspective_reaches_webrender_where_the_cpu_paints_it_at_two_device_px`
  (`3aff8a911`; it compares WebRender's frame matrix with `to_device_px`, 16 entries, and holds with `d6e9a9b0f`).
- status: fixed d6e9a9b0f


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
- cost: ms x frequency grows without bound: 40 us x N x 30/s; memory 0.3 MB x N.
- severity: high
- fix: A4-01 (`85d19c71a`): `txn.reset_dynamic_properties()` in front of the append. Read: `## Review of A4`.
- test: A4's (`webrender/core/src/scene.rs` test module and dll `audit_tests::a4`).
- status: fixed 85d19c71a


### A2-gpu-03
- file:line: `compositor2.rs:1486-1582` (reference frame), `:959-1167` (scroll frame), `raster.rs:1181-1295`, `:3511-3539`
- class: sign/offset
- evidence: the algebra of "Numeric example of the area" above: origin folded once (outer frame at the corner, inner at
  minus the corner, nothing in the matrix), translation `* dpi` once, scroll offset subtracted once on each side.
- fix: none needed.
- test: layout `a_scaled_box_in_a_scrolled_frame_is_painted_where_the_gpu_values_put_it`, dll
  `a_scaled_box_in_a_scrolled_frame_reaches_webrender_where_the_cpu_paints_it` (`a1706ad18`).
- severity: low
- status: checked: fine


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
- severity: low
- status: checked: fine


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
- severity: low
- fix: none on the GPU side (WebRender's snapping is its design). The two CPU-side leads were fixed by A2-cpu: the
  layered compositor's rounded pivot (its A2-cpu-14, `e14f80eb3`) and the border thinner than a device px (its
  A2-cpu-17, `c5f25d10d`). What is left is the difference of at most one device px at fractional device positions;
  the one clip rule of the CPU path is A2-cpu-06 (the coordinator chose the fills' rule; WebRender's order is noted
  there as the alternative).
- test: A2-cpu's `a2_cpu_14_*`, `a2_cpu_17_*`.
- status: fixed e14f80eb3


### A2-gpu-06
- file:line: `layout/src/cpurender/raster.rs:3726-3776`, `:2557-2568` (`opacity_group_region`)
- class: wrong-output (CPU flat walk)
- evidence: the CPU's opacity group is the node's box (`bounds`) inside the open clip; the GPU's is a stacking context
  around everything the node paints. A child positioned outside the box, a shadow, overflowing text: painted at the
  group's opacity by WebRender and at full opacity by the flat walk (it paints outside the snapshot the pop blends).
- fix: for A1 (the group's region should be what the node paints, as `reference_frame_extent` is for a frame).
- severity: low
- fix: in A1's `raster.rs` (the `PushOpacity` arm): the group's region has to be the extent of what the group paints,
  a scan like `GroupBounds` per `PushOpacity` (`## For area A1`). A1 holds it as its open row "A2-gpu-06". This branch
  predates `audit-A1-structure` and is append-only, so it cannot carry a patch of that file.
- status: open: A1's file (`raster.rs`, the `PushOpacity` arm): the opacity group's region must be its content's extent; A1's ledger row "A2-gpu-06"


### A2-gpu-07
- file:line: `layout/src/cpurender/raster.rs:3351-3434` (`ScrollBarStyled`)
- class: wrong-output (CPU flat walk)
- evidence: the opacity read from the key only gates the arm (`if scrollbar_opacity > 0.001`); track, buttons and thumb
  are painted in their own colours. A fade at 0.5 is opaque on the flat walk and half transparent on the GPU. Not
  checked against the layered compositor (full repaints), which may give the bar a layer of its own.
- severity: low
- fix: by A1 (`3445773d4`, test `a_scrollbar_is_painted_at_its_opacity`).
- status: fixed 3445773d4


### A2-gpu-08
- file:line: `core/src/gpu.rs:707-724` (`compute_transform_events`), `:811-828` (opacity), `window.rs:8727`, `:8917`,
  `display_list.rs:6683`, `:7331`
- class: waste
- evidence: `(Some(old), Some(new)) => Changed` was emitted for EVERY node with a transform on every sync, also when
  `old == new`: with 2,118 transformed labels a 2,118-element Vec (about 300 KB) per layout pass, merged into
  `pending_changes`, which nothing drains but the next pass. Each pass also built every CSS matrix three times
  (`synchronize_with_sizes` before layout, `reference_frame_at` in the display list builder, `refresh_transform_values`
  after layout): 3 x 2,118 cascade lookups and matrix products per pass.
- severity: low
- fix: `30065ea07`: an event only for a value that changed (transforms and opacities). `698410544`: the cache keeps the
  box size each CSS value was resolved against (`css_transform_sizes`); `reference_frame_at` reuses the live value at
  that size and `refresh_transform_values` rebuilds only the nodes whose size differs.
  Counts. 20 transforms + 20 opacities, second sync of the unchanged DOM: 0 + 0 events (was 20 + 20), the write
  generation where it was. Matrix builds per pass, 20 transformed nodes: first pass 20 + 20 + 20 = 60 (sizes are
  unknown before the first layout); every later unchanged pass 20 + 0 + 0 = 20 (was 60); one node resized by the
  layout: 1 by the list and 1 by the refresh, none for the 19 others.
- test: core `gpu::audit_a2_gpu_tests::a_sync_of_unchanged_values_reports_no_event`,
  `a_pass_that_changes_nothing_builds_each_matrix_once` (mutations in their docs).
- e2e: the manager fingerprint prints `pending=<events>` (`layout/src/e2e/full.rs:12549`): `## For area A11`. No
  expectation of azul or m4p had to change.
- status: fixed 698410544

### A2-gpu-09
- class: checked: fine (one line each)
- severity: low
- status: checked: fine
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
  FLIP animation showed only its CSS matrix: the drag / slide was not drawn. Same for a sticky box with a transform,
  and for a fading node with a CSS opacity (the fade replaced the opacity instead of multiplying it).
- severity: medium
- fix: `afe2fd120`. Still one frame per node, its key the first channel's (CSS, animation, sticky), its matrix the
  PRODUCT of the channels, CSS first: `css.then(animation).then(sticky)` (`frame_of_channels`). `reference_frame_at`
  bakes the product into the list, `for_each_bound_value` publishes it under the frame's key (both renderers), and
  `painted_transform_of` (the hit tester's) is the same function. An animation opacity is published as
  `animation x css`.
  Example: CSS `scale(2)` about the box corner, a slide of (30, 5): (10, 10) -> (20, 20) -> (50, 25). The other order
  gives (80, 30). CSS opacity 0.4 under a fade at 0.5: 0.2 (was 0.5).
- test: core `a_node_is_painted_with_the_product_of_its_channels`, `the_list_bakes_the_product_of_the_channels`,
  `a_fading_node_keeps_its_css_opacity`.
- status: fixed afe2fd120

### A2-gpu-11
- file:line: `core/src/transform.rs:128-133`, `:671-690`, `:762-776`
- class: wrong-output (degenerate inputs)
- evidence: `perspective(0)` made `m[2][3] = -inf` (CSS Transforms 2: a depth under 1px is treated as 1px). `inverse()`
  returned the IDENTITY when `|det| < f32::EPSILON` (a 2D `scale(0.0003)` has det 9e-8: its inverse was the identity,
  not `scale(3333)`). `transform_point2d` let `w = +0` through (a division by zero).
- severity: low
- fix: `548271af4`. `try_inverse() -> Option<Self>`: determinant and adjugate in f64, `None` for a determinant that is
  0 or not finite and for a result with an entry that is not finite (WebRender's rule: `det != 0 && !det.is_nan()`);
  `inverse()` is `try_inverse().unwrap_or(IDENTITY)`. `new_perspective(d)`: `d` under 1 is 1. `transform_point2d`:
  `None` for `w <= 0` or NaN.
  Examples. M: p -> (2x + 10, 4y + 20): inverse diag(0.5, 0.25) with translation (-5, -5); (14, 28) -> (2, 2).
  `scale(0.001)`: det 1e-6, inverse `scale(1000)` (was the identity). `scale(0, 1)`: `None`.
  `perspective(0) rotateY(60deg)` about (0, 0): (10, 0) -> x = 0.5176, as `perspective(1px)` (was -inf in the matrix).
- callers of an inverse on the rendering and hit-testing paths ("a frame without an inverse paints nothing and hits
  nothing"): `ComputedTransform3D::inverse()` has NO caller in `core/`, `layout/`, `dll/` on any audit branch. The
  paths invert by themselves: `cpurender/pixmap.rs:427` (`!det.is_finite() || det.abs() < 1e-12`: draws nothing);
  `headless.rs` `ScreenMapAffine::invert` (A6's: `## For area A6`); `raster.rs` `group_visible_part` (det < 1e-9:
  the whole group is kept, a culling decision only); `widgets/map/overlay.rs:357` (`invert(&forward)?`); WebRender's
  own spatial nodes. A new caller should take `try_inverse` and treat `None` as "nothing".
- test: core `transform::audit_a2_gpu_tests::a_matrix_says_when_it_has_no_inverse`,
  `a_perspective_of_less_than_a_pixel_is_one_pixel`; the seven changed guards of `transform_test.rs` are named in the
  commit.
- status: fixed 548271af4

### A2-gpu-12
- file:line: `compositor2.rs:1529`, `:1567`; `display_list.rs:6681-6703`
- class: wrong-output (a crash: WebRender's duplicate spatial key)
- evidence: the spatial key of a transformed frame was `(transform_key.id, 0)`. A node painted twice in one list (a box
  broken across regions: `generate_for_stacking_context` runs once per part; a `position: fixed` box replicated per
  page) opens two frames with the same key, and WebRender requires keys to be unique per pipeline.
- severity: low
- fix: A4-06 (`689b5367f`): the key counts how often the list has opened that transform key. Read: `## Review of A4`.
  The scrollbar thumb's frame has the same exposure and was not covered: A2-gpu-15.
- status: fixed 689b5367f

### A2-gpu-13 (the write generation: A3-core-08, A9's H4 / H6 / H7)
- file:line: `core/src/gpu.rs` (the 24 pub maps of `GpuValueCache` and `sheen_light`),
  `layout/src/managers/gpu_state.rs:629-716` (the sticky and thumb writers), `:718-756` (`remap_node_ids`)
- class: waste (the enabler: a frame that wrote no value still copies, compares and walks all of them)
- evidence: A3-core's `value_generation` (`b37653684`) was moved by `core/` only; every other writer wrote the pub maps
  directly, so no reader could skip on an unchanged generation. `remap_node_ids` named the maps one by one and had
  forgotten the bound colours and the size memo.
- severity: medium
- fix: `6048e40cc`: write methods for every kind of write (insert, update, remove, retain, clear of a transform, an
  opacity, a scrollbar fade, a colour, the sheen light), each moving the generation exactly when the cache's content
  changes; `rekey_nodes(&mut impl RekeyNodes)` hands a rebuild every node-keyed map (21) and every `(DomId, NodeId)`
  map (4); every write of `gpu_state.rs` goes through them; `GpuStateManager::value_generation()` is the number of
  the whole window (DOM ids folded in). `98df549bb`: key-only writes for the sheen channel. `898ea1234`:
  `ensure_transform_key` without `entry()` (on integration `entry()` restamps a `StampedMap` on every call and the
  generation test failed there).
- test: core `a_write_method_moves_the_generation_exactly_when_it_changes_something`,
  `each_channel_is_its_own_pair_of_maps_and_the_renderers_read_them`,
  `a_css_value_written_by_hand_is_not_taken_for_the_cascades`, `a_rebuild_rekeys_every_map_of_the_cache`; layout
  `a_thumb_that_moves_moves_the_write_generation_and_one_that_stays_does_not`,
  `a_rebuild_moves_every_value_with_its_node_and_the_window_generation`.
- what is left is not in this area's files: the direct writers, `## Integration`.
- status: fixed 6048e40cc

### A2-gpu-14 (from A4: its A4-10 a)
- file:line: `layout/src/managers/gpu_state.rs:470-622` (`update_scrollbar_transforms`); called per frame from
  `wr_translate2.rs:2773` and `:3112` (GPU) and through `window.rs:9932` from `:17008` (CPU frame)
- class: waste (per-frame O(boxes))
- evidence: every frame looked at every layout box of every DOM, and for each box that carries scrollbar
  requirements - layout stores them on nearly every box (`cache.rs:5319`), a bar on a handful - it looked the node up
  twice in the scroll manager (two `BTreeMap` misses), unpacked its box properties and measured its scroll extent
  (`LayoutTree::scroll_extent`, which walks the box's in-flow children) BEFORE asking whether the box has a bar.
  12.5k boxes x 2 to 3 calls per frame: more than A4's estimate of 0.1 to 0.4 ms.
- severity: low
- fix: `cc1fd463b`.
  1. `update_scroll_driven_box` asks first: a box that is neither sticky nor has a bar costs two field reads.
  2. `update_scrollbar_transforms` (the walk: same signature, values and events, for whatever tree it is handed)
     leaves the list of the DOM's scroll-driven boxes behind (`ScrollDrivenBoxes`: layout index and DOM node of every
     box of every node that has a bar or is sticky).
  3. `refresh_scroll_driven_transforms`, the form for a frame, reads the listed boxes only. It walks instead when
     nothing was walked for the DOM, when the tree has another number of boxes, when a listed box belongs to another
     node, or when the scroll manager holds a bar for a node the list does not name
     (`ScrollManager::nodes_with_a_bar`). The last clause is what sees a bar raised in place after the pass
     (`scroll_registration.rs:199`, `:227`; `window.rs:26450`): each of those sites registers the node with its bar.
     A sticky box is made by a layout pass only (`solver3/mod.rs:1750-1756`), and every pass that installs a tree
     calls the walk (`window.rs:9608`). A rebuild (`remap_node_ids`) drops the list.
  4. the CPU frame (`refresh_scrollbar_gpu_cache_for_cpu_frame`) calls the frame form.
  5. `GpuStateManager::scroll_boxes_looked_at`: the counter. `AZ_WALK_SCROLL_BOXES` (`2151bbf0a`): every frame walks.
  Count, 1000 boxes of which 2 scroll: the walk looks at 1000, a frame at 2 (scrolled or not), a frame of a DOM
  without a scroll or sticky box at 0. Thumb by hand (box 100 px, content 1000 px, classic bar 16 px): track
  100 - 2 x 16 = 68, thumb max(68 x 0.1, 2 x 16) = 32, scrolled 450 of 900: (68 - 32) x 0.5 = 18; at the end: 36.
- not covered by a check: a tree installed for a DOM WITHOUT the walk in which an unlisted box became sticky (a bar is
  seen as soon as it is registered). No such path exists today outside `skip_gpu_sync` (the headless web path, which
  has no sticky keys and no display list).
- test: layout `managers::gpu_state::audit_a2_gpu_tests::a_frame_looks_at_the_scroll_boxes_and_the_walk_at_every_box`,
  `a_bar_raised_after_the_pass_sends_the_frame_back_to_the_walk`, `a_frame_handed_another_tree_walks_it`.
- done / missing: the cost per box (both paths), the list and the CPU frame are done. The two GPU call sites are
  inside functions A4 claims (`build_webrender_transaction`, `build_image_only_transaction`); each becomes the frame
  form by one changed line (`## For area A4`).
- status: open: two call sites in functions A4 claims (`wr_translate2.rs`, one changed line each, under `## For area A4`)

### A2-gpu-15 (found while reading A4-06)
- file:line: `dll/src/desktop/compositor2.rs:789` at A4's tip (`ScrollBarStyled` arm; `:745` at base)
- class: wrong-output (a crash: WebRender's duplicate spatial key); not reproduced by a scenario
- evidence: the thumb's reference frame is keyed `SpatialTreeItemKey::new(transform_key.id as u64, 0)` whatever was
  opened before. The builder gives a scrollbar the key the cache holds for its NODE
  (`display_list.rs:10351`: `cache.transform_keys.get(&nid)`), so a scroll container that is painted twice in one list
  (the cases of A4-06: a box broken across regions, a `position: fixed` box replicated per page; also a node with two
  boxes that both paint a bar) opens two thumb frames with one key. Example: node 7, thumb key 41, painted on two
  pages: `(41, 0)` and `(41, 0)`; after A4-06 its own transformed frame would be `(k, 0)` and `(k, 1)`.
- severity: low
- fix: the rule of `compositor2.rs:1602-1611` (A4's tip) at `:789`, with the same two collections (thumb keys and
  frame keys are both `TransformKey::unique()` ids, so they cannot meet): `## Review of A4`.
- status: open: inside `translate_displaylist_to_wr`, which A4 claims; the exact change is under `## Review of A4`

## Review of A4

Second reader of six commits of `audit/A4`. Nothing of A4's was patched.

- `85d19c71a` (A4-01, reset before append): **read, agree.** The question was whether a frame between the reset and
  the append can see an empty set. No: both are frame ops of ONE transaction, `RenderBackend::update_document`
  (`webrender/core/src/render_backend.rs:1371-1385`) applies a transaction's frame ops in order in one loop and calls
  `flush_pending_updates` once after it, and a frame is built after that. The early returns of
  `synchronize_gpu_values` send neither op, so WebRender keeps the last set.
  One thing to know, not introduced by this commit: a set carried by a transaction that goes through the scene
  builder (a full frame) is applied when its scene is swapped in; a lightweight transaction sent after it that
  carries a NEWER set and no scene would be applied first, and the older set then replaces the newer one while
  `last_gpu_value_digest` says the newer one is held. Today every shell calls `flush_scene_builder()` after a send
  that builds a scene (`macos/mod.rs:9868`, `common/layout.rs:2111` at A4's tip), which keeps the order. If that
  flush is ever dropped, `invalidate_submitted_digests()` after a scene swap is the remedy.
- `d6e9a9b0f` (A2-gpu-01 at the call sites): **read, agree.** Both sites divide `m[0][3]`, `m[1][3]`, `m[2][3]` by
  the scale and multiply `m[3][0..3]` by it: entry for entry `to_device_px` for a scale > 0 (`to_device_px` leaves the
  matrix alone for a scale that is not positive and finite; `dpi_scale` never is: `get_hidpi_factor` falls back to 96
  dpi). Worked example of the area (100 x 40 box, `perspective(400px) rotateY(60deg)` about its centre, dpi 2): the
  corner at device (200, 0) gets `w = 0.891747 + 200 x 0.00108253 = 1.108253` -> (145.116, 3.907), twice the logical
  (72.558, 1.954). The division is also in the rewritten `synchronize_gpu_values` (`39a77728b`). My dll test
  (`3aff8a911`) compares the 16 entries.
- `39a77728b` (A4-07, the digest without a copy): **read, agree.** What a frame depends on and where it is: the
  matrix of every frame (all 16 entries as bits, so the perspective column and the translation row), every opacity,
  the scrollbar thumbs' transforms and the fades' opacities (all through `for_each_bound_value`, i.e. the composed
  values of A2-gpu-10), the bound colours, the counts per kind and the dpi. A frame's ORIGIN is not a bound value: it
  is in the display list (`push_reference_frame(origin, ..)`), which has its own submission record. The sum of
  per-entry hashes is independent of the order and changes when two keys swap values (the key is hashed with its
  value). An empty set returns "unchanged" without touching the stored digest: harmless (the next non-empty set
  differs from it or equals what WebRender holds). Still one walk over all values per frame:
  `GpuStateManager::value_generation()` makes the unchanged frame O(number of DOMs) once the writers of
  `## Integration` are done.
- `56bb92351` (A4-02, text under a scale or a rotation): **read, agree.** Where glyph positions and snapping come
  from now: unchanged by the commit, because the fork's rasteriser never honoured the font's transform
  (`webrender/glyph/src/font.rs`: it rasterises at `font.size`, returns `GlyphFormat::Alpha`, scale 1.0) and the shader
  path is chosen by the glyph's format, so text under a transform already took the path WITHOUT
  `WR_FEATURE_GLYPH_TRANSFORM` (`ps_text_run.glsl`): `raster_glyph_offset = floor(glyph.offset x glyph_raster_scale +
  snap_bias) / res.scale`, `glyph_origin = glyph_scale_inv x (res.offset + raster_glyph_offset) + text_offset`, with
  `glyph_raster_scale = raster_scale x device_pixel_scale = 1` in the fork. The commit only stops the per-zoom-step
  instance and raster; the run's offset stays unsnapped under a scale or rotation (`drawn_through_transform`), and
  the batch's tight bounds follow the shader's path (`batch.rs`).
  Is a turned or scaled label where the CPU paints it: yes, as before. Each glyph is placed on a whole px of the
  frame's LOCAL space (W, device px at frame scale 1) and the frame's matrix is applied to the quad. A glyph at local
  x = 10.3 in a frame scaled by 2 is drawn at 2 x floor(10.3 + 0.5) = 20, not 20.6: up to 0.5 x scale device px per
  glyph, the bitmap magnified. The CPU path paints a transformed frame's text into the group's surface in group px and
  resamples the surface (A1's `TransformGroup`), i.e. the same two steps; its per-glyph rounding was not re-read for
  this review. This is the price of "no raster per zoom step" on both paths and ends with the settle's relayout.
- `689b5367f` (A4-06, distinct spatial keys): **read, agree**, with one finding: **A2-gpu-15**. The outer frame is
  keyed `(key, n)`, the inner one `(key, u64::MAX - n)`, `n` = how often this list opened the key before; a key
  opened once keeps the keys every earlier list had. The scrollbar thumb's frame (`compositor2.rs:789` at A4's tip)
  still says `SpatialTreeItemKey::new(transform_key.id as u64, 0)`. Proposed, in place of that line:
  ```rust
  let push_index = if reference_frames_seen.insert(transform_key.id as u64) {
      0
  } else {
      let repeats = reference_frame_repeats.entry(transform_key.id as u64).or_insert(0);
      *repeats += 1;
      *repeats
  };
  let spatial_key = SpatialTreeItemKey::new(transform_key.id as u64, push_index);
  ```
  Test, next to A4's: a list with the same `ScrollBarStyled` item (thumb key 41) twice translates to two reference
  frames with the keys `(41, 0)` and `(41, 1)`; once: `(41, 0)`. Mutation: `push_index` fixed at 0 makes them equal.
- `5c6ac5a65` (A10-15, the document's size): **read, agree.** `LogicalSize::to_physical` rounds
  (`core/src/geom.rs:591`), as `WindowSize::get_physical_size` does for every later frame: 333.7 x 1.5 = 500.55 -> 501
  (was 500 by truncation). The factor is the one the old line used (`actual_hidpi_factor`). The Wayland half is
  another platform: noted, not checked further.

## Integration

Tree: `audit/integration` at `72236fc80` (it holds this branch up to `98df549bb`). Line numbers in this section are
that tree's.

**What the merge made of the generation.** A6 turned the twelve transform maps into `StampedMap`s (A6-05): a direct
write of a TRANSFORM map now moves the generation by itself. The opacity maps (6), the colour maps (2), the scrollbar
fade maps (4) and `sheen_light` are plain: a direct write of those is invisible to a reader of `value_generation()`.
Two ways to finish, for the coordinator to choose:
- (I) privacy, as planned: the 64 writes below go through the methods, and the 176 direct reads (36 files name the
  maps, tests included) get the read accessors (`transform_key`, `transform_value`, `transform_keys_of`,
  `opacity_key`, `opacity_value`, `opacity_keys_of`, `scrollbar_opacity_key`, `scrollbar_opacity`, `color_key`,
  `color_value`, `color_keys_of`). Three accessors are missing for that and are one line each in `core/src/gpu.rs`:
  `sheen_light()`, an iterator over a channel's values, an iterator over the scrollbar fades.
- (II) A6's proposal (its notes, "For area A2-gpu", item 4): make the twelve plain maps stamped too (`StampedMap` with
  a key parameter for the `(DomId, NodeId)` ones) and keep the fields public. Then only `sheen_light` (1 write) and
  the `entry()` sites (9, marked E below: `entry()` restamps whether or not it writes) need the replacement.
  Smaller and nothing can be forgotten; I would take (II) and still apply the E rows.

**Already wrong on that tree, fixed here:** `ensure_transform_key` used `entry()` on a `StampedMap`; core test
`a_write_method_moves_the_generation_exactly_when_it_changes_something` fails there at "the node had its key".
`898ea1234` (this branch) is the fix; the function's text is identical on both trees, so the merge takes it.

**The production writes** (all in `layout/src/window.rs`; `cache` / `gpu_cache` is the DOM's `GpuValueCache`; `T` =
`TransformChannel`, `O` = `OpacityChannel`):

| lines | function (owner) | today | replacement |
|---|---|---|---|
| 9926, 9928 | `layout_dom_recursive_impl` (A3-window) | `if !transform_keys.contains_key(nid) { insert key; insert value }` | `gpu_cache.register_transform(T::ScrollbarVertical, *nid, transform_key, info.thumb_initial_transform);` |
| 9934, 9936 | same | the horizontal thumb | `gpu_cache.register_transform(T::ScrollbarHorizontal, *nid, transform_key, info.thumb_initial_transform);` |
| 9981, 9985 (E) | same | `scrollbar_v_opacity_keys.entry(key)` vacant: insert key and value | `gpu_cache.register_scrollbar_opacity(ScrollbarOrientation::Vertical, dom_id, *nid, opacity_key, initial_opacity);` |
| 9992, 9996 (E) | same | the horizontal bar | `gpu_cache.register_scrollbar_opacity(ScrollbarOrientation::Horizontal, dom_id, *nid, opacity_key, initial_opacity);` |
| 10397, 10398 | `update_drag_visual` (A3-window) | remove key and value of the old node | `cache.remove_transform(T::Animation, old);` |
| 10405, 10408 | same | `keyed = contains_key`; mint a key; insert the value | `let keyed = cache.transform_key(T::Animation, node).is_some(); cache.key_transform(T::Animation, node, ComputedTransform3D::new_translation(x, y, 0.0));` |
| 10430, 10431 | `end_drag_visual` (A3-window) | remove key and value | `cache.remove_transform(T::Animation, node);` |
| 12266 | `sync_sheen_keys` (A3-window) | `cache.sheen_light = light;` | `cache.set_sheen_light(light);` (`same_light` is read before it, as now) |
| 12270, 12272 | same | two `retain` | `cache.retain_transforms(T::Sheen, |node| wanted.contains(&node));` |
| 12274, 12276 | same | two `retain` | `cache.retain_opacities(O::Sheen, |node| wanted.contains(&node));` |
| 12280 (E), 12284 (E) | same | `entry(*node).or_insert_with(unique)` twice | `cache.ensure_transform_key(T::Sheen, *node); cache.ensure_opacity_key(O::Sheen, *node);` |
| 12305, 12309 | `publish_sheen_values` (A3-window) | insert transform value, insert opacity value | `cache.set_transform_value(T::Sheen, node, ..translation..); cache.set_opacity_value(O::Sheen, node, surface.amount);` |
| 18889 (E), 18893 | `tick_animations` (A3-window) | `anim_transform_keys.entry(..).or_insert_with(unique)`; insert value | `cache.key_transform(T::Animation, node_id, <the matrix>);` |
| 18896 (E), 18900 | same | the opacity pair | `cache.key_opacity(O::Animation, node_id, <the opacity>);` |
| 18929-18932 | same | four `remove` | `cache.remove_transform(T::Animation, node_id); cache.remove_opacity(O::Animation, node_id);` |
| 19172 | same | `current_color_values.insert(tr.node, to_c)` | `cache.set_color_value(tr.node, to_c);` |
| 19194, 19195 | same | remove colour key and value | `cache.remove_color(tr.node);` |
| 19202 (E), 19205 | same | `color_keys.entry(..)`; insert value | `cache.key_color(tr.node, to_c);` |
| 19436-19439 | same | four `remove` | `cache.remove_transform(T::Animation, node_id); cache.remove_opacity(O::Animation, node_id);` |
| 19458-19461 | same | four `remove` on `view_cache` | `view_cache.remove_transform(T::Animation, node); view_cache.remove_opacity(O::Animation, node);` |
| 19494, 19496 | `retain_running_color_keys` (A3-window) | two `retain` | `cache.retain_colors(|node| running.contains(&node));` |
| 19539, 19542 | `release_undriven_animation_values` (A3-window) | two `retain` | `cache.retain_transforms(T::Animation, <the same test, by value>);` |
| 19545, 19548 | same | two `retain` | `cache.retain_opacities(O::Animation, <the same test, by value>);` |
| 19694, 19696 | `synchronize_scrollbar_opacity` (A3-window) | insert key, insert value (vertical) | `gpu_cache.set_scrollbar_opacity(ScrollbarOrientation::Vertical, dom_id, node_id, opacity_key, <value>);` |
| 19708 | same | insert value | `gpu_cache.set_scrollbar_opacity_value(ScrollbarOrientation::Vertical, dom_id, node_id, <value>);` |
| 19722, 19723 | same | remove key and value | `gpu_cache.remove_scrollbar_opacity(ScrollbarOrientation::Vertical, dom_id, node_id)` (returns the key the event needs) |
| 19739, 19741, 19753, 19767, 19768 | same | the horizontal bar | the three calls above with `ScrollbarOrientation::Horizontal` |
| 22251 (E), 22254 | `publish_live_sample` (A6) | `anim_transform_keys.entry(..)`; insert value | `cache.key_transform(T::Animation, node, <the matrix>);` |
| 22271 (E), 22274 | same | the opacity pair | `cache.key_opacity(O::Animation, node, smp.opacity);` |

64 lines, 10 functions. The methods return what the surrounding code reads today (the key, the previous value, or
whether something changed): see their docs in `core/src/gpu.rs`. `git grep` recipe that lists them again after other
merges: the script of this session,
`/private/tmp/claude-501/-Users-fschutt-Development-m4p/dee24094-6dae-421c-a577-867672149470/scratchpad/a2gpu_writes.py <rev>`
(pure python over `git show`; it takes some test modules for production code: check the function name it prints), or
`git grep -nE '\.(anim_|css_|sticky_|sheen_|h_)?(current_)?(transform|opacity|color)_(keys|values)\b|scrollbar_[vh]_opacity_(keys|values)|sheen_light' <rev> -- layout dll`.

**Test code that writes the maps** (compiles as long as the fields are public; under (I) each becomes the method of
the same name as above): `layout/src/managers/gpu_state.rs` (28, the tests before `audit_a2_gpu_tests`),
`layout/src/cpurender/raster.rs` 11352-11468 (20), `layout/src/audit_tests/a9.rs` (6), `layout/src/managers/mod.rs`
480-485 (5), `layout/src/audit_tests/a6.rs` (2), `layout/src/headless.rs` 5554-5558 (`set_frame`, 2:
`cache.key_transform(TransformChannel::Css, node, matrix)`), `layout/src/solver3/mod.rs` 3848-3850 (2),
`layout/src/window.rs` tests (2), `layout/tests/a_node_mid_slide_is_hit_where_it_is_painted.rs` (2),
`layout/tests/a_sliding_box_carries_its_positioned_children.rs` (2), A4's `dll/src/desktop/audit_tests/a4.rs`
(`bind_opacity`). `core/src/gpu_test.rs` is a child module of the cache and is not affected.

**Readers of the generation** (none yet): `synchronize_gpu_values` (A4: keep the generation sent next to
`last_gpu_value_digest`, return before the walk when `gpu_state_manager.value_generation()` is unchanged, forget it in
`invalidate_submitted_digests`), `CpuBackend::render_frame` / `WindowGpuValues::of_doms` and `window_value_damage`
(A1 / A9's H4, H6: reuse `previous_gpu_values` when it is unchanged). Only equality is meaningful.

**This branch's later commits** (`cc1fd463b`, `898ea1234`, `2151bbf0a`, the notes) touch `gpu_state.rs` (identical on
integration to `98df549bb`), one hunk of `scroll_state.rs` after `any_nonzero_offset` (A6's hunks are elsewhere), one
function of `window.rs` (identical on every branch) and `ensure_transform_key`: no conflict expected.

## For area A4

Applied by A4 already: A2-gpu-01 (`d6e9a9b0f`), A2-gpu-02 (`85d19c71a`), A2-gpu-12 (`689b5367f`). Open:

- **A2-gpu-14, two lines.** `dll/src/desktop/wr_translate2.rs`, in `build_webrender_transaction` ("Step 1.8", line
  2773 at base, 3175 at A4's tip, 2787 on integration) and in `build_image_only_transaction` ("Step 3.5", 3112 at
  base, 3569 at A4's tip, 3126 on integration):
  ```rust
  -            gpu_state_manager.update_scrollbar_transforms(
  +            gpu_state_manager.refresh_scroll_driven_transforms(
  ```
  Same arguments, same return value, same values. Both run on trees the layout pass already walked
  (`window.rs:9608` and the refresh after registration, `:3753`). Without the change both sites keep the walk, at its
  new cost per box (two field reads instead of two map lookups and an extent).
  Counter for your frame report if you want it: `gpu_state_manager.scroll_boxes_looked_at` (difference per frame).
- **A2-gpu-15**: the thumb frame's spatial key, `## Review of A4` (`689b5367f`).
- **The reader of the write generation**: `## Integration`, "Readers".
- Not taken by A4, still true, no behaviour change: `compositor2.rs:284-310` (`offset_stack` is never pushed, the
  `offset` parameters are constant zero), `:1067` (a scroll frame defines the clip its `PushClip` just defined).

## For area A6

- `layout/src/managers/scroll_state.rs`: I added `ScrollManager::nodes_with_a_bar(dom_id)` after `any_nonzero_offset`
  (`cc1fd463b`): the nodes of a DOM whose state has a bar on an axis, in node order, without a copy
  (`states.range((dom_id, NodeId::ZERO)..)`). A frame holds the list of scroll-driven boxes against it.
- `layout/src/window.rs` `refresh_scrollbar_gpu_cache_for_cpu_frame` (your range): `self.refresh_scrollbar_transforms()`
  became a loop over `layout_results` calling `gpu_state_manager.refresh_scroll_driven_transforms(..)`; the opacity
  loop is unchanged.
- Your design of A6-05b / "raised bars" (the flags a registration raises kept in the scroll manager instead of written
  into the tree): `update_scroll_driven_box` (`gpu_state.rs`) is the one place of this area that reads
  `warm.scrollbar_info`; it would call your `ScrollManager::scrollbar_info(dom, layout_index, tree)` there. The frame
  form needs nothing else: a raised bar is a registered bar, which `nodes_with_a_bar` already reports.
- `headless.rs` `ScreenMapAffine::invert`: for a matrix without an inverse (det 0) it returns non-finite entries and
  its callers take the non-finite point for a miss. That is the rule ("hits nothing"), but it holds by the callers'
  comparisons, not by a test of the determinant; `ComputedTransform3D::try_inverse` (`548271af4`) is the function
  that says `None`.
- `publish_live_sample` and the test helper `headless.rs::set_frame`: rows of `## Integration`.
- `transform_stamps()` (yours) lists the six maps `reference_frame_of` reads. `frame_of_channels` composes CSS,
  animation and sticky: a channel added to that product has to be added to the stamps.

## For area A11

- `pending=<n>` of `fp_gpu_state` (`layout/src/e2e/full.rs:12549-12555`) counts the events of the last sync. Since
  `30065ea07` a sync of unchanged values adds 0 (it added one `Changed` per transformed or faded node). The four
  azul scenarios that compare manager fingerprints (`e2e/noninterference-idle-tick-moves-no-manager.json`,
  `noninterference-scroll-does-not-move-focus.json`, `noninterference-tab-focus-does-not-move-scroll.json`,
  `op-image-swap-repaints.json`) have no `transform` and no `opacity` in their DOMs: their numbers do not move. The 13
  scenarios of `~/Development/m4p/mapedit3/e2e/*.json` use no manager snapshot. Nothing to update on either side.
- `fp_gpu_state` reads six maps of the cache by field (`full.rs:12538-12542`); if the fields become private:
  `cache.transform_keys_of(TransformChannel::ScrollbarVertical).count()` and so on.
- A counter for a guard, if wanted: `LayoutWindow::gpu_state_manager.scroll_boxes_looked_at` (A2-gpu-14). A frame of
  the Tuebingen view should add the number of its scroll and sticky boxes per DOM, not its ~12.5k boxes.

## For area A2-cpu

Both leads were fixed by A2-cpu (its A2-cpu-14 `e14f80eb3`, A2-cpu-17 `c5f25d10d`); kept for the record:

- (A2-gpu-05) `compositor.rs:1325-1328`: the layered compositor rounded a frame's pivot to whole device px
  (`layout_offset_device_px`); `raster.rs:1229-1232` (flat walk) and the GPU use the unrounded corner.
- (A2-gpu-05) `raster.rs:6091-6101`: a solid border's device width was `sw as i32`; a width under one device px
  painted nothing, the GPU paints one device px (`get_webrender_border`, `device_width`).
- WebRender's rule, to align the CPU with if one rule is chosen (your A2-cpu-06): scroll offsets are snapped to whole
  device px first (`round`), then every axis-aligned rect and clip edge is rounded to the nearest device px; an
  iframe's (view's) box is rounded before its content is placed.

## For area A1

- (A2-gpu-06, open in A1's ledger) `raster.rs:3744-3748`: the opacity group's region is the node's box; content of
  the group outside the box is painted at full opacity (the GPU fades it). Proposed: the region = the union of the
  visual bounds of the items from the push to its pop (what `reference_frame_extent` computes for a frame), inside
  the open clip.
- (A2-gpu-07, fixed by A1 `3445773d4`) `raster.rs:3359-3364`: `ScrollBarStyled` ignored the value of its opacity key
  except as an on / off gate.
- (cross-check, Border; A1 holds it as `blocked (a)`: its rasteriser has one radius per corner) `raster.rs:3125-3146`
  resolves a percentage `border-radius` of a `Border` item against the box's WIDTH for both axes of every corner; the
  GPU resolves the horizontal radius against the width and the vertical one against the height. `border-radius: 50%`
  on a bordered 100 x 40 box: an ellipse of 50 x 20 on the GPU, a 50 px arc (then clamped) on the CPU.
- The reader of the write generation on the CPU path: `## Integration`, "Readers".

## What to look at on the real window

No change of this area draws differently by default except where a finding says the old picture was wrong:

1. GPU window on the Retina panel, a tilted map (`perspective`): the tilt is the CPU backend's (A2-gpu-01 through
   A4's `d6e9a9b0f`). Before, it was twice as strong at 2x.
2. A node with a CSS `transform` that is dragged, or that slides in by an animation: it follows the pointer / slides
   (A2-gpu-10). Before, it stayed at its CSS matrix until the drop. A fading node with a CSS opacity fades from that
   opacity, not from 1.
3. CPU window: scroll a list with a visible scrollbar; the thumb follows, a `position: sticky` header holds, and a
   text field that begins to overflow while typing gets a thumb that moves (A2-gpu-14). If a thumb or a sticky box
   ever stays behind: start with `AZ_WALK_SCROLL_BOXES=1` in the environment. If it is right then, the list of
   scroll-driven boxes is the cause (default: the list, because values and counts are tested headless).
4. A long zoom on the GPU window: the frame time does not grow with the number of zoom steps (A2-gpu-02 / A4-01).

## What this area did not read

- `layout/src/cpurender/compositor.rs` beyond `is_identity_2d`, `translation_2d`, `composite_layer_recursive`'s matrix,
  `affine_rect_about`, `layout_offset_device_px` (A2-cpu audits the rest); `pixmap.rs` blits (A1).
- hit-testing on either path (`headless.rs`, `AsyncHitTester`: A6), present rects and their y flip (A10), the halo
  images' placement (A5), glyph-cell quantisation (A8).
- `compositor2.rs`: the gradient, filter and image-mask arms were read for their coordinates only; the test modules not.
- `wr_translate2.rs`: resources, fonts, images, overlay uploads (lines 660-1860) not read.
- the WebRender fork: `spatial_node.rs` (update of frames and scroll frames), `scene_building.rs::push_iframe`,
  `scene.rs::SceneProperties`, `render_api.rs::Transaction`, the device-pixel-scale uses in `frame_builder.rs`; for the
  review also `render_backend.rs::update_document`, `res/ps_text_run.glsl`, `glyph/src/font.rs`, the text arm of
  `batch.rs`. WebRender's rect snapping (`snap_rect`) is quoted from the names and upstream behaviour, not re-read.
- `layout/src/solver3/scrollbar.rs`: the geometry functions read, the tests not. `window.rs`: only
  `virtual_view_content_offset` .. `patch_virtual_view_content_offset`, `refresh_scrollbar_transforms`,
  `refresh_scrollbar_gpu_cache_for_cpu_frame`, `synchronize_scrollbar_opacity`, `compute_scroll_ids`, the GPU sync in
  the layout pass (8700-8930, 9500-9610), `reclaim_unchanged_child`, the writers of `## Integration`,
  `write_bound_gpu_properties`, `publish_gpu_only_restyle`, `sync_css_gpu_values`.
- `layout/src/managers/scroll_registration.rs`: `register_scroll_nodes` up to the registration call (for A2-gpu-14);
  `scroll_state.rs`: `register_or_update_scroll_node`, `set_scroll_position`, the accessors.
- sticky frames' offsets (`StickyConstraint::shift_at`) and the sheen layer's values: the frames are ordinary
  reference frames on both backends; the values themselves were not checked.
- the CPU path's per-glyph rounding inside a transformed group (asked by the review of `56bb92351`): not re-read.

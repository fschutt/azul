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
- A7-editor's `mapedit3/src/ui/main/map.rs` (m4p): minimal patches, bigger things to `## For area A7-editor`.

Tests: `dll/src/desktop/audit_tests/a2_cpu.rs`, `layout/src/audit_tests/a2_cpu.rs`, and `audit_a2_cpu_tests` modules
directly below the functions whose private items they need.

## Spaces

The coordinate spaces of the CPU path. Every space is y-down with its origin at a top-left corner. "Logical px" = CSS px;
"device px" = logical px x `dpi_factor` (the window's hidpi factor, `current_window_state.size.get_hidpi_factor()`).
Read this before writing coordinate code; the conversion rules below are what the rasteriser does today, and what every
damage / shift / hit-test function has to agree with.

| # | space | origin | unit | what is stored in it | converts to |
|---|---|---|---|---|---|
| L | **list space** of a display list (also called content coords, layout space) | the list's DOM: for the root list the window's top-left, for a `VirtualView` child list the child DOM's (0, 0) | logical px | every `DisplayListItem` rect: `bounds`, `clip_rect`, `line_box`, `GlyphInstance.point`, `PushClip.bounds`, `PushScrollFrame.clip_bounds`, `PushReferenceFrame.bounds`, `VirtualView.bounds`; `reference_frame_extent`; `FrameIndex::opaque_fills`; `visual_bounds()` | P by the walk below |
| P | **painted space** of a list (called "on screen", "viewport space", "window space" for the root list) | same origin as L of the same list | logical px | where the list's items are painted in the surface the list is painted into. Root list: damage rects (`LogicalRect` in `FrameDamage::Rects`, `render_display_list_damaged`'s `damage_rects`), `ViewShift.clip`, the `clip` of `collect_scroll_shifts`, `present_extra`, `ScrollStack.open[..].2`, `ScrollStack.clips` | D by `x dpi_factor`; a child list's P goes to the PARENT list's L by the view rule |
| D | **device pixels of the frame** (`AzulPixmap` of the window: `CpuBackend.last_frame` or the native target) | window top-left | device px; pixel `i` covers `[i, i+1)`, its centre is `i + 0.5` | `AzRect` clips of `clip_stack` / `real_clip_stack` (f32, NOT snapped), `SnappedRect.x0..y1`, `TransformGroup.outer_clip`, `PixelBox`, blit clips `(x0, y0, x1, y1)` half-open | present rects (A10) |
| G | **group surface pixels** of an isolated reference frame (`TransformGroup`) | device pixel `(x, y) = floor(extent_P.origin x dpi)` of the surface below it | device px | the group's `AzulPixmap`, the `AzRect` clips pushed while the group is open, `group_visible_part`'s result | the surface below by `TransformGroup.matrix` |
| Y | **layer pixels** of the retained compositor (`Layer.pixbuf`; full repaints only) | `layer.bounds.origin + layer.scroll_offset` in L | device px | `Layer.pixbuf`; `Layer.bounds` is L (logical), EXCEPT the root layer's `bounds.size`, which is the pixmap size in device px | output by `composite_layer_recursive` |

`pixel_w = ceil(width x dpi)`, `pixel_h = ceil(height x dpi)` (`CpuBackend::render_frame`): the frame can be a fraction of
a device pixel larger than the window's logical size.

### L -> P: the walk (one rule, `raster.rs` `render_single_item`; mirrored by `compositor.rs` `ScrollStack` + `FrameStack`)

State: an accumulated offset `acc` (the top of `scroll_offset_stack`; the walk paints at `pos - acc`) and the open
reference frames.

1. **Scroll frame** `PushScrollFrame { scroll_id }` with offset `s = scroll_offsets[scroll_id]` (positive = scrolled
   down / right): `acc += s`. An item at L position `p` inside it is painted at `p - s`. The frame's own `clip_bounds`
   and the `PushClip` in front of it are in the space AROUND the frame (not moved by `s`).
2. **Reference frame** `PushReferenceFrame { bounds, transform_key, initial_transform }`. The matrix is
   `render_state.transforms[transform_key.id]`, else `initial_transform` (the fallback is `initial_transform`, never the
   identity). `ComputedTransform3D.m` is a ROW-vector 4x4:
   `x' = x m[0][0] + y m[1][0] + m[3][0]`, `y' = x m[0][1] + y m[1][1] + m[3][1]`, `w = x m[0][3] + y m[1][3] + m[3][3]`
   (translation in logical px). `transform-origin` is already folded in by the producer
   (`core/src/transform.rs` `from_style_transform_vec`: `T(-origin) . M . T(origin)` in the node's own box, last listed
   function applied first). The matrix acts **about the frame's origin `o = bounds.origin - acc`** (the box's top-left
   after the scroll frames around it): a point painted at `q` without the frame is painted at `o + M (q - o)`.
   Three cases (`ReferenceFrameGroup::begin`):
   - identity within 1e-4 (`is_identity_2d`: the four linear terms and `m[3][0]`, `m[3][1]`): nothing;
   - translation within 1e-4 (`translation_2d`): `acc -= (tx, ty)` (no resampling, fractional `tx` allowed);
   - anything else: an isolated group (space G).
3. **View** `VirtualView { bounds, content_offset, child_dom_id }`: the child list (space L of the child) is walked inline
   with `acc_child = acc - bounds.origin - content_offset`, clipped to `bounds - acc` (the BOX, not `clip_rect`). So a child
   point `c` is at parent-L position `bounds.origin + content_offset + c`.
   `content_offset = materialized_window_origin - scroll_offset`, snapped to whole device pixels
   (`LayoutWindow::virtual_view_content_offset`): a view scrolled DOWN by 300 px carries `content_offset.y = -300`.
   As a scroll delta: `delta = old_content_offset - new_content_offset`, `offset = -new_content_offset` (`ViewShift`).
4. Frames nest in list order. Example: scroll frame S (offset `s`) around frame F (matrix `M`, box origin `o`) around item
   at `p`: painted at `(o - s) + M (p - o)`. Frame F around scroll frame S around `p`: `o + M (p - s - o)`.
   NOT `T_total(p - s_total)`: that formula (the comment of `headless::ResolvedChain`) is right only when every matrix is
   a translation, see A2-cpu-03.

Numeric example (all in logical px, dpi 1): view box at (50, 38) in the root list, `content_offset = (-12, 0)`, picture
root frame in the child at child-L (0, 0) with `M = scale(1.5) then translate(20, 10)` (the editor's zoom), a label at
child-L (100, 40). Child P: `0 + 1.5 x 100 + 20 = 170`, `1.5 x 40 + 10 = 70`. Root L = root P = `(50 - 12 + 170, 38 + 70)`
= **(208, 108)**. Device px at dpi 2: (416, 216).

### P -> D and the rounding rules in force (they are NOT one rule: A2-cpu-06)

| what | near edge | far edge | where |
|---|---|---|---|
| damage rect -> cleared / repainted box | `floor(x dpi)` | `ceil((x + w) dpi)`, clamped to the pixmap | `render_display_list_damaged` (`snap_out`) |
| scroll / view memmove region | `floor` | `ceil`, clamped | `scroll_shift_region_impl` |
| opaque square rect fill | `round` | `round` | `render_rect` (`round_edge`) |
| translucent rect, agg paths, borders, text clip, image clip (`AzRect` clip -> pixel box) | `trunc` (`as i32`) | `trunc` | `agg_fill_path_clipped`, `render_rect` (blend_bar), `text_clip_pixel_box`, `render_image` |
| box-shadow blit clip | `trunc` | `ceil` | `render_box_shadow` (`blit_clipped`) |
| text-shadow output box, filter group | `floor` | `ceil` | `text_shadow_boxes`, `FilterGroup::begin` |
| opacity group region | `trunc(x)` | `trunc(x) + trunc(w)` | `PushOpacity` arm |
| isolated group: surface box | `floor` | `ceil` | `ReferenceFrameGroup::begin` |
| isolated group / layer: composite clip | `round` | `round` | `ReferenceFrameGroup::begin` (`outer_clip`), `composite_layer_recursive` (`static_clip`) |
| layer placement | `round((pivot - inherited) dpi)` | | `layout_offset_device_px` |
| scroll shift distance | `round(new dpi) - round(prev dpi)` (not `round(delta dpi)`) | | `scroll_shift_region_impl` |

Rects whose device edges are whole numbers (dpi 1 or 2 with integral layout) agree under every rule; at dpi 1.25 / 1.5
or with fractional layout positions they differ by one pixel column / row.

### G: the isolated group (`ReferenceFrameGroup::begin`, `TransformGroup`)

- surface box in the pixels below: `x = floor(e.x dpi)`, `x1 = ceil((e.x + e.w) dpi)`, same for y, where
  `e = reference_frame_extent - acc` (what the frame paints, in its own UNTRANSFORMED space: its box and the visual
  bounds of every item down to its pop, see A2-cpu-04 for what that leaves out);
- inside the group the walk paints with `acc_group = acc + (x, y) / dpi`: an item at L `p` lands on group pixel
  `(p - acc) dpi - (x, y)`;
- group pixel `g` (continuous, corner coordinates) -> pixel below: `matrix = T(o_d) . Lm . T((x, y) - o_d)` with
  `o_d = (bounds.origin - acc) dpi` and
  `Lm = [m00 m10 m30 dpi; m01 m11 m31 dpi; m03 / dpi m13 / dpi m33]` (column-vector `Mat3`, row-major);
- the composite samples at destination pixel centres: `src = inv(matrix)(d + 0.5) - 0.5`, bilinear, taps outside the
  surface are transparent (the edge of a group fades over one source texel); a matrix without shear takes the separable
  fixed-point path (`blit_scale.rs`, 8-bit weights, same sampling points);
- `group_visible_part` maps the clip open around the push (pixels below, half-open) back through the inverse of the
  affine `matrix` and widens it by 64 group pixels; the walk paints (and culls) inside that part only.

Example (dpi 1): frame box origin `o = (10, 20)`, extent `(10, 20) 100 x 50`, `M = scale(2)`: surface 100 x 50 at
`(x, y) = (10, 20)`, `matrix = T(10, 20) . S(2) . T(0, 0)`: group pixel (30, 5) -> (10 + 60, 20 + 10) = (70, 30). With the
clip `[0, 110) x [0, 60)` below, `group_visible_part` = corners back: `((0 - 10) / 2, (0 - 20) / 2) = (-5, -10)` and
`((110 - 10) / 2, (60 - 20) / 2) = (50, 20)`, widened by 64 and cut to the surface: `(0, 0, 100, 50)`.

### Who adds, who subtracts (producers against consumers)

| quantity | producer | consumers | agree? |
|---|---|---|---|
| scroll offset `s` (paint at `p - s`) | `ScrollManager::build_scroll_offset_map` | raster `scroll_rect`; `ScrollStack::step` / `moved_by`; `compute_display_list_damage_impl` (`offset_stack`); `InPlaceShift::summed_offset` | yes |
| `content_offset` (child at `bounds.origin + content_offset + c`) | `virtual_view_content_offset` (`window_origin - scroll`, snapped) | raster `VirtualView` arm (`acc - origin - content_offset`); `view_damage_in_list` `lift` (`+ origin + content_offset`); `list_value_damage` (`+`); `headless::resolve_virtual_view_placements` (`+`); `execute_virtual_view_shift` `covered_at` (`- (around - origin + scroll)`, `scroll = -content_offset`); `virtual_view_frame_scroll_offsets` (`-content_offset`, for WebRender's pipeline scroll) | yes (Z1 fixed the one that subtracted) |
| frame matrix about `o` | `GpuValueCache` / `initial_transform` | raster `ReferenceFrameGroup::begin`; `affine_rect_about`; `FrameStack::map`; layers `composite_layer_recursive` (`pivot`) | yes among these; `headless::resolve_chain` (hit-test, IME, `window_space_offset_of_dom`) applies it about the window origin: A2-cpu-03 |
| fallback matrix of a key without a value | | raster: `initial_transform`; `ScrollStack`, `FrameValues`, layers, `node_groups_on_screen`: `initial_transform`; `gpu_value_damage` changed-frame extent: identity | no: A2-cpu-01 |
| frame translation as an offset | `translation_2d` | raster: `acc - t`; `InPlaceShift::Translation`: `(-tx, -ty)`; `ScrollStack::placement`: `scrolled - t` | yes |
| view shift delta | `collect_virtual_view_shifts`: `old_off - new_off` | `scroll_shift_region_impl`: positive = content moves up / left, strip at the bottom / right | yes, all eight directions (checked: section Shift below) |

## Findings

(first batch below; the list grows)

### A2-cpu-01 `gpu_value_damage` maps a changed frame whose key has no value with the identity; the rasteriser uses `initial_transform`
- file:line `layout/src/cpurender/compositor.rs:2861-2862` (`gpu_value_damage`)
- class: sign/offset (wrong fallback matrix: damage lands where nothing was painted)
- evidence: lines 2848-2855 compute `old_m` / `new_m` with `unwrap_or(*initial_transform)` (what `raster.rs:2906` paints
  with), lines 2861-2862 shadow them with `unwrap_or(&identity)` for the extent of the changed frame. A key bound by the
  list can be missing from a value map (`GpuValueCache::for_each_bound_value` skips a key without a value). Example: frame
  box origin (10, 20), extent 100 x 50, `initial_transform = translate(100, 0)`, old value `translate(50, 0)`, new: no
  value. Painted now at x 110..210. Old code: damage `[60, 160)` (old) and `[10, 110)` (new, identity): `[160, 210)` is
  not repainted. Expected new rect: `[110, 210)`.
- fix: use the `old_m` / `new_m` already computed (drop the two shadowing lines).
- test: `a_changed_frame_without_a_live_value_is_damaged_where_its_initial_transform_paints_it`
- status: open (patch after `audit-A1-structure`)

### A2-cpu-02 open question 1 of `CPU_RENDERING.md`: why the zoom-out frame at k = 1.000 was an isolated group
- file:line `css/src/props/basic/length.rs:223` (`FloatValue::new`), `css/src/cast.rs:40`, `compositor.rs:70-84`
- class: rounding
- evidence: `FloatValue::new(v)` is `(v * 1000.0) as isize`: it TRUNCATES toward zero. A zoom in and out again leaves
  `k = s / s0` one ulp under 1 (0.99999994): `0.99999994 * 1000 = 999.99994 -> 999 -> 0.999`. `is_identity_2d` wants
  `|m00 - 1| < 1e-4`; `|0.999 - 1| = 1e-3`: not the identity, not a translation, so `ReferenceFrameGroup::begin` opened an
  isolated group of the whole picture (5.78 MP) and resampled it at 0.999: the 216 ms of the "identity" frame. A `k` one
  ulp above 1 (1.0000001 -> 1000 -> 1.000) was the identity, so the cost depended on the direction of the rounding error.
  The editor's snap (`map.rs` `snap_transform`, `0095bba`: `|k - 1| < 1e-3 -> 1`) covers the range the truncation can
  produce ((0.999, 1.001) exclusive), and writes no `Scale` at all for `k == 1`, so the answer is closed for the editor.
  The engine side remains: `FloatValue::new` truncates, so `scale(0.29)` is stored as 0.289 and `translate(-0.0007px)` as
  0 while `+0.9996` becomes 0.999 (A2-gpu's file; noted for them).
- status: answered by reading; the editor side is fixed (`0095bba`), see A2-cpu-20 for the test that pins it.

### A2-cpu-03 the hit-test / IME chain applies reference-frame matrices about the window origin; the rasteriser applies them about the frame's origin
- file:line `layout/src/headless.rs:349-382` (`resolve_chain`), `:321-346` (`ResolvedChain`), `:728-746`
  (`nested_dom_window_origin`, which `window_space_offset_of_dom` returns)
- class: sign/offset
- evidence: `resolve_chain` sums every scroll offset and composes the raw matrices: `screen = T_total(pos - scroll_total)`.
  The raster paints `o + M (p - o)` with `o` the frame's box origin after the scroll frames around it (space rule 2). The
  two agree only for translations (or `o = (0, 0)` and no scroll frame outside a non-translation). Example: a label box at
  L (300, 200), 100 x 12, `transform-origin: 0 12px; transform: rotate(90deg)`: its local point (50, 6) is painted at
  `(300, 200) + (6, 62) = (306, 262)`. The chain maps the screen point (306, 262) back with `M^-1` about (0, 0):
  `(250, -294)`, outside the box `[300, 400) x [200, 212)`: a press on the painted label misses. Expected local point:
  `o + M^-1 (s - o) = (350, 206)`.
- fix: for A6 (it claims `resolve_chain` and `nested_dom_window_origin`): see `## For area A6`.
- status: open (A6)

### A2-cpu-04 `reference_frame_extent` leaves out what the frames INSIDE a frame move, so the surface, the layer and the value damage of the outer frame miss it
- file:line `layout/src/solver3/display_list.rs:3536-3607` (`GroupBounds`), `:3683-3767` (`FrameIndex::of`); consumers
  `raster.rs:2934`, `compositor.rs:786`, `:2858`, `:5170`
- class: wrong-output / sign-offset (extent in the wrong space)
- evidence: `GroupBounds::step` unions the L bounds of every item between the push and the pop, also of items inside a
  nested `PushReferenceFrame`, which are painted through that inner matrix. Example: column frame F (box (0, 0) 200 x 400,
  animated `translateY`), inside it a card frame G (box (0, 0) 200 x 80) with `translateX(-50%)` = `translate(-100, 0)`: the
  card is painted at x -100..100, F's extent is x 0..200. (a) value damage of F (`gpu_value_damage`) covers x 0..200 moved
  by F's old and new matrix: the left half of the card (x -100..0) leaves a trail; (b) when F is an isolated group
  (scale), its surface is x 0..200: the left half of the card is cut off.
- fix: for A1 (owner of `FrameIndex`): see `## For area A1`.
- status: open (A1)

### A2-cpu-05 a changed `PushTextShadow` damages nothing in the paired diff
- file:line `compositor.rs:5607-5624` (`compute_display_list_damage_impl`), `display_list.rs:3461` (`bounds()` is `None`)
- class: wrong-output (missed damage)
- evidence: two lists that pair item by item and differ only in a `PushTextShadow { shadow }` (the halo's colour or
  width): `is_visually_equal` is false for the push, the arm falls through to `old_item.visual_bounds()` /
  `new_item.visual_bounds()`, both `None`: no rect. The `Text` items under it are equal. Result `Some(vec![])`: the root
  list's frame is idle (nothing repaints); in a view the whole view is damaged only when the diff found NOTHING
  (`view_damage_in_list`), so a halo change together with any other change keeps the old halo.
- fix: on a changed `PushTextShadow` pair, return `None` (the windowed path already gives up on a shadow in its window).
- test: `a_changed_text_shadow_is_not_an_idle_frame`
- status: open (patch after `audit-A1-structure`)

### A2-cpu-06 one clip, six ways to round it: content of an isolated group shows one pixel past a fractional clip, and the memmove of a pan drags a neighbour's column
- file:line `raster.rs:1216-1223` (`outer_clip`: round / round), `pixmap.rs:1519-1524` and five more sites (trunc /
  trunc), `raster.rs:572-575` (trunc / ceil), `raster.rs:3757-3760` (trunc + trunc of the width),
  `compositor.rs:2043-2048` (floor / ceil)
- class: rounding
- evidence: table "P -> D" above. Example at dpi 1.25: a view box at logical x 50, width 1230 -> device 62.5 .. 1600.
  Direct paints of the view's content (fills, text, images) are clipped to columns `[62, 1600)`; during a zoom the
  picture is an isolated group and its composite is clipped to `[round(62.5), round(1600)) = [63, 1600)`: column 62 shows
  the backdrop for the length of the zoom. Second example, device edge 1600.5: fills stop at column 1600 (exclusive), the
  pan's memmove region is `[62, ceil(1600.5)) = [62, 1601)`: a pan to the right copies the view's column
  `1600 - shift` over the neighbour's column 1600, and the strip that is repainted is on the left.
- fix: one helper for "the pixel box of a clip" used by all of them. Minimal version (mine): the group's `outer_clip`
  and the memmove region take the fills' rule. Full version: for A1, see `## For area A1`.
- status: open

### A2-cpu-07 `execute_virtual_view_shift` checks that the DELTA is a whole device pixel but moves by the difference of the two rounded OFFSETS
- file:line `compositor.rs:3671-3683` (gate), `:3740` -> `:2028-2032` (distance)
- class: rounding
- evidence: gate: `|delta dpi - round(delta dpi)| < 1e-3`. Distance: `round(-new_off dpi) - round(-old_off dpi)`. With
  offsets that are not whole device pixels the two differ: `-old_off dpi = 10.4999`, `-new_off dpi = 13.5004`: delta
  3.0005 passes the gate, distance `14 - 10 = 4`: the pixels move 4, the content 3. Second case `10.5003 -> 11.4999`:
  delta 0.9996 passes, distance `11 - 11 = 0`: `strips.is_empty()` returns NO damage for content that moved a pixel.
  Today's caller is safe: `virtual_view_content_offset` snaps to whole device pixels, so both offsets are integers (to an
  ulp). Any other producer of `content_offset` (a test, a future caller, the e2e runner) hits it.
- fix: move by `round(delta dpi)`: hand `scroll_shift_region` the rounded delta as both delta and offset. For A1
  (it claims the function): see `## For area A1`.
- status: open (A1)

### checked: fine
- A2-cpu-08 `ReferenceFrameGroup::begin` matrix `T(o) . Lm . T((x, y) - o)` against `affine_rect_about` / `FrameStack::map`
  (`o + M (p - o)`) and against the layer path (`place . layer_h . from_pivot`): same mapping; `Lm`'s perspective row is
  divided by dpi and the translation multiplied, as `composite_layer_recursive` does. checked: fine.
- A2-cpu-09 `group_visible_part`: the inverse `((d dx - b dy) / det, (-c dx + a dy) / det)` is the inverse of
  `x' = a x + b y + tx, y' = c x + d y + ty`; the corners are pixel edges in both spaces; the margin of 64 covers the two
  bilinear taps and a blur of up to 64 device px inside the group; non-invertible (`|det| < 1e-9`), non-finite and
  perspective matrices keep the whole surface. checked: fine. (A blur wider than 64 px at the cut is A2-cpu-12.)
- A2-cpu-10 `blit_scale::axis`: `s = (d + 0.5) / scale - shift / scale - 0.5` is the general path's
  `inv(d + 0.5) - 0.5`; weight `floor(frac x 256)` in 0..=255; `lerp` cannot overflow a 16-bit lane
  (`255 x 256 + 128 < 65536`); weight 0 returns the texel exactly (scale 1 with a whole shift is a copy); the last row /
  column (`s` in `[extent - 1, extent)`) blends with a transparent tap, like the general path; negative scale and
  negative translation go through the same formula; bands write disjoint row ranges. checked: fine.
- A2-cpu-11 `scroll_shift_region_impl` and its three movers, all eight directions: see section Shift. checked: fine
  (except the far-edge rounding of the region, A2-cpu-06).

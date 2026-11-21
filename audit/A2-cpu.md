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
"device px" = logical px x `dpi_factor`. THE factor is `dpi / 96` (`LayoutWindow::frame_dpi_factor()`, what every shell
hands `CpuBackend::render_frame`), NOT `WindowSize::get_hidpi_factor()`, which is cut to three decimals (A2-cpu-13).
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
| isolated group: composite clip | `trunc` | `trunc` (`AzRect::pixel_box`, since A2-cpu-06; was `round` / `round`) | `GroupGeometry::of` (`outer_clip`) |
| layer: composite clip | `round` | `round` | `composite_layer_recursive` (`static_clip`) |
| layer placement | plain layer: `round((origin - inherited) dpi)`; a reference frame's layer: `(pivot - inherited) dpi`, not rounded (A2-cpu-14) | | `composite_layer_recursive` |
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

## Shift: `scroll_shift_region_impl` and `execute_virtual_view_shift`, all eight directions (A2-cpu-11)

`delta` is a SCROLL delta (`old_content_offset - new_content_offset` for a view): positive x = the content moves LEFT.
`px_d = round(new_off dpi) - round(prev_off dpi)`, `prev_off = new_off - delta`.

| pan (content_offset grows by) | `px_dx`, `px_dy` | mover | rows / columns copied | exposed strip (logical, dpi 1, view box (48, 40) 304 x 200) |
|---|---|---|---|---|
| (+12, 0): content right | -12, 0 | `shift_horizontal_1d`, `px_dx < 0` | `dst[x + 12] = src[x]` (memmove inside the row) | left: (48, 40) 13 x 200 |
| (-12, 0): content left | +12, 0 | `shift_horizontal_1d`, `px_dx > 0` | `dst[x] = src[x + 12]` | right: x = 48 + 304 - 13 = 339 |
| (0, +7): content down | 0, -7 | `shift_vertical_1d`, bottom to top | `dst[y] = src[y - 7]` | top: (48, 40) 304 x 8 |
| (0, -7): content up | 0, +7 | `shift_vertical_1d`, top to bottom | `dst[y] = src[y + 7]` | bottom: y = 40 + 200 - 8 = 232 |
| four diagonals | both | `shift_diagonal_2d` (one pass, `src_col` / `dst_col` by the sign of `px_dx`, row order by the sign of `px_dy`) | each row copied once from its diagonal source | both strips (an L; they stay two rects through `coalesce_damage_rects`) |

- the strip is `|px| + 1` device pixels (one of over-cover), computed from the CLAMPED region;
- a delta that is no whole device pixel (gate 1e-3): no move, damage = the clip, `present_extra` empty;
- `|px| >=` the region's width or height: no move, damage = the clip (`present_extra` = the clip all the same);
- the delta rounds to zero pixels: no damage at all (see A2-cpu-07 for when that is wrong);
- negative and positive offsets: only the difference of the two rounded offsets enters;
- `present_extra` = the view's whole clip whenever pixels moved (the whole box did change on screen); A1 narrows what
  is PRESENTED, the rect itself is right;
- overlays painted after the view: repainted at `over` and at `over - delta` (where the move dragged them), same sign as
  `execute_scroll_shift`'s ghost (`g - delta`).

Guards: `a2_cpu_11_a_pans_strip_is_on_the_side_the_content_came_from_in_all_eight_directions` (the table above, exact) and
`a2_cpu_11_a_panned_frame_is_a_fresh_renders_at_every_scale_and_for_every_kind_of_pan`.

## Findings

Severity = how often it draws wrongly x how visible (ms where it costs time). Tests are in
`layout/src/audit_tests/a2_cpu.rs` unless a file is named.

### A2-cpu-01 `gpu_value_damage` mapped a changed frame whose key has no value with the identity; the rasteriser uses `initial_transform`
- file:line `layout/src/cpurender/compositor.rs:2861-2862` (base) - class: sign/offset - severity: low (needs a key that
  loses or gains its value while the list stays; then a box-sized stale rect)
- evidence: a key bound by the list can be missing from the value map (`GpuValueCache::for_each_bound_value` skips a key
  without a value); the raster then paints with `initial_transform` (`raster.rs`, `PushReferenceFrame` arm). The function
  computed that fallback (`old_m` / `new_m`) and shadowed it with the identity for the changed frame's own extent.
  Example: box (10, 20) 100 x 50, built with `translate(100, 0)`, old value `translate(50, 0)`, new: none. Painted now at
  x 110..210; old damage `[60, 160)` + `[10, 110)`; new and expected `[60, 160)` + `[110, 210)`.
- test: `a2_cpu_01_a_changed_frame_without_a_live_value_is_damaged_where_its_initial_transform_paints_it`
- status: fixed `db350dbe6`

### A2-cpu-02 open question 1 of `CPU_RENDERING.md`: why the zoom-out frame at k = 1.000 was an isolated group
- file:line `css/src/props/basic/length.rs:223` (`FloatValue::new`), `css/src/cast.rs:40`, `compositor.rs:70-84` - class: rounding
- answer: `FloatValue::new(v)` is `(v * 1000.0) as isize`: it TRUNCATES. A zoom in and out again leaves `k = s / s0` one ulp
  under 1 (0.99999994): `x 1000 = 999.99994 -> 999 -> 0.999`. `is_identity_2d` wants `|m00 - 1| < 1e-4`, and
  `|0.999 - 1| = 1e-3`: not the identity, not a translation: `ReferenceFrameGroup::begin` opened an isolated group of the
  whole picture (5.78 MP) and resampled it at 0.999. A `k` one ulp ABOVE 1 (1.0000001 -> 1000 -> 1.000) was the identity,
  so the cost hung on the direction of a rounding error; in the measured scenario it fell under 1 every time. The
  editor's snap (`map.rs` `snap_transform`, `0095bba`: `|k - 1| < 1e-3 -> 1`, and no `Scale` at all for `k == 1`) covers
  exactly the range the truncation can produce, so the editor side is closed
  (`a_transform_close_to_the_identity_is_written_as_the_identity` in `map.rs` pins it). The engine side stays: A2-cpu-24.
- status: answered by reading

### A2-cpu-03 the hit-test / IME chain applies reference-frame matrices about the window origin; the rasteriser applies them about the frame's origin
- file:line `layout/src/headless.rs:349-382` (`resolve_chain`), `:321-346`, `:728-746` (`nested_dom_window_origin`, which my
  `window_space_offset_of_dom` returns) - class: sign/offset - severity: high for the editor's turned labels (923 in the
  Tuebingen view) and for the "press during a zoom" item of the brief
- evidence: `resolve_chain` sums every scroll offset and composes the raw matrices: `screen = T_total(pos - scroll_total)`.
  The raster paints `o + M (p - o)`, `o` the frame's box origin after the scroll frames around it (space rule 2); the two
  agree only for translations (or `o = (0, 0)` with no scroll frame outside a non-translation). Example: a label box at
  L (300, 200), 100 x 12, `transform-origin: 0 12px; transform: rotate(90deg)`. Its local point (50, 6) is painted at
  `(300, 200) + (6, 62) = (306, 262)`. The chain maps the screen point (306, 262) back with `M^-1` about (0, 0):
  `(250, -294)`, outside the box `[300, 400) x [200, 212)`: the press misses. Expected `o + M^-1 (s - o) = (350, 206)`.
- status: open (A6 claims the functions): see `## For area A6`

### A2-cpu-04 `reference_frame_extent` leaves out what the frames INSIDE a frame move
- file:line `layout/src/solver3/display_list.rs:3536-3607` (`GroupBounds`), `:3683-3767` (`FrameIndex::of`); consumers
  `raster.rs` (`GroupGeometry::of`'s `extent`), `compositor.rs:786`, `:2858`, `:5170` - class: wrong-output - severity: medium
  (any nested transform whose inner frame moves content out of the outer frame's layout box)
- evidence: `GroupBounds::step` unions the L bounds of every item between push and pop, also of items inside a nested
  `PushReferenceFrame`, which are painted through that inner matrix. Example: column frame F (box (0, 0) 200 x 400,
  animated `translateY`), in it a card frame G with `translate(-100, 0)`: the card is painted at x -100..100, F's extent is
  x 0..200. (a) value damage of F covers x 0..200 moved by F's old and new matrix: the left half of the card leaves a
  trail; (b) with F an isolated group (scale), its surface is x 0..200: the left half of the card is cut off.
  In the editor the picture root's extent is the ground rect (256 px around the view), so turned labels are inside it.
- test: `a2_cpu_04_the_damage_of_a_moved_frame_covers_what_a_frame_inside_it_moved` (`#[ignore]`)
- status: open (A1): see `## For area A1`

### A2-cpu-05 a changed `PushTextShadow` damaged nothing in the paired diff
- file:line `compositor.rs:5607-5624` (base), `display_list.rs:3461` - class: wrong-output (missed damage) - severity: medium
  (a halo's colour or width changes: idle frame in the window's list; in a view only when something else changed too)
- test: `a2_cpu_05_a_changed_text_shadow_is_not_an_idle_frame`
- status: fixed `f47353dc3` (the diff returns `None` for such a pair)

### A2-cpu-06 one clip, six ways to round it
- file:line see the table "P -> D" - class: rounding - severity: low at dpi 1 / 2 (whole device edges), visible at 1.25 / 1.5
- evidence: at dpi 1.25 a view box at logical x 50, width 300 is the clip 62.5..437.5. Direct paints write columns
  62..436; the composite of an isolated group was clipped to 63..437: during a zoom column 62 showed the backdrop and
  column 437 the content. Second example, far edge 1600.5: fills stop at column 1600, the pan's memmove region is
  `[62, ceil(1600.5)) = [62, 1601)`: a pan to the right copies the view's column over the neighbour's column 1600, and the
  repainted strip is on the left.
- test: `a2_cpu_06_a_scaled_group_fills_the_same_columns_of_a_clip_between_pixels_as_a_direct_paint`
- status: the group's clip fixed `9f2741513` (`AzRect::pixel_box`); the other sites open (A1): see `## For area A1`

### A2-cpu-07 `execute_virtual_view_shift` checks that the DELTA is a whole device pixel but moves by the difference of the two rounded OFFSETS
- file:line `compositor.rs:3671-3683` (gate), `:3740` -> `:2028-2032` (distance) - class: rounding - severity: none today
  (`virtual_view_content_offset` snaps both offsets), a pixel of error for any other producer of `content_offset`
- evidence: `-old_off dpi = 10.4999`, `-new_off dpi = 13.5004`: delta 3.0005 passes the gate, the pixels move
  `14 - 10 = 4`, a box at child x 10.3 moves 3. And `10.5003 -> 11.4999`: delta 0.9996 passes, distance `11 - 11 = 0`:
  `strips.is_empty()` returns NO damage for content that moved a pixel.
- test: `a2_cpu_07_a_pan_between_offsets_off_the_pixel_grid_is_a_fresh_renders_frame` (`#[ignore]`)
- status: open (A1 claims the function): see `## For area A1`

### A2-cpu-12 a blur wider than 64 device px is cut at the painted part of a group
- file:line `raster.rs` `GROUP_VISIBLE_MARGIN` / `group_part_within` - class: wrong-output - severity: very low
- evidence: the walk paints a group only inside the clip mapped back plus 64 group px. A `filter: blur()` or a shadow
  whose kernel is wider than 64 device px reads transparent pixels past that cut: the visible edge of the blur differs
  from an unculled render within `radius - 64` px of the damage edge. No content in the editor has such a radius.
- status: open (note for A1: make the margin `max(64, widest blur of the group)` if a group can hold one)

### A2-cpu-13 a view's content offset was snapped on `get_hidpi_factor()`'s grid, not on the frame's
- file:line `layout/src/window.rs:4353` (base; mine) - class: rounding / DPI - severity: none at 1, 1.25, 1.5, 1.75, 2; at any
  other scale (4/3, 1.1: Wayland fractional scaling, custom Windows scaling) every pan / wheel step repainted the whole
  view (the delta failed the whole-pixel gate), and A2-cpu-07 became reachable
- evidence: `get_hidpi_factor()` is a `FloatValue` (three decimals, cut off: dpi 128 reads 1.333); every shell renders at
  `dpi / 96` (1.3333334). Offset -300.3: old `round(-400.2999) / 1.333 = -300.075`, in the frame `-400.1`; new
  `round(-400.4) / 1.3333334`, in the frame `-400.0`.
- test: `a2_cpu_13_a_views_content_offset_is_a_whole_device_pixel_of_the_frame_at_four_thirds`
- status: fixed `8b69ea0a6` (`LayoutWindow::frame_dpi_factor()`); the shells should take the factor from it: `## For area A10`

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
   neighbour's column 1600 along). If WebRender's rule is preferred instead (offsets snapped first, then every edge
   `round`: A2-gpu-05), change `pixel_box` and the five `as i32` clip sites together; the tests only ask that a group
   and a direct paint agree.
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

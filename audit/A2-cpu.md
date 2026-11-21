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
- severity: low
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
- severity: low
- status: checked: fine (the question is answered; the editor snaps since `0095bba`; the engine side is A2-cpu-24)

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
- severity: high
- status: fixed `1f910ffbc` (by A6, on `audit/A6`: frames about their own corner, links in nesting order; read under `## Review of A6`)

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
- severity: medium
- status: fixed `66af92a36` (by A1, on `audit/A1`: `FrameIndex::extent_with`; `a2_cpu_04_...` is un-ignored there; read under `## Review of A1`, item 3; the perspective rest is A2-cpu-R01)

### A2-cpu-05 a changed `PushTextShadow` damaged nothing in the paired diff
- file:line `compositor.rs:5607-5624` (base), `display_list.rs:3461` - class: wrong-output (missed damage) - severity: medium
  (a halo's colour or width changes: idle frame in the window's list; in a view only when something else changed too)
- fix, in two commits: `f47353dc3` answered `None` for such a pair (the window, or the view's whole box: an 80 ms frame
  for the map view); `163f54b38` replaces that with the precise damage. The paired walk keeps the text shadows open at
  the current item; under one that differs between the lists every `Text` item is damaged where the old shadow painted
  and where the new one paints: `visual_bounds()` widened by `text_shadow_reach` (the larger offset + blur + spread) plus
  `TEXT_SHADOW_DAMAGE_SLACK` = 3 px (blur radius, spread and the dilation's rim are each rounded up to device pixels when
  the shadow is drawn: 2.5 device px; plus the run's own fringe). The rects go through the scroll offsets, the frames and
  a view's content offset like every rect of the diff.
- example: three labels on the baselines 30, 90, 150, pens at x 20, 32, 44, 24 px, each in its clip `(16, y - 28) 150 x 40`
  under a halo of 2 px spread; the middle halo turns yellow. Ink of the middle run: x `-4 .. 68`, y `54 .. 108`, cut to
  the clip: `(16, 62) 52 x 40`; widened by 2 + 3: **(11, 57) 62 x 50**; in a view at (10, 10) with its content at (-6, 4):
  **(15, 71) 62 x 50**. The first label ends at y 56 on screen, the third starts at 136. (The mutant "no slack" gives
  `(14, 60) 56 x 44`; the commit message of `163f54b38` names another rect for it by mistake.)
- tests: `a2_cpu_05_a_changed_text_shadow_damages_the_runs_under_it_by_its_reach_and_no_other_run` (the two rects, exact);
  `a2_cpu_05_one_labels_halo_changes_colour_in_a_view_and_only_that_label_is_repainted` (`raster.rs`, `text_shadow_tests`:
  real glyphs; damaged render == full render, every pixel; skipped without a system font)
- severity: medium
- status: fixed `163f54b38` (after `f47353dc3`). The windowed (structural) path still repaints whole for a shadow in its window: lists of different lengths, a view render, not a hover

### A2-cpu-06 one clip, six ways to round it
- file:line see the table "P -> D" - class: rounding - severity: low at dpi 1 / 2 (whole device edges), visible at 1.25 / 1.5
- evidence: at dpi 1.25 a view box at logical x 50, width 300 is the clip 62.5..437.5. Direct paints write columns
  62..436; the composite of an isolated group was clipped to 63..437: during a zoom column 62 showed the backdrop and
  column 437 the content. Second example, far edge 1600.5: fills stop at column 1600, the pan's memmove region is
  `[62, ceil(1600.5)) = [62, 1601)`: a pan to the right copies the view's column over the neighbour's column 1600, and the
  repainted strip is on the left.
- test: `a2_cpu_06_a_scaled_group_fills_the_same_columns_of_a_clip_between_pixels_as_a_direct_paint`
- severity: low
- status: open: A1 applies `AzRect::pixel_box` at the five remaining sites (asked for its second round); the group's clip is fixed `9f2741513`

### A2-cpu-07 `execute_virtual_view_shift` checks that the DELTA is a whole device pixel but moves by the difference of the two rounded OFFSETS
- file:line `compositor.rs:3671-3683` (gate), `:3740` -> `:2028-2032` (distance) - class: rounding - severity: none today
  (`virtual_view_content_offset` snaps both offsets), a pixel of error for any other producer of `content_offset`
- evidence: `-old_off dpi = 10.4999`, `-new_off dpi = 13.5004`: delta 3.0005 passes the gate, the pixels move
  `14 - 10 = 4`, a box at child x 10.3 moves 3. And `10.5003 -> 11.4999`: delta 0.9996 passes, distance `11 - 11 = 0`:
  `strips.is_empty()` returns NO damage for content that moved a pixel.
- test: `a2_cpu_07_a_pan_between_offsets_off_the_pixel_grid_is_a_fresh_renders_frame` (`#[ignore]`)
- severity: low
- status: fixed `7865fb887` (by A1, on `audit/A1` after its rebase, with the gate on both offsets; `a2_cpu_07_...` is un-ignored there)

### A2-cpu-12 a blur wider than 64 device px is cut at the painted part of a group
- file:line `raster.rs` `GROUP_VISIBLE_MARGIN` / `group_part_within` - class: wrong-output - severity: very low
- evidence (read again for the review round; the first reading was wrong): the walk paints a group only inside the clip
  mapped back plus 64 group px, and nothing reads the group's pixels across that cut. A box shadow is drawn and blurred
  in a buffer of its own, the size of the whole shadow (`render_box_shadow`: `sw x sh`, cached), and only its BLIT is
  clipped; a text shadow's buffer reaches the blur radius past what lands (`text_shadow_boxes`: `out +- r`), a kept halo
  is drawn whole; a `filter: blur()` is not painted by the flat walk at all (layers only: the push carries no work
  there); `flood()` / `composite()` act per pixel. What the margin has to cover is the composite's two bilinear taps.
- severity: low
- status: checked: fine (see the evidence: nothing in the flat walk blurs pixels of the group it was not given a buffer for)

### A2-cpu-13 a view's content offset was snapped on `get_hidpi_factor()`'s grid, not on the frame's
- file:line `layout/src/window.rs:4353` (base; mine) - class: rounding / DPI - severity: none at 1, 1.25, 1.5, 1.75, 2; at any
  other scale (4/3, 1.1: Wayland fractional scaling, custom Windows scaling) every pan / wheel step repainted the whole
  view (the delta failed the whole-pixel gate), and A2-cpu-07 became reachable
- evidence: `get_hidpi_factor()` is a `FloatValue` (three decimals, cut off: dpi 128 reads 1.333); every shell renders at
  `dpi / 96` (1.3333334). Offset -300.3: old `round(-400.2999) / 1.333 = -300.075`, in the frame `-400.1`; new
  `round(-400.4) / 1.3333334`, in the frame `-400.0`.
- test: `a2_cpu_13_a_views_content_offset_is_a_whole_device_pixel_of_the_frame_at_four_thirds`
- severity: medium
- status: fixed `8b69ea0a6`

### A2-cpu-14 the layered compositor rounded a reference frame's pivot to a device pixel; the flat walk and WebRender do not
- file:line `compositor.rs:1325-1328` (base) - class: rounding - severity: low at whole device positions, visible at
  fractional ones (a full repaint and a damage repaint of one frame half a pixel apart). Lead from A2-gpu (A2-gpu-05).
- evidence: dpi 1, frame box (20.5, 30.5), `scale(2)`: flat walk `x' = 2 g + 19.5`, layer `x' = round(20.5) + 2 (g - 0.5)
  = 2 g + 20`.
- test: `a2_cpu_14_a_scaled_frame_between_pixels_is_composited_where_the_damage_repaint_puts_it` (the two paths' frames
  are equal, every pixel)
- severity: low
- status: fixed `e14f80eb3`

### A2-cpu-15 a turned frame at the window's level: the full repaint (a layer) and the damage repaint (a group) are not the same pixels when layers nest inside it
- file:line `compositor.rs:1496-1504` (`child_base`), the tests `lane_az_1_*` (tolerances of 40 and 400 pixels)
- class: wrong-output - severity: low (edges only; the editor's turned labels are inside a view, which both paths paint
  with the flat walk)
- evidence (traced for the review round): a layer inside a promoted frame is NOT composited through the matrix on its
  own: `composite_layer_recursive` takes the `is_group` branch for a turned layer with children, flattens them into a
  copy of the frame's pixels (`local_base`) and turns the result once, as the flat walk does. For
  `lane_az_1_the_groups_of_a_promoted_turned_frame_keep_their_layers` (frame box (40, 30), scroll box (100, 40) inside):
  both paths end with the same 120 x 60 surface (blue, the green box at (60, 10)) and the same matrix
  `T(40, 30) . R(12 deg)`; the tolerances of those tests (40 and 400 pixels) are slack, not a difference by design. At a
  fractional device position the child layer is placed at `round(origin dpi)` where the flat walk rounds each item:
  that is A2-cpu-21.
- severity: low
- status: checked: fine (at whole device positions the two paths build the same surface and the same matrix; at fractional ones it is A2-cpu-21)

### A2-cpu-16 a snapped offset is `n / s`, and `(x + n / s) x s` is `x s + n` only to 1e-5
- file:line `window.rs` `virtual_view_content_offset` (the snap), `raster.rs` `VirtualView` arm (`acc - origin -
  content_offset`), `GroupGeometry::of` (`o = bounds.origin * dpi`) - class: rounding - severity: low (scales 1.25, 1.5, ...
  only: an edge on a half device pixel can round the other way after a pan, a resampled group can differ by a level)
- evidence: at 1.5 an odd logical edge is `k + 0.5` device px; after a pan by `n` device pixels its position is computed
  as `(x + 48 + n / 1.5) x 1.5 = k + 0.5 + n (1 +- 6e-8)`: which way it rounds depends on the noise, so the moved pixels
  (rounded at the old offset) and a fresh render (rounded at the new one) can differ by a pixel along that edge. At 1 and
  2 every term is exact. The tests of this area therefore stand on multiples of 4 px and allow two levels at 1.25 / 1.5.
- fix: carry the view's offset in whole device pixels (an integer) through the raster and add it AFTER the scale.
- severity: low
- status: open: A1 holds it as `blocked (a)` (the view's offset as whole device pixels outside `acc` changes the coordinate input of every paint function); kept surfaces are keyed on the phase since `451b684d8`, which meets the pan during a zoom

### A2-cpu-17 a solid border thinner than one device pixel painted nothing (or two rows, one outside the box)
- file:line `raster.rs` `render_border`, `let swi = sw as i32` - class: rounding - severity: low (hairline borders). Lead from A2-gpu.
- test: `a2_cpu_17_a_border_thinner_than_a_device_pixel_is_one_pixel_inside_its_box`
- severity: low
- status: fixed `c5f25d10d`

### A2-cpu-18 the damaged walk culled a text under a shadow by its box, without the shadow's reach
- file:line `raster.rs` `walk_items`, the `WalkCull::Damage` block (base: `render_display_list_damaged` 2743-2771)
- class: wrong-output (missed repaint) - severity: low-medium (window-level text with an offset / wide shadow; the labels of
  a view go through `outside_active_clip`, which had the reach)
- test: `a2_cpu_18_a_damage_rect_on_a_text_shadow_alone_repaints_the_shadow` (`raster.rs`, `text_shadow_tests`)
- severity: medium
- status: fixed `6836b9c89`

### A2-cpu-19 `execute_virtual_view_shift` counts every item after the view as an overlay, also the ones that paint nothing
- file:line `compositor.rs:3755-3780` - class: waste - severity: a whole-view repaint per pan frame when it happens
- evidence: the loop damages `it.visual_bounds().or_else(|| it.bounds())` of every non-state item after the view that
  overlaps its clip. `HitTestArea`, `TextLayout` and `WebView` have bounds and paint nothing: a hit-test area emitted
  after the view over the view's box makes every pan repaint the box. (`overlay_rects_after_frame_in` has the same rule
  for scroll frames.) Not seen in the editor's measurements (13 px strips), so its list has none there today.
- severity: low
- status: open: A1 owns `execute_virtual_view_shift` (asked for its second round): skip `HitTestArea | TextLayout | WebView` in both loops

### A2-cpu-20 the value damage and the item diff are placed with the REAL scroll offsets, an incremental frame paints at the BASELINE offsets
- file:line `cpu_backend.rs:501-507`, `:598-608`, `:823-828` against `:1200-1204` - class: sign/offset - severity: very low
- evidence: a frame whose scroll delta is under half a device pixel keeps its previous baseline (`next_scroll_baseline`)
  and is painted there; `window_value_damage`, `compute_display_list_damage_live` and `collect_virtual_view_shifts` get
  `scroll_offsets`. The rects are up to half a device pixel from the pixels; after the outward snap a one-pixel column
  of a changed item inside such a frame can stay stale. (`compute_virtual_view_damage_live` gets the baseline: right.)
- severity: low
- status: open: A1 owns `render_frame` (asked for its second round): pass `next_scroll_baseline` to all three

### A2-cpu-21 a plain layer is placed on a whole pixel and its content rounded relative to it; the flat walk rounds each item's own position
- file:line `compositor.rs` `composite_layer_recursive` (`layout_offset_device_px`), `render_layers` (`offset_x`) against
  `raster.rs` `render_rect` - class: rounding - severity: low (fractional scales, content inside scroll-frame layers)
- evidence: dpi 1.25, scroll frame clip origin x = 10 (12.5 device: the layer is placed at 13), an opaque rect at x = 20,
  w = 30 inside. Flat walk: `[round(25), round(62.5)) = [25, 63)`. Layer: local `[round(12.5), round(50)) = [13, 50)`, plus
  13: `[26, 63)`. Column 25 differs between a full repaint and a damage repaint of the same list.
- severity: low
- status: fixed `93c22709b` (by A1, on `audit/A1`, A1-21: a full repaint is the damage walk over the whole frame; layers only for a list with a blur, which is A2-cpu-R08)

### A2-cpu-22 `node_groups_on_screen` subtracts the whole scroll AFTER the frames
- file:line `compositor.rs:3199-3208` - class: sign/offset - severity: very low (a culler that errs to "visible" otherwise)
- evidence: `affine_rect_about(m, bounds.origin, content)` with the unscrolled origin, then `moved_by(r, stack.scrolled())`.
  Right for scroll frames outside every frame; a scroll frame INSIDE a scaling frame is off by `(M - I) s`, and the group
  can be judged off screen while it shows (its animation would be culled).
- severity: low
- status: fixed `808ce391e`

### A2-cpu-23 the editor's zoom keeps the pointer's map point only for the scale it MEANT, not for the scale the engine draws
- file:line m4p `mapedit3/src/ui/main/map.rs:564-593` (`write_picture_transform`), `:518` (`picture_transform`)
- class: rounding - severity: medium-low (every zoom step, 30 / s: up to 2.5 px of slip at the far edge of a 2560 px view,
  gone at the settle render: labels visibly jump when the zoom settles)
- evidence: the editor writes `translate(t) scale(k)` with `t = s (pan - pan0)`, exact for `k = s / s0`. The engine keeps
  three decimals of `k`, cut off (A2-cpu-02): it draws `k_q <= k`, `k - k_q < 0.001`. A picture point `P` is shown at
  `k_q P + t = (k P + t) - (k - k_q) P`. Example: one wheel notch, `k = 1.189207 -> 1.189`: the point under a pointer at
  picture x 2000 is drawn 0.41 px left of the pointer; `k = 1.6817928 -> 1.681`: 1.6 px; at the settle it is back.
  Always to the left / up (the cut only shrinks).
- severity: medium
- status: fixed `385bbb2` (m4p, by A7-editor, with `1ec4150`; the assertion `error.anchor_px` by A11, `cad9f09`)

### A2-cpu-24 `FloatValue::new` truncates
- file:line `css/src/props/basic/length.rs:223`, `css/src/cast.rs:40` - class: rounding - severity: low, but it is the root
  of A2-cpu-02, -13 and -23
- evidence: `FloatValue::new(0.29)`: `0.29f32 = 0.28999999`, `x 1000 = 289.99999 -> 289`: the value is 0.289.
  `scale(0.9999999)` is 0.999; `translate(-0.0007px)` is 0; `get_hidpi_factor()` at dpi 128 is 1.333.
- fix: `(v * 1000.0).round() as isize` in `FloatValue::new` (and the same in `PixelValue` if it has its own).
- severity: low
- status: fixed `fb8751e41` (by A3-core, on `audit/A3-core`, A3-core-14: `FloatValue::new` rounds to the nearest thousandth)

### A2-cpu-25 `is_identity_2d` does not look at the perspective terms
- file:line `compositor.rs:77-84` - class: wrong-output - severity: very low (`matrix3d()` only)
- evidence: it tests `m00, m01, m10, m11, m30, m31`. A matrix with those at the identity and `m03` / `m13` / `m33` not is
  painted in place, untilted; `translation_2d` does test them.
- severity: low
- status: fixed `c44c8d260`

### A2-cpu-26 an opaque rect rounds both edges, a translucent one truncates them
- file:line `raster.rs` `render_rect`: `round_edge` against `rect.x as i32` (`blend_bar`) - class: rounding - severity: low
- evidence: a rect at x = 62.8, w = 10.4: opaque `[63, 73)`, at alpha 254 `[62, 73)`: a fade moves the box's left edge.
- severity: low
- status: fixed `88f18ef32`

### A2-cpu-27 the view shift's "covered" test reads the child's opaque fills at their layout place
- file:line `compositor.rs:3704-3729` (`FrameIndex::opaque_fills`) - class: sign/offset - severity: very low today
- evidence: a ground inside the picture's frame is taken at its L bounds; with the frame scaled to 0.5 the painted
  ground no longer covers the view, `covered` still says yes and the memmove runs over whatever shows around the
  picture. Harmless over a flat backdrop (what the editor has, and it pans through the transform while zoomed).
- severity: low
- status: open: A1 owns `execute_virtual_view_shift` and `FrameIndex` (asked for its second round): count only fills outside every non-identity frame of the child

### A2-cpu-28 the windowed diff damages a changed frame's box, not what the frame paints
- file:line `compositor.rs:4898-4980` (`collect_window`) - class: wrong-output (missed damage) - severity: low
- evidence: a `PushReferenceFrame` inside the changed window contributes its `bounds` through the frames. Its content can
  lie in the visually-equal SUFFIX (the window ends before it): content that reaches past the frame's box (a shadow, an
  overflowing child) is then not damaged although its matrix changed. The paired path uses the frame's extent.
- severity: low
- status: fixed `3659e7e92`

### A2-cpu-29 a run that changed under a text shadow that did not was damaged by its ink alone
- file:line `compositor.rs` `compute_display_list_damage_impl` (the tail: `fringe`, `inflate`; the hint's `inside`),
  `windowed_structural_damage` (`collect_window`) - class: wrong-output (missed damage: stale and cut shadow pixels)
- evidence (found while reading A1's `shadow_reach`): a run under a shadow paints the shadow `text_shadow_reach` past its
  ink (`render_text_shadow`). Both diffs damaged a changed run by `visual_bounds()` + 1 px; only a CHANGED shadow widened
  it (A2-cpu-05), and the structural diff only refused a push INSIDE a window, not a run under a push of the equal
  prefix. Hidden for small halos by the padding of `visual_bounds` (1 em left, 1 em past the last pen, 0.75 em under
  the baseline); a drop shadow, a glow, or a halo behind a wide last glyph shows it. 12 px run, pens 100, 110, 120 on the
  baseline 50: bounds (88, 32) 44 x 27; shadow offset (8, 8), blur 4 (reach 12): the shadow ends at x 142 / y 62, the
  damage of the run losing a glyph ended at x 133 / y 60.
- fix: the paired diff keeps the reach of every open shadow and inflates a changed run by the largest plus
  `TEXT_SHADOW_DAMAGE_SLACK` ((73, 17) 74 x 57 in the example; without a shadow (87, 31) 46 x 29 as before); the hint
  counts a run as moved only with its halo inside the region; the structural diff follows the shadows open at the prefix
  boundary and widens the same way ((73, 17) 74 x 77 for the run broken in two).
- test: `a2_cpu_29_a_run_that_changed_under_an_unchanged_shadow_is_damaged_with_its_halo` (rects, both diffs),
  `a2_cpu_29_a_label_that_gets_shorter_under_a_drop_shadow_leaves_none_of_the_shadow` (pixels, with a system font)
- the same gap for a FRAME's extent (a moved frame, a group's surface) is A1's: A2-cpu-R05.
- severity: medium
- status: fixed `0580852f7`

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
  (`rect_minus`), the 1 px text fringe: right (what a run under a text shadow needs on top of it: A2-cpu-29). `coalesce_damage_rects`, `disjoint_damage`, `box_minus`: only ever
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

State at `audit/A1` `d17af4dc9`: item 1 (A2-cpu-04), item 3 (A2-cpu-07), A2-cpu-21 and the constraints of items 6
and 7 are done there (read under `## Review of A1`). Still A1's: item 2 (the five clip sites of A2-cpu-06), item 4
(A2-cpu-16, which A1 holds as `blocked (a)`), A2-cpu-19, -20, -27, and the review's findings A2-cpu-R01 to -R08.

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

State at `audit/A6` `12460bb7a`: done as asked (`1f910ffbc`, `da6be4605`, `31884b79c`; read under `## Review of A6`).
Still A6's: A2-cpu-R09.

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

## For area A10

- `CpuBackend::render_frame`'s `dpi_factor` should be `layout_window.frame_dpi_factor()` in every shell (headless, X11,
  macOS and Windows compute the same `ws.size.dpi as f32 / 96.0` today; the Wayland popup path has a `dpi_factor` of its
  own, `wayland/mod.rs` about 10200-10310): a view's offset is snapped on that grid (A2-cpu-13), and a frame rendered
  at another factor repaints the view on every pan step.
- `WindowSize::get_hidpi_factor()` is cut to three decimals (A2-cpu-24): do not size or place a frame with it.
- `pixel_w = ceil(width x dpi)`: at a fractional product the frame is a fraction of a pixel larger than the window; the
  last row / column holds whatever the root items' own rounding puts there.

## Review of A1

Branch `audit/A1` at `d17af4dc9` (it was rebased onto `3e63f996a`: the hashes below are those of that tip; the ones in
the coordinator's request are the same commits before the rebase). By algebra, one numeric example per item. Names of
spaces as in `## Spaces`.

Read: `cpurender/bands.rs` and `cpurender/retained.rs` whole; `raster.rs` 1130-1560 (groups), 2500-3130 (the walk, kept
surfaces), 3132-3490 (turned text), 3490-3845 (repair, stepping), 3894-4332 (damage rects, the whole repaint), 4395-4470
and 5039-5160 (frame and view arms); `solver3/display_list.rs` 3520-4170 (`GroupBounds`, `FrameIndex`); `compositor.rs`
2003-2160 (moves), 2840-3210 (value damage), 3700-3865 (view shift), 5196-5770 (paired diff, unchanged by A1), the test
at 11848; `cpu_backend.rs` 425-450, 583-640, 880-1110, 1240-1500. Not read: A1's changes in `text_halo.rs`,
`glyph_cache.rs`, `image_memo.rs`, `cpu_counters.rs`, `blit_scale.rs` (threads only), the bodies of the new tests.

### Per item

1. **Bands** (`8ff97ea4f`; `bands.rs` 145-168 `plan_bands`, 233-278 `paint_band`): read, agree. The bands of
   `[x0, y0, x1, y1]` are `[x0, y, x1, min(y + rows, y1)]` from `y = y0`: half-open, adjacent, their union is the rect
   (no row twice, none left out), the last one shorter. `[25, 19, 2535, 1421]` on 4 threads: `wanted = min(8, 17, 29)`
   = 8, `rows = ceil(1402 / 8) = 176`, bands at y = 19, 195, ..., 1251, the last 1251..1421 (170 rows). A band's pass
   gets `SnappedRect::from_physical(x0, y0, x1, y1)`, the clip `[x0, x1) x [y0, y1)` in D and positions in the FRAME's
   coordinates (its pixmap has the frame's size), so every `(p - acc) dpi` is the single pass's value bit for bit; only
   `[x0, x1)` of its rows is copied back. What a pass reads of the target outside its clip rows: nothing (LCD text
   reads horizontal neighbours, and the band holds its rows in full width; the isolating filter groups snapshot their
   whole box, but rows outside the band are the band's private zeros and are not copied back; `flood` / `composite`
   are per pixel; what such a group writes OUTSIDE the pass's rect is R10). The exception A1 documents (a run's path chosen by the pixels under the clipped run) is A1-23 /
   A1-24.
2. **Frame stepping** (`8ff97ea4f`; `raster.rs` 3746-3843 `frame_paints_nothing`, `display_list.rs` 3794-4042
   `FrameIndex::of`): read, agree. Identity / translation leaf: `painted_box = extent - acc + t`, widened by the reach
   of the shadows in and around the frame, tested against the clip (in a group or a view) or the damage rect, with the
   same "overlap or adjacent" as the item culls; a frame that is not a leaf is always walked. A1's example checked:
   extent (100, 40) 60 x 20, `t = (30, 0)`, `acc = (10, 0)`, reach 3: x 117..183, y 37..63; under damage x < 110
   nothing, under x < 120 it may. Isolated frame: `GroupGeometry::of` on the same `bounds - acc`, `extent - acc` and
   clip that `begin` gets, and `group_visible_part`: the test `begin` makes. `self_contained` (scopes opened inside are
   closed inside, innermost first) is what makes the jump `next = frame.end + 1` leave the stacks as a walk through
   the frame does. Findings: R05 (the extent has no shadow reach).
3. **A2-cpu-04 by A1** (`66af92a36`; `display_list.rs` 4102-4153 `extent_with`): read, agree. `extent = own ∪
   bbox(o_i + M_i (extent_with(child_i) - o_i))` with `o_i` the child's `bounds.origin`, in the frame's own space, by
   the live value of each child (else its push's matrix): the walk's rule. A1's example checked: column (0, 0) 200 x
   400, card (0, 0) 200 x 80 with `translate(-100, 0)`: card at x -100..100, extent (-100, 0) 300 x 400. A child frame
   inside a scroll frame of the frame is united unscrolled and unclipped (a superset: its content is cut by the scroll
   frame's clip, which `own` holds). The walk (2645-2659), `gpu_value_damage` (2939-2942, old values for the old
   place, new for the new) use it; `status` of A2-cpu-04 above. Finding: R01 (perspective).
4. **Kept surfaces** (`f5359cff4`, `451b684d8`, `ef0c9e4f7`; `retained.rs`; `raster.rs` 2896-2937 `values_inside`,
   2947-2953 `surface_phase`, 2987-3091 `retained_candidate`, 2606-2639 and 2711-2738 in the walk, 3511-3727 repair):
   read. Agree with the key and with a hit; three findings (R03, R04, R07).
   - Surface to group to device: the content of list position `p` is at surface px `(p - acc) dpi - g`, `g =
     floor((extent - acc) dpi)`; surface px `s` lands at `o_d + L (s + g - o_d) + t dpi` (`GroupGeometry::of`), `o_d =
     (bounds.origin - acc) dpi`. The candidate computes `g`, the size, the matrix and the clip with the expressions
     `begin` uses (`on_surface` = `scroll_rect`), so a miss opens exactly the surface the key describes; the hit
     composites with `TransformGroup { matrix, outer_clip }` of the LIVE geometry.
   - The key. In it: the list's allocation (held by the entry, so the address cannot be reused), the push index, the
     size, the factor's bits, the phase `round(frac((extent - acc) dpi) x 1024)`, a hash of the live values bound
     INSIDE (inner frames' matrices, opacities, bound colours, thumbs, scroll offsets; a frame's own key is not in its
     own `FrameInner`: `FrameIndex::of` records the keys before it opens the frame, 3885-3924 against 3992), a hash of
     the text settings. Not in it: the frame's own matrix, the clip. Never kept: nested, under a text shadow, with a
     view inside, perspective. Checked against everything the walk reads inside a frame: complete, with the one hole
     of R03 (the list is replaced and the extent moves by whole pixels).
   - Rule 4 (the phase): at an offset that differs by `n` whole device px, `(p - acc) dpi` and `g` both move by `n`:
     the same picture. dpi 2, extent.x 37.25, item at 40.1: `acc = 0`: `g = floor(74.5) = 74`, phase 512, item at 80.2
     - 74 = 6.2; `acc = 6` (12 px): `g = floor(62.5) = 62`, phase 512, item at 68.2 - 62 = 6.2. Remark, no finding:
     equal keys mean equal pictures up to 1/2048 px (the quantum) plus the 1e-5 px of A2-cpu-16; an edge that
     `round_edge` puts within that of a half pixel can sit one pixel apart in the kept and in a fresh picture. Exact
     once offsets are whole device pixels outside `acc` (A2-cpu-16); a view's offset is on the grid (A2-cpu-13), so
     the editor's pans have the same phase to 1e-5.
   - Painted so far against read: `needed = group_part_within(.., 2)`. The composite samples `s = inv(M)(d + 0.5) -
     0.5` for destination pixels `d` of the clip box and reads texels `floor(s)`, `floor(s) + 1` (`blit_scale` 38-52
     and `blit_pixmap_affine_general` 302-336: two taps a axis, no wider filter when minifying). `inv(M)` of the clip
     box lies in the bbox `[x0, x1]` of its corners, so the texels read are `floor(x0) - 1 ..= floor(x1) + 1`, inside
     `[floor(x0 - 2), ceil(x1 + 2))`: the margin of 2 covers them (1.5 would). `valid` = the union of the parts
     painted (each `group_visible_part`, margin 64, clipped as the walk's clip): a pixel painted under a clip is the
     pixel of an unclipped paint (items are culled by what they paint, shadows and halos come from buffers of their
     own, text in a group is grayscale by the stack, not by the pixels), so the whole part is valid. `covers` =
     `needed` minus every valid box is empty. A hit therefore reads only texels a fresh paint of this frame would
     have painted with the same values: identical to a fresh paint, with A1-24 as A1's own open exception (the
     pre-blended path is chosen under the CLIPPED run: it also decides pixels of a kept part and of a repaired box).
   - Bands: every band offers its part of the same key; `commit` merges them in a fixed order into one surface.
5. **Present moves** (`1330635f1`, `4e8d84f40`; `compositor.rs` 2022-2030, 2141-2151; test 11848): read, agree with
   the move itself. A scroll delta `px_dx` moves the content by `-px_dx`: `dx = -px_dx`, `dst = [cx0, cx1) ∩ ([cx0,
   cx1) + dx)`, `new[p] = old[p - (dx, dy)]`: the Shift table above with the sign named for the presenter. Clip (10,
   10) 60 x 40, delta (12, 0): `dx = -12`, `dst = (10, 10, 48, 40)`, pixel (20, 15) shows old (32, 15), the strip
   57..70 (12 + 1 over-covered) overlaps the last column of `dst` and is presented after the move: as A10's contract
   asks. Finding: R02 (a frame that also blits a layout move).
6. **The shift gate** (`7865fb887`; `compositor.rs` 3734-3758): read, agree. Both offsets and the delta on the device
   grid (1e-3); then `round(new dpi) - round(prev dpi)` IS the delta. 10.4999 -> 13.5004: not on the grid, the clip
   is repainted. `prev = offset - delta` is the old offset negated, as `ViewShift` defines the two.
7. **A1-15** (`9326d2e51`; `raster.rs` 5138-5155, `compositor.rs` 3115-3207): read, agree. A view's list is walked
   with `bind_colors = true`; `color_value_damage` of the child list lands at `r + bounds.origin + content_offset`,
   cut to the view's box, through the frames around the view's item (`ScrollStack::on_screen`): the placement of the
   `VirtualView` arm.
8. **A1-20, turned text drawn directly** (`1815e987a`; `raster.rs` 3135-3327, 3349-3488): read, agree with the map.
   `TurnedMap::at(q) = o + L (q - o) + t dpi`, `L = [m00 m10; m01 m11]`, `o = (bounds.origin - acc) dpi`: the group's
   matrix without the group. A shadow's offset is added to the pen BEFORE the matrix (it turns with the label, as in
   a group). The glyph transform `scale . translate(rel pen) . L . translate(-box)` uses agg's `(sx, shy, shx, sy) =
   (a, c, b, d)`: `x' = a x + b y`. Example: `rotate(90deg)` (m00 = 0, m01 = 1, m10 = -1, m11 = 0), `o = (300, 200)`,
   pen (350, 206) at dpi 1: `q - o = (50, 6)`, `L (q - o) = (-6, 50)`: (294, 250), plus `t dpi` = (12, 12) for a pivot at (0, 12): (306, 262),
   the point of the example under `## Review of A6`. The clip is
   `AzRect::pixel_box` (the one rule). Remarks: (a) the anchor is `round()`ed to a whole device pixel per run (the
   only text on the CPU path placed on whole pixels; upright text is on a quarter-pixel grid): within 0.5 px of the
   group path, by design; during a slow zoom labels step by whole pixels against the geometry under them (see `What
   to look at on the real window`); (b) a pan by whole device px moves every anchor by that number except where
   `frac` is within 1e-5 of 0.5 (A2-cpu-16 again). Findings: R05 (the halo passes the extent), R06 (runs outside the
   clip, runs too large).
9. **A1-21, the full repaint by the walk** (`93c22709b`; `raster.rs` 4231-4272, `cpu_backend.rs` 1336-1390): read,
   agree: a full frame is the damage pass over `[0, 0, w, h]`, so "damaged == full" holds by construction for every
   list the walk paints; A2-cpu-21 and the fractional part of A2-cpu-15 are gone with it. Finding: R08 (a list with a
   blur keeps two paths).

### Findings

#### A2-cpu-R01 a perspective inside a frame: `extent_with` tests it with another tolerance than the rasteriser and gives the laid-out extent
- severity: low
- where: `layout/src/solver3/display_list.rs:4121-4123` (and the same test at `raster.rs:3583`, `3169-3171`)
- algebra: the rasteriser treats a matrix as affine when `m03 == 0 && m13 == 0 && |m33 - 1| < EPSILON`
  (`has_no_perspective`, A2-cpu-25) and composites a group projectively when `|m03 / dpi| >= 1e-12`
  (`mat3_is_affine`); `extent_with` maps a child affinely while `|m03| <= 1e-6` and returns `None` above it, and the
  walk then keeps the laid-out extent as the surface. A point `q` of a child about `o` lands at `o + (L q + t) / w`,
  `w = m03 qx + m13 qy + m33`.
- example: child box (0, 0) 400 x 100 about `o = (0, 0)`, identity but `m03 = -0.001` (a tilt): the corner (400, 0)
  has `w = 0.6` and lands at x = 666.7; the laid-out extent ends at 400: the outer frame's surface cuts 267 px of the
  child. With `m03 = -5e-7` and a 2000 px child: `w = 0.999`, the corner lands at 2002.0, the extent says 2000.
- change: in the corner loop, `let w = qx * m[0][3] + qy * m[1][3] + m[3][3]; if !(w > 1e-6) { return None; }` and
  divide both sums by `w`; delete the early `return None` on the perspective terms. For an affine matrix `w` is 1.0
  exactly and the result is today's bit for bit. (Four corners with `w > 0` bound the image: `w` is linear, so it is
  positive on the whole box, and the image of the box is the convex quad of the mapped corners.) `None` stays for a
  corner at or behind the eye. The same in `stale_boxes`.
- test: a column with a card whose matrix has `m03 = -0.001`: `extent_with` reaches x = 666.7 (mutation: the affine
  sums give 400).

#### A2-cpu-R02 a frame that blits a layout move AND shifts a scroll frame or a view reports moves that do not rebuild it
- severity: low (needs a layout patch with its translate hint and a scroll or a pan in ONE frame; the hint is off
  while any scroll offset is non-zero, so: a scroll back to exactly 0, or a view's pan)
- where: `layout/src/cpu_backend.rs:1046-1075` (the blit, first), `1076-1109` (the reported moves), `1483-1490`
- algebra: the invariant is "previous frame, the moves in order, then `last_present_damage_after_moves` copied =
  the new frame". The translate blit moves pixels BEFORE the reported moves and is not one of them; its boxes
  (`present_extra` of the blit, in the positions BEFORE the scroll) are in `present_unmoved`. A reported move then
  carries blitted pixels: for `p` in a move's `dst` with `p - d` inside a blit box and `p` outside every box of
  `present_unmoved`, the frame holds the blitted pixel and the presenter shows `old[p - d]`, the pixel from before
  the blit.
- example (dpi 1, frame 100 x 80): a panel (10, 10) 30 x 20 moves down 5 by the layout blit (box rows 15..35); the
  page scrolls from 8 to 0 in the same frame: move `dst = (0, 8, 100, 72)`, `dy = +8`, which carries the blitted
  panel to rows 23..43. The presenter moves ITS pixels (panel at rows 10..30) to 18..38 and copies the strip and
  (10, 15) 30 x 20 from the frame: rows 35..43 of columns 10..40 show old rows 27..35 (the panel's last 3 rows, then
  background) where the frame has the panel's rows 12..20: 240 px wrong.
- change: when the blit ran (`present_unmoved` is not empty), report no moves: `last_present_moves = Vec::new()`
  and `last_present_damage_after_moves = ` what `last_present_damage` is. (Or report each mover of the blit as a
  move of its own in front of the others; the first is three lines.)
- test: the invariant test of `4e8d84f40` with a blit before the shift (mutation: the moves reported as today).
- note: no consumer on macOS today (A10-06 is `blocked (a)`); A10-24 is the X11 / Win32 consumer.

#### A2-cpu-R03 a kept surface repaired for a replaced list is not checked for the frame's extent: a frame whose extent moved by whole pixels is composited displaced
- severity: medium (wrong pixels that stay until the key changes; needs content OUTSIDE the frame's box to change at
  both ends, or to move by whole pixels with the same size)
- where: `layout/src/cpurender/raster.rs:3519-3526` (`stale_boxes`), used by `refresh_retained_surfaces` 3654-3712
- algebra: the picture in a surface is `(p - acc) dpi - g` with `g = floor((extent - acc) dpi)`, and a hit
  composites with the LIVE `g`. Within one list the extent is fixed (given `values`), so only `acc` moves `g`, and
  the phase tells whole steps apart from the rest. The repair moves the entry to ANOTHER list and keeps `size` and
  `phase`; when the new list's frame has an extent with the same size and phase but another origin, the key matches
  and the live `g` differs from the `g` the picture was painted with by the number of whole pixels the extent moved.
- example (dpi 1, `acc` 0, frame `scale(1.5)`, box (100, 0) 800 x 300): old list: A (0, 0) 100 x 300 (overflows to
  the left), C (100, 0) 800 x 300: extent (0, 0) 900 x 300, `g = 0`, C at surface x 100..900. New list, same item
  count: A's slot holds B (900, 0) 100 x 300: extent (100, 0) 900 x 300. `stale_boxes`: A's box [0, 0, 102, 300] and
  B's [898, 0, 900, 300] (clamped), 31,200 px of 270,000: repaired (the first box cleared, B is outside the old
  surface), entry re-keyed. The walk: `g = 100`, size (900, 300), phase (0, 0): a hit; surface x 100..900 lands as
  list x 200..1000: C is drawn 100 px (150 device px) to the right, list x 100..200 is transparent, B is missing.
- change: in `stale_boxes` after the `end` test: the extents the walk would use must be equal:
  `let ext = |i: &FrameIndex, items, laid_out| i.extent_with(items, key.push, &render_state.transforms).unwrap_or(laid_out);`
  `if ext(&old_index, &old.items, old_frame.extent) != ext(&index, &new.items, frame.extent) { return None; }`
- test: the example: after the frame, the retained render equals the fresh one (mutation: without the test C is 150
  px to the right).

#### A2-cpu-R04 the repair of a kept surface of the window's own list walks it as a view's list
- severity: low
- where: `layout/src/cpurender/raster.rs:3691` (`WalkCull::ActiveClip` for every slot)
- algebra: inside a group every walk culls by the clip; the modes differ in one thing there: `paint_turned_text_frame`
  is taken only under `WalkCull::ActiveClip` (2686-2687). A frame of the root list is painted under `Damage(..)`:
  its turned text frames are nested groups (ink resampled, halo cut at the extent). The repair paints the same
  frames directly (anchor rounded, outlines turned): the repaired boxes are another picture than a fresh paint.
- example: a kept `scale(1.5)` panel of the window's list with a `rotate(90deg)` caption inside; another item of the
  panel changes under the caption: the box is repainted with the caption drawn directly, up to 0.5 px from, and
  sharper than, the rest of the caption outside the box.
- change: `if entry.placement.slot == usize::MAX { WalkCull::InGroups } else { WalkCull::ActiveClip }` (with the
  isolated entry on the mask stack `in_group` is true, so `InGroups` culls by the clip as the root's walk does).
- test: `a_kept_picture_is_painted_again_...` (rule 1) on a root-list frame with a turned caption: retained == fresh.

#### A2-cpu-R05 the extent of a frame counts a run's ink without the reach of the text shadows it is under
- severity: medium (stale or cut halo pixels; hidden while the reach is under the padding of `visual_bounds`: 1 em
  left, `1 em - the last advance` right, about 0.5 em above and below)
- where: `layout/src/solver3/display_list.rs:3575-3602` (`GroupBounds::step`); consumers: `GroupGeometry::of` (the
  surface), `gpu_value_damage` 2939-2942, the paired diff's frame arm 5273-5279, `windowed_structural_damage`
  (A2-cpu-28), `node_groups_on_screen`
- algebra: under shadows `S` a run paints `ink ⊕ text_shadow_reach(S)` (upright: `render_text_shadow`, source = ink,
  halo `reach` past it, cut by the active clip; directly drawn turned text: the same through the matrix, "a halo is
  no longer cut at the frame's box"). `FrameEntry::extent` unites `visual_bounds()`, the ink alone. So: (a) a moved
  identity / translation frame and a directly drawn turned frame are damaged by `extent` and leave the ring `reach`
  past it stale at the old place and cut at the new; (b) an isolated group's surface IS the extent: the halo is cut
  there; (c) `frame_paints_nothing` and `stale_boxes` widen by `shadow_reach` themselves: they are right.
- example: font 12 px, "mmm" with pens at x = 100, 110, 120 (advance 10), baseline 50, a wide clip: `visual_bounds`
  = x 88..132, y 32..59. `text-shadow: 8px 8px 4px` (reach 12): the shadow paints to x = 130 + 12 = 142 and y = 50 +
  12 = 62. A frame of this label moved by its matrix damages x <= 132, y <= 59: columns 132..142 and rows 59..62 of
  the old shadow stay; as a group the surface cuts them.
- change: `GroupBounds` gets `shadows: Vec<f32>`; in `step`, after the scroll-frame arms: `PushTextShadow { shadow }
  => self.shadows.push(text_shadow_reach(shadow))`, `PopTextShadow => { self.shadows.pop(); }`, and a `Text` item's
  `visual` is widened by the largest entry before `add(clip, b)`. Every open frame steps over the same items, so
  each extent (and `own`) gets it; `shadow_reach` at its two users becomes a harmless second margin. Group surfaces
  grow by the reach (halos are no longer cut in groups either).
- test: a translated frame with the example's label, moved by its matrix: damaged == full (mutation: the ring
  stays); `GroupGeometry::of` of the frame at `scale(2)` covers x up to 142.
- note: the per-ITEM damage of a changed run under an unchanged shadow has the same gap; that one is mine
  (A2-cpu-29, fixed on `audit/A2-cpu`).

#### A2-cpu-R06 directly drawn turned text builds the picture of every run of the frame, and skips a run it cannot build
- severity: medium for a text-only frame with many runs under a scale (a zoomed page of text in a view); none for
  one label
- where: `layout/src/cpurender/raster.rs:3266-3321` (the loop), `3294-3302` (`continue`), `3413-3420`
- algebra: the frame is drawn when ANY part of its surface may land (`frame_paints_nothing`); then every `Text` item
  gets `turned_run_picture` (rasterised whole and kept under a key that holds the matrix's linear part) before its
  box is cut by the clip: a zoom step of a text-only leaf frame of 2,000 lines rasterises 2,000 pictures at the new
  scale, on screen or not. And a picture wider or higher than 8192 px, or an anchor beyond 1e6, is `continue`d after
  the frame was committed to direct drawing: the run is not painted at all, where the group path paints the part
  that lands.
- example: `scale(8)` on a frame holding a 1,200 px line: the turned ink is 9,600 px: no picture, no text.
- change: (1) in the pre-check loop (3194-3216) compute each run's turned box (the corner loop of 3400-3417, as a
  helper) and `return false` (the frame is a group) when it is not finite or exceeds 8192; (2) in the paint loop
  test that box, placed at the anchor, against `clip_box` BEFORE `turned_run_picture`, and skip the run when it
  misses.
- test: a `scale(8)` text-only frame with a 1,200 px line in a view: painted == `AZ_CPU_TURNED_TEXT=group` within
  the anchor's tolerance (mutation: empty); 100 runs of which 3 meet the clip: 3 pictures built.

#### A2-cpu-R07 which kept surface makes room depends on the order in which band threads looked up
- severity: low (no pixel; `retained_hits` / `retained_misses` of later frames, and with them e2e counters)
- where: `layout/src/cpurender/retained.rs:271-277` (`lookup` advances the clock and stamps the entry)
- algebra: two entries both used by a frame get their `last_used` from whichever band's lookup came last; `commit`
  evicts `min_by_key(last_used)`. The module says "what a band hits does not depend on thread timing": true for the
  frame, not for which surface the next frame still has.
- change: advance `clock` in `begin_frame` only and stamp `last_used = clock` in `lookup` (a frame number); ties
  fall to the first entry, whose order `commit` already fixes.

#### A2-cpu-R08 a list with `filter: blur()` is blurred by the full repaint and not by a damaged one
- severity: medium (a sharp patch in a blurred element after any repaint of part of it; before A1-21 too)
- where: `layout/src/cpu_backend.rs:583-589` (`can_reuse_previous_frame`), `1348` (`full_repaint_needs_layers`);
  `raster.rs:5367-5375` (the walk's `PushFilter` arm isolates `flood` / `composite` only)
- algebra: full frame of such a list: layers, `apply_layer_filters` blurs the promoted layer. Damage rect: the flat
  walk, where a blur pushes `MaskEntry::Filter(None)` and nothing is filtered. "Damaged == full" does not hold for
  the one kind of list that still has two paths.
- example: a 200 x 100 box with `filter: blur(4px)` and a caret blinking over it: the caret's damage rect (2 x 18)
  is repainted unblurred inside the blurred box.
- change (now): `let can_reuse_previous_frame = frame_may_reuse_previous_pixels(..) && !needs_layers(display_list)
  && !previous_list_needed_layers;` (`full_repaint_needs_layers` of both lists; one bit kept in the list's
  `FrameIndex` saves the two scans): such a window repaints whole. Design for the rest: a blur as an isolated group
  of the walk (surface = bounds ⊕ radius, part = clip ⊕ radius, painted, blurred, composited under the clip), after
  which no list needs layers.
- test: `a2_cpu_the_damaged_render_is_the_full_render...` with a blurred box in the scene (mutation: today's gate).

#### A2-cpu-R10 an isolating filter group takes its whole box in every pass, whatever the pass's clip
- severity: low (wrong pixels only for a filter chain that leaves colour where the group painted nothing: a bare
  translucent `flood()`, `composite(out)`; otherwise work per pass)
- where: `layout/src/cpurender/raster.rs:5367-5375` (the `PushFilter` arm), `1688-1737` (`FilterGroup`); not A1's
  new code, found while checking item 1 ("a pass writes no pixel outside its rect")
- algebra: `FilterGroup::begin` snapshots and clears `bounds - acc` as it is; the push is a state item, so every pass
  (every damage rect, every band) does it for every such group of the list, then filters the box and writes "filtered
  over backdrop" back. Outside the pass's clip the group painted nothing (its items are clipped): the box there is the
  filter of nothing over the pixels of the frame before. For `flood() composite(in)` (the icon tint) that is the
  backdrop again: no change, four sweeps of the box per pass. For a chain that is not transparent there, it is blended
  onto pixels that already hold it, outside the rect the pass was given (in a band's own pixmap the rows outside the
  band are not copied back, the columns outside `[x0, x1)` neither: bands and one pass differ there).
- example: a 24 x 24 box at (10, 10) with `filter: flood(rgba(0, 0, 0, 0.5))`; a caret blinks at (300, 200): the pass
  of the caret's rect (2 x 18) clears the box and writes half black over what it showed: darker with every blink.
- change: the region of the group is its box INSIDE the clip, as an opacity group's is (`opacity_group_region`):
  `intersect_clips(clip_stack.last().copied().flatten(), logical_rect_to_az_rect(..))`, and no group (`None`) when
  that is empty.
- test: a full paint, then `render_display_list_damaged` with a rect beside the box: the box's pixels are unchanged
  (mutation: today's arm darkens them).

## Review of A10

Branch `audit/A10` at `5cabb06a2`, macOS only (the other shells' arms were not read). Read: `layout/src/window.rs`
676-1057 (`FrameDamage::to_rects_physical`, `present_geometry`, `LayoutWindow::frame_scale`), `macos/mod.rs`
2126-2211 (the pure rect functions), 2277-2323, 3246-3420 (`drawRect:` to the draw from the framebuffer), 4262-4330
(`update_framebuffer`), 4417-4507 (spans, invalidation), 8250-8360 and 9564-9660 (the DPI writes), 10646-10790 (the CPU
arm of `render_and_present_in_draw_rect`), `audit/A10.md` Spaces, A10-01..04, A10-17, the contract of `present_moves`,
the space mapping. Not read: the bitmap path below 3417, `draw_framebuffer_span*`, `resize_framebuffer`, the layer plan.

No finding. Per item:

1. **Present rect, flipped and in points** (`present_rect_in_view_points`, 2180-2194; `flip_rect_y`, window.rs
   920-923): read, agree. `(x, y, w, h)` in D (top-left, y down) of a frame `H` px high becomes `(x / s, (H - (y + h))
   / s, w / s, h / s)` in the view's points (bottom-left, y up; the view is not flipped). On the user's display (frame
   2940 x 1912, `s = 2`), a pan strip (2914, 40, 26, 1800): `y' = 1912 - 1840 = 72`: points (1457, 36, 13, 900).
2. **Rows copied** (`dirty_rows_to_copy` = `rows_of_bottom_left_rect`, window.rs 935-952): read, agree: the inverse
   of the flip, outward: `y0 = floor(H - (y + h))`, `y1 = ceil(H - y)`, `x0 = floor(x)`, `x1 = ceil(x + w)`, clamped.
   The dirty rect AppKit hands back, in backing px (2914, 72, 26, 1800): rows 40..1840, columns 2914..2940: the rows
   and columns invalidated. The inward twin (`rows_inside_backing_rect`, ceil / floor) is only used to decide that a
   rect is ALREADY being drawn, where inward is the safe side.
3. **Source and destination rects** (`bitmap_source_rect`, 2200-2211; 3377-3392): read, agree. Rows `y0..y1` of a
   top-row-first buffer are `(x0, H - y1, x1 - x0, y1 - y0)` in the bitmap's bottom-left space: (2914, 72, 26, 1800);
   the destination is that through `convertRectFromBacking:`: (1457, 36, 13, 900) points: what was invalidated.
4. **D rects from damage** (`to_rects_physical`, window.rs 724-759): read, agree: floor / ceil outward, clamped: the
   rule of "damage rect -> cleared / repainted box" in my P -> D table and of the memmove region. A painted rect comes
   back from the renderer as `x0 / dpi` and is multiplied again here; in `f32` that can floor one pixel lower or ceil
   one higher at a fractional factor: a superset by one pixel, never less (exact at 1 and 2).
5. **Frame size** (`frame_size_px`, window.rs 862-867; `cpu_view_framebuffer_size`, 2162-2169): read, agree:
   `ceil(logical x factor)` in `f32`, the expression of `CpuBackend::render_frame` (`cpu_backend.rs:390-391` on
   `audit/A1`), on `bounds as f32` (what the state holds). 1470 x 956 pt at 2: 2940 x 1912.
6. **`frame_scale()` against my `frame_dpi_factor()`**: the same number wherever mine is used.
   `LayoutWindow::frame_scale()` = the shell's exact factor if one was set, else `present_geometry::frame_dpi_factor(
   size.dpi)` = `dpi / 96`, 1.0 for a DPI of 0: the body of mine. `set_frame_scale` is called in `wayland/mod.rs`
   only (6013, 7964, 10369); macOS writes `size.dpi = (scale x 96) as u32` (8353, 9640: 96, 192, 288, exact) and
   renders at `layout_window.frame_scale()` (10659). My one user outside tests is `virtual_view_content_offset` (the
   snap grid of a view's offset), which must be the factor the frame is rendered with: that is what `frame_scale()`
   is. So the merge (`frame_dpi_factor()`'s body becomes `self.frame_scale()`) changes nothing on macOS, X11, Win32
   and the headless shell, and on Wayland's fractional path it makes the grid the frame's (today mine would snap on
   `dpi / 96` = 1.104 where the frame is rendered at 1.1: every pan step off the grid, the view repainted whole).
   `a2_cpu_13_...` sets `size.dpi` and renders at `dpi / 96` with no override: unchanged.
   For the integration (not a finding): `dll/.../headless/mod.rs` 1759, 5569, 11017, 12432 still compute
   `ws.size.dpi as f32 / 96.0` inline: equal to `frame_scale()` for every DPI above 0, and 0.0 where `frame_scale()`
   says 1.0 for a DPI of 0 (no frame either way); one call would make it one source.
7. **The space mapping** (`audit/A10.md`, "Space names"): agree with every row: A10's logical px = P of the root
   list, buffer px = D, its factor = `dpi_factor`; L of a child list, G and Y never reach a shell.
8. **The contract of `present_moves`**: agree; item 7's example is the Shift table's row for a content move to the
   left (`px_dx = +12`: `dst = (48, 40, 292, 200)`, `dx = -12`, the strip (339, 40, 13, 200) presented after it).
   Point 2 of the contract (the invariant) does not hold for a frame with a layout blit: A2-cpu-R02, on the
   producer's side.

Remark for the real window: `drawRect:` takes the scale as `backing.width / bounds.width` (3257-3261). It is 1 or 2
exactly as long as `convertRectToBacking:` does not round a fractional width; `self.window().backingScaleFactor()` is
the number itself. Only bounds that are not whole points could show a difference (A10-02's log line would say so).

## Review of A6

Branch `audit/A6` at `12460bb7a`; the commit is `1f910ffbc` ("A6-01, A6-02, A6-03: a box is hit where it is drawn"),
with `da6be4605` (A6-06) and `31884b79c` (A6-07) for the forward direction. Read: `layout/src/headless.rs` 690-1145
(links, `frame_to_screen`, `frame_to_local`, the chain maps), 1220-1301 (`rect_to_screen`), 1390-1512
(`compute_node_chains`), 1529-1542 and 1733-1899 (placements, the host chain), 2150-2223 (`map_point_through_chain`,
`sync_view_placements`), 2395-2462 (the frame corners); `core/src/gpu.rs` 257-332 (`reference_frame_of`,
`painted_transform_of`); the builder's three `push_reference_frame` sites (`display_list.rs` 7028-7053, 7690-7707,
8614). Not read: the index and the grids (A6-04, A6-05), the tests' bodies.

A2-cpu-03 is fixed by it. Per item:

1. **The inverse mapping**: read, agree. A frame's plane map is `[a b c; d e f; g h i] = [m00 m10 m30; m01 m11 m31;
   m03 m13 m33]` (910-923), the nine numbers of the rasteriser's `layer_h` in logical px; forward `o + ((a qx + b qy
   + c) / w, (d qx + e qy + f) / w)`, `w = g qx + h qy + i`, `q = p - o` (947-966): `o + M (p - o)`. The inverse is
   the adjugate applied to `(qx, qy, 1)` and divided by its third coordinate (974-1004); I checked the nine cofactors
   against the matrix and the determinant. Example (the one of `## For area A6`): box (300, 200) 100 x 12,
   `rotate(90deg)` about (0, 12): `a = 0, b = -1, c = 12, d = 1, e = 0, f = 12, g = h = 0, i = 1`, det = 1. Window
   point (306, 262): `q = (6, 62)`; `x = 0 x 6 + (0 + 1) x 62 + (-12 - 0) = 50`, `y = (0 - 1) x 6 + 0 + (12 - 0) =
   6`, `w = 1`: list point (350, 206); forward: `x' = 0 x 50 - 1 x 6 + 12 = 6`, `y' = 50 + 0 + 12 = 62`: (306, 262).
2. **The order of the links**: read, agree. A chain is outermost first (`sort_by` depth, then the reference frame of
   a box before its scroll frame, 1499); the inverse walks it in that order (`chain_to_local`, 1063-1073), the
   forward map in reverse (1050-1060). The builder pushes a node's frame before its own items and its scroll frame
   around its children, so a box is inside its own frame and outside its own scroll frame: the chain of a box holds
   its own `Transform` (`with_own`) and the scroll frames of its STRICT ancestors. A2's example for the order: a
   scroller scrolled (0, 30) around a box (100, 100) 50 x 50 with `scale(2)`: painted (100, 70) 100 x 100; window
   (190, 160): scroll first, `(190, 190)`, then the frame, `100 + (90, 90) / 2 = (145, 145)`: inside. (The frames are
   the node's transformed LAYOUT ancestors: right, because a transform makes its box the containing block and the
   stacking context of everything under it, so the list nests them the same way.)
3. **Content and scroll offsets**: read, agree. A child's static space is `L + bounds.origin + host placement +
   content_offset at build` (1830-1836); the `View` link adds `content_offset now - at build` (1014-1017, 2199-2222),
   read off the view's item, i.e. the snapped offset the walk paints with (A2-cpu-13). The child's own frames keep
   their corners in static space, which is right: `(o + shift) + M ((q + shift) - (o + shift)) = o + M (q - o) +
   shift`. The view's box is a clip in the HOST's chain without the `View` step (1849-1857): the content moves
   through the box. Scroll: `q - s` forward with `ScrollManager::get_current_offset`; an incremental frame paints at
   the baseline offsets (A2-cpu-20), which differ from the current ones by less than one device pixel: nothing a hit
   test can see.
4. **Which matrix**: read, agree. `GpuStateManager::painted_transform_of` = `GpuValueCache::reference_frame_of`: the
   CSS channel, else the animation channel, else the sticky one: the value the CPU frame reads through the frame's
   key (the live value, of every dom), and the rule by which the builder opens a frame at all. A frame's corner is
   taken from the list's own `PushReferenceFrame::bounds` (2423-2444), so it is the corner the rasteriser turns
   about (of a box broken across regions: the corner of its first part). One finding (R09, the value of a key that has none).
5. **Frames that cannot be inverted**: read, agree. `|det| < 1e-12` or a non-finite result: no point inside the
   frame, nothing in it is hit (985-987, 996-998); a point whose forward `w` is not above 1e-9 is behind the eye and
   not hit (1000-1002). The rasteriser draws nothing in both cases (the composite of a singular matrix returns, a
   destination pixel with `w <= 0` is skipped). A matrix that is a translation exactly takes the path without a
   division; one within the rasteriser's 1e-4 of a translation is inverted as it is (the two differ by 1e-4 x the
   distance from the corner: 0.1 px at 1000 px).

#### A2-cpu-R09 a reference frame whose key has no live value is hit as the identity; the rasteriser paints it with the list's own matrix
- severity: low (the builder opens a frame only for a node with a key AND a value, so it needs a list that outlives
  its value: the frame after an animation's key is removed, until the list is rebuilt; or a hand-made list)
- where: `layout/src/headless.rs:1018-1023` and `1039-1044` (`None => Some(p)`)
- algebra: `raster.rs`, the `PushReferenceFrame` arm: `render_state.transforms.get(key).map_or(&initial_transform.m,
  ..)`: without a value the frame is painted with the matrix the list was built with (the rule of A2-cpu-01). The
  links map such a frame with the identity.
- example: a card slid by the animation channel, `translate(40, 0)` baked at the last build, its key dropped by the
  finished animation, the list not rebuilt yet: painted at +40 (by a repaint in that state), hit at +0.
- change: keep, next to `frame_corners` (2423), the `initial_transform` of each owner's frame and of each host frame
  read in `resolve_virtual_view_placements`, and resolve a link with `resolve_transform(d, n).or(baked)`.
- note: the forward `rect_to_screen` (1255-1261) takes the corner from `calculated_positions`, the tester from the
  list's `bounds`: the same point except for a box broken across regions, where the list holds the part's rect.

## What to look at on the real window

Nothing of A2-cpu's is behind a switch: every change is a rect, a sign or a rounding with a headless test. What a
Retina window (2940 x 1912 px, factor 2) shows of them, with the CPU backend (`AZ_BACKEND=cpu`):

- A2-cpu-13 / -07: a pan of the map at rest (scale 1) moves pixels and paints strips. With `AZ_BLIT_DEBUG=1` no line
  `[viewshift] ... is no whole device pixel` may appear during a drag (offsets are multiples of 0.5 pt); a line per
  frame means the offset is off the frame's grid and every pan step repaints the view.
- A2-cpu-29 / -05 / -18: edit a label with a halo (shorter text, another halo colour) and drag one: no remains of the
  old halo around the old place, no halo cut along a straight edge at the new one.
- A2-cpu-26: a translucent box during a fade (a selection overlay, a tooltip fading in): its edges do not step by a
  pixel between the last translucent frame and the opaque one.
- A2-cpu-14 / -06: a scaled or turned box at the window's level (a dialog scaling in): no one-pixel jump between the
  first full frame and the damaged frames after it, and no one-column gap at a clip that lies between pixels.
- From the review (A1's code, `## Review of A1`): during a SLOW zoom the turned labels are placed on whole device
  pixels (item 8, remark a): compare with `AZ_CPU_TURNED_TEXT=group`; a label whose halo passes its box when it is
  dragged (R05); a box with `filter: blur()` under a blinking caret (R08).

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

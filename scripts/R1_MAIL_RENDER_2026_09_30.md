# R1_MAIL_RENDER - mail display engine bugs (E-TABLE, E-GRAD)

Branch `wt/r1-mail-render`, from `abbba2408`. Nothing was compiled (house rule); every Rust file was
parse-checked with `rustfmt --check`. The parent compiles and runs the commands below.

## What was built

### E-TABLE: the receipt crash, root cause

`measure_cell_content_width` (`layout/src/solver3/fc.rs`) measured a cell's min/max-content width by
handing it `f32::MAX / 2` as its width, typed MinContent/MaxContent. But
`ContainingBlock::from_flattened_with_width_type` reads every FINITE width as a definite length, so
the cell was laid out in a real 1.7e38 px containing block. A text cell was unaffected: its auto width
is its intrinsic max-content width. A cell WITHOUT text (`<td colspan=2><hr></td>`, or a
`width: 100%` div) has intrinsic width 0, so it took the whole 1.7e38 px. The `<hr>` or div filled it,
the spanning cell measured ~1.7e38, and `distribute_cell_width_across_columns` gave both spanned
columns ~0.85e38.

What followed:
- The price column's text was placed ~0.85e38 px to the right: the "missing price column" of the
  fixed receipt.
- The rule came out 1.7e38 wide: the "`<hr>` wider than the table".
- The LCD rasterizer panicked on the far-off glyphs.

The fix is in the same file:
1. The measurement builds its containing block with typed axes (`ContainingBlock::from_axes`). The
   flattened sentinel is kept only as the cache key. A cell's auto width is now its measured
   contribution, and a percentage inside it behaves as auto (css-sizing-3 5.2.1).
2. The measurement resets the cell's `used_size` first. A table cell's own layout never overwrites
   a `used_size` that is already set, and `layout_bfc` lays out the children inside it, so a
   re-layout used to measure inside the previous layout's column width.
3. `layout_cell_for_height`, block branch: the cell gets its column width before its final layout.
   Until now the children were laid out inside the measurement's width. After fix 1 that would be
   0 for the rule, before it was 1.7e38.
4. The same branch now reads the content height as the content box (the laid-out extent, or the
   measured border box minus padding and border). It used to read the measured border box, and the
   sum added padding and border a second time. So every block-content cell's row was
   2 x (padding + border) too tall, and `vertical-align: middle` pushed its content down. That
   pushed-down content is what the `nl` probe shows.

### Rasterizer hardening (`layout/src/cpurender/raster.rs`)

agg keeps glyph cells as `i32` (24.8 fixed point, and the LCD path triples x). A pen at 1e38
saturates to `i32::MAX`, then `int_x * 3` wraps and the cells' x range straddles all of `i32`.
`ScanlineU8::reset` then allocates a one-cover span, which gives `index out of bounds: the len is 1
but the index is 1`. AzWidgets aborted the same way with no table on screen (coordinator note), so
the guard is needed on its own.

Every glyph path now goes through three helpers: the LCD batch sweep `render_glyphs_lcd`, the LCD
pre-blended tiles and the grayscale `render_text`, which text-shadows also take.
- `glyph_ink_reach(em)`: 4 em. It returns `None` for an em that is not finite, not positive or
  larger than 65536 px, and the run is skipped.
- `glyph_pen_reaches_pixmap`: a pen that is not finite, or cannot put ink on the pixmap, is skipped
  before the glyph is decoded. Everything it admits stays within 4 em of the pixmap, far inside
  agg's integer range.
- `text_clip_pixel_box`: the clip is intersected with the pixmap before its `i32` casts, so the
  stripe clip's `* 3` cannot overflow. A clip that covers no whole pixel skips the run: agg's
  `clip_box_i` normalizes an inverted box, so a sub-pixel clip used to paint two pixels.

The tile path's proven-rect inset now saturates. Clips inside the pixmap produce exactly the integer
boxes they produced before. Path fills were already guarded (`ClampVertexSource` plus the
rasterizer clip box).

### Newsletter: why the body text was missing

Mail HTML is indented, so the newsletter's body `<td>` has whitespace text children next to its
`<h1>` and `<p>`. `layout_cell_for_height` took its inline branch for any cell with a Text child,
whitespace included. That branch lays the cell out as one IFC, which does not lay the blocks out, and
then clears the `inline_layout_result` of every child, wiping the heading's and the paragraph's text.

The inline branch now needs loose (non-whitespace) text, or only inline-level children. It reuses
`layout_tree::is_whitespace_only_text` and `has_only_inline_children`.

The exploration's reduction `nl.html` had lost the indentation, so it does not reproduce this. Its
smeared, left-shifted text at a 700 px window is the headless-harness caveat of section 1.3, plus the
double padding pushing the content down. The flat variant is kept as a guard test.

### E-GRAD: the gradients mail quote bars need

All renderers now get the gradient line and the stop offsets from one resolver, in
`css/src/props/style/background.rs` and `css/src/props/basic/direction.rs`:
- `Direction::gradient_line(w, h)` implements CSS Images 3.1.1. The line runs through the centre at
  the angle (0deg up, clockwise), `|w sin a| + |h cos a|` long. `to <corner>` uses
  `DirectionCorner::css_angle_degrees` ("magic corners"). The old angle arm negated the angle, so
  `90deg` ran mirrored (every non-axis angle did). `to_points` now rounds `gradient_line` for angles.
- Stop positions are `<length-percentage>`. The parser takes absolute lengths and a bare `0`.
  `NormalizedLinearColorStop` gains `offset_px`: a position is `offset` of the line plus `offset_px`
  px, so an auto stop between `10px` and `90%` is interpolated exactly.
  - `em`/`rem`/`vw` are refused, never guessed: a stop list has no font size or viewport.
  - Radial stops at a length stay refused, because the radial renderers place percentages only.
  - The linear normalizer is now its own function; the old macro remains for conic.
- `LinearGradient::resolve_in_box(w, h)` places each stop at its length divided by the line length.
  A stop that sits before the previous one moves up to it (on the real line, so mixed units order
  correctly), and hard stops are kept as two stops at one offset.
- `color_stops_on_the_line` cuts the stops to 0..=1 and interpolates the colour at an end the stops
  reach past.
- `ResolvedLinearGradient::to_repeat_period` gives a repeating gradient its first..last stop period.

On the CPU, one LUT builder, `build_gradient_lut`, now serves linear, radial and conic:
- agg keeps one stop per offset, which left `red 50%, blue 50%` transparent. The second stop of a
  hard pair is now nudged 1e-9 past the first.
- The LUT has one entry per line pixel (256..=4096), with gradient space in device px, so a 3 px bar
  in a 600 px block keeps its 3 px.
- `repeating-linear-gradient` now repeats on the CPU too (agg's repeat adaptor).

The GPU path (`dll/src/desktop/compositor2.rs`) uses the same resolver, cut and period. WebRender
draws two stops at one offset as a hard change.

## Commits

| Commit | Kind | What |
|---|---|---|
| `5ea61159c` | RED | raster unit tests: unplaceable glyph pens (NaN, inf, +-1e38) and sub-pixel clips on the LCD sweep, grayscale and tile paths |
| `1e4a14e02` | FIX | raster guards `glyph_ink_reach` / `glyph_pen_reaches_pixmap` / `text_clip_pixel_box`, plus font-free unit tests of the three |
| `56e2efc87` | RED | `a_receipts_price_column_sits_beside_its_labels_under_a_full_width_rule` (bis_D) and `a_heading_and_paragraph_in_an_indented_table_cell_paint_their_text` (sample 01 and the flat `nl.html` guard) |
| `3c8ac84cf` | FIX | table: typed measurement, `used_size` reset, column width before the final cell layout, content-box height, inline branch only for inline cells |
| `bb5e41e2c` | RED | `quote_bars_from_one_gradient_paint_each_colour_at_its_length` (`red 0 3px, transparent 3px 6px, blue 6px 9px, transparent 9px`) |
| `fb05834f9` | FIX | E-GRAD: `gradient_line`, length stops (`offset_px`), `resolve_in_box` / `color_stops_on_the_line` / `to_repeat_period`, CPU LUT and GPU path; old tests that pinned the bugs now state CSS; literal sites get `offset_px` |
| `0d1f13626` | FIX | compositor2: the gradient-cut closure spells its types |
| (progress commits) | | `scripts/R1_MAIL_RENDER.PROGRESS.md` |

The E-TABLE RED (`a40976f60`) and the E-GRAD RED (`222b44d18`) were already on the base.

## api.json (parent: run autofix, do not hand-edit)

- `NormalizedLinearColorStop` gets a new struct field `offset_px: FloatValue`, appended after
  `color`. It stays `repr(C)`: 16 bytes become 24, with no padding (`offset` 8/align 8,
  `color` 8/align 4, `offset_px` 8/align 8). `NormalizedLinearColorStop::new` fills it with 0.
- The following are Rust-only, not for api.json: `Direction::gradient_line`,
  `DirectionCorner::css_angle_degrees`, `LinearGradient::resolve_in_box`, `ResolvedLinearGradient`
  (+ `to_repeat_period`), `color_stops_on_the_line` (generic, takes a closure), and
  `LinearColorStop.offset1_px` / `offset2_px` (a parse-only type that is not in api.json).
- Codegen goldens change: every printed stop gains `offset_px`. Bless them with
  `AZ_BLESS=1 cargo test -p azul-css --features codegen --test codegen_goldens` and review the diff.
  It should only add `offset_px` to `NormalizedLinearColorStop` literals.

## Files outside my task that I had to touch (minimal)

- `layout/src/widgets/themes/flat.rs`: 8 existing `NormalizedLinearColorStop { .. }` const literals
  each got one inserted line, `offset_px: azul_css::props::basic::FloatValue::const_new(0)`. Nothing
  was reordered. The type change forces this.
- Same one-line insertion in `layout/src/widgets/{list_view,node_graph,progressbar,tabs}.rs` and
  `tests/src/css-parser.rs`.
- `css/src/dynamic_selector.rs` now carries `offset_px` through system-colour resolution.
- `css/src/codegen/{format,lower_types}.rs` now print and lower the new field.
- I did not touch `page_breaks.rs` or `a_padded_table_cell_stays_in_its_row.rs`.

## Least sure to compile

1. `css/src/props/style/background.rs`, `color_stops_on_the_line`: it is generic over `C: Copy` with
   an `impl Fn(C, C, f32) -> C` parameter and a let-else over `(stops.first(), stops.last())`.
2. `background.rs`, `get_normalized_linear_stops`: `let value = if all_percent { percent } else { px }`
   picks between two `&mut f32` from an `if let Some((percent, px)) = pos` over `&mut Option<(f32, f32)>`.
3. `layout/src/cpurender/raster.rs`, `render_linear_gradient`: the `device_line` closure borrows
   `rect` and is called twice. There is also `#[allow(..)]` on a `let` statement, and
   `GradientRepeatAdaptor::new(GradientX)` passed to the generic `agg_fill_gradient_clipped`.
4. `dll/src/desktop/compositor2.rs`: two block-scoped `use`s inside the `LinearGradient` match arm
   (`ExtendMode`, `ColorU as CssColorU`); they may shadow outer names.
5. `layout/src/solver3/fc.rs`, `layout_cell_for_height`: `let styled_dom = ctx.styled_dom;` and the
   `is_text` closure are used twice before `ctx` is borrowed mutably again.
6. New RED tests that call private fns (`render_glyphs_lcd`, `render_text`, `render_text_with_bg`)
   from a sibling test module through `lcd_pretile_tests::{load_test_font_pub, rr_with_pub, shape_pub}`.

## Commands for the parent

Pins from the prompt:

```
cargo test --release -p azul-layout --test all a_full_width_rule_in_a_spanning_table_cell_renders
cargo test --release -p azul-layout --test all a_linear_gradient_puts_its_colours_where_css_says
```

My new tests:

```
cargo test --release -p azul-layout --test all a_receipts_price_column_sits_beside_its_labels_under_a_full_width_rule
cargo test --release -p azul-layout --test all a_heading_and_paragraph_in_an_indented_table_cell_paint_their_text
cargo test --release -p azul-layout --test all quote_bars_from_one_gradient_paint_each_colour_at_its_length
cargo test --release -p azul-layout --lib unplaceable_glyph_geometry_tests
cargo test --release -p azul-layout --lib gradient_lut
cargo test --release -p azul-css --lib gradient_line_follows_css_images_3
cargo test --release -p azul-css --lib a_linear_gradient_resolves_its_stops_on_its_line
cargo test --release -p azul-css --lib background
```

Regressions to watch:

```
cargo test --release -p azul-layout --test all table
cargo test --release -p azul-layout --lib cpurender
cargo test --release -p azul-css --lib direction
cargo test --release -p azul-test            # tests/src/css-parser.rs
AZ_BLESS=1 cargo test -p azul-css --features codegen --test codegen_goldens   # then review
```

## Behaviour changes to expect

- Rows of block-content table cells (`<td><span>..</span></td>`, `<td><p>..</p></td>`) are
  2 x (padding + border) shorter: the doubled padding is gone. With the UA `td` padding of 1 px that
  is 4 px per row. A test that pinned the doubled height needs its number changed. The other session's
  `a_padded_table_cell_stays_in_its_row` only checks relative stacking, so it is unaffected.
- A table re-layout now measures like the first layout instead of inside the previous column
  widths.
- `90deg`, `270deg` and every non-axis angle now run as CSS says. The flora theme's `deg(90)`
  "off the left edge" shadows now really sit on the left; before, they were on the right.
- `repeating-linear-gradient` repeats on the CPU.

## What is left

- The min-content measurement of a text cell still returns its max-content width: the `TableCell`
  arm of `calculate_used_size_for_node` ignores MinContent. As a result, table columns never shrink
  below max-content.
- `calculate_table_intrinsic_sizes` ignores `colspan`.
- A cell holding loose text AND blocks (`<td>Label<div>..</div></td>`) still takes the inline branch,
  and the clearing still drops the block's text. A follow-up should clear only inline-level
  children.
- Radial and conic stops at a length are still refused (they need the ray length in the radial
  renderers). `em`/`rem` gradient stops are refused (the display list carries no font size).
- The AzWidgets pens that hit the rasterizer are made harmless but not root-caused. They are most
  likely another layout sentinel leak like the table one. A one-time log line when
  `glyph_pen_reaches_pixmap` drops a glyph would find them.
- The headless-harness smear at narrow windows (section 1.3) is not addressed.

# Fix Gradients and CSS Filters Rendering

## Context

Gradients don't render properly when there are multiple gradients in one frame, CSS filters (`blur`, `grayscale`, `brightness`, etc.) are completely non-functional, and box-shadows are rendered as flat rectangles. The root causes are:

1. **WebRender stop consumption bug**: The custom Vec-based serialization (replacing serde) has a greedy-read bug where `SetGradientStops` consumes ALL remaining stops, so only the first gradient in a display list gets its stops — subsequent gradients get zero stops.
2. **Missing offset application**: Gradients don't apply stacking context offsets (`apply_offset`), so they render at wrong positions in nested contexts.
3. **Missing border-radius clipping**: Gradients ignore their `border_radius` field entirely.
4. **Radial/conic center not DPI-scaled**: Centers are computed in logical pixels but the rect is in physical pixels.
5. **Filters never generated**: The display list builder never reads `filter`, `backdrop-filter`, `box-shadow`, `opacity` from the CSS property cache — no display list items are ever created.
6. **Compositor stubs**: Even if filter items existed, compositor2.rs ignores filter data (pushes empty stacking contexts).
7. **Missing CSS filter types**: `StyleFilter` lacks standard CSS filter functions (`grayscale`, `brightness`, `contrast`, `hue-rotate`, `invert`, `saturate`, `sepia`).

## Plan

### Step 1: Fix WebRender gradient stop consumption (CRITICAL)

**File**: `webrender/api/src/display_list.rs`

Add a `stop_count` field to `SetGradientStops` handling. The `push_stops` method should record how many stops it pushes, and the iterator should only consume that many.

- Change `DisplayItem::SetGradientStops` to `DisplayItem::SetGradientStops { stop_count: usize }` in `webrender/api/src/display_item.rs`
- Update `push_stops()` (line 1808) to push `SetGradientStops { stop_count: stops.len() }`
- Update `next_raw()` (line 867) to only advance `stop_index` by `stop_count`:
  ```rust
  SetGradientStops { stop_count } => {
      let end = (self.stop_index + stop_count).min(self.payload.stops.len());
      self.cur_stops = &self.payload.stops[self.stop_index..end];
      self.stop_index = end;
  }
  ```

Apply the same fix for `SetFilterOps`, `SetFilterPrimitives`, and `SetPoints`.

**Files**:
- `webrender/api/src/display_item.rs` — add count fields to marker variants
- `webrender/api/src/display_list.rs` — update push methods and iterator

### Step 2: Fix gradient offset, DPI, and border-radius in compositor2.rs

**File**: `dll/src/desktop/compositor2.rs`

For all three gradient types (linear ~line 1429, radial ~line 1506, conic ~line 1662):

a) **Apply offset**: Add `let current_offset = current_offset!(); let rect = apply_offset(rect, current_offset);` (same as `Rect` item at line 188-190)

b) **DPI-scale radial/conic centers**: Multiply center_x/center_y by `dpi_scale` (or compute from `scaled_width`/`scaled_height` instead of `bounds.size`):
```rust
let center_x = match ... {
    Center => scaled_width / 2.0,  // not bounds.size.width / 2.0
    ...
};
```

c) **Border-radius clipping**: Add the same clip region logic used by `Rect` items (lines 219-270) — create a rounded rect clip when `border_radius` is non-zero.

### Step 3: Add missing CSS filter variants to `StyleFilter`

**File**: `css/src/props/style/filter.rs`

Add variants:
```rust
pub enum StyleFilter {
    // existing...
    Brightness(PercentageValue),
    Contrast(PercentageValue),
    Grayscale(PercentageValue),
    HueRotate(AngleValue),
    Invert(PercentageValue),
    Saturate(PercentageValue),
    Sepia(PercentageValue),
}
```

Update the parser `parse_style_filter()` (line 640) to recognize these function names. Update `PrintAsCssValue` and `FormatAsRustCode` impls.

### Step 4: Generate filter/box-shadow/opacity display list items

**File**: `layout/src/solver3/display_list.rs`

In `generate_for_stacking_context()` (~line 1711), after `builder.push_stacking_context()` and before painting children:

- Read `get_opacity()` from prop cache — if < 1.0, emit `PushOpacity { bounds, opacity }`
- Read `get_filter()` — if non-empty, emit `PushFilter { bounds, filters }`
- Read `get_backdrop_filter()` — if non-empty, emit `PushBackdropFilter { bounds, filters }`
- At corresponding pop points (~line 1769 before `pop_stacking_context`), emit the matching `Pop*` items

In `paint_node_background_and_border()` (~line 2336), before painting backgrounds:

- Read `get_box_shadow_left/right/top/bottom()` — emit `BoxShadow` items before the background rect

Note: `get_opacity` is already called at line 3302 but only for stacking context detection (`needs_stacking_context`). The actual `PushOpacity` item emission is missing.

### Step 5: Wire up compositor2.rs filter/shadow stubs to actual WebRender calls

**File**: `dll/src/desktop/compositor2.rs`

a) **PushFilter** (line 1781): Convert `StyleFilter` variants to `FilterOp`:
```rust
let wr_filters: Vec<FilterOp> = filters.iter().map(|f| match f {
    StyleFilter::Blur(b) => FilterOp::Blur(b.width.to_px(), b.height.to_px()),
    StyleFilter::Opacity(o) => FilterOp::Opacity(PropertyBinding::Value(o.normalized()), o.normalized()),
    StyleFilter::Grayscale(v) => FilterOp::Grayscale(v.normalized()),
    StyleFilter::Brightness(v) => FilterOp::Brightness(v.normalized()),
    StyleFilter::Contrast(v) => FilterOp::Contrast(v.normalized()),
    StyleFilter::HueRotate(a) => FilterOp::HueRotate(a.to_degrees_raw()),
    StyleFilter::Invert(v) => FilterOp::Invert(v.normalized()),
    StyleFilter::Saturate(v) => FilterOp::Saturate(v.normalized()),
    StyleFilter::Sepia(v) => FilterOp::Sepia(v.normalized()),
    StyleFilter::ColorMatrix(m) => FilterOp::ColorMatrix(m.as_f32_array()),
    StyleFilter::DropShadow(s) => FilterOp::DropShadow(Shadow { ... }),
    _ => FilterOp::Identity,
}).collect();
builder.push_simple_stacking_context_with_filters(origin, spatial_id, flags, &wr_filters, &[], &[]);
```

b) **PushOpacity** (line 1829): Use `FilterOp::Opacity(...)` in a stacking context.

c) **PushBackdropFilter** (line 1805): Use `builder.push_backdrop_filter(...)`.

d) **BoxShadow** (line 1738): Use `builder.push_box_shadow(...)` with proper blur_radius, spread_radius, border_radius, and clip_mode.

### Step 6: Fix `get_backdrop_filter` bug

**File**: `core/src/prop_cache.rs` (line ~3623)

Change the query from `CssPropertyType::Filter` to `CssPropertyType::BackdropFilter`.

## Key Files

| File | Changes |
|------|---------|
| `webrender/api/src/display_item.rs` | Add count fields to `SetGradientStops`, `SetFilterOps`, `SetFilterPrimitives`, `SetPoints` |
| `webrender/api/src/display_list.rs` | Fix push methods and iterator to use counts |
| `dll/src/desktop/compositor2.rs` | Fix gradient offset/DPI/clip; implement filter/shadow/opacity rendering |
| `css/src/props/style/filter.rs` | Add Grayscale/Brightness/Contrast/HueRotate/Invert/Saturate/Sepia variants + parsing |
| `layout/src/solver3/display_list.rs` | Generate filter/box-shadow/opacity display list items |
| `core/src/prop_cache.rs` | Fix `get_backdrop_filter` querying wrong property type |

## Verification

1. Create a test page with multiple gradients (linear, radial, conic) — verify all render correctly, not just the first
2. Test `filter: blur(5px)`, `filter: grayscale(100%)`, `filter: brightness(1.5)` etc.
3. Test `box-shadow: 10px 10px 20px rgba(0,0,0,0.5)` with border-radius
4. Test gradient with `border-radius` — should clip to rounded corners
5. Test nested gradients inside scroll frames / stacking contexts — verify correct positioning
6. Use debug server `take_screenshot` to verify rendering matches

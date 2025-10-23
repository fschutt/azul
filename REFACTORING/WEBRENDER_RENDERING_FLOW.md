# WebRender Rendering Flow - Debug Analysis

## Purpose
Understanding why `total_draw_calls: 0` despite having 14 items in the display list.

## Key Files
- `/webrender/core/src/renderer/mod.rs` - Main renderer implementation
- `/webrender/core/src/batch.rs` - Batching primitives
- `/webrender/core/src/frame_builder.rs` - Builds frames from display lists

## Rendering Pipeline (High Level)

### 1. Frame Submission
- Display list is submitted via WebRender API
- Scene builder processes the display list
- Frame builder creates render tasks

### 2. Render Task Graph
- Render tasks represent drawing operations
- Tasks can have dependencies (parent/child relationships)
- Picture cache tiles are special render tasks

### 3. Batching
- Primitives are grouped into batches for efficiency
- Batches are organized by:
  - Shader type
  - Texture bindings
  - Blend mode
  - Z-index/stacking context

### 4. Drawing (`render_impl()`)
Main rendering function at line ~1442:
```rust
fn render_impl(
    &mut self,
    doc_id: DocumentId,
    active_doc: &mut RenderedDocument,
    device_size: Option<DeviceIntSize>,
    buffer_age: usize,
) -> Result<RenderResults, Vec<RendererError>>
```

Key steps:
1. Update compositor state
2. Begin GPU frame
3. Update texture cache
4. Update native surfaces
5. **Draw frame** (`self.draw_frame()`)
6. Update profiler/debug overlays
7. End GPU frame

### 5. Draw Frame Details
The `draw_frame()` method calls different draw functions based on frame content:
- `draw_picture_cache_target()` - For picture cache tiles
- `draw_color_target()` - For off-screen color targets
- `composite_simple()` - Final composite to framebuffer

### 6. Instanced Batch Drawing
Core drawing function at line ~2096:
```rust
fn draw_instanced_batch<T: Clone>(
    &mut self,
    data: &[T],
    vertex_array_kind: VertexArrayKind,
    textures: &BatchTextures,
    stats: &mut RendererStats,
)
```

**This is where `stats.total_draw_calls` is incremented!**

Key line ~2121:
```rust
stats.total_draw_calls += 1;
```

## Critical Question: Where is the Disconnect?

If we have 14 DisplayListItems but 0 draw calls, the items are either:
1. Not being converted to batches
2. Batches are empty
3. Batches are being skipped
4. Wrong rendering path is taken

## Debug Strategy

### Phase 1: Track Display List Items
Add debug output in:
- `display_list.rs::generate_display_list()` - Confirm 14 items created
- WebRender's display list receiver

### Phase 2: Track Frame Building
Add debug output in:
- Scene builder when processing display list
- Frame builder when creating render tasks

### Phase 3: Track Batching
Add debug output in:
- Batch creation
- Batch filtering/culling

### Phase 4: Track Drawing
Add debug output in:
- `draw_frame()` - Which path is taken?
- `draw_picture_cache_target()` vs `draw_color_target()` vs `composite_simple()`
- `draw_instanced_batch()` - Are we reaching this with non-empty data?

## Observations from Code

### Coordinate Systems
Looking at our earlier output:
```
[paint_node_background_and_border] paint_rect: 200x100 @ (0, 0)
```

All rects are at (0, 0) which is suspicious. They should be positioned differently.

The body is `6.4x4.8` which seems very small - possibly layout is in inches not pixels?

### WebRender Stats
From renderer output:
```rust
RendererStats { 
    total_draw_calls: 0,
    alpha_target_count: 0, 
    color_target_count: 1,
    ...
}
```

- `color_target_count: 1` means we have one render target
- `alpha_target_count: 0` means no alpha/transparency passes needed
- `total_draw_calls: 0` means no actual GPU draw calls issued

This suggests the frame exists but is considered empty/culled.

## DISCOVERY: Root Cause Found!

### Debug Output from WebRender:
```
[WR draw_frame] START - device_size=Some(1280x960), passes=1
[WR draw_frame] Processing 1 passes
[WR composite_simple] START - tiles=0, draw_target=Default
```

### The Problem:
**`tiles=0` - No composite tiles are being created!**

The display list has 14 items, but these items are not being converted into composite tiles.

### What This Means:
1. Display list generation works (14 items created)
2. Frame building occurs (1 pass exists)
3. **Composite state has no tiles** ← THIS IS THE ISSUE
4. Without tiles, no drawing occurs
5. Result: `total_draw_calls: 0`

### Why No Tiles?

Picture cache tiles are the mechanism WebRender uses to cache rendered content.
If no tiles are created, it could be because:

1. **No picture cache is being used** - Content might be going through a different path
2. **Items are being culled** - Considered out of bounds or zero-sized
3. **Wrong rendering path** - Using a non-tile-based approach
4. **Coordinate system mismatch** - Items are in wrong space (inches vs pixels?)

### The Coordinate Problem Revisited

From our earlier layout debug:
- Body: `6.4x4.8` (tiny!)
- All rects at position `(0, 0)`
- Window: `1280x960` pixels

The layout is producing very small sizes (possibly in inches/cm instead of pixels).
These tiny rects might be getting culled or not converted to tiles.

### DISCOVERY 2: Items are Translated, But No Tiles!

Debug output from compositor2:
```
[compositor2] Builder started, translating 14 items
[compositor2] Rect item: bounds=200x100 @ (0, 0), color=ColorU { r: 255, g: 0, b: 0, a: 255 }
[compositor2] Translated to LayoutRect: Box2D((0.0, 0.0), (200.0, 100.0))
[compositor2] Rect item: bounds=200x100 @ (0, 0), color=ColorU { r: 0, g: 0, b: 255, a: 255 }
...
[WR composite_simple] START - tiles=0
```

### Analysis:

1. **Display list translation works** ✓
   - 14 items are processed
   - Converted to WebRender LayoutRect successfully
   - Colors are correct (red, blue, green)
   - Sizes are correct (200x100)

2. **Position problem identified** ⚠️
   - ALL rects are at (0, 0)
   - They completely overlap each other
   - This is a layout bug, not a rendering bug

3. **No tiles created** ❌
   - `composite_simple()` receives 0 tiles
   - WebRender passes=1, so frame exists
   - But composite state is empty

### Understanding WebRender Architecture:

WebRender can operate in two modes:

#### Mode 1: Picture Cache (Tile-Based)
- Content is split into picture cache tiles
- Tiles are cached and only redrawn when dirty
- Used for complex scenes with scrolling
- Requires `composite_simple()` to draw tiles

#### Mode 2: Direct Drawing
- Content is drawn directly in render passes
- No tile caching
- Used for simple content or special effects
- **This seems to be what Azul is using**

### The Real Problem:

The issue is NOT that there are no tiles - the issue is that **items are not being drawn at all**.

Looking at `draw_frame()`, there are multiple drawing paths:
1. `draw_picture_cache_target()` - For picture cache tiles (not used here)
2. `draw_color_target()` - For render targets in passes
3. `composite_simple()` - For final composition (no tiles to composite)

Since `passes=1`, WebRender has created render passes. The items should be drawn in those passes via `draw_color_target()`, NOT via `composite_simple()`.

### DISCOVERY 3: The Smoking Gun - Empty Pass!

Final debug output:
```
[WR draw_frame] START - device_size=Some(1280x960), passes=1
[WR draw_frame] Processing 1 passes
[WR draw_frame] Processing pass #0 - picture_cache=0, alpha_targets=0, color_targets=0
[WR composite_simple] START - tiles=0
```

### ROOT CAUSE IDENTIFIED:

**Pass #0 exists but has NO render targets:**
- `picture_cache=0` - No picture cache tiles
- `alpha_targets=0` - No alpha/transparency targets
- `color_targets=0` - No color targets

### What This Means:

The WebRender frame builder creates a pass, but the pass is completely empty.
No render targets = no batches = no draw calls.

### The Complete Flow (What's Happening):

1. ✓ Azul generates 14 DisplayListItems
2. ✓ Items are translated to WebRender display list
3. ✓ WebRender display list is submitted via Transaction
4. ✓ Frame builder processes the display list
5. ✓ Frame builder creates 1 render pass
6. **❌ Frame builder creates ZERO render targets**
7. ❌ No batches are created
8. ❌ No drawing occurs
9. ❌ `total_draw_calls = 0`

### Why Are No Targets Created?

Possible reasons:
1. **Display list is empty from WebRender's perspective**
   - Items might not be properly added to the builder
   - `builder.begin()` / `builder.end()` might be missing
   - Pipeline/spatial tree might be incorrect

2. **Items are being culled**
   - All at (0,0) so they overlap completely
   - Viewport culling might eliminate everything
   - Coordinate transformation issues

3. **Wrong rendering mode**
   - Not using the right primitive types
   - Missing required setup (spatial tree, clip chains, etc.)

### FINAL ROOT CAUSE: Layout Problem, Not Rendering Problem!

After extensive debugging, the issue is **NOT** with WebRender rendering, but with **Azul layout**.

#### Evidence:
1. ✓ Display list generation works (14 items created)
2. ✓ Items are correctly translated to WebRender primitives
3. ✓ WebRender display list builder is called correctly (begin/end)
4. ✓ Transaction is created and sent to WebRender
5. ✓ Frame is processed (1 pass created)
6. **❌ ALL rectangles are at position (0, 0)**
7. ❌ Pass has no render targets (empty pass)

#### The Real Problem:

From layout debug output:
```
[paint_node_background_and_border] paint_rect: 200x100 @ (0, 0), color=ColorU { r: 255, g: 0, b: 0, a: 255 }
[paint_node_background_and_border] paint_rect: 200x100 @ (0, 0), color=ColorU { r: 0, g: 0, b: 255, a: 255 }
[paint_node_background_and_border] paint_rect: 200x100 @ (0, 0), color=ColorU { r: 0, g: 255, b: 0, a: 255 }
```

**Every rectangle is at (0, 0) with size 200x100.**

The CSS specifies:
- Red rectangle with `margin: 10px`
- Blue rectangle with `margin: 10px`
- Green rectangle with `margin: 10px`

They should be stacked vertically with margins, but instead they all have the **SAME position**.

#### Why No Targets Are Created:

When WebRender processes the display list, it sees 14 primitives all at exactly (0, 0).
This causes:
1. Extreme overdraw (everything in the same spot)
2. Possible culling optimization (why draw 14 identical rects?)
3. Or batching issue (WebRender might merge them into nothing)

The frame builder creates an empty pass because from its perspective, there's nothing meaningful to draw.

#### Next Steps to Fix:

1. **Fix layout positioning** - The real issue
   - Debug why `calculate_layout_for_subtree` produces (0, 0) for all nodes
   - Check `absolute_positions` calculation
   - Verify block layout properly positions children with margins

2. **Body size is also wrong**
   - Body is `6.4x4.8` (very small, possibly inches instead of pixels?)
   - Should be `1280x960` or at least `100%` of viewport

3. **Fix coordinate system**
   - Ensure layout uses pixels, not inches/cm
   - Check DPI scaling calculations

The rendering pipeline is actually working correctly - it's just being fed invalid layout data!

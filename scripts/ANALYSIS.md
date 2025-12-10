# Azul White Window Debug Analysis

## Problem
Azul GUI shows a white/blank window while webrender-upstream examples display content correctly.

## What Works
- **webrender-upstream** `basic` example: Shows "ABC" text correctly
- Gemini AI verification returns "SUCCESS" for webrender-upstream

## What Fails
- **azul** `hello_world_window`: Shows blank white window
- Gemini AI verification returns "FAILURE" for azul
- WebRender stats show: `total_draw_calls: 0, picture_cache=0, tiles=0`

## Root Cause Found
The `BuiltDisplayList` created by `compositor2.rs` has **size_in_bytes()=0** when it reaches WebRender's scene builder.

### Evidence Chain
1. `compositor2.rs` creates display list with 4 items (PushStackingContext, Rect, HitTestArea, PopStackingContext)
2. `builder.push_rect()` is called with correct coordinates: `Box2D((0.0, 0.0), (200.0, 200.0))`
3. After `builder.end()`: `dl.size_in_bytes()=0` ❌
4. WebRender's `SceneBuilder::build_all()` receives empty display list
5. No primitives are added to tile cache → no tiles generated → nothing rendered

## Code Flow Analysis

### Display List Creation (compositor2.rs)
```
builder.begin()
  → push_simple_stacking_context()
  → push_rect() with red color at (0,0) 200x200
  → pop_stacking_context()
builder.end() → returns BuiltDisplayList with 0 bytes!
```

### Scene Building (scene_building.rs)
```
SceneBuilder::build()
  → build_all() - traversal finds 0 items because display list is empty
  → tile_cache_builder.build() - 0 secondary_slices
  → tile_cache_pictures is empty
  → No tiles created
```

### Rendering (renderer/mod.rs)
```
composite_simple() - tiles=0
  → Nothing to composite
  → White window
```

## Hypothesis
The `DisplayListBuilder` in azul's webrender fork is not correctly serializing items to its internal buffer. The `push_rect()` and other methods may not be writing data to `payload.items_data`.

## Files Investigated
- `/azul/dll/src/desktop/compositor2.rs` - Display list translation
- `/azul/webrender/core/src/scene_building.rs` - Scene building from display list
- `/azul/webrender/core/src/tile_cache.rs` - Tile cache construction
- `/azul/webrender/core/src/frame_builder.rs` - Frame building with composite state
- `/azul/webrender/api/src/display_list.rs` - DisplayListBuilder implementation

## Next Steps
1. Investigate `DisplayListBuilder::push_rect()` in `/azul/webrender/api/src/display_list.rs`
2. Check if items are being serialized to `payload.items_data`
3. Compare with webrender-upstream's `DisplayListBuilder` implementation
4. Verify `push_item()` internal method is correctly writing bytes

## Debug Logs Location
`/azul/scripts/debugrun/run_20251209_194319/`
- `azul_stderr.log` - Full debug output
- `azul_screenshot.png` - White window screenshot

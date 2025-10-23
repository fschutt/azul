# Stacking Context Translation Fix

## Problem

WebRender was receiving 14 display list items but producing `total_draw_calls: 0`. Investigation revealed:

1. ✅ Display list generation worked correctly
2. ✅ Item translation to WebRender formats worked correctly  
3. ❌ **WebRender pass had 0 render targets** (picture_cache=0, alpha_targets=0, color_targets=0)

Root cause: **Missing stacking context translation**

- Azul has internal `StackingContext` system (for CSS z-index)
- `DisplayListItem` enum had no `PushStackingContext`/`PopStackingContext` variants
- compositor2 never called WebRender's `push_stacking_context` API
- **WebRender requires explicit stacking contexts to create render targets**

## Solution

### 1. Added Stacking Context Variants to DisplayListItem

**File:** `layout/src/solver3/display_list.rs`

Added two new variants to the `DisplayListItem` enum:

```rust
pub enum DisplayListItem {
    // ... existing variants ...
    
    /// Pushes a new stacking context for proper z-index layering.
    PushStackingContext {
        /// The z-index for this stacking context (for debugging/validation)
        z_index: i32,
        /// The bounds of the stacking context root element
        bounds: LogicalRect,
    },
    /// Pops the current stacking context.
    PopStackingContext,
    
    // ... rest of variants ...
}
```

### 2. Added Builder Methods

**File:** `layout/src/solver3/display_list.rs`

Added convenience methods to `DisplayListBuilder`:

```rust
impl DisplayListBuilder {
    pub fn push_stacking_context(&mut self, z_index: i32, bounds: LogicalRect) {
        self.items.push(DisplayListItem::PushStackingContext {
            z_index,
            bounds,
        });
    }

    pub fn pop_stacking_context(&mut self) {
        self.items.push(DisplayListItem::PopStackingContext);
    }
}
```

### 3. Modified Display List Generation

**File:** `layout/src/solver3/display_list.rs`

Modified `generate_for_stacking_context` to emit Push/Pop items:

```rust
fn generate_for_stacking_context(
    &self,
    builder: &mut DisplayListBuilder,
    context: &StackingContext,
) -> Result<()> {
    let node = /* ... */;
    
    // Calculate stacking context bounds
    let node_pos = self.positioned_tree
        .absolute_positions
        .get(&context.node_index)
        .copied()
        .unwrap_or_default();
    let node_size = node.used_size.unwrap_or(LogicalSize { width: 0.0, height: 0.0 });
    let node_bounds = LogicalRect {
        origin: node_pos,
        size: node_size,
    };
    
    // PUSH STACKING CONTEXT
    builder.push_stacking_context(context.z_index, node_bounds);
    
    // Paint content (background, borders, children, etc.)
    // ...
    
    // POP STACKING CONTEXT
    builder.pop_stacking_context();
    
    Ok(())
}
```

**Key change:** Fixed field access from `node.layout.size` → `node.used_size`
(LayoutNode structure was refactored to use `used_size: Option<LogicalSize>`)

### 4. Implemented WebRender Translation

**File:** `dll/src/desktop/compositor2/mod.rs`

Added imports:

```rust
use webrender::api::{
    // ... existing imports ...
    units::LayoutTransform,
    TransformStyle, PropertyBinding, ReferenceFrameKind,
    SpatialTreeItemKey, PrimitiveFlags as WrPrimitiveFlags,
};
```

Added translation logic in `translate_displaylist_to_wr`:

```rust
match item {
    // ... existing cases ...
    
    DisplayListItem::PushStackingContext { z_index, bounds } => {
        eprintln!("[compositor2] PushStackingContext: z_index={}, bounds={:?}", z_index, bounds);
        
        // Create a spatial node (reference frame) for this stacking context
        let parent_spatial_id = *spatial_stack.last().unwrap();
        let spatial_id = builder.push_reference_frame(
            LayoutPoint::new(bounds.origin.x, bounds.origin.y),
            parent_spatial_id,
            TransformStyle::Flat,
            PropertyBinding::Value(LayoutTransform::identity()),
            ReferenceFrameKind::Transform {
                is_2d_scale_translation: true,
                should_snap: true,
                paired_with_perspective: false,
            },
            SpatialTreeItemKey::new(0, 0), // Default key
        );
        spatial_stack.push(spatial_id);

        // Push a simple stacking context
        let stacking_context_bounds = LayoutRect::from_origin_and_size(
            LayoutPoint::zero(), // Already positioned by reference frame
            LayoutSize::new(bounds.size.width, bounds.size.height),
        );
        builder.push_simple_stacking_context(
            stacking_context_bounds.min,
            spatial_id,
            WrPrimitiveFlags::IS_BACKFACE_VISIBLE,
        );
    }

    DisplayListItem::PopStackingContext => {
        eprintln!("[compositor2] PopStackingContext");
        builder.pop_stacking_context();
        
        // Pop the spatial node we created
        if spatial_stack.len() > 1 {
            spatial_stack.pop();
            builder.pop_reference_frame();
        }
    }
}
```

**Key implementation details:**
- Creates a WebRender reference frame for spatial positioning
- Pushes a simple stacking context within that frame
- Maintains a `spatial_stack` to track nested contexts
- Properly cleans up both stacking context and reference frame on Pop

### 5. Fixed CPU Renderer

**File:** `layout/src/cpurender/mod.rs`

Added no-op handlers (CPU renderer handles order via display list traversal):

```rust
match item {
    // ... existing cases ...
    
    DisplayListItem::PushStackingContext { .. } => {
        // For CPU rendering, stacking is handled by display list order
    }
    DisplayListItem::PopStackingContext => {
        // No action needed
    }
}
```

## Expected Outcome

After these changes:

1. ✅ Display list generation includes Push/PopStackingContext items
2. ✅ compositor2 translates these to WebRender API calls
3. ✅ WebRender creates proper reference frames and stacking contexts
4. ✅ **Render pass should have non-zero targets**
5. ✅ **draw_instanced_batch should be called**
6. ✅ **total_draw_calls > 0**

## Testing

Run the test:
```bash
cd azul/dll
cargo run --bin test_display_list
```

Look for these debug messages:
```
[compositor2] PushStackingContext: z_index=0, bounds=LogicalRect { ... }
[WR draw_frame] Processing pass #0 - color_targets=1 (not 0!)
[WR draw_instanced_batch] Called with X instances
[WR draw_frame] Rendered frame - total_draw_calls: 1 (not 0!)
```

## Related Files Changed

- `layout/src/solver3/display_list.rs` - Added enum variants, builder methods, generation logic
- `dll/src/desktop/compositor2/mod.rs` - Added WebRender translation
- `layout/src/cpurender/mod.rs` - Added no-op handlers
- All files compile successfully ✅

## Next Steps

1. **Verify rendering works** - items should now be visible on screen
2. **Fix positioning bug** - All rectangles are still at (0,0) - separate issue
3. **Test z-index ordering** - Verify stacking contexts properly layer content
4. **Add transforms** - Could extend stacking contexts with CSS transforms

## Architecture Notes

**Why WebRender needs stacking contexts:**

WebRender's frame building process:
1. Display list → Scene building → Render tasks
2. **Render tasks** require stacking contexts to know:
   - What spatial coordinate system to use
   - How to batch primitives
   - What render targets to create
3. Without stacking contexts: No render targets → No batches → No draw calls

**Azul's two-level stacking context system:**

1. **Internal `StackingContext` struct** - Used for z-index ordering during display list generation
2. **`DisplayListItem` Push/Pop** - Serialized form that gets translated to WebRender

This separation allows:
- Clean display list generation (z-index sorted tree)
- Portable serialization (display lists can be sent over IPC)
- Renderer-agnostic abstraction (works with CPU/GPU renderers)

# Scrollbar GPU Integration Plan

## Overview
Integrate scrollbar opacity fading with the GPU value system to enable minimal GPU updates without re-sending the display list.

## Current Architecture

### GpuValueCache (core/src/gpu.rs)
- Manages CSS opacity and transform values
- Synchronized with StyledDom per frame
- Generates delta events (Added/Changed/Removed)
- Keys: `OpacityKey`, `TransformKey`
- Values sent to WebRender as dynamic properties

### ScrollManager (layout/src/scroll.rs)
- Manages scroll state per node
- Calculates scrollbar opacity based on activity timestamp
- Has `get_scrollbar_opacity(dom_id, node_id, now, fade_delay, fade_duration)` method
- Tracks scroll activity and animations

## Problem
Currently:
1. Scrollbar opacity is calculated in ScrollManager
2. Not integrated with GPU value system
3. Would require re-sending display list on every frame during fade

## Solution: Scrollbar Opacity Keys

### Phase 1: Extend GpuValueCache for Scrollbars

**Add to `core/src/gpu.rs`:**

```rust
#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
pub struct GpuValueCache {
    pub transform_keys: BTreeMap<NodeId, TransformKey>,
    pub current_transform_values: BTreeMap<NodeId, ComputedTransform3D>,
    pub opacity_keys: BTreeMap<NodeId, OpacityKey>,
    pub current_opacity_values: BTreeMap<NodeId, f32>,
    
    // NEW: Scrollbar opacity keys (separate from CSS opacity)
    pub scrollbar_v_opacity_keys: BTreeMap<(DomId, NodeId), OpacityKey>,
    pub scrollbar_h_opacity_keys: BTreeMap<(DomId, NodeId), OpacityKey>,
    pub scrollbar_v_opacity_values: BTreeMap<(DomId, NodeId), f32>,
    pub scrollbar_h_opacity_values: BTreeMap<(DomId, NodeId), f32>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum GpuScrollbarOpacityEvent {
    VerticalAdded(DomId, NodeId, OpacityKey, f32),
    VerticalChanged(DomId, NodeId, OpacityKey, f32, f32),
    VerticalRemoved(DomId, NodeId, OpacityKey),
    HorizontalAdded(DomId, NodeId, OpacityKey, f32),
    HorizontalChanged(DomId, NodeId, OpacityKey, f32, f32),
    HorizontalRemoved(DomId, NodeId, OpacityKey),
}

pub struct GpuEventChanges {
    pub transform_key_changes: Vec<GpuTransformKeyEvent>,
    pub opacity_key_changes: Vec<GpuOpacityKeyEvent>,
    pub scrollbar_opacity_changes: Vec<GpuScrollbarOpacityEvent>, // NEW
}
```

### Phase 2: Synchronize Scrollbar Opacity in Layout

**Add method to `GpuValueCache`:**

```rust
impl GpuValueCache {
    /// Synchronize scrollbar opacity values from ScrollManager
    pub fn synchronize_scrollbar_opacity(
        &mut self,
        scroll_manager: &ScrollManager,
        layout_tree: &LayoutTree,
        now: Instant,
        fade_delay: Duration,
        fade_duration: Duration,
    ) -> Vec<GpuScrollbarOpacityEvent> {
        let mut events = Vec::new();
        
        // Iterate over all nodes with scrollbars
        for node_idx in 0..layout_tree.nodes.len() {
            let node = &layout_tree.nodes[node_idx];
            
            // Check if node needs scrollbars
            let scrollbar_info = match &node.scrollbar_info {
                Some(info) => info,
                None => continue,
            };
            
            let (dom_id, node_id) = match node.dom_node_id {
                Some(id) => id,
                None => continue, // anonymous boxes don't have scrollbars
            };
            
            // Calculate current opacity from ScrollManager
            let vertical_opacity = if scrollbar_info.needs_vertical {
                scroll_manager.get_scrollbar_opacity(
                    dom_id, 
                    node_id, 
                    now.clone(), 
                    fade_delay, 
                    fade_duration
                )
            } else {
                0.0
            };
            
            let horizontal_opacity = if scrollbar_info.needs_horizontal {
                scroll_manager.get_scrollbar_opacity(
                    dom_id, 
                    node_id, 
                    now.clone(), 
                    fade_delay, 
                    fade_duration
                )
            } else {
                0.0
            };
            
            // Handle vertical scrollbar
            if scrollbar_info.needs_vertical {
                let key = (dom_id, node_id);
                let existing = self.scrollbar_v_opacity_values.get(&key);
                
                match existing {
                    None if vertical_opacity > 0.0 => {
                        let opacity_key = OpacityKey::unique();
                        self.scrollbar_v_opacity_keys.insert(key, opacity_key);
                        self.scrollbar_v_opacity_values.insert(key, vertical_opacity);
                        events.push(GpuScrollbarOpacityEvent::VerticalAdded(
                            dom_id, node_id, opacity_key, vertical_opacity
                        ));
                    }
                    Some(&old_opacity) if (old_opacity - vertical_opacity).abs() > 0.001 => {
                        let opacity_key = self.scrollbar_v_opacity_keys[&key];
                        self.scrollbar_v_opacity_values.insert(key, vertical_opacity);
                        events.push(GpuScrollbarOpacityEvent::VerticalChanged(
                            dom_id, node_id, opacity_key, old_opacity, vertical_opacity
                        ));
                    }
                    _ => {}
                }
            } else {
                // Remove if scrollbar no longer needed
                let key = (dom_id, node_id);
                if let Some(opacity_key) = self.scrollbar_v_opacity_keys.remove(&key) {
                    self.scrollbar_v_opacity_values.remove(&key);
                    events.push(GpuScrollbarOpacityEvent::VerticalRemoved(
                        dom_id, node_id, opacity_key
                    ));
                }
            }
            
            // Handle horizontal scrollbar (same logic)
            if scrollbar_info.needs_horizontal {
                let key = (dom_id, node_id);
                let existing = self.scrollbar_h_opacity_values.get(&key);
                
                match existing {
                    None if horizontal_opacity > 0.0 => {
                        let opacity_key = OpacityKey::unique();
                        self.scrollbar_h_opacity_keys.insert(key, opacity_key);
                        self.scrollbar_h_opacity_values.insert(key, horizontal_opacity);
                        events.push(GpuScrollbarOpacityEvent::HorizontalAdded(
                            dom_id, node_id, opacity_key, horizontal_opacity
                        ));
                    }
                    Some(&old_opacity) if (old_opacity - horizontal_opacity).abs() > 0.001 => {
                        let opacity_key = self.scrollbar_h_opacity_keys[&key];
                        self.scrollbar_h_opacity_values.insert(key, horizontal_opacity);
                        events.push(GpuScrollbarOpacityEvent::HorizontalChanged(
                            dom_id, node_id, opacity_key, old_opacity, horizontal_opacity
                        ));
                    }
                    _ => {}
                }
            } else {
                // Remove if scrollbar no longer needed
                let key = (dom_id, node_id);
                if let Some(opacity_key) = self.scrollbar_h_opacity_keys.remove(&key) {
                    self.scrollbar_h_opacity_values.remove(&key);
                    events.push(GpuScrollbarOpacityEvent::HorizontalRemoved(
                        dom_id, node_id, opacity_key
                    ));
                }
            }
        }
        
        events
    }
}
```

### Phase 3: Update Display List Generation

**Modify `solver3/display_list.rs`:**

Instead of:
```rust
builder.push_scrollbar(
    sb_bounds,
    ColorU::new(192, 192, 192, 255), // Static opacity
    ScrollbarOrientation::Vertical,
);
```

Use:
```rust
// Get opacity key from gpu_value_cache
let opacity_key = gpu_value_cache
    .scrollbar_v_opacity_keys
    .get(&(dom_id, node_id))
    .copied();

builder.push_scrollbar_with_opacity_key(
    sb_bounds,
    ColorU::new(192, 192, 192, 255), // Base color
    ScrollbarOrientation::Vertical,
    opacity_key, // Dynamic opacity from GPU
);
```

### Phase 4: Update WebRender Translation

**Modify `dll/src/desktop/wr_translate.rs`:**

```rust
pub(crate) fn synchronize_gpu_values(
    layout_results: &[LayoutResult],
    dpi: &DpiScaleFactor,
    txn: &mut WrTransaction,
) {
    // ... existing transform code ...
    
    // ... existing opacity code ...
    
    // NEW: Scrollbar opacity
    let scrollbar_opacities = layout_results
        .iter()
        .flat_map(|lr| {
            let mut scrollbar_floats = Vec::new();
            
            // Vertical scrollbars
            scrollbar_floats.extend(
                lr.gpu_value_cache
                    .scrollbar_v_opacity_keys
                    .iter()
                    .filter_map(|((dom_id, node_id), key)| {
                        let value = lr.gpu_value_cache
                            .scrollbar_v_opacity_values
                            .get(&(*dom_id, *node_id))?;
                        Some((key, *value))
                    })
            );
            
            // Horizontal scrollbars
            scrollbar_floats.extend(
                lr.gpu_value_cache
                    .scrollbar_h_opacity_keys
                    .iter()
                    .filter_map(|((dom_id, node_id), key)| {
                        let value = lr.gpu_value_cache
                            .scrollbar_h_opacity_values
                            .get(&(*dom_id, *node_id))?;
                        Some((key, *value))
                    })
            );
            
            scrollbar_floats.into_iter()
        })
        .map(|(k, v)| WrPropertyValue {
            key: WrPropertyBindingKey::new(k.id as u64),
            value: v,
        })
        .collect::<Vec<_>>();
    
    // Merge with existing floats
    floats.extend(scrollbar_opacities);
    
    txn.update_dynamic_properties(WrDynamicProperties {
        transforms,
        floats, // Now includes scrollbar opacity
        colors: Vec::new(),
    });
}
```

## Benefits

1. **Minimal GPU Updates**: Only opacity values are updated, not the entire display list
2. **Smooth Fading**: Scrollbar opacity can fade continuously without layout recalculation
3. **Consistent Architecture**: Uses same system as CSS transforms/opacity
4. **Efficient**: Only changed opacity keys are sent to GPU

## Call Sites

### Layout Engine (layout/src/solver3/mod.rs or layout/src/window.rs)

```rust
// After layout, before returning DisplayList
let scrollbar_opacity_changes = gpu_value_cache.synchronize_scrollbar_opacity(
    &scroll_manager,
    &layout_tree,
    now,
    Duration::from_millis(500), // fade_delay
    Duration::from_millis(200), // fade_duration
);

// Merge into overall GPU changes
gpu_changes.scrollbar_opacity_changes = scrollbar_opacity_changes;
```

### Application Event Loop

```rust
// On every frame or when scroll activity occurs
let gpu_changes = layout_result.gpu_value_cache.synchronize_scrollbar_opacity(...);
synchronize_gpu_values(&layout_results, dpi, &mut transaction);
```

## Configuration

Add to window/app config:
```rust
pub struct ScrollbarConfig {
    pub fade_delay: Duration,      // Default: 500ms
    pub fade_duration: Duration,   // Default: 200ms
    pub base_color: ColorU,        // Default: rgb(192, 192, 192)
}
```

## Testing

1. Test scrollbar appears at full opacity on scroll
2. Test fade delay (visible for 500ms after scroll ends)
3. Test fade animation (smooth transition over 200ms)
4. Test multiple scrollable areas with independent fading
5. Test scrollbar disappears when content shrinks (no scrolling needed)
6. Verify no display list updates during fade (only GPU property updates)

## Implementation Order

1. ✅ Phase 2.2: Scrollbar reflow loop (DONE)
2. 🔄 Phase 2.3a: Extend `GpuValueCache` for scrollbar opacity
3. 🔄 Phase 2.3b: Add `synchronize_scrollbar_opacity()` method
4. 🔄 Phase 2.3c: Update display list to use opacity keys
5. 🔄 Phase 2.3d: Update WebRender translation layer
6. 🔄 Phase 2.3e: Wire up in main layout loop
7. ⏳ Phase 3: Comprehensive testing

## Notes

- Scrollbar opacity is independent of CSS opacity (different keys)
- Each scrollbar (vertical/horizontal) has its own opacity key
- Opacity is calculated from `ScrollManager` activity tracking
- Keys are created/destroyed based on `scrollbar_info.needs_vertical/horizontal`

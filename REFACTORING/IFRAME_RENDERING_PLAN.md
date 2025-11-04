# IFrame Rendering Implementation - Architecture and Plan

## Current Status: IFrames Partially Working

IFrame callback logic and state management is COMPLETE. What's missing is the WebRender integration to actually render nested display lists.

## What Works ✅

### 1. IFrame Manager (`layout/src/managers/iframe.rs`)
- ✅ PipelineId generation (1 per IFrame)
- ✅ Nested DomId allocation
- ✅ Re-invocation logic (all 5 rules):
  1. InitialRender
  2. BoundsExpanded (window resize)
  3. EdgeScrolled (infinite scroll)
  4. Parent DOM recreated
  5. Programmatic scroll beyond rendered content
- ✅ State tracking (scroll_size, virtual_scroll_size, invoked_for_current_expansion, etc.)
- ✅ Edge detection with 200px threshold

### 2. IFrame Callbacks (`layout/src/window.rs`)
- ✅ `invoke_iframe_callback_impl()` - Calls user callbacks
- ✅ Returns `IFrameCallbackReturn` with child StyledDom
- ✅ Recursive layout of child DOMs
- ✅ Child layout results stored in `layout_window.layout_results[child_dom_id]`

### 3. Display List Items
- ✅ `DisplayListItem::IFrame { child_dom_id, bounds, clip_rect }`
- ✅ Included in parent display list at correct position
- ✅ `child_dom_id` references the nested DOM's layout result

## What's Missing ✗

### 1. **Recursive Display List Translation**

**Problem**: `translate_displaylist_to_wr()` processes one display list at a time and doesn't handle IFrame children.

**Current Code** (dll/src/desktop/compositor2.rs):
```rust
DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
    // TODO: Implement iframe embedding (nested pipelines)
    eprintln!("[compositor2] WARNING: IFrame rendering not yet fully implemented");
}
```

**Required Architecture**:

```rust
pub fn translate_displaylist_to_wr(
    display_list: &DisplayList,
    pipeline_id: PipelineId,
    viewport_size: DeviceIntSize,
    renderer_resources: &RendererResources,
    dpi: DpiScaleFactor,
    wr_resources: Vec<WrResourceUpdate>,
    // NEW PARAMETERS:
    layout_results: &BTreeMap<DomId, LayoutResult>,  // Access to child DOMs
) -> Result<Vec<(PipelineId, WrBuiltDisplayList)>, String> {  // Return ALL display lists
    
    let mut all_display_lists = Vec::new();
    let mut builder = WrDisplayListBuilder::new(pipeline_id);
    builder.begin();
    
    for item in &display_list.items {
        match item {
            DisplayListItem::IFrame { child_dom_id, bounds, clip_rect } => {
                // Get child layout result
                let child_layout = layout_results.get(child_dom_id)
                    .ok_or("Child DOM not found")?;
                
                // Calculate child pipeline ID (child_dom_id maps to pipeline)
                let child_pipeline_id = PipelineId(child_dom_id.inner as u32, pipeline_id.1);
                
                // RECURSIVE CALL - Build child display list
                let child_display_lists = translate_displaylist_to_wr(
                    &child_layout.display_list,
                    child_pipeline_id,
                    viewport_size,
                    renderer_resources,
                    dpi,
                    vec![],  // Resources already collected at top level
                    layout_results,  // Pass down for nested iframes
                )?;
                
                // Collect child and its descendants
                all_display_lists.extend(child_display_lists);
                
                // Push iframe reference in parent
                let wr_bounds = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(bounds.origin.x, bounds.origin.y),
                    LayoutSize::new(bounds.size.width, bounds.size.height),
                );
                let wr_clip_rect = LayoutRect::from_origin_and_size(
                    LayoutPoint::new(clip_rect.origin.x, clip_rect.origin.y),
                    LayoutSize::new(clip_rect.size.width, clip_rect.size.height),
                );
                
                let space_and_clip = SpaceAndClipInfo {
                    spatial_id: *spatial_stack.last().unwrap(),
                    clip_chain_id: *clip_stack.last().unwrap(),
                };
                
                builder.push_iframe(
                    wr_bounds,
                    wr_clip_rect,
                    &space_and_clip,
                    child_pipeline_id,
                    false,  // ignore_missing_pipeline
                );
            }
            // ... other items
        }
    }
    
    let (_, dl) = builder.end();
    
    // Add this display list first (depth-first order)
    all_display_lists.push((pipeline_id, dl));
    
    Ok(all_display_lists)
}
```

### 2. **Update generate_frame() to Handle Multiple Display Lists**

**Current Code** (dll/src/desktop/wr_translate2.rs):
```rust
for (dom_id, layout_result) in &layout_window.layout_results {
    let pipeline_id = wr_translate_pipeline_id(PipelineId(dom_id.inner as u32, ...));
    
    match compositor2::translate_displaylist_to_wr(...) {
        Ok((_, built_display_list)) => {
            txn.set_display_list(epoch, (pipeline_id, built_display_list));
        }
    }
}
```

**Required Changes**:
```rust
// Only process ROOT DOMs (DomId::ROOT_ID), not nested iframe DOMs
// Nested DOMs will be processed recursively by translate_displaylist_to_wr

let root_pipeline_id = wr_translate_pipeline_id(PipelineId(0, document_id.id));

if let Some(root_layout) = layout_window.layout_results.get(&DomId::ROOT_ID) {
    match compositor2::translate_displaylist_to_wr(
        &root_layout.display_list,
        root_pipeline_id,
        viewport_size,
        &layout_window.renderer_resources,
        dpi,
        vec![],
        &layout_window.layout_results,  // NEW: Pass all layout results
    ) {
        Ok(all_display_lists) => {  // NEW: Returns Vec instead of single DL
            // Add ALL display lists (parent + all nested iframes)
            for (pipeline_id, built_display_list) in all_display_lists {
                eprintln!(
                    "[generate_frame] Adding display list for pipeline {:?}",
                    pipeline_id
                );
                txn.set_display_list(epoch, (pipeline_id, built_display_list));
            }
        }
    }
}
```

### 3. **PipelineId = DomId Mapping**

**Current**: Each DomId gets its own PipelineId via IFrameManager

**WebRender Requirement**: Each iframe needs a unique PipelineId to reference its display list

**Mapping**:
```rust
// In IFrameManager
pub fn get_or_create_pipeline_id(&mut self, dom_id: DomId, node_id: NodeId) -> PipelineId {
    *self.pipeline_ids
        .entry((dom_id, node_id))
        .or_insert_with(|| {
            PipelineId(
                self.get_or_create_nested_dom_id(dom_id, node_id).inner as u32,
                0  // namespace - same for all iframes in this document
            )
        })
}
```

**Usage**:
```rust
// When building child iframe reference
let child_pipeline_id = layout_window.iframe_manager
    .get_or_create_pipeline_id(parent_dom_id, node_id);
```

### 4. **Stack Management in Recursive Builder**

**Problem**: `spatial_stack` and `clip_stack` need to be passed through recursive calls or managed differently.

**Solution 1**: Pass stacks as parameters (messy)
```rust
fn translate_displaylist_to_wr(
    // ...
    spatial_stack: Vec<SpatialId>,
    clip_stack: Vec<ClipChainId>,
) -> Result<...>
```

**Solution 2**: Each display list starts with fresh stacks (cleaner)
```rust
// Each iframe gets its own coordinate space - start fresh
let mut spatial_stack = vec![builder.root_spatial_id()];
let mut clip_stack = vec![builder.root_clip_chain_id()];
```

WebRender handles coordinate transforms between pipelines automatically.

## Implementation Phases

### Phase 1: Make translate_displaylist_to_wr Recursive (2-3 hours)

1. Change return type to `Vec<(PipelineId, BuiltDisplayList)>`
2. Add `layout_results` parameter
3. Implement recursive call for `DisplayListItem::IFrame`
4. Use `builder.push_iframe()` to reference child pipeline
5. Collect all display lists in depth-first order

### Phase 2: Update generate_frame() (1 hour)

1. Only call `translate_displaylist_to_wr` for root DOM
2. Iterate over returned Vec and call `txn.set_display_list()` for each
3. Update error handling

### Phase 3: Testing (1-2 hours)

1. Simple iframe test (static content)
2. Nested iframe test (iframe within iframe)
3. Infinite scroll test (edge-triggered callback)
4. Window resize test (bounds expansion)

### Phase 4: Advanced Features (Future)

- IFrame transforms/scaling
- IFrame opacity/filters
- Cross-iframe hit testing
- IFrame clip paths

## Testing Strategy

### Unit Tests

Location: `layout/src/managers/iframe.rs` and `layout/src/solver3/tests.rs`

Tests already exist (though some are commented out):
- ✅ `test_iframe_manager_initial_dom_id_creation`
- ✅ `test_iframe_manager_multiple_iframes`
- ✅ `test_iframe_manager_check_reinvoke_initial_render`
- ✅ `test_iframe_manager_no_reinvoke_same_bounds`
- ✅ `test_iframe_manager_reinvoke_on_bounds_expansion`
- ✅ `test_iframe_manager_no_reinvoke_on_bounds_shrink`
- ✅ `test_iframe_manager_update_scroll_info`
- ✅ `test_iframe_manager_nested_iframes`

**Action**: Ensure all tests pass (currently disabled in some refactors)

### Integration Tests

Create `dll/examples/test_iframe.rs`:

1. **Basic IFrame**:
   - Parent renders "Parent Content"
   - IFrame renders "Child Content" in red box
   - Verify both visible

2. **Nested IFrame**:
   - Parent → IFrame1 → IFrame2
   - Each with different background color
   - Verify all three layers visible

3. **Infinite Scroll**:
   - IFrame with 1000 virtual rows
   - Initially renders rows 0-20
   - Scroll to bottom
   - Callback invoked, renders rows 980-1000
   - Verify smooth scrolling

4. **Window Resize**:
   - IFrame initially 300x300
   - Resize window to 600x600
   - Callback invoked with larger bounds
   - IFrame expands content

## Performance Considerations

- **Caching**: Layout results are cached, so iframes don't re-layout unless necessary
- **Lazy Loading**: Edge-triggered callbacks enable infinite virtual content
- **GPU Offloading**: WebRender handles nested pipeline compositing on GPU

## Related Files

- `layout/src/managers/iframe.rs` - IFrame state management (COMPLETE)
- `layout/src/window.rs` - IFrame callbacks (COMPLETE)
- `dll/src/desktop/compositor2.rs` - Display list translation (NEEDS RECURSION)
- `dll/src/desktop/wr_translate2.rs` - Resource management (NEEDS UPDATE)
- `core/src/callbacks.rs` - IFrame callback types (COMPLETE)

## Estimated Total Effort

- Phase 1 (Recursion): 2-3 hours
- Phase 2 (generate_frame): 1 hour  
- Phase 3 (Testing): 1-2 hours
- **Total: 4-6 hours**

Much less than initially estimated because the hard parts (state management, callbacks) are done!

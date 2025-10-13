# Callback Architecture Refactoring Plan v2

**Updated Architecture**: LayoutWindow becomes the central high-level object in azul-layout that manages all callback state (timers, threads, hit-tests, GPU cache). All LayoutResult code is migrated and organized into azul-layout modules.

## Current Situation

### Problem 1: LayoutResult No Longer Exists
- **azul-core** contains `CallbackInfo` struct that references `*const LayoutResult`
- `LayoutResult` was the old layout representation (from solver1/solver2)
- **azul-layout** now uses `LayoutWindow` with `solver3` - completely different architecture
- Compilation fails in azul-core due to missing `LayoutResult` type

### Problem 2: Wrong Dependency Direction
- **azul-core** (low-level) tries to reference **azul-layout** (high-level) types
- Callbacks need access to layout information (node sizes, positions, hierarchy)
- Current architecture: `core/callbacks.rs` would need to import from `layout/window.rs`
- This creates circular dependency: `layout` depends on `core`, but callbacks need `layout`

### Problem 3: Resource GC is Monolithic
- `RendererResources::do_gc()` takes `&[LayoutResult]` and scans for used resources
- Not split into separate concerns:
  - Scanning used resources (layout concern)
  - Comparing previous vs current frame (GC logic)
  - Generating resource deletion messages (renderer concern)
- Cannot handle multi-window scenarios efficiently

## Files Affected

### azul-core
```
core/src/callbacks.rs      - CallbackInfo, CallbackType, all callback definitions
core/src/resources.rs       - RendererResources::do_gc() with LayoutResult references
core/src/window.rs          - FullHitTest, hit test logic with LayoutResult
core/src/window_state.rs    - Event handling with LayoutResult
core/src/old_layout_result.rs - Legacy LayoutResult struct (should be removed)
```

### azul-layout  
```
layout/src/window.rs        - LayoutWindow (new architecture)
layout/src/solver3/         - New layout engine
```

## Proposed Solution v2: LayoutWindow-Centric Architecture

### Core Principle
**LayoutWindow becomes the single source of truth for all layout and callback state.**

Instead of spreading state across multiple LayoutResult objects, everything lives in LayoutWindow:
- Layout results (per-DOM)
- Timers and threads
- GPU value cache (transforms, opacity)
- Hit-test results
- Scroll states
- Selection states
- IFrame states

### Phase 1: Extend LayoutWindow with Callback State

```rust
// layout/src/window.rs
pub struct LayoutWindow {
    // Existing fields
    layout_cache: Solver3LayoutCache,
    text_cache: TextLayoutCache,
    font_manager: FontManager<PathLoader>,
    
    // Per-DOM layout results
    layout_results: BTreeMap<DomId, DomLayoutResult>,
    
    // NEW: Callback state
    timers: BTreeMap<TimerId, Timer>,
    threads: BTreeMap<ThreadId, Thread>,
    gpu_value_cache: GpuValueCache,
    
    // Per-DOM scroll/selection state
    scroll_states: BTreeMap<(DomId, NodeId), ScrollPosition>,
    selections: BTreeMap<DomId, SelectionState>,
    
    // IFrame tracking
    iframe_states: BTreeMap<(DomId, NodeId), IFrameState>,
    next_dom_id: u64,
}

impl LayoutWindow {
    // Timer management
    pub fn add_timer(&mut self, timer: Timer) -> TimerId { }
    pub fn remove_timer(&mut self, timer_id: TimerId) -> Option<Timer> { }
    pub fn get_timer(&self, timer_id: TimerId) -> Option<&Timer> { }
    pub fn tick_timers(&mut self, current_time: Instant) -> Vec<TimerId> { }
    
    // Thread management  
    pub fn add_thread(&mut self, thread: Thread) -> ThreadId { }
    pub fn remove_thread(&mut self, thread_id: ThreadId) -> Option<Thread> { }
    pub fn get_thread(&self, thread_id: ThreadId) -> Option<&Thread> { }
    
    // GPU cache access
    pub fn get_gpu_value_cache(&self) -> &GpuValueCache { }
    pub fn get_gpu_value_cache_mut(&mut self) -> &mut GpuValueCache { }
    pub fn update_gpu_cache_for_dom(&mut self, dom_id: DomId) { }
    
    // Hit-test computation
    pub fn compute_hit_test(
        &self,
        cursor_position: LogicalPosition,
    ) -> FullHitTest { }
    
    pub fn compute_cursor_type_hit_test(
        &self,
        hit_test: &FullHitTest,
    ) -> CursorTypeHitTest { }
}
```

### Phase 2: Create Organized Modules in azul-layout

Instead of monolithic files, organize functionality:

```
layout/src/
├── window.rs           # LayoutWindow + core state management
├── callbacks.rs        # CallbackInfo using LayoutWindow
├── hit_test.rs         # Hit-test computation (moved from core/window.rs)
├── timers.rs           # Timer management logic
├── threads.rs          # Thread management logic  
├── gpu_cache.rs        # GPU value cache management
└── solver3/
    ├── mod.rs
    ├── display_list.rs
    └── tests.rs
```

**Migration**:
- `core/window.rs` hit-test code → `layout/hit_test.rs`
- Timer/thread logic → `layout/timers.rs`, `layout/threads.rs`
- GPU cache logic → `layout/gpu_cache.rs`

### Phase 3: Update CallbackInfo to Use LayoutWindow

```rust
// layout/src/callbacks.rs
pub struct CallbackInfo {
    layout_window: *mut LayoutWindow,  // Mutable access for timer/thread changes
    renderer_resources: *const RendererResources,
    // ... other fields
}

impl CallbackInfo {
    // Layout queries (delegate to LayoutWindow)
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        unsafe { (*self.layout_window).get_node_size(node_id) }
    }
    
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        unsafe { (*self.layout_window).get_node_position(node_id) }
    }
    
    // Timer management (delegate to LayoutWindow)
    pub fn start_timer(&mut self, timer: Timer) -> TimerId {
        unsafe { (*self.layout_window).add_timer(timer) }
    }
    
    pub fn stop_timer(&mut self, timer_id: TimerId) -> bool {
        unsafe { (*self.layout_window).remove_timer(timer_id).is_some() }
    }
    
    // Thread management (delegate to LayoutWindow)
    pub fn start_thread(&mut self, ...) -> Option<ThreadId> {
        unsafe { 
            Some((*self.layout_window).add_thread(thread))
        }
    }
    
    // GPU cache access
    pub fn get_gpu_transform(&self, node_id: DomNodeId) -> Option<Transform> {
        unsafe { 
            (*self.layout_window)
                .get_gpu_value_cache()
                .get_transform(node_id)
        }
    }
}
```

### Phase 1: Move Callbacks to azul-layout

**Rationale**: Callbacks need access to LayoutWindow, which lives in azul-layout.

**Changes**:
1. Create `layout/src/callbacks.rs`
2. Move from `core/callbacks.rs` to `layout/callbacks.rs`:
   - `CallbackInfo` struct
   - `CallbackType` type alias
   - `Callback` struct
   - `IFrameCallback` and `IFrameCallbackInfo`
   - `RenderImageCallback` and `RenderImageCallbackInfo`
   - All callback invocation logic
3. Keep in `core/callbacks.rs` (C-ABI types):
   - `RefAny`, `Ref`, `RefMut`, `RefCount`
   - `Update` enum
   - `DocumentId`, `PipelineId`
   - `DomNodeId`
   - `ScrollPosition`
   - `HitTestItem`, `ScrollHitTestItem`
   - Callback macros

### Phase 4: Testing Strategy - Unit Tests for LayoutWindow

**Critical**: Each new capability gets comprehensive unit tests.

```rust
// layout/src/window.rs or layout/src/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_timer_management() {
        let mut window = LayoutWindow::new(/* ... */);
        
        // Add timer
        let timer = Timer::new(/* ... */);
        let timer_id = window.add_timer(timer);
        
        // Verify timer exists
        assert!(window.get_timer(timer_id).is_some());
        
        // Remove timer
        assert!(window.remove_timer(timer_id).is_some());
        assert!(window.get_timer(timer_id).is_none());
    }
    
    #[test]
    fn test_timer_ticking() {
        let mut window = LayoutWindow::new(/* ... */);
        
        // Add timer that fires after 100ms
        let timer = Timer::with_delay(Duration::from_millis(100));
        let timer_id = window.add_timer(timer);
        
        // Tick at t=0
        let fired = window.tick_timers(Instant::now());
        assert_eq!(fired.len(), 0);
        
        // Tick at t=150ms
        let fired = window.tick_timers(Instant::now() + Duration::from_millis(150));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0], timer_id);
    }
    
    #[test]
    fn test_thread_management() {
        let mut window = LayoutWindow::new(/* ... */);
        
        // Add thread
        let thread = Thread::new(/* ... */);
        let thread_id = window.add_thread(thread);
        
        // Verify thread exists
        assert!(window.get_thread(thread_id).is_some());
        
        // Remove thread
        assert!(window.remove_thread(thread_id).is_some());
    }
    
    #[test]
    fn test_hit_test_computation() {
        let mut window = LayoutWindow::new(/* ... */);
        
        // Layout a simple DOM
        let styled_dom = create_test_dom_with_button();
        window.layout_and_generate_display_list(styled_dom, /* ... */);
        
        // Hit-test at button position
        let cursor_pos = LogicalPosition::new(50.0, 50.0);
        let hit_test = window.compute_hit_test(cursor_pos);
        
        // Verify we hit the button
        assert!(hit_test.hovered_nodes.contains_key(&DomId::ROOT_ID));
    }
    
    #[test]
    fn test_iframe_callback_invocation() {
        let mut window = LayoutWindow::new(/* ... */);
        
        // Create DOM with IFrame
        let dom_with_iframe = create_test_dom_with_iframe();
        
        // Track callback invocations
        let callback_tracker = Arc::new(Mutex::new(IFrameCallbackTracker::new()));
        
        // Layout should invoke IFrame callback
        window.layout_and_generate_display_list(dom_with_iframe, /* ... */);
        
        // Verify callback was invoked
        let tracker = callback_tracker.lock().unwrap();
        assert_eq!(tracker.invocation_count, 1);
    }
    
    #[test]
    fn test_image_callback_on_resize() {
        let mut window = LayoutWindow::new(/* ... */);
        
        // Create DOM with image callback
        let dom_with_image = create_test_dom_with_image_callback();
        
        // First layout at 100x100
        window.layout_and_generate_display_list(
            dom_with_image.clone(),
            LayoutRect::new(0, 0, 100, 100),
        );
        
        // Second layout at 200x200 - should invoke callback
        let callback_tracker = Arc::new(Mutex::new(ImageCallbackTracker::new()));
        window.layout_and_generate_display_list(
            dom_with_image,
            LayoutRect::new(0, 0, 200, 200),
        );
        
        // Verify callback was invoked due to size change
        let tracker = callback_tracker.lock().unwrap();
        assert_eq!(tracker.invocation_count, 1);
        assert_eq!(tracker.last_size, Some(LogicalSize::new(200.0, 200.0)));
    }
    
    #[test]
    fn test_gpu_cache_transform_computation() {
        let mut window = LayoutWindow::new(/* ... */);
        
        // Create DOM with CSS transforms
        let styled_dom = create_test_dom_with_transforms();
        window.layout_and_generate_display_list(styled_dom, /* ... */);
        
        // Update GPU cache
        window.update_gpu_cache_for_dom(DomId::ROOT_ID);
        
        // Verify transforms are cached
        let node_id = DomNodeId { dom: DomId::ROOT_ID, node: NodeId::new(0) };
        let transform = window.get_gpu_value_cache().get_transform(node_id);
        assert!(transform.is_some());
    }
}
```

### Phase 5: Callback Invocation Flow

**Old flow** (broken):
```
window_state.rs → CallbackInfo::new(&[LayoutResult]) → callback() → ???
```

**New flow** (working):
```
1. User event occurs
2. window_state.rs creates CallbackInfo with &mut LayoutWindow
3. Callback executes:
   - Can query layout: callback_info.get_node_size(node_id)
   - Can modify timers: callback_info.start_timer(timer)
   - Can modify threads: callback_info.start_thread(thread)
4. After callback returns:
   - LayoutWindow.tick_timers() processes timer callbacks
   - Timer callbacks can modify CSS → triggers relayout
5. Next frame layout reuses/updates LayoutWindow state
```

**Implementation in window_state.rs**:
```rust
// core/src/window_state.rs (simplified)
pub fn call_callbacks(
    layout_window: &mut LayoutWindow,  // NEW: pass LayoutWindow
    event: &Event,
    // ... other params
) -> CallbackResult {
    // Find nodes that have callbacks for this event
    let nodes_with_callbacks = layout_window.find_nodes_with_callback(event);
    
    for node_id in nodes_with_callbacks {
        let mut callback_info = CallbackInfo::new(
            layout_window,  // Pass mutable reference
            renderer_resources,
            // ... other params
        );
        
        // Invoke callback
        let update = (callback.cb)(&mut callback_data, &mut callback_info);
        
        // callback_info may have modified timers/threads in LayoutWindow
    }
    
    // After all callbacks, tick timers
    let current_time = Instant::now();
    let fired_timers = layout_window.tick_timers(current_time);
    
    for timer_id in fired_timers {
        // Invoke timer callback
        // Timer callback can modify CSS → schedule relayout
    }
    
    CallbackResult { /* ... */ }
}
```

## Migration Steps v2

### Step 1: Extend LayoutWindow with callback state
- [ ] Add `timers: BTreeMap<TimerId, Timer>` to LayoutWindow
- [ ] Add `threads: BTreeMap<ThreadId, Thread>` to LayoutWindow
- [ ] Add `gpu_value_cache: GpuValueCache` to LayoutWindow
- [ ] Implement timer management methods
- [ ] Implement thread management methods
- [ ] Write unit tests for timer/thread management

### Step 2: Create modular structure in azul-layout
- [ ] Create `layout/src/hit_test.rs` module
- [ ] Move hit-test logic from `core/window.rs` to `layout/hit_test.rs`
- [ ] Create `layout/src/timers.rs` for timer utilities
- [ ] Create `layout/src/threads.rs` for thread utilities
- [ ] Update `layout/src/lib.rs` to export new modules

### Step 3: Implement hit-test in LayoutWindow
- [ ] Add `compute_hit_test()` method to LayoutWindow
- [ ] Add `compute_cursor_type_hit_test()` method
- [ ] Write unit tests for hit-testing
- [ ] Test with simple DOM, nested DOMs, IFrames

### Step 4: Update CallbackInfo in layout crate
- [ ] Change `layout_window: *const LayoutWindow` to `*mut LayoutWindow`
- [ ] Implement all delegation methods (timers, threads, GPU cache)
- [ ] Remove old LayoutResult references completely
- [ ] Write unit tests for CallbackInfo methods

### Step 5: Update callback invocation in window_state.rs
- [ ] Change signature to accept `&mut LayoutWindow` instead of `&[LayoutResult]`
- [ ] Update CallbackInfo::new() call site
- [ ] Add timer ticking after callback invocation
- [ ] Test end-to-end callback flow

### Step 6: Implement IFrame callback logic
- [ ] Add IFrame scanning to LayoutWindow
- [ ] Implement conditional re-invocation (bounds/scroll changed)
- [ ] Recursively layout IFrame DOMs
- [ ] Write unit tests with callback tracker
- [ ] Test multi-level IFrame nesting

### Step 7: Implement Image callback logic
- [ ] Add Image callback scanning to LayoutWindow
- [ ] Implement size-change detection
- [ ] Invoke callback when image node resizes
- [ ] Write unit tests with size tracking
- [ ] Test dynamic image loading

### Step 8: Clean up old code
- [ ] Remove old_layout_result.rs (after all migration complete)
- [ ] Remove LayoutResult exports from ui_solver.rs
- [ ] Fix any remaining compilation errors
- [ ] Run full test suite

## Testing Checklist

### Timer Tests
- [ ] Add timer, verify it exists
- [ ] Remove timer, verify it's gone
- [ ] Timer fires at correct time
- [ ] Multiple timers fire in correct order
- [ ] Repeating timer fires multiple times
- [ ] Timer callback can modify CSS
- [ ] Timer callback can stop itself

### Thread Tests  
- [ ] Add thread, verify it exists
- [ ] Remove thread, verify it's gone
- [ ] Thread receives messages
- [ ] Thread callback updates UI
- [ ] Multiple threads run concurrently

### Hit-Test Tests
- [ ] Simple button hit-test works
- [ ] Nested elements return correct node
- [ ] IFrame hit-test returns iframe DOM
- [ ] Scrolled content hit-tests correctly
- [ ] Cursor type updates based on hover

### IFrame Tests
- [ ] IFrame callback invoked on first layout
- [ ] IFrame callback NOT invoked if bounds unchanged
- [ ] IFrame callback invoked if bounds changed
- [ ] IFrame callback invoked if scroll changed
- [ ] Nested IFrames work correctly
- [ ] IFrame DOMs get unique DomIds

### Image Tests
- [ ] Image callback invoked on first layout
- [ ] Image callback NOT invoked if size unchanged
- [ ] Image callback invoked when node resizes
- [ ] Image callback can return new texture
- [ ] Multiple image callbacks work

### Integration Tests
- [ ] Full app with timers + threads + callbacks
- [ ] Animation using timer + CSS modification
- [ ] Infinite scroll using IFrame callbacks
- [ ] Dynamic image loading using image callbacks
- [ ] Complex multi-window scenario

## Benefits of v2 Architecture

1. **Single Source of Truth**: LayoutWindow owns all state
2. **Testable**: Each component has clear unit tests
3. **Modular**: Hit-test, timers, threads in separate modules
4. **Type-Safe**: No more LayoutResult confusion
5. **Efficient**: State reused across frames
6. **Extensible**: Easy to add new callback types
7. **Multi-Window Ready**: Each window has own LayoutWindow

## Timeline Estimate v2

- Phase 1 (LayoutWindow extension): 4-5 hours
- Phase 2 (Modular structure): 2-3 hours
- Phase 3 (Hit-test implementation): 3-4 hours
- Phase 4 (CallbackInfo update): 2-3 hours
- Phase 5 (Callback invocation): 3-4 hours
- Phase 6 (IFrame callbacks): 4-5 hours
- Phase 7 (Image callbacks): 3-4 hours
- Phase 8 (Cleanup): 2-3 hours
- Testing throughout: 8-10 hours

**Total: 31-45 hours of focused work**

**Old Design**:
```rust
// core/src/callbacks.rs
pub struct CallbackInfo {
    layout_results: *const LayoutResult,  // ❌ Doesn't exist
    layout_results_count: usize,
    // ... other fields
}

impl CallbackInfo {
    fn internal_get_layout_results(&self) -> &[LayoutResult] { ... }
    
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.internal_get_layout_results().get(node_id.dom.inner)?;
        let positioned_rectangles = layout_result.rects.as_ref();
        let positioned_rectangle = positioned_rectangles.get(nid)?;
        Some(positioned_rectangle.size)
    }
}
```

**New Design**:
```rust
// layout/src/callbacks.rs
pub struct CallbackInfo {
    layout_window: *const LayoutWindow,  // ✅ Direct access
    // ... other fields (renderer_resources, window_state, etc.)
}

impl CallbackInfo {
    fn internal_get_layout_window(&self) -> &LayoutWindow { 
        unsafe { &*self.layout_window }
    }
    
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_window = self.internal_get_layout_window();
        layout_window.get_node_size(node_id)
    }
    
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<PositionInfo> {
        let layout_window = self.internal_get_layout_window();
        layout_window.get_node_position(node_id)
    }
    
    pub fn get_computed_css_property(
        &self, 
        node_id: DomNodeId, 
        property_type: CssPropertyType
    ) -> Option<CssProperty> {
        let layout_window = self.internal_get_layout_window();
        layout_window.get_computed_style(node_id, property_type)
    }
}
```

### Phase 3: Add Query Methods to LayoutWindow

```rust
// layout/src/window.rs
impl LayoutWindow {
    /// Query the size of a laid-out node across all DOMs
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangle = layout_result.absolute_positions.get(nid)?;
        Some(positioned_rectangle.size())
    }
    
    /// Query the position of a laid-out node
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<PositionInfo> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangle = layout_result.absolute_positions.get(nid)?;
        Some(positioned_rectangle.position())
    }
    
    /// Get computed CSS property value for a node
    pub fn get_computed_style(
        &self, 
        node_id: DomNodeId, 
        property_type: CssPropertyType
    ) -> Option<CssProperty> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let styled_node = layout_result.styled_dom
            .styled_nodes
            .as_container()
            .get(nid)?;
        
        // Query from CSS property cache
        layout_result.styled_dom.css_property_cache
            .ptr
            .get_property(&styled_node.state, property_type)
    }
    
    /// Get node hierarchy item (parent, siblings, children)
    pub fn get_node_hierarchy(&self, node_id: DomNodeId) -> Option<&NodeHierarchyItem> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        layout_result.styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)
    }
    
    /// Scan all fonts used in this LayoutWindow
    pub fn scan_used_fonts(&self) -> BTreeSet<FontKey> {
        let mut fonts = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // Scan layout_result.styled_dom for font references
            // Add to fonts set
        }
        fonts
    }
    
    /// Scan all images used in this LayoutWindow  
    pub fn scan_used_images(&self) -> BTreeSet<ImageRefHash> {
        let mut images = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // Scan layout_result.styled_dom for image references
            // Add to images set
        }
        images
    }
}
```

### Phase 4: Refactor Resource GC

**Old Design** (Monolithic):
```rust
// core/src/resources.rs
impl RendererResources {
    pub fn do_gc(
        &mut self,
        all_resource_updates: &mut Vec<ResourceUpdate>,
        css_image_cache: &ImageCache,
        new_layout_results: &[LayoutResult],  // ❌ Doesn't exist
        gl_texture_cache: &GlTextureCache,
    ) {
        // 1. Scan new_layout_results for used resources
        // 2. Compare with last frame
        // 3. Generate deletion messages
        // 4. Update internal state
    }
}
```

**New Design** (Split Concerns):
```rust
// layout/src/window.rs
impl LayoutWindow {
    /// Scan this LayoutWindow for all used resources
    pub fn scan_used_resources(&self, css_image_cache: &ImageCache) -> UsedResourceSet {
        UsedResourceSet {
            image_keys: self.scan_used_images(),
            font_keys: self.scan_used_fonts(),
            font_instances: self.scan_used_font_instances(),
        }
    }
}

// core/src/resources.rs
pub struct UsedResourceSet {
    pub image_keys: BTreeSet<ImageRefHash>,
    pub font_keys: BTreeSet<FontKey>,
    pub font_instances: BTreeSet<FontInstanceKey>,
}

pub struct ResourceDeletionList {
    pub delete_images: Vec<DeleteImageMsg>,
    pub delete_fonts: Vec<DeleteFontMsg>,
}

impl RendererResources {
    /// Compare previous and current frame resource usage
    pub fn collect_unused_resources(
        &self,
        previous_resources: &UsedResourceSet,
        current_resources: &UsedResourceSet,
    ) -> ResourceDeletionList {
        let delete_images = self.currently_registered_images
            .iter()
            .filter(|(hash, _)| !current_resources.image_keys.contains(hash))
            .map(|(_, resolved)| DeleteImageMsg(resolved.key.clone()))
            .collect();
            
        let delete_fonts = /* similar logic for fonts */;
        
        ResourceDeletionList {
            delete_images,
            delete_fonts,
        }
    }
    
    /// Apply resource deletions and return ResourceUpdate messages
    pub fn apply_resource_deletions(
        &mut self,
        deletions: ResourceDeletionList,
    ) -> Vec<ResourceUpdate> {
        // Remove from internal maps
        // Generate ResourceUpdate messages
        // Return updates for WebRender
    }
}

// Usage:
let current_resources = layout_window.scan_used_resources(&image_cache);
let deletions = renderer_resources.collect_unused_resources(
    &previous_resources, 
    &current_resources
);
let updates = renderer_resources.apply_resource_deletions(deletions);
```

### Phase 5: Update Layout/Callback Flow

**New rendering pipeline**:
```rust
// In layout/src/window.rs or similar

pub fn layout_and_render(
    layout_window: &mut LayoutWindow,
    styled_dom: StyledDom,
    viewport: LayoutRect,
    callbacks: &CallbackRegistry,
    renderer_resources: &mut RendererResources,
) -> DisplayList {
    // 1. Layout the DOM
    let display_list = layout_window.layout_and_generate_display_list(
        styled_dom,
        viewport,
        /* font manager, etc. */
    );
    
    // 2. Invoke IFrame callbacks
    let iframe_callbacks = display_list.scan_for_iframe_callbacks();
    for (node_id, iframe_callback, bounds) in iframe_callbacks {
        let mut callback_info = IFrameCallbackInfo::new(
            &layout_window,  // ✅ Direct access
            bounds,
            node_id,
        );
        
        let iframe_dom = (iframe_callback.cb)(&mut callback_info);
        
        // Recursively layout iframe content
        let iframe_dom_id = layout_window.allocate_dom_id();
        let iframe_display_list = layout_window.layout_and_generate_display_list(
            iframe_dom,
            bounds.into(),
            /* ... */
        );
        
        // Merge display lists
        display_list.merge_iframe(node_id, iframe_display_list);
    }
    
    // 3. Invoke regular callbacks (onclick, etc.)
    for event in events {
        let mut callback_info = CallbackInfo::new(
            &layout_window,  // ✅ Direct access
            &renderer_resources,
            &window_state,
            /* ... */
        );
        
        let update = (callback.cb)(&mut callback_info);
        // Handle update...
    }
    
    display_list
}
```

## Migration Steps

### Step 1: Create layout/callbacks.rs stub
- [ ] Create empty `layout/src/callbacks.rs`
- [ ] Add to `layout/src/lib.rs`: `pub mod callbacks;`

### Step 2: Move callback types to layout
- [ ] Move `CallbackInfo`, `CallbackType`, `Callback` to layout/callbacks.rs
- [ ] Move `IFrameCallback`, `RenderImageCallback` to layout/callbacks.rs
- [ ] Update imports in layout crate

### Step 3: Add LayoutWindow query methods
- [ ] Implement `get_node_size()` in LayoutWindow
- [ ] Implement `get_node_position()` in LayoutWindow
- [ ] Implement `get_computed_style()` in LayoutWindow
- [ ] Implement `get_node_hierarchy()` in LayoutWindow

### Step 4: Update CallbackInfo implementation
- [ ] Replace `layout_results: *const LayoutResult` with `layout_window: *const LayoutWindow`
- [ ] Reimplement all query methods using LayoutWindow
- [ ] Test callback functionality

### Step 5: Split resource GC
- [ ] Add `UsedResourceSet` struct to resources.rs
- [ ] Add `ResourceDeletionList` struct to resources.rs
- [ ] Implement `LayoutWindow::scan_used_resources()`
- [ ] Refactor `do_gc()` into three methods

### Step 6: Clean up core crate
- [ ] Remove `old_layout_result.rs`
- [ ] Remove LayoutResult imports from core files
- [ ] Keep only C-ABI types in core/callbacks.rs
- [ ] Verify azul-core compiles independently

### Step 7: Test integration
- [ ] Write tests for CallbackInfo queries
- [ ] Write tests for resource GC with multi-window
- [ ] Write tests for IFrame callback invocation
- [ ] Verify end-to-end rendering pipeline

## Benefits

1. **Correct Dependency Direction**: Layout types stay in layout crate
2. **Multi-DOM Support**: CallbackInfo can query any DOM in LayoutWindow
3. **Cleaner GC**: Resource scanning separated from GC logic
4. **Better Testability**: Each concern can be tested independently
5. **Multi-Window Ready**: Resource GC works per-window
6. **Extensibility**: Easy to add new callback query methods

## Risks

1. **Large Refactor**: Touches many files across crates
2. **API Breaking**: External users (Python, C++) need updates
3. **Testing Burden**: Need comprehensive tests for new architecture

## Timeline Estimate

- Phase 1 (Move callbacks): 2-3 hours
- Phase 2 (CallbackInfo redesign): 3-4 hours  
- Phase 3 (LayoutWindow queries): 2-3 hours
- Phase 4 (Resource GC split): 3-4 hours
- Phase 5 (Integration): 2-3 hours
- Testing & debugging: 4-6 hours

**Total: 16-23 hours of focused work**

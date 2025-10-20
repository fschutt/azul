# macOS Shell2 - Next Steps

## Current Status

✅ **COMPLETED:**
1. LayoutWindow initialization with proper font cache management
2. WebRender integration (renderer, render_api, hit_tester)
3. Hit-testing with DomId-based pipeline management
4. Event handling framework (437 lines)
5. Menu system (169 lines)
6. `regenerate_layout()` - calls layout callback, performs layout, calls stub display list translation
7. `gpu_scroll()` - creates scroll events, updates scroll manager, sends WebRender transactions
8. Stub functions for `rebuild_display_list()` and `generate_frame()` in wr_translate2

🏗️ **IN PROGRESS:**
1. Callback dispatch infrastructure
2. Remaining TODOs in events.rs

## Architecture Overview

### Resource Management
- **FcFontCache**: Should be shared across all windows via App (LazyFcCache pattern)
- **ImageCache**: Per-window resource cache
- **RendererResources**: Per-window GPU texture cache
- LayoutWindow gets fc_cache passed as parameter to `regenerate_layout()`

### Pipeline Management
- **DomId = PipelineId**: Each DOM gets its own WebRender pipeline for iframe support
- Root DOM is DomId::ROOT_ID (0)
- `generate_frame()` sets root pipeline based on DomId
- Future: Iterate through `layout_results.keys()` to submit multiple pipelines

### Event Flow
```
NSEvent → MacOSEvent → handle_* methods → perform_hit_test() → dispatch_*_callbacks()
                                                                         ↓
                                                              LayoutWindow::invoke_single_callback()
                                                                         ↓
                                                              Update (RefreshDom/DoNothing/etc.)
                                                                         ↓
                                                          regenerate_layout() or ShouldReRender
```

### Rendering Flow
```
Layout Callback (User Code)
          ↓
    StyledDom
          ↓
layout_and_generate_display_list() (solver3)
          ↓
  layout_results: BTreeMap<DomId, DomLayoutResult>
          ↓
rebuild_display_list() (compositor2) ← STUB
          ↓
WebRender Display Lists
          ↓
generate_frame()
          ↓
WebRender Rendering
```

## Remaining TODOs

### HIGH PRIORITY

#### 1. Callback Dispatch Implementation

**Location**: `dll/src/desktop/shell2/macos/events.rs`

**Tasks**:
- [ ] `dispatch_mouse_down_callbacks()` (line ~342)
  - Get callbacks from layout_result for hit node
  - Filter by `On::MouseDown` event
  - Invoke with `LayoutWindow::invoke_single_callback()`
  - Process Update result
  
- [ ] `dispatch_mouse_up_callbacks()` (line ~360)
  - Filter by `On::MouseUp` event
  
- [ ] `dispatch_hover_callbacks()` (line ~370)
  - Filter by `On::Hover` event
  - Track last_hovered_node for enter/leave
  
- [ ] `dispatch_file_drop_callbacks()` (line ~435)
  - Filter by `On::FileDrop` event

**Reference**: `dll/src/desktop/shell/process.rs` - `process_callback_results()`

**API**:
```rust
// From layout_result, get callbacks for node
let callbacks = layout_result.styled_dom.get_callbacks_for_node(node_id);

// Filter by event type
let filtered = callbacks.iter().filter(|cb| cb.event == On::MouseDown);

// Invoke callback
let result = layout_window.invoke_single_callback(
    callback,
    &callback_info,
    app_data,
    fc_cache,
);

// Process Update result
match result.update {
    Update::RefreshDom => regenerate_layout(),
    Update::RefreshDomAllWindows => notify_all_windows(),
    Update::DoNothing => {},
}
```

#### 2. Keyboard State Management

**Location**: `dll/src/desktop/shell2/macos/events.rs` (line ~332)

**Task**: Update `self.current_window_state.keyboard_state` on key events

```rust
fn handle_key_down(&mut self, event: &NSEvent, character: Option<char>) {
    let keycode = unsafe { event.keyCode() };
    let vk = self.convert_keycode(keycode);
    
    // Update keyboard state
    if let Some(vk_code) = vk {
        self.current_window_state.keyboard_state.pressed_virtual_keycodes.insert(vk_code);
    }
    if let Some(ch) = character {
        self.current_window_state.keyboard_state.pressed_scancodes.insert(ch);
    }
    
    // Dispatch callbacks...
}

fn handle_key_up(&mut self, event: &NSEvent, character: Option<char>) {
    // Remove from pressed keys
    if let Some(vk_code) = vk {
        self.current_window_state.keyboard_state.pressed_virtual_keycodes.remove(&vk_code);
    }
    // ...
}
```

#### 3. Window Resize Handling

**Location**: `dll/src/desktop/shell2/macos/events.rs` (line ~461-462)

**Tasks**:
- [ ] Notify WebRender compositor of new framebuffer size
- [ ] Resize GL viewport via `glViewport()`
- [ ] Trigger `regenerate_layout()` since layout depends on window size

```rust
fn handle_resize(&mut self, new_size: LogicalSize, fc_cache: &mut FcFontCache, app_data: &mut RefAny) {
    self.current_window_state.size.dimensions = new_size;
    
    // Notify WebRender of new framebuffer size
    let physical_size = new_size.to_physical(self.current_window_state.size.get_hidpi_factor());
    let framebuffer_size = webrender::api::units::DeviceIntSize::new(
        physical_size.width as i32,
        physical_size.height as i32,
    );
    
    let mut txn = WrTransaction::new();
    txn.set_document_view(webrender::api::units::DeviceIntRect::from_origin_and_size(
        webrender::api::units::DeviceIntPoint::new(0, 0),
        framebuffer_size,
    ));
    self.render_api.send_transaction(
        wr_translate_document_id(self.document_id),
        txn,
    );
    
    // Resize GL viewport
    if let Some(ref gl_funcs) = self.gl_functions {
        unsafe {
            gl_funcs.functions.viewport_gl(
                0,
                0,
                physical_size.width as i32,
                physical_size.height as i32,
            );
        }
    }
    
    // Regenerate layout with new window size
    self.regenerate_layout(app_data, fc_cache)
        .unwrap_or_else(|e| eprintln!("Layout error: {}", e));
}
```

#### 4. Focus Management

**Task**: Update `self.current_window_state.focused_node` based on hit-testing and user input

```rust
// In handle_mouse_down:
if let Some(hit_node) = self.perform_hit_test(position) {
    let dom_node_id = DomNodeId {
        dom: DomId { inner: hit_node.dom_id as usize },
        node: NodeId::from_crate_internal(hit_node.node_id as u32),
    };
    
    // Check if node is focusable
    if let Some(layout_window) = &self.layout_window {
        if let Some(layout_result) = layout_window.layout_results.get(&dom_node_id.dom) {
            if is_focusable(&layout_result, dom_node_id.node) {
                self.current_window_state.focused_node = Some(dom_node_id);
            }
        }
    }
}
```

### MEDIUM PRIORITY

#### 5. Full Display List Translation

**Location**: `dll/src/desktop/wr_translate2.rs` - `rebuild_display_list()`

**Task**: Implement full compositor2 translation from `DomLayoutResult` to WebRender display lists

**Steps**:
1. For each `(dom_id, layout_result)` in `layout_window.layout_results`:
   - Get cached display list: `layout_result.get_cached_display_list()`
   - Scale for DPI
   - Translate to WebRender using compositor2 functions
   - Set pipeline ID = DomId
   
2. Update resources (images, fonts):
   - `txn.update_resources(...)` for new images
   - `txn.add_font_instance(...)` for fonts
   
3. Set display list for each pipeline:
   - `txn.set_display_list(epoch, None, size, (pipeline_id, display_list), preserve)`

**Reference**: `dll/src/desktop/wr_translate.rs` - `rebuild_display_list()` (line 407-467)

#### 6. GPU Value Synchronization

**Location**: `dll/src/desktop/shell2/macos/mod.rs` - `synchronize_gpu_values()`

**Task**: Implement transform/opacity synchronization to WebRender

**Reference**: `dll/src/desktop/wr_translate.rs` - `synchronize_gpu_values()` (line 293-383)

```rust
fn synchronize_gpu_values(&mut self, layout_window: &LayoutWindow, txn: &mut WrTransaction) {
    use webrender::api::{DynamicProperties, PropertyBindingKey, PropertyValue};
    
    let dpi = layout_window.current_window_state.size.get_hidpi_factor();
    
    let transforms = layout_window.layout_results.values()
        .flat_map(|lr| {
            lr.gpu_value_cache.transform_keys.iter()
                .filter_map(|(nid, key)| {
                    let mut value = lr.gpu_value_cache.current_transform_values.get(nid)?;
                    value.scale_for_dpi(dpi);
                    Some((key, value))
                })
        })
        .map(|(k, v)| PropertyValue {
            key: PropertyBindingKey::new(k.id as u64),
            value: wr_translate_layout_transform(&v),
        })
        .collect::<Vec<_>>();
    
    let floats = layout_window.layout_results.values()
        .flat_map(|lr| {
            lr.gpu_value_cache.opacity_keys.iter()
                .filter_map(|(nid, key)| {
                    let value = lr.gpu_value_cache.current_opacity_values.get(nid)?;
                    Some((key, *value))
                })
        })
        .map(|(k, v)| PropertyValue {
            key: PropertyBindingKey::new(k.id as u64),
            value: v,
        })
        .collect::<Vec<_>>();
    
    txn.update_dynamic_properties(DynamicProperties {
        transforms,
        floats,
        colors: Vec::new(),
    });
}
```

#### 7. Scroll Node Synchronization

**Location**: `dll/src/desktop/shell2/macos/mod.rs` - `scroll_all_nodes()`

**Task**: Map scroll states to WebRender external scroll IDs

**Reference**: `dll/src/desktop/wr_translate.rs` - `scroll_all_nodes()` (line 276-289)

```rust
fn scroll_all_nodes(&mut self, scroll_manager: &ScrollManager, txn: &mut WrTransaction) {
    use webrender::api::ScrollClamping;
    
    for ((dom_id, node_id), scroll_state) in scroll_manager.iter_scroll_states() {
        // Get layout result for dom_id
        if let Some(layout_window) = &self.layout_window {
            if let Some(layout_result) = layout_window.layout_results.get(&dom_id) {
                // Find external_scroll_id for node_id in layout_result
                if let Some(external_scroll_id) = layout_result.get_external_scroll_id(node_id) {
                    txn.scroll_node_with_id(
                        wr_translate_logical_position(scroll_state.get_offset()),
                        wr_translate_external_scroll_id(external_scroll_id),
                        ScrollClamping::ToContentBounds,
                    );
                }
            }
        }
    }
}
```

### LOW PRIORITY

#### 8. Timer Support

**Location**: To be added to `MacOSWindow`

**Task**: Implement timer registration and callback dispatch

**Reference**: `dll/src/desktop/shell/process.rs` - `process_timer()`

#### 9. Thread Support

**Location**: To be added to `MacOSWindow`

**Task**: Implement background thread task support

**Reference**: `dll/src/desktop/shell/process.rs` - `process_threads()`

#### 10. IME Support

**Location**: To be added to `events.rs`

**Task**: Implement Input Method Editor for CJK text input

## Testing Strategy

1. **Unit Tests**: Test individual event handlers with mock data
2. **Integration Tests**: Test full event → callback → layout → render cycle
3. **Visual Tests**: Compare rendering output with reference images
4. **Performance Tests**: Measure hit-testing and layout performance

## Performance Considerations

- Use `skip_scene_builder()` when display list hasn't changed
- Batch WebRender transactions when possible
- Cache hit-test results between frames if cursor hasn't moved
- Use GPU scrolling (no relayout) for smooth 60fps scrolling
- Lazy font cache initialization (LazyFcCache pattern)

## Known Issues

1. **TODO: external_scroll_id mapping** - Need to establish mapping between (DomId, NodeId) and WebRender ExternalScrollId
2. **TODO: iframe_mapping** - Field removed from DomLayoutResult, needs re-implementation for nested iframe hit-testing
3. **TODO: scrollable_nodes** - Field removed from DomLayoutResult, affects scroll hit-testing
4. **TODO: proper window ID tracking** - Currently using 0, need proper WindowId management

## References

- **Old Implementation**: `dll/src/desktop/shell/appkit/mod.rs`
- **Windows Reference**: `dll/src/desktop/shell/win32/mod.rs`
- **Process Logic**: `dll/src/desktop/shell/process.rs`
- **WebRender Translation**: `dll/src/desktop/wr_translate.rs`
- **Layout System**: `layout/src/window.rs`, `layout/src/solver3/mod.rs`

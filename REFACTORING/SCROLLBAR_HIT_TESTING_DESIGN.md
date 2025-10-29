# Scrollbar Hit-Testing Design

## Current Status

Das Scrollbar-System ist teilweise implementiert:

✅ **Vorhanden:**
- `ScrollbarHitId` enum in `core/src/hit_test.rs`
- `ScrollbarHitTestItem` struct in `core/src/hit_test.rs`
- `ScrollbarDragState` in `layout/src/window.rs`
- `push_scrollbar()` in display list builder
- Scrollbar-Generierung basierend auf `ScrollbarInfo`

❌ **Fehlt:**
- Hit-Test IDs werden nicht an scrollbar items übergeben
- WebRender hit-testing für scrollbar components
- Event-Handler für scrollbar interactions
- Integration mit `handle_mouse_down/up/move`

## Architecture Overview

### 1. Display List Generation (Layout → WebRender)

**Current:**
```rust
// In solver3/display_list.rs
builder.push_scrollbar(
    sb_bounds,
    ColorU::new(192, 192, 192, 255),
    ScrollbarOrientation::Vertical,
    opacity_key,  // For GPU animations
);
```

**Proposed:**
```rust
// Generate unique hit-test ID for this scrollbar
let scrollbar_hit_id = ScrollbarHitId::VerticalThumb(dom_id, node_id);

builder.push_scrollbar(
    sb_bounds,
    ColorU::new(192, 192, 192, 255),
    ScrollbarOrientation::Vertical,
    opacity_key,
    Some(scrollbar_hit_id),  // NEW: For WebRender hit-testing
);
```

### 2. DisplayListItem Extension

**Add hit_id field to ScrollBar variant:**

```rust
// In solver3/display_list.rs
pub enum DisplayListItem {
    ScrollBar {
        bounds: LogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
        opacity_key: Option<OpacityKey>,
        hit_id: Option<ScrollbarHitId>,  // NEW
    },
    // ... other variants
}
```

### 3. WebRender Translation

**In `dll/src/desktop/wr_translate2.rs`:**

```rust
fn translate_display_list_item(
    item: &DisplayListItem,
    builder: &mut WrDisplayListBuilder,
    spatial_id: WrSpatialId,
) {
    match item {
        DisplayListItem::ScrollBar {
            bounds,
            color,
            orientation,
            opacity_key,
            hit_id,
        } => {
            // Create WebRender primitive info with hit-testing tag
            let mut prim_info = WrPrimitiveInfo::new(
                wr_translate_logical_rect(*bounds)
            );
            
            // Add hit-test tag if present
            if let Some(scrollbar_hit_id) = hit_id {
                prim_info.tag = Some(wr_translate_scrollbar_hit_id(*scrollbar_hit_id));
            }
            
            // Push rectangle to WebRender
            builder.push_rect(
                &prim_info,
                spatial_id,
                wr_translate_color_u(*color),
            );
        }
        // ... other variants
    }
}

fn wr_translate_scrollbar_hit_id(hit_id: ScrollbarHitId) -> WrItemTag {
    match hit_id {
        ScrollbarHitId::VerticalTrack(dom_id, node_id) => {
            WrItemTag((dom_id.inner as u64) << 32 | (node_id.index() as u64))
        }
        ScrollbarHitId::VerticalThumb(dom_id, node_id) => {
            WrItemTag((dom_id.inner as u64) << 32 | (node_id.index() as u64) | (1 << 62))
        }
        // ... other variants
    }
}
```

### 4. Hit-Testing Flow

```
User clicks on screen position
    ↓
NSEvent → handle_mouse_down(event, button)
    ↓
WebRender hit-test at position
    ↓
Returns WrItemTag
    ↓
Translate WrItemTag → ScrollbarHitId
    ↓
Check if hit_id is ScrollbarHitId variant
    ↓
If YES: handle_scrollbar_click(scrollbar_hit_id, position)
If NO:  perform_regular_hit_test() → dispatch_callbacks()
```

### 5. Event Handler Implementation

```rust
// In dll/src/desktop/shell2/macos/events.rs
impl MacOSWindow {
    pub fn handle_mouse_down(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = 
            CursorPosition::InWindow(position);
        
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = true,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = true,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = true,
            _ => {}
        }

        // First: Check if we hit a scrollbar using WebRender hit-test
        if let Some(scrollbar_hit) = self.perform_webrender_hit_test(position) {
            return self.handle_scrollbar_click(scrollbar_hit, position);
        }

        // Second: Regular DOM node hit-test
        let hit_test_result = self.perform_hit_test(position);
        if let Some(hit_node) = hit_test_result {
            self.last_hovered_node = Some(hit_node);
            
            let mut app_data_borrowed = self.app_data.borrow_mut();
            let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

            let callback_result = self.dispatch_mouse_down_callbacks(
                hit_node,
                button,
                position,
                &mut *app_data_borrowed,
                &mut *fc_cache_borrowed,
            );

            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    fn perform_webrender_hit_test(
        &self,
        position: LogicalPosition,
    ) -> Option<ScrollbarHitId> {
        // Use WebRender hit-tester
        let hit_tester = match &self.hit_tester {
            AsyncHitTester::Resolved(ht) => ht,
            _ => return None,
        };

        let cursor_world_point = WrWorldPoint::new(position.x, position.y);
        
        let hit_test_result = hit_tester.hit_test(
            None, // No pipeline filter
            cursor_world_point,
            WrHitTestFlags::empty(),
        );

        // Check if any hit item has a scrollbar tag
        for item in &hit_test_result.items {
            if let Some(tag) = item.tag {
                if let Some(scrollbar_hit_id) = translate_item_tag_to_scrollbar_hit_id(tag) {
                    return Some(scrollbar_hit_id);
                }
            }
        }

        None
    }

    fn handle_scrollbar_click(
        &mut self,
        scrollbar_hit_id: ScrollbarHitId,
        position: LogicalPosition,
    ) -> EventProcessResult {
        match scrollbar_hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id) 
            | ScrollbarHitId::HorizontalThumb(dom_id, node_id) => {
                // Start drag operation
                let orientation = match scrollbar_hit_id {
                    ScrollbarHitId::VerticalThumb(..) => ScrollbarOrientation::Vertical,
                    ScrollbarHitId::HorizontalThumb(..) => ScrollbarOrientation::Horizontal,
                    _ => unreachable!(),
                };

                // Get current scroll offset
                let layout_window = match self.layout_window.as_ref() {
                    Some(lw) => lw,
                    None => return EventProcessResult::DoNothing,
                };

                let scroll_offset = layout_window
                    .scroll_states
                    .get_current_offset(dom_id, node_id)
                    .unwrap_or_default();

                // Start drag
                self.scrollbar_drag_state = Some(ScrollbarDragState {
                    dom_id,
                    node_id,
                    orientation,
                    start_position: position,
                    start_scroll_offset: scroll_offset,
                });

                EventProcessResult::RequestRedraw
            }

            ScrollbarHitId::VerticalTrack(dom_id, node_id) 
            | ScrollbarHitId::HorizontalTrack(dom_id, node_id) => {
                // Jump to clicked position
                self.scroll_to_position_at_click(dom_id, node_id, position)?;
                EventProcessResult::RequestRedraw
            }
        }
    }

    pub fn handle_mouse_move(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = 
            CursorPosition::InWindow(position);

        // Handle scrollbar drag if active
        if let Some(drag_state) = &self.scrollbar_drag_state {
            return self.handle_scrollbar_drag(position);
        }

        // Regular hover handling
        let hit_test_result = self.perform_hit_test(position);
        if let Some(hit_node) = hit_test_result {
            if self.last_hovered_node != Some(hit_node) {
                self.last_hovered_node = Some(hit_node);

                let mut app_data_borrowed = self.app_data.borrow_mut();
                let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

                let callback_result = self.dispatch_hover_callbacks(
                    hit_node,
                    position,
                    &mut *app_data_borrowed,
                    &mut *fc_cache_borrowed,
                );

                return self.process_callback_result_to_event_result(callback_result);
            }
        } else {
            self.last_hovered_node = None;
        }

        EventProcessResult::DoNothing
    }

    pub fn handle_mouse_up(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let position = LogicalPosition::new(location.x as f32, location.y as f32);

        // Clear button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = false,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = false,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = false,
            _ => {}
        }

        // End scrollbar drag if active
        if self.scrollbar_drag_state.is_some() {
            self.scrollbar_drag_state = None;
            return EventProcessResult::RequestRedraw;
        }

        // Regular mouse up handling
        let hit_test_result = self.perform_hit_test(position);
        if let Some(hit_node) = hit_test_result {
            let mut app_data_borrowed = self.app_data.borrow_mut();
            let mut fc_cache_borrowed = self.fc_cache.borrow_mut();

            let callback_result = self.dispatch_mouse_up_callbacks(
                hit_node,
                button,
                position,
                &mut *app_data_borrowed,
                &mut *fc_cache_borrowed,
            );

            return self.process_callback_result_to_event_result(callback_result);
        }

        EventProcessResult::DoNothing
    }

    fn handle_scrollbar_drag(&mut self, current_position: LogicalPosition) -> EventProcessResult {
        let drag_state = match &self.scrollbar_drag_state {
            Some(ds) => ds.clone(),
            None => return EventProcessResult::DoNothing,
        };

        // Calculate delta from start position
        let delta = LogicalPosition::new(
            current_position.x - drag_state.start_position.x,
            current_position.y - drag_state.start_position.y,
        );

        // Convert drag delta to scroll delta based on scrollbar geometry
        let layout_window = match self.layout_window.as_ref() {
            Some(lw) => lw,
            None => return EventProcessResult::DoNothing,
        };

        // Get scrollbar state to calculate ratio
        let scrollbar_state = layout_window
            .scroll_states
            .get_scrollbar_state(
                drag_state.dom_id,
                drag_state.node_id,
                drag_state.orientation,
            );

        let scroll_delta = match scrollbar_state {
            Some(sb_state) => {
                // Calculate scroll delta based on drag delta and scrollbar geometry
                match drag_state.orientation {
                    ScrollbarOrientation::Vertical => {
                        let track_height = sb_state.track_rect.size.height;
                        let content_height = sb_state.thumb_size_ratio; // TODO: Get actual content height
                        let scroll_ratio = delta.y / track_height;
                        LogicalPosition::new(0.0, scroll_ratio * content_height)
                    }
                    ScrollbarOrientation::Horizontal => {
                        let track_width = sb_state.track_rect.size.width;
                        let content_width = sb_state.thumb_size_ratio; // TODO: Get actual content width
                        let scroll_ratio = delta.x / track_width;
                        LogicalPosition::new(scroll_ratio * content_width, 0.0)
                    }
                }
            }
            None => return EventProcessResult::DoNothing,
        };

        // Apply scroll delta
        let new_offset = LogicalPosition::new(
            drag_state.start_scroll_offset.x + scroll_delta.x,
            drag_state.start_scroll_offset.y + scroll_delta.y,
        );

        // Use gpu_scroll to update scroll position
        if let Err(e) = self.gpu_scroll(
            drag_state.dom_id.inner as u64,
            drag_state.node_id.index() as u64,
            scroll_delta.x,
            scroll_delta.y,
        ) {
            eprintln!("Scrollbar drag failed: {}", e);
            return EventProcessResult::DoNothing;
        }

        EventProcessResult::RequestRedraw
    }

    fn scroll_to_position_at_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        click_position: LogicalPosition,
    ) -> Result<(), String> {
        // TODO: Calculate scroll target based on click position in scrollbar track
        // This should scroll by one "page" (container height/width)
        Ok(())
    }
}
```

## Implementation Steps

### Phase 1: Display List Integration ✅ (Mostly Done)
- [x] `ScrollbarHitId` enum exists
- [x] `ScrollbarHitTestItem` exists
- [ ] Add `hit_id` field to `DisplayListItem::ScrollBar`
- [ ] Generate unique `ScrollbarHitId` during display list build
- [ ] Pass hit_id to `push_scrollbar()`

### Phase 2: WebRender Translation
- [ ] Implement `wr_translate_scrollbar_hit_id()`
- [ ] Add hit-test tag to scrollbar primitives in WebRender
- [ ] Implement `translate_item_tag_to_scrollbar_hit_id()`

### Phase 3: Event Handling
- [ ] Implement `perform_webrender_hit_test()`
- [ ] Implement `handle_scrollbar_click()`
- [ ] Implement `handle_scrollbar_drag()`
- [ ] Integrate into `handle_mouse_down/up/move()`

### Phase 4: Scrollbar Components
Currently we only have Track and Thumb. Need to add:
- [ ] `ScrollbarHitId::VerticalTopButton`
- [ ] `ScrollbarHitId::VerticalBottomButton`
- [ ] `ScrollbarHitId::HorizontalLeftButton`
- [ ] `ScrollbarHitId::HorizontalRightButton`
- [ ] Render buttons in display list
- [ ] Handle button clicks (scroll by one page)

### Phase 5: Refinements
- [ ] Cursor changes (hand on hover, grabbing during drag)
- [ ] Smooth scrolling animations
- [ ] Scrollbar styling (colors, hover states)
- [ ] Keyboard navigation (PageUp/Down, Home/End)

## Key Design Decisions

### 1. Why WebRender Hit-Testing?

**Advantages:**
- ✅ Unified hit-testing system (same as regular DOM nodes)
- ✅ Handles z-ordering automatically (scrollbars on top)
- ✅ Handles clipping/transforms correctly
- ✅ Efficient (GPU-accelerated in hardware mode)
- ✅ Supports nested scrollbars (iframes with scrollable content)

**Alternatives Rejected:**
- ❌ Geometry-based hit-testing in `scroll.rs`: Duplicates logic, doesn't handle transforms
- ❌ Separate hit-test system: Complexity, ordering issues

### 2. ScrollbarHitId Encoding

Current encoding in `core/src/hit_test.rs`:
```rust
pub enum ScrollbarHitId {
    VerticalTrack(DomId, NodeId),
    VerticalThumb(DomId, NodeId),
    HorizontalTrack(DomId, NodeId),
    HorizontalThumb(DomId, NodeId),
}
```

**WebRender Tag Encoding:**
- Bits 0-31: NodeId.index()
- Bits 32-61: DomId.inner
- Bits 62-63: Component type (00=VTrack, 01=VThumb, 10=HTrack, 11=HThumb)

### 3. Transform-Based Sizing

Scrollbars are still rendered as 1:1 squares with GPU transforms:
- Base size: 12x12 pixels
- Scale via `opacity_key` (already exists)
- Transform updates via `GpuStateManager`

This allows:
- Window resize without display list rebuild
- Smooth animations
- Efficient GPU processing

## Testing Plan

### Unit Tests
- [ ] `wr_translate_scrollbar_hit_id()` round-trip
- [ ] `translate_item_tag_to_scrollbar_hit_id()` decoding
- [ ] Scrollbar geometry calculations

### Integration Tests
- [ ] Click on scrollbar track → scroll to position
- [ ] Drag scrollbar thumb → continuous scroll
- [ ] Click on scrollbar buttons → page scroll
- [ ] Nested scrollbars (iframe) → correct z-ordering

### Manual Tests
- [ ] Drag vertical scrollbar smoothly
- [ ] Drag horizontal scrollbar smoothly
- [ ] Click above/below thumb (jump scroll)
- [ ] Click on top/bottom buttons
- [ ] Resize window (scrollbars scale correctly)
- [ ] Cursor changes on hover

## Migration from Previous Design

The previous session implemented:
- ✅ `ScrollManager` with scrollbar state tracking
- ✅ `ScrollbarState` with geometry calculations
- ✅ `ExternalScrollId` mapping
- ✅ `calculate_scrollbar_states()` method
- ⚠️ `hit_test_scrollbars()` - REPLACE with WebRender hit-test

**Migration:**
- Keep: `ScrollbarState` for geometry and GPU transforms
- Keep: `calculate_scrollbar_states()` for per-frame updates
- Replace: `hit_test_scrollbars()` → use WebRender hit-test
- Add: `hit_id` to display list items
- Add: WebRender tag translation

This design is **cleaner** and **more maintainable** than the previous geometric approach!

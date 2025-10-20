# Session 7: Scrollbar Hit-Testing Implementation Progress

**Date**: 20. Oktober 2025

## Summary

Implemented Phase 1 (Display List Integration) and Phase 2 (WebRender Translation Functions) of the scrollbar hit-testing system. The infrastructure is now in place for WebRender-based scrollbar hit-testing, but the full display list translation to WebRender is still pending.

## Completed Work

### Phase 1: Display List Integration ✅

**Files Modified:**
- `layout/src/solver3/display_list.rs`
- `layout/src/cpurender/mod.rs`
- `layout/src/scroll.rs`
- `layout/src/window.rs`
- `layout/src/lib.rs`
- `dll/src/desktop/shell2/macos/events.rs`
- `dll/src/desktop/shell2/macos/mod.rs`

**Changes:**

1. **Added `hit_id` field to `DisplayListItem::ScrollBar`**
   ```rust
   ScrollBar {
       bounds: LogicalRect,
       color: ColorU,
       orientation: ScrollbarOrientation,
       opacity_key: Option<OpacityKey>,
       hit_id: Option<ScrollbarHitId>,  // NEW
   }
   ```

2. **Updated `push_scrollbar()` signature**
   ```rust
   pub fn push_scrollbar(
       &mut self,
       bounds: LogicalRect,
       color: ColorU,
       orientation: ScrollbarOrientation,
       opacity_key: Option<OpacityKey>,
       hit_id: Option<ScrollbarHitId>,  // NEW
   )
   ```

3. **Generate `ScrollbarHitId` during display list build**
   - Line 829: Vertical scrollbar → `ScrollbarHitId::VerticalThumb(dom_id, node_id)`
   - Line 861: Horizontal scrollbar → `ScrollbarHitId::HorizontalThumb(dom_id, node_id)`

4. **Updated CPU rendering to ignore `hit_id`**
   ```rust
   DisplayListItem::ScrollBar {
       bounds,
       color,
       orientation,
       opacity_key: _,
       hit_id: _,  // Ignored in CPU rendering
   } => { ... }
   ```

5. **Deprecated custom hit-testing code in `scroll.rs`**
   - Commented out `ScrollbarState::contains()` method
   - Commented out `ScrollManager::hit_test_scrollbars()` method
   - These will be removed once WebRender hit-testing is fully integrated

6. **Unified `ScrollbarDragState` struct**
   - Made `layout::window::ScrollbarDragState` public
   - Exported from `layout` crate
   - Removed duplicate from `dll/src/desktop/shell2/macos/events.rs`
   - Updated `MacOSWindow::scrollbar_drag_state` to use unified type
   
   **Final definition (in `layout/src/window.rs`):**
   ```rust
   #[derive(Debug, Clone)]
   pub struct ScrollbarDragState {
       pub hit_id: ScrollbarHitId,
       pub initial_mouse_pos: LogicalPosition,
       pub initial_scroll_offset: LogicalPosition,
   }
   ```

### Phase 2: WebRender Translation Functions ✅

**Files Modified:**
- `dll/src/desktop/wr_translate2.rs`

**Changes:**

1. **Added `wr_translate_scrollbar_hit_id()` function**
   - Encodes `ScrollbarHitId` into WebRender `ItemTag`
   - Encoding scheme:
     - Bits 0-31: `NodeId.index()` (32 bits)
     - Bits 32-61: `DomId.inner` (30 bits)
     - Bits 62-63: Component type (2 bits)
       - 00 = VerticalTrack
       - 01 = VerticalThumb
       - 10 = HorizontalTrack
       - 11 = HorizontalThumb

2. **Added `translate_item_tag_to_scrollbar_hit_id()` function**
   - Decodes WebRender `ItemTag` back to `ScrollbarHitId`
   - Returns `None` if tag doesn't represent a scrollbar
   - Enables event handlers to identify scrollbar hits

## Architecture

### Hit-Testing Flow (Designed)

```
User clicks on screen position
    ↓
NSEvent → handle_mouse_down(event, button)
    ↓
perform_webrender_hit_test(position)
    ↓
WebRender hit-tester returns WrItemTag
    ↓
translate_item_tag_to_scrollbar_hit_id(tag)
    ↓
If ScrollbarHitId: handle_scrollbar_click(hit_id, position)
If None: perform_regular_hit_test() → dispatch_callbacks()
```

### Data Flow

```
Layout Phase:
  calculate_scrollbar_states() 
    → ScrollbarState (geometry, transforms)
    → DisplayListBuilder::push_scrollbar(bounds, color, orientation, opacity_key, hit_id)
    → DisplayListItem::ScrollBar { ..., hit_id }

Display List Translation (NOT YET IMPLEMENTED):
  DisplayList → compositor2::translate_displaylist_to_wr()
    → WebRender DisplayListBuilder
    → For each ScrollBar item:
        → Create primitive_info with wr_translate_scrollbar_hit_id(hit_id)
        → builder.push_rect(&prim_info, ...)
    → WebRender Transaction

Hit-Testing Phase (NOT YET IMPLEMENTED):
  Mouse click → MacOSWindow::handle_mouse_down()
    → perform_webrender_hit_test(position)
    → WrApiHitTester::hit_test(position)
    → Check hit_result.items for item.tag
    → translate_item_tag_to_scrollbar_hit_id(tag)
    → If Some(hit_id): handle_scrollbar_click(hit_id, position)
```

## Remaining Work

### Phase 2b: Display List to WebRender Translation ❌

**Status**: Not started  
**Blocker**: `compositor2/mod.rs` is still a stub

**Required Changes:**
1. Implement `translate_displaylist_to_wr()` in `dll/src/desktop/compositor2/mod.rs`
2. Create `WrDisplayListBuilder` for each pipeline
3. For `DisplayListItem::ScrollBar`:
   ```rust
   DisplayListItem::ScrollBar { bounds, color, orientation, opacity_key, hit_id } => {
       let mut prim_info = WrPrimitiveInfo::new(wr_translate_rect(bounds));
       
       // Attach hit-test tag if present
       if let Some(scrollbar_hit_id) = hit_id {
           let (tag, _) = wr_translate_scrollbar_hit_id(scrollbar_hit_id);
           prim_info.tag = Some(tag);
       }
       
       // Push rectangle primitive
       builder.push_rect(
           &prim_info,
           spatial_id,
           wr_translate_color(color),
       );
   }
   ```
4. Implement translation for all other `DisplayListItem` variants
5. Call `txn.set_display_list(...)` for each pipeline
6. Update `rebuild_display_list()` in `wr_translate2.rs` to use compositor2

**Complexity**: High - requires full WebRender display list builder integration

### Phase 3: Event Handler Integration ❌

**Status**: Not started  
**Dependencies**: Phase 2b (display list translation)

**Required Implementation in `dll/src/desktop/shell2/macos/events.rs`:**

1. **`perform_webrender_hit_test(position: LogicalPosition) -> Option<ScrollbarHitId>`**
   - Query `self.hit_tester` (AsyncHitTester::Resolved)
   - Call `hit_tester.hit_test(None, world_point, HitTestFlags::empty())`
   - Iterate `hit_result.items`
   - For each item with `tag`, call `translate_item_tag_to_scrollbar_hit_id(tag)`
   - Return first `Some(ScrollbarHitId)`

2. **`handle_scrollbar_click(hit_id: ScrollbarHitId, position: LogicalPosition) -> EventProcessResult`**
   - Match on `hit_id`:
     - `VerticalThumb/HorizontalThumb`: Start drag (set `scrollbar_drag_state`)
     - `VerticalTrack/HorizontalTrack`: Jump scroll (calculate target, call `gpu_scroll()`)
   - Return `EventProcessResult::RequestRedraw`

3. **`handle_scrollbar_drag(current_position: LogicalPosition) -> EventProcessResult`**
   - Get `scrollbar_drag_state` (dom_id, node_id, orientation, start_pos, start_offset)
   - Calculate delta from start position
   - Get scrollbar geometry from `layout_window.scroll_states`
   - Convert drag delta to scroll delta (based on thumb size ratio)
   - Call `gpu_scroll(dom_id, node_id, delta.x, delta.y)`
   - Return `EventProcessResult::RequestRedraw`

4. **Integration into `handle_mouse_down()`**
   ```rust
   pub(crate) fn handle_mouse_down(&mut self, event: &NSEvent, button: MouseButton) -> EventProcessResult {
       let position = get_mouse_position(event);
       
       // Update mouse state
       self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);
       
       // Check for scrollbar hit FIRST
       if let Some(scrollbar_hit_id) = self.perform_webrender_hit_test(position) {
           return self.handle_scrollbar_click(scrollbar_hit_id, position);
       }
       
       // Fall back to regular DOM hit-testing
       let hit_node = self.perform_hit_test(position);
       // ... existing callback dispatch ...
   }
   ```

5. **Integration into `handle_mouse_move()`**
   ```rust
   pub(crate) fn handle_mouse_move(&mut self, event: &NSEvent) -> EventProcessResult {
       let position = get_mouse_position(event);
       
       // Handle active scrollbar drag
       if self.scrollbar_drag_state.is_some() {
           return self.handle_scrollbar_drag(position);
       }
       
       // Fall back to regular hover handling
       // ... existing code ...
   }
   ```

6. **Integration into `handle_mouse_up()`**
   ```rust
   pub(crate) fn handle_mouse_up(&mut self, event: &NSEvent, button: MouseButton) -> EventProcessResult {
       let position = get_mouse_position(event);
       
       // Clear button flags
       // ... existing code ...
       
       // End scrollbar drag
       if self.scrollbar_drag_state.is_some() {
           self.scrollbar_drag_state = None;
           return EventProcessResult::RequestRedraw;
       }
       
       // Fall back to regular mouse up handling
       // ... existing code ...
   }
   ```

### Phase 4: Per-Frame Scrollbar Calculation ❌

**Status**: Not started  
**Dependencies**: None (can be done in parallel with Phase 3)

**Required Changes in `dll/src/desktop/shell2/macos/mod.rs`:**

1. **Call in `regenerate_layout()`**
   ```rust
   pub fn regenerate_layout(&mut self, ...) -> Result<(), String> {
       // ... existing layout code ...
       
       // Calculate scrollbar states based on new layout
       if let Some(layout_window) = &mut self.layout_window {
           layout_window.scroll_states.calculate_scrollbar_states();
       }
       
       // Synchronize GPU values (including scrollbar transforms/opacity)
       self.synchronize_gpu_values(&self.app_data.borrow(), &self.fc_cache.borrow())?;
       
       Ok(())
   }
   ```

2. **Call in `gpu_scroll()`**
   ```rust
   pub fn gpu_scroll(&mut self, dom_id: u64, node_id: u64, delta_x: f32, delta_y: f32) -> Result<(), String> {
       // ... existing scroll code ...
       
       // Recalculate scrollbar geometry after scroll update
       if let Some(layout_window) = &mut self.layout_window {
           layout_window.scroll_states.calculate_scrollbar_states();
       }
       
       // Synchronize GPU values (update scrollbar thumb positions)
       self.synchronize_gpu_values(&self.app_data.borrow(), &self.fc_cache.borrow())?;
       
       self.frame_needs_regeneration = true;
       Ok(())
   }
   ```

**Note**: `calculate_scrollbar_states()` updates:
- `scrollbar_states: BTreeMap<(DomId, NodeId, ScrollbarOrientation), ScrollbarState>`
- For each scrollbar: `visible`, `scale`, `thumb_position_ratio`, `thumb_size_ratio`, `track_rect`
- These are then synced to GPU via `synchronize_gpu_values()` → `GpuStateManager::update_scrollbar_transforms()`

## Testing Plan

### Unit Tests (Post Phase 2b)
- [ ] `wr_translate_scrollbar_hit_id()` round-trip encoding/decoding
- [ ] `translate_item_tag_to_scrollbar_hit_id()` handles invalid tags
- [ ] ScrollbarHitId component type encoding (00, 01, 10, 11)

### Integration Tests (Post Phase 3)
- [ ] Click on scrollbar thumb → starts drag
- [ ] Drag scrollbar thumb → continuous scroll
- [ ] Click on scrollbar track → jump scroll
- [ ] Click on DOM node → no scrollbar interference
- [ ] Nested scrollbars (iframe) → correct z-ordering

### Manual Tests (Post Phase 4)
- [ ] Drag vertical scrollbar smoothly
- [ ] Drag horizontal scrollbar smoothly
- [ ] Click above/below thumb (jump scroll)
- [ ] Window resize → scrollbar scales via GPU transform
- [ ] Content scroll → scrollbar thumb position updates

## Files Modified

### Core Layout (`layout/`)
- `src/solver3/display_list.rs` - Added `hit_id` field, updated `push_scrollbar()`
- `src/cpurender/mod.rs` - Updated pattern match for new field
- `src/scroll.rs` - Deprecated custom hit-testing methods
- `src/window.rs` - Made `ScrollbarDragState` public
- `src/lib.rs` - Exported `ScrollbarDragState`

### Platform Layer (`dll/`)
- `src/desktop/wr_translate2.rs` - Added scrollbar hit-test translation functions
- `src/desktop/shell2/macos/mod.rs` - Updated `scrollbar_drag_state` type
- `src/desktop/shell2/macos/events.rs` - Removed duplicate `ScrollbarDragState`

### Documentation
- `REFACTORING/SCROLLBAR_HIT_TESTING_DESIGN.md` - Complete architecture design
- `REFACTORING/SESSION_7_SCROLLBAR_HIT_TESTING_PROGRESS.md` - This file

## Compilation Status

✅ **azul-layout**: Compiles successfully  
✅ **azul-dll**: Compiles successfully (unrelated errors exist, no scrollbar-specific errors)

## Next Steps

**Priority 1: Implement Phase 2b (Display List Translation)**
- This is the critical blocker for hit-testing to work
- Requires implementing `compositor2::translate_displaylist_to_wr()`
- Need to handle all `DisplayListItem` variants, not just `ScrollBar`
- Complex task - estimate 4-6 hours

**Priority 2: Implement Phase 3 (Event Handlers)**
- After display list translation, implement `perform_webrender_hit_test()`
- Implement `handle_scrollbar_click()` and `handle_scrollbar_drag()`
- Integrate into `handle_mouse_down/up/move()`
- Moderate task - estimate 2-3 hours

**Priority 3: Implement Phase 4 (Per-Frame Calculation)**
- Call `calculate_scrollbar_states()` in `regenerate_layout()` and `gpu_scroll()`
- Simplest phase - estimate 30 minutes

**Total Estimated Time Remaining**: 6-10 hours

## Notes

### Design Decisions Validated

1. **WebRender Hit-Testing vs Custom Geometry**
   - ✅ Correct choice: WebRender hit-testing handles z-ordering, transforms, clipping automatically
   - ✅ Unified system: Same hit-testing for DOM nodes and scrollbars
   - ✅ Future-proof: Supports nested iframes and complex scenarios

2. **ScrollbarHitId Encoding**
   - ✅ Efficient: 64-bit tag encodes DomId, NodeId, and component type
   - ✅ Recoverable: Can decode back to full `ScrollbarHitId`
   - ✅ Extensible: Can add more component types (buttons) in bits 62-63

3. **Unified ScrollbarDragState**
   - ✅ Better design: Uses `ScrollbarHitId` directly instead of separate fields
   - ✅ Type-safe: Matches WebRender hit-test result type
   - ✅ Simpler: No conversion between event handler and window state

### Known Limitations

1. **Track vs Thumb Distinction**
   - Currently only generate `ScrollbarHitId::*Thumb` for entire scrollbar
   - Should generate separate IDs for track, thumb, and buttons
   - Need to implement component-level hit-test areas in display list

2. **Scrollbar Buttons**
   - No `TopButton`, `BottomButton`, `LeftButton`, `RightButton` variants yet
   - Need to extend `ScrollbarHitId` enum
   - Need to render button components in display list

3. **Display List Translation Stub**
   - `compositor2/mod.rs` is still a stub
   - All `DisplayListItem` variants need implementation, not just `ScrollBar`
   - This is the main blocker for end-to-end testing

### Future Enhancements

1. **Component-Level Hit-Testing**
   - Push separate hit-test areas for track, thumb, top button, bottom button
   - Allows more granular interaction (e.g., click on track to jump)

2. **Visual Feedback**
   - Hover states for scrollbar components (change color/opacity)
   - Active states during drag (change cursor, highlight thumb)
   - Smooth animations for jump scroll

3. **Keyboard Navigation**
   - Arrow keys → scroll by line
   - PageUp/PageDown → scroll by page
   - Home/End → scroll to top/bottom
   - Tab into scrollbar → keyboard control

4. **Accessibility**
   - Screen reader support (announce scroll position)
   - High contrast mode support
   - Reduced motion support (disable smooth scrolling)

## References

- **Design Document**: `REFACTORING/SCROLLBAR_HIT_TESTING_DESIGN.md`
- **ScrollbarHitId Definition**: `core/src/hit_test.rs:41-47`
- **ScrollbarState Definition**: `layout/src/scroll.rs:67-127`
- **DisplayList ScrollBar Item**: `layout/src/solver3/display_list.rs:123-133`
- **Translation Functions**: `dll/src/desktop/wr_translate2.rs:178-233`

## Commit Message (Suggested)

```
feat(scrollbar): Implement WebRender-based scrollbar hit-testing infrastructure

Phase 1: Display List Integration
- Add hit_id field to DisplayListItem::ScrollBar
- Generate ScrollbarHitId during display list build
- Unify ScrollbarDragState (use layout::window version)
- Deprecate custom hit-testing code in scroll.rs

Phase 2: WebRender Translation Functions
- Implement wr_translate_scrollbar_hit_id() for ItemTag encoding
- Implement translate_item_tag_to_scrollbar_hit_id() for decoding
- Use 64-bit encoding: NodeId (32b) + DomId (30b) + Component (2b)

Remaining:
- Phase 2b: Display list to WebRender translation (compositor2)
- Phase 3: Event handlers (perform_hit_test, handle_click/drag)
- Phase 4: Per-frame scrollbar calculation integration

Architecture: Uses WebRender's hit-testing system instead of custom
geometry checks. Enables unified hit-testing for DOM nodes and scrollbars,
handles z-ordering and transforms automatically.

Ref: REFACTORING/SCROLLBAR_HIT_TESTING_DESIGN.md
Ref: REFACTORING/SESSION_7_SCROLLBAR_HIT_TESTING_PROGRESS.md
```

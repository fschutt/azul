# Text Selection System - Refactoring Summary

## What We Discovered

During implementation of the text selection system (Phases 1-4, 80% complete), we identified critical architectural issues that need fixing before continuing with remaining features.

### The Core Problem

**Current State**: Text selection was being implemented with **direct state mutation** and **event-based detection**, similar to how the old system worked.

**Problem**: This doesn't follow the **lazy-application changeset pattern** that the text input system uses, which means:
- ❌ No preventDefault support for selection/cursor operations
- ❌ Can't implement undo/redo properly
- ❌ Can't validate operations before applying
- ❌ Code duplication between cursor and selection scrolling
- ❌ No way to support drag-to-scroll with acceleration
- ❌ State mutations scattered throughout codebase

### The Insight

Looking at how `scroll_focused_cursor_into_view()` currently works, we realized:

1. **Cursor IS a selection** - just with 0 size (collapsed to insertion point)
2. **Scrolling should be unified** - one function handles both cursor and selection
3. **Acceleration is needed** - drag-to-scroll should go faster when mouse is further from edge
4. **State analysis, not events** - analyze manager state changes, not track event sequences
5. **Changesets are the answer** - same pattern as text input, but for ALL operations

## What We've Implemented (Keep)

### ✅ Phase 1: Click State Tracking
**File**: `layout/src/managers/selection.rs`
- `ClickState` struct with 500ms timeout, 5px distance
- Cycling click count (1→2→3→1)
- Works correctly - **keep as-is**

### ✅ Phase 2: Word/Paragraph Selection
**File**: `layout/src/text3/selection.rs`
- `select_word_at_cursor()` - Unicode word boundaries
- `select_paragraph_at_cursor()` - Line detection
- 5 unit tests, all passing
- Works correctly - **keep as-is**

### ✅ Phase 3: Event Loop Integration (Hooks)
**File**: `dll/src/desktop/shell2/common/event_v2.rs`
- Pre/post callback filter hooks in place
- Infrastructure for system events
- Works correctly - **refactor internals, keep structure**

### ✅ Phase 4: Clipboard Content Extraction
**Files**: `layout/src/managers/selection.rs`, `layout/src/window.rs`
- `ClipboardContent` with styled runs
- `get_clipboard_content()` extraction
- `to_html()` conversion
- Works correctly - **keep as-is**

## What We Need to Refactor

### 🔄 Problem 1: Manual Scroll Calls

**Current** (`layout/src/window.rs:1417-1556`):
```rust
fn scroll_focused_cursor_into_view(&mut self) {
    let cursor_rect = self.get_focused_cursor_rect()?;
    // ... manual scroll logic
}
```

**Issues**:
- Only handles cursor (not selection)
- Called manually from event loop
- No acceleration for drag-to-scroll
- Code duplication if we add selection scrolling

**Solution**: Unified scroll system
```rust
fn scroll_selection_into_view(
    &mut self,
    scroll_type: SelectionScrollType,  // Cursor / Selection / DragSelection
    scroll_mode: ScrollMode,            // Instant / Accelerated
) -> bool {
    // Unified logic for cursor (0-size) and selection
    // Distance-based acceleration for drag-to-scroll
    // Called from post-callback event processing
}
```

### 🔄 Problem 2: Direct State Mutation

**Current** (`layout/src/window.rs:3473-3577`):
```rust
pub fn process_mouse_click_for_selection(&mut self, ...) {
    // Directly mutates SelectionManager
    self.selection_manager.set_selection(range);  // WRONG
}
```

**Issues**:
- No preventDefault support
- Can't inspect changes before applying
- Can't implement undo/redo
- Can't validate operations

**Solution**: Changeset system
```rust
// Phase 1: Create changeset (pre-callback)
let changeset = TextChangeset {
    operation: TextOperation::SetSelection {
        old_range: current,
        new_range: word_range,
    },
};

// Phase 2: User callbacks (can preventDefault)
let prevent = run_callbacks(&changesets);

// Phase 3: Apply if not prevented (post-callback)
if !prevent {
    apply_changeset(changeset);
}
```

### 🔄 Problem 3: Event-Based Detection

**Current** (`core/src/events.rs:2535-2674`):
```rust
pub fn pre_callback_filter_internal_events(
    events: &[SyntheticEvent],
    hit_test: Option<&FullHitTest>,
    click_count: u8,
    mouse_down: bool,  // Local variable
    drag_start_position: Option<LogicalPosition>,  // Local variable
    focused_node: Option<DomNodeId>,
) -> PreCallbackFilterResult {
    // Tracks event sequences with local variables
    for event in events {
        match event.event_type {
            EventType::MouseDown => { ... }
            EventType::MouseMove if mouse_down => { ... }
        }
    }
}
```

**Issues**:
- Tracks event sequences (brittle)
- Uses local variables (not manager state)
- Hardcoded key_code checks (should use VirtualKeyCode)
- Can't analyze state transitions

**Solution**: State-based analysis
```rust
pub fn pre_callback_filter_internal_events(
    events: &[SyntheticEvent],
    hit_test: Option<&FullHitTest>,
    keyboard_state: &KeyboardState,       // Use VirtualKeyCode
    mouse_state: &MouseState,             // Use left_button_down
    selection_manager: &SelectionManager, // Use click_state, drag_start
    focus_manager: &FocusManager,
    scroll_manager: &ScrollManager,
) -> PreCallbackFilterResult {
    // Analyze state transitions
    if mouse_state.left_button_down {
        if let Some(drag_start) = selection_manager.drag_start_position {
            // Drag detected from state, not event sequence
        }
    }
    
    // Use VirtualKeyCode (already exists!)
    if keyboard_state.ctrl_down() {
        match keyboard_state.current_virtual_keycode {
            Some(VirtualKeyCode::C) => { /* Copy shortcut */ }
            Some(VirtualKeyCode::Left) => { /* Word jump */ }
            // ...
        }
    }
}
```

### 🔄 Problem 4: Hardcoded Post-Processing

**Current** (`dll/src/desktop/shell2/common/event_v2.rs:1100-1250`):
```rust
if should_apply_text_input {
    apply_text_changeset();
    scroll_focused_cursor_into_view();  // Always called!
}
```

**Issues**:
- Scroll called unconditionally
- No analysis of what actually changed
- No support for drag-to-scroll timing

**Solution**: State-based post-filter
```rust
pub fn post_callback_filter_internal_events(
    prevent_default: bool,
    changesets: &[TextChangeset],
    layout_window: &mut LayoutWindow,
) -> PostCallbackFilterResult {
    if !prevent_default {
        apply_changesets(changesets, layout_window);
        
        // Analyze what changed
        for changeset in changesets {
            match changeset.operation {
                TextOperation::MoveCursor { new_position, .. } => {
                    if !is_visible(new_position) {
                        return PostCallbackSystemEvent::ScrollSelectionIntoView {
                            scroll_type: SelectionScrollType::Cursor,
                            scroll_mode: ScrollMode::Instant,
                        };
                    }
                }
                // ...
            }
        }
    }
}
```

## Refactoring Plan (Detailed)

See `TEXT_SELECTION_CHANGESET_ARCHITECTURE.md` for complete details.

### Phase 1: Unified Scroll System ⏱️ 4-6 hours

**Goal**: One function handles cursor, selection, and drag-to-scroll

**Tasks**:
1. Implement `scroll_selection_into_view()` with `SelectionScrollType` enum
2. Add `calculate_selection_bounding_rect()` method
3. Implement `calculate_edge_distance()` for acceleration
4. Implement `calculate_accelerated_scroll_delta()` with zones:
   - 0-20px: Dead zone (no scroll)
   - 20-50px: Slow (1x)
   - 50-100px: Medium (2x)
   - 100-200px: Fast (4x)
   - 200+px: Very fast (8x)
5. Add `ScrollMode::Instant` and `ScrollMode::Accelerated`
6. Replace all `scroll_focused_cursor_into_view()` calls
7. Add unit tests for acceleration zones

**Files**:
- `layout/src/window.rs` (main implementation)
- `dll/src/desktop/shell2/common/event_v2.rs` (update calls)

### Phase 2: Complete Changeset System ⏱️ 6-8 hours

**Goal**: All operations produce changesets (create → inspect → apply)

**Tasks**:
1. Create `layout/src/managers/changeset.rs` module
2. Define `TextChangeset` struct
3. Define `TextOperation` enum with 15+ variants:
   - Text mutations: InsertText, DeleteText, ReplaceText
   - Selection mutations: SetSelection, ExtendSelection, ClearSelection
   - Cursor mutations: MoveCursor
   - Clipboard: Copy, Cut, Paste
4. Implement `create_changesets_from_system_events()`
5. Implement `apply_changesets()` with validation
6. Add unit tests for each operation type
7. Add integration tests for preventDefault

**Files**:
- `layout/src/managers/changeset.rs` (new file)
- `layout/src/managers/mod.rs` (add module)

### Phase 3: State-Based Pre-Filter ⏱️ 4-6 hours

**Goal**: Analyze manager state, not event sequences

**Tasks**:
1. Update `pre_callback_filter_internal_events()` signature:
   - Add `keyboard_state: &KeyboardState`
   - Add `mouse_state: &MouseState`
   - Add `selection_manager: &SelectionManager`
   - Add `focus_manager: &FocusManager`
   - Add `scroll_manager: &ScrollManager`
2. Remove hardcoded key_code checks
3. Use `VirtualKeyCode::Left/Right/Up/Down` for arrows
4. Use `VirtualKeyCode::C/X/V/A/Z` for shortcuts
5. Detect drag from `mouse_state.left_button_down` + selection state
6. Detect click from `selection_manager.click_state`
7. Add unit tests for state-based detection

**Files**:
- `core/src/events.rs` (update function)
- `dll/src/desktop/shell2/common/event_v2.rs` (pass state to filter)

### Phase 4: State-Based Post-Filter ⏱️ 3-4 hours

**Goal**: Analyze applied changesets, not hardcoded logic

**Tasks**:
1. Update `post_callback_filter_internal_events()` signature:
   - Add `changesets: &[TextChangeset]`
   - Add `layout_window: &mut LayoutWindow`
2. Analyze changesets to determine scroll needs
3. Generate `ScrollSelectionIntoView` events (not manual calls)
4. Detect drag-to-scroll conditions
5. Generate `StartAutoScrollTimer` / `CancelAutoScrollTimer`
6. Add unit tests for state analysis

**Files**:
- `core/src/events.rs` (update function)
- `dll/src/desktop/shell2/common/event_v2.rs` (process events)

### Phase 5: Auto-Scroll Timer System ⏱️ 4-6 hours

**Goal**: Continuous scrolling during drag operations

**Tasks**:
1. Implement timer that fires at 60fps
2. Check if still dragging on each tick
3. Call `scroll_selection_into_view()` with `ScrollMode::Accelerated`
4. Update selection to current mouse position
5. Cancel timer on mouse up or mouse returns to container
6. Add performance tests (ensure 60fps maintained)

**Files**:
- `dll/src/desktop/shell2/common/event_v2.rs` (timer implementation)
- Platform-specific timer APIs (macOS/Windows/X11/Wayland)

**Total Estimated Time**: 21-30 hours (3-4 days)

## After Refactoring

Once the architecture is solid, we can continue with:

### Phase 6: Selection Rendering
- Visual feedback in display list
- Selection highlight rectangles
- Cursor caret rendering

### Phase 7: Keyboard Shortcuts
- All shortcuts through changeset system
- Ctrl+C/X/V/A/Z/Y implementation
- Undo/redo stack integration

### Phase 8: Platform Clipboard
- macOS NSPasteboard
- Windows Clipboard API
- X11 XClipboard
- Wayland wl_data_device

### Phase 9: Test Suite
- Integration tests for all features
- Performance benchmarks
- Edge case coverage

## Key Decisions Made

### ✅ Use VirtualKeyCode (Don't Reinvent)
- Already exists in `KeyboardState::current_virtual_keycode`
- Has all needed keys: Left/Right/Up/Down, A/C/X/V/Z
- Use `keyboard_state.ctrl_down()` + `current_virtual_keycode`

### ✅ Cursor = 0-Size Selection
- Unifies scrolling logic
- Reduces code duplication
- Matches browser behavior

### ✅ State-Based Analysis
- More robust than event tracking
- Easier to test
- Clearer code flow

### ✅ Two-Phase Changesets
- Create (pre-callback) → Apply (post-callback)
- Enables preventDefault for everything
- Enables undo/redo
- Enables validation

### ✅ Distance-Based Acceleration
- Common UI pattern (VSCode, browsers)
- Improves UX for drag-to-scroll
- Easy to implement with zones

## Next Steps

1. **Review architecture document**: Read `TEXT_SELECTION_CHANGESET_ARCHITECTURE.md` in full
2. **Start with Phase 1**: Implement unified scroll system
3. **Test incrementally**: Don't move to next phase until current works
4. **Update as you go**: Keep documentation synchronized with code
5. **Get feedback**: Test with real use cases after each phase

## Success Criteria

### Functional
- [ ] All operations use changesets (no direct mutation)
- [ ] preventDefault works for all operations
- [ ] Cursor and selection scrolling unified
- [ ] Drag-to-scroll with acceleration works
- [ ] Auto-scroll activates/cancels correctly

### Performance
- [ ] Pre-filter O(1) state analysis
- [ ] Post-filter O(n) changeset analysis
- [ ] Auto-scroll maintains 60fps
- [ ] No frame drops during drag operations

### Code Quality
- [ ] No code duplication
- [ ] All operations testable
- [ ] Clear separation of concerns
- [ ] >80% test coverage for new code

---

**Current Status**: Architecture documented, ready to begin refactoring

**Estimated Completion**: 3-4 days of focused work

**Confidence**: High - pattern proven with text input system

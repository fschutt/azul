# Text Selection Integration Guide

This document describes how the text selection system integrates with existing cursor navigation and provides convenience APIs for callbacks.

## 1. UnifiedLayout Cursor Navigation (Already Implemented)

The `UnifiedLayout` in `layout/src/text3/cache.rs` already has comprehensive cursor navigation:

### Hit Testing (Unified)
```rust
// Single unified implementation (hittest_cursor replaces hit_test_to_cursor)
pub fn hittest_cursor(&self, point: LogicalPosition) -> Option<TextCursor>

// Deprecated: Use hittest_cursor instead
#[deprecated(since = "0.1.0", note = "Use hittest_cursor instead")]
pub fn hit_test_to_cursor(&self, local_pos: LogicalPosition) -> Option<TextCursor>
```

**Algorithm:**
1. Find closest cluster vertically and horizontally (combined distance)
2. Prioritize vertical proximity (2x weight)
3. Determine Leading/Trailing affinity based on which half was clicked
4. Handle non-cluster items (objects, combined blocks) as single-character clusters

### Cursor Movement (Already Implemented)
```rust
// Navigate cursor left/right (handles line wrapping, Bidi)
pub fn move_cursor_left(&self, cursor: TextCursor, debug: &mut Option<Vec<String>>) -> TextCursor
pub fn move_cursor_right(&self, cursor: TextCursor, debug: &mut Option<Vec<String>>) -> TextCursor

// Navigate cursor up/down (preserves horizontal column via goal_x)
pub fn move_cursor_up(&self, cursor: TextCursor, goal_x: &mut Option<f32>, debug: &mut Option<Vec<String>>) -> TextCursor
pub fn move_cursor_down(&self, cursor: TextCursor, goal_x: &mut Option<f32>, debug: &mut Option<Vec<String>>) -> TextCursor

// Navigate to line boundaries
pub fn move_cursor_to_line_start(&self, cursor: TextCursor, debug: &mut Option<Vec<String>>) -> TextCursor
pub fn move_cursor_to_line_end(&self, cursor: TextCursor, debug: &mut Option<Vec<String>>) -> TextCursor
```

### Selection Rectangles (Already Implemented)
```rust
// Get visual rectangles covering a selection range
pub fn get_selection_rects(&self, range: &SelectionRange) -> Vec<LogicalRect>

// Get cursor caret rectangle
pub fn get_cursor_rect(&self, cursor: &TextCursor) -> Option<LogicalRect>
```

## 2. SelectionManager Enhancement

Located in `layout/src/managers/selection.rs`, now provides comprehensive selection management:

### Data Structure
```rust
pub struct SelectionManager {
    // Maps DomId -> SelectionState
    pub selections: BTreeMap<DomId, SelectionState>,
}

// In azul-core/src/selection.rs:
pub struct SelectionState {
    pub selections: Vec<Selection>,  // Supports multi-cursor!
    pub node_id: DomNodeId,
}

pub enum Selection {
    Cursor(TextCursor),
    Range(SelectionRange),
}
```

### Core Methods
```rust
// Basic access
pub fn get_selection(&self, dom_id: &DomId) -> Option<&SelectionState>
pub fn get_selection_mut(&mut self, dom_id: &DomId) -> Option<&mut SelectionState>
pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState)

// Convenient setters (replace all selections)
pub fn set_cursor(&mut self, dom_id: DomId, node_id: DomNodeId, cursor: TextCursor)
pub fn set_range(&mut self, dom_id: DomId, node_id: DomNodeId, range: SelectionRange)

// Multi-cursor support (add to existing selections)
pub fn add_selection(&mut self, dom_id: DomId, node_id: DomNodeId, selection: Selection)

// Queries
pub fn has_selection(&self, dom_id: &DomId) -> bool
pub fn get_primary_cursor(&self, dom_id: &DomId) -> Option<TextCursor>
pub fn get_ranges(&self, dom_id: &DomId) -> Vec<SelectionRange>

// Clear operations
pub fn clear_selection(&mut self, dom_id: &DomId)
pub fn clear_all(&mut self)
```

## 3. CallbackInfo API Extensions

Located in `layout/src/callbacks.rs`, provides callback access to selection and text layout:

### Selection Access
```rust
// Get current selection state
pub fn get_selection(&self, dom_id: &DomId) -> Option<&SelectionState>

// Set selection (replace all)
pub fn set_cursor(&mut self, dom_id: DomId, node_id: DomNodeId, cursor: TextCursor)
pub fn set_selection_range(&mut self, dom_id: DomId, node_id: DomNodeId, range: SelectionRange)

// Multi-cursor support
pub fn add_selection(&mut self, dom_id: DomId, node_id: DomNodeId, selection: Selection)

// Queries
pub fn has_selection(&self, dom_id: &DomId) -> bool
pub fn get_primary_cursor(&self, dom_id: &DomId) -> Option<TextCursor>
pub fn get_selection_ranges(&self, dom_id: &DomId) -> Vec<SelectionRange>

// Clear operations
pub fn clear_selection(&mut self, dom_id: &DomId)
pub fn clear_all_selections(&mut self)
```

### Text Layout Access
```rust
// Get UnifiedLayout for text operations
pub fn get_text_layout(
    &self,
    dom_id: &DomId,
    node_id: DomNodeId
) -> Option<&UnifiedLayout<ParsedFont>>
```

## 4. Usage Examples

### Example 1: Mouse Click → Cursor Placement
```rust
fn on_mouse_down(data: &mut MyData, info: &mut CallbackInfo) -> Update {
    let dom_id = info.get_hit_node().dom;
    let node_id = info.get_hit_node();
    
    // Get mouse position relative to node
    let mouse_pos = info.get_current_mouse_state().mouse_position;
    let node_rect = info.get_node_rect(node_id)?;
    let local_pos = LogicalPosition {
        x: mouse_pos.x - node_rect.origin.x,
        y: mouse_pos.y - node_rect.origin.y,
    };
    
    // Get text layout and hit test
    let layout = info.get_text_layout(&dom_id, node_id)?;
    let cursor = layout.hittest_cursor(local_pos)?;
    
    // Set cursor position
    info.set_cursor(dom_id, node_id, cursor);
    
    Update::RefreshDom
}
```

### Example 2: Arrow Key Navigation
```rust
fn on_key_down(data: &mut MyData, info: &mut CallbackInfo) -> Update {
    let key = info.get_current_keyboard_state().current_char?;
    let dom_id = info.get_hit_node().dom;
    let node_id = info.get_hit_node();
    
    // Get current cursor
    let cursor = info.get_primary_cursor(&dom_id)?;
    
    // Get text layout
    let layout = info.get_text_layout(&dom_id, node_id)?;
    
    // Navigate based on key
    let new_cursor = match key {
        VirtualKeyCode::Left => layout.move_cursor_left(cursor, &mut None),
        VirtualKeyCode::Right => layout.move_cursor_right(cursor, &mut None),
        VirtualKeyCode::Up => {
            let mut goal_x = None;
            layout.move_cursor_up(cursor, &mut goal_x, &mut None)
        },
        VirtualKeyCode::Down => {
            let mut goal_x = None;
            layout.move_cursor_down(cursor, &mut goal_x, &mut None)
        },
        VirtualKeyCode::Home => layout.move_cursor_to_line_start(cursor, &mut None),
        VirtualKeyCode::End => layout.move_cursor_to_line_end(cursor, &mut None),
        _ => return Update::DoNothing,
    };
    
    // Update cursor
    info.set_cursor(dom_id, node_id, new_cursor);
    
    Update::RefreshDom
}
```

### Example 3: Drag Selection
```rust
struct MyData {
    drag_start: Option<(DomId, DomNodeId, TextCursor)>,
}

fn on_mouse_down(data: &mut MyData, info: &mut CallbackInfo) -> Update {
    let dom_id = info.get_hit_node().dom;
    let node_id = info.get_hit_node();
    let layout = info.get_text_layout(&dom_id, node_id)?;
    
    let mouse_pos = /* calculate local position */;
    let cursor = layout.hittest_cursor(mouse_pos)?;
    
    // Start drag
    data.drag_start = Some((dom_id, node_id, cursor));
    info.set_cursor(dom_id, node_id, cursor);
    
    Update::RefreshDom
}

fn on_mouse_move(data: &mut MyData, info: &mut CallbackInfo) -> Update {
    let Some((dom_id, node_id, start_cursor)) = data.drag_start else {
        return Update::DoNothing;
    };
    
    let layout = info.get_text_layout(&dom_id, node_id)?;
    let mouse_pos = /* calculate local position */;
    let end_cursor = layout.hittest_cursor(mouse_pos)?;
    
    // Create selection range
    let range = SelectionRange {
        start: start_cursor,
        end: end_cursor,
    };
    
    info.set_selection_range(dom_id, node_id, range);
    
    Update::RefreshDom
}

fn on_mouse_up(data: &mut MyData, info: &mut CallbackInfo) -> Update {
    data.drag_start = None;
    Update::DoNothing
}
```

### Example 4: Multi-Cursor (Future)
```rust
fn on_ctrl_click(data: &mut MyData, info: &mut CallbackInfo) -> Update {
    let dom_id = info.get_hit_node().dom;
    let node_id = info.get_hit_node();
    let layout = info.get_text_layout(&dom_id, node_id)?;
    
    let mouse_pos = /* calculate local position */;
    let cursor = layout.hittest_cursor(mouse_pos)?;
    
    // Add additional cursor (multi-cursor support)
    info.add_selection(dom_id, node_id, Selection::Cursor(cursor));
    
    Update::RefreshDom
}
```

### Example 5: Get Selection Rectangles for Rendering
```rust
fn on_render(data: &MyData, info: &CallbackInfo) -> Update {
    let dom_id = &data.dom_id;
    let node_id = data.node_id;
    
    // Get selection ranges
    let ranges = info.get_selection_ranges(dom_id);
    
    // Get text layout
    let layout = info.get_text_layout(dom_id, node_id)?;
    
    // Get visual rectangles for each range
    for range in ranges {
        let rects = layout.get_selection_rects(&range);
        
        // Render selection highlight rectangles
        for rect in rects {
            // Draw blue highlight at rect position
            render_highlight(rect, Color::BLUE.with_alpha(0.3));
        }
    }
    
    // Get cursor rectangle
    if let Some(cursor) = info.get_primary_cursor(dom_id) {
        if let Some(cursor_rect) = layout.get_cursor_rect(&cursor) {
            // Render cursor caret
            render_cursor(cursor_rect, Color::BLACK);
        }
    }
    
    Update::DoNothing
}
```

## 5. Key Design Decisions

### 5.1 Unified Hit Testing
- **Old**: Two methods `hittest_cursor()` and `hit_test_to_cursor()` with different implementations
- **New**: Single `hittest_cursor()` method, `hit_test_to_cursor()` marked deprecated
- **Benefit**: Eliminates code duplication, single source of truth

### 5.2 Selection State Structure
```rust
// BTreeMap<DomId, SelectionState> where:
SelectionState {
    selections: Vec<Selection>,  // Ordered list for multi-cursor
    node_id: DomNodeId,          // Which node this applies to
}
```
- Supports future multi-cursor (Vec instead of single selection)
- Tracks which node selections belong to
- One selection state per DOM (not per node)

### 5.3 CallbackInfo Integration
- **Read Access**: All selection queries available
- **Write Access**: Convenient setters (cursor, range, multi-cursor)
- **Text Layout Access**: Direct access to UnifiedLayout for navigation
- **No Direct Manager Access**: Callbacks work through CallbackInfo API

### 5.4 Cursor Navigation Preservation
- All existing cursor navigation methods unchanged
- `debug: &mut Option<Vec<String>>` parameter for debugging cursor movement
- `goal_x: &mut Option<f32>` parameter for up/down navigation (preserves column)

## 6. Integration Checklist

### Completed ✅
- [x] Unified `hittest_cursor()` implementation
- [x] Deprecated duplicate `hit_test_to_cursor()`
- [x] Enhanced `SelectionManager` with multi-cursor support
- [x] Added selection APIs to `CallbackInfo`
- [x] Added text layout access to `CallbackInfo`
- [x] Documented cursor navigation (already implemented)
- [x] Created integration guide with examples
- [x] **Implement click state tracking for multi-click detection** (ClickState in SelectionManager)
- [x] **Add double/triple-click selection logic** (select_word_at_cursor, select_paragraph_at_cursor in text3/selection.rs)
- [x] **Implement clipboard integration** (ClipboardContent, StyledTextRun, get_clipboard_content() in window.rs)
- [x] **Event loop integration** (pre/post callback filters with system events)
- [x] **Mouse click processing** (process_mouse_click_for_selection() in window.rs)

### TODO ❌
- [ ] Add `text_layout_cache` field to `DomLayoutResult` (for `get_text_layout()`)
- [ ] **Implement selection rendering in display list** (push_selection_highlight, push_cursor_rect)
- [ ] **Add keyboard shortcut handlers** (Ctrl+C/X/A in event system)
- [ ] **Arrow key navigation integration** (Ctrl+Left/Right word jumps, Shift+arrows for selection)
- [ ] Add selection change callbacks (On::SelectionChange)
- [ ] **Create comprehensive test suite for clipboard extraction**
- [ ] **Platform clipboard integration** (actual copy/paste system calls)

## 7. Current Implementation Status

### ✅ Phase 1: Click State Tracking - **COMPLETE**
**Location**: `layout/src/managers/selection.rs` (lines 14-38)

```rust
pub struct ClickState {
    pub last_node: Option<DomNodeId>,
    pub last_position: LogicalPosition,
    pub last_time_ms: u64,
    pub click_count: u8,
}

impl SelectionManager {
    const MULTI_CLICK_TIMEOUT_MS: u64 = 500;
    const MULTI_CLICK_DISTANCE_PX: f32 = 5.0;
    
    pub fn update_click_count(&mut self, ...) -> u8 {
        // Implemented with timeout/distance/node validation
    }
    
    pub fn analyze_click_for_selection(&self, ...) -> Option<u8> {
        // Analyzes click patterns for event system
    }
}
```

### ✅ Phase 2: Multi-Click Selection - **COMPLETE**
**Location**: `layout/src/text3/selection.rs`

```rust
pub fn select_word_at_cursor(
    layout: &UnifiedLayout,
    cursor: TextCursor,
) -> SelectionRange {
    // Unicode word boundary detection
    // Handles alphanumeric + underscore
    // 5 passing unit tests
}

pub fn select_paragraph_at_cursor(
    layout: &UnifiedLayout,
    cursor: TextCursor,
) -> SelectionRange {
    // Line-based paragraph detection
    // Uses PositionedItem.line_index
}
```

**Tests**: 5 unit tests in `selection.rs` - all passing ✅

### ✅ Phase 3: Event Loop Integration - **COMPLETE**
**Location**: `dll/src/desktop/shell2/common/event_v2.rs` (lines 920-1088)

- Pre-callback filter extracts internal system events (SystemTextSingleClick/Double/Triple)
- Internal events processed BEFORE user callbacks
- Post-callback filter respects `preventDefault()` from user code
- Uses `AzInstant.duration_since()` with `ExternalSystemCallbacks::rust_internal`
- Extensible architecture for future framework functionality

### ✅ Phase 4: Clipboard Content Extraction - **COMPLETE**
**Location**: `layout/src/managers/selection.rs` (lines 286-350), `layout/src/window.rs` (lines 3584-3683)

```rust
pub struct StyledTextRun {
    pub text: String,
    pub font_family: Option<String>,
    pub font_size_px: f32,
    pub color: ColorU,
    pub is_bold: bool,
    pub is_italic: bool,
}

pub struct ClipboardContent {
    pub plain_text: String,
    pub styled_runs: Vec<StyledTextRun>,
}

impl ClipboardContent {
    pub fn to_html(&self) -> String {
        // Converts to HTML with inline CSS
    }
}

impl LayoutWindow {
    pub fn get_clipboard_content(&self, dom_id: &DomId) -> Option<ClipboardContent> {
        // Extracts text + styling from SelectionRanges
        // Iterates ShapedClusters, extracts text + style
        // Preserves font, size, color, bold/italic
    }
}
```

## 8. Next Implementation Steps

### Phase 5: Selection Rendering in Display List (Next Priority)
**Target**: `layout/src/solver3/display_list.rs`

```rust
impl DisplayList {
    pub fn push_selection_highlight(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,  // e.g., rgba(0, 120, 215, 0.3)
    ) {
        // Render selection background rectangle
    }
    
    pub fn push_cursor_rect(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,  // e.g., black
    ) {
        // Render cursor caret line
    }
}

// In display list generation loop:
if let Some(selection_state) = window.selection_manager.get_selection(&dom_id) {
    for selection in &selection_state.selections {
        match selection {
            Selection::Range(range) => {
                if let Some(layout) = text_cache.get_layout(&cache_id) {
                    let rects = layout.get_selection_rects(range);
                    for rect in rects {
                        display_list.push_selection_highlight(
                            rect,
                            ColorU::new(0, 120, 215, 76)  // Blue @ 30% opacity
                        );
                    }
                }
            }
            Selection::Cursor(cursor) => {
                if let Some(layout) = text_cache.get_layout(&cache_id) {
                    if let Some(rect) = layout.get_cursor_rect(cursor) {
                        display_list.push_cursor_rect(
                            rect,
                            ColorU::BLACK
                        );
                    }
                }
            }
        }
    }
}
```

### Phase 6: Keyboard Shortcuts (Following)
**Target**: Event system pre-callback filter

```rust
// In pre_callback_filter_internal_events():
// Detect Ctrl+C/X/A before user callbacks
if keyboard_event && modifiers.ctrl {
    match key {
        'c' | 'C' => {
            // Extract clipboard content
            if let Some(content) = window.get_clipboard_content(&dom_id) {
                println!("Copy: {}", content.plain_text);
                // TODO: Platform clipboard integration
            }
            internal_events.push(InternalSystemEvent::Copy);
        }
        'x' | 'X' => {
            // Extract + clear selection
            println!("Cut: ...");
            internal_events.push(InternalSystemEvent::Cut);
        }
        'a' | 'A' => {
            // Select all text in focused element
            internal_events.push(InternalSystemEvent::SelectAll);
        }
        _ => {}
    }
}

// Arrow keys with Shift for selection extension
if keyboard_event && modifiers.shift {
    match key {
        VirtualKeyCode::Left | Right | Up | Down => {
            internal_events.push(InternalSystemEvent::ExtendSelection(direction));
        }
        _ => {}
    }
}
```

### Phase 7: Platform Clipboard Integration
**Platforms**: macOS (NSPasteboard), Windows (Clipboard API), X11 (XClipboard), Wayland (wl_data_device)

```rust
// Platform-specific clipboard writing
#[cfg(target_os = "macos")]
fn write_to_clipboard(content: &ClipboardContent) {
    // NSPasteboard.general.clearContents()
    // NSPasteboard.general.setString(_:forType:.string)
    // NSPasteboard.general.setString(_:forType:.html)
}

#[cfg(target_os = "windows")]
fn write_to_clipboard(content: &ClipboardContent) {
    // OpenClipboard, EmptyClipboard
    // SetClipboardData(CF_UNICODETEXT, ...)
    // SetClipboardData(CF_HTML, ...)
}
```

## 9. Testing Status

### Unit Tests - **5 Passing** ✅
**Location**: `layout/src/text3/selection.rs` (lines 209-289)

```
test text3::selection::tests::test_is_word_char ... ok
test text3::selection::tests::test_word_boundaries_simple ... ok
test text3::selection::tests::test_word_boundaries_punctuation ... ok
test text3::selection::tests::test_word_boundaries_underscore ... ok
test text3::selection::tests::test_word_boundaries_start_end ... ok
```

### TODO: Integration Tests ❌
- [ ] Test clipboard extraction with complex styling (bold, italic, colors)
- [ ] Test multi-range selection extraction
- [ ] Test HTML generation from styled runs
- [ ] Test click state timeout/distance validation
- [ ] Test double/triple-click selection boundaries
- [ ] Test selection rendering in display list
- [ ] Test keyboard shortcuts (Ctrl+C/X/A)
- [ ] Test arrow key navigation with selection extension

## 10. Next Steps Summary

### Immediate (This Week)
1. ✅ ~~Clipboard content extraction~~ - **COMPLETE**
2. **Selection rendering in display list** - Ready to implement
3. **Add clipboard extraction tests** - Create test suite

### Short Term (Next Week)
4. **Keyboard shortcut handlers** - Integrate Ctrl+C/X/A
5. **Arrow key selection extension** - Shift+arrows
6. **Platform clipboard integration** - System copy/paste

### Medium Term
7. Selection change callbacks (`On::SelectionChange`)
8. `user-select` CSS property support
9. Drag-to-select refinements
10. Multi-cursor UI/UX improvements

## 11. API Summary

### ✅ UnifiedLayout (cache.rs) - ALL IMPLEMENTED
- `hittest_cursor(point)` → Convert position to cursor ✅
- `move_cursor_left/right/up/down(cursor)` → Navigate cursor ✅
- `move_cursor_to_line_start/end(cursor)` → Jump to line boundaries ✅
- `get_selection_rects(range)` → Get visual rectangles ✅
- `get_cursor_rect(cursor)` → Get caret rectangle ✅

### ✅ SelectionManager (managers/selection.rs) - ALL IMPLEMENTED
- `set_cursor(dom, node, cursor)` → Set single cursor ✅
- `set_range(dom, node, range)` → Set selection range ✅
- `add_selection(dom, node, selection)` → Multi-cursor support ✅
- `get_primary_cursor(dom)` → Get main cursor ✅
- `get_ranges(dom)` → Get all ranges ✅
- `has_selection(dom)` → Check if selected ✅
- `clear_selection(dom)` → Clear one DOM ✅
- `update_click_count(node, pos, time)` → Multi-click detection ✅
- `analyze_click_for_selection(node, pos, time)` → Event system analysis ✅

### ✅ LayoutWindow (window.rs) - CORE METHODS IMPLEMENTED
- `process_mouse_click_for_selection(pos, time)` → Process clicks for selection ✅
- `get_clipboard_content(dom)` → Extract styled text ✅

### ✅ ClipboardContent (managers/selection.rs) - COMPLETE
- `plain_text: String` → Plain UTF-8 text ✅
- `styled_runs: Vec<StyledTextRun>` → Rich text with styling ✅
- `to_html()` → Convert to HTML with inline CSS ✅

### ✅ CallbackInfo (callbacks.rs) - ALL PROXIED
- All SelectionManager methods proxied ✅
- `get_text_layout(dom, node)` → Access UnifiedLayout ✅
- `get_selection(dom)` → Read selection state ✅
- Selection manipulation through manager API ✅

### 🔄 Event System Integration - PARTIALLY COMPLETE
- ✅ Pre-callback filter (extracts internal events)
- ✅ Post-callback filter (respects preventDefault)
- ✅ System internal events (SystemTextSingleClick/Double/Triple)
- ❌ Keyboard shortcuts (Ctrl+C/X/A) - TODO
- ❌ Arrow key selection extension (Shift+arrows) - TODO

### ❌ Display List Rendering - NOT YET IMPLEMENTED
- ❌ `push_selection_highlight(rect, color)` - TODO
- ❌ `push_cursor_rect(rect, color)` - TODO  
- ❌ Selection rendering loop in display list generation - TODO

### ❌ Platform Integration - NOT YET IMPLEMENTED
- ❌ macOS NSPasteboard - TODO
- ❌ Windows Clipboard API - TODO
- ❌ X11 XClipboard - TODO
- ❌ Wayland wl_data_device - TODO

## 12. Summary

**Current Status**: **Text selection needs major architecture refactoring (80% complete but needs redesign)**

---

### ⚠️ CRITICAL: Architecture Refactoring Required

**See: `TEXT_SELECTION_CHANGESET_ARCHITECTURE.md` for complete refactoring plan**

The current 80% implementation has architectural issues that need fixing:

**Current Problems**:
1. ❌ Selection operations mutate state directly (no changeset system)
2. ❌ No preventDefault support for selection/cursor operations  
3. ❌ Pre-filter uses raw events instead of manager state analysis
4. ❌ Cursor and selection scrolling are separate (code duplication)
5. ❌ No drag-to-scroll with distance-based acceleration
6. ❌ Manual scroll calls instead of post-callback state analysis

**Required Refactoring** (detailed in architecture doc):

### 1. Unified Scroll System
- **Replace**: `scroll_focused_cursor_into_view()` 
- **With**: `scroll_selection_into_view(SelectionScrollType, ScrollMode)`
- **Treat cursor as**: 0-size selection (collapsed selection)
- **Add acceleration**: Distance-based zones (20px→50px→100px→200px = 1x→2x→4x→8x)
- **Scroll modes**: Instant (typing), Accelerated (drag-to-scroll)

### 2. Complete Changeset System
```rust
pub struct TextChangeset {
    pub operation: TextOperation,  // InsertText, DeleteText, SetSelection, MoveCursor, etc.
    pub target: DomNodeId,
}

// Two-phase processing:
// 1. PRE-CALLBACK: Create changesets (don't apply yet)
// 2. POST-CALLBACK: Apply changesets if !preventDefault
```
**Benefits**: preventDefault for all operations, undo/redo ready, validation

### 3. State-Based Pre-Filter
```rust
pub fn pre_callback_filter_internal_events(
    keyboard_state: &KeyboardState,      // Use VirtualKeyCode
    mouse_state: &MouseState,             // Use left_button_down
    selection_manager: &SelectionManager, // Use click_state, drag_start
    focus_manager: &FocusManager,
    scroll_manager: &ScrollManager,
) -> PreCallbackFilterResult
```
**Analyze state transitions**, not raw events. Example: Drag = `mouse_state.left_button_down` + `selection_manager.has_selection()`

### 4. State-Based Post-Filter
```rust
pub fn post_callback_filter_internal_events(
    changesets: &[TextChangeset],
    layout_window: &mut LayoutWindow,
) -> PostCallbackFilterResult {
    // Analyze applied changesets
    // Generate ScrollSelectionIntoView events (not manual calls)
    // Generate StartAutoScrollTimer / CancelAutoScrollTimer
}
```
**No hardcoded scroll calls** - analyze state changes after changeset application

### 5. Auto-Scroll Timer System
- 60fps timer during drag operations
- Calls `scroll_selection_into_view()` with accelerated mode
- Distance zones determine scroll speed
- Auto-cancels when mouse returns to container or drag ends

---

### ✅ What's Already Implemented (Keep)

- ✅ **Click State Tracking**: 500ms timeout, 5px distance, cycling 1→2→3
- ✅ **Word/Paragraph Selection**: Unicode boundaries, line detection (5 tests passing)
- ✅ **Event Loop Integration**: Pre/post callback filter hooks
- ✅ **Clipboard Extraction**: ClipboardContent with styled runs, to_html()
- ✅ **System Event Types**: PreCallbackSystemEvent, PostCallbackSystemEvent enums
- ✅ **Manager Infrastructure**: SelectionManager, basic state tracking

### ❌ What Needs Refactoring

- 🔄 **Pre-Filter**: Change from event-based to state-based analysis
- 🔄 **Post-Filter**: Change from hardcoded to changeset analysis
- 🔄 **Scroll System**: Unify cursor + selection scrolling, add acceleration
- 🔄 **Changeset System**: Create complete two-phase processing
- 🔄 **VirtualKeyCode**: Use existing enums, not hardcoded key_code

### ❌ What's Still Missing (After Refactoring)

- ❌ **Selection Rendering**: Visual feedback in display list
- ❌ **Keyboard Shortcuts**: Ctrl+C/X/V/A/Z/Y implementation
- ❌ **Platform Clipboard**: macOS/Windows/X11/Wayland integration
- ❌ **Test Suite**: Comprehensive integration tests
- ❌ **Performance**: Profiling and optimization

---

### Migration Path

**Step 1**: Implement unified scroll system (`scroll_selection_into_view`)
**Step 2**: Create changeset infrastructure (`TextChangeset`, `TextOperation`)
**Step 3**: Refactor pre-filter to use manager state (not events)
**Step 4**: Refactor post-filter to analyze changesets (not hardcoded)
**Step 5**: Implement auto-scroll timer system
**Step 6**: Remove manual scroll calls and direct mutations
**Step 7**: Continue with rendering, shortcuts, platform integration

**Estimated Effort**: 2-3 days for complete refactoring

**Next Action**: Start with Phase 1 (Unified Scroll System) from architecture document.

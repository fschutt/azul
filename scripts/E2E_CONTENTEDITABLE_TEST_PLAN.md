# E2E Test Plan: Contenteditable Text Input with Cursor Blinking

## Overview

This plan implements end-to-end tests for contenteditable text input functionality including:
- Cursor blinking
- Text editing (insert, delete)
- Click-to-position cursor
- Selection (drag, Ctrl+A, Shift+Arrow)
- Scroll-into-view

## Phase 1: New Debug API Endpoints

We need additional debug endpoints to verify cursor/focus state:

### 1.1 `GetFocusState` API

Returns the currently focused node and its contenteditable status.

```json
// Request
{ "type": "GetFocusState" }

// Response
{
  "has_focus": true,
  "focused_node": {
    "dom_id": 0,
    "node_id": 15,
    "selector": "div.text-input",
    "is_contenteditable": true,
    "text_content": "Hello World"
  }
}
```

### 1.2 `GetCursorState` API

Returns the cursor position and blink state.

```json
// Request
{ "type": "GetCursorState" }

// Response
{
  "has_cursor": true,
  "cursor": {
    "dom_id": 0,
    "node_id": 15,
    "position": 11,  // grapheme cluster index
    "affinity": "downstream",
    "is_visible": true,
    "blink_timer_active": true
  }
}
```

### Implementation Locations

- **File**: `dll/src/desktop/shell2/common/debug_server.rs`
- **Add to** `DebugEvent` enum (line ~909)
- **Add to** `ResponseData` enum (needs to add new response types)
- **Add handlers** in the match block (~line 2109+)

---

## Phase 2: Test Cases

### Test 2.1: Focus and Cursor Initialization

1. Launch app with empty contenteditable field
2. Click on the field → `Click { selector: ".single-line" }`
3. Wait frame → `WaitFrame`
4. Verify: `GetFocusState` returns the field as focused with `is_contenteditable: true`
5. Verify: `GetCursorState` returns cursor at position 0 (empty field)
6. Verify: `GetCursorState` shows `blink_timer_active: true`

### Test 2.2: Cursor Blinking

1. Focus on contenteditable → `Click { selector: ".single-line" }`
2. Wait 530ms → `Wait { ms: 530 }`
3. Verify: `GetCursorState` → `is_visible` may have toggled
4. Wait 530ms → `Wait { ms: 530 }`
5. Verify: `GetCursorState` → `is_visible` toggled again
6. Type character → `TextInput { text: "a" }`
7. Verify: `GetCursorState` → `is_visible: true` (input resets blink)

### Test 2.3: Text Input

1. Focus field → `Click { selector: ".single-line" }`
2. Type "Hello" → `TextInput { text: "Hello" }`
3. Wait frame
4. Verify: `GetSelectionState` → cursor at position 5
5. Verify: `GetFocusState` → `text_content: "Hello"`

### Test 2.4: Click-to-Position

1. Pre-condition: Field contains "Hello World"
2. Click in middle of "World" → Calculate x,y from `GetNodeLayout`
3. Wait frame
4. Verify: `GetCursorState` → cursor at ~position 8 (between "Wo" and "rld")

### Test 2.5: Arrow Key Navigation

1. Focus field with "Hello"
2. `KeyDown { key: "ArrowLeft" }` 
3. Verify: cursor position decremented
4. `KeyDown { key: "ArrowLeft", modifiers: { shift: true } }`
5. Verify: `GetSelectionState` shows selection range

### Test 2.6: Select All (Ctrl+A)

1. Focus field with text
2. `KeyDown { key: "a", modifiers: { ctrl: true } }`
3. Verify: `GetSelectionState` shows range from 0 to text_length

### Test 2.7: Backspace/Delete

1. Focus field with "Hello"
2. Cursor at end (position 5)
3. `KeyDown { key: "Backspace" }`
4. Verify: text is "Hell", cursor at position 4

### Test 2.8: Scroll Into View

1. Multi-line contenteditable with text that overflows
2. Cursor is at end, scrolled out of view
3. Type character
4. Verify: `GetScrollStates` shows scroll position changed to bring cursor into view

---

## Phase 3: Implementation Tasks

### Task 3.1: Add Debug API Endpoints

```rust
// In DebugEvent enum, add:
GetFocusState,
GetCursorState,

// In ResponseData enum, add:
FocusState(FocusStateResponse),
CursorState(CursorStateResponse),

// Add response structs
```

### Task 3.2: Add Response Structs

```rust
#[derive(Debug, Serialize)]
pub struct FocusStateResponse {
    pub has_focus: bool,
    pub focused_node: Option<FocusedNodeInfo>,
}

#[derive(Debug, Serialize)]
pub struct FocusedNodeInfo {
    pub dom_id: u32,
    pub node_id: u64,
    pub selector: Option<String>,
    pub is_contenteditable: bool,
    pub text_content: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CursorStateResponse {
    pub has_cursor: bool,
    pub cursor: Option<CursorInfo>,
}

#[derive(Debug, Serialize)]
pub struct CursorInfo {
    pub dom_id: u32,
    pub node_id: u64,
    pub position: usize,
    pub affinity: String,
    pub is_visible: bool,
    pub blink_timer_active: bool,
}
```

### Task 3.3: Implement Handlers

```rust
DebugEvent::GetFocusState => {
    let layout_window = callback_info.get_layout_window();
    let focus_manager = &layout_window.focus_manager;
    // ... build response
}

DebugEvent::GetCursorState => {
    let layout_window = callback_info.get_layout_window();
    let cursor_manager = &layout_window.cursor_manager;
    // ... build response
}
```

### Task 3.4: Update Test Script

Update `tests/e2e/test_contenteditable.sh` with new tests.

---

## Phase 4: Execution Order

1. ✅ Add `GetFocusState` to DebugEvent enum - DONE
2. ✅ Add `GetCursorState` to DebugEvent enum - DONE
3. ✅ Add `FocusStateResponse` struct - DONE
4. ✅ Add `CursorStateResponse` struct - DONE
5. ✅ Add `FocusState` and `CursorState` to ResponseData enum - DONE
6. ✅ Implement `GetFocusState` handler - DONE
7. ✅ Implement `GetCursorState` handler - DONE
8. ⚠️ Compile and test - BLOCKED (pre-existing build errors in azul-dll)
9. ✅ Create test_contenteditable_v2.sh with new tests - DONE
10. ⏳ Run E2E tests - PENDING (needs build)

---

## Build Issues

The azul-dll crate currently has 453 pre-existing build errors unrelated to
our changes (missing imports like `azul_layout::json`, `fluent` feature issues).
These need to be fixed before the new Debug APIs can be tested.

The implementation is complete in source code:
- `dll/src/desktop/shell2/common/debug_server.rs`: Added GetFocusState, GetCursorState handlers
- `tests/e2e/test_contenteditable_v2.sh`: New test script using the APIs

---

## Files to Modify

| File | Changes |
|------|---------|
| `dll/src/desktop/shell2/common/debug_server.rs` | Add enums, structs, handlers |
| `tests/e2e/test_contenteditable.sh` | Add new test cases |

---

## Notes

- Cursor blink timer uses ID `0x0001` (reserved)
- Blink interval is 530ms
- `CursorManager.is_visible` is toggled by timer callback
- `CursorManager.blink_timer_active` tracks if timer is running
- Focus changes trigger `handle_focus_change_for_cursor_blink()` in event_v2.rs

# Changeset Inspection & Modification API

## Overview

Added comprehensive changeset inspection and modification methods to `CallbackInfo`, allowing user callbacks to inspect pending operations before they execute and optionally modify or block them.

**File:** `layout/src/callbacks.rs`  
**Lines:** ~1915-2215

## New API Methods

### 📋 Clipboard Operation Inspection

#### `inspect_copy_changeset(target: DomNodeId) -> Option<ClipboardContent>`
Inspect what would be copied to clipboard.

```rust
On::Copy -> |info| {
    if let Some(content) = info.inspect_copy_changeset(target) {
        if content.plain_text.contains("secret") {
            return Update::PreventDefault; // Block copying secrets
        }
    }
    Update::DoNothing
}
```

**Returns:**
- `ClipboardContent` with `plain_text` and `html_text` fields
- `None` if no selection exists

#### `inspect_cut_changeset(target: DomNodeId) -> Option<ClipboardContent>`
Inspect what would be cut (copied + deleted).

**Returns:** Same as `inspect_copy_changeset`

#### `inspect_paste_target_range(target: DomNodeId) -> Option<SelectionRange>`
Inspect the selection range that will be replaced by paste.

**Returns:**
- `Some(SelectionRange)` if text is selected (paste will replace)
- `None` if no selection (paste will insert at cursor)

---

### 🔍 Select All Inspection

#### `inspect_select_all_changeset(target: DomNodeId) -> Option<(String, SelectionRange)>`
Inspect what would be selected by Select All operation.

```rust
On::KeyDown(VirtualKeyCode::A) -> |info| {
    if let Some((text, range)) = info.inspect_select_all_changeset(target) {
        println!("Would select {} characters", text.len());
        if text.len() > 10000 {
            return Update::PreventDefault; // Don't select huge texts
        }
    }
    Update::DoNothing
}
```

**Returns:**
- Tuple of `(full_text, selection_range)`
- `None` if node has no text content

---

### ⌫ Delete Operation Inspection

#### `inspect_delete_changeset(target: DomNodeId, forward: bool) -> Option<(SelectionRange, String)>`
Inspect what would be deleted by backspace/delete.

**Arguments:**
- `forward: true` → Delete key (delete after cursor)
- `forward: false` → Backspace key (delete before cursor)

**Returns:**
- Tuple of `(range_to_delete, deleted_text)`
- `None` if nothing to delete (at boundary or no cursor)

```rust
On::KeyDown(VirtualKeyCode::Back) -> |info| {
    if let Some((range, text)) = info.inspect_delete_changeset(target, false) {
        if text == "\n" {
            return Update::PreventDefault; // Don't delete newlines
        }
    }
    Update::DoNothing
}
```

---

### ↶ Undo/Redo Operation Inspection

#### `inspect_undo_operation(node_id: NodeId) -> Option<&UndoableOperation>`
Inspect what operation would be undone.

```rust
On::KeyDown(VirtualKeyCode::Z) -> |info| {
    if let Some(op) = info.inspect_undo_operation(node_id) {
        // Check pre-state
        if op.pre_state.text_content.is_empty() {
            return Update::PreventDefault; // Don't undo if would make empty
        }
        
        // Check operation type
        match &op.changeset.operation {
            TextOperation::DeleteText { .. } => {
                // This was a deletion, undo will restore text
            }
            TextOperation::InsertText { .. } => {
                // This was an insertion, undo will remove text
            }
            _ => {}
        }
    }
    Update::DoNothing
}
```

**Returns:**
- Reference to `UndoableOperation` containing:
  - `changeset: TextChangeset` - The original operation
  - `pre_state: NodeStateSnapshot` - State before operation
    - `text_content: String`
    - `cursor_position: Option<TextCursor>`
    - `selection_range: Option<SelectionRange>`
    - `timestamp: Instant`

#### `inspect_redo_operation(node_id: NodeId) -> Option<&UndoableOperation>`
Inspect what operation would be redone.

**Returns:** Same as `inspect_undo_operation`

---

### ✏️ Content Modification (Planned)

#### `set_copy_content(target: DomNodeId, content: ClipboardContent) -> bool`
Override clipboard content before copying.

**Status:** ⚠️ Returns `false` (not yet implemented)

**Planned Usage:**
```rust
On::Copy -> |info| {
    if let Some(mut content) = info.inspect_copy_changeset(target) {
        // Transform content
        content.plain_text = format!("[COPIED FROM APP]\n{}", content.plain_text);
        info.set_copy_content(target, content);
    }
    Update::DoNothing
}
```

#### `set_cut_content(target: DomNodeId, content: ClipboardContent) -> bool`
Override clipboard content before cutting.

**Status:** ⚠️ Returns `false` (not yet implemented)

#### `set_select_all_range(target: DomNodeId, range: SelectionRange) -> bool`
Override what range gets selected by Select All.

**Status:** ⚠️ Returns `false` (not yet implemented)

**Planned Usage:**
```rust
On::KeyDown(VirtualKeyCode::A) -> |info| {
    if let Some((text, range)) = info.inspect_select_all_changeset(target) {
        // Only select first paragraph
        if let Some(newline_pos) = text.find('\n') {
            let limited_range = SelectionRange {
                start: range.start,
                end: TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: 0,
                        start_byte_in_run: newline_pos as u32,
                    },
                    affinity: CursorAffinity::Leading,
                },
            };
            info.set_select_all_range(target, limited_range);
        }
    }
    Update::DoNothing
}
```

---

### 📊 Helper Query Methods

#### `get_node_text_content(target: DomNodeId) -> Option<String>`
Get current text content of a node.

```rust
let text = info.get_node_text_content(target)?;
println!("Current text: {}", text);
```

#### `get_node_cursor_position(target: DomNodeId) -> Option<TextCursor>`
Get current cursor position (only if node is focused).

```rust
if let Some(cursor) = info.get_node_cursor_position(target) {
    println!("Cursor at byte {}", cursor.cluster_id.start_byte_in_run);
}
```

#### `get_node_selection_ranges(target: DomNodeId) -> Vec<SelectionRange>`
Get all active selection ranges for the node's DOM.

```rust
let ranges = info.get_node_selection_ranges(target);
for range in ranges {
    println!("Selection from {:?} to {:?}", range.start, range.end);
}
```

#### `node_has_selection(target: DomNodeId) -> bool`
Check if a specific node has an active selection.

**Note:** This differs from `has_selection(dom_id: &DomId)` which checks the entire DOM.

```rust
if info.node_has_selection(target) {
    println!("Node has selection");
}
```

#### `get_node_text_length(target: DomNodeId) -> Option<usize>`
Get byte length of text in node (for bounds checking).

```rust
let len = info.get_node_text_length(target)?;
if len > 1000 {
    println!("Long text: {} bytes", len);
}
```

---

## Integration with LayoutWindow

All inspect methods leverage existing `LayoutWindow` functionality:

- `get_clipboard_content()` - Extract clipboard content from selection
- `get_text_before_textinput()` - Read text cache
- `extract_text_from_inline_content()` - Convert InlineContent to String
- `selection_manager` - Query selection ranges
- `cursor_manager` - Query cursor position
- `focus_manager` - Check focus state
- `undo_redo_manager` - Access undo/redo stacks

This ensures consistency with the existing text editing system.

---

## Usage Examples

### Block Copying Sensitive Data

```rust
dom.with_callback(On::Copy, |info| {
    let target = info.get_callback_node_id();
    
    if let Some(content) = info.inspect_copy_changeset(target) {
        // Check for sensitive patterns
        if content.plain_text.contains("SSN:") || 
           content.plain_text.contains("Password:") {
            info.prevent_default();
            return Update::PreventDefault;
        }
    }
    
    Update::DoNothing
})
```

### Prevent Undo of Critical Changes

```rust
dom.with_callback(On::KeyDown(VirtualKeyCode::Z), |info| {
    let node_id = NodeId::new(0);
    
    if let Some(operation) = info.inspect_undo_operation(node_id) {
        // Check if operation has critical flag (custom metadata)
        if operation.is_critical() {
            info.prevent_default();
            return Update::RefreshDom; // Show warning instead
        }
    }
    
    Update::DoNothing
})
```

### Custom Delete Behavior

```rust
dom.with_callback(On::KeyDown(VirtualKeyCode::Back), |info| {
    let target = info.get_callback_node_id();
    
    if let Some((range, text)) = info.inspect_delete_changeset(target, false) {
        // Prevent deleting specific characters
        if text.chars().all(|c| c.is_whitespace()) {
            // Allow deleting whitespace
            return Update::DoNothing;
        }
        
        if text.chars().any(|c| !c.is_ascii()) {
            // Block deleting unicode
            info.prevent_default();
            return Update::PreventDefault;
        }
    }
    
    Update::DoNothing
})
```

### Validate Text Length Before Paste

```rust
dom.with_callback(On::KeyDown(VirtualKeyCode::V), |info| {
    let target = info.get_callback_node_id();
    
    // Check current length
    let current_len = info.get_node_text_length(target).unwrap_or(0);
    
    // Inspect what would be pasted (requires clipboard access)
    if let Ok(clipboard) = clipboard2::SystemClipboard::new() {
        if let Ok(paste_text) = clipboard.get_string_contents() {
            let new_len = current_len + paste_text.len();
            
            if new_len > 1000 {
                info.prevent_default();
                return Update::PreventDefault; // Text too long
            }
        }
    }
    
    Update::DoNothing
})
```

---

## Architecture Benefits

### ✅ Read-Only Inspection
All `inspect_*` methods are read-only - they don't mutate state. This allows safe querying without side effects.

### ✅ Consistent with LayoutWindow
All methods use existing `LayoutWindow` APIs, ensuring consistency with internal text editing logic.

### ✅ preventDefault Support
Callbacks can inspect operations and return `Update::PreventDefault` to block them.

### ✅ No Breaking Changes
New methods are additive - existing code continues to work.

### ✅ Future Extensibility
The `set_*` methods provide placeholders for future content override functionality.

---

## Implementation Status

### ✅ Completed
- All `inspect_*` methods implemented
- All helper query methods implemented
- Full read access to managers (undo/redo, selection, cursor, focus)
- Compiles successfully

### ⚠️ Planned (Post-1.0)
- `set_copy_content()` - Override clipboard before copy
- `set_cut_content()` - Override clipboard before cut
- `set_select_all_range()` - Limit selection range
- Requires extending `SelectionManager` API for temporary overrides

---

## Testing

All methods can be tested via callbacks:

```rust
// Test copy inspection
let copy_callback = |info: &mut CallbackInfo| {
    let target = info.get_callback_node_id();
    assert!(info.inspect_copy_changeset(target).is_some());
    Update::DoNothing
};

// Test undo inspection
let undo_callback = |info: &mut CallbackInfo| {
    let node_id = NodeId::new(0);
    if info.get_undo_redo_manager().can_undo(node_id) {
        assert!(info.inspect_undo_operation(node_id).is_some());
    }
    Update::DoNothing
};
```

---

## Related Files

- `layout/src/callbacks.rs` - CallbackInfo implementation (lines ~1915-2215)
- `layout/src/managers/changeset.rs` - TextChangeset types
- `layout/src/managers/undo_redo.rs` - UndoRedoManager
- `layout/src/managers/selection.rs` - SelectionManager, ClipboardContent
- `layout/src/window.rs` - LayoutWindow text query methods

---

## Next Steps

1. ✅ Add inspect methods - **COMPLETED**
2. ⚠️ Add preventDefault events (BeforeCopy, BeforeUndo, etc.)
3. ⚠️ Implement set_* override methods
4. ⚠️ Add explicit On::Copy, On::Cut, On::Paste callbacks
5. ⚠️ Add explicit On::Undo, On::Redo callbacks

**For 1.0 Release:** Items 1-2 are critical, items 3-5 can be post-1.0.

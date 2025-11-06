# Event Flow Verification - Text Editing System

## Current Implementation Status (November 2025)

### ✅ COMPLETED: Edit Event → Manager Updates → User Callbacks

#### 1. Text Input Events (WORKING)

**Flow:**
```
User types → event_v2.rs::process_text_input()
           → LayoutWindow::process_text_input()
           → TextInputManager records TextEdit 
           → apply_text_changeset() captures pre-state
           → UndoRedoManager::record_operation() stores undo info
           → Text cache updated
           → User callbacks invoked (if registered)
```

**File:** `dll/src/desktop/shell2/common/event_v2.rs` lines ~1100-1150
- Handles `PreCallbackSystemEvent::TextInput`
- Calls `layout_window.process_text_input(text)`
- Returns affected nodes for re-render

**File:** `layout/src/window.rs` lines ~2600-2700
- `process_text_input()` applies text edits
- `apply_text_changeset()` records undo before mutation
- Integrates with TextInputManager, UndoRedoManager, cursor/selection

#### 2. Copy/Cut/Paste Events (WORKING)

**Copy (Ctrl+C):** `event_v2.rs` line ~1161
```rust
KeyboardShortcut::Copy => {
    // Get clipboard content from SelectionManager
    let content = layout_window.get_clipboard_content(&dom_id);
    // Write to system clipboard via clipboard2
    clipboard.set_string_contents(content.plain_text);
}
```

**Cut (Ctrl+X):** `event_v2.rs` line ~1174
```rust
KeyboardShortcut::Cut => {
    // Copy to clipboard first
    clipboard.set_string_contents(content);
    // Then delete selection via delete_selection()
    layout_window.delete_selection(target, false);
}
```

**Paste (Ctrl+V):** `event_v2.rs` line ~1192
```rust
KeyboardShortcut::Paste => {
    // Read from system clipboard
    let clipboard_text = clipboard.get_string_contents();
    // Insert as text input (goes through undo system)
    layout_window.process_text_input(&clipboard_text);
}
```

**Status:** ✅ Working, uses clipboard2::SystemClipboard
**Missing:** preventDefault support (see below)

#### 3. Select All (Ctrl+A) - STUB

**File:** `event_v2.rs` line ~1207
```rust
KeyboardShortcut::SelectAll => {
    // TODO: Implement select_all operation
}
```

**Status:** ⚠️ Not implemented
**Helper exists:** `changeset.rs::create_select_all_changeset()` (line ~449)

#### 4. Undo/Redo (Ctrl+Z/Y) (WORKING)

**File:** `event_v2.rs` lines 1219-1301

**Undo Flow:**
```
Ctrl+Z → pop_undo() → get pre-state snapshot
      → Create InlineContent with old text
      → update_text_cache_after_edit() restores text
      → Restore cursor position
      → push_redo() for redo stack
```

**Redo Flow:**
```
Ctrl+Y → pop_redo() → get original operation
      → process_text_input() re-applies text
      → push_undo() back to undo stack
```

**Status:** ✅ Working, integrated with UndoRedoManager
**Platform:** All 4 platforms (Windows, macOS, Linux/X11, Wayland) via event_v2.rs
**macOS Special:** Also has NSResponder integration (mod.rs lines 262-320, 651-709, 3688-3820)

#### 5. Backspace/Delete (WORKING)

**File:** `event_v2.rs` line ~1305
```rust
PreCallbackSystemEvent::DeleteSelection { target, forward } => {
    // Calls delete_selection() directly
    layout_window.delete_selection(target, forward);
}
```

**Status:** ✅ Working
**Note:** Does NOT go through UndoRedoManager yet (direct mutation)
**TODO:** Integrate with changeset system for undo support

### ✅ VERIFIED: CallbackInfo Read Access to UndoRedoManager

**File:** `layout/src/callbacks.rs` line 1560
```rust
pub fn get_undo_redo_manager(&self) -> &UndoRedoManager {
    &self.internal_get_layout_window().undo_redo_manager
}
```

**Access Pattern:**
```rust
fn my_callback(info: &mut CallbackInfo) {
    let undo_manager = info.get_undo_redo_manager();
    let can_undo = undo_manager.can_undo(node_id);
    let can_redo = undo_manager.can_redo(node_id);
    // Read-only access to undo history
}
```

**Also via Deref:**
```rust
fn my_callback(info: &mut CallbackInfo) {
    // CallbackInfo derefs to LayoutWindow
    let undo_manager = &info.undo_redo_manager;
    let history = undo_manager.get_undo_history(node_id);
}
```

**Status:** ✅ Full read access available

---

## ❌ MISSING: preventDefault Support

### Problem: No PreCallbackSystemEvent for these operations

Currently, events are processed AFTER user callbacks, not BEFORE. This means:
- User callbacks cannot preventDefault on Copy/Cut/Paste/Undo/Redo
- Events are applied immediately, then callbacks run

### Required: Two-Phase Event System

**Needed Events in `azul_core::events::PreCallbackSystemEvent`:**

```rust
pub enum PreCallbackSystemEvent {
    // Existing events...
    TextInput { target, text },
    DeleteSelection { target, forward },
    
    // ❌ MISSING - Need these for preventDefault:
    BeforeCopy { target, content },
    BeforeCut { target, content },
    BeforePaste { target, clipboard_content },
    BeforeUndo { target, operation },
    BeforeRedo { target, operation },
    
    // Also useful:
    BeforeSelectAll { target },
}
```

**Two-Phase Flow:**
```
1. PRE-CALLBACK: Generate PreCallbackSystemEvent
   ├─ User callback runs with event
   ├─ User can call preventDefault()
   └─ Event returns whether to proceed

2. POST-CALLBACK: If !preventDefault, apply changes
   ├─ Update managers (UndoRedoManager, TextInputManager, etc.)
   ├─ Mutate state (text cache, selection, cursor)
   └─ Mark nodes for re-render
```

**Current Gap:** Copy/Cut/Paste/Undo/Redo happen in ONE phase:
```
❌ Current: Apply change → Update managers → Invoke callbacks (too late!)
✅ Needed:  Generate event → Invoke callbacks → If !preventDefault, apply
```

---

## 📊 Event Coverage Matrix

| Event | Event Generated? | Manager Updated? | Callback Fired? | preventDefault? | Undo Support? |
|-------|-----------------|------------------|-----------------|-----------------|---------------|
| Text Input | ✅ `TextInput` | ✅ TextInputManager | ✅ On::TextInput | ✅ Via TextInput | ✅ UndoRedoManager |
| Backspace/Delete | ✅ `DeleteSelection` | ⚠️ Direct mutation | ✅ On::TextInput | ❌ No event | ❌ Not integrated |
| Copy (Ctrl+C) | ✅ `KeyboardShortcut::Copy` | ✅ SelectionManager | ❌ No callback | ❌ No pre-event | N/A (no mutation) |
| Cut (Ctrl+X) | ✅ `KeyboardShortcut::Cut` | ✅ Selection + Text | ❌ No callback | ❌ No pre-event | ❌ Not integrated |
| Paste (Ctrl+V) | ✅ `KeyboardShortcut::Paste` | ✅ Via process_text_input | ✅ On::TextInput | ✅ Via TextInput | ✅ Via process_text_input |
| Select All (Ctrl+A) | ✅ `KeyboardShortcut::SelectAll` | ❌ Stub | ❌ No callback | ❌ No pre-event | N/A (no mutation) |
| Undo (Ctrl+Z) | ✅ `KeyboardShortcut::Undo` | ✅ UndoRedoManager | ❌ No callback | ❌ No pre-event | N/A (reversal) |
| Redo (Ctrl+Y) | ✅ `KeyboardShortcut::Redo` | ✅ UndoRedoManager | ✅ On::TextInput (via redo) | ✅ Via TextInput | ✅ Re-applied |

### Legend
- ✅ Fully implemented
- ⚠️ Partial implementation
- ❌ Not implemented
- N/A Not applicable

---

## 🎯 Action Items for 1.0 Release

### HIGH PRIORITY (Blocking)

1. **Add preventDefault for Copy/Cut**
   - Add `PreCallbackSystemEvent::BeforeCopy/BeforeCut`
   - Fire callbacks BEFORE clipboard write
   - Allow user to cancel operation

2. **Add preventDefault for Undo/Redo**
   - Add `PreCallbackSystemEvent::BeforeUndo/BeforeRedo`
   - Fire callbacks BEFORE state reversion
   - Allow user to validate/cancel

3. **Implement Select All**
   - Use existing `create_select_all_changeset()` helper
   - Call `SelectionManager::set_selection()` with full range
   - Fire `On::SelectionChange` callback

4. **Integrate Backspace/Delete with Undo**
   - Capture pre-state before `delete_selection()`
   - Record operation to UndoRedoManager
   - Use `create_delete_selection_changeset()` helper

### MEDIUM PRIORITY (Nice to have for 1.0)

5. **Add explicit callbacks for clipboard operations**
   - `On::Copy`, `On::Cut`, `On::Paste` events
   - Allow inspection of clipboard content
   - Enable custom clipboard formatting

6. **Add explicit callbacks for undo/redo**
   - `On::Undo`, `On::Redo` events
   - Allow inspection of operation being reverted
   - Enable custom undo behavior per node

### LOW PRIORITY (Post-1.0)

7. **Complete changeset system**
   - Uncomment `create_changesets_from_system_events()`
   - Implement all changeset creation helpers
   - Unify two-phase architecture across ALL events

---

## 📝 API Examples

### Current API (Works Today)

```rust
// Read undo/redo state
fn my_callback(info: &mut CallbackInfo) {
    let undo_manager = info.get_undo_redo_manager();
    
    // Check if undo/redo available
    let node_id = NodeId::new(0);
    if undo_manager.can_undo(node_id) {
        println!("Undo available");
    }
    
    // Get history for UI
    let history = undo_manager.get_undo_history(node_id);
    println!("Undo depth: {}", history.len());
}

// Handle text input with preventDefault
fn text_input_callback(info: &mut CallbackInfo, event: &TextInputEvent) {
    // Filter certain characters
    if event.text.contains("@") {
        return Update::PreventDefault; // Block @ character
    }
    Update::DoNothing
}
```

### Needed API (For preventDefault on Copy/Cut/Undo/Redo)

```rust
// Handle copy with preventDefault
fn copy_callback(info: &mut CallbackInfo, event: &CopyEvent) -> Update {
    // Inspect what will be copied
    if event.content.plain_text.contains("secret") {
        return Update::PreventDefault; // Block copy of secrets
    }
    Update::DoNothing
}

// Handle undo with preventDefault
fn undo_callback(info: &mut CallbackInfo, event: &UndoEvent) -> Update {
    // Inspect what will be undone
    if event.operation.is_critical() {
        return Update::PreventDefault; // Protect critical edits
    }
    Update::DoNothing
}

// Register callbacks
dom.with_callback(On::Copy, copy_callback)
   .with_callback(On::Undo, undo_callback);
```

---

## ✅ Verification Checklist

- [x] Text input goes through UndoRedoManager
- [x] Copy/Cut/Paste work with system clipboard
- [x] Undo/Redo work on all 4 platforms
- [x] macOS has NSResponder integration
- [x] CallbackInfo has read access to UndoRedoManager
- [x] Text mutations record pre-state snapshots
- [x] Paste integrates with undo (via process_text_input)
- [ ] Copy/Cut have preventDefault support
- [ ] Undo/Redo have preventDefault support  
- [ ] Select All is implemented
- [ ] Backspace/Delete integrated with UndoRedoManager
- [ ] All events fire user callbacks before applying changes

**Overall Status:** Core architecture is solid, missing preventDefault for some operations

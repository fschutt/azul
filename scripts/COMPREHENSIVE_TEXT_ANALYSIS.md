# Comprehensive Text Input Analysis

**Date:** 2025-01-29  
**Status:** ✅ IMPLEMENTED

## Executive Summary

This document provides a comprehensive analysis of the contenteditable and text input
bugs in Azul, verified against the actual source code. All major bugs have been fixed
and committed.

### Commits Made

1. `98558325` - fix(text): Handle CursorAffinity in insert/delete operations
2. `fb856d8f` - fix(contenteditable): Set focus and cursor on click
3. `e4738af9` - fix(scroll): Scroll cursor into view after text edit
4. `13f3ce4f` - fix(macos): Improve NSTextInputClient implementation
5. `9ebd72bb` - test(e2e): Update contenteditable tests and gitignore
6. `9621a80f` - docs: Add comprehensive text input analysis and debug scripts
7. `128a0107` - fix(text): Change UnifiedConstraints default width to MaxContent
8. `486c06a4` - fix(contenteditable): Also check contenteditable boolean field
9. `c4e5668b` - fix(scrollbar): Add epsilon to overflow comparisons
10. `957bab8c` - feat(dom): Add calculate_structural_hash for text node matching
11. `3a3102da` - feat(diff): Add structural hash matching and cursor reconciliation

---

## Part 1: Bug Verification

### Bug 1: Focus Not Set on ContentEditable Click ✅ FIXED

**Location:** [layout/src/window.rs](../layout/src/window.rs#L6500-L6540)

**Original Problem:**
`process_mouse_click_for_selection()` set SelectionManager state but never called
`FocusManager.set_focused_node()`, causing `record_text_input()` to early-return
because `focus_manager.get_focused_node()` returned `None`.

**Current Status:** FIXED in current codebase. The fix walks up the DOM tree to
find contenteditable ancestors and sets focus:

```rust
// CRITICAL FIX 1: Set focus on the clicked node
let is_contenteditable = self.layout_results.get(&dom_id)
    .map(|lr| {
        // Walk up the DOM tree to check if any ancestor has contenteditable
        let mut current_node = Some(ifc_root_node_id);
        while let Some(node_id) = current_node {
            if let Some(styled_node) = node_data.get(node_id.index()) {
                let has_contenteditable = styled_node.attributes.as_ref().iter().any(|attr| {
                    matches!(attr, azul_core::dom::AttributeType::ContentEditable(_))
                });
                if has_contenteditable { return true; }
            }
            current_node = node_hierarchy.get(node_id).and_then(|h| h.parent_id());
        }
        false
    })
    .unwrap_or(false);

if is_contenteditable {
    self.focus_manager.set_focused_node(Some(dom_node_id));
}
```

**⚠️ Potential Issue:** The code checks `AttributeType::ContentEditable(_)` in attributes,
but `NodeData` also has a direct `contenteditable: bool` field (line 1328 in dom.rs).
The fix should ALSO check `styled_node.contenteditable` boolean field for robustness.

---

### Bug 2: Line Wrapping Due to Zero Width Default ✅ FIXED

**Location:** [layout/src/text3/cache.rs](../layout/src/text3/cache.rs#L682-L692)

**Problem:** `UnifiedConstraints::default()` sets `available_width: AvailableSpace::Definite(0.0)`
which causes immediate line breaking (each character on its own line).

**Fix Applied (commit 128a0107):**
```rust
impl Default for UnifiedConstraints {
    fn default() -> Self {
        Self {
            // Use MaxContent as default to avoid premature line breaking.
            available_width: AvailableSpace::MaxContent,
            // ...
        }
    }
}
```

---

### Bug 3: Cursor Affinity Handling ✅ FIXED

**Location:** [layout/src/text3/edit.rs](../layout/src/text3/edit.rs#L160-L215)

**Problem:** `insert_text()`, `delete_backward()`, and `delete_forward()` did not
respect `CursorAffinity` - they always used `start_byte_in_run` directly.

**Status:** FIXED in current codebase. All three functions now properly handle affinity:

```rust
let byte_offset = match cursor.affinity {
    CursorAffinity::Leading => cluster_start_byte,
    CursorAffinity::Trailing => {
        // Find the end of the grapheme cluster
        run.text[cluster_start_byte..]
            .grapheme_indices(true)
            .next()
            .map(|(_, grapheme)| cluster_start_byte + grapheme.len())
            .unwrap_or(run.text.len())
    },
};
```

---

### Bug 4: Scrollbar Visibility - Epsilon Comparison ✅ FIXED

**Location:** [layout/src/solver3/fc.rs](../layout/src/solver3/fc.rs#L5806-L5850)

**Problem:** Scrollbar visibility uses exact float comparison instead of epsilon.

**Fix Applied (commit c4e5668b):**
```rust
const EPSILON: f32 = 1.0;

let mut needs_horizontal = match overflow_x {
    OverflowBehavior::Auto => content_size.width > container_size.width + EPSILON,
    // ...
};

let mut needs_vertical = match overflow_y {
    OverflowBehavior::Auto => content_size.height > container_size.height + EPSILON,
    // ...
};
```

---

### Bug 5: dirty_text_nodes Check ✅ FIXED

**Location:** [layout/src/window.rs](../layout/src/window.rs#L5495-L5510)

**Problem:** `get_text_before_textinput()` was reading from StyledDom instead of
checking `dirty_text_nodes` first, causing each keystroke to read stale state.

**Status:** FIXED in current codebase:
```rust
// CRITICAL FIX: Check dirty_text_nodes first!
if let Some(dirty_node) = self.dirty_text_nodes.get(&(dom_id, node_id)) {
    return dirty_node.content.clone();
}
// Fallback to committed state from StyledDom
```

---

### Bug 6: Scroll Into View After Text Edit ✅ FIXED

**Location:** [dll/src/desktop/shell2/common/event_v2.rs](../dll/src/desktop/shell2/common/event_v2.rs#L2925-L2932)

**Problem:** After applying text changesets, the view was not scrolled to keep the
cursor visible (typing at end of long text would scroll cursor out of view).

**Status:** FIXED in current codebase:
```rust
// CRITICAL FIX: Scroll cursor into view after text edit
layout_window.scroll_selection_into_view(
    azul_layout::window::SelectionScrollType::Cursor,
    azul_layout::window::ScrollMode::Instant,
);
```

---

## Part 2: Architecture Analysis - CursorManager vs SelectionManager

### Current Architecture

**CursorManager** (353 lines, [layout/src/managers/cursor.rs](../layout/src/managers/cursor.rs)):
- Manages single cursor position
- Fields: `cursor: Option<TextCursor>`, `cursor_location: Option<CursorLocation>`, 
  `is_visible`, `last_input_time`, `blink_timer_active`
- Responsibilities: Cursor blinking, scroll-into-view, cursor initialization

**SelectionManager** (633 lines, [layout/src/managers/selection.rs](../layout/src/managers/selection.rs)):
- Manages text selections with DUAL models:
  1. Legacy: `selections: BTreeMap<DomId, SelectionState>` (per-node)
  2. New: `text_selections: BTreeMap<DomId, TextSelection>` (anchor/focus)
- Also handles: Click detection (double/triple), clipboard content

### Merge Analysis

**Conceptual Alignment:**
A cursor IS a collapsed selection (anchor == focus, length == 0). This aligns with
the W3C Selection API and browser behavior.

**Current Overlap:**
- Both track `TextCursor` positions
- Both need NodeId remapping after DOM reconciliation
- Both have `remap_node_ids()` methods
- CursorManager's `cursor_location` duplicates SelectionManager's anchor tracking

**Merge Recommendation: YES, with caveats**

1. **Merge CursorManager INTO SelectionManager** - Don't create a new class
2. **Keep blink logic separate** - Move to a `CursorBlinkState` helper struct
3. **Unified cursor = collapsed TextSelection** where `anchor == focus`

### Proposed Unified API

```rust
pub struct SelectionManager {
    /// Multi-node selection state using anchor/focus model
    text_selections: BTreeMap<DomId, TextSelection>,
    
    /// Click state for multi-click detection
    click_state: ClickState,
    
    /// Cursor blink state (only for collapsed selections)
    blink_state: CursorBlinkState,
}

pub struct CursorBlinkState {
    is_visible: bool,
    last_input_time: Option<Instant>,
    blink_timer_active: bool,
}

impl SelectionManager {
    /// Get cursor position if selection is collapsed (anchor == focus)
    pub fn get_cursor(&self, dom_id: &DomId) -> Option<&TextCursor> {
        self.text_selections.get(dom_id)
            .filter(|sel| sel.is_collapsed())
            .map(|sel| &sel.anchor.cursor)
    }
    
    /// Check if we should draw cursor (collapsed selection + visible)
    pub fn should_draw_cursor(&self, dom_id: &DomId) -> bool {
        self.get_cursor(dom_id).is_some() && self.blink_state.is_visible
    }
    
    /// Set cursor position (creates collapsed selection)
    pub fn set_cursor(&mut self, dom_id: DomId, ifc_root: NodeId, cursor: TextCursor, bounds: LogicalRect, mouse_pos: LogicalPosition) {
        self.start_selection(dom_id, ifc_root, cursor, bounds, mouse_pos);
    }
}
```

### Migration Path

1. Add `CursorBlinkState` to SelectionManager
2. Add `get_cursor()`, `should_draw_cursor()`, `set_cursor()` methods
3. Update all call sites to use `selection_manager.get_cursor()` instead of `cursor_manager.get_cursor()`
4. Remove CursorManager struct
5. Update `remap_node_ids()` to handle both selection and cursor remapping in one pass

---

## Part 3: NodeData Hash for DOM Reconciliation

### Current Problem

**Location:** [core/src/dom.rs](../core/src/dom.rs#L1334-L1369)

The `impl Hash for NodeData` hashes `self.node_type`, which includes `NodeType::Text(AzString)`.
This means `Text("Hello")` and `Text("Hello World")` have different hashes, causing
DOM reconciliation to see them as completely different nodes.

**Impact:**
- Cursor position lost on every text edit
- Selection state lost on every DOM regeneration
- Focus state lost when text changes

### The Text Diff Problem

When the user types in a contenteditable element:
1. The app regenerates the DOM with new text content
2. `reconcile_dom()` computes hashes
3. Old `Text("A")` hash ≠ new `Text("AB")` hash
4. Node is treated as Unmount+Mount, not Update
5. Cursor position is lost

### Proposed Solution

**Option A: Exclude Text Content from Hash (Simple)**
```rust
impl Hash for NodeData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash node TYPE discriminant only, not text content
        core::mem::discriminant(&self.node_type).hash(state);
        // ... rest of hash
    }
}
```

**Problem:** Two completely different text nodes (`<p>Foo</p>` and `<p>Bar</p>`) 
would be considered "the same" and incorrectly matched.

**Option B: Use Structural Key for Text Nodes (Recommended)**

Add a `structural_key` that represents the node's position in the DOM tree:

```rust
pub fn calculate_structural_hash(&self, parent_key: Option<u64>, sibling_index: usize) -> DomNodeHash {
    let mut hasher = DefaultHasher::new();
    
    // Hash structure: parent + sibling position
    parent_key.hash(&mut hasher);
    sibling_index.hash(&mut hasher);
    
    // Hash node type discriminant (div, p, span) but NOT text content
    core::mem::discriminant(&self.node_type).hash(&mut hasher);
    
    // Hash attributes, callbacks, etc. (but not dataset which may change)
    self.ids_and_classes.as_ref().hash(&mut hasher);
    self.attributes.as_ref().hash(&mut hasher);
    
    DomNodeHash(hasher.finish())
}
```

**Option C: Key-Based Matching for Contenteditable (Hybrid)**

For contenteditable nodes specifically, require explicit keys:

```rust
Div::new()
    .with_contenteditable(true)
    .with_key(my_stable_id)  // User must provide stable key
    .with_text(&text_content)
```

---

## Part 4: Text Diff and Cursor Preservation

### The Full Solution

To preserve cursor position across DOM regeneration with changing text:

1. **Structural Matching** - Match nodes by DOM position, not content hash
2. **Cursor Reconciliation** - Map old cursor byte offset to new text
3. **Selection Reconciliation** - Map old selection ranges to new text

### Cursor Reconciliation Algorithm

```rust
/// Reconcile cursor position when text content changes
pub fn reconcile_cursor(
    old_text: &str,
    new_text: &str,
    old_cursor_byte: usize,
) -> usize {
    // If texts are equal, cursor is unchanged
    if old_text == new_text {
        return old_cursor_byte;
    }
    
    // Find common prefix length
    let common_prefix = old_text.chars()
        .zip(new_text.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let prefix_bytes = old_text.chars().take(common_prefix)
        .map(|c| c.len_utf8()).sum::<usize>();
    
    // If cursor was in prefix, it stays where it is
    if old_cursor_byte <= prefix_bytes {
        return old_cursor_byte;
    }
    
    // Find common suffix length
    let common_suffix = old_text.chars().rev()
        .zip(new_text.chars().rev())
        .take_while(|(a, b)| a == b)
        .count();
    
    let old_suffix_start = old_text.len() - old_text.chars().rev()
        .take(common_suffix).map(|c| c.len_utf8()).sum::<usize>();
    
    // If cursor was in suffix, adjust by length difference
    if old_cursor_byte >= old_suffix_start {
        let delta = new_text.len() as isize - old_text.len() as isize;
        return (old_cursor_byte as isize + delta).max(0) as usize;
    }
    
    // Cursor was in the changed region - place at end of new content
    // This handles insertions (cursor moves right) and deletions (cursor at edit point)
    let new_suffix_start = new_text.len() - new_text.chars().rev()
        .take(common_suffix).map(|c| c.len_utf8()).sum::<usize>();
    
    new_suffix_start
}
```

### Integration with DOM Diff

Modify `transfer_states()` in diff.rs:

```rust
fn transfer_states(
    old_dom: &StyledDom,
    new_dom: &mut StyledDom,
    node_moves: &[NodeMove],
    cursor_manager: &mut CursorManager,
    selection_manager: &mut SelectionManager,
) {
    for NodeMove { old_node_id, new_node_id } in node_moves {
        // Transfer dataset, scroll position, etc.
        // ...
        
        // Reconcile cursor if it was in this node
        if let Some(cursor_loc) = cursor_manager.get_cursor_location() {
            if cursor_loc.node_id == *old_node_id {
                let old_text = get_node_text(old_dom, *old_node_id);
                let new_text = get_node_text(new_dom, *new_node_id);
                
                if let Some(cursor) = cursor_manager.get_cursor() {
                    let old_byte = cursor.cluster_id.start_byte_in_run as usize;
                    let new_byte = reconcile_cursor(&old_text, &new_text, old_byte);
                    
                    // Update cursor to new position
                    cursor_manager.move_cursor_to(
                        TextCursor {
                            cluster_id: GraphemeClusterId {
                                source_run: cursor.cluster_id.source_run,
                                start_byte_in_run: new_byte as u32,
                            },
                            affinity: cursor.affinity,
                        },
                        cursor_loc.dom_id,
                        *new_node_id,
                    );
                }
            }
        }
    }
}
```

---

## Part 5: Implementation Roadmap

### Phase 1: Immediate Bug Fixes (Day 1)

1. **Fix UnifiedConstraints default** - Change to `MaxContent`
2. **Fix contenteditable check** - Also check `styled_node.contenteditable` boolean
3. **Verify scrollbar epsilon** - Add epsilon to visibility check

### Phase 2: CursorManager Merge (Day 2)

1. Add `CursorBlinkState` to SelectionManager
2. Add cursor methods to SelectionManager
3. Update call sites in window.rs, event_v2.rs
4. Remove CursorManager

### Phase 3: NodeData Hash Refactor (Day 3)

1. Implement `calculate_structural_hash()` 
2. Modify `reconcile_dom()` to use structural matching
3. Add cursor reconciliation to `transfer_states()`

### Phase 4: Testing & Validation (Day 4)

1. Run all e2e tests
2. Manual testing of contenteditable
3. Performance benchmarking of DOM diff

---

## Appendix A: Files to Modify

| File | Changes |
|------|---------|
| `layout/src/text3/cache.rs` | Fix `UnifiedConstraints::default()` |
| `layout/src/window.rs` | Check `contenteditable` boolean field |
| `layout/src/managers/cursor.rs` | Mark deprecated, merge into selection.rs |
| `layout/src/managers/selection.rs` | Add CursorBlinkState, cursor methods |
| `core/src/dom.rs` | Add `calculate_structural_hash()` |
| `core/src/diff.rs` | Use structural hash, add cursor reconciliation |

## Appendix B: Test Cases

1. Click on contenteditable → cursor appears AND focus is set
2. Type characters → text appears at cursor position
3. Long text → cursor stays visible (scrolls into view)
4. DOM regeneration → cursor position preserved
5. Select text → selection persists across frames
6. Resize window → no spurious line wrapping

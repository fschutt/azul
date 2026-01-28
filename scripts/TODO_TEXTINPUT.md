Here is the detailed analysis and fix plan for the reported bugs.

# 1. Root Cause Analysis

### Bug 1: Text Input Not Working ("No focused node")
**Cause:** The function `process_mouse_click_for_selection` in `layout/src/window.rs` successfully calculates the cursor position but fails to set the **keyboard focus** on the node.
The code attempts to check if the node is `contenteditable` by iterating over `styled_node.attributes`. However, `contenteditable` is a special property often handled during DOM construction or parsed into the `NodeData` struct directly. The current check fails to find the attribute, so it skips `self.focus_manager.set_focused_node(...)`.
Additionally, `record_text_input` strictly requires a focused node. Since the focus manager was never updated, the input is dropped.

### Bug 2: Line Wrapping When It Shouldn't
**Cause:** In `layout/src/text3/cache.rs`, `UnifiedConstraints::default()` initializes `available_width` to `AvailableSpace::Definite(0.0)`.
When the layout solver needs to calculate the *intrinsic size* (min-content/max-content) of an element, it often uses default constraints. Passing `Definite(0.0)` tells the text layout engine: "You have 0 pixels of width, wrap as much as possible." This forces hard wraps at every character or word boundary, resulting in a tall, skinny text block even when `white-space: nowrap` is desired or when measuring max-content.

### Bugs 4-7: Scrollbar Visibility & Geometry
**Cause:**
1.  **Visibility (Bug 4):** In `layout/src/solver3/fc.rs`, `check_scrollbar_necessity` likely compares floating point numbers directly (`content > container`). Due to float precision, `100.00001 > 100.0` returns true, triggering scrollbars when content fits perfectly.
2.  **Track Width/Position (Bug 5, 6):** The scrollbar rendering logic in `display_list.rs` relies on `scrollbar_info` calculated during layout. If the layout assumes scrollbars are present (due to Bug 4), it reserves space. The display list generation then calculates the track rect based on the *padding-box* of the node.
3.  **Border Detachment (Bug 7):** This is a side effect of the layout constraint issue. If the text engine wraps text prematurely (Bug 2), the calculated height of the text run doesn't match the container's height expectations, leading to the border (drawn on the container) being visually disconnected from the content (drawn by text3) which has collapsed or shifted.

---

# 2. Specific Code Fixes

## Fix 1: Enable Focus for ContentEditable
**File:** `layout/src/window.rs`
**Location:** Inside `process_mouse_click_for_selection` (approx line ~6486 in original)

We need a robust check that looks at both the `NodeData` boolean field AND the raw attributes.

```rust
        // ... [inside process_mouse_click_for_selection] ...

        // CRITICAL FIX: Check node_data.contenteditable boolean first, then attributes
        let is_contenteditable = self.layout_results.get(&dom_id)
            .map(|lr| {
                let node_hierarchy = lr.styled_dom.node_hierarchy.as_container();
                let node_data = lr.styled_dom.node_data.as_ref();
                
                // Walk up the DOM tree
                let mut current_node = Some(ifc_root_node_id);
                while let Some(node_id) = current_node {
                    if let Some(data) = node_data.get(node_id.index()) {
                        // 1. Check the optimized boolean field
                        if data.is_contenteditable() {
                            return true;
                        }
                        
                        // 2. Fallback: Check raw attributes
                        let has_attr = data.attributes.as_ref().iter().any(|attr| {
                            matches!(attr, azul_core::dom::AttributeType::ContentEditable(true))
                        });
                        if has_attr { 
                            return true; 
                        }
                    }
                    // Move to parent
                    current_node = node_hierarchy.get(node_id).and_then(|h| h.parent_id());
                }
                false
            })
            .unwrap_or(false);

        // ... [rest of the function] ...
```

## Fix 2: Prevent Premature Line Wrapping
**File:** `layout/src/text3/cache.rs`
**Location:** `impl Default for UnifiedConstraints`

Change the default width to `MaxContent` (effectively infinite). This ensures that unless a specific width constraint is passed, the text engine measures the text on a single line.

```rust
impl Default for UnifiedConstraints {
    fn default() -> Self {
        Self {
            shape_boundaries: Vec::new(),
            shape_exclusions: Vec::new(),

            // CRITICAL FIX: Use MaxContent instead of Definite(0.0)
            // Definite(0.0) forces wrapping at every character.
            // MaxContent allows text to flow naturally during intrinsic sizing.
            available_width: AvailableSpace::MaxContent,
            
            available_height: None,
            // ... [rest of struct] ...
        }
    }
}
```

## Fix 3: Fix macOS Input Pipeline
**File:** `dll/src/desktop/shell2/macos/events.rs`
**Location:** `handle_key_down`

Do **not** manually insert text in `handle_key_down`. Rely on `interpretKeyEvents:` which triggers the system IME, eventually calling `insertText:`. Manual insertion here bypasses IME and causes duplicates or "No focused node" errors if the timing is wrong.

```rust
    pub fn handle_key_down(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        self.previous_window_state = Some(self.current_window_state.clone());
        self.update_keyboard_state(key_code, modifiers, true);

        // CRITICAL FIX: Removed manual handle_text_input() call.
        // We rely entirely on [GLView/CPUView keyDown:] calling interpretKeyEvents:
        // which will callback into insertText:replacementRange:
        
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }
```

## Fix 4: Fix Scrollbar Visibility Logic (Epsilon Check)
**File:** `layout/src/solver3/fc.rs`
**Location:** `check_scrollbar_necessity`

Add an epsilon to the comparison to prevent floating point jitter from triggering scrollbars.

```rust
pub fn check_scrollbar_necessity(
    content_size: LogicalSize,
    container_size: LogicalSize,
    overflow_x: OverflowBehavior,
    overflow_y: OverflowBehavior,
) -> ScrollbarRequirements {
    
    // Epsilon for float comparison
    const EPSILON: f32 = 0.5; // 0.5px tolerance

    let mut needs_horizontal = match overflow_x {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        // FIX: Add epsilon
        OverflowBehavior::Auto => content_size.width > (container_size.width + EPSILON),
    };

    let mut needs_vertical = match overflow_y {
        OverflowBehavior::Visible | OverflowBehavior::Hidden | OverflowBehavior::Clip => false,
        OverflowBehavior::Scroll => true,
        // FIX: Add epsilon
        OverflowBehavior::Auto => content_size.height > (container_size.height + EPSILON),
    };

    // ... [rest of logic handling scrollbar overlap] ...
}
```

---

# 3. Architecture Recommendations

### 1. Reconciliation & State Persistence
Your current plan relies on `transfer_states` in `diff.rs`. This handles `RefAny` data well, but `CursorManager` and `ScrollManager` store state keyed by `(DomId, NodeId)`.
*   **Recommendation:** Extend `DiffResult` to include a `node_id_map` (Old -> New).
*   **Action:** In `LayoutWindow::regenerate_layout`, immediately after `reconcile_dom`, call a new method `self.scroll_manager.remap_node_ids(&diff.node_id_map)` and `self.cursor_manager.remap_node_ids(...)`. This ensures that if a text node moves from index 5 to 6, its scroll position and cursor location move with it.

### 2. IFC Root vs. Text Node Ambiguity
The code frequently converts between `NodeId` of a text node and `NodeId` of its parent (IFC Root).
*   **Recommendation:** Enforce a rule: **Text Editing logic always operates on the IFC Root NodeId**.
*   **Why:** The `inline_layout_result` is stored on the Root. `contenteditable` is usually on the Root.
*   **Implementation:** In `window.rs`, add a helper `resolve_to_ifc_root(node_id) -> NodeId`. Use this at the top of `record_text_input` and `process_mouse_click`.

### 3. Opt-in Text Input
Relying on scanning attributes `contenteditable="true"` is slow and fragile.
*   **Recommendation:** Add `is_text_input: bool` to `NodeData`. Set this flag during DOM construction (`Dom::text_input(...)`). Use this flag for hit-testing focusability. Keep `contenteditable` attribute for compatibility with HTML serialization/debugging.

---

# 4. Testing Strategy

1.  **Cursor Focus Test:**
    *   Run the app with `AZUL_DEBUG=1`.
    *   Click the text.
    *   **Verify:** Log output must show `[DEBUG] Set focus on contenteditable node...` (This confirms Fix 1 worked).

2.  **Typing Test:**
    *   Type 'hello'.
    *   **Verify:** Logs show `[GLView::insertText] Inserting text: 'h'`, then `e`, etc.
    *   **Verify:** No "No focused node" errors.

3.  **Layout/Wrap Test:**
    *   Type a very long string without spaces.
    *   **Verify:** The text extends *past* the right edge of the box (triggering horizontal scroll) instead of wrapping to a new line immediately. This confirms Fix 2 (`AvailableSpace::MaxContent`) is working.

4.  **Scrollbar Test:**
    *   Delete the text until it fits.
    *   **Verify:** The scrollbars disappear completely (Fix 4).
    *   Type until it overflows.
    *   **Verify:** Scrollbars appear and don't overlap the text content (Fix 4 & 5).

---


Okay, here are the files. First analyze the bug, I've also supplied a document with the original plan, how the entire loop is supposed to work - basically, the "contenteditable" just edits the visual state (the cache, which is stored on the IFC).

There is also a difference between the DomNodeId and the LayoutNode ID (which we can use to get the IFC if ANY is associated with this node). We don't need a "traverse parents" thing.

IMPORTANT: The architectural idea is that contenteditable just gives the application some basic "content editing" but doesn't particularly persist state. So, the next time you call `layout()` and anything returns `Update::RefreshDom`, the cursor state, selection state and text content would be "gone" again. This is intentional, in order to force the programmer to persist these changes into his app data model if he cares about the edits. What we do now need is a way of "recognizing" which nodes changed content. There is a "reconciliation" logic ALREADY present, but we have to extend it, so that it can also reconcile selections and cursor positions, so, the idea is:

1. user inputs text, onchange callback writes that changeset into the data model, but returns Update::DoNothing -> visual text updates, but Window.layout_cb() is NOT called - the text is just updated in the IFC keyed cache

2. something "big" happens and layout_cb is re-invoked, leading to a new StyledDom. Reconcile logic analyzes the nodes. The content of the <p> text now has changed, but the node is currently holding a cursor - now we need to see: are there diffs before the cursor / selection = shift the cursor / selection / scroll

3. this will lead to no visual change - in the "quick update" path, the cursor moves along with the user typing, in the "relayout everything" path, the cursor ends up at the same position again (otherwise we'd have to invalidate the cursor or selection)

We need to refactor the diff handling a bit: Azul does not work like a "classical" browser, but heavily relies on the diffing to bridge the gap between immediate-mode, reactive and retained-mode GUI. The goal is to get a single contenteditable with horizontal scroll state, cursor and selection to be "stable" across multiple layout() calls.

We need an "IFC diff" logic. The idea is that, for example, users can input Markdown text with backticks. Now, the challenge is to render it "visually" as Markdown, while also preserving the cursor position as the user is typing. But, where we previously had one "::text" node, we now have "text, span, other text, boldtext" - which would all render as different DOM items. The data model however only should store the raw Markdown text - but when the md2html2domtree is happening in the layout_cb - then we want to preserve the cursor position **as the user is still typing**. Analyze and recommend how to extend the "diff" logic with this architecture.

---

This approach leverages the existing diffing infrastructure by decoupling **identity** from **content**, and adds a post-reconciliation pass for cursor restoration.

### 1. Root Cause & Solution Strategy

*   **Identity Instability:** Currently, `NodeData::hash` includes the text content. When a user types a backtick, the content changes, the hash changes, and `reconcile_dom` sees a "Destroy Old / Create New" event instead of an "Update".
*   **The Fix:** Remove the string content from `NodeData`'s hash. `Text("A")` and `Text("B")` should hash identically. This allows `reconcile_dom` to match them as the "same" node (just updated), preserving the node's identity across edits.
*   **Cursor Mapping:** Once the nodes are matched (or their containers are matched), we use **Linear Content Projection**. We treat the parent container (the IFC root) as a single linear text buffer. We calculate the cursor's absolute byte offset in the old DOM and project it onto the new DOM's structure, regardless of how many `<span>`s were added or removed.

---

### 2. Code Changes

#### Step 1: Modify `NodeData` Hashing
**File:** `core/src/dom.rs`

We exclude the string payload from `NodeType::Text` hashing. This ensures that text nodes are matched by their position/type, not their content.

```rust
impl Hash for NodeData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // self.node_type.hash(state); // <-- OLD: Hashes content
        
        // NEW: Hash discriminator only for Text/Comment, full hash for others
        mem::discriminant(&self.node_type).hash(state);
        
        match &self.node_type {
            NodeType::Text(_) | NodeType::Comment(_) => {
                // Do NOT hash the string content. 
                // This allows Text("A") and Text("B") to be reconciled as the same node.
            },
            // For elements (Div, Image, etc.), we might want to hash specific fields
            // if they define identity, but usually type + key + order is enough.
            // For safety, we can stick to just hashing attributes/classes etc. below.
            _ => {
                // If NodeType has other data (like ImageRef), hash it here
                if let NodeType::Image(img) = &self.node_type {
                    img.hash(state);
                }
                // IFrame, Icon etc...
            }
        }

        self.dataset.hash(state);
        self.ids_and_classes.as_ref().hash(state);
        self.attributes.as_ref().hash(state);
        self.contenteditable.hash(state);
        // ... callbacks, css_props, extra ...
    }
}
```

#### Step 2: Implement `reconcile_selection`
**File:** `core/src/diff.rs`

This is the logic that runs *after* `reconcile_dom`. It takes the old selection, finds the corresponding location in the new DOM, and returns the new selection.

```rust
use crate::dom::NodeType;
use crate::selection::{Selection, SelectionRange, TextCursor, GraphemeClusterId, CursorAffinity};
use crate::id::NodeHierarchy;
use crate::dom::NodeData;

/// Reconciles the selection/cursor position after a DOM update.
/// 
/// This handles complex transformations where a single text node might split into
/// multiple spans (e.g., Markdown highlighting). It calculates the "global" byte 
/// offset within the container and maps it to the new structure.
pub fn reconcile_selection(
    old_selection: &Selection,
    old_dom: &NodeDataContainer<NodeData>,
    old_hierarchy: &NodeHierarchy,
    new_dom: &NodeDataContainer<NodeData>,
    new_hierarchy: &NodeHierarchy,
    node_moves: &FastHashMap<NodeId, NodeId>, // From DiffResult
) -> Option<Selection> {

    // Helper to get the absolute byte offset of a cursor within its block container
    let get_global_offset = |dom: &NodeDataContainer<NodeData>, hierarchy: &NodeHierarchy, node_id: NodeId, cursor: &TextCursor| -> Option<(NodeId, usize)> {
        // 1. Find the container (IFC Root)
        // Walk up until we find a non-inline element or root
        let mut container = node_id;
        while let Some(parent) = hierarchy[container].parent {
             // In a real implementation, check display property. 
             // For now, assume immediate parent of text is container.
             container = parent;
             break; 
        }
        
        // 2. Sum lengths of preceding siblings
        let mut offset = 0;
        let children = container.children(hierarchy); // Pseudo-code for iterator
        let mut found = false;
        
        for child in children {
            if child == node_id {
                offset += cursor.cluster_id.start_byte_in_run as usize;
                found = true;
                break;
            }
            
            if let NodeType::Text(s) = &dom[child].node_type {
                offset += s.as_str().len();
            } else {
                // Count non-text inline objects as length 1? 
                // Or 0? Depends on if they are selectable cursor positions.
            }
        }
        
        if found { Some((container, offset)) } else { None }
    };

    // Helper to resolve a global offset back to a specific node/local-offset
    let resolve_offset = |dom: &NodeDataContainer<NodeData>, hierarchy: &NodeHierarchy, container: NodeId, target_offset: usize| -> Option<(NodeId, TextCursor)> {
        let mut current_offset = 0;
        let children = container.children(hierarchy);
        
        let mut last_text_child = None;
        
        for child in children {
            if let NodeType::Text(s) = &dom[child].node_type {
                let len = s.as_str().len();
                let end = current_offset + len;
                last_text_child = Some(child);
                
                // If target is within this node (inclusive of end for caret)
                if target_offset <= end {
                    let local = target_offset - current_offset;
                    return Some((child, TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: 0, // Will be fixed by text layout
                            start_byte_in_run: local as u32,
                        },
                        affinity: CursorAffinity::Leading,
                    }));
                }
                current_offset = end;
            }
        }
        
        // Fallback: If we overshot (e.g. text deleted), clamp to end of last text node
        if let Some(child) = last_text_child {
            if let NodeType::Text(s) = &dom[child].node_type {
                 return Some((child, TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: 0,
                        start_byte_in_run: s.as_str().len() as u32,
                    },
                    affinity: CursorAffinity::Trailing,
                }));
            }
        }
        
        None
    };

    // --- Main Logic ---

    let process_cursor = |cursor_node: NodeId, cursor: &TextCursor| -> Option<(NodeId, TextCursor)> {
        // 1. Calculate old global offset
        let (old_container, global_offset) = get_global_offset(old_dom, old_hierarchy, cursor_node, cursor)?;
        
        // 2. Find new container using the diff map
        let new_container = node_moves.get(&old_container).copied()?;
        
        // 3. Resolve in new DOM
        resolve_offset(new_dom, new_hierarchy, new_container, global_offset)
    };

    match old_selection {
        Selection::Cursor(c) => {
             // We need to know WHICH node the cursor was on. 
             // Ideally Selection struct holds (NodeId, Cursor). 
             // Assuming we have access to the old focused node ID from somewhere context.
             // For this function signature, let's assume `old_selection` wraps the location info or we pass it in.
             // If Selection is just the cursor, we need to pass `old_focus_node`.
             
             // Placeholder: assuming we passed `old_focus_node` into this function
             // let (new_node, new_cursor) = process_cursor(old_focus_node, c)?;
             // Some(Selection::Cursor(new_cursor))
             None 
        },
        Selection::Range(r) => {
            // Similar logic for Start and End of range
            None
        }
    }
}
```

*Note: You will need to adjust the function signature of `reconcile_selection` to accept the `old_focus_node_id` since `Selection::Cursor` usually doesn't strictly imply the DOM Node ID in all contexts (though in `LayoutWindow` they are paired).*

#### Step 3: Integration in `LayoutWindow`

**File:** `layout/src/window.rs`

Update `regenerate_layout` (or wherever the DOM update happens).

```rust
pub fn regenerate_layout(&mut self) -> Result<(), String> {
    // ... [Inside the update loop] ...

    // 1. Reconcile DOM Structure (Diff)
    let diff = reconcile_dom(
        &old_dom_data, 
        &new_dom_data, 
        // ...
    );

    // 2. Reconcile Scroll & State (Existing logic)
    let migration_map = create_migration_map(&diff.node_moves);
    self.scroll_manager.remap_node_ids(dom_id, &migration_map);

    // 3. Reconcile Selection / Cursor (NEW)
    if let Some(focused) = self.focus_manager.get_focused_node() {
        if let Some(selection) = self.selection_manager.get_selection(dom_id) {
             let new_selection = reconcile_selection(
                 selection,
                 &old_dom.node_data,
                 &old_dom.node_hierarchy,
                 &new_dom.node_data,
                 &new_dom.node_hierarchy,
                 &migration_map // Or refactor to use the Vec directly
             );
             
             if let Some(s) = new_selection {
                 // We also need the new NodeId returned by resolve_offset
                 // For the cursor, set the new focus
                 // self.focus_manager.set_focused_node(...);
                 // self.selection_manager.set_selection(dom_id, s);
             }
        }
    }
    
    // ... [Continue with layout] ...
}
```

### 3. Verification Strategy

1.  **Stable Hash Test:** Create a unit test in `dom.rs`. Create `NodeData::Text("A")` and `NodeData::Text("B")`. Assert `calculate_node_data_hash()` returns the **same** value for both.
2.  **Split Test:**
    *   Setup `old_dom`: `div -> text("AB")`. Cursor at index 1 (between A and B).
    *   Setup `new_dom`: `div -> text("A"), span("X"), text("B")`.
    *   Map `old_div` to `new_div` manually in a `node_moves` map.
    *   Run `reconcile_selection`.
    *   Expect: New cursor at index 1 of the *first* text node (or index 0 of the span, depending on boundary preference, but functionally valid).
3.  **App Test:**
    *   Type backtick in the markdown editor.
    *   Ensure focus isn't lost (because the `div` matched).
    *   Ensure cursor doesn't jump to the start (because offset was mapped).


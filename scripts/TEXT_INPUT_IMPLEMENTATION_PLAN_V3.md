# Text Input Implementation Plan V3

## Executive Summary

This document provides the definitive implementation plan for Azul's text input system. The key architectural insight is the **dual-path layout system**:

1. **Initial Layout** → Runs on `StyledDom` (committed state from `layout()` callback)
2. **Relayout** → Runs on `LayoutCache` (respects quick edits, handles text node resizing)

This separation enables:
- Instant visual feedback during typing (no callback latency)
- Proper layout shift handling when text causes reflow
- Clean separation between "optimistic" and "committed" state
- Support for complex multi-node editing

---

## Part 1: Architectural Overview

### 1.1 The Two Layout Paths

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         INITIAL LAYOUT PATH                              │
│                                                                          │
│   User Data Model ──► layout() callback ──► StyledDom ──► LayoutCache   │
│        (RefAny)           (pure fn)         (committed)    (visual)      │
│                                                                          │
│   Triggered by: Update::RefreshDom, window resize, first render          │
└─────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────┐
│                          RELAYOUT PATH                                   │
│                                                                          │
│   LayoutCache ──► detect dirty nodes ──► partial relayout ──► repaint   │
│   (with edits)      (text changed)        (text only)         (fast)     │
│                                                                          │
│   Triggered by: Text input, cursor movement, selection change            │
└─────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Key Data Flow

```
User types 'a' 
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│ 1. Platform layer receives keypress (macOS: NSTextInputClient)  │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│ 2. TextInputManager.record_input() creates PendingTextEdit      │
│    - Records: inserted_text="a", old_text, node                  │
│    - Does NOT modify any caches yet                              │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│ 3. Synthetic 'Input' event generated for contenteditable node   │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│ 4. User's On::TextInput callback fires                          │
│    - Can call info.get_text_changeset() to inspect              │
│    - Can call info.prevent_default() to cancel                  │
│    - Updates their data model (RefAny)                          │
│    - Returns Update::DoNothing (fast) or Update::RefreshDom     │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│ 5. If NOT prevented: apply_text_changeset()                     │
│    - Calls text3::edit::edit_text() to compute new content      │
│    - Calls update_text_cache_after_edit() for visual update     │
│    - Updates cursor position                                     │
│    - Marks node as dirty for relayout                           │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│ 6. Relayout runs on dirty nodes                                 │
│    - Reads from LayoutCache (with edits), NOT StyledDom         │
│    - Handles text node resizing                                  │
│    - Propagates layout shifts to ancestors if needed            │
└──────────────────────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────────────────────┐
│ 7. Display list regenerated, repaint triggered                  │
└──────────────────────────────────────────────────────────────────┘
```

---

## Part 2: Data Structures

### 2.1 New Fields in LayoutWindow

```rust
// In layout/src/window.rs

pub struct LayoutWindow {
    // ... existing fields ...
    
    /// Cache of text layout constraints for each IFC root node.
    /// Used to perform consistent optimistic updates.
    pub text_constraints_cache: TextConstraintsCache,
    
    /// Tracks which nodes have been edited since last full layout.
    /// Key: (DomId, NodeId of IFC root)
    /// Value: The edited Vec<InlineContent> that should be used for relayout
    pub dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>,
}

#[derive(Debug, Clone)]
pub struct TextConstraintsCache {
    /// Constraints used for each IFC during initial layout
    pub constraints: BTreeMap<(DomId, NodeId), UnifiedConstraints>,
}

#[derive(Debug, Clone)]
pub struct DirtyTextNode {
    /// The new inline content (text + images) after editing
    pub content: Vec<InlineContent>,
    /// The new cursor position after editing
    pub cursor: Option<TextCursor>,
    /// Whether this edit requires ancestor relayout (e.g., text grew taller)
    pub needs_ancestor_relayout: bool,
}
```

### 2.2 UnifiedConstraints (Already Exists, Needs Caching)

```rust
// In layout/src/text3/cache.rs

#[derive(Debug, Clone)]
pub struct UnifiedConstraints {
    pub available_width: AvailableSpace,
    pub text_align: StyleTextAlign,
    pub direction: Option<BidiDirection>,
    pub writing_mode: WritingMode,
    pub line_height: LineHeight,
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,
    pub white_space: WhiteSpace,
    pub text_indent: f32,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    // ... etc
}
```

### 2.3 Enhanced PendingTextEdit

```rust
// In layout/src/managers/text_input.rs

#[derive(Debug, Clone)]
pub struct PendingTextEdit {
    /// The IFC root node being edited
    pub node: DomNodeId,
    /// The text that was inserted (can be empty for deletions)
    pub inserted_text: String,
    /// The old Vec<InlineContent> before the edit
    pub old_content: Vec<InlineContent>,
    /// The new Vec<InlineContent> after the edit (computed by text3::edit)
    pub new_content: Option<Vec<InlineContent>>,
    /// The new cursor position after the edit
    pub new_cursor: Option<TextCursor>,
    /// Source of the edit
    pub source: TextInputSource,
}
```

---

## Part 3: Event Handling Flow

### 3.1 Platform Layer → TextInputManager

**File:** `dll/src/desktop/shell2/macos/text_input.rs` (and Windows/Linux equivalents)

```rust
// When user types a character:

fn insert_text(&mut self, string: &str) {
    let layout_window = self.get_layout_window_mut();
    
    // 1. Find the focused contenteditable node
    let focused_node = layout_window.focus_manager.get_focused_node();
    let Some(node_id) = focused_node else { return };
    
    // 2. Get current content from cache (NOT StyledDom!)
    let old_content = layout_window.get_current_inline_content(node_id);
    
    // 3. Record the input (Phase 1 - just record, don't apply)
    layout_window.text_input_manager.record_input(
        node_id,
        string.to_string(),
        old_content,
        TextInputSource::Keyboard,
    );
    
    // 4. Generate synthetic Input event
    layout_window.pending_events.push(SyntheticEvent::new(
        EventType::Input,
        EventSource::User,
        node_id,
        Instant::now(),
        EventData::None,
    ));
}
```

### 3.2 Event Dispatch → User Callback

**File:** `dll/src/desktop/shell2/common/event_v2.rs`

```rust
fn process_input_event(
    &mut self,
    event: &SyntheticEvent,
    layout_window: &mut LayoutWindow,
) -> ProcessedCallbackResult {
    // 1. Find callbacks registered for On::TextInput on this node
    let callbacks = layout_window.get_callbacks_for_event(event);
    
    // 2. Invoke each callback
    let mut result = ProcessedCallbackResult::default();
    for callback_data in callbacks {
        let callback_info = CallbackInfo::new(layout_window, event);
        
        // User callback can:
        // - info.get_text_changeset() to see what's being inserted
        // - info.prevent_default() to cancel the edit
        // - Modify their RefAny data model
        let update = (callback_data.callback.cb)(
            callback_data.refany.clone(),
            callback_info,
        );
        
        result.update.max_self(update);
    }
    
    // 3. Check if prevented
    if result.changes.contains(&CallbackChange::PreventDefault) {
        layout_window.text_input_manager.clear_changeset();
        return result;
    }
    
    // 4. Apply the changeset (Phase 2)
    layout_window.apply_text_changeset();
    
    result
}
```

### 3.3 Applying the Changeset

**File:** `layout/src/window.rs`

```rust
pub fn apply_text_changeset(&mut self) {
    let Some(changeset) = self.text_input_manager.pending_changeset.take() else {
        return;
    };
    
    let dom_id = changeset.node.dom;
    let node_id = changeset.node.node.into_crate_internal().unwrap();
    
    // 1. Get current cursor position
    let current_cursor = self.cursor_manager.get_cursor();
    
    // 2. Get the old content (from cache if dirty, else from original layout)
    let old_content = self.get_current_inline_content_internal(dom_id, node_id);
    
    // 3. Compute the edit using text3::edit
    let edit_result = crate::text3::edit::edit_text(
        &old_content,
        &changeset.inserted_text,
        current_cursor,
        self.selection_manager.get_selection(dom_id),
    );
    
    let Some((new_content, new_selections)) = edit_result else {
        return;
    };
    
    // 4. Update the visual cache (optimistic update)
    self.update_text_cache_after_edit(dom_id, node_id, new_content.clone());
    
    // 5. Update cursor position
    if let Some(new_cursor) = new_selections.cursor {
        let now = Instant::now();
        self.cursor_manager.set_cursor_with_time(
            Some(new_cursor),
            Some(CursorLocation { dom_id, node_id }),
            now,
        );
    }
    
    // 6. Mark node as dirty for relayout
    self.dirty_text_nodes.insert((dom_id, node_id), DirtyTextNode {
        content: new_content,
        cursor: new_selections.cursor,
        needs_ancestor_relayout: false, // Will be determined during relayout
    });
    
    // 7. Schedule relayout
    self.needs_relayout = true;
}
```

---

## Part 4: The Dual Layout System

### 4.1 Initial Layout (StyledDom Path)

**File:** `layout/src/solver3/mod.rs`

```rust
/// Full layout pass - reads from StyledDom
/// Called on: first render, Update::RefreshDom, window resize
pub fn layout_document(
    styled_dom: &StyledDom,
    constraints: &LayoutConstraints,
    font_manager: &mut FontManager,
    // ... other params
) -> LayoutTree {
    // Clear dirty nodes - we're rebuilding from committed state
    // The dirty_text_nodes map should be cleared by caller
    
    // Traverse StyledDom and build layout tree
    for node in styled_dom.nodes() {
        match node.node_type {
            NodeType::Text(ref text) => {
                // Convert text to InlineContent
                let inline_content = text_to_inline_content(text, node.style);
                // Layout the text
                let layout = layout_inline_formatting_context(inline_content, ...);
                // Cache the constraints for later relayout
                ctx.text_constraints_cache.insert((dom_id, node_id), constraints);
            }
            // ... other node types
        }
    }
}
```

### 4.2 Relayout (LayoutCache Path)

**File:** `layout/src/window.rs`

```rust
/// Partial relayout - respects dirty text nodes
/// Called on: text input, when needs_relayout is true
pub fn relayout_dirty_nodes(&mut self) {
    if !self.needs_relayout || self.dirty_text_nodes.is_empty() {
        return;
    }
    
    for ((dom_id, node_id), dirty_node) in self.dirty_text_nodes.iter() {
        // 1. Get cached constraints
        let Some(constraints) = self.text_constraints_cache.constraints.get(&(*dom_id, *node_id)) else {
            continue;
        };
        
        // 2. Re-run lightweight text layout
        let new_layout = self.relayout_text_node(*dom_id, *node_id, &dirty_node.content, constraints);
        
        let Some(new_layout) = new_layout else {
            continue;
        };
        
        // 3. Check if size changed (needs ancestor relayout)
        let old_size = self.get_node_size(*dom_id, *node_id);
        let new_size = new_layout.bounds().size();
        
        if old_size.height != new_size.height || old_size.width != new_size.width {
            // Text node changed size - need to propagate layout shift
            self.propagate_layout_shift(*dom_id, *node_id, old_size, new_size);
        }
        
        // 4. Update the cache
        self.update_layout_cache(*dom_id, *node_id, new_layout);
    }
    
    self.needs_relayout = false;
    self.needs_display_list_update = true;
}

fn relayout_text_node(
    &self,
    dom_id: DomId,
    node_id: NodeId,
    content: &[InlineContent],
    constraints: &UnifiedConstraints,
) -> Option<UnifiedLayout> {
    use crate::text3::cache::{
        create_logical_items, reorder_logical_items, 
        shape_visual_items, perform_fragment_layout, BreakCursor
    };
    
    // Stage 1: Create logical items from InlineContent
    let logical_items = create_logical_items(content, &[], &mut None);
    
    // Stage 2: Bidi reordering
    let base_direction = constraints.direction.unwrap_or(BidiDirection::Ltr);
    let visual_items = reorder_logical_items(&logical_items, base_direction, &mut None)?;
    
    // Stage 3: Shape text (resolve fonts, create glyphs)
    let loaded_fonts = self.font_manager.get_loaded_fonts();
    let shaped_items = shape_visual_items(
        &visual_items,
        self.font_manager.get_font_chain_cache(),
        &self.font_manager.fc_cache,
        &loaded_fonts,
        &mut None,
    )?;
    
    // Stage 4: Fragment layout (line breaking, positioning)
    let mut cursor = BreakCursor::new(&shaped_items);
    perform_fragment_layout(&mut cursor, &logical_items, constraints, &mut None, &loaded_fonts).ok()
}
```

### 4.3 Layout Shift Propagation

```rust
fn propagate_layout_shift(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    old_size: LogicalSize,
    new_size: LogicalSize,
) {
    // When a text node changes size, ancestors may need relayout
    // This is the "layout shift" that can cascade up the tree
    
    let height_delta = new_size.height - old_size.height;
    let width_delta = new_size.width - old_size.width;
    
    if height_delta.abs() < 0.001 && width_delta.abs() < 0.001 {
        return; // No significant change
    }
    
    // For now: mark that we need full relayout for this DOM
    // Future optimization: incremental ancestor relayout
    self.needs_full_relayout.insert(dom_id);
}
```

---

## Part 5: Cursor and Selection

### 5.1 Cursor Click Positioning

**File:** `layout/src/text3/selection.rs`

```rust
use azul_core::geom::LogicalPosition;
use azul_core::selection::{TextCursor, CursorAffinity, GraphemeClusterId};
use crate::text3::cache::{UnifiedLayout, PositionedItem, ShapedItem};
use std::collections::BTreeMap;

/// Maps a click position to a TextCursor within a UnifiedLayout.
/// The `point` must be relative to the layout's container origin.
pub fn hit_test_text_at_point(
    layout: &UnifiedLayout,
    point: LogicalPosition,
) -> Option<TextCursor> {
    if layout.items.is_empty() {
        // Empty contenteditable - cursor at beginning
        return Some(TextCursor {
            cluster_id: GraphemeClusterId::default(),
            affinity: CursorAffinity::Leading,
        });
    }
    
    // Step 1: Find the line closest to the Y coordinate
    let mut line_bounds: BTreeMap<usize, (f32, f32)> = BTreeMap::new();
    for item in &layout.items {
        let bounds = item.item.bounds();
        let entry = line_bounds.entry(item.line_index).or_insert((f32::MAX, f32::MIN));
        entry.0 = entry.0.min(item.position.y);
        entry.1 = entry.1.max(item.position.y + bounds.height);
    }
    
    let closest_line = line_bounds.iter()
        .min_by(|(_, (a_min, a_max)), (_, (b_min, b_max))| {
            let a_center = (a_min + a_max) / 2.0;
            let b_center = (b_min + b_max) / 2.0;
            (point.y - a_center).abs().partial_cmp(&(point.y - b_center).abs()).unwrap()
        })
        .map(|(idx, _)| *idx)
        .unwrap_or(0);
    
    // Step 2: Find the closest cluster on that line
    let clusters_on_line: Vec<_> = layout.items.iter()
        .filter(|item| item.line_index == closest_line)
        .filter(|item| item.item.as_cluster().is_some())
        .collect();
    
    if clusters_on_line.is_empty() {
        // Empty line - find previous line's last cluster
        return layout.items.iter().rev()
            .filter(|item| item.line_index < closest_line)
            .find_map(|item| item.item.as_cluster().map(|c| TextCursor {
                cluster_id: c.source_cluster_id,
                affinity: CursorAffinity::Trailing,
            }));
    }
    
    let closest_cluster = clusters_on_line.iter()
        .min_by(|a, b| {
            let a_dist = horizontal_distance(point.x, a);
            let b_dist = horizontal_distance(point.x, b);
            a_dist.partial_cmp(&b_dist).unwrap()
        })?;
    
    let cluster = closest_cluster.item.as_cluster()?;
    
    // Step 3: Determine affinity (leading vs trailing half)
    let cluster_mid_x = closest_cluster.position.x + cluster.advance / 2.0;
    let affinity = if point.x < cluster_mid_x {
        CursorAffinity::Leading
    } else {
        CursorAffinity::Trailing
    };
    
    Some(TextCursor {
        cluster_id: cluster.source_cluster_id,
        affinity,
    })
}

fn horizontal_distance(x: f32, item: &PositionedItem) -> f32 {
    let bounds = item.item.bounds();
    let left = item.position.x;
    let right = left + bounds.width;
    
    if x < left {
        left - x
    } else if x > right {
        x - right
    } else {
        0.0
    }
}
```

### 5.2 Focus Transfer

**File:** `layout/src/window.rs`

```rust
/// Handles focus change for cursor blinking
/// Returns the action the platform should take for the blink timer
pub fn handle_focus_change_for_cursor_blink(
    &mut self,
    old_focus: Option<DomNodeId>,
    new_focus: Option<DomNodeId>,
) -> CursorBlinkTimerAction {
    // Clear old cursor if focus was on a contenteditable
    if let Some(old_node) = old_focus {
        if self.is_node_contenteditable(old_node) {
            self.cursor_manager.clear();
        }
    }
    
    // Initialize new cursor if focus is on a contenteditable
    if let Some(new_node) = new_focus {
        if self.is_node_contenteditable(new_node) {
            // Set flag for deferred initialization (will be overridden by click)
            self.focus_manager.set_pending_contenteditable_focus(
                new_node.dom,
                new_node.node.into_crate_internal().unwrap(),
            );
            return CursorBlinkTimerAction::Start;
        }
    }
    
    CursorBlinkTimerAction::Stop
}

/// Called after layout pass to finalize deferred focus changes
pub fn finalize_pending_focus_changes(&mut self) {
    if let Some((dom_id, node_id)) = self.focus_manager.take_pending_contenteditable_focus() {
        // Get the layout for this node
        if let Some(layout) = self.get_inline_layout_for_node(dom_id, node_id) {
            // Place cursor at end of text
            let cursor = get_cursor_at_end(&layout);
            let now = Instant::now();
            self.cursor_manager.set_cursor_with_time(
                Some(cursor),
                Some(CursorLocation { dom_id, node_id }),
                now,
            );
        }
    }
}
```

---

## Part 6: Callback Info API

### 6.1 Text Changeset Access

**File:** `layout/src/callbacks.rs`

```rust
impl CallbackInfo {
    /// Get the pending text changeset for the current Input event.
    /// Returns None if this is not a text input event.
    pub fn get_text_changeset(&self) -> Option<&PendingTextEdit> {
        self.get_layout_window()
            .text_input_manager
            .get_pending_changeset()
    }
    
    /// Prevent the default text input behavior.
    /// The typed character will not be inserted.
    pub fn prevent_default(&mut self) {
        self.push_change(CallbackChange::PreventDefault);
    }
    
    /// Override the text that will be inserted.
    /// Useful for input filtering or transformation.
    pub fn set_text_changeset(&mut self, new_text: String) {
        self.push_change(CallbackChange::SetInsertedText { text: new_text });
    }
    
    /// Change the text of a node (for TextInput widget pattern)
    pub fn change_node_text(&mut self, node_id: DomNodeId, new_text: AzString) {
        self.push_change(CallbackChange::ChangeNodeText { node_id, text: new_text });
    }
}
```

### 6.2 TextInput Widget Pattern (Reference)

**File:** `layout/src/widgets/text_input.rs`

```rust
/// The TextInput widget demonstrates the "controlled component" pattern:
/// 1. Widget has internal state (TextInputStateWrapper)
/// 2. On text input, callback fires BEFORE visual update
/// 3. Callback can validate/transform input
/// 4. If valid, callback updates its internal state
/// 5. Callback calls info.change_node_text() for visual update

extern "C" fn default_on_text_input(text_input: RefAny, info: CallbackInfo) -> Update {
    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;
    
    // 1. Get the changeset
    let changeset = info.get_text_changeset()?;
    let inserted_text = changeset.inserted_text.clone();
    
    if inserted_text.is_empty() {
        return Update::DoNothing;
    }
    
    // 2. Call user's validation callback if set
    let validation_result = if let Some(on_text_input) = &text_input.on_text_input {
        let new_state = compute_new_state(&text_input.inner, &inserted_text);
        (on_text_input.callback.cb)(on_text_input.refany.clone(), info.clone(), new_state)
    } else {
        OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
    };
    
    // 3. If valid, apply the change
    if validation_result.valid == TextInputValid::Yes {
        // Update internal state
        text_input.inner.text.extend(inserted_text.chars().map(|c| c as u32));
        text_input.inner.cursor_pos += inserted_text.len();
        
        // Update visual (for custom TextInput widget)
        let label_node_id = get_label_node_id(&info);
        info.change_node_text(label_node_id, text_input.inner.get_text().into());
    } else {
        // Prevent the edit
        info.prevent_default();
    }
    
    validation_result.update
}
```

---

## Part 7: Implementation Steps

### Step 1: Add TextConstraintsCache (Day 1)

**Files to modify:**
- `layout/src/window.rs` - Add `text_constraints_cache` field to `LayoutWindow`
- `layout/src/solver3/fc.rs` - Cache constraints during IFC layout

```rust
// In layout/src/window.rs
impl LayoutWindow {
    pub fn new(...) -> Self {
        Self {
            // ... existing fields
            text_constraints_cache: TextConstraintsCache::default(),
            dirty_text_nodes: BTreeMap::new(),
            needs_relayout: false,
        }
    }
}

// In layout/src/solver3/fc.rs, in layout_inline_formatting_context()
// After creating constraints:
if let Some(cache) = ctx.text_constraints_cache.as_mut() {
    cache.constraints.insert((ctx.dom_id, ifc_root_node_id), constraints.clone());
}
```

### Step 2: Implement update_text_cache_after_edit (Day 1-2)

**File:** `layout/src/window.rs`

Replace the TODO stub with the full implementation from Part 4.2.

### Step 3: Add hit_test_text_at_point (Day 2)

**File:** `layout/src/text3/selection.rs`

Add the function from Part 5.1.

### Step 4: Integrate with MouseDown Handler (Day 2-3)

**File:** `dll/src/desktop/shell2/common/event_v2.rs`

```rust
// In handle_mouse_down or process_mouse_event:

fn handle_mouse_down_for_text(
    &mut self,
    position: LogicalPosition,
    layout_window: &mut LayoutWindow,
) {
    // 1. Hit test to find node under cursor
    let hit_result = layout_window.hit_test_point(position);
    
    // 2. Check if it's a contenteditable
    if let Some(hit_node) = hit_result.deepest_contenteditable() {
        let dom_id = hit_node.dom;
        let node_id = hit_node.node.into_crate_internal().unwrap();
        
        // 3. Get the inline layout for hit testing
        if let Some(inline_layout) = layout_window.get_inline_layout_for_node(dom_id, node_id) {
            // 4. Calculate local position relative to node
            let node_pos = layout_window.get_node_position(hit_node).unwrap_or_default();
            let local_pos = LogicalPosition {
                x: position.x - node_pos.x,
                y: position.y - node_pos.y,
            };
            
            // 5. Hit test for cursor position
            if let Some(cursor) = hit_test_text_at_point(&inline_layout, local_pos) {
                // 6. Set focus
                let old_focus = layout_window.focus_manager.get_focused_node();
                layout_window.focus_manager.set_focused_node(Some(hit_node));
                
                // 7. Handle focus change (stops old timer, starts new)
                layout_window.handle_focus_change_for_cursor_blink(old_focus, Some(hit_node));
                
                // 8. Set cursor position (overrides deferred init)
                let now = Instant::now();
                layout_window.cursor_manager.set_cursor_with_time(
                    Some(cursor),
                    Some(CursorLocation { dom_id, node_id }),
                    now,
                );
                
                // 9. Clear any selection
                layout_window.selection_manager.clear_text_selection(&dom_id);
            }
        }
    }
}
```

### Step 5: Implement relayout_dirty_nodes (Day 3-4)

**File:** `layout/src/window.rs`

Add the function from Part 4.2 and integrate it into the render loop.

### Step 6: Add Event Processing Integration (Day 4)

**File:** `dll/src/desktop/shell2/common/event_v2.rs`

Ensure the Input event triggers the correct flow from Part 3.2.

### Step 7: Testing (Day 5)

Create test cases for:
1. Single character insertion
2. Backspace deletion
3. Multi-character paste
4. Cursor positioning on click
5. Focus transfer between inputs
6. Text that causes layout shift (line wrap)

---

## Part 8: Test Cases

### 8.1 Basic Text Input

```c
// tests/e2e/contenteditable_basic.c

void test_single_char_input() {
    // 1. Focus contenteditable
    // 2. Type 'a'
    // Expected: 'a' appears, cursor moves right
    // StyledDom: unchanged
    // LayoutCache: updated
}

void test_backspace() {
    // 1. Focus contenteditable with "hello"
    // 2. Press Backspace
    // Expected: 'hell' remains, cursor at end
}

void test_paste() {
    // 1. Focus empty contenteditable
    // 2. Paste "hello world"
    // Expected: full text appears, cursor at end
}
```

### 8.2 Cursor Positioning

```c
void test_click_positioning() {
    // 1. Create contenteditable with "hello world"
    // 2. Click in middle of "world"
    // Expected: cursor appears between 'o' and 'r'
}

void test_click_empty_line() {
    // 1. Create contenteditable with "line1\n\nline3"
    // 2. Click on empty line 2
    // Expected: cursor at start of line 2
}
```

### 8.3 Focus Transfer

```c
void test_focus_transfer_click() {
    // 1. Two contenteditables
    // 2. Focus first, type "hello"
    // 3. Click on second
    // Expected: first cursor gone, second cursor at click position
}

void test_focus_transfer_tab() {
    // 1. Two contenteditables
    // 2. Focus first
    // 3. Press Tab
    // Expected: second gets focus, cursor at end of its text
}
```

### 8.4 Layout Shift

```c
void test_line_wrap() {
    // 1. Create narrow contenteditable (100px wide)
    // 2. Type long text that wraps to second line
    // Expected: text wraps, container height increases
}
```

---

## Part 9: Known Limitations & Future Work

### Current Scope (V3)
- Single text node editing
- Basic cursor positioning
- Focus transfer
- Text-only content

### Future Work (V4+)
- Multi-node selections (bold/italic spans)
- Inline images from clipboard
- Undo/redo integration
- IME composition support
- Right-to-left text
- Vertical writing modes

---

## Appendix: Key File Locations

| Component | File | Key Functions |
|-----------|------|---------------|
| TextInputManager | `layout/src/managers/text_input.rs` | `record_input()`, `get_pending_changeset()` |
| CursorManager | `layout/src/managers/cursor.rs` | `set_cursor_with_time()`, `clear()` |
| FocusManager | `layout/src/managers/focus_cursor.rs` | `set_focused_node()`, `set_pending_contenteditable_focus()` |
| Window Coordination | `layout/src/window.rs` | `apply_text_changeset()`, `update_text_cache_after_edit()`, `relayout_dirty_nodes()` |
| Text Editing | `layout/src/text3/edit.rs` | `edit_text()`, `insert_text()`, `delete_range()` |
| Text Layout | `layout/src/text3/cache.rs` | `perform_fragment_layout()`, `shape_visual_items()` |
| Event Handling | `dll/src/desktop/shell2/common/event_v2.rs` | `process_input_event()`, `handle_mouse_down_for_text()` |
| CallbackInfo | `layout/src/callbacks.rs` | `get_text_changeset()`, `prevent_default()` |
| Display List | `layout/src/solver3/display_list.rs` | Cursor/selection rendering |

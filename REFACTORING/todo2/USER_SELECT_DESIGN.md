# User-Select System Design

## Overview

Comprehensive design for text selection, contenteditable, and clipboard integration in Azul. This document describes how mouse clicks, text selection, and keyboard shortcuts (Ctrl+C/X/A) integrate with the existing event system, focus management, and text layout.

## Architecture Components

### 1. CSS `user-select` Property (✅ Implemented)

```rust
// css/src/props/style/text.rs
pub enum StyleUserSelect {
    Auto,   // Default browser behavior
    Text,   // Text is selectable
    None,   // Text is not selectable
    All,    // Select entire element content on click
}
```

**Current State**: Property parsing, caching, and `is_text_selectable()` check implemented.

### 2. Selection State Management

#### 2.1 Core Types (✅ Exists in `core/src/selection.rs`)

```rust
pub struct TextCursor {
    pub cluster_id: GraphemeClusterId,
    pub affinity: CursorAffinity,
}

pub struct SelectionRange {
    pub start: TextCursor,
    pub end: TextCursor,
}

pub enum Selection {
    Cursor(TextCursor),    // Blinking cursor (contenteditable)
    Range(SelectionRange), // Text selection
}

pub struct SelectionState {
    pub selections: Vec<Selection>, // Sorted, non-overlapping
    pub node_id: DomNodeId,
}
```

#### 2.2 Selection Manager (🔨 Needs Implementation)

```rust
// layout/src/managers/selection.rs

/// Manages all active text selections across all DOMs
pub struct SelectionManager {
    /// Map of DOM ID -> list of selections
    selections: BTreeMap<DomId, Vec<SelectionState>>,
    
    /// Current multi-click state (for double/triple click)
    click_state: ClickState,
}

#[derive(Debug, Clone)]
struct ClickState {
    /// Node ID of last click
    last_node: Option<DomNodeId>,
    /// Position of last click
    last_position: LogicalPosition,
    /// Timestamp of last click
    last_time: Instant,
    /// Number of consecutive clicks (1=single, 2=double, 3=triple)
    click_count: u8,
}

impl SelectionManager {
    /// Handle single click: set cursor or clear selection
    pub fn handle_single_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
        focus_manager: &FocusManager,
        now: Instant,
    ) -> SelectionAction;
    
    /// Handle double click: select word
    pub fn handle_double_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
    ) -> SelectionAction;
    
    /// Handle triple click: select paragraph/line
    pub fn handle_triple_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
    ) -> SelectionAction;
    
    /// Handle click + drag: update selection range
    pub fn handle_drag(
        &mut self,
        dom_id: DomId,
        start_position: LogicalPosition,
        current_position: LogicalPosition,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
    ) -> SelectionAction;
    
    /// Get all selected content for copy/cut operations
    pub fn get_clipboard_content(
        &self,
        dom_id: DomId,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
    ) -> ClipboardContent;
    
    /// Select all selectable content in focused node
    pub fn select_all(
        &mut self,
        dom_id: DomId,
        focus_manager: &FocusManager,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
    ) -> SelectionAction;
    
    /// Clear all selections (called on focus change)
    pub fn clear_all(&mut self);
    
    /// Clear selections for specific DOM
    pub fn clear_dom(&mut self, dom_id: DomId);
}

/// Action to take after selection change
pub enum SelectionAction {
    /// Need to redraw to show new selection
    Redraw,
    /// Set text cursor in contenteditable
    SetCursor(TextCursor),
    /// Set focus to node
    SetFocus(DomNodeId),
    /// Clear existing selection
    ClearSelection,
    /// No action needed
    None,
}
```

### 3. Click Detection & Multi-Click Handling

#### 3.1 Click Count Detection (🔨 Needs Implementation)

```rust
// layout/src/managers/selection.rs

const DOUBLE_CLICK_TIME_MS: u64 = 500;  // 500ms between clicks
const DOUBLE_CLICK_DISTANCE_PX: f32 = 5.0; // 5px max movement

impl SelectionManager {
    fn update_click_count(
        &mut self,
        node_id: DomNodeId,
        position: LogicalPosition,
        now: Instant,
    ) -> u8 {
        let should_increment = if let Some(last_node) = self.click_state.last_node {
            // Same node, within time limit, close position
            last_node == node_id
                && now.duration_since(self.click_state.last_time).as_millis() 
                   < DOUBLE_CLICK_TIME_MS as u128
                && distance(position, self.click_state.last_position) 
                   < DOUBLE_CLICK_DISTANCE_PX
        } else {
            false
        };

        let click_count = if should_increment {
            (self.click_state.click_count + 1).min(3) // Cap at triple-click
        } else {
            1 // Reset to single click
        };

        self.click_state = ClickState {
            last_node: Some(node_id),
            last_position: position,
            last_time: now,
            click_count,
        };

        click_count
    }
}
```

#### 3.2 Integration with Mouse Event Handler (🔨 Needs Implementation)

```rust
// layout/src/window.rs or dll/src/desktop/shell2/.../events.rs

impl LayoutWindow {
    pub fn handle_mouse_down(
        &mut self,
        dom_id: DomId,
        position: LogicalPosition,
        button: MouseButton,
        now: Instant,
    ) -> ProcessEventResult {
        if button != MouseButton::Left {
            return ProcessEventResult::DoNothing;
        }

        // Hit test to find node
        let Some(hit_node_id) = self.hit_test(dom_id, position) else {
            return ProcessEventResult::DoNothing;
        };

        let layout_result = &self.layout_results[&dom_id];
        let styled_dom = &layout_result.styled_dom;
        let node_id = hit_node_id.node;

        // Check if text is selectable
        if !self.is_text_selectable(styled_dom, node_id) {
            // Not selectable - clear selection and propagate event normally
            self.selection_manager.clear_dom(dom_id);
            return ProcessEventResult::PropagateEvent;
        }

        // Update click count (single/double/triple)
        let click_count = self.selection_manager.update_click_count(
            hit_node_id,
            position,
            now,
        );

        // Handle based on click count
        let action = match click_count {
            1 => self.selection_manager.handle_single_click(
                dom_id, node_id, position, styled_dom, 
                &self.text_cache, &self.focus_manager, now
            ),
            2 => self.selection_manager.handle_double_click(
                dom_id, node_id, position, styled_dom, &self.text_cache
            ),
            3 => self.selection_manager.handle_triple_click(
                dom_id, node_id, position, styled_dom, &self.text_cache
            ),
            _ => SelectionAction::None,
        };

        self.process_selection_action(action)
    }
}
```

### 4. Mouse Drag Selection (🔨 Needs Implementation)

```rust
// Track drag state in LayoutWindow
pub struct LayoutWindow {
    // ... existing fields
    selection_drag_state: Option<SelectionDragState>,
}

struct SelectionDragState {
    dom_id: DomId,
    start_node: NodeId,
    start_position: LogicalPosition,
}

impl LayoutWindow {
    pub fn handle_mouse_move(
        &mut self,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        // If dragging selection, update selection range
        if let Some(drag) = &self.selection_drag_state {
            let layout_result = &self.layout_results[&drag.dom_id];
            let action = self.selection_manager.handle_drag(
                drag.dom_id,
                drag.start_position,
                position,
                &layout_result.styled_dom,
                &self.text_cache,
            );
            return self.process_selection_action(action);
        }

        // ... existing mouse move handling
        ProcessEventResult::DoNothing
    }

    pub fn handle_mouse_up(
        &mut self,
        position: LogicalPosition,
        button: MouseButton,
    ) -> ProcessEventResult {
        if button == MouseButton::Left {
            // End selection drag
            self.selection_drag_state = None;
        }

        // ... existing mouse up handling
        ProcessEventResult::DoNothing
    }
}
```

### 5. Text Selection Algorithms (🔨 Needs Implementation)

```rust
// layout/src/text3/selection.rs (new file)

use azul_core::selection::{Selection, SelectionRange, TextCursor};
use crate::text3::cache::TextLayoutCache;

/// Convert pixel position to text cursor
pub fn position_to_cursor(
    node_id: NodeId,
    position: LogicalPosition,
    text_cache: &TextLayoutCache,
) -> Option<TextCursor> {
    let shaped_text = text_cache.get_shaped_text(node_id)?;
    
    // Use existing hit-test logic from InlineText
    let hit = shaped_text.hit_test(position)?;
    
    Some(TextCursor {
        cluster_id: hit.cluster_id,
        affinity: hit.affinity,
    })
}

/// Select word at cursor position
pub fn select_word_at_cursor(
    cursor: &TextCursor,
    content: &[InlineContent],
) -> Option<SelectionRange> {
    // Find word boundaries using Unicode word break algorithm
    let word_start = find_word_start(cursor, content)?;
    let word_end = find_word_end(cursor, content)?;
    
    Some(SelectionRange {
        start: word_start,
        end: word_end,
    })
}

/// Select paragraph/line at cursor position
pub fn select_paragraph_at_cursor(
    cursor: &TextCursor,
    content: &[InlineContent],
) -> Option<SelectionRange> {
    // Find paragraph boundaries (newline characters)
    let para_start = find_paragraph_start(cursor, content)?;
    let para_end = find_paragraph_end(cursor, content)?;
    
    Some(SelectionRange {
        start: para_start,
        end: para_end,
    })
}

/// Create selection range from two cursors
pub fn range_from_positions(
    start_cursor: TextCursor,
    end_cursor: TextCursor,
) -> SelectionRange {
    // Ensure start < end logically
    if start_cursor.cluster_id <= end_cursor.cluster_id {
        SelectionRange {
            start: start_cursor,
            end: end_cursor,
        }
    } else {
        SelectionRange {
            start: end_cursor,
            end: start_cursor,
        }
    }
}

/// Get all text content within selection range
pub fn extract_selection_content(
    selection: &SelectionRange,
    content: &[InlineContent],
) -> Vec<InlineContent> {
    // Extract text between start and end cursors
    // Preserve styling information
    todo!()
}
```

### 6. Clipboard Integration (🔨 Needs Implementation)

#### 6.1 Clipboard Content Structure

```rust
// core/src/selection.rs (add to existing file)

/// Rich clipboard content with text and styling
#[derive(Debug, Clone)]
pub struct ClipboardContent {
    /// Plain text representation
    pub plain_text: String,
    /// Rich content with styling
    pub rich_content: Vec<StyledTextRun>,
}

#[derive(Debug, Clone)]
pub struct StyledTextRun {
    /// The text content
    pub text: String,
    /// The DOM node this text came from
    pub node_id: DomNodeId,
    /// Computed styles for this run
    pub style: TextRunStyle,
}

#[derive(Debug, Clone)]
pub struct TextRunStyle {
    pub font_family: String,
    pub font_size: f32,
    pub color: ColorU,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    // ... other style properties
}
```

#### 6.2 Copy/Cut Implementation

```rust
// layout/src/managers/selection.rs

impl SelectionManager {
    pub fn get_clipboard_content(
        &self,
        dom_id: DomId,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
    ) -> ClipboardContent {
        let mut plain_text = String::new();
        let mut rich_content = Vec::new();

        // Get all selections for this DOM
        let Some(selections) = self.selections.get(&dom_id) else {
            return ClipboardContent::empty();
        };

        for selection_state in selections {
            for selection in &selection_state.selections {
                let Selection::Range(range) = selection else {
                    continue; // Skip cursors
                };

                // Get node content
                let node_id = selection_state.node_id.node;
                let node_data = &styled_dom.node_data.as_container()[node_id];
                
                // Extract inline content for this selection
                if let Some(inline_text) = text_cache.get_inline_text(node_id) {
                    let content = extract_selection_content(range, &inline_text);
                    
                    // Build plain text
                    for item in &content {
                        plain_text.push_str(&item.get_text());
                    }
                    
                    // Build rich content with styling
                    let style = self.extract_text_style(
                        node_id, 
                        &styled_dom.css_property_cache
                    );
                    
                    rich_content.push(StyledTextRun {
                        text: plain_text.clone(),
                        node_id: selection_state.node_id,
                        style,
                    });
                }
            }
        }

        ClipboardContent {
            plain_text,
            rich_content,
        }
    }

    fn extract_text_style(
        &self,
        node_id: NodeId,
        cache: &CssPropertyCache,
    ) -> TextRunStyle {
        // Extract computed styles from CSS cache
        let node_data = // ... get node data
        let node_state = // ... get node state
        
        TextRunStyle {
            font_family: cache.get_font_family(node_data, &node_id, node_state)
                .and_then(|v| v.get_property())
                .and_then(|f| f.first())
                .map(|f| f.as_string())
                .unwrap_or_else(|| "sans-serif".to_string()),
            
            font_size: cache.get_font_size(node_data, &node_id, node_state)
                .and_then(|v| v.get_property())
                .map(|s| s.inner.to_pixels(16.0))
                .unwrap_or(16.0),
            
            color: cache.get_text_color(node_data, &node_id, node_state)
                .and_then(|v| v.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU { r: 0, g: 0, b: 0, a: 255 }),
            
            // TODO: Extract bold, italic, underline from font-weight, font-style, text-decoration
            bold: false,
            italic: false,
            underline: false,
        }
    }
}
```

### 7. Keyboard Shortcut Handling (🔨 Needs Implementation)

#### 7.1 Default Keyboard Handlers

```rust
// layout/src/window.rs or in keyboard event handler

impl LayoutWindow {
    /// Handle keyboard event - returns true if event was consumed
    pub fn handle_keyboard_event(
        &mut self,
        key: VirtualKeyCode,
        modifiers: ModifiersState,
        dom_id: DomId,
    ) -> bool {
        // Check for clipboard shortcuts
        if modifiers.ctrl || modifiers.meta {
            match key {
                VirtualKeyCode::C => {
                    // Ctrl+C: Copy
                    self.handle_copy(dom_id);
                    return true;
                }
                VirtualKeyCode::X => {
                    // Ctrl+X: Cut
                    self.handle_cut(dom_id);
                    return true;
                }
                VirtualKeyCode::A => {
                    // Ctrl+A: Select All
                    self.handle_select_all(dom_id);
                    return true;
                }
                _ => {}
            }
        }

        // Event not consumed - propagate to callbacks
        false
    }

    fn handle_copy(&mut self, dom_id: DomId) {
        let layout_result = &self.layout_results[&dom_id];
        let content = self.selection_manager.get_clipboard_content(
            dom_id,
            &layout_result.styled_dom,
            &self.text_cache,
        );

        // Send to system clipboard
        self.set_clipboard(content);
        
        // Print for debugging (as requested)
        println!("=== COPY OPERATION ===");
        println!("Plain text:\n{}", content.plain_text);
        println!("\nRich content:");
        for run in &content.rich_content {
            println!("  Node: {:?}", run.node_id);
            println!("  Text: {}", run.text);
            println!("  Style: {:?}", run.style);
        }
    }

    fn handle_cut(&mut self, dom_id: DomId) {
        // Same as copy, but also delete selected content
        let layout_result = &self.layout_results[&dom_id];
        let content = self.selection_manager.get_clipboard_content(
            dom_id,
            &layout_result.styled_dom,
            &self.text_cache,
        );

        self.set_clipboard(content.clone());

        // Delete selected content if contenteditable
        if let Some(focused) = self.focus_manager.get_focused_node() {
            if self.is_contenteditable(dom_id, focused.node) {
                self.delete_selection(dom_id);
            }
        }

        println!("=== CUT OPERATION ===");
        println!("Plain text:\n{}", content.plain_text);
        // ... same as copy
    }

    fn handle_select_all(&mut self, dom_id: DomId) {
        let layout_result = &self.layout_results[&dom_id];
        self.selection_manager.select_all(
            dom_id,
            &self.focus_manager,
            &layout_result.styled_dom,
            &self.text_cache,
        );
    }
}
```

#### 7.2 Clipboard Content API

```rust
// Accessor functions for clipboard content analysis

impl ClipboardContent {
    /// Get list of nodes in selection order
    pub fn get_nodes_in_order(&self) -> Vec<DomNodeId> {
        self.rich_content.iter()
            .map(|run| run.node_id)
            .collect()
    }

    /// Get text with node boundaries marked
    pub fn get_text_with_markers(&self) -> String {
        let mut result = String::new();
        for (i, run) in self.rich_content.iter().enumerate() {
            result.push_str(&format!("[Node {:?}] ", run.node_id));
            result.push_str(&run.text);
            if i < self.rich_content.len() - 1 {
                result.push_str(" ");
            }
        }
        result
    }

    /// Serialize to HTML with inline styles
    pub fn to_html(&self) -> String {
        let mut html = String::from("<div>");
        for run in &self.rich_content {
            html.push_str(&format!(
                r#"<span style="font-family: {}; font-size: {}px; color: rgb({}, {}, {});">{}</span>"#,
                run.style.font_family,
                run.style.font_size,
                run.style.color.r,
                run.style.color.g,
                run.style.color.b,
                html_escape(&run.text),
            ));
        }
        html.push_str("</div>");
        html
    }
}
```

### 8. ContentEditable Integration

#### 8.1 Cursor vs Selection

```rust
// Single click behavior depends on contenteditable attribute

impl SelectionManager {
    pub fn handle_single_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
        styled_dom: &StyledDom,
        text_cache: &TextLayoutCache,
        focus_manager: &FocusManager,
        now: Instant,
    ) -> SelectionAction {
        let node_data = &styled_dom.node_data.as_container()[node_id];
        let is_contenteditable = node_data.attributes
            .iter()
            .any(|attr| matches!(attr, AttributeType::ContentEditable(_)));

        if is_contenteditable {
            // Set text cursor for editing
            if let Some(cursor) = position_to_cursor(node_id, position, text_cache) {
                // Clear any existing selection
                self.clear_dom(dom_id);
                
                // Set cursor
                let selection_state = SelectionState {
                    selections: vec![Selection::Cursor(cursor)],
                    node_id: DomNodeId { dom: dom_id, node: node_id },
                };
                self.selections.entry(dom_id)
                    .or_insert_with(Vec::new)
                    .push(selection_state);
                
                return SelectionAction::SetCursor(cursor);
            }
        } else if focus_manager.get_focused_node().map(|f| f.node) == Some(node_id) {
            // If already focused, clear selection
            self.clear_dom(dom_id);
            return SelectionAction::ClearSelection;
        } else {
            // Set focus on non-contenteditable node
            return SelectionAction::SetFocus(DomNodeId { 
                dom: dom_id, 
                node: node_id 
            });
        }

        SelectionAction::None
    }
}
```

### 9. User-Select Property Behavior

```rust
// Check user-select property before allowing selection

impl LayoutWindow {
    fn is_text_selectable(&self, styled_dom: &StyledDom, node_id: NodeId) -> bool {
        use azul_css::props::style::StyleUserSelect;

        let node_data = &styled_dom.node_data.as_container()[node_id];
        let node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

        styled_dom
            .css_property_cache
            .ptr
            .get_user_select(node_data, &node_id, node_state)
            .and_then(|v| v.get_property())
            .map(|us| match us {
                StyleUserSelect::None => false,
                StyleUserSelect::Auto | 
                StyleUserSelect::Text | 
                StyleUserSelect::All => true,
            })
            .unwrap_or(true) // Default: selectable
    }

    fn get_selection_behavior(
        &self, 
        styled_dom: &StyledDom, 
        node_id: NodeId
    ) -> SelectionBehavior {
        use azul_css::props::style::StyleUserSelect;

        let node_data = &styled_dom.node_data.as_container()[node_id];
        let node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

        styled_dom
            .css_property_cache
            .ptr
            .get_user_select(node_data, &node_id, node_state)
            .and_then(|v| v.get_property())
            .map(|us| match us {
                StyleUserSelect::None => SelectionBehavior::NotSelectable,
                StyleUserSelect::Auto | StyleUserSelect::Text => SelectionBehavior::Normal,
                StyleUserSelect::All => SelectionBehavior::SelectAll, // Select entire node content
            })
            .unwrap_or(SelectionBehavior::Normal)
    }
}

enum SelectionBehavior {
    NotSelectable,
    Normal,
    SelectAll, // Select entire element content on any click
}
```

## Implementation Phases

### Phase 1: Selection Manager Core (Week 1)
- [ ] Create `SelectionManager` struct in `layout/src/managers/selection.rs`
- [ ] Implement `clear_all()`, `clear_dom()` methods
- [ ] Add `SelectionState` storage per DOM
- [ ] Integrate with `FocusManager` (clear selections on focus change)

### Phase 2: Click Detection (Week 1-2)
- [ ] Implement `ClickState` tracking for multi-click detection
- [ ] Add `update_click_count()` with timing and distance checks
- [ ] Integrate into `handle_mouse_down()` in event handler

### Phase 3: Single Click Selection (Week 2)
- [ ] Implement `position_to_cursor()` using existing hit-test
- [ ] Add `handle_single_click()` with contenteditable check
- [ ] Test cursor placement in contenteditable elements
- [ ] Test focus changes on non-contenteditable elements

### Phase 4: Multi-Click Selection (Week 2-3)
- [ ] Implement word boundary detection (`find_word_start/end`)
- [ ] Implement paragraph boundary detection
- [ ] Add `handle_double_click()` for word selection
- [ ] Add `handle_triple_click()` for paragraph selection
- [ ] Create `layout/src/text3/selection.rs` with helper functions

### Phase 5: Drag Selection (Week 3)
- [ ] Add `SelectionDragState` to `LayoutWindow`
- [ ] Implement `handle_drag()` in `SelectionManager`
- [ ] Update `handle_mouse_move()` to process drag
- [ ] Clear drag state in `handle_mouse_up()`

### Phase 6: Clipboard Integration (Week 3-4)
- [ ] Create `ClipboardContent` struct
- [ ] Implement `get_clipboard_content()` with style extraction
- [ ] Add `extract_selection_content()` to get text from range
- [ ] Implement style extraction from CSS cache
- [ ] Add platform clipboard integration (copypasta crate?)

### Phase 7: Keyboard Shortcuts (Week 4)
- [ ] Add keyboard event pre-processing in event handler
- [ ] Implement `handle_copy()` with println debugging
- [ ] Implement `handle_cut()` with content deletion
- [ ] Implement `handle_select_all()` with focus integration
- [ ] Add preventDefault mechanism for callbacks

### Phase 8: User-Select Property (Week 4-5)
- [ ] Implement `get_selection_behavior()` for user-select:all
- [ ] Add selection prevention for user-select:none
- [ ] Test cross-node selections with mixed user-select values
- [ ] Add automatic element selection for user-select:all

### Phase 9: Polish & Testing (Week 5)
- [ ] Visual selection rendering in display list
- [ ] Selection persistence across re-layouts
- [ ] Selection highlighting with proper Bidi handling
- [ ] Comprehensive test suite
- [ ] Documentation and examples

## Integration Points

### With Existing Systems

1. **FocusManager**: Clear selections on focus change
2. **TextLayoutCache**: Use for hit-testing and content extraction
3. **Event System**: Integrate into mouse and keyboard event handlers
4. **Display List**: Render selection highlights
5. **CSS Cache**: Extract styles for clipboard content

### New Dependencies

- None! All functionality can be built using existing infrastructure

## Testing Strategy

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_single_click_contenteditable() {
        // Create contenteditable node
        // Click at position
        // Verify cursor set correctly
    }

    #[test]
    fn test_double_click_word_selection() {
        // Create text "Hello World"
        // Double-click on "Hello"
        // Verify "Hello" selected
    }

    #[test]
    fn test_triple_click_paragraph() {
        // Create multi-line text
        // Triple-click
        // Verify entire paragraph selected
    }

    #[test]
    fn test_drag_selection() {
        // Create text
        // Simulate mouse down, drag, up
        // Verify range selected
    }

    #[test]
    fn test_user_select_none() {
        // Create node with user-select: none
        // Attempt selection
        // Verify no selection created
    }

    #[test]
    fn test_clipboard_content() {
        // Create selection with styled text
        // Get clipboard content
        // Verify plain text and styling preserved
    }
}
```

## Open Questions

1. **Multi-DOM selections**: Should Ctrl+C copy from all DOMs or just focused one?
2. **Selection across nodes**: How to handle selections spanning multiple text nodes?
3. **Images in selection**: Include images in clipboard content?
4. **Paste operation**: Should we implement Ctrl+V as well?
5. **Custom clipboard formats**: Support HTML/RTF clipboard formats?

## Future Enhancements

- [ ] Ctrl+V paste support with rich content
- [ ] Drag-and-drop text editing
- [ ] Find-and-replace functionality
- [ ] Spell-check integration
- [ ] Autocomplete/suggestions
- [ ] Multi-cursor editing (like VS Code)

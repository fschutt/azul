# User-Select System Design (Revised)

## Overview

Comprehensive design for text selection, contenteditable, and clipboard integration in Azul. This system integrates with the existing `SelectionManager`, `UnifiedLayout` text system, and event handlers to enable:

- Multi-click text selection (single/double/triple click)
- Drag selection  
- Keyboard shortcuts (Ctrl+C/X/A)
- Rich clipboard content with styling
- Multi-cursor support (future: Sublime Text-style)
- Minimal undo/redo per selection edit

**Design Philosophy**:
- Use early-return style for clarity
- Build rich clipboard content always, extract lazily
- Don't own data model - provide query API for callbacks
- User callbacks control actual data mutations via `preventDefault()`

## Existing Infrastructure

### ✅ Already Implemented

1. **SelectionManager** (`layout/src/managers/selection.rs`)
   ```rust
   pub struct SelectionManager {
       pub selections: BTreeMap<DomId, SelectionState>,
   }
   ```
   - Maps DomId → SelectionState
   - Has `get_selection()`, `set_selection()`, `clear_all()` methods

2. **SelectionState** (`core/src/selection.rs`)
   ```rust
   pub struct SelectionState {
       pub selections: Vec<Selection>,  // Multiple cursors!
       pub node_id: DomNodeId,
   }
   
   pub enum Selection {
       Cursor(TextCursor),
       Range(SelectionRange),
   }
   ```
   - Already supports multi-cursor
   - Has `add()`, `set_cursor()` methods

3. **UnifiedLayout** (`layout/src/text3/cache.rs`)
   ```rust
   pub struct UnifiedLayout<T: ParsedFontTrait> {
       pub items: Vec<PositionedItem<T>>,
       pub bounds: Rect,
       pub overflow: OverflowInfo<T>,
       pub used_fonts: BTreeMap<u64, T>,
   }
   ```
   - **HAS** `hittest_cursor()` method already!
   - Returns `Option<TextCursor>` from LogicalPosition
   - This is our `position_to_cursor()` function!

4. **ShapedGlyph** (`layout/src/text3/cache.rs`)
   ```rust
   pub struct ShapedGlyph<T: ParsedFontTrait> {
       pub glyph_id: u16,
       pub cluster_offset: u32,
       pub advance: f32,
       pub style: Arc<StyleProperties>,  // Per-character styling!
       pub font: T,
       // ... offset, script, etc.
   }
   ```
   - Each character has full styling information
   - Already used for display list generation

5. **CSS user-select Property** (✅ Implemented)
   - `is_text_selectable()` checks CSS property
   - Blocks selection for `user-select: none`

## Architecture Components

### 1. Click State Tracking

```rust
// layout/src/managers/selection.rs (extend existing)

#[derive(Debug, Clone)]
struct ClickState {
    last_node: Option<DomNodeId>,
    last_position: LogicalPosition,
    last_time: Instant,
    click_count: u8,  // 1=single, 2=double, 3=triple
}

const DOUBLE_CLICK_TIME_MS: u64 = 500;
const DOUBLE_CLICK_DISTANCE_PX: f32 = 5.0;

impl SelectionManager {
    // Add field: click_state: ClickState
    
    fn update_click_count(
        &mut self,
        node_id: DomNodeId,
        position: LogicalPosition,
        now: Instant,
    ) -> u8 {
        let should_increment = if let Some(last_node) = self.click_state.last_node {
            if last_node != node_id {
                return 1; // Different node
            }
            
            let time_delta = now.duration_since(self.click_state.last_time).as_millis();
            if time_delta >= DOUBLE_CLICK_TIME_MS as u128 {
                return 1; // Too much time passed
            }
            
            let distance = ((position.x - self.click_state.last_position.x).powi(2) +
                           (position.y - self.click_state.last_position.y).powi(2)).sqrt();
            if distance >= DOUBLE_CLICK_DISTANCE_PX {
                return 1; // Moved too far
            }
            
            true
        } else {
            false
        };

        let click_count = if should_increment {
            (self.click_state.click_count + 1).min(3)
        } else {
            1
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

### 2. Mouse Event Integration

```rust
// In event handler (dll/src/desktop/shell2/.../events.rs or layout/src/window.rs)

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

        // Hit test
        let Some(hit_node_id) = self.hit_test(dom_id, position) else {
            return ProcessEventResult::DoNothing;
        };

        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return ProcessEventResult::DoNothing;
        };

        let styled_dom = &layout_result.styled_dom;
        let node_id = hit_node_id.node;

        // Check user-select CSS property
        if !self.is_text_selectable(styled_dom, node_id) {
            self.selection_manager.clear_selection(&dom_id);
            return ProcessEventResult::PropagateEvent;
        }

        // Update click count
        let click_count = self.selection_manager.update_click_count(
            hit_node_id,
            position,
            now,
        );

        // Handle based on click count
        match click_count {
            1 => self.handle_single_click(dom_id, node_id, position),
            2 => self.handle_double_click(dom_id, node_id, position),
            3 => self.handle_triple_click(dom_id, node_id, position),
            _ => ProcessEventResult::DoNothing,
        }
    }
    
    fn handle_single_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return ProcessEventResult::DoNothing;
        };
        
        let styled_dom = &layout_result.styled_dom;
        let node_data = &styled_dom.node_data.as_container()[node_id];
        
        // Check contenteditable attribute
        let is_contenteditable = node_data.attributes
            .iter()
            .any(|attr| matches!(attr, AttributeType::ContentEditable(_)));

        if !is_contenteditable {
            // Not contenteditable - set focus, clear selection
            self.selection_manager.clear_selection(&dom_id);
            return ProcessEventResult::RequestFocus(DomNodeId { dom: dom_id, node: node_id });
        }

        // Get text layout for this node
        let Some(text_layout) = self.text_cache.get_text_layout(node_id) else {
            return ProcessEventResult::DoNothing;
        };

        // Use existing hittest_cursor!
        let Some(cursor) = text_layout.hittest_cursor(position) else {
            return ProcessEventResult::DoNothing;
        };

        // Create selection state with cursor
        let selection_state = SelectionState {
            selections: vec![Selection::Cursor(cursor)],
            node_id: DomNodeId { dom: dom_id, node: node_id },
        };

        self.selection_manager.set_selection(dom_id, selection_state);
        
        // Start drag state for potential drag selection
        self.selection_drag_state = Some(SelectionDragState {
            dom_id,
            start_node: node_id,
            start_cursor: cursor,
        });

        ProcessEventResult::RequestRedraw
    }
}
```

### 3. Double/Triple Click Selection

```rust
// layout/src/text3/selection.rs (new file)

use azul_core::selection::{TextCursor, SelectionRange};
use crate::text3::cache::UnifiedLayout;

/// Select word at cursor position
pub fn select_word_at_cursor<T: ParsedFontTrait>(
    cursor: &TextCursor,
    layout: &UnifiedLayout<T>,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let item = layout.items.iter().find(|item| {
        // Check if cursor is within this item's cluster range
        item_contains_cursor(&item.item, cursor)
    })?;

    // Extract text from this item
    let text = extract_text_from_item(&item.item);
    
    // Find word boundaries using Unicode word break
    let cursor_byte_offset = cursor.cluster_id.start_byte_in_run as usize;
    
    let word_start = text[..cursor_byte_offset]
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() || c.is_ascii_punctuation())
        .map(|(i, _)| i + 1)
        .unwrap_or(0);
    
    let word_end = text[cursor_byte_offset..]
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || c.is_ascii_punctuation())
        .map(|(i, _)| cursor_byte_offset + i)
        .unwrap_or(text.len());

    // Create selection range
    Some(SelectionRange {
        start: TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: cursor.cluster_id.source_run,
                start_byte_in_run: word_start as u32,
            },
            affinity: CursorAffinity::Leading,
        },
        end: TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: cursor.cluster_id.source_run,
                start_byte_in_run: word_end as u32,
            },
            affinity: CursorAffinity::Trailing,
        },
    })
}

/// Select paragraph/line at cursor
pub fn select_paragraph_at_cursor<T: ParsedFontTrait>(
    cursor: &TextCursor,
    layout: &UnifiedLayout<T>,
) -> Option<SelectionRange> {
    // Find the line containing this cursor
    let line_items: Vec<_> = layout.items.iter()
        .filter(|item| item_contains_cursor(&item.item, cursor))
        .collect();

    if line_items.is_empty() {
        return None;
    }

    // Get first and last item on this line
    let first_item = line_items.first()?;
    let last_item = line_items.last()?;

    // Create selection spanning entire line
    Some(SelectionRange {
        start: get_item_start_cursor(&first_item.item),
        end: get_item_end_cursor(&last_item.item),
    })
}

impl LayoutWindow {
    fn handle_double_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        let Some(text_layout) = self.text_cache.get_text_layout(node_id) else {
            return ProcessEventResult::DoNothing;
        };

        let Some(cursor) = text_layout.hittest_cursor(position) else {
            return ProcessEventResult::DoNothing;
        };

        let Some(range) = select_word_at_cursor(&cursor, text_layout) else {
            return ProcessEventResult::DoNothing;
        };

        let selection_state = SelectionState {
            selections: vec![Selection::Range(range)],
            node_id: DomNodeId { dom: dom_id, node: node_id },
        };

        self.selection_manager.set_selection(dom_id, selection_state);
        ProcessEventResult::RequestRedraw
    }

    fn handle_triple_click(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        let Some(text_layout) = self.text_cache.get_text_layout(node_id) else {
            return ProcessEventResult::DoNothing;
        };

        let Some(cursor) = text_layout.hittest_cursor(position) else {
            return ProcessEventResult::DoNothing;
        };

        let Some(range) = select_paragraph_at_cursor(&cursor, text_layout) else {
            return ProcessEventResult::DoNothing;
        };

        let selection_state = SelectionState {
            selections: vec![Selection::Range(range)],
            node_id: DomNodeId { dom: dom_id, node: node_id },
        };

        self.selection_manager.set_selection(dom_id, selection_state);
        ProcessEventResult::RequestRedraw
    }
}
```

### 4. Drag Selection

```rust
// layout/src/window.rs

struct SelectionDragState {
    dom_id: DomId,
    start_node: NodeId,
    start_cursor: TextCursor,
}

impl LayoutWindow {
    pub fn handle_mouse_move(
        &mut self,
        position: LogicalPosition,
    ) -> ProcessEventResult {
        let Some(drag_state) = &self.selection_drag_state else {
            return ProcessEventResult::DoNothing; // Not dragging
        };

        let Some(text_layout) = self.text_cache.get_text_layout(drag_state.start_node) else {
            return ProcessEventResult::DoNothing;
        };

        let Some(current_cursor) = text_layout.hittest_cursor(position) else {
            return ProcessEventResult::DoNothing;
        };

        // Create range from start to current position
        let range = SelectionRange {
            start: drag_state.start_cursor,
            end: current_cursor,
        };

        let selection_state = SelectionState {
            selections: vec![Selection::Range(range)],
            node_id: DomNodeId { 
                dom: drag_state.dom_id, 
                node: drag_state.start_node 
            },
        };

        self.selection_manager.set_selection(drag_state.dom_id, selection_state);
        ProcessEventResult::RequestRedraw
    }

    pub fn handle_mouse_up(
        &mut self,
        button: MouseButton,
    ) -> ProcessEventResult {
        if button == MouseButton::Left {
            self.selection_drag_state = None;
        }
        ProcessEventResult::DoNothing
    }
}
```

### 5. Rich Clipboard Content

```rust
// core/src/selection.rs (add to existing file)

/// Rich clipboard content built from UnifiedLayout
#[derive(Debug, Clone)]
pub struct ClipboardContent {
    /// Plain text (lazily extracted)
    plain_text: Option<String>,
    /// Rich content with per-character styling
    rich_runs: Vec<StyledTextRun>,
}

#[derive(Debug, Clone)]
pub struct StyledTextRun {
    pub text: String,
    pub node_id: DomNodeId,
    pub font_family: String,
    pub font_size: f32,
    pub color: ColorU,
    // Add more style properties as needed
}

impl ClipboardContent {
    /// Get plain text (extracts on first call, caches)
    pub fn get_plain_text(&mut self) -> &str {
        if self.plain_text.is_none() {
            self.plain_text = Some(
                self.rich_runs.iter()
                    .map(|run| run.text.as_str())
                    .collect::<Vec<_>>()
                    .join("")
            );
        }
        self.plain_text.as_ref().unwrap()
    }

    /// Get rich content
    pub fn get_rich_runs(&self) -> &[StyledTextRun] {
        &self.rich_runs
    }

    /// Export to HTML with inline styles
    pub fn to_html(&self) -> String {
        let mut html = String::from("<div>");
        for run in &self.rich_runs {
            html.push_str(&format!(
                r#"<span style="font-family: {}; font-size: {}px; color: rgb({}, {}, {});">{}</span>"#,
                run.font_family,
                run.font_size,
                run.color.r,
                run.color.g,
                run.color.b,
                html_escape(&run.text),
            ));
        }
        html.push_str("</div>");
        html
    }

    /// Get nodes in order
    pub fn get_nodes_in_order(&self) -> Vec<DomNodeId> {
        self.rich_runs.iter()
            .map(|run| run.node_id)
            .collect()
    }
}
```

### 6. Clipboard Content Extraction

```rust
// layout/src/managers/selection.rs (extend existing)

impl SelectionManager {
    /// Build rich clipboard content from current selection
    /// ALWAYS builds rich content, plain text extracted lazily
    pub fn get_clipboard_content<T: ParsedFontTrait>(
        &self,
        dom_id: &DomId,
        text_layout: &UnifiedLayout<T>,
    ) -> Option<ClipboardContent> {
        let selection_state = self.get_selection(dom_id)?;

        let mut rich_runs = Vec::new();

        for selection in &selection_state.selections {
            let range = match selection {
                Selection::Range(r) => r,
                Selection::Cursor(_) => continue, // Skip cursors
            };

            // Extract text and styling from range using ShapedGlyph data
            let runs = extract_styled_runs_from_range(range, text_layout);
            rich_runs.extend(runs);
        }

        if rich_runs.is_empty() {
            return None;
        }

        Some(ClipboardContent {
            plain_text: None, // Lazy
            rich_runs,
        })
    }
}

/// Extract styled text runs from selection range
fn extract_styled_runs_from_range<T: ParsedFontTrait>(
    range: &SelectionRange,
    layout: &UnifiedLayout<T>,
) -> Vec<StyledTextRun> {
    let mut runs = Vec::new();
    let mut current_run: Option<StyledTextRun> = None;

    for item in &layout.items {
        let ShapedItem::Text { glyphs, .. } = &item.item else {
            continue;
        };

        for glyph in glyphs {
            // Check if glyph is within selection range
            if !is_glyph_in_range(glyph, range) {
                continue;
            }

            // Get styling from ShapedGlyph
            let style = &glyph.style;

            // Check if we need to start a new run (style changed)
            let needs_new_run = if let Some(ref curr) = current_run {
                curr.font_family != style.font_selector.family ||
                (curr.font_size - style.font_size_px).abs() > 0.01 ||
                curr.color != style.color
            } else {
                true
            };

            if needs_new_run {
                // Push previous run if exists
                if let Some(run) = current_run.take() {
                    runs.push(run);
                }

                // Start new run
                current_run = Some(StyledTextRun {
                    text: String::from(glyph.codepoint),
                    node_id: /* get from layout */,
                    font_family: style.font_selector.family.clone(),
                    font_size: style.font_size_px,
                    color: style.color,
                });
            } else {
                // Append to current run
                if let Some(ref mut run) = current_run {
                    run.text.push(glyph.codepoint);
                }
            }
        }
    }

    // Push last run
    if let Some(run) = current_run {
        runs.push(run);
    }

    runs
}
```

### 7. Keyboard Shortcuts

```rust
// In keyboard event handler

impl LayoutWindow {
    pub fn handle_keyboard_event(
        &mut self,
        key: VirtualKeyCode,
        modifiers: ModifiersState,
        dom_id: DomId,
    ) -> HandleKeyboardResult {
        // Check for clipboard shortcuts (Ctrl on Windows/Linux, Cmd on macOS)
        let is_modifier_pressed = if cfg!(target_os = "macos") {
            modifiers.meta
        } else {
            modifiers.ctrl
        };

        if !is_modifier_pressed {
            return HandleKeyboardResult::NotHandled;
        }

        match key {
            VirtualKeyCode::C => {
                self.handle_copy(dom_id);
                HandleKeyboardResult::Handled
            }
            VirtualKeyCode::X => {
                self.handle_cut(dom_id);
                HandleKeyboardResult::Handled
            }
            VirtualKeyCode::A => {
                self.handle_select_all(dom_id);
                HandleKeyboardResult::Handled
            }
            _ => HandleKeyboardResult::NotHandled,
        }
    }

    fn handle_copy(&mut self, dom_id: DomId) {
        let Some(selection_state) = self.selection_manager.get_selection(&dom_id) else {
            return;
        };

        let node_id = selection_state.node_id.node;
        
        let Some(text_layout) = self.text_cache.get_text_layout(node_id) else {
            return;
        };

        let Some(mut clipboard_content) = self.selection_manager
            .get_clipboard_content(&dom_id, text_layout) else {
            return;
        };

        // Print for debugging
        println!("=== COPY OPERATION ===");
        println!("Plain text:\n{}", clipboard_content.get_plain_text());
        println!("\nRich content ({} runs):", clipboard_content.get_rich_runs().len());
        for (i, run) in clipboard_content.get_rich_runs().iter().enumerate() {
            println!("  Run {}: '{}' ({}, {}px, {:?})", 
                i, run.text, run.font_family, run.font_size, run.color);
        }
        println!("\nHTML:\n{}", clipboard_content.to_html());

        // Set system clipboard
        self.set_clipboard_text(clipboard_content.get_plain_text());
        
        // TODO: Set HTML clipboard format on platforms that support it
    }

    fn handle_cut(&mut self, dom_id: DomId) {
        // Get content before deletion
        let Some(selection_state) = self.selection_manager.get_selection(&dom_id) else {
            return;
        };

        let node_id = selection_state.node_id.node;
        
        let Some(text_layout) = self.text_cache.get_text_layout(node_id) else {
            return;
        };

        let Some(mut clipboard_content) = self.selection_manager
            .get_clipboard_content(&dom_id, text_layout) else {
            return;
        };

        // Print same as copy
        println!("=== CUT OPERATION ===");
        println!("Plain text:\n{}", clipboard_content.get_plain_text());
        // ... same as copy

        // Set clipboard
        self.set_clipboard_text(clipboard_content.get_plain_text());

        // Check if contenteditable
        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return;
        };
        
        let styled_dom = &layout_result.styled_dom;
        let node_data = &styled_dom.node_data.as_container()[node_id];
        
        let is_contenteditable = node_data.attributes
            .iter()
            .any(|attr| matches!(attr, AttributeType::ContentEditable(_)));

        if !is_contenteditable {
            return; // Can't modify non-contenteditable
        }

        // Create Changeset for callbacks to query
        let changeset = SelectionChangeset {
            dom_id,
            node_id: selection_state.node_id,
            operation: ChangesetOperation::Delete,
            affected_ranges: selection_state.selections.clone(),
            clipboard_content,
        };

        // Store for callback queries
        self.current_changeset = Some(changeset.clone());

        // Fire On::Cut event - callbacks can preventDefault
        let callback_result = self.fire_event(On::Cut, dom_id, node_id);

        if callback_result.should_prevent_default {
            // User callback handled it
            self.current_changeset = None;
            return;
        }

        // Default impl: delete text (simplified - needs proper text editing)
        self.delete_selection_text(dom_id, node_id);
        self.current_changeset = None;
    }

    fn handle_select_all(&mut self, dom_id: DomId) {
        // Get focused node
        let Some(focused_node_id) = self.focus_manager.get_focused_node() else {
            return;
        };

        if focused_node_id.dom != dom_id {
            return;
        }

        let node_id = focused_node_id.node;

        // Check user-select
        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return;
        };
        
        if !self.is_text_selectable(&layout_result.styled_dom, node_id) {
            return;
        }

        // Get text layout
        let Some(text_layout) = self.text_cache.get_text_layout(node_id) else {
            return;
        };

        // Select entire text
        let range = SelectionRange {
            start: get_layout_start_cursor(text_layout),
            end: get_layout_end_cursor(text_layout),
        };

        let selection_state = SelectionState {
            selections: vec![Selection::Range(range)],
            node_id: focused_node_id,
        };

        self.selection_manager.set_selection(dom_id, selection_state);
    }
}
```

### 8. Changeset Query API

```rust
// layout/src/managers/selection.rs

/// Changeset for querying from callbacks
#[derive(Debug, Clone)]
pub struct SelectionChangeset {
    pub dom_id: DomId,
    pub node_id: DomNodeId,
    pub operation: ChangesetOperation,
    pub affected_ranges: Vec<Selection>,
    pub clipboard_content: ClipboardContent,
}

#[derive(Debug, Clone, Copy)]
pub enum ChangesetOperation {
    Copy,
    Cut,
    Paste,
}

impl SelectionChangeset {
    /// Get plain text that would be copied/cut
    pub fn get_plain_text(&mut self) -> &str {
        self.clipboard_content.get_plain_text()
    }

    /// Get rich content
    pub fn get_rich_content(&self) -> &[StyledTextRun] {
        self.clipboard_content.get_rich_runs()
    }

    /// Get affected nodes
    pub fn get_affected_nodes(&self) -> Vec<DomNodeId> {
        self.clipboard_content.get_nodes_in_order()
    }

    /// Export to HTML
    pub fn to_html(&self) -> String {
        self.clipboard_content.to_html()
    }
}

// In LayoutWindow
impl LayoutWindow {
    pub fn get_current_changeset(&self) -> Option<&SelectionChangeset> {
        self.current_changeset.as_ref()
    }
}

// In callbacks.rs
impl CallbackInfo {
    pub fn get_current_changeset(&self) -> Option<&SelectionChangeset> {
        self.window.get_current_changeset()
    }
}
```

### 9. User Callback Integration

```rust
// User code example

fn on_cut_handler(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let Some(changeset) = info.get_current_changeset() else {
        return Update::DoNothing;
    };

    println!("Cut operation detected:");
    println!("  Node: {:?}", changeset.node_id);
    println!("  Text: {}", changeset.get_plain_text());
    println!("  HTML: {}", changeset.to_html());

    // Store in application data model
    let data = data.downcast_mut::<MyAppData>().unwrap();
    data.store_cut_content(changeset.get_rich_content());

    // preventDefault - we handle it ourselves
    info.prevent_default();

    // Trigger DOM update
    Update::RefreshDom
}

// Dom builder
Dom::div()
    .with_callback(On::Cut, Callback { cb: on_cut_handler })
```

## Future: Multi-Cursor Support

### Sublime Text-style Multi-Cursor

```rust
// Future enhancement - already supported by SelectionState!

impl SelectionManager {
    /// Ctrl+Shift+L: Break selection into multiple cursors (one per line)
    pub fn split_selection_into_cursors(
        &mut self,
        dom_id: &DomId,
        text_layout: &UnifiedLayout<_>,
    ) {
        let Some(selection_state) = self.get_selection_mut(dom_id) else {
            return;
        };

        let mut new_selections = Vec::new();

        for selection in &selection_state.selections {
            let Selection::Range(range) = selection else {
                new_selections.push(*selection);
                continue;
            };

            // Get all lines in range
            let lines = get_lines_in_range(range, text_layout);

            // Create cursor at start of each line
            for line in lines {
                new_selections.push(Selection::Cursor(line.start_cursor));
            }
        }

        selection_state.selections = new_selections;
    }

    /// Ctrl+V with multiple selections
    pub fn paste_to_multiple_cursors(
        &mut self,
        dom_id: &DomId,
        pasted_content: &str,
    ) -> PasteStrategy {
        let Some(selection_state) = self.get_selection(dom_id) else {
            return PasteStrategy::Single;
        };

        let cursor_count = selection_state.selections.len();
        let paste_lines: Vec<&str> = pasted_content.lines().collect();

        if paste_lines.len() == cursor_count {
            // One line per cursor
            PasteStrategy::OnePerCursor(paste_lines)
        } else {
            // Duplicate entire content at each cursor
            PasteStrategy::DuplicateAll(pasted_content.to_string())
        }
    }
}
```

## Minimal Undo/Redo

```rust
// Per-selection edit history (lightweight)

#[derive(Debug, Clone)]
struct EditHistory {
    undo_stack: Vec<SelectionChangeset>,
    redo_stack: Vec<SelectionChangeset>,
    max_size: usize,
}

impl EditHistory {
    const MAX_HISTORY: usize = 50;

    fn push_edit(&mut self, changeset: SelectionChangeset) {
        if self.undo_stack.len() >= self.max_size {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(changeset);
        self.redo_stack.clear();
    }

    fn undo(&mut self) -> Option<SelectionChangeset> {
        let changeset = self.undo_stack.pop()?;
        self.redo_stack.push(changeset.clone());
        Some(changeset)
    }

    fn redo(&mut self) -> Option<SelectionChangeset> {
        let changeset = self.redo_stack.pop()?;
        self.undo_stack.push(changeset.clone());
        Some(changeset)
    }
}

// Ctrl+Z / Ctrl+Shift+Z
impl LayoutWindow {
    fn handle_undo(&mut self, dom_id: DomId) {
        let Some(changeset) = self.edit_history.undo() else {
            return;
        };

        // Fire On::Undo event
        let callback_result = self.fire_event(On::Undo, dom_id, changeset.node_id.node);

        if callback_result.should_prevent_default {
            return; // User handles undo
        }

        // Default: revert changeset (simplified)
        self.revert_changeset(&changeset);
    }
}
```

## Implementation Plan

### Phase 1: Core Selection (Week 1) ✅ PARTIALLY DONE
- [x] SelectionManager exists
- [x] SelectionState exists  
- [x] UnifiedLayout.hittest_cursor() exists
- [x] CSS user-select property
- [ ] Add ClickState to SelectionManager
- [ ] Implement update_click_count()

### Phase 2: Single Click (Week 1-2)
- [ ] Integrate handle_mouse_down() with click detection
- [ ] Implement handle_single_click() using hittest_cursor()
- [ ] Add SelectionDragState to LayoutWindow
- [ ] Test cursor placement in contenteditable

### Phase 3: Multi-Click (Week 2)
- [ ] Create layout/src/text3/selection.rs
- [ ] Implement select_word_at_cursor()
- [ ] Implement select_paragraph_at_cursor()
- [ ] Add handle_double_click() and handle_triple_click()

### Phase 4: Drag Selection (Week 2)
- [ ] Implement handle_mouse_move() drag logic
- [ ] Implement handle_mouse_up() to clear drag
- [ ] Test continuous drag selection

### Phase 5: Rich Clipboard (Week 3)
- [ ] Add ClipboardContent struct
- [ ] Implement extract_styled_runs_from_range()
- [ ] Build using ShapedGlyph style data
- [ ] Test HTML export

### Phase 6: Keyboard Shortcuts (Week 3)
- [ ] Add keyboard event pre-processing
- [ ] Implement handle_copy() with println debug
- [ ] Implement handle_cut() with changeset
- [ ] Implement handle_select_all()

### Phase 7: Callback Integration (Week 4)
- [ ] Add SelectionChangeset struct
- [ ] Add get_current_changeset() to CallbackInfo
- [ ] Implement preventDefault mechanism
- [ ] Add On::Cut, On::Copy, On::Paste events

### Phase 8: Undo/Redo (Week 4-5)
- [ ] Add EditHistory
- [ ] Implement Ctrl+Z/Ctrl+Shift+Z handlers
- [ ] Add On::Undo, On::Redo events
- [ ] Test undo/redo stack

### Phase 9: Polish (Week 5)
- [ ] Visual selection rendering in display list
- [ ] Selection highlighting with proper colors
- [ ] Test across platforms
- [ ] Documentation

## Key Design Decisions

1. **Use existing hittest_cursor()**: Don't reimplement position_to_cursor, UnifiedLayout already has it!

2. **Always build rich content**: ShapedGlyph has per-character styling, use it to build ClipboardContent with full styling info

3. **Lazy plain text extraction**: Only extract plain text when needed (get_plain_text())

4. **Don't own data**: SelectionManager doesn't modify DOM text, only tracks selections. Callbacks do actual mutations.

5. **Query API**: Provide SelectionChangeset for callbacks to inspect what would change, then preventDefault if they handle it

6. **Multi-cursor ready**: SelectionState.selections is already Vec, future Sublime-style multi-cursor is just adding more items

7. **Early returns**: All functions use early returns for clarity

8. **Platform shortcuts**: Use Cmd on macOS, Ctrl on Windows/Linux

## Testing Strategy

```rust
#[test]
fn test_hittest_cursor_existing() {
    // Test that UnifiedLayout.hittest_cursor() works
    let layout = create_test_layout("Hello World");
    let cursor = layout.hittest_cursor(LogicalPosition { x: 10.0, y: 5.0 });
    assert!(cursor.is_some());
}

#[test]
fn test_click_count_detection() {
    let mut manager = SelectionManager::new();
    let node = DomNodeId { dom: DomId(0), node: NodeId(0) };
    let pos = LogicalPosition { x: 10.0, y: 10.0 };
    
    let count1 = manager.update_click_count(node, pos, Instant::now());
    assert_eq!(count1, 1);
    
    let count2 = manager.update_click_count(node, pos, Instant::now());
    assert_eq!(count2, 2);
}

#[test]
fn test_word_selection() {
    let layout = create_test_layout("Hello World");
    let cursor = /* cursor at 'W' */;
    let range = select_word_at_cursor(&cursor, &layout);
    assert_eq!(extract_text_from_range(&range, &layout), "World");
}

#[test]
fn test_rich_clipboard_preserves_styling() {
    let selection = /* styled selection */;
    let content = extract_clipboard_content(selection);
    assert_eq!(content.rich_runs.len(), 2); // Two different styles
    assert_eq!(content.rich_runs[0].font_size, 16.0);
    assert_eq!(content.rich_runs[1].font_size, 20.0);
}
```

## Open Questions

1. **Cross-node selections**: How to handle selections spanning multiple text nodes?
   - Solution: Track multiple ranges in SelectionState.selections

2. **Images in selection**: Should we include images in ClipboardContent?
   - Solution: Add ImageRun variant to ClipboardContent

3. **Paste operation**: Should we implement Ctrl+V?
   - Yes, but keep minimal - fire On::Paste event with changeset

4. **Selection rendering**: How to render selection highlights?
   - Add DisplayListItem::Selection with color and rects

5. **Bidi text**: How to handle RTL text in selections?
   - TextCursor already has affinity, use it

## Summary

This design:
- ✅ Uses existing UnifiedLayout.hittest_cursor() (no reimplementation!)
- ✅ Leverages ShapedGlyph per-character styling
- ✅ Builds rich clipboard content always, extracts plain text lazily
- ✅ Provides query API for callbacks (SelectionChangeset)
- ✅ Doesn't own data model (callbacks do actual mutations)
- ✅ Ready for multi-cursor (SelectionState already has Vec)
- ✅ Uses early-return style throughout
- ✅ Minimal undo/redo per selection edit

Implementation: ~5 weeks, 80% leveraging existing infrastructure!

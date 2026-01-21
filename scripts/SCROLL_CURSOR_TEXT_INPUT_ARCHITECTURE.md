# Scroll-Into-View, Cursor Tracking, and Text Input Architecture

## Executive Summary

This document defines the architecture for interconnected features:

1. **Scroll-Into-View API**: Generic mechanism to scroll any rect into view
2. **Active Element Following**: Auto-scroll when focus, cursor, or selection changes
3. **Text Input with ContentEditable**: Unified text editing with cursor auto-scroll
4. **Multi-Cursor System**: Built-in support for multiple cursors per contenteditable node
5. **Selection Scoping (Ctrl+A)**: Context-aware select-all based on focused element
6. **Code Editor Widget**: `Vec<Dom>` with per-line contenteditable nodes

---

## 1. Core Principle: Rect-Based Scroll API

The W3C CSSOM View Module defines `scrollIntoView()` as the foundational API. Our architecture follows this pattern:

### 1.1 The Core API: `scroll_rect_into_view`

```rust
/// Core function: scroll a rect into the visible area of its scroll containers
///
/// This is the ONLY scroll-into-view primitive. All higher-level APIs call this.
pub fn scroll_rect_into_view(
    target_rect: LogicalRect,           // The rect to make visible
    target_node: DomNodeId,             // Node for finding scroll ancestors
    layout_results: &DomLayoutResult,   // Layout data
    scroll_manager: &mut ScrollManager, // Scroll state
    options: ScrollIntoViewOptions,     // How to scroll
    now: Instant,                       // For animation timing
) -> Vec<ScrollAdjustment>;
```

### 1.2 ScrollIntoViewOptions (W3C-compliant)

```rust
/// W3C-compliant scroll-into-view options
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollIntoViewOptions {
    /// Vertical alignment: start, center, end, nearest (default: nearest)
    pub block: ScrollLogicalPosition,
    /// Horizontal alignment: start, center, end, nearest (default: nearest)
    pub inline: ScrollLogicalPosition,
    /// Animation: auto, instant, smooth (default: auto)
    pub behavior: ScrollBehavior,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub enum ScrollLogicalPosition {
    Start,
    Center,
    End,
    #[default]
    Nearest,  // Minimum scroll distance
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub enum ScrollBehavior {
    #[default]
    Auto,     // Respect CSS scroll-behavior
    Instant,  // Immediate jump
    Smooth,   // Animated scroll
}
```

### 1.3 Return Type

```rust
/// Calculated scroll adjustment for one scroll container
#[derive(Debug, Clone)]
pub struct ScrollAdjustment {
    pub scroll_container: DomNodeId,
    pub delta: LogicalPosition,
    pub behavior: ScrollBehavior,
}
```

---

## 2. Higher-Level APIs (All Call `scroll_rect_into_view`)

### 2.1 Scroll Node Into View

```rust
/// Scroll a DOM node's bounding rect into view
pub fn scroll_node_into_view(
    node_id: DomNodeId,
    layout_results: &DomLayoutResult,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    // 1. Get node's bounding rect from layout
    let target_rect = get_node_rect(node_id, layout_results)?;
    
    // 2. Call the core rect-based API
    scroll_rect_into_view(target_rect, node_id, layout_results, scroll_manager, options, now)
}
```

### 2.2 Scroll Cursor Into View (Text)

```rust
/// Scroll a text cursor position into view
pub fn scroll_cursor_into_view(
    cursor: &TextCursor,
    node_id: DomNodeId,
    text_layout: &UnifiedLayout,
    layout_results: &DomLayoutResult,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    // 1. Get cursor's visual rect from text layout
    let cursor_rect = text_layout.get_cursor_rect(cursor)?;
    
    // 2. Transform to node-relative coordinates (cursor_rect is local to text)
    let node_rect = get_node_rect(node_id, layout_results)?;
    let absolute_rect = LogicalRect {
        origin: LogicalPosition {
            x: node_rect.origin.x + cursor_rect.origin.x,
            y: node_rect.origin.y + cursor_rect.origin.y,
        },
        size: cursor_rect.size,
    };
    
    // 3. Call the core rect-based API
    scroll_rect_into_view(absolute_rect, node_id, layout_results, scroll_manager, options, now)
}
```

### 2.3 Scroll Selection Into View

```rust
/// Scroll a text selection's focus point into view
pub fn scroll_selection_into_view(
    selection: &TextSelection,
    node_id: DomNodeId,
    text_layout: &UnifiedLayout,
    layout_results: &DomLayoutResult,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    // Follow the focus (active) end of the selection
    scroll_cursor_into_view(
        &selection.focus.cursor,
        node_id, 
        text_layout,
        layout_results, 
        scroll_manager,
        options,
        now
    )
}
```

---

## 3. Integration Points

### 3.1 Focus Change → Scroll Node Into View

**Location**: `layout/src/managers/focus_cursor.rs` or event processing

```rust
/// Called after focus changes
pub fn on_focus_change(
    old_focus: Option<DomNodeId>,
    new_focus: Option<DomNodeId>,
    layout_results: &DomLayoutResult,
    scroll_manager: &mut ScrollManager,
    now: Instant,
) {
    if let Some(focused_node) = new_focus {
        // Default behavior: scroll focused element into view with "nearest" alignment
        let options = ScrollIntoViewOptions {
            block: ScrollLogicalPosition::Nearest,
            inline: ScrollLogicalPosition::Nearest,
            behavior: ScrollBehavior::Auto,
        };
        
        scroll_node_into_view(focused_node, layout_results, scroll_manager, options, now);
    }
}
```

### 3.2 Cursor Move → Scroll Cursor Into View

**Location**: `layout/src/managers/cursor.rs` or text editing code

```rust
/// Called when cursor position changes (arrow keys, typing, etc.)
pub fn on_cursor_move(
    cursor: &TextCursor,
    node_id: DomNodeId,
    text_layout: &UnifiedLayout,
    layout_results: &DomLayoutResult,
    scroll_manager: &mut ScrollManager,
    now: Instant,
) {
    let options = ScrollIntoViewOptions {
        block: ScrollLogicalPosition::Nearest,
        inline: ScrollLogicalPosition::Nearest,
        behavior: ScrollBehavior::Instant, // Cursor movement is usually instant
    };
    
    scroll_cursor_into_view(cursor, node_id, text_layout, layout_results, scroll_manager, options, now);
}
```

### 3.3 Text Input → Move Cursor → Scroll

**Location**: `layout/src/managers/text_input.rs` or `text3::edit`

```rust
/// After text is inserted, cursor moves, then scroll
pub fn after_text_input_applied(
    cursor: &TextCursor,
    node_id: DomNodeId,
    text_layout: &UnifiedLayout,
    layout_results: &DomLayoutResult,
    scroll_manager: &mut ScrollManager,
    now: Instant,
) {
    // Text input always follows cursor with instant scroll
    on_cursor_move(cursor, node_id, text_layout, layout_results, scroll_manager, now);
}
```

---

## 4. ContentEditable Field Architecture

### 4.1 Current State

Azul already has:
- `CursorManager`: Tracks cursor position (GraphemeClusterId)
- `SelectionManager`: Tracks selections with anchor/focus model
- `TextInputManager`: Records pending text edits
- `text3::cache::UnifiedLayout`: Text layout with `get_cursor_rect()`
- `text3::edit`: Text editing operations

### 4.2 What's Missing for Full ContentEditable

1. **Scroll-into-view on cursor move** (this document addresses)
2. **IME composition rendering** (showing composition inline)
3. **Multi-line text input handling** (Enter key behavior)
4. **Paste handling** (with HTML parsing for rich text)

### 4.3 ContentEditable Node Detection

Azul uses an explicit `contenteditable: bool` field on `NodeData`:

```rust
// NodeData has a direct contenteditable field for performance
pub struct NodeData {
    pub node_type: NodeType,
    pub dataset: OptionRefAny,
    pub ids_and_classes: IdOrClassVec,
    pub attributes: AttributeVec,
    pub callbacks: CoreCallbackDataVec,
    pub css_props: CssPropertyWithConditionsVec,
    pub tab_index: OptionTabIndex,
    pub contenteditable: bool,  // Explicit flag for inline editing
    extra: Option<Box<NodeDataExt>>,
}

// API methods for contenteditable:
impl NodeData {
    pub fn set_contenteditable(&mut self, contenteditable: bool);
    pub fn is_contenteditable(&self) -> bool;
    pub fn with_contenteditable(self, contenteditable: bool) -> Self;
}

// Usage in event processing:
fn is_contenteditable(node_data: &NodeData) -> bool {
    node_data.is_contenteditable()
}

// Or directly:
if styled_dom.get_node(node_id).contenteditable {
    // Handle as editable content
}
```

This is more efficient than string attribute lookup and provides a clear API
for C/C++/Python bindings (NodeData_setContentEditable, NodeData_isContentEditable,
NodeData_withContentEditable).

---

## 5. Code Editor Architecture (Vec<Dom> with Lines)

### 5.1 Concept

A code editor can be built as:

```rust
struct CodeEditor {
    /// Each line is a separate contenteditable node
    lines: Vec<String>,
    /// Cursors (multi-cursor support)
    cursors: Vec<CodeEditorCursor>,
    /// Selections
    selections: Vec<CodeEditorSelection>,
}

struct CodeEditorCursor {
    line: usize,          // Which line (0-indexed)
    column: usize,        // Column within line
    preferred_column: usize, // For vertical movement
}

/// Maps to a DOM like:
/// <div class="code-editor" style="overflow: scroll">
///     <div class="line" contenteditable>line 1 content</div>
///     <div class="line" contenteditable>line 2 content</div>
///     <div class="line" contenteditable>line 3 content</div>
///     ...
/// </div>
```

### 5.2 Cursor Movement Between Lines

```rust
impl CodeEditor {
    pub fn move_cursor_down(&mut self, cursor_idx: usize) {
        let cursor = &mut self.cursors[cursor_idx];
        if cursor.line + 1 < self.lines.len() {
            cursor.line += 1;
            // Clamp column to new line length, but remember preferred
            let line_len = self.lines[cursor.line].len();
            cursor.column = cursor.preferred_column.min(line_len);
        }
        
        // After movement, scroll the new cursor position into view
        // This is where scroll_cursor_into_view() is called
    }
    
    pub fn move_cursor_up(&mut self, cursor_idx: usize) {
        let cursor = &mut self.cursors[cursor_idx];
        if cursor.line > 0 {
            cursor.line -= 1;
            let line_len = self.lines[cursor.line].len();
            cursor.column = cursor.preferred_column.min(line_len);
        }
    }
}
```

### 5.3 Cross-Line Selection

```rust
struct CodeEditorSelection {
    /// Anchor: where selection started
    anchor: CodeEditorPosition,
    /// Focus: where selection ends (follows cursor)
    focus: CodeEditorPosition,
}

struct CodeEditorPosition {
    line: usize,
    column: usize,
}

impl CodeEditor {
    /// Get the text of a selection (may span multiple lines)
    pub fn get_selection_text(&self, selection: &CodeEditorSelection) -> String {
        let (start, end) = selection.ordered(); // anchor/focus sorted
        
        if start.line == end.line {
            // Single line selection
            self.lines[start.line][start.column..end.column].to_string()
        } else {
            // Multi-line selection
            let mut result = String::new();
            
            // First line (from start.column to end)
            result.push_str(&self.lines[start.line][start.column..]);
            result.push('\n');
            
            // Middle lines (full lines)
            for line_idx in (start.line + 1)..end.line {
                result.push_str(&self.lines[line_idx]);
                result.push('\n');
            }
            
            // Last line (from start to end.column)
            result.push_str(&self.lines[end.line][..end.column]);
            
            result
        }
    }
}
```

---

## 6. Multi-Cursor System (Azul-Native)

### 6.1 Overview

Azul provides **built-in multi-cursor support** for contenteditable nodes. This is not W3C-standard
but essential for textarea, input, and code editor components. The multi-cursor state is tracked
in the `CursorManager` and rendered via CSS-configurable cursor styles.

### 6.2 Core Types

```rust
/// Multi-cursor state for a single contenteditable node
/// Stored in CursorManager alongside the primary cursor
#[derive(Debug, Clone, PartialEq)]
pub struct MultiCursorState {
    /// Primary cursor (keyboard input target)
    pub primary: TextCursor,
    /// Additional cursors (receive duplicated input)
    pub secondary: Vec<TextCursor>,
    /// Associated selections for each cursor
    pub cursor_selections: Vec<Option<CursorSelection>>,
}

/// Selection attached to a specific cursor
#[derive(Debug, Clone, PartialEq)]
pub struct CursorSelection {
    /// Anchor position (where selection started)
    pub anchor: GraphemeClusterId,
    /// Focus follows the cursor
    pub focus: GraphemeClusterId,
}

impl MultiCursorState {
    /// Create with single cursor (default)
    pub fn single(cursor: TextCursor) -> Self {
        Self {
            primary: cursor,
            secondary: Vec::new(),
            cursor_selections: Vec::new(),
        }
    }
    
    /// Add a cursor at position (Ctrl+Click or Ctrl+D)
    pub fn add_cursor(&mut self, cursor: TextCursor) {
        // Avoid duplicate positions
        if self.primary.cluster_id == cursor.cluster_id {
            return;
        }
        if self.secondary.iter().any(|c| c.cluster_id == cursor.cluster_id) {
            return;
        }
        self.secondary.push(cursor);
        self.cursor_selections.push(None);
    }
    
    /// Remove cursor nearest to position
    pub fn remove_cursor_at(&mut self, position: GraphemeClusterId) {
        if let Some(idx) = self.secondary.iter().position(|c| c.cluster_id == position) {
            self.secondary.remove(idx);
            self.cursor_selections.remove(idx);
        }
    }
    
    /// Get all cursors (primary + secondary)
    pub fn all_cursors(&self) -> impl Iterator<Item = &TextCursor> {
        core::iter::once(&self.primary).chain(self.secondary.iter())
    }
    
    /// Get mutable cursors for batch operations
    pub fn all_cursors_mut(&mut self) -> impl Iterator<Item = &mut TextCursor> {
        core::iter::once(&mut self.primary).chain(self.secondary.iter_mut())
    }
    
    /// Collapse all cursors to primary only
    pub fn collapse_to_primary(&mut self) {
        self.secondary.clear();
        self.cursor_selections.clear();
    }
}
```

### 6.3 CursorManager Extension

```rust
// In layout/src/managers/cursor.rs

impl CursorManager {
    /// Get multi-cursor state if active
    pub fn get_multi_cursor(&self) -> Option<&MultiCursorState> {
        self.multi_cursor.as_ref()
    }
    
    /// Enable multi-cursor mode at current position
    pub fn enable_multi_cursor(&mut self) {
        if let Some(cursor) = self.cursor.clone() {
            self.multi_cursor = Some(MultiCursorState::single(cursor));
        }
    }
    
    /// Add cursor at click position (Ctrl+Click)
    pub fn add_cursor_at_position(
        &mut self, 
        position: GraphemeClusterId,
        affinity: CursorAffinity,
    ) {
        if let Some(ref mut multi) = self.multi_cursor {
            multi.add_cursor(TextCursor {
                cluster_id: position,
                affinity,
            });
        } else if let Some(primary) = self.cursor.clone() {
            let mut multi = MultiCursorState::single(primary);
            multi.add_cursor(TextCursor {
                cluster_id: position,
                affinity,
            });
            self.multi_cursor = Some(multi);
        }
    }
    
    /// Add cursor at next occurrence of selected text (Ctrl+D)
    pub fn add_cursor_at_next_occurrence(&mut self, _text_layout: &UnifiedLayout) {
        // TODO: Find next occurrence of selection and add cursor
    }
    
    /// Process key input for all cursors
    pub fn broadcast_key_to_cursors(
        &mut self, 
        key: VirtualKeyCode, 
        text_layout: &mut UnifiedLayout,
    ) -> Vec<TextEdit> {
        let mut edits = Vec::new();
        
        if let Some(ref mut multi) = self.multi_cursor {
            // Move all cursors (reversed to handle positions correctly)
            for cursor in multi.all_cursors_mut() {
                if let Some(edit) = process_cursor_key(cursor, key, text_layout) {
                    edits.push(edit);
                }
            }
            
            // Merge overlapping cursors after movement
            multi.merge_overlapping();
        }
        
        edits
    }
}
```

### 6.4 CSS Styling for Cursors

Cursors are rendered via CSS pseudo-elements or custom rendering. Azul defines:

```css
/* Primary cursor styling (always visible) */
::cursor {
    width: 2px;
    background-color: currentColor;
    animation: cursor-blink 1s step-end infinite;
}

/* Secondary cursors (multi-cursor mode) */
::cursor-secondary {
    width: 1px;
    background-color: currentColor;
    opacity: 0.7;
    animation: cursor-blink 1s step-end infinite;
}

/* Cursor blink animation */
@keyframes cursor-blink {
    0%, 50% { opacity: 1; }
    50.01%, 100% { opacity: 0; }
}

/* Selection highlight for multi-cursors */
::selection {
    background-color: Highlight;
    color: HighlightText;
}

::selection-secondary {
    background-color: rgba(0, 120, 215, 0.3);
}
```

**Note**: These are pseudo-elements that Azul's text rendering system handles internally.
The actual CSS properties are read from the node's computed style.

### 6.5 Keyboard Shortcuts for Multi-Cursor

| Shortcut | Action | Implementation |
|----------|--------|----------------|
| Ctrl+Click | Add cursor at click position | `CursorManager::add_cursor_at_position()` |
| Ctrl+D | Add cursor at next occurrence | `CursorManager::add_cursor_at_next_occurrence()` |
| Ctrl+Shift+L | Add cursor to all occurrences | Search + add cursors |
| Escape | Collapse to primary cursor | `MultiCursorState::collapse_to_primary()` |
| Alt+Click | Column/block selection mode | Create vertical cursor column |

---

## 7. Selection Scoping (Ctrl+A)

### 7.1 Problem Statement

In browsers, Ctrl+A (Select All) behaves contextually:

- Inside a `<textarea>` or `<input>`: Selects only that element's content
- Inside a contenteditable with focus: Selects that element's content
- On body/document focus: Selects entire page content

Azul implements this context-aware selection through **selection scoping**.

### 7.2 Selection Scope Resolution

```rust
/// Determine the scope for select-all based on current focus
pub fn resolve_select_all_scope(
    focus_manager: &FocusManager,
    styled_dom: &StyledDom,
) -> SelectAllScope {
    // 1. Get currently focused node
    let focused = match focus_manager.get_focused_node() {
        Some(node) => node,
        None => {
            // No focus - select body/root content
            return SelectAllScope::Body;
        }
    };
    
    // 2. Check if focused node is contenteditable
    let node_data = styled_dom.get_node_data(focused.node_id);
    if node_data.is_contenteditable() {
        return SelectAllScope::Node(focused);
    }
    
    // 3. Walk up to find nearest contenteditable ancestor
    let mut current = focused.node_id;
    while let Some(parent) = styled_dom.get_parent(current) {
        let parent_data = styled_dom.get_node_data(parent);
        if parent_data.is_contenteditable() {
            return SelectAllScope::Node(DomNodeId { 
                dom_id: focused.dom_id, 
                node_id: parent 
            });
        }
        current = parent;
    }
    
    // 4. No contenteditable ancestor - select body
    SelectAllScope::Body
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectAllScope {
    /// Select content within specific contenteditable node
    Node(DomNodeId),
    /// Select all content in body/document
    Body,
}
```

### 7.3 Execute Select All

```rust
/// Execute Ctrl+A with proper scoping
pub fn execute_select_all(
    scope: SelectAllScope,
    selection_manager: &mut SelectionManager,
    cursor_manager: &mut CursorManager,
    styled_dom: &StyledDom,
    text_cache: &TextLayoutCache,
) {
    match scope {
        SelectAllScope::Node(node_id) => {
            // Select all text in this contenteditable node
            if let Some(text_layout) = text_cache.get_layout(node_id) {
                let text_len = text_layout.total_grapheme_count();
                
                // Set selection from start to end
                let selection = TextSelection {
                    anchor: SelectionAnchor::Position(GraphemeClusterId::start()),
                    focus: SelectionFocus::Position(GraphemeClusterId::at(text_len)),
                };
                
                selection_manager.set_text_selection(node_id.dom_id, selection);
                
                // Move cursor to end of selection
                cursor_manager.set_cursor(
                    Some(TextCursor {
                        cluster_id: GraphemeClusterId::at(text_len),
                        affinity: CursorAffinity::Trailing,
                    }),
                    Some(CursorLocation {
                        dom_id: node_id.dom_id,
                        node_id: node_id.node_id,
                    }),
                );
            }
        }
        SelectAllScope::Body => {
            // Select all visible/selectable content
            // This is more complex - may involve multiple nodes
            // For now, mark body-level selection
            selection_manager.set_body_selection();
        }
    }
}
```

### 7.4 Integration with DefaultAction

```rust
// In layout/src/default_actions.rs

pub fn determine_default_action(
    key: VirtualKeyCode,
    modifiers: KeyboardModifiers,
    focus_manager: &FocusManager,
    styled_dom: &StyledDom,
) -> Option<DefaultAction> {
    let ctrl = modifiers.ctrl_down();
    let shift = modifiers.shift_down();
    
    match key {
        VirtualKeyCode::A if ctrl && !shift => {
            // Ctrl+A - Select All (scoped)
            let scope = resolve_select_all_scope(focus_manager, styled_dom);
            Some(DefaultAction::SelectAll { scope })
        }
        // ... other shortcuts
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DefaultAction {
    // ... existing actions
    
    /// Select all text within scope
    SelectAll { scope: SelectAllScope },
}
```

---

## 8. Code Editor Widget Architecture

### 8.1 Overview

A code editor can be built as a Vec<Dom> where each line is a separate contenteditable element.
This architecture enables:

- Efficient partial re-rendering (only changed lines)
- Virtualized scrolling (only visible lines in DOM)
- Per-line syntax highlighting
- Cross-line cursor movement via Azul's multi-cursor system

### 8.2 Core Structure

```rust
/// Full-featured code editor widget
pub struct CodeEditor {
    /// Each line as a string
    lines: Vec<String>,
    /// Multi-cursor state (may span lines)
    cursors: CodeEditorCursors,
    /// Syntax highlighting language
    language: Option<SyntaxLanguage>,
    /// Undo/redo history
    history: EditHistory,
    /// View state (scroll position, visible range)
    view: CodeEditorView,
}

/// Cursor state specifically for multi-line code editor
pub struct CodeEditorCursors {
    /// Primary cursor
    primary: CodeEditorCursor,
    /// Secondary cursors (multi-cursor editing)
    secondary: Vec<CodeEditorCursor>,
}

pub struct CodeEditorCursor {
    /// Line index (0-based)
    line: usize,
    /// Column within line (grapheme index)
    column: usize,
    /// Preferred column for vertical movement
    /// (remembers column when moving through shorter lines)
    preferred_column: usize,
    /// Selection attached to this cursor (if any)
    selection: Option<CodeEditorSelection>,
}

pub struct CodeEditorSelection {
    /// Anchor position
    anchor_line: usize,
    anchor_column: usize,
    /// Focus follows cursor
    focus_line: usize,
    focus_column: usize,
}
```

### 8.3 DOM Structure

```rust
impl CodeEditor {
    /// Render to DOM
    pub fn render(&self) -> Dom {
        let visible_range = self.view.visible_line_range();
        
        Dom::div()
            .with_class("code-editor")
            .with_children(
                // Gutter (line numbers)
                Dom::div()
                    .with_class("gutter")
                    .with_children(
                        visible_range.clone().map(|line_num| {
                            Dom::div()
                                .with_class("line-number")
                                .with_text(&format!("{}", line_num + 1))
                        })
                    ),
                // Code area
                Dom::div()
                    .with_class("code-area")
                    .with_children(
                        visible_range.map(|line_num| {
                            self.render_line(line_num)
                        })
                    ),
            )
    }
    
    fn render_line(&self, line_num: usize) -> Dom {
        let line_content = &self.lines[line_num];
        let has_cursor = self.cursors.cursors_on_line(line_num);
        
        let mut line_dom = Dom::div()
            .with_class("code-line")
            .with_contenteditable(true)  // Uses new NodeData flag
            .with_attribute(("data-line", line_num.to_string()));
        
        // Apply syntax highlighting tokens if available
        if let Some(ref lang) = self.language {
            line_dom = self.apply_syntax_highlighting(line_dom, line_content, lang);
        } else {
            line_dom = line_dom.with_text(line_content);
        }
        
        line_dom
    }
}
```

### 8.4 Cross-Line Cursor Movement

```rust
impl CodeEditorCursors {
    /// Move cursor down (Arrow Down)
    pub fn move_down(&mut self, lines: &[String]) {
        for cursor in self.all_cursors_mut() {
            if cursor.line + 1 < lines.len() {
                cursor.line += 1;
                // Use preferred_column, clamp to line length
                let line_len = grapheme_count(&lines[cursor.line]);
                cursor.column = cursor.preferred_column.min(line_len);
            }
        }
        self.merge_overlapping();
    }
    
    /// Move cursor up (Arrow Up)
    pub fn move_up(&mut self, lines: &[String]) {
        for cursor in self.all_cursors_mut() {
            if cursor.line > 0 {
                cursor.line -= 1;
                let line_len = grapheme_count(&lines[cursor.line]);
                cursor.column = cursor.preferred_column.min(line_len);
            }
        }
        self.merge_overlapping();
    }
    
    /// Move cursor to start of line (Home)
    pub fn move_to_line_start(&mut self) {
        for cursor in self.all_cursors_mut() {
            cursor.column = 0;
            cursor.preferred_column = 0;
        }
    }
    
    /// Move cursor to end of line (End)
    pub fn move_to_line_end(&mut self, lines: &[String]) {
        for cursor in self.all_cursors_mut() {
            let line_len = grapheme_count(&lines[cursor.line]);
            cursor.column = line_len;
            cursor.preferred_column = line_len;
        }
    }
    
    /// Handle Enter key - split line at cursor(s)
    pub fn handle_enter(&mut self, lines: &mut Vec<String>) -> Vec<LineEdit> {
        let mut edits = Vec::new();
        
        // Process cursors in reverse line order to maintain indices
        let mut sorted_cursors: Vec<_> = self.all_cursors()
            .enumerate()
            .collect();
        sorted_cursors.sort_by(|a, b| b.1.line.cmp(&a.1.line)
            .then(b.1.column.cmp(&a.1.column)));
        
        for (idx, cursor) in sorted_cursors {
            let line = cursor.line;
            let col = cursor.column;
            
            // Split line
            let current_line = &lines[line];
            let (before, after) = split_at_grapheme(current_line, col);
            
            lines[line] = before.to_string();
            lines.insert(line + 1, after.to_string());
            
            edits.push(LineEdit::SplitLine { at_line: line, at_column: col });
        }
        
        // Update cursor positions (all move to next line, column 0)
        for cursor in self.all_cursors_mut() {
            cursor.line += 1;
            cursor.column = 0;
            cursor.preferred_column = 0;
        }
        
        edits
    }
    
    /// Merge overlapping cursors after movement
    fn merge_overlapping(&mut self) {
        // Remove secondary cursors that overlap with primary or each other
        let primary_pos = (self.primary.line, self.primary.column);
        
        self.secondary.retain(|c| {
            (c.line, c.column) != primary_pos
        });
        
        // Deduplicate secondary cursors
        let mut seen = vec![primary_pos];
        self.secondary.retain(|c| {
            let pos = (c.line, c.column);
            if seen.contains(&pos) {
                false
            } else {
                seen.push(pos);
                true
            }
        });
    }
}
```

### 8.5 Integration with Azul Event System

```rust
impl CodeEditor {
    /// Handle keyboard event from Azul
    pub fn on_keyboard_event(
        &mut self,
        event: &KeyboardEvent,
        callback_info: &mut CallbackInfo,
    ) -> Update {
        let key = event.virtual_keycode;
        let modifiers = event.modifiers;
        
        match key {
            // Navigation
            VirtualKeyCode::Up => {
                self.cursors.move_up(&self.lines);
                self.scroll_cursor_into_view(callback_info);
                Update::RefreshDom
            }
            VirtualKeyCode::Down => {
                self.cursors.move_down(&self.lines);
                self.scroll_cursor_into_view(callback_info);
                Update::RefreshDom
            }
            VirtualKeyCode::Home if modifiers.ctrl => {
                // Ctrl+Home - go to document start
                self.cursors.move_to_document_start();
                self.scroll_cursor_into_view(callback_info);
                Update::RefreshDom
            }
            VirtualKeyCode::End if modifiers.ctrl => {
                // Ctrl+End - go to document end
                self.cursors.move_to_document_end(&self.lines);
                self.scroll_cursor_into_view(callback_info);
                Update::RefreshDom
            }
            
            // Multi-cursor (Ctrl+D - add cursor at next occurrence)
            VirtualKeyCode::D if modifiers.ctrl => {
                if let Some(word) = self.get_word_at_primary_cursor() {
                    if let Some(next_pos) = self.find_next_occurrence(&word) {
                        self.cursors.add_secondary(next_pos);
                    }
                }
                Update::RefreshDom
            }
            
            // Escape - collapse to single cursor
            VirtualKeyCode::Escape => {
                self.cursors.collapse_to_primary();
                Update::RefreshDom
            }
            
            // Enter - split line(s)
            VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
                let edits = self.cursors.handle_enter(&mut self.lines);
                self.history.push_edits(edits);
                self.scroll_cursor_into_view(callback_info);
                Update::RefreshDom
            }
            
            _ => Update::DoNothing,
        }
    }
    
    fn scroll_cursor_into_view(&self, callback_info: &mut CallbackInfo) {
        // Get primary cursor's line node
        let line_node_id = self.get_line_node_id(self.cursors.primary.line);
        if let Some(node_id) = line_node_id {
            // Use Azul's scroll_node_into_view
            callback_info.scroll_node_into_view(node_id, ScrollIntoViewOptions::nearest());
        }
    }
}
```

---

## 9. Implementation Plan

### Phase 1: Core `scroll_rect_into_view` (Priority: HIGH)

**New file**: `layout/src/managers/scroll_into_view.rs`

```rust
// Core implementation
pub fn scroll_rect_into_view(
    target_rect: LogicalRect,
    target_node: DomNodeId,
    layout_results: &DomLayoutResult,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    let mut adjustments = Vec::new();
    
    // 1. Find scrollable ancestors from target to root
    let scroll_ancestors = find_scrollable_ancestors(target_node, layout_results);
    
    // 2. For each scrollable ancestor (innermost first)
    let mut current_rect = target_rect;
    
    for ancestor_id in scroll_ancestors {
        let ancestor_visible_rect = get_visible_rect(ancestor_id, layout_results, scroll_manager);
        
        // 3. Calculate scroll delta based on options.block and options.inline
        let delta = calculate_scroll_delta(
            current_rect, 
            ancestor_visible_rect, 
            options.block, 
            options.inline
        );
        
        if delta.x.abs() > 0.001 || delta.y.abs() > 0.001 {
            // 4. Apply scroll
            let behavior = resolve_scroll_behavior(options.behavior, ancestor_id, layout_results);
            
            apply_scroll_adjustment(
                scroll_manager, 
                ancestor_id, 
                delta, 
                behavior, 
                now
            );
            
            adjustments.push(ScrollAdjustment {
                scroll_container: ancestor_id,
                delta,
                behavior,
            });
            
            // 5. Adjust current_rect for next iteration (relative to new scroll)
            current_rect.origin.x -= delta.x;
            current_rect.origin.y -= delta.y;
        }
    }
    
    adjustments
}
```

### Phase 2: Integration with Focus System

**Modify**: `layout/src/managers/focus_cursor.rs` and event processing

1. After `resolve_focus_target()` returns a new focus node
2. Call `scroll_node_into_view()` for that node
3. Apply scroll adjustments before next frame

### Phase 3: Integration with Cursor/Text Input

**Modify**: `layout/src/managers/cursor.rs` and `text_input.rs`

1. After cursor position changes
2. Get cursor rect via `text_layout.get_cursor_rect()`
3. Call `scroll_cursor_into_view()`

### Phase 4: C API Exposure

**File**: `dll/src/api/...`

```c
// Scroll node into view
void AzDom_scrollIntoView(
    AzDomNodeId node_id, 
    AzScrollIntoViewOptions options
);

// Scroll arbitrary rect into view
void AzWindow_scrollRectIntoView(
    AzWindowHandle window,
    AzLogicalRect rect,
    AzDomNodeId context_node,
    AzScrollIntoViewOptions options
);
```

### Phase 5: Debug API Endpoint

```json
{"op": "scroll_into_view", "node_id": 5, "block": "center", "behavior": "smooth"}
```

### Phase 6: Multi-Cursor System (Priority: MEDIUM)

**Modify**: `layout/src/managers/cursor.rs`

1. Add `MultiCursorState` struct with primary + secondary cursors
2. Add `multi_cursor: Option<MultiCursorState>` field to `CursorManager`
3. Implement `add_cursor_at_position()` for Ctrl+Click
4. Implement `broadcast_key_to_cursors()` for synchronized editing
5. Implement `merge_overlapping()` to prevent duplicate cursors

**Modify**: `layout/src/default_actions.rs`

1. Add `DefaultAction::AddCursor` for Ctrl+Click detection
2. Add `DefaultAction::AddCursorNextOccurrence` for Ctrl+D
3. Add `DefaultAction::CollapseCursors` for Escape

**New CSS pseudo-elements** (handled in text rendering):
- `::cursor` - primary cursor styling
- `::cursor-secondary` - secondary cursor styling

### Phase 7: Selection Scoping / SelectAll (Priority: MEDIUM)

**New file**: `layout/src/managers/select_all.rs`

1. `resolve_select_all_scope()` - find nearest contenteditable ancestor
2. `execute_select_all()` - select content within scope

**Modify**: `layout/src/default_actions.rs`

1. Add `DefaultAction::SelectAll { scope: SelectAllScope }`
2. Handle Ctrl+A with scoping

**Modify**: `core/src/events.rs`

1. Add `SelectAllScope` enum to event types

### Phase 8: Code Editor Widget (Priority: LOW)

**New file**: `examples/rust/code_editor.rs`

1. Reference implementation of code editor using:
   - `Vec<String>` for lines
   - `with_contenteditable(true)` for each line node
   - Cross-line cursor management
   - Syntax highlighting (optional)
   - Undo/redo history

---

## 7. Key Algorithms

### 7.1 Find Scrollable Ancestors

```rust
fn find_scrollable_ancestors(
    node_id: DomNodeId,
    layout_results: &DomLayoutResult,
) -> Vec<DomNodeId> {
    let mut ancestors = Vec::new();
    let mut current = node_id;
    
    while let Some(parent) = get_parent(current, layout_results) {
        if is_scrollable(parent, layout_results) {
            ancestors.push(parent);
        }
        current = parent;
    }
    
    // Optionally add viewport as final scroll container
    if let Some(viewport) = get_viewport_scroll_node(layout_results) {
        ancestors.push(viewport);
    }
    
    ancestors // Innermost first
}

fn is_scrollable(node: DomNodeId, layout: &DomLayoutResult) -> bool {
    // Check overflow-x/overflow-y is scroll or auto
    // AND content exceeds container
    let overflow = get_computed_overflow(node, layout);
    let has_overflow_x = overflow.x == Overflow::Scroll || overflow.x == Overflow::Auto;
    let has_overflow_y = overflow.y == Overflow::Scroll || overflow.y == Overflow::Auto;
    
    if !has_overflow_x && !has_overflow_y {
        return false;
    }
    
    // Check if content overflows
    let container = get_content_box(node, layout);
    let content = get_scroll_content_size(node, layout);
    
    (has_overflow_x && content.width > container.width) ||
    (has_overflow_y && content.height > container.height)
}
```

### 7.2 Calculate Scroll Delta

```rust
fn calculate_scroll_delta(
    target: LogicalRect,
    container: LogicalRect,
    block: ScrollLogicalPosition,
    inline: ScrollLogicalPosition,
) -> LogicalPosition {
    LogicalPosition {
        x: calculate_axis_delta(
            target.origin.x, 
            target.size.width,
            container.origin.x, 
            container.size.width,
            inline
        ),
        y: calculate_axis_delta(
            target.origin.y, 
            target.size.height,
            container.origin.y, 
            container.size.height,
            block
        ),
    }
}

fn calculate_axis_delta(
    target_start: f32,
    target_size: f32,
    container_start: f32,
    container_size: f32,
    position: ScrollLogicalPosition,
) -> f32 {
    let target_end = target_start + target_size;
    let container_end = container_start + container_size;
    
    match position {
        ScrollLogicalPosition::Start => {
            // Align target start with container start
            target_start - container_start
        }
        ScrollLogicalPosition::End => {
            // Align target end with container end
            target_end - container_end
        }
        ScrollLogicalPosition::Center => {
            // Center target in container
            let target_center = target_start + target_size / 2.0;
            let container_center = container_start + container_size / 2.0;
            target_center - container_center
        }
        ScrollLogicalPosition::Nearest => {
            // Minimum scroll to make fully visible
            if target_start < container_start {
                // Target is above/left of visible area
                target_start - container_start
            } else if target_end > container_end {
                // Target is below/right of visible area
                if target_size <= container_size {
                    // Target fits, align end
                    target_end - container_end
                } else {
                    // Target doesn't fit, align start
                    target_start - container_start
                }
            } else {
                // Target is already fully visible
                0.0
            }
        }
    }
}
```

---

## 10. Test Cases

### 10.1 Focus Navigation Tests

- [ ] Tab to off-screen element scrolls it into view
- [ ] Shift+Tab backwards scrolls previous element into view
- [ ] Focus wrap-around scrolls to start/end of list
- [ ] Nested scroll containers both scroll appropriately
- [ ] Focus into iframe scrolls the iframe AND the content

### 10.2 Cursor Movement Tests

- [ ] Arrow keys at text edge scroll the cursor into view
- [ ] Typing at right edge scrolls horizontally
- [ ] Enter key in text field scrolls new line into view
- [ ] Page Up/Down scrolls and moves cursor
- [ ] Ctrl+End scrolls to document end

### 10.3 Selection Tests

- [ ] Drag selection off visible area auto-scrolls
- [ ] Shift+Arrow extends selection and scrolls focus into view
- [ ] Double-click word selection scrolls word into view
- [ ] Triple-click line selection scrolls line into view

### 10.4 Multi-Cursor Tests

- [ ] Ctrl+Click adds cursor at click position
- [ ] Ctrl+D adds cursor at next occurrence of selection
- [ ] Escape collapses to single cursor
- [ ] Typing with multi-cursor inserts at all positions
- [ ] Backspace with multi-cursor deletes at all positions
- [ ] Arrow keys move all cursors simultaneously
- [ ] Overlapping cursors merge automatically

### 10.5 Select All (Ctrl+A) Scoping Tests

- [ ] Ctrl+A in contenteditable selects only that node's content
- [ ] Ctrl+A in nested contenteditable selects innermost scope
- [ ] Ctrl+A with no contenteditable focus selects body content
- [ ] Ctrl+A inside textarea selects only textarea content
- [ ] Ctrl+A after clicking outside selects entire document

### 10.6 Code Editor Tests

- [ ] Arrow Up/Down move cursor across lines
- [ ] Home/End move to line start/end
- [ ] Ctrl+Home/End move to document start/end
- [ ] Enter splits line at all cursor positions
- [ ] Multi-cursor typing inserts at all positions
- [ ] Scroll follows primary cursor

---

## 11. File Structure

```
layout/src/managers/
├── mod.rs                 # Add scroll_into_view, select_all modules
├── scroll_state.rs        # Existing - scroll positions
├── scroll_into_view.rs    # NEW - scroll-into-view algorithms
├── select_all.rs          # NEW - scoped select-all logic
├── focus_cursor.rs        # Modify - call scroll after focus change
├── cursor.rs              # Modify - add MultiCursorState, scroll after move
├── selection.rs           # Modify - call scroll after selection change
└── text_input.rs          # Modify - call scroll after text input

core/src/
├── dom.rs                 # NodeData.contenteditable field (✅ DONE)
├── events.rs              # SelectAllScope enum
└── selection.rs           # MultiCursorState types
```

---

## 12. Summary

| Feature | Input | Output | Uses |
|---------|-------|--------|------|
| `scroll_rect_into_view` | LogicalRect + node | Vec<ScrollAdjustment> | Core primitive |
| `scroll_node_into_view` | DomNodeId | calls scroll_rect | Focus, Click |
| `scroll_cursor_into_view` | TextCursor + node | calls scroll_rect | Text editing |
| `scroll_selection_into_view` | Selection | calls scroll_cursor | Selection drag |
| `MultiCursorState` | Primary + secondary cursors | Synchronized edits | Code editors |
| `resolve_select_all_scope` | FocusManager + DOM | SelectAllScope | Ctrl+A handling |
| `execute_select_all` | SelectAllScope | Selection update | Text selection |

### Key Insights

1. **Scroll-into-view**: All functionality reduces to one primitive: scroll a rectangle into the visible area of its scroll container ancestry.

2. **Multi-cursor**: Not W3C-standard but essential for modern text editing. Cursors broadcast operations and merge when overlapping.

3. **Selection scoping**: Ctrl+A respects the focused contenteditable context rather than always selecting the entire page.

4. **Code editor widget**: Built from per-line contenteditable nodes with cross-line cursor management handled by Azul's core.

---

*Document created: January 2026*
*Updated: January 2026 - Added multi-cursor, selection scoping, code editor widget*
*Architecture for Azul scroll, cursor, and text input*

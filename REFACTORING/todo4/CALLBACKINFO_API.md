# CallbackInfo API Reference

Complete list of methods available on `CallbackInfo` for the kitchen sink example.

## Transaction System (Modern API)
All modifications go through `push_change()` which queues changes to be applied after callback returns.

### Change Management
- `push_change(change: CallbackChange)` - Core method for all modifications

### Timer Management
- `add_timer(timer_id: TimerId, timer: Timer)`
- `remove_timer(timer_id: TimerId)`
- `get_timer(&TimerId) -> Option<&Timer>`
- `get_timer_ids() -> Vec<TimerId>`

### Thread Management  
- `add_thread(thread_id: ThreadId, thread: Thread)`
- `remove_thread(thread_id: ThreadId)`
- `get_thread(&ThreadId) -> Option<&Thread>`
- `get_thread_ids() -> Vec<ThreadId>`

### Focus & Event Control
- `set_focus(target: FocusTarget)`
- `stop_propagation()`
- `prevent_default()`

### Window Management
- `create_window(options: WindowCreateOptions)`
- `close_window()`
- `modify_window_state(state: FullWindowState)`
- `get_current_window_state() -> &FullWindowState`
- `get_previous_window_state() -> &Option<FullWindowState>`
- `get_current_window_handle() -> RawWindowHandle`

### Node Modification
- `change_node_text(dom_id: DomId, node_id: NodeId, text: AzString)`
- `change_node_image(dom_id: DomId, node_id: NodeId, image: ImageRef, update_type: UpdateImageType)`
- `update_image_callback(dom_id: DomId, node_id: NodeId)` - Re-render image callback
- `change_node_image_mask(dom_id: DomId, node_id: NodeId, mask: ImageMask)`
- `change_node_css_properties(dom_id: DomId, node_id: NodeId, properties: Vec<CssProperty>)`

### Scrolling
- `scroll_to(dom_id: DomId, node_id: NodeHierarchyItemId, position: LogicalPosition)`
- `get_scroll_manager() -> &ScrollManager`

### Text Input & Changeset API (NEW)
- `get_text_changeset() -> Option<&TextChangeset>` - Get pending text input
- `set_text_changeset(changeset: TextChangeset)` - Override text input
- `insert_text(dom_id: DomId, node_id: NodeId, text: AzString)`
- `move_cursor(dom_id: DomId, node_id: NodeId, cursor: TextCursor)`
- `set_selection(dom_id: DomId, node_id: NodeId, selection: Selection)`
- `get_selection(&DomId) -> Option<&SelectionState>`
- `has_selection(&DomId) -> bool`
- `get_primary_cursor(&DomId) -> Option<TextCursor>`
- `get_selection_ranges(&DomId) -> Vec<SelectionRange>`

### Menu System
- `open_menu(menu: Menu)`
- `open_menu_at(menu: Menu, position: LogicalPosition)`
- `open_menu_for_node(menu: Menu, node_id: DomNodeId) -> bool`
- `open_menu_for_hit_node(menu: Menu) -> bool`

### Tooltip System
- `show_tooltip(text: AzString)`
- `show_tooltip_at(text: AzString, position: LogicalPosition)`
- `hide_tooltip()`

### Image Cache
- `add_image_to_cache(id: AzString, image: ImageRef)`
- `remove_image_from_cache(id: AzString)`
- `reload_system_fonts()`

### Node Query API
- `get_hit_node() -> DomNodeId`
- `get_node_size(node_id: DomNodeId) -> Option<LogicalSize>`
- `get_node_position(node_id: DomNodeId) -> Option<LogicalPosition>`
- `get_node_rect(node_id: DomNodeId) -> Option<LogicalRect>`
- `get_hit_node_rect() -> Option<LogicalRect>`
- `get_hit_node_layout_rect() -> Option<LogicalRect>`

### Node Hierarchy Navigation
- `get_parent(node_id: DomNodeId) -> Option<DomNodeId>`
- `get_previous_sibling(node_id: DomNodeId) -> Option<DomNodeId>`
- `get_next_sibling(node_id: DomNodeId) -> Option<DomNodeId>`
- `get_first_child(node_id: DomNodeId) -> Option<DomNodeId>`
- `get_last_child(node_id: DomNodeId) -> Option<DomNodeId>`

### Node Data
- `get_dataset(node_id: DomNodeId) -> Option<RefAny>`
- `get_node_id_of_root_dataset(search_key: RefAny) -> Option<DomNodeId>`
- `get_string_contents(node_id: DomNodeId) -> Option<AzString>`

### Layout & GPU Access
- `get_gpu_cache(&DomId) -> Option<&GpuValueCache>`
- `get_layout_result(&DomId) -> Option<&DomLayoutResult>`
- `get_dom_ids() -> Vec<DomId>`
- `get_layout_window() -> &LayoutWindow`
- `get_text_cache() -> &LayoutCache<FontRef>`

### CSS Query API
- `get_computed_css_property(node_id: DomNodeId, property_type: CssPropertyType) -> Option<CssProperty>`
- `get_computed_width(node_id: DomNodeId) -> Option<CssProperty>`
- `get_computed_height(node_id: DomNodeId) -> Option<CssProperty>`

### Input State
- `get_current_keyboard_state() -> KeyboardState`
- `get_current_mouse_state() -> MouseState`
- `get_previous_keyboard_state() -> Option<KeyboardState>`
- `get_previous_mouse_state() -> Option<MouseState>`
- `get_cursor_relative_to_node() -> OptionLogicalPosition`
- `get_cursor_relative_to_viewport() -> OptionLogicalPosition`
- `get_cursor_position() -> Option<LogicalPosition>`

### System
- `get_system_style() -> Arc<SystemStyle>`
- `get_system_time_fn() -> GetSystemTimeCallback`
- `get_current_time() -> Instant`

## Key Differences from Old API

### Text Input (MAJOR CHANGE)
**OLD**: Used `current_char` on keyboard state
```rust
// OLD WAY - DEPRECATED
let char = info.get_current_keyboard_state().current_char;
```

**NEW**: Uses changeset system
```rust
// NEW WAY
if let Some(changeset) = info.get_text_changeset() {
    eprintln!("Inserting: {}", changeset.inserted_text);
    eprintln!("Old text: {}", changeset.old_text);
    
    // Can modify or prevent
    info.prevent_default();
}
```

### Contenteditable
Use `contenteditable="true"` attribute on DOM nodes for native text input handling.

### Accessibility
Text inputs automatically get ARIA roles and labels when using contenteditable.

## Example Usage

```rust
extern "C" fn on_text_input(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    if let Some(changeset) = info.get_text_changeset() {
        // Inspect what's being typed
        let new_text = format!("{}{}", changeset.old_text, changeset.inserted_text);
        
        // Validate input (e.g., numbers only)
        if !new_text.chars().all(|c| c.is_numeric()) {
            info.prevent_default();
            return Update::DoNothing;
        }
    }
    Update::RefreshDom
}
```

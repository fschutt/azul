# CallbackInfo Refactoring Plan

## Goals
1. **Clear separation** between read-only and mutable data
2. **Transaction-based system** - all changes are collected in a Vec<CallbackChange>
3. **Extensible** - easy to add new change types
4. **Backward compatible API** - same method signatures

## New Structure

```rust
pub struct CallbackInfo {
    // ===== READ-ONLY DATA (Manager / Query Access) =====
    read_only_layout_window: *const LayoutWindow,
    read_only_renderer_resources: *const RendererResources,
    read_only_previous_window_state: *const Option<FullWindowState>,
    read_only_current_window_state: *const FullWindowState,
    read_only_gl_context: *const OptionGlContextPtr,
    read_only_current_scroll_manager: *const BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>>,
    read_only_scroll_manager: *const ScrollManager,
    read_only_gesture_drag_manager: *const GestureAndDragManager,
    read_only_current_window_handle: *const RawWindowHandle,
    read_only_system_callbacks: *const ExternalSystemCallbacks,
    read_only_system_style: Arc<SystemStyle>,
    
    // ===== CONTEXT INFO (Immutable Event Data) =====
    hit_dom_node: DomNodeId,
    cursor_relative_to_item: OptionLogicalPosition,
    cursor_in_viewport: OptionLogicalPosition,
    
    // ===== MUTABLE DATA (Transaction Container) =====
    changes: *mut Vec<CallbackChange>,
}

pub enum CallbackChange {
    // Window state changes
    ModifyWindowState(WindowState),
    CreateNewWindow(WindowCreateOptions),
    
    // Focus changes
    SetFocusTarget(Option<FocusTarget>),
    
    // Event propagation
    StopPropagation,
    
    // Timer/Thread management
    AddTimer { id: TimerId, timer: Timer },
    RemoveTimer(TimerId),
    AddThread { id: ThreadId, thread: Thread },
    RemoveThread(ThreadId),
    
    // Content modifications
    ChangeNodeText { dom: DomId, node: NodeId, text: AzString },
    ChangeNodeImage { dom: DomId, node: NodeId, image: ImageRef, update_type: UpdateImageType },
    ChangeNodeImageMask { dom: DomId, node: NodeId, mask: ImageMask },
    ChangeNodeCssProperties { dom: DomId, node: NodeId, properties: Vec<CssProperty> },
    
    // Scroll changes
    ScrollTo { dom: DomId, node: NodeHierarchyItemId, position: LogicalPosition },
    
    // Cache modifications
    AddImageToCache { id: AzString, image: ImageRef },
    RemoveImageFromCache(AzString),
}
```

## Implementation Strategy

### Phase 1: Create new types (non-breaking)
- Add `CallbackChange` enum
- Keep old `CallbackInfo` structure
- Add new methods that push to changes vec

### Phase 2: Migrate internal implementation
- Refactor `CallbackInfo` structure
- Update `new()` constructor
- Keep old API methods, delegate to change system

### Phase 3: Add query API
- Add all LayoutWindow query methods
- Add all manager query methods
- Make API comprehensive

### Phase 4: Migration
- Update all callback sites to use new structure
- Process changes after callback execution
- Remove old direct mutation code

## Benefits

1. **Transaction Safety**: All changes are validated and applied atomically
2. **Debugging**: Can log/inspect all changes made by callback
3. **Undo/Redo**: Changes vec can be reversed
4. **Testing**: Easy to verify callback behavior
5. **Future-proof**: Easy to add new change types
6. **Performance**: Batch processing of changes possible

## Migration Path

Old code:
```rust
fn my_callback(info: &mut CallbackInfo) -> Update {
    *info.stop_propagation_mut() = true;
    info.add_timer(id, timer);
    Update::DoNothing
}
```

New code (same API):
```rust
fn my_callback(info: &mut CallbackInfo) -> Update {
    info.stop_propagation(); // Now pushes StopPropagation change
    info.add_timer(id, timer); // Now pushes AddTimer change
    Update::DoNothing
}
```

Implementation processes changes after callback returns.

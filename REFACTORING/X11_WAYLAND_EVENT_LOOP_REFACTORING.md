# X11 and Wayland Event Loop Refactoring Plan

## Overview

This document outlines the complete refactoring of X11 and Wayland backends to match the Windows and macOS event loop architecture, integrating proper CSD and menu support.

## Architecture Analysis

### Windows/macOS Event Loop Pattern

The Windows and macOS implementations share a common architecture:

1. **State-Diffing Approach**
   - Store `previous_window_state` before every state change
   - Update `current_window_state` based on OS events
   - Call `create_events_from_states()` to detect changes
   - Use `dispatch_events()` from `azul_core::events` for routing

2. **Recursive Callback Processing**
   - `process_window_events_v2()` → `process_window_events_recursive_v2(depth)`
   - Max depth: 5 levels to prevent infinite loops
   - Callbacks can modify DOM, triggering recursion

3. **Event Handlers**
   ```rust
   fn handle_mouse_down(&mut self, ...) -> ProcessEventResult {
       // 1. Save previous state
       self.previous_window_state = Some(self.current_window_state.clone());
       
       // 2. Update current state
       self.current_window_state.mouse_state.left_down = true;
       
       // 3. Update hit test
       self.update_hit_test(position);
       
       // 4. Process events via V2 system
       let result = self.process_window_events_v2();
       
       // 5. Convert to platform result
       self.convert_result(result)
   }
   ```

4. **Scrollbar Integration**
   - `perform_scrollbar_hit_test()` uses WebRender hit-testing
   - `handle_scrollbar_click()` for thumb/track clicks
   - `handle_scrollbar_drag()` for continuous dragging
   - `gpu_scroll()` for efficient GPU-only scrolling

5. **CSD Integration**
   - Injected in `regenerate_layout()` via `csd::wrap_user_dom_with_decorations()`
   - CSD button callbacks processed through normal callback system
   - No platform-specific code needed

6. **Menu Integration**
   - Menus are proper Azul windows with `MarshaledLayoutCallback`
   - Use `menu_renderer::create_menu_styled_dom()` for rendering
   - Menu events handled through normal callback system
   - `create_menu_window_options()` creates `WindowCreateOptions`

## Current X11 Implementation Issues

### 1. Old Event Handling (`events.rs`)
- Direct state modification without previous state tracking
- No integration with `dispatch_events()` system
- Manual event routing instead of using hit-testing
- No support for recursive callbacks
- **Status**: Needs complete rewrite

### 2. Stub CSD (`decorations.rs`)
- Contains only placeholder code:
  ```rust
  pub fn create_decorations_dom(window_title: &str, has_menu: bool) -> Dom {
      Dom::div() // Placeholder
  }
  ```
- Not integrated with `csd.rs` implementation
- **Status**: Should be removed, CSD handled in `regenerate_layout()`

### 3. Legacy Menu System (`menu_old.rs` + `mod.rs` references)
- Uses X11 primitive drawing (`XDrawString`, `XFillRectangle`)
- Creates raw X11 windows instead of Azul windows
- Manual event handling with `XGrabPointer`
- **Status**: Remove `menu_old.rs`, update `mod.rs`

### 4. Incomplete `regenerate_layout()`
- Stub implementation, doesn't call layout system
- No CSD injection
- No display list building
- **Status**: Needs full implementation from macOS

### 5. Missing Helper Methods
- No `update_hit_test()`
- No `invoke_callbacks_v2()`
- No `process_callback_result_v2()`
- No `sync_window_state()`
- **Status**: Port from macOS

## Implementation Plan

### Phase 1: Events Module Refactoring ✅ DONE

**File**: `dll/src/desktop/shell2/linux/x11/events_new.rs` (created)

**Implemented**:
- ✅ `process_window_events_v2()` with recursive callback handling
- ✅ `invoke_callbacks_v2()` for callback dispatch
- ✅ `process_callback_result_v2()` for result processing
- ✅ Event handlers using state-diffing pattern:
  - `handle_mouse_button()` - mouse clicks
  - `handle_mouse_move()` - mouse movement
  - `handle_mouse_crossing()` - enter/leave window
  - `handle_keyboard()` - key press/release
- ✅ Scrollbar support:
  - `perform_scrollbar_hit_test()`
  - `handle_scrollbar_click()`
  - `handle_scrollbar_drag()`
  - `gpu_scroll()`
- ✅ Helper methods:
  - `update_hit_test()`
  - `get_first_hovered_node()`
  - `get_raw_window_handle()`
- ✅ Context menu support:
  - `try_show_context_menu()`
- ✅ IME support preserved from original

### Phase 2: Update X11Window Structure

**File**: `dll/src/desktop/shell2/linux/x11/mod.rs`

**Changes Needed**:
```rust
pub struct X11Window {
    // ... existing fields ...
    
    // ADD: Scrollbar drag state
    pub scrollbar_drag_state: Option<azul_layout::ScrollbarDragState>,
    
    // ADD: For full WindowState tracking
    pub current_window_state: azul_layout::window_state::FullWindowState,  // Change from WindowState
    pub previous_window_state: Option<azul_layout::window_state::FullWindowState>,
    
    // REMOVE: Legacy menu system
    // menu_manager: Option<menu::MenuManager>,  // DELETE THIS
}
```

**Update Methods**:
1. Replace `process_events()` calls with `process_window_events_v2()`
2. Update `poll_event()` to use new event handlers:
   ```rust
   defines::ButtonPress | defines::ButtonRelease => {
       let result = self.handle_mouse_button(unsafe { &event.button });
       if result != ProcessEventResult::DoNothing {
           self.request_redraw();
       }
   }
   ```
3. Remove `inject_menu_bar()` and `show_popup_menu()` - menus now created via callbacks

### Phase 3: Implement Full regenerate_layout()

**File**: `dll/src/desktop/shell2/linux/x11/mod.rs`

**Port from macOS** (`dll/src/desktop/shell2/macos/mod.rs:2650-2750`):
```rust
pub fn regenerate_layout(&mut self) -> Result<(), String> {
    let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;
    
    // 1. Borrow app_data (need to add Arc<RefCell<RefAny>> to X11Window)
    let mut app_data_borrowed = self.resources.app_data.borrow_mut();
    
    // 2. Create LayoutCallbackInfo with SystemStyle extension
    let mut callback_info = crate::desktop::callback_ext::new_with_system_style(
        self.current_window_state.size.clone(),
        self.current_window_state.theme,
        &self.image_cache,
        &self.gl_context_ptr,
        &*self.resources.fc_cache,
        &self.resources.system_style,
    );
    
    // 3. Call layout_callback to get StyledDom
    let user_styled_dom = match &self.current_window_state.layout_callback {
        LayoutCallback::Raw(inner) => {
            (inner.cb)(&mut *app_data_borrowed, &mut callback_info)
        }
        LayoutCallback::Marshaled(marshaled) => {
            (marshaled.cb.cb)(
                &mut marshaled.marshal_data.clone(),
                &mut *app_data_borrowed,
                &mut callback_info,
            )
        }
    };
    
    // 4. Inject CSD if needed
    let styled_dom = if crate::desktop::csd::should_inject_csd(
        self.current_window_state.flags.has_decorations,
        self.current_window_state.flags.decorations,
    ) {
        crate::desktop::csd::wrap_user_dom_with_decorations(
            user_styled_dom,
            &self.current_window_state.title,
            true,  // inject titlebar
            true,  // has minimize
            true,  // has maximize
            &self.resources.system_style,
        )
    } else {
        user_styled_dom
    };
    
    // 5. Perform layout with solver3
    layout_window.layout_and_generate_display_list(
        styled_dom,
        &self.current_window_state,
        &self.renderer_resources,
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )?;
    
    // 6. Calculate scrollbar states
    layout_window.scroll_states.calculate_scrollbar_states();
    
    // 7. Rebuild display list and send to WebRender
    let mut txn = WrTransaction::new();
    crate::desktop::wr_translate2::rebuild_display_list(
        &mut txn,
        layout_window,
        self.render_api.as_mut().unwrap(),
        &self.image_cache,
        Vec::new(),
        &mut self.renderer_resources,
        self.current_window_state.size.get_hidpi_factor(),
    );
    
    // 8. Mark frame needs regeneration
    self.frame_needs_regeneration = true;
    
    Ok(())
}
```

### Phase 4: Add sync_window_state()

**Port from macOS** (`dll/src/desktop/shell2/macos/mod.rs:1900-1970`):
```rust
fn sync_window_state(&mut self) {
    let (previous, current) = match &self.previous_window_state {
        Some(prev) => (prev.clone(), self.current_window_state.clone()),
        None => return,
    };
    
    // Title changed?
    if previous.title != current.title {
        let c_title = CString::new(current.title.as_str()).unwrap();
        unsafe {
            (self.xlib.XStoreName)(self.display, self.window, c_title.as_ptr());
        }
    }
    
    // Size changed?
    if previous.size.dimensions != current.size.dimensions {
        unsafe {
            (self.xlib.XResizeWindow)(
                self.display,
                self.window,
                current.size.dimensions.width as u32,
                current.size.dimensions.height as u32,
            );
        }
    }
    
    // Position changed?
    if previous.position != current.position {
        if let WindowPosition::Initialized(pos) = current.position {
            unsafe {
                (self.xlib.XMoveWindow)(
                    self.display,
                    self.window,
                    pos.x,
                    pos.y,
                );
            }
        }
    }
    
    // Visibility changed?
    if previous.flags.is_visible != current.flags.is_visible {
        if current.flags.is_visible {
            unsafe { (self.xlib.XMapWindow)(self.display, self.window) };
        } else {
            unsafe { (self.xlib.XUnmapWindow)(self.display, self.window) };
        }
    }
    
    // Cursor synchronization
    if let Some(layout_window) = self.layout_window.as_ref() {
        let cursor_test = layout_window.compute_cursor_type_hit_test(&current.last_hit_test);
        self.set_cursor(cursor_test.cursor_icon);
    }
}
```

### Phase 5: Remove Legacy Code

**Files to Modify**:

1. **Delete**: `dll/src/desktop/shell2/linux/x11/menu_old.rs`
   - Old X11 primitive menu system
   
2. **Delete**: `dll/src/desktop/shell2/linux/x11/decorations.rs`
   - Stub CSD implementation
   
3. **Rename**: `events_new.rs` → `events.rs`
   - Replace old events.rs with refactored version

4. **Update**: `dll/src/desktop/shell2/linux/x11/mod.rs`
   - Remove `mod menu;` (keep only `pub mod menu;` for new menu.rs)
   - Remove `mod decorations;`
   - Remove `menu_manager` field from X11Window
   - Remove `inject_menu_bar()` and `show_popup_menu()` methods
   - Update event handling in `poll_event()` to use new methods

### Phase 6: Add Missing Fields to X11Window

**File**: `dll/src/desktop/shell2/linux/x11/mod.rs`

**Add to X11Window**:
```rust
pub struct X11Window {
    // ... existing fields ...
    
    // State tracking (upgrade to FullWindowState)
    pub current_window_state: azul_layout::window_state::FullWindowState,
    pub previous_window_state: Option<azul_layout::window_state::FullWindowState>,
    
    // Scrollbar drag state
    pub scrollbar_drag_state: Option<azul_layout::ScrollbarDragState>,
    
    // Frame regeneration flag
    pub frame_needs_regeneration: bool,
    
    // Shared resources (already has some via Arc<AppResources>)
    // Make sure resources contains:
    // - fc_cache: Arc<FcFontCache>
    // - system_style: Arc<SystemStyle>
    // - app_data: Arc<RefCell<RefAny>>
}
```

### Phase 7: Port to Wayland

**Files**: `dll/src/desktop/shell2/linux/wayland/*.rs`

**Same Changes**:
1. Copy refactored `events.rs` pattern
2. Implement `process_window_events_v2()`
3. Implement `regenerate_layout()` with CSD
4. Use `menu.rs` for menu popups
5. Add scrollbar handling
6. Remove any legacy menu code

**Wayland-Specific**:
- Use `wl_surface` instead of X11 Window
- Handle Wayland protocols for window management
- CSD is mandatory on Wayland (no window manager decorations)

## Testing Plan

### Test Cases

1. **Mouse Events**
   - Click detection
   - Hover states
   - Drag operations
   - Mouse enter/leave

2. **Keyboard Input**
   - Key press/release
   - Character input
   - IME composition
   - Modifier keys

3. **Scrolling**
   - Mouse wheel scrolling
   - Scrollbar thumb dragging
   - Track clicking
   - GPU scroll performance

4. **CSD Buttons**
   - Minimize button
   - Maximize button
   - Close button
   - Titlebar dragging

5. **Menus**
   - Context menus on right-click
   - Menu item selection
   - Menu positioning
   - Keyboard navigation

6. **Window State**
   - Resize synchronization
   - Title changes
   - Position changes
   - Visibility toggling

## Benefits of New Architecture

### 1. Platform Consistency
- X11, Wayland, macOS, and Windows all use identical event processing
- Bugs fixed once apply to all platforms
- Easier to maintain

### 2. Proper Callback System
- Callbacks can modify DOM
- Recursive processing with depth limit
- Stop propagation support
- Proper event bubbling

### 3. CSD Support
- Native look via SystemStyle
- Works through normal callback system
- No platform-specific code
- Automatic on Wayland, optional on X11

### 4. Menu System
- Proper Azul windows, not primitives
- StyledDom rendering via menu_renderer
- Event handling through callbacks
- Works identically on all platforms

### 5. Scrollbar Integration
- WebRender hit-testing
- GPU-only scrolling (no relayout)
- Smooth dragging
- Proper thumb/track interaction

### 6. State Synchronization
- Window title, size, position synced automatically
- Cursor type updated from hit-testing
- Focus state tracked
- Background color support

## Migration Notes

### Breaking Changes
- `X11Window::inject_menu_bar()` removed - use menu callbacks instead
- `X11Window::show_popup_menu()` removed - use `info.create_window()` with menu options
- `MenuManager` removed - menus are now proper windows
- `decorations.rs` removed - CSD via `csd.rs`

### Compatibility
- IME support preserved (XIM)
- All existing callbacks continue to work
- No API changes for end users
- Only internal shell2 changes

## Implementation Status

- [x] Phase 1: Create new events.rs with V2 system
- [ ] Phase 2: Update X11Window structure
- [ ] Phase 3: Implement regenerate_layout()
- [ ] Phase 4: Add sync_window_state()
- [ ] Phase 5: Remove legacy code
- [ ] Phase 6: Add missing fields
- [ ] Phase 7: Port to Wayland
- [ ] Phase 8: Testing and validation

## Next Steps

1. Replace old `events.rs` with `events_new.rs`
2. Update `mod.rs` to match new architecture
3. Implement `regenerate_layout()` fully
4. Test on X11 with simple example
5. Port changes to Wayland
6. Comprehensive testing

## Files Created

- `dll/src/desktop/shell2/linux/x11/events_new.rs` - Complete V2 event system for X11

## Files to Modify

- `dll/src/desktop/shell2/linux/x11/mod.rs` - Update structure and methods
- `dll/src/desktop/shell2/linux/x11/events.rs` - Replace with events_new.rs
- `dll/src/desktop/shell2/linux/wayland/*.rs` - Port same changes

## Files to Delete

- `dll/src/desktop/shell2/linux/x11/menu_old.rs` - Legacy X11 menu system
- `dll/src/desktop/shell2/linux/x11/decorations.rs` - Stub CSD implementation

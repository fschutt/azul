# CSD/Fallback Menu System - Implementation Progress

**Date**: 2025-10-28  
**Session**: Implementation Session 1  
**Status**: 🟢 Phase 1 Complete

## Completed: Phase 1 - Core Infrastructure

### ✅ 1. Window Type System

**File**: `core/src/window.rs`

Added new window classification system:

```rust
pub enum WindowType {
    Normal,    // Standard application window
    Menu,      // Popup menu window (auto-closing, always-on-top)
    Tooltip,   // Tooltip window (non-interactive)
    Dialog,    // Modal dialog window
}
```

**File**: `core/src/window.rs`

Extended `WindowFlags` with new fields:

```rust
pub struct WindowFlags {
    // ... existing fields ...
    
    /// Window type classification
    pub window_type: WindowType,
    
    /// Enable client-side decorations (custom titlebar)
    pub has_decorations: bool,
}
```

**Compilation**: ✅ SUCCESS

### ✅ 2. Window Query API

**File**: `dll/src/desktop/window_helpers.rs` (NEW)

Created trait for unified window state queries:

```rust
pub trait WindowQuery {
    fn get_flags(&self) -> &WindowFlags;
    
    // Convenience methods:
    fn is_menu_window(&self) -> bool;
    fn is_tooltip_window(&self) -> bool;
    fn is_dialog_window(&self) -> bool;
    fn has_focus(&self) -> bool;
    fn close_requested(&self) -> bool;
    fn has_csd(&self) -> bool;
}
```

**Implementations**:
- ✅ `MacOSWindow` implements `WindowQuery`
- ✅ `Win32Window` implements `WindowQuery`
- ✅ `X11Window` implements `WindowQuery`

**Compilation**: ✅ SUCCESS

### ✅ 3. Menu Injection API Stubs

**File**: `dll/src/desktop/shell2/macos/mod.rs`

```rust
impl MacOSWindow {
    pub fn inject_menu_bar(&mut self) -> Result<(), String> {
        // TODO: Implement native NSMenu creation
        eprintln!("[inject_menu_bar] TODO: Implement native macOS menu injection");
        Ok(())
    }
}
```

**File**: `dll/src/desktop/shell2/windows/mod.rs`

```rust
impl Win32Window {
    pub fn inject_menu_bar(&mut self) -> Result<(), String> {
        // TODO: Implement native HMENU creation
        eprintln!("[inject_menu_bar] TODO: Implement native Windows menu injection");
        Ok(())
    }
}
```

**File**: `dll/src/desktop/shell2/linux/x11/mod.rs`

```rust
impl X11Window {
    pub fn inject_menu_bar(&mut self) -> Result<(), String> {
        // TODO: Implement fallback popup menu system
        eprintln!("[inject_menu_bar] TODO: Implement fallback menu system for Linux X11");
        Ok(())
    }
}
```

**Compilation**: ✅ SUCCESS

### Summary of Changes

**Files Modified**:
1. `core/src/window.rs` - Added WindowType enum, extended WindowFlags
2. `dll/src/desktop/mod.rs` - Added window_helpers module
3. `dll/src/desktop/window_helpers.rs` - NEW: Window query trait
4. `dll/src/desktop/shell2/macos/mod.rs` - Added inject_menu_bar() and WindowQuery impl
5. `dll/src/desktop/shell2/windows/mod.rs` - Added inject_menu_bar() and WindowQuery impl
6. `dll/src/desktop/shell2/linux/x11/mod.rs` - Added inject_menu_bar() and WindowQuery impl

**Compilation Status**: ✅ All code compiles without errors (only benign warnings in examples)

## Next Steps: Phase 2 - CSD Titlebar

The next phase will create the CSD (Client-Side Decorations) module:

### Planned Tasks

1. **Create CSD Module** (`dll/src/desktop/csd.rs`)
   - Titlebar DOM generation function
   - Built-in callbacks for close/minimize/maximize
   - Default CSS styling for titlebar

2. **Integrate CSD into Layout Pipeline**
   - Detect `has_decorations` flag during layout
   - Inject titlebar DOM before user content
   - Wire up window control callbacks

3. **Test on All Platforms**
   - Verify titlebar appears on frameless windows
   - Test window controls (close, minimize, maximize)
   - Ensure callbacks properly modify window state

### Implementation Strategy

The CSD titlebar will be injected automatically during the layout callback phase:

```rust
// Pseudo-code for CSD injection
if window.flags.has_decorations && window.decorations == WindowDecorations::None {
    let titlebar_dom = csd::create_titlebar(&window.title);
    user_dom.prepend_child(titlebar_dom);
}
```

Built-in callbacks will modify window state flags:

```rust
fn on_close_button(info: &mut CallbackInfo) -> Update {
    info.window_state_mut().flags.close_requested = true;
    Update::DoNothing
}
```

After callbacks execute, the shell processes state changes:

```rust
if window.close_requested() {
    window.close();
}
```

---

**Ready for Phase 2**: All foundational infrastructure is in place. The window type system and query APIs provide the building blocks for CSD and menu systems.

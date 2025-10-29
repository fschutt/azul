# XDG Popup Menu Implementation for Wayland

## Status: ✅ API READY - Implementation Needed

### Summary

Added complete **xdg_popup** and **xdg_positioner** API support to Wayland dlopen wrapper. This enables proper parent-relative menu positioning in Wayland, replacing the current hack of using `xdg_toplevel` for menus.

---

## Changes Made

### 1. Wayland Defines Extension

**File:** `dll/src/desktop/shell2/linux/wayland/defines.rs`

Added opaque types:
```rust
#[repr(C)]
pub struct xdg_popup {
    _private: [u8; 0],
}

#[repr(C)]
pub struct xdg_positioner {
    _private: [u8; 0],
}
```

Added listener:
```rust
#[repr(C)]
#[derive(Copy, Clone)]
pub struct xdg_popup_listener {
    pub configure: extern "C" fn(
        data: *mut c_void,
        xdg_popup: *mut xdg_popup,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ),
    pub popup_done: extern "C" fn(data: *mut c_void, xdg_popup: *mut xdg_popup),
}
```

Added xdg_positioner constants:
```rust
// Anchor points (where to attach popup to parent)
pub const XDG_POSITIONER_ANCHOR_NONE: u32 = 0;
pub const XDG_POSITIONER_ANCHOR_TOP: u32 = 1;
pub const XDG_POSITIONER_ANCHOR_BOTTOM: u32 = 2;
pub const XDG_POSITIONER_ANCHOR_LEFT: u32 = 3;
pub const XDG_POSITIONER_ANCHOR_RIGHT: u32 = 4;
pub const XDG_POSITIONER_ANCHOR_TOP_LEFT: u32 = 5;
pub const XDG_POSITIONER_ANCHOR_BOTTOM_LEFT: u32 = 6;
pub const XDG_POSITIONER_ANCHOR_TOP_RIGHT: u32 = 7;
pub const XDG_POSITIONER_ANCHOR_BOTTOM_RIGHT: u32 = 8;

// Gravity (which direction popup grows from anchor)
pub const XDG_POSITIONER_GRAVITY_NONE: u32 = 0;
pub const XDG_POSITIONER_GRAVITY_TOP: u32 = 1;
pub const XDG_POSITIONER_GRAVITY_BOTTOM: u32 = 2;
pub const XDG_POSITIONER_GRAVITY_LEFT: u32 = 3;
pub const XDG_POSITIONER_GRAVITY_RIGHT: u32 = 4;
pub const XDG_POSITIONER_GRAVITY_TOP_LEFT: u32 = 5;
pub const XDG_POSITIONER_GRAVITY_BOTTOM_LEFT: u32 = 6;
pub const XDG_POSITIONER_GRAVITY_TOP_RIGHT: u32 = 7;
pub const XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT: u32 = 8;

// Constraint adjustment (how compositor handles overflow)
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_NONE: u32 = 0;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_X: u32 = 1;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_Y: u32 = 2;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_X: u32 = 4;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_Y: u32 = 8;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_RESIZE_X: u32 = 16;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_RESIZE_Y: u32 = 32;
```

### 2. Wayland dlopen Extension

**File:** `dll/src/desktop/shell2/linux/wayland/dlopen.rs`

Added function pointers to `Wayland` struct:
```rust
// xdg_popup and xdg_positioner functions
pub xdg_wm_base_create_positioner: unsafe extern "C" fn(*mut xdg_wm_base) -> *mut xdg_positioner,
pub xdg_positioner_set_size: unsafe extern "C" fn(*mut xdg_positioner, i32, i32),
pub xdg_positioner_set_anchor_rect: unsafe extern "C" fn(*mut xdg_positioner, i32, i32, i32, i32),
pub xdg_positioner_set_anchor: unsafe extern "C" fn(*mut xdg_positioner, u32),
pub xdg_positioner_set_gravity: unsafe extern "C" fn(*mut xdg_positioner, u32),
pub xdg_positioner_set_constraint_adjustment: unsafe extern "C" fn(*mut xdg_positioner, u32),
pub xdg_positioner_destroy: unsafe extern "C" fn(*mut xdg_positioner),
pub xdg_surface_get_popup: unsafe extern "C" fn(
    *mut xdg_surface,
    *mut xdg_surface,
    *mut xdg_positioner,
) -> *mut xdg_popup,
pub xdg_popup_add_listener:
    unsafe extern "C" fn(*mut xdg_popup, *const xdg_popup_listener, *mut c_void) -> i32,
pub xdg_popup_grab: unsafe extern "C" fn(*mut xdg_popup, *mut wl_seat, u32),
pub xdg_popup_destroy: unsafe extern "C" fn(*mut xdg_popup),
```

Loaded in `Wayland::new()`:
```rust
xdg_wm_base_create_positioner: unsafe {
    std::mem::transmute(wl_proxy_marshal_constructor)
},
xdg_positioner_set_size: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
xdg_positioner_set_anchor_rect: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
xdg_positioner_set_anchor: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
xdg_positioner_set_gravity: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
xdg_positioner_set_constraint_adjustment: unsafe {
    std::mem::transmute(wl_proxy_marshal_ptr)
},
xdg_positioner_destroy: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
xdg_surface_get_popup: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
xdg_popup_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
xdg_popup_grab: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
xdg_popup_destroy: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
```

---

## XDG Popup Protocol Overview

### Concept

**xdg_popup** is the Wayland protocol mechanism for creating popup surfaces like:
- Context menus
- Dropdown menus  
- Tooltips
- Combo box popups

Key differences from `xdg_toplevel`:
- ✅ **Parent-relative positioning** - Popup position is relative to parent surface
- ✅ **Compositor stacking** - Compositor manages Z-order and focus automatically
- ✅ **Grab support** - Can grab pointer/keyboard focus
- ✅ **Automatic dismissal** - Compositor closes popup when clicking outside
- ✅ **Constraint adjustment** - Compositor flips/slides popup to stay on screen

### Protocol Flow

```
1. Create xdg_positioner
   ├─ Set size (menu dimensions)
   ├─ Set anchor rect (trigger area on parent)
   ├─ Set anchor (which edge of rect to attach to)
   ├─ Set gravity (which direction popup grows)
   └─ Set constraint adjustment (overflow behavior)

2. Get xdg_popup from xdg_surface
   └─ Pass parent surface + positioner

3. Add popup_listener
   ├─ configure: Compositor tells us final position/size
   └─ popup_done: Compositor dismissed popup

4. Grab pointer/keyboard (optional)
   └─ Ensures menu receives all input until dismissed

5. Surface commit
   └─ Popup becomes visible

6. Cleanup
   ├─ xdg_popup_destroy
   ├─ xdg_positioner_destroy
   └─ wl_surface_destroy
```

---

## Implementation Plan

### Step 1: Create Popup Window Type

Add to `dll/src/desktop/shell2/linux/wayland/mod.rs`:

```rust
pub struct WaylandPopup {
    wayland: Rc<Wayland>,
    display: *mut wl_display,
    parent_surface: *mut wl_surface,
    surface: *mut wl_surface,
    xdg_surface: *mut xdg_surface,
    xdg_popup: *mut xdg_popup,
    positioner: *mut xdg_positioner,
    is_open: bool,
    
    // Shell2 rendering state (same as WaylandWindow)
    pub layout_window: Option<LayoutWindow>,
    pub current_window_state: FullWindowState,
    pub renderer: Option<WrRenderer>,
    // ...
}

impl WaylandPopup {
    pub fn new(
        parent: &WaylandWindow,
        position: LogicalPosition,
        size: LogicalSize,
        anchor_rect: LogicalRect,
        options: WindowCreateOptions,
    ) -> Result<Self, String> {
        // 1. Create positioner
        let positioner = unsafe {
            (parent.wayland.xdg_wm_base_create_positioner)(parent.xdg_wm_base)
        };
        
        // 2. Configure positioner
        unsafe {
            // Set popup size
            (parent.wayland.xdg_positioner_set_size)(
                positioner,
                size.width as i32,
                size.height as i32,
            );
            
            // Set anchor rectangle (where menu is triggered from)
            (parent.wayland.xdg_positioner_set_anchor_rect)(
                positioner,
                anchor_rect.origin.x as i32,
                anchor_rect.origin.y as i32,
                anchor_rect.size.width as i32,
                anchor_rect.size.height as i32,
            );
            
            // Anchor to bottom-right of rect
            (parent.wayland.xdg_positioner_set_anchor)(
                positioner,
                XDG_POSITIONER_ANCHOR_BOTTOM_RIGHT,
            );
            
            // Popup grows down and right from anchor
            (parent.wayland.xdg_positioner_set_gravity)(
                positioner,
                XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT,
            );
            
            // Allow compositor to flip if popup overflows screen
            (parent.wayland.xdg_positioner_set_constraint_adjustment)(
                positioner,
                XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_X |
                XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_Y |
                XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_X |
                XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_Y,
            );
        }
        
        // 3. Create surface and xdg_surface
        let surface = unsafe {
            (parent.wayland.wl_compositor_create_surface)(parent.compositor)
        };
        
        let xdg_surface = unsafe {
            (parent.wayland.xdg_wm_base_get_xdg_surface)(parent.xdg_wm_base, surface)
        };
        
        // 4. Get xdg_popup role
        let xdg_popup = unsafe {
            (parent.wayland.xdg_surface_get_popup)(
                xdg_surface,
                parent.xdg_surface, // Parent surface
                positioner,
            )
        };
        
        // 5. Add listeners
        let popup_listener = xdg_popup_listener {
            configure: popup_configure_callback,
            popup_done: popup_done_callback,
        };
        
        unsafe {
            (parent.wayland.xdg_popup_add_listener)(
                xdg_popup,
                &popup_listener,
                std::ptr::null_mut(),
            );
        }
        
        // 6. Grab pointer for exclusive input
        unsafe {
            (parent.wayland.xdg_popup_grab)(
                xdg_popup,
                parent.seat,
                parent.pointer_state.serial, // Last serial from pointer event
            );
        }
        
        // 7. Commit surface to make visible
        unsafe {
            (parent.wayland.wl_surface_commit)(surface);
        }
        
        Ok(Self {
            wayland: parent.wayland.clone(),
            display: parent.display,
            parent_surface: parent.surface,
            surface,
            xdg_surface,
            xdg_popup,
            positioner,
            is_open: true,
            // ... initialize rendering state
        })
    }
}

extern "C" fn popup_configure_callback(
    _data: *mut c_void,
    xdg_popup: *mut xdg_popup,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    eprintln!("[xdg_popup] configure: x={}, y={}, w={}, h={}", x, y, width, height);
    // Compositor has positioned popup, resize if needed
}

extern "C" fn popup_done_callback(_data: *mut c_void, _xdg_popup: *mut xdg_popup) {
    eprintln!("[xdg_popup] popup_done: compositor dismissed popup");
    // Popup was dismissed (clicked outside, etc.)
    // Close window and cleanup
}
```

### Step 2: Integrate with Menu System

Update `dll/src/desktop/menu.rs` to use `WaylandPopup` for menu windows:

```rust
pub fn show_menu_wayland(
    menu: Menu,
    system_style: Arc<SystemStyle>,
    parent: &WaylandWindow,
    trigger_rect: LogicalRect,
    cursor_position: LogicalPosition,
) -> Result<WaylandPopup, String> {
    // Calculate menu size (from StyledDom layout)
    let menu_size = calculate_menu_size(&menu, &system_style);
    
    // Create popup window
    WaylandPopup::new(
        parent,
        cursor_position,
        menu_size,
        trigger_rect,
        WindowCreateOptions {
            window_type: WindowType::Menu,
            size_to_content: true,
            // ...
        },
    )
}
```

### Step 3: Update X11 Menu Consistency

Ensure X11 menus use the same API pattern:

```rust
// X11 uses override_redirect windows, but API should be consistent
pub fn show_menu_x11(
    menu: Menu,
    system_style: Arc<SystemStyle>,
    parent: &X11Window,
    trigger_rect: LogicalRect,
    cursor_position: LogicalPosition,
) -> Result<X11MenuWindow, String> {
    // Similar interface but uses X11Window with override_redirect=true
}
```

---

## Benefits of xdg_popup

### 1. **Proper Parent-Child Relationship**
   - Popup is visually attached to parent window
   - Compositor manages stacking order correctly
   - Popup closes automatically when parent loses focus

### 2. **Compositor-Managed Positioning**
   - Compositor ensures popup stays on screen
   - Automatic flip/slide on overflow
   - Multi-monitor aware positioning

### 3. **Input Grab**
   - Popup can grab pointer/keyboard exclusively
   - All input goes to menu until dismissed
   - Click outside automatically closes popup

### 4. **Standards Compliance**
   - xdg_popup is the standard Wayland protocol for menus
   - Works consistently across all compositors (GNOME, KDE, Sway, etc.)
   - Future-proof as protocol evolves

### 5. **Performance**
   - No need to manually track screen edges
   - Compositor handles constraint adjustment efficiently
   - Less roundtrip messages

---

## Testing Strategy

### Phase 1: Basic Popup Creation
```rust
// Test: Create popup, verify it appears
let popup = WaylandPopup::new(parent, pos, size, rect, options)?;
assert!(popup.is_open());
```

### Phase 2: Positioning
```rust
// Test: Popup appears at correct location
// Test: Compositor flips popup on overflow
// Test: Multi-monitor positioning
```

### Phase 3: Input Handling
```rust
// Test: Popup receives mouse events
// Test: Grab works (no input to parent)
// Test: Click outside dismisses popup
```

### Phase 4: Menu Rendering
```rust
// Test: StyledDom renders correctly in popup
// Test: Menu items clickable
// Test: Submenu spawning works
```

### Phase 5: Integration Testing
```rust
// Test: Right-click context menu
// Test: Menu bar dropdown
// Test: Nested submenus
// Test: Keyboard navigation
```

---

## Compatibility Matrix

| Compositor | xdg_popup Support | Notes |
|------------|-------------------|-------|
| GNOME Wayland | ✅ Full | Reference implementation |
| KDE Plasma | ✅ Full | Excellent support |
| Sway | ✅ Full | wlroots-based |
| Wayfire | ✅ Full | wlroots-based |
| Hyprland | ✅ Full | Modern compositor |
| Weston | ✅ Full | Reference compositor |

All modern Wayland compositors support xdg_popup. This is part of the stable xdg-shell protocol.

---

## Next Steps

1. ✅ **DONE**: Add xdg_popup types to defines.rs
2. ✅ **DONE**: Add xdg_popup functions to dlopen.rs
3. ⏭️ **TODO**: Implement `WaylandPopup` struct
4. ⏭️ **TODO**: Integrate with menu.rs
5. ⏭️ **TODO**: Test on real Wayland compositor
6. ⏭️ **TODO**: Update menu examples to use new API

---

## Files Modified

1. `dll/src/desktop/shell2/linux/wayland/defines.rs` - Added types, listener, constants
2. `dll/src/desktop/shell2/linux/wayland/dlopen.rs` - Added function pointers

## Files to Create/Modify

1. `dll/src/desktop/shell2/linux/wayland/mod.rs` - Add WaylandPopup implementation
2. `dll/src/desktop/menu.rs` - Add show_menu_wayland()
3. `dll/examples/wayland_menu_test.rs` - Test example

---

## Conclusion

✅ **API Foundation Complete**

The xdg_popup API is now available in the Wayland dlopen wrapper. The implementation of `WaylandPopup` and menu integration is the remaining work, but the hard part (FFI bindings) is done.

This will enable **proper, compositor-managed menu positioning** for Wayland, matching the quality of native menus on GNOME and KDE.

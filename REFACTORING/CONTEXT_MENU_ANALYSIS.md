# GNOME DBus Menus for Context Menus - Analysis

## Current Status

The GNOME DBus menu implementation (`gnome_menu` module) is currently designed for **application menu bars** only. However, the DBus protocol theoretically supports popup menus.

## Context Menu Implementation in X11

Currently, context menus in X11 are implemented using **window-based rendering** (not native):

**Location:** `dll/src/desktop/shell2/linux/x11/events.rs`

```rust
fn try_show_context_menu(&mut self, node: HitTestNode, position: LogicalPosition) -> bool {
    // 1. Get context menu from node
    let context_menu = match node_data.get_context_menu() {
        Some(m) => m,
        None => return false,
    };
    
    // 2. Create a separate X11 window for the menu
    let menu_window = create_menu_window(
        (**context_menu).clone(),
        screen_pos,
        None, // No trigger rect for context menus
        &self.current_window_state,
        // ...
    );
    
    // 3. Register as child window
    self.menu_child_windows.push(menu_window);
}
```

## GTK DBus Protocol Limitations for Context Menus

### Problem 1: Menu Bar Only
The GTK DBus protocol (`org.gtk.Menus` and `org.gtk.Actions`) is specifically designed for:
- Application menu bar (top bar in GNOME Shell)
- App menu (GNOME 3.x only, deprecated in GNOME 40+)

**It does NOT support:**
- Popup menus at arbitrary positions
- Context menus triggered by right-click
- Dynamic position-based menus

### Problem 2: GNOME Shell Integration Required
For DBus menus to work, GNOME Shell must:
1. Read X11 window properties (`_GTK_MENUBAR_OBJECT_PATH`, etc.)
2. Query the DBus interface on application startup
3. Render the menu in the top bar

**Context menus require:**
- Dynamic creation on-demand
- Position at cursor or specific screen coordinates
- No GNOME Shell integration (app-level rendering)

### Problem 3: No GTK Popup Menu Protocol
GTK+ itself uses different mechanisms for popup menus:
- **GTK3:** `gtk_menu_popup_at_pointer()` - Local window-based rendering
- **GTK4:** `GtkPopoverMenu` - Still uses local windows
- **Wayland:** `xdg_popup` protocol - Shell compositor assistance

**None of these use DBus.**

## Alternative Approaches for Native Context Menus

### Option 1: XDG Desktop Portal (Recommended)
Use the **org.freedesktop.portal.FileChooser** pattern for context menus.

**Advantages:**
- Works on all Linux desktops (GNOME, KDE, XFCE, etc.)
- Not GNOME-specific
- Proper sandboxing support (Flatpak, Snap)

**Disadvantages:**
- Not yet a standard portal (would need custom specification)
- Complex implementation

### Option 2: libdbusmenu (Legacy)
Use **com.canonical.dbusmenu** protocol (Ubuntu Unity legacy).

**Advantages:**
- Designed for popup menus
- Some applications still use it (Skype, Dropbox, etc.)

**Disadvantages:**
- Deprecated (Unity is discontinued)
- GNOME Shell doesn't support it by default (needs extension)
- Poor maintenance

### Option 3: Keep Window-Based Rendering (Current Approach)
Continue using custom X11 windows for context menus.

**Advantages:**
- Full control over appearance
- Works on all desktops
- Already implemented

**Disadvantages:**
- Not "native" looking
- Requires custom rendering code
- May have focus/stacking issues

## Recommendation

**For Context Menus:** Keep the current window-based implementation.

**Reasons:**
1. DBus menus are menu-bar-only (by design)
2. No cross-desktop standard for native context menus exists
3. Window-based rendering works reliably
4. Other toolkits (Qt, GTK, Electron) also use window-based context menus

**For Application Menus:** Use GNOME DBus menus (already implemented).

## Implementation Strategy

```rust
// In X11Window::try_show_context_menu()

#[cfg(feature = "gnome-menus")]
fn try_show_context_menu(&mut self, node: HitTestNode, position: LogicalPosition) -> bool {
    // Check if we're on GNOME with native menu bar active
    if self.gnome_menu.is_some() && self.has_native_menu_bar() {
        // For consistency, use window-based rendering even on GNOME
        // (DBus protocol doesn't support context menus)
        self.show_window_based_context_menu(node, position)
    } else {
        // Standard window-based rendering
        self.show_window_based_context_menu(node, position)
    }
}

#[cfg(not(feature = "gnome-menus"))]
fn try_show_context_menu(&mut self, node: HitTestNode, position: LogicalPosition) -> bool {
    self.show_window_based_context_menu(node, position)
}
```

## Future Improvements

If a cross-desktop standard for native context menus emerges:
1. Monitor **XDG Desktop Portal** specifications
2. Check if **KDE Plasma** or **GNOME** introduces new protocols
3. Consider implementing **Wayland-specific** solutions (xdg_popup)

## Testing Context Menus

Current context menu functionality can be tested:

```rust
// In your application
dom.div()
    .with_context_menu(Menu::new(vec![
        MenuItem::String(StringMenuItem::new("Copy".into())),
        MenuItem::String(StringMenuItem::new("Paste".into())),
    ].into()))
```

**Expected behavior:**
- Right-click opens window-based context menu
- Works on all desktops (GNOME, KDE, XFCE, i3, etc.)
- Consistent appearance (styled via CSS)
- Full Azul rendering pipeline

## Conclusion

**GNOME DBus menus should NOT be used for context menus.**

The current window-based approach is the correct solution and matches how other cross-platform toolkits handle this problem.

# GNOME Native Menu Module

## ⚠️ Feature Flag Required

This module is **optional** and requires the `gnome-menus` feature flag:

```toml
[dependencies]
azul-dll = { version = "0.0.5", features = ["gnome-menus"] }
```

**Not enabled by default** to avoid breaking cross-compilation.

## Overview

This is a completely self-contained module for GNOME native 
menu integration via DBus. It implements the GTK DBus menu 
protocol (`org.gtk.Menus` and `org.gtk.Actions`) to integrate 
with GNOME Shell's global menu bar.

## ⚠️ Application Menus Only

**This implementation is for application menu bars only.**

For **context menus** (right-click menus), use the standard window-based 
rendering system. See `CONTEXT_MENU_ANALYSIS.md` for details on why 
DBus menus cannot be used for context menus.

**Summary:**
- ✅ Application menu bar → Use GNOME DBus menus (this module)
- ✅ Context menus → Use window-based rendering (existing implementation)

The GTK DBus protocol is specifically designed for menu bars in the 
GNOME Shell top bar and does not support popup menus at arbitrary positions.

## Module Structure

### `mod.rs` (Main Entry Point)

- Public API: `GnomeMenuManager`
- Detection logic: `should_use_gnome_menus()`
- Error types: `GnomeMenuError`
- Debug logging: `debug_log()`
- Coordinates all submodules

### `dbus_connection.rs`

- DBus session bus connection
- Service name registration
- Object path management
- Connection lifecycle

### `menu_protocol.rs`

- `org.gtk.Menus` interface implementation
- Menu structure storage
- `Start()` method - Subscribe to menu groups
- `End()` method - Unsubscribe from menu groups
- DBus format: `a(uuaa{sv})`

### `actions_protocol.rs`

- `org.gtk.Actions` interface implementation
- Action registration and storage
- `List()` - Return all action names
- `Describe()` - Return action details
- `DescribeAll()` - Return all actions with details
- `Activate()` - Invoke action callback

### `menu_conversion.rs`

- Convert Azul `Menu` → DBus menu groups
- Extract actions from menu items
- Generate unique action names
- Flatten menu tree to DBus groups

### `x11_properties.rs`

- Set X11 window properties via Xlib
- Properties: `_GTK_APPLICATION_ID`, `_GTK_UNIQUE_BUS_NAME`, etc.
- Advertise DBus services to GNOME Shell

## Public API

### Main Entry Point

```rust
use gnome_menu::GnomeMenuManager;

// Create manager (returns None if GNOME menus not available)
let manager = GnomeMenuManager::new("MyApp")?;

// Set X11 window properties
manager.set_window_properties(window_id, display)?;

// Update menu structure
manager.update_menu(&menu)?;

// Shutdown (or automatic via Drop)
manager.shutdown();
```

### Detection

```rust
use gnome_menu::should_use_gnome_menus;

if should_use_gnome_menus() {
    // GNOME desktop with DBus available
} else {
    // Use CSD menus
}
```

### Error Handling

```rust
match manager.set_window_properties(window_id, display) {
    Ok(_) => {
        // GNOME menus active
    }
    Err(GnomeMenuError::NotImplemented) => {
        // Feature not yet complete - use CSD fallback
    }
    Err(e) => {
        // Other error - use CSD fallback
        eprintln!("GNOME menu error: {}", e);
    }
}
```

## Environment Variables

### `AZ_DISABLE_GNOME_MENUS`

Force fallback to CSD window-based menus  

- `1` = disabled
- any other value or unset = enabled

### `AZ_GNOME_MENU_DEBUG`

Enable debug logging to stderr  

- `1` = enabled
- any other value or unset = disabled

**Output Examples:**
```
[AZUL GNOME MENU] Creating GNOME menu manager for app: MyApp
[AZUL GNOME MENU] DBus connection established
[AZUL GNOME MENU] Registering org.gtk.Menus interface with DBus
[AZUL GNOME MENU] Setting X11 window properties for GNOME menu
[AZUL GNOME MENU] Menu update complete
```

## GTK DBus Protocol

### X11 Window Properties

GNOME Shell discovers our menus by reading these X11 window properties:

| Property | Type | Example Value |
|----------|------|---------------|
| `_GTK_APPLICATION_ID` | STRING | `"org.example.MyApp"` |
| `_GTK_UNIQUE_BUS_NAME` | STRING | `"org.gtk.MyApp"` |
| `_GTK_APPLICATION_OBJECT_PATH` | STRING | `"/org/gtk/MyApp"` |
| `_GTK_APP_MENU_OBJECT_PATH` | STRING | `"/org/gtk/MyApp/menus/AppMenu"` |
| `_GTK_MENUBAR_OBJECT_PATH` | STRING | `"/org/gtk/MyApp/menus/MenuBar"` |

### org.gtk.Menus Interface

**Object Path:** `/org/gtk/MyApp/menus/MenuBar`

**Methods:**

1. **Start(subscriptions: au) → a(uuaa{sv})**

   - Called by GNOME Shell to subscribe to menu groups
   - Returns menu structure in DBus format
   - Subscriptions: Array of group IDs to watch

2. **End(subscriptions: au)**

   - Called by GNOME Shell to unsubscribe
   - Subscriptions: Array of group IDs to stop watching

**Menu Format:**

```
array of (group_id, menu_id, items)

items = array of dict {
    "label": variant<string>,          # "File"
    "action": variant<string>,         # "app.file.new"
    "target": variant<...>,            # Action parameter
    "submenu": variant<(uint, uint)>,  # (group_id, menu_id)
    "section": variant<(uint, uint)>,  # For separators
}
```

### org.gtk.Actions Interface

**Object Path:** `/org/gtk/MyApp`

**Methods:**

1. **List() → as**
   - Returns array of all action names
   - Example: `["app.file.new", "app.file.open", "app.quit"]`

2. **Describe(action: s) → (bsav)**
   - Returns action details
   - Returns: `(enabled, param_type, state)`
   - Example: `(true, "", [])`

3. **DescribeAll() → a{s(bsav)}**
   - Returns all actions with details
   - Dictionary mapping action names to details

4. **Activate(action: s, parameter: av, platform_data: a{sv})**
   - Invokes action callback
   - Called when user clicks menu item
   - Parameter: Optional action parameter
   - Platform data: Additional context (timestamp, etc.)

## Compilation and Features

```bash
cargo build -p azul-dll --features gnome-menus
```

For cross-compilation targets, **do not enable** `gnome-menus`.

## Usage Example

```rust
use azul_dll::desktop::shell2::linux::gnome_menu::GnomeMenuManager;

// In X11Window::new_with_resources()
if options.state.flags.use_native_menus {
    if let Some(title) = options.state.title.as_ref() {
        match GnomeMenuManager::new(title.as_str()) {
            Some(menu_manager) => {
                match menu_manager.set_window_properties(window.window, display as *mut _) {
                    Ok(_) => {
                        println!("GNOME menus enabled");
                        window.gnome_menu = Some(menu_manager);
                    }
                    Err(e) => {
                        eprintln!("GNOME menu setup failed: {} - using CSD", e);
                        // Continue with CSD menus
                    }
                }
            }
            None => {
                println!("GNOME menus not available - using CSD");
            }
        }
    }
}

// Later, when menu changes
if let Some(ref menu_manager) = window.gnome_menu {
    if let Err(e) = menu_manager.update_menu(&new_menu) {
        eprintln!("Failed to update GNOME menu: {}", e);
        // CSD menu is still functional
    }
}
```

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    GnomeMenuManager                          │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  Detection Logic (should_use_gnome_menus)            │   │
│  │  • Check AZ_DISABLE_GNOME_MENUS                    │   │
│  │  • Check XDG_CURRENT_DESKTOP                         │   │
│  │  • Check DBUS_SESSION_BUS_ADDRESS                    │   │
│  └──────────────────────────────────────────────────────┘   │
│                           ↓                                   │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  DbusConnection                                       │   │
│  │  • Session bus connection                            │   │
│  │  • Service registration (org.gtk.MyApp)              │   │
│  │  • Object path (/org/gtk/MyApp)                      │   │
│  └──────────────────────────────────────────────────────┘   │
│         ↓                          ↓                          │
│  ┌──────────────────┐      ┌────────────────────────┐       │
│  │  MenuProtocol    │      │  ActionsProtocol       │       │
│  │  org.gtk.Menus   │      │  org.gtk.Actions       │       │
│  │  • Start()       │      │  • List()              │       │
│  │  • End()         │      │  • Describe()          │       │
│  │                  │      │  • DescribeAll()       │       │
│  │                  │      │  • Activate()          │       │
│  └──────────────────┘      └────────────────────────┘       │
│         ↑                          ↑                          │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  MenuConversion                                       │   │
│  │  • convert_menu(Menu) → Vec<DbusMenuGroup>          │   │
│  │  • extract_actions(Menu) → Vec<DbusAction>          │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                               │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  X11Properties                                        │   │
│  │  • set_properties() → Set X11 window properties      │   │
│  │  • _GTK_APPLICATION_ID                               │   │
│  │  • _GTK_UNIQUE_BUS_NAME                              │   │
│  │  • _GTK_MENUBAR_OBJECT_PATH                          │   │
│  └──────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                           ↓
         ┌─────────────────────────────────────┐
         │      GNOME Shell (Compositor)        │
         │  • Reads X11 properties              │
         │  • Queries org.gtk.Menus via DBus    │
         │  • Displays menu in top bar          │
         │  • Invokes org.gtk.Actions on click  │
         └─────────────────────────────────────┘
```


## Design Principles

1. **Independence**
   - Module can be completely removed by deleting the `gnome_menu/` directory
   - No dependencies on module from other parts of codebase (except optional integration)
   - All logic self-contained

2. **Safety**
   - Every function returns `Result<T, GnomeMenuError>`
   - All errors trigger graceful fallback
   - No panics, no unwraps in production code
   - Comprehensive error messages

3. **User Control**
   - Environment variables for troubleshooting
   - Debug logging for diagnostics
   - Easy to disable if problematic

4. **Testability**
   - Unit tests for every component
   - Mock-friendly interfaces
   - Integration tests for real-world scenarios

5. **Documentation**
   - Every file has module-level documentation
   - Every public function has doc comments
   - README explains architecture and usage


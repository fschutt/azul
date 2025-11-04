# GNOME Menu Integration V2 - Usage Guide

## Overview

The new GNOME menu implementation uses **dlopen** to load libdbus-1.so dynamically at runtime. This eliminates compile-time dependencies and enables full cross-compilation support.

## Architecture

```
Application Startup
    ↓
Load DBusLib once (shared via Rc)
    ↓
Create GnomeMenuManagerV2 per window
    ↓
Register org.gtk.Menus and org.gtk.Actions interfaces
    ↓
Set X11 window properties
    ↓
Event loop: process_messages() regularly
```

## API Usage

### 1. Initialize DBus Library (Once at Startup)

```rust
use azul_dll::desktop::shell2::linux::gnome_menu::{get_shared_dbus_lib, is_dbus_available};

// Check if DBus is available
if !is_dbus_available() {
    println!("DBus not available, using fallback menus");
    return;
}

// Get shared DBus library instance (loads once, cached)
let dbus_lib = match get_shared_dbus_lib() {
    Some(lib) => lib,
    None => {
        println!("Failed to load DBus library");
        return;
    }
};

// Now dbus_lib can be shared across all windows
```

### 2. Create Menu Manager Per Window

```rust
use azul_dll::desktop::shell2::linux::gnome_menu::{GnomeMenuManagerV2, should_use_gnome_menus};

// Check if GNOME menus should be used
if !should_use_gnome_menus() {
    println!("GNOME menus not available or disabled");
    return;
}

// Create manager (one per window)
let menu_manager = match GnomeMenuManagerV2::new("MyApp", dbus_lib.clone()) {
    Ok(manager) => manager,
    Err(e) => {
        eprintln!("Failed to create GNOME menu manager: {}", e);
        return;
    }
};

// Set X11 window properties (tells GNOME Shell where to find our menus)
menu_manager.set_window_properties(window_id, display_ptr)?;
```

### 3. Update Menu Structure

```rust
use azul_dll::desktop::shell2::linux::gnome_menu::{DbusMenuGroup, DbusMenuItem};

// Build menu structure
let menu_groups = vec![
    DbusMenuGroup {
        group_id: 0,
        menu_id: 0,
        items: vec![
            DbusMenuItem {
                label: "File".to_string(),
                action: Some("app.file".to_string()),
                target: None,
                submenu: Some((1, 0)), // References submenu group
                section: None,
                enabled: true,
            },
        ],
    },
    DbusMenuGroup {
        group_id: 1,
        menu_id: 0,
        items: vec![
            DbusMenuItem {
                label: "New".to_string(),
                action: Some("app.file.new".to_string()),
                target: None,
                submenu: None,
                section: None,
                enabled: true,
            },
            DbusMenuItem {
                label: "Open".to_string(),
                action: Some("app.file.open".to_string()),
                target: None,
                submenu: None,
                section: None,
                enabled: true,
            },
        ],
    },
];

// Update menus
menu_manager.update_menu(menu_groups)?;
```

### 4. Register Actions (Callbacks)

```rust
use std::sync::Arc;
use azul_dll::desktop::shell2::linux::gnome_menu::DbusAction;

let actions = vec![
    DbusAction {
        name: "app.file.new".to_string(),
        enabled: true,
        parameter_type: None,
        state: None,
        callback: Arc::new(|_param| {
            println!("New file action triggered!");
            // Your callback code here
        }),
    },
    DbusAction {
        name: "app.file.open".to_string(),
        enabled: true,
        parameter_type: None,
        state: None,
        callback: Arc::new(|_param| {
            println!("Open file action triggered!");
        }),
    },
];

menu_manager.register_actions(actions)?;
```

### 5. Process Messages in Event Loop

```rust
// In your event loop, call this regularly to handle incoming DBus method calls
menu_manager.process_messages();
```

## Complete Example

```rust
use azul_dll::desktop::shell2::linux::gnome_menu::*;
use std::sync::Arc;

fn setup_gnome_menus(window_id: u64, display: *mut std::ffi::c_void) -> Option<GnomeMenuManagerV2> {
    // 1. Check availability
    if !should_use_gnome_menus() || !is_dbus_available() {
        return None;
    }

    // 2. Get shared DBus library
    let dbus_lib = get_shared_dbus_lib()?;

    // 3. Create manager
    let manager = GnomeMenuManagerV2::new("MyApp", dbus_lib).ok()?;

    // 4. Set window properties
    manager.set_window_properties(window_id, display).ok()?;

    // 5. Build menu structure
    let menu_groups = vec![
        DbusMenuGroup {
            group_id: 0,
            menu_id: 0,
            items: vec![
                DbusMenuItem {
                    label: "File".to_string(),
                    action: None,
                    target: None,
                    submenu: Some((1, 0)),
                    section: None,
                    enabled: true,
                },
            ],
        },
        DbusMenuGroup {
            group_id: 1,
            menu_id: 0,
            items: vec![
                DbusMenuItem {
                    label: "Quit".to_string(),
                    action: Some("app.quit".to_string()),
                    target: None,
                    submenu: None,
                    section: None,
                    enabled: true,
                },
            ],
        },
    ];

    manager.update_menu(menu_groups).ok()?;

    // 6. Register actions
    let actions = vec![
        DbusAction {
            name: "app.quit".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
            callback: Arc::new(|_| {
                println!("Quit requested!");
                std::process::exit(0);
            }),
        },
    ];

    manager.register_actions(actions).ok()?;

    Some(manager)
}

// In event loop:
fn event_loop(menu_manager: &GnomeMenuManagerV2) {
    loop {
        // Process events...
        
        // Process DBus messages
        menu_manager.process_messages();
        
        // Sleep or wait for events...
    }
}
```

## Environment Variables

- **`AZUL_DISABLE_GNOME_MENUS=1`**: Force disable GNOME menus (use fallback)
- **`AZUL_GNOME_MENU_DEBUG=1`**: Enable debug logging to stderr

## Integration with X11Window and WaylandWindow

### X11 Integration

```rust
// In X11Window::new()
pub struct X11Window {
    // ... existing fields ...
    gnome_menu_manager: Option<GnomeMenuManagerV2>,
}

impl X11Window {
    pub fn new(...) -> Result<Self, WindowError> {
        // ... existing code ...

        // Initialize GNOME menus if requested
        let gnome_menu_manager = if options.state.flags.use_native_menus {
            if let Some(dbus_lib) = get_shared_dbus_lib() {
                match GnomeMenuManagerV2::new(title, dbus_lib) {
                    Ok(manager) => {
                        if let Err(e) = manager.set_window_properties(window_id, display) {
                            eprintln!("Failed to set GNOME menu properties: {}", e);
                            None
                        } else {
                            Some(manager)
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to create GNOME menu manager: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            // ... existing fields ...
            gnome_menu_manager,
        })
    }

    // In event loop
    pub fn process_events(&mut self) {
        // ... existing event processing ...

        // Process GNOME menu messages
        if let Some(ref manager) = self.gnome_menu_manager {
            manager.process_messages();
        }
    }
}
```

## Advantages Over Old Implementation

1. **No Compile-Time Dependencies**: libdbus-1.so loaded at runtime
2. **Cross-Compilation**: Build from any platform to Linux
3. **Shared Library Instance**: Load once, use across all windows
4. **Low-Level Control**: Direct C API for maximum flexibility
5. **Graceful Degradation**: Falls back if DBus unavailable
6. **Type-Safe**: Rust wrappers around unsafe C code

## Migration from Old API

### Before (with dbus crate)

```rust
use dbus::blocking::Connection;

let conn = Connection::new_session()?;
// ... complex tree building ...
```

### After (with dlopen)

```rust
let dbus_lib = get_shared_dbus_lib()?;
let manager = GnomeMenuManagerV2::new("MyApp", dbus_lib)?;
// Simple high-level API
```

## Performance Considerations

- DBus library loaded **once** at startup (shared via Rc)
- Message processing is **non-blocking** (timeout = 0)
- Menu updates are **in-memory** (no network overhead)
- Callbacks invoked directly (no async complexity)

## Debugging

Enable debug logging:

```bash
AZUL_GNOME_MENU_DEBUG=1 ./myapp
```

Output:
```
[AZUL GNOME MENU] Attempting to load libdbus-1.so
[AZUL GNOME MENU] Successfully loaded libdbus-1.so
[AZUL GNOME MENU] Creating GNOME menu manager V2 for app: MyApp
[AZUL GNOME MENU] Bus name: org.gtk.MyApp
[AZUL GNOME MENU] Object path: /org/gtk/MyApp
[AZUL GNOME MENU] DBus service registered successfully
[AZUL GNOME MENU] Registering org.gtk.Menus interface with dlopen DBus
[AZUL GNOME MENU] org.gtk.Menus interface registered successfully
[AZUL GNOME MENU] Registering org.gtk.Actions interface with dlopen DBus
[AZUL GNOME MENU] org.gtk.Actions interface registered successfully
```

## Testing

Monitor DBus messages:

```bash
dbus-monitor "interface='org.gtk.Menus'"
dbus-monitor "interface='org.gtk.Actions'"
```

Check X11 properties:

```bash
xprop -id <window_id> | grep GTK
```

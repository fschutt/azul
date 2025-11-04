# Native Menu Callback Architecture

## Current Implementation Status Matrix

| Platform | Native Menu Bar | Native Context Menu | Fallback Menu Bar | Fallback Context Menu |
|----------|----------------|--------------------|--------------------|----------------------|
| **Windows** | ✅ **COMPLETE** | ✅ **COMPLETE** | ✅ **COMPLETE** | ✅ **COMPLETE** |
| **macOS** | ✅ **COMPLETE** | ✅ **COMPLETE** | ❌ **MISSING** | ❌ **MISSING** |
| **Linux** | 🔄 **PARTIAL** | ❌ **N/A** | ✅ **COMPLETE** | ✅ **COMPLETE** |

### Details by Platform

#### Windows
- **Native Path**: Fully functional
  - Menu bar: WM_COMMAND with HMENU
  - Context menus: TrackPopupMenu API
  - Callbacks: HashMap<u16, CoreMenuCallback>
- **Fallback Path**: Fully functional
  - Uses menu_renderer.rs
  - DOM-based Azul windows
  - Same callback system

#### macOS  
- **Native Path**: Fully functional
  - Menu bar: NSMenu with AzulMenuTarget delegate
  - Context menus: NSMenu::popUpContextMenu with AzulMenuTarget
  - Callbacks: MenuState with tag → callback mapping
  - Queue system: take_pending_menu_actions()
- **Fallback Path**: **NOT IMPLEMENTED**
  - Needs menu_renderer.rs integration
  - Should create Azul window for menus
  - No code exists yet

#### Linux
- **Native Path**: Partially implemented
  - GNOME Shell: Code exists in gnome_menu/ but not integrated
  - DBusMenu protocol implementation available
  - Not connected to window creation yet
  - No runtime detection
- **Fallback Path**: Fully functional (Primary path)
  - Uses menu_renderer.rs (primary implementation)
  - DOM-based Azul windows
  - Works on all Linux environments

### Gap Analysis

**Critical Gap**: macOS has no fallback menu implementation
- Cannot disable native menus on macOS currently
- Limits testing and customization options
- Inconsistent with cross-platform design

**Integration Gap**: GNOME Shell menus not integrated
- Code exists but not used by X11/Wayland backends
- No runtime detection for GNOME Shell availability
- Feature flag may not work correctly

**Recommendation**: Implement macOS fallback path first (smaller scope), then integrate GNOME menus (larger scope).

This document describes the **dual-path menu architecture** for handling menu callbacks across all platforms (Windows, macOS, Linux) within the Azul GUI framework. Each platform supports both **native menus** (using OS-provided menu APIs) and **fallback menus** (using Azul's DOM-based window menus), with runtime selection via `WindowFlags::use_native_menus`.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    WindowFlags                                  │
│  - use_native_menus: bool                                       │
│  - use_native_context_menus: bool                               │
└──────────────────┬──────────────────────────────────────────────┘
                   │
        ┌──────────┴──────────┐
        │                     │
        ▼                     ▼
┌───────────────┐     ┌───────────────┐
│ Native Menus  │     │Fallback Menus │
│ (OS-Provided) │     │ (DOM-based)   │
└───────────────┘     └───────────────┘
```

### Native Menus (use_native_menus = true)
- **Windows**: Win32 HMENU with WM_COMMAND messages
- **macOS**: NSMenu/NSMenuItem with Objective-C delegate pattern
- **Linux**: GNOME Shell DBus API (behind `gnome_menu` feature flag)

### Fallback Menus (use_native_menus = false OR unavailable)
- **All platforms**: Azul windows with DOM nodes as menu items
- Rendered via `menu_renderer.rs` → StyledDom
- Callbacks attached as On::MouseUp event handlers
- Works identically across Windows, macOS, Linux

## Current Status (November 2025)

### Working Implementations

#### 1. Windows - COMPLETE ✅
**Native Menus (WM_COMMAND)**
- **File:** `dll/src/desktop/shell2/windows/mod.rs`
- **File:** `dll/src/desktop/shell2/windows/menu.rs`
- **Mechanism:** Win32 HMENU with WM_COMMAND message handling
- **Storage:** `Win32Window.menu_bar.callbacks: HashMap<u16, CoreMenuCallback>`
- **Selection:** Always uses native menus when `use_native_menus = true`
- **Flow:**
  1. User clicks menu item → Windows sends `WM_COMMAND`
  2. WndProc extracts command_id from wparam
  3. Looks up callback in HashMap
  4. Invokes via `LayoutWindow::invoke_single_callback()`

**Fallback Menus (DOM-based)**
- Uses common `menu_renderer.rs` implementation
- Creates Azul window with clickable DOM nodes
- Callbacks fire via On::MouseUp event handlers

#### 2. macOS - NATIVE COMPLETE ✅, FALLBACK MISSING ⚠️
**Native Menus (NSMenu)**
- **File:** `dll/src/desktop/shell2/macos/menu.rs` - ✅ COMPLETE
- **Mechanism:** NSMenu/NSMenuItem with AzulMenuTarget delegate
- **Storage:** `MacOSWindow.menu_state: MenuState` with tag→callback mapping
- **Selection:** Uses native menus when `use_native_menus = true`
- **Flow:**
  1. User clicks menu item → NSMenuItem action fires
  2. AzulMenuTarget delegate pushes tag to global queue
  3. Event loop polls `take_pending_menu_actions()`
  4. Calls `handle_menu_action(tag)`
  5. Looks up callback via `menu_state.get_callback_for_tag()`
  6. Invokes via `LayoutWindow::invoke_single_callback()`

**Fallback Menus (DOM-based)** ⚠️ **MISSING**
- Should use common `menu_renderer.rs` implementation
- Currently NO implementation exists for fallback path
- **TODO**: Add fallback menu support to macOS backend

#### 3. Linux - FALLBACK COMPLETE ✅, NATIVE PARTIAL 🔄
**Native Menus (GNOME Shell DBus)**
- **Directory:** `dll/src/desktop/shell2/linux/gnome_menu/`
- **Status:** Implementation exists but NOT integrated
- **Feature Flag:** `gnome_menu` (disabled by default)
- **TODO**: Complete integration and testing

**Fallback Menus (DOM-based)** ✅ **PRIMARY IMPLEMENTATION**
- **Files:**
  - `dll/src/desktop/menu_renderer.rs` - Core rendering logic
  - `dll/src/desktop/menu.rs` - Menu window creation
  - `dll/src/desktop/shell2/linux/x11/menu.rs` - X11 integration
  - `dll/src/desktop/shell2/linux/wayland/menu.rs` - Wayland integration
- **Mechanism:** Menus as Azul windows with StyledDom
- **Selection:** Default for Linux (use_native_menus defaults to false)
- **Flow:**
  1. User clicks menu item DOM node
  2. On::MouseUp callback fires (`menu_item_click_callback`)
  3. Extracts menu item's CoreCallback from RefAny
  4. Invokes via `Callback::from_core()`
  5. Closes menu window

### Incomplete/Missing Implementations

## Architectural Analysis

### Shared Patterns Across All Platforms

All working implementations follow this pattern:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Platform-Specific Event Capture                         │
│    - Windows: WM_COMMAND message                           │
│    - macOS: NSMenuItem action → delegate                   │
│    - Linux CSD: DOM node On::MouseUp event                 │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Callback Lookup                                          │
│    - Windows: command_id → HashMap<u16, CoreMenuCallback>  │
│    - macOS: tag → menu_state.get_callback_for_tag()        │
│    - Linux CSD: Embedded in DOM node's RefAny data         │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│ 3. CoreMenuCallback → layout::MenuCallback Conversion      │
│    let layout_callback = Callback::from_core(callback);    │
│    let menu_callback = MenuCallback {                      │
│        callback: layout_callback,                          │
│        data: callback.data                                 │
│    };                                                       │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│ 4. Invocation via LayoutWindow                             │
│    layout_window.invoke_single_callback(                   │
│        &mut menu_callback.callback,                        │
│        &mut menu_callback.data,                            │
│        &raw_window_handle,                                 │
│        &gl_context_ptr,                                    │
│        &mut image_cache,                                   │
│        &mut fc_cache,                                      │
│        system_style,                                       │
│        &ExternalSystemCallbacks::rust_internal(),          │
│        &previous_window_state,                             │
│        &current_window_state,                              │
│        &renderer_resources                                 │
│    )                                                        │
└────────────────┬────────────────────────────────────────────┘
                 │
                 ▼
┌─────────────────────────────────────────────────────────────┐
│ 5. Result Processing via event_v2                          │
│    - Windows: Direct processing in WndProc                 │
│    - macOS: PlatformWindowV2::process_callback_result_v2() │
│    - Linux: Handled by menu_renderer callback              │
└─────────────────────────────────────────────────────────────┘
```

### Key Insight: Delegation Pattern

The successful implementations use a **delegation pattern** where:
1. Platform-specific code captures the menu click event
2. A **tag/id** identifies which menu item was clicked
3. A **lookup table** maps tag/id to the actual Azul callback
4. Invocation is delegated to the unified `LayoutWindow::invoke_single_callback()` method

## Problem Statement: macOS Native Menu Creation

The issue in `macos/events.rs` is that while the NSMenu hierarchy is created correctly, the individual NSMenuItem objects don't have their target/action set up. The comment identifies the solution:

```rust
// Implementation plan:
// 1. Create an NSObject-based delegate class
// 2. Store callback info (callback_ptr, data_ptr) in the delegate
// 3. Set menu_item.setTarget(&delegate)
// 4. Set menu_item.setAction(sel!(menuItemClicked:))
// 5. In menuItemClicked:, extract callback and invoke it
```

However, this creates a problem: **NSMenuItem requires an Objective-C delegate object**, but we're in Rust code.

### Current Workaround in macOS

Looking at the existing macOS implementation more carefully:

```rust
// File: dll/src/desktop/shell2/macos/events.rs
// Line ~1070-1130: create_native_menu_recursive()
```

The function creates NSMenu hierarchies but the actual **callback handling is done elsewhere** through the MacOSWindow's menu_state system, which suggests that:

1. Menu items ARE wired up somewhere (otherwise the existing menu callbacks wouldn't work)
2. The TODO comment is misleading or outdated

Let me search for where menu items actually get their actions set...

## Investigation: How macOS Menu Items Get Actions

### Finding: AppDelegate Pattern

Looking at the macOS implementation, I need to find where `handle_menu_action` gets called from:

**File:** `dll/src/desktop/shell2/macos/mod.rs` line 3096:
```rust
self.handle_menu_action(tag);
```

This is called from somewhere in the event loop. The key is that macOS uses an **AppDelegate** pattern that's set up when the application starts.

### Solution Architecture

The proper solution is already partially implemented but not documented. Here's how it should work:

## Proposed Unified Architecture for Native Menus

### Core Principle: Two-Stage Callback Registration

1. **Stage 1: Menu Creation** (Platform-Specific)
   - Windows: Create HMENU hierarchy with unique command IDs
   - macOS: Create NSMenu hierarchy with unique tags
   - Linux: Create DOM-based menu windows OR use GNOME Shell API

2. **Stage 2: Callback Wiring** (Unified)
   - Store mapping: `menu_id → CoreMenuCallback`
   - Set up platform event handler to lookup and invoke callbacks
   - All invocations go through `LayoutWindow::invoke_single_callback()`

### Integration with event_v2 and PlatformWindowV2

The event_v2 system should provide a unified interface:

```rust
pub trait PlatformWindowV2 {
    // Existing methods...
    
    /// Handle a menu action triggered by the platform
    /// 
    /// This is called when a native menu item is clicked.
    /// The platform-specific code translates the platform event
    /// (WM_COMMAND, NSMenuItem action, etc.) into a menu_id,
    /// then calls this method.
    fn handle_menu_action(&mut self, menu_id: u64) -> ProcessEventResult;
    
    /// Register a menu callback
    /// 
    /// This stores the callback in the platform's lookup table
    /// and returns a unique menu_id that should be used when
    /// creating the native menu item.
    fn register_menu_callback(&mut self, callback: CoreMenuCallback) -> u64;
}
```

## Implementation Plan

### Phase 1: Documentation and Analysis ✅ (COMPLETE)
- [x] Analyze Windows native + fallback implementations
- [x] Analyze macOS native implementation
- [x] Analyze Linux fallback implementation
- [x] Identify gaps (macOS fallback, Linux native integration)
- [x] Document dual-path architecture with WindowFlags

### Phase 2: Fix Context Menu Callbacks in macOS ✅ (COMPLETE)
- [x] Update `recursive_build_nsmenu` in `macos/events.rs`
- [x] Wire up callbacks using AzulMenuTarget (same as menu bar)
- [x] Remove misleading TODO comments
- [x] Test context menus work correctly

### Phase 3: Add Fallback Menu Support to macOS
- [ ] Add menu window creation to macOS backend
- [ ] Use `menu_renderer.rs` for DOM-based menus
- [ ] Respect `use_native_menus` flag
- [ ] Fall back to DOM menus when `use_native_menus = false`
- [ ] Test menu bar and context menus with fallback path

### Phase 4: Complete GNOME Shell Native Menu Integration
- [ ] Review `gnome_menu/` implementation status
- [ ] Add runtime detection for GNOME Shell availability
- [ ] Integrate with X11 backend (check `use_native_menus` flag)
- [ ] Integrate with Wayland backend (check `use_native_menus` flag)
- [ ] Fall back to DOM menus if GNOME Shell unavailable
- [ ] Test on various Linux distributions
- [ ] Update feature flag documentation

### Phase 5: Standardize Menu Callback Registry (Optional)
- [ ] Create unified `MenuCallbackRegistry` struct
- [ ] Move Windows HashMap to registry
- [ ] Move macOS MenuState to registry
- [ ] Update event_v2 trait for menu handling
- [ ] Ensure Linux fallback uses same pattern

## Design Decision: use_native_menus Flag

The `WindowFlags::use_native_menus` field controls which menu implementation is used:

```rust
pub struct WindowFlags {
    // ... other fields ...
    
    /// Use native menus (Win32 HMENU, macOS NSMenu) instead of Azul window-based menus
    /// Default: true on Windows/macOS, false on Linux
    pub use_native_menus: bool,
    
    /// Use native context menus instead of Azul window-based context menus
    /// Default: true on Windows/macOS, false on Linux
    pub use_native_context_menus: bool,
}
```

### Default Values by Platform

| Platform | `use_native_menus` | `use_native_context_menus` | Reason |
|----------|-------------------|---------------------------|---------|
| Windows  | `true`            | `true`                    | Native menus are well-integrated |
| macOS    | `true`            | `true`                    | NSMenu is the standard |
| Linux    | `false`           | `false`                   | No universal native menu API |

### Selection Logic

```rust
fn should_use_native_menus(window_flags: &WindowFlags) -> bool {
    #[cfg(target_os = "windows")]
    return window_flags.use_native_menus; // Always available
    
    #[cfg(target_os = "macos")]
    return window_flags.use_native_menus; // Always available
    
    #[cfg(target_os = "linux")]
    {
        // Only use native if flag is true AND GNOME Shell is available
        window_flags.use_native_menus && is_gnome_shell_available()
    }
}
```

### Context Menus (Right-Click Menus)

Context menus follow the same pattern but are controlled by `use_native_context_menus`:

- **Windows**: Native context menus via TrackPopupMenu API
- **macOS**: Native context menus via NSMenu::popUpContextMenu
- **Linux**: Fallback only (no native context menu API)

## Code Examples

### Example 1: Windows (Current Working Code)

```rust
// File: dll/src/desktop/shell2/windows/mod.rs
// Lines ~2048-2100

WM_COMMAND => {
    let command_id = (wparam & 0xFFFF) as u16;
    
    // Lookup callback
    let callback_opt = if let Some(menu_bar) = &window.menu_bar {
        menu_bar.callbacks.get(&command_id).cloned()
    } else {
        None
    };
    
    if let Some(callback) = callback_opt {
        // Convert to layout callback
        let layout_callback = Callback::from_core(callback.callback);
        let mut menu_callback = MenuCallback {
            callback: layout_callback,
            data: callback.data,
        };
        
        // Invoke via LayoutWindow
        let callback_result = layout_window.invoke_single_callback(
            &mut menu_callback.callback,
            &mut menu_callback.data,
            // ... other parameters ...
        );
        
        // Process result
        // ... handle callback_result ...
    }
}
```

### Example 2: macOS (Current Working Code)

```rust
// File: dll/src/desktop/shell2/macos/mod.rs
// Lines ~2523-2600

fn handle_menu_action(&mut self, tag: isize) {
    // Lookup callback
    let callback = match self.menu_state.get_callback_for_tag(tag as i64) {
        Some(cb) => cb.clone(),
        None => return,
    };
    
    // Convert to layout callback
    let layout_callback = Callback::from_core(callback.callback);
    let mut menu_callback = MenuCallback {
        callback: layout_callback,
        data: callback.data,
    };
    
    // Invoke via LayoutWindow
    let callback_result = layout_window.invoke_single_callback(
        &mut menu_callback.callback,
        &mut menu_callback.data,
        // ... other parameters ...
    );
    
    // Process result via event_v2
    let event_result = self.process_callback_result_v2(&callback_result);
    // ... handle event_result ...
}
```

### Example 3: Linux CSD (Current Working Code)

```rust
// File: dll/src/desktop/menu_renderer.rs
// Lines ~48-90

extern "C" fn menu_item_click_callback(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let callback_data = data.downcast_ref::<MenuItemCallbackData>()?;
    
    // Invoke the menu item's callback if present
    if let Some(ref menu_callback) = callback_data.menu_item.callback.as_option() {
        // Convert CoreCallback
        let callback = Callback::from_core(menu_callback.callback);
        
        // Invoke with the menu item's data
        let mut callback_data_refany = menu_callback.data.clone();
        let result = callback.invoke(&mut callback_data_refany, info);
        
        // Close the menu window
        let mut flags = info.get_current_window_flags();
        flags.close_requested = true;
        info.set_window_flags(flags);
        
        return result;
    }
    
    Update::DoNothing
}
```

## Conclusion

The architecture for menu callbacks is **already well-designed and consistently implemented** across Windows, macOS, and Linux (CSD). The main issues are:

1. **Documentation Gap:** The macOS TODO comment suggests incomplete implementation, but the code may actually be complete
2. **Integration Gap:** GNOME Shell native menus exist but aren't integrated into the X11/Wayland backends
3. **Standardization Opportunity:** Could unify the callback registry pattern across platforms

The next step is to **verify the actual state of macOS menu callbacks** by tracing the code path and testing whether native menus actually work.

## References

- `dll/src/desktop/shell2/windows/mod.rs` - Windows implementation
- `dll/src/desktop/shell2/macos/mod.rs` - macOS implementation
- `dll/src/desktop/shell2/macos/events.rs` - macOS event handling and menu creation
- `dll/src/desktop/menu_renderer.rs` - Cross-platform menu rendering
- `dll/src/desktop/menu.rs` - Menu window creation
- `dll/src/desktop/shell2/linux/gnome_menu/` - GNOME Shell native menus
- `dll/src/desktop/shell2/common/event_v2.rs` - V2 event system trait

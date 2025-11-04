# Tooltip and Window Flags Implementation Guide

## Overview

This document describes the implementation requirements for two new features:
1. **Tooltip API** - Platform-specific tooltip display
2. **Window Flags** - `is_top_level` and `prevent_system_sleep` flags

Both features have been added to the core API but require platform-specific implementation in the DLL layer.

---

## 1. Tooltip API

### Core API (✅ Implemented)

**Location:** `layout/src/callbacks.rs`

```rust
pub enum CallbackChange {
    // ... existing variants ...
    
    /// Show a tooltip at a specific position
    ShowTooltip {
        text: AzString,
        position: LogicalPosition,
    },
    /// Hide the currently displayed tooltip
    HideTooltip,
}
```

**CallbackInfo Methods:**
```rust
impl CallbackInfo {
    /// Show tooltip at cursor position
    pub fn show_tooltip(&mut self, text: AzString);
    
    /// Show tooltip at specific position
    pub fn show_tooltip_at(&mut self, text: AzString, position: LogicalPosition);
    
    /// Hide current tooltip
    pub fn hide_tooltip(&mut self);
}
```

**CallCallbacksResult:**
```rust
pub struct CallCallbacksResult {
    // ... existing fields ...
    
    /// Tooltips to show (text, position)
    pub tooltips_to_show: Vec<(AzString, LogicalPosition)>,
    
    /// Whether to hide the current tooltip
    pub hide_tooltip: bool,
}
```

### Platform Implementation Requirements

#### Windows (Win32)

**Native API:** `TOOLTIPS_CLASS` (Common Controls)

```c
// Required Windows API calls:
HWND CreateWindowEx(
    WS_EX_TOPMOST,           // Extended style
    TOOLTIPS_CLASS,          // Class name
    NULL,                    // Window text
    WS_POPUP | TTS_NOPREFIX | TTS_ALWAYSTIP,
    CW_USEDEFAULT,           // X position
    CW_USEDEFAULT,           // Y position
    CW_USEDEFAULT,           // Width
    CW_USEDEFAULT,           // Height
    hwndParent,              // Parent window
    NULL,                    // Menu
    hInstance,               // Instance
    NULL                     // lParam
);

// Add tool to tooltip control:
TOOLINFO ti = { 0 };
ti.cbSize = sizeof(TOOLINFO);
ti.uFlags = TTF_TRACK | TTF_ABSOLUTE;
ti.hwnd = hwndParent;
ti.lpszText = tooltipText;
SendMessage(hwndTooltip, TTM_ADDTOOL, 0, (LPARAM)&ti);

// Position and show:
SendMessage(hwndTooltip, TTM_TRACKPOSITION, 0, MAKELPARAM(x, y));
SendMessage(hwndTooltip, TTM_TRACKACTIVATE, TRUE, (LPARAM)&ti);

// Hide:
SendMessage(hwndTooltip, TTM_TRACKACTIVATE, FALSE, (LPARAM)&ti);
```

**Implementation Location:** `dll/src/platform/windows/tooltip.rs` (to be created)

**Key Points:**
- Create a global tooltip window per application
- Use `TTM_TRACKPOSITION` for positioning
- Use `TTM_TRACKACTIVATE` to show/hide
- Tooltip should auto-dismiss on mouse movement or after timeout

#### macOS (Cocoa)

**Native API:** `NSPopover` or custom `NSWindow` with tooltip styling

```objc
// Option 1: NSPopover (modern, recommended)
NSPopover *popover = [[NSPopover alloc] init];
popover.contentViewController = tooltipViewController;
popover.behavior = NSPopoverBehaviorSemitransient;
[popover showRelativeToRect:rect 
                      ofView:view 
               preferredEdge:NSRectEdgeMinY];

// Option 2: Custom NSWindow (legacy)
NSWindow *tooltipWindow = [[NSWindow alloc] 
    initWithContentRect:frame
              styleMask:NSWindowStyleMaskBorderless
                backing:NSBackingStoreBuffered
                  defer:NO];
tooltipWindow.backgroundColor = [NSColor colorWithWhite:1.0 alpha:0.9];
tooltipWindow.level = NSPopUpMenuWindowLevel;
tooltipWindow.opaque = NO;
tooltipWindow.hasShadow = YES;
[tooltipWindow orderFront:nil];
```

**Implementation Location:** `dll/src/platform/macos/tooltip.rs` (to be created)

**Key Points:**
- Use `NSPopover` for modern macOS (10.7+)
- Set `behavior` to `NSPopoverBehaviorSemitransient` for auto-dismiss
- Position relative to parent window
- Use system tooltip styling for consistency

#### Linux (X11)

**Native API:** Transient window with `_NET_WM_WINDOW_TYPE_TOOLTIP`

```c
// Create window:
Window tooltip_window = XCreateSimpleWindow(
    display, parent, x, y, width, height,
    0, 0, 0xFFFFE1  // Light yellow background
);

// Set window type:
Atom tooltip_type = XInternAtom(display, "_NET_WM_WINDOW_TYPE_TOOLTIP", False);
Atom window_type = XInternAtom(display, "_NET_WM_WINDOW_TYPE", False);
XChangeProperty(display, tooltip_window, window_type, XA_ATOM, 32,
                PropModeReplace, (unsigned char *)&tooltip_type, 1);

// Set transient for parent:
XSetTransientForHint(display, tooltip_window, parent);

// Map window:
XMapWindow(display, tooltip_window);

// Render text using Xft or cairo
```

**Implementation Location:** `dll/src/platform/x11/tooltip.rs` (to be created)

**Key Points:**
- Use `_NET_WM_WINDOW_TYPE_TOOLTIP` for proper WM integration
- Set as transient for parent window
- Use system-style colors (light yellow background)
- Implement timeout for auto-dismiss
- Handle `FocusOut` and `LeaveWindow` events

#### Linux (Wayland)

**Native API:** `zwlr_layer_shell_v1` with overlay layer

```c
// Get layer shell:
struct zwlr_layer_shell_v1 *layer_shell = // ... from registry

// Create surface:
struct wl_surface *surface = wl_compositor_create_surface(compositor);
struct zwlr_layer_surface_v1 *layer_surface = 
    zwlr_layer_shell_v1_get_layer_surface(
        layer_shell, surface, output,
        ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY,
        "tooltip"
    );

// Configure:
zwlr_layer_surface_v1_set_size(layer_surface, width, height);
zwlr_layer_surface_v1_set_anchor(layer_surface, 0);  // No anchoring
zwlr_layer_surface_v1_set_exclusive_zone(layer_surface, -1);
zwlr_layer_surface_v1_set_keyboard_interactivity(layer_surface, 0);

// Commit:
wl_surface_commit(surface);
```

**Implementation Location:** `dll/src/platform/wayland/tooltip.rs` (to be created)

**Key Points:**
- Use `ZWLR_LAYER_SHELL_V1_LAYER_OVERLAY` for top-level rendering
- Set no keyboard interactivity
- Position using `set_margin` or manual positioning
- Fallback: Create regular surface if layer shell unavailable
- Implement timeout timer for auto-dismiss

---

## 2. Window Flags: `is_top_level` and `prevent_system_sleep`

### Core API (✅ Implemented)

**Location:** `core/src/window.rs`

```rust
pub struct WindowFlags {
    // ... existing fields ...
    
    /// Keep window above all others (even from other applications)
    /// Platform-specific: Uses SetWindowPos(HWND_TOPMOST) on Windows,
    /// [NSWindow setLevel:] on macOS, _NET_WM_STATE_ABOVE on X11,
    /// zwlr_layer_shell on Wayland
    pub is_top_level: bool,
    
    /// Prevent system from sleeping while window is open
    /// Platform-specific: Uses SetThreadExecutionState on Windows,
    /// IOPMAssertionCreateWithName on macOS,
    /// org.freedesktop.ScreenSaver.Inhibit on Linux
    pub prevent_system_sleep: bool,
}
```

### Platform Implementation Requirements

#### Windows (Win32)

##### is_top_level

```c
// Set window as topmost:
SetWindowPos(
    hwnd,
    HWND_TOPMOST,      // Insert after
    0, 0, 0, 0,        // Position/size (ignored with SWP_NOMOVE | SWP_NOSIZE)
    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
);

// Remove topmost:
SetWindowPos(
    hwnd,
    HWND_NOTOPMOST,
    0, 0, 0, 0,
    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE
);
```

##### prevent_system_sleep

```c
// Prevent sleep:
SetThreadExecutionState(
    ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED
);

// Allow sleep:
SetThreadExecutionState(ES_CONTINUOUS);
```

**Implementation Location:** `dll/src/platform/windows/window.rs`

**Key Points:**
- Apply `SetWindowPos` when flag changes in window state
- Call `SetThreadExecutionState` on window creation and flag changes
- Restore execution state on window destroy

#### macOS (Cocoa)

##### is_top_level

```objc
// Set window level:
if (is_top_level) {
    [window setLevel:NSPopUpMenuWindowLevel];  // Above most windows
    // Or use NSFloatingWindowLevel for always-on-top
} else {
    [window setLevel:NSNormalWindowLevel];
}

// Alternative for true top-level:
[window setLevel:NSScreenSaverWindowLevel];  // Above everything
```

##### prevent_system_sleep

```objc
#import <IOKit/pwr_mgt/IOPMLib.h>

// Prevent sleep:
IOPMAssertionID assertionID;
CFStringRef reasonForActivity = CFSTR("Application active");
IOReturn success = IOPMAssertionCreateWithName(
    kIOPMAssertionTypeNoDisplaySleep,
    kIOPMAssertionLevelOn,
    reasonForActivity,
    &assertionID
);

// Allow sleep:
IOPMAssertionRelease(assertionID);
```

**Implementation Location:** `dll/src/platform/macos/window.rs`

**Key Points:**
- Use `NSPopUpMenuWindowLevel` (25) or `NSFloatingWindowLevel` (3)
- For extreme cases: `NSScreenSaverWindowLevel` (1000)
- Store `IOPMAssertionID` in window state
- Release assertion on window destroy
- Link against IOKit framework: `-framework IOKit`

#### Linux (X11)

##### is_top_level

```c
// Set _NET_WM_STATE_ABOVE:
Atom net_wm_state = XInternAtom(display, "_NET_WM_STATE", False);
Atom net_wm_state_above = XInternAtom(display, "_NET_WM_STATE_ABOVE", False);

XChangeProperty(
    display, window,
    net_wm_state, XA_ATOM, 32,
    PropModeAppend,
    (unsigned char *)&net_wm_state_above, 1
);

// Or send client message to WM:
XClientMessageEvent event = {0};
event.type = ClientMessage;
event.window = window;
event.message_type = net_wm_state;
event.format = 32;
event.data.l[0] = 1;  // _NET_WM_STATE_ADD
event.data.l[1] = net_wm_state_above;
XSendEvent(display, root, False,
           SubstructureNotifyMask | SubstructureRedirectMask,
           (XEvent *)&event);
```

##### prevent_system_sleep

```c
// Use D-Bus to inhibit screensaver:
// org.freedesktop.ScreenSaver.Inhibit(application_name, reason)

#include <dbus/dbus.h>

DBusConnection *connection = dbus_bus_get(DBUS_BUS_SESSION, &error);
DBusMessage *message = dbus_message_new_method_call(
    "org.freedesktop.ScreenSaver",
    "/org/freedesktop/ScreenSaver",
    "org.freedesktop.ScreenSaver",
    "Inhibit"
);

const char *app_name = "Azul Application";
const char *reason = "Playback active";
dbus_message_append_args(
    message,
    DBUS_TYPE_STRING, &app_name,
    DBUS_TYPE_STRING, &reason,
    DBUS_TYPE_INVALID
);

// Send and get cookie:
DBusMessage *reply = dbus_connection_send_with_reply_and_block(
    connection, message, -1, &error
);
dbus_uint32_t cookie;
dbus_message_get_args(reply, &error,
                      DBUS_TYPE_UINT32, &cookie,
                      DBUS_TYPE_INVALID);

// Later: UnInhibit(cookie)
```

**Implementation Location:** `dll/src/platform/x11/window.rs`

**Key Points:**
- Use `_NET_WM_STATE_ABOVE` property for window manager
- Send client message for dynamic changes
- Link D-Bus library: `-ldbus-1`
- Store inhibit cookie for later removal
- Handle D-Bus connection errors gracefully
- Fallback: XResetScreenSaver() periodically (not recommended)

#### Linux (Wayland)

##### is_top_level

```c
// Use zwlr_layer_shell_v1:
struct zwlr_layer_surface_v1 *layer_surface = 
    zwlr_layer_shell_v1_get_layer_surface(
        layer_shell, surface, output,
        ZWLR_LAYER_SHELL_V1_LAYER_TOP,  // or LAYER_OVERLAY
        "azul_window"
    );

// For regular windows, use xdg_toplevel:
// Note: True "always on top" is limited in Wayland
// Compositor has final say on window stacking
```

##### prevent_system_sleep

```c
// Use org.freedesktop.portal.Inhibit (preferred):
#include <gio/gio.h>

GDBusProxy *proxy = g_dbus_proxy_new_for_bus_sync(
    G_BUS_TYPE_SESSION,
    G_DBUS_PROXY_FLAGS_NONE,
    NULL,
    "org.freedesktop.portal.Desktop",
    "/org/freedesktop/portal/desktop",
    "org.freedesktop.portal.Inhibit",
    NULL, NULL
);

GVariantBuilder options;
g_variant_builder_init(&options, G_VARIANT_TYPE_VARDICT);
g_variant_builder_add(&options, "{sv}", "reason", g_variant_new_string("Playback active"));

GVariant *result = g_dbus_proxy_call_sync(
    proxy, "Inhibit",
    g_variant_new("(su@a{sv})",
                  "",  // Window handle
                  4,   // Inhibit flags: idle=1, suspend=2, idle|suspend=4
                  g_variant_builder_end(&options)),
    G_DBUS_CALL_FLAGS_NONE,
    -1, NULL, NULL
);

// Get handle for later removal
```

**Implementation Location:** `dll/src/platform/wayland/window.rs`

**Key Points:**
- Use `ZWLR_LAYER_SHELL_V1_LAYER_TOP` or `_OVERLAY` for top-level
- Wayland compositor controls final stacking order
- Use XDG Desktop Portal for sleep inhibition (cross-compositor)
- Fallback to D-Bus org.freedesktop.ScreenSaver if portal unavailable
- Link GIO library: `pkg-config --libs gio-2.0`

---

## 3. Integration Points in DLL

### Required Changes

#### dll/src/app.rs

```rust
pub struct App {
    // ... existing fields ...
    
    // Add tooltip state tracking
    tooltip_state: Option<TooltipState>,
}

struct TooltipState {
    text: String,
    position: LogicalPosition,
    #[cfg(target_os = "windows")]
    hwnd_tooltip: HWND,
    #[cfg(target_os = "macos")]
    popover: id,  // NSPopover*
    #[cfg(all(unix, not(target_os = "macos")))]
    tooltip_window: Option<Window>,  // X11/Wayland
}
```

#### Window State Synchronization

When `CallCallbacksResult` is processed:

```rust
// In event loop after callbacks:
if !callback_result.tooltips_to_show.is_empty() {
    for (text, position) in callback_result.tooltips_to_show {
        platform::show_tooltip(&text, position);
    }
}

if callback_result.hide_tooltip {
    platform::hide_tooltip();
}

// Window flags changes:
if let Some(new_state) = callback_result.modified_window_state {
    if new_state.flags.is_top_level != old_flags.is_top_level {
        platform::set_window_top_level(window, new_state.flags.is_top_level);
    }
    
    if new_state.flags.prevent_system_sleep != old_flags.prevent_system_sleep {
        platform::set_prevent_sleep(new_state.flags.prevent_system_sleep);
    }
}
```

---

## 4. Testing

### Tooltip Testing

```rust
// Example test app:
On::MouseOver -> |info| {
    info.show_tooltip("This is a tooltip!".into());
    Update::DoNothing
}

On::MouseOut -> |info| {
    info.hide_tooltip();
    Update::DoNothing
}
```

### Window Flags Testing

```rust
// Test is_top_level:
let mut window_state = WindowState::default();
window_state.flags.is_top_level = true;
info.modify_window_state(window_state);

// Test prevent_system_sleep:
let mut window_state = WindowState::default();
window_state.flags.prevent_system_sleep = true;
info.modify_window_state(window_state);
```

---

## 5. Summary

### Completed (Core API)
- ✅ `CallbackChange::ShowTooltip` / `HideTooltip` enum variants
- ✅ `CallbackInfo::show_tooltip()` / `show_tooltip_at()` / `hide_tooltip()` methods
- ✅ `CallCallbacksResult::tooltips_to_show` / `hide_tooltip` fields
- ✅ `WindowFlags::is_top_level` / `prevent_system_sleep` fields
- ✅ CallbackChange processing in `apply_callback_changes()`

### TODO (Platform Layer)
- ❌ Windows tooltip implementation (TOOLTIPS_CLASS)
- ❌ macOS tooltip implementation (NSPopover)
- ❌ X11 tooltip implementation (_NET_WM_WINDOW_TYPE_TOOLTIP)
- ❌ Wayland tooltip implementation (zwlr_layer_shell_v1)
- ❌ Windows `is_top_level` (SetWindowPos HWND_TOPMOST)
- ❌ macOS `is_top_level` ([NSWindow setLevel:])
- ❌ X11 `is_top_level` (_NET_WM_STATE_ABOVE)
- ❌ Wayland `is_top_level` (zwlr_layer_shell)
- ❌ Windows `prevent_system_sleep` (SetThreadExecutionState)
- ❌ macOS `prevent_system_sleep` (IOPMAssertionCreateWithName)
- ❌ X11 `prevent_system_sleep` (org.freedesktop.ScreenSaver.Inhibit)
- ❌ Wayland `prevent_system_sleep` (org.freedesktop.portal.Inhibit)

### Priority Order
1. **High**: Tooltip implementation (user-facing feature)
2. **High**: `is_top_level` flag (common use case)
3. **Medium**: `prevent_system_sleep` flag (media apps)

---

## 6. References

### Windows
- [Tooltip Controls (Microsoft Docs)](https://docs.microsoft.com/en-us/windows/win32/controls/tooltip-controls)
- [SetWindowPos (Microsoft Docs)](https://docs.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos)
- [SetThreadExecutionState (Microsoft Docs)](https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-setthreadexecutionstate)

### macOS
- [NSPopover (Apple Developer)](https://developer.apple.com/documentation/appkit/nspopover)
- [NSWindow Level (Apple Developer)](https://developer.apple.com/documentation/appkit/nswindow/1419511-level)
- [IOKit Power Management (Apple Developer)](https://developer.apple.com/documentation/iokit/power_management)

### Linux
- [Extended Window Manager Hints (freedesktop.org)](https://specifications.freedesktop.org/wm-spec/wm-spec-latest.html)
- [Layer Shell Protocol (wlroots)](https://wayland.app/protocols/wlr-layer-shell-unstable-v1)
- [ScreenSaver Inhibit (freedesktop.org)](https://www.freedesktop.org/wiki/Specifications/idle-inhibit-spec/)
- [XDG Desktop Portal](https://flatpak.github.io/xdg-desktop-portal/)


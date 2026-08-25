### System Style Discovery Analysis Report

#### Executive Summary
The current implementation relies almost exclusively on **CLI wrapping** (`std::process::Command`) to query system state. While this avoids link-time dependencies on heavy UI toolkits (Cocoa, GTK, Windows SDK), it is performantly expensive (spawn costs), fragile (text parsing stdout), and often retrieves "saved configuration" rather than "resolved active styling."

A robust production-grade discovery system should use **native IPC** (Inter-Process Communication) or **FFI** (Foreign Function Interface) calls to OS APIs.

---

### 1. Windows (Win32 / UWP)
**Current Status:** Parses the Registry (`HKCU\Software\Microsoft\...`) via `reg.exe`.
**Critique:** Registry keys are internal implementation details. They do not account for high-contrast overrides, transient states, or correct alpha blending of accent colors.

#### Recommended Native APIs
*   **Color & Theme (Modern):** `Windows.UI.ViewManagement.UISettings` (WinRT).
    *   This API provides the definitive `GetColorValue` method.
    *   It exposes `UIColorType` (Background, Foreground, Accent, AccentDark1, AccentLight1, etc.).
    *   It supports event listeners (`ColorValuesChanged`) for real-time updates.
*   **System Metrics:** `GetSystemMetrics` (User32).
    *   Standard for retrieving scrollbar width (`SM_CXVSCROLL`), border padding, and icon sizing.
*   **Fonts:** `SystemParametersInfoW` (User32).
    *   Calling this with `SPI_GETNONCLIENTMETRICS` returns a `NONCLIENTMETRICSW` struct containing `LOGFONT` data for MessageFonts, CaptionFonts, and MenuFonts (the actual fonts the OS uses, rather than hardcoded "Segoe UI").
*   **High Contrast:** `SystemParametersInfoW` with `SPI_GETHIGHCONTRAST`.

### 2. macOS (Cocoa / AppKit)
**Current Status:** Parses `defaults read -g` and `sw_vers`.
**Critique:** `defaults` reads the plist on disk. It does not resolve **semantic colors**. For example, `NSColor.windowBackgroundColor` is not a single hex code; it is a dynamic proxy that resolves differently based on the active appearance (Dark/Light/High Contrast) and vibrancy settings.

#### Recommended Native APIs (via Objective-C Runtime)
*   **Theme Detection:** `NSAppearance` (AppKit).
    *   Use `[NSApp effectiveAppearance]` to determine if the resolved appearance is Aqua (Light) or Dark Aqua.
*   **Colors:** `NSColor` (AppKit).
    *   Instead of hardcoding RGBs, query semantic standard colors: `[NSColor labelColor]`, `[NSColor controlAccentColor]`, `[NSColor windowBackgroundColor]`.
    *   *Note:* You must resolve these colors against the current context using `CGColor` extraction to get actual RGB values.
*   **Fonts:** `NSFont` (AppKit).
    *   `[NSFont systemFontOfSize: 0.0]` returns the user's preferred UI font (usually SF Pro).
    *   `[NSFont monospacedSystemFontOfSize: ...]` returns SF Mono.
*   **Metrics:** `NSScroller`
    *   `[NSScroller scrollerWidthForControlSize: ...]` is the canonical way to get the scrollbar width, which changes based on input device settings (mouse vs. trackpad).

### 3. Linux (Freedesktop / DBus)
**Current Status:** Wraps `gsettings` (GNOME) and `kreadconfig5` (KDE).
**Critique:** While `gsettings` is stable, spawning processes is slow. The implementation also manually parses specific config files for Hyprland/Pywal, which is brittle.

#### Recommended Native APIs
*   **The Unified Standard:** **XDG Desktop Portals** (DBus).
    *   This is the modern, distro-agnostic standard (used by Flatpak/Snap, but available natively).
    *   **Interface:** `org.freedesktop.portal.Settings`.
    *   **Property:** `Read("org.freedesktop.appearance", "color-scheme")` returns `0` (No Preference), `1` (Dark), or `2` (Light). This works on GNOME 42+, KDE Plasma 6, and Sway/Hyprland (via `xdg-desktop-portal-wlr` or `gtk`).
*   **GTK Specifics:** `GtkSettings` (via FFI/GObject Introspection).
    *   If Portals are unavailable, querying the GObject property `gtk-theme-name` and `gtk-font-name` directly via the C API is faster than shelling out.
*   **KDE Specifics:** `KConfig` (C++) is hard to bind to Rust.
    *   *Alternative:* Direct parsing of `~/.config/kdeglobals` is acceptable here if avoiding C++, but utilizing the XDG Portal is preferred for the theme switch.

### 4. Accessibility & Metrics
**Current Status:** Limited registry checks and `gsettings` keys.

#### Cross-Platform API Gaps
*   **Text Scaling:**
    *   **Windows:** `IDWriteFactory::GetSystemFontCollection` reflects user scaling preferences better than fixed point sizes.
    *   **Linux:** `org.gnome.desktop.interface/text-scaling-factor` (float).
*   **Animation/Motion:**
    *   **Windows:** `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)`.
    *   **Web Standard Mapping:** The implementation correctly maps these to `prefers-reduced-motion`, but the discovery method needs to be via system APIs (like `UIAccessibilityIsReduceMotionEnabled` on macOS) to be compliant with store guidelines.

### 5. Implementation Strategy Recommendation

Instead of `std::process::Command`, the module should be refactored to use standard Rust FFI crates that wrap these APIs without requiring a full GUI toolkit dependency:

1.  **Windows:** Use the `windows` or `windows-sys` crate. These are zero-cost bindings to the Win32 and WinRT APIs mentioned above.
2.  **macOS:** Use the `objc2` and `objc2-app-kit` crates. This allows sending messages to `NSColor` and `NSFont` dynamically at runtime without linking the entire Cocoa framework statically if strict separation is needed.
3.  **Linux:** Use the `zbus` crate (pure Rust DBus implementation) to query `org.freedesktop.portal.Settings`. This removes the dependency on `gsettings` or KDE binaries existing in the PATH.

This approach transforms the system from "best-guess parsing" to "native OS integration," ensuring that when a user changes their accent color or switches to Dark Mode, the framework receives the exact values the OS intends.

---

Based on the code provided, your discovery system focuses primarily on **static colors** and **basic font names**. However, a native-feeling UI framework requires **dynamic behavioral metrics** and **rendering hints** that are currently missing.

Here is a report on the missing customizations and the specific APIs required to retrieve them.

---

### 1. Visual Materials & Translucency (Crucial for "Modern" Look)
The current implementation treats backgrounds as solid RGB colors. Modern OS designs (Windows 11, macOS, recent GNOME) rely on material effects that blend the window with the desktop wallpaper or windows behind it.

*   **Missing Data:**
    *   **Windows:** Mica (opaque but tinted by wallpaper), Acrylic (blur), and Smoke (glass).
    *   **macOS:** Vibrancy (NSVisualEffectView materials: Sidebar, HUD, Popover, UnderWindow).
    *   **Linux:** Blur strength and opacity (common in Hyprland/KDE).

*   **How to retrieve it:**
    *   **Windows:** You cannot "read" this value; you must request the OS to apply it to your window handle (HWND). Use the `windows` crate to call `DwmSetWindowAttribute` with `DWMWA_SYSTEMBACKDROP_TYPE`.
    *   **macOS:** Use `objc2`. You need to check `[NSVisualEffectView material]` types to emulate them, or more accurately, simply flag your window to use these native backing layers.
    *   **Linux (Hyprland):** Query the socket for `decoration:blur` and `decoration:opacity`.

### 2. Input Interaction Metrics
Your code contains `DoubleClick` logic (implied by the framework context), but it likely uses hardcoded timing (e.g., 500ms). OS users customize this, and ignoring it makes the app feel "laggy" or "jittery."

*   **Missing Data:**
    *   **Double-Click Time:** The max milliseconds between clicks to register a double-click.
    *   **Double-Click Distance:** How many pixels the mouse can move between clicks and still count.
    *   **Drag Threshold:** How many pixels the mouse must move while held down before a drag operation starts.
    *   **Caret Blink Rate:** How fast the text cursor blinks (or if it shouldn't blink).

*   **How to retrieve it:**
    *   **Windows:** `GetDoubleClickTime()`, `GetSystemMetrics(SM_CXDOUBLECLK)`, `GetCaretBlinkTime()`.
    *   **macOS:** `NSEvent.doubleClickInterval`.
    *   **Linux:**
        *   **GTK/GNOME:** `gsettings get org.gnome.settings-daemon.peripherals.mouse double-click`.
        *   **X11:** `XGetDefault` settings.

### 3. Scrollbar Behavior (macOS Specifics)
The current code has a `ScrollbarInfo` struct, but it misses the **visibility trigger**. macOS users often set scrollbars to "Show automatically based on mouse or trackpad."

*   **Missing Data:**
    *   **Scrollbar Visibility:** `Automatic` (only when scrolling), `WhenScrolling`, or `Always`.
    *   **Click Behavior:** Does clicking the track jump to the spot or page down?

*   **How to retrieve it:**
    *   **macOS:** `[NSScroller preferredScrollerStyle]` returns `NSScrollerStyleOverlay` (iPhone-like) or `NSScrollerStyleLegacy` (always visible).
    *   **Windows:** `SystemParametersInfo` with `SPI_GETWHEELSCROLLLINES` to determine how many lines to scroll per notch (defaults to 3, but often customized).

### 4. Text Rendering & Scaling
Your code detects `text_scale_factor`, but misses the deeper rendering hints required for crisp text that matches the OS.

*   **Missing Data:**
    *   **Font Smoothing (Antialiasing):** Windows users can tune ClearType (RGB vs BGR subpixel layout, contrast).
    *   **Text Contrast:** macOS increases contrast in "Darker System Colors" mode.

*   **How to retrieve it:**
    *   **Windows:** `SystemParametersInfo(SPI_GETFONTSMOOTHING...)`. To get the actual ClearType parameters, you access the registry at `HKCU\Control Panel\Desktop\FontSmoothingGamma`, etc.
    *   **macOS:** `[[NSWorkspace sharedWorkspace] accessibilityDisplayShouldIncreaseContrast]`.

### 5. Focus Visuals
Your `SystemColors` struct includes `accent` and `selection`, but often the **Focus Ring** is distinct.

*   **Missing Data:**
    *   **Focus Ring Color:** On macOS, this can be distinct from the accent color (e.g., a user might have a Blue accent but a Graphite focus ring).
    *   **Focus Animation:** Windows "marching ants" vs. macOS "glowing ring" vs. GTK "dashed line".

*   **How to retrieve it:**
    *   **macOS:** `[NSColor keyboardFocusIndicatorColor]`.
    *   **Windows:** `SystemParametersInfo(SPI_GETFOCUSBORDERHEIGHT)`.

### 6. Linux "Ricing" Specifics (Icon & Cursor Themes)
The code checks for Hyprland borders but misses the two most common customizations in the Linux community: Icons and Cursors.

*   **Missing Data:**
    *   **Icon Theme Name:** e.g., "Papirus", "Numix". Used to resolve paths to SVG icons.
    *   **Cursor Theme & Size:** e.g., "Breeze_Snow", size 24 vs 48.

*   **How to retrieve it:**
    *   **Unified (Portal):** Query `org.freedesktop.appearance` via DBus.
    *   **GSettings:** `org.gnome.desktop.interface icon-theme` and `cursor-theme`.
    *   **Env Vars:** `XCURSOR_THEME` and `XCURSOR_SIZE` are standard across almost all WMs (Hyprland, Sway, i3).

---

### Recommended Implementation Plan

To get these without slowing down startup (shelling out is slow), I recommend moving `SystemStyle::detect()` to a threaded initialization or using FFI crates:

1.  **For Windows:** Use the `windows` crate.
    ```rust
    unsafe {
        let mut time = 0;
        // Near-instant retrieval via User32
        windows::Win32::UI::Input::KeyboardAndMouse::GetCaretBlinkTime(); 
    }
    ```

2.  **For macOS:** Use `objc2` and `objc2-app-kit`.
    ```rust
    use objc2_app_kit::NSScroller;
    // Check if scrollbars should overlay or take up space
    let style = unsafe { NSScroller::preferredScrollerStyle() };
    ```

3.  **For Linux:** Use `zbus` (pure Rust DBus) to hit the XDG Portal. This is the only way to get "System Theme" reliably across KDE, GNOME, and Hyprland without specific hacks for each.

---

To implement a **"soft fallback"** discovery system—where you try to load OS libraries at runtime and fall back to hardcoded defaults if they fail—you need to use the `libloading` crate (or raw `dlopen`/`LoadLibrary` calls).

Here are the specific dynamic libraries and the **symbols (functions)** you need to load to get the missing customizations mentioned in the previous report.

---

### 1. Windows (Win32 API)

On Windows, system DLLs are guaranteed to exist, but specific functions (like Dark Mode detection in `Dwmapi`) might be missing on older versions (Windows 7).

**Library:** `User32.dll` (Core UI Metrics)
*   **Why:** Basic system metrics, colors, and input settings.
*   **Symbols to Load:**
    *   `GetSystemMetrics`: Retrieves scrollbar width (`SM_CXVSCROLL`), double-click dimensions (`SM_CXDOUBLECLK`), and high-contrast status.
    *   `SystemParametersInfoW`: Retrieves "SPI" values (Caret blink time, Font smoothing, Animation effects).
    *   `GetSysColor`: Retrieves standard RGB colors (ButtonFace, WindowText).

**Library:** `Dwmapi.dll` (Desktop Window Manager)
*   **Why:** Modern "Mica" materials and Dark Mode checks for titlebars.
*   **Symbols to Load:**
    *   `DwmGetColorizationColor`: Gets the user's accent color and transparency blend.
    *   `DwmGetWindowAttribute`: Used with `DWMWA_USE_IMMERSIVE_DARK_MODE` (20) to check if the app should draw a dark titlebar.

**Library:** `UxTheme.dll` (Visual Styles)
*   **Why:** Accessing specific theme definitions (e.g., "Aero", "Luna").
*   **Symbols to Load:**
    *   `IsThemeActive`: Checks if the user is using a High Contrast theme (returns false) or a visual style.

---

### 2. macOS (Cocoa / Objective-C Runtime)

macOS does not use "DLLs" in the Windows sense. You interact with **Frameworks**. You cannot easily `dlopen` just the style logic; you must load the Objective-C runtime and message the `AppKit` framework.

**Library:** `/usr/lib/libobjc.A.dylib` (The Runtime)
*   **Why:** You need this to send messages to the system APIs without linking.
*   **Symbols to Load:**
    *   `objc_getClass`: To get class references (e.g., `NSColor`, `NSFont`, `NSScroller`).
    *   `sel_registerName`: To register selectors (method names like `systemFontOfSize:`, `currentControlTint`).
    *   `objc_msgSend`: To actually call the methods.

**Library:** `/System/Library/Frameworks/AppKit.framework/AppKit`
*   **Why:** This contains the logic for colors and metrics.
*   **Strategy:** You `dlopen` this path to ensure the framework is loaded into memory. Once loaded, you use the `libobjc` functions above to query classes like `NSColor`.
*   **Key Classes to Query (via Runtime):**
    *   `NSColor`: Ask for `controlAccentColor` (returns a dynamic color object).
    *   `NSScroller`: Ask for `preferredScrollerStyle` (Overlay vs Legacy).
    *   `NSWorkspace`: Ask `accessibilityDisplayShouldIncreaseContrast`.

---

### 3. Linux (GIO / GObject)

**Avoid loading GTK.** Loading `libgtk-3.so` or `libgtk-4.so` purely for settings is dangerous—it may try to open a display connection to X11/Wayland, which can crash your app if you are initializing your own windowing system (like Winit) simultaneously.

Instead, load **GLib/GIO**. This allows you to query `GSettings` (the registry of GNOME/Unity/Budgie) without initializing a GUI.

**Library:** `libgio-2.0.so.0` (or `libgio-2.0.so`)
*   **Why:** To read `org.gnome.desktop.interface` without spawning the `gsettings` CLI process (which is slow).
*   **Symbols to Load:**
    *   `g_settings_new`: Open a schema (e.g., "org.gnome.desktop.interface").
    *   `g_settings_get_value`: Read a variant (color, font string).
    *   `g_settings_get_string`: Helper to get strings directly.
    *   `g_settings_get_int`: Helper for metrics (cursor size).

**Library:** `libgobject-2.0.so.0`
*   **Why:** Required to handle the types returned by GIO.
*   **Symbols to Load:**
    *   `g_object_unref`: To clean up the settings objects (memory management).

### Summary Table for `libloading`

| Platform | Filename to Load | Critical Symbols (dlsym) | Usage |
| :--- | :--- | :--- | :--- |
| **Windows** | `User32.dll` | `GetSystemMetrics`, `SystemParametersInfoW` | Metrics (Scrollbar size, double click) |
| **Windows** | `Dwmapi.dll` | `DwmGetColorizationColor` | Accent color & glass effects |
| **macOS** | `libobjc.A.dylib` | `objc_msgSend`, `sel_registerName` | Calling AppKit methods dynamically |
| **macOS** | `AppKit.framework` | *(None - just dlopen to load classes)* | Ensures `NSColor` class exists |
| **Linux** | `libgio-2.0.so.0` | `g_settings_new`, `g_settings_get_value` | Reading GNOME/GTK config efficiently |

### Recommendation for Linux
On Linux, even dynamic loading of `libgio` can be brittle due to ABI versioning.
**Better Alternative:** Use the crate **`zbus`**.
It is a **pure Rust** implementation of DBus. It requires **no dynamic linking** to C libraries. You can talk directly to the `freedesktop` portals or `org.gtk.Settings` over the DBus socket. This is the most robust, crash-proof method for Linux system discovery.

---

This is a great initiative. To build a true "Superset" `SystemStyle` that captures the distinct "feel" of Qt/KDE, GNOME, Windows, and macOS, you need to aggregate **Behavioral**, **Visual**, and **Input** preferences.

Here is the breakdown of the fields you should add to your `SystemStyle` struct and exactly how to dynamically load them on each OS without linking.

### The "Superset" Struct Definition

First, expand your `SystemStyle` to include these categories:

```rust
pub struct SystemStyle {
    // ... existing fields (colors, fonts) ...

    pub visual_hints: VisualHints,
    pub input_metrics: InputMetrics,
    pub animation_metrics: AnimationMetrics,
    pub audio_metrics: AudioMetrics,
}

pub struct VisualHints {
    /// Show icons on push buttons? (Common in KDE, rare in Win/Mac)
    pub show_button_images: bool, 
    /// Show icons in context menus? (GNOME defaults off, Win/Mac/KDE usually on)
    pub show_menu_images: bool,
    /// Toolbar style: Icons only, Text only, Text beside Icon, Text below Icon
    pub toolbar_style: ToolbarStyle,
    /// Should tooltips be shown?
    pub show_tooltips: bool,
    /// Flash the window taskbar entry on alert?
    pub flash_on_alert: bool,
}

pub struct InputMetrics {
    /// Max ms between clicks to count as double-click (e.g., 500ms)
    pub double_click_time_ms: u32,
    /// Max pixels cursor can move during double-click (e.g., 4px)
    pub double_click_distance: u32,
    /// Pixels mouse must move to start a drag operation
    pub drag_threshold: u32,
    /// Ms to wait before a hover event triggers (tooltips)
    pub hover_time_ms: u32,
    /// Text cursor blink interval (0 = no blink)
    pub caret_blink_time_ms: u32,
    /// Width of the text cursor
    pub caret_width: u32,
}

pub struct AnimationMetrics {
    /// Global enable/disable for UI animations
    pub animations_enabled: bool,
    /// Global animation speed factor (1.0 = normal, 0.5 = slow, 2.0 = fast)
    /// Heavily used in KDE.
    pub animation_duration_factor: f32,
    /// Focus rectangle behavior (Always visible vs. Only on keyboard nav)
    pub focus_indicator_behavior: FocusBehavior,
}

pub struct AudioMetrics {
    /// Should the app make sounds on events? (Error ping, etc.)
    pub event_sounds_enabled: bool,
    /// Should the app make sounds on input? (Clicking, typing)
    pub input_feedback_sounds_enabled: bool,
}
```

---

### 1. Visual Hints (Images, Toolbars, Menus)

This is where the "Qt/KDE vs GNOME" fight happens. Windows/macOS are more opinionated.

*   **Linux (The main target for this):**
    *   **Source:** `GSettings` (GNOME) or `KConfig` (KDE).
    *   **API:**
        *   **GNOME:** `org.gnome.desktop.interface` keys:
            *   `gtk-enable-primary-paste` (bool)
            *   `menus-have-icons` (bool) - *Often deprecated in newer GNOME, but check legacy.*
            *   `buttons-have-icons` (bool)
            *   `toolbar-style` (string: "both", "icons", "text", "both-horiz")
        *   **KDE:** Read `~/.config/kdeglobals`:
            *   Group `[KDE]`: `ShowIconsOnPushButtons`
            *   Group `[Toolbar style]`: `ToolButtonStyle`
*   **Windows:**
    *   **Source:** `SystemParametersInfo` (User32).
    *   **Detail:** Windows generally implies "Yes" for menu icons and "No" for button icons unless using specific controls.
    *   **API:** `SPI_GETMENUDROPALIGNMENT` (Left/Right alignment). Windows doesn't explicitly expose "show images on buttons" as a system-wide flag; you should default to `false` (standard Win32/UWP look) or `true` if imitating older styles.
*   **macOS:**
    *   **Source:** N/A (Human Interface Guidelines).
    *   **Detail:** macOS strictly defines this. Menus have icons (if provided). Standard push buttons do *not* have icons. Toolbars have user-configurable styles.
    *   **Implementation:** Hardcode defaults: Buttons=False, Menus=True.

### 2. Input Metrics (Double Click, Drag, Caret)

This is critical for the app not feeling "laggy."

*   **Windows:**
    *   **Library:** `User32.dll`
    *   **API:**
        *   `GetDoubleClickTime()` (returns UINT ms).
        *   `GetSystemMetrics(SM_CXDOUBLECLK)` / `SM_CYDOUBLECLK` (rect size).
        *   `GetSystemMetrics(SM_CXDRAG)` (drag threshold).
        *   `GetCaretBlinkTime()` (ms).
        *   `SystemParametersInfo(SPI_GETMOUSEHOVERTIME)`.
*   **macOS:**
    *   **Library:** `AppKit` (via `objc2`).
    *   **API:**
        *   `NSEvent.doubleClickInterval` (NSTimeInterval).
        *   `NSValuedKey` in `NSGlobalDomain`: `NSTextInsertionPointBlinkPeriod`.
        *   Drag threshold is usually treated as 3-4 pixels standard, not strictly exposed.
*   **Linux:**
    *   **Source:** GSettings / XSettings.
    *   **API:**
        *   `org.gnome.settings-daemon.peripherals.mouse double-click` (int).
        *   `org.gnome.desktop.interface cursor-blink-time` (int).
        *   `org.gnome.desktop.interface cursor-blink` (bool).
        *   `gtk-dnd-drag-threshold` (int).

### 3. Animation & Focus (The "Snappy" Feel)

*   **Windows:**
    *   **Library:** `User32.dll`
    *   **API:**
        *   `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)`: Global animation toggle.
        *   `SystemParametersInfo(SPI_GETKEYBOARDCUES)`: Returns `TRUE` if focus rectangles should *only* show after a key press, `FALSE` if always visible. **(Very important for Windows feel!)**
*   **macOS:**
    *   **Library:** `AppKit`.
    *   **API:**
        *   `NSWorkspace.accessibilityDisplayShouldReduceMotion` (Logic inversion: if true, animations_enabled = false).
        *   Focus rings are always visible on focus in macOS; no separate setting.
*   **Linux:**
    *   **Source:** KDE Globals / GSettings.
    *   **API:**
        *   **GNOME:** `org.gnome.desktop.interface enable-animations` (bool).
        *   **KDE:** `[KDE] AnimationDurationFactor` in `kdeglobals`. (e.g., 0.5 makes animations 2x faster).

### 4. Audio Feedback (Sound Themes)

*   **Windows:**
    *   **Library:** Registry / `User32`.
    *   **API:** Check `SystemParametersInfo(SPI_GETBEEP)` for simple beeps. Complex sound schemes are in Registry `AppEvents\Schemes`.
*   **macOS:**
    *   **Library:** `AppKit`.
    *   **API:** `NSSound.soundEffectAudioVolume` > 0.
*   **Linux:**
    *   **API:**
        *   `org.gnome.desktop.sound event-sounds` (bool).
        *   `org.gnome.desktop.sound input-feedback-sounds` (bool).

---

### Implementation Strategy: The "Soft Load" Architecture

Since you want to avoid linking, use `libloading` for Windows/macOS and `zbus` (pure Rust, no C-link) for Linux.

#### 1. Windows Implementation (`libloading` wrapper)

```rust
#[cfg(target_os = "windows")]
fn get_windows_metrics() -> InputMetrics {
    unsafe {
        // Load User32 dynamically
        let user32 = libloading::Library::new("user32.dll").ok();
        
        // Define function signatures
        type GetDoubleClickTime = unsafe extern "system" fn() -> u32;
        type GetSystemMetrics = unsafe extern "system" fn(i32) -> i32;
        type GetCaretBlinkTime = unsafe extern "system" fn() -> u32;

        let mut metrics = InputMetrics::default();

        if let Some(lib) = user32 {
            if let Ok(func) = lib.get::<GetDoubleClickTime>(b"GetDoubleClickTime") {
                metrics.double_click_time_ms = func();
            }
            if let Ok(func) = lib.get::<GetSystemMetrics>(b"GetSystemMetrics") {
                // SM_CXDRAG = 68
                metrics.drag_threshold = func(68) as u32;
            }
            if let Ok(func) = lib.get::<GetCaretBlinkTime>(b"GetCaretBlinkTime") {
                metrics.caret_blink_time_ms = func();
            }
        }
        metrics
    }
}
```

#### 2. Linux Implementation (File Parsing + DBus)

Don't `dlopen` GTK. It's unsafe.
1.  **Try DBus (XDG Portal):** Use `zbus` to check `org.freedesktop.portal.Settings`. This is the future-proof way.
2.  **Fallback (Config Parsing):** If DBus fails, read `~/.config/gtk-3.0/settings.ini`. It is a standard INI file.

```ini
[Settings]
gtk-double-click-time=400
gtk-enable-animations=1
gtk-menu-images=0
```

*Parsing this text file is faster and safer than loading libgtk.so.*

#### 3. macOS Implementation (`objc2` / `block` crates)

You don't need `libloading` for macOS frameworks in the same way; you need the Objective-C runtime.

```rust
#[cfg(target_os = "macos")]
fn get_macos_visuals() -> VisualHints {
    use objc2::rc::Retained;
    use objc2_app_kit::NSWorkspace; // Requires objc2-app-kit crate (safe bindings)
    
    // Note: objc2 links to Foundation/AppKit, but these are system frameworks 
    // guaranteed to be there. It's effectively dynamic loading.
    
    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    let reduce_motion = unsafe { workspace.accessibilityDisplayShouldReduceMotion() };
    
    VisualHints {
        animations_enabled: !reduce_motion,
        // macOS standard overrides:
        show_button_images: false, 
        show_menu_images: true,
        // ...
    }
}
```

### Summary of What to Watch Out For

1.  **Windows "Keyboard Cues" (`SPI_GETKEYBOARDCUES`):** This is the most often missed "native feel" feature. If you don't respect this, your app looks cluttered compared to native Windows apps which hide underlines/dotted boxes until Alt/Tab is pressed.
2.  **Linux Fragmentation:** Do not rely solely on `gsettings`. KDE users might not have the GNOME schema installed, causing your discovery to crash or return defaults. **Always** check `XDG_CURRENT_DESKTOP` and parse `kdeglobals` if it's KDE.
3.  **macOS Scrollbars:** The setting "Show scroll bars: Automatically based on mouse or trackpad" is crucial. If you force scrollbars to be visible when the user expects them to be hidden overlays, the app looks "ported" and old.
4.  **Performance:** Do **not** run these checks every frame. Run them on startup and set up a watcher (Win32 event loop listener / DBus signal listener) for changes.
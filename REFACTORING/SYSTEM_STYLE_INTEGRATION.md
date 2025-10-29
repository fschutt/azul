# SystemStyle Integration Guide

**Date**: 2025-10-28  
**Status**: Partial implementation - Linux only

---

## Overview

SystemStyle provides access to platform-specific UI styling information (theme colors, fonts, DPI, etc.) detected at application startup. This allows applications to adapt their appearance to match the host operating system.

## Architecture

### SystemStyle Detection

`SystemStyle` is detected once at application startup using `azul_css::system::SystemStyle::new()`. This performs platform-specific queries:

- **Linux**: Queries gsettings (GNOME), kreadconfig5 (KDE), or parses config files (riced desktops)
- **Windows**: Queries registry for theme and accent colors
- **macOS**: Queries NSUserDefaults for appearance mode
- **Android/iOS**: Returns platform defaults

### Storage and Sharing

SystemStyle is stored in `Arc<SystemStyle>` and shared across all windows:

- **Linux**: Stored in `AppResources` structure
- **Windows**: TODO - needs to be added
- **macOS**: TODO - needs to be added

### Exposure to User Callbacks

SystemStyle is exposed to user layout callbacks through `LayoutCallbackInfo` using the ABI extension mechanism:

```rust
// In window's regenerate_layout() method
use crate::desktop::callback_ext;

let mut callback_info = callback_ext::new_with_system_style(
    window_state.size,
    window_state.theme,
    &image_cache,
    &gl_context_ptr,
    &fc_cache,
    &system_style,  // Arc<SystemStyle> from AppResources
);

// User callback receives this
let styled_dom = (layout_callback)(&mut app_data, &mut callback_info);

// In user code, retrieve SystemStyle
if let Some(style) = callback_ext::get_system_style(&callback_info) {
    eprintln!("Theme: {:?}", style.theme);
    eprintln!("Accent color: {:?}", style.colors.accent);
    eprintln!("UI font: {:?}", style.fonts.ui_font);
}
```

## Implementation Status

| Platform | SystemStyle Detection | Storage in AppResources | Exposed to Callbacks |
|----------|----------------------|------------------------|---------------------|
| Linux X11 | ✅ Complete | ✅ Complete | ⚠️ Infrastructure ready, not wired up |
| Linux Wayland | ❌ Not implemented | ❌ Not implemented | ❌ Not implemented |
| Windows | ✅ Complete (in css crate) | ❌ TODO | ❌ TODO |
| macOS | ✅ Complete (in css crate) | ❌ TODO | ❌ TODO |
| Android | ✅ Complete (in css crate) | N/A | N/A |
| iOS | ✅ Complete (in css crate) | N/A | N/A |

## Integration Tasks

### Linux (Completed)

1. ✅ Create `AppResources` structure with `Arc<SystemStyle>`
2. ✅ Initialize SystemStyle in `run()` function
3. ✅ Pass AppResources to all windows
4. ⚠️ **TODO**: Wire up `callback_ext::new_with_system_style()` in `X11Window::regenerate_layout()`

### Windows (TODO)

1. ❌ Add `system_style: Arc<SystemStyle>` to `Win32Window` struct
2. ❌ Initialize SystemStyle in `run()` function
3. ❌ Pass to window constructor
4. ❌ Wire up `callback_ext::new_with_system_style()` in `Win32Window::regenerate_layout()`

### macOS (TODO)

1. ❌ Add `system_style: Arc<SystemStyle>` to `MacOSWindow` struct
2. ❌ Initialize SystemStyle in `run()` function
3. ❌ Pass to window constructor
4. ❌ Wire up `callback_ext::new_with_system_style()` in `MacOSWindow::regenerate_layout()`

## API Reference

### dll/src/desktop/callback_ext.rs

#### `new_with_system_style()`

Creates a `LayoutCallbackInfo` with SystemStyle extension data.

```rust
pub fn new_with_system_style(
    window_size: WindowSize,
    theme: WindowTheme,
    image_cache: &ImageCache,
    gl_context: &OptionGlContextPtr,
    fc_cache: &FcFontCache,
    system_style: &Arc<SystemStyle>,
) -> LayoutCallbackInfo
```

**Parameters**:
- Standard LayoutCallbackInfo fields
- `system_style`: Shared reference to SystemStyle

**Returns**: LayoutCallbackInfo with extension data populated

#### `get_system_style()`

Retrieves SystemStyle from a LayoutCallbackInfo.

```rust
pub fn get_system_style(callback_info: &LayoutCallbackInfo) -> Option<&SystemStyle>
```

**Returns**: 
- `Some(&SystemStyle)` if added via `new_with_system_style()`
- `None` if created with standard `LayoutCallbackInfo::new()`

#### `cleanup_callback_info_extension()`

Frees extension data to prevent memory leaks.

```rust
pub unsafe fn cleanup_callback_info_extension(callback_info: &mut LayoutCallbackInfo)
```

**Safety**: Must only be called if callback_info was created with `new_with_system_style()`.

**Note**: Currently not called automatically - needs to be integrated into window Drop implementations.

### core/src/callbacks.rs

#### Added methods to LayoutCallbackInfo:

```rust
// Get ABI extension pointer
pub fn get_abi_ref(&self) -> *const c_void

// Set ABI extension pointer (unsafe)
pub unsafe fn set_abi_ref(&mut self, ptr: *const c_void)
```

These methods provide access to the `_abi_ref` field for extension data while maintaining ABI stability.

## Usage Examples

### User Code (Layout Callback)

```rust
use azul_dll::desktop::callback_ext;

extern "C" fn my_layout(
    data: &mut RefAny,
    info: &mut LayoutCallbackInfo
) -> StyledDom {
    // Access system style if available
    if let Some(style) = callback_ext::get_system_style(info) {
        // Use system colors for theming
        let bg_color = style.colors.background
            .unwrap_or(ColorU { r: 255, g: 255, b: 255, a: 255 });
        
        // Use system fonts
        let ui_font = style.fonts.ui_font
            .as_deref()
            .unwrap_or("sans-serif");
        
        // Adapt to dark/light theme
        let is_dark = matches!(style.theme, Theme::Dark);
        
        // Build DOM with system-aware styling
        Dom::div()
            .with_inline_style(&format!(
                "background: rgb({},{},{}); font-family: {};",
                bg_color.r, bg_color.g, bg_color.b, ui_font
            ))
            .with_child(Dom::text("Hello, themed world!"))
            .style(window_css)
    } else {
        // Fallback for when SystemStyle is not available
        Dom::text("SystemStyle not available")
            .style(window_css)
    }
}
```

### Window Implementation (Internal)

```rust
// In regenerate_layout() method
use crate::desktop::callback_ext;

// Get SystemStyle from window's resources
let system_style = &self.resources.system_style; // Linux
// OR: let system_style = &self.system_style; // Windows/macOS (when implemented)

// Create callback info with SystemStyle
let mut callback_info = callback_ext::new_with_system_style(
    self.current_window_state.size.clone(),
    self.current_window_state.theme,
    &self.image_cache,
    &self.gl_context_ptr,
    &*self.fc_cache,
    system_style,
);

// Call user's layout callback
let styled_dom = match &self.current_window_state.layout_callback {
    LayoutCallback::Raw(inner) => {
        (inner.cb)(&mut *app_data, &mut callback_info)
    }
    // ... other callback types
};

// TODO: Add cleanup when callback_info is dropped
// unsafe { callback_ext::cleanup_callback_info_extension(&mut callback_info); }
```

## Memory Management

### Current Status

The extension data is allocated with `Box::new()` and converted to a raw pointer. Currently, **this memory is leaked** - cleanup is not automatic.

### TODO: Implement Proper Cleanup

Option 1: Add Drop impl to LayoutCallbackInfo (requires azul-core changes):
```rust
impl Drop for LayoutCallbackInfo {
    fn drop(&mut self) {
        // Check if extension data exists and free it
        unsafe {
            crate::callbacks::cleanup_extension_if_present(self);
        }
    }
}
```

Option 2: Call cleanup explicitly in window code:
```rust
// In regenerate_layout(), after calling user callback
let styled_dom = (callback)(&mut app_data, &mut callback_info);
unsafe {
    callback_ext::cleanup_callback_info_extension(&mut callback_info);
}
// Continue processing styled_dom...
```

Option 3: Use reference counting instead of raw pointer:
```rust
// Store Arc<LayoutCallbackExtension> instead of raw pointer
// This would require changing the extension pointer type
```

**Recommended**: Implement Option 2 as a short-term fix, then Option 1 for long-term solution.

## Testing

### Unit Tests

The `callback_ext` module includes a roundtrip test:

```bash
cargo test --package azul-dll --lib desktop::callback_ext::tests
```

### Integration Testing

To test SystemStyle detection on Linux:

```bash
# Check that SystemStyle is detected
AZUL_LOG=debug cargo run --example simple_window

# Should print:
# [AppResources] System style detected:
#   Platform: Linux(Gnome)
#   Theme: Dark
#   UI Font: Some("Ubuntu")
#   Accent Color: Some(ColorU { r: 53, g: 132, b: 228, a: 255 })
```

## Future Enhancements

1. **Automatic refresh on theme change**: Listen for system events and update SystemStyle when user changes theme
2. **Per-monitor DPI**: Track DPI per monitor instead of single global value
3. **Color scheme queries**: Add helper methods like `style.is_high_contrast()`, `style.supports_transparency()`
4. **Font fallbacks**: Provide multiple font options per category (sans-serif, serif, monospace)
5. **CSS variable injection**: Automatically inject system colors as CSS custom properties

## References

- SystemStyle implementation: `css/src/system.rs`
- Linux resources: `dll/src/desktop/shell2/linux/resources.rs`
- Callback extension: `dll/src/desktop/callback_ext.rs`
- LayoutCallbackInfo: `core/src/callbacks.rs`
- Action plan: `REFACTORING/actionplan-linux2.md` (Section 3.1)

# Unused and Stubbed Code Inventory

**Date:** October 30, 2025  
**Purpose:** Comprehensive list of dead code, stubs, and incomplete features for cleanup

---

## Executive Summary

The codebase contains several categories of unused or incomplete code:

1. **Dead Code** - Written but never called
2. **Stubs** - Placeholders that don't do anything functional
3. **Incomplete Features** - Partially implemented functionality
4. **Disabled Code** - Explicitly commented out or blocked

This document catalogs all such code for potential removal or completion.

---

## Category 1: Dead Code (Never Called)

### 1.1 `csd_titlebar_doubleclick_callback`

**Location:** `dll/src/desktop/csd.rs`

**Status:** 🔴 Dead Code

**Details:**
- Function exists and is exported
- **Never attached** to any DOM node in `create_titlebar_dom()`
- Intended to maximize/restore window on double-click
- Feature is planned but not wired up

**Code:**
```rust
extern "C" fn csd_titlebar_doubleclick_callback(
    _data: &mut RefAny,
    info: &mut CallbackInfo,
) -> Update {
    // Logic exists but is never called
    // ...
}
```

**Action:** Either wire it up or remove it.

---

### 1.2 `msg_box_ok_cancel` and `msg_box_yes_no`

**Location:** `dll/src/desktop/dialogs.rs`

**Status:** 🟡 Unused Public API

**Details:**
- Part of public API (`pub fn`)
- Never called by any internal code
- `msg_box_ok()` is used, but these variants are not
- May be used by external consumers (check before removing)

**Code:**
```rust
pub fn msg_box_ok_cancel(...) -> MsgBoxResult { ... }
pub fn msg_box_yes_no(...) -> MsgBoxResult { ... }
```

**Action:** Keep (part of public API), but mark with `#[allow(dead_code)]` if needed.

---

## Category 2: Stubbed Code (Non-Functional)

### 2.1 CPU Compositor

**Location:** `dll/src/desktop/shell2/common/cpu_compositor.rs`

**Status:** 🔴 Stubbed

**Details:**
- `CpuCompositor::rasterize()` just clears to white
- Not a real software rasterizer
- WebRender in software mode is the actual CPU rendering path

**Code:**
```rust
pub fn rasterize(&mut self, _display_list: &[DisplayListItem]) -> Result<(), String> {
    // Clear entire buffer to white
    for pixel in self.framebuffer.chunks_exact_mut(4) {
        pixel[0] = 255; // R
        pixel[1] = 255; // G
        pixel[2] = 255; // B
        pixel[3] = 255; // A
    }
    Ok(())
}
```

**Action:** Either implement a real CPU rasterizer (complex) or document this as a placeholder.

---

### 2.2 iFrame Support

**Location:** `dll/src/desktop/wr_translate2.rs`

**Status:** 🟡 Disabled

**Details:**
- Code exists but is explicitly disabled
- Comment: `// TODO: Re-enable iframe support when needed`
- The rendering path is present but inactive

**Code:**
```rust
// TODO: Re-enable iframe support when needed
// if let Some(iframe_pipeline_id) = pipeline_id {
//     // iframe rendering code...
// }
```

**Action:** Either complete the implementation or remove the dead code paths.

---

### 2.3 Image Rendering

**Location:** `dll/src/desktop/compositor2.rs`

**Status:** 🔴 Stubbed

**Details:**
- `DisplayListItem::Image` is not handled
- TODO comment confirms this is unimplemented
- Images in the DOM won't render

**Code:**
```rust
match item {
    DisplayListItem::Image { .. } => {
        // TODO: Implement image rendering with push_image
    }
    // ...
}
```

**Action:** Implement `push_image()` call or remove the TODO.

---

### 2.4 Scroll Frames

**Location:** `dll/src/desktop/compositor2.rs`

**Status:** 🔴 Stubbed

**Details:**
- `PushScrollFrame` and `PopScrollFrame` are not fully implemented
- TODO comment confirms this

**Code:**
```rust
DisplayListItem::PushScrollFrame { .. } => {
    // TODO: Implement scroll frames properly
}
DisplayListItem::PopScrollFrame => {
    // TODO: Implement scroll frames properly
}
```

**Action:** Complete implementation or document why it's not needed.

---

### 2.5 `translate_hit_test_result`

**Location:** `dll/src/desktop/wr_translate2.rs`

**Status:** 🔴 Stubbed

**Details:**
- Function is called but returns empty result
- WebRender hit-test data is not being properly translated
- May impact hit-testing accuracy

**Code:**
```rust
pub fn translate_hit_test_result(
    _wr_result: &webrender::HitTestResult,
) -> FullHitTest {
    FullHitTest::empty(None) // Always returns empty!
}
```

**Action:** Implement proper translation or document why it's not needed.

---

### 2.6 Wayland CPU Rendering

**Location:** `dll/src/desktop/shell2/linux/wayland/mod.rs`

**Status:** 🟡 Placeholder

**Details:**
- `CpuFallbackState::draw_blue()` just fills screen with solid blue
- Not a real renderer
- Sufficient for testing but not production

**Code:**
```rust
fn draw_blue(&self) {
    let size = (self.stride * self.height) as usize;
    let slice = unsafe { std::slice::from_raw_parts_mut(self.data, size) };
    for chunk in slice.chunks_exact_mut(4) {
        chunk[0] = 0xFF; // Blue
        chunk[1] = 0x00; // Green
        chunk[2] = 0x00; // Red
        chunk[3] = 0xFF; // Alpha (ARGB format)
    }
}
```

**Action:** Document as intentional placeholder for CPU fallback testing.

---

## Category 3: Incomplete Features

### 3.1 Wayland Timers

**Location:** `dll/src/desktop/shell2/linux/wayland/mod.rs`

**Status:** 🔴 Not Implemented

**Details:**
- `start_timer()` returns error: "Timers not yet implemented for Wayland"
- `stop_timer()` returns error
- Wayland backend cannot use Azul's timer system

**Code:**
```rust
fn start_timer(&mut self, _timer_id: usize, _timeout: Duration) -> Result<(), String> {
    Err("Timers not yet implemented for Wayland".into())
}

fn stop_timer(&mut self, _timer_id: usize) -> Result<(), String> {
    Err("Timers not yet implemented for Wayland".into())
}
```

**Action:** Implement using `timerfd_create()` or event loop timeout mechanism.

---

### 3.2 Wayland Monitor Detection

**Location:** `dll/src/desktop/shell2/linux/mod.rs`

**Status:** 🟡 Approximation

**Details:**
- Uses environment variables instead of Wayland protocol
- Not robust or accurate
- Should use `wl_output` listener

**Code:**
```rust
pub fn get_monitors() -> Vec<Monitor> {
    // TODO: Implement proper Wayland output protocol handling
    let width = std::env::var("WAYLAND_WIDTH").unwrap_or_else(|_| "1920".to_string());
    let height = std::env::var("WAYLAND_HEIGHT").unwrap_or_else(|_| "1080".to_string());
    // ...
}
```

**Action:** Implement proper `wl_output` protocol handling.

---

### 3.3 Window-Based Context Menus (Windows/macOS)

**Location:** 
- `dll/src/desktop/shell2/windows/mod.rs`
- `dll/src/desktop/shell2/macos/events.rs`

**Status:** 🟡 Planned But Not Implemented

**Details:**
- Code generates `WindowCreateOptions` correctly
- But doesn't actually spawn the window
- Logs TODO message instead
- X11/Wayland have full implementation

**Code:**
```rust
eprintln!("[Windows] Window-based context menu requested - requires multi-window support");
eprintln!("[macOS] Window-based context menu requested - requires multi-window support");
```

**Action:** Implement window creation queue or document as intentional limitation.

---

### 3.4 Windows DPI Detection

**Location:** `dll/src/desktop/shell2/windows/display.rs`

**Status:** 🟡 Hardcoded

**Details:**
- TODO comment: "Get actual DPI"
- Currently hardcodes `scale_factor = 1.0`
- Should query actual DPI from Windows

**Code:**
```rust
// TODO: Get actual DPI
let scale_factor = 1.0;
```

**Action:** Implement using `GetDpiForMonitor()` or similar API.

---

### 3.5 Wayland Visibility Control

**Location:** `dll/src/desktop/shell2/linux/wayland/mod.rs`

**Status:** 🟡 Not Implemented

**Details:**
- TODO comment in `set_visible()`
- Wayland visibility control requires `xdg_toplevel` methods
- Not yet wired up

**Code:**
```rust
fn set_visible(&mut self, _visible: bool) {
    // TODO: Wayland visibility control via xdg_toplevel methods
}
```

**Action:** Implement using `xdg_toplevel_set_minimized()` or similar.

---

### 3.6 Wayland Popup Dismissal

**Location:** `dll/src/desktop/shell2/linux/wayland/mod.rs`

**Status:** 🟡 Not Implemented

**Details:**
- Popup dismiss handler exists but doesn't signal application
- TODO comment: "Signal to application that popup was dismissed"

**Code:**
```rust
extern "C" fn popup_done_handler(...) {
    // TODO: Signal to application that popup was dismissed
}
```

**Action:** Add callback or event to notify when popup closes.

---

## Category 4: Duplicate Code (To Be Removed)

### 4.1 V2 Event Processing (4x Duplication)

**Locations:**
- `dll/src/desktop/shell2/macos/events.rs`
- `dll/src/desktop/shell2/windows/process.rs`
- `dll/src/desktop/shell2/linux/x11/events.rs`
- `dll/src/desktop/shell2/linux/wayland/mod.rs`

**Status:** 🔴 Duplication - ~2000 lines

**Action:** Refactor to `shell2/common/event_v2.rs` (see V2_UNIFICATION_PLAN.md)

---

### 4.2 Scrollbar Logic (3x Duplication)

**Locations:**
- `dll/src/desktop/shell2/macos/events.rs`
- `dll/src/desktop/shell2/windows/mod.rs`
- `dll/src/desktop/shell2/linux/x11/events.rs`

**Status:** 🔴 Duplication - ~600 lines

**Action:** Refactor to `shell2/common/scrollbar_v2.rs` (see V2_UNIFICATION_PLAN.md)

---

### 4.3 Layout Regeneration (4x Duplication)

**Locations:**
- `dll/src/desktop/shell2/macos/mod.rs`
- `dll/src/desktop/shell2/windows/mod.rs`
- `dll/src/desktop/shell2/linux/x11/mod.rs`
- `dll/src/desktop/shell2/linux/wayland/mod.rs`

**Status:** 🔴 Duplication - ~400 lines

**Action:** Refactor to `shell2/common/layout_v2.rs` (see V2_UNIFICATION_PLAN.md)

---

### 4.4 Menu Creation Logic (Duplication)

**Locations:**
- `dll/src/desktop/shell2/linux/x11/menu.rs`
- `dll/src/desktop/shell2/linux/wayland/menu.rs`
- `dll/src/desktop/menu.rs` (canonical version)

**Status:** 🟡 Partial Duplication

**Details:**
- X11 and Wayland have their own `create_menu_window_options()` functions
- `desktop/menu.rs` already has unified `show_menu()` function
- Platform-specific versions are redundant

**Action:** Remove platform-specific versions, use unified `show_menu()`.

---

## Summary Statistics

| Category | Count | Lines | Priority |
|----------|-------|-------|----------|
| **Dead Code** | 3 items | ~100 | Medium |
| **Stubbed Code** | 6 items | ~50 | Medium |
| **Incomplete Features** | 7 items | ~200 | Low-Medium |
| **Duplicate Code** | 4 items | ~3000 | **High** |
| **Total** | **20 items** | **~3350** | - |

---

## Recommended Actions

### High Priority (Do First)
1. ✅ **Refactor V2 event system** - Saves 2000 lines, improves maintainability
2. ✅ **Refactor scrollbar logic** - Saves 600 lines
3. ✅ **Refactor layout regeneration** - Saves 400 lines

### Medium Priority
4. Remove or wire up `csd_titlebar_doubleclick_callback`
5. Complete iFrame support or remove dead code
6. Implement image rendering in compositor
7. Implement scroll frames in compositor

### Low Priority
8. Implement Wayland timers
9. Implement proper Wayland monitor detection
10. Implement Windows DPI detection
11. Complete window-based context menus (Windows/macOS)

### Documentation Only
12. Document CPU compositor as intentional placeholder
13. Document Wayland CPU rendering as testing-only
14. Mark `msg_box_ok_cancel`/`msg_box_yes_no` as public API

---

## Conclusion

The codebase has **~3350 lines** of unused, stubbed, or duplicated code:

- **~3000 lines** can be eliminated through refactoring (V2 system unification)
- **~350 lines** are stubs/incomplete features that need completion or documentation
- **Most critical:** The duplicate V2 event system refactoring (saves 2000+ lines)

**Next Steps:**
1. Complete V2 unification refactoring (see V2_UNIFICATION_PLAN.md)
2. Review and complete/remove stubbed features
3. Document intentional placeholders
4. Remove genuinely dead code

---

**End of Inventory**

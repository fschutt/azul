# IME Callback Architecture Design

## Summary

This document describes the callback architecture for IME (Input Method Editor) positioning in Azul. Based on W3C UI Events specification research, IME activation is **automatic** - no manual activation required.

## Key Findings from W3C Spec

### IME Activation Pattern (Automatic)

From W3C UI Events § 3.8.7.1 and § 4.3.3:

1. User focuses an editable element
2. User presses a key (e.g., "s" for Japanese input)
3. **OS/IME automatically decides** whether to start composition
4. If composition starts → `compositionstart` event fires **after** `keydown`
5. Subsequent keys have `isComposing=true`

**Event sequence example (Japanese "市" city):**

```
1. keydown "s" (isComposing=false)
2. compositionstart         ← IME auto-activated!
3. beforeinput
4. compositionupdate "s"
5. input
6. keyup "s" (isComposing=true)
7. keydown "i" (isComposing=true)
8. compositionupdate "し"
9. ...
```

### Critical Insight

- **NO** manual IME activation required
- **NO** explicit "enable IME" API call
- OS decides based on:
  - Active keyboard layout (Japanese, Chinese, Korean, etc.)
  - Input context (focused element type)
  - User preferences

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Event Flow                            │
└─────────────────────────────────────────────────────────────┘

1. Focus Event (HTML <input>, contenteditable, etc.)
   ↓
   ├─→ OnFocus Callback
   │   └─→ Detect focused element
   │       └─→ Get cursor position (if available)
   │           └─→ Set initial ime_position
   │
2. Layout Pass
   ↓
   ├─→ Post-Layout Callback
   │   └─→ If focused element exists:
   │       ├─→ Get cursor screen position
   │       ├─→ Update ime_position
   │       └─→ Sync to OS (ImmSetCompositionWindow, etc.)
   │
3. KeyDown Event (e.g., "s")
   ↓
   ├─→ OS decides: Start composition? (YES)
   │
4. CompositionStart Event
   ↓
   ├─→ OnCompositionStart Callback
   │   ├─→ Verify ime_position is set
   │   ├─→ If not set: Calculate from cursor
   │   └─→ Update OS IME window position
   │
5. CompositionUpdate Events
   ↓
   ├─→ OnCompositionUpdate Callback
   │   ├─→ Store composition string
   │   └─→ Trigger re-layout (for inline rendering)
   │
6. Post-Layout (again)
   ↓
   ├─→ Render composition inline
   │   ├─→ Create GlyphRun with is_ime_preview=true
   │   └─→ Auto-underline applied
   │
7. CompositionEnd Event
   ↓
   └─→ OnCompositionEnd Callback
       └─→ Clear composition state
```

## Callback Points

### 1. OnFocus (Early Preparation)

**When:** Element receives focus  
**Purpose:** Initialize IME position BEFORE composition might start  
**Implementation Location:** `handle_focus_event()` in platform shell

```rust
fn handle_focus_event(&mut self, focused_element: &FocusedElement) {
    // Get cursor position from layout
    if let Some(cursor_pos) = self.get_cursor_position(focused_element) {
        // Convert to screen coordinates
        let screen_pos = self.local_to_screen(cursor_pos);
        
        // Store in window state
        self.current_window_state.ime_position = Some(screen_pos);
        
        // Optionally: Pre-notify OS (Windows only)
        #[cfg(target_os = "windows")]
        self.set_ime_composition_window(screen_pos);
    }
}
```

**Timing:** Synchronous, during focus event handling

### 2. Post-Layout (Primary IME Update)

**When:** After layout pass completes  
**Purpose:** Update IME position with accurate cursor coordinates  
**Implementation Location:** `after_layout()` in layout solver

```rust
fn after_layout(&mut self, window_id: WindowId) {
    // Get focused element from DOM
    if let Some(focused_node) = self.get_focused_text_input() {
        // Get cursor position from text cursor manager
        if let Some(cursor) = self.text_cursor_manager.get_active_cursor(focused_node) {
            // Calculate screen position
            let local_pos = cursor.get_local_position();
            let screen_pos = self.window_local_to_screen(window_id, local_pos);
            
            // Update window state
            let window_state = &mut self.windows[window_id].current_window_state;
            window_state.ime_position = Some(screen_pos);
            
            // Sync to OS
            self.sync_ime_position_to_os(window_id);
        }
    }
}
```

**Timing:** End of layout pass, before rendering

### 3. OnCompositionStart (Verification)

**When:** `compositionstart` event fires  
**Purpose:** Ensure IME position is set, update if needed  
**Implementation Location:** Platform-specific IME handlers

```rust
// Windows: WM_IME_STARTCOMPOSITION
WM_IME_STARTCOMPOSITION => {
    window.on_composition_start();
    (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
}

impl MacOSWindow {
    fn on_composition_start(&mut self) {
        // Verify ime_position is set
        if self.current_window_state.ime_position.is_none() {
            // Fallback: Calculate from current focus
            if let Some(pos) = self.calculate_ime_position_from_focus() {
                self.current_window_state.ime_position = Some(pos);
            }
        }
        
        // Update OS IME window
        self.sync_ime_position_to_os();
    }
}
```

**Timing:** Synchronous, during composition event handling

### 4. OnCompositionUpdate (State Management)

**When:** `compositionupdate` event fires  
**Purpose:** Store composition string, trigger re-layout for inline rendering  
**Implementation Location:** Platform-specific IME handlers

```rust
// Windows: WM_IME_COMPOSITION with GCS_COMPSTR
WM_IME_COMPOSITION if lparam & GCS_COMPSTR != 0 => {
    if let Some(composition_str) = extract_composition_string(hwnd) {
        window.ime_composition = Some(composition_str);
        
        // Trigger re-layout to render composition inline
        window.previous_window_state = Some(window.current_window_state.clone());
        let result = window.process_window_events_recursive_v2(0);
        
        if !matches!(result, ProcessEventResult::DoNothing) {
            (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
        }
    }
}
```

**Timing:** Synchronous, triggers async re-layout

### 5. OnCompositionEnd (Cleanup)

**When:** `compositionend` event fires  
**Purpose:** Clear composition state  
**Implementation Location:** Platform-specific IME handlers

```rust
WM_IME_ENDCOMPOSITION => {
    window.ime_composition = None;
    // ime_position remains set for future compositions
    (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
}
```

**Timing:** Synchronous

## Data Flow

### WindowState Extensions

```rust
pub struct WindowState {
    // ... existing fields ...
    
    /// IME composition window position in screen coordinates
    /// Set on focus and updated after each layout pass
    pub ime_position: Option<LogicalPosition>,
    
    /// Current IME composition string (if any)
    /// Used for inline rendering with automatic underline
    pub ime_composition: Option<String>,
}
```

**Already Implemented:** Both fields exist (grep confirmed)

### Cross-Platform Sync

```rust
impl WindowState {
    /// Sync ime_position to OS-specific IME APIs
    fn sync_ime_position_to_os(&self, platform: &mut PlatformWindow) {
        if let Some(pos) = self.ime_position {
            #[cfg(target_os = "windows")]
            platform.set_ime_composition_window(pos);
            
            #[cfg(target_os = "macos")]
            platform.update_text_input_rect(pos);
            
            #[cfg(target_os = "linux")]
            platform.set_ime_cursor_location(pos);
        }
    }
}
```

## Platform-Specific Implementation

### Windows (IMM32)

**API:** `ImmSetCompositionWindow`

```rust
fn set_ime_composition_window(&self, pos: LogicalPosition) {
    if let Some(ref imm32) = self.win32.imm32 {
        unsafe {
            let hwnd = self.hwnd;
            let himc = (imm32.ImmGetContext)(hwnd);
            
            if !himc.is_null() {
                let mut comp_form = COMPOSITIONFORM {
                    dwStyle: CFS_POINT,
                    ptCurrentPos: POINT {
                        x: pos.x as i32,
                        y: pos.y as i32,
                    },
                    rcArea: RECT::default(),
                };
                
                (imm32.ImmSetCompositionWindow)(himc, &comp_form);
                (imm32.ImmReleaseContext)(hwnd, himc);
            }
        }
    }
}
```

### macOS (NSTextInputClient)

**API:** `firstRectForCharacterRange:`

```rust
#[unsafe(method(firstRectForCharacterRange:actualRange:))]
fn first_rect_for_character_range(
    &self,
    _range: NSRange,
    _actual_range: *mut NSRange,
) -> NSRect {
    // Get ime_position from window state
    if let Some(window_ptr) = self.get_window_ptr() {
        unsafe {
            let window = &*(window_ptr as *const MacOSWindow);
            if let Some(pos) = window.current_window_state.ime_position {
                return NSRect {
                    origin: NSPoint { x: pos.x, y: pos.y },
                    size: NSSize { width: 0.0, height: 20.0 }, // Cursor height
                };
            }
        }
    }
    
    NSRect::ZERO
}
```

### Linux (Wayland/X11)

**API:** `gtk_im_context_set_cursor_location` or XIM

```rust
fn set_ime_cursor_location(&mut self, pos: LogicalPosition) {
    #[cfg(feature = "wayland")]
    if let Some(ref mut im_context) = self.im_context {
        let rect = GdkRectangle {
            x: pos.x as i32,
            y: pos.y as i32,
            width: 1,
            height: 20,
        };
        gtk_im_context_set_cursor_location(im_context, &rect);
    }
}
```

## Cursor Position Calculation

### From TextCursorManager

```rust
impl LayoutWindow {
    /// Get cursor position for focused text input
    fn get_ime_cursor_position(&self) -> Option<LogicalPosition> {
        // Get focused node
        let focused_node = self.focused_node?;
        
        // Check if it's a text input
        if !self.is_text_input_node(focused_node) {
            return None;
        }
        
        // Get cursor from text cursor manager
        let cursor = self.text_cursor_manager.get_active_cursor(focused_node)?;
        
        // Get cursor local position
        let local_pos = cursor.get_local_position()?;
        
        // Convert to window coordinates
        let node_offset = self.get_node_screen_offset(focused_node)?;
        
        Some(LogicalPosition {
            x: node_offset.x + local_pos.x,
            y: node_offset.y + local_pos.y + cursor.height, // Bottom of cursor
        })
    }
}
```

### Fallback: From Caret Position

If `TextCursorManager` doesn't have cursor info:

```rust
fn calculate_ime_position_from_focus(&self) -> Option<LogicalPosition> {
    let focused_node = self.focused_node?;
    let node_rect = self.get_node_screen_rect(focused_node)?;
    
    // Default: Top-left corner of focused element
    Some(LogicalPosition {
        x: node_rect.origin.x,
        y: node_rect.origin.y + node_rect.size.height, // Below element
    })
}
```

## Integration Points

### 1. Focus Management

**File:** `dll/src/desktop/shell2/*/mod.rs`  
**Function:** Focus event handlers

```rust
// Add ime_position initialization
OnFocus => {
    if let Some(pos) = self.calculate_ime_cursor_position() {
        self.current_window_state.ime_position = Some(pos);
    }
}
```

### 2. Layout Completion

**File:** `layout/src/solver3/mod.rs`  
**Function:** `solve_layout()` or similar

```rust
pub fn solve_layout(&mut self, window_id: WindowId) -> LayoutResult {
    // ... existing layout logic ...
    
    // After layout: Update IME position
    self.update_ime_position(window_id);
    
    result
}
```

### 3. Platform IME Handlers

**Files:**
- `dll/src/desktop/shell2/windows/mod.rs` - WM_IME_* handlers
- `dll/src/desktop/shell2/macos/mod.rs` - NSTextInputClient methods
- `dll/src/desktop/shell2/linux/mod.rs` - GTK IM context callbacks

Add `sync_ime_position_to_os()` calls after `ime_position` updates.

### 4. Display List Generation

**File:** `layout/src/solver3/display_list.rs`  
**Function:** `paint_inline_content()` or similar

```rust
// Render IME composition inline
if let Some(ref composition) = window.ime_composition {
    let glyph_run = self.shape_text(
        composition,
        &current_style,
        true, // is_ime_preview
    );
    
    // Position at cursor
    let cursor_pos = self.get_cursor_position()?;
    
    display_list.push(DisplayListItem::Text(TextDisplayItem {
        glyphs: glyph_run,
        position: cursor_pos,
        // ... other fields ...
    }));
    
    // Underline automatically applied via is_ime_preview flag
}
```

## Edge Cases & Error Handling

### 1. No Focused Element

```rust
if self.current_window_state.ime_position.is_none() {
    // Fallback: Use last known mouse position
    self.current_window_state.ime_position = self.last_mouse_position;
}
```

### 2. Multi-Window Scenarios

```rust
fn sync_ime_position_to_os(&self, window_id: WindowId) {
    // Only sync if this window has focus
    if !self.window_has_focus(window_id) {
        return;
    }
    
    // ... sync logic ...
}
```

### 3. Layout Invalidation During Composition

```rust
// Prevent infinite layout loops
if self.is_composing && self.composition_changed() {
    self.schedule_relayout(); // Async
} else {
    self.relayout_immediate(); // Sync
}
```

### 4. Platform Differences

```rust
#[cfg(target_os = "windows")]
const IME_WINDOW_OFFSET_Y: f32 = 0.0; // At cursor

#[cfg(target_os = "macos")]
const IME_WINDOW_OFFSET_Y: f32 = 20.0; // Below cursor

#[cfg(target_os = "linux")]
const IME_WINDOW_OFFSET_Y: f32 = 5.0; // Slightly below
```

## Performance Considerations

### 1. Update Frequency

- **OnFocus:** Once per focus change (low frequency)
- **Post-Layout:** Once per layout pass (medium frequency)
- **OnComposition:** Only during active composition (low frequency)

### 2. Optimization: Dirty Checking

```rust
fn should_update_ime_position(&self, new_pos: LogicalPosition) -> bool {
    match self.current_window_state.ime_position {
        Some(old_pos) => {
            // Only update if moved > 1px
            let dx = (new_pos.x - old_pos.x).abs();
            let dy = (new_pos.y - old_pos.y).abs();
            dx > 1.0 || dy > 1.0
        }
        None => true,
    }
}
```

### 3. Async Updates

```rust
// Don't block rendering on IME updates
self.schedule_ime_position_update(pos); // Async
```

## Testing Strategy

### Unit Tests

```rust
#[test]
fn test_ime_position_on_focus() {
    let mut window = create_test_window();
    window.handle_focus_event(/* ... */);
    assert!(window.current_window_state.ime_position.is_some());
}

#[test]
fn test_ime_position_after_layout() {
    let mut layout = create_test_layout();
    layout.solve_layout(window_id);
    let pos = layout.windows[window_id].ime_position;
    assert_eq!(pos, Some(LogicalPosition { x: 10.0, y: 20.0 }));
}
```

### Integration Tests

1. **Japanese Input:** Type "shi" → "し" → "詩" / "市"
2. **Chinese Pinyin:** Type "ni" → "你" / "泥" / "尼"
3. **Korean Hangul:** Type "ㄱ" → "가" → "강"
4. **Dead Keys:** Type "^" + "e" → "ê"

### Manual Testing Checklist

- [ ] IME window appears at cursor position
- [ ] IME window moves with cursor (arrow keys, mouse)
- [ ] IME window updates after scrolling
- [ ] Multi-line input: Cursor on line 2, IME at correct position
- [ ] Window resize: IME position recalculated
- [ ] Multi-monitor: IME on correct screen

## Implementation Phases

### Phase 1: Core Infrastructure (2-4 hours)

1. Add `sync_ime_position_to_os()` to WindowState
2. Implement platform-specific IME window positioning APIs
3. Add cursor position calculation helpers

### Phase 2: Callback Integration (3-5 hours)

1. OnFocus callback: Initialize ime_position
2. Post-Layout callback: Update ime_position
3. OnCompositionStart: Verify and sync ime_position

### Phase 3: Inline Rendering (4-6 hours)

1. Detect `window.ime_composition.is_some()`
2. Shape composition text with `is_ime_preview=true`
3. Insert into display list at cursor position
4. Auto-underline already works!

### Phase 4: Testing & Polish (2-3 hours)

1. Test Japanese, Chinese, Korean input
2. Test dead keys (French, German accents)
3. Fix edge cases (no focus, multi-window)

**Total Estimate:** 11-18 hours

## Success Criteria

- [ ] IME candidate window appears at cursor position (all platforms)
- [ ] IME window follows cursor during navigation
- [ ] Composition string renders inline with underline
- [ ] No crashes or performance regressions
- [ ] Works with major IME systems:
  - Windows: Microsoft IME, Google Japanese Input
  - macOS: Built-in Japanese/Chinese IME
  - Linux: ibus, fcitx, uim

## References

- **W3C UI Events:** https://w3c.github.io/uievents/#events-compositionevents
- **Windows IMM32:** https://docs.microsoft.com/en-us/windows/win32/intl/imm-functions
- **macOS NSTextInputClient:** https://developer.apple.com/documentation/appkit/nstextinputclient
- **Linux GTK IM:** https://docs.gtk.org/gtk3/class.IMContext.html

## Next Steps

1. Mark "Research browser IME activation patterns" as COMPLETE ✅
2. Mark "Design IME callback architecture" as COMPLETE ✅
3. Start "Implement IME position from cursor" (Phase 1)
4. Test with real IME systems

---

**Status:** Design complete, ready for implementation  
**Author:** GitHub Copilot  
**Date:** 2025-11-04

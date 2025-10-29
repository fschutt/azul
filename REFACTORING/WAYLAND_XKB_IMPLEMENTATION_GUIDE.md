# Wayland XKB Keyboard Translation Implementation Guide

## Current Status
The Wayland keyboard handling has the XKB infrastructure in place but `handle_key()` is stubbed out with TODOs.

## XKB Functions Available
From `dll/src/desktop/shell2/linux/x11/dlopen.rs` (re-exported for Wayland):

```rust
pub struct Xkb {
    pub xkb_context_new: unsafe extern "C" fn(flags: u32) -> *mut xkb_context,
    pub xkb_context_unref: unsafe extern "C" fn(context: *mut xkb_context),
    pub xkb_keymap_new_from_string: unsafe extern "C" fn(*mut xkb_context, *const i8, u32, u32) -> *mut xkb_keymap,
    pub xkb_keymap_unref: unsafe extern "C" fn(keymap: *mut xkb_keymap),
    pub xkb_state_new: unsafe extern "C" fn(keymap: *mut xkb_keymap) -> *mut xkb_state,
    pub xkb_state_unref: unsafe extern "C" fn(state: *mut xkb_state),
    pub xkb_state_update_mask: unsafe extern "C" fn(*mut xkb_state, u32, u32, u32, u32, u32, u32) -> u32,
    pub xkb_state_key_get_one_sym: unsafe extern "C" fn(*mut xkb_state, u32) -> u32,
    pub xkb_state_key_get_utf8: unsafe extern "C" fn(*mut xkb_state, u32, *mut i8, usize) -> i32,
}
```

## Current Wayland Keyboard State
Location: `dll/src/desktop/shell2/linux/wayland/mod.rs`

```rust
pub struct WaylandWindow {
    // ...
    keyboard_state: events::KeyboardState,
    // ...
}

// In events.rs:
pub(super) struct KeyboardState {
    pub(super) context: *mut xkb_context,
    pub(super) keymap: *mut xkb_keymap,
    pub(super) state: *mut xkb_state,
}
```

**Initialization happens in:** `keyboard_keymap_handler()` in `events.rs:180-215`
- Receives keymap from compositor as file descriptor
- Creates XKB context, keymap, and state
- **This is already working!**

## Implementation Plan for `handle_key()`

### Current Stub:
```rust
pub fn handle_key(&mut self, key: u32, state: u32) {
    use azul_core::window::{OptionChar, OptionVirtualKeyCode};
    
    self.current_window_state.keyboard_state.current_char = OptionChar::None;
    self.current_window_state.keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::None;
    
    // TODO: Use XKB to translate key to VirtualKeyCode
    // TODO: Update modifier states (shift, ctrl, alt, super)
    
    self.frame_needs_regeneration = true;
}
```

### Full Implementation Needed:

```rust
pub fn handle_key(&mut self, key: u32, state: u32) {
    use azul_core::window::{OptionChar, OptionVirtualKeyCode, VirtualKeyCode};
    
    // Only process key press events (state == 1)
    let is_pressed = state == 1;
    
    // XKB uses keycode = evdev_keycode + 8
    let xkb_keycode = key + 8;
    
    // Get XKB state
    let xkb_state = self.keyboard_state.state;
    if xkb_state.is_null() {
        return;
    }
    
    // Get keysym (symbolic key identifier)
    let keysym = unsafe { (self.xkb.xkb_state_key_get_one_sym)(xkb_state, xkb_keycode) };
    
    // Translate keysym to VirtualKeyCode
    let virtual_keycode = translate_keysym_to_virtual_keycode(keysym);
    self.current_window_state.keyboard_state.current_virtual_keycode = 
        OptionVirtualKeyCode::Some(virtual_keycode);
    
    // Get UTF-8 character (if printable)
    if is_pressed {
        let mut buffer = [0i8; 32];
        let len = unsafe {
            (self.xkb.xkb_state_key_get_utf8)(
                xkb_state,
                xkb_keycode,
                buffer.as_mut_ptr(),
                buffer.len(),
            )
        };
        
        if len > 0 && len < buffer.len() as i32 {
            let utf8_str = unsafe {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                    buffer.as_ptr() as *const u8,
                    len as usize,
                ))
            };
            if let Some(ch) = utf8_str.chars().next() {
                self.current_window_state.keyboard_state.current_char = 
                    OptionChar::Some(ch);
            }
        }
    }
    
    // Update modifier states
    // (Modifier state is tracked via keyboard_modifiers_handler)
    
    self.frame_needs_regeneration = true;
}
```

### Keysym Translation Function:

Create a new function `translate_keysym_to_virtual_keycode()`:

```rust
fn translate_keysym_to_virtual_keycode(keysym: u32) -> VirtualKeyCode {
    // XKB keysyms are defined in <xkbcommon/xkbcommon-keysyms.h>
    // Common keysyms:
    const XKB_KEY_Escape: u32 = 0xff1b;
    const XKB_KEY_Return: u32 = 0xff0d;
    const XKB_KEY_Tab: u32 = 0xff09;
    const XKB_KEY_BackSpace: u32 = 0xff08;
    const XKB_KEY_Delete: u32 = 0xffff;
    
    const XKB_KEY_Left: u32 = 0xff51;
    const XKB_KEY_Up: u32 = 0xff52;
    const XKB_KEY_Right: u32 = 0xff53;
    const XKB_KEY_Down: u32 = 0xff54;
    
    const XKB_KEY_F1: u32 = 0xffbe;
    const XKB_KEY_F2: u32 = 0xffbf;
    // ... etc
    
    match keysym {
        XKB_KEY_Escape => VirtualKeyCode::Escape,
        XKB_KEY_Return => VirtualKeyCode::Return,
        XKB_KEY_Tab => VirtualKeyCode::Tab,
        XKB_KEY_BackSpace => VirtualKeyCode::Back,
        XKB_KEY_Delete => VirtualKeyCode::Delete,
        XKB_KEY_Left => VirtualKeyCode::Left,
        XKB_KEY_Up => VirtualKeyCode::Up,
        XKB_KEY_Right => VirtualKeyCode::Right,
        XKB_KEY_Down => VirtualKeyCode::Down,
        XKB_KEY_F1 => VirtualKeyCode::F1,
        XKB_KEY_F2 => VirtualKeyCode::F2,
        // For letters a-z (keysyms 0x0061-0x007a)
        0x0061..=0x007a => {
            let offset = keysym - 0x0061;
            VirtualKeyCode::Key(b'a' + offset as u8)
        }
        // For numbers 0-9 (keysyms 0x0030-0x0039)
        0x0030..=0x0039 => {
            let offset = keysym - 0x0030;
            VirtualKeyCode::Key(b'0' + offset as u8)
        }
        _ => VirtualKeyCode::Key(0), // Unknown key
    }
}
```

### Modifier State Tracking:

The `keyboard_modifiers_handler()` in `events.rs` needs to be implemented:

```rust
pub(super) extern "C" fn keyboard_modifiers_handler(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    mods_depressed: u32,
    mods_latched: u32,
    mods_locked: u32,
    group: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    
    // Update XKB state
    if !window.keyboard_state.state.is_null() {
        unsafe {
            (window.xkb.xkb_state_update_mask)(
                window.keyboard_state.state,
                mods_depressed,
                mods_latched,
                mods_locked,
                0,
                0,
                group,
            );
        };
    }
    
    // Update KeyboardState modifiers
    // Need to query XKB state for specific modifiers
    // This requires additional XKB functions like xkb_state_mod_name_is_active
    // For now, we can track basic state
    window.frame_needs_regeneration = true;
}
```

## Testing
1. Run Azul app on Wayland
2. Test keyboard input:
   - Text input (letters, numbers, symbols)
   - Special keys (Escape, Return, arrows)
   - Modifier keys (Shift, Ctrl, Alt)
   - Function keys (F1-F12)

## References
- XKB documentation: https://xkbcommon.org/doc/current/
- Wayland keyboard protocol: https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_keyboard
- Keysym definitions: `/usr/include/xkbcommon/xkbcommon-keysyms.h`

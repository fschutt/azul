//! X11 Event handling - converts XEvent to Azul events and window state changes.
//! Includes full IME (XIM) support.

use super::defines::*;
use super::dlopen::Xlib;
use super::X11Window;
use azul_core::window::{
    CursorPosition, FullWindowState, KeyboardState, MouseButton, MouseState, VirtualKeyCode,
};
use azul_css::Au;
use std::ffi::{CStr, CString};
use std::rc::Rc;

// -- IME Support (X Input Method) --

pub(super) struct ImeManager {
    xlib: Rc<Xlib>,
    xim: XIM,
    xic: XIC,
}

impl ImeManager {
    pub(super) fn new(xlib: &Rc<Xlib>, display: *mut Display, window: Window) -> Option<Self> {
        unsafe {
            // Set the locale. This is crucial for XIM to work correctly.
            let locale = CString::new("").unwrap();
            (xlib.XSetLocaleModifiers)(locale.as_ptr());

            let xim = (xlib.XOpenIM)(display, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());
            if xim.is_null() {
                eprintln!("[X11 IME] Could not open input method. IME will not be available.");
                return None;
            }

            // We want to handle pre-edit drawing ourselves.
            let style = CString::new("inputStyle").unwrap();
            let preedit = CString::new("preeditStyle").unwrap();
            let status = CString::new("statusStyle").unwrap();
            let client_window = CString::new("clientWindow").unwrap();

            let xic = (xlib.XCreateIC)(
                xim,
                style.as_ptr(),
                XIMPreeditNothing | XIMStatusNothing,
                client_window.as_ptr(),
                window,
                std::ptr::null_mut() as *const i8, // Sentinel
            );

            if xic.is_null() {
                eprintln!("[X11 IME] Could not create input context. IME will not be available.");
                (xlib.XCloseIM)(xim);
                return None;
            }
            Some(Self {
                xlib: xlib.clone(),
                xim,
                xic,
            })
        }
    }

    /// Filters an event through the IME.
    /// Returns `true` if the event was consumed by the IME.
    pub(super) fn filter_event(&self, event: &mut XEvent) -> bool {
        unsafe { (self.xlib.XFilterEvent)(event, 0) != 0 }
    }

    /// Translates a key event into a character and a keysym, considering the IME.
    pub(super) fn lookup_string(&self, event: &mut XKeyEvent) -> (Option<String>, Option<KeySym>) {
        let mut keysym: KeySym = 0;
        let mut status: i32 = 0;
        let mut buffer: [i8; 16] = [0; 16];

        let count = unsafe {
            (self.xlib.XmbLookupString)(
                self.xic,
                event,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                &mut keysym,
                &mut status,
            )
        };

        let chars = if count > 0 {
            Some(unsafe { CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned() })
        } else {
            None
        };

        let keysym = if keysym != 0 { Some(keysym) } else { None };

        (chars, keysym)
    }
}

impl Drop for ImeManager {
    fn drop(&mut self) {
        unsafe {
            (self.xlib.XDestroyIC)(self.xic);
            (self.xlib.XCloseIM)(self.xim);
        }
    }
}

// -- Standard Event Handlers --

pub(super) fn handle_mouse_button(window: &mut X11Window, event: &XButtonEvent) {
    let is_down = event.type_ == ButtonPress;
    let button = match event.button {
        1 => MouseButton::Left,
        2 => MouseButton::Middle,
        3 => MouseButton::Right,
        4 => {
            if is_down { window.current_window_state.mouse_state.scroll_y = Au::from_px(1.0).into(); }
            return;
        }
        5 => {
            if is_down { window.current_window_state.mouse_state.scroll_y = Au::from_px(-1.0).into(); }
            return;
        }
        _ => MouseButton::Other(event.button as u8),
    };

    let position = azul_core::geom::LogicalPosition::new(event.x as f32, event.y as f32);
    window.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

    match button {
        MouseButton::Left => window.current_window_state.mouse_state.left_down = is_down,
        MouseButton::Right => window.current_window_state.mouse_state.right_down = is_down,
        MouseButton::Middle => window.current_window_state.mouse_state.middle_down = is_down,
        _ => {}
    }
}

pub(super) fn handle_mouse_move(window: &mut X11Window, event: &XMotionEvent) {
    let position = azul_core::geom::LogicalPosition::new(event.x as f32, event.y as f32);
    window.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);
}

pub(super) fn handle_mouse_crossing(window: &mut X11Window, event: &XCrossingEvent) {
    let position = azul_core::geom::LogicalPosition::new(event.x as f32, event.y as f32);
    if event.type_ == EnterNotify {
        window.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);
    } else if event.type_ == LeaveNotify {
        window.current_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow(position);
    }
}

pub(super) fn handle_keyboard(window: &mut X11Window, event: &mut XKeyEvent) {
    let is_down = event.type_ == KeyPress;

    let (char_str, keysym) = if let Some(ime) = &window.ime_manager {
        ime.lookup_string(event)
    } else {
        // Fallback for when IME is not available
        let mut keysym: KeySym = 0;
        let mut buffer = [0; 16];
        unsafe {
            (window.xlib.XLookupString)(event, buffer.as_mut_ptr(), buffer.len() as i32, &mut keysym, std::ptr::null_mut());
        }
        let chars = if keysym != 0 { unsafe { CStr::from_ptr(buffer.as_ptr()).to_string_lossy().into_owned() } } else { String::new() };
        (Some(chars), Some(keysym))
    };
    
    if let Some(vk) = keysym.and_then(keysym_to_virtual_keycode) {
        if is_down {
            window.current_window_state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
            window.current_window_state.keyboard_state.current_virtual_keycode = Some(vk).into();
        } else {
            window.current_window_state.keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
            window.current_window_state.keyboard_state.current_virtual_keycode = None.into();
        }
    }

    if is_down {
        if let Some(s) = char_str {
            if let Some(c) = s.chars().next() {
                window.current_window_state.keyboard_state.current_char = Some(c as u32).into();
            }
        }
    } else {
        window.current_window_state.keyboard_state.current_char = None.into();
    }
}

fn keysym_to_virtual_keycode(keysym: KeySym) -> Option<VirtualKeyCode> {
    // This is a partial mapping based on X11/keysymdef.h
    match keysym as u32 {
        XK_BackSpace => Some(VirtualKeyCode::Back),
        XK_Tab => Some(VirtualKeyCode::Tab),
        XK_Return => Some(VirtualKeyCode::Return),
        XK_Pause => Some(VirtualKeyCode::Pause),
        XK_Scroll_Lock => Some(VirtualKeyCode::Scroll),
        XK_Escape => Some(VirtualKeyCode::Escape),
        XK_Home => Some(VirtualKeyCode::Home),
        XK_Left => Some(VirtualKeyCode::Left),
        XK_Up => Some(VirtualKeyCode::Up),
        XK_Right => Some(VirtualKeyCode::Right),
        XK_Down => Some(VirtualKeyCode::Down),
        XK_Page_Up => Some(VirtualKeyCode::PageUp),
        XK_Page_Down => Some(VirtualKeyCode::PageDown),
        XK_End => Some(VirtualKeyCode::End),
        XK_Insert => Some(VirtualKeyCode::Insert),
        XK_Delete => Some(VirtualKeyCode::Delete),
        XK_space => Some(VirtualKeyCode::Space),
        XK_0 => Some(VirtualKeyCode::Key0),
        XK_1 => Some(VirtualKeyCode::Key1),
        XK_2 => Some(VirtualKeyCode::Key2),
        XK_3 => Some(VirtualKeyCode::Key3),
        XK_4 => Some(VirtualKeyCode::Key4),
        XK_5 => Some(VirtualKeyCode::Key5),
        XK_6 => Some(VirtualKeyCode::Key6),
        XK_7 => Some(VirtualKeyCode::Key7),
        XK_8 => Some(VirtualKeyCode::Key8),
        XK_9 => Some(VirtualKeyCode::Key9),
        XK_a | XK_A => Some(VirtualKeyCode::A),
        XK_b | XK_B => Some(VirtualKeyCode::B),
        XK_c | XK_C => Some(VirtualKeyCode::C),
        XK_d | XK_D => Some(VirtualKeyCode::D),
        XK_e | XK_E => Some(VirtualKeyCode::E),
        XK_f | XK_F => Some(VirtualKeyCode::F),
        XK_g | XK_G => Some(VirtualKeyCode::G),
        XK_h | XK_H => Some(VirtualKeyCode::H),
        XK_i | XK_I => Some(VirtualKeyCode::I),
        XK_j | XK_J => Some(VirtualKeyCode::J),
        XK_k | XK_K => Some(VirtualKeyCode::K),
        XK_l | XK_L => Some(VirtualKeyCode::L),
        XK_m | XK_M => Some(VirtualKeyCode::M),
        XK_n | XK_N => Some(VirtualKeyCode::N),
        XK_o | XK_O => Some(VirtualKeyCode::O),
        XK_p | XK_P => Some(VirtualKeyCode::P),
        XK_q | XK_Q => Some(VirtualKeyCode::Q),
        XK_r | XK_R => Some(VirtualKeyCode::R),
        XK_s | XK_S => Some(VirtualKeyCode::S),
        XK_t | XK_T => Some(VirtualKeyCode::T),
        XK_u | XK_U => Some(VirtualKeyCode::U),
        XK_v | XK_V => Some(VirtualKeyCode::V),
        XK_w | XK_W => Some(VirtualKeyCode::W),
        XK_x | XK_X => Some(VirtualKeyCode::X),
        XK_y | XK_Y => Some(VirtualKeyCode::Y),
        XK_z | XK_Z => Some(VirtualKeyCode::Z),
        XK_F1 => Some(VirtualKeyCode::F1),
        XK_F2 => Some(VirtualKeyCode::F2),
        XK_F3 => Some(VirtualKeyCode::F3),
        XK_F4 => Some(VirtualKeyCode::F4),
        XK_F5 => Some(VirtualKeyCode::F5),
        XK_F6 => Some(VirtualKeyCode::F6),
        XK_F7 => Some(VirtualKeyCode::F7),
        XK_F8 => Some(VirtualKeyCode::F8),
        XK_F9 => Some(VirtualKeyCode::F9),
        XK_F10 => Some(VirtualKeyCode::F10),
        XK_F11 => Some(VirtualKeyCode::F11),
        XK_F12 => Some(VirtualKeyCode::F12),
        XK_Shift_L => Some(VirtualKeyCode::LShift),
        XK_Shift_R => Some(VirtualKeyCode::RShift),
        XK_Control_L => Some(VirtualKeyCode::LControl),
        XK_Control_R => Some(VirtualKeyCode::RControl),
        XK_Alt_L => Some(VirtualKeyCode::LAlt),
        XK_Alt_R => Some(VirtualKeyCode::RAlt),
        XK_Super_L => Some(VirtualKeyCode::LWin),
        XK_Super_R => Some(VirtualKeyCode::RWin),
        _ => None,
    }
}
//! Client-side decorations for X11 windows.

use super::dlopen::Xlib;
use super::defines::*;
use azul_core::dom::{Dom, DomVec};
use azul_css::props::basic::color::ColorU;
use std::rc::Rc;
use azul_core::callbacks::Update;

const TITLE_BAR_HEIGHT: f32 = 30.0;
const BUTTON_WIDTH: f32 = 45.0;

/// State for the client-side decorations.
#[derive(Default)]
pub struct Decorations {
    pub is_dragging: bool,
    pub drag_start_pos: (i32, i32),
    pub close_button_hover: bool,
    pub maximize_button_hover: bool,
    pub minimize_button_hover: bool,
}

/// Returns a DOM for rendering the title bar.
pub fn render_decorations(title: &str, state: &Decorations) -> Dom {
    fn button_style(is_hovered: bool) -> Dom {
        let bg_color = if is_hovered { ColorU::new(220, 70, 70, 255) } else { ColorU::new(50, 50, 50, 255) };
        Dom::div()
            .with_style("width", LayoutWidth::px(BUTTON_WIDTH))
            .with_style("height", LayoutHeight::px(TITLE_BAR_HEIGHT))
            .with_style("background-color", bg_color)
            .with_style("justify-content", LayoutJustifyContent::Center)
            .with_style("align-items", LayoutAlignItems::Center)
    }

    Dom::div()
        .with_id("csd-title-bar")
        .with_style("width", LayoutWidth::percentage(100.0))
        .with_style("height", LayoutHeight::px(TITLE_BAR_HEIGHT))
        .with_style("background-color", ColorU::new(45, 45, 45, 255))
        .with_style("flex-direction", LayoutFlexDirection::Row)
        .with_style("align-items", LayoutAlignItems::Center)
        .with_children(DomVec::from_vec(vec![
            Dom::div() // Title
                .with_style("flex-grow", 1.0)
                .with_style("padding-left", 10.0)
                .with_child(Dom::text(title.into())),
            button_style(state.minimize_button_hover) // Minimize
                .with_child(Dom::text("-".into()))
                .with_callback(on_minimize_click),
            button_style(state.maximize_button_hover) // Maximize
                .with_child(Dom::text("[ ]".into()))
                .with_callback(on_maximize_click),
            button_style(state.close_button_hover) // Close
                .with_child(Dom::text("X".into()))
                .with_callback(on_close_click),
        ]))
}

fn on_close_click(_: &mut azul_core::refany::RefAny, info: &mut azul_layout::callbacks::CallbackInfo) -> Update {
    let mut flags = info.get_current_window_flags();
    flags.close_requested = true;
    info.set_window_flags(flags);
    Update::DoNothing
}

fn on_minimize_click(_: &mut azul_core::refany::RefAny, info: &mut azul_layout::callbacks::CallbackInfo) -> Update { Update::DoNothing }
fn on_maximize_click(_: &mut azul_core::refany::RefAny, info: &mut azul_layout::callbacks::CallbackInfo) -> Update { Update::DoNothing }


/// Checks if an event falls on a decoration and handles it.
/// Returns true if the event was consumed.
pub fn handle_decoration_event(
    window: &mut super::X11Window,
    event: &XEvent,
) -> bool {
    match unsafe { event.type_ } {
        ButtonPress => {
            let bev = unsafe { event.button };
            if (bev.y as f32) < TITLE_BAR_HEIGHT {
                let width = window.current_window_state.size.dimensions.width;
                if bev.x > (width - BUTTON_WIDTH) as i32 {
                    window.close();
                } else {
                    window.decorations.is_dragging = true;
                    window.decorations.drag_start_pos = (bev.x_root, bev.y_root);
                }
                return true;
            }
        },
        ButtonRelease => {
            if window.decorations.is_dragging {
                window.decorations.is_dragging = false;
                return true;
            }
        },
        MotionNotify => {
             if window.decorations.is_dragging {
                let motion_event = unsafe { &event.motion };
                let (start_x, start_y) = window.decorations.drag_start_pos;
                let (dx, dy) = (motion_event.x_root - start_x, motion_event.y_root - start_y);

                let current_pos = &window.current_window_state.position;
                let new_x = current_pos.get_x() as i32 + dx;
                let new_y = current_pos.get_y() as i32 + dy;

                send_move_resize_message(window, new_x, new_y, 8); // _NET_WM_MOVERESIZE_MOVE

                return true;
            }
        },
        _ => {}
    }
    false
}

fn send_move_resize_message(window: &mut super::X11Window, x: i32, y: i32, direction: u32) {
    let atom = unsafe { (window.xlib.XInternAtom)(window.display, b"_NET_WM_MOVERESIZE\0".as_ptr() as _, 0) };
    let root = unsafe { (window.xlib.XRootWindow)(window.display, (window.xlib.XDefaultScreen)(window.display)) };

    let mut xev: XEvent = unsafe { std::mem::zeroed() };
    let mut client_message = unsafe { &mut xev.client_message };

    client_message.type_ = ClientMessage;
    client_message.window = window.window;
    client_message.message_type = atom;
    client_message.format = 32;
    client_message.data.as_longs_mut()[0] = x as _;
    client_message.data.as_longs_mut()[1] = y as _;
    client_message.data.as_longs_mut()[2] = direction as _;
    client_message.data.as_longs_mut()[3] = 1; // Button 1
    client_message.data.as_longs_mut()[4] = 0;

    unsafe {
        (window.xlib.XSendEvent)(window.display, root, 0, SubstructureRedirectMask | SubstructureNotifyMask, &mut xev);
        (window.xlib.XFlush)(window.display);
    }
}

//! The canvas node and its input: the render callback, the pointer, the
//! wheel and pinch, the marching-ants timer, the keyboard shortcuts - and
//! [`push_effects`], the one place a state change reaches the image node.

use azul::{
    callbacks::{RenderImageCallbackInfo, UpdateImageType},
    dom::{DomId, NodeId, RenderImageCallback, VirtualKeyCode},
    image::{ImageRef, RawImageFormat},
    prelude::*,
    time::SystemTimeDiff,
    vec::U8VecRef,
    widgets::StatusBar,
};

use crate::{
    codec,
    commands::{self, Command},
    raster::IRect,
    say,
    state::{Effects, Mods, Tool},
    AppScreen, PhotoApp,
};

/// The canvas image node's DOM id.
pub const CANVAS_ID: &str = "photo-canvas";
/// The status bar segment that shows the cursor position.
pub const CURSOR_MARKER: &str = "photo-cursor";

/// The canvas node: one image whose pixels the render callback provides.
pub fn canvas_dom(app: &RefAny, tool: Tool) -> Dom {
    let cursor = match tool {
        Tool::Hand => "grab",
        Tool::Move => "move",
        Tool::Zoom => "zoom-in",
        Tool::Text => "text",
        _ => "crosshair",
    };
    Dom::create_image(ImageRef::callback(
        RenderImageCallback::create(render_canvas).to_core(),
        app.clone(),
    ))
    .with_id(CANVAS_ID)
    .with_css(format!(
        "flex-grow: 1; align-self: stretch; min-width: 0px; min-height: 0px; cursor: {cursor};"
    ))
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseDown), app.clone(), on_down)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseOver), app.clone(), on_move)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), app.clone(), on_up)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseLeave), app.clone(), on_leave)
    .with_callback(EventFilter::Hover(HoverEventFilter::TouchStart), app.clone(), on_down)
    .with_callback(EventFilter::Hover(HoverEventFilter::TouchMove), app.clone(), on_move)
    .with_callback(EventFilter::Hover(HoverEventFilter::TouchEnd), app.clone(), on_up)
    .with_callback(EventFilter::Hover(HoverEventFilter::Scroll), app.clone(), on_wheel)
    .with_callback(EventFilter::Hover(HoverEventFilter::PinchIn), app.clone(), on_pinch)
    .with_callback(EventFilter::Hover(HoverEventFilter::PinchOut), app.clone(), on_pinch)
}

/// The canvas node's (dom, node) in the current DOM.
#[must_use]
pub fn canvas_node(info: &CallbackInfo) -> Option<(DomId, NodeId)> {
    let dom = DomId { inner: 0 };
    let raw = info.get_node_id_by_id_attribute(dom, CANVAS_ID).into_raw();
    (raw != 0).then(|| (dom, NodeId::create(raw - 1)))
}

/// A view rect as the renderer's dirty rect.
#[must_use]
pub fn layout_rect(r: IRect) -> LayoutRect {
    LayoutRect::create(
        LayoutPoint::create(r.x as isize, r.y as isize),
        LayoutSize::create(r.w as isize, r.h as isize),
    )
}

/// Hand a state change to the window: the changed view rect to the canvas
/// node (only that rect is uploaded), the cursor label, a DOM rebuild.
pub fn push_effects(app: &mut PhotoApp, info: &mut CallbackInfo, e: Effects) -> Update {
    if e.view_all || e.view.is_some() {
        if let Some(image) = codec::view_image(app.s.buf.width, app.s.buf.height, &app.s.buf.bgra) {
            if let Some((dom, node)) = canvas_node(info) {
                match e.view {
                    Some(r) if !e.view_all => {
                        say(&format!("AZPHOTO_UPDATE {} {} {} {}", r.x, r.y, r.w, r.h));
                        info.change_node_image_rect(dom, node, image.clone(), layout_rect(r));
                    }
                    _ => info.change_node_image(dom, node, image.clone(), UpdateImageType::Content),
                }
            }
            app.canvas_image = Some(image);
        }
    }
    if e.cursor && !e.dom {
        update_cursor_label(app, info);
    }
    if e.dom {
        Update::RefreshDom
    } else {
        Update::DoNothing
    }
}

/// "X 120  Y 48  R 214 G 176 B 152" (or the size when off the image).
#[must_use]
pub fn cursor_label(app: &PhotoApp) -> String {
    let (w, h) = app.s.engine.size();
    match app.s.cursor {
        Some((x, y)) if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h => {
            let p = app.s.engine.sample(x as u32, y as u32);
            format!("X {x}  Y {y}  R {} G {} B {}", p[0], p[1], p[2])
        }
        _ => String::from("X -  Y -"),
    }
}

fn update_cursor_label(app: &PhotoApp, info: &mut CallbackInfo) {
    let label = cursor_label(app);
    if let Some(node) = info.get_node_id_by_marker(CURSOR_MARKER).into_option() {
        let _ = StatusBar::update_segment_label(*info, node, label.as_str());
    }
}

/// Draw the viewport at the node's physical size; answer the current image.
extern "C" fn render_canvas(mut data: RefAny, info: RenderImageCallbackInfo) -> ImageRef {
    let bounds = info.get_bounds();
    let physical = bounds.get_physical_size();
    let logical = bounds.get_logical_size();
    let (w, h) = (physical.width.max(1), physical.height.max(1));
    let placeholder = || ImageRef::null_image(w as usize, h as usize, RawImageFormat::BGRA8, U8VecRef::from(&[][..]));
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return placeholder();
    };
    let app = &mut *guard;
    if logical.width > 0.0 {
        app.hidpi = w as f32 / logical.width;
    }
    if (w, h) != (app.s.view.width, app.s.view.height) || !app.s.view_ready {
        let _ = app.s.set_view_size(w, h);
        app.canvas_image = None;
        say(&format!("AZPHOTO_VIEW {w}x{h}"));
    }
    if app.canvas_image.is_none() {
        app.canvas_image = codec::view_image(app.s.buf.width, app.s.buf.height, &app.s.buf.bgra);
    }
    app.canvas_image.clone().unwrap_or_else(placeholder)
}

/// The pointer in the canvas's physical pixels, and the pen pressure.
fn pointer(info: &CallbackInfo, hidpi: f32) -> Option<(f32, f32, f32)> {
    let pos = info.get_cursor_relative_to_node().into_option()?;
    let pressure = match info.get_pen_state().into_option() {
        Some(pen) if pen.in_contact => pen.pressure.clamp(0.05, 1.0),
        _ => 1.0,
    };
    Some((pos.x * hidpi, pos.y * hidpi, pressure))
}

fn mods(info: &CallbackInfo) -> Mods {
    let k = info.get_key_modifiers();
    Mods {
        shift: k.shift,
        alt: k.alt,
        cmd: k.ctrl || k.meta,
    }
}

fn after_pointer(app: &mut PhotoApp, app_ref: &RefAny, info: &mut CallbackInfo, e: Effects) -> Update {
    if e.dom {
        app.announce_layers();
        app.announce_history();
        if !app.s.status.is_empty() {
            say(&format!("AZPHOTO_STATUS {}", app.s.status));
        }
    }
    if app.s.engine.document().selection.is_some() {
        ensure_ants_timer(app, app_ref, info);
    }
    push_effects(app, info, e)
}

extern "C" fn on_down(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let buttons = info.get_current_mouse_state();
    if buttons.right_down || buttons.middle_down {
        return Update::DoNothing;
    }
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let Some((x, y, pressure)) = pointer(&info, app.hidpi) else {
        return Update::DoNothing;
    };
    let e = app.s.pointer_down(x, y, pressure, mods(&info));
    after_pointer(app, &app_ref, &mut info, e)
}

extern "C" fn on_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let Some((x, y, pressure)) = pointer(&info, app.hidpi) else {
        return Update::DoNothing;
    };
    let e = app.s.pointer_move(x, y, pressure, mods(&info));
    after_pointer(app, &app_ref, &mut info, e)
}

extern "C" fn on_up(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let (x, y) = match pointer(&info, app.hidpi) {
        Some((x, y, _)) => (x, y),
        None => return Update::DoNothing,
    };
    let e = app.s.pointer_up(x, y, mods(&info));
    after_pointer(app, &app_ref, &mut info, e)
}

extern "C" fn on_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let e = app.s.pointer_left();
    push_effects(app, &mut info, e)
}

/// The wheel pans; with Ctrl / Cmd or Alt it zooms about the pointer.
extern "C" fn on_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let node = NodeId::create(hit.node.into_raw().saturating_sub(1));
    let Some(delta) = info.get_scroll_delta(hit.dom, node).into_option() else {
        return Update::DoNothing;
    };
    if delta.x == 0.0 && delta.y == 0.0 {
        return Update::DoNothing;
    }
    info.prevent_default();
    let m = mods(&info);
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let e = if m.cmd || m.alt {
        let about = pointer(&info, app.hidpi).map(|(x, y, _)| (x, y));
        let factor = (-delta.y * 0.002).exp();
        let zoom = app.s.view.zoom * factor;
        app.s.zoom_to(zoom, about)
    } else {
        app.s.view.pan_x = (app.s.view.pan_x - delta.x * app.hidpi).round();
        app.s.view.pan_y = (app.s.view.pan_y - delta.y * app.hidpi).round();
        app.s.render_all()
    };
    push_effects(app, &mut info, e)
}

/// A pinch zooms about its centre.
extern "C" fn on_pinch(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pinch) = info.get_pinch().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    let previous = if pinch.began { 1.0 } else { app.last_pinch.unwrap_or(1.0) };
    app.last_pinch = Some(pinch.scale);
    if pinch.scale <= 0.0 || previous <= 0.0 {
        return Update::DoNothing;
    }
    let about = Some((pinch.center.x * app.hidpi, pinch.center.y * app.hidpi));
    let zoom = app.s.view.zoom * pinch.scale / previous;
    let e = app.s.zoom_to(zoom, about);
    push_effects(app, &mut info, e)
}

/// Start the marching-ants timer (once).
pub fn ensure_ants_timer(app: &mut PhotoApp, app_ref: &RefAny, info: &mut CallbackInfo) {
    if app.ants_timer {
        return;
    }
    app.ants_timer = true;
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(app_ref.clone(), on_ants, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(200))),
    );
}

/// The ants move one step: only the selection's edge is redrawn.
extern "C" fn on_ants(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let app = &mut *guard;
    if app.screen != AppScreen::Editor || app.s.dragging() || app.sheet.is_some() {
        return TimerCallbackReturn::continue_unchanged();
    }
    let e = app.s.tick_ants();
    if e.view.is_some() {
        let _ = push_effects(app, &mut info.callback_info, e);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// The letter of a key, for the tool shortcuts.
fn letter(key: VirtualKeyCode) -> Option<char> {
    use VirtualKeyCode as K;
    Some(match key {
        K::A => 'A',
        K::B => 'B',
        K::C => 'C',
        K::D => 'D',
        K::E => 'E',
        K::G => 'G',
        K::H => 'H',
        K::I => 'I',
        K::L => 'L',
        K::M => 'M',
        K::S => 'S',
        K::T => 'T',
        K::U => 'U',
        K::V => 'V',
        K::W => 'W',
        K::X => 'X',
        K::Z => 'Z',
        _ => return None,
    })
}

/// The shortcut a key press means, if any.
#[must_use]
pub fn shortcut(key: VirtualKeyCode, m: Mods, current: Tool) -> Option<Command> {
    use VirtualKeyCode as K;
    if m.cmd {
        return Some(match key {
            K::Z if m.shift => Command::Redo,
            K::Z => Command::Undo,
            K::Y => Command::Redo,
            K::S => Command::Save,
            K::O => Command::Open,
            K::N if m.shift => Command::NewLayer,
            K::N => Command::Sheet(crate::Sheet::NewImage),
            K::E if m.shift => Command::Sheet(crate::Sheet::Export),
            K::E => Command::MergeDown,
            K::A => Command::SelectAll,
            K::D => Command::Deselect,
            K::I if m.shift => Command::Inverse,
            K::J => Command::Duplicate,
            K::Key0 => Command::Fit,
            K::Key1 => Command::ActualPixels,
            K::Equals | K::Plus | K::NumpadAdd => Command::ZoomIn,
            K::Minus | K::NumpadSubtract => Command::ZoomOut,
            _ => return None,
        });
    }
    match key {
        K::Delete | K::Back => Some(Command::Clear),
        K::Escape => Some(Command::Escape),
        K::LBracket => Some(Command::BrushSize(false)),
        K::RBracket => Some(Command::BrushSize(true)),
        K::X => Some(Command::SwapColors),
        K::D => Some(Command::DefaultColors),
        other => letter(other)
            .and_then(|c| Tool::for_key(c, current))
            .map(Command::Tool),
    }
}

/// The window's keyboard shortcuts.
pub extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let Some(key) = keyboard.current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let m = mods(&info);
    let app_ref = data.clone();
    let Some(mut guard) = data.downcast_mut::<PhotoApp>() else {
        return Update::DoNothing;
    };
    let app = &mut *guard;
    if app.screen != AppScreen::Editor && !m.cmd {
        return Update::DoNothing;
    }
    let Some(command) = shortcut(key, m, app.s.tool) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    commands::run(app, &app_ref, &mut info, command)
}

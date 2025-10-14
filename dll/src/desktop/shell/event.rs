use std::{collections::BTreeMap, rc::Rc};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    events::{NodesToCheck, ProcessEventResult},
    geom::{LogicalPosition, LogicalSize, PhysicalSize},
    hit_test::{CursorTypeHitTest, FullHitTest},
    resources::ResourceUpdate,
    window::{
        CursorPosition, MouseCursorType, OptionMouseCursorType, RawWindowHandle, VirtualKeyCode,
        WindowFrame, WindowId,
    },
};
use azul_layout::{
    callbacks::MenuCallback,
    // window_state::StyleAndLayoutChanges, // TODO: This type needs to be ported from
    // REFACTORING/portedfromcore.rs
};
use gl_context_loader::GenericGlContext;
use webrender::{
    api::units::{DeviceIntRect, DeviceIntSize},
    Transaction,
};

use super::{CommandMap, MenuTarget, AZ_THREAD_TICK, AZ_TICK_REGENERATE_DOM};
use crate::desktop::app::{self, App};
#[cfg(target_os = "macos")]
use crate::desktop::shell::appkit::{AppData, GlContextGuard, Window};
#[cfg(target_os = "windows")]
use crate::desktop::shell::win32::{AppData, GlContextGuard, Window};
///! This module encapsulates the different "event actions" that were formerly
///! triggered by Windows messages such as `AZ_REGENERATE_DOM`, `AZ_REDO_HIT_TEST`,
///! and so on.
///
///! Instead of sending `PostMessageW(...)`, we call these event functions directly.
///! This way, the same logic is reusable for both Win32 and macOS.
use crate::desktop::wr_translate::{
    generate_frame, rebuild_display_list, wr_synchronize_updated_images, wr_translate_document_id,
    AsyncHitTester,
};

fn az_regenerate_dom(current_window: &mut Window, userdata: &mut App, _guard: &GlContextGuard) {
    let mut ret = ProcessEventResult::DoNothing;

    // borrow checker :|
    let fc_cache = &mut userdata.fc_cache;
    let data = &mut userdata.data;
    let image_cache = &mut userdata.image_cache;

    let document_id = current_window.internal.document_id;
    let mut hit_tester = &mut current_window.hit_tester;
    let internal = &mut current_window.internal;
    let gl_context = &current_window.gl_context_ptr;

    // unset the focus
    internal.current_window_state.focused_node = None;

    let mut resource_updates = Vec::new();
    fc_cache.apply_closure(|fc_cache| {
        internal.regenerate_styled_dom(
            data,
            image_cache,
            gl_context,
            &mut resource_updates,
            internal.get_dpi_scale_factor(),
            &crate::desktop::app::CALLBACKS,
            fc_cache,
            azul_layout::solver2::do_the_relayout,
            |window_state, scroll_states, layout_results| {
                crate::desktop::wr_translate::fullhittest_new_webrender(
                    &*hit_tester.resolve(),
                    document_id,
                    window_state.focused_node,
                    layout_results,
                    &window_state.mouse_state.cursor_position,
                    window_state.size.get_hidpi_factor(),
                )
            },
            &mut None,
        );
    });

    // stop timers that have a DomNodeId attached to them
    current_window.stop_timers_with_node_ids();

    /*
    current_window.context_menu = None;
    Window::set_menu_bar(
        hwnd,
        &mut current_window.menu_bar,
        current_window.internal.get_menu_bar()
    );
    */

    // rebuild the display list and send it
    rebuild_display_list(
        &mut current_window.internal,
        &mut current_window.render_api,
        image_cache,
        resource_updates,
    );

    current_window.render_api.flush_scene_builder();

    let wr_document_id = wr_translate_document_id(current_window.internal.document_id);
    current_window.hit_tester =
        AsyncHitTester::Requested(current_window.render_api.request_hit_tester(wr_document_id));

    let hit_test = crate::desktop::wr_translate::fullhittest_new_webrender(
        &*current_window.hit_tester.resolve(),
        current_window.internal.document_id,
        current_window.internal.current_window_state.focused_node,
        &current_window.internal.layout_results,
        &current_window
            .internal
            .current_window_state
            .mouse_state
            .cursor_position,
        current_window
            .internal
            .current_window_state
            .size
            .get_hidpi_factor(),
    );

    current_window.internal.previous_window_state = None;
    current_window.internal.current_window_state.last_hit_test = hit_test;

    let mut nodes_to_check = NodesToCheck::simulated_mouse_move(
        &current_window.internal.current_window_state.last_hit_test,
        current_window.internal.current_window_state.focused_node,
        current_window
            .internal
            .current_window_state
            .mouse_state
            .mouse_down(),
    );

    // TODO: StyleAndLayoutChanges no longer exists - need to reimplement with new API
    // let mut style_layout_changes = StyleAndLayoutChanges::new(
    //     &nodes_to_check,
    //     &mut current_window.internal.layout_results,
    //     &image_cache,
    //     &mut current_window.internal.renderer_resources,
    //     current_window
    //         .internal
    //         .current_window_state
    //         .size
    //         .get_layout_size(),
    //     &current_window.internal.document_id,
    //     None,
    //     None,
    //     &None,
    //     azul_layout::solver2::do_the_relayout,
    // );

    az_regenerate_display_list(current_window, userdata, _guard);
}

/// `az_redo_hit_test => ProcessEventResult``
///
/// ```rust,ignore
///    match az_redo_hittest {
///        ProcessEventResult::DoNothing => { },
///        ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
///            az_regenerate_dom(window, app, guard);
///        },
///        ProcessEventResult::ShouldRegenerateDomAllWindows => {
///            for window in mac_app.windows.values() {
///                let guard = window.make_gl_current();
///                az_regenerate_dom(window, app, guard);
///            }
///        },
///        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
///            az_regenerate_display_list(window, app, guard);
///        },
///        ProcessEventResult::UpdateHitTesterAndProcessAgain => {
///            window.internal.previous_window_state = Some(w.internal.current_window_state.clone());
///            az_regenerate_display_list(window, app, guard);
///            az_redo_hit_test(window, app, guard);
///        },
///        ProcessEventResult::ShouldReRenderCurrentWindow => {
///            az_gpu_scroll_render(window, app, guard);
///        },
///    }
/// ```
pub fn az_redo_hit_test(
    current_window: &mut Window,
    userdata: &mut App,
    _guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("az_redo_hit_test");

    let fc_cache = &mut userdata.fc_cache;
    let image_cache = &mut userdata.image_cache;
    let config = &userdata.config;

    let mut new_windows = Vec::new();
    let mut destroyed_windows = Vec::new();

    crate::desktop::shell::process::process_event(
        handle,
        current_window,
        fc_cache,
        image_cache,
        config,
        &mut new_windows,
        &mut destroyed_windows,
    )

    // create_windows(hinstance, shared_application_data, new_windows);
    // destroy_windows(ab, destroyed_windows);
}

fn az_regenerate_display_list(
    current_window: &mut Window,
    userdata: &mut App,
    _guard: &GlContextGuard,
) {
    println!("az_regenerate_display_list");

    let image_cache = &userdata.image_cache;

    rebuild_display_list(
        &mut current_window.internal,
        &mut current_window.render_api,
        image_cache,
        Vec::new(), // no resource updates
    );

    let wr_document_id = wr_translate_document_id(current_window.internal.document_id);
    current_window.hit_tester =
        AsyncHitTester::Requested(current_window.render_api.request_hit_tester(wr_document_id));

    generate_frame(
        &mut current_window.internal,
        &mut current_window.render_api,
        true,
    );
}

fn az_gpu_scroll_render(current_window: &mut Window, userdata: &mut App, _guard: &GlContextGuard) {
    println!("az_gpu_scroll_render");

    generate_frame(
        &mut current_window.internal,
        &mut current_window.render_api,
        false,
    );
}

// Window has received focus
pub(crate) fn wm_set_focus(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("wm_set_focus");
    current_window.internal.previous_window_state =
        Some(current_window.internal.current_window_state.clone());
    current_window.internal.current_window_state.flags.has_focus = true;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

// Window has lost focus
pub(crate) fn wm_kill_focus(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("wm_kill_focus");
    current_window.internal.previous_window_state =
        Some(current_window.internal.current_window_state.clone());
    current_window.internal.current_window_state.flags.has_focus = false;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_mousemove(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    newpos: LogicalPosition,
) -> ProcessEventResult {
    println!("wm_mousemove {newpos:?}");

    let pos = CursorPosition::InWindow(newpos);

    // call SetCapture(hwnd) so that we can capture the WM_MOUSELEAVE event
    let cur_cursor_pos = current_window
        .internal
        .current_window_state
        .mouse_state
        .cursor_position;
    let prev_cursor_pos = current_window
        .internal
        .previous_window_state
        .as_ref()
        .map(|m| m.mouse_state.cursor_position)
        .unwrap_or_default();

    if !prev_cursor_pos.is_inside_window() && cur_cursor_pos.is_inside_window() {
        current_window.on_mouse_enter(prev_cursor_pos, cur_cursor_pos);
    }

    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);
    current_window
        .internal
        .current_window_state
        .mouse_state
        .cursor_position = pos;

    // mouse moved, so we need a new hit test
    let hit_test = crate::desktop::wr_translate::fullhittest_new_webrender(
        &*current_window.hit_tester.resolve(),
        current_window.internal.document_id,
        current_window.internal.current_window_state.focused_node,
        &current_window.internal.layout_results,
        &current_window
            .internal
            .current_window_state
            .mouse_state
            .cursor_position,
        current_window
            .internal
            .current_window_state
            .size
            .get_hidpi_factor(),
    );
    let cht = CursorTypeHitTest::new(&hit_test, &current_window.internal.layout_results);
    current_window.internal.current_window_state.last_hit_test = hit_test;

    // update the cursor if necessary
    if current_window
        .internal
        .current_window_state
        .mouse_state
        .mouse_cursor_type
        != OptionMouseCursorType::Some(cht.cursor_icon)
    {
        // TODO: unset previous cursor?
        current_window
            .internal
            .current_window_state
            .mouse_state
            .mouse_cursor_type = OptionMouseCursorType::Some(cht.cursor_icon);
        current_window.on_cursor_change(
            current_window
                .internal
                .current_window_state
                .mouse_state
                .mouse_cursor_type
                .as_ref()
                .cloned(),
            cht.cursor_icon,
        );
    }

    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_keydown(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    scancode: u32,
    vk: Option<VirtualKeyCode>,
) -> ProcessEventResult {
    println!("wm_keydown {scancode} - {vk:?}");

    current_window.internal.previous_window_state =
        Some(current_window.internal.current_window_state.clone());
    current_window
        .internal
        .current_window_state
        .keyboard_state
        .current_char = None.into();
    current_window
        .internal
        .current_window_state
        .keyboard_state
        .pressed_scancodes
        .insert_hm_item(scancode);

    if let Some(vk) = vk {
        current_window
            .internal
            .current_window_state
            .keyboard_state
            .current_virtual_keycode = Some(vk).into();
        current_window
            .internal
            .current_window_state
            .keyboard_state
            .pressed_virtual_keycodes
            .insert_hm_item(vk);
    }

    az_redo_hit_test(current_window, userdata, guard, handle)
}

// Composite text input
pub(crate) fn wm_char(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    c: char,
) -> ProcessEventResult {
    println!("wm_char {c}");

    if c.is_control() {
        return ProcessEventResult::DoNothing;
    }

    current_window.internal.previous_window_state =
        Some(current_window.internal.current_window_state.clone());
    current_window
        .internal
        .current_window_state
        .keyboard_state
        .current_char = Some(c as u32).into();
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_keyup(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    scancode: u32,
    vk: Option<VirtualKeyCode>,
) -> ProcessEventResult {
    println!("wm_keyup {scancode} - {vk:?}");

    current_window.internal.previous_window_state =
        Some(current_window.internal.current_window_state.clone());
    current_window
        .internal
        .current_window_state
        .keyboard_state
        .current_char = None.into();
    current_window
        .internal
        .current_window_state
        .keyboard_state
        .pressed_scancodes
        .remove_hm_item(&scancode);
    if let Some(vk) = vk {
        current_window
            .internal
            .current_window_state
            .keyboard_state
            .pressed_virtual_keycodes
            .remove_hm_item(&vk);
        current_window
            .internal
            .current_window_state
            .keyboard_state
            .current_virtual_keycode = None.into();
    }
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_mouseleave(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("wm_mouseleave");

    let current_focus = current_window.internal.current_window_state.focused_node;
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);
    let last_seen = match current_window
        .internal
        .current_window_state
        .mouse_state
        .cursor_position
    {
        CursorPosition::InWindow(i) => i,
        _ => LogicalPosition::zero(),
    };
    current_window
        .internal
        .current_window_state
        .mouse_state
        .cursor_position = CursorPosition::OutOfWindow(last_seen);
    current_window.internal.current_window_state.last_hit_test = FullHitTest::empty(current_focus);
    current_window
        .internal
        .current_window_state
        .mouse_state
        .mouse_cursor_type = OptionMouseCursorType::None;

    current_window.set_cursor(handle, MouseCursorType::Default);

    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_rbuttondown(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("wm_rbuttondown");
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);
    current_window
        .internal
        .current_window_state
        .mouse_state
        .right_down = true;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_rbuttonup(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    active_menus: &mut BTreeMap<MenuTarget, CommandMap>,
) -> ProcessEventResult {
    println!("wm_rbuttonup");
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);

    // open context menu
    if let Some((context_menu, hit, node_id)) = current_window.internal.get_context_menu() {
        current_window.create_and_open_context_menu(&*context_menu, &hit, node_id, active_menus);
    }

    current_window
        .internal
        .current_window_state
        .mouse_state
        .right_down = false;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_mbuttondown(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("wm_mbuttondown");
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);
    current_window
        .internal
        .current_window_state
        .mouse_state
        .middle_down = true;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_mbuttonup(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("wm_mbuttonup");
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);
    current_window
        .internal
        .current_window_state
        .mouse_state
        .middle_down = false;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_lbuttondown(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
) -> ProcessEventResult {
    println!("wm_lbuttondown");
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);
    current_window
        .internal
        .current_window_state
        .mouse_state
        .left_down = true;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_lbuttonup(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    active_menus: &mut BTreeMap<MenuTarget, CommandMap>,
) -> ProcessEventResult {
    println!("wm_lbuttonup");
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);

    // open context menu
    if let Some((context_menu, hit, node_id)) = current_window.internal.get_context_menu() {
        current_window.create_and_open_context_menu(&*context_menu, &hit, node_id, active_menus);
    }

    current_window
        .internal
        .current_window_state
        .mouse_state
        .left_down = false;
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_mousewheel(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    scroll_amount: f32,
) -> ProcessEventResult {
    println!("wm_mousewheel {scroll_amount}");
    let previous_state = current_window.internal.current_window_state.clone();
    current_window.internal.previous_window_state = Some(previous_state);
    current_window
        .internal
        .current_window_state
        .mouse_state
        .scroll_y = Some(scroll_amount).into();
    az_redo_hit_test(current_window, userdata, guard, handle)
}

pub(crate) fn wm_dpichanged(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    dpi: u32,
) -> ProcessEventResult {
    println!("wm_dpichanged {dpi}");
    // TODO!
    ProcessEventResult::DoNothing
}

// NOTE: This will generate a new frame, but not paint it yet, call wm_paint() afterwards
pub(crate) fn wm_size(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    new_size: PhysicalSize<u32>,
    dpi: u32,
    frame: WindowFrame, // minimized, maximized, restored
) {
    println!("wm_size {dpi} {frame:?} {new_size:?}");

    let mut new_window_state = current_window.internal.current_window_state.clone();
    new_window_state.size.dpi = dpi;
    new_window_state.size.dimensions =
        new_size.to_logical(new_window_state.size.get_hidpi_factor());
    new_window_state.flags.frame = frame;

    let mut fc_cache = &mut userdata.fc_cache;
    let image_cache = &userdata.image_cache;

    let resize_result = fc_cache.apply_closure(|mut fc_cache| {
        current_window.internal.do_quick_resize(
            &image_cache,
            &crate::desktop::app::CALLBACKS,
            azul_layout::solver2::do_the_relayout,
            &mut *fc_cache,
            &current_window.gl_context_ptr,
            &new_window_state.size,
            new_window_state.theme,
        )
    });

    let mut txn = Transaction::new();

    wr_synchronize_updated_images(resize_result.updated_images, &mut txn);

    current_window.internal.previous_window_state =
        Some(current_window.internal.current_window_state.clone());
    current_window.internal.current_window_state = new_window_state;

    txn.set_document_view(DeviceIntRect::from_size(DeviceIntSize::new(
        new_size.width as i32,
        new_size.height as i32,
    )));
    current_window.render_api.send_transaction(
        wr_translate_document_id(current_window.internal.document_id),
        txn,
    );

    rebuild_display_list(
        &mut current_window.internal,
        &mut current_window.render_api,
        &userdata.image_cache,
        Vec::new(),
    );

    let wr_document_id = wr_translate_document_id(current_window.internal.document_id);
    current_window.hit_tester =
        AsyncHitTester::Requested(current_window.render_api.request_hit_tester(wr_document_id));

    generate_frame(
        &mut current_window.internal,
        &mut current_window.render_api,
        true,
    );
}

pub(crate) fn wm_paint(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    gl_functions: Rc<GenericGlContext>,
) {
    // Assuming that the display list has been submitted and the
    // scene on the background thread has been rebuilt, now tell
    // webrender to pain the scene

    // gl context is current (assured by GlContextGuard)

    println!("wm_paint");

    let rect_size = current_window
        .internal
        .current_window_state
        .size
        .dimensions
        .to_physical(
            current_window
                .internal
                .current_window_state
                .size
                .get_hidpi_factor(),
        );

    // Block until all transactions (display list build)
    // have finished processing
    //
    // Usually this shouldn't take too long, since DL building
    // happens asynchronously between WM_SIZE and WM_PAINT
    current_window.render_api.flush_scene_builder();

    let mut gl = &gl_functions;

    gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
    gl.disable(gl_context_loader::gl::FRAMEBUFFER_SRGB);
    gl.disable(gl_context_loader::gl::MULTISAMPLE);
    gl.viewport(0, 0, rect_size.width as i32, rect_size.height as i32);

    let mut current_program = [0_i32];
    unsafe {
        gl.get_integer_v(
            gl_context_loader::gl::CURRENT_PROGRAM,
            (&mut current_program[..]).into(),
        )
    };

    let framebuffer_size = DeviceIntSize::new(rect_size.width as i32, rect_size.height as i32);

    // Render
    if let Some(r) = current_window.renderer.as_mut() {
        r.update();
        let _ = r.render(framebuffer_size, 0);
    }

    current_window.swap_buffers(handle);

    gl.bind_framebuffer(gl_context_loader::gl::FRAMEBUFFER, 0);
    gl.bind_texture(gl_context_loader::gl::TEXTURE_2D, 0);
    gl.use_program(current_program[0] as u32);
}

pub(crate) fn wm_quit(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    gl_functions: Rc<GenericGlContext>,
) {
    println!("wm_quit");
    // TODO: execute quit callback
}

pub(crate) fn wm_destroy(
    current_window: &mut Window,
    userdata: &mut App,
    guard: &GlContextGuard,
    handle: &RawWindowHandle,
    gl_functions: Rc<GenericGlContext>,
) {
    println!("wm_destroy");

    // deallocate objects, etc.

    current_window.destroy(userdata, guard, handle, gl_functions);
}

/// A helper that matches on the `ProcessEventResult`.
///
/// - `window` (the current window)
/// - `app` (mutable reference to your main app data)
/// - `guard` (the already-current OpenGL context)
/// - `raw` (the `RawWindowHandle`)
pub fn handle_process_event_result(
    ret: ProcessEventResult,
    all_windows: &mut BTreeMap<WindowId, Window>,
    window_id: WindowId,
    app: &mut App,
    guard: &GlContextGuard,
    raw: &RawWindowHandle,
) -> Option<()> {
    match ret {
        ProcessEventResult::DoNothing => {}
        ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
            // Rebuild the DOM for this one window (OpenGL context is already current)
            let cw = all_windows.get_mut(&window_id)?;
            az_regenerate_dom(cw, app, guard);
        }
        ProcessEventResult::ShouldRegenerateDomAllWindows => {
            for cw in all_windows.values_mut() {
                az_regenerate_dom(cw, app, guard);
            }
        }
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
            // Rebuild the display list, but not the DOM for this single window
            let cw = all_windows.get_mut(&window_id)?;
            az_regenerate_display_list(cw, app, guard);
        }
        ProcessEventResult::UpdateHitTesterAndProcessAgain => {
            let cw = all_windows.get_mut(&window_id)?;

            // Record old state
            cw.internal.previous_window_state = Some(cw.internal.current_window_state.clone());

            // 1) Rebuild the display list
            az_regenerate_display_list(cw, app, guard);

            // 2) Then re-run the “redo hit test” We have the same references, so just call it:
            az_redo_hit_test(cw, app, guard, raw);
        }
        ProcessEventResult::ShouldReRenderCurrentWindow => {
            let cw = all_windows.get_mut(&window_id)?;
            az_gpu_scroll_render(cw, app, guard);
        }
    }

    Some(())
}

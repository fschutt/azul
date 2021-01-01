use std::{
    time::{Duration, Instant},
    collections::BTreeMap,
};
use glutin::{
    window::{
        Window as GlutinWindow,
        WindowId as GlutinWindowId,
    },
    event::{
        WindowEvent as GlutinWindowEvent,
    },
    event_loop::EventLoopWindowTarget as GlutinEventLoopWindowTarget,
    Context, NotCurrent,
};
#[cfg(feature = "logging")]
use log::LevelFilter;
use crate::{
    wr_api::WrApi,
    display_shader::DisplayShader,
    window::Window
};
use azul_core::{
    FastHashMap,
    window::{RendererType, WindowCreateOptions, DebugState, FullWindowState},
    gl::GlContextPtr,
    callbacks::{RefAny, UpdateScreen},
    app_resources::{AppResources, LoadFontFn, LoadImageFn},
};

#[cfg(not(test))]
use crate::window::{FakeDisplay, RendererCreationError};

#[cfg(test)]
use azul_core::app_resources::FakeRenderApi;

/// Graphical application that maintains some kind of application state
#[derive(Debug)]
pub struct App {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: RefAny,
    /// Application configuration, whether to enable logging, etc.
    pub config: AppConfig,
    /// The window create options (only set at startup), get moved into the `.run_inner()` method
    /// No window is actually shown until the `.run_inner()` method is called.
    pub windows: Vec<WindowCreateOptions>,
    /// The actual renderer of this application
    #[cfg(not(test))]
    display_shader: DisplayShader,
    #[cfg(not(test))]
    fake_display: FakeDisplay,
    #[cfg(test)]
    render_api: FakeRenderApi,
}

/// Configuration for optional features, such as whether to enable logging or panic hooks
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppConfig {
    /// If enabled, logs error and info messages.
    ///
    /// Default is `Some(LevelFilter::Error)` to log all errors by default
    #[cfg(feature = "logging")]
    pub enable_logging: Option<LevelFilter>,
    /// Path to the output log if the logger is enabled
    pub log_file_path: Option<String>,
    /// If the app crashes / panics, a window with a message box pops up.
    /// Setting this to `false` disables the popup box.
    pub enable_visual_panic_hook: bool,
    /// If this is set to `true` (the default), a backtrace + error information
    /// gets logged to stdout and the logging file (only if logging is enabled).
    pub enable_logging_on_panic: bool,
    /// (STUB) Whether keyboard navigation should be enabled (default: true).
    /// Currently not implemented.
    pub enable_tab_navigation: bool,
    /// Whether to force a hardware or software renderer
    pub renderer_type: RendererType,
    /// Debug state for all windows
    pub debug_state: DebugState,
    /// Framerate (i.e. 16ms) - sets how often the timer / tasks should check
    /// for updates. Default: 30ms
    pub min_frame_duration: Duration,
    /// Function that is called when a font should be loaded. This is necessary to be
    /// configurable so that "desktop" and "web" versions of azul can have different
    /// implementations of loading fonts
    pub font_loading_fn: LoadFontFn,
    /// Function that is called when a font should be loaded. Necessary to be
    /// configurable so that "desktop" and "web" versions of azul can have
    /// different implementations of loading images.
    pub image_loading_fn: LoadImageFn,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "logging")]
            enable_logging: Some(LevelFilter::Error),
            log_file_path: None,
            enable_visual_panic_hook: true,
            enable_logging_on_panic: true,
            enable_tab_navigation: true,
            renderer_type: RendererType::default(),
            debug_state: DebugState::default(),
            min_frame_duration: Duration::from_millis(30),
            font_loading_fn: LoadFontFn { cb: azulc::font_loading::font_source_get_bytes }, // assumes "font_loading" feature enabled
            image_loading_fn: LoadImageFn { cb: azulc::image_loading::image_source_get_bytes }, // assumes "image_loading" feature enabled
        }
    }
}

impl App {

    #[cfg(not(test))]
    #[allow(unused_variables)]
    /// Creates a new, empty application using a specified callback. This does not open any windows.
    pub fn new(initial_data: RefAny, app_config: AppConfig) -> Result<Self, RendererCreationError> {

        #[cfg(feature = "logging")] {
            if let Some(log_level) = app_config.enable_logging {
                crate::logging::set_up_logging(app_config.log_file_path.as_ref().map(|s| s.as_str()), log_level);

                if app_config.enable_logging_on_panic {
                    crate::logging::set_up_panic_hooks();
                }

                if app_config.enable_visual_panic_hook {
                    use std::sync::atomic::Ordering;
                    crate::logging::SHOULD_ENABLE_PANIC_HOOK.store(true, Ordering::SeqCst);
                }
            }
        }

        #[cfg(not(test))] {
            use crate::wr_translate::set_webrender_debug_flags;

            let mut fake_display = FakeDisplay::new(app_config.renderer_type.clone())?;
            let display_shader = DisplayShader::compile(&fake_display.gl_context)?;

            if let Some(r) = &mut fake_display.renderer {
                set_webrender_debug_flags(r, &app_config.debug_state);
            }

            Ok(Self {
                windows: Vec::new(),
                data: initial_data,
                config: app_config,
                fake_display,
                display_shader: display_shader,
            })
        }

        #[cfg(test)] {
           Ok(Self {
               windows: Vec::new(),
               data: initial_data,
               config: app_config,
               render_api: FakeRenderApi::new(),
           })
        }
    }

    /// Toggles debugging flags in webrender, updates `self.config.debug_state`
    #[cfg(not(test))]
    pub fn toggle_debug_flags(&mut self, new_state: DebugState) {
        use crate::wr_translate::set_webrender_debug_flags;
        if let Some(r) = &mut self.fake_display.renderer {
            set_webrender_debug_flags(r, &new_state);
        }
        self.config.debug_state = new_state;
    }
}

impl App {

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn add_window(&mut self, create_options: WindowCreateOptions) {
        self.windows.push(create_options);
    }

    /// Start the rendering loop for the currently added windows. The run() function
    /// takes one `WindowCreateOptions` as an argument, which is the "root" window, i.e.
    /// the main application window.
    #[cfg(not(test))]
    pub fn run(mut self, root_window: WindowCreateOptions) -> ! {

        #[cfg(target_os = "macos")]
        {
            use core_foundation::{self as cf, base::TCFType};
            let i = cf::bundle::CFBundle::main_bundle().info_dictionary();
            let mut i = unsafe { i.to_mutable() };
            i.set(
                cf::string::CFString::new("NSSupportsAutomaticGraphicsSwitching"),
                cf::boolean::CFBoolean::true_value().into_CFType(),
            );
        }

        self.add_window(root_window);
        self.run_inner()
    }

    #[cfg(not(test))]
    #[allow(unused_variables)]
    fn run_inner(self) -> ! {

        let App { mut data, mut fake_display, mut display_shader, config, windows } = self;

        let mut timers = FastHashMap::new();
        let mut tasks = Vec::new();
        let mut resources = AppResources::default();
        let mut last_style_reload = Instant::now();
        let mut active_windows = BTreeMap::new();

        let FakeDisplay { mut render_api, mut renderer, mut hidden_context, hidden_event_loop, gl_context } = fake_display;

        // Create the windows (makes them actually show up on the screen)
        for window_create_options in windows {
            create_window(
                &data,
                window_create_options,
                &gl_context,
                hidden_context.headless_context_not_current().unwrap(),
                &hidden_event_loop,
                &mut render_api,
                &mut active_windows,
                &mut resources,
            );
        };

        hidden_event_loop.run(move |event, event_loop_target, control_flow| {

            use glutin::event::Event;
            use glutin::event_loop::ControlFlow;
            use std::collections::HashSet;

            let mut redrawn_from_event = HashSet::new();
            let mut windows_created = Vec::<WindowCreateOptions>::new();

            let now = std::time::Instant::now();

            match event {
                Event::DeviceEvent { .. } => {
                    // ignore high-frequency events
                    *control_flow = ControlFlow::Wait;
                    return;
                },
                Event::RedrawRequested(window_id) => {

                    let window = match active_windows.get_mut(&window_id) {
                        Some(s) => s,
                        None => { return; },
                    };

                    // render the display list to a texture
                    if let Some(texture) = window.render_display_list_to_texture(&mut hidden_context, &mut render_api, renderer.as_mut().unwrap(), &gl_context) {
                        // if the rendering went OK, render the texture to the screen
                        window.draw_texture_to_screen_and_swap(&texture, &display_shader);
                    }
                },
                Event::WindowEvent { event, window_id } => {

                    use azul_core::window_state::{Events, NodesToCheck};
                    use azul_core::window::{FullHitTest, CursorTypeHitTest};

                    let window = match active_windows.get_mut(&window_id) {
                        Some(s) => s,
                        None => {return; },
                    };

                    // ONLY update the window_state of the window, don't do anything else
                    // everything is then
                    process_window_event(&event, &mut window.internal.current_window_state, &window.display.window(), &event_loop_target);

                    let mut need_regenerate_display_list = false;
                    let mut should_scroll_render = false;
                    let mut should_callback_render = false;

                    loop {
                        let mut events = Events::new(&window.internal.current_window_state, &window.internal.previous_window_state);
                        let layout_callback_changed = window.internal.current_window_state.layout_callback_changed(&window.internal.previous_window_state);
                        let hit_test = if !events.needs_hit_test() { FullHitTest::empty() } else {
                            let ht = FullHitTest::new(&window.internal.layout_results, &window.internal.current_window_state.mouse_state.cursor_position, &window.internal.scroll_states);
                            window.internal.current_window_state.hovered_nodes = ht.hovered_nodes;
                            ht
                        };

                        if events.is_empty() || layout_callback_changed { break; } // previous_window_state = current_window_state, nothing to do

                        let scroll_event = window.internal.current_window_state.get_scroll_amount();
                        let nodes_to_check = NodesToCheck::new(&hit_test, &events);
                        let callback_results = window.call_callbacks(&nodes_to_check, &events, &gl_context, &mut resources);

                        let cur_should_callback_render = callback_results.should_scroll_render;
                        if cur_should_callback_render { should_callback_render = true; }
                        let cur_should_scroll_render = window.internal.current_window_state.get_scroll_amount().as_ref().map(|se| window.internal.scroll_states.should_scroll_render(se, &hit_test)).unwrap_or(false);
                        if cur_should_scroll_render { should_scroll_render = true; }
                        window.internal.current_window_state.mouse_state.reset_scroll_to_zero();
                        let mut dom_changed = false;

                        if layout_callback_changed {
                            window.regenerate_styled_dom(&gl_context, &mut resources, &mut render_api);
                            need_regenerate_display_list = true;
                            callback_results.update_focused_node = Some(None); // unset the focus
                        } else {
                            match callback_results.callbacks_update_screen {
                                UpdateScreen::RegenerateStyledDomForCurrentWindow => {
                                    window.regenerate_styled_dom(&gl_context, &mut resources, &mut render_api);
                                    need_regenerate_display_list = true;
                                    callback_results.update_focused_node = Some(None); // unset the focus
                                },
                                UpdateScreen::RegenerateStyledDomForAllWindows => {
                                    /* for window in active_windows { window.regenerate_styled_dom(); } */
                                },
                                UpdateScreen::DoNothing => {
                                    // let changes = StyleAndLayoutChanges::new(&nodes_to_check, &mut window.internal.layout_results, &mut resources, window.internal.current_window_state.size.dimensions, window.internal.);

                                    /*
                                    new(
                                            nodes: &NodesToCheck,
                                            layout_results: &mut [LayoutResult],
                                            app_resources: &mut AppResources,
                                            window_size: LayoutSize,
                                            pipeline_id: PipelineId,
                                            relayout_cb: fn(LayoutRect, &mut LayoutResult, &mut AppResources, PipelineId, &RelayoutNodes) -> Vec<NodeId>
                                        )
                                        callback_results.
                                    */

                                    let mut need_relayout = false;
                                    if nodes_to_check.need_restyle() || callback_results.need_restyle() {
                                        need_relayout = window.internal.restyle();
                                        need_regenerate_display_list = true;
                                    }
                                    if events.need_relayout() || callback_results.need_relayout() || need_relayout {
                                        let need_layout_redraw = window.internal.relayout();
                                        if need_layout_redraw {
                                            need_regenerate_display_list = true;
                                        }
                                    }
                                }
                            }
                        }

                        windows_created.extend(callback_results.windows_created.drain(..));
                        timers.extend(callback_results.timers.drain());
                        tasks.append(&mut callback_results.tasks);

                        // see if the callbacks modified the WindowState - if yes, re-determine the events
                        let current_window_save_state = window.internal.current_window_state.clone();
                        let callbacks_changed_cursor = callback_results.cursor_changed();
                        let callbacks_changed_focus = callback_results.focus_changed();
                        if !callbacks_changed_cursor {
                            let ht = FullHitTest::new(&window.internal.layout_results, &window.internal.current_window_state.mouse_state.cursor_position, &window.internal.scroll_states);
                            let cht = CursorTypeHitTest::new(&ht, &window.internal.layout_results);
                            callback_results.modified_window_state.mouse_state.mouse_cursor_type = Some(cht.cursor_icon).into();
                        }
                        if let Some(callback_new_focus) = callback_results.update_focused_node.as_ref() {
                            window.internal.current_window_state.focused_node = *callback_new_focus;
                        }
                        let window_state_changed_in_callbacks = window.synchronize_window_state_with_os(callback_results.modified_window_state);
                        window.internal.previous_window_state = Some(current_window_save_state);
                        if !window_state_changed_in_callbacks || callbacks_changed_focus { break; } else { continue; }
                    }

                    if need_regenerate_display_list {
                        window.rebuild_display_list(&resources, &mut render_api);
                        should_callback_render = true;
                    }
                    if should_scroll_render || should_callback_render {
                        redrawn_from_event.insert(window_id);
                        window.display.window().request_redraw();
                    }
                },
                _ => { },
            }

            // close windows
            let windows_to_remove = active_windows.iter()
            .filter(|(id, window)| window.internal.current_window_state.flags.is_about_to_close)
            .map(|(id, window)| id)
            .collect::<Vec<_>>();

            for window_id in windows_to_remove {
                let window = match active_windows.remove(&window_id) {
                    Some(w) => w,
                    None => continue,
                };
                // TODO: implement on_window_close callback!
                // if window.state.invoke_on_window_close_callback(&mut data) {
                close_window(window, &mut resources, &mut render_api);
                // }
            }

            // open windows
            for window_create_options in windows_created.into_iter() {
                create_window(
                    &data,
                    window_create_options,
                    &gl_context,
                    &hidden_context.headless_context_not_current().unwrap(),
                    &event_loop_target,
                    &mut render_api,
                    &mut active_windows,
                    &mut resources,
                );
            }

            *control_flow = if !active_windows.is_empty() {
                // If no timers / tasks are running, wait until next user event
                if timers.is_empty() && tasks.is_empty() {
                     ControlFlow::Wait
                } else {
                    use azul_core::task::{run_all_timers, clean_up_finished_tasks};

                    // If timers are running, check whether they need to redraw
                    let should_redraw_timers = run_all_timers(&mut timers, &mut data, &mut resources);
                    let should_redraw_tasks = clean_up_finished_tasks(&mut tasks, &mut timers);
                    let should_redraw_timers_tasks = [should_redraw_timers, should_redraw_tasks].iter().any(|i| *i != UpdateScreen::DoNothing);

                    if should_redraw_timers_tasks {
                        for (win_id, window) in active_windows.iter() {
                            if !redrawn_from_event.contains(win_id) {
                                window.display.window().request_redraw();
                            }
                        }
                        ControlFlow::Poll
                    } else {
                        ControlFlow::WaitUntil(now + config.min_frame_duration)
                    }
                }
            } else {

                // Application shutdown

                use gleam::gl;

                // for task in tasks.iter() { task.sender.send_event(TaskEvent::ApplicationShutdown); }

                // NOTE: For some reason this is necessary, otherwise the renderer crashes on shutdown
                hidden_context.make_current();

                // Important: destroy all OpenGL textures before the shared
                // OpenGL context is destroyed.
                azul_core::gl::gl_textures_clear_opengl_cache();

                gl_context.disable(gl::FRAMEBUFFER_SRGB);
                gl_context.disable(gl::MULTISAMPLE);
                gl_context.disable(gl::POLYGON_SMOOTH);

                if let Some(renderer) = renderer.take() {
                    renderer.deinit();
                }

                ControlFlow::Exit
            };
        })
    }
}

/// Updates the `FullWindowState` with the new event
fn process_window_event(event: &GlutinWindowEvent, current_window_state: &mut FullWindowState, window: &GlutinWindow, event_loop: &GlutinEventLoopWindowTarget<()>) {

    use glutin::event::{KeyboardInput, Touch};
    use azul_core::window::{CursorPosition, WindowPosition, LogicalPosition};
    use crate::wr_translate::winit_translate::{
        winit_translate_physical_size, winit_translate_physical_position,
    };

    match event {
        GlutinWindowEvent::ModifiersChanged(modifier_state) => {
            current_window_state.keyboard_state.shift_down = modifier_state.shift();
            current_window_state.keyboard_state.ctrl_down = modifier_state.ctrl();
            current_window_state.keyboard_state.alt_down = modifier_state.alt();
            current_window_state.keyboard_state.super_down = modifier_state.logo();
        },
        GlutinWindowEvent::Resized(physical_size) => {
            current_window_state.size.dimensions = winit_translate_physical_size(*physical_size).to_logical(current_window_state.size.system_hidpi_factor as f32);
        },
        GlutinWindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => {
            use crate::window::get_hidpi_factor;
            let (hidpi_factor, system_hidpi_factor) = get_hidpi_factor(window, event_loop);
            current_window_state.size.system_hidpi_factor = system_hidpi_factor;
            current_window_state.size.hidpi_factor = hidpi_factor;
        },
        GlutinWindowEvent::Moved(new_window_position) => {
            current_window_state.position = WindowPosition::Initialized(winit_translate_physical_position(*new_window_position));
        },
        GlutinWindowEvent::CursorMoved { position, .. } => {
            let world_pos_x = position.x as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            let world_pos_y = position.y as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(world_pos_x, world_pos_y));
        },
        GlutinWindowEvent::CursorLeft { .. } => {
            current_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow;
        },
        GlutinWindowEvent::CursorEntered { .. } => {
            current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(0.0, 0.0));
        },
        GlutinWindowEvent::KeyboardInput { input: KeyboardInput { state, virtual_keycode, scancode, .. }, .. } => {
            use crate::wr_translate::winit_translate::translate_virtual_keycode;
            use glutin::event::ElementState;
            match state {
                ElementState::Pressed => {
                    if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                        current_window_state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
                        current_window_state.keyboard_state.current_virtual_keycode = Some(vk).into();
                    }
                    current_window_state.keyboard_state.pressed_scancodes.insert_hm_item(*scancode);
                    current_window_state.keyboard_state.current_char = None.into();
                },
                ElementState::Released => {
                    if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                        current_window_state.keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
                        current_window_state.keyboard_state.current_virtual_keycode = None.into();
                    }
                    current_window_state.keyboard_state.pressed_scancodes.remove_hm_item(scancode);
                    current_window_state.keyboard_state.current_char = None.into();
                }
            }
        },
        // The char event is sliced inbetween a keydown and a keyup event, so the keyup
        // has to clear the character again
        GlutinWindowEvent::ReceivedCharacter(c) => {
            current_window_state.keyboard_state.current_char = Some((*c) as u32).into();
        },
        GlutinWindowEvent::MouseInput { state, button, .. } => {
            use glutin::event::{ElementState::*, MouseButton::*};
            match state {
                Pressed => {
                    match button {
                        Left => current_window_state.mouse_state.left_down = true,
                        Right => current_window_state.mouse_state.right_down = true,
                        Middle => current_window_state.mouse_state.middle_down = true,
                        _ => current_window_state.mouse_state.left_down = true,
                    }
                },
                Released => {
                    match button {
                        Left => current_window_state.mouse_state.left_down = false,
                        Right => current_window_state.mouse_state.right_down = false,
                        Middle => current_window_state.mouse_state.middle_down = false,
                        _ => current_window_state.mouse_state.left_down = false,
                    }
                },
            }
        },
        GlutinWindowEvent::MouseWheel { delta, .. } => {

            const LINE_DELTA: f32 = 38.0;

            use glutin::event::MouseScrollDelta;

            let (scroll_x_px, scroll_y_px) = match delta {
                MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                MouseScrollDelta::LineDelta(x, y) => (x * LINE_DELTA, y * LINE_DELTA),
            };

            // TODO: "natural scrolling" + configurable LINE_DELTA?
            current_window_state.mouse_state.scroll_x = Some(-scroll_x_px).into();
            current_window_state.mouse_state.scroll_y = Some(-scroll_y_px).into();
        },
        GlutinWindowEvent::HoveredFile(file_path) => {
            current_window_state.hovered_file = Some(file_path.clone());
            current_window_state.dropped_file = None;
        },
        GlutinWindowEvent::HoveredFileCancelled => {
            current_window_state.hovered_file = None;
            current_window_state.dropped_file = None;
        },
        GlutinWindowEvent::DroppedFile(file_path) => {
            current_window_state.hovered_file = None;
            current_window_state.dropped_file = Some(file_path.clone());
        },
        GlutinWindowEvent::Focused(f) => {
            current_window_state.flags.has_focus = *f;
        },
        GlutinWindowEvent::CloseRequested => {
            current_window_state.flags.is_about_to_close = true;
        },
        GlutinWindowEvent::Touch(Touch { phase, location, .. }) => {
            // TODO: use current_window_state.touch_state instead, this is wrong
            // TODO: multitouch
            let world_pos_x = location.x as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            let world_pos_y = location.y as f32 / current_window_state.size.hidpi_factor * current_window_state.size.system_hidpi_factor;
            current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(world_pos_x, world_pos_y));
        },
        GlutinWindowEvent::TouchpadPressure { .. } => {
            // At the moment, only supported on Apple forcetouch-capable macbooks.
            // The parameters are: pressure level (value between 0 and 1 representing how hard the touchpad is being pressed) and stage
            // (integer representing the click level).

            // TODO!
        },
        GlutinWindowEvent::AxisMotion { .. } => {
            // Motion on some analog axis. May report data redundant to other, more specific events.

            // TODO!
        },
    }
}

fn create_window(
    data: &RefAny,
    window_create_options: WindowCreateOptions,
    gl_context: &GlContextPtr,
    shared_context: &Context<NotCurrent>,
    events_loop: &GlutinEventLoopWindowTarget<()>,
    render_api: &mut WrApi,
    active_windows: &mut BTreeMap<GlutinWindowId, Window>,
    app_resources: &mut AppResources,
) {

    let window = Window::new(
         &data,
         gl_context,
         window_create_options,
         shared_context,
         events_loop,
         app_resources,
         render_api,
    );

    let window = match window {
        Ok(o) => o,
        Err(e) => {
            #[cfg(feature = "logging")] {
                error!("Error initializing window: {}", e);
            }
            return;
        }
    };

    // TODO: is a redraw() necessary here?

    let glutin_window_id = window.display.window().id();
    active_windows.insert(glutin_window_id, window);
}

fn close_window(window: Window, app_resources: &mut AppResources, render_api: &mut WrApi) {
    // Close the window
    // TODO: Invoke callback to reject the window close event!
    use azul_core::gl::gl_textures_remove_active_pipeline;
    use crate::wr_translate::wr_translate_document_id;
    gl_textures_remove_active_pipeline(&window.internal.pipeline_id);
    app_resources.delete_pipeline(&window.internal.pipeline_id, render_api);
    render_api.api.delete_document(wr_translate_document_id(window.internal.document_id));
}
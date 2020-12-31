use std::{
    time::{Duration, Instant},
    collections::BTreeMap,
};
use glutin::{
    window::{
        Window as GlutinWindow,
        WindowId as GlutinWindowId,
    },
    event::ModifiersState as GlutinModifiersState,
    event_loop::EventLoopWindowTarget as GlutinEventLoopWindowTarget,
};
use gleam::gl::{self, GLuint};
use webrender::{
    PipelineInfo as WrPipelineInfo,
    Renderer as WrRenderer,
    api::{
        units::{LayoutSize as WrLayoutSize, DeviceIntSize as WrDeviceIntSize},
        Transaction as WrTransaction,
    },
};
#[cfg(feature = "logging")]
use log::LevelFilter;
use azul_css::ColorU;
use crate::{
    resources::WrApi,
    window::{Window, ScrollStates, HeadlessContextState}
};
use azul_core::{
    FastHashMap,
    window::{RendererType, WindowCreateOptions, WindowSize, DebugState, WindowState, FullWindowState},
    dom::{NodeId, ScrollTagId},
    gl::{GlShader, GlContextPtr},
    ui_solver::ScrolledNodes,
    callbacks::{RefAny, HitTestItem, UpdateScreen},
    task::{Task, Timer, TimerId},
    window::{AzulUpdateEvent, CallbacksOfHitTest, KeyboardState, WindowId},
    callbacks::PipelineId,
    display_list::CachedDisplayList,
    app_resources::{
        AppResources, Epoch, FontId, ImageId, LoadedFont, ImmediateFontId,
        TextId, ImageSource, FontSource, CssImageId, ImageInfo, LoadFontFn, LoadImageFn,
    },
};

#[cfg(not(test))]
use crate::window::{ FakeDisplay, RendererCreationError };
#[cfg(not(test))]
use webrender::api::{units::WorldPoint, HitTestFlags};

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
    /// Background color for all windows
    pub background_color: ColorU,
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
            background_color: COLOR_WHITE,
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
            let display_shader = DisplayShader::new(&fake_display.gl_context)?;

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

        use glutin::event_loop::ControlFlow;
        use azul_core::window::{CursorPosition, LogicalPosition};
        use crate::wr_translate::winit_translate::{
            winit_translate_physical_size, winit_translate_physical_position,
        };

        let App { mut data, config, windows, mut fake_display, mut display_shader } = self;

        let mut timers = FastHashMap::new();
        let mut tasks = Vec::new();
        let mut resources = AppResources::default();
        let mut last_style_reload = Instant::now();

        // Create the windows (makes them actually show up on the screen)
        let mut active_windows = window_create_options.into_iter().filter_map(|window_create_options| {
            let window = Window::new(
                &mut fake_display.render_api,
                fake_display.hidden_context.headless_context_not_current().unwrap(),
                &fake_display.hidden_event_loop,
                window_create_options,
                config.background_color,
                app_resources,
            ).ok()?;
            let glutin_window_id = window.display.window().id();
            Some(((window_id, glutin_window_id), window))
        }).collect::<BTreeMap<_, _>>();

        let FakeDisplay { mut render_api, mut renderer, mut hidden_context, hidden_event_loop, gl_context } = fake_display;

        hidden_event_loop.run(move |event, event_loop_target, control_flow| {

            match event {
                Event::DeviceEvent { .. } => {
                    // ignore high-frequency events
                    *control_flow = ControlFlow::Wait;
                    return;
                },
                Event::RedrawRequested(window_id) => {

                    let window = match active_windows.get_mut(window_id) {
                        Some(s) => s,
                        None => return;
                    };

                    // render to a texture
                    let texture = window.render_display_list_to_texture(
                        &mut hidden_context,
                        &mut render_api,
                        renderer.as_mut().unwrap(),
                        &gl_context,
                    );

                    // render the texture to the screen
                    window.display.make_current();
                    gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
                    display_shader.draw_texture_to_screen(texture);
                    window.display.windowed_context().unwrap().swap_buffers().unwrap();
                    window.display.make_not_current();
                },
                Event::WindowEvent { event, window_id } => {

                    let window = match active_windows.get_mut(window_id) {
                        Some(s) => s,
                        None => return;
                    };

                    // ONLY update the window_state of the window, don't do anything else
                    // everything is then
                    process_window_event(&mut window.internal.current_window_state, &event);

                    let mut should_scroll_render = false;
                    let mut should_callback_render = false;

                    loop {

                        let mut events = Events::new(&window.internal.current_window_state, &window.internal.previous_window_state);
                        let hit_test = if !events.needs_hit_test() { HitTest::empty() } else {
                            let ht = HitTest::new(&window.internal.current_window_state, &window.internal.layout_results, &window.internal.scroll_states);
                            window.internal.current_window_state.hovered_nodes = hit_test.hovered_nodes;
                            ht
                        };

                        if events.is_empty() { break; } // previous_window_state = current_window_state, nothing to do

                        let scroll_event = window.internal.current_window_state.mouse_state.get_scroll_amount();
                        let cur_should_scroll_render = scroll_event.map(|se| window.internal.scroll_states.should_scroll_render(se, &hit_test)).unwrap_or(false);
                        if cur_should_scroll_render { should_scroll_render = true; }
                        let nodes_to_check = NodesToCheck::new(&hit_test, &events);

                        let callback_results = window.internal.call_callbacks(&nodes_to_check, &events, &gl_context, &mut app_resources);
                        let cur_should_callback_render = callback_results.should_scroll_render;
                        if cur_should_callback_render { should_callback_render = true; }

                        window.internal.current_window_state.mouse_state.reset_scroll_to_zero();

                        if callback_results.callbacks_update_screen == UpdateScreen::Redraw {
                            window.regenerate_styled_dom();
                            should_callback_render = true;
                        } else if nodes_to_check.need_relayout() {
                            window.restyle_and_relayout(); // relayout the DOM
                            should_callback_render = true;
                        }

                        if nodes_to_check.need_restyle() ||
                           callbacks_result.style_layout_properties_changed() ||
                           callbacks_result.focus_layout_properties_changed() ||
                           should_callback_render {
                            window.regenerate_display_list();
                            should_callback_render = true;
                        }

                        timers.extend(callback_results.timers.drain(..));
                        tasks.append(&mut callback_results.tasks);

                        // see if the callbacks modified the WindowState - if yes, re-determine the events
                        let current_window_save_state = window.internal.current_window_state.clone();
                        let window_state_changed_in_callbacks = window.synchronize_window_state_with_os(&callback_results.modified_window_state);
                        window.internal.previous_window_state = Some(current_window_save_state);
                        if window_state_changed_in_callbacks || force_regenerate_events { continue; } else { break; }
                    }

                    if should_scroll_render || should_callback_render {
                        window.display.request_redraw();
                    }
                },
                _ => { },
            }

            if !active_windows.is_empty() {

                *control_flow = ControlFlow::Wait;

                // If no timers / tasks are running, wait until next user event
                if timers.is_empty() && tasks.is_empty() {
                    *control_flow = ControlFlow::Wait;
                } else {
                    use azul_core::task::{run_all_timers, clean_up_finished_tasks};

                    // If timers are running, check whether they need to redraw
                    let should_redraw_timers = run_all_timers(&mut timers, &mut data, &mut resources);
                    let should_redraw_tasks = clean_up_finished_tasks(&mut tasks, &mut timers);
                    let should_redraw_timers_tasks = [should_redraw_timers, should_redraw_tasks].iter().any(|i| *i == UpdateScreen::Redraw);

                    if should_redraw_timers_tasks {
                        *control_flow = ControlFlow::Poll;
                        for (_, window) in active_windows.iter() {
                            window.display.window().request_redraw();
                        }
                    } else {
                        *control_flow = ControlFlow::WaitUntil(now + config.min_frame_duration);
                    }
                }

            } else {

                // Application shutdown

                use azul_core::gl::gl_textures_clear_opengl_cache;

                // NOTE: For some reason this is necessary, otherwise the renderer crashes on shutdown
                hidden_context.make_current();

                // Important: destroy all OpenGL textures before the shared
                // OpenGL context is destroyed.
                gl_textures_clear_opengl_cache();

                gl_context.disable(gl::FRAMEBUFFER_SRGB);
                gl_context.disable(gl::MULTISAMPLE);
                gl_context.disable(gl::POLYGON_SMOOTH);

                if let Some(renderer) = renderer.take() {
                    renderer.deinit();
                }

                *control_flow = ControlFlow::Exit;
            }
        })
    }
}

/// Updates the `FullWindowState` to the
fn process_window_event(event: GlutinWindowEvent, window_state: &mut FullWindowState) {
    use glutin::event::{WindowEvent, KeyboardInput, Touch, Event};
    match &event {
        WindowEvent::ModifiersChanged(modifier_state) => {
            for full_window_state in eld.full_window_states.values_mut() {
                update_keyboard_state_from_modifier_state(&mut full_window_state.keyboard_state, &modifier_state);
            }
            *control_flow = ControlFlow::Wait;
        },
        WindowEvent::Resized(physical_size) => {
            {
                // relayout, rebuild cached display list, reinitialize scroll states
                let mut windowed_context = eld.active_windows.get_mut(&glutin_window_id);
                let windowed_context = windowed_context.as_mut().unwrap();
                let dpi_factor = windowed_context.display.window().scale_factor();
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();

                full_window_state.size.winit_hidpi_factor = dpi_factor as f32;
                full_window_state.size.hidpi_factor = dpi_factor as f32;
                full_window_state.size.dimensions = winit_translate_physical_size(*physical_size).to_logical(dpi_factor as f32);

                windowed_context.display.make_current();
                windowed_context.display.windowed_context().unwrap().resize(*physical_size);
                windowed_context.display.make_not_current();
            }
            // TODO: Only rebuild UI if the resize is going across a resize boundary
            send_user_event(AzulUpdateEvent::RebuildUi { window_id }, &mut eld);
        },
        WindowEvent::ScaleFactorChanged { scale_factor, new_inner_size } => {
            let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
            full_window_state.size.winit_hidpi_factor = *scale_factor as f32;
            full_window_state.size.hidpi_factor = *scale_factor as f32;
        },
        WindowEvent::Moved(new_window_position) => {
            let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
            full_window_state.position = Some(winit_translate_physical_position(*new_window_position));
        },
        WindowEvent::CursorMoved { position, .. } => {
            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                let world_pos_x = position.x as f32 / full_window_state.size.hidpi_factor * full_window_state.size.winit_hidpi_factor;
                let world_pos_y = position.y as f32 / full_window_state.size.hidpi_factor * full_window_state.size.winit_hidpi_factor;
                full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(world_pos_x, world_pos_y));
            }
            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::CursorLeft { .. } => {
            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                full_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow;
            }
            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::CursorEntered { .. } => {
            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(0.0, 0.0));
            }
            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::KeyboardInput { input: KeyboardInput { state, virtual_keycode, scancode, .. }, .. } => {

            use crate::wr_translate::winit_translate::translate_virtual_keycode;
            use glutin::event::ElementState;

            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                match state {
                    ElementState::Pressed => {
                        if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                            full_window_state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
                            full_window_state.keyboard_state.current_virtual_keycode = Some(vk).into();
                        }
                        full_window_state.keyboard_state.pressed_scancodes.insert_hm_item(*scancode);
                        full_window_state.keyboard_state.current_char = None.into();
                    },
                    ElementState::Released => {
                        if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                            full_window_state.keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
                            full_window_state.keyboard_state.current_virtual_keycode = None.into();
                        }
                        full_window_state.keyboard_state.pressed_scancodes.remove_hm_item(scancode);
                        full_window_state.keyboard_state.current_char = None.into();
                    },
                }
            }

            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        // The char event is sliced inbetween a keydown and a keyup event, so the keyup
        // has to clear the character again
        WindowEvent::ReceivedCharacter(c) => {
            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                full_window_state.keyboard_state.current_char = Some((*c) as u32).into();
            }

            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::MouseInput { state, button, .. } => {

            {
                use glutin::event::{ElementState::*, MouseButton::*};
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();

                match state {
                    Pressed => {
                        match button {
                            Left => full_window_state.mouse_state.left_down = true,
                            Right => full_window_state.mouse_state.right_down = true,
                            Middle => full_window_state.mouse_state.middle_down = true,
                            _ => full_window_state.mouse_state.left_down = true,
                        }
                    },
                    Released => {
                        match button {
                            Left => full_window_state.mouse_state.left_down = false,
                            Right => full_window_state.mouse_state.right_down = false,
                            Middle => full_window_state.mouse_state.middle_down = false,
                            _ => full_window_state.mouse_state.left_down = false,
                        }
                    },
                }
            }

            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::MouseWheel { delta, .. } => {

            let should_scroll_render_from_input_events;

            {
                use glutin::event::MouseScrollDelta;

                const LINE_DELTA: f32 = 38.0;

                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();

                let (scroll_x_px, scroll_y_px) = match delta {
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                    MouseScrollDelta::LineDelta(x, y) => (x * LINE_DELTA, y * LINE_DELTA),
                };

                // TODO: "natural scrolling"?
                full_window_state.mouse_state.scroll_x = Some(-scroll_x_px).into();
                full_window_state.mouse_state.scroll_y = Some(-scroll_y_px).into();

                let window = eld.active_windows.get_mut(&glutin_window_id).unwrap();
                let hit_test_results = do_hit_test(window, &full_window_state, &eld.render_api);
                let scrolled_nodes = &window.internal.scrolled_nodes;
                let scroll_states = &mut window.internal.scroll_states;

                should_scroll_render_from_input_events = scrolled_nodes.values().any(|scrolled_node| {
                    update_scroll_state(full_window_state, scrolled_node, scroll_states, &hit_test_results)
                });
            }

            if should_scroll_render_from_input_events {
                send_user_event(AzulUpdateEvent::ScrollRender { window_id }, &mut eld);
            }
        },
        WindowEvent::Touch(Touch { phase, location, .. }) => {
            use glutin::event::TouchPhase::*;

            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                let mut windowed_context = eld.active_windows.get_mut(&glutin_window_id);
                let windowed_context = windowed_context.as_mut().unwrap();
                let dpi_factor = windowed_context.display.window().scale_factor();
                full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(winit_translate_physical_position(*location).to_logical(dpi_factor as f32));
            }

            match phase {
                Started => {
                    send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
                },
                Moved => {
                    // TODO: Do hit test and update window.internal.scroll_states!
                    send_user_event(AzulUpdateEvent::ScrollRender { window_id }, &mut eld);
                },
                Ended => {
                    send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
                },
                Cancelled => {
                    send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
                },
            }
        },
        WindowEvent::HoveredFile(file_path) => {
            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                full_window_state.hovered_file = Some(file_path.clone());
                full_window_state.dropped_file = None;
            }
            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::HoveredFileCancelled => {
            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                full_window_state.hovered_file = None;
                full_window_state.dropped_file = None;
            }
            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::DroppedFile(file_path) => {
            {
                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                full_window_state.hovered_file = None;
                full_window_state.dropped_file = Some(file_path.clone());
            }
            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
        },
        WindowEvent::Focused(false) => {
            let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
            full_window_state.keyboard_state.current_char = None.into();
            full_window_state.keyboard_state.pressed_virtual_keycodes.clear();
            full_window_state.keyboard_state.current_virtual_keycode = None.into();
            full_window_state.keyboard_state.pressed_scancodes.clear();
        },
        WindowEvent::CloseRequested => {
            send_user_event(AzulUpdateEvent::CloseWindow { window_id }, &mut eld);
        },
        _ => { },
    }
}

fn create_window(
    event_loop: &EventLoopWindowTarget<()>,
    fake_display: &mut FakeDisplay,
    active_windows: &mut BTreeMap<GlutinWindowId, Window>,
    app_resources: &mut AppResources,
    render_api: &mut WrApi,
) {

    let window = Window::new(
        render_api,
        fake_display,
        &event_loop,
        window_create_options,
        resources,
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

fn close_window(
    active_windows: &mut BTreeMap<GlutinWindowId, Window>,
    resources: &mut AppResources,
    render_api: &mut WrApi
) {
    // Close the window
    // TODO: Invoke callback to reject the window close event!

    use azul_core::gl::gl_textures_remove_active_pipeline;

    let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
        Some(s) => s.clone(),
        None => return,
    };

    let w = match active_windows.remove(&glutin_window_id) {
        Some(w) => w,
        None => return,
    };

    gl_textures_remove_active_pipeline(&w.internal.pipeline_id);
    resources.delete_pipeline(&w.internal.pipeline_id, eld.render_api);
    render_api.api.delete_document(wr_translate_document_id(w.internal.document_id));
}
use std::{
    rc::Rc,
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
use gleam::gl::{self, Gl, GLuint};
use webrender::{
    PipelineInfo as WrPipelineInfo,
    Renderer as WrRenderer,
    api::{
        Transaction as WrTransaction,
        units::{
            LayoutSize as WrLayoutSize,
            DeviceIntSize as WrDeviceIntSize,
        }
    },
};
#[cfg(feature = "logging")]
use log::LevelFilter;
use azul_css::{ColorU, HotReloadHandler};
use crate::{
    resources::WrApi,
    window::{Window, ScrollStates, HeadlessContextState}
};
use azul_core::{
    FastHashMap,
    window::{RendererType, WindowCreateOptions, WindowSize, DebugState, WindowState, FullWindowState},
    dom::{DomId, NodeId, ScrollTagId},
    gl::GlShader,
    traits::Layout,
    ui_state::UiState,
    ui_solver::ScrolledNodes,
    callbacks::{
        LayoutCallback, HitTestItem, Redraw, DontRedraw,
        ScrollPosition,
    },
    task::{Task, Timer, TimerId},
    window::{AzulUpdateEvent, CallbacksOfHitTest, KeyboardState, WindowId, CallCallbacksResult},
    callbacks::PipelineId,
    ui_description::UiDescription,
    ui_solver::LayoutResult,
    display_list::CachedDisplayList,
    app_resources::{
        AppResources, Epoch, FontId, ImageId, LoadedFont, ImmediateFontId,
        TextId, ImageSource, FontSource, CssImageId, ImageInfo,
    },
};

#[cfg(not(test))]
use crate::window::{ FakeDisplay };
#[cfg(not(test))]
use glutin::CreationError;
#[cfg(not(test))]
use webrender::api::HitTestFlags;
use webrender::api::units::WorldPoint;

#[cfg(test)]
use azul_core::app_resources::FakeRenderApi;

// Default clear color is white, to signify that there is rendering going on
// (otherwise, "transparent") backgrounds would be painted black.
const COLOR_WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 0 };

/// Graphical application that maintains some kind of application state
pub struct App<T> {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: T,
    /// Fonts, images and cached text that is currently loaded inside the app (window-independent).
    ///
    /// Accessing this field is often required to load new fonts or images, so instead of
    /// requiring the `FontHashMap`, a lot of functions just require the whole `AppResources` field.
    pub resources: AppResources,
    /// Currently running timers (polling functions, run on the main thread)
    pub timers: FastHashMap<TimerId, Timer<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub tasks: Vec<Task<T>>,
    /// Application configuration, whether to enable logging, etc.
    pub config: AppConfig,
    /// The window create options (only set at startup), get moved into the `.run_inner()` method
    /// No window is actually shown until the `.run_inner()` method is called.
    windows: BTreeMap<WindowId, WindowCreateOptions<T>>,
    /// The `Layout::layout()` callback, stored as a function pointer,
    /// There are multiple reasons for doing this (instead of requiring `T: Layout` everywhere):
    ///
    /// - It seperates the `Dom<T>` from the `Layout` trait, making it possible to split the
    ///   UI solving and styling into reusable crates
    /// - It's less typing work (prevents having to type `<T: Layout>` everywhere)
    /// - It's potentially more efficient to compile (less type-checking required)
    /// - It's a preparation for the C ABI, in which traits don't exist (for language bindings).
    ///   In the C ABI "traits" are simply structs with function pointers (and void* instead of T)
    layout_callback: LayoutCallback<T>,
    /// The actual renderer of this application
    #[cfg(not(test))]
    fake_display: FakeDisplay,
    #[cfg(test)]
    render_api: FakeRenderApi,
}

impl<T> App<T> {
    impl_task_api!();
    impl_image_api!(resources);
    impl_font_api!(resources);
}

/// Configuration for optional features, such as whether to enable logging or panic hooks
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(not(feature = "logging"), derive(Copy))]
pub struct AppConfig {
    /// If enabled, logs error and info messages.
    ///
    /// Default is `Some(LevelFilter::Error)` to log all errors by default
    #[cfg(feature = "logging")]
    pub enable_logging: Option<LevelFilter>,
    /// Path to the output log if the logger is enabled
    #[cfg(feature = "logging")]
    pub log_file_path: Option<String>,
    /// If the app crashes / panics, a window with a message box pops up.
    /// Setting this to `false` disables the popup box.
    #[cfg(feature = "logging")]
    pub enable_visual_panic_hook: bool,
    /// If this is set to `true` (the default), a backtrace + error information
    /// gets logged to stdout and the logging file (only if logging is enabled).
    #[cfg(feature = "logging")]
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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            #[cfg(feature = "logging")]
            enable_logging: Some(LevelFilter::Error),
            #[cfg(feature = "logging")]
            log_file_path: None,
            #[cfg(feature = "logging")]
            enable_visual_panic_hook: true,
            #[cfg(feature = "logging")]
            enable_logging_on_panic: true,
            enable_tab_navigation: true,
            renderer_type: RendererType::default(),
            debug_state: DebugState::default(),
            background_color: COLOR_WHITE,
            min_frame_duration: Duration::from_millis(30),
        }
    }
}

impl<T: Layout> App<T> {

    #[cfg(not(test))]
    #[allow(unused_variables)]
    /// Create a new, empty application. This does not open any windows.
    pub fn new(initial_data: T, app_config: AppConfig) -> Result<Self, CreationError> {

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
            let mut fake_display = FakeDisplay::new(app_config.renderer_type)?;
            if let Some(r) = &mut fake_display.renderer {
                use crate::wr_translate::set_webrender_debug_flags;
                set_webrender_debug_flags(r, &app_config.debug_state);
            }
            Ok(Self {
                windows: BTreeMap::new(),
                data: initial_data,
                resources: AppResources::default(),
                timers: FastHashMap::default(),
                tasks: Vec::new(),
                config: app_config,
                layout_callback: T::layout,
                fake_display,
            })
        }

        #[cfg(test)] {
           Ok(Self {
               windows: BTreeMap::new(),
               data: initial_data,
               resources: AppResources::default(),
               timers: FastHashMap::default(),
               tasks: Vec::new(),
               config: app_config,
               layout_callback: T::layout,
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

impl<T: 'static> App<T> {

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn add_window(&mut self, create_options: WindowCreateOptions<T>) {
        self.windows.insert(WindowId::new(), create_options);
    }

    /// Start the rendering loop for the currently added windows. The run() function
    /// takes one `WindowCreateOptions` as an argument, which is the "root" window, i.e.
    /// the main application window.
    #[cfg(not(test))]
    pub fn run(mut self, root_window: WindowCreateOptions<T>) -> ! {
        self.add_window(root_window);
        self.run_inner()
    }

    #[cfg(not(test))]
    fn run_inner(self) -> ! {

        use glutin::{
            event::{WindowEvent, KeyboardInput, Touch, Event},
            event_loop::ControlFlow,
        };
        use azul_core::window::{CursorPosition, LogicalPosition};
        use crate::wr_translate::winit_translate::{
            translate_winit_logical_size, translate_winit_logical_position,
        };

        let App { mut data, mut resources, mut timers, mut tasks, config, windows, layout_callback, mut fake_display } = self;

        let window_states = initialize_window_states(&windows);
        let initialized_windows = initialize_windows(windows, &mut fake_display, &mut resources, &config);

        let (mut active_windows, mut window_id_mapping, mut reverse_window_id_mapping) = initialized_windows;
        let mut full_window_states = initialize_full_window_states(&reverse_window_id_mapping, &window_states);
        let mut ui_state_cache = initialize_ui_state_cache(&data, fake_display.gl_context.clone(), &resources, &active_windows, &mut full_window_states, layout_callback);
        let mut ui_description_cache = initialize_ui_description_cache(&mut ui_state_cache, &mut full_window_states);

        let FakeDisplay { mut render_api, mut renderer, mut hidden_context, hidden_event_loop, gl_context } = fake_display;

        // TODO: When the callbacks are run, rebuild the default callbacks again,
        // otherwise there could be a memory "leak" as default callbacks only
        // get added and never removed.

        let mut eld = EventLoopData {
            data: &mut data,
            event_loop_target: None,
            resources: &mut resources,
            timers: &mut timers,
            tasks: &mut tasks,
            config: &config,
            layout_callback: layout_callback,
            active_windows: &mut active_windows,
            window_id_mapping: &mut window_id_mapping,
            reverse_window_id_mapping: &mut reverse_window_id_mapping,
            full_window_states: &mut full_window_states,
            ui_state_cache: &mut ui_state_cache,
            ui_description_cache: &mut ui_description_cache,
            render_api: &mut render_api,
            renderer: &mut renderer,
            hidden_context: &mut hidden_context,
            gl_context: gl_context.clone(),
        };

        let window_keys = eld.reverse_window_id_mapping.keys().cloned().collect::<Vec<_>>();
        for window_id in window_keys {
            send_user_event(AzulUpdateEvent::RelayoutUi { window_id }, &mut eld);
        }

        #[cfg(debug_assertions)]
        let mut last_style_reload = Instant::now();

        hidden_event_loop.run(move |event, event_loop_target, control_flow| {

            let _now = Instant::now();

            match event {
                Event::DeviceEvent { .. } => {
                    // ignore high-frequency events
                    *control_flow = ControlFlow::Wait;
                    return;
                },
                Event::WindowEvent { event, window_id } => {

                    let mut eld = EventLoopData {
                        data: &mut data,
                        event_loop_target: Some(event_loop_target),
                        resources: &mut resources,
                        timers: &mut timers,
                        tasks: &mut tasks,
                        config: &config,
                        layout_callback: layout_callback,
                        active_windows: &mut active_windows,
                        window_id_mapping: &mut window_id_mapping,
                        reverse_window_id_mapping: &mut reverse_window_id_mapping,
                        full_window_states: &mut full_window_states,
                        ui_state_cache: &mut ui_state_cache,
                        ui_description_cache: &mut ui_description_cache,
                        render_api: &mut render_api,
                        renderer: &mut renderer,
                        hidden_context: &mut hidden_context,
                        gl_context: gl_context.clone(),
                    };

                    let glutin_window_id = window_id;
                    let window_id = match eld.window_id_mapping.get(&glutin_window_id) {
                        Some(s) => *s,
                        None => {
                            // glutin also sends events for the root window here!
                            // However, the root window only exists as a "hidden" window
                            // to share the same OpenGL context across all windows.
                            // In this case, simply ignore the event.
                            *control_flow = ControlFlow::Wait;
                            return;
                        }
                    };

                    match &event {
                        WindowEvent::Resized(physical_size) => {
                            {
                                // relayout, rebuild cached display list, reinitialize scroll states
                                let mut windowed_context = eld.active_windows.get_mut(&glutin_window_id);
                                let windowed_context = windowed_context.as_mut().unwrap();
                                let dpi_factor = windowed_context.display.window().scale_factor();
                                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();

                                full_window_state.size.winit_hidpi_factor = dpi_factor as f32;
                                full_window_state.size.hidpi_factor = dpi_factor as f32;
                                full_window_state.size.dimensions = translate_winit_logical_size(physical_size.to_logical(dpi_factor));

                                windowed_context.display.make_current();
                                windowed_context.display.windowed_context().unwrap().resize(*physical_size);
                                windowed_context.display.make_not_current();
                            }
                            // TODO: Only rebuild UI if the resize is going across a resize boundary
                            send_user_event(AzulUpdateEvent::RebuildUi { window_id }, &mut eld);
                        },
                        WindowEvent::ScaleFactorChanged{scale_factor, new_inner_size: _} => {
                            let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.size.winit_hidpi_factor = *scale_factor as f32;
                            full_window_state.size.hidpi_factor = *scale_factor as f32;
                        },
                        WindowEvent::Moved(new_window_position) => {
                            let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.position = Some(translate_winit_logical_position(new_window_position.to_logical(full_window_state.size.winit_hidpi_factor as f64)));
                        },
                        WindowEvent::CursorMoved { position,  .. } => {
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
                        WindowEvent::KeyboardInput { input: KeyboardInput { state, virtual_keycode, scancode,  .. }, .. } => {

                            use crate::wr_translate::winit_translate::translate_virtual_keycode;
                            use glutin::event::ElementState;

                            {
                                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();

                                match state {
                                    ElementState::Pressed => {
                                        if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                                            full_window_state.keyboard_state.pressed_virtual_keycodes.insert(vk);
                                            full_window_state.keyboard_state.current_virtual_keycode = Some(vk);
                                        }
                                        full_window_state.keyboard_state.pressed_scancodes.insert(*scancode);
                                        full_window_state.keyboard_state.current_char = None;
                                    },
                                    ElementState::Released => {
                                        if let Some(vk) = virtual_keycode.map(translate_virtual_keycode) {
                                            full_window_state.keyboard_state.pressed_virtual_keycodes.remove(&vk);
                                            full_window_state.keyboard_state.current_virtual_keycode = None;
                                        }
                                        full_window_state.keyboard_state.pressed_scancodes.remove(scancode);
                                        full_window_state.keyboard_state.current_char = None;
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
                                full_window_state.keyboard_state.current_char = Some(*c);
                            }

                            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
                        },
                        WindowEvent::MouseInput { state, button, .. } => {

                            {

                                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                                match state {
                                    glutin::event::ElementState::Pressed => {
                                        match button {
                                            glutin::event::MouseButton::Left   => {full_window_state.mouse_state.left_down = true;},
                                            glutin::event::MouseButton::Right  => {full_window_state.mouse_state.right_down = true;},
                                            glutin::event::MouseButton::Middle => {full_window_state.mouse_state.middle_down = true;},
                                            _ => {}
                                        }
                                    },
                                    glutin::event::ElementState::Released => {
                                        match button {
                                            glutin::event::MouseButton::Left   => {full_window_state.mouse_state.left_down = false;},
                                            glutin::event::MouseButton::Right  => {full_window_state.mouse_state.right_down = false;},
                                            glutin::event::MouseButton::Middle => {full_window_state.mouse_state.middle_down = false;},
                                            _ => {}
                                        }
                                    },
                                };
                            }

                            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
                        },
                        WindowEvent::ModifiersChanged (modifiers) => {
                            {

                                let full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                                update_keyboard_state_from_modifier_state(&mut full_window_state.keyboard_state, modifiers);
                            }
                            send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);

                        },
                        WindowEvent::MouseWheel { delta, .. } => {

                            let should_scroll_render_from_input_events;

                            {
                                use glutin::event::MouseScrollDelta;
                                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();

                                const LINE_DELTA: f32 = 38.0;


                                let (scroll_x_px, scroll_y_px) = match delta {
                                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                                    MouseScrollDelta::LineDelta(x, y) => (x * LINE_DELTA, y * LINE_DELTA),
                                };

                                // TODO: "natural scrolling"?
                                full_window_state.mouse_state.scroll_x = Some(-scroll_x_px);
                                full_window_state.mouse_state.scroll_y = Some(-scroll_y_px);

                                let window = eld.active_windows.get_mut(&glutin_window_id).unwrap();
                                let hit_test_results = do_hit_test(window, &full_window_state, &eld.render_api);
                                let scrolled_nodes = &window.internal.scrolled_nodes;
                                let scroll_states = &mut window.internal.scroll_states;

                                should_scroll_render_from_input_events = scrolled_nodes.values().any(|scrolled_node| {
                                    update_scroll_state(full_window_state, scrolled_node, scroll_states, &hit_test_results)
                                });
                            }

                            if should_scroll_render_from_input_events {
                                send_user_event(AzulUpdateEvent::UpdateScrollStates { window_id }, &mut eld);
                            }
                        },
                        WindowEvent::Touch(Touch { phase, location, .. }) => {
                            use glutin::event::TouchPhase::*;

                            {
                                let mut full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                                full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(translate_winit_logical_position(location.to_logical(full_window_state.size.winit_hidpi_factor as f64)));
                            }

                            match phase {
                                Started => {
                                    send_user_event(AzulUpdateEvent::DoHitTest { window_id }, &mut eld);
                                },
                                Moved => {
                                    // TODO: Do hit test and update window.internal.scroll_states!
                                    send_user_event(AzulUpdateEvent::UpdateScrollStates { window_id }, &mut eld);
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
                            full_window_state.keyboard_state.current_char = None;
                            full_window_state.keyboard_state.pressed_virtual_keycodes.clear();
                            full_window_state.keyboard_state.current_virtual_keycode = None;
                            full_window_state.keyboard_state.pressed_scancodes.clear();
                        },
                        // WindowEvent::RedrawRequested => {

                        //     println!("rerender!");
                        //     println!("-------");
                        //     let full_window_state = eld.full_window_states.get(&glutin_window_id).unwrap();
                        //     let mut windowed_context = eld.active_windows.get_mut(&glutin_window_id);
                        //     let mut windowed_context = windowed_context.as_mut().unwrap();

                        //     let pipeline_id = windowed_context.internal.pipeline_id;

                        //     // Render + swap the screen (call webrender + draw to texture)
                        //     render_inner(
                        //         &mut windowed_context,
                        //         &full_window_state,
                        //         &mut eld.hidden_context,
                        //         &mut eld.render_api,
                        //         eld.renderer.as_mut().unwrap(),
                        //         eld.gl_context.clone(),
                        //         WrTransaction::new(),
                        //         eld.config.background_color,
                        //     );

                        //     // After rendering + swapping, remove the unused OpenGL textures
                        //     clean_up_unused_opengl_textures(eld.renderer.as_mut().unwrap().flush_pipeline_info(), &pipeline_id);
                        // },
                        WindowEvent::CloseRequested => {
                            send_user_event(AzulUpdateEvent::CloseWindow { window_id }, &mut eld);
                        },
                        _ => { },
                    }
                },
                _ => { },
            }

            // Application shutdown
            if active_windows.is_empty() {

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
            } else {

                *control_flow = ControlFlow::Wait;

                // Reload CSS if necessary
                #[cfg(debug_assertions)] {
                    const DONT_FORCE_CSS_RELOAD: bool = false;

                    let mut should_update = false;

                    for (glutin_window_id, window) in active_windows.iter() {
                        let full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                        let hot_reload_handler = window.hot_reload_handler.as_ref().map(|hr| &hr.0);
                        if hot_reload_css(full_window_state, hot_reload_handler, &mut last_style_reload, DONT_FORCE_CSS_RELOAD).0 {
                            should_update = true;
                        }
                    }

                    if should_update {
                        let mut eld = EventLoopData {
                            data: &mut data,
                            event_loop_target: Some(event_loop_target),
                            resources: &mut resources,
                            timers: &mut timers,
                            tasks: &mut tasks,
                            config: &config,
                            layout_callback: layout_callback,
                            active_windows: &mut active_windows,
                            window_id_mapping: &mut window_id_mapping,
                            reverse_window_id_mapping: &mut reverse_window_id_mapping,
                            full_window_states: &mut full_window_states,
                            ui_state_cache: &mut ui_state_cache,
                            ui_description_cache: &mut ui_description_cache,
                            render_api: &mut render_api,
                            renderer: &mut renderer,
                            hidden_context: &mut hidden_context,
                            gl_context: gl_context.clone(),
                        };

                        for window_id in eld.window_id_mapping.clone().values() {
                            send_user_event(AzulUpdateEvent::RebuildUi { window_id: *window_id }, &mut eld);
                        }

                        for (_, window) in eld.active_windows.iter() {
                            window.display.window().request_redraw();
                        }
                    }
                }

                /*
                // If no timers / tasks are running, wait until next user event
                if timers.is_empty() && tasks.is_empty() {
                    *control_flow = ControlFlow::Wait;
                } else {
                    use azul_core::task::{run_all_timers, clean_up_finished_tasks};

                    // If timers are running, check whether they need to redraw
                    let should_redraw_timers = run_all_timers(&mut timers, &mut data, &mut resources);
                    let should_redraw_tasks = clean_up_finished_tasks(&mut tasks, &mut timers);
                    let should_redraw_timers_tasks = [should_redraw_timers, should_redraw_tasks].iter().any(|i| *i == Redraw);
                    if should_redraw_timers_tasks {
                        *control_flow = ControlFlow::Poll;
                        redraw_all_windows!();
                    } else {
                        *control_flow = ControlFlow::WaitUntil(now + config.min_frame_duration);
                    }
                }
                */
            }
        })
    }
}

#[cfg(not(test))]
struct EventLoopData<'a, T> {
    data: &'a mut T,
    event_loop_target: Option<&'a GlutinEventLoopWindowTarget<()>>,
    resources: &'a mut AppResources,
    timers: &'a mut FastHashMap<TimerId, Timer<T>>,
    tasks: &'a mut Vec<Task<T>>,
    config: &'a AppConfig,
    layout_callback: LayoutCallback<T>,
    active_windows: &'a mut BTreeMap<GlutinWindowId, Window<T>>,
    window_id_mapping: &'a mut BTreeMap<GlutinWindowId, WindowId>,
    reverse_window_id_mapping: &'a mut BTreeMap<WindowId, GlutinWindowId>,
    full_window_states: &'a mut BTreeMap<GlutinWindowId, FullWindowState>,
    ui_state_cache: &'a mut BTreeMap<GlutinWindowId, BTreeMap<DomId, UiState<T>>>,
    ui_description_cache: &'a mut BTreeMap<GlutinWindowId, BTreeMap<DomId, UiDescription>>,
    render_api: &'a mut WrApi,
    renderer: &'a mut Option<WrRenderer>,
    hidden_context: &'a mut HeadlessContextState,
    gl_context: Rc<dyn Gl>,
}

/// Similar to `events_loop_proxy.send_user_event(ev)`, however, when dispatching events using glutin,
/// the "user events" get interleaved with system events, they are also reordered and batched in a
/// non-predictable way, which leads to redrawing bugs.
///
/// The function recurses until there's nothing left to do, i.e. sending a `send_user_event(DoHitTest { }, eld)`
/// will internally call the function again with `send_user_event(RebuildUi { })` if necessary and so on.
#[cfg(not(test))]
fn send_user_event<'a, T>(
    ev: AzulUpdateEvent<T>,
    eld: &mut EventLoopData<'a, T>,
) {

    use azul_core::window::AzulUpdateEvent::*;

    macro_rules! redraw_all_windows {() => {
        for (_, window) in eld.active_windows.iter() {
            window.display.window().request_redraw();
        }
    };}

    match ev {
        CreateWindow { window_create_options } => {

            let event_loop_target = match &eld.event_loop_target {
                Some(s) => s,
                None => return,
            };

            let full_window_state: FullWindowState = window_create_options.state.clone().into();
            let window = Window::new(
                eld.render_api,
                eld.hidden_context.headless_context_not_current().unwrap(),
                event_loop_target,
                window_create_options,
                eld.config.background_color,
                eld.resources,
            );

            let window = match window {
                Ok(o) => o,
                Err(e) => {
                    error!("Error initializing window: {}", e);
                    return;
                }
            };

            let glutin_window_id = window.display.window().id();
            let window_id = window.id;

            eld.full_window_states.insert(glutin_window_id, full_window_state);

            #[cfg(debug_assertions)] {
                const FORCE_CSS_RELOAD: bool = true;
                let full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                let hot_reload_handler = window.hot_reload_handler.as_ref().map(|hr| &hr.0);
                let _ = hot_reload_css(full_window_state, hot_reload_handler, &mut Instant::now(), FORCE_CSS_RELOAD);
            }

            let dom_id_map = call_layout_fn(
                eld.data,
                eld.gl_context.clone(),
                eld.resources,
                &eld.full_window_states[&glutin_window_id],
                eld.layout_callback,
            );

            eld.active_windows.insert(glutin_window_id, window);
            eld.ui_state_cache.insert(glutin_window_id, dom_id_map);
            eld.ui_description_cache.insert(glutin_window_id,
                cascade_style(
                    eld.ui_state_cache.get_mut(&glutin_window_id).unwrap(),
                    eld.full_window_states.get_mut(&glutin_window_id).unwrap()
                )
            );
            eld.window_id_mapping.insert(glutin_window_id, window_id);
            eld.reverse_window_id_mapping.insert(window_id, glutin_window_id);
        },
        CloseWindow { window_id } => {

            // Close the window
            // TODO: Invoke callback to reject the window close event!

            use azul_core::gl::gl_textures_remove_active_pipeline;

            let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                Some(s) => s.clone(),
                None => return,
            };

            let window = eld.active_windows.remove(&glutin_window_id);
            let window_id = eld.window_id_mapping.remove(&glutin_window_id);
            if let Some(wid) = window_id {
                eld.reverse_window_id_mapping.remove(&wid);
            }
            eld.full_window_states.remove(&glutin_window_id);
            eld.ui_state_cache.remove(&glutin_window_id);
            eld.ui_description_cache.remove(&glutin_window_id);

            let w = match window {
                Some(w) => w,
                None => return,
            };

            gl_textures_remove_active_pipeline(&w.internal.pipeline_id);
            eld.resources.delete_pipeline(&w.internal.pipeline_id, eld.render_api);
            eld.render_api.api.delete_document(w.internal.document_id);
        },
        DoHitTest { window_id } => {

            println!("doing hit test!");

            // Hit test if any nodes were hit, see if any callbacks need to be called

            let mut callbacks_update_screen = false;
            let mut callbacks_set_new_focus_target = false;
            let mut callbacks_hover_restyle = false;
            let mut callbacks_hover_relayout = false;
            let mut nodes_were_scrolled_from_callbacks = false;
            let should_call_callbacks;
            let needs_relayout_anyways;
            let needs_redraw_anyways;

            {
                let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                    Some(s) => s.clone(),
                    None => return,
                };

                let ui_state = &eld.ui_state_cache[&glutin_window_id];
                let ui_description = &eld.ui_description_cache[&glutin_window_id];

                let events = {
                    let full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                    let window = &eld.active_windows[&glutin_window_id];
                    let hit_test_results = do_hit_test(window, &full_window_state, &eld.render_api);
                    determine_events(&hit_test_results, full_window_state, ui_state)
                };

                // TODO: Add all off-click callbacks for all non-hit windows here!
                should_call_callbacks = events.values().any(|e| e.should_call_callbacks());
                needs_relayout_anyways = events.values().any(|e| e.needs_relayout_anyways);
                needs_redraw_anyways = events.values().any(|e| e.needs_redraw_anyways);

                if should_call_callbacks {

                    // call callbacks
                    println!("calling callbacks!");

                    let active_windows = &mut *eld.active_windows;
                    let data = &mut *eld.data;
                    let timers = &mut *eld.timers;
                    let tasks = &mut *eld.tasks;
                    let full_window_states = &mut *eld.full_window_states;
                    let gl_context = eld.gl_context.clone();
                    let resources = &mut *eld.resources;

                    let call_callbacks_results = active_windows.values_mut().map(|window| {
                        let scroll_states = window.internal.get_current_scroll_states(&ui_state);
                        call_callbacks(
                            data,
                            &events,
                            ui_state,
                            ui_description,
                            timers,
                            tasks,
                            &scroll_states,
                            &mut window.internal.scroll_states,
                            full_window_states.get_mut(&glutin_window_id).unwrap(),
                            &window.internal.layout_result.solved_layouts,
                            &window.internal.scrolled_nodes,
                            &window.internal.cached_display_list,
                            gl_context.clone(),
                            resources,
                            &*window.display.window(),
                        )
                    }).collect::<Vec<_>>();

                    // Application state has been updated, now figure out what to update from the callbacks

                    // TODO: .any() or .all() ??
                    callbacks_update_screen = call_callbacks_results.iter().any(|cr| cr.callbacks_update_screen == Redraw);
                    callbacks_set_new_focus_target = call_callbacks_results.iter().any(|cr| cr.needs_restyle_focus_changed);
                    callbacks_hover_restyle = call_callbacks_results.iter().any(|cr| cr.needs_restyle_hover_active);
                    callbacks_hover_relayout = call_callbacks_results.iter().any(|cr| cr.needs_relayout_hover_active);
                    nodes_were_scrolled_from_callbacks = call_callbacks_results.iter().any(|cr| cr.should_scroll_render);
                }

            } // end of borrowing eld

            if should_call_callbacks {
                if callbacks_update_screen {
                    eld.reverse_window_id_mapping.clone().keys().for_each(|window_id| {
                        send_user_event(AzulUpdateEvent::RebuildUi { window_id: *window_id }, eld);
                    });
                } else if callbacks_set_new_focus_target {
                    send_user_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: false }, eld);
                } else if callbacks_hover_restyle {
                    send_user_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: callbacks_hover_relayout }, eld);
                } else if nodes_were_scrolled_from_callbacks {
                    send_user_event(AzulUpdateEvent::UpdateScrollStates { window_id }, eld);
                }
            } else if needs_relayout_anyways {
                send_user_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: false }, eld);
            } else if needs_redraw_anyways {
                send_user_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: true }, eld);
            }
        },
        RebuildUi { window_id } => {
            println!("rebuild ui!");

            // Call the .layout() function, build UiState
            {
                let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                    Some(s) => s.clone(),
                    None => return,
                };

                let full_window_state = &eld.full_window_states[&glutin_window_id];
                let new_ui_state = call_layout_fn(
                    &*eld.data,
                    eld.gl_context.clone(),
                    &*eld.resources,
                    full_window_state,
                    eld.layout_callback,
                );

                *eld.ui_state_cache.get_mut(&glutin_window_id).unwrap() = new_ui_state;
            } // end of borrowing eld

            // optimization: create diff to previous UI State:
            // - only restyle the nodes that were added / removed
            // - if diff is empty (same UI), skip restyle, go straight to re-layouting
            send_user_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: false }, eld);
        },
        RestyleUi { window_id, skip_layout } => {

            println!("restyle ui!");

            // Cascade the CSS to the HTML nodes
            {
                let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                    Some(s) => s.clone(),
                    None => return,
                };

                let full_window_state = eld.full_window_states.get_mut(&glutin_window_id).unwrap();
                let ui_state = eld.ui_state_cache.get_mut(&glutin_window_id).unwrap();
                *eld.ui_description_cache.get_mut(&glutin_window_id).unwrap() = cascade_style(ui_state, full_window_state);
            } // end of borrowing eld

            // in cases like `:hover` and `:active`, layouting can be skipped
            // (if it is known that the re-styling doesn't modify the layout)
            if skip_layout {
                send_user_event(AzulUpdateEvent::RebuildDisplayList { window_id }, eld);
            } else {
                send_user_event(AzulUpdateEvent::RelayoutUi { window_id }, eld);
            }
        },
        RelayoutUi { window_id } => {

            use azul_core::display_list::SolvedLayout;

            println!("relayout ui!");

            // Layout the CSSOM
            {
                let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                    Some(s) => s.clone(),
                    None => return,
                };

                let window = eld.active_windows.get_mut(&glutin_window_id).unwrap();
                let full_window_state = &eld.full_window_states[&glutin_window_id];

                // Make sure unused scroll states are garbage collected.
                window.internal.scroll_states.remove_unused_scroll_states();
                eld.hidden_context.make_not_current();
                window.display.make_current();

                let SolvedLayout { solved_layout_cache, gl_texture_cache } = SolvedLayout::new(
                    window.internal.epoch,
                    window.internal.pipeline_id,
                    full_window_state,
                    eld.gl_context.clone(),
                    eld.render_api,
                    eld.resources,
                    eld.ui_state_cache.get_mut(&glutin_window_id).unwrap(),
                    eld.ui_description_cache.get_mut(&glutin_window_id).unwrap(),
                    azul_core::gl::insert_into_active_gl_textures,
                    azul_layout::ui_solver::do_the_layout,
                    crate::resources::font_source_get_bytes,
                    crate::resources::image_source_get_bytes,
                );

                window.display.make_not_current();
                eld.hidden_context.make_current();
                eld.hidden_context.make_not_current();

                window.internal.layout_result = solved_layout_cache;
                window.internal.gl_texture_cache = gl_texture_cache;
            } // end of borrowing eld

            // optimization with diff:
            // - only relayout the nodes that were added / removed
            // - if diff is empty (same UI), skip relayout, go straight to rebuilding display list
            send_user_event(AzulUpdateEvent::RebuildDisplayList { window_id }, eld);
        },
        RebuildDisplayList { window_id } => {

            println!("rebuild DL!");

            // Build the display list
            {
                let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                    Some(s) => s.clone(),
                    None => return,
                };

                let window = eld.active_windows.get_mut(&glutin_window_id).unwrap();
                let full_window_state = &eld.full_window_states[&glutin_window_id];

                let cached_display_list = CachedDisplayList::new(
                    window.internal.epoch,
                    window.internal.pipeline_id,
                    &full_window_state,
                    &eld.ui_state_cache[&glutin_window_id],
                    &window.internal.layout_result,
                    &window.internal.gl_texture_cache,
                    &eld.resources,
                );

                // optimization with diff:
                // - only rebuild the nodes that were added / removed
                // - if diff is empty (same UI), skip rebuilding the display list, go straight to sending the DL

                window.internal.cached_display_list = cached_display_list;
            } // end borrowing &mut eld

            send_user_event(AzulUpdateEvent::SendDisplayListToWebRender { window_id }, eld);
        },
        SendDisplayListToWebRender { window_id } => {

            println!("send display list!");

            // Build the display list
            {
                let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                    Some(s) => s.clone(),
                    None => return,
                };

                let window = eld.active_windows.get_mut(&glutin_window_id).unwrap();
                let full_window_state = &eld.full_window_states[&glutin_window_id];

                send_display_list_to_webrender(
                    window,
                    full_window_state,
                    eld.render_api,
                );

                send_user_event(AzulUpdateEvent::UpdateScrollStates { window_id }, eld);

            } // end borrowing &mut eld

            redraw_all_windows!();
        },
        UpdateScrollStates { window_id } => {
            // Synchronize all the scroll states from window.internal.scroll_states with webrender
            println!("update scroll states!");
            let glutin_window_id = match eld.reverse_window_id_mapping.get(&window_id) {
                Some(s) => s.clone(),
                None => return,
            };

            let window = eld.active_windows.get_mut(&glutin_window_id).unwrap();
            let mut txn = WrTransaction::new();
            scroll_all_nodes(&mut window.internal.scroll_states, &mut txn);
            eld.render_api.api.send_transaction(window.internal.document_id, txn);
        },
        UpdateAnimations { window_id } => {
            // send transaction to update animations in WR
            // if no other events happened, skip to SendDisplayListToWebRender step
            send_user_event(AzulUpdateEvent::SendDisplayListToWebRender { window_id }, eld);
        },
        UpdateImages { window_id } => {
            // send transaction to update images in WR
            // if no other events happened, skip to SendDisplayListToWebRender step
            send_user_event(AzulUpdateEvent::SendDisplayListToWebRender { window_id }, eld);
        },
    }
}

fn update_keyboard_state_from_modifier_state(keyboard_state: &mut KeyboardState, modifiers: &GlutinModifiersState) {
    keyboard_state.shift_down = modifiers.shift();
    keyboard_state.ctrl_down = modifiers.ctrl();
    keyboard_state.alt_down = modifiers.alt();
    keyboard_state.super_down = modifiers.logo();
}

fn initialize_window_states<T>(
    window_create_options: &BTreeMap<WindowId, WindowCreateOptions<T>>,
) -> BTreeMap<WindowId, WindowState> {
    window_create_options.iter().map(|(id, s)| (*id, s.state.clone())).collect()
}

/// Creates the intial windows on the screen and returns a mapping from
/// the (azul-internal) WindowId to the (glutin-internal) WindowId.
///
/// Theoretically, this mapping isn't used anywhere else, but it might
/// be useful for future refactoring.
#[cfg(not(test))]
fn initialize_windows<T>(
    window_create_options: BTreeMap<WindowId, WindowCreateOptions<T>>,
    fake_display: &mut FakeDisplay,
    app_resources: &mut AppResources,
    config: &AppConfig,
) -> (
    BTreeMap<GlutinWindowId, Window<T>>,
    BTreeMap<GlutinWindowId, WindowId>,
    BTreeMap<WindowId, GlutinWindowId>
) {

    let windows = window_create_options.into_iter().filter_map(|(window_id, window_create_options)| {
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

    let window_id_mapping = windows.keys().cloned()
        .map(|(window_id, glutin_window_id)| (glutin_window_id, window_id))
        .collect();

    let reverse_window_id_mapping = windows.keys().cloned()
        .map(|(window_id, glutin_window_id)| (window_id, glutin_window_id))
        .collect();

    let windows = windows.into_iter()
        .map(|((_, glutin_window_id), window)| (glutin_window_id, window))
        .collect();

    (windows, window_id_mapping, reverse_window_id_mapping)
}

fn initialize_full_window_states(
    active_window_ids: &BTreeMap<WindowId, GlutinWindowId>,
    window_states: &BTreeMap<WindowId, WindowState>,
) -> BTreeMap<GlutinWindowId, FullWindowState> {
    active_window_ids.iter().filter_map(|(window_id, glutin_window_id)| {
        let window_state = window_states.get(window_id)?;
        let full_window_state: FullWindowState = window_state.clone().into();
        Some((*glutin_window_id, full_window_state))
    }).collect()
}

#[cfg(not(test))]
fn initialize_ui_state_cache<T>(
    data: &T,
    gl_context: Rc<dyn Gl>,
    app_resources: &AppResources,
    windows: &BTreeMap<GlutinWindowId, Window<T>>,
    full_window_states: &mut BTreeMap<GlutinWindowId, FullWindowState>,
    layout_callback: LayoutCallback<T>,
) -> BTreeMap<GlutinWindowId, BTreeMap<DomId, UiState<T>>> {


    let mut ui_state_map = BTreeMap::new();

    for (glutin_window_id, window) in windows {
        DomId::reset();

        #[cfg(debug_assertions)] {
            const FORCE_CSS_RELOAD: bool = true;
            let full_window_state = full_window_states.get_mut(glutin_window_id).unwrap();
            let hot_reload_handler = window.hot_reload_handler.as_ref().map(|hr| &hr.0);
            let _ = hot_reload_css(full_window_state, hot_reload_handler, &mut Instant::now(), FORCE_CSS_RELOAD);
        }

        let full_window_state = full_window_states.get(glutin_window_id).unwrap();
        let dom_id_map = call_layout_fn(
            data,
            gl_context.clone(),
            app_resources,
            &full_window_state,
            layout_callback,
        );
        ui_state_map.insert(*glutin_window_id, dom_id_map);
    }

    DomId::reset();

    ui_state_map
}

/// Returns (whether the screen should update, whether the CSS had an error).
#[cfg(debug_assertions)]
fn hot_reload_css(
    full_window_state: &mut FullWindowState,
    hot_reload_handler: Option<&Box<dyn HotReloadHandler>>,
    last_style_reload: &mut Instant,
    force_css_reload: bool,
) -> (bool, bool) {

    let hot_reload_result = crate::css::hot_reload_css(
        &mut full_window_state.css,
        hot_reload_handler,
        last_style_reload,
        force_css_reload,
    );

    match hot_reload_result {
        Ok(has_reloaded) => (has_reloaded, false),
        Err(css_error) => {
            println!("{}\n----\n", css_error);
            (true, true)
        },
    }
}

fn call_layout_fn<T>(
    data: &T,
    gl_context: Rc<dyn Gl>,
    app_resources: &AppResources,
    full_window_state: &FullWindowState,
    layout_callback: LayoutCallback<T>,
) -> BTreeMap<DomId, UiState<T>> {

    use azul_core::callbacks::LayoutInfo;

    // Any top-level DOM has no "parent", parents are only relevant for IFrames
    const PARENT_DOM: Option<(DomId, NodeId)> = None;

    // TODO: Use these "stop sizes" to optimize not calling layout() on redrawing!
    let mut stop_sizes_width = Vec::new();
    let mut stop_sizes_height = Vec::new();

    let mut ui_state = {
        let full_window_state = &full_window_state;
        let layout_info = LayoutInfo {
            window_size: &full_window_state.size,
            window_size_width_stops: &mut stop_sizes_width,
            window_size_height_stops: &mut stop_sizes_height,
            gl_context: gl_context.clone(),
            resources: app_resources,
        };

        UiState::new_from_app_state(data, layout_info, PARENT_DOM, layout_callback)
    };

    ui_state.dom_id = DomId::ROOT_ID;

    let ui_state_dom_id = ui_state.dom_id.clone();

    let mut dom_id_map = BTreeMap::new();
    dom_id_map.insert(ui_state_dom_id.clone(), ui_state);

    dom_id_map
}

fn initialize_ui_description_cache<T>(
    ui_states: &mut BTreeMap<GlutinWindowId, BTreeMap<DomId, UiState<T>>>,
    full_window_states: &mut BTreeMap<GlutinWindowId, FullWindowState>,
) -> BTreeMap<GlutinWindowId, BTreeMap<DomId, UiDescription>> {
    ui_states.iter_mut().map(|(glutin_window_id, ui_states)| {
        let full_window_state = full_window_states.get_mut(glutin_window_id).unwrap();
        (*glutin_window_id, cascade_style(ui_states, full_window_state))
    }).collect()
}

// HTML (UiState) + CSS (FullWindowState) => CSSOM (UiDescription)
fn cascade_style<T>(
     ui_states: &mut BTreeMap<DomId, UiState<T>>,
     full_window_state: &mut FullWindowState,
) -> BTreeMap<DomId, UiDescription>{
    ui_states.iter_mut().map(|(dom_id, mut ui_state)| {
        (dom_id.clone(), UiDescription::new(
            &mut ui_state,
            &full_window_state.css,
            &full_window_state.focused_node,
            &full_window_state.hovered_nodes.entry(dom_id.clone()).or_insert_with(|| BTreeMap::default()),
            full_window_state.mouse_state.mouse_down(),
        ))
    }).collect()
}

/// Returns the currently hit-tested results, in back-to-front order
#[cfg(not(test))]
fn do_hit_test<T>(
    window: &Window<T>,
    full_window_state: &FullWindowState,
    render_api: &WrApi,
) -> Vec<HitTestItem> {

    use crate::wr_translate::{wr_translate_hittest_item, wr_translate_pipeline_id};

    let cursor_location = match full_window_state.mouse_state.cursor_position.get_position() {
        Some(pos) => WorldPoint::new(pos.x, pos.y),
        None => return Vec::new(),
    };

    let mut hit_test_results: Vec<HitTestItem> = render_api.api.hit_test(
        window.internal.document_id,
        Some(wr_translate_pipeline_id(window.internal.pipeline_id)),
        cursor_location,
        HitTestFlags::FIND_ALL
    ).items.into_iter().map(wr_translate_hittest_item).collect();

    // Execute callbacks back-to-front, not front-to-back
    hit_test_results.reverse();

    hit_test_results
}

/// Given the current (and previous) window state and the hit test results,
/// determines which `On::X` filters to actually call.
fn determine_events<T>(
    hit_test_results: &[HitTestItem],
    full_window_state: &mut FullWindowState,
    ui_state_map: &BTreeMap<DomId, UiState<T>>,
) -> BTreeMap<DomId, CallbacksOfHitTest<T>> {
    use azul_core::window_state::determine_callbacks;
    ui_state_map.iter().map(|(dom_id, ui_state)| {
        (dom_id.clone(), determine_callbacks(full_window_state, &hit_test_results, ui_state))
    }).collect()
}

fn call_callbacks<T>(
    data: &mut T,
    callbacks_filter_list: &BTreeMap<DomId, CallbacksOfHitTest<T>>,
    ui_state_map: &BTreeMap<DomId, UiState<T>>,
    ui_description_map: &BTreeMap<DomId, UiDescription>,
    timers: &mut FastHashMap<TimerId, Timer<T>>,
    tasks: &mut Vec<Task<T>>,
    scroll_states: &BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>,
    modifiable_scroll_states: &mut ScrollStates,
    full_window_state: &mut FullWindowState,
    layout_result: &BTreeMap<DomId, LayoutResult>,
    scrolled_nodes: &BTreeMap<DomId, ScrolledNodes>,
    cached_display_list: &CachedDisplayList,
    gl_context: Rc<dyn Gl>,
    resources: &mut AppResources,
    glutin_window: &GlutinWindow,
) -> CallCallbacksResult {

    use crate::callbacks::{CallbackInfo, DefaultCallbackInfo};
    use crate::window;

    let mut ret = CallCallbacksResult {
        needs_restyle_hover_active: callbacks_filter_list.values().any(|v| v.needs_redraw_anyways),
        needs_relayout_hover_active: callbacks_filter_list.values().any(|v| v.needs_relayout_anyways),
        needs_restyle_focus_changed: false,
        should_scroll_render: false,
        callbacks_update_screen: DontRedraw,
    };
    let mut new_focus_target = None;
    let mut nodes_scrolled_in_callbacks = BTreeMap::new();
    let mut modifiable_window_state: WindowState = full_window_state.clone().into();

    // Run all default callbacks - **before** the user-defined callbacks are run!
    for (dom_id, ui_state) in ui_state_map.iter() {
        for (node_id, callback_results) in callbacks_filter_list[dom_id].nodes_with_callbacks.iter() {
            let hit_item = &callback_results.hit_test_item;
            for event_filter in callback_results.default_callbacks.keys() {

                let default_callback = ui_state.dom.arena.node_data
                    .get(*node_id)
                    .map(|nd| nd.get_default_callbacks())
                    .and_then(|dc| dc.iter().find_map(|(evt, cb)| if evt == event_filter { Some(cb) } else { None }));

                let (default_callback, default_callback_ptr) = match default_callback {
                    Some(s) => s,
                    None => continue,
                };

                let mut new_focus = None;

                let default_callback_return = (default_callback.0)(DefaultCallbackInfo {
                    state: default_callback_ptr,
                    current_window_state: &full_window_state,
                    modifiable_window_state: &mut modifiable_window_state,
                    layout_result,
                    scrolled_nodes,
                    cached_display_list,
                    gl_context: gl_context.clone(),
                    resources,
                    timers,
                    tasks,
                    ui_state: ui_state_map,
                    focus_target: &mut new_focus,
                    current_scroll_states: scroll_states,
                    nodes_scrolled_in_callback: &mut nodes_scrolled_in_callbacks,
                    hit_dom_node: (dom_id.clone(), *node_id),
                    cursor_relative_to_item: hit_item.as_ref().map(|hi| (hi.point_relative_to_item.x, hi.point_relative_to_item.y)),
                    cursor_in_viewport: hit_item.as_ref().map(|hi| (hi.point_in_viewport.x, hi.point_in_viewport.y)),
                });

                if default_callback_return == Redraw {
                    ret.callbacks_update_screen = Redraw;
                }

                if let Some(new_focus) = new_focus.clone() {
                    new_focus_target = Some(new_focus);
                }
            }
        }
    }

    // Run all regular callbacks
    for dom_id in ui_state_map.keys().cloned() {
        for (node_id, callback_results) in callbacks_filter_list[&dom_id].nodes_with_callbacks.iter() {
            let hit_item = &callback_results.hit_test_item;
            for callback in callback_results.normal_callbacks.values() {

                let mut new_focus = None;

                if (callback.0)(CallbackInfo {
                    state: data,
                    current_window_state: &full_window_state,
                    modifiable_window_state: &mut modifiable_window_state,
                    layout_result,
                    scrolled_nodes,
                    cached_display_list,
                    gl_context: gl_context.clone(),
                    resources,
                    timers,
                    tasks,
                    ui_state: ui_state_map,
                    focus_target: &mut new_focus,
                    current_scroll_states: scroll_states,
                    nodes_scrolled_in_callback: &mut nodes_scrolled_in_callbacks,
                    hit_dom_node: (dom_id.clone(), *node_id),
                    cursor_relative_to_item: hit_item.as_ref().map(|hi| (hi.point_relative_to_item.x, hi.point_relative_to_item.y)),
                    cursor_in_viewport: hit_item.as_ref().map(|hi| (hi.point_in_viewport.x, hi.point_in_viewport.y)),
                }) == Redraw {
                    ret.callbacks_update_screen = Redraw;
                }

                if let Some(new_focus) = new_focus {
                    new_focus_target = Some(new_focus);
                }
            }
        }
    }

    // Scroll nodes from programmatic callbacks
    for (dom_id, callback_scrolled_nodes) in nodes_scrolled_in_callbacks {
        let scrolled_nodes = match scrolled_nodes.get(&dom_id) {
            Some(s) => s,
            None => continue,
        };

        for (scroll_node_id, scroll_position) in &callback_scrolled_nodes {
            let overflowing_node = match scrolled_nodes.overflowing_nodes.get(&scroll_node_id) {
                Some(s) => s,
                None => continue,
            };

            modifiable_scroll_states.set_scroll_position(&overflowing_node, *scroll_position);
            ret.should_scroll_render = true;
        }
    }

    let new_focus_node = new_focus_target.and_then(|ft| ft.resolve(&ui_description_map, &ui_state_map).ok()?);
    let focus_has_not_changed = full_window_state.focused_node == new_focus_node;
    if !focus_has_not_changed {
        // TODO: Emit proper On::FocusReceived / On::FocusLost events!
    }

    // Update the FullWindowState that we got from the frame event (updates window dimensions and DPI)
    full_window_state.focused_node = new_focus_node;

    // Update the window state every frame that was set by the user
    window::synchronize_window_state_with_os_window(
        full_window_state,
        &mut modifiable_window_state,
        glutin_window,
    );

    // Reset the scroll amount to 0 (for the next frame)
    window::clear_scroll_state(full_window_state);

    ret
}

/// Build the display list and send it to webrender
#[cfg(not(test))]
fn send_display_list_to_webrender<T>(
    window: &mut Window<T>,
    full_window_state: &FullWindowState,
    render_api: &mut WrApi,
) {
    use crate::wr_translate::{
        wr_translate_pipeline_id,
        wr_translate_display_list,
        wr_translate_epoch,
    };

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    let display_list = wr_translate_display_list(window.internal.cached_display_list.clone(), window.internal.pipeline_id);

    let (logical_size, _) = convert_window_size(&full_window_state.size);

    let mut txn = WrTransaction::new();
    txn.set_display_list(
        wr_translate_epoch(window.internal.epoch),
        None,
        logical_size.clone(),
        (wr_translate_pipeline_id(window.internal.pipeline_id), logical_size, display_list),
        true,
    );

    render_api.api.send_transaction(window.internal.document_id, txn);
}

/// Scroll all nodes in the ScrollStates to their correct position and insert
/// the positions into the transaction
///
/// NOTE: scroll_states has to be mutable, since every key has a "visited" field, to
/// indicate whether it was used during the current frame or not.
fn scroll_all_nodes(scroll_states: &mut ScrollStates, txn: &mut WrTransaction) {
    use webrender::api::ScrollClamping;
    use crate::wr_translate::{wr_translate_external_scroll_id, wr_translate_layout_point};
    println!("scrolling nodes: {:#?}", scroll_states);
    for (key, value) in scroll_states.0.iter_mut() {
        txn.scroll_node_with_id(
            wr_translate_layout_point(value.get()),
            wr_translate_external_scroll_id(*key),
            ScrollClamping::ToContentBounds
        );
    }
}

/// Returns the (logical_size, physical_size) as LayoutSizes, which can then be passed to webrender
fn convert_window_size(size: &WindowSize) -> (WrLayoutSize, WrDeviceIntSize) {
    let physical_size = size.get_physical_size();
    (
        WrLayoutSize::new(size.dimensions.width, size.dimensions.height),
        WrDeviceIntSize::new(physical_size.width as i32, physical_size.height as i32)
    )
}

/// Special rendering function that skips building a layout and only does
/// hit-testing and rendering - called on pure scroll events, since it's
/// significantly less CPU-intensive to just render the last display list instead of
/// re-layouting on every single scroll event.
#[must_use]
fn update_scroll_state(
    full_window_state: &mut FullWindowState,
    scrolled_nodes: &ScrolledNodes,
    scroll_states: &mut ScrollStates,
    hit_test_items: &[HitTestItem],
) -> bool {

    const SCROLL_THRESHOLD: f32 = 0.5; // px

    if full_window_state.mouse_state.scroll_x.is_none() && full_window_state.mouse_state.scroll_y.is_none() {
        return false;
    }

    let scroll_x = full_window_state.mouse_state.get_scroll_x();
    let scroll_y = full_window_state.mouse_state.get_scroll_y();

    if scroll_x.abs() < SCROLL_THRESHOLD && scroll_y.abs() < SCROLL_THRESHOLD {
        return false;
    }

    let mut should_scroll_render = false;

    for scroll_node in hit_test_items.iter()
        .filter_map(|item| scrolled_nodes.tags_to_node_ids.get(&ScrollTagId(item.tag)))
        .filter_map(|node_id| scrolled_nodes.overflowing_nodes.get(&node_id)) {

        // The external scroll ID is constructed from the DOM hash
        scroll_states.scroll_node(&scroll_node, scroll_x as f32, scroll_y as f32);
        should_scroll_render = true;
    }

    should_scroll_render
}

fn clean_up_unused_opengl_textures(pipeline_info: WrPipelineInfo, pipeline_id: &PipelineId) {

    use azul_core::gl::gl_textures_remove_epochs_from_pipeline;
    use crate::wr_translate::translate_epoch_wr;

    // TODO: currently active epochs can be empty, why?
    //
    // I mean, while the renderer is rendering, there can never be "no epochs" active,
    // at least one epoch must always be active.
    if pipeline_info.epochs.is_empty() {
        return;
    }

    // TODO: pipeline_info.epochs does not contain all active epochs,
    // at best it contains the lowest in-use epoch. I.e. if `Epoch(43)`
    // is listed, you can remove all textures from Epochs **lower than 43**
    // BUT NOT EPOCHS HIGHER THAN 43.
    //
    // This means that "all active epochs" (in the documentation) is misleading
    // since it doesn't actually list all active epochs, otherwise it'd list Epoch(43),
    // Epoch(44), Epoch(45), which are currently active.
    let oldest_to_remove_epoch = pipeline_info.epochs.values().min().unwrap();

    gl_textures_remove_epochs_from_pipeline(pipeline_id, translate_epoch_wr(*oldest_to_remove_epoch));
}

// Function wrapper that is invoked on scrolling and normal rendering - only renders the
// window contents and updates the screen, assumes that all transactions via the WrApi
// have been committed before this function is called.
//
// WebRender doesn't reset the active shader back to what it was, but rather sets it
// to zero, which glutin doesn't know about, so on the next frame it tries to draw with shader 0.
// This leads to problems when invoking GlCallbacks, because those don't expect
// the OpenGL state to change between calls. Also see: https://github.com/servo/webrender/pull/2880
//
// NOTE: For some reason, webrender allows rendering to a framebuffer with a
// negative width / height, although that doesn't make sense
#[cfg(not(test))]
fn render_inner<T>(
    window: &mut Window<T>,
    full_window_state: &FullWindowState,
    headless_shared_context: &mut HeadlessContextState,
    render_api: &mut WrApi,
    renderer: &mut WrRenderer,
    gl_context: Rc<dyn Gl>,
    mut txn: WrTransaction,
    background_color: ColorU,
) {

    use webrender::api::units::{DeviceIntRect, DeviceIntPoint};
    use azul_css::ColorF;
    use crate::wr_translate;

    let (_, framebuffer_size) = convert_window_size(&full_window_state.size);

    // Especially during minimization / maximization of a window, it can happen that the window
    // width or height is zero. In that case, no rendering is necessary (doing so would crash
    // the application, since glTexImage2D may never have a 0 as the width or height.
    if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
        return;
    }

    // We don't want the epoch to increase to u32::MAX, since
    // u32::MAX represents an invalid epoch, which could confuse webrender
    fn increase_epoch(old: Epoch) -> Epoch {
        use std::u32;
        const MAX_ID: u32 = u32::MAX - 1;
        match old.0 {
            MAX_ID => Epoch(0),
            other => Epoch(other + 1),
        }
    }

    window.internal.epoch = increase_epoch(window.internal.epoch);

    txn.set_document_view(
        // framebuffer_size.clone(),
        DeviceIntRect::new(DeviceIntPoint::new(0, 0), framebuffer_size),
        full_window_state.size.hidpi_factor as f32
    );
    txn.set_root_pipeline(wr_translate::wr_translate_pipeline_id(window.internal.pipeline_id));
    scroll_all_nodes(&mut window.internal.scroll_states, &mut txn);
    txn.generate_frame();

    render_api.api.send_transaction(window.internal.document_id, txn);

    // Update WR texture cache
    renderer.update();

    let background_color_f: ColorF = background_color.into();

    unsafe {

        // NOTE: The `hidden_display` must share the OpenGL context with the `window`,
        // otherwise this will segfault! Use `ContextBuilder::with_shared_lists` to share the
        // OpenGL context across different windows.
        //
        // The context **must** be made current before calling `.bind_framebuffer()`,
        // otherwise EGL will panic with EGL_BAD_MATCH. The current context has to be the
        // hidden_display context, otherwise this will segfault on Windows.
        headless_shared_context.make_current();

        let mut current_program = [0_i32];
        gl_context.get_integer_v(gl::CURRENT_PROGRAM, &mut current_program);

        // Generate a framebuffer (that will contain the final, rendered screen output).
        let framebuffers = gl_context.gen_framebuffers(1);
        gl_context.bind_framebuffer(gl::FRAMEBUFFER, framebuffers[0]);

        // Create the texture to render to
        let textures = gl_context.gen_textures(1);

        gl_context.bind_texture(gl::TEXTURE_2D, textures[0]);
        gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::RGB as i32, framebuffer_size.width, framebuffer_size.height, 0, gl::RGB, gl::UNSIGNED_BYTE, None);

        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

        let depthbuffers = gl_context.gen_renderbuffers(1);
        gl_context.bind_renderbuffer(gl::RENDERBUFFER, depthbuffers[0]);
        gl_context.renderbuffer_storage(gl::RENDERBUFFER, gl::DEPTH_COMPONENT, framebuffer_size.width, framebuffer_size.height);
        gl_context.framebuffer_renderbuffer(gl::FRAMEBUFFER, gl::DEPTH_ATTACHMENT, gl::RENDERBUFFER, depthbuffers[0]);

        // Set "textures[0]" as the color attachement #0
        gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, textures[0], 0);

        gl_context.draw_buffers(&[gl::COLOR_ATTACHMENT0]);

        // Check that the framebuffer is complete
        debug_assert!(gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

        // Disable SRGB and multisample, otherwise, WebRender will crash
        gl_context.disable(gl::FRAMEBUFFER_SRGB);
        gl_context.disable(gl::MULTISAMPLE);
        gl_context.disable(gl::POLYGON_SMOOTH);

        // Invoke WebRender to render the frame - renders to the currently bound FB
        gl_context.clear_color(background_color_f.r, background_color_f.g, background_color_f.b, background_color_f.a);
        gl_context.clear(gl::COLOR_BUFFER_BIT);
        gl_context.clear_depth(0.0);
        gl_context.clear(gl::DEPTH_BUFFER_BIT);
        renderer.render(framebuffer_size).unwrap();

        // FBOs can't be shared between windows, but textures can.
        // In order to draw on the windows backbuffer, first make the window current, then draw to FB 0
        headless_shared_context.make_not_current();
        window.display.make_current();
        draw_texture_to_screen(gl_context.clone(), textures[0], framebuffer_size);
        window.display.windowed_context().unwrap().swap_buffers().unwrap();
        window.display.make_not_current();
        headless_shared_context.make_current();

        // Only delete the texture here...
        gl_context.delete_framebuffers(&framebuffers);
        gl_context.delete_renderbuffers(&depthbuffers);
        gl_context.delete_textures(&textures);

        gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl_context.bind_texture(gl::TEXTURE_2D, 0);
        gl_context.use_program(current_program[0] as u32);
        headless_shared_context.make_not_current();
    };
}

/// When called with glDrawArrays(0, 3), generates a simple triangle that
/// spans the whole screen.
const DISPLAY_VERTEX_SHADER: &str = "
    #version 130
    out vec2 vTexCoords;
    void main() {
        float x = -1.0 + float((gl_VertexID & 1) << 2);
        float y = -1.0 + float((gl_VertexID & 2) << 1);
        vTexCoords = vec2((x+1.0)*0.5, (y+1.0)*0.5);
        gl_Position = vec4(x, y, 0, 1);
    }
";

/// Shader that samples an input texture (`fScreenTex`) to the output FB.
const DISPLAY_FRAGMENT_SHADER: &str = "
    #version 130
    in vec2 vTexCoords;
    uniform sampler2D fScreenTex;
    out vec4 fColorOut;

    void main() {
        fColorOut = texture(fScreenTex, vTexCoords);
    }
";

// NOTE: Compilation is thread-unsafe, should only be compiled on the main thread
static mut DISPLAY_SHADER: Option<GlShader> = None;

/// Compiles the display vertex / fragment shader, returns the compiled shaders.
fn compile_screen_shader(context: Rc<dyn Gl>) -> GLuint {
    unsafe { DISPLAY_SHADER.get_or_insert_with(|| {
        GlShader::new(context, DISPLAY_VERTEX_SHADER, DISPLAY_FRAGMENT_SHADER).unwrap()
    }) }.program_id
}

// Draws a texture to the currently bound framebuffer. Texture has to be cleaned up by the caller.
fn draw_texture_to_screen(context: Rc<dyn Gl>, texture: GLuint, framebuffer_size: WrDeviceIntSize) {

    context.bind_framebuffer(gl::FRAMEBUFFER, 0);

    // Compile or get the cached shader
    let shader = compile_screen_shader(context.clone());
    let texture_location = context.get_uniform_location(shader, "fScreenTex");

    // The uniform value for a sampler refers to the texture unit, not the texture id, i.e.:
    //
    // TEXTURE0 = uniform_1i(location, 0);
    // TEXTURE1 = uniform_1i(location, 1);

    context.active_texture(gl::TEXTURE0);
    context.bind_texture(gl::TEXTURE_2D, texture);
    context.use_program(shader);
    context.uniform_1i(texture_location, 0);

    // The vertices are generated in the vertex shader using gl_VertexID, however,
    // drawing without a VAO is not allowed (except for glDrawArraysInstanced,
    // which is only available in OGL 3.3)

    let vao = context.gen_vertex_arrays(1);
    context.bind_vertex_array(vao[0]);
    context.viewport(0, 0, framebuffer_size.width, framebuffer_size.height);
    context.draw_arrays(gl::TRIANGLE_STRIP, 0, 3);
    context.delete_vertex_arrays(&vao);

    context.bind_vertex_array(0);
    context.use_program(0);
    context.bind_texture(gl::TEXTURE_2D, 0);
}

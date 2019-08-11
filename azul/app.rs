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
    event::{
        ModifiersState as GlutinModifiersState,
    },
};
use gleam::gl::{self, Gl, GLuint};
use webrender::{
    PipelineInfo, Renderer,
    api::{LayoutSize, DeviceIntSize, Epoch, Transaction, RenderApi},
};
use log::LevelFilter;
use azul_css::{ColorU, HotReloadHandler};
use crate::{
    FastHashMap,
    window::{
        Window, ScrollStates, RendererType, WindowSize,
        DebugState, WindowState, FullWindowState, HeadlessContextState,
    },
    window_state::CallbacksOfHitTest,
    dom::{Dom, DomId, NodeId, ScrollTagId},
    gl::GlShader,
    traits::Layout,
    ui_state::UiState,
    task::{Task, Timer, TimerId},
    callbacks::{
        LayoutCallback, HitTestItem, Redraw, DontRedraw,
        ScrollPosition, DefaultCallbackIdMap,
    },
    display_list::{SolvedLayoutCache, GlTextureCache},
};
use azul_core::{
    ui_solver::ScrolledNodes,
    window::{KeyboardState, WindowId, CallCallbacksResult},
    callbacks::PipelineId,
    ui_description::UiDescription,
    ui_solver::LayoutResult,
    display_list::CachedDisplayList,
};
pub use crate::app_resources::AppResources;

#[cfg(not(test))]
use crate::window::{ FakeDisplay, WindowCreateOptions };
#[cfg(not(test))]
use glutin::CreationError;
#[cfg(not(test))]
use webrender::api::{WorldPoint, HitTestFlags};

#[cfg(test)]
use crate::app_resources::FakeRenderApi;

pub use azul_core::app::*; // {App, AppState, AppStateNoData, RuntimeError}

// Default clear color is white, to signify that there is rendering going on
// (otherwise, "transparent") backgrounds would be painted black.
const COLOR_WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 0 };

/// Graphical application that maintains some kind of application state
pub struct App<T: 'static> {
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
    fake_display: FakeDisplay<T>,
    #[cfg(test)]
    render_api: FakeRenderApi,
}

impl<T: 'static> App<T> {
    impl_task_api!();
}

/// Configuration for optional features, such as whether to enable logging or panic hooks
#[derive(Debug, Clone)]
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
}

impl<T> App<T> {

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    #[cfg(not(test))]
    pub fn add_window(&mut self, create_options: WindowCreateOptions<T>) {
        self.windows.insert(WindowId::new(), create_options);
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

    /// Start the rendering loop for the currently open windows
    /// This is the "main app loop", "main game loop" or whatever you want to call it.
    /// Usually this is the last function you call in your `main()` function, since exiting
    /// it means that the user has closed all windows and wants to close the app.
    ///
    /// When all windows are closed, this function returns the internal data again.
    /// This is useful for ex. CLI application that run procedurally, but then want to
    /// open a window temporarily, to ask for user input in a "nicer" way than a pure
    /// CLI-way.
    ///
    /// This way you can do this:
    ///
    /// ```no_run,ignore
    /// let app = App::new(MyData { username: None, password: None });
    /// app.create_window(WindowCreateOptions::default(), azul_native_style::native());
    ///
    /// // pop open a window that asks the user for his username and password...
    /// let MyData { username, password } = app.run();
    ///
    /// // continue the rest of the program here...
    /// println!("username: {:?}, password: {:?}", username, password);
    /// ```
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
        use azul_core::window::{AzulUpdateEvent, CursorPosition, LogicalPosition};
        use crate::wr_translate::winit_translate::{
            translate_winit_logical_size, translate_winit_logical_position,
        };

        let App { mut data, mut resources, mut timers, mut tasks, config, windows, layout_callback, mut fake_display } = self;

        let window_states = get_window_states(&windows);
        let initialized_windows = initialize_windows(windows, &mut fake_display, &mut resources, &config);
        let (mut active_windows, mut window_id_mapping, mut reverse_window_id_mapping) = initialized_windows;

        let mut full_window_states = initialize_full_window_states(&reverse_window_id_mapping, &window_states);
        let (mut ui_state_cache, mut default_callbacks_cache) = initialize_ui_state_cache(&data, fake_display.gl_context.clone(), &resources, &active_windows, &mut full_window_states, layout_callback);
        let mut ui_description_cache = initialize_ui_description_cache(&mut ui_state_cache, &mut full_window_states);

        let FakeDisplay { mut render_api, mut renderer, mut hidden_context, hidden_event_loop, gl_context } = fake_display;

        let event_loop_proxy = hidden_event_loop.create_proxy();

        // TODO: When the callbacks are run, rebuild the default callbacks again,
        // otherwise there could be a memory "leak" as default callbacks only
        // get added and never removed.

        hidden_event_loop.run(move |event, _, control_flow| {

            let now = Instant::now();

            macro_rules! close_window {($glutin_window_id:expr) => {
                    let window = active_windows.remove(&$glutin_window_id);
                    let window_id = window_id_mapping.remove(&$glutin_window_id);
                    if let Some(wid) = window_id {
                        reverse_window_id_mapping.remove(&wid);
                    }
                    full_window_states.remove(&$glutin_window_id);
                    ui_state_cache.remove(&$glutin_window_id);
                    ui_description_cache.remove(&$glutin_window_id);

                    if let Some(w) = window {
                        use crate::compositor::remove_active_pipeline;
                        use crate::app_resources::delete_pipeline;
                        remove_active_pipeline(&w.internal.pipeline_id);
                        delete_pipeline(&mut resources, &mut render_api, &w.internal.pipeline_id);
                        render_api.delete_document(w.internal.document_id);
                    }
                };
            }

            macro_rules! redraw_all_windows {() => {
                for (_, window) in active_windows.iter() {
                    window.display.window().request_redraw();
                }
            };}

            match event {
                Event::DeviceEvent { .. } => {
                    // ignore high-frequency events
                    *control_flow = ControlFlow::Wait;
                    return;
                },
                Event::WindowEvent { event, window_id } => {

                    let glutin_window_id = window_id;
                    let window_id = match window_id_mapping.get(&glutin_window_id) {
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
                        WindowEvent::Resized(logical_size) => {
                            // relayout, rebuild cached display list, reinitialize scroll states
                            let mut windowed_context = active_windows.get_mut(&glutin_window_id);
                            let windowed_context = windowed_context.as_mut().unwrap();
                            let dpi_factor = windowed_context.display.window().hidpi_factor();
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();

                            full_window_state.size.winit_hidpi_factor = dpi_factor as f32;
                            full_window_state.size.hidpi_factor = dpi_factor as f32;
                            full_window_state.size.dimensions = translate_winit_logical_size(*logical_size);

                            windowed_context.display.make_current();
                            windowed_context.display.windowed_context().unwrap().resize(logical_size.to_physical(dpi_factor));
                            windowed_context.display.make_not_current();

                            // TODO: Only rebuild UI if the resize is going across a resize boundary
                            event_loop_proxy.send_event(AzulUpdateEvent::RebuildUi { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::HiDpiFactorChanged(dpi_factor) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.size.winit_hidpi_factor = *dpi_factor as f32;
                            full_window_state.size.hidpi_factor = *dpi_factor as f32;
                        },
                        WindowEvent::Moved(new_window_position) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.position = Some(translate_winit_logical_position(*new_window_position));
                        },
                        WindowEvent::CursorMoved { position, modifiers, .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            let world_pos_x = position.x as f32 / full_window_state.size.hidpi_factor * full_window_state.size.winit_hidpi_factor;
                            let world_pos_y = position.y as f32 / full_window_state.size.hidpi_factor * full_window_state.size.winit_hidpi_factor;
                            full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(world_pos_x, world_pos_y));
                            update_keyboard_state_from_modifier_state(&mut full_window_state.keyboard_state, modifiers);
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::CursorLeft { .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow;
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::CursorEntered { .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(LogicalPosition::new(0.0, 0.0));
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::KeyboardInput { input: KeyboardInput { state, virtual_keycode, scancode, modifiers, .. }, .. } => {

                            use crate::wr_translate::winit_translate::translate_virtual_keycode;
                            use glutin::event::ElementState;

                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            update_keyboard_state_from_modifier_state(&mut full_window_state.keyboard_state, modifiers);

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

                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        // The char event is sliced inbetween a keydown and a keyup event, so the keyup
                        // has to clear the character again
                        WindowEvent::ReceivedCharacter(c) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.keyboard_state.current_char = Some(*c);
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::MouseInput { state, button, modifiers, .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            update_keyboard_state_from_modifier_state(&mut full_window_state.keyboard_state, modifiers);
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::MouseWheel { delta, phase, modifiers, .. } => {

                            use glutin::event::MouseScrollDelta;

                            const LINE_DELTA: f32 = 38.0;

                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            update_keyboard_state_from_modifier_state(&mut full_window_state.keyboard_state, modifiers);

                            let (scroll_x_px, scroll_y_px) = match delta {
                                MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                                MouseScrollDelta::LineDelta(x, y) => (x * LINE_DELTA, y * LINE_DELTA),
                            };

                            // TODO: "natural scrolling"?
                            full_window_state.mouse_state.scroll_x = Some(-scroll_x_px);
                            full_window_state.mouse_state.scroll_y = Some(-scroll_y_px);

                            let window = active_windows.get_mut(&glutin_window_id).unwrap();
                            let hit_test_results = do_hit_test(window, &full_window_state, &render_api);
                            let scrolled_nodes = &window.internal.scrolled_nodes;
                            let scroll_states = &mut window.internal.scroll_states;

                            let should_scroll_render_from_input_events = scrolled_nodes.values()
                                .any(|scrolled_node| update_scroll_state(full_window_state, scrolled_node, scroll_states, &hit_test_results));

                            if should_scroll_render_from_input_events {
                                event_loop_proxy.send_event(AzulUpdateEvent::UpdateScrollStates { window_id }).unwrap();
                            }
                        },
                        WindowEvent::Touch(Touch { phase, id, location, .. }) => {
                            use glutin::event::TouchPhase::*;
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(translate_winit_logical_position(*location));

                            match phase {
                                Started => {
                                    event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                                },
                                Moved => {
                                    // TODO: Do hit test and update window.internal.scroll_states!
                                    event_loop_proxy.send_event(AzulUpdateEvent::UpdateScrollStates { window_id }).unwrap(); // TODO!
                                },
                                Ended => {
                                    event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                                },
                                Cancelled => {
                                    event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                                },
                            }
                        },
                        WindowEvent::HoveredFile(file_path) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.hovered_file = Some(file_path.clone());
                            full_window_state.dropped_file = None;
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::HoveredFileCancelled => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.hovered_file = None;
                            full_window_state.dropped_file = None;
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::DroppedFile(file_path) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.hovered_file = None;
                            full_window_state.dropped_file = Some(file_path.clone());
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id }).unwrap(); // TODO!
                        },
                        WindowEvent::Focused(false) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.keyboard_state.current_char = None;
                            full_window_state.keyboard_state.pressed_virtual_keycodes.clear();
                            full_window_state.keyboard_state.current_virtual_keycode = None;
                            full_window_state.keyboard_state.pressed_scancodes.clear();
                        },
                        WindowEvent::RedrawRequested => {

                            let full_window_state = full_window_states.get(&glutin_window_id).unwrap();
                            let mut windowed_context = active_windows.get_mut(&glutin_window_id);
                            let mut windowed_context = windowed_context.as_mut().unwrap();

                            let pipeline_id = windowed_context.internal.pipeline_id;

                            // Render + swap the screen (call webrender + draw to texture)
                            render_inner(
                                &mut windowed_context,
                                &full_window_state,
                                &mut hidden_context,
                                &mut render_api,
                                renderer.as_mut().unwrap(),
                                gl_context.clone(),
                                Transaction::new(),
                                config.background_color,
                            );

                            // After rendering + swapping, remove the unused OpenGL textures
                            clean_up_unused_opengl_textures(renderer.as_mut().unwrap().flush_pipeline_info(), &pipeline_id);
                        },
                        WindowEvent::CloseRequested => {
                            close_window!(glutin_window_id);
                        },
                        _ => { },
                    }
                },
                Event::UserEvent(event) => {
                    use azul_core::window::AzulUpdateEvent::*;

                    match event {
                        CreateWindow { window_create_options } => {
                            /*
                            if let Ok(window) = Window::new(
                                &mut render_api,
                                hidden_context.headless_context_not_current().unwrap(),
                                &event_loop_proxy,
                                window_create_options,
                                config.background_color,
                            ) {
                                let glutin_window_id = window.display.window().id();
                                let window_id = window.id;
                                active_windows.insert(glutin_window_id, window);
                                full_window_states.insert(glutin_window_id, /* ... */);
                                ui_state_cache.insert(glutin_window_id, /* ... */);
                                ui_description_cache.insert(glutin_window_id, /* ... */);
                                window_id_mapping.insert(glutin_window_id, window_id);
                                reverse_window_id_mapping.insert(window_id, glutin_window_id);
                                // app_resources_register_pipeline()
                            }
                            */
                        },
                        CloseWindow { window_id } => {
                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();
                            if let Some(glutin_window_id) = glutin_id {
                                close_window!(glutin_window_id);
                            }
                        },
                        DoHitTest { window_id } => {
                            // Hit test if any nodes were hit, see if any callbacks need to be called

                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();

                            if let Some(glutin_window_id) = glutin_id {

                                let events = {
                                    let full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                                    let ui_state = &ui_state_cache[&glutin_window_id];
                                    let window = &active_windows[&glutin_window_id];
                                    let hit_test_results = do_hit_test(window, &full_window_state, &render_api);
                                    determine_events(&hit_test_results, full_window_state, ui_state)
                                };

                                // TODO: Add all off-click callbacks for all non-hit windows here!

                                if events.values().any(|e| e.should_call_callbacks()) {
                                    let ui_state = &ui_state_cache[&glutin_window_id];

                                    // call callbacks
                                    let call_callbacks_results = active_windows.iter_mut().map(|(window_id, window)| {
                                        let scroll_states = window.internal.get_current_scroll_states(&ui_state);
                                        call_callbacks(
                                            &mut data,
                                            &events,
                                            ui_state,
                                            default_callbacks_cache.get_mut(window_id).unwrap(),
                                            &mut timers,
                                            &mut tasks,
                                            &scroll_states,
                                            &mut window.internal.scroll_states,
                                            full_window_states.get_mut(&glutin_window_id).unwrap(),
                                            &window.internal.layout_result.solved_layouts,
                                            &window.internal.scrolled_nodes,
                                            &window.internal.cached_display_list,
                                            gl_context.clone(),
                                            &mut resources,
                                            &*window.display.window(),
                                        )
                                    }).collect::<Vec<_>>();

                                    // Application state has been updated, now figure out what to update from the callbacks

                                    // TODO: .any() or .all() ??
                                    let callbacks_update_screen = call_callbacks_results.iter().any(|cr| cr.callbacks_update_screen == Redraw);
                                    let callbacks_set_new_focus_target = call_callbacks_results.iter().any(|cr| cr.new_focus_target.is_some());
                                    let callbacks_hover_restyle = call_callbacks_results.iter().any(|cr| cr.needs_rerender_hover_active);
                                    let callbacks_hover_relayout = call_callbacks_results.iter().any(|cr| cr.needs_relayout_hover_active);
                                    let nodes_were_scrolled_from_callbacks = call_callbacks_results.iter().any(|cr| cr.should_scroll_render);

                                    if callbacks_update_screen {
                                        reverse_window_id_mapping.keys().for_each(|window_id| {
                                            event_loop_proxy.send_event(AzulUpdateEvent::RebuildUi { window_id: *window_id }).unwrap();
                                        });
                                    } else if callbacks_set_new_focus_target {
                                        event_loop_proxy.send_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: false }).unwrap();
                                    } else if callbacks_hover_restyle {
                                        event_loop_proxy.send_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: callbacks_hover_relayout }).unwrap();
                                    }

                                    if nodes_were_scrolled_from_callbacks {
                                        event_loop_proxy.send_event(AzulUpdateEvent::UpdateScrollStates { window_id }).unwrap();
                                    }
                                } else if events.values().any(|e| e.needs_relayout_anyways) {
                                    event_loop_proxy.send_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: false }).unwrap();
                                } else if events.values().any(|e| e.needs_redraw_anyways) {
                                    event_loop_proxy.send_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: true }).unwrap();
                                }
                            }
                        },
                        RebuildUi { window_id } => {
                            // Call the .layout() function, build UiState

                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();
                            if let Some(glutin_window_id) = glutin_id {

                                let full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                                let window = &active_windows[&glutin_window_id];
                                let force_css_reload = false;
                                let (new_ui_state, default_callbacks_map) = call_layout_fn(
                                    &data,
                                    gl_context.clone(),
                                    &resources,
                                    full_window_state,
                                    window.hot_reload_handler.as_ref().map(|hr| &hr.0),
                                    layout_callback,
                                    force_css_reload,
                                );

                                *default_callbacks_cache.get_mut(&glutin_window_id).unwrap() = default_callbacks_map;
                                *ui_state_cache.get_mut(&glutin_window_id).unwrap() = new_ui_state;

                                // optimization: create diff to previous UI State:
                                // - only restyle the nodes that were added / removed
                                // - if diff is empty (same UI), skip restyle, go straight to re-layouting
                                event_loop_proxy.send_event(AzulUpdateEvent::RestyleUi { window_id, skip_layout: false }).unwrap();
                            }
                        },
                        RestyleUi { window_id, skip_layout } => {
                            // Cascade the CSS to the HTML nodes
                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();

                            if let Some(glutin_window_id) = glutin_id {

                                let full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                                let ui_state = ui_state_cache.get_mut(&glutin_window_id).unwrap();
                                *ui_description_cache.get_mut(&glutin_window_id).unwrap() = cascade_style(ui_state, full_window_state);

                                // in cases like `:hover` and `:active`, layouting can be skipped
                                // (if it is known that the re-styling doesn't modify the layout)
                                if skip_layout {
                                    event_loop_proxy.send_event(AzulUpdateEvent::RebuildDisplayList { window_id }).unwrap();
                                } else {
                                    event_loop_proxy.send_event(AzulUpdateEvent::RelayoutUi { window_id }).unwrap();
                                }
                            }
                        },
                        RelayoutUi { window_id } => {
                            // Layout the CSSOM

                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();

                            if let Some(glutin_window_id) = glutin_id {

                                use crate::display_list::do_layout_for_display_list;

                                let window = active_windows.get_mut(&glutin_window_id).unwrap();
                                let full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();

                                let (solved_layout_cache, gl_texture_cache) = do_layout_for_display_list(
                                    &mut data,
                                    &mut resources,
                                    window,
                                    &mut fake_display,
                                    ui_state_cache.get_mut(&glutin_window_id).unwrap(),
                                    ui_description_cache.get_mut(&glutin_window_id).unwrap(),
                                    full_window_state,
                                    default_callbacks_cache.get_mut(&glutin_window_id).unwrap(),

                                );

                                 window.internal.layout_result = solved_layout_cache;
                                 window.internal.gl_texture_cache = gl_texture_cache;

                                 event_loop_proxy.send_event(AzulUpdateEvent::RebuildDisplayList { window_id }).unwrap();
                            }

                            // optimization with diff:
                            // - only relayout the nodes that were added / removed
                            // - if diff is empty (same UI), skip relayout, go straight to rebuilding display list
                        },
                        RebuildDisplayList { window_id } => {
                            // Build the display list

                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();

                            if let Some(glutin_window_id) = glutin_id {

                                let window = &active_windows[&glutin_window_id];
                                let full_window_state = &full_window_states[&glutin_window_id];

                                let cached_display_list = build_cached_display_list(
                                    window.internal.epoch,
                                    window.internal.pipeline_id,
                                    &full_window_state,
                                    &ui_state_cache[&glutin_window_id],
                                    &window.internal.layout_result,
                                    &window.internal.gl_texture_cache,
                                    &resources,
                                );

                                // optimization with diff:
                                // - only rebuild the nodes that were added / removed
                                // - if diff is empty (same UI), skip rebuilding the display list, go straight to sending the DL

                                window.internal.cached_display_list = cached_display_list;

                                event_loop_proxy.send_event(AzulUpdateEvent::SendDisplayListToWebRender { window_id }).unwrap();
                            }
                        },
                        SendDisplayListToWebRender { window_id } => {

                            // Build the display list
                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();

                            if let Some(glutin_window_id) = glutin_id {

                                let window = active_windows.get_mut(&glutin_window_id).unwrap();
                                let full_window_state = &full_window_states[&glutin_window_id];

                                send_display_list_to_webrender(
                                    window,
                                    full_window_state,
                                    &mut fake_display,
                                    &mut resources,
                                );

                                redraw_all_windows!();
                            }
                        },
                        UpdateScrollStates { window_id } => {
                            // Send all the scroll states from
                            let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();

                            if let Some(glutin_window_id) = glutin_id {
                                let window = active_windows.get_mut(&glutin_window_id).unwrap();
                                let mut txn = Transaction::new();
                                scroll_all_nodes(&mut window.internal.scroll_states, &mut txn);
                                render_api.send_transaction(window.internal.document_id, txn);
                            }
                        },
                        UpdateAnimations { window_id } => {
                            // send transaction to update animations in WR
                            // if no other events happened, skip to SendDisplayListToWebRender step
                            event_loop_proxy.send_event(AzulUpdateEvent::SendDisplayListToWebRender { window_id }).unwrap();
                        },
                        UpdateImages { window_id } => {
                            // send transaction to update images in WR
                            // if no other events happened, skip to SendDisplayListToWebRender step
                            event_loop_proxy.send_event(AzulUpdateEvent::SendDisplayListToWebRender { window_id }).unwrap();
                        },
                    }
                },
                _ => { },
            }

            // Application shutdown
            if active_windows.is_empty() {

                use crate::compositor::clear_opengl_cache;

                // NOTE: For some reason this is necessary, otherwise the renderer crashes on shutdown
                //
                // TODO: This still crashes on Linux because the makeCurrent call doesn't succeed
                // (likely because the underlying surface has been destroyed). In those cases,
                // we don't de-initialize the rendered (since this is an application shutdown it
                // doesn't matter, the resources are going to get cleaned up by the OS).
                hidden_context.make_current();

                // Important: destroy all OpenGL textures before the shared
                // OpenGL context is destroyed.
                clear_opengl_cache();

                gl_context.disable(gl::FRAMEBUFFER_SRGB);
                gl_context.disable(gl::MULTISAMPLE);
                gl_context.disable(gl::POLYGON_SMOOTH);

                if let Some(renderer) = renderer.take() {
                    renderer.deinit();
                }

                *control_flow = ControlFlow::Exit;
            } else {
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
            }
        })
    }
}

fn update_keyboard_state_from_modifier_state(keyboard_state: &mut KeyboardState, modifiers: &GlutinModifiersState) {
    keyboard_state.shift_down = modifiers.shift;
    keyboard_state.ctrl_down = modifiers.ctrl;
    keyboard_state.alt_down = modifiers.alt;
    keyboard_state.super_down = modifiers.logo;
}

fn get_window_states<T>(
    window_create_options: &BTreeMap<WindowId, WindowCreateOptions<T>>,
) -> BTreeMap<WindowId, WindowState> {
    window_create_options.iter().map(|(id, s)| (*id, s.state.clone())).collect()
}

/// Creates the intial windows on the screen and returns a mapping from
/// the (azul-internal) WindowId to the (glutin-internal) WindowId.
///
/// Theoretically, this mapping isn't used anywhere else, but it might
/// be useful for future refactoring.
fn initialize_windows<T>(
    window_create_options: BTreeMap<WindowId, WindowCreateOptions<T>>,
    fake_display: &mut FakeDisplay<T>,
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
    use azul_core::window::full_window_state_from_window_state;

    active_window_ids.iter().filter_map(|(window_id, glutin_window_id)| {
        let window_state = window_states.get(window_id)?;
        let full_window_state = full_window_state_from_window_state(window_state.clone());
        Some((*glutin_window_id, full_window_state))
    }).collect()
}

fn initialize_ui_state_cache<T>(
    data: &T,
    gl_context: Rc<Gl>,
    app_resources: &AppResources,
    windows: &BTreeMap<GlutinWindowId, Window<T>>,
    full_window_states: &mut BTreeMap<GlutinWindowId, FullWindowState>,
    layout_callback: LayoutCallback<T>,
) -> (
    BTreeMap<GlutinWindowId, BTreeMap<DomId, UiState<T>>>,
    BTreeMap<GlutinWindowId, BTreeMap<DomId, DefaultCallbackIdMap<T>>>,
) {
    const FORCE_CSS_RELOAD: bool = true;

    let mut ui_state_map = BTreeMap::new();
    let mut default_callbacks_id_map = BTreeMap::new();

    for (glutin_window_id, window) in windows {
        let full_window_state = full_window_states.get_mut(glutin_window_id).unwrap();
        let (dom_id_map, default_callbacks_map) = call_layout_fn(
            data,
            gl_context.clone(),
            app_resources,
            full_window_state,
            window.hot_reload_handler.as_ref().map(|hr| &hr.0),
            layout_callback,
            FORCE_CSS_RELOAD,
        );
        ui_state_map.insert(*glutin_window_id, dom_id_map);
        default_callbacks_id_map.insert(*glutin_window_id, default_callbacks_map);
    }

    DomId::reset();

    (ui_state_map, default_callbacks_id_map)
}

fn call_layout_fn<T>(
    data: &T,
    gl_context: Rc<Gl>,
    app_resources: &AppResources,
    full_window_state: &mut FullWindowState,
    hot_reload_handler: Option<&Box<HotReloadHandler>>,
    layout_callback: LayoutCallback<T>,
    force_css_reload: bool,
) -> (BTreeMap<DomId, UiState<T>>, BTreeMap<DomId, DefaultCallbackIdMap<T>>){

    use azul_core::callbacks::LayoutInfo;
    use crate::ui_state::{ui_state_from_app_state, ui_state_from_dom};

    // Any top-level DOM has no "parent", parents are only relevant for IFrames
    const PARENT_DOM: Option<(DomId, NodeId)> = None;

    // TODO: Use these "stop sizes" to optimize not calling layout() on redrawing!
    let mut stop_sizes_width = Vec::new();
    let mut stop_sizes_height = Vec::new();
    let mut default_callbacks = BTreeMap::new();

    // Hot-reload the CSS for this window
    #[cfg(debug_assertions)]
    let mut ui_state = {

        let css_has_error = {
            use crate::css::hot_reload_css;

            let hot_reload_result = hot_reload_css(
                &mut full_window_state.css,
                hot_reload_handler,
                &mut Instant::now(),
                force_css_reload,
            );
            let (_, css_has_error) = match hot_reload_result {
                Ok(has_reloaded) => (has_reloaded, None),
                Err(css_error) => (true, Some(css_error)),
            };
            css_has_error
        };

        match &css_has_error {
            None => {
                let layout_info = LayoutInfo {
                    window_size: &full_window_state.size,
                    window_size_width_stops: &mut stop_sizes_width,
                    window_size_height_stops: &mut stop_sizes_height,
                    default_callbacks: &mut default_callbacks,
                    gl_context: gl_context.clone(),
                    resources: app_resources,
                };
                ui_state_from_app_state(data, layout_info, PARENT_DOM, layout_callback)
            },
            Some(s) => {
                println!("{}", s);
                ui_state_from_dom(Dom::label(s.clone()).with_class("__azul_css_error"), None)
            },
        }
    };

    #[cfg(not(debug_assertions))]
    let mut ui_state = {
        let full_window_state = &full_window_state;
        let layout_info = LayoutInfo {
            window_size: &full_window_state.size,
            window_size_width_stops: &mut stop_sizes_width,
            window_size_height_stops: &mut stop_sizes_height,
            default_callbacks: &mut default_callbacks,
            gl_context: gl_context.clone(),
            resources: app_resources,
        };

        ui_state_from_app_state(data, layout_info, PARENT_DOM, layout_callback)
    };

    ui_state.dom_id = DomId::ROOT_ID;

    let ui_state_dom_id = ui_state.dom_id.clone();

    let mut dom_id_map = BTreeMap::new();
    dom_id_map.insert(ui_state_dom_id.clone(), ui_state);

    let mut default_callbacks_map = BTreeMap::new();
    default_callbacks_map.insert(ui_state_dom_id.clone(), default_callbacks);

    (dom_id_map, default_callbacks_map)
}

fn initialize_ui_description_cache<T>(
    ui_states: &mut BTreeMap<GlutinWindowId, BTreeMap<DomId, UiState<T>>>,
    full_window_states: &mut BTreeMap<GlutinWindowId, FullWindowState>,
) -> BTreeMap<GlutinWindowId, BTreeMap<DomId, UiDescription<T>>> {
    ui_states.iter_mut().map(|(glutin_window_id, ui_states)| {
        let full_window_state = full_window_states.get_mut(glutin_window_id).unwrap();
        (*glutin_window_id, cascade_style(ui_states, full_window_state))
    }).collect()
}

// HTML (UiState) + CSS (FullWindowState) => CSSOM (UiDescription)
#[cfg(not(test))]
fn cascade_style<T>(
     ui_states: &mut BTreeMap<DomId, UiState<T>>,
     full_window_state: &mut FullWindowState,
) -> BTreeMap<DomId, UiDescription<T>>{
    ui_states.iter_mut().map(|(dom_id, mut ui_state)| {
        (dom_id.clone(), UiDescription::match_css_to_dom(
            &mut ui_state,
            &full_window_state.css,
            &mut full_window_state.focused_node,
            &mut full_window_state.pending_focus_target,
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
    render_api: &RenderApi,
) -> Vec<HitTestItem> {

    use crate::wr_translate::{wr_translate_hittest_item, wr_translate_pipeline_id};

    let cursor_location = match full_window_state.mouse_state.cursor_position.get_position() {
        Some(pos) => WorldPoint::new(pos.x, pos.y),
        None => return Vec::new(),
    };

    let mut hit_test_results: Vec<HitTestItem> = render_api.hit_test(
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
    use crate::window_state::determine_callbacks;
    ui_state_map.iter().map(|(dom_id, ui_state)| {
        (dom_id.clone(), determine_callbacks(full_window_state, &hit_test_results, ui_state))
    }).collect()
}

fn call_callbacks<T>(
    data: &mut T,
    callbacks_filter_list: &BTreeMap<DomId, CallbacksOfHitTest<T>>,
    ui_state_map: &BTreeMap<DomId, UiState<T>>,
    default_callbacks: &mut BTreeMap<DomId, DefaultCallbackIdMap<T>>,
    timers: &mut FastHashMap<TimerId, Timer<T>>,
    tasks: &mut Vec<Task<T>>,
    scroll_states: &BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>,
    modifiable_scroll_states: &mut ScrollStates,
    full_window_state: &mut FullWindowState,
    layout_result: &BTreeMap<DomId, LayoutResult>,
    scrolled_nodes: &BTreeMap<DomId, ScrolledNodes>,
    cached_display_list: &CachedDisplayList,
    gl_context: Rc<Gl>,
    resources: &mut AppResources,
    glutin_window: &GlutinWindow,
) -> CallCallbacksResult {

    use crate::callbacks::{CallbackInfo, DefaultCallbackInfoUnchecked};
    use crate::window;

    let mut ret = CallCallbacksResult {
        needs_rerender_hover_active: callbacks_filter_list.values().any(|v| v.needs_redraw_anyways),
        needs_relayout_hover_active: callbacks_filter_list.values().any(|v| v.needs_relayout_anyways),
        should_scroll_render: false,
        callbacks_update_screen: DontRedraw,
        new_focus_target: None,
    };

    let mut nodes_scrolled_in_callbacks = BTreeMap::new();
    let mut modifiable_window_state = window::full_window_state_to_window_state(full_window_state);

    // Run all default callbacks - **before** the user-defined callbacks are run!
    for dom_id in ui_state_map.keys().cloned() {
        for (node_id, callback_results) in callbacks_filter_list[&dom_id].nodes_with_callbacks.iter() {
            let hit_item = &callback_results.hit_test_item;
            for default_callback_id in callback_results.default_callbacks.values() {

                let mut new_focus = None;

                let default_callback = default_callbacks.get(&dom_id).and_then(|dc| dc.get(default_callback_id).cloned());

                let default_callback_redraws = match default_callback {
                    Some((callback_ptr, callback_fn)) => {
                        (callback_fn.0)(DefaultCallbackInfoUnchecked {
                            ptr: callback_ptr,
                            current_window_state: &full_window_state,
                            modifiable_window_state: &mut modifiable_window_state,
                            layout_result,
                            scrolled_nodes,
                            cached_display_list,
                            default_callbacks: default_callbacks.get_mut(&dom_id).unwrap(),
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
                        })
                    },
                    None => DontRedraw,
                };

                if default_callback_redraws == Redraw {
                    ret.callbacks_update_screen = Redraw;
                }

                // Overwrite the focus from the callback info
                if let Some(new_focus) = new_focus.clone() {
                    ret.new_focus_target = Some(new_focus);
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
                    data,
                    current_window_state: &full_window_state,
                    modifiable_window_state: &mut modifiable_window_state,
                    layout_result,
                    scrolled_nodes,
                    cached_display_list,
                    default_callbacks: default_callbacks.get_mut(&dom_id).unwrap(),
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
                    ret.new_focus_target = Some(new_focus);
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

    // Update the FullWindowState that we got from the frame event (updates window dimensions and DPI)
    full_window_state.pending_focus_target = ret.new_focus_target.clone();

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

// Build the cached display list
#[cfg(not(test))]
fn build_cached_display_list<T>(
    epoch: Epoch,
    pipeline_id: PipelineId,
    full_window_state: &FullWindowState,
    ui_state_cache: &BTreeMap<DomId, UiState<T>>,
    layout_result_cache: &SolvedLayoutCache,
    gl_texture_cache: &GlTextureCache,
    app_resources: &AppResources,
) -> CachedDisplayList {
    use crate::display_list::{
        DisplayListParametersRef,
        push_rectangles_into_displaylist
    };

    CachedDisplayList {
        root: push_rectangles_into_displaylist(
            full_window_state.size,
            &DisplayListParametersRef {
                dom_id: DomId::ROOT_ID,
                epoch,
                full_window_state,
                pipeline_id,
                layout_result: layout_result_cache,
                gl_texture_cache,
                ui_state_cache,
                app_resources,
            },
        )
    }
}

/// Build the display list and send it to webrender
#[cfg(not(test))]
fn send_display_list_to_webrender<T>(
    window: &mut Window<T>,
    full_window_state: &FullWindowState,
    fake_display: &mut FakeDisplay<T>,
    app_resources: &mut AppResources,
) {
    use crate::wr_translate::{
        wr_translate_pipeline_id,
        wr_translate_display_list,
    };

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    let display_list = wr_translate_display_list(window.internal.cached_display_list.clone(), window.internal.pipeline_id);

    let (logical_size, _) = convert_window_size(&full_window_state.size);

    let mut txn = Transaction::new();
    txn.set_display_list(
        window.internal.epoch,
        None,
        logical_size.clone(),
        (wr_translate_pipeline_id(window.internal.pipeline_id), logical_size, display_list),
        true,
    );

    fake_display.render_api.send_transaction(window.internal.document_id, txn);
}

/// Scroll all nodes in the ScrollStates to their correct position and insert
/// the positions into the transaction
///
/// NOTE: scroll_states has to be mutable, since every key has a "visited" field, to
/// indicate whether it was used during the current frame or not.
fn scroll_all_nodes(scroll_states: &mut ScrollStates, txn: &mut Transaction) {
    use webrender::api::ScrollClamping;
    use crate::wr_translate::{wr_translate_external_scroll_id, wr_translate_layout_point};
    for (key, value) in scroll_states.0.iter_mut() {
        txn.scroll_node_with_id(
            wr_translate_layout_point(value.get()),
            wr_translate_external_scroll_id(*key),
            ScrollClamping::ToContentBounds
        );
    }
}

/// Returns the (logical_size, physical_size) as LayoutSizes, which can then be passed to webrender
fn convert_window_size(size: &WindowSize) -> (LayoutSize, DeviceIntSize) {
    let physical_size = size.get_physical_size();
    (
        LayoutSize::new(size.dimensions.width, size.dimensions.height),
        DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32)
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
        .filter_map(|item| scrolled_nodes.tags_to_node_ids.get(&ScrollTagId(item.tag.0)))
        .filter_map(|node_id| scrolled_nodes.overflowing_nodes.get(&node_id)) {

        // The external scroll ID is constructed from the DOM hash
        scroll_states.scroll_node(&scroll_node, scroll_x as f32, scroll_y as f32);
        should_scroll_render = true;
    }

    should_scroll_render
}

fn clean_up_unused_opengl_textures(pipeline_info: PipelineInfo, pipeline_id: &PipelineId) {

    use crate::compositor::remove_epochs_from_pipeline;

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

    remove_epochs_from_pipeline(pipeline_id, *oldest_to_remove_epoch);
}

// Function wrapper that is invoked on scrolling and normal rendering - only renders the
// window contents and updates the screen, assumes that all transactions via the RenderApi
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
    render_api: &mut RenderApi,
    renderer: &mut Renderer,
    gl_context: Rc<Gl>,
    mut txn: Transaction,
    background_color: ColorU,
) {

    use webrender::api::{DeviceIntRect, DeviceIntPoint};
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

    txn.set_window_parameters(
        framebuffer_size.clone(),
        DeviceIntRect::new(DeviceIntPoint::new(0, 0), framebuffer_size),
        full_window_state.size.hidpi_factor as f32
    );
    txn.set_root_pipeline(wr_translate::wr_translate_pipeline_id(window.internal.pipeline_id));
    scroll_all_nodes(&mut window.internal.scroll_states, &mut txn);
    txn.generate_frame();

    render_api.send_transaction(window.internal.document_id, txn);

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
fn draw_texture_to_screen(context: Rc<dyn Gl>, texture: GLuint, framebuffer_size: DeviceIntSize) {

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

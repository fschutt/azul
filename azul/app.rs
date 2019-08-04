use std::{
    rc::Rc,
    time::{Duration, Instant},
    collections::BTreeMap,
};
use glutin::{
    window::WindowId as GlutinWindowId,
    event::WindowEvent as GlutinWindowEvent,
};
use gleam::gl::{self, Gl, GLuint};
use webrender::{
    PipelineInfo, Renderer,
    api::{LayoutSize, DeviceIntSize, Epoch, Transaction, RenderApi},
};
use log::LevelFilter;
use azul_css::{ColorU, HotReloadHandler, Css, LayoutPoint};
use {
    FastHashMap,
    window::{
        Window, ScrollStates, LogicalPosition, LogicalSize,
        RendererType, WindowSize, DebugState, WindowState,
        FullWindowState, HeadlessContextState,
    },
    dom::{Dom, DomId, NodeId, ScrollTagId},
    gl::GlShader,
    traits::Layout,
    ui_state::UiState,
    async::{Task, Timer, TimerId, TerminateTimer},
    callbacks::{
        LayoutCallback, FocusTarget, UpdateScreen, HitTestItem,
        Redraw, DontRedraw, ScrollPosition, DefaultCallbackIdMap,
    },
};
use azul_core::{
    ui_solver::ScrolledNodes,
    window::{WindowId, SingleWindowHitTestResult},
    callbacks::PipelineId,
    ui_description::UiDescription,
};
pub use app_resources::AppResources;

#[cfg(not(test))]
use window::{ FakeDisplay, WindowCreateOptions };
#[cfg(not(test))]
use glutin::CreationError;
#[cfg(not(test))]
use webrender::api::{WorldPoint, HitTestFlags};

#[cfg(test)]
use app_resources::FakeRenderApi;

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

pub(crate) struct FrameEventInfo {
    pub(crate) should_hittest: bool,
    pub(crate) cur_cursor_pos: LogicalPosition,
    pub(crate) new_window_size: Option<LogicalSize>,
    pub(crate) new_dpi_factor: Option<f32>,
    pub(crate) is_resize_event: bool,
}

impl Default for FrameEventInfo {
    fn default() -> Self {
        Self {
            should_hittest: false,
            cur_cursor_pos: LogicalPosition::new(0.0, 0.0),
            new_window_size: None,
            new_dpi_factor: None,
            is_resize_event: false,
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
                ::logging::set_up_logging(app_config.log_file_path.as_ref().map(|s| s.as_str()), log_level);

                if app_config.enable_logging_on_panic {
                    ::logging::set_up_panic_hooks();
                }

                if app_config.enable_visual_panic_hook {
                    use std::sync::atomic::Ordering;
                    ::logging::SHOULD_ENABLE_PANIC_HOOK.store(true, Ordering::SeqCst);
                }
            }
        }

        #[cfg(not(test))] {
            let mut fake_display = FakeDisplay::new(app_config.renderer_type)?;
            if let Some(r) = &mut fake_display.renderer {
                use wr_translate::set_webrender_debug_flags;
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
        use wr_translate::set_webrender_debug_flags;
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
            event::{WindowEvent, Touch, Event, StartCause},
            event_loop::ControlFlow,
        };
        use azul_core::window::{AzulUpdateEvent, CursorPosition};
        use wr_translate::winit_translate::{
            translate_winit_logical_size, translate_winit_logical_position,
        };

        let App { data, resources, timers, tasks, config, windows, layout_callback, fake_display } = self;
        let FakeDisplay { mut render_api, mut renderer, mut hidden_context, hidden_event_loop, gl_context } = fake_display;

        let initialized_windows = initialize_windows(windows, &mut fake_display, &mut resources, &config);
        let (mut active_windows, mut window_id_mapping, mut reverse_window_id_mapping) = initialized_windows;
        let mut full_window_states = initialize_full_window_states(&windows);
        let (mut ui_state_cache, mut default_callbacks_cache) = initialize_ui_state_cache(&data, gl_context.clone(), &resources, &active_windows, &mut full_window_states, layout_callback);
        let mut ui_description_cache = initialize_ui_description_cache(&mut ui_state_cache, &mut full_window_states);

        let event_loop_proxy = hidden_event_loop.create_proxy();

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
                        use compositor::remove_active_pipeline;
                        use wr_translate::wr_translate_pipeline_id;
                        use app_resources::delete_pipeline;
                        remove_active_pipeline(&w.internal.pipeline_id);
                        delete_pipeline(&w.internal.pipeline_id);
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
                Event::WindowEvent { event, window_id } => {

                    let glutin_window_id = window_id;
                    let window_id = window_id_mapping[&glutin_window_id];

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
                            event_loop_proxy.send_event(AzulEvent::RebuildUi { window_id });
                        },
                        WindowEvent::Moved(new_window_position) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.position = Some(translate_winit_logical_position(*new_window_position));
                        },
                        WindowEvent::CursorLeft { .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow;
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id });
                        },
                        WindowEvent::CursorMoved { position, .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(translate_winit_logical_position(*position));
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id });
                        },
                        WindowEvent::MouseInput { state, button, modifiers, .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id });
                        },
                        WindowEvent::MouseWheel { delta, phase, modifiers, .. } => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            event_loop_proxy.send_event(AzulUpdateEvent::UpdateScrollStates { window_id });
                        },
                        WindowEvent::Touch(Touch { phase, id, location, .. }) => {
                            use glutin::event::TouchPhase::*;
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.mouse_state.cursor_position = CursorPosition::InWindow(translate_winit_logical_position(*location));

                            match phase {
                                Started => {
                                    event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id });
                                },
                                Moved => {
                                    event_loop_proxy.send_event(AzulUpdateEvent::UpdateScrollStates { window_id });
                                },
                                Ended => {
                                    event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id });
                                },
                                Cancelled => {
                                    event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id });
                                },
                            }
                        },
                        WindowEvent::HoveredFile(file_path) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.hovered_file = Some(file_path.clone());
                            full_window_state.dropped_file = None;
                        },
                        WindowEvent::DroppedFile(file_path) => {
                            let mut full_window_state = full_window_states.get_mut(&glutin_window_id).unwrap();
                            full_window_state.hovered_file = None;
                            full_window_state.dropped_file = Some(file_path.clone());
                        },
                        WindowEvent::RedrawRequested => {

                            use app_resources::garbage_collect_fonts_and_images;

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
                            // Delete unused font and image keys
                            garbage_collect_fonts_and_images(&mut resources, &mut render_api, pipeline_id);
                        },
                        WindowEvent::CloseRequested => {
                            close_window!(glutin_window_id);
                        },
                        _ => { },
                    }
                },
                Event::UserEvent(AzulUpdateEvent::CreateWindow { window_create_options }) => {
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
                Event::UserEvent(AzulUpdateEvent::CloseWindow { window_id }) => {
                    let glutin_id = reverse_window_id_mapping.get(&window_id).cloned();
                    if let Some(glutin_window_id) = glutin_id {
                        close_window!(glutin_window_id);
                    }
                },
                Event::UserEvent(AzulUpdateEvent::DoHitTest { window_id }) => {
                    // generate events
                    // if events > 0 {
                    //     event.dispatch(AzulUpdateEvent::CallCallbacks { window_id });
                    // }
                },
                Event::UserEvent(AzulUpdateEvent::CallCallbacks { window_id }) => {
                    // generate events
                    // if callbacks say screen should update {
                    for window_id in active_windows.keys() {
                        //  event.dispatch(AzulUpdateEvent::RebuildUi { window_id });
                    }
                    // }
                },
                Event::UserEvent(AzulUpdateEvent::RebuildUi { window_id }) => {
                    // event.dispatch(AzulUpdateEvent::RestyleUi { window_id });
                },
                Event::UserEvent(AzulUpdateEvent::RestyleUi { window_id }) => {
                    // event.dispatch(AzulUpdateEvent::RestyleUi { window_id });

                },
                Event::UserEvent(AzulUpdateEvent::RelayoutUi { window_id }) => {
                    // event.dispatch(AzulUpdateEvent::RebuildDisplayList { window_id });
                },
                Event::UserEvent(AzulUpdateEvent::RebuildDisplayList { window_id }) => {

                    redraw_all_windows!()
                },
                Event::UserEvent(AzulUpdateEvent::UpdateScrollStates { window_id }) => {
                    // TODO: Only do hit test + scroll, then redraw - no UI rebuilding, no callbacks!
                    event_loop_proxy.send_event(AzulUpdateEvent::DoHitTest { window_id });
                },
                Event::UserEvent(AzulUpdateEvent::UpdateAnimations { window_id }) => {
                    // send transaction to update images in WR

                },
                Event::UserEvent(AzulUpdateEvent::UpdateImages { window_id }) => {
                    // send transaction to update images in WR
                },
                _ => { },
            }

            // Application shutdown
            if active_windows.is_empty() {

                use compositor::clear_opengl_cache;

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
                    use azul_core::async::{run_all_timers, clean_up_finished_tasks};

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

/// Creates the intial windows on the screen and returns a mapping from
/// the (azul-internal) WindowId to the (glutin-internal) WindowId.
///
/// Theoretically, this mapping isn't used anywhere else, but it might
/// be useful for future refactoring.
fn initialize_windows<T>(
    windows: BTreeMap<WindowId, WindowCreateOptions<T>>,
    fake_display: &mut FakeDisplay<T>,
    app_resources: &mut AppResources,
    config: &AppConfig,
) -> (
    BTreeMap<GlutinWindowId, Window<T>>,
    BTreeMap<GlutinWindowId, WindowId>,
    BTreeMap<WindowId, GlutinWindowId>
) {

    let windows = windows.into_iter().filter_map(|(window_id, window_create_options)| {
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

fn initialize_full_window_states<T>(
    active_window_ids: &BTreeMap<WindowId, GlutinWindowId>,
    windows: &BTreeMap<WindowId, WindowCreateOptions<T>>,
) -> BTreeMap<GlutinWindowId, FullWindowState> {
    use azul_core::window::full_window_state_from_window_state;

    active_window_ids.iter().filter_map(|(window_id, glutin_window_id)| {
        let window_create_options = windows.get(window_id)?;
        let full_window_state = full_window_state_from_window_state(window_create_options.state.clone());
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

    use ui_state::{ui_state_from_app_state, ui_state_from_dom};
    use azul_core::callbacks::LayoutInfo;

    // Any top-level DOM has no "parent", parents are only relevant for IFrames
    const PARENT_DOM: Option<(DomId, NodeId)> = None;
    const FORCE_CSS_RELOAD: bool = true;

    let mut ui_state_map = BTreeMap::new();
    let mut default_callbacks_id_map = BTreeMap::new();

    for (glutin_window_id, window) in windows {

        // TODO: Use these "stop sizes" to optimize not calling layout() on redrawing!
        let mut stop_sizes_width = Vec::new();
        let mut stop_sizes_height = Vec::new();
        let mut default_callbacks = BTreeMap::new();

        // Hot-reload the CSS for this window
        #[cfg(debug_assertions)]
        let mut ui_state = {

            let css_has_error = {
                use css::hot_reload_css;

                let full_window_state = &mut full_window_states[glutin_window_id];
                let hot_reload_result = hot_reload_css(
                    &mut full_window_state.css,
                    &window.hot_reload_handler.0,
                    &mut Instant::now(),
                    FORCE_CSS_RELOAD
                );
                let (_, css_has_error) = match hot_reload_result {
                    Ok(has_reloaded) => (has_reloaded, None),
                    Err(css_error) => (true, Some(css_error)),
                };
                css_has_error
            };

            match &css_has_error {
                None => {
                    let full_window_state = &full_window_states[glutin_window_id];
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
            let full_window_state = &full_window_states[glutin_window_id];
            let layout_info = LayoutInfo {
                window_size: &full_window_state.size,
                window_size_width_stops: &stop_sizes_width,
                window_size_height_stops: &stop_sizes_height,
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
        ui_state_map.insert(*glutin_window_id, dom_id_map);

        let mut default_callbacks_map = BTreeMap::new();
        default_callbacks_map.insert(ui_state_dom_id.clone(), default_callbacks);
        default_callbacks_id_map.insert(*glutin_window_id, default_callbacks_map);
    }

    DomId::reset();

    (ui_state_map, default_callbacks_id_map)
}

fn initialize_ui_description_cache<T>(
    ui_states: &mut BTreeMap<GlutinWindowId, BTreeMap<DomId, UiState<T>>>,
    full_window_states: &mut BTreeMap<GlutinWindowId, FullWindowState>,
) -> BTreeMap<GlutinWindowId, BTreeMap<DomId, UiDescription<T>>> {
    ui_states.iter_mut().map(|(glutin_window_id, ui_states)| {
        let full_window_state = &mut full_window_states[glutin_window_id];
        (*glutin_window_id, cascade_style(ui_states, full_window_state))
    }).collect()
}

/// Do the hit test for a given window event, call all necessary callbacks
#[cfg(not(test))]
fn hit_test_and_call_callbacks<T>(
    data: &mut T,
    event: &GlutinWindowEvent,
    window: &mut Window<T>,
    fake_display: &mut FakeDisplay<T>,
    full_window_state: &mut FullWindowState,
    ui_state_cache: &mut BTreeMap<DomId, UiState<T>>,
    default_callbacks: &mut BTreeMap<DomId, DefaultCallbackIdMap<T>>,
    timers: &mut FastHashMap<TimerId, Timer<T>>,
    tasks: &mut Vec<Task<T>>,
    resources: &mut AppResources,
) -> SingleWindowHitTestResult {

    use azul_core::app::RuntimeError::*;
    use window;

    // Update the FullWindowState from user events
    window::update_window_state(full_window_state, &events);

    let mut ret = SingleWindowHitTestResult {
        needs_rerender_hover_active: false,
        needs_relayout_hover_active: false,
        should_scroll_render: false,
        needs_relayout_refresh: false,
        callbacks_update_screen: DontRedraw,
        hit_test_results: Vec::new(),
        new_focus_target: None,
    };

    ret.hit_test_results = do_hit_test(&window, &full_window_state, fake_display);

    // From the event and the current (and last) window state, determines
    // what nodes need to be called
    let azul_events = determine_events(event, &ui_state_cache, &ret.hit_test_results, full_window_state);

    let scroll_states = window.internal.get_current_scroll_states(&ui_state_cache);

    // Create a "stub" copy of the current window state that the user can modify in the callbacks
    let mut modifiable_window_state = window::full_window_state_to_window_state(full_window_state);
    let mut nodes_scrolled_in_callbacks = BTreeMap::new();

    let callback_result = call_callbacks(
        data,
        azul_events,
        &ui_state_map,
        default_callbacks,
        timers,
        tasks,
        &scroll_states,
        &mut modifiable_window_state,
        &full_window_state,
        &window.internal.layout_result,
        &window.internal.scrolled_nodes,
        &mut nodes_scrolled_in_callbacks,
        &window.internal.cached_display_list,
        fake_display.get_gl_context(),
        resources,
    );

    if callback_result.should_update_screen == Redraw {
        ret.callbacks_update_screen = Redraw;
    }

    if callback_result.needs_redraw_anyways {
        ret.needs_rerender_hover_active = true;
    }

    if callback_result.needs_relayout_anyways {
        ret.needs_relayout_hover_active = true;
    }

    // Note: Don't set `pending_focus_target` directly here, because otherwise
    // callbacks that return `Some()` would get immediately overwritten again
    // by callbacks that return `None`.
    if let Some(overwrites_focus) = callback_result.callbacks_overwrites_focus {
        ret.new_focus_target = Some(overwrites_focus);
    }

    // Scroll nodes from input (mouse scroll) events
    let mut should_scroll_render_from_input_events = false;

    for (_dom_id, scrolled_nodes) in &window.internal.scrolled_nodes {
        if update_scroll_state(delta, scrolled_nodes, &mut window.internal.scroll_states, &ret.hit_test_results) {
            should_scroll_render_from_input_events = true;
        }
    }

    let mut should_scroll_render_from_callbacks = false;

    // Scroll nodes that were scrolled via the callbacks
    for (dom_id, callback_scrolled_nodes) in nodes_scrolled_in_callbacks {
        let scrolled_nodes = match window.internal.scrolled_nodes.get(&dom_id) {
            Some(s) => s,
            None => continue,
        };

        for (scroll_node_id, scroll_position) in &callback_scrolled_nodes {
            let overflowing_node = match scrolled_nodes.overflowing_nodes.get(&scroll_node_id) {
                Some(s) => s,
                None => continue,
            };

            window.internal.scroll_states.set_scroll_position(&overflowing_node, *scroll_position);
            should_scroll_render_from_callbacks = true;
        }
    }

    ret.should_scroll_render = should_scroll_render_from_input_events || should_scroll_render_from_callbacks;

    // Update the FullWindowState that we got from the frame event (updates window dimensions and DPI)
    full_window_state.pending_focus_target = ret.new_focus_target.clone();

    // Update the window state every frame that was set by the user
    window::synchronize_window_state_with_os_window(
        full_window_state,
        &mut modifiable_window_state,
        &*window.display.window(),
    );

    window::update_from_external_window_state(
        full_window_state,
        &mut frame_event_info,
        &fake_display.hidden_event_loop,
        &window.display.window()
    );

    // Reset the scroll amount to 0 (for the next frame)
    window::clear_scroll_state(full_window_state);

    ret
}

// HTML (UiState) + CSS (FullWindowState) => CSSOM (UiDescription)
#[cfg(not(test))]
fn cascade_style<T>(
     ui_states: &mut BTreeMap<DomId, UiState<T>>,
     full_window_state: &mut FullWindowState,
) -> BTreeMap<DomId, UiDescription<T>>{
    ui_states.iter_mut().map(|(dom_id, ui_state)| {
        (dom_id.clone(), UiDescription::match_css_to_dom(
            &mut ui_state,
            &full_window_state.css,
            &mut full_window_state.focused_node,
            &mut full_window_state.pending_focus_target,
            &full_window_state.hovered_nodes[dom_id],
            full_window_state.mouse_state.mouse_down(),
        ))
    }).collect()
}

// Do the layout for a single window
#[cfg(not(test))]
fn layout_display_lists(
    data: &mut T,
    app_resources: &mut AppResources,
    window: &mut Window<T>,
    fake_display: &mut FakeDisplay<T>,
    ui_states: &mut BTreeMap<DomId, UiState<T>>,
    ui_descriptions: &mut BTreeMap<DomId, UiDescription<T>>,
    full_window_state: &mut FullWindowState,
) -> BTreeMap<DomId, CachedDisplayListResult> {
    ui_states.iter().map(|(dom_id, ui_state)| {
        let ui_description = &ui_descriptions[dom_id];
        (*dom_id, layout_display_list(
            data,
            ui_description,
            ui_state,
            window,
            &full_window_state,
            fake_display,
            app_resources,
        ))
    }).collect()
}

/// Returns the currently hit-tested results, in back-to-front order
#[cfg(not(test))]
fn do_hit_test<T>(
    window: &Window<T>,
    full_window_state: &FullWindowState,
    fake_display: &mut FakeDisplay<T>,
) -> Vec<HitTestItem> {

    use wr_translate::{wr_translate_hittest_item, wr_translate_pipeline_id};

    let cursor_location = match full_window_state.mouse_state.cursor_position.get_position() {
        Some(pos) => WorldPoint::new(pos.x, pos.y),
        None => return Vec::new(),
    };

    let mut hit_test_results: Vec<HitTestItem> = fake_display.render_api.hit_test(
        window.internal.document_id,
        Some(wr_translate_pipeline_id(window.internal.pipeline_id)),
        cursor_location,
        HitTestFlags::FIND_ALL
    ).items.into_iter().map(wr_translate_hittest_item).collect();

    // Execute callbacks back-to-front, not front-to-back
    hit_test_results.reverse();

    hit_test_results
}

/// Struct returned from the `call_callbacks()` function -
/// returns important information from the callbacks
#[derive(Debug, Clone, PartialEq, PartialOrd)]
struct CallCallbackReturn {
    /// Whether one or more callbacks say to redraw the screen or not
    pub should_update_screen: UpdateScreen,
    /// Whether one or more callbacks have messed with the current
    /// focused element i.e. via `.clear_focus()` or similar.
    pub callbacks_overwrites_focus: Option<FocusTarget>,
    /// Whether the screen should be redrawn even if no Callback returns an `UpdateScreen::Redraw`.
    /// This is necessary for `:hover` and `:active` mouseovers - otherwise the screen would
    /// only update on the next resize.
    pub needs_redraw_anyways: bool,
    /// Same as `needs_redraw_anyways`, but for reusing the layout from the previous frame.
    /// Each `:hover` and `:active` group stores whether it modifies the layout, as
    /// a performance optimization.
    pub needs_relayout_anyways: bool,
}

/// Given the glutin events of a single window and the hit test results,
/// determines which `On::X` filters to actually call.
fn determine_events<T>(
    event: &GlutinWindowEvent,
    ui_state_map: &BTreeMap<DomId, UiState<T>>,
    hit_test_results: &[HitTestItem],
    full_window_state: &mut FullWindowState,
) -> BTreeMap<DomId, CallbacksOfHitTest<T>> {
    use window_state::determine_callbacks;
    ui_state_map.iter().map(|(dom_id, ui_state)| {
        (dom_id.clone(), determine_callbacks(full_window_state, &hit_test_results, event, ui_state))
    }).collect()
}

/// Returns an bool whether the window should be redrawn or not
/// (true - redraw the screen, false: don't redraw).
fn call_callbacks<T>(
    data: &mut T,
    callbacks_filter_list: BTreeMap<DomId, CallbacksOfHitTest<T>>,
    ui_state_map: &BTreeMap<DomId, UiState<T>>,
    default_callbacks: &mut BTreeMap<DomId, DefaultCallbackIdMap<T>>,
    timers: &mut FastHashMap<TimerId, Timer<T>>,
    tasks: &mut Vec<Task<T>>,
    scroll_states: &BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>,
    modifiable_window_state: &mut WindowState,
    current_window_state: &FullWindowState,
    layout_result: &BTreeMap<DomId, LayoutResult>,
    scrolled_nodes: &BTreeMap<DomId, ScrolledNodes>,
    nodes_scrolled_in_callbacks: &mut BTreeMap<DomId, BTreeMap<NodeId, LayoutPoint>>,
    cached_display_list: &CachedDisplayList,
    gl_context: Rc<Gl>,
    resources: &mut AppResources,
) -> CallCallbackReturn {

    use callbacks::{CallbackInfo, DefaultCallbackInfoUnchecked};

    let mut should_update_screen = DontRedraw;
    let mut callbacks_overwrites_focus = None;

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
                            current_window_state,
                            modifiable_window_state,
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
                            hit_test_items: &hit_test_items,
                            cursor_relative_to_item: hit_item.as_ref().map(|hi| (hi.point_relative_to_item.x, hi.point_relative_to_item.y)),
                            cursor_in_viewport: hit_item.as_ref().map(|hi| (hi.point_in_viewport.x, hi.point_in_viewport.y)),
                        })
                    },
                    None => DontRedraw,
                };

                if default_callback_redraws == Redraw {
                    should_update_screen = Redraw;
                }

                // Overwrite the focus from the callback info
                if let Some(new_focus) = new_focus.clone() {
                    callbacks_overwrites_focus = Some(new_focus);
                }
            }
        }
    }

    for dom_id in ui_state_map.keys().cloned() {
        for (node_id, callback_results) in callbacks_filter_list[&dom_id].nodes_with_callbacks.iter() {
            let hit_item = &callback_results.hit_test_item;
            for callback in callback_results.normal_callbacks.values() {

                let mut new_focus = None;

                if (callback.0)(CallbackInfo {
                    data,
                    current_window_state,
                    modifiable_window_state,
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
                    hit_test_items: &hit_test_items,
                    cursor_relative_to_item: hit_item.as_ref().map(|hi| (hi.point_relative_to_item.x, hi.point_relative_to_item.y)),
                    cursor_in_viewport: hit_item.as_ref().map(|hi| (hi.point_in_viewport.x, hi.point_in_viewport.y)),
                }) == Redraw {
                    should_update_screen = Redraw;
                }

                if let Some(new_focus) = new_focus {
                    callbacks_overwrites_focus = Some(new_focus);
                }
            }
        }
    }

    CallCallbackReturn {
        should_update_screen,
        callbacks_overwrites_focus,
        needs_redraw_anyways: callbacks_filter_list.values().any(|v| v.needs_redraw_anyways),
        needs_relayout_anyways: callbacks_filter_list.values().any(|v| v.needs_relayout_anyways),
    }
}

// Most expensive step: Do the layout + build the actual display list (but don't send it to webrender yet!)
#[cfg(not(test))]
fn layout_display_list<T>(
    data: &mut T,
    ui_description: &UiDescription<T>,
    ui_state: &UiState<T>,
    window: &mut Window<T>,
    full_window_state: &FullWindowState,
    fake_display: &mut FakeDisplay<T>,
    app_resources: &mut AppResources,
) -> CachedDisplayListResult {

    let display_list = display_list_from_ui_description(ui_description, ui_state);

    // Make sure unused scroll states are garbage collected.
    window.internal.scroll_states.remove_unused_scroll_states();

    fake_display.hidden_context.make_not_current();
    window.display.make_current();

    DomId::reset();

    let display_list = display_list_to_cached_display_list(display_list, data, window, full_window_state, app_resources, &mut fake_display.render_api);

    window.display.make_not_current();
    fake_display.hidden_context.make_current();

    display_list
}

/// Build the display list and send it to webrender
#[cfg(not(test))]
fn send_display_list_to_webrender<T>(
    display_list: CachedDisplayListResult,
    window: &mut Window<T>,
    full_window_state: &FullWindowState,
    fake_display: &mut FakeDisplay<T>,
    app_resources: &mut AppResources,
) {
    use display_list::{
        display_list_from_ui_description,
        display_list_to_cached_display_list,
        CachedDisplayListResult,
    };
    use app_resources::add_resources;
    use wr_translate::{
        wr_translate_pipeline_id,
        wr_translate_display_list,
    };

    for (_dom_id, image_resource_updates) in display_list.image_resource_updates {
        add_resources(app_resources, &mut fake_display.render_api, &window.internal.pipeline_id, Vec::new(), image_resource_updates);
    }

    window.internal.layout_result = display_list.layout_result;
    window.internal.scrolled_nodes = display_list.scrollable_nodes;
    window.internal.cached_display_list = display_list.cached_display_list.clone();

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    let display_list = wr_translate_display_list(display_list.cached_display_list, window.internal.pipeline_id);

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
    fake_display.hidden_context.make_not_current();
}

/// Scroll all nodes in the ScrollStates to their correct position and insert
/// the positions into the transaction
///
/// NOTE: scroll_states has to be mutable, since every key has a "visited" field, to
/// indicate whether it was used during the current frame or not.
fn scroll_all_nodes(scroll_states: &mut ScrollStates, txn: &mut Transaction) {
    use webrender::api::ScrollClamping;
    use wr_translate::{wr_translate_external_scroll_id, wr_translate_layout_point};
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

    let scroll_x = full_window_state.mouse_state.scroll_x;
    let scroll_y = full_window_state.mouse_state.scroll_y;

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

    use compositor::remove_epochs_from_pipeline;
    use wr_translate::wr_translate_pipeline_id;

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
    use wr_translate;

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
    #version 100
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
    #version 100
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

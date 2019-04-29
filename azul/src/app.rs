use std::{
    mem,
    fmt,
    rc::Rc,
    time::Instant,
    collections::BTreeMap,
};
#[cfg(debug_assertions)]
use azul_css::HotReloadHandler;
use glium::{
    SwapBuffersError,
    glutin::WindowEvent,
};
use gleam::gl::{self, Gl, GLuint};
use webrender::{
    PipelineInfo, Renderer,
    api::{
        HitTestFlags, DevicePixel, WorldPoint,
        LayoutSize, LayoutPoint, Epoch, Transaction,
    },
};
#[cfg(feature = "image_loading")]
use app_resources::ImageSource;
#[cfg(feature = "logging")]
use log::LevelFilter;
use azul_css::{Css, ColorU};
use {
    FastHashMap,
    window::{
        Window, FakeWindow, ScrollStates, LogicalPosition, LogicalSize, FakeDisplay,
        WindowCreateError, WindowCreateOptions, RendererType, WindowSize, DebugState,
        FullWindowState,
    },
    dom::{Dom, ScrollTagId},
    gl::GlShader,
    traits::Layout,
    ui_state::UiState,
    ui_description::UiDescription,
    async::{Task, TimerId, TerminateTimer},
    callbacks::{
        FocusTarget, UpdateScreen, HitTestItem, Redraw, DontRedraw, LayoutInfo,
    },
    display_list::ScrolledNodes,
};
pub use app_resources::AppResources;
pub use azul_core::{
    app::{AppState, AppStateNoData},
    window::WindowId,
};
#[cfg(test)]
use app_resources::FakeRenderApi;

type DeviceIntSize = ::euclid::TypedSize2D<i32, DevicePixel>;

// Default clear color is white, to signify that there is rendering going on
// (otherwise, "transparent") backgrounds would be painted black.
const COLOR_WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 0 };

/// Graphical application that maintains some kind of application state
pub struct App<T> {
    /// The graphical windows, indexed by their system ID / handle
    windows: BTreeMap<WindowId, Window<T>>,
    /// Actual state of the window (synchronized with the OS window)
    window_states: BTreeMap<WindowId, FullWindowState>,
    /// The global application state
    pub app_state: AppState<T>,
    /// Application configuration, whether to enable logging, etc.
    pub config: AppConfig,
    /// The `Layout::layout()` callback, stored as a function pointer,
    /// There are multiple reasons for doing this (instead of requiring `T: Layout` everywhere):
    ///
    /// - It seperates the `Dom<T>` from the `Layout` trait, making it possible to split the UI solving and styling into reusable crates
    /// - It's less typing work (prevents having to type `<T: Layout>` everywhere)
    /// - It's potentially more efficient to compile (less type-checking required)
    /// - It's a preparation for the C ABI, in which traits don't exist (for language bindings).
    ///   In the C ABI "traits" are simply structs with function pointers (and void* instead of T)
    layout_callback: fn(&T, layout_info: LayoutInfo<T>) -> Dom<T>,
    /// The actual renderer of this application
    #[cfg(not(test))]
    fake_display: FakeDisplay,
    #[cfg(test)]
    render_api: FakeRenderApi,
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
        }
    }
}

/// Error returned by the `.run()` function
///
/// If the `.run()` function would panic, that would need `T` to
/// implement `Debug`, which is not necessary if we just return an error.
#[derive(Debug)]
pub enum RuntimeError {
    /// Could not swap the display (drawing error)
    GlSwapError(SwapBuffersError),
    /// Error indexing into internal BTreeMap - wrong window ID
    WindowIndexError,
}

pub(crate) struct FrameEventInfo {
    pub(crate) should_redraw_window: bool,
    pub(crate) should_hittest: bool,
    pub(crate) cur_cursor_pos: LogicalPosition,
    pub(crate) new_window_size: Option<LogicalSize>,
    pub(crate) new_dpi_factor: Option<f32>,
    pub(crate) is_resize_event: bool,
}

impl Default for FrameEventInfo {
    fn default() -> Self {
        Self {
            should_redraw_window: false,
            should_hittest: false,
            cur_cursor_pos: LogicalPosition::new(0.0, 0.0),
            new_window_size: None,
            new_dpi_factor: None,
            is_resize_event: false,
        }
    }
}

impl From<SwapBuffersError> for RuntimeError {
    fn from(e: SwapBuffersError) -> Self {
        RuntimeError::GlSwapError(e)
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RuntimeError::*;
        match self {
            GlSwapError(e) => write!(f, "Failed to swap GL display: {}", e),
            WindowIndexError => write!(f, "Invalid window index"),
        }
    }
}

impl<T: Layout> App<T> {

    #[cfg(not(test))]
    #[allow(unused_variables)]
    /// Create a new, empty application. This does not open any windows.
    pub fn new(initial_data: T, app_config: AppConfig) -> Result<Self, WindowCreateError> {

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
                set_webrender_debug_flags(r, &DebugState::default(), &app_config.debug_state);
            }
            Ok(Self {
                windows: BTreeMap::new(),
                window_states: BTreeMap::new(),
                app_state: AppState::new(initial_data),
                config: app_config,
                layout_callback: T::layout,
                fake_display,
            })
        }

        #[cfg(test)] {
           Ok(Self {
               windows: BTreeMap::new(),
               window_states: BTreeMap::new(),
               app_state: AppState::new(initial_data),
               config: app_config,
               layout_callback: T::layout,
               render_api: FakeRenderApi::new(),
           })
        }
    }
}

impl<T> App<T> {

    /// Creates a new window
    #[cfg(not(test))]
    pub fn create_window(&mut self, options: WindowCreateOptions, css: Css)
    -> Result<Window<T>, WindowCreateError>
    {
        Window::new(
            &mut self.fake_display.render_api,
            &mut self.fake_display.hidden_display.gl_window().context(),
            &mut self.fake_display.hidden_events_loop,
            options,
            css,
            self.config.background_color,
        )
    }

    #[cfg(debug_assertions)]
    #[cfg(not(test))]
    pub fn create_hot_reload_window(&mut self, options: WindowCreateOptions, css_loader: Box<dyn HotReloadHandler>)
    -> Result<Window<T>, WindowCreateError> {
        Window::new_hot_reload(
            &mut self.fake_display.render_api,
            &mut self.fake_display.hidden_display.gl_window().context(),
            &mut self.fake_display.hidden_events_loop,
            options,
            css_loader,
            self.config.background_color,
        )
    }

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn add_window(&mut self, window: Window<T>) {
        use window_state::full_window_state_from_normal_state;
        let window_id = window.id;
        let fake_window = FakeWindow {
            state: window.state.clone(),
            default_callbacks: BTreeMap::new(),
            gl_context: window.get_gl_context(),
        };
        let full_window_state = full_window_state_from_normal_state(window.state.clone());
        self.app_state.windows.insert(window_id, fake_window);
        self.windows.insert(window_id, window);
        self.window_states.insert(window_id, full_window_state);
    }

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
    pub fn run(mut self, window: Window<T>) -> Result<T, RuntimeError> {

        // Apps need to have at least one window open
        self.add_window(window);
        self.run_inner()?;

        // NOTE: This is necessary because otherwise, the Arc::try_unwrap would fail,
        // since one Arc is still owned by the app_state.tasks structure
        //
        // See https://github.com/maps4print/azul/issues/24#issuecomment-429737273
        mem::drop(self.app_state.tasks);

        Ok(self.app_state.data)
    }

    #[cfg(not(test))]
    fn run_inner(&mut self) -> Result<(), RuntimeError> {

        use std::{thread, time::Duration};
        use glium::glutin::Event;
        use ui_state::ui_state_from_app_state;
        use self::RuntimeError::*;

        let mut ui_state_cache = {
            let app_state = &mut self.app_state;
            let mut ui_state_map = BTreeMap::new();

            for window_id in self.windows.keys() {
                ui_state_map.insert(*window_id, ui_state_from_app_state(app_state, window_id, self.layout_callback)?);
            }

            ui_state_map
        };
        let mut ui_description_cache = self.windows.keys().map(|window_id| (*window_id, UiDescription::default())).collect::<BTreeMap<_, _>>();
        let mut force_redraw_cache = self.windows.keys().map(|window_id| (*window_id, 2)).collect();
        let mut awakened_tasks = self.windows.keys().map(|window_id| (*window_id, false)).collect();

        #[cfg(debug_assertions)]
        let mut last_style_reload = Instant::now();
        #[cfg(debug_assertions)]
        let mut should_print_css_error = true;

        while !self.windows.is_empty() {

            let time_start = Instant::now();

            use glium::glutin::WindowId as GliumWindowId;

            let glium_window_id_to_window_id = self.windows.iter()
                .map(|(window_id, window)| (window.display.gl_window().id(), *window_id))
                .collect::<BTreeMap<GliumWindowId, WindowId>>();

            let mut closed_windows = Vec::<WindowId>::new();
            let mut frame_was_resize = false;
            let mut events = BTreeMap::new();

            self.fake_display.hidden_events_loop.poll_events(|e| match e {
                // Filter out all events that are uninteresting or unnecessary
                Event::WindowEvent { event: WindowEvent::Refresh, .. } => { },
                Event::WindowEvent { window_id, event } => {
                    events.entry(glium_window_id_to_window_id[&window_id]).or_insert_with(|| Vec::new()).push(event);
                },
                _ => { },
            });

            let mut single_window_results = Vec::with_capacity(self.windows.len());

            for (current_window_id, mut window) in self.windows.iter_mut() {

                // Only process the events belong to this window ID...
                let window_events: Vec<WindowEvent> = events.get(current_window_id).cloned().unwrap_or_default();

                let single_window_result =
                    hit_test_single_window(
                        &window_events,
                        &current_window_id,
                        &mut window,
                        self.window_states.get_mut(current_window_id).ok_or(WindowIndexError)?,
                        &mut self.app_state,
                        &mut self.fake_display,
                        &mut ui_state_cache,
                        &mut force_redraw_cache,
                        &mut awakened_tasks,
                    )?;

                if single_window_result.needs_relayout_resize {
                    frame_was_resize = true;
                }

                if single_window_result.window_should_close {
                    closed_windows.push(*current_window_id);

                    // TODO: Currently there is no way to return from the main event loop
                    // i.e. the windows aren't actually getting closed
                    // This is a hack, so that windows currently close properly
                    return Ok(());
                }

                single_window_results.push(single_window_result);
            }

            // Close windows if necessary
            closed_windows.into_iter().for_each(|closed_window_id| {
                ui_state_cache.remove(&closed_window_id);
                ui_description_cache.remove(&closed_window_id);
                force_redraw_cache.remove(&closed_window_id);
                self.windows.remove(&closed_window_id);
                self.window_states.remove(&closed_window_id);
            });

            #[cfg(debug_assertions)]
            let css_has_reloaded = hot_reload_css(
                    &mut self.windows,
                    &mut last_style_reload,
                    &mut should_print_css_error,
                )?;

            #[cfg(not(debug_assertions))]
            let css_has_reloaded = false;

            let should_relayout_all_windows = css_has_reloaded || single_window_results.iter().any(|res| res.should_relayout());
            let should_rerender_all_windows = should_relayout_all_windows || single_window_results.iter().any(|res| res.should_rerender());

            let should_redraw_timers = app_state_run_all_timers(&mut self.app_state);
            let should_redraw_tasks = app_state_clean_up_finished_tasks(&mut self.app_state);
            let should_redraw_timers_or_tasks = [should_redraw_timers, should_redraw_tasks].into_iter().any(|e| *e == Redraw);

            // If there is a relayout necessary, re-layout *all* windows!
            if should_relayout_all_windows || should_redraw_timers_or_tasks {
                for (current_window_id, mut window) in self.windows.iter_mut() {
                    let full_window_state = self.window_states.get_mut(current_window_id).unwrap();
                    relayout_single_window(
                        self.layout_callback,
                        &current_window_id,
                        &mut window,
                        full_window_state,
                        &mut self.app_state,
                        &mut self.fake_display,
                        &mut ui_state_cache,
                        &mut ui_description_cache,
                        &mut force_redraw_cache,
                        &mut awakened_tasks,
                    )?;
                }
            }

            // If there is a re-render necessary, re-render *all* windows
            if should_rerender_all_windows || should_redraw_timers_or_tasks {
                for window in self.windows.values_mut() {
                    // TODO: For some reason this function has to be called twice in order
                    // to actually update the screen. For some reason the first swap_buffers() has
                    // no effect (winit bug?)
                    rerender_single_window(
                        &self.config,
                        window,
                        &mut self.fake_display,
                    );
                    rerender_single_window(
                        &self.config,
                        window,
                        &mut self.fake_display,
                    );
                }
            }

            if should_relayout_all_windows || should_redraw_timers_or_tasks {
                use app_resources::garbage_collect_fonts_and_images;

                // Automatically remove unused fonts and images from webrender
                // Tell the font + image GC to start a new frame
                #[cfg(not(test))] {
                    garbage_collect_fonts_and_images(
                        &mut self.app_state.resources,
                        &mut self.fake_display.render_api,
                    );
                }
                #[cfg(test)] {
                    garbage_collect_fonts_and_images(
                        &mut self.app_state.resources,
                        &mut self.fake_render_api,
                    );
                }
            }

            if !frame_was_resize {
                // Wait until 16ms have passed, but not during a resize event
                let diff = time_start.elapsed();
                const FRAME_TIME: Duration = Duration::from_millis(16);
                if diff < FRAME_TIME {
                    thread::sleep(FRAME_TIME - diff);
                }
            }
        }

        Ok(())
    }

    /// See `AppState::add_task`.
    pub fn add_task(&mut self, task: Task<T>) {
        self.app_state.add_task(task);
    }

    /// Toggles debugging flags in webrender, updates `self.config.debug_state`
    #[cfg(not(test))]
    pub fn toggle_debug_flags(&mut self, new_state: DebugState) {
        if let Some(r) = &mut self.fake_display.renderer {
            set_webrender_debug_flags(r, &self.config.debug_state, &new_state);
        }
        self.config.debug_state = new_state;
    }
}

/// Run all currently registered timers
#[must_use]
fn app_state_run_all_timers<T>(app_state: &mut AppState<T>) -> UpdateScreen {

    let mut should_update_screen = DontRedraw;
    let mut timers_to_terminate = Vec::new();

    for (key, timer) in app_state.timers.iter_mut() {
        let (should_update, should_terminate) = timer.invoke_callback_with_data(
            &mut app_state.data, &mut app_state.resources
        );

        if should_update == Redraw {
            should_update_screen = Redraw;
        }

        if should_terminate == TerminateTimer::Terminate {
            timers_to_terminate.push(key.clone());
        }
    }

    for key in timers_to_terminate {
        app_state.timers.remove(&key);
    }

    should_update_screen
}

/// Remove all tasks that have finished executing
#[must_use] fn app_state_clean_up_finished_tasks<T>(app_state: &mut AppState<T>) -> UpdateScreen {
    let old_count = app_state.tasks.len();
    let mut timers_to_add = Vec::new();
    app_state.tasks.retain(|task| {
        if task.is_finished() {
            if let Some(timer) = task.after_completion_timer {
                timers_to_add.push((TimerId::new(), timer));
            }
            false
        } else {
            true
        }
    });

    let timers_is_empty = timers_to_add.is_empty();
    let new_count = app_state.tasks.len();

    // Start all the timers that should run after the completion of the task
    for (timer_id, timer) in timers_to_add {
        app_state.add_timer(timer_id, timer);
    }

    if old_count == new_count && timers_is_empty {
        DontRedraw
    } else {
        Redraw
    }
}

struct SingleWindowContentResult {
    needs_rerender_hover_active: bool,
    needs_relayout_hover_active: bool,
    needs_relayout_resize: bool,
    window_should_close: bool,
    should_scroll_render: bool,
    needs_relayout_tasks: bool,
    needs_relayout_refresh: bool,
    callbacks_update_screen: UpdateScreen,
    hit_test_results: Option<Vec<HitTestItem>>,
    new_focus_target: Option<FocusTarget>,
}

impl SingleWindowContentResult {

    pub fn should_relayout(&self) -> bool {
        self.needs_relayout_hover_active ||
        self.needs_relayout_resize ||
        self.needs_relayout_tasks ||
        self.needs_relayout_refresh ||
        self.callbacks_update_screen == Redraw
    }

    pub fn should_rerender(&self) -> bool {
        self.should_relayout() || self.should_scroll_render || self.needs_rerender_hover_active
    }
}

/// Call the callbacks / do the hit test
/// Returns (if the event was a resize event, if the window was closed)
#[cfg(not(test))]
fn hit_test_single_window<T>(
    events: &[WindowEvent],
    window_id: &WindowId,
    window: &mut Window<T>,
    full_window_state: &mut FullWindowState,
    app_state: &mut AppState<T>,
    fake_display: &mut FakeDisplay,
    ui_state_cache: &mut BTreeMap<WindowId, UiState<T>>,
    force_redraw_cache: &mut BTreeMap<WindowId, usize>,
    awakened_tasks: &mut BTreeMap<WindowId, bool>,
) -> Result<SingleWindowContentResult, RuntimeError> {

    use self::RuntimeError::*;
    use window;

    let (mut frame_event_info, window_should_close) = window::update_window_state(full_window_state, &events);
    let mut ret = SingleWindowContentResult {
        needs_rerender_hover_active: false,
        needs_relayout_hover_active: false,
        needs_relayout_resize: frame_event_info.is_resize_event,
        window_should_close,
        should_scroll_render: false,
        needs_relayout_tasks: *(awakened_tasks.get(window_id).ok_or(WindowIndexError)?),
        needs_relayout_refresh: *(force_redraw_cache.get(window_id).ok_or(WindowIndexError)?) > 0,
        callbacks_update_screen: DontRedraw,
        hit_test_results: None,
        new_focus_target: None,
    };

    if events.is_empty() && !ret.should_relayout() && !ret.should_rerender() {
        // Event was not a resize event, window should **not** close
        ret.window_should_close = window_should_close;
        return Ok(ret);
    }

    if frame_event_info.should_hittest {

        ret.hit_test_results = do_hit_test(&window, full_window_state, fake_display);

        for event in events.iter() {

            app_state.windows.get_mut(window_id).unwrap().state =
                window::full_window_state_to_window_state(full_window_state);

            let callback_result = call_callbacks(
                ret.hit_test_results.as_ref(),
                event,
                full_window_state,
                &window_id,
                ui_state_cache.get_mut(window_id).ok_or(WindowIndexError)?,
                app_state
            )?;

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
        }
    }

    ret.hit_test_results = ret.hit_test_results.or_else(|| do_hit_test(window, full_window_state, fake_display));

    // Scroll for the scrolled amount for each node that registered a scroll state.
    let should_scroll_render = match &ret.hit_test_results {
        Some(hit_test_results) => {
            update_scroll_state(
                full_window_state,
                &window.internal.last_scrolled_nodes,
                &mut window.scroll_states,
                hit_test_results,
            )
        }
        None => false,
    };

    ret.should_scroll_render = should_scroll_render;

    // if frame_event_info.is_resize_event {
    //     // This is a hack because during a resize event, winit eats the "awakened"
    //     // event. So what we do is that we call the layout-and-render again, to
    //     // trigger a second "awakened" event. So when the window is resized, the
    //     // layout function is called twice (the first event will be eaten by winit)
    //     //
    //     // This is a reported bug and should be fixed somewhere in July
    //     *force_redraw_cache.get_mut(window_id).ok_or(WindowIndexError)? = 2;
    // }

    // See: https://docs.rs/glutin/0.19.0/glutin/struct.CombinedContext.html#method.resize
    //
    // Some platforms (macOS, Wayland) require being manually updated when their window
    // or surface is resized.
    #[cfg(not(target_os = "windows"))] {
        if frame_event_info.is_resize_event {
            // Resize gl window
            let gl_window = window.display.gl_window();
            let size = gl_window.get_inner_size().unwrap().to_physical(gl_window.get_hidpi_factor());
            gl_window.resize(size);
        }
    }

    // Update the FullWindowState that we got from the frame event (updates window dimensions and DPI)
    full_window_state.pending_focus_target = ret.new_focus_target.clone();

    // Update the window state every frame that was set by the user
    window::synchronize_window_state_with_os_window(
        full_window_state,
        &mut app_state.windows.get_mut(window_id).unwrap().state,
        &*window.display.gl_window(),
    );

    window::update_from_external_window_state(
        full_window_state,
        &mut frame_event_info,
        &fake_display.hidden_events_loop,
        &window.display.gl_window()
    );

    // Reset the scroll amount to 0 (for the next frame)
    window::clear_scroll_state(full_window_state);

    app_state.windows.get_mut(window_id).unwrap().state = window::full_window_state_to_window_state(full_window_state);

    Ok(ret)
}

#[cfg(not(test))]
fn relayout_single_window<T>(
    layout_callback: fn(&T, LayoutInfo<T>) -> Dom<T>,
    window_id: &WindowId,
    window: &mut Window<T>,
    full_window_state: &mut FullWindowState,
    app_state: &mut AppState<T>,
    fake_display: &mut FakeDisplay,
    ui_state_cache: &mut BTreeMap<WindowId, UiState<T>>,
    ui_description_cache: &mut BTreeMap<WindowId, UiDescription<T>>,
    force_redraw_cache: &mut BTreeMap<WindowId, usize>,
    awakened_tasks: &mut BTreeMap<WindowId, bool>,
) -> Result<(), RuntimeError> {

    use self::RuntimeError::*;
    use ui_state::ui_state_from_app_state;

    // Call the Layout::layout() fn, get the DOM
    *ui_state_cache.get_mut(window_id).ok_or(WindowIndexError)? =
        ui_state_from_app_state(
            app_state,
            window_id,
            layout_callback
        )?;

    // Style the DOM (is_mouse_down is necessary for styling :hover, :active + :focus nodes)
    let is_mouse_down = full_window_state.mouse_state.mouse_down();

    *ui_description_cache.get_mut(window_id).ok_or(WindowIndexError)? =
        UiDescription::match_css_to_dom(
            ui_state_cache.get_mut(window_id).ok_or(WindowIndexError)?,
            &window.css,
            &mut full_window_state.focused_node,
            &mut full_window_state.pending_focus_target,
            &full_window_state.hovered_nodes,
            is_mouse_down,
        );

    let mut fake_window = app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?;
    update_display_list(
        &mut app_state.data,
        &ui_description_cache[window_id],
        &ui_state_cache[window_id],
        &mut *window,
        &mut fake_window,
        fake_display,
        &mut app_state.resources,
    );
    *awakened_tasks.get_mut(window_id).ok_or(WindowIndexError)? = false;

    if let Some(i) = force_redraw_cache.get_mut(window_id) {
        if *i > 0 { *i -= 1 };
        if *i == 1 {
            clean_up_unused_opengl_textures(fake_display.renderer.as_mut().unwrap().flush_pipeline_info());
        }
    }

    Ok(())
}

#[cfg(not(test))]
fn rerender_single_window<T>(
    config: &AppConfig,
    window: &mut Window<T>,
    fake_display: &mut FakeDisplay,
) {
    render_inner(window, fake_display, Transaction::new(), config.background_color);
}

/// Returns if the CSS has been successfully reloaded
#[cfg(debug_assertions)]
fn hot_reload_css<T>(
    windows: &mut BTreeMap<WindowId, Window<T>>,
    last_style_reload: &mut Instant,
    should_print_error: &mut bool,
) -> Result<bool, RuntimeError> {

    let mut has_reloaded = false;

    for window in windows.values_mut() {

        // Hot-reload a style if necessary
        let hot_reloader = match window.css_loader.as_mut() {
            None => continue,
            Some(s) => s,
        };

        let should_reload = Instant::now() - *last_style_reload > hot_reloader.get_reload_interval();

        if !should_reload {
            continue;
        }

        match hot_reloader.reload_style() {
            Ok(mut new_css) => {
                new_css.sort_by_specificity();
                window.css = new_css;
                if !(*should_print_error) {
                    println!("--- OK: CSS parsed without errors, continuing hot-reload.");
                }
                *last_style_reload = Instant::now();
                has_reloaded = true;
                *should_print_error = true;
            },
            Err(why) => {
                if *should_print_error {
                    println!("{}", why);
                }
                *should_print_error = false;
            },
        };
    }

    Ok(has_reloaded)
}

/// Returns the currently hit-tested results, in back-to-front order
#[cfg(not(test))]
fn do_hit_test<T>(
    window: &Window<T>,
    full_window_state: &FullWindowState,
    fake_display: &mut FakeDisplay,
) -> Option<Vec<HitTestItem>> {

    use wr_translate::{wr_translate_hittest_item, wr_translate_pipeline_id};

    let cursor_location = full_window_state.mouse_state.cursor_pos.get_position().map(|pos| WorldPoint::new(pos.x, pos.y))?;

    let mut hit_test_results: Vec<HitTestItem> = fake_display.render_api.hit_test(
        window.internal.document_id,
        Some(wr_translate_pipeline_id(window.internal.pipeline_id)),
        cursor_location,
        HitTestFlags::FIND_ALL
    ).items.into_iter().map(wr_translate_hittest_item).collect();

    // Execute callbacks back-to-front, not front-to-back
    hit_test_results.reverse();

    Some(hit_test_results)
}

/// Struct returned from the `call_callbacks()` function -
/// returns important information from the callbacks
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

/// Returns an bool whether the window should be redrawn or not (true - redraw the screen, false: don't redraw).
fn call_callbacks<T>(
    hit_test_results: Option<&Vec<HitTestItem>>,
    event: &WindowEvent,
    full_window_state: &mut FullWindowState,
    window_id: &WindowId,
    ui_state: &UiState<T>,
    app_state: &mut AppState<T>
) -> Result<CallCallbackReturn, RuntimeError> {

    use {
        callbacks::CallbackInfo,
        window_state::determine_callbacks,
        window::fake_window_run_default_callback,
    };

    let mut should_update_screen = DontRedraw;

    let hit_test_items = hit_test_results.map(|items| items.clone()).unwrap_or_default();

    let callbacks_filter_list = determine_callbacks(full_window_state, &hit_test_items, event, ui_state);

    let mut callbacks_overwrites_focus = None;

    let mut default_timers = FastHashMap::default();
    let mut default_tasks = Vec::new();

    // Run all default callbacks - **before** the user-defined callbacks are run!
    for (node_id, callback_results) in callbacks_filter_list.nodes_with_callbacks.iter() {
        let hit_item = &callback_results.hit_test_item;
        for default_callback_id in callback_results.default_callbacks.values() {

            let mut callback_info = CallbackInfo {
                focus: None,
                window_id,
                hit_dom_node: *node_id,
                ui_state,
                hit_test_items: &hit_test_items,
                cursor_relative_to_item: hit_item.as_ref().map(|hi| (hi.point_relative_to_item.x, hi.point_relative_to_item.y)),
                cursor_in_viewport: hit_item.as_ref().map(|hi| (hi.point_in_viewport.x, hi.point_in_viewport.y)),
            };

            let mut app_state_no_data = AppStateNoData {
                windows: &app_state.windows,
                resources: &mut app_state.resources,
                timers: FastHashMap::default(),
                tasks: Vec::new(),
            };

            if fake_window_run_default_callback(
                &app_state.windows[window_id],
                &mut app_state.data,
                default_callback_id,
                &mut app_state_no_data,
                &mut callback_info
            ) == Redraw {
                should_update_screen = Redraw;
            }

            default_timers.extend(app_state_no_data.timers.into_iter());
            default_tasks.extend(app_state_no_data.tasks.into_iter());

            // Overwrite the focus from the callback info
            if let Some(new_focus) = callback_info.focus {
                callbacks_overwrites_focus = Some(new_focus);
            }
        }
    }

    // If the default callbacks have started timers or tasks, add them to the main app state
    for (timer_id, timer) in default_timers {
        app_state.add_timer(timer_id, timer);
    }

    for task in default_tasks {
        app_state.add_task(task);
    }

    for (node_id, callback_results) in callbacks_filter_list.nodes_with_callbacks.iter() {
        let hit_item = &callback_results.hit_test_item;
        for callback in callback_results.normal_callbacks.values() {

            let mut callback_info = CallbackInfo {
                focus: None,
                window_id,
                hit_dom_node: *node_id,
                ui_state: &ui_state,
                hit_test_items: &hit_test_items,
                cursor_relative_to_item: hit_item.as_ref().map(|hi| (hi.point_relative_to_item.x, hi.point_relative_to_item.y)),
                cursor_in_viewport: hit_item.as_ref().map(|hi| (hi.point_in_viewport.x, hi.point_in_viewport.y)),
            };

            if (callback.0)(app_state, &mut callback_info) == Redraw {
                should_update_screen = Redraw;
            }

            if let Some(new_focus) = callback_info.focus {
                callbacks_overwrites_focus = Some(new_focus);
            }
        }
    }

    Ok(CallCallbackReturn {
        should_update_screen,
        callbacks_overwrites_focus,
        needs_redraw_anyways: callbacks_filter_list.needs_redraw_anyways,
        needs_relayout_anyways: callbacks_filter_list.needs_relayout_anyways,
    })
}

/// Build the display list and send it to webrender
#[cfg(not(test))]
fn update_display_list<T>(
    app_data: &mut T,
    ui_description: &UiDescription<T>,
    ui_state: &UiState<T>,
    window: &mut Window<T>,
    fake_window: &mut FakeWindow<T>,
    fake_display: &mut FakeDisplay,
    app_resources: &mut AppResources,
) {
    use display_list::{
        display_list_from_ui_description,
        display_list_to_cached_display_list,
    };
    use wr_translate::{
        wr_translate_pipeline_id,
        wr_translate_display_list,
    };

    let display_list = display_list_from_ui_description(ui_description, ui_state);

    // NOTE: layout_result contains all words, text information, etc.
    // - very important for selection!
    let (display_list, scrolled_nodes, _layout_result) = display_list_to_cached_display_list(
        display_list,
        app_data,
        window,
        fake_window,
        app_resources,
        &mut fake_display.render_api,
    );

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    let display_list = wr_translate_display_list(display_list);
    window.internal.last_scrolled_nodes = scrolled_nodes;

    let (logical_size, _) = convert_window_size(&window.state.size);

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
    for (key, value) in scroll_states.0.iter_mut() {
        let (x, y) = value.get();
        txn.scroll_node_with_id(LayoutPoint::new(x, y), *key, ScrollClamping::ToContentBounds);
    }
}

/// Returns the (logical_size, physical_size) as LayoutSizes, which can then be passed to webrender
fn convert_window_size(size: &WindowSize) -> (LayoutSize, DeviceIntSize) {
    let logical_size = LayoutSize::new(
        (size.dimensions.width * size.winit_hidpi_factor) as f32,
        (size.dimensions.height * size.winit_hidpi_factor)  as f32
    );
    let physical_size = size.dimensions.to_physical(size.winit_hidpi_factor);
    let physical_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
    (logical_size, physical_size)
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
        let scroll_id = scroll_node.parent_external_scroll_id;

        if scroll_states.0.contains_key(&scroll_id) {
            // TODO: make scroll speed configurable (system setting?)
            scroll_states.scroll_node(&scroll_id, scroll_x as f32, scroll_y as f32);
            should_scroll_render = true;
        }
    }

    should_scroll_render
}

fn clean_up_unused_opengl_textures(pipeline_info: PipelineInfo) {

    use compositor::ACTIVE_GL_TEXTURES;

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

    let mut active_textures_lock = ACTIVE_GL_TEXTURES.lock().unwrap();

    // Retain all OpenGL textures from epochs higher than the lowest epoch
    //
    // TODO: Handle overflow of Epochs correctly (low priority)
    active_textures_lock.retain(|key, _| key > oldest_to_remove_epoch);
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

// Function wrapper that is invoked on scrolling and normal rendering - only renders the
// window contents and updates the screen, assumes that all transactions via the RenderApi
// have been committed before this function is called.
//
// WebRender doesn't reset the active shader back to what it was, but rather sets it
// to zero, which glium doesn't know about, so on the next frame it tries to draw with shader 0.
// This leads to problems when invoking GlTextureCallbacks, because those don't expect
// the OpenGL state to change between calls. Also see: https://github.com/servo/webrender/pull/2880
//
// NOTE: For some reason, webrender allows rendering to a framebuffer with a
// negative width / height, although that doesn't make sense
#[cfg(not(test))]
fn render_inner<T>(
    window: &mut Window<T>,
    fake_display: &mut FakeDisplay,
    mut txn: Transaction,
    background_color: ColorU,
) {

    use window::get_gl_context;
    use glium::glutin::ContextTrait;
    use webrender::api::{DeviceIntRect, DeviceIntPoint};
    use azul_css::ColorF;
    use wr_translate;

    let (_, framebuffer_size) = convert_window_size(&window.state.size);

    // Especially during minimization / maximization of a window, it can happen that the window
    // width or height is zero. In that case, no rendering is necessary (doing so would crash
    // the application, since glTexImage2D may never have a 0 as the width or height.
    if framebuffer_size.width == 0 || framebuffer_size.height == 0 {
        return;
    }

    window.internal.epoch = increase_epoch(window.internal.epoch);

    txn.set_window_parameters(
        framebuffer_size.clone(),
        DeviceIntRect::new(DeviceIntPoint::new(0, 0), framebuffer_size),
        window.state.size.hidpi_factor as f32
    );
    txn.set_root_pipeline(wr_translate::wr_translate_pipeline_id(window.internal.pipeline_id));
    scroll_all_nodes(&mut window.scroll_states, &mut txn);
    txn.generate_frame();

    fake_display.render_api.send_transaction(window.internal.document_id, txn);

    // Update WR texture cache
    fake_display.renderer.as_mut().unwrap().update();

    let background_color_f: ColorF = background_color.into();

    unsafe {

        // NOTE: GlContext is the context of the app-global, hidden window
        // (that shares the renderer), not the context of the window itself.
        let gl_context = get_gl_context(&fake_display.hidden_display).unwrap();

        // NOTE: The `hidden_display` must share the OpenGL context with the `window`,
        // otherwise this will segfault! Use `ContextBuilder::with_shared_lists` to share the
        // OpenGL context across different windows.
        //
        // The context **must** be made current before calling `.bind_framebuffer()`,
        // otherwise EGL will panic with EGL_BAD_MATCH. The current context has to be the
        // hidden_display context, otherwise this will segfault on Windows.
        fake_display.hidden_display.gl_window().make_current().unwrap();

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
        gl_context.clear_depth(0.0);
        fake_display.renderer.as_mut().unwrap().render(framebuffer_size).unwrap();

        gl_context.delete_framebuffers(&framebuffers);
        gl_context.delete_renderbuffers(&depthbuffers);

        // FBOs can't be shared between windows, but textures can.
        // In order to draw on the windows backbuffer, first make the window current, then draw to FB 0
        window.display.gl_window().make_current().unwrap();
        draw_texture_to_screen(gl_context.clone(), textures[0], framebuffer_size);
        window.display.swap_buffers().unwrap();

        fake_display.hidden_display.gl_window().make_current().unwrap();

        // Only delete the texture here...
        gl_context.delete_textures(&textures);

        gl_context.bind_framebuffer(gl::FRAMEBUFFER, 0);
        gl_context.bind_texture(gl::TEXTURE_2D, 0);
        gl_context.use_program(current_program[0] as u32);

    };

    // The initial setup can lead to flickering during startup, by default
    // the window is hidden until the first frame has been rendered.
    if window.create_options.state.is_visible && window.state.is_visible {
        window.display.gl_window().window().show();
        window.state.is_visible = true;
        window.create_options.state.is_visible = false;
    }
}

/// When called with glDrawArrays(0, 3), generates a simple triangle that
/// spans the whole screen.
const DISPLAY_VERTEX_SHADER: &str = "
    #version 140
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
    #version 140
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
fn compile_screen_shader(context: Rc<Gl>) -> GLuint {
    unsafe { DISPLAY_SHADER.get_or_insert_with(|| {
        GlShader::new(context, DISPLAY_VERTEX_SHADER, DISPLAY_FRAGMENT_SHADER).unwrap()
    }) }.program_id
}

// Draws a texture to the currently bound framebuffer. Texture has to be cleaned up by the caller.
fn draw_texture_to_screen(context: Rc<Gl>, texture: GLuint, framebuffer_size: DeviceIntSize) {

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

fn set_webrender_debug_flags(r: &mut Renderer, old_flags: &DebugState, new_flags: &DebugState) {

    use webrender::DebugFlags;

    if old_flags.profiler_dbg != new_flags.profiler_dbg {
        r.set_debug_flag(DebugFlags::PROFILER_DBG, new_flags.profiler_dbg);
    }
    if old_flags.render_target_dbg != new_flags.render_target_dbg {
        r.set_debug_flag(DebugFlags::RENDER_TARGET_DBG, new_flags.render_target_dbg);
    }
    if old_flags.texture_cache_dbg != new_flags.texture_cache_dbg {
        r.set_debug_flag(DebugFlags::TEXTURE_CACHE_DBG, new_flags.texture_cache_dbg);
    }
    if old_flags.gpu_time_queries != new_flags.gpu_time_queries {
        r.set_debug_flag(DebugFlags::GPU_TIME_QUERIES, new_flags.gpu_time_queries);
    }
    if old_flags.gpu_sample_queries != new_flags.gpu_sample_queries {
        r.set_debug_flag(DebugFlags::GPU_SAMPLE_QUERIES, new_flags.gpu_sample_queries);
    }
    if old_flags.disable_batching != new_flags.disable_batching {
        r.set_debug_flag(DebugFlags::DISABLE_BATCHING, new_flags.disable_batching);
    }
    if old_flags.epochs != new_flags.epochs {
        r.set_debug_flag(DebugFlags::EPOCHS, new_flags.epochs);
    }
    if old_flags.compact_profiler != new_flags.compact_profiler {
        r.set_debug_flag(DebugFlags::COMPACT_PROFILER, new_flags.compact_profiler);
    }
    if old_flags.echo_driver_messages != new_flags.echo_driver_messages {
        r.set_debug_flag(DebugFlags::ECHO_DRIVER_MESSAGES, new_flags.echo_driver_messages);
    }
    if old_flags.new_frame_indicator != new_flags.new_frame_indicator {
        r.set_debug_flag(DebugFlags::NEW_FRAME_INDICATOR, new_flags.new_frame_indicator);
    }
    if old_flags.new_scene_indicator != new_flags.new_scene_indicator {
        r.set_debug_flag(DebugFlags::NEW_SCENE_INDICATOR, new_flags.new_scene_indicator);
    }
    if old_flags.show_overdraw != new_flags.show_overdraw {
        r.set_debug_flag(DebugFlags::SHOW_OVERDRAW, new_flags.show_overdraw);
    }
    if old_flags.gpu_cache_dbg != new_flags.gpu_cache_dbg {
        r.set_debug_flag(DebugFlags::GPU_CACHE_DBG, new_flags.gpu_cache_dbg);
    }
}

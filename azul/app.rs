use std::{
    rc::Rc,
    time::Instant,
    collections::BTreeMap,
};
use glium::glutin::WindowEvent;
use gleam::gl::{self, Gl, GLuint};
use webrender::{
    PipelineInfo, Renderer,
    api::{ DevicePixel, LayoutSize, Epoch, Transaction },
};
use log::LevelFilter;
use azul_css::{ColorU, LayoutPoint};
use {
    FastHashMap,
    window::{
        Window, ScrollStates, LogicalPosition, LogicalSize,
        RendererType, WindowSize, DebugState,
        FullWindowState,
    },
    dom::{Dom, DomId, NodeId, ScrollTagId},
    gl::GlShader,
    traits::Layout,
    ui_state::UiState,
    async::{Task, TimerId, TerminateTimer},
    callbacks::{
        LayoutCallback, FocusTarget, UpdateScreen, HitTestItem,
        Redraw, DontRedraw, ScrollPosition,
    },
};
use azul_core::{
    ui_solver::ScrolledNodes,
    window::WindowId,
    ui_description::UiDescription,
};
pub use app_resources::AppResources;
#[cfg(not(test))]
use azul_core::{
    window::FakeWindow,
};
#[cfg(not(test))]
use window::{ FakeDisplay, WindowCreateError, WindowCreateOptions };
#[cfg(not(test))]
use azul_css::{HotReloadHandler, Css};
#[cfg(not(test))]
use webrender::api::{WorldPoint, HitTestFlags};
#[cfg(test)]
use app_resources::FakeRenderApi;
pub use azul_core::app::*; // {App, AppState, AppStateNoData, RuntimeError}

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
    #[cfg(not(test))]
    pub fn add_window(&mut self, window: Window<T>) {
        use window_state::full_window_state_from_normal_state;
        let window_id = window.id;
        let fake_window = FakeWindow {
            state: window.state.clone(),
            default_callbacks: BTreeMap::new(),
            gl_context: self.fake_display.get_gl_context(),
            cached_display_list: window.internal.cached_display_list.clone(),
            scrolled_nodes: window.internal.scrolled_nodes.clone(),
            layout_result: window.internal.layout_result.clone(),
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
        std::mem::drop(self.app_state.tasks);

        Ok(self.app_state.data)
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

    #[cfg(not(test))]
    fn run_inner(&mut self) -> Result<(), RuntimeError> {

        use std::{thread, time::Duration};
        use glium::glutin::{Event, WindowId as GliumWindowId};
        use ui_state::{ui_state_from_dom, ui_state_from_app_state};
        use azul_core::app::RuntimeError::*;

        // Initialize UI state cache
        let mut ui_state_cache = initialize_ui_state_cache(&mut self.windows, &mut self.app_state, self.layout_callback)?;
        let mut ui_description_cache = initialize_ui_description_cache(&ui_state_cache);
        let mut awakened_tasks = self.windows.keys().map(|window_id| (*window_id, false)).collect();

        #[cfg(debug_assertions)]
        let mut last_style_reload = Instant::now();

        while !self.windows.is_empty() {

            let time_start = Instant::now();

            let glium_window_id_to_window_id = self.windows.iter()
                .map(|(window_id, window)| (window.display.gl_window().id(), *window_id))
                .collect::<BTreeMap<GliumWindowId, WindowId>>();

            let mut events = BTreeMap::new();

            self.fake_display.hidden_events_loop.poll_events(|e| match e {
                // Filter out all events that are uninteresting or unnecessary
                // Event::WindowEvent { event: WindowEvent::Refresh, .. } => { },
                Event::WindowEvent { window_id, event } => {
                    if let Some(wid) = glium_window_id_to_window_id.get(&window_id) {
                        events.entry(wid).or_insert_with(|| Vec::new()).push(event);
                    }
                },
                _ => { },
            });

            let mut closed_windows = Vec::<WindowId>::new();
            let mut frame_was_resize = false;
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
                self.windows.remove(&closed_window_id);
                self.window_states.remove(&closed_window_id);
            });

            #[cfg(debug_assertions)]
            let (css_has_reloaded, css_has_error) = match hot_reload_css(&mut self.windows, &mut last_style_reload, false) {
                Ok(has_reloaded) => (has_reloaded, None),
                Err(css_error) => (true, Some(css_error)),
            };

            #[cfg(not(debug_assertions))]
            let css_has_error: Option<String> = None;

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

                    // Call the Layout::layout() fn, get the DOM
                    let mut rendered_dom = match &css_has_error {
                        None => ui_state_from_app_state(&mut self.app_state, current_window_id, None, self.layout_callback)?,
                        Some(s) => {
                            println!("{}", s);
                            ui_state_from_dom(Dom::label(s.clone()).with_class("__azul_css_error"), None)
                        },
                    };

                    // Since this is the root DOM of the window, set the DomID to 0
                    rendered_dom.dom_id = DomId::ROOT_ID;

                    let mut ui_state_map = BTreeMap::new();
                    ui_state_map.insert(rendered_dom.dom_id.clone(), rendered_dom);
                    *ui_state_cache.get_mut(current_window_id).ok_or(WindowIndexError)? = ui_state_map;

                    relayout_single_window(
                        &current_window_id,
                        &mut window,
                        full_window_state,
                        &mut self.app_state,
                        &mut self.fake_display,
                        &mut ui_state_cache,
                        &mut ui_description_cache,
                        &mut awakened_tasks,
                    )?;
                }
            }

            // TODO: For some reason, the window state and the full window state get out of sync
            for (window_id, full_window_state) in &self.window_states {
                self.windows.get_mut(&window_id).ok_or(WindowIndexError)?.state =
                    ::window::full_window_state_to_window_state(full_window_state);
            }

            // If there is a re-render necessary, re-render *all* windows
            if should_rerender_all_windows || should_redraw_timers_or_tasks {
                for window in self.windows.values_mut() {
                    // TODO: For some reason this function has to be called twice in order
                    // to actually update the screen. For some reason the first swap_buffers() has
                    // no effect (winit bug?)
                    render_inner(window, &mut self.fake_display, Transaction::new(), self.config.background_color);
                    // render_inner(window, &mut self.fake_display, Transaction::new(), self.config.background_color);
                }
                clean_up_unused_opengl_textures(self.fake_display.renderer.as_mut().unwrap().flush_pipeline_info());
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
}

fn initialize_ui_state_cache<T>(
    windows: &mut BTreeMap<WindowId, Window<T>>,
    app_state: &mut AppState<T>,
    layout_callback: LayoutCallback<T>,
) -> Result<BTreeMap<WindowId, BTreeMap<DomId, UiState<T>>>, RuntimeError> {

    use ui_state::{ui_state_from_app_state, ui_state_from_dom};

    let mut ui_state_map = BTreeMap::new();
    let window_ids = windows.keys().cloned().collect::<Vec<_>>();

    for window_id in window_ids {

        #[cfg(debug_assertions)]
        let mut ui_state = {
            let (_, css_has_error) = match hot_reload_css(windows, &mut Instant::now(), true) {
                Ok(has_reloaded) => (has_reloaded, None),
                Err(css_error) => (true, Some(css_error)),
            };

            match &css_has_error {
                None => ui_state_from_app_state(app_state, &window_id, None, layout_callback),
                Some(s) => {
                    println!("{}", s);
                    Ok(ui_state_from_dom(Dom::label(s.clone()).with_class("__azul_css_error"), None))
                },
            }
        }?;

        #[cfg(not(debug_assertions))]
        let mut ui_state = ui_state_from_app_state(app_state, &window_id, None, layout_callback)?;

        ui_state.dom_id = DomId::ROOT_ID;

        let mut dom_id_map = BTreeMap::new();
        dom_id_map.insert(ui_state.dom_id.clone(), ui_state);
        ui_state_map.insert(window_id, dom_id_map);
    }

    DomId::reset();

    Ok(ui_state_map)
}

fn initialize_ui_description_cache<T>(
    ui_states: &BTreeMap<WindowId, BTreeMap<DomId, UiState<T>>>
) -> BTreeMap<WindowId, BTreeMap<DomId, UiDescription<T>>> {
    ui_states.iter().map(|(window_id, ui_states)| {
        (*window_id, ui_states.iter().map(|(dom_id, _)| (dom_id.clone(), UiDescription::default())).collect())
    }).collect()
}

/// Run all currently registered timers
#[must_use]
fn app_state_run_all_timers<T>(app_state: &mut AppState<T>) -> UpdateScreen {

    use azul_core::callbacks::TimerCallbackInfo;

    let mut should_update_screen = DontRedraw;
    let mut timers_to_terminate = Vec::new();

    for (key, timer) in app_state.timers.iter_mut() {
        let (should_update, should_terminate) = timer.invoke(TimerCallbackInfo {
            state: &mut app_state.data,
            app_resources: &mut app_state.resources,
        });

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
    ui_state_cache: &mut BTreeMap<WindowId, BTreeMap<DomId, UiState<T>>>,
    awakened_tasks: &mut BTreeMap<WindowId, bool>,
) -> Result<SingleWindowContentResult, RuntimeError> {

    use azul_core::app::RuntimeError::*;
    use window;

    let (mut frame_event_info, window_should_close) = window::update_window_state(full_window_state, &events);
    let mut ret = SingleWindowContentResult {
        needs_rerender_hover_active: false,
        needs_relayout_hover_active: false,
        needs_relayout_resize: frame_event_info.is_resize_event,
        window_should_close,
        should_scroll_render: false,
        needs_relayout_tasks: *(awakened_tasks.get(window_id).ok_or(WindowIndexError)?),
        needs_relayout_refresh: false,
        callbacks_update_screen: DontRedraw,
        hit_test_results: None,
        new_focus_target: None,
    };

    if events.is_empty() && !ret.should_relayout() && !ret.should_rerender() {
        // Event was not a resize event, window should **not** close
        ret.window_should_close = window_should_close;
        return Ok(ret);
    }

    let scroll_states = window.internal.get_current_scroll_states(&ui_state_cache[window_id]);
    let mut scrolled_nodes = BTreeMap::new();

    if frame_event_info.should_hittest {

        ret.hit_test_results = do_hit_test(&window, full_window_state, fake_display);

        for event in events.iter() {

            app_state.windows.get_mut(window_id).unwrap().state =
                window::full_window_state_to_window_state(full_window_state);

            let callback_result = call_callbacks(
                event,
                &window_id,
                ret.hit_test_results.as_ref(),
                full_window_state,
                &scroll_states,
                &mut scrolled_nodes,
                ui_state_cache.get_mut(window_id).ok_or(WindowIndexError)?,
                app_state,
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

    // Scroll nodes from input (mouse scroll) events
    let mut should_scroll_render_from_input_events = false;

    if let Some(hit_test_results) = &ret.hit_test_results {
        for (_dom_id, scrolled_nodes) in &window.internal.scrolled_nodes {
            if update_scroll_state(full_window_state, scrolled_nodes, &mut window.internal.scroll_states, hit_test_results) {
                should_scroll_render_from_input_events = true;
            }
        }
    }

    let mut should_scroll_render_from_callbacks = false;

    // Scroll nodes that were scrolled via the callbacks
    for (dom_id, callback_scrolled_nodes) in scrolled_nodes {
        if let Some(scrolled_nodes) = window.internal.scrolled_nodes.get(&dom_id) {
            for (scroll_node_id, scroll_position) in &callback_scrolled_nodes {
                if let Some(overflowing_node) = scrolled_nodes.overflowing_nodes.get(&scroll_node_id) {
                    window.internal.scroll_states.set_scroll_position(&overflowing_node, *scroll_position);
                    should_scroll_render_from_callbacks = true;
                }
            }
        }
    }

    let should_scroll_render = should_scroll_render_from_input_events || should_scroll_render_from_callbacks;

    ret.should_scroll_render = should_scroll_render;

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

    app_state.windows.get_mut(window_id).unwrap().state = window::full_window_state_to_window_state(full_window_state);

    // Reset the scroll amount to 0 (for the next frame)
    window::clear_scroll_state(full_window_state);

    Ok(ret)
}

#[cfg(not(test))]
fn relayout_single_window<T>(
    window_id: &WindowId,
    window: &mut Window<T>,
    full_window_state: &mut FullWindowState,
    app_state: &mut AppState<T>,
    fake_display: &mut FakeDisplay,
    ui_state_cache: &mut BTreeMap<WindowId, BTreeMap<DomId, UiState<T>>>,
    ui_description_cache: &mut BTreeMap<WindowId, BTreeMap<DomId, UiDescription<T>>>,
    awakened_tasks: &mut BTreeMap<WindowId, bool>,
) -> Result<(), RuntimeError> {

    use azul_core::app::RuntimeError::*;

    // Style the DOM (is_mouse_down is necessary for styling :hover, :active + :focus nodes)
    let is_mouse_down = full_window_state.mouse_state.mouse_down();

    {
        let ui_state_mut = ui_state_cache.get_mut(window_id).ok_or(WindowIndexError)?;
        let ui_description_mut = ui_description_cache.get_mut(window_id).ok_or(WindowIndexError)?;
        *ui_description_mut = ui_state_mut.iter_mut().map(|(dom_id, ui_state)| {
            let hovered_nodes = full_window_state.hovered_nodes.get(&dom_id).cloned().unwrap_or_default();
            (dom_id.clone(), UiDescription::match_css_to_dom(
                ui_state,
                &window.css,
                &mut full_window_state.focused_node,
                &mut full_window_state.pending_focus_target,
                &hovered_nodes,
                is_mouse_down,
            ))
        }).collect();
    }

    let mut fake_window = app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?;

    for (window_id, ui_state_map) in ui_state_cache.iter() {
        for (dom_id, ui_state) in ui_state_map.iter() {
            let ui_description = &ui_description_cache[window_id][dom_id];
            update_display_list(
                &mut app_state.data,
                ui_description,
                ui_state,
                &mut *window,
                &mut fake_window,
                fake_display,
                &mut app_state.resources,
            );
        }
    }

    *awakened_tasks.get_mut(window_id).ok_or(WindowIndexError)? = false;

    Ok(())
}

/// Returns if the CSS has been successfully reloaded
#[cfg(debug_assertions)]
fn hot_reload_css<T>(
    windows: &mut BTreeMap<WindowId, Window<T>>,
    last_style_reload: &mut Instant,
    force_reload: bool,
) -> Result<bool, String> {

    let mut has_reloaded = false;

    for window in windows.values_mut() {

        // Hot-reload a style if necessary
        let hot_reloader = match window.css_loader.as_mut() {
            None => continue,
            Some(s) => s,
        };

        if !force_reload {
            let should_reload = Instant::now() - *last_style_reload > hot_reloader.get_reload_interval();

            if !should_reload {
                continue;
            }
        }

        match hot_reloader.reload_style() {
            Ok(mut new_css) => {
                new_css.sort_by_specificity();
                window.css = new_css;
                *last_style_reload = Instant::now();
                has_reloaded = true;
            },
            Err(why) => {
                return Err(format!("{}", why));
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

/// Returns an bool whether the window should be redrawn or not (true - redraw the screen, false: don't redraw).
fn call_callbacks<T>(
    event: &WindowEvent,
    window_id: &WindowId,
    hit_test_results: Option<&Vec<HitTestItem>>,
    full_window_state: &mut FullWindowState,
    scroll_states: &BTreeMap<DomId, BTreeMap<NodeId, ScrollPosition>>,
    scrolled_nodes: &mut BTreeMap<DomId, BTreeMap<NodeId, LayoutPoint>>,
    ui_state_map: &BTreeMap<DomId, UiState<T>>,
    app_state: &mut AppState<T>
) -> Result<CallCallbackReturn, RuntimeError> {

    use {
        callbacks::{CallbackInfo, DefaultCallbackInfoUnchecked},
        window_state::determine_callbacks,
    };

    let hit_test_items = hit_test_results.map(|items| items.clone()).unwrap_or_default();
    let callbacks_filter_list = ui_state_map.iter().map(|(dom_id, ui_state)| {
        (dom_id.clone(), determine_callbacks(full_window_state, &hit_test_items, event, ui_state))
    }).collect::<BTreeMap<_, _>>();

    let mut should_update_screen = DontRedraw;
    let mut callbacks_overwrites_focus = None;
    let mut default_timers = FastHashMap::default();
    let mut default_tasks = Vec::new();

    // Run all default callbacks - **before** the user-defined callbacks are run!
    for dom_id in ui_state_map.keys().cloned() {
        for (node_id, callback_results) in callbacks_filter_list[&dom_id].nodes_with_callbacks.iter() {
            let hit_item = &callback_results.hit_test_item;
            for default_callback_id in callback_results.default_callbacks.values() {

                let mut new_focus = None;
                let mut timers = FastHashMap::default();
                let mut tasks = Vec::new();

                if app_state.windows[window_id].default_callbacks.get(default_callback_id).cloned().and_then(|(callback_ptr, callback_fn)| {
                    let info = DefaultCallbackInfoUnchecked {
                        ptr: callback_ptr,
                        state: AppStateNoData {
                            windows: &app_state.windows,
                            resources: &mut app_state.resources,
                            timers: &mut timers,
                            tasks: &mut tasks,
                        },
                        focus_target: &mut new_focus,
                        current_scroll_states: scroll_states,
                        scrolled_nodes,
                        window_id,
                        hit_dom_node: (dom_id.clone(), *node_id),
                        ui_state: ui_state_map,
                        hit_test_items: &hit_test_items,
                        cursor_relative_to_item: hit_item.as_ref().map(|hi| (hi.point_relative_to_item.x, hi.point_relative_to_item.y)),
                        cursor_in_viewport: hit_item.as_ref().map(|hi| (hi.point_in_viewport.x, hi.point_in_viewport.y)),
                    };
                    (callback_fn.0)(info)
                }) == Redraw {
                    should_update_screen = Redraw;
                }

                default_timers.extend(timers.into_iter());
                default_tasks.extend(tasks.into_iter());

                // Overwrite the focus from the callback info
                if let Some(new_focus) = new_focus {
                    callbacks_overwrites_focus = Some(new_focus);
                }
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

    for dom_id in ui_state_map.keys().cloned() {
        for (node_id, callback_results) in callbacks_filter_list[&dom_id].nodes_with_callbacks.iter() {
            let hit_item = &callback_results.hit_test_item;
            for callback in callback_results.normal_callbacks.values() {

                let mut new_focus = None;

                if (callback.0)(CallbackInfo {
                    state: app_state,
                    focus_target: &mut new_focus,
                    current_scroll_states: &scroll_states,
                    scrolled_nodes,
                    window_id,
                    hit_dom_node: (dom_id.clone(), *node_id),
                    ui_state: ui_state_map,
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

    Ok(CallCallbackReturn {
        should_update_screen,
        callbacks_overwrites_focus,
        needs_redraw_anyways: callbacks_filter_list.values().any(|v| v.needs_redraw_anyways),
        needs_relayout_anyways: callbacks_filter_list.values().any(|v| v.needs_relayout_anyways),
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
        CachedDisplayListResult,
    };
    use app_resources::add_resources;
    use wr_translate::{
        wr_translate_pipeline_id,
        wr_translate_display_list,
    };
    use glium::glutin::ContextTrait;

    let display_list = display_list_from_ui_description(ui_description, ui_state);

    // Make sure unused scroll states are garbage collected.
    window.internal.scroll_states.remove_unused_scroll_states();

    unsafe { window.display.gl_window().make_current().unwrap() };

    DomId::reset();

    // NOTE: layout_result contains all words, text information, etc.
    // - very important for selection!
    let CachedDisplayListResult {
        cached_display_list,
        scrollable_nodes,
        image_resource_updates,
        layout_result
    } = display_list_to_cached_display_list(
        display_list,
        app_data,
        window,
        fake_window,
        app_resources,
        &mut fake_display.render_api,
    );

    unsafe { fake_display.hidden_display.gl_window().make_current().unwrap() };

    for (_dom_id, image_resource_updates) in image_resource_updates {
        add_resources(app_resources, &mut fake_display.render_api, Vec::new(), image_resource_updates);
    }

    window.internal.layout_result = layout_result;
    window.internal.scrolled_nodes = scrollable_nodes;
    window.internal.cached_display_list = cached_display_list.clone();

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    let display_list = wr_translate_display_list(cached_display_list, window.internal.pipeline_id);

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
// This leads to problems when invoking GlCallbacks, because those don't expect
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
    scroll_all_nodes(&mut window.internal.scroll_states, &mut txn);
    txn.generate_frame();

    fake_display.render_api.send_transaction(window.internal.document_id, txn);

    // Update WR texture cache
    fake_display.renderer.as_mut().unwrap().update();

    let background_color_f: ColorF = background_color.into();

    unsafe {

        // NOTE: GlContext is the context of the app-global, hidden window
        // (that shares the renderer), not the context of the window itself.
        let gl_context = fake_display.get_gl_context();

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
        gl_context.clear(gl::COLOR_BUFFER_BIT);
        gl_context.clear_depth(0.0);
        gl_context.clear(gl::DEPTH_BUFFER_BIT);
        fake_display.renderer.as_mut().unwrap().render(framebuffer_size).unwrap();

        // FBOs can't be shared between windows, but textures can.
        // In order to draw on the windows backbuffer, first make the window current, then draw to FB 0
        window.display.gl_window().make_current().unwrap();
        draw_texture_to_screen(gl_context.clone(), textures[0], framebuffer_size);
        window.display.swap_buffers().unwrap();

        fake_display.hidden_display.gl_window().make_current().unwrap();

        // Only delete the texture here...
        gl_context.delete_framebuffers(&framebuffers);
        gl_context.delete_renderbuffers(&depthbuffers);
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

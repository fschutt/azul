use azul_css::{FontId, PixelValue, StyleLetterSpacing};
use glium::{
    glutin::{
        dpi::{LogicalPosition, LogicalSize},
        Event,
    },
    SwapBuffersError,
};
#[cfg(feature = "image_loading")]
use image::ImageError;
#[cfg(feature = "image_loading")]
use images::ImageType;
#[cfg(feature = "logging")]
use log::LevelFilter;
use std::{
    fmt,
    io::Read,
    mem,
    sync::{Arc, Mutex, PoisonError},
    time::Instant,
};
use webrender::{
    api::{
        DevicePixel, Epoch, HitTestFlags, HitTestResult, LayoutPoint, LayoutSize, Transaction,
        WorldPoint,
    },
    PipelineInfo,
};
use {
    app_resources::AppResources,
    app_state::AppState,
    daemon::Daemon,
    dom::{ScrollTagId, UpdateScreen},
    error::{ClipboardError, FontError},
    text_cache::TextId,
    traits::Layout,
    ui_description::UiDescription,
    ui_state::UiState,
    window::{FakeWindow, ScrollStates, Window, WindowId},
    window_state::WindowSize,
};

type DeviceUintSize = ::euclid::TypedSize2D<u32, DevicePixel>;
type DeviceIntSize = ::euclid::TypedSize2D<i32, DevicePixel>;

/// Graphical application that maintains some kind of application state
pub struct App<T: Layout> {
    /// The graphical windows, indexed by ID
    windows: Vec<Window<T>>,
    /// The global application state
    pub app_state: AppState<T>,
}

/// Error returned by the `.run()` function
///
/// If the `.run()` function would panic, that would need `T` to
/// implement `Debug`, which is not necessary if we just return an error.
pub enum RuntimeError<T: Layout> {
    // Could not swap the display (drawing error)
    GlSwapError(SwapBuffersError),
    ArcUnlockError,
    MutexPoisonError(PoisonError<T>),
}

impl<T: Layout> From<PoisonError<T>> for RuntimeError<T> {
    fn from(e: PoisonError<T>) -> Self {
        RuntimeError::MutexPoisonError(e)
    }
}

impl<T: Layout> From<SwapBuffersError> for RuntimeError<T> {
    fn from(e: SwapBuffersError) -> Self {
        RuntimeError::GlSwapError(e)
    }
}

impl<T: Layout> fmt::Debug for RuntimeError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::RuntimeError::*;
        match self {
            GlSwapError(e) => write!(f, "RuntimeError::GlSwapError({:?})", e),
            ArcUnlockError => write!(f, "RuntimeError::ArcUnlockError"),
            MutexPoisonError(e) => write!(f, "RuntimeError::MutexPoisonError({:?})", e),
        }
    }
}

pub(crate) struct FrameEventInfo {
    pub(crate) should_redraw_window: bool,
    pub(crate) should_swap_window: bool,
    pub(crate) should_hittest: bool,
    pub(crate) cur_cursor_pos: LogicalPosition,
    pub(crate) new_window_size: Option<LogicalSize>,
    pub(crate) new_dpi_factor: Option<f64>,
    pub(crate) is_resize_event: bool,
}

impl Default for FrameEventInfo {
    fn default() -> Self {
        Self {
            should_redraw_window: false,
            should_swap_window: false,
            should_hittest: false,
            cur_cursor_pos: LogicalPosition::new(0.0, 0.0),
            new_window_size: None,
            new_dpi_factor: None,
            is_resize_event: false,
        }
    }
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
        }
    }
}

impl<T: Layout> App<T> {
    #[allow(unused_variables)]
    /// Create a new, empty application. This does not open any windows.
    pub fn new(initial_data: T, config: AppConfig) -> Self {
        #[cfg(feature = "logging")]
        {
            if let Some(log_level) = config.enable_logging {
                ::logging::set_up_logging(config.log_file_path, log_level);

                if config.enable_logging_on_panic {
                    ::logging::set_up_panic_hooks();
                }

                if config.enable_visual_panic_hook {
                    use std::sync::atomic::Ordering;
                    ::logging::SHOULD_ENABLE_PANIC_HOOK.store(true, Ordering::SeqCst);
                }
            }
        }

        Self {
            windows: Vec::new(),
            app_state: AppState::new(initial_data),
        }
    }

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn push_window(&mut self, window: Window<T>) {
        use default_callbacks::DefaultCallbackSystem;

        // TODO: push_window doesn't work dynamically!

        self.app_state.windows.push(FakeWindow {
            state: window.state.clone(),
            default_callbacks: DefaultCallbackSystem::new(),
            read_only_window: window.display.clone(),
        });

        self.windows.push(window);
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
    pub fn run(mut self, window: Window<T>) -> Result<T, RuntimeError<T>> {
        // Apps need to have at least one window open
        self.push_window(window);
        self.run_inner()?;

        // NOTE: This is necessary because otherwise, the Arc::try_unwrap would fail,
        // since one Arc is still owned by the app_state.tasks structure
        //
        // See https://github.com/maps4print/azul/issues/24#issuecomment-429737273
        mem::drop(self.app_state.tasks);

        let unique_arc =
            Arc::try_unwrap(self.app_state.data).map_err(|_| RuntimeError::ArcUnlockError)?;
        unique_arc.into_inner().map_err(|e| e.into())
    }

    fn run_inner(&mut self) -> Result<(), RuntimeError<T>> {
        use std::{
            thread,
            time::{Duration, Instant},
        };

        let mut ui_state_cache = Self::initialize_ui_state(&self.windows, &mut self.app_state);
        let mut ui_description_cache = vec![UiDescription::default(); self.windows.len()];
        let mut force_redraw_cache = vec![1_usize; self.windows.len()];
        let mut awakened_task = vec![false; self.windows.len()];

        #[cfg(debug_assertions)]
        let mut last_style_reload = Instant::now();
        #[cfg(debug_assertions)]
        let mut should_print_css_error = true;

        while !self.windows.is_empty() {
            let time_start = Instant::now();
            let mut closed_windows = Vec::<usize>::new();

            let mut frame_was_resize = false;

            'window_loop: for (idx, window) in self.windows.iter_mut().enumerate() {
                let window_id = WindowId { id: idx };
                let mut frame_event_info = FrameEventInfo::default();

                let mut events = Vec::new();
                window.events_loop.poll_events(|e| events.push(e));
                if events.is_empty() {
                    continue 'window_loop;
                }

                for event in &events {
                    if preprocess_event(event, &mut frame_event_info, awakened_task[idx])
                        == WindowCloseEvent::AboutToClose
                    {
                        closed_windows.push(idx);
                        continue 'window_loop;
                    }
                    window.state.update_mouse_cursor_position(event);
                    window.state.update_scroll_state(event);
                    window.state.update_keyboard_modifiers(event);
                    window.state.update_keyboard_pressed_chars(event);
                    window.state.update_misc_events(event);
                }

                let mut hit_test_results = None;

                if frame_event_info.should_hittest {
                    for event in &events {
                        hit_test_results = do_hit_test(&window);
                        call_callbacks(
                            hit_test_results.as_ref(),
                            event,
                            window,
                            window_id,
                            &mut frame_event_info,
                            &ui_state_cache,
                            &mut self.app_state,
                        );
                    }
                }

                // Scroll for the scrolled amount for each node that registered a scroll state.
                render_on_scroll(window, hit_test_results, &frame_event_info);

                if frame_event_info.should_swap_window
                    || frame_event_info.is_resize_event
                    || force_redraw_cache[idx] > 0
                {
                    window.display.swap_buffers()?;
                    if let Some(i) = force_redraw_cache.get_mut(idx) {
                        if *i > 0 {
                            *i -= 1
                        };
                        if *i == 0 {
                            clean_up_unused_opengl_textures(
                                window.renderer.as_mut().unwrap().flush_pipeline_info(),
                            );
                        }
                    }
                }

                if frame_event_info.is_resize_event || frame_event_info.should_redraw_window {
                    // This is a hack because during a resize event, winit eats the "awakened"
                    // event. So what we do is that we call the layout-and-render again, to
                    // trigger a second "awakened" event. So when the window is resized, the
                    // layout function is called twice (the first event will be eaten by winit)
                    //
                    // This is a reported bug and should be fixed somewhere in July
                    force_redraw_cache[idx] = 2;
                    frame_was_resize = true;
                }

                // Update the window state that we got from the frame event (updates window dimensions and DPI)
                window.update_from_external_window_state(&mut frame_event_info);
                // Update the window state every frame that was set by the user
                window.update_from_user_window_state(self.app_state.windows[idx].state.clone());
                // Reset the scroll amount to 0 (for the next frame)
                window.clear_scroll_state();

                if frame_event_info.should_redraw_window || force_redraw_cache[idx] > 0 {
                    // Call the Layout::layout() fn, get the DOM
                    let window_id = WindowId { id: idx };
                    ui_state_cache[idx] = UiState::from_app_state(&mut self.app_state, window_id);

                    // Style the DOM (is_mouse_down, etc. necessary for styling :hover, :active + :focus nodes)
                    let is_mouse_down = window.state.mouse_state.left_down
                        || window.state.mouse_state.middle_down
                        || window.state.mouse_state.right_down;

                    ui_description_cache[idx] = UiDescription::match_css_to_dom(
                        &mut ui_state_cache[idx],
                        &window.css,
                        window.state.focused_node,
                        &window.state.hovered_nodes,
                        is_mouse_down,
                    );

                    // render the window (webrender will send an Awakened event when the frame is done)
                    let arc_mutex_t_clone = self.app_state.data.clone();

                    render(
                        arc_mutex_t_clone,
                        &ui_description_cache[idx],
                        &ui_state_cache[idx],
                        &mut *window,
                        &mut self.app_state.windows[idx],
                        &mut self.app_state.resources,
                    );

                    awakened_task[idx] = false;
                }
            }

            #[cfg(debug_assertions)]
            {
                hot_reload_css(
                    &mut self.windows,
                    &mut last_style_reload,
                    &mut should_print_css_error,
                    &mut awakened_task,
                )
            }

            // Close windows if necessary
            closed_windows.into_iter().for_each(|closed_window_id| {
                ui_state_cache.remove(closed_window_id);
                ui_description_cache.remove(closed_window_id);
                force_redraw_cache.remove(closed_window_id);
                self.windows.remove(closed_window_id);
            });

            let should_redraw_daemons = self.app_state.run_all_daemons();
            let should_redraw_tasks = self.app_state.clean_up_finished_tasks();

            if [should_redraw_daemons, should_redraw_tasks]
                .into_iter()
                .any(|e| *e == UpdateScreen::Redraw)
            {
                self.windows
                    .iter()
                    .for_each(|w| w.events_loop.create_proxy().wakeup().unwrap_or(()));
                awakened_task = vec![true; self.windows.len()];
            } else if !frame_was_resize {
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

    fn initialize_ui_state(windows: &[Window<T>], app_state: &mut AppState<T>) -> Vec<UiState<T>> {
        windows
            .iter()
            .enumerate()
            .map(|(idx, _window)| {
                let window_id = WindowId { id: idx };
                UiState::from_app_state(app_state, window_id)
            })
            .collect()
    }

    /// Add an image to the internal resources. Only available with
    /// `--feature="image_loading"` (on by default)
    ///
    /// ## Returns
    ///
    /// - `Ok(Some(()))` if an image with the same ID already exists.
    /// - `Ok(None)` if the image was added, but didn't exist previously.
    /// - `Err(e)` if the image couldn't be decoded
    #[cfg(feature = "image_loading")]
    pub fn add_image<S: Into<String>, R: Read>(
        &mut self,
        id: S,
        data: &mut R,
        image_type: ImageType,
    ) -> Result<Option<()>, ImageError> {
        self.app_state.add_image(id, data, image_type)
    }

    /// Removes an image from the internal app resources.
    /// Returns `Some` if the image existed and was removed.
    /// If the given ID doesn't exist, this function does nothing and returns `None`.
    pub fn delete_image<S: AsRef<str>>(&mut self, id: S) -> Option<()> {
        self.app_state.delete_image(id)
    }

    /// Checks if an image is currently registered and ready-to-use
    pub fn has_image<S: AsRef<str>>(&mut self, id: S) -> bool {
        self.app_state.has_image(id)
    }

    /// Add a font (TTF or OTF) as a resource, identified by ID
    ///
    /// ## Returns
    ///
    /// - `Ok(Some(()))` if an font with the same ID already exists.
    /// - `Ok(None)` if the font was added, but didn't exist previously.
    /// - `Err(e)` if the font couldn't be decoded
    pub fn add_font<R: Read>(&mut self, id: FontId, data: &mut R) -> Result<Option<()>, FontError> {
        self.app_state.add_font(id, data)
    }

    /// Checks if a font is currently registered and ready-to-use
    pub fn has_font(&mut self, id: &FontId) -> bool {
        self.app_state.has_font(id)
    }

    /// Deletes a font from the internal app resources.
    ///
    /// ## Arguments
    ///
    /// - `id`: The stringified ID of the font to remove, e.g. `"Helvetica-Bold"`.
    ///
    /// ## Returns
    ///
    /// - `Some(())` if if the image existed and was successfully removed
    /// - `None` if the given ID doesn't exist. In that case, the function does
    ///    nothing.
    ///
    /// Wrapper function for [`AppState::delete_font`]. After this function has been
    /// called, you can be sure that the renderer doesn't know about your font anymore.
    /// This also means that the font needs to be re-parsed if you want to add it again.
    /// Use with care.
    ///
    /// [`AppState::delete_font`]: ../app_state/struct.AppState.html#method.delete_font
    pub fn delete_font(&mut self, id: &FontId) -> Option<()> {
        self.app_state.delete_font(id)
    }

    /// Create a daemon. Does nothing if a daemon with the function pointer location already exists.
    ///
    /// If the daemon was inserted, returns true, otherwise false
    pub fn add_daemon(&mut self, daemon: Daemon<T>) -> bool {
        self.app_state.add_daemon(daemon)
    }

    pub fn add_text_uncached<S: Into<String>>(&mut self, text: S) -> TextId {
        self.app_state.add_text_uncached(text)
    }

    pub fn add_text_cached<S: Into<String>>(
        &mut self,
        text: S,
        font_id: &FontId,
        font_size: PixelValue,
        letter_spacing: Option<StyleLetterSpacing>,
    ) -> TextId {
        self.app_state
            .add_text_cached(text, font_id, font_size, letter_spacing)
    }

    pub fn delete_text(&mut self, id: TextId) {
        self.app_state.delete_text(id);
    }

    pub fn clear_all_texts(&mut self) {
        self.app_state.clear_all_texts();
    }

    /// Get the contents of the system clipboard as a string
    pub fn get_clipboard_string(&mut self) -> Result<String, ClipboardError> {
        self.app_state.get_clipboard_string()
    }

    /// Set the contents of the system clipboard as a string
    pub fn set_clipboard_string(&mut self, contents: String) -> Result<(), ClipboardError> {
        self.app_state.set_clipboard_string(contents)
    }

    /// See `AppState::add_custom_task`.
    pub fn add_custom_task<U: Send + 'static>(
        &mut self,
        data: &Arc<Mutex<U>>,
        callback: fn(Arc<Mutex<U>>, Arc<()>),
        after_completion_deamons: &[Daemon<T>],
    ) {
        self.app_state
            .add_custom_task(data, callback, after_completion_deamons);
    }
}

impl<T: Layout + Send + 'static> App<T> {
    /// See `AppState::add_ask`.
    pub fn add_task(
        &mut self,
        callback: fn(Arc<Mutex<T>>, Arc<()>),
        after_completion_callbacks: &[Daemon<T>],
    ) {
        self.app_state
            .add_task(callback, after_completion_callbacks);
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum WindowCloseEvent {
    AboutToClose,
    NoCloseEvent,
}

/// Returns if there was an error with the CSS reloading, necessary so that the error message is only printed once
#[cfg(debug_assertions)]
fn hot_reload_css<T: Layout>(
    windows: &mut [Window<T>],
    last_style_reload: &mut Instant,
    should_print_error: &mut bool,
    awakened_tasks: &mut [bool],
) {
    for (window_idx, window) in windows.iter_mut().enumerate() {
        // Hot-reload a style if necessary
        let hot_reloader = match window.css_loader.as_mut() {
            None => continue,
            Some(s) => s,
        };

        let should_reload =
            Instant::now() - *last_style_reload > hot_reloader.get_reload_interval();

        if !should_reload {
            return;
        }

        match hot_reloader.reload_style() {
            Ok(mut new_css) => {
                new_css.sort_by_specificity();
                window.css = new_css;
                if !(*should_print_error) {
                    println!("CSS parsed without errors, continuing hot-reloading.");
                }
                *last_style_reload = Instant::now();
                window.events_loop.create_proxy().wakeup().unwrap_or(());
                awakened_tasks[window_idx] = true;
                *should_print_error = true;
            }
            Err(why) => {
                if *should_print_error {
                    println!("{}", why);
                }
                *should_print_error = false;
            }
        };
    }
}

/// Pre-filters any events that are not handled by the framework yet, since it would be wasteful
/// to process them. Modifies the `frame_event_info`
///
/// `awakened_task` is a special field that should be set to true if the `Task`
/// system fired a `WindowEvent::Awakened`.
fn preprocess_event(
    event: &Event,
    frame_event_info: &mut FrameEventInfo,
    awakened_task: bool,
) -> WindowCloseEvent {
    use glium::glutin::WindowEvent;

    match event {
        Event::WindowEvent { event, .. } => match event {
            WindowEvent::CursorMoved { position, .. } => {
                frame_event_info.should_hittest = true;
                frame_event_info.cur_cursor_pos = *position;
            }
            WindowEvent::Resized(wh) => {
                frame_event_info.new_window_size = Some(*wh);
                frame_event_info.is_resize_event = true;
                frame_event_info.should_redraw_window = true;
            }
            WindowEvent::Refresh => {
                frame_event_info.should_redraw_window = true;
            }
            WindowEvent::HiDpiFactorChanged(dpi) => {
                frame_event_info.new_dpi_factor = Some(*dpi);
                frame_event_info.should_redraw_window = true;
            }
            WindowEvent::CloseRequested => {
                return WindowCloseEvent::AboutToClose;
            }
            WindowEvent::Destroyed => {
                return WindowCloseEvent::AboutToClose;
            }
            WindowEvent::KeyboardInput { .. }
            | WindowEvent::ReceivedCharacter(_)
            | WindowEvent::MouseWheel { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::Touch(_) => {
                frame_event_info.should_hittest = true;
            }
            _ => {}
        },
        Event::Awakened => {
            frame_event_info.should_swap_window = true;
            if awakened_task {
                frame_event_info.should_redraw_window = true;
            }
        }
        _ => {}
    }

    WindowCloseEvent::NoCloseEvent
}

/// Returns the currently hit-tested results, in back-to-front order
fn do_hit_test<T: Layout>(window: &Window<T>) -> Option<HitTestResult> {
    let cursor_location = window
        .state
        .mouse_state
        .cursor_pos
        .and_then(|pos| Some(WorldPoint::new(pos.x as f32, pos.y as f32)))?;

    let mut hit_test_results = window.internal.api.hit_test(
        window.internal.document_id,
        Some(window.internal.pipeline_id),
        cursor_location,
        HitTestFlags::FIND_ALL,
    );

    if hit_test_results.items.is_empty() {
        return None;
    }

    // Execute callbacks back-to-front, not front-to-back
    hit_test_results.items.reverse();

    Some(hit_test_results)
}

fn call_callbacks<T: Layout>(
    hit_test_results: Option<&HitTestResult>,
    event: &Event,
    window: &mut Window<T>,
    window_id: WindowId,
    info: &mut FrameEventInfo,
    ui_state_cache: &[UiState<T>],
    app_state: &mut AppState<T>,
) {
    use app_state::AppStateNoData;
    use dom::UpdateScreen;
    use window::WindowEvent;
    use window_state::{KeyboardState, MouseState};

    let hit_test_results = match hit_test_results {
        None => return,
        Some(s) => s,
    };

    let mut should_update_screen = UpdateScreen::DontRedraw;

    let callbacks_filter_list =
        window
            .state
            .determine_callbacks(&hit_test_results, event, &ui_state_cache[window_id.id]);

    // TODO: this should be refactored - currently very stateful and error-prone!
    app_state.windows[window_id.id].set_keyboard_state(&window.state.keyboard_state);
    app_state.windows[window_id.id].set_mouse_state(&window.state.mouse_state);

    // Run all default callbacks - **before** the user-defined callbacks are run!
    {
        let mut lock = app_state.data.lock().unwrap();
        for (node_id, callback_results) in callbacks_filter_list.iter() {
            let hit_item = &callback_results.hit_test_item;
            for default_callback_id in callback_results.default_callbacks.values() {
                let window_event = WindowEvent {
                    window: window_id.id,
                    hit_dom_node: *node_id,
                    ui_state: &ui_state_cache[window_id.id],
                    hit_test_result: &hit_test_results,
                    cursor_relative_to_item: (
                        hit_item.point_relative_to_item.x,
                        hit_item.point_relative_to_item.y,
                    ),
                    cursor_in_viewport: (
                        hit_item.point_in_viewport.x,
                        hit_item.point_in_viewport.y,
                    ),
                };

                let app_state_no_data = AppStateNoData {
                    windows: &app_state.windows,
                    resources: &mut app_state.resources,
                };

                // safe unwrap, we have added the callback previously
                if app_state.windows[window_id.id]
                    .default_callbacks
                    .run_callback(
                        &mut *lock,
                        default_callback_id,
                        app_state_no_data,
                        window_event,
                    )
                    == UpdateScreen::Redraw
                {
                    should_update_screen = UpdateScreen::Redraw;
                }
            }
        }
    } // release mutex

    for (node_id, callback_results) in callbacks_filter_list.iter() {
        let hit_item = &callback_results.hit_test_item;
        for callback in callback_results.normal_callbacks.values() {
            let window_event = WindowEvent {
                window: window_id.id,
                hit_dom_node: *node_id,
                ui_state: &ui_state_cache[window_id.id],
                hit_test_result: &hit_test_results,
                cursor_relative_to_item: (
                    hit_item.point_relative_to_item.x,
                    hit_item.point_relative_to_item.y,
                ),
                cursor_in_viewport: (hit_item.point_in_viewport.x, hit_item.point_in_viewport.y),
            };

            if (callback.0)(app_state, window_event) == UpdateScreen::Redraw {
                should_update_screen = UpdateScreen::Redraw;
            }
        }
    }

    app_state.windows[window_id.id].set_keyboard_state(&KeyboardState::default());
    app_state.windows[window_id.id].set_mouse_state(&MouseState::default());

    if should_update_screen == UpdateScreen::Redraw {
        info.should_redraw_window = true;
    }
}

fn render<T: Layout>(
    app_data: Arc<Mutex<T>>,
    ui_description: &UiDescription<T>,
    ui_state: &UiState<T>,
    window: &mut Window<T>,
    fake_window: &mut FakeWindow<T>,
    app_resources: &mut AppResources,
) {
    use display_list::DisplayList;

    use webrender::api::{DeviceIntPoint, DeviceIntRect, Transaction};

    let display_list = DisplayList::new_from_ui_description(ui_description, ui_state);

    let (builder, scrolled_nodes) =
        display_list.into_display_list_builder(app_data, window, fake_window, app_resources);

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    window.internal.last_display_list_builder = builder.finalize().2;
    window.internal.last_scrolled_nodes = scrolled_nodes;

    let (logical_size, framebuffer_size) = convert_window_size(&window.state.size);

    let webrender_transaction = {
        let mut txn = Transaction::new();

        // Send webrender the size and buffer of the display
        let bounds = DeviceIntRect::new(DeviceIntPoint::new(0, 0), framebuffer_size);
        txn.set_window_parameters(
            framebuffer_size,
            bounds,
            window.state.size.hidpi_factor as f32,
        );

        txn.set_display_list(
            window.internal.epoch,
            None,
            logical_size,
            (
                window.internal.pipeline_id,
                logical_size,
                window.internal.last_display_list_builder.clone(),
            ),
            true,
        );

        txn.set_root_pipeline(window.internal.pipeline_id);
        scroll_all_nodes(&mut window.scroll_states, &mut txn);
        txn.generate_frame();
        txn
    };

    window.internal.epoch = increase_epoch(window.internal.epoch);
    window
        .internal
        .api
        .send_transaction(window.internal.document_id, webrender_transaction);
    window.renderer.as_mut().unwrap().update();
    render_inner(window, framebuffer_size);
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
        txn.scroll_node_with_id(
            LayoutPoint::new(x, y),
            *key,
            ScrollClamping::ToContentBounds,
        );
    }
}

/// Returns the (logical_size, physical_size) as LayoutSizes, which can then be passed to webrender
fn convert_window_size(size: &WindowSize) -> (LayoutSize, DeviceIntSize) {
    let logical_size = LayoutSize::new(size.dimensions.width as f32, size.dimensions.height as f32);
    let physical_size = size.dimensions.to_physical(size.hidpi_factor);
    let physical_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
    (logical_size, physical_size)
}

/// Special rendering function that skips building a layout and only does
/// hit-testing and rendering - called on pure scroll events, since it's
/// significantly less CPU-intensive to just render the last display list instead of
/// re-layouting on every single scroll event.
///
/// If `hit_test_results`
fn render_on_scroll<T: Layout>(
    window: &mut Window<T>,
    hit_test_results: Option<HitTestResult>,
    frame_event_info: &FrameEventInfo,
) {
    const SCROLL_THRESHOLD: f64 = 0.5; // px

    let hit_test_results = match hit_test_results {
        Some(s) => s,
        None => match do_hit_test(&window) {
            Some(s) => s,
            None => return,
        },
    };

    let scroll_x = window.state.mouse_state.scroll_x;
    let scroll_y = window.state.mouse_state.scroll_y;

    if scroll_x.abs() < SCROLL_THRESHOLD && scroll_y.abs() < SCROLL_THRESHOLD {
        return;
    }

    let mut should_scroll_render = false;

    {
        let scrolled_nodes = &window.internal.last_scrolled_nodes;
        let scroll_states = &mut window.scroll_states;

        for scroll_node in hit_test_results
            .items
            .iter()
            .filter_map(|item| {
                scrolled_nodes
                    .tags_to_node_ids
                    .get(&ScrollTagId(item.tag.0))
            })
            .filter_map(|node_id| scrolled_nodes.overflowing_nodes.get(&node_id))
        {
            // The external scroll ID is constructed from the DOM hash
            let scroll_id = scroll_node.parent_external_scroll_id;

            if scroll_states.0.contains_key(&scroll_id) {
                // TODO: make scroll speed configurable (system setting?)
                scroll_states.scroll_node(&scroll_id, scroll_x as f32, scroll_y as f32);
                should_scroll_render = true;
            }
        }
    }

    // If there is already a layout construction in progress, prevent
    // re-rendering on layout, otherwise this leads to jankiness during scrolling
    if !frame_event_info.should_redraw_window && should_scroll_render {
        render_on_scroll_no_layout(window);
    }
}

fn render_on_scroll_no_layout<T: Layout>(window: &mut Window<T>) {
    use webrender::api::*;

    let mut txn = Transaction::new();

    scroll_all_nodes(&mut window.scroll_states, &mut txn);

    txn.generate_frame();

    window
        .internal
        .api
        .send_transaction(window.internal.document_id, txn);
    window.renderer.as_mut().unwrap().update();

    let (_, physical_size) = convert_window_size(&window.state.size);
    render_inner(window, physical_size);
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

// See: https://github.com/servo/webrender/pull/2880
// webrender doesn't reset the active shader back to what it was, but rather sets it
// to zero, which glium doesn't know about, so on the next frame it tries to draw with shader 0
//
// For some reason, webrender allows rendering negative width / height, although that doesn't make sense
fn render_inner<T: Layout>(window: &mut Window<T>, framebuffer_size: DeviceIntSize) {
    use gleam::gl;
    use window::get_gl_context;

    // use glium::glutin::GlContext;
    // unsafe { window.display.gl_window().make_current().unwrap(); }

    let mut current_program = [0_i32];
    unsafe {
        get_gl_context(&window.display)
            .unwrap()
            .get_integer_v(gl::CURRENT_PROGRAM, &mut current_program)
    };
    window
        .renderer
        .as_mut()
        .unwrap()
        .render(framebuffer_size)
        .unwrap();
    get_gl_context(&window.display)
        .unwrap()
        .use_program(current_program[0] as u32);
}

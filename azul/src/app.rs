use std::{
    mem,
    fmt,
    io::Read,
    collections::BTreeMap,
    sync::{Arc, Mutex, PoisonError},
};
#[cfg(debug_assertions)]
use std::time::Instant;
use glium::{
    SwapBuffersError,
    glutin::{
        Event,
        dpi::{LogicalPosition, LogicalSize}
    },
};
use webrender::{
    PipelineInfo,
    api::{
        HitTestResult, HitTestFlags, DevicePixel,
        WorldPoint, LayoutSize, LayoutPoint,
        Epoch, Transaction, ImageFormat as RawImageFormat,
    },
};
#[cfg(feature = "image_loading")]
use image::ImageError;
#[cfg(feature = "logging")]
use log::LevelFilter;
#[cfg(feature = "image_loading")]
use images::ImageType;
use azul_css::{FontId, PixelValue, StyleLetterSpacing};
use {
    error::{FontError, ClipboardError},
    window::{Window, WindowId, FakeWindow, ScrollStates},
    window_state::WindowSize,
    text_cache::TextId,
    dom::{ScrollTagId, UpdateScreen},
    app_resources::AppResources,
    app_state::AppState,
    traits::Layout,
    ui_state::UiState,
    ui_description::UiDescription,
    daemon::Daemon,
    focus::FocusTarget,
};

type DeviceUintSize = ::euclid::TypedSize2D<u32, DevicePixel>;
type DeviceIntSize = ::euclid::TypedSize2D<i32, DevicePixel>;

/// Graphical application that maintains some kind of application state
pub struct App<T: Layout> {
    /// The graphical windows, indexed by ID
    windows: BTreeMap<WindowId, Window<T>>,
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
    MutexLockError,
    WindowIndexError,
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
            GlSwapError(e) => write!(f, "Failed to swap GL display: {}", e),
            ArcUnlockError => write!(f, "Failed to unlock arc on application shutdown"),
            MutexPoisonError(e) => write!(f, "Mutex poisoned (thread panicked unexpectedly): {}", e),
            MutexLockError => write!(f, "Failed to lock application state mutex"),
            WindowIndexError => write!(f, "Invalid window index"),
        }
    }
}

impl<T: Layout> fmt::Display for RuntimeError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", format!("{:?}", self))
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
    /// (STUB) Whether keyboard navigation should be enabled (default: true).
    /// Currently not implemented.
    pub enable_tab_navigation: bool,
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
        }
    }
}

impl<T: Layout> App<T> {

    #[allow(unused_variables)]
    /// Create a new, empty application. This does not open any windows.
    pub fn new(initial_data: T, config: AppConfig) -> Self {
        #[cfg(feature = "logging")] {
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
            windows: BTreeMap::new(),
            app_state: AppState::new(initial_data),
        }
    }

    /// Spawn a new window on the screen. Note that this should only be used to
    /// create extra windows, the default window will be the window submitted to
    /// the `.run` method.
    pub fn push_window(&mut self, window: Window<T>) {
        use default_callbacks::DefaultCallbackSystem;

        let window_id = window.id;
        let fake_window = FakeWindow {
            state: window.state.clone(),
            default_callbacks: DefaultCallbackSystem::new(),
            read_only_window: window.display.clone(),
        };

        self.app_state.windows.insert(window_id, fake_window);
        self.windows.insert(window_id, window);
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
    pub fn run(mut self, window: Window<T>) -> Result<T, RuntimeError<T>>
    {
        // Apps need to have at least one window open
        self.push_window(window);
        self.run_inner()?;

        // NOTE: This is necessary because otherwise, the Arc::try_unwrap would fail,
        // since one Arc is still owned by the app_state.tasks structure
        //
        // See https://github.com/maps4print/azul/issues/24#issuecomment-429737273
        mem::drop(self.app_state.tasks);

        let unique_arc = Arc::try_unwrap(self.app_state.data).map_err(|_| RuntimeError::ArcUnlockError)?;
        unique_arc.into_inner().map_err(|e| e.into())
    }

    fn run_inner(&mut self) -> Result<(), RuntimeError<T>> {

        use std::{thread, time::{Duration, Instant}};
        use dom::Redraw;

        let mut ui_state_cache = {
            let app_state = &mut self.app_state;
            let mut ui_state_map = BTreeMap::new();
            for window_id in self.windows.keys() {
              ui_state_map.insert(*window_id, UiState::from_app_state(app_state, window_id)?);
            }
            ui_state_map
        };
        let mut ui_description_cache = self.windows.keys().map(|window_id| {
            (*window_id, UiDescription::default())
        }).collect();
        let mut force_redraw_cache = self.windows.keys().map(|window_id| {
            (*window_id, 1)
        }).collect();
        let mut awakened_task = self.windows.keys().map(|window_id| {
            (*window_id, false)
        }).collect();

        #[cfg(debug_assertions)]
        let mut last_style_reload = Instant::now();
        #[cfg(debug_assertions)]
        let mut should_print_css_error = true;

        while !self.windows.is_empty() {

            let time_start = Instant::now();
            let mut closed_windows = Vec::<WindowId>::new();
            let mut frame_was_resize = false;

            'window_loop: for (window_id, mut window) in self.windows.iter_mut() {
                let (event_was_resize, window_was_closed) =
                render_single_window_content(
                    &mut window,
                    &window_id,
                    &mut self.app_state,
                    &mut ui_state_cache,
                    &mut ui_description_cache,
                    &mut force_redraw_cache,
                    &mut awakened_task,
                )?;

                if event_was_resize {
                    frame_was_resize = true;
                }
                if window_was_closed {
                    closed_windows.push(*window_id);
                }

            }

            #[cfg(debug_assertions)] {
                hot_reload_css(&mut self.windows, &mut last_style_reload, &mut should_print_css_error, &mut awakened_task)?;
            }

            // Close windows if necessary
            closed_windows.into_iter().for_each(|closed_window_id| {
                ui_state_cache.remove(&closed_window_id);
                ui_description_cache.remove(&closed_window_id);
                force_redraw_cache.remove(&closed_window_id);
                self.windows.remove(&closed_window_id);
            });

            let should_redraw_daemons = self.app_state.run_all_daemons();
            let should_redraw_tasks = self.app_state.clean_up_finished_tasks();

            if [should_redraw_daemons, should_redraw_tasks].into_iter().any(|e| *e == Redraw) {
                self.windows.iter().for_each(|(_, window)| window.events_loop.create_proxy().wakeup().unwrap_or(()));
                awakened_task = self.windows.keys().map(|window_id| {
                    (*window_id, true)
                }).collect();
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

    /// Add an image to the internal resources. Only available with
    /// `--feature="image_loading"` (on by default)
    ///
    /// ## Returns
    ///
    /// - `Ok(Some(()))` if an image with the same ID already exists.
    /// - `Ok(None)` if the image was added, but didn't exist previously.
    /// - `Err(e)` if the image couldn't be decoded
    #[cfg(feature = "image_loading")]
    pub fn add_image<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R, image_type: ImageType)
        -> Result<Option<()>, ImageError>
    {
        self.app_state.add_image(id, data, image_type)
    }

    pub fn add_image_raw<S: Into<String>>(
        &mut self,
        id: S,
        pixels: Vec<u8>,
        image_dimensions: (u32, u32),
        data_format: RawImageFormat
    ) -> Option<()>
    {
        self.app_state.add_image_raw(id, pixels, image_dimensions, data_format)
    }

    /// Removes an image from the internal app resources.
    /// Returns `Some` if the image existed and was removed.
    /// If the given ID doesn't exist, this function does nothing and returns `None`.
    pub fn delete_image<S: AsRef<str>>(&mut self, id: S)
        -> Option<()>
    {
        self.app_state.delete_image(id)
    }

    /// Checks if an image is currently registered and ready-to-use
    pub fn has_image<S: AsRef<str>>(&mut self, id: S)
        -> bool
    {
        self.app_state.has_image(id)
    }

    /// Add a font (TTF or OTF) as a resource, identified by ID
    ///
    /// ## Returns
    ///
    /// - `Ok(Some(()))` if an font with the same ID already exists.
    /// - `Ok(None)` if the font was added, but didn't exist previously.
    /// - `Err(e)` if the font couldn't be decoded
    pub fn add_font<R: Read>(&mut self, id: FontId, data: &mut R)
        -> Result<Option<()>, FontError>
    {
        self.app_state.add_font(id, data)
    }

    /// Checks if a font is currently registered and ready-to-use
    pub fn has_font(&mut self, id: &FontId)
        -> bool
    {
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
    pub fn delete_font(&mut self, id: &FontId)
        -> Option<()>
    {
        self.app_state.delete_font(id)
    }

    /// Create a daemon. Does nothing if a daemon with the function pointer location already exists.
    ///
    /// If the daemon was inserted, returns true, otherwise false
    pub fn add_daemon(&mut self, daemon: Daemon<T>)
        -> bool
    {
        self.app_state.add_daemon(daemon)
    }

    pub fn add_text_uncached<S: Into<String>>(&mut self, text: S)
    -> TextId
    {
        self.app_state.add_text_uncached(text)
    }

    pub fn add_text_cached<S: Into<String>>(&mut self, text: S, font_id: &FontId, font_size: PixelValue, letter_spacing: Option<StyleLetterSpacing>)
    -> TextId
    {
        self.app_state.add_text_cached(text, font_id, font_size, letter_spacing)
    }

    pub fn delete_text(&mut self, id: TextId) {
        self.app_state.delete_text(id);
    }

    pub fn clear_all_texts(&mut self) {
        self.app_state.clear_all_texts();
    }

    /// Get the contents of the system clipboard as a string
    pub fn get_clipboard_string(&mut self)
    -> Result<String, ClipboardError>
    {
        self.app_state.get_clipboard_string()
    }

    /// Set the contents of the system clipboard as a string
    pub fn set_clipboard_string(&mut self, contents: String)
    -> Result<(), ClipboardError>
    {
        self.app_state.set_clipboard_string(contents)
    }

    /// See `AppState::add_custom_task`.
    pub fn add_custom_task<U: Send + 'static>(
        &mut self,
        data: &Arc<Mutex<U>>,
        callback: fn(Arc<Mutex<U>>, Arc<()>),
        after_completion_deamons: &[Daemon<T>])
    {
        self.app_state.add_custom_task(data, callback, after_completion_deamons);
    }
}

impl<T: Layout + Send + 'static> App<T> {
    /// See `AppState::add_ask`.
    pub fn add_task(
        &mut self,
        callback: fn(Arc<Mutex<T>>, Arc<()>),
        after_completion_callbacks: &[Daemon<T>])
    {
        self.app_state.add_task(callback, after_completion_callbacks);
    }
}

/// Render the contents of one single window. Returns
/// (if the event was a resize event, if the window was closed)
fn render_single_window_content<T: Layout>(
    window: &mut Window<T>,
    window_id: &WindowId,
    app_state: &mut AppState<T>,
    ui_state_cache: &mut BTreeMap<WindowId, UiState<T>>,
    ui_description_cache: &mut BTreeMap<WindowId, UiDescription<T>>,
    force_redraw_cache: &mut BTreeMap<WindowId, usize>,
    awakened_task: &mut BTreeMap<WindowId, bool>,
) -> Result<(bool, bool), RuntimeError<T>>
{
    use dom::Redraw;
    use self::RuntimeError::*;

    let mut frame_was_resize = false;

    let mut events = Vec::new();
    window.events_loop.poll_events(|e| events.push(e));
    if events.is_empty() {
        let window_should_close = false;
        return Ok((frame_was_resize, window_should_close));
    }

    let (mut frame_event_info, window_should_close) =
        window.state.update_window_state(&events, awakened_task[window_id]);

    if window_should_close {
        let window_should_close = true;
        return Ok((frame_was_resize, window_should_close));
    }

    let mut hit_test_results = None;

    if frame_event_info.should_hittest {

        hit_test_results = do_hit_test(&window);

        for event in &events {

            let callback_result = call_callbacks(
                hit_test_results.as_ref(),
                event,
                window,
                &window_id,
                &ui_state_cache[&window_id],
                app_state
            )?;

            if callback_result.should_update_screen == Redraw {
                frame_event_info.should_redraw_window = true;
            }

            window.state.pending_focus_target = callback_result.callbacks_overwrites_focus;
        }
    }

    // Scroll for the scrolled amount for each node that registered a scroll state.
    render_on_scroll(window, hit_test_results, &frame_event_info);

    if frame_event_info.should_swap_window || frame_event_info.is_resize_event || force_redraw_cache[window_id] > 0 {
        window.display.swap_buffers()?;
        if let Some(i) = force_redraw_cache.get_mut(window_id) {
            if *i > 0 { *i -= 1 };
            if *i == 0 {
                clean_up_unused_opengl_textures(window.renderer.as_mut().unwrap().flush_pipeline_info());
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
        *force_redraw_cache.get_mut(window_id).ok_or(WindowIndexError)? = 2;
        frame_was_resize = true;
    }

    if frame_event_info.is_resize_event {
        // Resize gl window
        let gl_window = window.display.gl_window();
        let size = gl_window.get_inner_size().unwrap().to_physical(gl_window.get_hidpi_factor());
        gl_window.resize(size);
    }

    // Update the window state that we got from the frame event (updates window dimensions and DPI)
    // Sets frame_event_info.needs redraw if the event was a
    window.update_from_external_window_state(&frame_event_info);
    // Update the window state every frame that was set by the user
    window.update_from_user_window_state(app_state.windows[&window_id].state.clone());
    // Reset the scroll amount to 0 (for the next frame)
    window.clear_scroll_state();

    if frame_event_info.should_redraw_window || force_redraw_cache[window_id] > 0 {

        // Call the Layout::layout() fn, get the DOM
        *ui_state_cache.get_mut(window_id).ok_or(WindowIndexError)? =
            UiState::from_app_state(app_state, window_id)?;

        // Style the DOM (is_mouse_down is necessary for styling :hover, :active + :focus nodes)
        let is_mouse_down = window.state.mouse_state.mouse_down();

        *ui_description_cache.get_mut(window_id).ok_or(WindowIndexError)? =
            UiDescription::match_css_to_dom(
                ui_state_cache.get_mut(window_id).ok_or(WindowIndexError)?,
                &window.css,
                &mut window.state.focused_node,
                &mut window.state.pending_focus_target,
                &window.state.hovered_nodes,
                is_mouse_down,
            );

        // Render the window (webrender will send an Awakened event when the frame is done)
        let mut fake_window = app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?;
        render(
            &mut app_state.data,
            &ui_description_cache[window_id],
            &ui_state_cache[window_id],
            &mut *window,
            &mut fake_window,
            &mut app_state.resources,
        );

        *awakened_task.get_mut(window_id).ok_or(WindowIndexError)? = false;
    }

    let window_should_close = false;
    Ok((frame_was_resize, window_should_close))
}

/// Returns if there was an error with the CSS reloading, necessary so that the error message is only printed once
#[cfg(debug_assertions)]
fn hot_reload_css<T: Layout>(
    windows: &mut BTreeMap<WindowId, Window<T>>,
    last_style_reload: &mut Instant,
    should_print_error: &mut bool,
    awakened_tasks: &mut BTreeMap<WindowId, bool>)
-> Result<(), RuntimeError<T>>
{
    use self::RuntimeError::*;
    for (window_id, window) in windows.iter_mut() {
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
                window.events_loop.create_proxy().wakeup().unwrap_or(());
                *awakened_tasks.get_mut(window_id).ok_or(WindowIndexError)? = true;

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

    Ok(())
}

/// Returns the currently hit-tested results, in back-to-front order
fn do_hit_test<T: Layout>(window: &Window<T>) -> Option<HitTestResult> {

    let cursor_location = window.state.mouse_state.cursor_pos
        .map(|pos| WorldPoint::new(pos.x as f32, pos.y as f32))?;

    let mut hit_test_results = window.internal.api.hit_test(
        window.internal.document_id,
        Some(window.internal.pipeline_id),
        cursor_location,
        HitTestFlags::FIND_ALL
    );

    // Execute callbacks back-to-front, not front-to-back
    hit_test_results.items.reverse();

    Some(hit_test_results)
}

struct CallCallbackReturn {
    pub should_update_screen: UpdateScreen,
    pub callbacks_overwrites_focus: Option<FocusTarget>,
}

/// Returns an bool whether the window should be redrawn or not (true - redraw the screen, false: don't redraw).
fn call_callbacks<T: Layout>(
    hit_test_results: Option<&HitTestResult>,
    event: &Event,
    window: &mut Window<T>,
    window_id: &WindowId,
    ui_state: &UiState<T>,
    app_state: &mut AppState<T>)
-> Result<CallCallbackReturn, RuntimeError<T>>
{
    use app_state::AppStateNoData;
    use window::CallbackInfo;
    use dom::{Redraw, DontRedraw};
    use window_state::{KeyboardState, MouseState};
    use self::RuntimeError::*;

    let mut should_update_screen = DontRedraw;

    let hit_test_items = hit_test_results.map(|h| h.items.clone()).unwrap_or_default();

    let callbacks_filter_list = window.state.determine_callbacks(&hit_test_items, event, ui_state);

    // TODO: this should be refactored - currently very stateful and error-prone!
    app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?
        .set_keyboard_state(&window.state.keyboard_state);
    app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?
        .set_mouse_state(&window.state.mouse_state);

    let mut callbacks_overwrites_focus = None;

    // Run all default callbacks - **before** the user-defined callbacks are run!
    {
        let mut lock = app_state.data.lock().map_err(|_| RuntimeError::MutexLockError)?;

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

                let app_state_no_data = AppStateNoData {
                    windows: &app_state.windows,
                    resources: &mut app_state.resources,
                };

                if app_state.windows[window_id].default_callbacks.run_callback(
                    &mut *lock,
                    default_callback_id,
                    app_state_no_data,
                    &mut callback_info
                ) == Redraw {
                    should_update_screen = Redraw;
                }

                // Overwrite the focus from the callback info
                if let Some(new_focus) = callback_info.focus {
                    callbacks_overwrites_focus = Some(new_focus);
                }
            }
        }
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

    if callbacks_filter_list.needs_redraw_anyways {
        should_update_screen = Redraw;
    }

    app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?
        .set_keyboard_state(&KeyboardState::default());
    app_state.windows.get_mut(window_id).ok_or(WindowIndexError)?
        .set_mouse_state(&MouseState::default());

    Ok(CallCallbackReturn {
        should_update_screen,
        callbacks_overwrites_focus,
    })
}

fn render<T: Layout>(
    app_data: &mut Arc<Mutex<T>>,
    ui_description: &UiDescription<T>,
    ui_state: &UiState<T>,
    window: &mut Window<T>,
    fake_window: &mut FakeWindow<T>,
    app_resources: &mut AppResources)
{
    use display_list::DisplayList;

    use webrender::api::{Transaction, DeviceIntRect, DeviceIntPoint};

    let display_list = DisplayList::new_from_ui_description(ui_description, ui_state);

    let (builder, scrolled_nodes) = display_list.into_display_list_builder(
        app_data,
        window,
        fake_window,
        app_resources,
    );

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    window.internal.last_display_list_builder = builder.finalize().2;
    window.internal.last_scrolled_nodes = scrolled_nodes;

    let (logical_size, framebuffer_size) = convert_window_size(&window.state.size);

    let webrender_transaction = {
        let mut txn = Transaction::new();

        // Send webrender the size and buffer of the display
        let bounds = DeviceIntRect::new(DeviceIntPoint::new(0, 0), framebuffer_size);
        txn.set_window_parameters(framebuffer_size, bounds, window.state.size.hidpi_factor as f32);

        txn.set_display_list(
            window.internal.epoch,
            None,
            logical_size,
            (window.internal.pipeline_id, logical_size, window.internal.last_display_list_builder.clone()),
            true,
        );

        txn.set_root_pipeline(window.internal.pipeline_id);
        scroll_all_nodes(&mut window.scroll_states, &mut txn);
        txn.generate_frame();
        txn
    };

    window.internal.epoch = increase_epoch(window.internal.epoch);
    window.internal.api.send_transaction(window.internal.document_id, webrender_transaction);
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
        txn.scroll_node_with_id(LayoutPoint::new(x, y), *key, ScrollClamping::ToContentBounds);
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
fn render_on_scroll<T: Layout>(
    window: &mut Window<T>,
    hit_test_results: Option<HitTestResult>,
    frame_event_info: &FrameEventInfo)
{
    const SCROLL_THRESHOLD: f64 = 0.5; // px

    let hit_test_results = match hit_test_results {
        Some(s) => s,
        None => match do_hit_test(&window) {
            Some(s) => s,
            None => return,
        }
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

        for scroll_node in hit_test_results.items.iter()
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

    window.internal.api.send_transaction(window.internal.document_id, txn);
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
    unsafe { get_gl_context(&window.display).unwrap().get_integer_v(gl::CURRENT_PROGRAM, &mut current_program) };
    window.renderer.as_mut().unwrap().render(framebuffer_size).unwrap();
    get_gl_context(&window.display).unwrap().use_program(current_program[0] as u32);
}

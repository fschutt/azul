use std::{
    mem,
    fmt,
    io::Read,
    sync::{Arc, Mutex, PoisonError},
};
use glium::{
    SwapBuffersError,
    glutin::{
        Event,
        dpi::{LogicalPosition, LogicalSize}
    },
};
use webrender::{PipelineInfo, api::{HitTestFlags, DevicePixel}};
use euclid::TypedSize2D;
#[cfg(feature = "image_loading")]
use image::ImageError;
#[cfg(feature = "logging")]
use log::LevelFilter;
#[cfg(feature = "image_loading")]
use images::ImageType;
use {
    error::{FontError, ClipboardError},
    window::{Window, WindowId},
    css_parser::{FontId, PixelValue, StyleLetterSpacing},
    text_cache::TextId,
    dom::UpdateScreen,
    window::FakeWindow,
    css::{FakeCss, ParsedCss},
    app_resources::AppResources,
    app_state::AppState,
    traits::Layout,
    ui_state::UiState,
    ui_description::UiDescription,
    daemon::Daemon,
};

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
            css: FakeCss::default(),
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
    /// app.create_window(WindowCreateOptions::default(), Css::native());
    ///
    /// // pop open a window that asks the user for his username and password...
    /// let MyData { username, password } = app.run();
    ///
    /// // continue the rest of the program here...
    /// println!("username: {:?}, password: {:?}", username, password);
    /// ```
    pub fn run(mut self, window: Window<T>) -> Result<T, RuntimeError<T>>
    {
        self.push_window(window);
        self.run_inner()?;
        mem::drop(self.app_state.tasks);
        let unique_arc = Arc::try_unwrap(self.app_state.data).map_err(|_| RuntimeError::ArcUnlockError)?;
        unique_arc.into_inner().map_err(|e| e.into())
    }

    fn run_inner(&mut self) -> Result<(), RuntimeError<T>> {

        use std::{thread, time::{Duration, Instant}};

        let mut ui_state_cache = Self::initialize_ui_state(&self.windows, &mut self.app_state);
        let mut ui_description_cache = vec![UiDescription::default(); self.windows.len()];
        let mut force_redraw_cache = vec![1_usize; self.windows.len()];
        let mut parsed_css_cache = vec![None; self.windows.len()];
        let mut awakened_task = vec![false; self.windows.len()];

        #[cfg(debug_assertions)]
        let mut last_css_reload = Instant::now();

        while !self.windows.is_empty() {

            let time_start = Instant::now();
            let mut closed_windows = Vec::<usize>::new();

            'window_loop: for (idx, ref mut window) in self.windows.iter_mut().enumerate() {

                let window_id = WindowId { id: idx };
                let mut frame_event_info = FrameEventInfo::default();

                let mut events = Vec::new();
                window.events_loop.poll_events(|e| events.push(e));

                for event in &events {
                    if preprocess_event(event, &mut frame_event_info, awakened_task[idx]) == WindowCloseEvent::AboutToClose {
                        closed_windows.push(idx);
                        continue 'window_loop;
                    }
                    window.state.update_mouse_cursor_position(event);
                    window.state.update_keyboard_modifiers(event);
                    window.state.update_keyboard_pressed_chars(event);
                }

                if frame_event_info.should_hittest {
                    for event in &events {
                        do_hit_test_and_call_callbacks(
                            event,
                            window,
                            window_id,
                            &mut frame_event_info,
                            &ui_state_cache,
                            &mut self.app_state);
                    }
                }

                if frame_event_info.should_swap_window || frame_event_info.is_resize_event {
                    window.display.swap_buffers()?;
                    if let Some(i) = force_redraw_cache.get_mut(idx) {
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
                    force_redraw_cache[idx] = 2;
                }

                // Update the window state that we got from the frame event (updates window dimensions and DPI)
                window.update_from_external_window_state(&mut frame_event_info);
                // Update the window state every frame that was set by the user
                window.update_from_user_window_state(self.app_state.windows[idx].state.clone());
                // Reset the scroll amount to 0 (for the next frame)
                window.clear_scroll_state();

                if frame_event_info.should_redraw_window || force_redraw_cache[idx] > 0 {

                    let should_update_parsed_css = {
                        // In debug mode, if hot-reloading is active, we want to always update the ParsedCss
                        #[cfg(not(debug_assertions))] {
                            parsed_css_cache[idx].is_none()
                        }
                        #[cfg(debug_assertions)] {
                            parsed_css_cache[idx].is_none() || window.css.hot_reload_path.is_some()
                        }
                    };

                    if should_update_parsed_css {
                        parsed_css_cache[idx] = Some(ParsedCss::from_css(&window.css));
                    }

                    let parsed_css = parsed_css_cache[idx].as_ref().unwrap();

                    // Call the Layout::layout() fn, get the DOM
                    let window_id = WindowId { id: idx };
                    ui_state_cache[idx] = UiState::from_app_state(&mut self.app_state, window_id);

                    // Style the DOM
                    ui_description_cache[idx] = UiDescription::from_dom(
                        &ui_state_cache[idx],
                        &parsed_css,
                        &window.css.dynamic_css_overrides
                    );

                    // Send webrender the size and buffer of the display
                    Self::update_display(&window);

                    // render the window (webrender will send an Awakened event when the frame is done)
                    let arc_mutex_t_clone = self.app_state.data.clone();

                    render(
                        arc_mutex_t_clone,
                        true, /* has_window_size_changed */

                        &ui_description_cache[idx],
                        &ui_state_cache[idx],
                        &parsed_css,

                        &mut *window,
                        &mut self.app_state.windows[idx],
                        &mut self.app_state.resources);

                    awakened_task[idx] = false;
                }
            }

            #[cfg(debug_assertions)] {
                for (window_idx, window) in self.windows.iter_mut().enumerate() {
                    // Hot-reload CSS if necessary
                    if window.css.hot_reload_path.is_some() && (Instant::now() - last_css_reload) > Duration::from_millis(500) {
                        window.css.reload_css();
                        window.css.needs_relayout = true;
                        last_css_reload = Instant::now();
                        window.events_loop.create_proxy().wakeup().unwrap_or(());
                        awakened_task[window_idx] = true;
                    }
                }
            }

            // Close windows if necessary
            closed_windows.into_iter().for_each(|closed_window_id| {
                ui_state_cache.remove(closed_window_id);
                ui_description_cache.remove(closed_window_id);
                force_redraw_cache.remove(closed_window_id);
                parsed_css_cache.remove(closed_window_id);
                self.windows.remove(closed_window_id);
            });

            let should_redraw_daemons = self.app_state.run_all_daemons();
            let should_redraw_tasks = self.app_state.clean_up_finished_tasks();

            if [should_redraw_daemons, should_redraw_tasks].into_iter().any(|e| *e == UpdateScreen::Redraw) {
                self.windows.iter().for_each(|w| w.events_loop.create_proxy().wakeup().unwrap_or(()));
                awakened_task = vec![true; self.windows.len()];
            } else {
                // Wait until 16ms have passed
                let diff = time_start.elapsed();
                const FRAME_TIME: Duration = Duration::from_millis(16);
                if diff < FRAME_TIME {
                    thread::sleep(FRAME_TIME - diff);
                }
            }
        }

        Ok(())
    }

    fn update_display(window: &Window<T>)
    {
        use webrender::api::{Transaction, DeviceUintRect, DeviceUintPoint};
        use euclid::TypedSize2D;

        let mut txn = Transaction::new();
        let physical_fb_dimensions = window.state.size.dimensions.to_physical(window.state.size.hidpi_factor);
        let framebuffer_size = TypedSize2D::new(physical_fb_dimensions.width as u32, physical_fb_dimensions.height as u32);
        let bounds = DeviceUintRect::new(DeviceUintPoint::new(0, 0), framebuffer_size);

        txn.set_window_parameters(framebuffer_size, bounds, window.state.size.hidpi_factor as f32);
        window.internal.api.send_transaction(window.internal.document_id, txn);
    }

    fn initialize_ui_state(windows: &[Window<T>], app_state: &mut AppState<T>)
    -> Vec<UiState<T>>
    {
        windows.iter().enumerate().map(|(idx, _window)| {
            let window_id = WindowId { id: idx };
            UiState::from_app_state(app_state, window_id)
        }).collect()
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
    /// ## Example
    ///
    #[cfg_attr(feature = "no-opengl-tests", doc = " ```no_run")]
    #[cfg_attr(not(feature = "no-opengl-tests"), doc = " ```")]
    /// # use azul::prelude::*;
    /// # const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/weblysleekuil.ttf");
    /// #
    /// # struct MyAppData { }
    /// #
    /// # impl Layout for MyAppData {
    /// #     fn layout(&self, _window_id: WindowInfo<MyAppData>) -> Dom<MyAppData> {
    /// #         Dom::new(NodeType::Div)
    /// #    }
    /// # }
    /// #
    /// # fn main() {
    /// let mut app = App::new(MyAppData { }, AppConfig::default());
    /// app.add_font(FontId::ExternalFont("Webly Sleeky UI".into()), &mut TEST_FONT).unwrap();
    /// app.delete_font(&FontId::ExternalFont("Webly Sleeky UI".into()));
    /// // NOTE: The font isn't immediately removed, only in the next draw call
    /// app.mock_render_frame();
    /// assert!(!app.has_font(&FontId::ExternalFont("Webly Sleeky UI".into())));
    /// # }
    /// ```
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

    /// Mock rendering function, for creating a hidden window and rendering one frame
    /// Used in unit tests. You **have** to enable software rendering, otherwise,
    /// this function won't work in a headless environment.
    ///
    /// **NOTE**: In a headless environment, such as Travis, you have to use XVFB to
    /// create a fake X11 server. XVFB also has a bug where it loads with the default of
    /// 8-bit greyscale color (see [here]). In order to fix that, you have to run:
    ///
    /// `xvfb-run --server-args "-screen 0 1920x1080x24" cargo test --features "doc-test"`
    ///
    /// [here]: https://unix.stackexchange.com/questions/104914/
    ///
    #[cfg(any(feature = "doc-test"))]
    pub fn mock_render_frame(&mut self) {
        use prelude::*;
        let hidden_create_options = WindowCreateOptions {
            state: WindowState { is_visible: false, .. Default::default() },
            /// force sofware renderer (OSMesa)
            renderer_type: RendererType::Software,
            .. Default::default()
        };
        self.push_window(Window::new(hidden_create_options, Css::native()).unwrap());
        // TODO: do_first_redraw shouldn't exist, need to find a better way to update the resources
        // This will make App::delete_font doc-test fail if run without `no-opengl-tests`.
        //
        // let ui_state_cache = Self::initialize_ui_state(&self.windows, &self.app_state);
        // Self::do_first_redraw(&mut self.windows, &mut self.app_state, &ui_state_cache);
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

#[derive(Debug, Copy, Clone, PartialEq)]
enum WindowCloseEvent {
    AboutToClose,
    NoCloseEvent,
}

/// Pre-filters any events that are not handled by the framework yet, since it would be wasteful
/// to process them. Modifies the `frame_event_info`
///
/// `awakened_task` is a special field that should be set to true if the `Task`
/// system fired a `WindowEvent::Awakened`.
fn preprocess_event(event: &Event, frame_event_info: &mut FrameEventInfo, awakened_task: bool) -> WindowCloseEvent {
    use glium::glutin::WindowEvent;

    match event {
        Event::WindowEvent { event, .. } => {
            match event {
                WindowEvent::MouseInput { .. } => {
                    frame_event_info.should_hittest = true;
                },
                WindowEvent::CursorMoved { position, .. } => {
                    frame_event_info.should_hittest = true;
                    frame_event_info.cur_cursor_pos = *position;
                },
                WindowEvent::Resized(wh) => {
                    frame_event_info.new_window_size = Some(*wh);
                    frame_event_info.is_resize_event = true;
                    frame_event_info.should_redraw_window = true;
                },
                WindowEvent::Refresh => {
                    frame_event_info.should_redraw_window = true;
                },
                WindowEvent::HiDpiFactorChanged(dpi) => {
                    frame_event_info.new_dpi_factor = Some(*dpi);
                    frame_event_info.should_redraw_window = true;
                },
                WindowEvent::MouseWheel { .. } => {
                    frame_event_info.should_hittest = true;
                },
                WindowEvent::CloseRequested => {
                    return WindowCloseEvent::AboutToClose;
                },
                WindowEvent::KeyboardInput { .. } => {
                    frame_event_info.should_hittest = true;
                }
                _ => { },
            }
        },
        Event::Awakened => {
            frame_event_info.should_swap_window = true;
            if awakened_task {
                frame_event_info.should_redraw_window = true;
            }
        },
        _ => { },
    }

    WindowCloseEvent::NoCloseEvent
}

fn do_hit_test_and_call_callbacks<T: Layout>(
    event: &Event,
    window: &mut Window<T>,
    window_id: WindowId,
    info: &mut FrameEventInfo,
    ui_state_cache: &[UiState<T>],
    app_state: &mut AppState<T>)
{
    use dom::UpdateScreen;
    use webrender::api::WorldPoint;
    use window::WindowEvent;
    use dom::Callback;
    use window_state::{KeyboardState, MouseState};

    let cursor_location = match window.state.mouse_state.cursor_pos {
        Some(pos) => WorldPoint::new(pos.x as f32, pos.y as f32),
        None => return,
    };

    let hit_test_results = window.internal.api.hit_test(
        window.internal.document_id,
        Some(window.internal.pipeline_id),
        cursor_location,
        HitTestFlags::FIND_ALL);

    let mut should_update_screen = UpdateScreen::DontRedraw;

    let callbacks_filter_list = window.state.determine_callbacks(event);

    // TODO: this should be refactored - currently very stateful and error-prone!
    app_state.windows[window_id.id].set_keyboard_state(&window.state.keyboard_state);
    app_state.windows[window_id.id].set_mouse_state(&window.state.mouse_state);

    // Run all default callbacks - **before** the user-defined callbacks are run!
    // TODO: duplicated code!
    {
        use app_state::AppStateNoData;

        let mut lock = app_state.data.lock().unwrap();

        for (item, callback_id_list) in hit_test_results.items.iter().filter_map(|item|
            ui_state_cache[window_id.id].tag_ids_to_default_callbacks // <- NOTE: tag_ids_to_default_callbacks
            .get(&item.tag.0)
            .and_then(|callback_id_list| Some((item, callback_id_list)))
        ) {
            use dom::On;

            let window_event = WindowEvent {
                window: window_id.id,
                hit_dom_node: ui_state_cache[window_id.id].tag_ids_to_node_ids[&item.tag.0],
                ui_state: &ui_state_cache[window_id.id],
                hit_test_result: &hit_test_results,
                cursor_relative_to_item: (item.point_in_viewport.x, item.point_in_viewport.y),
                cursor_in_viewport: (item.point_in_viewport.x, item.point_in_viewport.y),
            };

            // Invoke On::MouseOver callback - TODO: duplicated code (due to borrowing issues)!
            if let Some(callback_id) = callback_id_list.get(&On::MouseOver) {

                let app_state_no_data = AppStateNoData {
                    windows: &app_state.windows,
                    resources: &mut app_state.resources,
                };

                // safe unwrap, we have added the callback previously
                if app_state.windows[window_id.id].default_callbacks.run_callback(
                    &mut *lock, callback_id, app_state_no_data, window_event
                    ) == UpdateScreen::Redraw {
                    should_update_screen = UpdateScreen::Redraw;
                }
            }

            for callback_id in callbacks_filter_list.iter().filter_map(|on| callback_id_list.get(on)) {

                let app_state_no_data = AppStateNoData {
                    windows: &app_state.windows,
                    resources: &mut app_state.resources,
                };

                // safe unwrap, we have added the callback previously
                if app_state.windows[window_id.id].default_callbacks.run_callback(
                    &mut *lock, callback_id, app_state_no_data, window_event
                    ) == UpdateScreen::Redraw {
                    should_update_screen = UpdateScreen::Redraw;
                }
            }
        }
    } // unlock AppState mutex

    // For all hit items, lookup the callback and call it
    for (item, callback_list) in hit_test_results.items.iter().filter_map(|item|
        ui_state_cache[window_id.id].tag_ids_to_callbacks
        .get(&item.tag.0)
        .and_then(|callback_list| Some((item, callback_list)))
    ) {
        use dom::On;

        let window_event = WindowEvent {
            window: window_id.id,
            hit_dom_node: ui_state_cache[window_id.id].tag_ids_to_node_ids[&item.tag.0],
            ui_state: &ui_state_cache[window_id.id],
            hit_test_result: &hit_test_results,
            cursor_relative_to_item: (item.point_in_viewport.x, item.point_in_viewport.y),
            cursor_in_viewport: (item.point_in_viewport.x, item.point_in_viewport.y),
        };

        let mut invoke_callback = |&Callback(callback_func)| {
            if (callback_func)(app_state, window_event) == UpdateScreen::Redraw {
                should_update_screen = UpdateScreen::Redraw;
            }
        };

        // Invoke On::MouseOver callback
        if let Some(callback_id) = callback_list.get(&On::MouseOver) {
            invoke_callback(callback_id);
        }

        // Invoke user-defined callback if necessary
        for callback_id in callbacks_filter_list.iter().filter_map(|on| callback_list.get(on)) {
            invoke_callback(callback_id);
        }
    }

    app_state.windows[window_id.id].set_keyboard_state(&KeyboardState::default());
    app_state.windows[window_id.id].set_mouse_state(&MouseState::default());

    if should_update_screen == UpdateScreen::Redraw {
        info.should_redraw_window = true;
        // TODO: THIS IS PROBABLY THE WRONG PLACE TO DO THIS!!!
        // Copy the current fake CSS changes to the real CSS, then clear the fake CSS again
        // TODO: .clone() and .clear() can be one operation
        window.css.dynamic_css_overrides = app_state.windows[window_id.id].css.dynamic_css_overrides.clone();
        // clear the dynamic CSS overrides
        app_state.windows[window_id.id].css.clear();
        app_state.windows[window_id.id].default_callbacks.clear();
    }
}

fn render<T: Layout>(
    app_data: Arc<Mutex<T>>,
    has_window_size_changed: bool,

    ui_description: &UiDescription<T>,
    ui_state: &UiState<T>,
    parsed_css: &ParsedCss,

    window: &mut Window<T>,
    fake_window: &mut FakeWindow<T>,
    app_resources: &mut AppResources)
{
    use webrender::api::*;
    use display_list::DisplayList;
    use euclid::TypedSize2D;
    use std::u32;

    let display_list = DisplayList::new_from_ui_description(ui_description, ui_state);

    let builder = display_list.into_display_list_builder(
        app_data,
        window.internal.pipeline_id,
        window.internal.epoch,
        has_window_size_changed,

        &window.internal.api,
        &parsed_css,
        &window.state.size,

        &mut *fake_window,
        &mut *app_resources);

    // NOTE: Display list has to be rebuilt every frame, otherwise, the epochs get out of sync
    window.internal.last_display_list_builder = builder.finalize().2;

    let mut txn = Transaction::new();

    let LogicalSize { width, height } = window.state.size.dimensions;
    let layout_size = TypedSize2D::new(width as f32, height as f32);
    let framebuffer_size_physical = window.state.size.dimensions.to_physical(window.state.size.hidpi_factor);
    let framebuffer_size = TypedSize2D::new(framebuffer_size_physical.width as u32, framebuffer_size_physical.height as u32);

    txn.set_display_list(
        window.internal.epoch,
        None,
        layout_size,
        (window.internal.pipeline_id, layout_size, window.internal.last_display_list_builder.clone()),
        true,
    );

    // We don't want the epoch to increase to u32::MAX, since u32::MAX represents
    // an invalid epoch, which could confuse webrender
    window.internal.epoch = Epoch(if window.internal.epoch.0 == (u32::MAX - 1) {
        0
    } else {
        window.internal.epoch.0 + 1
    });

    txn.set_root_pipeline(window.internal.pipeline_id);
    txn.generate_frame();

    window.internal.api.send_transaction(window.internal.document_id, txn);
    window.renderer.as_mut().unwrap().update();
    render_inner(window, framebuffer_size);
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

// See: https://github.com/servo/webrender/pull/2880
// webrender doesn't reset the active shader back to what it was, but rather sets it
// to zero, which glium doesn't know about, so on the next frame it tries to draw with shader 0
fn render_inner<T: Layout>(window: &mut Window<T>, framebuffer_size: TypedSize2D<u32, DevicePixel>) {

    use gleam::gl;
    use window::get_gl_context;

    // use glium::glutin::GlContext;
    // unsafe { window.display.gl_window().make_current().unwrap(); }

    let mut current_program = [0_i32];
    unsafe { get_gl_context(&window.display).unwrap().get_integer_v(gl::CURRENT_PROGRAM, &mut current_program) };
    window.renderer.as_mut().unwrap().render(framebuffer_size).unwrap();
    get_gl_context(&window.display).unwrap().use_program(current_program[0] as u32);
}

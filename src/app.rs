use std::{
    fmt,
    io::Read,
    sync::{Arc, Mutex, PoisonError},
};
use glium::{SwapBuffersError, glutin::Event};
use glium::glutin::dpi::{LogicalPosition, LogicalSize};
use webrender::api::{RenderApi, HitTestFlags, DevicePixel};
use image::ImageError;
use euclid::{TypedScale, TypedSize2D};
use {
    images::ImageType,
    errors::{FontError, ClipboardError},
    window::{Window, WindowCreateOptions, WindowCreateError, WindowId},
    css_parser::{Font as FontId, PixelValue, FontSize},
    text_cache::TextId,
    dom::UpdateScreen,
    window::FakeWindow,
    css::{Css, FakeCss},
    resources::AppResources,
    app_state::AppState,
    traits::Layout,
    ui_state::UiState,
    ui_description::UiDescription,
};

/// Graphical application that maintains some kind of application state
pub struct App<'a, T: Layout> {
    /// The graphical windows, indexed by ID
    windows: Vec<Window<T>>,
    /// The global application state
    pub app_state: AppState<'a, T>,
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
        write!(f, "{:?}", self)
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

impl<'a, T: Layout> App<'a, T> {

    /// Create a new, empty application. This does not open any windows.
    pub fn new(initial_data: T) -> Self {
        Self {
            windows: Vec::new(),
            app_state: AppState::new(initial_data),
        }
    }

    /// Spawn a new window on the screen. If an application has no windows,
    /// the [`run`](#method.run) function will exit immediately.
    pub fn create_window(&mut self, options: WindowCreateOptions, css: Css) -> Result<(), WindowCreateError> {
        let window = Window::new(options, css)?;
        self.app_state.windows.push(FakeWindow {
            state: window.state.clone(),
            css: FakeCss::default(),
            read_only_window: window.display.clone(),
        });
        self.windows.push(window);
        Ok(())
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
    pub fn run(mut self) -> Result<T, RuntimeError<T>>
    {
        self.run_inner()?;
        let unique_arc = Arc::try_unwrap(self.app_state.data).map_err(|_| RuntimeError::ArcUnlockError)?;
        unique_arc.into_inner().map_err(|e| e.into())
    }

    fn run_inner(&mut self) -> Result<(), RuntimeError<T>> {
        use std::{thread, time::{Duration, Instant}};
        use window::{ReadOnlyWindow, WindowInfo};

        let mut ui_state_cache = Self::initialize_ui_state(&self.windows, &self.app_state);
        let mut ui_description_cache = vec![UiDescription::default(); self.windows.len()];
        let mut force_redraw_cache = vec![1_usize; self.windows.len()];

        while !self.windows.is_empty() {

            let time_start = Instant::now();
            let mut closed_windows = Vec::<usize>::new();

            'window_loop: for (idx, ref mut window) in self.windows.iter_mut().enumerate() {
/*
                unsafe {
                    use glium::glutin::GlContext;
                    window.display.gl_window().make_current().unwrap();
                }
*/
                let window_id = WindowId { id: idx };
                let mut frame_event_info = FrameEventInfo::default();

                let mut events = Vec::new();
                window.events_loop.poll_events(|e| events.push(e));

                for event in &events {
                    if preprocess_event(event, &mut frame_event_info) == WindowCloseEvent::AboutToClose {
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
                            use compositor::{TO_DELETE_TEXTURES, ACTIVE_GL_TEXTURES};
                            let mut to_delete_lock = TO_DELETE_TEXTURES.lock().unwrap();
                            let mut active_textures_lock = ACTIVE_GL_TEXTURES.lock().unwrap();
                            to_delete_lock.drain().for_each(|tex| { active_textures_lock.remove(&tex); });
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
                    // Call the Layout::layout() fn, get the DOM
                    ui_state_cache[idx] = UiState::from_app_state(&self.app_state, WindowInfo {
                        window_id: WindowId { id: idx },
                        window: ReadOnlyWindow {
                            inner: window.display.clone(),
                        }
                    });
                    // Style the DOM
                    ui_description_cache[idx] = UiDescription::from_ui_state(&ui_state_cache[idx], &mut window.css);
                    // send webrender the size and buffer of the display
                    Self::update_display(&window);
                    // render the window (webrender will send an Awakened event when the frame is done)
                    render(window, &WindowId { id: idx }, &ui_description_cache[idx], &mut self.app_state.resources, true);
                }
            }

            // Close windows if necessary
            closed_windows.into_iter().for_each(|closed_window_id| {
                ui_state_cache.remove(closed_window_id);
                ui_description_cache.remove(closed_window_id);
                force_redraw_cache.remove(closed_window_id);
                self.windows.remove(closed_window_id);
            });

            // Run deamons and remove them from the even queue if they are finished
            self.app_state.run_all_deamons();

            // Clean up finished tasks, remove them if possible
            self.app_state.clean_up_finished_tasks();

            // Wait until 16ms have passed
            let diff = time_start.elapsed();
            const FRAME_TIME: Duration = Duration::from_millis(16);
            if diff < FRAME_TIME {
                thread::sleep(FRAME_TIME - diff);
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

    fn initialize_ui_state(windows: &[Window<T>], app_state: &AppState<'a, T>)
    -> Vec<UiState<T>>
    {
        use window::{ReadOnlyWindow, WindowInfo};

        windows.iter().enumerate().map(|(idx, w)|
            UiState::from_app_state(app_state, WindowInfo {
                window_id: WindowId { id: idx },
                window: ReadOnlyWindow {
                    inner: w.display.clone(),
                }
            })
        ).collect()
    }

    /// Add an image to the internal resources
    ///
    /// ## Returns
    ///
    /// - `Ok(Some(()))` if an image with the same ID already exists.
    /// - `Ok(None)` if the image was added, but didn't exist previously.
    /// - `Err(e)` if the image couldn't be decoded
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
    pub fn add_font<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R)
        -> Result<Option<()>, FontError>
    {
        self.app_state.add_font(id, data)
    }

    /// Checks if a font is currently registered and ready-to-use
    pub fn has_font<S: Into<String>>(&mut self, id: S)
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
    /// #     fn layout(&self, _window_id: WindowInfo) -> Dom<MyAppData> {
    /// #         Dom::new(NodeType::Div)
    /// #    }
    /// # }
    /// #
    /// # fn main() {
    /// let mut app = App::new(MyAppData { });
    /// app.add_font("Webly Sleeky UI", &mut TEST_FONT).unwrap();
    /// app.delete_font("Webly Sleeky UI");
    /// // NOTE: The font isn't immediately removed, only in the next draw call
    /// app.mock_render_frame();
    /// assert!(!app.has_font("Webly Sleeky UI"));
    /// # }
    /// ```
    ///
    /// [`AppState::delete_font`]: ../app_state/struct.AppState.html#method.delete_font
    pub fn delete_font<S: Into<String>>(&mut self, id: S)
        -> Option<()>
    {
        self.app_state.delete_font(id)
    }

    /// Create a deamon. Does nothing if a deamon with the same ID already exists.
    ///
    /// If the deamon was inserted, returns true, otherwise false
    pub fn add_deamon<S: Into<String>>(&mut self, id: S, deamon: fn(&mut T) -> UpdateScreen)
        -> bool
    {
        self.app_state.add_deamon(id, deamon)
    }

    /// Remove a currently running deamon from running. Does nothing if there is
    /// already a deamon with the same ID
    pub fn delete_deamon<S: AsRef<str>>(&mut self, id: S)
        -> bool
    {
        self.app_state.delete_deamon(id)
    }

    pub fn add_text_uncached<S: Into<String>>(&mut self, text: S)
    -> TextId
    {
        self.app_state.add_text_uncached(text)
    }

    pub fn add_text_cached<S: AsRef<str>>(&mut self, text: S, font_id: &FontId, font_size: PixelValue)
    -> TextId
    {
        self.app_state.add_text_cached(text, font_id, font_size)
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
        self.create_window(hidden_create_options, Css::native()).unwrap();
        // TODO: do_first_redraw shouldn't exist, need to find a better way to update the resources
        // This will make App::delete_font doc-test fail if run without `no-opengl-tests`.
        //
        // let ui_state_cache = Self::initialize_ui_state(&self.windows, &self.app_state);
        // Self::do_first_redraw(&mut self.windows, &mut self.app_state, &ui_state_cache);
    }
}

impl<'a, T: Layout + Send + 'static> App<'a, T> {
    /// Tasks, once started, cannot be stopped, which is why there is no `.delete()` function
    pub fn add_task(&mut self, callback: fn(Arc<Mutex<T>>, Arc<()>))
    {
        self.app_state.add_task(callback);
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
enum WindowCloseEvent {
    AboutToClose,
    NoCloseEvent,
}

fn preprocess_event(event: &Event, frame_event_info: &mut FrameEventInfo) -> WindowCloseEvent {
    use glium::glutin::WindowEvent;
    use glium::glutin::dpi::LogicalSize;

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
                _ => { },
            }
        },
        Event::Awakened => {
            frame_event_info.should_swap_window = true;
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

    let (cursor_x, cursor_y) = window.state.mouse_state.cursor_pos
        .and_then(|pos| {
            let physical_position = pos.to_physical(window.state.size.hidpi_factor);
            Some((physical_position.x as f32, physical_position.y as f32))
        }).unwrap_or((0.0, 0.0));

    let point = WorldPoint::new(cursor_x, cursor_y);

    let hit_test_results =  window.internal.api.hit_test(
        window.internal.document_id,
        Some(window.internal.pipeline_id),
        point,
        HitTestFlags::FIND_ALL);

    let mut should_update_screen = UpdateScreen::DontRedraw;

    let callbacks_filter_list = window.state.determine_callbacks(event);
    // TODO: this should be refactored - currently very stateful and error-prone!
    app_state.windows[window_id.id].set_keyboard_state(&window.state.keyboard_state);
    app_state.windows[window_id.id].set_mouse_state(&window.state.mouse_state);

    // NOTE: for some reason hit_test_results is empty...
    // ... but only when the mouse is relased - possible timing issue?
    for (item, callback_list) in hit_test_results.items.iter().filter_map(|item|
        ui_state_cache[window_id.id].node_ids_to_callbacks_list
        .get(&item.tag.0)
        .and_then(|callback_list| Some((item, callback_list)))
    ) {
        // TODO: currently we don't have information about what DOM node was hit
        let window_event = WindowEvent {
            window: window_id.id,
            number_of_previous_siblings: None,
            cursor_relative_to_item: (item.point_in_viewport.x, item.point_in_viewport.y),
            cursor_in_viewport: (item.point_in_viewport.x, item.point_in_viewport.y),
        };

        // Invoke callback if necessary
        for callback_id in callbacks_filter_list.iter().filter_map(|on| callback_list.get(on)) {
            let Callback(callback_func) = ui_state_cache[window_id.id].callback_list[callback_id];
            if (callback_func)(app_state, window_event) == UpdateScreen::Redraw {
                should_update_screen = UpdateScreen::Redraw;
            }
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
    }
}

fn render<T: Layout>(
    window: &mut Window<T>,
    _window_id: &WindowId,
    ui_description: &UiDescription<T>,
    app_resources: &mut AppResources,
    has_window_size_changed: bool)
{
    use webrender::api::*;
    use display_list::DisplayList;
    use euclid::TypedSize2D;

    let display_list = DisplayList::new_from_ui_description(ui_description);
    let builder = display_list.into_display_list_builder(
        window.internal.pipeline_id,
        &mut window.solver,
        &mut window.css,
        app_resources,
        &window.internal.api,
        has_window_size_changed,
        &window.state.size);

    if let Some(new_builder) = builder {
        // only finalize the list if we actually need to. Otherwise just redraw the last display list
        window.internal.last_display_list_builder = new_builder.finalize().2;
    }

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

    txn.set_root_pipeline(window.internal.pipeline_id);
    txn.generate_frame();

    window.internal.api.send_transaction(window.internal.document_id, txn);
    window.renderer.as_mut().unwrap().update();

    render_inner(window, framebuffer_size);
}

// See: https://github.com/servo/webrender/pull/2880
// webrender doesn't reset the active shader back to what it was, but rather sets it
// to zero, which glium doesn't know about, so on the next frame it tries to draw with shader 0
fn render_inner<T: Layout>(window: &mut Window<T>, framebuffer_size: TypedSize2D<u32, DevicePixel>) {

    use glium::backend::Facade;
    use gleam::gl;
    use window::get_gl_context;

    let mut current_program = [0_i32];
    unsafe { get_gl_context(&window.display).unwrap().get_integer_v(gl::CURRENT_PROGRAM, &mut current_program) };
    window.renderer.as_mut().unwrap().render(framebuffer_size).unwrap();
    get_gl_context(&window.display).unwrap().use_program(current_program[0] as u32);
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_app_file() {

}

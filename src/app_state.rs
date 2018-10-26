use std::{
    io::Read,
    collections::hash_map::Entry::*,
    sync::{Arc, Mutex},
    rc::Rc,
};
#[cfg(feature = "image_loading")]
use image::ImageError;
#[cfg(feature = "image_loading")]
use images::ImageType;
use rusttype::Font;
use {
    FastHashMap,
    text_cache::TextId,
    window::FakeWindow,
    task::Task,
    dom::UpdateScreen,
    traits::Layout,
    app_resources::AppResources,
    font::FontError,
    css_parser::{FontId, StyleFontSize, PixelValue, StyleLetterSpacing},
    error::ClipboardError,
    daemon::{Daemon, DaemonId, TerminateDaemon},
};

/// Wrapper for your application data, stores the data, windows and resources, as
/// well as running daemons and asynchronous tasks.
///
/// In order to be layout-able, your data model needs to satisfy the `Layout` trait,
/// which maps the state of your application to a DOM (how the application data should be laid out)
pub struct AppState<T: Layout> {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: Arc<Mutex<T>>,
    /// This field represents the state of the windows, public to the user. You can
    /// mess around with the state as you like, however, the actual window won't update
    /// until the next frame. This is done to "decouple" the frameworks internal
    /// state updating logic from the user code (and to make the API future-proof
    /// in case extra functions are introduced).
    ///
    /// Another reason this is needed is to (later) introduce testing for the window
    /// state - if the API would directly modify the window itself, these changes
    /// wouldn't be recorded anywhere, so there wouldn't be a way to unit-test certain APIs.
    ///
    /// This also allows you to dynamically override CSS values (to conditionally
    /// style your elements at runtime, for example on a `On::Hover` event).
    ///
    /// The state of these `FakeWindow`s gets deleted and recreated on each frame, especially
    /// the CSS. This should force a user to design his code in a functional way,
    /// without relying on state-based conditions. Example:
    ///
    /// ```no_run,ignore
    /// let window_state = &mut app_state.windows[event.window];
    /// // For one frame, set the dynamic CSS value with `my_id` to `color: orange`
    /// window_state.css.set_dynamic_property("my_id", ("color", "orange")).unwrap();
    /// // Update the title
    /// window_state.state.title = "Hello";
    /// ```
    pub windows: Vec<FakeWindow<T>>,
    /// Fonts, images and cached text that is currently loaded inside the app (window-independent).
    ///
    /// Accessing this field is often required to load new fonts or images, so instead of
    /// requiring the `FontHashMap`, a lot of functions just require the whole `AppResources` field.
    pub resources: AppResources,
    /// Currently running daemons (polling functions, run on the main thread)
    pub(crate) daemons: FastHashMap<DaemonId, Daemon<T>>,
    /// Currently running tasks (asynchronous functions running each on a different thread)
    pub(crate) tasks: Vec<Task<T>>,
}

/// Same as the [AppState](./struct.AppState.html) but without the
/// `self.data` field - used for default callbacks, so that callbacks can
/// load and unload fonts or images + access the system clipboard
///
/// Default callbacks don't have access to the `AppState.data` field,
/// since they use a `StackCheckedPointer` instead.
pub struct AppStateNoData<'a, T: 'a + Layout> {
    /// See [`AppState.windows`](./struct.AppState.html#structfield.windows)
    pub windows: &'a Vec<FakeWindow<T>>,
    /// See [`AppState.resources`](./struct.AppState.html#structfield.resources)
    pub resources : &'a mut AppResources,
}

impl<T: Layout> AppState<T> {

    /// Creates a new `AppState`
    pub fn new(initial_data: T) -> Self {
        Self {
            data: Arc::new(Mutex::new(initial_data)),
            windows: Vec::new(),
            resources: AppResources::default(),
            daemons: FastHashMap::default(),
            tasks: Vec::new(),
        }
    }

    /// Add an image to the internal resources.
    ///
    /// ## Arguments
    ///
    /// - `id`: A stringified ID (hash) for the image. It's recommended to use the
    ///         file path as the hash, maybe combined with a timestamp or a hash
    ///         of the file contents if the image will change.
    /// - `data`: The data of the image - can be a File, a network stream, etc.
    /// - `image_type`: If you know the type of image that you are adding, it is
    ///                 recommended to specify it. In case you don't know, use
    ///                 [`ImageType::GuessImageFormat`]
    ///
    /// ## Returns
    ///
    /// - `Ok(Some(()))` if an image with the same ID already exists.
    /// - `Ok(None)` if the image was added, but didn't exist previously.
    /// - `Err(e)` if the image couldn't be decoded
    ///
    /// **NOTE:** This function blocks the current thread.
    ///
    /// [`ImageType::GuessImageFormat`]: ../prelude/enum.ImageType.html#variant.GuessImageFormat
    ///
    #[cfg(feature = "image_loading")]
    pub fn add_image<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R, image_type: ImageType)
        -> Result<Option<()>, ImageError>
    {
        self.resources.add_image(id, data, image_type)
    }

    /// Checks if an image is currently registered and ready-to-use
    pub fn has_image<S: AsRef<str>>(&mut self, id: S)
        -> bool
    {
        self.resources.has_image(id)
    }

    /// Removes an image from the internal app resources.
    /// Returns `Some` if the image existed and was removed.
    /// If the given ID doesn't exist, this function does nothing and returns `None`.
    pub fn delete_image<S: AsRef<str>>(&mut self, id: S)
        -> Option<()>
    {
        self.resources.delete_image(id)
    }

    /// Add a font (TTF or OTF) to the internal resources
    ///
    /// ## Arguments
    ///
    /// - `id`: The stringified ID of the font to add, e.g. `"Helvetica-Bold"`.
    /// - `data`: The bytes of the .ttf or .otf font file. Can be anything
    ///    that is read-able, i.e. a File, a network stream, etc.
    ///
    /// ## Returns
    ///
    /// - `Ok(Some(()))` if an font with the same ID already exists.
    /// - `Ok(None)` if the font was added, but didn't exist previously.
    /// - `Err(e)` if the font couldn't be decoded
    ///
    /// ## Example
    ///
    /// This function exists so you can add functions to the app-internal state
    /// at runtime in a [`Callback`](../dom/enum.Callback.html) function.
    ///
    /// Here is an example of how to add a font at runtime (when the app is already running):
    ///
    /// ```
    /// # use azul::prelude::*;
    /// const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/weblysleekuil.ttf");
    ///
    /// struct MyAppData { }
    ///
    /// impl Layout for MyAppData {
    ///      fn layout(&self, _window_id: WindowInfo<MyAppData>) -> Dom<MyAppData> {
    ///          Dom::new(NodeType::Div)
    ///             .with_callback(On::MouseEnter, Callback(my_callback))
    ///      }
    /// }
    ///
    /// fn my_callback(app_state: &mut AppState<MyAppData>, event: WindowEvent<MyAppData>) -> UpdateScreen {
    ///     /// Here you can add your font at runtime to the app_state
    ///     app_state.add_font(FontId::ExternalFont("Webly Sleeky UI".into()), &mut TEST_FONT).unwrap();
    ///     UpdateScreen::DontRedraw
    /// }
    /// ```
    pub fn add_font<R: Read>(&mut self, id: FontId, data: &mut R)
        -> Result<Option<()>, FontError>
    {
        self.resources.add_font(id, data)
    }

    /// Checks if a font is currently registered and ready-to-use
    pub fn has_font(&self, id: &FontId)
        -> bool
    {
        self.resources.has_font(id)
    }

    pub fn get_font(&self, id: &FontId) -> Option<(Rc<Font<'static>>, Rc<Vec<u8>>)> {
        self.resources.get_font(id)
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
    /// After this function has been
    /// called, you can be sure that the renderer doesn't know about your font anymore.
    /// This also means that the font needs to be re-parsed if you want to add it again.
    /// Use with care.
    ///
    /// You can also call this function on an `App` struct, see [`App::add_font`].
    ///
    /// [`App::add_font`]: ../app/struct.App.html#method.add_font
    pub fn delete_font(&mut self, id: &FontId)
        -> Option<()>
    {
        self.resources.delete_font(id)
    }

    /// Create a daemon. Does nothing if a daemon already exists.
    ///
    /// If the daemon was inserted, returns true, otherwise false
    pub fn add_daemon(&mut self, daemon: Daemon<T>) -> bool {
        match self.daemons.entry(daemon.id) {
            Occupied(_) => false,
            Vacant(v) => { v.insert(daemon); true },
        }
    }

    /// Run all currently registered daemons
    #[must_use]
    pub(crate) fn run_all_daemons(&mut self)
    -> UpdateScreen
    {
        let mut should_update_screen = UpdateScreen::DontRedraw;
        let mut lock = self.data.lock().unwrap();
        let mut daemons_to_terminate = Vec::new();

        for (key, daemon) in self.daemons.iter_mut() {
            let (should_update, should_terminate) = daemon.invoke_callback_with_data(&mut lock, &mut self.resources);

            if should_update == UpdateScreen::Redraw &&
               should_update_screen == UpdateScreen::DontRedraw {
                should_update_screen = UpdateScreen::Redraw;
            }

            if should_terminate == TerminateDaemon::Terminate {
                daemons_to_terminate.push(key.clone());
            }
        }

        for key in daemons_to_terminate {
            self.daemons.remove(&key);
        }

        should_update_screen
    }

    /// Remove all tasks that have finished executing
    #[must_use]
    pub(crate) fn clean_up_finished_tasks(&mut self)
    -> UpdateScreen
    {
        let old_count = self.tasks.len();
        let mut daemons_to_add = Vec::new();
        self.tasks.retain(|task| {
            if !task.is_finished() {
                true
            } else {
                daemons_to_add.extend(task.after_completion_daemons.iter().cloned());
                false
            }
        });

        let daemons_is_empty = daemons_to_add.is_empty();
        let new_count = self.tasks.len();

        // Start all the daemons that should run after the completion of the task
        for daemon in daemons_to_add {
            self.add_daemon(daemon);
        }

        if old_count == new_count && daemons_is_empty {
            UpdateScreen::DontRedraw
        } else {
            UpdateScreen::Redraw
        }
    }

    pub fn add_text_uncached<S: Into<String>>(&mut self, text: S)
    -> TextId
    {
        self.resources.add_text_uncached(text)
    }

    pub fn add_text_cached<S: Into<String>>(&mut self, text: S, font_id: &FontId, font_size: PixelValue, letter_spacing: Option<StyleLetterSpacing>)
    -> TextId
    {
        let font_size = StyleFontSize(font_size);
        self.resources.add_text_cached(text, font_id, font_size, letter_spacing)
    }

    pub fn delete_text(&mut self, id: TextId) {
        self.resources.delete_text(id);
    }

    pub fn clear_all_texts(&mut self) {
        self.resources.clear_all_texts();
    }

    /// Get the contents of the system clipboard as a string
    pub fn get_clipboard_string(&mut self)
    -> Result<String, ClipboardError>
    {
        self.resources.get_clipboard_string()
    }

    /// Set the contents of the system clipboard as a string
    pub fn set_clipboard_string(&mut self, contents: String)
    -> Result<(), ClipboardError>
    {
        self.resources.set_clipboard_string(contents)
    }

    /// Custom tasks can be used when the `AppState` isn't `Send`. For example
    /// `SvgCache` isn't thread-safe, since it has to interact with OpenGL, so
    /// it can't be sent to other threads safely.
    ///
    /// What you can do instead, is take a part of your application data, wrap
    /// that in an `Arc<Mutex<>>` and push a task that takes it onto the queue.
    /// This way you can modify a part of the application state on a different
    /// thread, while not requiring that everything is thread-safe.
    ///
    /// While you can't modify the `SvgCache` from a different thread, you can
    /// modify other things in the `AppState` and leave the SVG cache alone.
    pub fn add_custom_task<U: Send + 'static>(
        &mut self,
        data: &Arc<Mutex<U>>,
        callback: fn(Arc<Mutex<U>>, Arc<()>),
        after_completion_deamons: &[Daemon<T>])
    {
        let task = Task::new(data, callback).then(after_completion_deamons);
        self.tasks.push(task);
    }
}

impl<T: Layout + Send + 'static> AppState<T> {
    /// Add a task that has access to the entire `AppState`.
    pub fn add_task(
        &mut self,
        callback: fn(Arc<Mutex<T>>, Arc<()>),
        after_completion_deamons: &[Daemon<T>])
    {
        let task = Task::new(&self.data, callback).then(after_completion_deamons);
        self.tasks.push(task);
    }
}

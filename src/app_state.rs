use traits::LayoutScreen;
use resources::{AppResources};
use std::io::Read;
use images::ImageType;
use image::ImageError;
use font::FontError;
use std::collections::hash_map::Entry::*;
use FastHashMap;
use deamon::DeamonCallback;

/// Wrapper for your application data. In order to be layout-able,
/// you need to satisfy the `LayoutScreen` trait (how the application
/// should be laid out)
pub struct AppState<'a, T: LayoutScreen> {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: T,
    /// Fonts and images that are currently loaded into the app
    pub(crate) resources: AppResources<'a>,
    /// Currently running deamons (polling functions)
    pub(crate) deamons: FastHashMap<String, DeamonCallback<T>>,
}

impl<'a, T: LayoutScreen> AppState<'a, T> {

    /// Creates a new `AppState`
    pub fn new(initial_data: T) -> Self {
        Self {
            data: initial_data,
            resources: AppResources::default(),
            deamons: FastHashMap::default(),
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
    /// impl LayoutScreen for MyAppData {
    ///      fn get_dom(&self, _window_id: WindowId) -> Dom<MyAppData> {
    ///          let mut dom = Dom::new(NodeType::Div);
    ///          dom.event(On::MouseEnter, Callback::Sync(my_callback));
    ///          dom
    ///      }
    /// }
    ///
    /// fn my_callback(app_state: &mut AppState<MyAppData>) -> UpdateScreen {
    ///     /// Here you can add your font at runtime to the app_state
    ///     app_state.add_font("Webly Sleeky UI", &mut TEST_FONT).unwrap();
    ///     UpdateScreen::DontRedraw
    /// }
    /// ```
    pub fn add_font<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R)
        -> Result<Option<()>, FontError>
    {
        self.resources.add_font(id, data)
    }

    /// Checks if a font is currently registered and ready-to-use
    pub fn has_font<S: Into<String>>(&mut self, id: S)
        -> bool
    {
        self.resources.has_font(id)
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
    pub fn delete_font<S: Into<String>>(&mut self, id: S)
        -> Option<()>
    {
        self.resources.delete_font(id)
    }

    /// Create a deamon. Does nothing if a deamon with the same ID already exists.
    ///
    /// If the deamon was inserted, returns true, otherwise false
    pub fn add_deamon<S: Into<String>>(&mut self, id: S, deamon: DeamonCallback<T>) -> bool {
        let id_string = id.into();
        match self.deamons.entry(id_string) {
            Occupied(_) => false,
            Vacant(v) => { v.insert(deamon); true },
        }
    }

    /// Remove a currently running deamon from running. Does nothing if there is
    /// already a deamon with the same ID
    pub fn delete_deamon<S: AsRef<String>>(&mut self, id: S) -> bool {
        self.deamons.remove(id.as_ref()).is_some()
    }
}

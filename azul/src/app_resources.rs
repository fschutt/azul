use std::{
    fmt,
    rc::Rc,
    cell::RefCell,
    path::PathBuf,
    io::Error as IoError,
    collections::hash_map::Entry::*,
};
use webrender::api::{FontKey, ImageData, ImageDescriptor, FontInstanceKey};
pub use webrender::api::ImageFormat as RawImageFormat;
#[cfg(feature = "image_loading")]
use image::ImageError;
use FastHashMap;
use app_units::Au;
use clipboard2::{Clipboard, ClipboardError, SystemClipboard};
use rusttype::Font;
use azul_css::{PixelValue, StyleLetterSpacing};
use {
    text_layout::{split_text_into_words, TextSizePx},
    text_cache::{TextId, TextCache},
    font::{FontState, FontError},
    images::{ImageId, ImageState},
    window::{FakeDisplay, WindowCreateError},
    app::AppConfig,
};

pub type CssImageId = String;
pub type CssFontId = String;

static IMAGE_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ImageId {
    id: usize,
}

impl ImageId {
    pub(crate) fn new() -> Self {
        let unique_id = IMAGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            id: unique_id,
        }
    }
}

static FONT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontId {
    id: usize,
}

impl FontId {
    pub(crate) fn new() -> Self {
        let unique_id = FONT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self {
            id: unique_id,
        }
    }
}

/// Since the code for "FontSource" and "ImageSource" is pretty much the same
/// this generates the different structs for "FontSource", "FontReloadError",
/// "ImageSource" and "ImageReloadError"
macro_rules! external_data_source {($image_source:ident, $image_reload_error:ident) => (

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub enum $image_source {
        /// The image is embedded inside the binary file
        Embedded(&'static [u8]),
        File(PathBuf),
    }

    #[derive(Debug)]
    pub enum $image_reload_error {
        Io(IoError, PathBuf),
    }

    impl Clone for $image_reload_error {
        fn clone(&self) -> Self {
            use self::$image_reload_error::*;
            match self {
                Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
            }
        }
    }

    impl fmt::Display for $image_reload_error {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            use self::$image_reload_error::*;
            match self {
                Io(err, path_buf) =>
                write!(
                    f, "Could not load \"{}\" - IO error: {}",
                    path_buf.as_path().to_string_lossy(), err
                ),
            }
        }
    }

    impl $image_source {

        /// Creates an image source from a `&static [u8]`.
        pub fn new_from_static(bytes: &'static [u8]) -> Self {
            $image_source::Embedded(bytes)
        }

        /// Creates an image source from a file
        pub fn new_from_file<I: Into<PathBuf>>(file_path: I) -> Self {
            $image_source::File(file_path.into())
        }

        /// Returns the bytes of the font
        pub(crate) fn get_bytes(&self) -> Result<Vec<u8>, ImageReloadError> {
            use std::fs;
            use self::$image_source::*;
            match self {
                Embedded(bytes) => Ok(bytes.to_vec()),
                File(file_path) => fs::read(file_path).map_err(|e| ImageReloadError::Io(e, file_path.clone())),
            }
        }
    }
)}

external_data_source!(ImageSource, ImageReloadError);
external_data_source!(FontSource, FontReloadError);

/// Raw
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImage {
    pixels: Vec<u8>,
    image_dimensions: (u32, u32),
    data_format: RawImageFormat,
}

/// Stores the resources for the application, souch as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
pub struct AppResources {
    /// The CssImageId is the string used in the CSS, i.e. "my_image" -> ImageId(4)
    pub(crate) css_ids_to_image_ids: FastHashMap<CssImageId, ImageId>,
    /// Stores where the images were loaded from
    pub(crate) images: FastHashMap<ImageId, ImageSource>,
    /// Raw images are the same as
    pub(crate) raw_images: FastHashMap<ImageId, RawImage>,
    /// Same as CssImageId -> ImageId, but for fonts, i.e. "Roboto" -> FontId(9)
    pub(crate) css_ids_to_font_ids: FastHashMap<CssFontId, FontId>,
    /// Stores where the fonts were loaded from
    pub(crate) fonts: FastHashMap<FontId, FontSource>,
    /// If a font does not get used for one frame, the corresponding instance key gets
    /// deleted. If a FontId has no FontInstanceKeys anymore, the font key gets deleted.
    ///
    /// The only thing remaining in memory permanently is the FontSource (which is only
    /// the string of the file path where the font was loaded from, so no huge memory pressure).
    /// The reason for this agressive strategy is that the
    pub(crate) last_frame_font_keys: FastHashMap<FontId, FastHashMap<Au, FontInstanceKey>>,
    /// Same thing for images: If the image isn't displayed, it is deleted from memory, only
    /// the `ImageSource` (i.e. the path / source where the image was loaded from) remains.
    ///
    /// This way the image can be re-loaded if necessary but doesn't have to reside in memory at all times.
    pub(crate) last_frame_image_keys: FastHashSet<ImageId>,
    /// Stores long texts across frames
    pub(crate) text_cache: TextCache,
    /// In order to properly load / unload fonts and images as well as share resources
    /// between windows, this field stores the (application-global) Renderer.
    pub(crate) fake_display: FakeDisplay,
    /// Keyboard clipboard storage and retrieval functionality
    clipboard: SystemClipboard,
}

impl AppResources {
    /// Creates a new renderer (the renderer manages the resources and is therfore tied to the resources).
    fn new(app_config: &AppConfig) -> Result<Self, WindowCreateError> {
        Ok(Self {
            css_ids_to_image_ids: FastHashMap::default(),
            images: FastHashMap::default(),
            raw_images: FastHashMap::default(),
            css_ids_to_font_ids: FastHashMap::default(),
            fonts: FastHashMap::default(),
            last_frame_font_keys: FastHashMap::default(),
            last_frame_image_keys: FastHashSet::default(),
            text_cache: TextCache::default(),
            fake_display: FakeDisplay::new(app_config.renderer_type, &app_config.debug_state, app_config.background_color)?,
            clipboard: SystemClipboard::new().unwrap(),
        })
    }
}

impl AppResources {

    /// Returns the IDs of all currently loaded fonts in `self.font_data`
    pub fn get_loaded_font_ids(&self) -> Vec<FontId> {
        self.font_data.borrow().keys().cloned().collect()
    }

    pub fn get_loaded_image_ids(&self) -> Vec<ImageId> {
        self.images.keys().cloned().collect()
    }

    pub fn get_loaded_css_image_ids(&self) -> Vec<CssImageId> {
        self.css_ids_to_image_ids.keys().cloned().collect()
    }

    pub fn get_loaded_css_font_ids(&self) -> Vec<CssFontId> {
        self.css_ids_to_font_ids.keys().cloned().collect()
    }

    pub fn get_loaded_text_ids(&self) -> Vec<TextId> {
        let mut text_ids = Vec::new();
        text_ids.extend(self.text_cache.string_cache.keys().cloned());
        text_ids.extend(self.text_cache.layouted_strings_cache.keys().cloned());
        text_ids
    }

    // -- ImageId cache

    /// Add an image from a PNG, JPEG or other - note that for specialized image formats,
    /// you have to enable them as features in the Cargo.toml file.
    ///
    /// ### Returns
    ///
    /// - `Some(())` if the image was inserted correctly
    /// - `None` if the ImageId already exists (you have to delete the image first using `.delete_image()`)
    #[cfg(feature = "image_loading")]
    pub fn add_image(&mut self, image_id: ImageId, image_source: ImageSource) -> Option<()> {
        match self.images.entry(image_id) {
            Occupied(_) => None,
            Vacant(v) => {
                v.insert(image_source);
                Some(())
            }
        }
    }

    /// Add raw image data (directly from a Vec<u8>) in BRGA8 or A8 format
    ///
    /// ### Returns
    ///
    /// - `Some(())` if the image was inserted correctly
    /// - `None` if the ImageId already exists (you have to delete the image first using `.delete_image()`)
    pub fn add_image_raw(&mut self, image_id: ImageId, image: RawImage) -> Option<()> {

        use images; // the module, not the crate!

        match self.raw_images.entry(image_id) {
            Occupied(_) => None,
            Vacant(v) => {
                v.insert(image);
                Some(())
            }
        }
    }

    /// See [`AppState::has_image()`](../app_state/struct.AppState.html#method.has_image)
    pub fn has_image(&self, image_id: &ImageId) -> bool {
        self.images.get(image_id).is_some()
    }

    pub fn delete_image(&mut self, image_id: ImageId) -> Option<()> {
        match self.images.get_mut(&image_id) {
            None => None,
            Some(v) => {
                let to_delete_image_key = match *v {
                    ImageState::Uploaded(ref image_info) => {
                        Some((Some(image_info.key.clone()), image_info.descriptor.clone()))
                    },
                    _ => None,
                };
                if let Some((key, descriptor)) = to_delete_image_key {
                    *v = ImageState::AboutToBeDeleted((key, descriptor));
                }
                Some(())
            }
        }
    }

    pub fn add_css_image_id<S: Into<String>>(&mut self, css_id: S) -> ImageId {
        *self.css_ids_to_image_ids.entry(css_id.into()).or_insert_with(|| ImageId::new())
    }

    pub fn has_css_image_id<S: AsRef<str>>(&self, css_id: S) -> bool {
        self.get_css_image_id(css_id).is_some()
    }

    /// Returns the ImageId for a given CSS ID - the CSS ID is what you added your image as:
    ///
    /// ```no_run,ignore
    /// let image_id = app_resources.add_image("test", include_bytes!("./my_image.ttf"));
    /// ```
    pub fn get_css_image_id<S: AsRef<str>>(&self, css_id: S) -> Option<ImageId> {
        self.css_ids_to_image_ids.get(css_id.as_ref()).cloned()
    }

    pub fn delete_css_image_id<S: AsRef<str>>(&mut self, css_id: S) -> Option<ImageId> {
        self.css_ids_to_image_ids.remove(css_id.as_ref())
    }

    // -- FontId cache

    pub fn add_font<I: Into<Vec<u8>>>(&mut self, id: FontId, font_data_bytes: I) -> Result<Option<()>, FontError> {
        use font;

        match self.font_data.borrow_mut().entry(id) {
            Occupied(_) => Ok(None),
            Vacant(v) => {
                let font_data = font_data_bytes.into();
                let (parsed_font, fd) = font::rusttype_load_font(font_data.clone(), None)?;
                v.insert((Rc::new(parsed_font), Rc::new(fd), Rc::new(RefCell::new(FontState::ReadyForUpload(font_data)))));
                Ok(Some(()))
            },
        }
    }

    /// Given a `FontId`, returns the `Font` and the original bytes making up the font
    /// or `None`, if the `FontId` is invalid.
    pub fn get_font(&self, id: &FontId) -> Option<(Rc<Font<'static>>, Rc<Vec<u8>>)> {
        self.get_font_internal(id).and_then(|(font, bytes, _)| Some((font, bytes)))
    }

    /// Note the pub(crate) here: We don't want to expose the FontState in the public API
    pub(crate) fn get_font_state(&self, id: &FontId) -> Option<Rc<RefCell<FontState>>> {
        self.get_font_internal(id).and_then(|(_, _, state)| Some(state))
    }

    /// Checks if a `FontId` is valid, i.e. if a font is currently ready-to-use
    pub fn has_font(&self, id: &FontId) -> bool {
        self.font_data.borrow().get(id).is_some()
    }

    pub fn delete_font(&mut self, id: &FontId) -> Option<()> {
        // TODO: can fonts that haven't been uploaded yet be deleted?
        let mut to_delete_font_key = None;

        match self.font_data.borrow().get(&id) {
            None => return None,
            Some(v) => match *(*v.2).borrow() {
                FontState::Uploaded(font_key) => { to_delete_font_key = Some(font_key.clone()); },
                _ => { },
            }
        }

        let mut borrow_mut = self.font_data.borrow_mut();
        *borrow_mut.get_mut(&id).unwrap().2.borrow_mut() = FontState::AboutToBeDeleted(to_delete_font_key);
        Some(())
    }

    // -- TextId cache

    /// Adds a string to the internal text cache, but only store it as a string,
    /// without caching the layout of the string.
    pub fn add_text_uncached<S: Into<String>>(&mut self, text: S) -> TextId {
        self.text_cache.add_text(text)
    }

    /// Calculates the widths for the words (layouts the string), then stores
    /// them in a text cache, together with the actual string
    ///
    /// This leads to a faster layout cycle, but has an upfront performance cost
    pub fn add_text_cached<S: Into<String>>(
        &mut self,
        text: S,
        font_id: &FontId,
        font_size: PixelValue,
        letter_spacing: Option<StyleLetterSpacing>
    ) -> TextId {
        // First, insert the text into the text cache
        let id = self.add_text_uncached(text);
        self.cache_text(id, font_id.clone(), font_size, letter_spacing);
        id
    }

    /// Promotes an uncached text (i.e. a text that was added via `add_text_uncached`)
    /// to a cached text by calculating the font metrics for the uncached text.
    /// This will not delete the original text!
    pub fn cache_text(&mut self, id: TextId, font: FontId, size: PixelValue, letter_spacing: Option<StyleLetterSpacing>) {
        // We need to assume that the actual string contents have already been stored in self.text_cache
        // Otherwise, how would the TextId be valid?
        let text = self.text_cache.string_cache.get(&id).expect("Invalid text Id");
        let font_size_no_line_height = TextSizePx(size.to_pixels());
        let rusttype_font = self.get_font(&font).expect("Invalid font ID");
        let words = split_text_into_words(text.as_ref(), &rusttype_font.0, font_size_no_line_height, letter_spacing);

        self.text_cache.layouted_strings_cache
            .entry(id).or_insert_with(|| FastHashMap::default())
            .entry(font).or_insert_with(|| FastHashMap::default())
            .insert(size, words);
    }

    /// Removes a string from both the string cache and the layouted text cache
    pub fn delete_text(&mut self, id: TextId) {
        self.text_cache.delete_text(id);
    }

    /// Removes a string from the string cache, but not the layouted text cache
    pub fn delete_string(&mut self, id: TextId) {
        self.text_cache.delete_string(id);
    }

    /// Removes a string from the layouted text cache, but not the string cache
    pub fn delete_layouted_text(&mut self, id: TextId) {
        self.text_cache.delete_layouted_text(id);
    }

    /// Empties the entire internal text cache, invalidating all `TextId`s. Use with care.
    pub fn clear_all_texts(&mut self) {
        self.text_cache.clear_all_texts();
    }

    // -- Clipboard

    /// Returns the contents of the system clipboard
    pub fn get_clipboard_string(&self) -> Result<String, ClipboardError> {
        self.clipboard.get_string_contents()
    }

    /// Sets the contents of the system clipboard - currently only strings are supported
    pub fn set_clipboard_string<S: Into<String>>(&mut self, contents: S) -> Result<(), ClipboardError> {
        self.clipboard.set_string_contents(contents.into())
    }

    // -- Helper functions

    /// Internal API - we want the user to get the first two fields of the
    fn get_font_internal(&self, id: &FontId) -> Option<(Rc<Font<'static>>, Rc<Vec<u8>>, Rc<RefCell<FontState>>)> {
        match id {
            FontId::BuiltinFont(b) => {
                if self.font_data.borrow().get(id).is_none() {
                    let (font, font_bytes, font_state) = get_builtin_font(b)?;
                    self.font_data.borrow_mut().insert(id.clone(), (Rc::new(font), Rc::new(font_bytes), Rc::new(RefCell::new(font_state))));
                }
                self.font_data.borrow().get(id).and_then(|(font, bytes, state)| Some((font.clone(), bytes.clone(), state.clone())))
            },
            FontId::ExternalFont(_) => {
                // For external fonts, we assume that the application programmer has
                // already loaded them, so we don't try to fallback to system fonts.
                self.font_data.borrow().get(id).and_then(|(font, bytes, state)| Some((font.clone(), bytes.clone(), state.clone())))
            },
        }
    }
}

/// Search for a builtin font on the users computer, validate and return it
fn get_builtin_font(id: &str) -> Option<(::rusttype::Font<'static>, Vec<u8>, FontState)>
{
    use font_loader::system_fonts::{self, FontPropertyBuilder};
    use font::rusttype_load_font;

    let (font_bytes, idx) = system_fonts::get(&FontPropertyBuilder::new().family(id).build())?;
    let (f, b) = rusttype_load_font(font_bytes.clone(), Some(idx)).ok()?;
    Some((f, b, FontState::ReadyForUpload(font_bytes)))
}

#[cfg(feature = "image_loading")]
fn decode_image_data<I: Into<Vec<u8>>>(image_data: I)
-> Result<(ImageData, ImageDescriptor), ImageError>
{
    use image; // the crate
    use images; // the module

    let image_data = image_data.into();
    let image_format = image::guess_format(&image_data)?;
    let decoded = image::load_from_memory_with_format(&image_data, image_format)?;
    Ok(images::prepare_image(decoded)?)
}


/// Looks if any new images need to be uploaded and stores the in the image resources
fn update_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    update_image_resources(api, app_resources, resource_updates);
    update_font_resources(api, app_resources, resource_updates);
}

fn update_image_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    use images::{ImageState, ImageInfo};

    let mut updated_images = Vec::<(ImageId, (ImageData, ImageDescriptor))>::new();
    let mut to_delete_images = Vec::<(ImageId, Option<ImageKey>)>::new();

    // possible performance bottleneck (duplicated cloning) !!
    for (key, value) in app_resources.images.iter() {
        match *value {
            ImageState::ReadyForUpload(ref d) => {
                updated_images.push((key.clone(), d.clone()));
            },
            ImageState::Uploaded(_) => { },
            ImageState::AboutToBeDeleted((ref k, _)) => {
                to_delete_images.push((key.clone(), k.clone()));
            }
        }
    }

    // Remove any images that should be deleted
    for (resource_key, image_key) in to_delete_images.into_iter() {
        if let Some(image_key) = image_key {
            resource_updates.push(ResourceUpdate::DeleteImage(image_key));
        }
        app_resources.images.remove(&resource_key);
    }

    // Upload all remaining images to the GPU only if the haven't been
    // uploaded yet
    for (resource_key, (data, descriptor)) in updated_images.into_iter() {

        let key = api.generate_image_key();
        resource_updates.push(ResourceUpdate::AddImage(
            AddImage { key, descriptor, data, tiling: None }
        ));

        *app_resources.images.get_mut(&resource_key).unwrap() =
            ImageState::Uploaded(ImageInfo {
                key: key,
                descriptor: descriptor
        });
    }
}

// almost the same as update_image_resources, but fonts
// have two HashMaps that need to be updated
fn update_font_resources(
    api: &RenderApi,
    app_resources: &mut AppResources,
    resource_updates: &mut Vec<ResourceUpdate>)
{
    use font::FontState;
    use azul_css::FontId;

    let mut updated_fonts = Vec::<(FontId, Vec<u8>)>::new();
    let mut to_delete_fonts = Vec::<(FontId, Option<(FontKey, Vec<FontInstanceKey>)>)>::new();

    for (key, value) in app_resources.font_data.borrow().iter() {
        match &*(*value.2).borrow() {
            FontState::ReadyForUpload(ref bytes) => {
                updated_fonts.push((key.clone(), bytes.clone()));
            },
            FontState::Uploaded(_) => { },
            FontState::AboutToBeDeleted(ref font_key) => {
                let to_delete_font_instances = font_key.and_then(|f_key| {
                    let to_delete_font_instances = app_resources.fonts[&f_key].values().cloned().collect();
                    Some((f_key.clone(), to_delete_font_instances))
                });
                to_delete_fonts.push((key.clone(), to_delete_font_instances));
            }
        }
    }

    // Delete the complete font. Maybe a more granular option to
    // keep the font data in memory should be added later
    for (resource_key, to_delete_instances) in to_delete_fonts.into_iter() {
        if let Some((font_key, font_instance_keys)) = to_delete_instances {
            for instance in font_instance_keys {
                resource_updates.push(ResourceUpdate::DeleteFontInstance(instance));
            }
            resource_updates.push(ResourceUpdate::DeleteFont(font_key));
            app_resources.fonts.remove(&font_key);
        }
        app_resources.font_data.borrow_mut().remove(&resource_key);
    }

    // Upload all remaining fonts to the GPU only if the haven't been uploaded yet
    for (resource_key, data) in updated_fonts.into_iter() {
        let key = api.generate_font_key();
        resource_updates.push(ResourceUpdate::AddFont(AddFont::Raw(key, data, 0))); // TODO: use the index better?
        let mut borrow_mut = app_resources.font_data.borrow_mut();
        *borrow_mut.get_mut(&resource_key).unwrap().2.borrow_mut() = FontState::Uploaded(key);
    }
}
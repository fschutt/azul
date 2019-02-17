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
use azul_css::{PixelValue, FontId, StyleLetterSpacing};
use {
    text_layout::{split_text_into_words, TextSizePx},
    text_cache::{TextId, TextCache},
    font::{FontState, FontError},
    images::{ImageId, ImageState},
};

pub type CssImageId = String;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImageSource {
    /// The image is embedded inside the binary file
    Embedded(&'static [u8]),
    File(PathBuf),
}

#[derive(Debug)]
pub enum ImageReloadError {
    Io(IoError, PathBuf),
}

impl Clone for ImageReloadError {
    fn clone(&self) -> Self {
        use self::ImageReloadError::*;
        match self {
            Io(err, path) => Io(IoError::new(err.kind(), "Io Error"), path.clone()),
        }
    }
}

impl fmt::Display for ImageReloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ImageReloadError::*;
        match self {
            Io(err, path_buf) =>
            write!(
                f, "Could not load \"{}\" - IO error: {}",
                path_buf.as_path().to_string_lossy(), err
            ),
        }
    }
}

impl ImageSource {

    /// Creates an image source from a `&static [u8]`.
    pub fn new_from_static(bytes: &'static [u8]) -> Self {
        ImageSource::Embedded(bytes)
    }

    /// Creates an image source from a file
    pub fn new_from_file<I: Into<PathBuf>>(file_path: I) -> Self {
        ImageSource::File(file_path.into())
    }

    /// Returns the bytes of the font
    pub(crate) fn get_bytes(&self) -> Result<Vec<u8>, ImageReloadError> {
        use std::fs;
        use self::ImageSource::*;
        match self {
            Embedded(bytes) => Ok(bytes.to_vec()),
            File(file_path) => fs::read(file_path).map_err(|e| ImageReloadError::Io(e, file_path.clone())),
        }
    }
}

/// Stores the resources for the application, souch as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
pub struct AppResources {
    /// When looking up images, there are two sources: Either the indirect way via using a
    /// CssImageId (which is a String) or a direct ImageId. The indirect way requires one
    /// extra lookup (to map from the stringified ID to the actual image ID).
    pub(crate) css_ids_to_image_ids: FastHashMap<CssImageId, ImageId>,
    /// The actual image cache, does NOT store the image data, only stores it temporarily
    /// while it is being uploaded to the GPU via webrender.
    pub(crate) images: FastHashMap<ImageId, ImageState>,
    // Fonts are trickier to handle than images.
    // First, we duplicate the font - webrender wants the raw font data,
    // but we also need access to the font metrics. So we first parse the font
    // to make sure that nothing is going wrong. In the next draw call, we
    // upload the font and replace the FontState with the newly created font key
    pub(crate) font_data: RefCell<FastHashMap<FontId, (Rc<Font<'static>>, Rc<Vec<u8>>, Rc<RefCell<FontState>>)>>,
    // After we've looked up the FontKey in the font_data map, we can then access
    // the font instance key (if there is any). If there is no font instance key,
    // we first need to create one.
    pub(crate) fonts: FastHashMap<FontKey, FastHashMap<Au, FontInstanceKey>>,
    /// Stores long texts across frames
    pub(crate) text_cache: TextCache,
    /// Keyboard clipboard storage and retrieval functionality
    clipboard: SystemClipboard,
}

impl Default for AppResources {
    fn default() -> Self {
        Self {
            css_ids_to_image_ids: FastHashMap::default(),
            fonts: FastHashMap::default(),
            font_data: RefCell::new(FastHashMap::default()),
            images: FastHashMap::default(),
            text_cache: TextCache::default(),
            clipboard: SystemClipboard::new().unwrap(),
        }
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

    pub fn get_loaded_css_ids(&self) -> Vec<CssImageId> {
        self.css_ids_to_image_ids.keys().cloned().collect()
    }

    pub fn get_loaded_text_ids(&self) -> Vec<TextId> {
        let mut text_ids = Vec::new();
        text_ids.extend(self.text_cache.string_cache.keys().cloned());
        text_ids.extend(self.text_cache.layouted_strings_cache.keys().cloned());
        text_ids
    }

    // -- ImageId cache


    #[cfg(feature = "image_loading")]
    pub fn add_image<I: Into<Vec<u8>>>(&mut self, id: ImageId, data: I) -> Result<(), ImageError> {
        match self.images.entry(id) {
            Occupied(_) => Ok(()),
            Vacant(v) => {
                v.insert(ImageState::ReadyForUpload(decode_image_data(data)?));
                Ok(())
            },
        }
    }

    /// Add raw image data (directly from a Vec<u8>) in BRGA8 or A8 format
    ///
    /// ### Returns
    ///
    /// - Some(()) if the image was inserted correctly
    /// - `None` if the ImageId already exists (you have to delete the image first using `.delete_image()`)
    pub fn add_image_raw(&mut self, image_id: ImageId, pixels: Vec<u8>, image_dimensions: (u32, u32), data_format: RawImageFormat) -> Option<()> {

        use images; // the module, not the crate!

        match self.images.entry(image_id) {
            Occupied(_) => None,
            Vacant(v) => {
                let opaque = images::is_image_opaque(data_format, &pixels[..]);
                let allow_mipmaps = true;
                let descriptor = ImageDescriptor::new(
                    image_dimensions.0 as i32,
                    image_dimensions.1 as i32,
                    data_format,
                    opaque,
                    allow_mipmaps
                );
                let data = ImageData::new(pixels);
                v.insert(ImageState::ReadyForUpload((data, descriptor)));
                Some(())
            },
        }
    }

    /// See [`AppState::has_image()`](../app_state/struct.AppState.html#method.has_image)
    pub fn has_image(&self, image_id: &ImageId) -> bool {
        self.images.get(&image_id).is_some()
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

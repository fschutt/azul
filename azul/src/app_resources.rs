use std::{
    io::Read,
    rc::Rc,
    cell::RefCell,
    collections::hash_map::Entry::*,
};
use webrender::api::{FontKey, FontInstanceKey};
#[cfg(feature = "image_loading")]
use image::{self, ImageError};
#[cfg(feature = "image_loading")]
use images::ImageType;
use FastHashMap;
use app_units::Au;
use clipboard2::{Clipboard, ClipboardError, SystemClipboard};
use rusttype::Font;
use azul_css::{StyleFontSize, FontId, StyleLetterSpacing};
use {
    text_layout::{split_text_into_words, TextSizePx},
    text_cache::{TextId, TextCache},
    font::{FontResourceUpdate, FontError},
    images::{ImageId, ImageInfo, ImageResourceUpdate},
};

/// Stores the resources for the application, souch as fonts, images and cached
/// texts, also clipboard strings
///
/// Images and fonts can be references across window contexts (not yet tested,
/// but should work).
pub struct AppResources {
    /// When looking up images, there are two sources: Either the indirect way via using a
    /// CssImageId (which is a String) or a direct ImageId. The indirect way requires one
    /// extra lookup (to map from the stringified ID to the actual image ID).
    pub(crate) style_ids_to_image_ids: FastHashMap<String, ImageId>,
    /// The actual image cache. The image data is not stored here; it is uploaded to the GPU.
    pub(crate) images: FastHashMap<ImageId, ImageInfo>,
    // Fonts are trickier to handle than images.
    // First, we duplicate the font - webrender wants the raw font data,
    // but we also need access to the font metrics. So we first parse the font
    // to make sure that nothing is going wrong. In the next draw call, we
    // upload the font and replace the FontState with the newly created font key
    pub(crate) font_data: RefCell<FastHashMap<FontId, (Rc<Font<'static>>, Rc<Vec<u8>>, Rc<RefCell<FontKey>>)>>,
    // After we've looked up the FontKey in the font_data map, we can then access
    // the font instance key (if there is any). If there is no font instance key,
    // we first need to create one.
    pub(crate) fonts: FastHashMap<FontKey, FastHashMap<Au, FontInstanceKey>>,
    /// Contains all incoming requests to add, remove, or update one of the app's resources, like
    /// fonts and images.
    pub(crate) resource_updates: ResourceUpdates,
    /// Stores long texts across frames
    pub(crate) text_cache: TextCache,
    /// Keyboard clipboard storage and retrieval functionality
    clipboard: SystemClipboard,
}

impl Default for AppResources {
    fn default() -> Self {
        Self {
            style_ids_to_image_ids: FastHashMap::default(),
            fonts: FastHashMap::default(),
            font_data: RefCell::new(FastHashMap::default()),
            images: FastHashMap::default(),
            resource_updates: ResourceUpdates::default(),
            text_cache: TextCache::default(),
            clipboard: SystemClipboard::new().unwrap(),
        }
    }
}

impl AppResources {

    /// Returns the IDs of all currently loaded fonts in `self.font_data`
    pub fn get_loaded_fonts(&self) -> Vec<FontId> {
        self.font_data.borrow().keys().cloned().collect()
    }

    /// See [`AppState::add_image()`](../app_state/struct.AppState.html#method.add_image)
    #[cfg(feature = "image_loading")]
    pub fn add_image<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R, image_type: ImageType)
        -> Result<Option<()>, ImageError>
    {
        use images; // the module, not the crate!

        // TODO: Handle image decoding failure better!

        let image_id = match self.style_ids_to_image_ids.entry(id.into()) {
            Occupied(_) => return Ok(None),
            Vacant(v) => {
                let new_id = images::new_image_id();
                v.insert(new_id)
            },
        };

        Ok(if self.images.contains_key(image_id) {
            None
        } else {
            let mut image_data = Vec::<u8>::new();
            data.read_to_end(&mut image_data).map_err(|e| ImageError::IoError(e))?;
            let image_format = image_type.into_image_format(&image_data)?;
            let decoded = image::load_from_memory_with_format(&image_data, image_format)?;
            let (data, desc) = images::prepare_image(decoded)?;
            self.resource_updates.image_updates.push(ImageResourceUpdate::Upload(image_id.clone(), data, desc));
            Some(())
        })
    }

    /// See [`AppState::delete_image()`](../app_state/struct.AppState.html#method.delete_image)
    pub fn delete_image<S: AsRef<str>>(&mut self, id: S)
        -> Option<()>
    {
        let image_id = self.style_ids_to_image_ids.remove(id.as_ref())?;

        match self.images.get_mut(&image_id) {
            None => None,
            Some(image_info) => {
                let key = image_info.key.clone();
                let descriptor = image_info.descriptor.clone();
                self.resource_updates.image_updates.push(ImageResourceUpdate::Delete(
                    image_id.clone(),
                    Some(key),
                    descriptor
                ));
                Some(())
            }
        }
    }

    /// See [`AppState::has_image()`](../app_state/struct.AppState.html#method.has_image)
    pub fn has_image<S: AsRef<str>>(&self, id: S)
        -> bool
    {
        let image_id = match self.get_image(id) {
            None => return false,
            Some(s) => s,
        };

        self.images.get(&image_id).is_some()
    }

    /// Returns the image ID looked up from a string
    pub fn get_image<S: AsRef<str>>(&self, id: S)
        -> Option<ImageId>
    {
        self.style_ids_to_image_ids.get(id.as_ref()).and_then(|id| Some(*id))
    }

    /// See [`AppState::add_font()`](./struct.AppState.html#method.add_font)
    pub fn add_font<R: Read>(&mut self, id: FontId, data: &mut R)
        -> Result<Option<()>, FontError>
    {
        use font;

        Ok(if self.font_data.borrow_mut().contains_key(&id) {
            None
        } else {
            let mut font_data = Vec::<u8>::new();
            data.read_to_end(&mut font_data).map_err(|e| FontError::IoError(e))?;
            let parsed_font = font::rusttype_load_font(&font_data, None)?;
            self.resource_updates.font_updates.push(FontResourceUpdate::Upload(id, parsed_font, font_data));
            Some(())
        })
    }

    /// Search for a builtin font on the users computer, validate and return it
    fn get_builtin_font(id: String) -> Option<(::rusttype::Font<'static>, Vec<u8>)>
    {
        use font_loader::system_fonts::{self, FontPropertyBuilder};
        use font::rusttype_load_font;

        let (font_bytes, idx) = system_fonts::get(&FontPropertyBuilder::new().family(&id).build())?;
        let f = rusttype_load_font(&font_bytes, Some(idx)).ok()?;
        Some((f, font_bytes))
    }

    /// Internal API - we want the user to get the first two fields of the
    fn get_font_internal(&self, id: &FontId) -> Option<(Rc<Font<'static>>, Rc<Vec<u8>>, Rc<RefCell<FontKey>>)> {
        match id {
            FontId::BuiltinFont(b) => {
                if self.font_data.borrow().get(id).is_none() {
                    let (font, font_bytes) = Self::get_builtin_font(b.clone())?;
                    // TODO system fonts are loaded for the first time from within the render loop,
                    // which is not good performance-wise and causes them to be unavailable until the second frame update.
                    // Splitting font resources off into its own immutable field exposed this issue,
                    // since the font data was in a RefCell.
                    //self.resource_updates.font_updates.push(FontResourceUpdate::Upload(id.clone(), font, font_bytes));
                    println!("Failed to load system font");
                }
                self.font_data.borrow().get(id).and_then(|(font, bytes, key)| Some((font.clone(), bytes.clone(), key.clone())))
            },
            FontId::ExternalFont(_) => {
                // For external fonts, we assume that the application programmer has
                // already loaded them, so we don't try to fallback to system fonts.
                self.font_data.borrow().get(id).and_then(|(font, bytes, key)| Some((font.clone(), bytes.clone(), key.clone())))
            },
        }
    }

    /// Given a `FontId`, returns the `Font` and the original bytes making up the font
    /// or `None`, if the `FontId` is invalid.
    pub fn get_font(&self, id: &FontId) -> Option<(Rc<Font<'static>>, Rc<Vec<u8>>)> {
        self.get_font_internal(id).and_then(|(font, bytes, _)| Some((font, bytes)))
    }

    /// Note the pub(crate) here: We don't want to expose the FontState in the public API
    pub(crate) fn get_font_key(&self, id: &FontId) -> Option<Rc<RefCell<FontKey>>> {
        self.get_font_internal(id).and_then(|(_, _, key)| Some(key))
    }

    /// Checks if a `FontId` is valid, i.e. if a font is currently ready-to-use
    pub fn has_font(&self, id: &FontId)
        -> bool
    {
        self.font_data.borrow().get(id).is_some()
    }

    /// See [`AppState::delete_font()`](./struct.AppState.html#method.delete_font)
    pub fn delete_font(&mut self, id: &FontId)
        -> Option<()>
    {
        // TODO: can fonts that haven't been uploaded yet be deleted?
        match self.font_data.borrow().get(&id) {
            None => None,
            Some(v) => {
                let font_key = *(*v.2).borrow();
                self.resource_updates.font_updates.push(FontResourceUpdate::Delete(
                    id.clone(),
                    Some(font_key.clone())
                ));
                Some(())
            }
        }
    }

    /// Adds a string to the internal text cache, but only store it as a string,
    /// without caching the layout of the string.
    pub fn add_text_uncached<S: Into<String>>(&mut self, text: S)
    -> TextId
    {
        self.text_cache.add_text(text)
    }

    /// Calculates the widths for the words (layouts the string), then stores
    /// them in a text cache, together with the actual string
    ///
    /// This leads to a faster layout cycle, but has an upfront performance cost
    pub fn add_text_cached<S: Into<String>>(&mut self, text: S, font_id: &FontId, font_size: StyleFontSize, letter_spacing: Option<StyleLetterSpacing>)
    -> TextId
    {
        // First, insert the text into the text cache
        let id = self.add_text_uncached(text);
        self.cache_text(id, font_id.clone(), font_size, letter_spacing);
        id
    }

    /// Promotes an uncached text (i.e. a text that was added via `add_text_uncached`)
    /// to a cached text by calculating the font metrics for the uncached text.
    /// This will not delete the original text!
    pub fn cache_text(&mut self, id: TextId, font: FontId, size: StyleFontSize, letter_spacing: Option<StyleLetterSpacing>) {
        // We need to assume that the actual string contents have already been stored in self.text_cache
        // Otherwise, how would the TextId be valid?
        let text = self.text_cache.string_cache.get(&id).expect("Invalid text Id");
        let font_size_no_line_height = TextSizePx(size.0.to_pixels());
        let rusttype_font = self.get_font(&font).expect("Invalid font ID");
        let words = split_text_into_words(text.as_ref(), &rusttype_font.0, font_size_no_line_height, letter_spacing);

        self.text_cache.layouted_strings_cache
            .entry(id).or_insert_with(|| FastHashMap::default())
            .entry(font).or_insert_with(|| FastHashMap::default())
            .insert(size, words);
    }

    /// Removes a string from the string cache, but not the layouted text cache
    pub fn delete_string(&mut self, id: TextId) {
        self.text_cache.delete_string(id);
    }

    /// Removes a string from the layouted text cache, but not the string cache
    pub fn delete_layouted_text(&mut self, id: TextId) {
        self.text_cache.delete_layouted_text(id);
    }

    /// Removes a string from both the string cache and the layouted text cache
    pub fn delete_text(&mut self, id: TextId) {
        self.text_cache.delete_text(id);
    }

    /// Empties the entire internal text cache, invalidating all `TextId`s. Use with care.
    pub fn clear_all_texts(&mut self) {
        self.text_cache.clear_all_texts();
    }

    /// Returns the contents of the system clipboard
    pub fn get_clipboard_string(&self)
    -> Result<String, ClipboardError>
    {
        self.clipboard.get_string_contents()
    }

    /// Sets the contents of the system clipboard - currently only strings are supported
    pub fn set_clipboard_string(&mut self, contents: String)
    -> Result<(), ClipboardError>
    {
        self.clipboard.set_string_contents(contents)
    }
}

/// Holds any requests to update an application's resources
#[derive(Default)]
pub(crate) struct ResourceUpdates {
    pub image_updates: Vec<ImageResourceUpdate>,
    pub font_updates: Vec<FontResourceUpdate>,
}

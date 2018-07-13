use images::ImageId;
use css_parser::FontSize;
use text_layout::RUSTTYPE_SIZE_HACK;
use text_layout::PX_TO_PT;
use text_layout::split_text_into_words;
use webrender::api::Epoch;
use dom::Texture;
use text_cache::TextCache;
use traits::Layout;
use std::sync::atomic::{AtomicUsize, Ordering};
use webrender::api::{ImageKey, FontKey, FontInstanceKey};
use FastHashMap;
use std::io::Read;
use images::{ImageState, ImageType};
use font::{FontState, FontError};
use image::{self, ImageError, DynamicImage, GenericImage};
use webrender::api::{ImageData, ImageDescriptor, ImageFormat};
use std::collections::hash_map::Entry::*;
use app_units::Au;
use css_parser;
use css_parser::FontId::{self, ExternalFont};
use text_cache::TextId;
use clipboard2::{Clipboard, ClipboardError, SystemClipboard};
use rusttype::Font;

/// Font and image keys
///
/// The idea is that azul doesn't know where the resources come from,
/// whether they are loaded from the network or a disk.
/// Fonts and images must be added and removed dynamically. If you have a
/// fonts that should be always accessible, then simply add them before the app
/// starts up.
///
/// Images and fonts can be references across window contexts
/// (not yet tested, but should work).
pub(crate) struct AppResources<'a> {
    /// When looking up images, there are two sources: Either the indirect way via using a
    /// CssId (which is a String) or a direct ImageId. The indirect way requires one extra
    /// lookup (to map from the stringified ID to the actual image ID). This is what this
    /// HashMap is for
    pub(crate) css_ids_to_image_ids: FastHashMap<String, ImageId>,
    /// The actual image cache, does NOT store the image data, only stores it temporarily
    /// while it is being uploaded to the GPU via webrender.
    pub(crate) images: FastHashMap<ImageId, ImageState>,
    // Fonts are trickier to handle than images.
    // First, we duplicate the font - webrender wants the raw font data,
    // but we also need access to the font metrics. So we first parse the font
    // to make sure that nothing is going wrong. In the next draw call, we
    // upload the font and replace the FontState with the newly created font key
    pub(crate) font_data: FastHashMap<FontId, (::rusttype::Font<'a>, Vec<u8>, FontState)>,
    // After we've looked up the FontKey in the font_data map, we can then access
    // the font instance key (if there is any). If there is no font instance key,
    // we first need to create one.
    pub(crate) fonts: FastHashMap<FontKey, FastHashMap<Au, FontInstanceKey>>,
    /// Stores long texts across frames
    pub(crate) text_cache: TextCache,
    /// Keyboard clipboard storage and retrieval functionality
    clipboard: SystemClipboard,
}

impl<'a> Default for AppResources<'a> {
    fn default() -> Self {
        let mut default_font_data = FastHashMap::default();
        load_system_fonts(&mut default_font_data);

        Self {
            css_ids_to_image_ids: FastHashMap::default(),
            fonts: FastHashMap::default(),
            font_data: default_font_data,
            images: FastHashMap::default(),
            text_cache: TextCache::default(),
            clipboard: SystemClipboard::new().unwrap(),
        }
    }
}

fn load_system_fonts<'a>(fonts: &mut FastHashMap<FontId, (::rusttype::Font<'a>, Vec<u8>, FontState)>) {

    use font_loader::system_fonts::{self, FontPropertyBuilder};
    use css_parser::FontId::BuiltinFont;
    use font::rusttype_load_font;

    fn insert_font<'b>(fonts: &mut FastHashMap<FontId, (::rusttype::Font<'b>, Vec<u8>, FontState)>, target: &'static str) {
        if let Some((font_bytes, idx)) = system_fonts::get(&FontPropertyBuilder::new().family(target).build()) {
            match rusttype_load_font(font_bytes.clone(), Some(idx)) {
                Ok((f, b)) =>  { fonts.insert(BuiltinFont(target), (f, b, FontState::ReadyForUpload(font_bytes))); },
                Err(e) => println!("error loading {} font: {:?}", target, e),
            }
        }
    }

    insert_font(fonts, "serif");
    insert_font(fonts, "sans-serif");
    insert_font(fonts, "monospace");
    insert_font(fonts, "cursive");
    insert_font(fonts, "fantasy");
}

impl<'a> AppResources<'a> {

    /// See `AppState::add_image()`
    pub(crate) fn add_image<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R, image_type: ImageType)
        -> Result<Option<()>, ImageError>
    {
        use images; // the module, not the crate!

        // TODO: Handle image decoding failure better!

        let image_id = match self.css_ids_to_image_ids.entry(id.into()) {
            Occupied(_) => return Ok(None),
            Vacant(v) => {
                let new_id = images::new_image_id();
                v.insert(new_id)
            },
        };

        match self.images.entry(*image_id) {
            Occupied(_) => Ok(None),
            Vacant(v) => {
                let mut image_data = Vec::<u8>::new();
                data.read_to_end(&mut image_data).map_err(|e| ImageError::IoError(e))?;
                let image_format = image_type.into_image_format(&image_data)?;
                let decoded = image::load_from_memory_with_format(&image_data, image_format)?;
                v.insert(ImageState::ReadyForUpload(images::prepare_image(decoded)?));
                Ok(Some(()))
            },
        }
    }

    /// See `AppState::delete_image()`
    pub(crate) fn delete_image<S: AsRef<str>>(&mut self, id: S)
        -> Option<()>
    {
        let image_id = self.css_ids_to_image_ids.remove(id.as_ref())?;

        match self.images.get_mut(&image_id) {
            None => None,
            Some(v) => {
                let to_delete_image_key = match *v {
                    ImageState::Uploaded(ref image_info) => {
                        Some(image_info.key.clone())
                    },
                    _ => None,
                };
                *v = ImageState::AboutToBeDeleted(to_delete_image_key);
                Some(())
            }
        }
    }

    /// See `AppState::has_image()`
    pub(crate) fn has_image<S: AsRef<str>>(&mut self, id: S)
        -> bool
    {
        let image_id = match self.css_ids_to_image_ids.get(id.as_ref()) {
            None => return false,
            Some(s) => s,
        };

        self.images.get(image_id).is_some()
    }

    /// See `AppState::add_font()`
    pub(crate) fn add_font<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R)
        -> Result<Option<()>, FontError>
    {
        use font;

        match self.font_data.entry(ExternalFont(id.into())) {
            Occupied(_) => Ok(None),
            Vacant(v) => {
                let mut font_data = Vec::<u8>::new();
                data.read_to_end(&mut font_data).map_err(|e| FontError::IoError(e))?;
                let (parsed_font, fd) = font::rusttype_load_font(font_data.clone(), None)?;
                v.insert((parsed_font, fd, FontState::ReadyForUpload(font_data)));
                Ok(Some(()))
            },
        }
    }

    pub fn get_font<'b>(&'b self, id: &FontId) -> Option<(&'b Font<'a>, &'b Vec<u8>)> {
        self.font_data.get(id).and_then(|(font, bytes, _)| Some((font, bytes)))
    }

    /// Checks if a font is currently registered and ready-to-use
    pub(crate) fn has_font<S: Into<String>>(&mut self, id: S)
        -> bool
    {
        self.font_data.get(&ExternalFont(id.into())).is_some()
    }

    /// See `AppState::delete_font()`
    pub(crate) fn delete_font<S: Into<String>>(&mut self, id: S)
        -> Option<()>
    {
        // TODO: can fonts that haven't been uploaded yet be deleted?
        match self.font_data.get_mut(&ExternalFont(id.into())) {
            None => None,
            Some(v) => {
                let to_delete_font_key = match v.2 {
                    FontState::Uploaded(ref font_key) => {
                        Some(font_key.clone())
                    },
                    _ => None,
                };
                v.2 = FontState::AboutToBeDeleted(to_delete_font_key);
                Some(())
            }
        }
    }

    pub(crate) fn add_text_uncached<S: Into<String>>(&mut self, text: S)
    -> TextId
    {
        use text_cache::LargeString;
        self.text_cache.add_text(LargeString::Raw(text.into()))
    }

    /// Calculates the widths for the words, then stores the widths of the words + the actual words
    ///
    /// This leads to a faster layout cycle, but has an upfront performance cost
    pub(crate) fn add_text_cached<S: AsRef<str>>(&mut self, text: S, font_id: &FontId, font_size: FontSize)
    -> TextId
    {
        use rusttype::Scale;
        use text_cache::LargeString;
        use std::rc::Rc;

        let font_size_no_line_height = Scale::uniform(font_size.0.to_pixels() * RUSTTYPE_SIZE_HACK * PX_TO_PT);
        let rusttype_font = self.font_data.get(font_id).expect("in resources.add_text_cached(): could not get font for caching text");
        let words = split_text_into_words(text.as_ref(), &rusttype_font.0, font_size_no_line_height);
        self.text_cache.add_text(LargeString::Cached { font: font_id.clone(), size: font_size, words: Rc::new(words) })
    }

    pub(crate) fn delete_text(&mut self, id: TextId) {
        self.text_cache.delete_text(id);
    }

    pub(crate) fn clear_all_texts(&mut self) {
        self.text_cache.clear_all_texts();
    }

    pub(crate) fn get_clipboard_string(&mut self)
    -> Result<String, ClipboardError>
    {
        self.clipboard.get_string_contents()
    }

    pub(crate) fn set_clipboard_string(&mut self, contents: String)
    -> Result<(), ClipboardError>
    {
        self.clipboard.set_string_contents(contents)
    }
}

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_resources_file() {

}
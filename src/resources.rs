use webrender::api::Epoch;
use dom::Texture;
use text_cache::TextRegistry;
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
use css_parser::Font::ExternalFont;
use svg::{SvgLayerId, SvgLayer, SvgParseError, SvgRegistry};
use text_cache::TextId;

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
#[derive(Clone)]
pub(crate) struct AppResources<'a, T: Layout> {
    /// Image cache
    pub(crate) images: FastHashMap<String, ImageState>,
    // Fonts are trickier to handle than images.
    // First, we duplicate the font - webrender wants the raw font data,
    // but we also need access to the font metrics. So we first parse the font
    // to make sure that nothing is going wrong. In the next draw call, we
    // upload the font and replace the FontState with the newly created font key
    pub(crate) font_data: FastHashMap<css_parser::Font, (::rusttype::Font<'a>, FontState)>,
    // After we've looked up the FontKey in the font_data map, we can then access
    // the font instance key (if there is any). If there is no font instance key,
    // we first need to create one.
    pub(crate) fonts: FastHashMap<FontKey, FastHashMap<Au, FontInstanceKey>>,
    /// Stores the polygon data for all SVGs. Polygons can be shared across windows
    /// without duplicating the data. This doesn't store any rendering-related data,
    /// only the polygons
    pub(crate) svg_registry: SvgRegistry<T>,
    /// Stores long texts across frames
    pub(crate) text_registry: TextRegistry,
}

impl<'a, T: Layout> Default for AppResources<'a, T> {
    fn default() -> Self {
        Self {
            svg_registry: SvgRegistry::default(),
            fonts: FastHashMap::default(),
            font_data: FastHashMap::default(),
            images: FastHashMap::default(),
            text_registry: TextRegistry::default(),
        }
    }
}

impl<'a, T: Layout> AppResources<'a, T> {

    /// See `AppState::add_image()`
    pub(crate) fn add_image<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R, image_type: ImageType)
        -> Result<Option<()>, ImageError>
    {
        use images; // the module, not the crate!

        match self.images.entry(id.into()) {
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
        match self.images.get_mut(id.as_ref()) {
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
        self.images.get(id.as_ref()).is_some()
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
                let parsed_font = font::rusttype_load_font(font_data.clone())?;
                v.insert((parsed_font, FontState::ReadyForUpload(font_data)));
                Ok(Some(()))
            },
        }
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
                let to_delete_font_key = match v.1 {
                    FontState::Uploaded(ref font_key) => {
                        Some(font_key.clone())
                    },
                    _ => None,
                };
                v.1 = FontState::AboutToBeDeleted(to_delete_font_key);
                Some(())
            }
        }
    }

    /// A "SvgLayer" represents one or more shapes that get drawn using the same style (necessary for batching).
    /// Adds the SVG layer as a resource to the internal resources, the returns the ID, which you can use in the
    /// `NodeType::SvgLayer` to draw the SVG layer.
    pub(crate) fn add_svg_layer(&mut self, layer: SvgLayer<T>)
    -> SvgLayerId
    {
        self.svg_registry.add_layer(layer)
    }

    /// See `AppState::`
    pub(crate) fn delete_svg_layer(&mut self, svg_id: SvgLayerId)
    {
        self.svg_registry.delete_layer(svg_id);
    }

    /// Clears all crate-internal resources and shapes. Use with care.
    pub(crate) fn clear_all_svg_layers(&mut self)
    {
        self.svg_registry.clear_all_layers();
    }

    pub(crate) fn add_text<S: Into<String>>(&mut self, text: S)
    -> TextId
    {
        self.text_registry.add_text(text)
    }

    pub(crate) fn delete_text(&mut self, id: TextId) {
        self.text_registry.delete_text(id);
    }

    pub(crate) fn clear_all_texts(&mut self) {
        self.text_registry.clear_all_texts();
    }

    /// Parses an input source, parses the SVG, adds the shapes as layers into
    /// the registry, returns the IDs of the added shapes, in the order that
    /// they appeared in the SVG text.
    pub(crate) fn add_svg<R: Read>(&mut self, input: R)
    -> Result<Vec<SvgLayerId>, SvgParseError>
    {
        self.svg_registry.add_svg(input)
    }
}
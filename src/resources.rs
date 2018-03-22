use std::sync::atomic::{AtomicUsize, Ordering};
use webrender::api::{ImageKey, FontKey};
use FastHashMap;
use std::io::Read;
use images::{ImageState, ImageType};
use image::{self, ImageError, DynamicImage, GenericImage};
use webrender::api::{ImageData, ImageDescriptor, ImageFormat};

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
#[derive(Debug, Default, Clone)]
pub(crate) struct AppResources {
    pub(crate) images: FastHashMap<String, ImageState>,
    pub(crate) fonts: FastHashMap<String, FastHashMap<FontSize, FontKey>>,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct FontSize(pub(crate) usize);

impl AppResources {

    /// See `AppState::add_image()`
    pub fn add_image<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R, image_type: ImageType) 
        -> Result<Option<()>, ImageError>
    {
        use std::collections::hash_map::Entry::*;
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

    /// See `AppState::remove_image()`
    pub fn remove_image<S: Into<String>>(&mut self, id: S) 
        -> Option<()> 
    {
        Some(())
    }

    /// See `AppState::has_image()`
    pub fn has_image<S: Into<String>>(&mut self, id: S) 
        -> bool 
    {
        false
    }

    /// See `AppState::add_font()`
    pub fn add_font<S: Into<String>, R: Read>(&mut self, id: S, data: &mut R)
        -> Result<Option<()>, ImageError>
    {
        Ok(Some(()))
    }

    /// See `AppState::remove_font()`
    pub(crate) fn remove_font<S: Into<String>>(&mut self, id: S) 
        -> Option<()>
    {
        Some(())
    }
}
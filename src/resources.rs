use std::sync::atomic::{AtomicUsize, Ordering};
use webrender::api::{ImageKey, FontKey};
use FastHashMap;
use std::io::Read;
use image::{ImageType, ImageError};

static LAST_FONT_ID: AtomicUsize = AtomicUsize::new(0);
static LAST_IMAGE_ID: AtomicUsize = AtomicUsize::new(0);

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
    pub(crate) images: FastHashMap<usize, ImageKey>,
    pub(crate) fonts: FastHashMap<usize, FontKey>,
}

impl AppResources {

    /// See `AppState::add_image()`
    pub fn add_image<S: AsRef<str>, R: Read>(&mut self, id: S, data: R, image_type: ImageType) 
        -> Result<Option<()>, ImageError>
    {
        Ok(Some(()))
    }

    /// See `AppState::remove_image()`
    pub fn remove_image<S: AsRef<str>>(&mut self, id: S) 
        -> Option<()> 
    {
        Some(())
    }

    /// See `AppState::has_image()`
    pub fn has_image<S: AsRef<str>>(&mut self, id: S) 
        -> bool 
    {
        false
    }

    /// See `AppState::add_font()`
    pub fn add_font<S: AsRef<str>, R: Read>(&mut self, id: S, data: R)
        -> Result<Option<()>, ImageError>
    {
        Ok(Some(()))
    }

    /// See `AppState::remove_font()`
    pub(crate) fn remove_font<S: AsRef<str>>(&mut self, id: S) 
        -> Option<()>
    {
        Some(())
    }
}

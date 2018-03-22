use traits::LayoutScreen;
use resources::{AppResources};
use std::io::Read;
use image::{ImageType, ImageError};

/// Wrapper for your application data. In order to be layout-able,
/// you need to satisfy the `LayoutScreen` trait (how the application
/// should be laid out)
pub struct AppState<T: LayoutScreen> {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: T,
    /// Fonts and images that are currently loaded into the app
    pub(crate) resources: AppResources,
}

impl<T: LayoutScreen> AppState<T> {

    /// Creates a new `AppState`
    pub fn new(initial_data: T) -> Self {
        Self {
            data: initial_data,
            resources: AppResources::default(),
        }
    }

    /// Add an image to the internal resources
    ///
    /// ## Returns
    /// 
    /// - `Ok(Some(()))` if an image with the same ID already exists. 
    /// - `Ok(None)` if the image was added, but didn't exist previously.
    /// - `Err(e)` if the image couldn't be decoded 
    pub fn add_image<S: AsRef<str>, R: Read>(&mut self, id: S, data: R, image_type: ImageType) 
        -> Result<Option<()>, ImageError>
    {
        self.resources.add_image(id, data, image_type)
    }

    /// Removes an image from the internal app resources.
    /// Returns `Some` if the image existed and was removed.
    /// If the given ID doesn't exist, this function does nothing and returns `None`.
    pub fn remove_image<S: AsRef<str>>(&mut self, id: S) 
        -> Option<()> 
    {
        self.resources.remove_image(id)
    }

    /// Checks if an image is currently registered and ready-to-use
    pub fn has_image<S: AsRef<str>>(&mut self, id: S) 
        -> bool 
    {
        self.resources.has_image(id)
    }

    /// Add a font (TTF or OTF) to the internal resources
    ///
    /// ## Returns
    /// 
    /// - `Ok(Some(()))` if an font with the same ID already exists. 
    /// - `Ok(None)` if the font was added, but didn't exist previously.
    /// - `Err(e)` if the font couldn't be decoded 
    pub fn add_font<S: AsRef<str>, R: Read>(&mut self, id: S, data: R)
        -> Result<Option<()>, ImageError>
    {
        self.resources.add_font(id, data)
    }

    /// Removes a font from the internal app resources.
    /// Returns `Some` if the image existed and was removed.
    /// If the given ID doesn't exist, this function does nothing and returns `None`.
    pub fn remove_font<S: AsRef<str>>(&mut self, id: S) 
        -> Option<()>
    {
        self.resources.remove_font(id)
    }
}

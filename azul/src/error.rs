pub use {
    app::RuntimeError,
    app_resources::{ImageReloadError, FontReloadError},
    widgets::errors::*,
    window::WindowCreateError,
};
// TODO: re-export the sub-types of ClipboardError!
pub use clipboard2::ClipboardError;

#[derive(Debug)]
pub enum Error {
    Resource(ResourceReloadError),
    Clipboard(ClipboardError),
    WindowCreate(WindowCreateError),
}

impl_from!(ResourceReloadError, Error::Resource);
impl_from!(ClipboardError, Error::Clipboard);
impl_from!(WindowCreateError, Error::WindowCreate);

#[derive(Debug)]
pub enum ResourceReloadError {
    Image(ImageReloadError),
    Font(FontReloadError),
}

impl_from!(ImageReloadError, ResourceReloadError::Image);
impl_from!(FontReloadError, ResourceReloadError::Font);

impl_display!(ResourceReloadError, {
    Image(e) => format!("Failed to load image: {}", e),
    Font(e) => format!("Failed to load font: {}", e),
});

impl_display!(Error, {
    Resource(e) => format!("{}", e),
    Clipboard(e) => format!("Clipboard error: {}", e),
    WindowCreate(e) => format!("Window creation error: {}", e),
});

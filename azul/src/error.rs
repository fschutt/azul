pub use app::RuntimeError;
#[cfg(debug_assertions)]
pub use style::HotReloadError;
pub use css_parser::{
    CssMetric, InvalidValueErr,
};
pub use font::FontError;
#[cfg(feature = "image_loading")]
pub use image::ImageError;
pub use simplecss::Error as CssSyntaxError;

// TODO: re-export the sub-types of ClipboardError!
pub use clipboard2::ClipboardError;

pub use widgets::errors::*;
pub use window::WindowCreateError;

#[derive(Debug)]
pub enum Error {
    Font(FontError),
    #[cfg(feature = "image_loading")]
    Image(ImageError),
    Clipboard(ClipboardError),
    WindowCreate(WindowCreateError),
    #[cfg(debug_assertions)]
    HotReload(HotReloadError),
}

impl From<FontError> for Error {
    fn from(e: FontError) -> Error {
        Error::Font(e)
    }
}

#[cfg(feature = "image_loading")]
impl From<ImageError> for Error {
    fn from(e: ImageError) -> Error {
        Error::Image(e)
    }
}

impl From<ClipboardError> for Error {
    fn from(e: ClipboardError) -> Error {
        Error::Clipboard(e)
    }
}

impl From<WindowCreateError> for Error {
    fn from(e: WindowCreateError) -> Error {
        Error::WindowCreate(e)
    }
}

#[cfg(debug_assertions)]
impl From<HotReloadError> for Error {
    fn from(e: HotReloadError) -> Error {
        Error::HotReload(e)
    }
}

use std::fmt;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Font(e) => write!(f, "[Font error] {}", e),
            #[cfg(feature = "image_loading")]
            Error::Image(e) => write!(f, "[Image error] {}", e),
            Error::Clipboard(e) => write!(f, "[Clipboard error] {}", e),
            Error::WindowCreate(e) => write!(f, "[Window creation error] {}", e),
            #[cfg(debug_assertions)]
            Error::HotReload(e) => write!(f, "[Hot-reload error] {}", e),
        }
    }
}

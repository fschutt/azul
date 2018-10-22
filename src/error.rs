pub use app::RuntimeError;
pub use css::{CssParseError, DynamicCssParseError};
#[cfg(debug_assertions)]
pub use css::HotReloadError;
pub use css_parser::{
    CssBackgroundParseError, CssBorderParseError, CssStyleBorderRadiusParseError, CssColorParseError,
    CssDirectionParseError, CssStyleFontFamilyParseError, CssGradientStopParseError, CssImageParseError,
    CssMetric, CssParsingError, CssShadowParseError, CssShapeParseError, InvalidValueErr,
    PercentageParseError, PixelParseError,
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
pub enum Error<'a> {
    CssParse(CssParseError<'a>),
    Font(FontError),
    #[cfg(feature = "image_loading")]
    Image(ImageError),
    Clipboard(ClipboardError),
    WindowCreate(WindowCreateError),
    #[cfg(debug_assertions)]
    HotReload(HotReloadError),
}

impl<'a> From<CssParseError<'a>> for Error<'a> {
    fn from(e: CssParseError<'a>) -> Error {
        Error::CssParse(e)
    }
}

impl<'a> From<FontError> for Error<'a> {
    fn from(e: FontError) -> Error<'a> {
        Error::Font(e)
    }
}

#[cfg(feature = "image_loading")]
impl<'a> From<ImageError> for Error<'a> {
    fn from(e: ImageError) -> Error<'a> {
        Error::Image(e)
    }
}

impl<'a> From<ClipboardError> for Error<'a> {
    fn from(e: ClipboardError) -> Error<'a> {
        Error::Clipboard(e)
    }
}

impl<'a> From<WindowCreateError> for Error<'a> {
    fn from(e: WindowCreateError) -> Error<'a> {
        Error::WindowCreate(e)
    }
}

#[cfg(debug_assertions)]
impl<'a> From<HotReloadError> for Error<'a> {
    fn from(e: HotReloadError) -> Error<'a> {
        Error::HotReload(e)
    }
}

use std::fmt;
impl<'a> fmt::Display for Error<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::CssParse(e) => write!(f, "[CSS parsing error] {}", e),
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

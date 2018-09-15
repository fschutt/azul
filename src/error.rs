pub use css::{CssParseError, DynamicCssParseError};
pub use css_parser::{
    CssBackgroundParseError, CssBorderParseError, CssBorderRadiusParseError, CssColorParseError,
    CssDirectionParseError, CssFontFamilyParseError, CssGradientStopParseError, CssImageParseError,
    CssMetric, CssParsingError, CssShadowParseError, CssShapeParseError, InvalidValueErr,
    PercentageParseError, PixelParseError,
};
pub use font::FontError;
pub use image::ImageError;
pub use simplecss::Error as CssSyntaxError;

// TODO: re-export the sub-types of ClipboardError!
pub use clipboard2::ClipboardError;

pub use widgets::errors::*;
pub use window::WindowCreateError;

macro_rules! impl_display {
    ($enum:ident, {$($variant:pat => $fmt_string:expr),+}) => (
    
        impl<'a> ::std::fmt::Display for $enum<'a> {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                use self::$enum::*;
                match &self {
                    $(
                        $variant => write!(f, "{}", $fmt_string),
                    )+
                }
            }
        }

    )
}

macro_rules! impl_display_without_lifetime {
    ($enum:ident, {$($variant:pat => $fmt_string:expr),+}) => (
    
        impl ::std::fmt::Display for $enum {
            fn fmt(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                use self::$enum::*;
                match &self {
                    $(
                        $variant => write!(f, "{}", $fmt_string),
                    )+
                }
            }
        }

    )
}

#[derive(Debug)]
pub enum Error<'a> {
    CssParse(CssParseError<'a>),
    Font(FontError),
    Image(ImageError),
    Clipboard(ClipboardError),
    WindowCreate(WindowCreateError)
}

impl_display! {Error, {
    CssParse(e) => format!("[CSS error] {}", e),
    Font(e) => format!("[Font error] {}", e),
    Image(e) => format!("[Image error] {}", e),
    Clipboard(e) => format!("[Clipboard error] {}", e),
    WindowCreate(e) => format!("[Window-create error] {}", e)
}}

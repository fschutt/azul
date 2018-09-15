extern crate azul;

use azul::error::{
    ClipboardError, CssBorderParseError, CssParseError, CssParsingError, DynamicCssParseError,
    Error, FontError, InvalidValueErr, PixelParseError, WindowCreateError,
};

pub fn main() {
    println!(
        "{}",
        Error::CssParse(CssParseError::UnexpectedValue(
            CssParsingError::InvalidValueErr(InvalidValueErr("Test"))
        ))
    );

    println!(
        "{}",
        Error::CssParse(CssParseError::DynamicCssParseError(
            DynamicCssParseError::EmptyBraces
        ))
    );

    println!(
        "{}",
        Error::CssParse(CssParseError::DynamicCssParseError(
            DynamicCssParseError::UnexpectedValue(CssParsingError::CssBorderParseError(
                CssBorderParseError::ThicknessParseError(PixelParseError::InvalidComponent("Foo"))
            ))
        ))
    );

    println!(
        "{}",
        Error::WindowCreate(WindowCreateError::WebGlNotSupported)
    );

    println!("{}", Error::Clipboard(ClipboardError::Unimplemented));

    println!("{}", Error::WindowCreate(WindowCreateError::Renderer));

    println!("{}", Error::Font(FontError::InvalidFormat));
}

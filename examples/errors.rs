extern crate azul;

use azul::error::{
    ClipboardError, CssBorderParseError, CssParseError, CssParsingError, DynamicCssParseError,
    Error, FontError, InvalidValueErr, PixelParseError, WindowCreateError,
};

pub fn main() {
    // [CSS error] Unexpected value: "Test"
    println!(
        "{}",
        Error::CssParse(CssParseError::UnexpectedValue(
            CssParsingError::InvalidValueErr(InvalidValueErr("Test"))
        ))
    );

    // [CSS error] Dynamic parsing error: Dynamic css property braces are empty, i.e. `[[ ]]`
    println!(
        "{}",
        Error::CssParse(CssParseError::DynamicCssParseError(
            DynamicCssParseError::EmptyBraces
        ))
    );

    // [CSS error] Dynamic parsing error: Unexpected value: Invalid border thickness: Invalid component: "Foo"
    println!(
        "{}",
        Error::CssParse(CssParseError::DynamicCssParseError(
            DynamicCssParseError::UnexpectedValue(CssParsingError::CssBorderParseError(
                CssBorderParseError::ThicknessParseError(PixelParseError::InvalidComponent("Foo"))
            ))
        ))
    );

    // [Window-create error] WebGl is not supported by webrender
    println!(
        "{}",
        Error::WindowCreate(WindowCreateError::WebGlNotSupported)
    );

    // [Clipboard error] Clipboard::Unimplemented: Attempted to get or set the clipboard, which hasn't been implemented yet.
    println!("{}", Error::Clipboard(ClipboardError::Unimplemented));

    // [Window-create error] Webrender creation error (probably OpenGL missing?)
    println!("{}", Error::WindowCreate(WindowCreateError::Renderer));

    // [Font error] Invalid format
    println!("{}", Error::Font(FontError::InvalidFormat));
}

//! Azul-XML-to-Rust compiler (library)

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![cfg_attr(not(feature = "std"), no_std)]

extern crate core;
#[macro_use]
extern crate alloc;
extern crate gleam;
extern crate xmlparser;
#[macro_use(impl_display)]
extern crate azul_core;
#[macro_use]
extern crate azul_css;
extern crate azul_layout;
#[cfg(feature = "font_loading")]
extern crate rust_fontconfig;
#[cfg(feature = "image_loading")]
extern crate image as image_crate;

/// XML-based DOM serialization and XML-to-Rust compiler implementation
#[cfg(feature = "xml")]
pub mod xml;
#[cfg(feature = "xml")]
pub mod xml_parser;
#[cfg(feature = "svg")]
pub mod svg;
// /// XML-based DOM serialization and XML-to-Rust compiler implementation
// pub mod xml_parser;
#[cfg(feature = "font_loading")]
pub mod font;
#[cfg(feature = "image_loading")]
pub mod image;
/// Module for compiling CSS to Rust code
pub mod css;
/// Re-export of the `azul-layout` crate
pub mod layout {
    pub use azul_layout::*;
}

/// Module for decoding and loading fonts
#[cfg(all(feature = "std", feature ="font_loading"))]
pub mod font_loading;

/// Parse a string in the format of "600x100" -> (600, 100)
pub fn parse_display_list_size(output_size: &str) -> Option<(f32, f32)> {
    let output_size = output_size.trim();
    let mut iter = output_size.split("x");
    let w = iter.next()?;
    let h = iter.next()?;
    let w = w.trim();
    let h = h.trim();
    let w = w.parse::<f32>().ok()?;
    let h = h.parse::<f32>().ok()?;
    Some((w, h))
}
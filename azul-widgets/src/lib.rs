//! Built-in widgets for the Azul GUI system

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

extern crate azul;

#[cfg(feature = "fonts")]
extern crate stb_truetype;
#[cfg(feature = "svg")]
extern crate gleam;
#[cfg(feature = "svg")]
extern crate lyon;
#[cfg(feature = "svg_parsing")]
extern crate usvg;

#[cfg(feature = "serde_serialization")]
extern crate serde;
#[cfg(feature = "serde_serialization")]
#[cfg_attr(feature = "serde_serialization", macro_use(Serialize, Deserialize))]
extern crate serde_derive;

#[cfg(feature = "svg")]
pub mod svg;
pub mod button;
pub mod label;
pub mod text_input;
pub mod table_view;

pub mod errors {
    #[cfg(all(feature = "svg", feature = "svg_parsing"))]
    pub use super::svg::SvgParseError;
}
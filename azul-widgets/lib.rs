extern crate azul_core;
extern crate azul_css;
#[cfg(feature = "svg")]
extern crate azul_dependencies;
#[cfg(feature = "svg")]
extern crate gleam;
#[cfg(feature = "serde_serialization")]
extern crate serde;
#[cfg(feature = "serde_serialization")]
#[cfg_attr(feature = "serde_serialization", macro_use(Serialize, Deserialize))]
extern crate serde_derive;


#[cfg(feature = "svg")]
pub(crate) use azul_dependencies::stb_truetype;
#[cfg(feature = "svg")]
pub(crate) use azul_dependencies::lyon;
#[cfg(feature = "svg_parsing")]
pub(crate) use azul_dependencies::usvg;

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
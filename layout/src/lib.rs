#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

#[cfg(feature = "font_loading")]
pub mod font;
pub mod image;
pub mod solver2;
#[cfg(feature = "text_layout")]
pub mod text;
#[cfg(feature = "xml")]
pub mod xml;

// #[cfg(feature = "text_layout")]
// pub use solver::{callback_info_shape_text, do_the_layout, do_the_relayout};
#[cfg(feature = "text_layout")]
pub use text::parse_font_fn;

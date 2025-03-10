#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

pub mod solver;
pub mod image;
#[cfg(feature = "font_loading")]
pub mod font;
#[cfg(feature = "text_layout")]
pub mod text;
#[cfg(feature = "xml")]
pub mod xml;

#[cfg(feature = "text_layout")]
pub use solver::{do_the_layout, do_the_relayout, callback_info_shape_text};
#[cfg(feature = "text_layout")]
pub use text::parse_font_fn;

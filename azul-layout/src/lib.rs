#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico"
)]
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

extern crate azul_core;
extern crate azul_css;
#[cfg(feature = "text_layout")]
extern crate azul_text_layout as text_layout;

mod layout_solver;

#[cfg(feature = "text_layout")]
pub use azul_text_layout::parse_font_fn;
#[cfg(feature = "text_layout")]
pub use layout_solver::callback_info_shape_text;
pub use layout_solver::{do_the_layout, do_the_relayout};

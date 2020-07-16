#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

extern crate azul_core;
extern crate azul_css;
#[cfg(feature = "text_layout")]
pub extern crate azul_text_layout as text_layout;

mod layout_solver;

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![allow(unused_variables)]

extern crate azul;

extern crate webrender;

extern crate simplecss;

#[macro_use]
mod macros;

mod css_parser;
mod css;
mod dom;

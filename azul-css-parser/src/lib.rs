//! Provides a reference implementation of a style parser for Azul, capable of parsing CSS
//! stylesheets into their respective `AppStyle` counterparts.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![allow(unused_variables)]

extern crate azul_style;

extern crate simplecss;

#[macro_use]
mod macros;

mod css_parser;
mod css;
mod dom;
mod hot_reloader;

pub use css::{
    new_from_str,
    CssParseError,
};

pub use css_parser::{
    from_kv,
};

pub use hot_reloader::{
    HotReloader,
};

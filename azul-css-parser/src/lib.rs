//! Provides a reference implementation of a style parser for Azul, capable of parsing CSS
//! stylesheets into their respective `Css` counterparts.

// #![no_std]

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![warn(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![allow(unused_variables)]

extern crate core;
#[macro_use]
extern crate alloc;
extern crate azul_css;
extern crate azul_simplecss;

#[macro_use]
mod macros;

mod css_parser;
mod css;

pub use crate::css::*;
pub use crate::css_parser::*;

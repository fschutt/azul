//! Provides a reference implementation of a style parser for Azul, capable of parsing CSS
//! stylesheets into their respective `Css` counterparts.

#[macro_use]
mod macros;
mod css;
mod css_parser;

pub use self::{css::*, css_parser::*};

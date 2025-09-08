//! Provides a reference implementation of a style parser for Azul, capable of parsing CSS
//! stylesheets into their respective `Css` counterparts.

#[macro_use]
mod macros;
mod css;
mod css_parser;

pub use self::{css::*, css_parser::*};

pub mod layout;
pub mod style;
pub mod types;
// Basic, reusable parsers
pub mod basic;
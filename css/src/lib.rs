//! Provides datatypes used to describe an application's style using the Azul GUI framework.
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

#[cfg(feature = "parser")]
pub mod parser;

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

#[macro_use]
mod macros;
mod css;
mod css_properties;
mod print_css;

pub use crate::{css::*, css_properties::*, print_css::*, macros::*};

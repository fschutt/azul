//! Provides datatypes used to describe an application's style using the Azul GUI framework.
#![allow(warnings)]

// #![no_std]

#[macro_use]
extern crate alloc;
extern crate core;

#[macro_use]
pub mod macros;
pub mod corety;
pub mod css;
pub mod dynamic_selector;
pub mod format_rust_code;
#[cfg(feature = "parser")]
pub mod parser2;
pub mod props;
pub mod shape;
pub mod shape_parser;
pub mod system;

pub use self::corety::*;

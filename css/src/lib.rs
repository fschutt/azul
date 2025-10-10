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
#[cfg(feature = "parser")]
pub mod parser2;
pub mod props;

pub use self::corety::*;

// pub mod parser; --- OLD CODE, IGNORE, ONLY USE AS REFERENCE ---
// pub mod css_properties; --- OLD CODE, IGNORE, ONLY USE AS REFERENCE ---
// pub mod print_css; --- OLD CODE, IGNORE, ONLY USE AS REFERENCE ---

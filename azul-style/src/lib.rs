//! Provides datatypes used to describe an application's style using the Azul GUI framework.

extern crate webrender;
extern crate euclid;

#[macro_use]
mod macros;
mod style;
mod style_properties;
mod hot_reload;

pub use style::*;
pub use style_properties::*;
pub use hot_reload::*;

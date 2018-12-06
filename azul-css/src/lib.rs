//! Provides datatypes used to describe an application's style using the Azul GUI framework.

extern crate webrender;
extern crate euclid;

#[macro_use]
mod macros;
mod css;
mod css_properties;
mod hot_reload;

pub use css::*;
pub use css_properties::*;
pub use hot_reload::*;

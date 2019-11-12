//! Azul-XML-to-Rust compiler (library)

extern crate gleam;
extern crate xmlparser;
#[macro_use(impl_from)]
extern crate azul_core;
extern crate azul_css;
extern crate azul_layout;

/// XML-based DOM serialization and XML-to-Rust compiler implementation
pub mod xml;
pub mod css;
pub mod layout {
    pub use azul_layout::*;
}

use azul_core::{
    display_list::CachedDisplayList,
    dom::Dom,
};
use azul_css::LayoutSize;

pub struct Dummy;

#[no_mangle]
pub fn compile_xml_to_rust_code(_input: &str) -> String {
    String::new() // TODO
}

#[no_mangle]
pub fn compile_xml_to_html(_input: &str) -> String {
    Dom::<Dummy>::div().get_html_string() // TODO
}

#[no_mangle]
pub fn compile_xml_to_display_list(_input: &str, layout_size: LayoutSize) -> CachedDisplayList {
    CachedDisplayList::empty(layout_size) // TODO
}

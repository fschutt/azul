//! Azul-XML-to-Rust compiler (library)

extern crate gleam;
extern crate xmlparser;
#[macro_use(impl_from)]
extern crate azul_core;
extern crate azul_css;
extern crate azul_css_parser;
extern crate azul_layout;

/// XML-based DOM serialization and XML-to-Rust compiler implementation
pub mod xml;
pub mod layout {
    pub use azul_layout::*;
}

use azul_core::{
    display_list::CachedDisplayList,
    dom::Dom,
};
use azul_css::LayoutSize;

#[no_mangle]
pub fn compile_xml_to_rust_code(_input: &str) -> String {
    String::new() // TODO
}

#[no_mangle]
pub fn compile_xml_to_dom<T>(_input: &str) -> Dom<T> {
    Dom::div() // TODO
}

#[no_mangle]
pub fn compile_xml_to_display_list(_input: &str, layout_size: LayoutSize) -> CachedDisplayList {
    CachedDisplayList::empty(layout_size) // TODO
}

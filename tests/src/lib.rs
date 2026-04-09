//! Contains all unit and integration tests, in order to keep /src clean from tests

extern crate alloc;

#[path = "./css.rs"]
mod css;
#[path = "./css-parser.rs"]
mod css_parser;
#[path = "./dom.rs"]
mod dom;
#[path = "./font-gc.rs"]
mod font_gc;
#[path = "./layout-test.rs"]
mod layout_test;
#[path = "./script.rs"]
mod script;
#[path = "./text-layout.rs"]
mod text_layout;
#[path = "./ui.rs"]
mod ui;
#[path = "./word-wrap.rs"]
mod word_wrap;
#[path = "./xml.rs"]
mod xml;

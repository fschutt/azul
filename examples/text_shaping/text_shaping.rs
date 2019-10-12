#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::prelude::*;
use std::time::Duration;

macro_rules! XML_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/text_shaping/text_shaping.xml")) }
macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/text_shaping/text_shaping.css")) }

struct DataModel { }

impl Layout for DataModel {
    fn layout(&self, _: LayoutInfo) -> Dom<Self> {
        DomXml::from_file(XML_PATH!(), &mut XmlComponentMap::default()).into()
    }
}

fn main() {

    let app = App::new(DataModel { }, AppConfig::default()).unwrap();

    #[cfg(debug_assertions)]
    let window = WindowCreateOptions::new_hot_reload(css::hot_reload_override_native(CSS_PATH!(), Duration::from_millis(500)));

    #[cfg(not(debug_assertions))]
    let window = WindowCreateOptions::new(css::override_native(include_str!(CSS_PATH!())).unwrap());

    app.run(window);
}
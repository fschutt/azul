extern crate azul;

use azul::prelude::*;
use std::time::Duration;

macro_rules! XML_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/ui.xml")) }
macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/xml.css")) }

struct DataModel { }

impl Layout for DataModel {
    fn layout(&self, _: LayoutInfo<DataModel>) -> Dom<DataModel> {
        Dom::from_file(XML_PATH!(), &XmlComponentMap::default())
    }
}

fn main() {

    let app = App::new(DataModel { }, AppConfig::default());

    #[cfg(debug_assertions)]
    let window = Window::new_hot_reload(WindowCreateOptions::default(), css::hot_reload_override_native(CSS_PATH!(), Duration::from_millis(500))).unwrap();

    #[cfg(not(debug_assertions))]
    let window = Window::new(WindowCreateOptions::default(), css::override_native(include_str!(CSS_PATH!())).unwrap()).unwrap();

    app.run(window).unwrap();
}
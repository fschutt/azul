extern crate azul;

use azul::prelude::*;
use std::time::Duration;

macro_rules! XML_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/xml/ui.xml")) }
macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/xml/xml.css")) }

struct DataModel { }

impl Layout for DataModel {
    fn layout(&self, _: LayoutInfo<DataModel>) -> Dom<DataModel> {
        DomXml::from_file(XML_PATH!(), &mut XmlComponentMap::default()).into()
    }
}

fn main() {

    let mut app = App::new(DataModel { }, AppConfig::default()).unwrap();

    #[cfg(debug_assertions)]
    let window = {
        let hot_reloader = css::hot_reload_override_native(CSS_PATH!(), Duration::from_millis(500));
        app.create_hot_reload_window(WindowCreateOptions::default(), hot_reloader).unwrap()
    };

    #[cfg(not(debug_assertions))]
    let window = {
        let css = css::override_native(include_str!(CSS_PATH!())).unwrap();
        app.create_window(WindowCreateOptions::default(), css).unwrap()
    };

    app.run(window).unwrap();
}
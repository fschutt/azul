#![windows_subsystem = "windows"]

extern crate azul;

use azul::prelude::*;

const TEST_IMAGE: &[u8] = include_bytes!("../assets/images/cat_image.jpg");

struct MyDataModel;

impl Layout for MyDataModel {
    fn layout(&self, info: WindowInfo<Self>) -> Dom<Self> {
        Dom::new(NodeType::Div).with_id("wrapper")
            .with_child(Dom::new(NodeType::Label(format!("Hello123"))).with_id("red"))
            .with_child(Dom::new(NodeType::Div).with_id("sub-wrapper")
                .with_child(Dom::new(NodeType::Div).with_id("yellow")
                    .with_child(Dom::new(NodeType::Div).with_id("orange-1"))
                    .with_child(Dom::new(NodeType::Div).with_id("orange-2"))
                )
                .with_child(Dom::new(NodeType::Div).with_id("grey"))
            )
            .with_child(Dom::new(NodeType::Image(info.resources.get_image("Cat01").unwrap())).with_id("cat"))
    }
}

fn main() {

    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/examples/hot_reload.css")) }

    #[cfg(debug_assertions)]
    let css = Css::hot_reload(CSS_PATH!()).unwrap();
    #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(CSS_PATH!())).unwrap();

    let mut app = App::new(MyDataModel, AppConfig::default());
    app.add_image("Cat01", &mut TEST_IMAGE, ImageType::Jpeg).unwrap();
    app.run(Window::new(WindowCreateOptions::default(), css).unwrap()).unwrap();
}
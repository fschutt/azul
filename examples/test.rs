extern crate azul;

use azul::prelude::*;

struct MyDataModel;

impl Layout for MyDataModel {
    fn layout(&self, _info: WindowInfo) -> Dom<Self> {
        Dom::new(NodeType::Div).with_id("wrapper")
            .with_child(Dom::new(NodeType::Label(format!("Hello World"))).with_id("red"))
            .with_child(Dom::new(NodeType::Div).with_id("sub-wrapper")
                .with_child(Dom::new(NodeType::Div).with_id("yellow")
                    .with_child(Dom::new(NodeType::Div).with_id("below-yellow")))
                .with_child(Dom::new(NodeType::Div).with_id("grey"))
            )
    }
}

fn main() {
    const CSS_PATH: &str = "/please/use/an/absolute/file/path/../test.css";

    #[cfg(debug_assertions)]
    let css = Css::hot_reload(CSS_PATH).unwrap();
    #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(CSS_PATH)).unwrap();

    let mut app = App::new(MyDataModel, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
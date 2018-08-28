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

    // workaround for: https://github.com/rust-lang/rust/issues/53749
    macro_rules! css_path { () => ("/please/use/an/absolute/file/path/../hot_reload.css") }

    #[cfg(debug_assertions)]
    let css = Css::hot_reload(css_path!()).unwrap();
    #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(css_path!())).unwrap();

    let mut app = App::new(MyDataModel, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
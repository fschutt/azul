extern crate azul;

use azul::prelude::*;

struct MyDataModel {
    counter: usize,
}

impl Layout for MyDataModel {
    fn layout(&self, _info: WindowInfo) -> Dom<Self> {
        Dom::new(NodeType::Div).with_id("wrapper")
            .with_child(Dom::new(NodeType::Label(format!("{}", self.counter))).with_id("red"))
            .with_child(Dom::new(NodeType::Div).with_id("sub-wrapper")
                .with_child(Dom::new(NodeType::Div).with_id("yellow")
                    .with_child(Dom::new(NodeType::Div).with_id("below-yellow")))
                .with_child(Dom::new(NodeType::Div).with_id("grey"))
            )
    }
}

fn main() {
    let css = Css::hot_reload("C:/please/use/an/absolute/pathname/../test.css").unwrap();
    let mut app = App::new(MyDataModel { counter: 0 }, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
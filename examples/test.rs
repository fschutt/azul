extern crate azul;

use azul::prelude::*;

struct MyDataModel {
    counter: usize,
}

impl Layout for MyDataModel {
    fn layout(&self, _info: WindowInfo) -> Dom<Self> {
        Dom::new(NodeType::Div).with_id("wrapper")
            .with_child(Dom::new(NodeType::Label(format!("{}", self.counter))).with_id("red"))
            .with_child(Dom::new(NodeType::Div).with_id("green"))
    }
}

fn main() {

    let css = Css::new_from_str("
            #wrapper {
                background-color: blue;
                flex-direction: column;
            }
            #green {
                background-color: green;
                width: 200px;
                height: 200px;
            }
            #red {
                background-color: red;
                color: white;
                font-size: 10px;
                font-family: sans-serif;
                width: 50px;
                height: 50px;
            }
    ").unwrap();

    let mut app = App::new(MyDataModel { counter: 0 }, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
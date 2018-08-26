extern crate azul;

use azul::prelude::*;

struct MyDataModel {
    counter: usize,
}

impl Layout for MyDataModel {
    fn layout(&self, _info: WindowInfo) -> Dom<Self> {
        Dom::new(NodeType::Div).with_id("wrapper")
            .with_child(Dom::new(NodeType::Label(format!("{}", self.counter))).with_id("red"))
            .with_child(Dom::new(NodeType::Div).with_id("green")
                .with_child(Dom::new(NodeType::Div).with_id("yellow"))
                .with_child(Dom::new(NodeType::Div).with_id("grey"))
            )
    }
}

fn main() {

    let css = Css::new_from_str("
            #wrapper {
                background-color: blue;
                flex-direction: row-reverse;
            }
            #red {
                background-color: red;
                color: white;
                font-size: 10px;
                font-family: sans-serif;
                width: 50px;
            }
            #green {
                background-color: green;
                flex-direction: column;
                width: 500px;
            }
            #yellow {
                background-color: yellow;
                height: 200px;
            }
            #grey {
                background-color: grey;
            }
    ").unwrap();

    let mut app = App::new(MyDataModel { counter: 0 }, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
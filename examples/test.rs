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

    let css = Css::new_from_str("
            #wrapper {
                background: linear-gradient(135deg, #004e92 0%,#000428 100%);
                flex-direction: row;
            }
            #red {
                background-color: red;
                color: white;
                font-size: 10px;
                font-family: sans-serif;
                width: 50px;
            }
            #sub-wrapper {
                flex-direction: column-reverse;
                width: 500px;
            }
            #yellow {
                background-color: yellow;
                height: 200px;
                flex-direction: row-reverse;
            }
            #below-yellow {
                background-color: red;
                width: 50px;
                height: 50px;
            }
            #grey {
                background-color: grey;
            }
    ").unwrap();

    let mut app = App::new(MyDataModel { counter: 0 }, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
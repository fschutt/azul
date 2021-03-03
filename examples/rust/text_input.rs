#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::prelude::*;
use azul_widgets::text_input::*;

const CSS: &str = "
#text_input_1 {
    border-radius: 2px;
    width: 200px;
    height: 20px;
    border: 2px solid transparent;
    background-color: #ccc;
    position: absolute;
    top: 40px;
    left: 40px;
}

#text_input_1 p {
    font-size: 12px;
}

#text_input_1:focus {
    border: 2px solid  #80ff80;
}
";

struct TestCrudApp {
    text_input: Ref<TextInputState>,
}

impl Default for TestCrudApp {
    fn default() -> Self {
        Self {
            text_input: Ref::new(TextInputState::new("Hover mouse over rectangle and press keys")),
        }
    }
}

impl Layout for TestCrudApp {
    fn layout(&self, _: LayoutInfo) -> Dom<Self> {
        TextInput::new(self.text_input.clone()).dom().with_id("text_input_1")
    }
}

fn main() {
    let app = App::new(TestCrudApp::default(), AppConfig::default()).unwrap();
    let css = css::override_native(CSS).unwrap();
    app.run(WindowCreateOptions::new(css));
}

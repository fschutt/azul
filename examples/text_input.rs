extern crate azul;

use azul::prelude::*;
use azul::widgets::text_input::*;

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
    font-size: 10px;
}

#text_input_1:focus {
    border: 2px solid  #80ff80;
}
";

struct TestCrudApp {
    text_input: TextInputState,
}

impl Default for TestCrudApp {
    fn default() -> Self {
        Self {
            text_input: TextInputState::new("Hover mouse over rectangle and press keys")
        }
    }
}

impl Layout for TestCrudApp {
    fn layout(&self, info: LayoutInfo<Self>) -> Dom<Self> {
        TextInput::new()
        .bind(info.window, &self.text_input, &self)
        .dom(&self.text_input)
        .with_id("text_input_1")
    }
}

fn main() {
    let app = App::new(TestCrudApp::default(), AppConfig::default());
    app.run(Window::new(WindowCreateOptions::default(), css::override_native(CSS).unwrap()).unwrap()).unwrap();
}

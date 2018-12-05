extern crate azul;

use azul::prelude::*;
use azul::widgets::text_input::*;

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
    fn layout(&self, info: WindowInfo<Self>) -> Dom<Self> {
        TextInput::new()
        .bind(info.window, &self.text_input, &self)
        .dom(&self.text_input)
    }
}

fn main() {
    let app = App::new(TestCrudApp::default(), AppConfig::default());
    app.run(Window::new(WindowCreateOptions::default(), native()).unwrap()).unwrap();
}

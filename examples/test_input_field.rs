extern crate azul;

use azul::prelude::*;
use azul::widgets::*;

struct TestCrudApp {
    text_input: TextInputOutcome,
}

impl Default for TestCrudApp {
    fn default() -> Self {
        Self {
            text_input: TextInputOutcome::new("Hover mouse over rectangle and press keys")
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
    let mut app = App::new(TestCrudApp::default(), AppConfig::default());
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    app.run().unwrap();
}
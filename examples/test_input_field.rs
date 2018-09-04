extern crate azul;

use azul::prelude::*;
use azul::widgets::*;

struct TestCrudApp {
    text_input: TextInputOutcome,
}

impl Default for TestCrudApp {
    fn default() -> Self {
        Self {
            text_input: TextInputOutcome {
               text: "Hover mouse over rectangle and press keys".into(),
            }
        }
    }
}

impl Layout for TestCrudApp {
    fn layout(&self, info: WindowInfo<Self>) -> Dom<Self> {
        Dom::new(NodeType::Div)
        .with_id("parent")
        .with_child(
            Dom::new(NodeType::Div)
            .with_id("wrapper_1")
            .with_child(
                TextInput::new()
                .bind(info.window, &self.text_input, &self)
                .dom(&self.text_input)
            )
        )
    }
}

fn main() {
    macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/examples/test_input_field.css")) }

    #[cfg(debug_assertions)]
    let css = Css::hot_reload(CSS_PATH!()).unwrap();
    #[cfg(not(debug_assertions))]
    let css = Css::new_from_str(include_str!(CSS_PATH!())).unwrap();

    let mut app = App::new(TestCrudApp::default(), AppConfig::default());
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}
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
    fn layout(&self, _: WindowInfo) -> Dom<Self> {
        Dom::new(NodeType::Div)
        .with_id("parent")
        .with_child(
            Dom::new(NodeType::Div)
            .with_id("wrapper_1")
            .with_child(
                TextInput::new()
                .bind(&self.text_input)
                .dom(&self.text_input)
                .with_callback(On::KeyDown, Callback(update_text_field))
            )
        )
    }
}

fn update_text_field(app_state: &mut AppState<TestCrudApp>, event: WindowEvent) -> UpdateScreen {
    app_state.data.modify(|state| state.text_input.update(&app_state.windows, &event));
    UpdateScreen::Redraw
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
extern crate azul;

use azul::prelude::*;

struct TestCrudApp {
    text: String,
}

impl Default for TestCrudApp {
    fn default() -> Self {
        Self {
            text: "Hover mouse over rectangle and press keys".into(),
        }
    }
}

impl Layout for TestCrudApp {
    fn layout(&self, _: WindowInfo) -> Dom<Self> {
        Dom::new(NodeType::Div).with_id("parent")
        .with_child(
            Dom::new(NodeType::Div)
            .with_id("wrapper_1")
            .with_child(
                Dom::new(NodeType::Div)
                .with_id("input_field")
                .with_callback(On::KeyDown, Callback(update_text_field))
                .with_child(
                    Dom::new(NodeType::Label(self.text.clone()))
                    .with_id("label")
                )
            )
        )
    }
}

fn update_text_field(app_state: &mut AppState<TestCrudApp>, event: WindowEvent) -> UpdateScreen {
    let keyboard_state = app_state.windows[event.window].get_keyboard_state();

    if keyboard_state.current_virtual_keycodes.contains(&VirtualKeyCode::Back) {
        app_state.data.modify(|state| { state.text.pop(); });
    } else {
        let shift_active = keyboard_state.shift_down;
        let mut keys = keyboard_state.current_keys.iter().cloned().collect::<String>();
        if shift_active {
            keys = keys.to_uppercase();
        }
        app_state.data.modify(|state| state.text += &keys);
    }

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
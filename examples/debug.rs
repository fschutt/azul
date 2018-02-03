extern crate azul;

use azul::prelude::*;

const TEST_CSS: &str = include_str!("test_content.css");

pub struct MyAppData {
    // Your app data goes here
    pub my_data: u32,
}

impl LayoutScreen for MyAppData {

    fn get_dom(&self, _window_id: WindowId) -> Dom<MyAppData> {
        Dom::new(NodeType::Div)
            .class("__azul-native-button")
            .event(On::MouseUp, Callback::Sync(my_button_click_handler))
        .add_sibling(Dom::new(NodeType::Label { 
            text: "Hello World".into(),
        }))
    }
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>) -> UpdateScreen {
    app_state.data.my_data += 1;
    println!("app_state.data.my_data: {:?}", app_state.data.my_data);
    UpdateScreen::DontRedraw
}

fn main() {
    let css = Css::new_from_string(TEST_CSS).unwrap();

    let my_app_data = MyAppData {
        my_data: 0,
    };

    let mut app = App::new(my_app_data);
    // TODO: Multi-window apps currently crash
    // Need to re-factor the event loop for that
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.start_render_loop();
}

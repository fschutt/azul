extern crate azul;

use azul::prelude::*;

const TEST_CSS: &str = include_str!("test_content.css");
const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/weblysleekuil.ttf");
const TEST_IMAGE: &[u8] = include_bytes!("../assets/images/cat_image.jpg");

#[derive(Debug)]
pub struct MyAppData {
    // Your app data goes here
    pub my_data: u32,
}

impl LayoutScreen for MyAppData {
    fn get_dom(&self, _window_id: WindowId) -> Dom<MyAppData> {
        Dom::new(NodeType::Label(format!("{}", self.my_data)))
            .with_class("__azul-native-button")
            .with_event(On::MouseUp, Callback(my_button_click_handler))
    }
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>) -> UpdateScreen {

    let should_start_deamon = {
        let mut app_state_lock = app_state.data.lock().unwrap();
        app_state_lock.my_data += 1;
        app_state_lock.my_data % 2 == 0
    };

    if should_start_deamon {
        app_state.add_deamon("hello", deamon_test_start);
    } else {
        app_state.delete_deamon("hello");
    }
    UpdateScreen::Redraw
}

fn deamon_test_start(app_state: &mut MyAppData) -> UpdateScreen {
    println!("Hello!");
    UpdateScreen::DontRedraw
}

fn main() {

    // Parse and validate the CSS
    let css = Css::new_from_string(TEST_CSS).unwrap();

    let my_app_data = MyAppData {
        my_data: 0,
    };

    let mut app = App::new(my_app_data);

    app.add_font("Webly Sleeky UI", &mut TEST_FONT).unwrap();
    // app.delete_font("Webly Sleeky UI");

    app.add_image("Cat01", &mut TEST_IMAGE, ImageType::Jpeg).unwrap();
    // app.delete_image("Cat01");

    // TODO: Multi-window apps currently crash
    // Need to re-factor the event loop for that
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run();
}

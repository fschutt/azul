extern crate azul;

use azul::prelude::*;
use std::collections::HashMap;

const TEST_CSS: &str = include_str!("test_content.css");
const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/weblysleekuil.ttf");
const TEST_IMAGE: &[u8] = include_bytes!("../assets/images/cat_image.jpg");

#[derive(Debug)]
pub struct MyAppData {
    // Your app data goes here
    pub my_data: u32,
    pub fonts: HashMap<String, FontId>,
    pub images: HashMap<String, ImageId>,
}

impl LayoutScreen for MyAppData {

    fn get_dom(&self, _window_id: WindowId) -> Dom<MyAppData> {

        let mut dom = Dom::new(NodeType::Div);
        dom.class("__azul-native-button");
        dom.event(On::MouseUp, Callback::Sync(my_button_click_handler));
        
        for i in 0..self.my_data {
            dom.add_sibling(Dom::new(NodeType::Label { 
                text: format!("{}", i),
            }));
        }

        dom
    }
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>) -> UpdateScreen {
    app_state.data.my_data += 1;
    UpdateScreen::Redraw
}

fn main() {
    let css = Css::new_from_string(TEST_CSS).unwrap();

    let mut fonts = HashMap::new();
    fonts.insert("Roboto".to_string(), azul::resources::new_font_id());

    let my_app_data = MyAppData {
        my_data: 0,
        fonts: fonts,
        images: HashMap::new(),
    };

    let mut app = App::new(my_app_data);
    
    app.add_font("Webly Sleeky UI", TEST_FONT); // adds a new font to use in the CSS
    // app.remove_font("Webly Sleeky UI"); // removes a font and all font instances

    app.add_image("MyImage", TEST_IMAGE); // adds an image
    // app.remove_image("MyImage"); // removes an image

    // TODO: Multi-window apps currently crash
    // Need to re-factor the event loop for that
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.start_render_loop();
}

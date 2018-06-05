extern crate azul;

use azul::prelude::*;
use azul::widgets::*;

const TEST_CSS: &str = include_str!("test_content.css");
const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/weblysleekuil.ttf");
const TEST_IMAGE: &[u8] = include_bytes!("../assets/images/cat_image.jpg");
const TEST_SVG: &[u8] = include_bytes!("../assets/svg/test.svg");

#[derive(Debug)]
pub struct MyAppData {
    // Your app data goes here
    pub my_data: u32,
    // SVG IDs
    pub my_svg_ids: Vec<SvgLayerId>,
}

impl Layout for MyAppData {
    fn layout(&self, info: WindowInfo)
    -> Dom<MyAppData>
    {
        Svg::empty().dom(&info.window)
    }
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>, _event: WindowEvent) -> UpdateScreen {
    // Load and parse the SVG file, register polygon data as IDs
/*
    let mut svg_ids = app_state.add_svg(TEST_SVG).unwrap();
    app_state.data.modify(|data| data.my_svg_ids.append(&mut svg_ids));
*/
    UpdateScreen::Redraw
}

fn main() {

    // Parse and validate the CSS
    let css = Css::new_from_string(TEST_CSS).unwrap();

    let my_app_data = MyAppData {
        my_data: 0,
        my_svg_ids: Vec::new(),
    };

    let mut app = App::new(my_app_data);

    app.add_font("Webly Sleeky UI", &mut TEST_FONT).unwrap();
    // app.delete_font("Webly Sleeky UI");

    app.add_image("Cat01", &mut TEST_IMAGE, ImageType::Jpeg).unwrap();
    // app.delete_image("Cat01");

    // app.create_window(WindowCreateOptions::default(), css.clone()).unwrap();
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}

extern crate azul;

use azul::prelude::*;

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

impl LayoutScreen for MyAppData {
    fn get_dom(&self, _window_id: WindowId) -> Dom<MyAppData> {
        Dom::new(NodeType::Label(format!(
            "Lorem ipsum dolor sit amet, consetetur sadipscing elitr, \
            sed diam nonumy eirmod tempor invidunt ut labore et dolore magna aliquyam erat, sed diam \
            voluptua. At vero eos et accusam et justo duo dolores et ea rebum. Stet clita kasd gubergren, \
            no sea takimata sanctus est Lorem ipsum dolor sit amet. Lorem ipsum dolor sit amet, \
            consetetur sadipscing elitr, sed diam nonumy eirmod tempor invidunt ut labore et dolore \
            magna aliquyam erat, sed diam voluptua. At vero eos et accusam et justo duo dolores et \
            ea rebum. Stet clita kasd gubergren, no sea takimata sanctus est Lorem ipsum dolor sit amet.")))
            .with_class("__azul-native-button")
            .with_event(On::MouseUp, Callback(my_button_click_handler))
    }
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>, event: WindowEvent) -> UpdateScreen {

    // TODO: The DisplayList does somehow not register / override the new ID
    // This is probably an issue of timing, see the notes in the app.rs file
    app_state.windows[event.window].css.set_dynamic_property("my_id", ("width", "500px")).unwrap();

    // This works: When the mouse is moved over the button, the title switches to "Hello".
    // TODO: performance optimize this
    app_state.windows[event.window].state.title = String::from("Hello");

    // SVG parsing test
    let mut svg_ids = app_state.add_svg(TEST_SVG).unwrap();
    println!("{:?}", svg_ids);
    app_state.data.modify(|data| data.my_svg_ids.append(&mut svg_ids));

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

    // TODO: Multi-window apps currently crash
    // Need to re-factor the event loop for that
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run();
}

#![windows_subsystem = "windows"]

extern crate azul;

use azul::prelude::*;
use azul::widgets::*;

const TEST_CSS: &str = include_str!("test_content.css");
const TEST_FONT: &[u8] = include_bytes!("../assets/fonts/weblysleekuil.ttf");
const TEST_SVG: &[u8] = include_bytes!("../assets/svg/tiger.svg");

#[derive(Debug)]
pub struct MyAppData {
    pub svg: Option<(SvgCache<MyAppData>, Vec<SvgLayerId>)>,
}

impl Layout for MyAppData {
    fn layout(&self, info: WindowInfo)
    -> Dom<MyAppData>
    {
        if let Some((svg_cache, svg_layers)) = &self.svg {
            Svg::with_layers(svg_layers).dom(&info.window, &svg_cache)
        } else {
            Dom::new(NodeType::Div)
                .with_class("__azul-native-button")
                .with_callback(On::MouseUp, Callback(my_button_click_handler))
        }
    }
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>, _event: WindowEvent) -> UpdateScreen {
    let mut svg_cache = SvgCache::empty();
    let svg_layers = svg_cache.add_svg(TEST_SVG).unwrap();
    app_state.data.modify(|data| data.svg = Some((svg_cache, svg_layers)));
    UpdateScreen::Redraw
}

fn main() {

    // Parse and validate the CSS
    let css = Css::new_from_string(TEST_CSS).unwrap();
    let mut app = App::new(MyAppData { svg: None });

    app.add_font("Webly Sleeky UI", &mut TEST_FONT).unwrap();
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}

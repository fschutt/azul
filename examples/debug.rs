#![windows_subsystem = "windows"]

extern crate azul;

use azul::prelude::*;
use azul::widgets::*;
use azul::dialogs::*;
use std::fs;

const TEST_CSS: &str = include_str!("test_content.css");

#[derive(Debug)]
pub struct MyAppData {
    pub map: Option<Map>,
}

#[derive(Debug)]
pub struct Map {
    pub cache: SvgCache<MyAppData>,
    pub layers: Vec<SvgLayerId>,
    pub zoom: f32,
    pub pan_horz: f32,
    pub pan_vert: f32,
}

impl Layout for MyAppData {
    fn layout(&self, info: WindowInfo)
    -> Dom<MyAppData>
    {
        if let Some(map) = &self.map {
            Svg::with_layers(map.layers.clone())
                .with_pan(map.pan_horz, map.pan_vert)
                .with_zoom(map.zoom)
                .dom(&info.window, &map.cache)
                .with_callback(On::Scroll, Callback(scroll_map_contents))
        } else {
            Button::with_label("Load SVG").dom()
                .with_callback(On::LeftMouseUp, Callback(my_button_click_handler))
        }
    }
}

fn scroll_map_contents(app_state: &mut AppState<MyAppData>, event: WindowEvent) -> UpdateScreen {
    app_state.data.modify(|data| {
        if let Some(map) = data.map.as_mut() {
            let mouse_state = app_state.windows[event.window].get_mouse_state();
            map.pan_horz += mouse_state.scroll_x;
            map.pan_vert += mouse_state.scroll_y;
        }
    });

    UpdateScreen::Redraw
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>, _event: WindowEvent) -> UpdateScreen {
    open_file_dialog(None, None)
        .and_then(|path| fs::read_to_string(path.clone()).ok())
        .and_then(|contents| {
            let mut svg_cache = SvgCache::empty();
            let svg_layers = svg_cache.add_svg(&contents).ok()?;
            app_state.data.modify(|data| data.map = Some(Map {
                cache: svg_cache,
                layers: svg_layers,
                zoom: 1.0,
                pan_horz: 0.0,
                pan_vert: 0.0,
            }));
            Some(UpdateScreen::Redraw)
        })
        .unwrap_or_else(|| {
            UpdateScreen::DontRedraw
        })
}

fn main() {

    // Parse and validate the CSS
    let css = Css::new_from_string(TEST_CSS).unwrap();
    let mut app = App::new(MyAppData { map: None });
    app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run().unwrap();
}

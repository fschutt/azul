#![windows_subsystem = "windows"]

extern crate azul;

use azul::prelude::*;
use azul::widgets::*;
use azul::dialogs::*;
use std::fs;

#[derive(Debug)]
pub struct MyAppData {
    pub map: Option<Map>,
}

#[derive(Debug)]
pub struct Map {
    pub cache: SvgCache<MyAppData>,
    pub layers: Vec<SvgLayerId>,
    pub zoom: f64,
    pub pan_horz: f64,
    pub pan_vert: f64,
}

impl Layout for MyAppData {
    fn layout(&self, info: WindowInfo)
    -> Dom<MyAppData>
    {
        if let Some(map) = &self.map {
            Svg::with_layers(map.layers.clone())
                .with_pan(map.pan_horz as f32, map.pan_vert as f32)
                .with_zoom(map.zoom as f32)
                .dom(&info.window, &map.cache)
                .with_callback(On::Scroll, Callback(scroll_map_contents))
        } else {
            // TODO: If this is changed to Label::new(), the text is cut off at the top
            // because of the (offset_top / 2.0) - see text_layout.rs file
            Button::with_label("Open SVG file...").dom()
               .with_callback(On::LeftMouseUp, Callback(my_button_click_handler))
        }
    }
}

fn scroll_map_contents(app_state: &mut AppState<MyAppData>, event: WindowEvent) -> UpdateScreen {
    app_state.data.modify(|data| {
        if let Some(map) = data.map.as_mut() {

            let mouse_state = app_state.windows[event.window].get_mouse_state();
            let keyboard_state = app_state.windows[event.window].get_keyboard_state();

            if keyboard_state.shift_down {
                map.pan_horz += mouse_state.scroll_y;
            } else if keyboard_state.ctrl_down {
                if mouse_state.scroll_y.is_sign_positive() {
                    map.zoom /= 2.0;
                } else {
                    map.zoom *= 2.0;
                }
            } else {
                map.pan_vert += mouse_state.scroll_y;
            }
        }
    });

    UpdateScreen::Redraw
}

fn my_button_click_handler(app_state: &mut AppState<MyAppData>, _event: WindowEvent) -> UpdateScreen {
    open_file_dialog(None, None)
        .and_then(|path| fs::read_to_string(path.clone()).ok())
        .and_then(|contents| {

            let mut svg_cache = SvgCache::empty();

            let (font, _) = app_state.get_font(&FontId::BuiltinFont("sans-serif"))?;

            let text_layer = LayerType::from_single_layer(SvgLayerType::from_character('a', font).1);
            let svg_layer = SvgLayer::default_from_layer(text_layer, SvgStyle::filled(ColorU { r: 0, b: 0, g: 0, a: 200 }));

            let mut svg_layers = svg_cache.add_svg(&contents).ok()?;

            let text_layer_id = svg_cache.add_layer(svg_layer);
            svg_layers.push(text_layer_id);

            {
                use azul::text_layout::*;
                let font_metrics = FontMetrics::new(font, &FontSize::px(10.0), None);
                let layout = layout_text("Hello World", font, &font_metrics);
                println!("text layout glyphs: {:?}", layout.layouted_glyphs);
                println!("text min w: {} min h: {}", layout.min_width, layout.min_height);
            }

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
    let mut app = App::new(MyAppData { map: None }, AppConfig::default());
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    app.run().unwrap();
}
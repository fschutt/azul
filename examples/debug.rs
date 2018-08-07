#![windows_subsystem = "windows"]

extern crate azul;

use azul::prelude::*;
use azul::widgets::*;
use azul::dialogs::*;

use std::fs;

const FONT_ID: FontId = FontId::BuiltinFont("sans-serif");
const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/weblysleekuil.ttf");

#[derive(Debug)]
pub struct MyAppData {
    pub map: Option<Map>,
}

#[derive(Debug)]
pub struct Map {
    pub cache: SvgCache<MyAppData>,
    pub layers: Vec<SvgLayerId>,
    pub font_cache: VectorizedFontCache,
    pub zoom: f64,
    pub pan_horz: f64,
    pub pan_vert: f64,
}

impl Layout for MyAppData {
    fn layout(&self, info: WindowInfo)
    -> Dom<MyAppData>
    {
        if let Some(map) = &self.map {
            Svg::with_layers(build_layers(&map.layers, &map.font_cache, &info.resources))
                .with_pan(map.pan_horz as f32, map.pan_vert as f32)
                .with_zoom(map.zoom as f32)
                .dom(&info.window, &map.cache)
                .with_callback(On::Scroll, Callback(scroll_map_contents))
        } else {
            // TODO: If this is changed to Label::new(), the text is cut off at the top
            // because of the (offset_top / 2.0) - see text_layout.rs file
            Button::with_label("Load SVG file...").dom()
                .with_callback(On::LeftMouseUp, Callback(my_button_click_handler))
        }
    }
}

fn build_layers(existing_layers: &[SvgLayerId], vector_font_cache: &VectorizedFontCache, resources: &AppResources)
-> Vec<SvgLayerResource>
{
    let mut layers: Vec<SvgLayerResource> = existing_layers.iter().map(|e| SvgLayerResource::Reference(*e)).collect();

    let font_id = FontId::ExternalFont(String::from("Webly Sleeky UI"));
    let curve = SampledBezierCurve::from_curve(&[
        BezierControlPoint { x: 0.0, y: 0.0 },
        BezierControlPoint { x: 40.0, y: 120.0 },
        BezierControlPoint { x: 80.0, y: 120.0 },
        BezierControlPoint { x: 120.0, y: 0.0 },
    ]);
    let font_size = FontSize::px(10.0);
    let font = resources.get_font(&font_id).unwrap().0;
    let text_layout_1 = SvgTextLayout::from_str("On Curve!!!!", &font, &font_size);
    let text_layout_2 = SvgTextLayout::from_str("Rotated", &font, &font_size);
    let text_layout_3 = SvgTextLayout::from_str("Unmodified", &font, &font_size);

    layers.push(SvgText {
        font_size: font_size,
        font_id: &font_id,
        text_layout: &text_layout_1,
        style: SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 }),
        placement: SvgTextPlacement::OnCubicBezierCurve(curve),
    }.to_svg_layer(vector_font_cache, resources));

    layers.push(SvgText {
        font_size: font_size,
        font_id: &font_id,
        text_layout: &text_layout_2,
        style: SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 }),
        placement: SvgTextPlacement::Rotated(-30.0),
    }.to_svg_layer(vector_font_cache, resources));

    layers.push(SvgText {
        font_size: font_size,
        font_id: &font_id,
        text_layout: &text_layout_3,
        style: SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 }),
        placement: SvgTextPlacement::Unmodified,
    }.to_svg_layer(vector_font_cache, resources));

    layers.push(curve.draw_lines());
    layers.push(curve.draw_control_handles());

    layers
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
            let svg_layers = svg_cache.add_svg(&contents).ok()?;

            let font_id = FontId::ExternalFont(String::from("Webly Sleeky UI"));

            // Pre-vectorize the glyphs of the font into vertex buffers
            let (font, _) = app_state.get_font(&font_id)?;
            let mut vectorized_font_cache = VectorizedFontCache::new();
            vectorized_font_cache.insert_if_not_exist(font_id, font);

            app_state.data.modify(|data| data.map = Some(Map {
                cache: svg_cache,
                font_cache: vectorized_font_cache,
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
    app.add_font("Webly Sleeky UI", &mut FONT_BYTES.clone()).unwrap();
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    app.run().unwrap();
}
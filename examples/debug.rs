#![windows_subsystem = "windows"]

extern crate azul;

use azul::prelude::*;
use azul::widgets::*;
use azul::dialogs::*;

use std::fs;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

const FONT_ID: FontId = FontId::BuiltinFont("sans-serif");
static TEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TextId(usize);

fn new_text_id() -> TextId {
    TextId(TEXT_ID.fetch_add(1, Ordering::SeqCst))
}

#[derive(Debug)]
pub struct MyAppData {
    pub map: Option<Map>,
}

#[derive(Debug)]
pub struct Map {
    pub cache: SvgCache<MyAppData>,
    pub layers: Vec<SvgLayerId>,
    pub font_cache: VectorizedFontCache,
    pub texts: HashMap<TextId, SvgText>,
    pub hovered_text: Option<TextId>,
    pub zoom: f64,
    pub pan_horz: f64,
    pub pan_vert: f64,
}

impl Layout for MyAppData {
    fn layout(&self, info: WindowInfo)
    -> Dom<MyAppData>
    {
        if let Some(map) = &self.map {
            Svg::with_layers(build_layers(&map.layers, &map.texts, &map.hovered_text, &map.font_cache, &info.resources))
                .with_pan(map.pan_horz as f32, map.pan_vert as f32)
                .with_zoom(map.zoom as f32)
                .dom(&info.window, &map.cache)
                .with_callback(On::Scroll, Callback(scroll_map_contents))
                .with_callback(On::MouseOver, Callback(check_hovered_font))
        } else {
            // TODO: If this is changed to Label::new(), the text is cut off at the top
            // because of the (offset_top / 2.0) - see text_layout.rs file
            Button::with_label("Load SVG file...").dom()
                .with_callback(On::LeftMouseUp, Callback(my_button_click_handler))
        }
    }
}

fn build_layers(
    existing_layers: &[SvgLayerId],
    texts: &HashMap<TextId, SvgText>,
    hovered_text: &Option<TextId>,
    vector_font_cache: &VectorizedFontCache,
    resources: &AppResources)
-> Vec<SvgLayerResource>
{
    let mut layers: Vec<SvgLayerResource> = existing_layers.iter().map(|e| SvgLayerResource::Reference(*e)).collect();

    layers.extend(texts.values().map(|text| text.to_svg_layer(vector_font_cache, resources)));
    layers.extend(texts.values().map(|text| text.get_bbox().draw_lines(ColorU { r: 0, g: 0, b: 0, a: 255 })));
/*
    if let Some(active) = hovered_text {
        layers.push(texts[active].get_bbox().draw_lines());
    }
*/
    // layers.push(curve.draw_lines());
    // layers.push(curve.draw_control_handles());

    layers
}

// Check what text was hovered over
fn check_hovered_font(app_state: &mut AppState<MyAppData>, event: WindowEvent) -> UpdateScreen {
    let (cursor_x, cursor_y) = event.cursor_relative_to_item;

    let mut should_redraw = UpdateScreen::DontRedraw;

    app_state.data.modify(|data| {
        if let Some(map) = data.map.as_mut() {
            for (k, v) in map.texts.iter() {
                if v.get_bbox().contains_point(cursor_x, cursor_y) {
                    map.hovered_text = Some(*k);
                    should_redraw = UpdateScreen::Redraw;
                    break;

                }
            }
        }
    });

    should_redraw
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

    let font_id = FONT_ID;
    let font_size = FontSize::px(10.0);
    let font = app_state.resources.get_font(&font_id).unwrap().0;

    // Texts only for testing
    let texts = [
        SvgText {
            font_size: font_size,
            font_id: font_id.clone(),
            text_layout: SvgTextLayout::from_str("On Curve!!!!", &font, &font_size),
            style: SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 }),
            placement: SvgTextPlacement::OnCubicBezierCurve(SampledBezierCurve::from_curve(&[
                BezierControlPoint { x: 0.0, y: 0.0 },
                BezierControlPoint { x: 40.0, y: 120.0 },
                BezierControlPoint { x: 80.0, y: 120.0 },
                BezierControlPoint { x: 120.0, y: 0.0 },
            ])),
            position: SvgPosition { x: 50.0, y: 50.0 },
        },
        SvgText {
            font_size: font_size,
            font_id: font_id.clone(),
            text_layout: SvgTextLayout::from_str("Rotated", &font, &font_size),
            style: SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 }),
            placement: SvgTextPlacement::Rotated(-30.0),
            position: SvgPosition { x: 50.0, y: 50.0 },
        },
        SvgText {
            font_size: font_size,
            font_id: font_id.clone(),
            text_layout: SvgTextLayout::from_str("Unmodified\nCool", &font, &font_size),
            style: SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 }),
            placement: SvgTextPlacement::Unmodified,
            position: SvgPosition { x: 50.0, y: 50.0 },
        },
    ];

    let mut cached_texts = HashMap::<TextId, SvgText>::new();
    for t in texts.into_iter() {
        let id = new_text_id();
        cached_texts.insert(id, t.clone());
    }

    open_file_dialog(None, None)
        .and_then(|path| fs::read_to_string(path.clone()).ok())
        .and_then(|contents| {

            let mut svg_cache = SvgCache::empty();
            let svg_layers = svg_cache.add_svg(&contents).ok()?;

            app_state.data.modify(|data| data.map = Some(Map {
                cache: svg_cache,
                font_cache: VectorizedFontCache::new(&app_state.resources),
                hovered_text: None,
                texts: cached_texts,
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
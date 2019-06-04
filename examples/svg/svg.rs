#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

extern crate azul;

use azul::{
    prelude::*,
    widgets::{button::Button, svg::*},
    dialogs::*,
};

use std::{
    fs,
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

const CSS: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/svg/svg.css"));

static TEXT_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TextId(usize);

impl TextId {
    pub fn new() -> Self { TextId(TEXT_ID.fetch_add(1, Ordering::SeqCst)) }
}

#[derive(Debug)]
pub struct MyAppData {
    pub map: Option<Map>,
}

#[derive(Debug)]
pub struct Map {
    pub cache: SvgCache,
    pub layers: Vec<(SvgLayerId, SvgStyle)>,
    pub font_cache: VectorizedFontCache,
    pub texts: HashMap<TextId, SvgText>,
    pub hovered_text: Option<TextId>,
    pub zoom: f32,
    pub pan_horz: f32,
    pub pan_vert: f32,
}

impl Layout for MyAppData {
    fn layout(&self, _info: LayoutInfo<Self>) -> Dom<MyAppData> {
        match &self.map {
            Some(map) => {
                Dom::div().with_id("parent-wrapper")
                    .with_child(Dom::div().with_id("child-1"))
                    .with_child(gl_texture_dom(&map, &self).with_id("child-2"))
            },
            None => Button::with_label("Load SVG file...").dom().with_callback(On::LeftMouseUp, my_button_click_handler),
        }
    }
}

fn gl_texture_dom(map: &Map, data: &MyAppData) -> Dom<MyAppData> {
    let ptr = StackCheckedPointer::new(data, map).unwrap();

    Dom::gl_texture(|info: GlCallbackInfoUnchecked<MyAppData>| unsafe {
        info.invoke_callback(|info: GlCallbackInfo<MyAppData, Map>| {
            let map = info.state;
            let physical_size = info.bounds.get_physical_size();
            let width = physical_size.width as usize;
            let height = physical_size.height as usize;
            let layers = build_layers(&map.layers, &map.texts, &map.hovered_text, &map.font_cache);

            Svg::with_layers(layers)
                .with_pan(map.pan_horz, map.pan_vert)
                .with_zoom(map.zoom)
                .render_svg(&map.cache, &info.layout_info.window, width, height)
        })
    }, ptr)
    .with_callback(On::Scroll, scroll_map_contents)
    .with_callback(On::MouseOver, check_hovered_font)
}

fn build_layers(
    existing_layers: &[(SvgLayerId, SvgStyle)],
    texts: &HashMap<TextId, SvgText>,
    hovered_text: &Option<TextId>,
    vector_font_cache: &VectorizedFontCache
) -> Vec<SvgLayerResource> {

    let mut layers: Vec<SvgLayerResource> = existing_layers.iter().map(|e| SvgLayerResource::Reference(*e)).collect();

    layers.extend(texts.values().filter_map(|text| text.to_svg_layer(vector_font_cache).map(SvgLayerResource::Direct)));
    layers.extend(texts.values().map(|text| {
        SvgLayerResource::Direct(text.get_bbox().draw_lines(ColorU::BLACK, 1.0))
    }));
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
fn check_hovered_font(info: CallbackInfo<MyAppData>) -> UpdateScreen {

    let (cursor_x, cursor_y) = info.cursor_relative_to_item?;
    let map = info.state.data.map.as_mut()?;

    map.texts.iter()
    .find_map(|(k, v)| if v.get_bbox().contains_point(cursor_x, cursor_y) { Some(*k) } else { None })
    .map(|k| map.hovered_text = Some(k))
}

fn scroll_map_contents(info: CallbackInfo<MyAppData>) -> UpdateScreen {

    let window_id = info.window_id;
    let map = info.state.data.map.as_mut()?;
    let mouse_state = info.state.windows.get(&window_id)?.get_mouse_state();
    let keyboard_state = info.state.windows.get(&window_id)?.get_keyboard_state();

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

    Redraw
}

fn my_button_click_handler(info: CallbackInfo<MyAppData>) -> UpdateScreen {

    use azul::resources::font_source_get_bytes;

    println!("my button click handler!");

    let font_size = 10.0;
    let font_id = info.state.resources.get_css_font_id("serif").cloned()?;
    let font_bytes = info.state.resources.get_font_source(&font_id).cloned().map(|r| font_source_get_bytes(&r))?.ok()?;
    let font_bytes = font_source_get_bytes(&FontSource::native("serif")).ok()?;

    let text_style = SvgStyle {
        fill: Some(ColorU { r: 0, g: 0, b: 0, a: 255 }),
        transform: SvgTransform {
            translation: Some(SvgTranslation { x: 50.0, y: 50.0 }),
            .. Default::default()
        },
        .. Default::default()
    };

    // Texts only for testing
    let texts = [
        SvgText {
            font_size_px: font_size,
            font_id: font_id.clone(),
            text_layout: svg_text_layout_from_str(
                "On Curve!!!!",
                &font_bytes.0,
                font_bytes.1 as u32,
                ResolvedTextLayoutOptions::default(),
                StyleTextAlignmentHorz::default(),
            ),
            style: text_style,
            placement: SvgTextPlacement::OnCubicBezierCurve(SampledBezierCurve::from_curve(&[
                BezierControlPoint { x: 0.0, y: 0.0 },
                BezierControlPoint { x: 40.0, y: 120.0 },
                BezierControlPoint { x: 80.0, y: 120.0 },
                BezierControlPoint { x: 120.0, y: 0.0 },
            ])),
        },
        SvgText {
            font_size_px: font_size,
            font_id: font_id.clone(),
            text_layout: svg_text_layout_from_str(
                "Rotated",
                &font_bytes.0,
                font_bytes.1 as u32,
                ResolvedTextLayoutOptions::default(),
                StyleTextAlignmentHorz::default(),
            ),
            style: text_style,
            placement: SvgTextPlacement::Rotated(-30.0),
        },
        SvgText {
            font_size_px: font_size,
            font_id: font_id.clone(),
            text_layout: svg_text_layout_from_str(
                "Unmodified\nCool",
                &font_bytes.0,
                font_bytes.1 as u32,
                ResolvedTextLayoutOptions::default(),
                StyleTextAlignmentHorz::default(),
            ),
            style: text_style,
            placement: SvgTextPlacement::Unmodified,
        },
    ];

    let cached_texts = texts.into_iter().map(|t| (TextId::new(), t.clone())).collect();

    println!("opening file dialog!");

    open_file_dialog(None, None)
        .and_then(|path| fs::read_to_string(path.clone()).ok())
        .and_then(|contents| {

            let mut svg_cache = SvgCache::empty();
            let svg_layers = svg_cache.add_svg(&contents).ok()?;

            info.state.data.map = Some(Map {
                cache: svg_cache,
                font_cache: VectorizedFontCache::new(),
                hovered_text: None,
                texts: cached_texts,
                layers: svg_layers,
                zoom: 1.0,
                pan_horz: 0.0,
                pan_vert: 0.0,
            });

            Some(Redraw)
        })
        .unwrap_or(DontRedraw)
}

fn main() {
    let css = css::override_native(CSS).unwrap();
    let mut app = App::new(MyAppData { map: None }, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}

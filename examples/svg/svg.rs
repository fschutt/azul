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

macro_rules! CSS_PATH { () => (concat!(env!("CARGO_MANIFEST_DIR"), "/../examples/svg/svg.css")) }

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
    fn layout(&self, _info: LayoutInfo<Self>)
    -> Dom<MyAppData>
    {
        if let Some(map) = &self.map {
            Dom::new(NodeType::Div).with_id("parent-wrapper")
                .with_child(Dom::new(NodeType::Div).with_id("child-1"))
                .with_child(gl_texture_dom(&map, &self).with_id("child-2"))
        } else {
            // TODO: If this is changed to Label::new(), the text is cut off at the top
            // because of the (offset_top / 2.0) - see text_layout.rs file
            Button::with_label("Load SVG file...").dom()
                .with_callback(On::LeftMouseUp, Callback(my_button_click_handler))
        }
    }
}

fn gl_texture_dom(map: &Map, data: &MyAppData) -> Dom<MyAppData> {
    Dom::new(NodeType::GlTexture((GlTextureCallback(render_map_callback), StackCheckedPointer::new(data, map).unwrap()) ))
        .with_callback(On::Scroll, Callback(scroll_map_contents))
        .with_callback(On::MouseOver, Callback(check_hovered_font))
}

fn render_map_callback(ptr: &StackCheckedPointer<MyAppData>, window_info: LayoutInfo<MyAppData>, dimensions: HidpiAdjustedBounds) -> Texture {
    unsafe { ptr.invoke_mut_texture(render_map, window_info, dimensions) }
}

fn render_map(map: &mut Map, info: LayoutInfo<MyAppData>, dimensions: HidpiAdjustedBounds) -> Texture {
    let physical_size = dimensions.get_physical_size();
    Svg::with_layers(build_layers(&map.layers, &map.texts, &map.hovered_text, &map.font_cache, &info.resources))
        .with_pan(map.pan_horz as f32, map.pan_vert as f32)
        .with_zoom(map.zoom as f32)
        .render_svg(
            &map.cache, &info.window,
            physical_size.width as usize,
            physical_size.height  as usize,
        )
}

fn build_layers(
    existing_layers: &[(SvgLayerId, SvgStyle)],
    texts: &HashMap<TextId, SvgText>,
    hovered_text: &Option<TextId>,
    vector_font_cache: &VectorizedFontCache,
    resources: &AppResources)
-> Vec<SvgLayerResource>
{
    let mut layers: Vec<SvgLayerResource> = existing_layers.iter().map(|e| SvgLayerResource::Reference(*e)).collect();

    layers.extend(texts.values().map(|text| SvgLayerResource::Direct(text.to_svg_layer(vector_font_cache, resources))));
    layers.extend(texts.values().map(|text| SvgLayerResource::Direct(text.get_bbox().draw_lines(ColorU { r: 0, g: 0, b: 0, a: 255 }, 1.0))));
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
fn check_hovered_font(app_state: &mut AppState<MyAppData>, event: &mut CallbackInfo<MyAppData>) -> UpdateScreen {
    let (cursor_x, cursor_y) = event.cursor_relative_to_item?;

    let mut data = app_state.data.lock().ok()?;
    let map = data.map.as_mut()?;

    let mut should_redraw = DontRedraw;

    for (k, v) in map.texts.iter() {
        if v.get_bbox().contains_point(cursor_x, cursor_y) {
            map.hovered_text = Some(*k);
            should_redraw = Redraw;
            break;
        }
    }

    should_redraw
}

fn scroll_map_contents(app_state: &mut AppState<MyAppData>, event: &mut CallbackInfo<MyAppData>) -> UpdateScreen {

    let mut data = app_state.data.lock().ok()?;
    let map = data.map.as_mut()?;

    let mouse_state = app_state.windows.get(event.window_id)?.get_mouse_state();
    let keyboard_state = app_state.windows.get(event.window_id)?.get_keyboard_state();

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

fn my_button_click_handler(app_state: &mut AppState<MyAppData>, _event: &mut CallbackInfo<MyAppData>) -> UpdateScreen {

    use azul::resources::font_source_get_bytes;

    let font_size = 10.0;
    let font_id = app_state.resources.get_css_font_id("sans-serif")?;
    let font_bytes = app_state.resources.get_font_source(&font_id).map(font_source_get_bytes)?.ok()?;

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
            text_layout: SvgTextLayout::from_str(
                "On Curve!!!!",
                &font_bytes.0,
                font_bytes.1 as u32,
                &TextLayoutOptions::default(),
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
            text_layout: SvgTextLayout::from_str(
                "Rotated",
                &font_bytes.0,
                font_bytes.1 as u32,
                &TextLayoutOptions::default(),
                StyleTextAlignmentHorz::default(),
            ),
            style: text_style,
            placement: SvgTextPlacement::Rotated(-30.0),
        },
        SvgText {
            font_size_px: font_size,
            font_id: font_id.clone(),
            text_layout: SvgTextLayout::from_str(
                "Unmodified\nCool",
                &font_bytes.0,
                font_bytes.1 as u32,
                &TextLayoutOptions::default(),
                StyleTextAlignmentHorz::default(),
            ),
            style: text_style,
            placement: SvgTextPlacement::Unmodified,
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
                font_cache: VectorizedFontCache::new(),
                hovered_text: None,
                texts: cached_texts,
                layers: svg_layers,
                zoom: 1.0,
                pan_horz: 0.0,
                pan_vert: 0.0,
            }));

            Some(Redraw)
        })
        .unwrap_or(DontRedraw)
}

fn main() {
    let css = css::override_native(include_str!(CSS_PATH!())).unwrap();
    let mut app = App::new(MyAppData { map: None }, AppConfig::default()).unwrap();
    let window = app.create_window(WindowCreateOptions::default(), css).unwrap();
    app.run(window).unwrap();
}

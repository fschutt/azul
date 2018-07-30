#![windows_subsystem = "windows"]

extern crate azul;

use azul::prelude::*;
use azul::widgets::*;
use azul::dialogs::*;
use azul::text_layout::*;

use std::fs;

const FONT_ID: FontId = FontId::BuiltinFont("sans-serif");

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
            Button::with_label("Open SVG file...").dom()
               .with_callback(On::LeftMouseUp, Callback(my_button_click_handler))
        }
    }
}

fn build_layers(existing_layers: &[SvgLayerId], vector_font_cache: &VectorizedFontCache, resources: &AppResources)
-> Vec<SvgLayerResource>
{
    let mut layers: Vec<SvgLayerResource> = existing_layers.iter().map(|e| SvgLayerResource::Reference(*e)).collect();

    let text_style = SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 });
    let font = resources.get_font(&FONT_ID).unwrap();
    let vectorized_font = vector_font_cache.get_font(&FONT_ID).unwrap();
    let font_size = FontSize::px(10.0);
    let test_curve = SampledBezierCurve::from_curve(&[
        BezierControlPoint { x: 0.0, y: 0.0 },
        BezierControlPoint { x: 40.0, y: 120.0 },
        BezierControlPoint { x: 80.0, y: 120.0 },
        BezierControlPoint { x: 120.0, y: 0.0 },
    ]);

    layers.push(text_on_curve("Hello World", &test_curve, text_style, &font.0, vectorized_font, font_size));
    layers.push(test_curve.draw_circles());
    layers.push(test_curve.draw_lines());
    layers.push(test_curve.draw_control_handles());

    layers
}

fn text_on_curve(
    text: &str,
    curve: &SampledBezierCurve,
    text_style: SvgStyle,
    font: &Font,
    vector_font: &VectorizedFont,
    font_size: FontSize)
-> SvgLayerResource
{
    let font_metrics = FontMetrics::new(font, &font_size, None);
    let layout = layout_text(text, font, &font_metrics);

    let (char_offsets, char_rotations) = curve.get_text_offsets_and_rotations(&layout.layouted_glyphs, 0.0);

    let fill_vertices = text_style.fill.and_then(|_| {
        Some(vector_text_to_vertices(&font_size, &layout.layouted_glyphs, vector_font, font, &char_offsets, &char_rotations, get_fill_vertices))
    });

    let stroke_vertices = text_style.stroke.and_then(|_| {
        Some(vector_text_to_vertices(&font_size, &layout.layouted_glyphs, vector_font, font, &char_offsets, &char_rotations, get_stroke_vertices))
    });

    SvgLayerResource::Direct {
        style: text_style,
        fill: fill_vertices,
        stroke: stroke_vertices,
    }
}

// Calculates the layout for one word block
fn vector_text_to_vertices(
    font_size: &FontSize,
    glyph_ids: &[GlyphInstance],
    vectorized_font: &VectorizedFont,
    original_font: &Font,
    char_offsets: &[(f32, f32)],
    char_rotations: &[f32],
    transform_func: fn(&VectorizedFont, &Font, &GlyphId) -> Option<VertexBuffers<SvgVert>>
) -> VerticesIndicesBuffer
{
    let fill_buf = glyph_ids.iter()
        .filter_map(|gid| {
            // 1. Transform glyph to vertex buffer && filter out all glyphs
            //    that don't have a vertex buffer
            transform_func(vectorized_font, original_font, &GlyphId(gid.index))
        })
        .zip(char_rotations.into_iter())
        .zip(char_offsets.iter())
        .map(|((mut vertex_buf, char_rot), char_offset)| {

            let (char_offset_x, char_offset_y) = char_offset; // weird borrow issue

            // 2. Scale characters to the final size
            scale_vertex_buffer(&mut vertex_buf.vertices, font_size);

            // 3. Rotate individual characters inside of the word
            let char_angle = char_rot.to_radians();
            let (char_sin, char_cos) = (char_angle.sin(), char_angle.cos());

            rotate_vertex_buffer(&mut vertex_buf.vertices, char_sin, char_cos);

            // 4. Transform characters to their respective positions
            transform_vertex_buffer(&mut vertex_buf.vertices, *char_offset_x, *char_offset_y);

            vertex_buf
        })
        .collect::<Vec<_>>();

    join_vertex_buffers(&fill_buf)
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

            // Pre-vectorize the glyphs of the font into vertex buffers
            let (font, _) = app_state.get_font(&FONT_ID)?;
            let mut vectorized_font_cache = VectorizedFontCache::new();
            vectorized_font_cache.insert_if_not_exist(FONT_ID, font);

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
    app.create_window(WindowCreateOptions::default(), Css::native()).unwrap();
    app.run().unwrap();
}
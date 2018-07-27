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

    let cur_string = "Helloldakjfalfkjadlkfjdsalfkjdsalfkjdsf World";
    let font = resources.get_font(&FONT_ID).unwrap();
    let vectorized_font = vector_font_cache.get_font(&FONT_ID).unwrap();

    let font_size = FontSize::px(10.0);
    let font_metrics = FontMetrics::new(&font.0, &font_size, None);
    let layout = layout_text(&cur_string, &font.0, &font_metrics);

    let style = SvgStyle::filled(ColorU { r: 0, g: 0, b: 0, a: 255 });

    // Calculates the layout for one word block
    fn get_vertices(
        font_size: &FontSize,
        glyph_ids: &[GlyphInstance],
        vectorized_font: &VectorizedFont,
        original_font: &Font,
        char_offsets: Vec<(f32, f32)>,
        transform_func: fn(&VectorizedFont, &Font, &GlyphId) -> Option<VertexBuffers<SvgVert>>
    ) -> VerticesIndicesBuffer
    {
        let character_rotations = vec![30.0_f32; glyph_ids.len()];

        let fill_buf = glyph_ids.iter()
            .filter_map(|gid| {
                // 1. Transform glyph to vertex buffer && filter out all glyphs
                //    that don't have a vertex buffer
                transform_func(vectorized_font, original_font, &GlyphId(gid.index))
            })
            .zip(character_rotations.into_iter())
            .zip(char_offsets.into_iter())
            .map(|((mut vertex_buf, char_rot), (char_offset_x, char_offset_y))| {

                // 2. Scale characters to the final size
                scale_vertex_buffer(&mut vertex_buf.vertices, font_size);

                // 3. Rotate individual characters inside of the word
                let char_angle = char_rot.to_radians();
                let (char_sin, char_cos) = (char_angle.sin(), char_angle.cos());
                rotate_vertex_buffer(&mut vertex_buf.vertices, char_sin, char_cos);

                // 4. Transform characters to their respective positions
                transform_vertex_buffer(&mut vertex_buf.vertices, char_offset_x, char_offset_y);

                vertex_buf
            })
            .collect::<Vec<_>>();

        join_vertex_buffers(&fill_buf)
    }

    let (circle_layer, char_offsets) = test_bezier_points_offsets(&layout.layouted_glyphs, 0.0);

    let fill_vertices = style.fill.and_then(|_| {
        Some(get_vertices(&font_size, &layout.layouted_glyphs, vectorized_font, &font.0, char_offsets.clone(), get_fill_vertices))
    });

    let stroke_vertices = style.stroke.and_then(|_| {
        Some(get_vertices(&font_size, &layout.layouted_glyphs, vectorized_font, &font.0, char_offsets, get_stroke_vertices))
    });

    layers.push(SvgLayerResource::Direct {
        style,
        fill: fill_vertices,
        stroke: stroke_vertices,
    });

    layers.push(circle_layer);

    // layers.append(&mut test_bezier_points());
    layers
}

/// Roughly estimate the length of a bezier curve arc using 10 samples
fn estimate_arc_length(curve: &[BezierControlPoint;4]) -> (Vec<BezierControlPoint>, f32) {

    let mut origin = curve[0];
    let mut total_distance = 0.0;
    let mut circles = vec![curve[0]];

    for i in 1..10 {
        let new_point = cubic_interpolate_bezier(curve, i as f32 / 10.0);
        total_distance += origin.distance(&new_point);
        circles.push(new_point);
        origin = new_point;
    }

    total_distance += origin.distance(&curve[3]);
    circles.push(curve[3]);
    (circles, total_distance)
}

fn test_bezier_points_offsets(glyphs: &[GlyphInstance], mut start_offset: f32) -> (SvgLayerResource, Vec<(f32, f32)>) {
    let test_curve = [
        BezierControlPoint { x: 0.0, y: 0.0 },
        BezierControlPoint { x: 40.0, y: 120.0 },
        BezierControlPoint { x: 80.0, y: 120.0 },
        BezierControlPoint { x: 120.0, y: 0.0 },
    ];

    let (circles, curve_length) = estimate_arc_length(&test_curve);

    let mut offsets = vec![];

    for glyph in glyphs {
        let char_bezier_pt = cubic_interpolate_bezier(&test_curve, start_offset);
        offsets.push((char_bezier_pt.x, char_bezier_pt.y));

        let x_advance_px = glyph.point.x * 2.0;
        let x_advance_percent = if x_advance_px > 0.00001 {
            x_advance_px / curve_length
        } else {
            0.0
        };
        start_offset += x_advance_percent;
    }

    let circles = circles.into_iter().map(|c| SvgCircle { center_x: c.x, center_y: c.y, radius: 1.0 }).collect::<Vec<_>>();

    (quick_circles(&circles, ColorU { r: 0, b: 0, g: 0, a: 255 }), offsets)
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
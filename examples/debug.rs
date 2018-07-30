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

    let (circle_layer, char_offsets) = test_bezier_points_offsets(&layout.layouted_glyphs, 0.0);

    let fill_vertices = style.fill.and_then(|_| {
        Some(vector_text_to_vertices(&font_size, &layout.layouted_glyphs, vectorized_font, &font.0, &char_offsets, get_fill_vertices))
    });

    let stroke_vertices = style.stroke.and_then(|_| {
        Some(vector_text_to_vertices(&font_size, &layout.layouted_glyphs, vectorized_font, &font.0, &char_offsets, get_stroke_vertices))
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

// Calculates the layout for one word block
fn vector_text_to_vertices(
    font_size: &FontSize,
    glyph_ids: &[GlyphInstance],
    vectorized_font: &VectorizedFont,
    original_font: &Font,
    char_offsets: &[(f32, f32)],
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

const BEZIER_SAMPLE_RATE: usize = 10;

type ArcLength = f32;

/// The sampled bezier curve stores information about 10 points that lie along the
/// bezier curve.
///
/// For example: To place a text on a curve, we only have the layout
/// of the text in pixels. In order to calculate the position and rotation of
/// the individual characters (to place the text on the curve) we need to know
/// what the percentage offset (from 0.0 to 1.0) of the current character is
/// (which we can then give to the bezier formula, which will calculate the position
/// and rotation of the character)
///
/// Calculating the position accurately is an unsolvable problem, but we can
/// "estimate" where the character would be, by solving 10 bezier points
/// for the offsets 0.0, 0.1, 0.2, and so on and storing the arc length from the
/// start for each position, ex. the position 0.1 is at 20 pixels, the position
/// 0.5 at 500 pixels, etc. Since a bezier curve is, well, curved, this offset is
/// not constantly increasing, it can vary from point to point.
///
/// Lastly, to get the percentage of the string on the curve, we simply interpolate
/// linearly between the two nearest values. I.e. if we need to place a character
/// at 300 pixels from the start, we interpolate linearly between 0.1
/// (which we know is at 20 pixels) and 0.5 (which we know is at 500 pixels).
///
/// This process is called "arc length parametrization". More info:
#[derive(Debug, Copy, Clone)]
struct SampledBezierCurve {
    /// Total length of the arc of the curve (from 0.0 to 1.0)
    arc_length: f32,
    /// Stores the x and y position of the sampled bezier points
    sampled_bezier_points: [BezierControlPoint;BEZIER_SAMPLE_RATE],
    /// Each index is the bezier value * 0.1, i.e. index 1 = 0.1,
    /// index 2 = 0.2 and so on.
    ///
    /// Stores the length of the BezierControlPoint at i from the
    /// start of the curve
    arc_length_parametrization: [ArcLength; BEZIER_SAMPLE_RATE],
}

impl SampledBezierCurve {

    /// Roughly estimate the length of a bezier curve arc using 10 samples
    pub fn from_curve(curve: &[BezierControlPoint;4]) -> Self {

        let mut sampled_bezier_points = [curve[0]; BEZIER_SAMPLE_RATE];
        let mut arc_length_parametrization = [0.0; BEZIER_SAMPLE_RATE];

        for i in 1..BEZIER_SAMPLE_RATE {
            sampled_bezier_points[i] = cubic_interpolate_bezier(curve, i as f32 / BEZIER_SAMPLE_RATE as f32);
        }

        sampled_bezier_points[BEZIER_SAMPLE_RATE - 1] = curve[3];

        // arc_length represents the sum of all sampled arcs up until the
        // current sampled iteration point
        let mut arc_length = 0.0;

        for (i, w) in sampled_bezier_points.windows(2).enumerate() {
            let dist_current = w[0].distance(&w[1]);
            arc_length_parametrization[i] = arc_length;
            arc_length += dist_current;
        }

        arc_length_parametrization[BEZIER_SAMPLE_RATE - 1] = arc_length;

        SampledBezierCurve {
            arc_length,
            sampled_bezier_points,
            arc_length_parametrization,
        }
    }

    /// Offset should be the point you seek from the start, i.e. 500 pixels for example.
    ///
    /// NOTE: Currently this function assumes a value that will be on the curve,
    /// not past the 1.0 mark.
    pub fn get_bezier_percentage_from_offset(&self, offset: f32) -> f32 {

        let mut lower_bound = 0;
        let mut upper_bound = BEZIER_SAMPLE_RATE - 1;

        // If the offset is too high (past 1.0) we simply interpolate between the 0.9
        // and 1.0 point. Because of this we don't want to include the last point when iterating
        for (i, param) in self.arc_length_parametrization.iter().take(BEZIER_SAMPLE_RATE - 1).enumerate() {
            if *param < offset {
                lower_bound = i;
            } else if *param > offset {
                upper_bound = i;
                break;
            }
        }

        // Now we know that the offset lies between the lower and upper bound, we need to
        // find out how much we should (linearly) interpolate
        let lower_bound_value = self.arc_length_parametrization[lower_bound];
        let upper_bound_value = self.arc_length_parametrization[upper_bound];
        let interpolate_percent = (offset - lower_bound_value) / (upper_bound_value - lower_bound_value);

        let lower_bound_percent = lower_bound as f32 / BEZIER_SAMPLE_RATE as f32;
        let upper_bound_percent = upper_bound as f32 / BEZIER_SAMPLE_RATE as f32;

        lower_bound_percent + ((upper_bound_percent - lower_bound_percent) * interpolate_percent)
    }
}

/// `start_offset` is in pixels - the offset of the text froma the start of the curve
fn test_bezier_points_offsets(glyphs: &[GlyphInstance], start_offset: f32) -> (SvgLayerResource, Vec<(f32, f32)>) {
    let test_curve = [
        BezierControlPoint { x: 0.0, y: 0.0 },
        BezierControlPoint { x: 40.0, y: 120.0 },
        BezierControlPoint { x: 80.0, y: 120.0 },
        BezierControlPoint { x: 120.0, y: 0.0 },
    ];

    let sampled_bezier_curve = SampledBezierCurve::from_curve(&test_curve);

    let mut offsets = vec![];
    let mut current_offset = start_offset;

    for glyph in glyphs {
        let char_bezier_percentage = sampled_bezier_curve.get_bezier_percentage_from_offset(current_offset);
        println!("current offset is: {}", current_offset);
        let char_bezier_pt = cubic_interpolate_bezier(&test_curve, char_bezier_percentage);
        offsets.push((char_bezier_pt.x, char_bezier_pt.y));
        current_offset += glyph.point.x * 2.0;
    }

    let circles = sampled_bezier_curve.sampled_bezier_points
        .into_iter()
        .map(|c| SvgCircle { center_x: c.x, center_y: c.y, radius: 1.0 })
        .collect::<Vec<_>>();

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
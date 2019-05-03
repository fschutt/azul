use std::{
    fmt,
    rc::Rc,
    sync::{atomic::{Ordering, AtomicUsize}},
    cell::{RefCell, RefMut},
    collections::hash_map::Entry::*,
};
#[cfg(feature = "svg_parsing")]
use std::io::{Error as IoError};
use lyon::{
    tessellation::{
        FillOptions, BuffersBuilder, FillVertex, FillTessellator,
        LineCap, LineJoin, StrokeTessellator, StrokeOptions, StrokeVertex,
        basic_shapes::{
            fill_circle, stroke_circle, fill_rounded_rectangle,
            stroke_rounded_rectangle, BorderRadii
        },
    },
    path::{
        default::{Builder, Path},
        builder::{PathBuilder, FlatPathBuilder},
    },
    geom::euclid::{TypedRect, TypedPoint2D, TypedSize2D, TypedVector2D, UnknownUnit},
};
#[cfg(feature = "svg_parsing")]
use usvg::{Error as SvgError};
use azul_css::{ColorU, ColorF, StyleTextAlignmentHorz};
use gleam::gl::{self, Gl};
use {
    FastHashMap,
    prelude::GlyphInstance,
    gl::{
        VertexBuffer, VertexLayout, VertexLayoutDescription, VertexAttributeType, FrameBuffer,
        VertexAttribute, IndexBuffer, Uniform, Texture, GlShader, GlApiVersion, IndexBufferFormat
    },
    window::FakeWindow,
    app_resources::{AppResources, FontId},
    text_layout::{Words, ScaledWords, WordPositions, LineBreaks, LayoutedGlyphs, TextLayoutOptions},
};

pub use lyon::tessellation::VertexBuffers;
pub use lyon::path::PathEvent;
pub use lyon::geom::math::Point;

static SVG_LAYER_ID: AtomicUsize = AtomicUsize::new(0);
static SVG_TRANSFORM_ID: AtomicUsize = AtomicUsize::new(0);
static SVG_VIEW_BOX_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SvgTransformId(usize);

pub fn new_svg_transform_id() -> SvgTransformId {
    SvgTransformId(SVG_TRANSFORM_ID.fetch_add(1, Ordering::SeqCst))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SvgViewBoxId(usize);

pub fn new_view_box_id() -> SvgViewBoxId {
    SvgViewBoxId(SVG_VIEW_BOX_ID.fetch_add(1, Ordering::SeqCst))
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct SvgLayerId(usize);

pub fn new_svg_layer_id() -> SvgLayerId {
    SvgLayerId(SVG_LAYER_ID.fetch_add(1, Ordering::SeqCst))
}

const SHADER_VERSION_GL: &str = "#version 150";
const SHADER_VERSION_GLES: &str = "#version 300 es";
const DEFAULT_GLYPH_TOLERANCE: f32 = 0.01;

const SVG_VERTEX_SHADER: &str = "

    precision highp float;

    #define attribute in
    #define varying out

    in vec2 vAttrXY;
    in vec2 vAttrNormal;

    uniform vec2 vBboxSize;
    uniform vec2 vGlobalOffset;
    uniform float vZIndex;
    uniform float vZoom;
    uniform vec2 vRotationCenter;
    uniform float vRotationSin;
    uniform float vRotationCos;
    uniform vec2 vScaleFactor;
    uniform vec2 vTranslatePx;

    void main() {

        // Rotation first, then scale, then translation -- all in pixel space
        vec2 vRotationCenterXY = vAttrXY - vRotationCenter;
        vec2 vAttrXYRotated = vec2(
            (vRotationCenterXY.x * rotation_cos) - (vRotationCenterXY.y * vRotationSin),
            (vAttrXY.x * vRotationSin) + (vAttrXY.y * rotation_cos)
        );

        vec2 vAttrXYScaled = vAttrXYRotated * scale_factor;
        vec2 vAttrXYTranslated = vAttrXYScaled + vTranslatePx + vRotationCenter;

        vec2 vPositionCentered = vAttrXYTranslated / vBboxSize;
        vec2 vPositionZoomed = vPositionCentered * vec2(vZoom);

        gl_Position = vec4(vPositionZoomed + (vGlobalOffset / vBboxSize) - vec2(1.0), vZIndex, 1.0);
    }";

const SVG_FRAGMENT_SHADER: &str = "

    precision highp float;

    #define attribute in
    #define varying out

    uniform vec4 fFillColor;
    out vec4 fOutColor;

    // The shader output is in SRGB color space,
    // and the shader assumes that the input colors are in SRGB, too.

    void main() {
        fOutColor = fFillColor;
    }
";

fn prefix_gl_version(shader: &str, gl: GlApiVersion) -> String {
    match gl {
        GlApiVersion::Gl { .. } => format!("{}\n{}", SHADER_VERSION_GL, shader),
        GlApiVersion::GlEs { .. } => format!("{}\n{}", SHADER_VERSION_GLES, shader),
    }
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct SvgShader {
    pub program: GlShader,
}

impl SvgShader {

    pub fn new(gl_context: Rc<Gl>) -> Self {

        let current_gl_api = GlApiVersion::get(&*gl_context);
        let vertex_source_prefixed = prefix_gl_version(SVG_VERTEX_SHADER, current_gl_api);
        let fragment_source_prefixed = prefix_gl_version(SVG_FRAGMENT_SHADER, current_gl_api);

        Self {
            program: GlShader::new(gl_context, &vertex_source_prefixed, &fragment_source_prefixed).unwrap(),
        }
    }
}

pub struct SvgCache {
    // Stores the vertices and indices necessary for drawing. Must be synchronized with the `layers`
    gpu_ready_to_upload_cache: FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    stroke_gpu_ready_to_upload_cache: FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    vertex_index_buffer_cache: RefCell<FastHashMap<SvgLayerId, Rc<(VertexBuffer<SvgVert>, IndexBuffer)>>>,
    stroke_vertex_index_buffer_cache: RefCell<FastHashMap<SvgLayerId, Rc<(VertexBuffer<SvgVert>, IndexBuffer)>>>,
    shader: RefCell<Option<SvgShader>>,
}

impl Default for SvgCache {
    fn default() -> Self {
        Self {
            gpu_ready_to_upload_cache: FastHashMap::default(),
            stroke_gpu_ready_to_upload_cache: FastHashMap::default(),
            vertex_index_buffer_cache: RefCell::new(FastHashMap::default()),
            stroke_vertex_index_buffer_cache: RefCell::new(FastHashMap::default()),
            shader: RefCell::new(None),
        }
    }
}

fn fill_vertex_buffer_cache<'a>(
    id: &SvgLayerId,
    mut rmut: RefMut<'a, FastHashMap<SvgLayerId, Rc<(VertexBuffer<SvgVert>, IndexBuffer)>>>,
    rnotmut: &FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    gl_context: Rc<Gl>,
) {
    use std::collections::hash_map::Entry::*;

    match rmut.entry(*id) {
        Occupied(_) => { },
        Vacant(v) => {
            let (vbuf, ibuf) = match rnotmut.get(id).as_ref() {
                Some(s) => s,
                None => return,
            };
            let vertex_buffer = VertexBuffer::new(gl_context.clone(), vbuf);
            let index_buffer = IndexBuffer::new(gl_context, ibuf, IndexBufferFormat::TriangleStrip);
            v.insert(Rc::new((vertex_buffer, index_buffer)));
        }
    }
}

impl SvgCache {

    /// Creates an empty SVG cache
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds and compiles the SVG shader if the shader isn't already present
    fn init_shader<'a>(&'a self, gl_context: Rc<Gl>) {
        self.shader.borrow_mut().get_or_insert_with(|| SvgShader::new(gl_context));
    }

    fn get_stroke_vertices_and_indices(&self, gl_context: Rc<Gl>, id: &SvgLayerId)
    -> Option<Rc<(VertexBuffer<SvgVert>, IndexBuffer)>>
    {
        {
            let rmut = self.stroke_vertex_index_buffer_cache.borrow_mut();
            let rnotmut = &self.stroke_gpu_ready_to_upload_cache;
            fill_vertex_buffer_cache(id, rmut, rnotmut, gl_context);
        }

        self.stroke_vertex_index_buffer_cache.borrow().get(id).map(|x| x.clone())
    }

    /// Note: panics if the ID isn't found.
    ///
    /// Since we are required to keep the `self.layers` and the `self.gpu_buffer_cache`
    /// in sync, a panic should never happen
    fn get_vertices_and_indices(&self, gl_context: Rc<Gl>, id: &SvgLayerId)
    -> Option<Rc<(VertexBuffer<SvgVert>, IndexBuffer)>>
    {
        // We need the SvgCache to call this function immutably, otherwise we can't
        // use it from the Layout::layout() function
        {
            let rmut = self.vertex_index_buffer_cache.borrow_mut();
            let rnotmut = &self.gpu_ready_to_upload_cache;

            fill_vertex_buffer_cache(id, rmut, rnotmut, gl_context);
        }

        self.vertex_index_buffer_cache.borrow().get(id).map(|x| x.clone())
    }

    pub fn add_layer(&mut self, layer: SvgLayerResourceDirect) -> (SvgLayerId, SvgStyle) {

        let SvgLayerResourceDirect { style, stroke, fill } = layer;

        let new_svg_id = new_svg_layer_id();

        if let Some(fill) = fill {
            self.gpu_ready_to_upload_cache.insert(new_svg_id, (fill.vertices, fill.indices));
        }

        if let Some(stroke) = stroke {
            self.stroke_gpu_ready_to_upload_cache.insert(new_svg_id, (stroke.vertices, stroke.indices));
        }

        (new_svg_id, style)
    }

    pub fn delete_layer(&mut self, svg_id: SvgLayerId) {
        self.gpu_ready_to_upload_cache.remove(&svg_id);
        self.stroke_gpu_ready_to_upload_cache.remove(&svg_id);
        let rmut = self.vertex_index_buffer_cache.get_mut();
        let stroke_rmut = self.stroke_vertex_index_buffer_cache.get_mut();
        rmut.remove(&svg_id);
        stroke_rmut.remove(&svg_id);
    }

    pub fn clear_all_layers(&mut self) {
        self.gpu_ready_to_upload_cache.clear();
        self.stroke_gpu_ready_to_upload_cache.clear();

        let rmut = self.vertex_index_buffer_cache.get_mut();
        rmut.clear();

        let stroke_rmut = self.stroke_vertex_index_buffer_cache.get_mut();
        stroke_rmut.clear();
    }

    /// Creates a new, identical, copied instance of the given layer - duplicates
    /// the object in GPU memory, but this way re-tesselation can be avoided.
    pub fn clone_layer(&mut self, svg_id: SvgLayerId) -> SvgLayerId {

        let new_svg_id = new_svg_layer_id();
        let old_svg_id = svg_id;

        if let Some(vertices_indices) = self.gpu_ready_to_upload_cache.get(&old_svg_id).cloned() {
            self.gpu_ready_to_upload_cache.insert(new_svg_id, vertices_indices);
        }

        if let Some(stroke_vertices_indices) = self.stroke_gpu_ready_to_upload_cache.get(&old_svg_id).cloned() {
            self.stroke_gpu_ready_to_upload_cache.insert(new_svg_id, stroke_vertices_indices);
        }

        // Needs to be on a separate line, otherwise it would get a BorrowMut error!
        let vertices_indices_clone = self.vertex_index_buffer_cache.borrow().get(&old_svg_id).cloned();
        if let Some(vertices_indices) = vertices_indices_clone {
            self.vertex_index_buffer_cache.borrow_mut().insert(new_svg_id, vertices_indices);
        }

        let stroke_vertices_indices_clone = self.stroke_vertex_index_buffer_cache.borrow().get(&old_svg_id).cloned();
        if let Some(stroke_vertices_indices) = stroke_vertices_indices_clone {
            self.stroke_vertex_index_buffer_cache.borrow_mut().insert(new_svg_id, stroke_vertices_indices);
        }

        new_svg_id
    }

    /// Parses an input source, parses the SVG, adds the shapes as layers into
    /// the registry, returns the IDs of the added shapes, in the order that they appeared in the Svg
    #[cfg(feature = "svg_parsing")]
    pub fn add_svg<S: AsRef<str>>(&mut self, input: S) -> Result<Vec<(SvgLayerId, SvgStyle)>, SvgParseError> {
        let layers = self::svg_to_lyon::parse_from(input)?;
        Ok(layers
            .into_iter()
            .map(|(layer, style)| SvgLayerResourceDirect::tesselate_from_layer(&layer, style))
            .map(|tesselated_layer| self.add_layer(tesselated_layer))
            .collect())
    }
}

impl fmt::Debug for SvgCache {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for layer_id in self.gpu_ready_to_upload_cache.keys() {
            write!(f, "{:?}", layer_id)?;
        }
        Ok(())
    }
}

const GL_RESTART_INDEX: u32 = ::std::u32::MAX;

/// Returns the (fill, stroke) vertices of a layer
pub fn tesselate_polygon_data(layer_data: &[SvgLayerType], style: SvgStyle)
-> SvgLayerResourceDirect // (Option<(Vec<SvgVert>, Vec<u32>)>, Option<(Vec<SvgVert>, Vec<u32>)>)
{
    let tolerance = 0.01;
    let fill = style.fill.is_some();
    let stroke_options = style.stroke.map(|s| s.1);

    let mut last_index = 0;
    let mut fill_vertex_buf = Vec::<SvgVert>::new();
    let mut fill_index_buf = Vec::<u32>::new();

    let mut last_stroke_index = 0;
    let mut stroke_vertex_buf = Vec::<SvgVert>::new();
    let mut stroke_index_buf = Vec::<u32>::new();

    for layer in layer_data {

        let mut path = None;

        if fill {
            let VertexBuffers { vertices, indices } = layer.tesselate_fill(tolerance, &mut path);
            let fill_vertices_len = vertices.len();
            fill_vertex_buf.extend(vertices.into_iter());
            fill_index_buf.extend(indices.into_iter().map(|i| i as u32 + last_index as u32));
            fill_index_buf.push(GL_RESTART_INDEX);
            last_index += fill_vertices_len;
        }

        if let Some(stroke_options) = &stroke_options {
            let VertexBuffers { vertices, indices } = layer.tesselate_stroke(tolerance, &mut path, *stroke_options);
            let stroke_vertices_len = vertices.len();
            stroke_vertex_buf.extend(vertices.into_iter());
            stroke_index_buf.extend(indices.into_iter().map(|i| i as u32 + last_stroke_index as u32));
            stroke_index_buf.push(GL_RESTART_INDEX);
            last_stroke_index += stroke_vertices_len;
        }
    }

    let fill_verts = if fill {
        Some(VerticesIndicesBuffer {
            vertices: fill_vertex_buf,
            indices: fill_index_buf,
        })
    } else { None };

    let stroke_verts = if stroke_options.is_some() {
        Some(VerticesIndicesBuffer {
            vertices: stroke_vertex_buf,
            indices: stroke_index_buf,
        })
    } else { None };

    SvgLayerResourceDirect {
        style,
        fill: fill_verts,
        stroke: stroke_verts,
    }
}

/// Quick helper function to generate the vertices for a black circle at runtime
pub fn quick_circle(circle: SvgCircle, fill_color: ColorU) -> SvgLayerResourceDirect {
    let style = SvgStyle::filled(fill_color);
    tesselate_polygon_data(&[SvgLayerType::Circle(circle)], style)
}

/// Quick helper function to generate the layer for **multiple** circles (in one draw call)
pub fn quick_circles(circles: &[SvgCircle], fill_color: ColorU) -> SvgLayerResourceDirect {
    let circles = circles.iter().map(|c| SvgLayerType::Circle(*c)).collect::<Vec<_>>();
    let style = SvgStyle::filled(fill_color);
    tesselate_polygon_data(&circles, style)
}

/// Helper function to easily draw some lines at runtime
///
/// ## Inputs
///
/// - `lines`: Each item in `lines` is a line (represented by a `Vec<(x, y)>`).
///    Lines that are shorter than 2 points are ignored / not rendered.
/// - `stroke_color`: The color of the line
/// - `stroke_options`: If the line should be round, square, etc.
pub fn quick_lines(lines: &[Vec<(f32, f32)>], stroke_color: ColorU, stroke_options: Option<SvgStrokeOptions>)
-> SvgLayerResourceDirect
{
    let stroke_options = stroke_options.unwrap_or_default();
    let style = SvgStyle::stroked(stroke_color, stroke_options);

    let polygons = lines.iter()
        .filter(|line| line.len() >= 2)
        .map(|line| {

            let first_point = &line[0];
            let mut poly_events = vec![PathEvent::MoveTo(TypedPoint2D::new(first_point.0, first_point.1))];

            for (x, y) in line.iter().skip(1) {
                poly_events.push(PathEvent::LineTo(TypedPoint2D::new(*x, *y)));
            }

            SvgLayerType::Polygon(poly_events)
        }).collect::<Vec<_>>();

    tesselate_polygon_data(&polygons, style)
}

pub fn quick_rects(rects: &[SvgRect], stroke_color: Option<ColorU>, fill_color: Option<ColorU>, stroke_options: Option<SvgStrokeOptions>)
-> SvgLayerResourceDirect
{
    let style = SvgStyle {
        stroke: stroke_color.map(|col| (col, stroke_options.unwrap_or_default())),
        fill: fill_color,
        .. Default::default()
    };
    let rects = rects.iter().map(|r| SvgLayerType::Rect(*r)).collect::<Vec<_>>();
    tesselate_polygon_data(&rects, style)
}

const BEZIER_SAMPLE_RATE: usize = 20;

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
/// This process is called "arc length parametrization". For more info + diagrams, see:
/// http://www.planetclegg.com/projects/WarpingTextToSplines.html
#[derive(Debug, Copy, Clone)]
pub struct SampledBezierCurve {
    /// Copy of the original curve which the SampledBezierCurve was created from
    original_curve: [BezierControlPoint;4],
    /// Total length of the arc of the curve (from 0.0 to 1.0)
    arc_length: f32,
    /// Stores the x and y position of the sampled bezier points
    sampled_bezier_points: [BezierControlPoint;BEZIER_SAMPLE_RATE + 1],
    /// Each index is the bezier value * 0.1, i.e. index 1 = 0.1,
    /// index 2 = 0.2 and so on.
    ///
    /// Stores the length of the BezierControlPoint at i from the
    /// start of the curve
    arc_length_parametrization: [ArcLength; BEZIER_SAMPLE_RATE + 1],
}

/// NOTE: The inner value is in **radians**, not degrees!
#[derive(Debug, Copy, Clone)]
pub struct BezierCharacterRotation(pub f32);

impl SampledBezierCurve {

    /// Roughly estimate the length of a bezier curve arc using 10 samples
    pub fn from_curve(curve: &[BezierControlPoint;4]) -> Self {

        let mut sampled_bezier_points = [curve[0]; BEZIER_SAMPLE_RATE + 1];
        let mut arc_length_parametrization = [0.0; BEZIER_SAMPLE_RATE + 1];

        for i in 1..(BEZIER_SAMPLE_RATE + 1) {
            sampled_bezier_points[i] = cubic_interpolate_bezier(
                curve, i as f32 / BEZIER_SAMPLE_RATE as f32
            );
        }

        sampled_bezier_points[BEZIER_SAMPLE_RATE] = curve[3];

        // arc_length represents the sum of all sampled arcs up until the
        // current sampled iteration point
        let mut arc_length = 0.0;

        for (i, w) in sampled_bezier_points.windows(2).enumerate() {
            let dist_current = w[0].distance(&w[1]);
            arc_length_parametrization[i] = arc_length;
            arc_length += dist_current;
        }

        arc_length_parametrization[BEZIER_SAMPLE_RATE] = arc_length;

        SampledBezierCurve {
            original_curve: *curve,
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
        let mut upper_bound = BEZIER_SAMPLE_RATE;

        // If the offset is too high (past 1.0) we simply interpolate between the 0.9
        // and 1.0 point. Because of this we don't want to include the last point when iterating
        for (i, param) in self.arc_length_parametrization.iter().take(BEZIER_SAMPLE_RATE).enumerate() {
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
        let lower_upper_diff = upper_bound_value - lower_bound_value;
        let interpolate_percent = (offset - lower_bound_value) / lower_upper_diff;

        let lower_bound_percent = lower_bound as f32 / BEZIER_SAMPLE_RATE as f32;
        let upper_bound_percent = upper_bound as f32 / BEZIER_SAMPLE_RATE as f32;

        let lower_upper_diff_percent = upper_bound_percent - lower_bound_percent;
        lower_bound_percent + (lower_upper_diff_percent * interpolate_percent)
    }

    /// Place some glyphs on a curve and calculate the respective offsets and rotations
    /// for the glyphs
    ///
    /// ## Inputs
    ///
    /// - `glyphs`: The glyph positions of the text you want to place on the curve
    /// - `start_offset` The offset of the first character from the start of the curve:
    ///    **Note**: `start_offset` is measured in pixels, not percent!
    ///
    /// ## Returns
    ///
    /// - `Vec<(f32, f32)>`: the x and y offsets of the glyph characters
    /// - `Vec<f32>`: The rotations in degrees of the glyph characters
    pub fn get_text_offsets_and_rotations(&self, glyphs: &[GlyphInstance], start_offset: f32)
    -> (Vec<(f32, f32)>, Vec<BezierCharacterRotation>)
    {
        let mut glyph_offsets = Vec::new();
        let mut glyph_rotations = Vec::new();

        // NOTE: g.point.x is the offset from the start, not the advance!
        let mut current_offset = start_offset + glyphs.get(0).and_then(|g| Some(g.point.x)).unwrap_or(0.0);
        let mut last_offset = start_offset;

        for glyph_idx in 0..glyphs.len() {
            let char_bezier_percentage = self.get_bezier_percentage_from_offset(current_offset);
            let char_bezier_pt = cubic_interpolate_bezier(&self.original_curve, char_bezier_percentage);
            glyph_offsets.push((char_bezier_pt.x / SVG_FAKE_FONT_SIZE, char_bezier_pt.y / SVG_FAKE_FONT_SIZE));

            let char_rotation_percentage = self.get_bezier_percentage_from_offset(last_offset);
            let rotation = cubic_bezier_normal(&self.original_curve, char_rotation_percentage).to_rotation();
            glyph_rotations.push(rotation);

            last_offset = current_offset;
            current_offset = start_offset + glyphs.get(glyph_idx + 1).map(|g| g.point.x).unwrap_or(0.0);
        }

        (glyph_offsets, glyph_rotations)
    }

    /// Returns the bounding box of the 4 points making up the curve.
    ///
    /// Since a bezier curve is always contained within the 4 control points,
    /// the returned Bbox can be used for hit-testing.
    pub fn get_bbox(&self) -> (SvgBbox, [(usize, usize);2]) {

        let mut lowest_x = self.original_curve[0].x;
        let mut highest_x = self.original_curve[0].x;
        let mut lowest_y = self.original_curve[0].y;
        let mut highest_y = self.original_curve[0].y;

        let mut lowest_x_idx = 0;
        let mut highest_x_idx = 0;
        let mut lowest_y_idx = 0;
        let mut highest_y_idx = 0;

        for (idx, BezierControlPoint { x, y }) in self.original_curve.iter().enumerate().skip(1) {
            if *x < lowest_x {
                lowest_x = *x;
                lowest_x_idx = idx;
            }
            if *x > highest_x {
                highest_x = *x;
                highest_x_idx = idx;
            }
            if *y < lowest_y {
                lowest_y = *y;
                lowest_y_idx = idx;
            }
            if *y > highest_y {
                highest_y = *y;
                highest_y_idx = idx;
            }
        }

        (
            SvgBbox(TypedRect::new(TypedPoint2D::new(lowest_x, lowest_y), TypedSize2D::new(highest_x - lowest_x, highest_y - lowest_y))),
            [(lowest_x_idx, lowest_y_idx), (highest_x_idx, highest_y_idx)]
        )
    }

    /// Returns the geometry necessary for drawing the points from `self.sampled_bezier_points`.
    /// Usually only good for debugging
    pub fn draw_circles(&self, color: ColorU) -> SvgLayerResourceDirect {
        quick_circles(
            &self.sampled_bezier_points
            .iter()
            .map(|c| SvgCircle { center_x: c.x, center_y: c.y, radius: 1.0 })
            .collect::<Vec<_>>(),
            color)
    }

    /// Returns the geometry necessary to draw the control handles of this curve
    pub fn draw_control_handles(&self, color: ColorU) -> SvgLayerResourceDirect {
        quick_circles(
            &self.original_curve
            .iter()
            .map(|c| SvgCircle { center_x: c.x, center_y: c.y, radius: 3.0 })
            .collect::<Vec<_>>(),
            color)
    }

    /// Returns the geometry necessary to draw the bezier curve (the actual line)
    pub fn draw_lines(&self, stroke_color: ColorU) -> SvgLayerResourceDirect {
        let line = [self.sampled_bezier_points.iter().map(|b| (b.x, b.y)).collect()];
        quick_lines(&line, stroke_color, None)
    }

    /// Returns the sampled points from this bezier curve
    pub fn get_sampled_points<'a>(&'a self) -> &'a [BezierControlPoint;BEZIER_SAMPLE_RATE + 1] {
        &self.sampled_bezier_points
    }
}

/// Joins multiple SvgVert buffers to one and calculates the indices
///
/// TODO: Wrap this in a nicer API
pub fn join_vertex_buffers(input: &[VertexBuffers<SvgVert, u32>]) -> VerticesIndicesBuffer {

    let mut last_index = 0;
    let mut vertex_buf = Vec::<SvgVert>::new();
    let mut index_buf = Vec::<u32>::new();

    for VertexBuffers { vertices, indices } in input {
        let vertices_len = vertices.len();
        vertex_buf.extend(vertices.into_iter());
        index_buf.extend(indices.into_iter().map(|i| *i as u32 + last_index as u32));
        index_buf.push(GL_RESTART_INDEX);
        last_index += vertices_len;
    }

    VerticesIndicesBuffer { vertices: vertex_buf, indices: index_buf }
}

pub fn transform_vertex_buffer(input: &mut [SvgVert], x: f32, y: f32) {
    for vert in input {
        vert.xy.0 += x;
        vert.xy.1 += y;
    }
}

/// sin and cos are the sinus and cosinus of the rotation
pub fn rotate_vertex_buffer(input: &mut [SvgVert], sin: f32, cos: f32) {
    for vert in input {
        let (x, y) = vert.xy;
        let new_x = (x * cos) - (y * sin);
        let new_y = (x * sin) + (y * cos);
        vert.xy = (new_x, new_y);
    }
}

#[cfg(feature = "svg_parsing")]
#[derive(Debug)]
pub enum SvgParseError {
    /// Syntax error in the Svg
    FailedToParseSvg(SvgError),
    /// Io error reading the Svg
    IoError(IoError),
}

#[cfg(feature = "svg_parsing")]
impl From<SvgError> for SvgParseError {
    fn from(e: SvgError) -> Self {
        SvgParseError::FailedToParseSvg(e)
    }
}

#[cfg(feature = "svg_parsing")]
impl From<IoError> for SvgParseError {
    fn from(e: IoError) -> Self {
        SvgParseError::IoError(e)
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct SvgStyle {
    /// Stroke color
    pub stroke: Option<(ColorU, SvgStrokeOptions)>,
    /// Fill color
    pub fill: Option<ColorU>,
    /// Stores rotation, translation
    pub transform: SvgTransform,
    // TODO: stroke-dasharray
}

impl SvgStyle {

    /// If the style already has a translation, adds the new translation,
    /// otherwise initializes the value to the new translation
    pub fn translate(&mut self, x_px: f32, y_px: f32) {
        let (cur_x, cur_y) = self.transform.translation.map(|t| (t.x, t.y)).unwrap_or((0.0, 0.0));
        self.transform.translation = Some(SvgTranslation { x: cur_x + x_px, y: cur_y + y_px });
    }

    /// If the style already has a rotation, adds the rotation, otherwise sets the rotation
    ///
    /// Input is in degrees.
    pub fn rotate(&mut self, degrees: f32) {
        let current_rotation = self.transform.rotation.map(|r| r.1.to_degrees()).unwrap_or(0.0);
        let current_rotation_point = self.transform.rotation.map(|r| r.0).unwrap_or_default();
        self.transform.rotation = Some((current_rotation_point, SvgRotation::degrees(current_rotation + degrees)));
    }

    /// If the style already has a scale, adds the rotation, otherwise sets the scale.
    pub fn scale(&mut self, scale_factor_x: f32, scale_factor_y: f32) {
        let (new_scale_x, new_scale_y) = match self.transform.scale {
            Some(s) => (s.x * scale_factor_x, s.y * scale_factor_y),
            None => (scale_factor_x, scale_factor_y),
        };
        self.transform.scale = Some(SvgScaleFactor { x: new_scale_x, y: new_scale_y });
    }

    /// If the style already has a rotation, adds the rotation, otherwise sets the rotation point to the new value
    pub fn move_rotation_point(&mut self, rotation_point_x: f32, rotation_point_y: f32) {
        let current_rotation_point = self.transform.rotation.map(|r| r.0).unwrap_or_default();
        let current_rotation = self.transform.rotation.unwrap_or_default().1;
        let new_rotation_point = SvgRotationPoint { x: current_rotation_point.x + rotation_point_x, y: current_rotation_point.y + rotation_point_y };
        self.transform.rotation = Some((new_rotation_point, current_rotation));
    }

    /// Replaces the translation value with the new x and y values - or initializes it to the new value
    pub fn set_translation(&mut self, x_px: f32, y_px: f32) {
        self.transform.translation = Some(SvgTranslation { x: x_px, y: y_px });
    }

    /// Replaces the rotation value with the new rotation values - or initializes it, if set to None
    pub fn set_rotation(&mut self, degrees: f32) {
        let current_rotation_point = self.transform.rotation.map(|r| r.0).unwrap_or_default();
        let new_rotation = SvgRotation { degrees };
        self.transform.rotation = Some((current_rotation_point, new_rotation));
    }

    /// Replaces the scale value with the new x and y values - or initializes it to the new value
    pub fn set_scale(&mut self, scale_factor_x: f32, scale_factor_y: f32) {
        self.transform.scale = Some(SvgScaleFactor { x: scale_factor_x, y: scale_factor_y });
    }

    /// Replaces the rotation value with the new rotation values - or initializes it, if set to None
    pub fn set_rotation_point(&mut self, rotation_point_x: f32, rotation_point_y: f32) {
        let current_rotation = self.transform.rotation.unwrap_or_default().1;
        let new_rotation_point = SvgRotationPoint { x: rotation_point_x, y: rotation_point_y };
        self.transform.rotation = Some((new_rotation_point, current_rotation));
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct SvgTransform {
    /// Rotation of this SVG layer in degrees, around the point specified in the SvgRotationPoint
    pub rotation: Option<(SvgRotationPoint, SvgRotation)>,
    /// Translates the individual layer additionally to the whole SVG
    pub translation: Option<SvgTranslation>,
    /// Scaling factor of this shape
    pub scale: Option<SvgScaleFactor>,
}

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct SvgRotation { degrees: f32 }

impl SvgRotation {
    /// Note: Assumes that the input is in degrees, not radians!
    pub fn degrees(degrees: f32) -> Self { Self { degrees } }

    pub fn to_degrees(&self) -> f32 { self.degrees }

    // Returns the (sin, cos) in radians
    fn to_rotation(&self) -> (f32, f32) {
        let rad = self.degrees.to_radians();
        (rad.sin(), rad.cos())
    }
}

/// Rotation point, local to the current SVG layer, i.e. (0.0, 0.0) will
/// rotate the shape on the top left corner
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct SvgRotationPoint { pub x: f32, pub y: f32 }

/// Scale factor (1.0, 1.0) by default. Unit is in normalized percent.
/// Shapes can be stretched and squished.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SvgScaleFactor { pub x: f32, pub y: f32 }

impl Default for SvgScaleFactor {
    fn default() -> Self {
        SvgScaleFactor { x: 1.0, y: 1.0 }
    }
}

/// Translation **in pixels** (or whatever the source unit for rendered SVG data
/// is, but usually this will be pixels)
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub struct SvgTranslation { pub x: f32, pub y: f32 }

impl SvgStyle {
    pub fn stroked(color: ColorU, stroke_opts: SvgStrokeOptions) -> Self {
        Self {
            stroke: Some((color, stroke_opts)),
            .. Default::default()
        }
    }

    pub fn filled(color: ColorU) -> Self {
        Self {
            fill: Some(color),
            .. Default::default()
        }
    }
}
// similar to lyon::SvgStrokeOptions, except the
// thickness is a usize (f32 * 1000 as usize), in order
// to implement Hash
#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub struct SvgStrokeOptions {
    /// What cap to use at the start of each sub-path.
    ///
    /// Default value: `LineCap::Butt`.
    pub start_cap: SvgLineCap,

    /// What cap to use at the end of each sub-path.
    ///
    /// Default value: `LineCap::Butt`.
    pub end_cap: SvgLineCap,

    /// See the SVG specification.
    ///
    /// Default value: `LineJoin::Miter`.
    pub line_join: SvgLineJoin,

    /// Line width
    ///
    /// Default value: `StrokeOptions::DEFAULT_LINE_WIDTH`.
    line_width: usize,

    /// See the SVG specification.
    ///
    /// Must be greater than or equal to 1.0.
    /// Default value: `StrokeOptions::DEFAULT_MITER_LIMIT`.
    miter_limit: usize,

    /// Maximum allowed distance to the path when building an approximation.
    ///
    /// See [Flattening and tolerance](index.html#flattening-and-tolerance).
    /// Default value: `StrokeOptions::DEFAULT_TOLERANCE`.
    tolerance: usize,

    /// Apply line width
    ///
    /// When set to false, the generated vertices will all be positioned in the centre
    /// of the line. The width can be applied later on (eg in a vertex shader) by adding
    /// the vertex normal multiplied by the line with to each vertex position.
    ///
    /// Default value: `true`.
    pub apply_line_width: bool,
}

const SVG_LINE_PRECISION: f32 = 1000.0;

impl SvgStrokeOptions {
    /// NOTE: Getters and setters are necessary here, because the line width, miter limit, etc.
    /// are all normalized to fit into a usize
    pub fn with_line_width(mut self, line_width: f32) -> Self { self.set_line_width(line_width); self }
    pub fn set_line_width(&mut self, line_width: f32) { self.line_width = (line_width * SVG_LINE_PRECISION) as usize; }
    pub fn get_line_width(&self) -> f32 { self.line_width as f32 / SVG_LINE_PRECISION }
    pub fn with_miter_limit(mut self, miter_limit: f32) -> Self { self.set_miter_limit(miter_limit); self }
    pub fn set_miter_limit(&mut self, miter_limit: f32) { self.miter_limit = (miter_limit * SVG_LINE_PRECISION) as usize; }
    pub fn get_miter_limit(&self) -> f32 { self.miter_limit as f32 / SVG_LINE_PRECISION }
    pub fn with_tolerance(mut self, tolerance: f32) -> Self { self.set_tolerance(tolerance); self }
    pub fn set_tolerance(&mut self, tolerance: f32) { self.tolerance = (tolerance * SVG_LINE_PRECISION) as usize; }
    pub fn get_tolerance(&self) -> f32 { self.tolerance as f32 / SVG_LINE_PRECISION }
}

impl Into<StrokeOptions> for SvgStrokeOptions {
    fn into(self) -> StrokeOptions {
        let target = StrokeOptions::default()
            .with_tolerance(self.get_tolerance())
            .with_start_cap(self.start_cap.into())
            .with_end_cap(self.end_cap.into())
            .with_line_join(self.line_join.into())
            .with_line_width(self.get_line_width())
            .with_miter_limit(self.get_miter_limit());

        if !self.apply_line_width {
            target.dont_apply_line_width()
        } else {
            target
        }
    }
}

impl Default for SvgStrokeOptions {
    fn default() -> Self {
        const DEFAULT_MITER_LIMIT: f32 = 4.0;
        const DEFAULT_LINE_WIDTH: f32 = 1.0;
        const DEFAULT_TOLERANCE: f32 = 0.1;

        Self {
            start_cap: SvgLineCap::default(),
            end_cap: SvgLineCap::default(),
            line_join: SvgLineJoin::default(),
            line_width: (DEFAULT_LINE_WIDTH * SVG_LINE_PRECISION) as usize,
            miter_limit: (DEFAULT_MITER_LIMIT * SVG_LINE_PRECISION) as usize,
            tolerance: (DEFAULT_TOLERANCE * SVG_LINE_PRECISION) as usize,
            apply_line_width: true,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub enum SvgLineCap {
    Butt,
    Square,
    Round,
}

impl Default for SvgLineCap {
    fn default() -> Self {
        SvgLineCap::Butt
    }
}

impl Into<LineCap> for SvgLineCap {
    #[inline]
    fn into(self) -> LineCap {
        use self::SvgLineCap::*;
        match self {
            Butt => LineCap::Butt,
            Square => LineCap::Square,
            Round => LineCap::Round,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub enum SvgLineJoin {
    Miter,
    MiterClip,
    Round,
    Bevel,
}

impl Default for SvgLineJoin {
    fn default() -> Self {
        SvgLineJoin::Miter
    }
}

impl Into<LineJoin> for SvgLineJoin {
    #[inline]
    fn into(self) -> LineJoin {
        use self::SvgLineJoin::*;
        match self {
            Miter => LineJoin::Miter,
            MiterClip => LineJoin::MiterClip,
            Round => LineJoin::Round,
            Bevel => LineJoin::Bevel,
        }
    }
}

/// One "layer" is simply one or more polygons that get drawn using the same style
/// i.e. one SVG `<path></path>` element
///
/// Note: If you want to draw text in a SVG element, you need to convert the character
/// of the font to a `Vec<PathEvent` via `SvgLayerType::from_character`
#[derive(Debug, Clone)]
pub enum SvgLayerType {
    Polygon(Vec<PathEvent>),
    Circle(SvgCircle),
    Rect(SvgRect),
}

#[derive(Debug, Copy, Clone)]
pub struct SvgVert {
    pub xy: (f32, f32),
    pub normal: (f32, f32),
}

// implement_vertex!(SvgVert, xy, normal);
impl VertexLayoutDescription for SvgVert {
    fn get_description() -> VertexLayout {
        use std::mem;
        VertexLayout {
            fields: vec![
                VertexAttribute {
                    name: "vAttrXY",
                    layout_location: None,
                    attribute_type: VertexAttributeType::Float,
                    item_size: mem::size_of::<f32>(),
                    item_count: 2,
                },
                VertexAttribute {
                    name: "vAttrNormal",
                    layout_location: None,
                    attribute_type: VertexAttributeType::Float,
                    item_size: mem::size_of::<f32>(),
                    item_count: 2,
                }
            ],
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub struct SvgWorldPixel;

pub type GlyphId = u32;

/// A vectorized font holds the glyphs for a given font, but in a vector format
#[derive(Debug, Clone, Default)]
pub struct VectorizedFont {
    /// Glyph -> Polygon map
    glyph_polygon_map: RefCell<FastHashMap<GlyphId, VertexBuffers<SvgVert, u32>>>,
    /// Glyph -> Stroke map
    glyph_stroke_map: RefCell<FastHashMap<GlyphId, VertexBuffers<SvgVert, u32>>>,
    /// Original font bytes
    font_bytes: Vec<u8>,
}

use stb_truetype::{FontInfo, Vertex};

impl VectorizedFont {

    /// Prepares a vectorized font from a set of bytes
    pub fn from_bytes(font_bytes: Vec<u8>) -> Self {
        Self {
            font_bytes,
            .. Default::default()
        }
    }

    pub fn get_fill_vertices(&self, glyphs: &[GlyphInstance]) -> Vec<VertexBuffers<SvgVert, u32>> {

        let font_info = match FontInfo::new(self.font_bytes.clone(), 0) {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut borrow_mut = self.glyph_polygon_map.borrow_mut();
        glyphs.iter().filter_map(|glyph| {
            match borrow_mut.entry(glyph.index) {
                Occupied(o) => Some(o.get().clone()),
                Vacant(v) => {
                    let glyph_shape = font_info.get_glyph_shape(glyph.index)?;
                    let poly = glyph_to_svg_layer_type(glyph_shape);
                    let mut path = None;
                    let polygon_verts = poly.tesselate_fill(DEFAULT_GLYPH_TOLERANCE, &mut path);
                    v.insert(polygon_verts.clone());
                    Some(polygon_verts)
                }
            }
        }).collect()
    }

    pub fn get_stroke_vertices(&self, glyphs: &[GlyphInstance], stroke_options: &SvgStrokeOptions) -> Vec<VertexBuffers<SvgVert, u32>> {

        let font_info = match FontInfo::new(self.font_bytes.clone(), 0) {
            Some(s) => s,
            None => return Vec::new(),
        };

        let mut borrow_mut = self.glyph_stroke_map.borrow_mut();
        glyphs.iter().filter_map(|glyph| {
            match borrow_mut.entry(glyph.index) {
                Occupied(o) => Some(o.get().clone()),
                Vacant(v) => {
                    let glyph_shape = font_info.get_glyph_shape(glyph.index)?;
                    let poly = glyph_to_svg_layer_type(glyph_shape);
                    let mut path = None;
                    let stroke_verts = poly.tesselate_stroke(DEFAULT_GLYPH_TOLERANCE, &mut path, *stroke_options);
                    v.insert(stroke_verts.clone());
                    Some(stroke_verts)
                }
            }
        }).collect()
    }
}


/// Converts a `Vec<stb_truetype::Vertex>` to a `SvgLayerType::Polygon`
fn glyph_to_svg_layer_type(vertices: Vec<Vertex>) -> SvgLayerType {
    SvgLayerType::Polygon(vertices.into_iter().map(rusttype_glyph_to_path_events).collect())
}

// Convert a Rusttype glyph to a Vec of PathEvents,
// in order to turn a glyph into a polygon
fn rusttype_glyph_to_path_events(vertex: Vertex) -> PathEvent {

    use stb_truetype::VertexType;

    // Rusttypes vertex type needs to be inverted in the Y axis
    // in order to work with lyon correctly
    match vertex.vertex_type() {
        VertexType::CurveTo =>  PathEvent::QuadraticTo(
                                    Point::new(vertex.cx as f32, -(vertex.cy as f32)),
                                    Point::new(vertex.x as f32,  -(vertex.y as f32))
                                ),
        VertexType::MoveTo =>   PathEvent::MoveTo(Point::new(vertex.x as f32, -(vertex.y as f32))),
        VertexType::LineTo =>   PathEvent::LineTo(Point::new(vertex.x as f32, -(vertex.y as f32))),
    }
}

#[derive(Debug, Clone, Default)]
pub struct VectorizedFontCache {
    /// Font -> Vectorized glyph map
    ///
    /// Needs to be wrapped in a RefCell / Rc since we want to lazy-load the
    /// fonts to keep the memory usage down
    vectorized_fonts: RefCell<FastHashMap<FontId, Rc<VectorizedFont>>>,
}

impl VectorizedFontCache {

    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_if_not_exist(&mut self, id: FontId, font_bytes: Vec<u8>) {
        self.vectorized_fonts.borrow_mut().entry(id).or_insert_with(|| Rc::new(VectorizedFont::from_bytes(font_bytes)));
    }

    pub fn insert(&mut self, id: FontId, font: VectorizedFont) {
        self.vectorized_fonts.borrow_mut().insert(id, Rc::new(font));
    }

    /// Returns true if the font cache has the respective font
    pub fn has_font(&self, id: &FontId) -> bool {
        self.vectorized_fonts.borrow().get(id).is_some()
    }

    pub fn get_font(&self, id: &FontId, app_resources: &AppResources) -> Option<Rc<VectorizedFont>> {
        use std::collections::hash_map::Entry::*;
        use app_resources::font_source_get_bytes;

        match self.vectorized_fonts.borrow_mut().entry(id.clone()) {
            Occupied(_) => { },
            Vacant(v) => {
                let font_bytes = app_resources.get_font_source(&id).map(font_source_get_bytes)?;
                let (font_bytes, _) = font_bytes.ok()?;
                v.insert(Rc::new(VectorizedFont::from_bytes(font_bytes)));
            }
        }

        self.vectorized_fonts.borrow().get(&id).map(|font| font.clone())
    }

    pub fn remove_font(&mut self, id: &FontId) {
        self.vectorized_fonts.borrow_mut().remove(id);
    }
}

impl SvgLayerType {

    pub fn tesselate_fill(&self, tolerance: f32, polygon: &mut Option<Path>)
    -> VertexBuffers<SvgVert, u32>
    {
        let mut geometry = VertexBuffers::new();

        match self {
            SvgLayerType::Polygon(p) => {
                if polygon.is_none() {
                    *polygon = Some(build_path_from_polygon(&p, tolerance));
                }

                let path = polygon.as_ref().unwrap();

                let mut tessellator = FillTessellator::new();
                tessellator.tessellate_path(
                    path.path_iter(),
                    &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }),
                ).unwrap();
            },
            SvgLayerType::Circle(c) => {
                let center = TypedPoint2D::new(c.center_x, c.center_y);
                fill_circle(center, c.radius, &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));
            },
            SvgLayerType::Rect(r) => {
                let (rect, radii) = get_radii(&r);
                fill_rounded_rectangle(&rect, &radii, &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));
            }
        }

        geometry
    }

    pub fn tesselate_stroke(
        &self,
        tolerance: f32,
        polygon: &mut Option<Path>,
        stroke: SvgStrokeOptions
    ) -> VertexBuffers<SvgVert, u32> {

        let mut stroke_geometry = VertexBuffers::new();
        let stroke_options: StrokeOptions = stroke.into();
        let stroke_options = stroke_options.with_tolerance(tolerance);

        match self {
            SvgLayerType::Polygon(p) => {
                if polygon.is_none() {
                    *polygon = Some(build_path_from_polygon(&p, tolerance));
                }

                let path = polygon.as_ref().unwrap();

                let mut stroke_tess = StrokeTessellator::new();
                stroke_tess.tessellate_path(
                    path.path_iter(),
                    &stroke_options,
                    &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }),
                );
            },
            SvgLayerType::Circle(c) => {
                let center = TypedPoint2D::new(c.center_x, c.center_y);
                stroke_circle(center, c.radius, &stroke_options,
                    &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));
            },
            SvgLayerType::Rect(r) => {
                let (rect, radii) = get_radii(&r);
                stroke_rounded_rectangle(&rect, &radii, &stroke_options,
                    &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));
            },
        }

        stroke_geometry
    }
}

fn get_radii(r: &SvgRect) -> (TypedRect<f32, UnknownUnit>, BorderRadii) {
    let rect = TypedRect::new(TypedPoint2D::new(r.x, r.y), TypedSize2D::new(r.width, r.height));
    let radii = BorderRadii {
        top_left: r.rx,
        top_right: r.rx,
        bottom_left: r.rx,
        bottom_right: r.rx,
    };
    (rect, radii)
}

fn build_path_from_polygon(polygon: &[PathEvent], tolerance: f32) -> Path {
    let mut builder = Builder::with_capacity(polygon.len()).flattened(tolerance);
    for event in polygon {
        builder.path_event(*event);
    }
    builder.with_svg().build()
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SvgCircle {
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

impl SvgCircle {
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        let x_diff = (x - self.center_x).abs();
        let y_diff = (y - self.center_y).abs();
        (x_diff * x_diff) + (y_diff * y_diff) < (self.radius * self.radius)
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SvgRect {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub rx: f32,
    pub ry: f32,
}

impl SvgRect {
    /// Note: does not incorporate rounded edges!
    /// Origin of x and y is assumed to be the top left corner
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x > self.x &&
        x < self.x + self.width &&
        y > self.y &&
        y < self.y + self.height
    }
}

#[cfg(feature = "svg_parsing")]
mod svg_to_lyon {

    use lyon::{
        math::Point,
        path::PathEvent,
    };
    use usvg::{Tree, PathSegment, Color, Options, Paint, Stroke, LineCap, LineJoin, NodeKind};
    use widgets::svg::{
        SvgStrokeOptions, SvgLineCap, SvgLineJoin,
        SvgLayerType, SvgStyle, SvgParseError
    };
    use azul_css::ColorU;

    pub fn parse_from<S: AsRef<str>>(svg_source: S) -> Result<Vec<(Vec<SvgLayerType>, SvgStyle)>, SvgParseError> {

        let opt = Options::default();
        let rtree = Tree::from_str(svg_source.as_ref(), &opt).unwrap();

        let mut layer_data = Vec::new();

        for node in rtree.root().descendants() {
            if let NodeKind::Path(p) = &*node.borrow() {
                let mut style = SvgStyle::default();

                if let Some(ref fill) = p.fill {

                    // fall back to always use color fill
                    // no gradients (yet?)
                    let color = match fill.paint {
                        Paint::Color(c) => c,
                        _ => FALLBACK_COLOR,
                    };

                    style.fill = Some(ColorU {
                        r: color.red,
                        g: color.green,
                        b: color.blue,
                        a: (fill.opacity.value() * 255.0) as u8
                    });
                }

                if let Some(ref stroke) = p.stroke {
                    style.stroke = Some(convert_stroke(stroke));
                }

                let layer = vec![SvgLayerType::Polygon(p.segments.iter().map(|e| as_event(e)).collect())];
                layer_data.push((layer, style));
            }
        }

        Ok(layer_data)
    }

    // Map resvg::tree::PathSegment to lyon::path::PathEvent
    fn as_event(ps: &PathSegment) -> PathEvent {
        match *ps {
            PathSegment::MoveTo { x, y } => PathEvent::MoveTo(Point::new(x as f32, y as f32)),
            PathSegment::LineTo { x, y } => PathEvent::LineTo(Point::new(x as f32, y as f32)),
            PathSegment::CurveTo { x1, y1, x2, y2, x, y, } => {
                PathEvent::CubicTo(
                    Point::new(x1 as f32, y1 as f32),
                    Point::new(x2 as f32, y2 as f32),
                    Point::new(x as f32, y as f32))
            }
            PathSegment::ClosePath => PathEvent::Close,
        }
    }

    pub const FALLBACK_COLOR: Color = Color {
        red: 0,
        green: 0,
        blue: 0,
    };

    // dissect a resvg::Stroke into a webrender::ColorU + SvgStrokeOptions
    pub fn convert_stroke(s: &Stroke) -> (ColorU, SvgStrokeOptions) {

        let color = match s.paint {
            Paint::Color(c) => c,
            _ => FALLBACK_COLOR,
        };
        let line_cap = match s.linecap {
            LineCap::Butt => SvgLineCap::Butt,
            LineCap::Square => SvgLineCap::Square,
            LineCap::Round => SvgLineCap::Round,
        };
        let line_join = match s.linejoin {
            LineJoin::Miter => SvgLineJoin::Miter,
            LineJoin::Bevel => SvgLineJoin::Bevel,
            LineJoin::Round => SvgLineJoin::Round,
        };

        let opts = SvgStrokeOptions {
            start_cap: line_cap,
            end_cap: line_cap,
            line_join,
            .. SvgStrokeOptions::default().with_line_width(s.width as f32)
        };

        (ColorU {
            r: color.red,
            g: color.green,
            b: color.blue,
            a: (s.opacity.value() * 255.0) as u8
        }, opts)
    }
}

#[derive(Debug, Clone)]
pub struct Svg {
    /// Currently active layers
    pub layers: Vec<SvgLayerResource>,
    /// Pan (horizontal, vertical) in pixels
    pub pan: (f32, f32),
    /// 1.0 = default zoom
    pub zoom: f32,
    /// Whether an FXAA shader should be applied to the resulting OpenGL texture
    pub enable_fxaa: bool,
    /// Should the SVG add the current HiDPI factor to the zoom?
    pub enable_hidpi: bool,
    /// Background color (default: transparent)
    pub background_color: ColorU,
    /// Multisampling (default: 1.0) - since there is no anti-aliasing yet, simply
    /// increases the texture size that is drawn to.
    pub multisampling_factor: usize,
}

impl Default for Svg {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            pan: (0.0, 0.0),
            zoom: 1.0,
            enable_fxaa: false,
            enable_hidpi: true,
            background_color: ColorU { r: 0, b: 0, g: 0, a: 0 },
            multisampling_factor: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SvgLayerResource {
    Reference((SvgLayerId, SvgStyle)),
    Direct(SvgLayerResourceDirect),
}

#[derive(Debug, Clone)]
pub struct SvgLayerResourceDirect {
    pub style: SvgStyle,
    pub fill: Option<VerticesIndicesBuffer>,
    pub stroke: Option<VerticesIndicesBuffer>,
}

impl SvgLayerResourceDirect {
    pub fn tesselate_from_layer(data: &[SvgLayerType], style: SvgStyle) -> Self {
        tesselate_polygon_data(data, style)
    }
}

#[derive(Debug, Clone, Default)]
pub struct VerticesIndicesBuffer {
    pub vertices: Vec<SvgVert>,
    pub indices: Vec<u32>,
}

#[cfg_attr(feature = "serde_serialization", derive(Serialize, Deserialize))]
#[derive(Debug, Copy, Clone)]
pub struct BezierControlPoint {
    pub x: f32,
    pub y: f32,
}

impl BezierControlPoint {
    /// Distance of two points
    pub fn distance(&self, other: &Self) -> f32 {
        ((other.x - self.x).powi(2) + (other.y - self.y).powi(2)).sqrt()
    }
}

/// Bezier formula for cubic curves (start, handle 1, handle 2, end).
///
/// ## Inputs
///
/// - `curve`: The 4 handles of the curve
/// - `t`: The interpolation amount - usually between 0.0 and 1.0 if the point
///   should be between the start and end
///
/// ## Returns
///
/// - `BezierControlPoint`: The calculated point which lies on the curve,
///    according the the bezier formula
pub fn cubic_interpolate_bezier(curve: &[BezierControlPoint;4], t: f32) -> BezierControlPoint {
    let one_minus = 1.0 - t;
    let one_minus_square = one_minus.powi(2);
    let one_minus_cubic = one_minus.powi(3);

    let t_pow2 = t.powi(2);
    let t_pow3 = t.powi(3);

    let x =         one_minus_cubic  *             curve[0].x
            + 3.0 * one_minus_square * t         * curve[1].x
            + 3.0 * one_minus        * t_pow2    * curve[2].x
            +                          t_pow3    * curve[3].x;

    let y =         one_minus_cubic  *             curve[0].y
            + 3.0 * one_minus_square * t         * curve[1].y
            + 3.0 * one_minus        * t_pow2    * curve[2].y
            +                          t_pow3    * curve[3].y;

    BezierControlPoint { x, y }
}

pub fn quadratic_interpolate_bezier(curve: &[BezierControlPoint;3], t: f32) -> BezierControlPoint {
    let one_minus = 1.0 - t;
    let one_minus_square = one_minus.powi(2);

    let t_pow2 = t.powi(2);

    // TODO: Why 3.0 and not 2.0?

    let x =         one_minus_square *             curve[0].x
            + 2.0 * one_minus        * t         * curve[1].x
            + 3.0                    * t_pow2    * curve[2].x;

    let y =         one_minus_square *             curve[0].y
            + 2.0 * one_minus        * t         * curve[1].y
            + 3.0                    * t_pow2    * curve[2].y;

    BezierControlPoint { x, y }
}

#[derive(Debug, Copy, Clone)]
pub struct BezierNormalVector {
    pub x: f32,
    pub y: f32,
}

impl BezierNormalVector {
    pub fn to_rotation(&self) -> BezierCharacterRotation {
        BezierCharacterRotation((-self.x).atan2(self.y))
    }
}

/// Calculates the normal vector at a certain point (perpendicular to the curve)
pub fn cubic_bezier_normal(curve: &[BezierControlPoint;4], t: f32) -> BezierNormalVector {

    // 1. Calculate the derivative of the bezier curve
    //
    // This means, we go from 4 control points to 3 control points and redistribute
    // the weights of the control points according to the formula:
    //
    // w'0 = 3(w1-w0)
    // w'1 = 3(w2-w1)
    // w'2 = 3(w3-w2)

    let weight_1_x = 3.0 * (curve[1].x - curve[0].x);
    let weight_1_y = 3.0 * (curve[1].y - curve[0].y);

    let weight_2_x = 3.0 * (curve[2].x - curve[1].x);
    let weight_2_y = 3.0 * (curve[2].y - curve[1].y);

    let weight_3_x = 3.0 * (curve[3].x - curve[2].x);
    let weight_3_y = 3.0 * (curve[3].y - curve[2].y);

    // The first derivative of a cubic bezier curve is a quadratic bezier curve
    // Luckily, the first derivative is also the tangent vector. So all we need to do
    // is to get the quadratic bezier
    let mut tangent = quadratic_interpolate_bezier(&[
        BezierControlPoint { x: weight_1_x, y: weight_1_y },
        BezierControlPoint { x: weight_2_x, y: weight_2_y },
        BezierControlPoint { x: weight_3_x, y: weight_3_y },
    ], t);

    // We normalize the tangent to have a lenght of 1
    let tangent_length = (tangent.x.powi(2) + tangent.y.powi(2)).sqrt();
    tangent.x /= tangent_length;
    tangent.y /= tangent_length;

    // The tangent is the vector that runs "along" the curve at a specific point.
    // To get the normal (to calcuate the rotation of the characters), we need to
    // rotate the tangent vector by 90 degrees.
    //
    // Rotating by 90 degrees is very simple, as we only need to flip the x and y axis

    BezierNormalVector {
        x: -tangent.y,
        y: tangent.x,
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SvgTextPlacement {
    /// Text is simply layouted from left-to-right
    Unmodified,
    /// Text is rotated by X degrees
    Rotated(f32),
    /// Text is placed on a cubic bezier curve
    OnCubicBezierCurve(SampledBezierCurve),
}

#[derive(Debug, Clone)]
pub struct SvgText {
    /// Font size of the text, in pixels
    pub font_size_px: f32,
    /// Font ID, such as FontId(0)
    pub font_id: FontId,
    /// What are the words / glyphs in this text
    pub text_layout: SvgTextLayout,
    /// What is the font color & stroke (if any)?
    pub style: SvgStyle,
    /// Is the text rotated or on a curve?
    pub placement: SvgTextPlacement,
}

/// An axis-aligned bounding box (not rotated / skewed)
#[derive(Debug, Copy, Clone)]
pub struct SvgBbox(pub TypedRect<f32, SvgWorldPixel>);

impl SvgBbox {

    /// Simple function for drawing a single bounding box
    pub fn draw_lines(&self, color: ColorU, line_width: f32) -> SvgLayerResourceDirect {
        quick_rects(&[SvgRect {
            width: self.0.size.width,
            height: self.0.size.height,
            x: self.0.origin.x,
            y: self.0.origin.y,
            rx: 0.0,
            ry: 0.0,
        }],
        Some(color),
        None,
        Some(SvgStrokeOptions::default().with_line_width(line_width))
    )}

    /// Checks if the bounding box contains a point
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        self.0.contains(&TypedPoint2D::new(x, y))
    }

    /// Translate the SvgBbox by x / y
    pub fn translate(&mut self, x: f32, y: f32) {
        self.0 = self.0.translate(&TypedVector2D::new(x, y));
    }
}

#[test]
fn translate_bbox() {
    let mut bbox = SvgBbox(TypedRect::zero());
    bbox.translate(200.0, 300.0);
    assert_eq!(bbox.0.origin.x, 200.0);
    assert_eq!(bbox.0.origin.y, 300.0);
}

pub fn is_point_in_shape(point: (f32, f32), shape: &[(f32, f32)]) -> bool {
    if shape.len() < 3 {
        // Shape must at least have 3 points, i.e. be a triangle
        return false;
    }

    // We iterate over the shape in 2 points.
    //
    // If the mouse cursor (target point) is on the left side for all points,
    // then cursor is inside of the shape. If it appears on the right side for
    // only one point, we know that it isn't inside the target shape.
    // all() is lazy and will quit on the first result where the target is not
    // inside the shape.
    shape.iter().zip(shape.iter().skip(1)).all(|(start, end)| {
        !(side_of_point(point, *start, *end).is_sign_positive())
    })
}

/// Determine which side of a vector the point is on.
///
/// Depending on if the result of this function is positive or negative,
/// the target point lies either right or left to the imaginary line from (start -> end)
#[inline]
pub fn side_of_point(target: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    ((target.0 - start.0) * (end.1 - start.1)) -
    ((target.1 - start.1) * (end.0 - start.0))
}

/// Creates a text layout for a single string of text
#[derive(Debug, Clone)]
pub struct SvgTextLayout {
    /// The words, broken up by whitespace
    pub words: Words,
    /// Words, scaled by a certain font size (with font metrics)
    pub scaled_words: ScaledWords,
    /// Layout of the positions, word-by-word
    pub word_positions: WordPositions,
    /// Positioned and horizontally aligned glyphs
    pub layouted_glyphs: LayoutedGlyphs,
    /// At what glyphs does the line actually break (necessary for aligning content)
    pub line_breaks: LineBreaks,
}

/// Since the SvgText is scaled on the GPU, the font size doesn't matter here
const SVG_FAKE_FONT_SIZE: f32 = 64.0;

impl SvgTextLayout {

    /// Calculate the text layout from a font and a font size -
    /// note: the idea is to let the user cache the returned result,
    /// it is not recommended to run this function on every frame,
    /// since it can be very expensive
    pub fn from_str(
        text: &str,
        font_bytes: &[u8],
        font_index: u32,
        text_layout_options: &TextLayoutOptions,
        horizontal_alignment: StyleTextAlignmentHorz,
    ) -> Self {

        use text_layout;

        let words = text_layout::split_text_into_words(text);
        let scaled_words = text_layout::words_to_scaled_words(&words, font_bytes, font_index, SVG_FAKE_FONT_SIZE);
        let word_positions = text_layout::position_words(&words, &scaled_words, text_layout_options, SVG_FAKE_FONT_SIZE);
        let (layouted_glyphs, line_breaks) = text_layout::get_layouted_glyphs_with_horizonal_alignment(&word_positions, &scaled_words, horizontal_alignment);

        SvgTextLayout {
           words, scaled_words, word_positions, layouted_glyphs, line_breaks
        }
    }

    /// Get the bounding box of a layouted text
    pub fn get_bbox(&self, placement: &SvgTextPlacement) -> SvgBbox {
        use self::SvgTextPlacement::*;

        // TODO: Scale by font size!

        let normal_width = self.word_positions.content_size.width;
        let normal_height = self.word_positions.content_size.height;

        SvgBbox(match placement {
            Unmodified => {
                TypedRect::new(
                    TypedPoint2D::new(0.0, 0.0),
                    TypedSize2D::new(normal_width, normal_height)
                )
            },
            Rotated(r) => {

                fn rotate_point((x, y): (f32, f32), sin: f32, cos: f32) -> (f32, f32) {
                    ((x * cos) - (y * sin), (x * sin) + (y * cos))
                }

                let rot_radians = r.to_radians();
                let sin = rot_radians.sin();
                let cos = rot_radians.cos();

                let top_left = (0.0, 0.0);
                let top_right = (0.0 + normal_width, 0.0);
                let bottom_right = (0.0 + normal_width, normal_height);
                let bottom_left = (0.0, normal_height);

                let (top_left_x, top_left_y) = rotate_point(top_left, sin, cos);
                let (top_right_x, top_right_y) = rotate_point(top_right, sin, cos);
                let (bottom_right_x, bottom_right_y) = rotate_point(bottom_right, sin, cos);
                let (bottom_left_x, bottom_left_y) = rotate_point(bottom_left, sin, cos);

                let min_x = top_left_x.min(top_right_x).min(bottom_right_x).min(bottom_left_x);
                let max_x = top_left_x.max(top_right_x).max(bottom_right_x).max(bottom_left_x);
                let min_y = top_left_y.min(top_right_y).min(bottom_right_y).min(bottom_left_y);
                let max_y = top_left_y.max(top_right_y).max(bottom_right_y).max(bottom_left_y);

                TypedRect::new(
                    TypedPoint2D::new(min_x, min_y),
                    TypedSize2D::new(max_x - min_x, max_y - min_y)
                )
            },
            OnCubicBezierCurve(curve) => {
                let (mut bbox, _bbox_indices) = curve.get_bbox();

                // TODO: There should be a more sophisticated Bbox calculation here
                // that takes the rotation of the text into account. Right now we simply
                // add the font size to the BBox height, so that we can still select text
                // even when the control points are aligned in a horizontal line.
                //
                // This is not so much about correctness as it is about simply making
                // it work for now.

                bbox.0.origin.y -= SVG_FAKE_FONT_SIZE;
                bbox.0.size.height += SVG_FAKE_FONT_SIZE;
                bbox.0
            }
        })
    }
}

impl SvgText {

    pub fn to_svg_layer(&self, vectorized_fonts_cache: &VectorizedFontCache, resources: &AppResources) -> SvgLayerResourceDirect {

        let vectorized_font = vectorized_fonts_cache.get_font(&self.font_id, resources).unwrap();

        // The text contains the vertices and indices in unscaled units. This is so that the font
        // can be cached and later on be scaled and rotated on the GPU instead of the CPU.
        let mut text = match self.placement {
            SvgTextPlacement::Unmodified => {
                normal_text(&self.text_layout, self.style, &*vectorized_font)
            },
            SvgTextPlacement::Rotated(degrees) => {
                let mut text = normal_text(&self.text_layout, self.style, &*vectorized_font);
                text.style.rotate(degrees);
                text
            },
            SvgTextPlacement::OnCubicBezierCurve(curve) => {
                text_on_curve(&self.text_layout, self.style, &*vectorized_font, &curve)
            },
        };

        // The glyphs are laid out to be 1px high, they are then later scaled to the correct font size
        text.style.scale(self.font_size_px, self.font_size_px);
        text
    }

    pub fn get_bbox(&self) -> SvgBbox {
        let mut bbox = self.text_layout.get_bbox(&self.placement);
        let translation = self.style.transform.translation.unwrap_or_default();
        bbox.translate(translation.x, translation.y);
        bbox
    }
}

pub fn normal_text(
    layout: &SvgTextLayout,
    text_style: SvgStyle,
    vectorized_font: &VectorizedFont,
) -> SvgLayerResourceDirect
{
    let fill_vertices = text_style.fill.map(|_| {
        let fill_verts = vectorized_font.get_fill_vertices(&layout.layouted_glyphs.glyphs);
        normal_text_to_vertices(&layout.layouted_glyphs.glyphs, fill_verts)
    });

    let stroke_vertices = text_style.stroke.map(|stroke| {
        let stroke_verts = vectorized_font.get_stroke_vertices(&layout.layouted_glyphs.glyphs, &stroke.1);
        normal_text_to_vertices(&layout.layouted_glyphs.glyphs, stroke_verts)
    });

    SvgLayerResourceDirect {
        style: text_style,
        fill: fill_vertices,
        stroke: stroke_vertices,
    }
}

pub fn normal_text_to_vertices(
    glyph_ids: &[GlyphInstance],
    mut vertex_buffers: Vec<VertexBuffers<SvgVert, u32>>,
) -> VerticesIndicesBuffer
{
    normal_text_to_vertices_inner(glyph_ids, &mut vertex_buffers);
    join_vertex_buffers(&vertex_buffers)
}

fn normal_text_to_vertices_inner(
    glyph_ids: &[GlyphInstance],
    vertex_buffers: &mut Vec<VertexBuffers<SvgVert, u32>>,
) {
    vertex_buffers.iter_mut().zip(glyph_ids).for_each(|(vertex_buf, gid)| {
        // NOTE: The gid.point has the font size already applied to it,
        // so we have to un-do the scaling for the glyph offsets, so all other scaling can be done on the GPU
        transform_vertex_buffer(&mut vertex_buf.vertices, gid.point.x / SVG_FAKE_FONT_SIZE, gid.point.y / SVG_FAKE_FONT_SIZE);
    });
}

pub fn text_on_curve(
    layout: &SvgTextLayout,
    text_style: SvgStyle,
    vectorized_font: &VectorizedFont,
    curve: &SampledBezierCurve
) -> SvgLayerResourceDirect
{
    // NOTE: char offsets are now in unscaled glyph space!
    let (char_offsets, char_rotations) = curve.get_text_offsets_and_rotations(&layout.layouted_glyphs.glyphs, 0.0);

    let fill_vertices = text_style.fill.map(|_| {
        let fill_verts = vectorized_font.get_fill_vertices(&layout.layouted_glyphs.glyphs);
        curved_vector_text_to_vertices(&char_offsets, &char_rotations, fill_verts)
    });

    let stroke_vertices = text_style.stroke.map(|stroke| {
        let stroke_verts = vectorized_font.get_stroke_vertices(&layout.layouted_glyphs.glyphs, &stroke.1);
        curved_vector_text_to_vertices(&char_offsets, &char_rotations, stroke_verts)
    });

    SvgLayerResourceDirect {
        style: text_style,
        fill: fill_vertices,
        stroke: stroke_vertices,
    }
}

// Calculates the layout for one word block
pub fn curved_vector_text_to_vertices(
    char_offsets: &[(f32, f32)],
    char_rotations: &[BezierCharacterRotation],
    mut vertex_buffers: Vec<VertexBuffers<SvgVert, u32>>,
) -> VerticesIndicesBuffer
{
    vertex_buffers.iter_mut()
    .zip(char_rotations.into_iter())
    .zip(char_offsets.iter())
    .for_each(|((vertex_buf, char_rot), char_offset)| {
        let (char_offset_x, char_offset_y) = char_offset; // weird borrow issue
        // 1. Rotate individual characters inside of the word
        let (char_sin, char_cos) = (char_rot.0.sin(), char_rot.0.cos());
        rotate_vertex_buffer(&mut vertex_buf.vertices, char_sin, char_cos);
        // 2. Transform characters to their respective positions
        transform_vertex_buffer(&mut vertex_buf.vertices, *char_offset_x, *char_offset_y);
    });

    join_vertex_buffers(&vertex_buffers)
}

impl Svg {

    #[inline]
    pub fn with_layers(layers: Vec<SvgLayerResource>) -> Self {
        Self { layers: layers, .. Default::default() }
    }

    #[inline]
    pub fn with_pan(mut self, horz: f32, vert: f32) -> Self {
        self.pan = (horz, vert);
        self
    }

    #[inline]
    pub fn with_zoom(mut self, zoom: f32) -> Self {
        self.zoom = zoom;
        self
    }

    #[inline]
    pub fn with_hidpi_enabled(mut self, hidpi_enabled: bool) -> Self {
        self.enable_hidpi = hidpi_enabled;
        self
    }

    #[inline]
    pub fn with_background_color(mut self, color: ColorU) -> Self {
        self.background_color = color;
        self
    }

    /// Since there is no anti-aliasing yet, this will enlarge the texture that is drawn to by
    /// the factor X. Default is `1.0`, but you could for example, render to a `1.2x` texture.
    #[inline]
    pub fn with_multisampling_factor(mut self, multisampling_factor: usize) -> Self {
        self.multisampling_factor = multisampling_factor;
        self
    }

    #[inline]
    pub fn with_fxaa(mut self, enable_fxaa: bool) -> Self {
        self.enable_fxaa = enable_fxaa;
        self
    }

    /// Renders the SVG to a texture. This should be called in a callback, since
    /// during DOM construction, the items don't know how large they will be.
    ///
    /// The final texture will be width * height large. Note that width and height
    /// need to be multiplied with the current `HiDPI` factor, otherwise the texture
    /// will be blurry on HiDPI screens. This isn't done automatically.
    pub fn render_svg<T>(&self, svg_cache: &SvgCache, window: &FakeWindow<T>, width: usize, height: usize) -> Texture {

        let texture_width = width;
        let texture_height = height;

        let bg_col: ColorF = self.background_color.into();

        let multisampling_factor = match self.multisampling_factor {
            0 => None,
            i if i <= 2 => Some(2),
            i if i <= 4 => Some(4),
            i if i <= 8 => Some(8),
            i if i <= 16 => Some(16),
            _ => None,
        };

        let z_index: f32 = 0.5;
        let bbox_size = TypedSize2D::new(texture_width as f32, texture_height as f32);

        let hidpi = window.get_hidpi_factor() as f32;

        let zoom = if self.enable_hidpi { self.zoom * hidpi } else { self.zoom } * self.multisampling_factor as f32;
        let pan = if self.enable_hidpi { (self.pan.0 * hidpi, self.pan.1 * hidpi) } else { self.pan };
        let pan = (pan.0 * self.multisampling_factor as f32, pan.1 * self.multisampling_factor as f32);

        let gl_context = window.get_gl_context();
        svg_cache.init_shader(gl_context.clone());

        let mut shader = svg_cache.shader.borrow_mut();
        let shader = &mut (*shader).as_mut().unwrap().program;

        let mut tex = Texture::new(gl_context.clone(), texture_width, texture_height);

        {
            let mut fb = FrameBuffer::new(&mut tex);

            fb.bind();

            gl_context.clear_color(bg_col.r, bg_col.g, bg_col.b, bg_col.a);
            gl_context.enable(gl::PRIMITIVE_RESTART_FIXED_INDEX);

            for layer in &self.layers {

                let style = match &layer {
                    SvgLayerResource::Reference((_, style)) => *style,
                    SvgLayerResource::Direct(d) => d.style,
                };

                let fill_vi = match &layer {
                    SvgLayerResource::Reference((layer_id, _)) => svg_cache.get_vertices_and_indices(gl_context.clone(), layer_id),
                    SvgLayerResource::Direct(d) => d.fill.as_ref().map(|f| {
                        let vertex_buffer = VertexBuffer::new(gl_context.clone(), &f.vertices);
                        let index_buffer = IndexBuffer::new(gl_context.clone(), &f.indices, IndexBufferFormat::TriangleStrip);
                        Rc::new((vertex_buffer, index_buffer))
                    }),
                };

                let stroke_vi = match &layer {
                    SvgLayerResource::Reference((layer_id, _)) => svg_cache.get_stroke_vertices_and_indices(gl_context.clone(), layer_id),
                    SvgLayerResource::Direct(d) => d.stroke.as_ref().map(|f| {
                        let vertex_buffer = VertexBuffer::new(gl_context.clone(), &f.vertices);
                        let index_buffer = IndexBuffer::new(gl_context.clone(), &f.indices, IndexBufferFormat::TriangleStrip);
                        Rc::new((vertex_buffer, index_buffer))
                    }),
                };

                if let (Some(fill_color), Some(fill_vi))  = (style.fill, fill_vi) {
                    let (fill_vertices, fill_indices) = &*fill_vi;
                    let uniforms = build_uniforms(&bbox_size, fill_color, z_index, pan, zoom, &style.transform);
                    shader.draw(&mut fb, fill_vertices, fill_indices, &uniforms);
                }

                if let (Some(stroke_color), Some(stroke_vi))  = (style.stroke, stroke_vi) {
                    let (stroke_vertices, stroke_indices) = &*stroke_vi;
                    let uniforms = build_uniforms(&bbox_size, stroke_color.0, z_index, pan, zoom, &style.transform);
                    shader.draw(&mut fb, stroke_vertices, stroke_indices, &uniforms);
                }
            }

            gl_context.disable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
            fb.unbind();
            fb.finish();
        }

        tex
    }
}

fn build_uniforms(
    bbox_size: &TypedSize2D<f32, SvgWorldPixel>,
    color: ColorU,
    z_index: f32,
    pan: (f32, f32),
    zoom: f32,
    layer_transform: &SvgTransform
) -> Vec<Uniform> {

    use gl::UniformType::*;

    let color: ColorF = color.into();

    let (layer_rotation_center, layer_rotation_degrees) = layer_transform.rotation.unwrap_or_default();
    let (rotation_sin, rotation_cos) = layer_rotation_degrees.to_rotation();
    let layer_translation = layer_transform.translation.unwrap_or_default();
    let layer_scale_factor = layer_transform.scale.unwrap_or_default();

    vec! [

        // Vertex shader
        Uniform::new("vBboxSize", FloatVec2([bbox_size.width / 2.0, bbox_size.height / 2.0])),
        Uniform::new("vGlobalOffset", FloatVec2([pan.0, pan.1])),
        Uniform::new("vZIndex", Float(z_index)),
        Uniform::new("vZoom", Float(zoom)),
        Uniform::new("vRotationCenter", FloatVec2([layer_rotation_center.x, layer_rotation_center.y])),
        Uniform::new("vRotationSin", Float(rotation_sin)),
        Uniform::new("vRotationCos", Float(rotation_cos)),
        Uniform::new("vScaleFactor", FloatVec2([layer_scale_factor.x, layer_scale_factor.y])),
        Uniform::new("vTranslatePx", FloatVec2([layer_translation.x, layer_translation.y])),

        // Fragment shader
        Uniform::new("fFillColor", FloatVec4([color.r, color.g, color.b, color.a])),
    ]
}

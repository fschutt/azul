use std::{
    fmt,
    rc::Rc,
    sync::{Arc, Mutex, atomic::{Ordering, AtomicUsize}},
    cell::{RefCell, RefMut},
    hash::{Hash, Hasher},
    collections::hash_map::Entry::*,
};
#[cfg(feature = "svg_parsing")]
use std::io::{Error as IoError};
use glium::{
    backend::Facade, index::PrimitiveType,
    DrawParameters, IndexBuffer, VertexBuffer,
    Program, Api, Surface,
};
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
        default::{Builder},
        builder::{PathBuilder, FlatPathBuilder},
    },
    geom::euclid::{TypedRect, TypedPoint2D, TypedSize2D, TypedVector2D},
};
#[cfg(feature = "svg_parsing")]
use usvg::{Error as SvgError, ViewBox, Transform};
use webrender::api::{ColorU, ColorF, GlyphInstance};
use rusttype::{Font, Glyph};
use {
    FastHashMap,
    dom::{Callback, Texture},
    traits::Layout,
    window::ReadOnlyWindow,
    css_parser::{FontId, StyleFontSize},
    app_resources::AppResources,
    text_layout::{FontMetrics, LayoutTextResult, TextLayoutOptions, layout_text},
};

pub use lyon::tessellation::VertexBuffers;
pub use rusttype::GlyphId;
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

    in vec2 xy;
    in vec2 normal;

    uniform vec2 bbox_origin;
    uniform vec2 bbox_size;
    uniform vec2 offset;
    uniform float z_index;
    uniform float zoom;

    void main() {
        vec2 position_centered = (xy - bbox_origin) / bbox_size;
        vec2 position_zoomed = position_centered * vec2(zoom);
        gl_Position = vec4(vec2(-1.0) + position_zoomed + (offset / bbox_size), z_index, 1.0);
    }";

fn prefix_gl_version(shader: &str, gl: Api) -> String {
    match gl {
        Api::Gl => format!("{}\n{}", SHADER_VERSION_GL, shader),
        Api::GlEs => format!("{}\n{}", SHADER_VERSION_GLES, shader),
    }
}

const SVG_FRAGMENT_SHADER: &str = "

    precision highp float;

    #define attribute in
    #define varying out

    uniform vec4 color;
    out vec4 out_color;

    void main() {
        out_color = color;
    }
";

// inputs:
//
// - `resolution`
// - `position`
// - `uv`
// - `source`
const SVG_FXAA_VERTEX_SHADER: &str = "

    precision mediump float;

    out vec2 v_rgbNW;
    out vec2 v_rgbNE;
    out vec2 v_rgbSW;
    out vec2 v_rgbSE;
    out vec2 v_rgbM;

    uniform vec2 resolution;
    uniform vec2 position;
    uniform vec2 uv;

    void texcoords(vec2 fragCoord, vec2 resolution,
                out vec2 v_rgbNW, out vec2 v_rgbNE,
                out vec2 v_rgbSW, out vec2 v_rgbSE,
                out vec2 v_rgbM) {
        vec2 inverseVP = 1.0 / resolution.xy;
        v_rgbNW = (fragCoord + vec2(-1.0, -1.0)) * inverseVP;
        v_rgbNE = (fragCoord + vec2(1.0, -1.0)) * inverseVP;
        v_rgbSW = (fragCoord + vec2(-1.0, 1.0)) * inverseVP;
        v_rgbSE = (fragCoord + vec2(1.0, 1.0)) * inverseVP;
        v_rgbM = vec2(fragCoord * inverseVP);
    }

    void main() {
        gl_Position = vec4(position, 1.0, 1.0);
        uv = (position + 1.0) * 0.5;
        uv.y = 1.0 - uv.y;
        vec2 frag_coord = uv * resolution;
        texcoords(frag_coord, resolution, v_rgbNW, v_rgbNE, v_rgbSW, v_rgbSE, v_rgbM);
    }
";

// Optimized version for mobile, where dependent texture reads can be a bottleneck
//
// Taken from: https://github.com/mattdesl/glsl-fxaa/blob/master/fxaa.glsl
//
// Basic FXAA implementation based on the code on geeks3d.com with the
// modification that the texture2DLod stuff was removed since it's
// unsupported by WebGL.
// --
//
// From:
//
// https://github.com/mitsuhiko/webgl-meincraft
//
// Copyright (c) 2011 by Armin Ronacher.
//
// Some rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
//     * Redistributions of source code must retain the above copyright
//       notice, this list of conditions and the following disclaimer.
//     * Redistributions in binary form must reproduce the above
//       copyright notice, this list of conditions and the following
//       disclaimer in the documentation and/or other materials provided
//       with the distribution.
//     * The names of the contributors may not be used to endorse or
//       promote products derived from this software without specific
//       prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
const SVG_FXAA_FRAG_SHADER: &str = "

    #define FXAA_REDUCE_MIN   (1.0/ 128.0)
    #define FXAA_REDUCE_MUL   (1.0 / 8.0)
    #define FXAA_SPAN_MAX     8.0

    precision mediump float;

    in vec2 v_rgbNW;
    in vec2 v_rgbNE;
    in vec2 v_rgbSW;
    in vec2 v_rgbSE;
    in vec2 v_rgbM;

    uniform vec2 resolution;
    uniform sampler2D source;

    vec4 fxaa(sampler2D tex, vec2 fragCoord, vec2 resolution,
                vec2 v_rgbNW, vec2 v_rgbNE,
                vec2 v_rgbSW, vec2 v_rgbSE,
                vec2 v_rgbM) {
        vec4 color;
        mediump vec2 inverseVP = vec2(1.0 / resolution.x, 1.0 / resolution.y);
        vec3 rgbNW = texture2D(tex, v_rgbNW).xyz;
        vec3 rgbNE = texture2D(tex, v_rgbNE).xyz;
        vec3 rgbSW = texture2D(tex, v_rgbSW).xyz;
        vec3 rgbSE = texture2D(tex, v_rgbSE).xyz;
        vec4 texColor = texture2D(tex, v_rgbM);
        vec3 rgbM  = texColor.xyz;
        vec3 luma = vec3(0.299, 0.587, 0.114);
        float lumaNW = dot(rgbNW, luma);
        float lumaNE = dot(rgbNE, luma);
        float lumaSW = dot(rgbSW, luma);
        float lumaSE = dot(rgbSE, luma);
        float lumaM  = dot(rgbM,  luma);
        float lumaMin = min(lumaM, min(min(lumaNW, lumaNE), min(lumaSW, lumaSE)));
        float lumaMax = max(lumaM, max(max(lumaNW, lumaNE), max(lumaSW, lumaSE)));

        mediump vec2 dir;
        dir.x = -((lumaNW + lumaNE) - (lumaSW + lumaSE));
        dir.y =  ((lumaNW + lumaSW) - (lumaNE + lumaSE));

        float dirReduce = max((lumaNW + lumaNE + lumaSW + lumaSE) *
                              (0.25 * FXAA_REDUCE_MUL), FXAA_REDUCE_MIN);

        float rcpDirMin = 1.0 / (min(abs(dir.x), abs(dir.y)) + dirReduce);
        dir = min(vec2(FXAA_SPAN_MAX, FXAA_SPAN_MAX),
                  max(vec2(-FXAA_SPAN_MAX, -FXAA_SPAN_MAX),
                  dir * rcpDirMin)) * inverseVP;

        vec3 rgbA = 0.5 * (
            texture2D(tex, fragCoord * inverseVP + dir * (1.0 / 3.0 - 0.5)).xyz +
            texture2D(tex, fragCoord * inverseVP + dir * (2.0 / 3.0 - 0.5)).xyz);
        vec3 rgbB = rgbA * 0.5 + 0.25 * (
            texture2D(tex, fragCoord * inverseVP + dir * -0.5).xyz +
            texture2D(tex, fragCoord * inverseVP + dir * 0.5).xyz);

        float lumaB = dot(rgbB, luma);
        if ((lumaB < lumaMin) || (lumaB > lumaMax))
            color = vec4(rgbA, texColor.a);
        else
            color = vec4(rgbB, texColor.a);
        return color;
    }

    void main() {
      gl_FragColor = fxaa(source, gl_FragCoord.xy, resolution, v_rgbNW, v_rgbNE, v_rgbSW, v_rgbSE, v_rgbM);
    }
";

#[derive(Debug, Clone)]
pub struct SvgShader {
    pub program: Rc<Program>,
}

impl SvgShader {
    pub fn new<F: Facade + ?Sized>(display: &F) -> Self {
        let current_gl_api = display.get_context().get_opengl_version().0;
        let vertex_source_prefixed = prefix_gl_version(SVG_VERTEX_SHADER, current_gl_api);
        let fragment_source_prefixed = prefix_gl_version(SVG_FRAGMENT_SHADER, current_gl_api);

        Self {
            program: Rc::new(Program::from_source(display, &vertex_source_prefixed, &fragment_source_prefixed, None).unwrap()),
        }
    }
}

pub struct SvgCache<T: Layout> {
    // note: one "layer" merely describes one or more polygons that have the same style
    layers: FastHashMap<SvgLayerId, SvgLayer<T>>,
    // Stores the vertices and indices necessary for drawing. Must be synchronized with the `layers`
    gpu_ready_to_upload_cache: FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    stroke_gpu_ready_to_upload_cache: FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    vertex_index_buffer_cache: RefCell<FastHashMap<SvgLayerId, Rc<(VertexBuffer<SvgVert>, IndexBuffer<u32>)>>>,
    stroke_vertex_index_buffer_cache: RefCell<FastHashMap<SvgLayerId, Rc<(VertexBuffer<SvgVert>, IndexBuffer<u32>)>>>,
    shader: Mutex<Option<SvgShader>>,
    // Stores the 2D transforms of the shapes on the screen. The vertices are
    // offset by the X, Y value in the transforms struct. This should be expanded
    // to full matrices later on, so you can do full 3D transformations
    // on 2D shapes later on. For now, each transform is just an X, Y offset
    #[cfg(feature = "svg_parsing")]
    transforms: FastHashMap<SvgTransformId, Transform>,
    #[cfg(feature = "svg_parsing")]
    view_boxes: FastHashMap<SvgViewBoxId, ViewBox>,
}

impl<T: Layout> Default for SvgCache<T> {
    fn default() -> Self {
        Self {
            layers: FastHashMap::default(),
            gpu_ready_to_upload_cache: FastHashMap::default(),
            stroke_gpu_ready_to_upload_cache: FastHashMap::default(),
            vertex_index_buffer_cache: RefCell::new(FastHashMap::default()),
            stroke_vertex_index_buffer_cache: RefCell::new(FastHashMap::default()),
            shader: Mutex::new(None),
            #[cfg(feature = "svg_parsing")]
            transforms: FastHashMap::default(),
            #[cfg(feature = "svg_parsing")]
            view_boxes: FastHashMap::default(),
        }
    }
}

fn fill_vertex_buffer_cache<'a, F: Facade>(
    id: &SvgLayerId,
    mut rmut: RefMut<'a, FastHashMap<SvgLayerId, Rc<(VertexBuffer<SvgVert>, IndexBuffer<u32>)>>>,
    rnotmut: &FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    window: &F)
{
    use std::collections::hash_map::Entry::*;

    match rmut.entry(*id) {
        Occupied(_) => { },
        Vacant(v) => {
            let (vbuf, ibuf) = match rnotmut.get(id).as_ref() {
                Some(s) => s,
                None => return,
            };
            let vertex_buffer = VertexBuffer::new(window, vbuf).unwrap();
            let index_buffer = IndexBuffer::new(window, PrimitiveType::TrianglesList, ibuf).unwrap();
            v.insert(Rc::new((vertex_buffer, index_buffer)));
        }
    }
}

impl<T: Layout> SvgCache<T> {

    /// Creates an empty SVG cache
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds and compiles the SVG shader if the shader isn't already present
    fn init_shader<F: Facade + ?Sized>(&self, display: &F) -> SvgShader {
        let mut shader_lock = self.shader.lock().unwrap();
        if shader_lock.is_none() {
            *shader_lock = Some(SvgShader::new(display));
        }
        shader_lock.as_ref().and_then(|s| Some(s.clone())).unwrap()
    }

    fn get_stroke_vertices_and_indices<'a, F: Facade>(&'a self, window: &F, id: &SvgLayerId)
    -> Option<Rc<(VertexBuffer<SvgVert>, IndexBuffer<u32>)>>
    {

        {
            let rmut = self.stroke_vertex_index_buffer_cache.borrow_mut();
            let rnotmut = &self.stroke_gpu_ready_to_upload_cache;
            fill_vertex_buffer_cache(id, rmut, rnotmut, window);
        }

        self.stroke_vertex_index_buffer_cache.borrow().get(id).and_then(|x| Some(x.clone()))
    }

    /// Note: panics if the ID isn't found.
    ///
    /// Since we are required to keep the `self.layers` and the `self.gpu_buffer_cache`
    /// in sync, a panic should never happen
    fn get_vertices_and_indices<'a, F: Facade>(&'a self, window: &F, id: &SvgLayerId)
    -> Option<Rc<(VertexBuffer<SvgVert>, IndexBuffer<u32>)>>
    {
        // We need the SvgCache to call this function immutably, otherwise we can't
        // use it from the Layout::layout() function
        {
            let rmut = self.vertex_index_buffer_cache.borrow_mut();
            let rnotmut = &self.gpu_ready_to_upload_cache;

            fill_vertex_buffer_cache(id, rmut, rnotmut, window);
        }

        self.vertex_index_buffer_cache.borrow().get(id).and_then(|x| Some(x.clone()))
    }

    fn get_style(&self, id: &SvgLayerId)
    -> SvgStyle
    {
        self.layers.get(id).as_ref().unwrap().style
    }

    pub fn add_layer(&mut self, layer: SvgLayer<T>) -> SvgLayerId {
        // TODO: set tolerance based on zoom
        let new_svg_id = new_svg_layer_id();

        let ((vertex_buf, index_buf), opt_stroke) =
            tesselate_layer_data(&layer.data, DEFAULT_GLYPH_TOLERANCE, layer.style.stroke.and_then(|s| Some(s.1.clone())));

        self.gpu_ready_to_upload_cache.insert(new_svg_id, (vertex_buf, index_buf));

        if let Some((stroke_vertex_buf, stroke_index_buf)) = opt_stroke {
            self.stroke_gpu_ready_to_upload_cache.insert(new_svg_id, (stroke_vertex_buf, stroke_index_buf));
        }

        self.layers.insert(new_svg_id, layer);

        new_svg_id
    }

    pub fn delete_layer(&mut self, svg_id: SvgLayerId) {
        self.layers.remove(&svg_id);
        self.gpu_ready_to_upload_cache.remove(&svg_id);
        self.stroke_gpu_ready_to_upload_cache.remove(&svg_id);
        let rmut = self.vertex_index_buffer_cache.get_mut();
        let stroke_rmut = self.stroke_vertex_index_buffer_cache.get_mut();
        rmut.remove(&svg_id);
        stroke_rmut.remove(&svg_id);
    }

    pub fn clear_all_layers(&mut self) {
        self.layers.clear();

        self.gpu_ready_to_upload_cache.clear();
        self.stroke_gpu_ready_to_upload_cache.clear();

        let rmut = self.vertex_index_buffer_cache.get_mut();
        rmut.clear();

        let stroke_rmut = self.stroke_vertex_index_buffer_cache.get_mut();
        stroke_rmut.clear();
    }

    #[cfg(feature = "svg_parsing")]
    pub fn add_transforms(&mut self, transforms: FastHashMap<SvgTransformId, Transform>) {
        transforms.into_iter().for_each(|(k, v)| {
            self.transforms.insert(k, v);
        });
    }

    /// Parses an input source, parses the SVG, adds the shapes as layers into
    /// the registry, returns the IDs of the added shapes, in the order that they appeared in the Svg
    #[cfg(feature = "svg_parsing")]
    pub fn add_svg<S: AsRef<str>>(&mut self, input: S) -> Result<Vec<SvgLayerId>, SvgParseError> {
        let (layers, transforms) = self::svg_to_lyon::parse_from(input, &mut self.view_boxes)?;
        self.add_transforms(transforms);
        Ok(layers
            .into_iter()
            .map(|layer| self.add_layer(layer))
            .collect())
    }
}

impl<T: Layout> fmt::Debug for SvgCache<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for layer in self.layers.keys() {
            write!(f, "{:?}", layer)?;
        }
        Ok(())
    }
}

const GL_RESTART_INDEX: u32 = ::std::u32::MAX;

fn tesselate_layer_data(layer_data: &LayerType, tolerance: f32, stroke_options: Option<SvgStrokeOptions>)
-> ((Vec<SvgVert>, Vec<u32>), Option<(Vec<SvgVert>, Vec<u32>)>)
{
    let mut last_index = 0;
    let mut vertex_buf = Vec::<SvgVert>::new();
    let mut index_buf = Vec::<u32>::new();

    let mut last_stroke_index = 0;
    let mut stroke_vertex_buf = Vec::<SvgVert>::new();
    let mut stroke_index_buf = Vec::<u32>::new();

    for layer in layer_data.get() {

        let (VertexBuffers { vertices, indices }, stroke_vertices) = layer.tesselate(tolerance, stroke_options);

        let vertices_len = vertices.len();
        vertex_buf.extend(vertices.into_iter());
        index_buf.extend(indices.into_iter().map(|i| i as u32 + last_index as u32));
        index_buf.push(GL_RESTART_INDEX);
        last_index += vertices_len;

        if let Some(VertexBuffers { vertices, indices }) = stroke_vertices {
            let stroke_vertices_len = vertices.len();
            stroke_vertex_buf.extend(vertices.into_iter());
            stroke_index_buf.extend(indices.into_iter().map(|i| i as u32 + last_stroke_index as u32));
            stroke_index_buf.push(GL_RESTART_INDEX);
            last_stroke_index += stroke_vertices_len;
        }
    }

    if stroke_options.is_some() {
        ((vertex_buf, index_buf), Some((stroke_vertex_buf, stroke_index_buf)))
    } else {
        ((vertex_buf, index_buf), None)
    }
}

/// Quick helper function to generate the vertices for a black circle at runtime
pub fn quick_circle(circle: SvgCircle, fill_color: ColorU) -> SvgLayerResource {
    let (fill, _) = tesselate_layer_data(&LayerType::from_single_layer(SvgLayerType::Circle(circle)), 0.01, None);
    let style = SvgStyle::filled(fill_color);
    SvgLayerResource::Direct {
        style: style,
        fill: Some(VerticesIndicesBuffer { vertices: fill.0, indices: fill.1 }),
        stroke: None,
    }
}

/// Quick helper function to generate the layer for **multiple** circles (in one draw call)
pub fn quick_circles(circles: &[SvgCircle], fill_color: ColorU) -> SvgLayerResource {
    let circles = circles.iter().map(|c| SvgLayerType::Circle(*c)).collect();
    let (fill, _) = tesselate_layer_data(&LayerType::from_polygons(circles), 0.01, None);
    let style = SvgStyle::filled(fill_color);
    SvgLayerResource::Direct {
        style: style,
        fill: Some(VerticesIndicesBuffer { vertices: fill.0, indices: fill.1 }),
        stroke: None,
    }
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
-> SvgLayerResource
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
        }).collect();

    let (_, stroke) = tesselate_layer_data(&LayerType::from_polygons(polygons), 0.01, Some(stroke_options));

    // Safe unwrap, since we passed Some(stroke_options) into tesselate_layer_data
    let stroke = stroke.unwrap();

    SvgLayerResource::Direct {
        style: style,
        fill: None,
        stroke: Some(VerticesIndicesBuffer { vertices: stroke.0, indices: stroke.1 }),
    }
}

pub fn quick_rects(rects: &[SvgRect], stroke_color: Option<ColorU>, fill_color: Option<ColorU>, stroke_options: Option<SvgStrokeOptions>)
-> SvgLayerResource
{
    let style = SvgStyle {
        stroke: stroke_color.and_then(|col| Some((col, stroke_options.unwrap_or_default()))),
        fill: fill_color,
    };

    let rects = rects.iter().map(|r| SvgLayerType::Rect(*r)).collect();
    let (fill, stroke) = tesselate_layer_data(&LayerType::from_polygons(rects), 0.01, style.stroke.and_then(|(_, options)| Some(options)));

    SvgLayerResource::Direct {
        style: style,
        fill: fill_color.and_then(|_| Some(VerticesIndicesBuffer { vertices: fill.0, indices: fill.1 })),
        stroke: stroke.and_then(|stroke_vertices| Some(VerticesIndicesBuffer { vertices: stroke_vertices.0, indices: stroke_vertices.1 })),
    }
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
            sampled_bezier_points[i] = cubic_interpolate_bezier(curve, i as f32 / BEZIER_SAMPLE_RATE as f32);
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
        let mut glyph_offsets = vec![];
        let mut glyph_rotations = vec![];

        // NOTE: g.point.x is the offset from the start, not the advance!
        let mut current_offset = start_offset + glyphs.get(0).and_then(|g| Some(g.point.x)).unwrap_or(0.0);
        let mut last_offset = start_offset;

        for glyph_idx in 0..glyphs.len() {
            let char_bezier_percentage = self.get_bezier_percentage_from_offset(current_offset);
            let char_bezier_pt = cubic_interpolate_bezier(&self.original_curve, char_bezier_percentage);
            glyph_offsets.push((char_bezier_pt.x, char_bezier_pt.y));

            let char_rotation_percentage = self.get_bezier_percentage_from_offset(last_offset);
            let rotation = cubic_bezier_normal(&self.original_curve, char_rotation_percentage).to_rotation();
            glyph_rotations.push(rotation);

            last_offset = current_offset;
            current_offset = start_offset + glyphs.get(glyph_idx + 1).and_then(|g| Some(g.point.x)).unwrap_or(0.0);
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
    pub fn draw_circles(&self, color: ColorU) -> SvgLayerResource {
        quick_circles(
            &self.sampled_bezier_points
            .iter()
            .map(|c| SvgCircle { center_x: c.x, center_y: c.y, radius: 1.0 })
            .collect::<Vec<_>>(),
            color)
    }

    /// Returns the geometry necessary to draw the control handles of this curve
    pub fn draw_control_handles(&self, color: ColorU) -> SvgLayerResource {
        quick_circles(
            &self.original_curve
            .iter()
            .map(|c| SvgCircle { center_x: c.x, center_y: c.y, radius: 3.0 })
            .collect::<Vec<_>>(),
            color)
    }

    /// Returns the geometry necessary to draw the bezier curve (the actual line)
    pub fn draw_lines(&self, stroke_color: ColorU) -> SvgLayerResource {
        let line = [self.sampled_bezier_points.iter().map(|b| (b.x, b.y)).collect()];
        quick_lines(&line, stroke_color, None)
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

pub fn scale_vertex_buffer(input: &mut [SvgVert], scale: &StyleFontSize, height_for_1px: f32) {
    let real_size = scale.to_pixels();
    let scale_factor = real_size * height_for_1px;
    for vert in input {
        vert.xy.0 *= scale_factor;
        vert.xy.1 *= scale_factor;
    }
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

pub struct SvgLayer<T: Layout> {
    pub data: LayerType,
    pub callbacks: SvgCallbacks<T>,
    pub style: SvgStyle,
    pub transform_id: Option<SvgTransformId>,
    // TODO: This is currently not used
    pub view_box_id: SvgViewBoxId,
}

impl<T: Layout> SvgLayer<T> {
    /// Shorthand for creating a SvgLayer from some data and style
    pub fn default_from_layer(data: LayerType, style: SvgStyle) -> Self {
        SvgLayer {
            data,
            callbacks: SvgCallbacks::None,
            style,
            transform_id: None,
            view_box_id: new_view_box_id(),
        }
    }
}

impl<T: Layout> fmt::Debug for SvgLayer<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SvgLayer {{ data: {:?}, callbacks: {:?}, style: {:?}, transform_id: {:?}, view_box_id: {:?} }}",
           self.data,
           self.callbacks,
           self.style,
           self.transform_id,
           self.view_box_id)
    }
}

#[derive(Debug, Clone)]
pub enum LayerType {
    KnownSize([SvgLayerType; 1]),
    UnknownSize(Vec<SvgLayerType>),
}

impl LayerType {
    pub fn get(&self) -> &[SvgLayerType] {
        use self::LayerType::*;
        match self {
            KnownSize(a) => &a[..],
            UnknownSize(b) => &b[..],
        }
    }

    pub fn from_polygons(data: Vec<SvgLayerType>) -> Self {
        LayerType::UnknownSize(data)
    }

    pub fn from_single_layer(data: SvgLayerType) -> Self {
        LayerType::KnownSize([data])
    }
}

impl<T: Layout> Clone for SvgLayer<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            callbacks: self.callbacks.clone(),
            style: self.style.clone(),
            transform_id: self.transform_id,
            view_box_id: self.view_box_id,
        }
    }
}

pub enum SvgCallbacks<T: Layout> {
    // No callbacks for this layer
    None,
    /// Call the callback on any of the items
    Any(Callback<T>),
    /// Call the callback when the SvgLayer item at index [x] is
    ///  hovered over / interacted with
    Some(Vec<(usize, Callback<T>)>),
}

impl<T: Layout> fmt::Debug for SvgCallbacks<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::SvgCallbacks::*;
        match self {
            None => write!(f, "SvgCallbacks::None"),
            Any(a) => write!(f, "SvgCallbacks::Any({:?})", a),
            Some(v) => {
                let mut s = String::new();
                for i in v.iter() {
                    s += &format!("{:?}", i);
                }
                write!(f, "SvgCallbacks::Some({})", s)
            },
        }
    }
}

impl<T: Layout> Clone for SvgCallbacks<T> {
    fn clone(&self) -> Self {
        use self::SvgCallbacks::*;
        match self {
            None => None,
            Any(c) => Any(c.clone()),
            Some(v) => Some(v.clone()),
        }
    }
}

impl<T: Layout> Hash for SvgCallbacks<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use self::SvgCallbacks::*;
        match self {
            None => 0.hash(state),
            Any(c) => { Any(*c).hash(state); },
            Some(ref v) => {
                2.hash(state);
                for (id, callback) in v {
                    id.hash(state);
                    callback.hash(state);
                }
            },
        }
    }
}

impl<T: Layout> PartialEq for SvgCallbacks<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self == rhs
    }
}

impl<T: Layout> Eq for SvgCallbacks<T> { }

#[derive(Debug, Default, Copy, Clone, PartialEq, Hash)]
pub struct SvgStyle {
    /// Stroke color
    pub stroke: Option<(ColorU, SvgStrokeOptions)>,
    /// Fill color
    pub fill: Option<ColorU>,
    // TODO: stroke-dasharray
}

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
    pub line_width: usize,

    /// See the SVG specification.
    ///
    /// Must be greater than or equal to 1.0.
    /// Default value: `StrokeOptions::DEFAULT_MITER_LIMIT`.
    pub miter_limit: usize,

    /// Maximum allowed distance to the path when building an approximation.
    ///
    /// See [Flattening and tolerance](index.html#flattening-and-tolerance).
    /// Default value: `StrokeOptions::DEFAULT_TOLERANCE`.
    pub tolerance: usize,

    /// Apply line width
    ///
    /// When set to false, the generated vertices will all be positioned in the centre
    /// of the line. The width can be applied later on (eg in a vertex shader) by adding
    /// the vertex normal multiplied by the line with to each vertex position.
    ///
    /// Default value: `true`.
    pub apply_line_width: bool,
}

impl Into<StrokeOptions> for SvgStrokeOptions {
    fn into(self) -> StrokeOptions {
        let target = StrokeOptions::default()
            .with_tolerance(self.tolerance as f32 / 1000.0)
            .with_start_cap(self.start_cap.into())
            .with_end_cap(self.end_cap.into())
            .with_line_join(self.line_join.into())
            .with_line_width(self.line_width as f32 / 1000.0)
            .with_miter_limit(self.miter_limit as f32 / 1000.0);

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
            line_width: (DEFAULT_LINE_WIDTH * 1000.0) as usize,
            miter_limit: (DEFAULT_MITER_LIMIT * 1000.0) as usize,
            tolerance: (DEFAULT_TOLERANCE * 1000.0) as usize,
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

implement_vertex!(SvgVert, xy, normal);

#[derive(Debug, Copy, Clone)]
pub struct SvgWorldPixel;

/// A vectorized font holds the glyphs for a given font, but in a vector format
#[derive(Debug, Clone)]
pub struct VectorizedFont {
    /// Glyph -> Polygon map
    glyph_polygon_map: Arc<Mutex<FastHashMap<GlyphId, VertexBuffers<SvgVert, u32>>>>,
    /// Glyph -> Stroke map
    glyph_stroke_map: Arc<Mutex<FastHashMap<GlyphId, VertexBuffers<SvgVert, u32>>>>,
}

impl VectorizedFont {
    pub fn from_font(font: &Font) -> Self {

        let mut glyph_polygon_map = FastHashMap::default();
        let mut glyph_stroke_map = FastHashMap::default();

        let stroke_options = SvgStrokeOptions::default();

        // TODO: In a regular font (4000 characters), this is pretty slow!
        // Pre-load the "A..Z | a..z" characters
        for g in (65..122).filter_map(|i| {
            let g = font.glyph(GlyphId(i));
            if g.id() == GlyphId(0) {
                None
            } else {
                Some(g)
            }
        }) {
            // Tesselate all the font vertices and store them in the glyph map
            let glyph_id = g.id();
            if let Some((polygon_verts, stroke_verts)) =
                glyph_to_svg_layer_type(g)
                .and_then(|poly| Some(poly.tesselate(DEFAULT_GLYPH_TOLERANCE, Some(stroke_options))))
            {
                // safe unwrap, since we set the stroke_options to Some()
                glyph_polygon_map.insert(glyph_id, polygon_verts);
                glyph_stroke_map.insert(glyph_id, stroke_verts.unwrap());
            }
        }

        if let Some((polygon_verts_zero, stroke_verts_zero)) =
            glyph_to_svg_layer_type(font.glyph(GlyphId(0)))
            .and_then(|poly| Some(poly.tesselate(DEFAULT_GLYPH_TOLERANCE, Some(stroke_options))))
        {
            glyph_polygon_map.insert(GlyphId(0), polygon_verts_zero);
            glyph_stroke_map.insert(GlyphId(0), stroke_verts_zero.unwrap());
        }

        Self {
            glyph_polygon_map: Arc::new(Mutex::new(FastHashMap::default())),
            glyph_stroke_map: Arc::new(Mutex::new(FastHashMap::default())),
        }
    }

    /// Loads a vectorized font from a path
    pub fn from_path(path: &str) -> Option<Self> {
        use std::fs;
        use font::rusttype_load_font;

        let file_contents = fs::read(path).ok()?;
        let font = rusttype_load_font(file_contents, None).ok()?.0;
        Some(Self::from_font(&font))
    }
}

/// Note: Since `VectorizedFont` has to lock access on this, you'll want to get the
/// fill vertices for all the characters at once
pub fn get_fill_vertices(vectorized_font: &VectorizedFont, original_font: &Font, ids: &[GlyphInstance])
-> Vec<VertexBuffers<SvgVert, u32>>
{
    let svg_stroke_opts = Some(SvgStrokeOptions::default());

    let mut glyph_stroke_lock = vectorized_font.glyph_stroke_map.lock().unwrap();
    let mut glyph_polygon_lock = vectorized_font.glyph_polygon_map.lock().unwrap();

    ids.iter().filter_map(|id| {
        let id = GlyphId(id.index);
        match glyph_polygon_lock.entry(id) {
            Occupied(o) => Some(o.get().clone()),
            Vacant(v) => {
                let g = original_font.glyph(id);
                let poly = glyph_to_svg_layer_type(g)?;
                let (polygon_verts, stroke_verts) = poly.tesselate(DEFAULT_GLYPH_TOLERANCE, svg_stroke_opts);
                v.insert(polygon_verts.clone());
                glyph_stroke_lock.insert(id, stroke_verts.unwrap());
                Some(polygon_verts)
            }
        }
    }).collect()
}

/// Note: Since `VectorizedFont` has to lock access on this, you'll want to get the
/// stroke vertices for all the characters at once
pub fn get_stroke_vertices(vectorized_font: &VectorizedFont, original_font: &Font, ids: &[GlyphInstance])
-> Vec<VertexBuffers<SvgVert, u32>>
{
    let svg_stroke_opts = Some(SvgStrokeOptions::default());

    let mut glyph_stroke_lock = vectorized_font.glyph_stroke_map.lock().unwrap();
    let mut glyph_polygon_lock = vectorized_font.glyph_polygon_map.lock().unwrap();

    ids.iter().filter_map(|id| {
        let id = GlyphId(id.index);
        match glyph_stroke_lock.entry(id) {
            Occupied(o) => Some(o.get().clone()),
            Vacant(v) => {
                let g = original_font.glyph(id);
                let poly = glyph_to_svg_layer_type(g)?;
                let (polygon_verts, stroke_verts) = poly.tesselate(DEFAULT_GLYPH_TOLERANCE, svg_stroke_opts);
                let stroke_verts = stroke_verts.unwrap();
                v.insert(stroke_verts.clone());
                glyph_polygon_lock.insert(id, polygon_verts);
                Some(stroke_verts)
            }
        }
    }).collect()
}

/// Converts a glyph to a `SvgLayerType::Polygon`
fn glyph_to_svg_layer_type<'a>(glyph: Glyph<'a>) -> Option<SvgLayerType> {
    Some(SvgLayerType::Polygon(glyph
        .standalone()
        .get_data()?.shape
        .as_ref()?
        .iter()
        .map(rusttype_glyph_to_path_events)
        .collect()))
}

use stb_truetype::Vertex;

// Convert a Rusttype glyph to a Vec of PathEvents,
// in order to turn a glyph into a polygon
fn rusttype_glyph_to_path_events(vertex: &Vertex)
-> PathEvent
{   use stb_truetype::VertexType;
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

#[derive(Debug)]
pub struct VectorizedFontCache {
    /// Font -> Vectorized glyph map
    ///
    /// Needs to be wrapped in a RefCell / Rc since we want to lazy-load the
    /// fonts to keep the memory usage down
    vectorized_fonts: Mutex<FastHashMap<FontId, Arc<VectorizedFont>>>,
}

#[test]
fn test_vectorized_font_cache_is_send() {
    fn is_send<T: Send>() {}
    is_send::<VectorizedFontCache>();
}

impl VectorizedFontCache {

    pub fn new() -> Self {
        Self {
            vectorized_fonts: Mutex::new(FastHashMap::default()),
        }
    }

    pub fn insert_if_not_exist(&mut self, id: FontId, font: &Font) {
        self.vectorized_fonts.lock().unwrap().entry(id).or_insert_with(|| Arc::new(VectorizedFont::from_font(font)));
    }

    pub fn insert(&mut self, id: FontId, font: VectorizedFont) {
        self.vectorized_fonts.lock().unwrap().insert(id, Arc::new(font));
    }

    /// Returns true if the font cache has the respective font
    pub fn has_font(&self, id: &FontId) -> bool {
        self.vectorized_fonts.lock().unwrap().get(id).is_some()
    }

    pub fn get_font(&self, id: &FontId, app_resources: &AppResources) -> Option<Arc<VectorizedFont>> {
        self.vectorized_fonts.lock().unwrap().entry(id.clone())
            .or_insert_with(|| Arc::new(VectorizedFont::from_font(&*app_resources.get_font(&id).unwrap().0)));
        self.vectorized_fonts.lock().unwrap().get(&id).and_then(|font| Some(font.clone()))
    }

    pub fn remove_font(&mut self, id: &FontId) {
        self.vectorized_fonts.lock().unwrap().remove(id);
    }
}

impl SvgLayerType {
    pub fn tesselate(&self, tolerance: f32, stroke: Option<SvgStrokeOptions>)
    -> (VertexBuffers<SvgVert, u32>, Option<VertexBuffers<SvgVert, u32>>)
    {
        let mut geometry = VertexBuffers::new();
        let mut stroke_geometry = VertexBuffers::new();
        let stroke = stroke.and_then(|s| {
            let s: StrokeOptions = s.into();
            Some(s.with_tolerance(tolerance))
        });

        match self {
            SvgLayerType::Polygon(p) => {
                let mut builder = Builder::with_capacity(p.len()).flattened(tolerance);
                for event in p {
                    builder.path_event(*event);
                }
                let path = builder.with_svg().build();

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

                if let Some(ref stroke_options) = stroke {
                    let mut stroke_tess = StrokeTessellator::new();
                    stroke_tess.tessellate_path(
                        path.path_iter(),
                        stroke_options,
                        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                            SvgVert {
                                xy: (vertex.position.x, vertex.position.y),
                                normal: (vertex.normal.x, vertex.position.y),
                            }
                        }),
                    );
                }
            },
            SvgLayerType::Circle(c) => {
                let center = TypedPoint2D::new(c.center_x, c.center_y);
                let radius = c.radius;
                fill_circle(center, radius, &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));

                if let Some(ref stroke_options) = stroke {
                    stroke_circle(center, radius, stroke_options,
                        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                            SvgVert {
                                xy: (vertex.position.x, vertex.position.y),
                                normal: (vertex.normal.x, vertex.position.y),
                            }
                        }
                    ));
                }
            },
            SvgLayerType::Rect(r) => {
                let size = TypedSize2D::new(r.width, r.height);
                let rect = TypedRect::new(TypedPoint2D::new(r.x, r.y), size);
                let radii = BorderRadii {
                    top_left: r.rx,
                    top_right: r.rx,
                    bottom_left: r.rx,
                    bottom_right: r.rx,
                };

                fill_rounded_rectangle(&rect, &radii, &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));

                if let Some(ref stroke_options) = stroke {
                    stroke_rounded_rectangle(&rect, &radii, stroke_options,
                        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                            SvgVert {
                                xy: (vertex.position.x, vertex.position.y),
                                normal: (vertex.normal.x, vertex.position.y),
                            }
                        }
                    ));
                }
            }
        }

        if stroke.is_some() {
            (geometry, Some(stroke_geometry))
        } else {
            (geometry, None)
        }
    }
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
    use usvg::{ViewBox, Transform, Tree, PathSegment,
        Color, Options, Paint, Stroke, LineCap, LineJoin, NodeKind};
    use widgets::svg::{SvgLayer, SvgStrokeOptions, SvgLineCap, SvgLineJoin,
        SvgLayerType, SvgStyle, SvgCallbacks, SvgParseError, SvgTransformId,
        new_svg_transform_id, new_view_box_id, SvgViewBoxId, LayerType};
    use traits::Layout;
    use webrender::api::ColorU;
    use FastHashMap;

    pub fn parse_from<S: AsRef<str>, T: Layout>(svg_source: S, view_boxes: &mut FastHashMap<SvgViewBoxId, ViewBox>)
    -> Result<(Vec<SvgLayer<T>>, FastHashMap<SvgTransformId, Transform>), SvgParseError> {
        let opt = Options::default();
        let rtree = Tree::from_str(svg_source.as_ref(), &opt).unwrap();

        let mut layer_data = Vec::new();
        let mut transform = None;
        let mut transforms = FastHashMap::default();

        let view_box = rtree.svg_node().view_box;
        let view_box_id = new_view_box_id();
        view_boxes.insert(view_box_id, view_box);

        for node in rtree.root().descendants() {
            if let NodeKind::Path(p) = &*node.borrow() {
                let mut style = SvgStyle::default();

                // use the first transform component
                if transform.is_none() {
                    transform = Some(node.borrow().transform());
                }

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

                let transform_id = transform.and_then(|t| {
                    let new_id = new_svg_transform_id();
                    transforms.insert(new_id, t.clone());
                    Some(new_id)
                });

                layer_data.push(SvgLayer {
                    data: LayerType::KnownSize([SvgLayerType::Polygon(p.segments.iter().map(|e| as_event(e)).collect())]),
                    callbacks: SvgCallbacks::None,
                    style: style,
                    transform_id: transform_id,
                    view_box_id: view_box_id,
                })
            }
        }

        Ok((layer_data, transforms))
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
            line_width: ((s.width as f32) * 1000.0) as usize,
            start_cap: line_cap,
            end_cap: line_cap,
            line_join,
            .. Default::default()
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
        }
    }
}

#[derive(Debug, Clone)]
pub enum SvgLayerResource {
    Reference(SvgLayerId),
    Direct {
        style: SvgStyle,
        fill: Option<VerticesIndicesBuffer>,
        stroke: Option<VerticesIndicesBuffer>,
    },
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
    /// Font size of the text
    pub font_size: StyleFontSize,
    /// Font ID, such as "ExternalFont('Arial')"
    pub font_id: FontId,
    /// What are the glyphs in this text
    pub text_layout: SvgTextLayout,
    /// What is the font color & stroke (if any)?
    pub style: SvgStyle,
    /// Is the text rotated or on a curve?
    pub placement: SvgTextPlacement,
    /// X and Y displacement of the font in the drawing, measured from the top left
    pub position: SvgPosition,
}

#[derive(Debug, Clone)]
pub struct SvgTextLayout(pub LayoutTextResult);

/// An axis-aligned bounding box (not rotated / skewed)
#[derive(Debug, Copy, Clone)]
pub struct SvgBbox(pub TypedRect<f32, SvgWorldPixel>);

impl SvgBbox {
    /// Simple function for drawing a single bounding box
    pub fn draw_lines(&self, color: ColorU, line_width: f32) -> SvgLayerResource {
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
        Some(SvgStrokeOptions {
            line_width: (line_width * 1000.0) as usize,
            .. Default::default()
        }))
    }

    /// Checks if the bounding box contains a point
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        self.0.contains(&TypedPoint2D::new(x, y))
    }

    /// Translate the SvgBbox by x / y
    pub fn translate(&mut self, x: f32, y: f32) {
        self.0 = self.0.translate(&TypedVector2D::new(x, y));
    }
}

#[inline]
fn is_point_in_shape(point: (f32, f32), shape: &[(f32, f32)]) -> bool {
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
fn side_of_point(target: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
    ((target.0 - start.0) * (end.1 - start.1)) -
    ((target.1 - start.1) * (end.0 - start.0))
}

impl SvgTextLayout {
    /// Calculate the text layout from a font and a font size.
    ///
    /// Warning: may be slow on large texts.
    pub fn from_str(text: &str, font: &Font, font_size: &StyleFontSize, text_layout_options: &TextLayoutOptions) -> Self {
        let font_metrics = FontMetrics::new(font, font_size, text_layout_options);
        SvgTextLayout(layout_text(text, font, &font_metrics))
    }

    /// Get the bounding box of a layouted text
    pub fn get_bbox(&self, placement: &SvgTextPlacement) -> SvgBbox {
        use self::SvgTextPlacement::*;
        use text_layout::{DEFAULT_CHARACTER_WIDTH_MULTIPLIER, DEFAULT_LINE_HEIGHT_MULTIPLIER};

        let normal_width = self.0.min_width / DEFAULT_CHARACTER_WIDTH_MULTIPLIER;
        let normal_height = self.0.min_height / DEFAULT_LINE_HEIGHT_MULTIPLIER;

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

                let font_size = self.0.font_metrics.font_size_no_line_height.y;
                bbox.0.origin.y -= font_size;
                bbox.0.size.height += font_size;
                bbox.0
            }
        })
    }
}

impl SvgText {
    pub fn to_svg_layer(&self, vectorized_fonts_cache: &VectorizedFontCache, resources: &AppResources)
    -> SvgLayerResource
    {
        let font = resources.get_font(&self.font_id).unwrap().0;
        let vectorized_font = vectorized_fonts_cache.get_font(&self.font_id, resources).unwrap();
        let font_metrics = FontMetrics::new(&font, &self.font_size, &TextLayoutOptions::default());
        match self.placement {
            SvgTextPlacement::Unmodified => {
                normal_text(&self.text_layout.0, &self.position, self.style, &font, &*vectorized_font, &self.font_size, &font_metrics)
            },
            SvgTextPlacement::Rotated(degrees) => {
                rotated_text(&self.text_layout.0, &self.position, self.style, &font, &*vectorized_font, &self.font_size, &font_metrics, degrees)
            },
            SvgTextPlacement::OnCubicBezierCurve(curve) => {
                text_on_curve(&self.text_layout.0, &self.position, self.style, &font, &*vectorized_font, &self.font_size, &font_metrics, &curve)
            }
        }
    }

    pub fn get_bbox(&self) -> SvgBbox {
        let mut bbox = self.text_layout.get_bbox(&self.placement);
        bbox.translate(self.position.x, self.position.y);
        bbox
    }
}

fn normal_text(
    layout: &LayoutTextResult,
    position: &SvgPosition,
    text_style: SvgStyle,
    font: &Font,
    vectorized_font: &VectorizedFont,
    font_size: &StyleFontSize,
    font_metrics: &FontMetrics)
-> SvgLayerResource
{
    let fill_vertices = text_style.fill.and_then(|_| {
        Some(normal_text_to_vertices(&font_size, position, &layout.layouted_glyphs, vectorized_font, font, font_metrics, get_fill_vertices))
    });

    let stroke_vertices = text_style.stroke.and_then(|_| {
        Some(normal_text_to_vertices(&font_size, position, &layout.layouted_glyphs, vectorized_font, font, font_metrics, get_stroke_vertices))
    });

    SvgLayerResource::Direct {
        style: text_style,
        fill: fill_vertices,
        stroke: stroke_vertices,
    }
}

#[derive(Debug, Copy, Clone)]
pub struct SvgPosition {
    pub x: f32,
    pub y: f32,
}

impl Default for SvgPosition {
    fn default() -> Self {
        SvgPosition { x: 0.0, y: 0.0 }
    }
}

fn normal_text_to_vertices(
    font_size: &StyleFontSize,
    position: &SvgPosition,
    glyph_ids: &[GlyphInstance],
    vectorized_font: &VectorizedFont,
    original_font: &Font,
    font_metrics: &FontMetrics,
    transform_func: fn(&VectorizedFont, &Font, &[GlyphInstance]) -> Vec<VertexBuffers<SvgVert, u32>>
) -> VerticesIndicesBuffer
{
    use text_layout::{DEFAULT_LINE_HEIGHT_MULTIPLIER, DEFAULT_CHARACTER_WIDTH_MULTIPLIER};

    let mut vertex_buffers = transform_func(vectorized_font, original_font, glyph_ids);

    vertex_buffers.iter_mut().zip(glyph_ids).for_each(|(vertex_buf, gid)| {
        scale_vertex_buffer(&mut vertex_buf.vertices, font_size, font_metrics.height_for_1px);
        transform_vertex_buffer(&mut vertex_buf.vertices, (gid.point.x / DEFAULT_CHARACTER_WIDTH_MULTIPLIER) + position.x, (gid.point.y / DEFAULT_LINE_HEIGHT_MULTIPLIER) + position.y);
    });

    join_vertex_buffers(&vertex_buffers)
}

fn rotated_text(
    layout: &LayoutTextResult,
    position: &SvgPosition,
    text_style: SvgStyle,
    font: &Font,
    vectorized_font: &VectorizedFont,
    font_size: &StyleFontSize,
    font_metrics: &FontMetrics,
    rotation_degrees: f32)
-> SvgLayerResource
{
    let fill_vertices = text_style.fill.and_then(|_| {
        Some(rotated_text_to_vertices(&font_size, position, &layout.layouted_glyphs, vectorized_font, font, rotation_degrees, font_metrics, get_fill_vertices))
    });

    let stroke_vertices = text_style.stroke.and_then(|_| {
        Some(rotated_text_to_vertices(&font_size, position, &layout.layouted_glyphs, vectorized_font, font, rotation_degrees, font_metrics, get_stroke_vertices))
    });

    SvgLayerResource::Direct {
        style: text_style,
        fill: fill_vertices,
        stroke: stroke_vertices,
    }
}

fn rotated_text_to_vertices(
    font_size: &StyleFontSize,
    position: &SvgPosition,
    glyph_ids: &[GlyphInstance],
    vectorized_font: &VectorizedFont,
    original_font: &Font,
    rotation_degrees: f32,
    font_metrics: &FontMetrics,
    transform_func: fn(&VectorizedFont, &Font, &[GlyphInstance]) -> Vec<VertexBuffers<SvgVert, u32>>
) -> VerticesIndicesBuffer
{
    use text_layout::{DEFAULT_CHARACTER_WIDTH_MULTIPLIER, DEFAULT_LINE_HEIGHT_MULTIPLIER};

    let rotation_rad = rotation_degrees.to_radians();
    let (char_sin, char_cos) = (rotation_rad.sin(), rotation_rad.cos());

    let mut vertex_buffers = transform_func(vectorized_font, original_font, glyph_ids);

    vertex_buffers.iter_mut().zip(glyph_ids).for_each(|(vertex_buf, gid)| {
        scale_vertex_buffer(&mut vertex_buf.vertices, font_size, font_metrics.height_for_1px);
        transform_vertex_buffer(&mut vertex_buf.vertices, gid.point.x / DEFAULT_CHARACTER_WIDTH_MULTIPLIER, gid.point.y / DEFAULT_LINE_HEIGHT_MULTIPLIER);
        rotate_vertex_buffer(&mut vertex_buf.vertices, char_sin, char_cos);
        transform_vertex_buffer(&mut vertex_buf.vertices, position.x, position.y);
    });

    join_vertex_buffers(&vertex_buffers)
}

fn text_on_curve(
    layout: &LayoutTextResult,
    position: &SvgPosition,
    text_style: SvgStyle,
    font: &Font,
    vectorized_font: &VectorizedFont,
    font_size: &StyleFontSize,
    font_metrics: &FontMetrics,
    curve: &SampledBezierCurve)
-> SvgLayerResource
{
    let (char_offsets, char_rotations) = curve.get_text_offsets_and_rotations(&layout.layouted_glyphs, 0.0);

    let fill_vertices = text_style.fill.and_then(|_| {
        Some(curved_vector_text_to_vertices(font_size, position, &layout.layouted_glyphs, vectorized_font, font, &char_offsets, &char_rotations, font_metrics, get_fill_vertices))
    });

    let stroke_vertices = text_style.stroke.and_then(|_| {
        Some(curved_vector_text_to_vertices(font_size, position, &layout.layouted_glyphs, vectorized_font, font, &char_offsets, &char_rotations, font_metrics, get_stroke_vertices))
    });

    SvgLayerResource::Direct {
        style: text_style,
        fill: fill_vertices,
        stroke: stroke_vertices,
    }
}

// Calculates the layout for one word block
fn curved_vector_text_to_vertices(
    font_size: &StyleFontSize,
    position: &SvgPosition,
    glyph_ids: &[GlyphInstance],
    vectorized_font: &VectorizedFont,
    original_font: &Font,
    char_offsets: &[(f32, f32)],
    char_rotations: &[BezierCharacterRotation],
    font_metrics: &FontMetrics,
    transform_func: fn(&VectorizedFont, &Font, &[GlyphInstance]) -> Vec<VertexBuffers<SvgVert, u32>>
) -> VerticesIndicesBuffer
{
    let mut vertex_buffers = transform_func(vectorized_font, original_font, glyph_ids);

    vertex_buffers.iter_mut()
    .zip(char_rotations.into_iter())
    .zip(char_offsets.iter())
    .for_each(|((vertex_buf, char_rot), char_offset)| {
        let (char_offset_x, char_offset_y) = char_offset; // weird borrow issue
        // 2. Scale characters to the final size
        scale_vertex_buffer(&mut vertex_buf.vertices, font_size, font_metrics.height_for_1px);
        // 3. Rotate individual characters inside of the word
        let (char_sin, char_cos) = (char_rot.0.sin(), char_rot.0.cos());
        rotate_vertex_buffer(&mut vertex_buf.vertices, char_sin, char_cos);
        // 4. Transform characters to their respective positions
        transform_vertex_buffer(&mut vertex_buf.vertices, *char_offset_x + position.x, *char_offset_y + position.y);
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
    pub fn render_svg<T: Layout>(
        &self,
        svg_cache: &SvgCache<T>,
        window: &ReadOnlyWindow,
        width: usize,
        height: usize)
    -> Texture
    {
        let tex = window.create_texture(width as u32, height as u32);

        // TODO: This currently doesn't work - only the first draw call is drawn
        // This is probably because either webrender or glium messes with the texture
        // in some way. Need to investigate.
        let background_color: ColorF = self.background_color.into();
        tex.as_surface().clear_color(
            background_color.r,
            background_color.g,
            background_color.b,
            0.0);

        let z_index: f32 = 0.5;
        let bbox: TypedRect<f32, SvgWorldPixel> = TypedRect {
                origin: TypedPoint2D::new(0.0, 0.0),
                size: TypedSize2D::new(width as f32, height as f32),
        };
        let shader = svg_cache.init_shader(window);

        let hidpi = window.get_hidpi_factor() as f32;
        let zoom = if self.enable_hidpi { self.zoom * hidpi } else { self.zoom };
        let pan = if self.enable_hidpi { (self.pan.0 * hidpi, self.pan.1 * hidpi) } else { self.pan };

        let draw_options = DrawParameters {
            primitive_restart_index: true,
            .. Default::default()
        };

        {
            let mut surface = tex.as_surface();

            for layer in &self.layers {

                let style = match layer {
                    SvgLayerResource::Reference(layer_id) => { svg_cache.get_style(layer_id) },
                    SvgLayerResource::Direct { style, .. } => *style,
                };

                if let Some(color) = style.fill {
                    if let Some(fill_vi) = match &layer {
                        SvgLayerResource::Reference(layer_id) => svg_cache.get_vertices_and_indices(window, layer_id),
                        SvgLayerResource::Direct { fill, .. } => fill.as_ref().and_then(|f| {
                            let vertex_buffer = VertexBuffer::new(window, &f.vertices).unwrap();
                            let index_buffer = IndexBuffer::new(window, PrimitiveType::TrianglesList, &f.indices).unwrap();
                            Some(Rc::new((vertex_buffer, index_buffer)))
                    })} {
                        let (ref fill_vertices, ref fill_indices) = *fill_vi;
                        draw_vertex_buffer_to_surface(
                            &mut surface,
                            &shader.program,
                            &fill_vertices,
                            &fill_indices,
                            &draw_options,
                            &bbox,
                            color.into(),
                            z_index,
                            pan,
                            zoom);
                    }
                }

                if let Some((stroke_color, _)) = style.stroke {
                    if let Some(stroke_vi) = match &layer {
                        SvgLayerResource::Reference(layer_id) => svg_cache.get_stroke_vertices_and_indices(window, layer_id),
                        SvgLayerResource::Direct { stroke, .. } => stroke.as_ref().and_then(|f| {
                            let vertex_buffer = VertexBuffer::new(window, &f.vertices).unwrap();
                            let index_buffer = IndexBuffer::new(window, PrimitiveType::TrianglesList, &f.indices).unwrap();
                            Some(Rc::new((vertex_buffer, index_buffer)))
                        })} {
                        let (ref stroke_vertices, ref stroke_indices) = *stroke_vi;
                        draw_vertex_buffer_to_surface(
                            &mut surface,
                            &shader.program,
                            &stroke_vertices,
                            &stroke_indices,
                            &draw_options,
                            &bbox,
                            stroke_color.into(),
                            z_index,
                            pan,
                            zoom);
                    }
                }
            }
        }

        if self.enable_fxaa {
            // TODO: apply FXAA shader
        }

        tex
    }
}

fn draw_vertex_buffer_to_surface<S: Surface>(
        surface: &mut S,
        shader: &Program,
        vertices: &VertexBuffer<SvgVert>,
        indices: &IndexBuffer<u32>,
        draw_options: &DrawParameters,
        bbox: &TypedRect<f32, SvgWorldPixel>,
        color: ColorF,
        z_index: f32,
        pan: (f32, f32),
        zoom: f32)
{
    let color = srgba_to_linear(color);

    let uniforms = uniform! {
        bbox_origin: (bbox.origin.x, bbox.origin.y),
        bbox_size: (bbox.size.width / 2.0, bbox.size.height / 2.0),
        z_index: z_index,
        color: (
            color.r as f32,
            color.g as f32,
            color.b as f32,
            color.a as f32
        ),
        offset: (pan.0, pan.1),
        zoom: zoom,
    };

    surface.draw(vertices, indices, shader, &uniforms, draw_options).unwrap();
}

/// Taken from the `palette` crate - I wouldn't want to
/// import the entire crate just for one function (due to added compile time)
///
/// The MIT License (MIT)
///
/// Copyright (c) 2015 Erik Hedvall
///
/// Permission is hereby granted, free of charge, to any person obtaining a copy
/// of this software and associated documentation files (the "Software"), to deal
/// in the Software without restriction, including without limitation the rights
/// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
/// copies of the Software, and to permit persons to whom the Software is
/// furnished to do so, subject to the following conditions:
///
/// The above copyright notice and this permission notice shall be included in all
/// copies or substantial portions of the Software.
///
/// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
/// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
/// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
/// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
/// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
/// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
/// SOFTWARE.
fn srgba_to_linear(color: ColorF) -> ColorF {

    fn into_linear(x: f32) -> f32 {
        if x <= 0.04045 {
            x / 12.92
        } else {
            ((x + 0.055) / 1.055).powf(2.4)
        }
    }

    ColorF {
        r: into_linear(color.r),
        g: into_linear(color.g),
        b: into_linear(color.b),
        a: color.a,
    }
}

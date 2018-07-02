use std::{
    fmt,
    rc::Rc,
    io::{Error as IoError, Read},
    sync::{Mutex, atomic::{Ordering, AtomicUsize}},
    cell::UnsafeCell,
    hash::{Hash, Hasher},
};
use glium::{
    backend::Facade,
    DrawParameters, IndexBuffer, VertexBuffer, Display,
    Texture2d, Program,
};
use lyon::{
    tessellation::{
        VertexBuffers, FillOptions, BuffersBuilder, FillVertex, FillTessellator,
        LineCap, LineJoin, StrokeTessellator, StrokeOptions, StrokeVertex,
        basic_shapes::{
            fill_circle, stroke_circle, fill_rounded_rectangle,
            stroke_rounded_rectangle, BorderRadii
        },
    },
    path::{
        default::{Builder, Path},
        builder::{PathBuilder, FlatPathBuilder}, PathEvent,
    },
    geom::euclid::{TypedRect, TypedPoint2D, TypedSize2D},
};
use resvg::usvg::{Error as SvgError, ViewBox, Transform};
use webrender::api::{ColorU, ColorF};
use {
    FastHashMap,
    dom::Callback,
    traits::Layout,
    id_tree::NonZeroUsizeHack,
};

/// In order to store / compare SVG files, we have to
pub(crate) static SVG_BLOB_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SvgTransformId(NonZeroUsizeHack);

const SVG_TRANSFORM_ID: AtomicUsize = AtomicUsize::new(0);

pub fn new_svg_transform_id() -> SvgTransformId {
    SvgTransformId(NonZeroUsizeHack::new(SVG_TRANSFORM_ID.fetch_add(1, Ordering::SeqCst)))
}

const SVG_VIEW_BOX_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct SvgViewBoxId(usize);

pub fn new_view_box_id() -> SvgViewBoxId {
    SvgViewBoxId(SVG_VIEW_BOX_ID.fetch_add(1, Ordering::SeqCst))
}

const SVG_VERTEX_SHADER: &str = "
    #version 130

    in vec2 xy;
    in vec2 normal;

    uniform vec2 bbox_origin;
    uniform vec2 bbox_size;
    uniform float z_index;

    void main() {
        gl_Position = vec4(vec2(-1.0) + ((xy - bbox_origin) / bbox_size), z_index, 1.0);
    }";

const SVG_FRAGMENT_SHADER: &str = "
    #version 130
    uniform vec4 color;

    out vec4 out_color;

    void main() {
        out_color = color;
    }
";

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct SvgLayerId(usize);

#[derive(Debug, Clone)]
pub struct SvgShader {
    pub program: Rc<Program>,
}

impl SvgShader {
    pub fn new<F: Facade + ?Sized>(display: &F) -> Self {
        Self {
            program: Rc::new(Program::from_source(display, SVG_VERTEX_SHADER, SVG_FRAGMENT_SHADER, None).unwrap()),
        }
    }
}

pub struct SvgCache<T: Layout> {
    // note: one "layer" merely describes one or more polygons that have the same style
    layers: FastHashMap<SvgLayerId, SvgLayer<T>>,
    // Stores the vertices and indices necessary for drawing. Must be synchronized with the `layers`
    gpu_ready_to_upload_cache: FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    stroke_gpu_ready_to_upload_cache: FastHashMap<SvgLayerId, (Vec<SvgVert>, Vec<u32>)>,
    vertex_index_buffer_cache: UnsafeCell<FastHashMap<SvgLayerId, (VertexBuffer<SvgVert>, IndexBuffer<u32>)>>,
    stroke_vertex_index_buffer_cache: UnsafeCell<FastHashMap<SvgLayerId, (VertexBuffer<SvgVert>, IndexBuffer<u32>)>>,
    shader: Mutex<Option<SvgShader>>,
    // Stores the 2D transforms of the shapes on the screen. The vertices are
    // offset by the X, Y value in the transforms struct. This should be expanded
    // to full matrices later on, so you can do full 3D transformations
    // on 2D shapes later on. For now, each transform is just an X, Y offset
    transforms: FastHashMap<SvgTransformId, Transform>,
    view_boxes: FastHashMap<SvgViewBoxId, ViewBox>,
}

impl<T: Layout> Default for SvgCache<T> {
    fn default() -> Self {
        Self {
            layers: FastHashMap::default(),
            gpu_ready_to_upload_cache: FastHashMap::default(),
            stroke_gpu_ready_to_upload_cache: FastHashMap::default(),
            vertex_index_buffer_cache: UnsafeCell::new(FastHashMap::default()),
            stroke_vertex_index_buffer_cache: UnsafeCell::new(FastHashMap::default()),
            shader: Mutex::new(None),
            transforms: FastHashMap::default(),
            view_boxes: FastHashMap::default(),
        }
    }
}

impl<T: Layout> SvgCache<T> {

    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds and compiles the SVG shader if the shader isn't already present
    pub fn init_shader<F: Facade + ?Sized>(&self, display: &F) -> SvgShader {
        let mut shader_lock = self.shader.lock().unwrap();
        if shader_lock.is_none() {
            *shader_lock = Some(SvgShader::new(display));
        }
        shader_lock.as_ref().and_then(|s| Some(s.clone())).unwrap()
    }

    pub fn get_stroke_vertices_and_indices<'a, F: Facade>(&'a self, window: &F, id: &SvgLayerId)
    -> &'a (VertexBuffer<SvgVert>, IndexBuffer<u32>)
    {
        use std::collections::hash_map::Entry::*;
        use glium::{VertexBuffer, IndexBuffer, index::PrimitiveType};

        let rmut = unsafe { &mut *self.stroke_vertex_index_buffer_cache.get() };
        let rnotmut = &self.stroke_gpu_ready_to_upload_cache;

        rmut.entry(*id).or_insert_with(|| {
            let (vbuf, ibuf) = rnotmut.get(id).as_ref().unwrap();
            let vertex_buffer = VertexBuffer::new(window, vbuf).unwrap();
            let index_buffer = IndexBuffer::new(window, PrimitiveType::TrianglesList, ibuf).unwrap();
            (vertex_buffer, index_buffer)
        })
    }

    /// Note: panics if the ID isn't found.
    ///
    /// Since we are required to keep the `self.layers` and the `self.gpu_buffer_cache`
    /// in sync, a panic should never happen
    pub fn get_vertices_and_indices<'a, F: Facade>(&'a self, window: &F, id: &SvgLayerId)
    -> &'a (VertexBuffer<SvgVert>, IndexBuffer<u32>)
    {
        use std::collections::hash_map::Entry::*;
        use glium::{VertexBuffer, IndexBuffer, index::PrimitiveType};

        // First, we need the SvgCache to call this function immutably, otherwise we can't
        // use it from the Layout::layout() function
        //
        // Rust does also not "understand" that we want to return a reference into
        // self.vertex_index_buffer_cache, so the reference that we are returning lives as
        // long as the self.gpu_ready_to_upload_cache (at least until it's removed)

        // We need to use UnsafeCell here - when using a regular RefCell, Rust thinks we
        // are destroying the reference after the borrow, but that isn't true.

        let rmut = unsafe { &mut *self.vertex_index_buffer_cache.get() };
        let rnotmut = &self.gpu_ready_to_upload_cache;

        rmut.entry(*id).or_insert_with(|| {
            let (vbuf, ibuf) = rnotmut.get(id).as_ref().unwrap();
            let vertex_buffer = VertexBuffer::new(window, vbuf).unwrap();
            let index_buffer = IndexBuffer::new(window, PrimitiveType::TrianglesList, ibuf).unwrap();
            (vertex_buffer, index_buffer)
        })
    }

    pub fn get_style(&self, id: &SvgLayerId)
    -> SvgStyle
    {
        self.layers.get(id).as_ref().unwrap().style
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

impl<T: Layout> SvgCache<T> {

    pub fn add_layer(&mut self, layer: SvgLayer<T>) -> SvgLayerId {
        // TODO: set tolerance based on zoom
        let new_svg_id = SvgLayerId(SVG_BLOB_ID.fetch_add(1, Ordering::SeqCst));

        let ((vertex_buf, index_buf), opt_stroke) =
            tesselate_layer_data(&layer.data, 0.01, layer.style.stroke.and_then(|s| Some(s.1.clone())));

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
        let rmut = unsafe { &mut *self.vertex_index_buffer_cache.get() };
        let stroke_rmut = unsafe { &mut *self.stroke_vertex_index_buffer_cache.get() };
        rmut.remove(&svg_id);
        stroke_rmut.remove(&svg_id);
    }

    pub fn clear_all_layers(&mut self) {
        self.layers.clear();

        self.gpu_ready_to_upload_cache.clear();
        self.stroke_gpu_ready_to_upload_cache.clear();

        let rmut = unsafe { &mut *self.vertex_index_buffer_cache.get() };
        rmut.clear();

        let stroke_rmut = unsafe { &mut *self.stroke_vertex_index_buffer_cache.get() };
        stroke_rmut.clear();
    }

    pub fn add_transforms(&mut self, transforms: FastHashMap<SvgTransformId, Transform>) {
        transforms.into_iter().for_each(|(k, v)| {
            self.transforms.insert(k, v);
        });
    }

    /// Parses an input source, parses the SVG, adds the shapes as layers into
    /// the registry, returns the IDs of the added shapes, in the order that they appeared in the Svg
    pub fn add_svg<S: AsRef<str>>(&mut self, input: S) -> Result<Vec<SvgLayerId>, SvgParseError> {
        let (layers, transforms) = self::svg_to_lyon::parse_from(input, &mut self.view_boxes)?;
        self.add_transforms(transforms);
        Ok(layers
            .into_iter()
            .map(|layer| self.add_layer(layer))
            .collect())
    }
}

fn tesselate_layer_data(layer_data: &LayerType, tolerance: f32, stroke_options: Option<SvgStrokeOptions>)
-> ((Vec<SvgVert>, Vec<u32>), Option<(Vec<SvgVert>, Vec<u32>)>)
{
    const GL_RESTART_INDEX: u32 = ::std::u32::MAX;

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

#[derive(Debug)]
pub enum SvgParseError {
    /// Syntax error in the Svg
    FailedToParseSvg(SvgError),
    /// Io error reading the Svg
    IoError(IoError),
}

impl From<SvgError> for SvgParseError {
    fn from(e: SvgError) -> Self {
        SvgParseError::FailedToParseSvg(e)
    }
}

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
    pub view_box_id: SvgViewBoxId,
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
#[derive(Debug, Clone)]
pub enum SvgLayerType {
    Polygon(Vec<PathEvent>),
    Circle(SvgCircle),
    Rect(SvgRect),
    Text(String),
}

#[derive(Debug, Copy, Clone)]
pub struct SvgVert {
    pub xy: (f32, f32),
    pub normal: (f32, f32),
}

implement_vertex!(SvgVert, xy, normal);

#[derive(Debug, Copy, Clone)]
pub struct SvgWorldPixel;


impl SvgLayerType {
    pub fn tesselate(&self, tolerance: f32, stroke: Option<SvgStrokeOptions>)
    -> (VertexBuffers<SvgVert>, Option<VertexBuffers<SvgVert>>)
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
            },
            SvgLayerType::Text(_t) => { },
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

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SvgRect {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub rx: f32,
    pub ry: f32,
}

mod svg_to_lyon {

    use std::{slice, iter, io::Read};
    use lyon::{
        math::Point,
        path::{PathEvent, iterator::PathIter},
        tessellation::{self, StrokeOptions},
    };
    use resvg::usvg::{self, ViewBox, Transform, Tree, Path, PathSegment,
        Color, Options, Paint, Stroke, LineCap, LineJoin, NodeKind};
    use svg::{SvgLayer, SvgStrokeOptions, SvgLineCap, SvgLineJoin,
        SvgLayerType, SvgStyle, SvgCallbacks, SvgParseError, SvgTransformId,
        new_svg_transform_id, new_view_box_id, SvgViewBoxId, LayerType};
    use traits::Layout;
    use webrender::api::ColorU;
    use FastHashMap;

    pub fn parse_from<S: AsRef<str>, T: Layout>(mut svg_source: S, view_boxes: &mut FastHashMap<SvgViewBoxId, ViewBox>)
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

// Empty test, for some reason codecov doesn't detect any files (and therefore
// doesn't report codecov % correctly) except if they have at least one test in
// the file. This is an empty test, which should be updated later on
#[test]
fn __codecov_test_svg_file() {

}
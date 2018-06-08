use std::sync::Mutex;
use glium::backend::Facade;
use std::rc::Rc;
use glium::DrawParameters;
use glium::IndexBuffer;
use glium::VertexBuffer;
use glium::Display;
use glium::Texture2d;
use glium::Program;
use webrender::api::ColorF;
use std::io::Read;
use lyon::path::default::Path;
use webrender::api::ColorU;
use dom::Callback;
use traits::Layout;
use std::sync::atomic::{Ordering, AtomicUsize};
use FastHashMap;
use std::hash::{Hash, Hasher};
use svg_crate::parser::Error as SvgError;
use std::io::Error as IoError;
use std::fmt;
use euclid::TypedRect;
use lyon::tessellation::VertexBuffers;
use std::cell::UnsafeCell;

/// In order to store / compare SVG files, we have to
pub(crate) static SVG_BLOB_ID: AtomicUsize = AtomicUsize::new(0);

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
    vertex_index_buffer_cache: UnsafeCell<FastHashMap<SvgLayerId, (VertexBuffer<SvgVert>, IndexBuffer<u32>)>>,
    shader: Mutex<Option<SvgShader>>,
}

impl<T: Layout> Default for SvgCache<T> {
    fn default() -> Self {
        Self {
            layers: FastHashMap::default(),
            gpu_ready_to_upload_cache: FastHashMap::default(),
            vertex_index_buffer_cache: UnsafeCell::new(FastHashMap::default()),
            shader: Mutex::new(None),
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
        let new_svg_id = SvgLayerId(SVG_BLOB_ID.fetch_add(1, Ordering::SeqCst));
        let (vertex_buf, index_buf) = tesselate_layer_data(&layer.data);
        self.layers.insert(new_svg_id, layer);
        self.gpu_ready_to_upload_cache.insert(new_svg_id, (vertex_buf, index_buf));
        new_svg_id
    }

    pub fn delete_layer(&mut self, svg_id: SvgLayerId) {
        self.layers.remove(&svg_id);
        self.gpu_ready_to_upload_cache.remove(&svg_id);
        let rmut = unsafe { &mut *self.vertex_index_buffer_cache.get() };
        rmut.remove(&svg_id);
    }

    pub fn clear_all_layers(&mut self) {
        self.layers.clear();
        self.gpu_ready_to_upload_cache.clear();
        let rmut = unsafe { &mut *self.vertex_index_buffer_cache.get() };
        rmut.clear();
    }

    /// Parses an input source, parses the SVG, adds the shapes as layers into
    /// the registry, returns the IDs of the added shapes, in the order that they appeared in the Svg
    pub fn add_svg<R: Read>(&mut self, input: R) -> Result<Vec<SvgLayerId>, SvgParseError> {
        Ok(self::svg_to_lyon::parse_from(input)?
            .into_iter()
            .map(|layer|
                self.add_layer(layer))
            .collect())
    }
}

fn tesselate_layer_data(layer_data: &[SvgLayerType]) -> (Vec<SvgVert>, Vec<u32>) {
    const GL_RESTART_INDEX: u32 = ::std::u32::MAX;

    let mut last_index = 0;
    let mut vertex_buf = Vec::<SvgVert>::new();
    let mut index_buf = Vec::<u32>::new();

    for layer in layer_data {
        let VertexBuffers { vertices, indices } = layer.tesselate();
        let vertices_len = vertices.len();
        vertex_buf.extend(vertices.into_iter());
        index_buf.extend(indices.into_iter().map(|i| i as u32 + last_index as u32));
        index_buf.push(GL_RESTART_INDEX);
        last_index += vertices_len;
    }

    (vertex_buf, index_buf)
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
    pub data: Vec<SvgLayerType>,
    pub callbacks: SvgCallbacks<T>,
    pub style: SvgStyle,
}

impl<T: Layout> Clone for SvgLayer<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            callbacks: self.callbacks.clone(),
            style: self.style.clone(),
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
    pub stroke: Option<ColorU>,
    /// Stroke width * 1000, since otherwise `Hash` can't be derived
    ///
    /// i.e. a stroke width of `5.0` = `5000`.
    pub stroke_width: Option<usize>,
    /// Fill color
    pub fill: Option<ColorU>,
    // missing:
    //
    // fill-opacity
    // stroke-miterlimit
    // stroke-dasharray
    // stroke-opacity
}

impl SvgStyle {
    /// Parses the Svg style from a string, on error returns the default `SvgStyle`.
    pub fn from_svg_string(input: &str) -> Self {
        use css_parser::parse_css_color;
        use FastHashMap;

        let mut style = FastHashMap::<&str, &str>::default();

        for kv in input.split(";") {
            let mut iter = kv.trim().split(":");
            let key = iter.next();
            let value = iter.next();
            if let (Some(k), Some(v)) = (key, value) {
                style.insert(k, v);
            }
        }

        let fill = style.get("fill")
            .and_then(|s| parse_css_color(s).ok());

        let stroke = style.get("stroke")
            .and_then(|s| parse_css_color(s).ok());

        let stroke_width = style.get("stroke-width")
            .and_then(|s| s.parse::<f32>().ok())
            .and_then(|sw_float| Some((sw_float * 1000.0) as usize));

        Self {
            fill,
            stroke_width,
            stroke,
        }
    }
}

/// One "layer" is simply one or more polygons that get drawn using the same style
/// i.e. one SVG `<path></path>` element
#[derive(Debug, Clone)]
pub enum SvgLayerType {
    Polygon(Path),
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
    pub fn tesselate(&self)
    -> VertexBuffers<SvgVert>
    {
        use self::SvgLayerType::*;
        use lyon::tessellation::{VertexBuffers, FillOptions, BuffersBuilder, FillVertex, FillTessellator};
        use lyon::tessellation::basic_shapes::{fill_circle, fill_rounded_rectangle};
        use lyon::geom::euclid::{TypedRect, TypedPoint2D, TypedSize2D};
        use lyon::tessellation::basic_shapes::BorderRadii;

        let mut geometry = VertexBuffers::new();

        match self {
            Polygon(p) => {
                let mut tessellator = FillTessellator::new();
                tessellator.tessellate_path(
                    p.path_iter(),
                    &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }),
                ).unwrap();
            },
            Circle(c) => {
                fill_circle(
                    TypedPoint2D::new(c.center_x, c.center_y), c.radius, &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));
            },
            Rect(r) => {
                fill_rounded_rectangle(
                    &TypedRect::new(TypedPoint2D::new(r.x, r.y), TypedSize2D::new(r.width, r.height)),
                    &BorderRadii {
                        top_left: r.rx,
                        top_right: r.rx,
                        bottom_left: r.rx,
                        bottom_right: r.rx,
                    },
                    &FillOptions::default(),
                    &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                        SvgVert {
                            xy: (vertex.position.x, vertex.position.y),
                            normal: (vertex.normal.x, vertex.position.y),
                        }
                    }
                ));
            },
            Text(_t) => { },
        }

        geometry
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

    use svg_crate::node::Attributes;
    use std::io::Read;
    use std::collections::HashMap;
    use lyon::path::default::Path;
    use lyon::{
        path::{PathEvent, default::Builder, builder::SvgPathBuilder},
        tessellation::{self, StrokeOptions},
        math::Point,
        geom::{ArcFlags, euclid::{TypedPoint2D, TypedVector2D, Angle}},
        path::{SvgEvent, builder::SvgBuilder},
    };
    use svg::{SvgCircle, SvgRect, SvgParseError, SvgLayer, SvgStyle};
    use svg_crate::node::element::path::Parameters;
    use svg_crate::node::element::tag::Tag;
    use traits::Layout;

    pub fn parse_from<R: Read, T: Layout>(svg_source: R)
    -> Result<Vec<SvgLayer<T>>, SvgParseError>
    {
        use svg_crate::{read, parser::{Event, Error}};
        use std::mem::discriminant;
        use svg::{SvgLayerType, SvgCallbacks};

        let file = read(svg_source)?;

        let mut last_err = None;

        let layer_data = file
            // We are only interested in tags, not comments or other stuff
            .filter_map(|event| match event {
                    Event::Tag(id, _, attributes) => Some((id, attributes)),
                    Event::Error(e) => { /* TODO: hacky */ last_err = Some(e); None },
                    _ => None,
                }
            )
            // assert that the shape has a style. If it doesn't have a style, we can't draw it,
            // so there is no point in parsing it
            .filter_map(|(id, attributes)| {
                let svg_style = match attributes.get("style") {
                    Some(style_string) => SvgStyle::from_svg_string(style_string),
                    _ => return None,
                };
                Some((id, svg_style, attributes))
            })
            // Now parse the shape
            .filter_map(|(id, style, attributes)| {
                let layer_data = match id {
                   "path" => match parse_path(&attributes) {
                        None => return None,
                        Some(s) => SvgLayerType::Polygon(s),
                    }
                   "circle" => match parse_circle(&attributes) {
                        None => return None,
                        Some(s) => SvgLayerType::Circle(s),
                    },
                   "rect" => match parse_rect(&attributes) {
                        None => return None,
                        Some(s) => SvgLayerType::Rect(s),
                    },
                   "flowRoot" => match parse_flow_root(&attributes) {
                        None => return None,
                        Some(s) => SvgLayerType::Text(s),
                    },
                   "text" => match parse_text(&attributes) {
                        None => return None,
                        Some(s) => SvgLayerType::Text(s),
                    },
                    _ => return None,
                };
                Some((layer_data, style))
            })
            .map(|(data, style)| {
                SvgLayer {
                    data: vec![data],
                    callbacks: SvgCallbacks::None,
                    style: style,
                }
            })
            .collect();

        if let Some(e) = last_err {
            Err(e.into())
        } else {
            Ok(layer_data)
        }
    }

    fn parse_path(attributes: &Attributes) -> Option<Path> {
        use lyon::path::default::Builder;
        use lyon::path::builder::SvgPathBuilder;
        use lyon::path::builder::FlatPathBuilder;
        use lyon::path::SvgEvent;
        use svg_crate::node::element::{
            tag::Path,
            path::{Command, Command::*, Data},
        };
        use svg_crate::node::element::path::Position::*;

        let data = attributes.get("d")?;
        let data = Data::parse(data).ok()?;

        let mut builder = SvgPathBuilder::new(Builder::new());

        for command in data.iter() {
            match command {
                Move(position, parameters) => match position {
                    Absolute => parameters.chunks(2).for_each(|chunk| match *chunk {
                        [x, y] => builder.svg_event(SvgEvent::MoveTo(TypedPoint2D::new(x, y))),
                        _ => { },
                    }),
                    Relative => parameters.chunks(2).for_each(|chunk| match *chunk {
                        [x, y] => builder.svg_event(SvgEvent::RelativeMoveTo(TypedVector2D::new(x, y))),
                        _ => { },
                    }),
                },
                Line(position, parameters) => match position {
                    Absolute => parameters.chunks(2).for_each(|chunk| match *chunk {
                        [x, y] => builder.svg_event(SvgEvent::LineTo(TypedPoint2D::new(x, y))),
                        _ => { },
                    }),
                    Relative => parameters.chunks(2).for_each(|chunk| match *chunk {
                        [x, y] => builder.svg_event(SvgEvent::RelativeLineTo(TypedVector2D::new(x, y))),
                        _ => { },
                    }),
                },
                HorizontalLine(position, parameters) => match position {
                    Absolute => parameters.iter().for_each(|num| builder.svg_event(SvgEvent::HorizontalLineTo(*num))),
                    Relative => parameters.iter().for_each(|num| builder.svg_event(SvgEvent::RelativeHorizontalLineTo(*num))),
                },
                VerticalLine(position, parameters) => match position {
                    Absolute => parameters.iter().for_each(|num| builder.svg_event(SvgEvent::VerticalLineTo(*num))),
                    Relative => parameters.iter().for_each(|num| builder.svg_event(SvgEvent::RelativeVerticalLineTo(*num))),
                },
                QuadraticCurve(position, parameters) => match position {
                    Absolute => parameters.chunks(4).for_each(|chunk| match *chunk {
                        [x1, y1, x2, y2] => builder.svg_event(SvgEvent::QuadraticTo(TypedPoint2D::new(x1, y1), TypedPoint2D::new(x2, y2))),
                        _ => { },
                    }),
                    Relative => parameters.chunks(4).for_each(|chunk| match *chunk {
                        [x1, y1, x2, y2] => builder.svg_event(SvgEvent::RelativeQuadraticTo(
                            TypedVector2D::new(x1, y1), TypedVector2D::new(x2, y2))),
                        _ => { },
                    }),
                },
                SmoothQuadraticCurve(position, parameters) => match position {
                    Absolute => parameters.chunks(2).for_each(|chunk| match *chunk {
                        [x, y] => builder.svg_event(SvgEvent::SmoothQuadraticTo(TypedPoint2D::new(x, y))),
                        _ => { },
                    }),
                    Relative => parameters.chunks(2).for_each(|chunk| match *chunk {
                        [x, y] => builder.svg_event(SvgEvent::SmoothRelativeQuadraticTo(TypedVector2D::new(x, y))),
                        _ => { },
                    }),
                },
                CubicCurve(position, parameters) => match position {
                    Absolute => parameters.chunks(6).for_each(|chunk| match *chunk {
                        [x1, y1, x2, y2, x3, y3] => builder.svg_event(SvgEvent::CubicTo(
                            TypedPoint2D::new(x1, y1), TypedPoint2D::new(x2, y2), TypedPoint2D::new(x3, y3))),
                        _ => { },
                    }),
                    Relative => parameters.chunks(6).for_each(|chunk| match *chunk {
                        [x1, y1, x2, y2, x3, y3] => builder.svg_event(SvgEvent::RelativeCubicTo(
                            TypedVector2D::new(x1, y1), TypedVector2D::new(x2, y2), TypedVector2D::new(x3, y3))),
                        _ => { },
                    }),
                },
                SmoothCubicCurve(position, parameters) => match position {
                    Absolute => parameters.chunks(4).for_each(|chunk| match *chunk {
                        [x1, y1, x2, y2] => builder.svg_event(SvgEvent::SmoothCubicTo(
                            TypedPoint2D::new(x1, y1), TypedPoint2D::new(x2, y2))),
                        _ => { },
                    }),
                    Relative => parameters.chunks(4).for_each(|chunk| match *chunk {
                        [x1, y1, x2, y2] => builder.svg_event(SvgEvent::SmoothRelativeCubicTo(
                            TypedVector2D::new(x1, y1), TypedVector2D::new(x2, y2))),
                        _ => { },
                    }),
                },
                EllipticalArc(position, parameters) => match position {
                    Absolute => parameters.chunks(5).for_each(|chunk| match *chunk {
                        [x1, y1, angle, x2, y2] => builder.svg_event(
                            SvgEvent::ArcTo(
                                TypedVector2D::new(x1, y1),
                                Angle::degrees(angle),
                                ArcFlags { large_arc: true, sweep: true, },
                                TypedPoint2D::new(x2, y2)
                            )),
                        _ => { },
                    }),
                    Relative => parameters.chunks(5).for_each(|chunk| match *chunk {
                        [x1, y1, angle, x2, y2] => builder.svg_event(
                            SvgEvent::ArcTo(
                                TypedVector2D::new(x1, y1),
                                Angle::degrees(angle),
                                ArcFlags { large_arc: true, sweep: true, },
                                TypedPoint2D::new(x2, y2)
                            )),
                        _ => { },
                    }),
                },
                Close => {
                    builder.close();
                },
            }
        }

        Some(builder.build())
    }

    fn parse_circle(attributes: &Attributes) -> Option<SvgCircle> {
        let center_x = attributes.get("cx")?.parse::<f32>().ok()?;
        let center_y = attributes.get("cy")?.parse::<f32>().ok()?;
        let radius = attributes.get("r")?.parse::<f32>().ok()?;

        Some(SvgCircle {
            center_x,
            center_y,
            radius
        })
    }

    fn parse_rect(attributes: &Attributes) -> Option<SvgRect> {
        let width = attributes.get("width")?.parse::<f32>().ok()?;
        let height = attributes.get("height")?.parse::<f32>().ok()?;
        let x = attributes.get("x")?.parse::<f32>().ok()?;
        let y = attributes.get("y")?.parse::<f32>().ok()?;
        let rx = attributes.get("rx")?.parse::<f32>().ok()?;
        let ry = attributes.get("ry")?.parse::<f32>().ok()?;
        Some(SvgRect {
            width,
            height,
            x, y,
            rx, ry
        })
    }

    // TODO: use text attributes instead of string
    fn parse_flow_root(attributes: &Attributes) -> Option<String> {
        Some(String::from("hello"))
    }

    // TODO: use text attributes instead of string
    fn parse_text(attributes: &Attributes) -> Option<String> {
        Some(String::from("hello"))
    }
}

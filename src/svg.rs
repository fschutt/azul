use dom::Callback;
use traits::LayoutScreen;
use std::sync::atomic::AtomicUsize;
use FastHashMap;
use std::hash::{Hash, Hasher};

/// In order to store / compare SVG files, we have to
pub(crate) static mut SVG_BLOB_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct SvgId(usize);

pub(crate) struct SvgRegistry {
    svg_items: FastHashMap<SvgId, SvgShape>,
}

impl SvgRegistry {
    pub fn add_shape(&mut self, polygon: SvgShape) -> SvgId {
        // TODO
        SvgId(0)
    }

    pub fn delete_shape(&mut self, svg_id: SvgId) {
        // TODO
    }
}

pub struct Svg<T: LayoutScreen> {
    pub layers: Vec<SvgLayer<T>>,
}

impl<T: LayoutScreen> Clone for Svg<T> {
    fn clone(&self) -> Self {
        Self { layers: self.layers.clone() }
    }
}

impl<T: LayoutScreen> Hash for Svg<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for layer in &self.layers {
            layer.hash(state);
        }
    }
}

impl<T: LayoutScreen> PartialEq for Svg<T> {
    fn eq(&self, rhs: &Self) -> bool {
        for (a, b) in self.layers.iter().zip(rhs.layers.iter()) {
            if *a != *b {
                return false
            }
        }
        true
    }
}

impl<T: LayoutScreen> Eq for Svg<T> { }

pub struct SvgLayer<T: LayoutScreen> {
    pub id: String,
    pub data: Vec<SvgId>,
    pub callbacks: SvgCallbacks<T>,
    pub style: SvgStyle,
}

impl<T: LayoutScreen> Clone for SvgLayer<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            data: self.data.clone(),
            callbacks: self.callbacks.clone(),
            style: self.style.clone(),
        }
    }
}

impl<T: LayoutScreen> Hash for SvgLayer<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.data.hash(state);
        self.callbacks.hash(state);
        self.style.hash(state);
    }
}

impl<T: LayoutScreen> PartialEq for SvgLayer<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self.id == rhs.id &&
        self.data == rhs.data &&
        self.callbacks == rhs.callbacks &&
        self.style == rhs.style
    }
}

impl<T: LayoutScreen> Eq for SvgLayer<T> { }

pub enum SvgCallbacks<T: LayoutScreen> {
    // No callbacks for this layer
    None,
    /// Call the callback on any of the items
    Any(Callback<T>),
    /// Call the callback when the SvgLayer item at index [x] is
    ///  hovered over / interacted with
    Some(Vec<(SvgId, Callback<T>)>),
}

impl<T: LayoutScreen> Clone for SvgCallbacks<T> {
    fn clone(&self) -> Self {
        use self::SvgCallbacks::*;
        match self {
            None => None,
            Any(c) => Any(c.clone()),
            Some(v) => Some(v.clone()),
        }
    }
}

impl<T: LayoutScreen> Hash for SvgCallbacks<T> {
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

impl<T: LayoutScreen> PartialEq for SvgCallbacks<T> {
    fn eq(&self, rhs: &Self) -> bool {
        self == rhs
    }
}

impl<T: LayoutScreen> Eq for SvgCallbacks<T> { }

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub struct Rgba {
    pub data: [u8;4],
}

#[derive(Debug, Copy, Clone, PartialEq, Hash)]
pub struct SvgStyle {
    pub outline: Option<Rgba>,
    pub fill: Option<Rgba>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SvgShape {
    Polygon(Vec<(f32, f32)>),
}

// SvgShape::Polygon(vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)])

impl<T: LayoutScreen> Svg<T> {
    pub fn default_testing() -> Self {
        Self {
            layers: vec![SvgLayer {
                id: String::from("svg-layer-01"),
                // simple triangle for testing
                data: vec![SvgId(0)],
                callbacks: SvgCallbacks::None,
                style: SvgStyle {
                    outline: Some(Rgba { data: [0, 0, 0, 255] }),
                    fill: Some(Rgba { data: [255, 0, 0, 255] }),
                }
            }]
        }
    }
}

mod resvg_to_lyon {

    use resvg::tree::{self, Color, Paint, Stroke, PathSegment};

    use lyon::{
        path::PathEvent,
        tessellation::{self, StrokeOptions},
        math::Point,
    };

    fn point(x: f64, y: f64) -> Point {
        Point::new(x as f32, y as f32)
    }

    pub const FALLBACK_COLOR: Color = Color { red: 0, green: 0, blue: 0 };

    pub(super) fn as_event(ps: &PathSegment) -> PathEvent {
        match *ps {
            PathSegment::MoveTo { x, y } => PathEvent::MoveTo(point(x, y)),
            PathSegment::LineTo { x, y } => PathEvent::LineTo(point(x, y)),
            PathSegment::CurveTo { x1, y1, x2, y2, x, y, } => {
                PathEvent::CubicTo(point(x1, y1), point(x2, y2), point(x, y))
            }
            PathSegment::ClosePath => PathEvent::Close,
        }
    }

    pub(super) fn convert_stroke(s: &Stroke) -> (Color, StrokeOptions) {
        let color = match s.paint {
            Paint::Color(c) => c,
            _ => FALLBACK_COLOR,
        };
        let linecap = match s.linecap {
            tree::LineCap::Butt => tessellation::LineCap::Butt,
            tree::LineCap::Square => tessellation::LineCap::Square,
            tree::LineCap::Round => tessellation::LineCap::Round,
        };
        let linejoin = match s.linejoin {
            tree::LineJoin::Miter => tessellation::LineJoin::Miter,
            tree::LineJoin::Bevel => tessellation::LineJoin::Bevel,
            tree::LineJoin::Round => tessellation::LineJoin::Round,
        };

        let opt = StrokeOptions::tolerance(0.01)
            .with_line_width(s.width as f32)
            .with_line_cap(linecap)
            .with_line_join(linejoin);

        (color, opt)
    }
}
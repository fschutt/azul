use dom::Callback;
use image::Rgba;
use traits::LayoutScreen;
use std::sync::atomic::AtomicUsize;

/// In order to store / compare SVG files, we have to
pub(crate) static mut SVG_BLOB_ID: AtomicUsize = AtomicUsize::new(0);



#[derive(Debug, Clone)]
pub struct Svg<T: LayoutScreen> {
    pub layers: Vec<SvgLayer<T>>,
}

#[derive(Debug, Clone)]
pub struct SvgLayer<T: LayoutScreen> {
    pub id: String,
    pub data: Vec<SvgShape>,
    pub callbacks: SvgCallbacks<T>,
    pub style: SvgStyle,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SvgCallbacks<T: LayoutScreen> {
    // No callbacks for this layer
    None,
    /// Call the callback on any of the items
    Any(Callback<T>),
    /// Call the callback when the SvgLayer item at index [x] is
    ///  hovered over / interacted with
    Some(Vec<(usize, Callback<T>)>),
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SvgStyle {
    outline: Option<Rgba<f32>>,
    fill: Option<Rgba<f32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SvgShape {
    Polygon(Vec<(f32, f32)>),
}


impl<T: LayoutScreen> Svg<T> {
    pub fn default_testing() -> Self {
        Self {
            layers: vec![SvgLayer {
                id: String::from("svg-layer-01"),
                // simple triangle for testing
                data: vec![SvgShape::Polygon(vec![(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)])],
                callbacks: SvgCallbacks::None,
                style: SvgStyle {
                    outline: Some(Rgba { data: [0.0, 0.0, 0.0, 1.0] }),
                    fill: Some(Rgba { data: [1.0, 0.0, 0.0, 1.0] }),
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
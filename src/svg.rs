use dom::Callback;
use image::Rgb;
use traits::LayoutScreen;

pub struct Svg<T: LayoutScreen> {
    pub layers: Vec<SvgLayer<T>>,
}

pub struct SvgLayer<T: LayoutScreen> {
    pub id: String,
    pub data: Vec<SvgShape>,
    pub callbacks: SvgCallbacks<T>,
    pub style: SvgStyle,
}

pub enum SvgCallbacks<T: LayoutScreen> {
    // No callbacks for this layer
    None,
    /// Call the callback on any of the items
    Any(Callback<T>),
    /// Call the callback when the SvgLayer item at index [x] is
    ///  hovered over / interacted with
    Some(Vec<(usize, Callback<T>)>),
}

pub struct SvgStyle {
    outline: Option<Rgb<f32>>,
    fill: Option<Rgb<f32>>,
}

pub enum SvgShape {
    Polygon(Vec<(f32, f32)>),
}
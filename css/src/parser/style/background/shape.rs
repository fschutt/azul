use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum Shape {
    Ellipse,
    Circle,
}

impl Default for Shape {
    fn default() -> Self {
        Shape::Ellipse
    }
}

multi_type_parser!(parse_shape, Shape, ["circle", Circle], ["ellipse", Ellipse]);

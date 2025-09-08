use crate::{css_properties::*, parser::*};

/// Represents a `border-top-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderTopWidth {
    pub inner: PixelValue,
}
/// Represents a `border-left-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderLeftWidth {
    pub inner: PixelValue,
}
/// Represents a `border-right-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderRightWidth {
    pub inner: PixelValue,
}
/// Represents a `border-bottom-width` attribute
#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct LayoutBorderBottomWidth {
    pub inner: PixelValue,
}

impl_pixel_value!(LayoutBorderTopWidth);
impl_pixel_value!(LayoutBorderLeftWidth);
impl_pixel_value!(LayoutBorderRightWidth);
impl_pixel_value!(LayoutBorderBottomWidth);

typed_pixel_value_parser!(parse_style_border_top_width, LayoutBorderTopWidth);
typed_pixel_value_parser!(parse_style_border_bottom_width, LayoutBorderBottomWidth);
typed_pixel_value_parser!(parse_style_border_right_width, LayoutBorderRightWidth);
typed_pixel_value_parser!(parse_style_border_left_width, LayoutBorderLeftWidth);

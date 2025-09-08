use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFloat {
    Left,
    Right,
    None,
}

impl Default for LayoutFloat {
    fn default() -> Self {
        LayoutFloat::Left
    }
}

multi_type_parser!(
    parse_layout_float,
    LayoutFloat,
    ["left", Left],
    ["right", Right],
    ["none", None]
);

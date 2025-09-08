use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutPosition {
    Static,
    Relative,
    Absolute,
    Fixed,
}

impl LayoutPosition {
    pub fn is_positioned(&self) -> bool {
        *self != LayoutPosition::Static
    }
}

impl Default for LayoutPosition {
    fn default() -> Self {
        LayoutPosition::Static
    }
}

multi_type_parser!(
    parse_layout_position,
    LayoutPosition,
    ["static", Static],
    ["fixed", Fixed],
    ["absolute", Absolute],
    ["relative", Relative]
);

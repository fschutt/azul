use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleBackfaceVisibility {
    Hidden,
    Visible,
}

impl Default for StyleBackfaceVisibility {
    fn default() -> Self {
        StyleBackfaceVisibility::Visible
    }
}

multi_type_parser!(
    parse_style_backface_visibility,
    StyleBackfaceVisibility,
    ["hidden", Hidden],
    ["visible", Visible]
);

use crate::{css_properties::*, impl_option, parser::*};

/// Force text direction: default - `Ltr`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleDirection {
    Ltr,
    Rtl,
}

impl Default for StyleDirection {
    fn default() -> Self {
        StyleDirection::Ltr
    }
}

impl_option!(
    StyleDirection,
    OptionStyleDirection,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

multi_type_parser!(
    parse_style_direction,
    StyleDirection,
    ["ltr", Ltr],
    ["rtl", Rtl]
);

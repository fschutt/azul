use crate::{css_properties::*, impl_option, parser::*};

/// Force text hyphens: default - `Ltr`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleWhiteSpace {
    Normal,
    Pre,
    Nowrap,
}

impl Default for StyleWhiteSpace {
    fn default() -> Self {
        StyleWhiteSpace::Normal
    }
}

impl_option!(
    StyleWhiteSpace,
    OptionStyleWhiteSpace,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

multi_type_parser!(
    parse_style_white_space,
    StyleWhiteSpace,
    ["normal", Normal],
    ["pre", Pre],
    ["nowrap", Nowrap]
);

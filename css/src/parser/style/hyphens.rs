use crate::{css_properties::*, impl_option, parser::*};

/// Force text hyphens: default - `Ltr`
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleHyphens {
    Auto,
    None,
}

impl Default for StyleHyphens {
    fn default() -> Self {
        StyleHyphens::Auto
    }
}

impl_option!(
    StyleHyphens,
    OptionStyleHyphens,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

multi_type_parser!(
    parse_style_hyphens,
    StyleHyphens,
    ["auto", Auto],
    ["none", None]
);

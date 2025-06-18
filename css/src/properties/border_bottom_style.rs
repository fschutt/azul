//! `border-bottom-style` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{BorderStyle, derive_debug_zero, derive_display_zero};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderBottomStyle {
    pub inner: BorderStyle,
}

derive_debug_zero!(StyleBorderBottomStyle);
derive_display_zero!(StyleBorderBottomStyle);

pub type StyleBorderBottomStyleValue = CssPropertyValue<StyleBorderBottomStyle>;

impl_option!(
    StyleBorderBottomStyle,
    OptionStyleBorderBottomStyle,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

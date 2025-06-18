//! `border-left-style` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{BorderStyle, derive_debug_zero, derive_display_zero};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderLeftStyle {
    pub inner: BorderStyle,
}

derive_debug_zero!(StyleBorderLeftStyle);
derive_display_zero!(StyleBorderLeftStyle);

pub type StyleBorderLeftStyleValue = CssPropertyValue<StyleBorderLeftStyle>;

impl_option!(
    StyleBorderLeftStyle,
    OptionStyleBorderLeftStyle,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

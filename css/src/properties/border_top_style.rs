//! `border-top-style` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{BorderStyle, derive_debug_zero, derive_display_zero};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderTopStyle {
    pub inner: BorderStyle,
}

derive_debug_zero!(StyleBorderTopStyle);
derive_display_zero!(StyleBorderTopStyle);

pub type StyleBorderTopStyleValue = CssPropertyValue<StyleBorderTopStyle>;

impl_option!(
    StyleBorderTopStyle,
    OptionStyleBorderTopStyle,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

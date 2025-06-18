//! `border-right-style` CSS property

use crate::css::CssPropertyValue;
use crate::css_properties::{BorderStyle, derive_debug_zero, derive_display_zero};
use crate::{impl_option};

#[derive(Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StyleBorderRightStyle {
    pub inner: BorderStyle,
}

derive_debug_zero!(StyleBorderRightStyle);
derive_display_zero!(StyleBorderRightStyle);

pub type StyleBorderRightStyleValue = CssPropertyValue<StyleBorderRightStyle>;

impl_option!(
    StyleBorderRightStyle,
    OptionStyleBorderRightStyle,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

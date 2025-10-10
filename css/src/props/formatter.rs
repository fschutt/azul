//! Trait and implementations for formatting CSS properties back into strings.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

// Re-export the PrintAsCssValue trait from the css module
pub use crate::css::PrintAsCssValue;
use crate::props::{
    basic::{
        angle::AngleValue,
        color::ColorU,
        direction::{Direction, DirectionCorner},
        font::*,
        length::{FloatValue, PercentageValue},
        pixel::PixelValue,
    },
    layout::{dimensions::*, display::*, flex::*, overflow::*, position::*, spacing::*},
    style::{
        background::*,
        border::*,
        border_radius::*,
        box_shadow::{BoxShadowClipMode, StyleBoxShadow},
        effects::*,
        filter::*,
        scrollbar::*,
        text::*,
        transform::*,
    },
};

pub trait FormatAsCssValue {
    fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result;
}

// --- Style Properties ---

impl PrintAsCssValue for StyleBorderTopLeftRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderTopRightRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderBottomLeftRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderBottomRightRadius {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleBorderTopStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderRightStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderBottomStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for StyleBorderLeftStyle {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for StyleBorderTopColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}
impl PrintAsCssValue for StyleBorderRightColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}
impl PrintAsCssValue for StyleBorderBottomColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}
impl PrintAsCssValue for StyleBorderLeftColor {
    fn print_as_css_value(&self) -> String {
        self.inner.to_hash()
    }
}

impl PrintAsCssValue for LayoutBorderTopWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutBorderRightWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutBorderBottomWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutBorderLeftWidth {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

// --- Layout Spacing Properties ---

impl PrintAsCssValue for LayoutPaddingTop {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutPaddingLeft {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutPaddingRight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutPaddingBottom {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

impl PrintAsCssValue for LayoutMarginTop {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMarginLeft {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMarginRight {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}
impl PrintAsCssValue for LayoutMarginBottom {
    fn print_as_css_value(&self) -> String {
        format!("{}", self.inner)
    }
}

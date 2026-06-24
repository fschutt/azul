//! Trait and implementations for formatting CSS properties back into strings.
//!
//! This module defines `FormatAsCssValue` (zero-alloc, `fmt::Formatter`-based)
//! and re-exports `PrintAsCssValue` (from `css.rs`, returns `String`).
//! `PrintAsCssValue` impls for border, padding, margin, and gap types live here.

use alloc::string::String;
use core::fmt;

// Re-export the PrintAsCssValue trait from the css module
pub use crate::css::PrintAsCssValue;
// wildcard imports: this formatter pulls in every layout/style value type it
// renders; enumerating them all explicitly is unmaintainable.
#[allow(clippy::wildcard_imports)]
use crate::props::{
    layout::{dimensions::*, spacing::*},
    style::{
        border::*,
        border_radius::*,
    },
};

/// Zero-allocation CSS value formatting trait using `fmt::Formatter`.
///
/// Unlike `PrintAsCssValue` (which returns a `String`), this trait writes
/// directly into a formatter and is suitable for `Display` impl delegation.
pub trait FormatAsCssValue {
    fn format_as_css_value(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

macro_rules! impl_print_as_css_display {
    ($($t:ty),+ $(,)?) => {
        $(impl PrintAsCssValue for $t {
            fn print_as_css_value(&self) -> String {
                format!("{}", self.inner)
            }
        })+
    };
}

macro_rules! impl_print_as_css_hash {
    ($($t:ty),+ $(,)?) => {
        $(impl PrintAsCssValue for $t {
            fn print_as_css_value(&self) -> String {
                self.inner.to_hash()
            }
        })+
    };
}

// --- Style Properties ---

impl_print_as_css_display!(
    StyleBorderTopLeftRadius,
    StyleBorderTopRightRadius,
    StyleBorderBottomLeftRadius,
    StyleBorderBottomRightRadius,
    StyleBorderTopStyle,
    StyleBorderRightStyle,
    StyleBorderBottomStyle,
    StyleBorderLeftStyle,
    LayoutBorderTopWidth,
    LayoutBorderRightWidth,
    LayoutBorderBottomWidth,
    LayoutBorderLeftWidth,
);

impl_print_as_css_hash!(
    StyleBorderTopColor,
    StyleBorderRightColor,
    StyleBorderBottomColor,
    StyleBorderLeftColor,
);

// --- Layout Spacing Properties ---

impl_print_as_css_display!(
    LayoutPaddingTop,
    LayoutPaddingLeft,
    LayoutPaddingRight,
    LayoutPaddingBottom,
    LayoutPaddingInlineStart,
    LayoutPaddingInlineEnd,
    LayoutMarginTop,
    LayoutMarginLeft,
    LayoutMarginRight,
    LayoutMarginBottom,
    LayoutColumnGap,
    LayoutRowGap,
);

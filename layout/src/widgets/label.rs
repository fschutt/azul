//! Label widget for displaying static text with platform-specific default styling.

use azul_core::dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

/// A static text label widget with platform-appropriate default styling.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Label {
    pub string: AzString,
    pub label_style: CssPropertyWithConditionsVec,
}

const SANS_SERIF_STR: &str = "system:ui";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

/// Standard label text color (#4C4C4C), matching platform UI defaults.
const COLOR_4C4C4C: ColorU = ColorU {
    r: 76,
    g: 76,
    b: 76,
    a: 255,
};

static LABEL_STYLE_DEFAULT: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

static LABEL_STYLE_MAC: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

/// No default styling on unsupported platforms (e.g. WASM, FreeBSD);
/// callers should provide explicit styles via `label_style`.
static LABEL_STYLE_OTHER: &[CssPropertyWithConditions] = &[];

impl Label {
    /// Creates a new label with the given text and platform-specific default styling.
    #[inline]
    #[must_use] pub fn create(string: AzString) -> Self {
        Self {
            string,
            #[cfg(target_os = "windows")]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_DEFAULT),
            #[cfg(target_os = "linux")]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_DEFAULT),
            #[cfg(target_os = "macos")]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_MAC),
            #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
            label_style: CssPropertyWithConditionsVec::from_const_slice(LABEL_STYLE_OTHER),
        }
    }

    /// Replaces `self` with an empty default label, returning the original.
    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this label into a DOM text node with the `__azul-native-label` class.
    #[inline]
    #[must_use] pub fn dom(self) -> Dom {
        static LABEL_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-label"))];

        Dom::create_text(self.string)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(LABEL_CLASS))
            .with_css_props(self.label_style)
    }
}

impl From<Label> for Dom {
    fn from(l: Label) -> Self {
        l.dom()
    }
}

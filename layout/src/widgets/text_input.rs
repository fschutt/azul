//! Text input (demonstrates two-way data binding)

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::Dom,
    refany::RefAny,
    task::OptionTimerId,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};
use azul_css::css::BoxOrStatic;

use crate::callbacks::{Callback, CallbackInfo};

const BACKGROUND_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
}; // white
const BLACK: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};
const TEXT_COLOR: StyleTextColor = StyleTextColor { inner: BLACK }; // black
const COLOR_9B9B9B: ColorU = ColorU {
    r: 155,
    g: 155,
    b: 155,
    a: 255,
}; // #9b9b9b
const COLOR_4286F4: ColorU = ColorU {
    r: 66,
    g: 134,
    b: 244,
    a: 255,
}; // #4286f4
const COLOR_4C4C4C: ColorU = ColorU {
    r: 76,
    g: 76,
    b: 76,
    a: 255,
}; // #4C4C4C

const CURSOR_COLOR_BLACK: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(BLACK)];
const CURSOR_COLOR: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(CURSOR_COLOR_BLACK);

const BACKGROUND_THEME_LIGHT: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(BACKGROUND_COLOR)];
const BACKGROUND_COLOR_LIGHT: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_LIGHT);

const SANS_SERIF_STR: &str = "sans-serif";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

// -- cursor style

const TEXT_CURSOR_TRANSFORM: &[StyleTransform] =
    &[StyleTransform::Translate(StyleTransformTranslate2D {
        x: PixelValue::const_px(0),
        y: PixelValue::const_px(2),
    })];

static TEXT_CURSOR_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(1))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(CURSOR_COLOR)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_transform(
        StyleTransformVec::from_const_slice(TEXT_CURSOR_TRANSFORM),
    )),
];

// -- container style

#[cfg(target_os = "windows")]
static TEXT_INPUT_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        BACKGROUND_COLOR_LIGHT,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        1,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(1),
    )),
    // border: 1px solid #484c52;
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    // Hover(border-color: #4c4c4c;)
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4C4C4C,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: COLOR_4C4C4C,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4C4C4C,
        },
    )),
    // Focus(border-color: #4286f4;)
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4286F4,
        },
    )),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        BACKGROUND_COLOR_LIGHT,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        1,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(1),
    )),
    // border: 1px solid #484c52;
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    // Hover(border-color: #4286f4;)
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4286F4,
        },
    )),
    // Focus(border-color: #4286f4;)
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4286F4,
        },
    )),
];

#[cfg(target_os = "macos")]
static TEXT_INPUT_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        BACKGROUND_COLOR_LIGHT,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(2),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        1,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(1),
    )),
    // border: 1px solid #484c52;
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle {
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Inset,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Inset,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_9B9B9B,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    // Hover(border-color: #4286f4;)
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4286F4,
        },
    )),
    // Focus(border-color: #4286f4;)
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(
        StyleBorderLeftColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4286F4,
        },
    )),
];

// -- label style

#[cfg(target_os = "windows")]
static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(target_os = "macos")]
static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

// --- placeholder

#[cfg(target_os = "windows")]
static TEXT_INPUT_PLACEHOLDER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_PLACEHOLDER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

#[cfg(target_os = "macos")]
static TEXT_INPUT_PLACEHOLDER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextInput {
    pub text_input_state: TextInputStateWrapper,
    pub placeholder_style: CssPropertyWithConditionsVec,
    pub container_style: CssPropertyWithConditionsVec,
    pub label_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextInputState {
    pub text: U32Vec, // Vec<char>
    pub placeholder: OptionString,
    pub max_len: usize,
    pub selection: OptionTextInputSelection,
    pub cursor_pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextInputStateWrapper {
    pub inner: TextInputState,
    pub on_text_input: OptionTextInputOnTextInput,
    pub on_virtual_key_down: OptionTextInputOnVirtualKeyDown,
    pub on_focus_lost: OptionTextInputOnFocusLost,
    pub update_text_input_before_calling_focus_lost_fn: bool,
    pub update_text_input_before_calling_vk_down_fn: bool,
    pub cursor_animation: OptionTimerId,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct OnTextInputReturn {
    pub update: Update,
    pub valid: TextInputValid,
}

#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub enum TextInputValid {
    Yes,
    No,
}

// The text input field has a special return which specifies
// whether the text input should handle the character
pub type TextInputOnTextInputCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextInputState) -> OnTextInputReturn;
impl_widget_callback!(
    TextInputOnTextInput,
    OptionTextInputOnTextInput,
    TextInputOnTextInputCallback,
    TextInputOnTextInputCallbackType
);

pub type TextInputOnVirtualKeyDownCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextInputState) -> OnTextInputReturn;
impl_widget_callback!(
    TextInputOnVirtualKeyDown,
    OptionTextInputOnVirtualKeyDown,
    TextInputOnVirtualKeyDownCallback,
    TextInputOnVirtualKeyDownCallbackType
);

pub type TextInputOnFocusLostCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextInputState) -> Update;
impl_widget_callback!(
    TextInputOnFocusLost,
    OptionTextInputOnFocusLost,
    TextInputOnFocusLostCallback,
    TextInputOnFocusLostCallbackType
);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C, u8)]
pub enum TextInputSelection {
    All,
    FromTo(TextInputSelectionRange),
}

azul_css::impl_option!(
    TextInputSelection,
    OptionTextInputSelection,
    copy = false,
    [Debug, Clone, Hash, PartialEq, Eq]
);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct TextInputSelectionRange {
    pub dir_from: usize,
    pub dir_to: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        TextInput {
            text_input_state: TextInputStateWrapper::default(),
            placeholder_style: CssPropertyWithConditionsVec::from_const_slice(
                TEXT_INPUT_PLACEHOLDER_PROPS,
            ),
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                TEXT_INPUT_CONTAINER_PROPS,
            ),
            label_style: CssPropertyWithConditionsVec::from_const_slice(TEXT_INPUT_LABEL_PROPS),
        }
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        TextInputState {
            text: Vec::new().into(),
            placeholder: None.into(),
            max_len: 50,
            selection: None.into(),
            cursor_pos: 0,
        }
    }
}

impl TextInputState {
    pub fn get_text(&self) -> String {
        self.text
            .iter()
            .filter_map(|c| core::char::from_u32(*c))
            .collect()
    }
}

impl Default for TextInputStateWrapper {
    fn default() -> Self {
        TextInputStateWrapper {
            inner: TextInputState::default(),
            on_text_input: None.into(),
            on_virtual_key_down: None.into(),
            on_focus_lost: None.into(),
            update_text_input_before_calling_focus_lost_fn: true,
            update_text_input_before_calling_vk_down_fn: true,
            cursor_animation: None.into(),
        }
    }
}

impl TextInput {
    pub fn create() -> Self {
        Self::default()
    }

    pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    pub fn set_text(&mut self, text: AzString) {
        self.text_input_state.inner.text = text
            .as_str()
            .chars()
            .map(|c| c as u32)
            .collect::<Vec<_>>()
            .into();
    }

    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.text_input_state.inner.placeholder = Some(placeholder).into();
    }

    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    pub fn set_on_text_input<C: Into<TextInputOnTextInputCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_input_state.on_text_input = Some(TextInputOnTextInput {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    pub fn with_on_text_input<C: Into<TextInputOnTextInputCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_text_input(refany, callback);
        self
    }

    pub fn set_on_virtual_key_down<C: Into<TextInputOnVirtualKeyDownCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_input_state.on_virtual_key_down = Some(TextInputOnVirtualKeyDown {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    pub fn with_on_virtual_key_down<C: Into<TextInputOnVirtualKeyDownCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_virtual_key_down(refany, callback);
        self
    }

    pub fn set_on_focus_lost<C: Into<TextInputOnFocusLostCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_input_state.on_focus_lost = Some(TextInputOnFocusLost {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    pub fn with_on_focus_lost<C: Into<TextInputOnFocusLostCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_focus_lost(refany, callback);
        self
    }

    pub fn set_placeholder_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.placeholder_style = style;
    }

    pub fn with_placeholder_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_placeholder_style(style);
        self
    }

    pub fn set_container_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.container_style = style;
    }

    pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    pub fn set_label_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.label_style = style;
    }

    pub fn with_label_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_label_style(style);
        self
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    pub fn dom(mut self) -> Dom {
        use azul_core::{
            callbacks::CoreCallbackData,
            dom::{EventFilter, FocusEventFilter, HoverEventFilter, IdOrClass::Class, TabIndex},
        };

        self.text_input_state.inner.cursor_pos = self.text_input_state.inner.text.len();

        let label_text: String = self
            .text_input_state
            .inner
            .text
            .iter()
            .filter_map(|s| core::char::from_u32(*s))
            .collect();

        let placeholder = self
            .text_input_state
            .inner
            .placeholder
            .as_ref()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();

        let state_ref = RefAny::new(self.text_input_state);

        Dom::create_div()
            .with_ids_and_classes(vec![Class("__azul-native-text-input-container".into())].into())
            .with_css_props(self.container_style)
            .with_tab_index(TabIndex::Auto)
            .with_dataset(Some(state_ref.clone()).into())
            .with_callbacks(
                vec![
                    CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                        refany: state_ref.clone(),
                        callback: CoreCallback {
                            cb: default_on_focus_received as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    },
                    CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::FocusLost),
                        refany: state_ref.clone(),
                        callback: CoreCallback {
                            cb: default_on_focus_lost as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    },
                    CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::TextInput),
                        refany: state_ref.clone(),
                        callback: CoreCallback {
                            cb: default_on_text_input as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    },
                    CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                        refany: state_ref.clone(),
                        callback: CoreCallback {
                            cb: default_on_virtual_key_down as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    },
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseOver),
                        refany: state_ref.clone(),
                        callback: CoreCallback {
                            cb: default_on_mouse_hover as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    },
                ]
                .into(),
            )
            .with_children(
                vec![
                    Dom::create_text(placeholder)
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-input-placeholder".into())].into(),
                        )
                        .with_css_props(self.placeholder_style),
                    Dom::create_text(label_text)
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-input-label".into())].into(),
                        )
                        .with_css_props(self.label_style)
                        .with_children(
                            vec![Dom::create_div()
                                .with_ids_and_classes(
                                    vec![Class("__azul-native-text-input-cursor".into())].into(),
                                )
                                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                                    TEXT_CURSOR_PROPS,
                                ))]
                            .into(),
                        ),
                ]
                .into(),
            )
    }
}

extern "C" fn default_on_focus_received(mut text_input: RefAny, mut info: CallbackInfo) -> Update {
    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let text_input = &mut *text_input;

    let placeholder_text_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // hide the placeholder text
    if text_input.inner.text.is_empty() {
        info.set_css_property(
            placeholder_text_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(0)),
        );
    }

    text_input.inner.cursor_pos = text_input.inner.text.len();

    Update::DoNothing
}

extern "C" fn default_on_focus_lost(mut text_input: RefAny, mut info: CallbackInfo) -> Update {
    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let text_input = &mut *text_input;

    let placeholder_text_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // show the placeholder text
    if text_input.inner.text.is_empty() {
        info.set_css_property(
            placeholder_text_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(100)),
        );
    }

    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let onfocuslost = &mut text_input.on_focus_lost;
        let inner = text_input.inner.clone();

        match onfocuslost.as_mut() {
            Some(TextInputOnFocusLost { callback, refany }) => {
                (callback.cb)(refany.clone(), info.clone(), inner)
            }
            None => Update::DoNothing,
        }
    };

    result
}

extern "C" fn default_on_text_input(text_input: RefAny, info: CallbackInfo) -> Update {
    default_on_text_input_inner(text_input, info).unwrap_or(Update::DoNothing)
}

fn default_on_text_input_inner(mut text_input: RefAny, mut info: CallbackInfo) -> Option<Update> {
    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;

    // Get the text changeset (replaces old keyboard_state.current_char API)
    let changeset = info.get_text_changeset()?;
    let inserted_text = changeset.inserted_text.as_str().to_string();

    // Early return if no text to insert
    if inserted_text.is_empty() {
        return None;
    }

    let placeholder_node_id = info.get_first_child(info.get_hit_node())?;
    let label_node_id = info.get_next_sibling(placeholder_node_id)?;
    let _cursor_node_id = info.get_first_child(label_node_id)?;

    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let ontextinput = &mut text_input.on_text_input;

        // inner_clone has the new text
        let mut inner_clone = text_input.inner.clone();
        inner_clone.cursor_pos = inner_clone.cursor_pos.saturating_add(inserted_text.len());
        inner_clone.text = {
            let mut internal = inner_clone.text.clone().into_library_owned_vec();
            internal.extend(inserted_text.chars().map(|c| c as u32));
            internal.into()
        };

        match ontextinput.as_mut() {
            Some(TextInputOnTextInput { callback, refany }) => {
                (callback.cb)(refany.clone(), info.clone(), inner_clone)
            }
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };

    if result.valid == TextInputValid::Yes {
        // hide the placeholder text
        info.set_css_property(
            placeholder_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(0)),
        );

        // append to the text
        text_input.inner.text = {
            let mut internal = text_input.inner.text.clone().into_library_owned_vec();
            internal.extend(inserted_text.chars().map(|c| c as u32));
            internal.into()
        };
        text_input.inner.cursor_pos = text_input
            .inner
            .cursor_pos
            .saturating_add(inserted_text.len());

        info.change_node_text(label_node_id, text_input.inner.get_text().into());
    }

    Some(result.update)
}

extern "C" fn default_on_virtual_key_down(text_input: RefAny, info: CallbackInfo) -> Update {
    default_on_virtual_key_down_inner(text_input, info).unwrap_or(Update::DoNothing)
}

fn default_on_virtual_key_down_inner(
    mut text_input: RefAny,
    mut info: CallbackInfo,
) -> Option<Update> {
    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;
    let keyboard_state = info.get_current_keyboard_state();

    let c = keyboard_state.current_virtual_keycode.into_option()?;
    let placeholder_node_id = info.get_first_child(info.get_hit_node())?;
    let label_node_id = info.get_next_sibling(placeholder_node_id)?;
    let _cursor_node_id = info.get_first_child(label_node_id)?;

    if c != VirtualKeyCode::Back {
        return None;
    }

    text_input.inner.text = {
        let mut internal = text_input.inner.text.clone().into_library_owned_vec();
        internal.pop();
        internal.into()
    };
    text_input.inner.cursor_pos = text_input.inner.cursor_pos.saturating_sub(1);

    info.change_node_text(label_node_id, text_input.inner.get_text().into());

    None
}

extern "C" fn default_on_mouse_hover(mut text_input: RefAny, _info: CallbackInfo) -> Update {
    let _text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // println!("default_on_mouse_hover");

    Update::DoNothing
}

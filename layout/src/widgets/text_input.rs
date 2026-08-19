//! Single-line text input widget with placeholder and two-way data binding.
//!
//! The main entry point is [`TextInput`], which holds the editable state
//! ([`TextInputState`]) together with per-platform default styles.  Call
//! [`TextInput::dom()`] to obtain a renderable [`Dom`] node.
//!
//! The widget is a `contenteditable` host: the container carries the flag and
//! the tab index, so the engine's `TextEditManager` owns the caret, the
//! selection and the buffer. Caret and selection are display-list items driven
//! by that manager — the widget contributes no cursor node — and edits run
//! through `record_text_input` / `apply_text_changeset`. [`TextInputState`] is a
//! *mirror* of that state, refreshed from the engine's changesets so the public
//! callbacks keep the shape existing hosts bind against.
//!
//! Both the value and the placeholder are `<p>` blocks wrapping a bare text
//! node: a [`NodeType::Text`](azul_core::dom::NodeType::Text) node is always
//! inline-level and owns no rect, so box-model properties on one are inert and
//! nothing bounds or clips the line.
//!
//! For higher-level text-input management (IME, clipboard, undo) see
//! `layout/src/managers/text_input.rs`.

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomNodeId},
    refany::RefAny,
    task::OptionTimerId,
};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
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

const BACKGROUND_THEME_LIGHT: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(BACKGROUND_COLOR)];
const BACKGROUND_COLOR_LIGHT: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_LIGHT);

const SANS_SERIF_STR: &str = "system:ui";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

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

// Mobile (Android / iOS) inherit the macOS-style container — same flex
// box-sizing and background; touch-target padding is the user's concern.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
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
//
// The label is the `<p>` block wrapping the value text, so it is the box that
// bounds the line and clips against the container's `overflow: hidden`.
// `white-space: pre` keeps a single-line field on one line and preserves the
// spaces the user typed.

#[cfg(target_os = "windows")]
static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Pre,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Pre,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Pre,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

// --- placeholder
//
// An absolutely-positioned `<p>` overlay inside the editable container. It is
// marked `contenteditable="false"` so the engine's inheritance walk stops at it
// and the prompt never becomes part of the buffer, and it is toggled with
// `display` as well as `opacity`: a hidden-but-laid-out overlay would still own
// the container's first inline layout, which is what
// `LayoutWindow::reshape_text_node` picks up when it looks for the IFC to write
// an edit into.

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

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
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

/// Single-line text input widget with platform-native styling.
///
/// Use [`TextInput::create()`] to build an instance, configure it with the
/// `with_*` / `set_*` builder methods, and call [`TextInput::dom()`] to
/// obtain a renderable DOM tree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TextInput {
    pub text_input_state: TextInputStateWrapper,
    pub placeholder_style: CssPropertyWithConditionsVec,
    pub container_style: CssPropertyWithConditionsVec,
    pub label_style: CssPropertyWithConditionsVec,
}

/// Editable state of a text input (text buffer, cursor position, selection).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TextInputState {
    pub text: U32Vec, // Vec<char>
    pub placeholder: OptionString,
    pub max_len: usize,
    pub selection: OptionTextInputSelection,
    pub cursor_pos: usize,
}

/// [`TextInputState`] together with optional user callbacks and cursor animation state.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Return value from a text-input callback indicating whether the framework
/// should update and whether the input was valid.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct OnTextInputReturn {
    pub update: Update,
    pub valid: TextInputValid,
}

/// Whether the text input accepted or rejected the most recent edit.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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

azul_core::impl_managed_callback! {
    wrapper:        TextInputOnTextInputCallback,
    info_ty:        CallbackInfo,
    return_ty:      OnTextInputReturn,
    default_ret:    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes },
    invoker_static: TEXT_INPUT_ON_TEXT_INPUT_INVOKER,
    invoker_ty:     AzTextInputOnTextInputCallbackInvoker,
    thunk_fn:       az_text_input_on_text_input_callback_thunk,
    setter_fn:      AzApp_setTextInputOnTextInputCallbackInvoker,
    from_handle_fn: AzTextInputOnTextInputCallback_createFromHostHandle,
    extra_args:     [ state: TextInputState ],
}

pub type TextInputOnVirtualKeyDownCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextInputState) -> OnTextInputReturn;
impl_widget_callback!(
    TextInputOnVirtualKeyDown,
    OptionTextInputOnVirtualKeyDown,
    TextInputOnVirtualKeyDownCallback,
    TextInputOnVirtualKeyDownCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TextInputOnVirtualKeyDownCallback,
    info_ty:        CallbackInfo,
    return_ty:      OnTextInputReturn,
    default_ret:    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes },
    invoker_static: TEXT_INPUT_ON_VIRTUAL_KEY_DOWN_INVOKER,
    invoker_ty:     AzTextInputOnVirtualKeyDownCallbackInvoker,
    thunk_fn:       az_text_input_on_virtual_key_down_callback_thunk,
    setter_fn:      AzApp_setTextInputOnVirtualKeyDownCallbackInvoker,
    from_handle_fn: AzTextInputOnVirtualKeyDownCallback_createFromHostHandle,
    extra_args:     [ state: TextInputState ],
}

pub type TextInputOnFocusLostCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextInputState) -> Update;
impl_widget_callback!(
    TextInputOnFocusLost,
    OptionTextInputOnFocusLost,
    TextInputOnFocusLostCallback,
    TextInputOnFocusLostCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TextInputOnFocusLostCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TEXT_INPUT_ON_FOCUS_LOST_INVOKER,
    invoker_ty:     AzTextInputOnFocusLostCallbackInvoker,
    thunk_fn:       az_text_input_on_focus_lost_callback_thunk,
    setter_fn:      AzApp_setTextInputOnFocusLostCallbackInvoker,
    from_handle_fn: AzTextInputOnFocusLostCallback_createFromHostHandle,
    extra_args:     [ state: TextInputState ],
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[derive(Copy, Debug, Clone, Hash, PartialEq, Eq)]
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

#[derive(Copy, Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct TextInputSelectionRange {
    pub dir_from: usize,
    pub dir_to: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        Self {
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
        Self {
            text: Vec::new().into(),
            placeholder: None.into(),
            max_len: 50,
            selection: None.into(),
            cursor_pos: 0,
        }
    }
}

impl TextInputState {
    #[must_use] pub fn get_text(&self) -> String {
        self.text
            .iter()
            .filter_map(|c| core::char::from_u32(*c))
            .collect()
    }
}

impl Default for TextInputStateWrapper {
    fn default() -> Self {
        Self {
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
    #[must_use] pub fn create() -> Self {
        Self::default()
    }

    #[must_use] pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    // owned AzString passed by value per the azul FFI / api.json setter convention.
    #[allow(clippy::needless_pass_by_value)]
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

    #[must_use] pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
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

    #[must_use]
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

    #[must_use]
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

    #[must_use]
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

    #[must_use] pub fn with_placeholder_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_placeholder_style(style);
        self
    }

    pub fn set_container_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.container_style = style;
    }

    #[must_use] pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    pub fn set_label_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.label_style = style;
    }

    #[must_use] pub fn with_label_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_label_style(style);
        self
    }

    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the widget.
    ///
    /// The container is the `contenteditable` host — the flag, the tab index and
    /// the focus callbacks all sit on it, because focus events do not bubble and
    /// the engine records an edit against the *focused* node. Its two children
    /// are `<p>` blocks wrapping a bare text node each; nothing else is emitted,
    /// in particular no caret node (the engine paints the caret and the
    /// selection from its display list).
    #[must_use] pub fn dom(mut self) -> Dom {
        use azul_core::{
            callbacks::CoreCallbackData,
            dom::{
                AttributeType, DomVec, EventFilter, FocusEventFilter, HoverEventFilter,
                IdOrClass::Class, TabIndex,
            },
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

        let mut placeholder_style = self.placeholder_style;
        if !self.text_input_state.inner.text.is_empty() {
            placeholder_style = hidden_placeholder_style(&placeholder_style);
        }

        let state_ref = RefAny::new(self.text_input_state);

        Dom::create_div()
            .with_ids_and_classes(vec![Class("__azul-native-text-input-container".into())].into())
            .with_css_props(self.container_style)
            .with_tab_index(TabIndex::Auto)
            .with_contenteditable(true)
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
                        refany: state_ref,
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
                    Dom::create_p()
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-input-placeholder".into())].into(),
                        )
                        .with_css_props(placeholder_style)
                        // appended, never `with_attributes`: that one replaces the
                        // whole vector, classes included
                        .with_attribute(AttributeType::ContentEditable(false))
                        .with_children(DomVec::from_vec(vec![Dom::create_text_do_not_use_without_block_level_wrapper(placeholder)])),
                    Dom::create_p()
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-input-label".into())].into(),
                        )
                        .with_css_props(self.label_style)
                        .with_children(DomVec::from_vec(vec![Dom::create_text_do_not_use_without_block_level_wrapper(label_text)])),
                ]
                .into(),
            )
    }
}

/// `style` with the placeholder taken out of the flow: `display: none` on top
/// of `opacity: 0`, so a hidden prompt owns neither pixels nor an inline layout.
fn hidden_placeholder_style(
    style: &CssPropertyWithConditionsVec,
) -> CssPropertyWithConditionsVec {
    let mut props = style.as_ref().to_vec();
    props.push(CssPropertyWithConditions::simple(CssProperty::const_display(
        LayoutDisplay::None,
    )));
    props.push(CssPropertyWithConditions::simple(CssProperty::const_opacity(
        StyleOpacity::const_new(0),
    )));
    CssPropertyWithConditionsVec::from_vec(props)
}

/// The placeholder `<p>` and the value `<p>`, in that order.
///
/// Both handlers and tests resolve them through the same hierarchy hops the
/// container's own layout guarantees; a subtree of any other shape yields
/// `None` and every handler bails out.
fn label_nodes(info: &CallbackInfo) -> Option<(DomNodeId, DomNodeId)> {
    let placeholder = info.get_first_child(info.get_hit_node())?;
    let label = info.get_next_sibling(placeholder)?;
    Some((placeholder, label))
}

/// Shows or hides the placeholder prompt.
fn set_placeholder_visible(info: &mut CallbackInfo, placeholder: DomNodeId, visible: bool) {
    let (display, opacity) = if visible {
        (LayoutDisplay::Block, StyleOpacity::const_new(100))
    } else {
        (LayoutDisplay::None, StyleOpacity::const_new(0))
    };
    info.set_css_property(placeholder, CssProperty::const_opacity(opacity));
    info.set_css_property(placeholder, CssProperty::const_display(display));
}

/// Adopts the engine's text for `node` into the widget's mirror.
///
/// The engine owns the buffer, so its answer wins — except that an empty answer
/// is ambiguous: `get_text_before_textinput` also yields nothing for a node
/// whose text sits under a block wrapper it does not descend into. An empty
/// read therefore never clears a non-empty mirror.
fn adopt_engine_text(state: &mut TextInputState, info: &CallbackInfo, node: DomNodeId) {
    let Some(text) = info.get_node_text_content(node) else {
        return;
    };
    if text.is_empty() && !state.text.is_empty() {
        return;
    }
    state.text = text.chars().map(|c| c as u32).collect::<Vec<_>>().into();
}

/// The engine's selection, in the widget's public shape.
///
/// Offsets are byte offsets into the value, matching the cursor positions the
/// engine reports; a range that spans the whole buffer collapses to
/// [`TextInputSelection::All`].
fn engine_selection(
    info: &CallbackInfo,
    node: DomNodeId,
    len: usize,
) -> Option<TextInputSelection> {
    let ranges = info.get_node_selection_ranges(node);
    let range = *ranges.as_ref().first()?;
    let dir_from = range.start.cluster_id.start_byte_in_run as usize;
    let dir_to = range.end.cluster_id.start_byte_in_run as usize;
    if len != 0 && dir_from == 0 && dir_to >= len {
        return Some(TextInputSelection::All);
    }
    Some(TextInputSelection::FromTo(TextInputSelectionRange {
        dir_from,
        dir_to,
    }))
}

/// Mirrors the insertion the engine is about to apply.
///
/// The engine inserts at the caret, so the mirror does too whenever the caret
/// is readable and lands on a character boundary; otherwise it appends, which
/// is where the caret sits for every append-only path. `cursor_pos` stays a
/// byte offset, as it has always been.
fn mirror_insertion(state: &mut TextInputState, inserted: &str, caret: Option<usize>) {
    let text = state.get_text();
    let at = caret
        .filter(|at| *at <= text.len() && text.is_char_boundary(*at))
        .unwrap_or(text.len());

    let mut next = String::with_capacity(text.len() + inserted.len());
    next.push_str(&text[..at]);
    next.push_str(inserted);
    next.push_str(&text[at..]);

    state.text = next.chars().map(|c| c as u32).collect::<Vec<_>>().into();
    state.cursor_pos = at.saturating_add(inserted.len());
}

/// The caret's byte offset inside the edited node, if the engine has one.
fn engine_caret(info: &CallbackInfo, node: DomNodeId) -> Option<usize> {
    info.get_node_cursor_position(node)
        .map(|c| c.cluster_id.start_byte_in_run as usize)
}

extern "C" fn default_on_focus_received(mut text_input: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut text_input) = text_input.downcast_mut::<TextInputStateWrapper>() else {
        return Update::DoNothing;
    };

    let text_input = &mut *text_input;

    let Some(placeholder_text_node_id) = info.get_first_child(info.get_hit_node()) else {
        return Update::DoNothing;
    };

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_input.inner, &info, container);

    // hide the placeholder text
    if text_input.inner.text.is_empty() {
        set_placeholder_visible(&mut info, placeholder_text_node_id, false);
    }

    // The engine seeds the caret at the end of the value when focus lands on a
    // contenteditable host; the mirror follows it.
    let end_of_text = text_input.inner.text.len();
    text_input.inner.cursor_pos = engine_caret(&info, container).unwrap_or(end_of_text);

    Update::DoNothing
}

extern "C" fn default_on_focus_lost(mut text_input: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut text_input) = text_input.downcast_mut::<TextInputStateWrapper>() else {
        return Update::DoNothing;
    };

    let text_input = &mut *text_input;

    let Some(placeholder_text_node_id) = info.get_first_child(info.get_hit_node()) else {
        return Update::DoNothing;
    };

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_input.inner, &info, container);

    // show the placeholder text
    if text_input.inner.text.is_empty() {
        set_placeholder_visible(&mut info, placeholder_text_node_id, true);
    }

    // rustc doesn't understand the borrowing lifetime here
    let text_input = &mut *text_input;
    let onfocuslost = &mut text_input.on_focus_lost;
    let inner = text_input.inner.clone();

    match onfocuslost.as_mut() {
        Some(TextInputOnFocusLost { callback, refany }) => {
            (callback.cb)(refany.clone(), info, inner)
        }
        None => Update::DoNothing,
    }
}

extern "C" fn default_on_text_input(text_input: RefAny, info: CallbackInfo) -> Update {
    default_on_text_input_inner(text_input, info).unwrap_or(Update::DoNothing)
}

fn default_on_text_input_inner(mut text_input: RefAny, mut info: CallbackInfo) -> Option<Update> {
    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;

    // The engine records the edit before the callbacks run and applies it after
    // them; this handler only observes it and mirrors it into the widget state.
    // An `Input` WITHOUT a pending record is a post-edit NOTIFICATION: an edit
    // committed outside the record pipeline (deletion, programmatic edit) that
    // is already applied — adopt it and inform the user hook; `valid` cannot
    // veto what already happened.
    let inserted_text = info
        .get_text_changeset()
        .map(|c| c.inserted_text.as_str().to_string())
        .unwrap_or_default();

    let (placeholder_node_id, _label_node_id) = label_nodes(&info)?;
    let container = info.get_hit_node();

    if inserted_text.is_empty() {
        // Idempotent: a notification that changed nothing observable (a
        // spurious Input, an edit already mirrored) stays a strict no-op, so
        // the no-changeset pins keep holding.
        let before = text_input.inner.get_text();
        adopt_engine_text(&mut text_input.inner, &info, container);
        if text_input.inner.get_text() == before {
            return None;
        }
        let len = text_input.inner.get_text().len();
        text_input.inner.selection = engine_selection(&info, container, len).into();
        // A field deleted to empty shows its placeholder again.
        set_placeholder_visible(&mut info, placeholder_node_id, len == 0);
        let result = {
            let text_input = &mut *text_input;
            let inner_clone = text_input.inner.clone();
            match text_input.on_text_input.as_mut() {
                Some(TextInputOnTextInput { callback, refany }) => {
                    (callback.cb)(refany.clone(), info, inner_clone)
                }
                None => OnTextInputReturn {
                    update: Update::DoNothing,
                    valid: TextInputValid::Yes,
                },
            }
        };
        return Some(result.update);
    }

    // A single-line field never accepts a line separator: veto insertions
    // carrying one (paste with newlines; the engine's Enter line break is
    // already vetoed in the key handler).
    if inserted_text.contains('\n') {
        info.prevent_default();
        return Some(Update::DoNothing);
    }

    let caret = engine_caret(&info, container);
    adopt_engine_text(&mut text_input.inner, &info, container);

    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let ontextinput = &mut text_input.on_text_input;

        // inner_clone has the new text
        let mut inner_clone = text_input.inner.clone();
        mirror_insertion(&mut inner_clone, &inserted_text, caret);
        let len = inner_clone.get_text().len();
        inner_clone.selection = engine_selection(&info, container, len).into();

        match ontextinput.as_mut() {
            Some(TextInputOnTextInput { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner_clone)
            }
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };

    if result.valid == TextInputValid::Yes {
        // hide the placeholder text
        set_placeholder_visible(&mut info, placeholder_node_id, false);

        mirror_insertion(&mut text_input.inner, &inserted_text, caret);
        let len = text_input.inner.get_text().len();
        text_input.inner.selection = engine_selection(&info, container, len).into();
    } else {
        // The engine applies the recorded changeset once the callbacks return,
        // unless one of them vetoes it.
        info.prevent_default();
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

    let keycode = keyboard_state.current_virtual_keycode.into_option()?;
    let (_placeholder_node_id, _label_node_id) = label_nodes(&info)?;

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_input.inner, &info, container);

    // Editing keys (Backspace, Delete, the arrows, Enter) are the engine's
    // default actions; this handler only forwards the key to the user's hook
    // and lets a rejection stop the default from running.
    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let mut inner_clone = text_input.inner.clone();
        let len = inner_clone.get_text().len();
        inner_clone.selection = engine_selection(&info, container, len).into();
        match text_input.on_virtual_key_down.as_mut() {
            Some(TextInputOnVirtualKeyDown { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner_clone)
            }
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };

    let len = text_input.inner.get_text().len();
    text_input.inner.selection = engine_selection(&info, container, len).into();

    if result.valid == TextInputValid::No {
        info.prevent_default();
    }

    // Single-line field: Enter must never edit the value. The engine-side
    // default in this host (white-space:pre) is a literal "\n" insert, so it
    // is vetoed here; activation semantics stay with the user's hook above.
    if matches!(
        keycode,
        azul_core::window::VirtualKeyCode::Return | azul_core::window::VirtualKeyCode::NumpadEnter
    ) {
        info.prevent_default();
    }

    Some(result.update)
}

extern "C" fn default_on_mouse_hover(mut text_input: RefAny, _info: CallbackInfo) -> Update {
    let Some(_text_input) = text_input.downcast_mut::<TextInputStateWrapper>() else {
        return Update::DoNothing;
    };

    Update::DoNothing
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::too_many_lines, clippy::float_cmp)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{
            AttributeType, DomId, DomNodeId, EventFilter, FocusEventFilter, HoverEventFilter,
            IdOrClass, NodeId, NodeType, TabIndex,
        },
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        managers::text_input::PendingTextEdit,
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ==================================================================
    // Sample data
    // ==================================================================

    /// Strings the buffer has to survive a `set_text` -> `get_text` round-trip on.
    /// Deliberately loaded with cases where "length" is ambiguous: the buffer counts
    /// *scalars*, `str::len()` counts *bytes*, and a human counts *graphemes* — three
    /// numbers that only agree for pure ASCII.
    const HOSTILE: [&str; 20] = [
        "",
        " ",
        "a",
        "hello world",
        "\0",           // a lone NUL is a perfectly good scalar
        "a\0b",         // ... and it survives in the middle of a string, too
        "\u{7f}",       // largest 1-byte scalar
        "\u{80}",       // smallest 2-byte scalar
        "\u{7ff}",      // largest 2-byte scalar
        "\u{800}",      // smallest 3-byte scalar
        "\u{ffff}",     // largest 3-byte scalar (and a non-character)
        "\u{10000}",    // smallest 4-byte scalar
        "\u{10ffff}",   // the largest scalar that exists at all
        "é",
        "e\u{301}",     // e + COMBINING ACUTE: 2 scalars, 1 grapheme
        "👨‍👩‍👧‍👦", // ZWJ family: 7 scalars, 1 grapheme, 25 bytes
        "日本語",
        "مرحبا",        // RTL
        "\u{200b}",     // zero-width space: invisible, but still one scalar
        "\r\n\t",
    ];

    /// `u32` code units that are **not** Unicode scalars, so `char::from_u32` rejects
    /// every one of them. The buffer is a `U32Vec`, so nothing stops them from being
    /// in there — `get_text` is the only thing standing between them and a `String`.
    const NON_SCALAR_UNITS: [u32; 6] = [
        0xD800,      // leading surrogate
        0xDC00,      // trailing surrogate
        0xDFFF,      // last surrogate
        0x0011_0000, // one past the last scalar
        0xFFFF_FFFE,
        u32::MAX,
    ];

    // ==================================================================
    // Widget fixtures
    // ==================================================================

    /// A `TextInput` whose buffer holds *raw* code units — the only way to build a
    /// state that `set_text` could never produce (it goes through `char`).
    fn input_with_units(units: &[u32]) -> TextInput {
        let mut input = TextInput::create();
        input.text_input_state.inner.text = units.to_vec().into();
        input
    }

    /// Renders `input` and hands back both the flattened DOM *and* the very `RefAny`
    /// the widget registered on its own handlers. Driving the handlers with these two
    /// is the real wiring: nothing is rebuilt by hand, so a mismatch between what
    /// `dom()` stores and what the handlers expect cannot hide behind the fixture.
    fn rendered(input: TextInput) -> (StyledDom, RefAny) {
        let dom = input.dom();
        let state = dom.root.callbacks.as_ref()[0].refany.clone();
        (StyledDom::create_from_dom(dom), state)
    }

    /// The `TextInputState` currently sitting behind a widget-state payload.
    fn state_of(state: &RefAny) -> TextInputState {
        let mut state = state.clone();
        let wrapper = state
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the widget state must still be a TextInputStateWrapper");
        wrapper.inner.clone()
    }

    /// Reaches into the live widget state — used to plant a cursor position that
    /// `dom()` would otherwise have already normalised away.
    fn poke(state: &RefAny, f: impl FnOnce(&mut TextInputStateWrapper)) {
        let mut state = state.clone();
        let mut wrapper = state
            .downcast_mut::<TextInputStateWrapper>()
            .expect("the widget state must still be a TextInputStateWrapper");
        f(&mut wrapper);
    }

    // ==================================================================
    // Recording hooks
    // ==================================================================

    /// A user payload that records every state it is handed and answers with a
    /// canned verdict. It arrives as the `refany` argument — a *shared* clone of
    /// what the test still holds — so the test reads back exactly what the widget
    /// passed, with no global state involved.
    struct Recorder {
        seen: Vec<TextInputState>,
        update: Update,
        valid: TextInputValid,
    }

    fn recorder(update: Update, valid: TextInputValid) -> RefAny {
        RefAny::new(Recorder {
            seen: Vec::new(),
            update,
            valid,
        })
    }

    fn recorded(probe: &RefAny) -> Vec<TextInputState> {
        let mut probe = probe.clone();
        let log = probe
            .downcast_ref::<Recorder>()
            .expect("the user payload must still be a Recorder");
        log.seen.clone()
    }

    extern "C" fn record_text_input(
        mut data: RefAny,
        _: CallbackInfo,
        state: TextInputState,
    ) -> OnTextInputReturn {
        let Some(mut log) = data.downcast_mut::<Recorder>() else {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            };
        };
        log.seen.push(state);
        OnTextInputReturn {
            update: log.update,
            valid: log.valid,
        }
    }

    // Deliberately *not* the same body as `record_text_input`: two hooks with
    // byte-identical bodies can be folded onto a single symbol by the linker, and
    // the two slots have to stay distinguishable by function pointer.
    extern "C" fn record_virtual_key(
        mut data: RefAny,
        _: CallbackInfo,
        state: TextInputState,
    ) -> OnTextInputReturn {
        match data.downcast_mut::<Recorder>() {
            Some(mut log) => {
                let answer = OnTextInputReturn {
                    update: log.update,
                    valid: log.valid,
                };
                log.seen.push(state);
                answer
            }
            None => OnTextInputReturn {
                update: Update::RefreshDom,
                valid: TextInputValid::No,
            },
        }
    }

    extern "C" fn record_focus_lost(
        mut data: RefAny,
        _: CallbackInfo,
        state: TextInputState,
    ) -> Update {
        data.downcast_mut::<Recorder>().map_or(Update::RefreshDom, |mut log| {
            log.seen.push(state);
            log.update
        })
    }

    /// A `Callback`-shaped (2-argument) function — the shape FFI bindings hand in,
    /// which the `From<Callback>` arm *transmutes* into the 3-argument widget slot.
    /// Never invoked; only its address is ever compared.
    extern "C" fn generic_shaped(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    // ==================================================================
    // CallbackInfo harness
    // ==================================================================

    /// The container is always the flattened root of `TextInput::dom()`.
    const CONTAINER: usize = 0;

    fn dom_node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "nothing concrete was hit"
    /// case. `CallbackInfo::set_css_property` *panics* on such an id, so every
    /// handler has to bail out before it ever gets there.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    fn inner_id(node: DomNodeId) -> NodeId {
        node.node
            .into_crate_internal()
            .expect("expected a concrete node id")
    }

    /// A `DomLayoutResult` carrying only a `styled_dom`: the text-input handlers
    /// reach only `get_hit_node` / `get_first_child` / `get_next_sibling`, all of
    /// which read the node hierarchy alone — no real layout (and no font) needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Everything a default handler can read out of the window.
    struct Env {
        styled_dom: StyledDom,
        hit: DomNodeId,
        keycode: Option<VirtualKeyCode>,
        changeset: Option<PendingTextEdit>,
    }

    impl Env {
        fn new(styled_dom: StyledDom) -> Self {
            Self {
                styled_dom,
                hit: dom_node(CONTAINER),
                keycode: None,
                changeset: None,
            }
        }

        fn hit(mut self, hit: DomNodeId) -> Self {
            self.hit = hit;
            self
        }

        fn key(mut self, keycode: VirtualKeyCode) -> Self {
            self.keycode = Some(keycode);
            self
        }

        fn insert(mut self, text: &str) -> Self {
            self.changeset = Some(PendingTextEdit {
                node: dom_node(CONTAINER),
                inserted_text: text.into(),
                old_text: AzString::from(""),
            });
            self
        }
    }

    /// The container's two `<p>` children plus the value's bare text leaf,
    /// resolved through the *same* API the handlers use — so no test has to
    /// hard-code a flattened index.
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    struct Nodes {
        placeholder: Option<DomNodeId>,
        label: Option<DomNodeId>,
        label_text: Option<DomNodeId>,
    }

    /// Runs `f` with a real `CallbackInfo` over a window holding `env.styled_dom` as
    /// the root DOM. Returns `f`'s value, every change the handler pushed onto the
    /// transaction log, and the resolved child node ids.
    fn run<R>(env: Env, f: impl FnOnce(CallbackInfo) -> R) -> (R, Vec<CallbackChange>, Nodes) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(env.styled_dom));
        if let Some(changeset) = env.changeset {
            layout_window.text_input_manager.set_changeset(changeset);
        }
        let layout_window = layout_window;

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let mut current_window_state = FullWindowState::default();
        current_window_state.keyboard_state.current_virtual_keycode = env.keycode.into();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let probe = CallbackInfo::new(
            &ref_data,
            &changes,
            dom_node(CONTAINER),
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        let placeholder = probe.get_first_child(dom_node(CONTAINER));
        let label = placeholder.and_then(|p| probe.get_next_sibling(p));
        let label_text = label.and_then(|l| probe.get_first_child(l));
        let nodes = Nodes {
            placeholder,
            label,
            label_text,
        };

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            env.hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let r = f(info);
        let pushed = info.take_changes();
        (r, pushed, nodes)
    }

    /// Every `(node, opacity)` pair pushed onto the transaction log, in push order.
    fn pushed_opacities(changes: &[CallbackChange]) -> Vec<(NodeId, f32)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id, properties, ..
                } => {
                    let o = properties.as_ref().iter().find_map(|p| match p {
                        CssProperty::Opacity(o) => o.get_property().map(|o| o.inner.normalized()),
                        _ => None,
                    })?;
                    Some((*node_id, o))
                }
                _ => None,
            })
            .collect()
    }

    /// Every `(node, text)` repaint pushed onto the transaction log, in push order.
    fn pushed_texts(changes: &[CallbackChange]) -> Vec<(DomNodeId, String)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeText { node_id, text } => {
                    Some((*node_id, text.as_str().to_string()))
                }
                _ => None,
            })
            .collect()
    }

    // ==================================================================
    // DOM probes
    // ==================================================================

    /// Flattened child indices of `TextInput::dom()`.
    const PLACEHOLDER_CHILD: usize = 0;
    const LABEL_CHILD: usize = 1;

    fn classes(node: &Dom) -> Vec<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The text a `<p>` label wraps, looking through the block wrapper the
    /// widget convention mandates (`p > text`).
    fn text_of(node: &Dom) -> String {
        assert!(
            matches!(node.root.get_node_type(), NodeType::P),
            "widget text must be wrapped in a <p> block"
        );
        match node.children.as_ref() {
            [only] => only
                .root
                .get_node_type()
                .format()
                .expect("expected a bare text leaf"),
            other => panic!("a label <p> wraps exactly one text node, found {}", other.len()),
        }
    }

    /// Every `NodeType::Text` node in `node`'s subtree that is not a bare leaf
    /// under a `<p>`: css props, callbacks, a tab index, a dataset or children
    /// on a text node are all inert, because a text node owns no rect.
    fn text_nodes_carrying_state(node: &Dom, parent_is_p: bool, bad: &mut Vec<String>) {
        if let NodeType::Text(t) = node.root.get_node_type() {
            let carries = !node.root.get_style().is_empty()
                || !node.root.get_callbacks().as_ref().is_empty()
                || node.root.get_tab_index().is_some()
                || node.root.get_dataset().is_some()
                || !node.children.as_ref().is_empty()
                || !parent_is_p;
            if carries {
                bad.push(t.as_ref().as_str().to_string());
            }
        }
        let is_p = matches!(node.root.get_node_type(), NodeType::P);
        for c in node.children.as_ref() {
            text_nodes_carrying_state(c, is_p, bad);
        }
    }

    fn dataset_state(dom: &Dom) -> TextInputState {
        let mut dataset = dom
            .root
            .get_dataset()
            .cloned()
            .expect("TextInput::dom must attach its state as the container's dataset");
        let wrapper = dataset
            .downcast_ref::<TextInputStateWrapper>()
            .expect("the dataset must be a TextInputStateWrapper");
        wrapper.inner.clone()
    }

    /// `n` properties lifted off the default container style — an easy way to mint
    /// pairwise-distinct style vectors without hard-coding any CSS.
    fn style(n: usize) -> CssPropertyWithConditionsVec {
        let all: Vec<CssPropertyWithConditions> =
            TextInput::default().container_style.as_ref().to_vec();
        assert!(n <= all.len(), "not enough default properties to slice");
        CssPropertyWithConditionsVec::from_vec(all.into_iter().take(n).collect())
    }

    // ==================================================================
    // TextInputState::get_text
    // ==================================================================

    #[test]
    fn get_text_on_a_default_state_is_the_empty_string() {
        let state = TextInputState::default();
        assert_eq!(state.get_text(), "");
        assert!(state.text.is_empty());
        assert_eq!(state.cursor_pos, 0);
        assert!(state.placeholder.is_none());
        assert!(state.selection.is_none());
    }

    #[test]
    fn get_text_round_trips_every_hostile_string() {
        // The buffer stores one `u32` per scalar, so the round-trip must be exact for
        // anything `chars()` can produce — combining marks, ZWJ sequences, embedded
        // NULs and the very last scalar included.
        for s in HOSTILE {
            let input = TextInput::create().with_text(s.into());
            assert_eq!(
                input.text_input_state.inner.get_text(),
                s,
                "the buffer did not round-trip {s:?}",
            );
        }
    }

    #[test]
    fn get_text_silently_drops_code_units_that_are_not_unicode_scalars() {
        // `get_text` is a `filter_map(char::from_u32)`: junk in the buffer is skipped,
        // not escaped and not panicked on. Pin that, because "skip" is the difference
        // between a lossy read and a crash on FFI-provided buffers.
        for unit in NON_SCALAR_UNITS {
            let state = TextInputState {
                text: vec![u32::from('A'), unit, u32::from('B')].into(),
                ..TextInputState::default()
            };
            assert_eq!(
                state.get_text(),
                "AB",
                "code unit {unit:#x} was not dropped from the rendered text",
            );
            assert_eq!(
                state.text.len(),
                3,
                "get_text must not mutate the buffer it reads",
            );
        }
    }

    #[test]
    fn get_text_on_a_buffer_of_nothing_but_junk_is_empty_and_does_not_panic() {
        let state = TextInputState {
            text: NON_SCALAR_UNITS.to_vec().into(),
            ..TextInputState::default()
        };
        assert_eq!(state.get_text(), "");
        assert_eq!(state.text.len(), NON_SCALAR_UNITS.len());
    }

    #[test]
    fn get_text_never_yields_more_chars_than_the_buffer_holds() {
        // The one invariant that holds for *any* buffer contents: filtering can only
        // ever shrink. A `get_text` that grew would mean the buffer and the rendered
        // label disagree about how far the cursor can travel.
        let mut units: Vec<u32> = Vec::new();
        for (i, unit) in NON_SCALAR_UNITS.iter().enumerate() {
            units.push(u32::from('x'));
            units.push(*unit);
            units.push(0x1F600 + i as u32);
        }
        let state = TextInputState {
            text: units.clone().into(),
            ..TextInputState::default()
        };
        let rendered = state.get_text();
        assert!(
            rendered.chars().count() <= state.text.len(),
            "get_text produced {} chars from a {}-unit buffer",
            rendered.chars().count(),
            state.text.len(),
        );
        assert_eq!(rendered.chars().count(), units.len() - NON_SCALAR_UNITS.len());
    }

    #[test]
    fn get_text_is_pure() {
        let state = TextInputState {
            text: vec![u32::from('a'), 0xD800, u32::from('b')].into(),
            ..TextInputState::default()
        };
        let before = state.clone();
        assert_eq!(state.get_text(), state.get_text());
        assert_eq!(state, before, "get_text mutated the state it was given");
    }

    #[test]
    fn get_text_on_a_very_large_buffer_does_not_panic() {
        let n = 50_000;
        let state = TextInputState {
            text: core::iter::repeat_n(u32::from('ß'), n).collect::<Vec<_>>().into(),
            ..TextInputState::default()
        };
        let text = state.get_text();
        assert_eq!(text.chars().count(), n);
        // 'ß' is two bytes: byte length and scalar count are *not* the same number.
        assert_eq!(text.len(), n * 2);
    }

    // ==================================================================
    // TextInput::set_text / with_text
    // ==================================================================

    #[test]
    fn with_text_is_exactly_set_text() {
        for s in HOSTILE {
            let mut a = TextInput::create();
            a.set_text(s.into());
            let b = TextInput::create().with_text(s.into());
            assert_eq!(a, b, "with_text and set_text disagree on {s:?}");
        }
    }

    #[test]
    fn set_text_stores_one_code_unit_per_scalar_not_per_byte() {
        // The classic off-by-UTF-8 bug: storing `s.len()` units for a string whose
        // scalar count is smaller. Every non-ASCII entry in the table has a byte
        // length strictly greater than its scalar count.
        for s in HOSTILE {
            let input = TextInput::create().with_text(s.into());
            assert_eq!(
                input.text_input_state.inner.text.len(),
                s.chars().count(),
                "the buffer length for {s:?} is not the scalar count",
            );
        }

        let family = "👨‍👩‍👧‍👦";
        let input = TextInput::create().with_text(family.into());
        assert_eq!(input.text_input_state.inner.text.len(), 7);
        assert_eq!(family.len(), 25, "the ZWJ family is 25 bytes, not 7");
    }

    #[test]
    fn set_text_stores_the_scalar_values_verbatim() {
        let input = TextInput::create().with_text("aé\u{10ffff}".into());
        assert_eq!(
            input.text_input_state.inner.text.as_slice(),
            &[0x61, 0xE9, 0x0010_FFFF],
        );
    }

    #[test]
    fn set_text_replaces_rather_than_appends() {
        let mut input = TextInput::create();
        input.set_text("first".into());
        input.set_text("second".into());
        assert_eq!(input.text_input_state.inner.get_text(), "second");
        assert_eq!(input.text_input_state.inner.text.len(), 6);
    }

    #[test]
    fn set_text_with_an_empty_string_clears_the_buffer() {
        let mut input = TextInput::create().with_text("something".into());
        input.set_text("".into());
        assert!(input.text_input_state.inner.text.is_empty());
        assert_eq!(input.text_input_state.inner.get_text(), "");
        assert_eq!(input, TextInput::create(), "clearing did not restore a fresh widget");
    }

    #[test]
    fn set_text_does_not_enforce_max_len() {
        // KNOWN GAP: `max_len` defaults to 50 and is never read anywhere in this
        // module — not by `set_text`, not by the text-input handler. A 200-char
        // assignment is stored whole. Pinned so that adding enforcement later shows
        // up as a deliberate change rather than a silent one.
        let long: String = "x".repeat(200);
        let input = TextInput::create().with_text(long.clone().into());
        assert_eq!(input.text_input_state.inner.max_len, 50);
        assert_eq!(input.text_input_state.inner.text.len(), 200);
        assert_eq!(input.text_input_state.inner.get_text(), long);
    }

    #[test]
    fn set_text_leaves_the_cursor_where_it_was() {
        // `set_text` writes the buffer and nothing else: the cursor is only
        // reconciled by `dom()` / the focus handler. A widget built with text but
        // never rendered therefore reports a cursor of 0 over a non-empty buffer.
        let input = TextInput::create().with_text("hello".into());
        assert_eq!(input.text_input_state.inner.cursor_pos, 0);
        assert_eq!(input.text_input_state.inner.text.len(), 5);
    }

    #[test]
    fn set_text_touches_nothing_but_the_buffer() {
        let mut input = TextInput::create()
            .with_placeholder("type here".into())
            .with_placeholder_style(style(3));
        let before = input.clone();
        input.set_text("abc".into());

        assert_eq!(
            input.text_input_state.inner.placeholder.as_ref().map(|s| s.as_str().to_string()),
            Some("type here".to_string()),
        );
        assert_eq!(input.placeholder_style, before.placeholder_style);
        assert_eq!(input.container_style, before.container_style);
        assert_eq!(input.label_style, before.label_style);
        assert_eq!(input.text_input_state.inner.max_len, before.text_input_state.inner.max_len);
        assert!(input.text_input_state.inner.selection.is_none());
    }

    #[test]
    fn with_text_on_a_very_large_string_does_not_panic() {
        let n = 50_000;
        let long: String = "a".repeat(n);
        let input = TextInput::create().with_text(long.into());
        assert_eq!(input.text_input_state.inner.text.len(), n);
    }

    #[test]
    fn set_text_is_idempotent() {
        for s in HOSTILE {
            let mut input = TextInput::create();
            input.set_text(s.into());
            let once = input.clone();
            input.set_text(s.into());
            assert_eq!(input, once, "re-assigning {s:?} changed the widget");
        }
    }

    // ==================================================================
    // TextInput::set_placeholder / with_placeholder
    // ==================================================================

    #[test]
    fn placeholder_is_absent_on_a_fresh_widget() {
        assert!(TextInput::create().text_input_state.inner.placeholder.is_none());
    }

    #[test]
    fn with_placeholder_is_exactly_set_placeholder_and_stores_the_string_verbatim() {
        for s in HOSTILE {
            let mut a = TextInput::create();
            a.set_placeholder(s.into());
            let b = TextInput::create().with_placeholder(s.into());
            assert_eq!(a, b, "with_placeholder and set_placeholder disagree on {s:?}");

            assert_eq!(
                a.text_input_state.inner.placeholder.as_ref().map(|p| p.as_str()),
                Some(s),
                "the placeholder {s:?} was not stored byte-for-byte",
            );
        }
    }

    #[test]
    fn set_placeholder_overwrites_a_previous_placeholder_and_never_clears_it() {
        let mut input = TextInput::create();
        input.set_placeholder("first".into());
        input.set_placeholder("".into());
        // An empty placeholder is still *a* placeholder, not the absence of one.
        assert_eq!(
            input.text_input_state.inner.placeholder.as_ref().map(|p| p.as_str()),
            Some(""),
        );
    }

    #[test]
    fn set_placeholder_does_not_touch_the_text_buffer() {
        let mut input = TextInput::create().with_text("abc".into());
        input.set_placeholder("hint".into());
        assert_eq!(input.text_input_state.inner.get_text(), "abc");
    }

    // ==================================================================
    // Style setters
    // ==================================================================

    #[test]
    fn each_style_setter_writes_exactly_one_slot() {
        let marker = style(1);

        let mut a = TextInput::create();
        a.set_placeholder_style(marker.clone());
        assert_eq!(a.placeholder_style, marker);
        assert_eq!(a.container_style, TextInput::create().container_style);
        assert_eq!(a.label_style, TextInput::create().label_style);

        let mut b = TextInput::create();
        b.set_container_style(marker.clone());
        assert_eq!(b.container_style, marker);
        assert_eq!(b.placeholder_style, TextInput::create().placeholder_style);
        assert_eq!(b.label_style, TextInput::create().label_style);

        let mut c = TextInput::create();
        c.set_label_style(marker.clone());
        assert_eq!(c.label_style, marker);
        assert_eq!(c.placeholder_style, TextInput::create().placeholder_style);
        assert_eq!(c.container_style, TextInput::create().container_style);
    }

    #[test]
    fn the_with_style_builders_are_exactly_their_setters() {
        let s = style(2);

        let mut a = TextInput::create();
        a.set_placeholder_style(s.clone());
        assert_eq!(a, TextInput::create().with_placeholder_style(s.clone()));

        let mut b = TextInput::create();
        b.set_container_style(s.clone());
        assert_eq!(b, TextInput::create().with_container_style(s.clone()));

        let mut c = TextInput::create();
        c.set_label_style(s.clone());
        assert_eq!(c, TextInput::create().with_label_style(s));
    }

    #[test]
    fn style_setters_accept_an_empty_vector_and_survive_rendering() {
        let empty = CssPropertyWithConditionsVec::from_vec(Vec::new());
        let input = TextInput::create()
            .with_placeholder_style(empty.clone())
            .with_container_style(empty.clone())
            .with_label_style(empty.clone());
        assert!(input.container_style.is_empty());

        // Stripping every declared property must not stop the widget from rendering.
        let dom = input.dom();
        assert_eq!(dom.children.as_ref().len(), 2);
    }

    #[test]
    fn style_setters_overwrite_rather_than_merge() {
        let mut input = TextInput::create();
        input.set_container_style(style(4));
        input.set_container_style(style(1));
        assert_eq!(input.container_style.len(), 1);
    }

    // ==================================================================
    // Callback setters
    // ==================================================================

    #[test]
    fn set_on_text_input_stores_the_fn_pointer_and_the_payload_verbatim() {
        let mut input = TextInput::create();
        input.set_on_text_input(
            RefAny::new(0xDEAD_BEEF_u32),
            record_text_input as TextInputOnTextInputCallbackType,
        );

        let slot = input
            .text_input_state
            .on_text_input
            .as_ref()
            .expect("set_on_text_input stored nothing");
        assert_eq!(
            slot.callback.cb as *const () as usize,
            record_text_input as TextInputOnTextInputCallbackType as *const () as usize,
            "the fn pointer was mangled on the way in",
        );

        let mut payload = slot.refany.clone();
        assert_eq!(
            *payload.downcast_ref::<u32>().expect("the payload changed type"),
            0xDEAD_BEEF,
        );
        assert!(
            payload.downcast_ref::<u64>().is_none(),
            "the payload must not be readable as a differently-typed value",
        );
    }

    #[test]
    fn set_on_virtual_key_down_and_set_on_focus_lost_store_their_own_slots() {
        let mut input = TextInput::create();
        input.set_on_virtual_key_down(
            RefAny::new(1_u8),
            record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
        );
        input.set_on_focus_lost(
            RefAny::new(2_u8),
            record_focus_lost as TextInputOnFocusLostCallbackType,
        );

        assert!(
            input.text_input_state.on_text_input.as_ref().is_none(),
            "the text-input slot was filled in by an unrelated setter",
        );
        assert_eq!(
            input
                .text_input_state
                .on_virtual_key_down
                .as_ref()
                .expect("the virtual-key slot is empty")
                .callback
                .cb as *const () as usize,
            record_virtual_key as TextInputOnVirtualKeyDownCallbackType as *const () as usize,
        );
        assert_eq!(
            input
                .text_input_state
                .on_focus_lost
                .as_ref()
                .expect("the focus-lost slot is empty")
                .callback
                .cb as *const () as usize,
            record_focus_lost as TextInputOnFocusLostCallbackType as *const () as usize,
        );
    }

    #[test]
    fn the_with_callback_builders_are_exactly_their_setters() {
        let payload = RefAny::new(7_u16);

        let mut a = TextInput::create();
        a.set_on_text_input(payload.clone(), record_text_input as TextInputOnTextInputCallbackType);
        assert_eq!(
            a,
            TextInput::create().with_on_text_input(
                payload.clone(),
                record_text_input as TextInputOnTextInputCallbackType,
            ),
        );

        let mut b = TextInput::create();
        b.set_on_virtual_key_down(
            payload.clone(),
            record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
        );
        assert_eq!(
            b,
            TextInput::create().with_on_virtual_key_down(
                payload.clone(),
                record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
            ),
        );

        let mut c = TextInput::create();
        c.set_on_focus_lost(payload.clone(), record_focus_lost as TextInputOnFocusLostCallbackType);
        assert_eq!(
            c,
            TextInput::create()
                .with_on_focus_lost(payload, record_focus_lost as TextInputOnFocusLostCallbackType),
        );
    }

    #[test]
    fn setting_a_callback_twice_replaces_it_rather_than_stacking() {
        let mut input = TextInput::create();
        input.set_on_text_input(
            RefAny::new(1_u8),
            record_text_input as TextInputOnTextInputCallbackType,
        );
        input.set_on_text_input(
            RefAny::new(2_u8),
            record_virtual_key as TextInputOnTextInputCallbackType,
        );

        let slot = input.text_input_state.on_text_input.as_ref().expect("slot is empty");
        assert_eq!(
            slot.callback.cb as *const () as usize,
            record_virtual_key as TextInputOnTextInputCallbackType as *const () as usize,
            "the second assignment did not win",
        );
        let mut payload = slot.refany.clone();
        assert_eq!(*payload.downcast_ref::<u8>().expect("wrong payload type"), 2);
    }

    #[test]
    fn a_generic_two_argument_callback_is_accepted_through_the_ffi_conversion() {
        // FFI bindings hand in a `Callback` (2 args) which the `From<Callback>` arm
        // transmutes into the 3-argument widget slot. It must survive being stored
        // and read back — only the *address* is meaningful, so that is all we check.
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::None,
        };
        let input = TextInput::create().with_on_text_input(RefAny::new(0_u8), generic);
        assert_eq!(
            input
                .text_input_state
                .on_text_input
                .as_ref()
                .expect("the transmuted callback was dropped")
                .callback
                .cb as *const () as usize,
            generic_shaped as *const () as usize,
        );
    }

    // ==================================================================
    // TextInput::create / swap_with_default
    // ==================================================================

    #[test]
    fn create_is_default_and_is_pure() {
        assert_eq!(TextInput::create(), TextInput::default());
        assert_eq!(TextInput::create(), TextInput::create());
    }

    #[test]
    fn create_starts_empty_with_no_hooks_and_no_running_animation() {
        let input = TextInput::create();
        assert!(input.text_input_state.inner.text.is_empty());
        assert!(input.text_input_state.inner.placeholder.is_none());
        assert!(input.text_input_state.inner.selection.is_none());
        assert_eq!(input.text_input_state.inner.cursor_pos, 0);
        assert_eq!(input.text_input_state.inner.max_len, 50);
        assert!(input.text_input_state.on_text_input.as_ref().is_none());
        assert!(input.text_input_state.on_virtual_key_down.as_ref().is_none());
        assert!(input.text_input_state.on_focus_lost.as_ref().is_none());
        assert!(input.text_input_state.cursor_animation.is_none());
        assert!(input.text_input_state.update_text_input_before_calling_focus_lost_fn);
        assert!(input.text_input_state.update_text_input_before_calling_vk_down_fn);
        assert!(!input.container_style.is_empty());
    }

    #[test]
    fn swap_with_default_returns_the_old_widget_and_leaves_a_fresh_one_behind() {
        let mut input = TextInput::create()
            .with_text("typed".into())
            .with_placeholder("hint".into());
        let old = input.swap_with_default();

        assert_eq!(old.text_input_state.inner.get_text(), "typed");
        assert_eq!(input, TextInput::create(), "what was left behind is not a fresh widget");
    }

    #[test]
    fn swapping_twice_round_trips_the_original_widget() {
        let mut a = TextInput::create().with_text("abc".into());
        let mut b = a.swap_with_default(); // a = default, b = "abc"
        let c = b.swap_with_default(); // b = default, c = "abc"

        assert_eq!(c, TextInput::create().with_text("abc".into()));
        assert_eq!(a, TextInput::create());
        assert_eq!(b, TextInput::create());
    }

    #[test]
    fn swap_with_default_moves_the_hooks_out_rather_than_copying_them() {
        let probe = recorder(Update::DoNothing, TextInputValid::Yes);
        let mut input = TextInput::create().with_on_text_input(
            probe.clone(),
            record_text_input as TextInputOnTextInputCallbackType,
        );

        let old = input.swap_with_default();

        assert!(
            old.text_input_state.on_text_input.as_ref().is_some(),
            "the hook vanished during the swap",
        );
        // A duplicated hook would fire twice, and a duplicated RefAny would
        // double-free its payload.
        assert!(
            input.text_input_state.on_text_input.as_ref().is_none(),
            "the hook was copied instead of moved",
        );
        // The payload is still alive and still typed after the move (a double-freed
        // RefAny would not survive the downcast inside `recorded`).
        assert!(recorded(&probe).is_empty(), "the hook fired during a swap");
    }

    // ==================================================================
    // TextInput::dom
    // ==================================================================

    #[test]
    fn dom_builds_a_container_with_a_placeholder_block_and_a_value_block() {
        let dom = TextInput::create().dom();

        assert_eq!(classes(&dom), vec!["__azul-native-text-input-container"]);
        assert_eq!(dom.children.as_ref().len(), 2);

        let placeholder = &dom.children.as_ref()[PLACEHOLDER_CHILD];
        let label = &dom.children.as_ref()[LABEL_CHILD];
        assert_eq!(classes(placeholder), vec!["__azul-native-text-input-placeholder"]);
        assert_eq!(classes(label), vec!["__azul-native-text-input-label"]);

        for block in [placeholder, label] {
            assert!(matches!(block.root.get_node_type(), NodeType::P));
            assert_eq!(block.children.as_ref().len(), 1);
            assert!(matches!(
                block.children.as_ref()[0].root.get_node_type(),
                NodeType::Text(_),
            ));
        }
    }

    #[test]
    fn dom_emits_no_cursor_node() {
        // The caret and the selection are display-list items driven by the
        // engine's TextEditManager; a widget-owned cursor div resolved against
        // the container and never tracked the caret.
        fn walk(node: &Dom, out: &mut Vec<String>) {
            out.extend(classes(node));
            for c in node.children.as_ref() {
                walk(c, out);
            }
        }
        let mut all = Vec::new();
        walk(&TextInput::create().with_text("typed".into()).dom(), &mut all);
        assert!(
            !all.iter().any(|c| c.contains("cursor")),
            "the widget still emits a cursor node: {all:?}",
        );
    }

    #[test]
    fn dom_carries_no_state_on_any_text_node() {
        // A NodeType::Text node is unconditionally inline-level and owns no
        // rect, so css props / callbacks / a tab index / a dataset / children on
        // one are all inert. Every text node must be a bare leaf under a <p>.
        for input in [
            TextInput::create(),
            TextInput::create().with_text("typed".into()).with_placeholder("hint".into()),
        ] {
            let mut bad = Vec::new();
            text_nodes_carrying_state(&input.dom(), false, &mut bad);
            assert!(bad.is_empty(), "text nodes carrying inert state: {bad:?}");
        }
    }

    #[test]
    fn dom_marks_the_container_as_keyboard_focusable_and_editable() {
        // Focus events do not bubble and the engine records an edit against the
        // FOCUSED node, so the tab index and the contenteditable flag have to
        // sit on the same node the handlers are attached to.
        let dom = TextInput::create().dom();
        assert_eq!(dom.root.get_tab_index(), Some(TabIndex::Auto));
        assert!(dom.root.is_contenteditable());
    }

    #[test]
    fn dom_keeps_the_placeholder_out_of_the_editable_content() {
        // Everything inside a contenteditable host is editable content unless a
        // node blocks the inheritance walk; the prompt must never be typed into.
        let dom = TextInput::create().with_placeholder("hint".into()).dom();
        let placeholder = &dom.children.as_ref()[PLACEHOLDER_CHILD];
        assert!(
            placeholder
                .root
                .attributes()
                .as_ref()
                .iter()
                .any(|a| matches!(a, AttributeType::ContentEditable(false))),
            "the placeholder is inside the editable host and does not opt out",
        );
        assert!(!dom.children.as_ref()[LABEL_CHILD]
            .root
            .attributes()
            .as_ref()
            .iter()
            .any(|a| matches!(a, AttributeType::ContentEditable(_))));
    }

    #[test]
    fn dom_hides_the_placeholder_when_the_buffer_is_not_empty() {
        let hidden = TextInput::create().with_text("typed".into()).dom();
        let shown = TextInput::create().dom();

        let display = |node: &Dom| -> Option<CssProperty> {
            node.root
                .style
                .iter_inline_properties()
                .map(|(p, _)| p.clone())
                .filter(|p| matches!(p, CssProperty::Display(_)))
                .last()
        };

        assert_eq!(
            display(&hidden.children.as_ref()[PLACEHOLDER_CHILD]),
            Some(CssProperty::const_display(LayoutDisplay::None)),
        );
        assert_ne!(
            display(&shown.children.as_ref()[PLACEHOLDER_CHILD]),
            Some(CssProperty::const_display(LayoutDisplay::None)),
        );
    }

    #[test]
    fn dom_registers_exactly_the_five_default_handlers_over_one_shared_state() {
        let dom = TextInput::create().dom();
        let callbacks = dom.root.callbacks.as_ref();

        let events: Vec<EventFilter> = callbacks.iter().map(|c| c.event).collect();
        assert_eq!(
            events,
            vec![
                EventFilter::Focus(FocusEventFilter::FocusReceived),
                EventFilter::Focus(FocusEventFilter::FocusLost),
                EventFilter::Focus(FocusEventFilter::TextInput),
                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                EventFilter::Hover(HoverEventFilter::MouseOver),
            ],
        );

        let targets: Vec<usize> = callbacks.iter().map(|c| c.callback.cb).collect();
        assert_eq!(
            targets,
            vec![
                default_on_focus_received as usize,
                default_on_focus_lost as usize,
                default_on_text_input as usize,
                default_on_virtual_key_down as usize,
                default_on_mouse_hover as usize,
            ],
            "the handlers are wired to the wrong events",
        );

        // All five handlers plus the dataset must share ONE state; separate copies
        // would let the focus handler and the text handler drift apart.
        for c in callbacks {
            assert_eq!(c.refany, callbacks[0].refany, "a handler got its own state copy");
        }
        assert_eq!(
            dom.root.get_dataset().expect("no dataset attached"),
            &callbacks[0].refany,
        );
    }

    #[test]
    fn dom_renders_the_buffer_into_the_label_and_the_placeholder_into_its_own_node() {
        let dom = TextInput::create()
            .with_text("typed".into())
            .with_placeholder("hint".into())
            .dom();

        assert_eq!(text_of(&dom.children.as_ref()[PLACEHOLDER_CHILD]), "hint");
        assert_eq!(text_of(&dom.children.as_ref()[LABEL_CHILD]), "typed");
    }

    #[test]
    fn dom_without_a_placeholder_still_renders_an_empty_placeholder_node() {
        // The handlers navigate `container -> first child -> next sibling`; dropping
        // the placeholder node when unset would make the label unreachable.
        let dom = TextInput::create().with_text("typed".into()).dom();
        assert_eq!(dom.children.as_ref().len(), 2);
        assert_eq!(text_of(&dom.children.as_ref()[PLACEHOLDER_CHILD]), "");
    }

    #[test]
    fn dom_passes_hostile_text_and_placeholder_through_unchanged() {
        for s in HOSTILE {
            let dom = TextInput::create()
                .with_text(s.into())
                .with_placeholder(s.into())
                .dom();
            assert_eq!(text_of(&dom.children.as_ref()[LABEL_CHILD]), s, "label mangled {s:?}");
            assert_eq!(
                text_of(&dom.children.as_ref()[PLACEHOLDER_CHILD]),
                s,
                "placeholder mangled {s:?}",
            );
        }
    }

    #[test]
    fn dom_syncs_the_cursor_to_the_end_of_the_buffer() {
        for s in HOSTILE {
            let dom = TextInput::create().with_text(s.into()).dom();
            assert_eq!(
                dataset_state(&dom).cursor_pos,
                s.chars().count(),
                "the cursor was not parked at the end of {s:?}",
            );
        }
    }

    #[test]
    fn dom_measures_the_cursor_in_code_units_which_can_outrun_the_rendered_text() {
        // KNOWN GAP: `dom()` sets `cursor_pos = text.len()` (code units), while the
        // label only renders the units that are valid scalars. A buffer holding junk
        // therefore ends up with a cursor past the end of what is on screen.
        let dom = input_with_units(&[u32::from('a'), 0xD800, u32::from('b')]).dom();
        assert_eq!(dataset_state(&dom).cursor_pos, 3);
        assert_eq!(text_of(&dom.children.as_ref()[LABEL_CHILD]), "ab");
    }

    #[test]
    fn dom_on_a_very_large_buffer_does_not_panic() {
        let n = 50_000;
        let long: String = "x".repeat(n);
        let dom = TextInput::create().with_text(long.into()).dom();
        assert_eq!(text_of(&dom.children.as_ref()[LABEL_CHILD]).len(), n);
        assert_eq!(dataset_state(&dom).cursor_pos, n);
    }

    #[test]
    fn dom_keeps_the_configured_styles_on_the_nodes_they_were_set_for() {
        let placeholder_style = style(1);
        let label_style = style(2);
        let container_style = style(3);
        let dom = TextInput::create()
            .with_placeholder_style(placeholder_style.clone())
            .with_label_style(label_style.clone())
            .with_container_style(container_style.clone())
            .dom();

        let inline = |node: &Dom| -> Vec<CssProperty> {
            node.root.style.iter_inline_properties().map(|(p, _)| p.clone()).collect()
        };
        let declared = |v: &CssPropertyWithConditionsVec| -> Vec<CssProperty> {
            v.as_ref().iter().map(|p| p.property.clone()).collect()
        };

        assert_eq!(inline(&dom), declared(&container_style));
        assert_eq!(
            inline(&dom.children.as_ref()[PLACEHOLDER_CHILD]),
            declared(&placeholder_style),
        );
        assert_eq!(inline(&dom.children.as_ref()[LABEL_CHILD]), declared(&label_style));
    }

    #[test]
    fn the_rendered_tree_flattens_to_the_shape_the_handlers_navigate() {
        let (styled_dom, _) = rendered(TextInput::create());
        let ((), _, nodes) = run(Env::new(styled_dom), |_| ());

        let placeholder = nodes.placeholder.expect("the container has no first child");
        let label = nodes.label.expect("the placeholder has no next sibling");
        let label_text = nodes.label_text.expect("the value block has no text leaf");

        assert_ne!(placeholder, label);
        assert_ne!(label, label_text);
        assert_ne!(placeholder, label_text);
        assert_ne!(placeholder, dom_node(CONTAINER));
    }

    // ==================================================================
    // default_on_focus_received
    // ==================================================================

    #[test]
    fn focus_received_with_a_foreign_payload_is_an_inert_no_op() {
        let (styled_dom, _) = rendered(TextInput::create());
        let foreign = RefAny::new(0xDEAD_BEEF_u32);
        let (update, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_focus_received(foreign.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a foreign payload still produced {changes:?}");
    }

    #[test]
    fn focus_received_on_a_node_with_no_children_bails_out_before_touching_css() {
        // `set_css_property` panics on a `None` node id, and the widget only has a
        // placeholder to hide if the hit node actually has children. Both escapes have
        // to happen before the css write.
        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, nodes) = run(Env::new(styled_dom).hit(node_none()), |info| {
            default_on_focus_received(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(nodes.placeholder.is_some(), "the fixture itself is malformed");
    }

    #[test]
    fn focus_received_on_the_value_text_leaf_is_a_no_op() {
        // The bare text leaf is the one node in the tree with no children.
        let (probe_dom, _) = rendered(TextInput::create());
        let (_, _, nodes) = run(Env::new(probe_dom), |_| ());
        let leaf = nodes.label_text.expect("no value text leaf");

        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, _) = run(Env::new(styled_dom).hit(leaf), |info| {
            default_on_focus_received(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a childless hit node still pushed {changes:?}");
    }

    #[test]
    fn focus_received_hides_the_placeholder_only_while_the_buffer_is_empty() {
        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, nodes) = run(Env::new(styled_dom), |info| {
            default_on_focus_received(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            pushed_opacities(&changes),
            vec![(inner_id(nodes.placeholder.expect("no placeholder")), 0.0)],
            "focusing an empty input did not hide its placeholder",
        );

        let (styled_dom, state) = rendered(TextInput::create().with_text("typed".into()));
        let (update, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_focus_received(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "focusing a non-empty input touched the placeholder anyway: {changes:?}",
        );
    }

    #[test]
    fn focus_received_reparks_the_cursor_at_the_end_of_the_buffer() {
        let (styled_dom, state) = rendered(TextInput::create().with_text("hello".into()));
        // Plant a cursor that is both stale and out of range.
        poke(&state, |w| w.inner.cursor_pos = usize::MAX);

        let (_, _, _) = run(Env::new(styled_dom), |info| {
            default_on_focus_received(state.clone(), info)
        });
        assert_eq!(state_of(&state).cursor_pos, 5);
    }

    // ==================================================================
    // default_on_focus_lost
    // ==================================================================

    #[test]
    fn focus_lost_shows_the_placeholder_only_while_the_buffer_is_empty() {
        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, nodes) =
            run(Env::new(styled_dom), |info| default_on_focus_lost(state.clone(), info));
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            pushed_opacities(&changes),
            vec![(inner_id(nodes.placeholder.expect("no placeholder")), 1.0)],
            "blurring an empty input did not bring its placeholder back",
        );

        let (styled_dom, state) = rendered(TextInput::create().with_text("typed".into()));
        let (_, changes, _) =
            run(Env::new(styled_dom), |info| default_on_focus_lost(state.clone(), info));
        assert!(
            changes.is_empty(),
            "blurring a non-empty input revealed the placeholder over the text: {changes:?}",
        );
    }

    #[test]
    fn focus_lost_hands_the_hook_the_live_state_and_returns_its_verdict() {
        let probe = recorder(Update::RefreshDomAllWindows, TextInputValid::Yes);
        let (styled_dom, state) = rendered(
            TextInput::create()
                .with_text("typed".into())
                .with_on_focus_lost(probe.clone(), record_focus_lost as TextInputOnFocusLostCallbackType),
        );

        let (update, _, _) =
            run(Env::new(styled_dom), |info| default_on_focus_lost(state.clone(), info));

        assert_eq!(update, Update::RefreshDomAllWindows, "the hook's Update was swallowed");
        let seen = recorded(&probe);
        assert_eq!(seen.len(), 1, "the hook fired {} times, expected once", seen.len());
        assert_eq!(seen[0].get_text(), "typed");
        assert_eq!(seen[0].cursor_pos, 5, "the hook saw a cursor that dom() should have synced");
    }

    #[test]
    fn focus_lost_without_a_hook_reports_no_work_to_do() {
        let (styled_dom, state) = rendered(TextInput::create().with_text("typed".into()));
        let (update, _, _) =
            run(Env::new(styled_dom), |info| default_on_focus_lost(state.clone(), info));
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn focus_lost_on_a_none_hit_node_skips_the_hook_entirely() {
        // The early return sits *above* the hook dispatch, so a blur that cannot find
        // its placeholder never reaches user code. Worth pinning: a user hook that
        // commits a form would otherwise fire on a malformed hit.
        let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
        let (styled_dom, state) = rendered(TextInput::create().with_on_focus_lost(
            probe.clone(),
            record_focus_lost as TextInputOnFocusLostCallbackType,
        ));

        let (update, changes, _) = run(Env::new(styled_dom).hit(node_none()), |info| {
            default_on_focus_lost(state.clone(), info)
        });

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(recorded(&probe).is_empty(), "the hook fired on a hit node that does not exist");
    }

    #[test]
    fn focus_lost_with_a_foreign_payload_is_an_inert_no_op() {
        let (styled_dom, _) = rendered(TextInput::create());
        let foreign = RefAny::new("not a text input".to_string());
        let (update, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_focus_lost(foreign.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    // ==================================================================
    // default_on_text_input
    // ==================================================================

    #[test]
    fn text_input_without_a_pending_changeset_does_nothing() {
        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, _) =
            run(Env::new(styled_dom), |info| default_on_text_input(state.clone(), info));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(state_of(&state).get_text(), "");
    }

    #[test]
    fn text_input_with_an_empty_insertion_does_nothing() {
        let (styled_dom, state) = rendered(TextInput::create().with_text("abc".into()));
        let (update, changes, _) = run(Env::new(styled_dom).insert(""), |info| {
            default_on_text_input(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "an empty insertion still repainted: {changes:?}");
        assert_eq!(state_of(&state).get_text(), "abc");
        assert_eq!(state_of(&state).cursor_pos, 3);
    }

    #[test]
    fn text_input_mirrors_the_insertion_and_hides_the_placeholder() {
        let (styled_dom, state) = rendered(TextInput::create().with_placeholder("hint".into()));
        let (update, changes, nodes) = run(Env::new(styled_dom).insert("hi"), |info| {
            default_on_text_input(state.clone(), info)
        });

        assert_eq!(update, Update::DoNothing, "no hook is installed, so nothing needs redrawing");
        assert_eq!(state_of(&state).get_text(), "hi");
        assert_eq!(state_of(&state).cursor_pos, 2);

        assert_eq!(
            pushed_opacities(&changes),
            vec![(inner_id(nodes.placeholder.expect("no placeholder")), 0.0)],
        );
        assert!(
            pushed_texts(&changes).is_empty(),
            "the widget repainted the value itself; the engine owns the buffer: {changes:?}",
        );
    }

    #[test]
    fn text_input_appends_to_an_existing_buffer_rather_than_replacing_it() {
        let (styled_dom, state) = rendered(TextInput::create().with_text("ab".into()));
        let (_, changes, _) = run(Env::new(styled_dom).insert("cd"), |info| {
            default_on_text_input(state.clone(), info)
        });
        assert_eq!(state_of(&state).get_text(), "abcd");
        assert!(pushed_texts(&changes).is_empty());
    }

    #[test]
    fn text_input_hands_the_hook_a_preview_that_already_contains_the_insertion() {
        // The hook is a *validator*: it has to see the would-be result, not the state
        // before the edit, or it can never reject an edit for what it produces.
        let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
        let (styled_dom, state) = rendered(
            TextInput::create()
                .with_text("ab".into())
                .with_on_text_input(probe.clone(), record_text_input as TextInputOnTextInputCallbackType),
        );

        let (update, _, _) = run(Env::new(styled_dom).insert("c"), |info| {
            default_on_text_input(state.clone(), info)
        });

        assert_eq!(update, Update::RefreshDom, "the hook's Update was swallowed");
        let seen = recorded(&probe);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].get_text(), "abc", "the hook was shown the pre-edit buffer");
        assert_eq!(seen[0].cursor_pos, 3);
    }

    #[test]
    fn text_input_rejected_by_the_hook_leaves_the_buffer_and_the_screen_untouched() {
        let probe = recorder(Update::RefreshDomAllWindows, TextInputValid::No);
        let (styled_dom, state) = rendered(
            TextInput::create()
                .with_text("ab".into())
                .with_placeholder("hint".into())
                .with_on_text_input(probe.clone(), record_text_input as TextInputOnTextInputCallbackType),
        );

        let (update, changes, _) = run(Env::new(styled_dom).insert("c"), |info| {
            default_on_text_input(state.clone(), info)
        });

        assert_eq!(update, Update::RefreshDomAllWindows, "a rejected edit still reports its Update");
        assert_eq!(state_of(&state).get_text(), "ab", "a rejected edit was applied anyway");
        assert_eq!(state_of(&state).cursor_pos, 2, "a rejected edit still moved the cursor");
        assert!(
            changes
                .iter()
                .all(|c| matches!(c, CallbackChange::PreventDefault)),
            "a rejected edit still repainted the widget: {changes:?}",
        );
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, CallbackChange::PreventDefault)),
            "a rejected edit did not stop the engine from applying the changeset",
        );
    }

    #[test]
    fn text_input_advances_the_cursor_by_utf8_byte_length_not_by_scalar_count() {
        // KNOWN GAP: the cursor is advanced by `inserted_text.len()` — the *byte*
        // length of the insertion — while the buffer grows by one unit per scalar.
        // For anything outside ASCII the two disagree, and the cursor ends up past
        // the end of the buffer it indexes into.
        let (styled_dom, state) = rendered(TextInput::create());
        let (_, _, _) = run(Env::new(styled_dom).insert("é"), |info| {
            default_on_text_input(state.clone(), info)
        });

        let after = state_of(&state);
        assert_eq!(after.get_text(), "é");
        assert_eq!(after.text.len(), 1, "the buffer holds one scalar");
        assert_eq!(after.cursor_pos, 2, "but the cursor moved by the two UTF-8 bytes");
        assert!(
            after.cursor_pos > after.text.len(),
            "the cursor is expected to overshoot here; see the KNOWN GAP above",
        );
    }

    #[test]
    fn text_input_recomputes_the_cursor_rather_than_accumulating_a_stale_one() {
        // The cursor is derived from the insertion point every time, so a stale
        // value planted by a host cannot survive — nor overflow.
        let (styled_dom, state) = rendered(TextInput::create());
        poke(&state, |w| w.inner.cursor_pos = usize::MAX);

        let (update, _, _) = run(Env::new(styled_dom).insert("abc"), |info| {
            default_on_text_input(state.clone(), info)
        });

        assert_eq!(update, Update::DoNothing);
        assert_eq!(state_of(&state).cursor_pos, 3);
        assert_eq!(state_of(&state).get_text(), "abc");
    }

    #[test]
    fn text_input_accepts_astral_combining_and_multi_scalar_insertions() {
        for s in ["\u{10ffff}", "e\u{301}", "👨‍👩‍👧‍👦", "日本語", "\0"] {
            let (styled_dom, state) = rendered(TextInput::create());
            let (_, changes, _) = run(Env::new(styled_dom).insert(s), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert_eq!(state_of(&state).get_text(), s, "the buffer mangled {s:?}");
            assert_eq!(state_of(&state).text.len(), s.chars().count());
            assert!(pushed_texts(&changes).is_empty());
        }
    }

    #[test]
    fn text_input_on_a_wrong_shaped_subtree_changes_nothing() {
        // Hitting the label means `first child -> next sibling` walks off the end of
        // the tree. The handler has to give up *before* mutating the buffer, or the
        // model and the screen would silently diverge.
        let (probe_dom, _) = rendered(TextInput::create());
        let (_, _, nodes) = run(Env::new(probe_dom), |_| ());
        let label = nodes.label.expect("no label");

        let (styled_dom, state) = rendered(TextInput::create().with_text("ab".into()));
        let (result, changes, _) = run(Env::new(styled_dom).hit(label).insert("c"), |info| {
            default_on_text_input_inner(state.clone(), info)
        });

        assert_eq!(result, None, "the handler claimed to have handled a malformed tree");
        assert!(changes.is_empty());
        assert_eq!(state_of(&state).get_text(), "ab", "the buffer changed anyway");
    }

    #[test]
    fn text_input_on_a_none_hit_node_changes_nothing() {
        let (styled_dom, state) = rendered(TextInput::create().with_text("ab".into()));
        let (update, changes, _) = run(Env::new(styled_dom).hit(node_none()).insert("c"), |info| {
            default_on_text_input(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(state_of(&state).get_text(), "ab");
    }

    #[test]
    fn text_input_with_a_foreign_payload_is_an_inert_no_op() {
        let (styled_dom, _) = rendered(TextInput::create());
        let foreign = RefAny::new(0_u8);
        let (result, changes, _) = run(Env::new(styled_dom).insert("a"), |info| {
            default_on_text_input_inner(foreign.clone(), info)
        });
        assert_eq!(result, None);
        assert!(changes.is_empty());
    }

    #[test]
    fn text_input_ignores_max_len() {
        // KNOWN GAP (same root cause as `set_text_does_not_enforce_max_len`): typing
        // past `max_len` is accepted, one changeset at a time.
        let (styled_dom, state) = rendered(TextInput::create());
        let filler: String = "x".repeat(80);
        let (_, _, _) = run(Env::new(styled_dom).insert(&filler), |info| {
            default_on_text_input(state.clone(), info)
        });
        let after = state_of(&state);
        assert_eq!(after.max_len, 50);
        assert_eq!(after.text.len(), 80);
    }

    // ==================================================================
    // default_on_virtual_key_down
    // ==================================================================

    #[test]
    fn virtual_key_down_without_a_pressed_key_does_nothing() {
        let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
        let (styled_dom, state) = rendered(
            TextInput::create().with_text("ab".into()).with_on_virtual_key_down(
                probe.clone(),
                record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
            ),
        );

        let (update, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_virtual_key_down(state.clone(), info)
        });

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(recorded(&probe).is_empty(), "the hook fired without a key being down");
        assert_eq!(state_of(&state).get_text(), "ab");
    }

    #[test]
    fn backspace_is_the_engines_default_action_not_the_widgets() {
        // Deletion is `SystemChange::ApplySelectionOp` on the engine side; a
        // widget that also popped its own buffer would double-delete.
        let (styled_dom, state) = rendered(TextInput::create().with_text("abc".into()));
        let (update, changes, _) =
            run(Env::new(styled_dom).key(VirtualKeyCode::Back), |info| {
                default_on_virtual_key_down(state.clone(), info)
            });

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "backspace still mutated the DOM: {changes:?}");
        assert_eq!(state_of(&state).get_text(), "abc", "the widget deleted behind the engine");
    }

    #[test]
    fn every_key_reaches_the_hook_and_leaves_the_buffer_alone() {
        for key in [VirtualKeyCode::A, VirtualKeyCode::Back, VirtualKeyCode::Return] {
            let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
            let (styled_dom, state) = rendered(
                TextInput::create().with_text("ab".into()).with_on_virtual_key_down(
                    probe.clone(),
                    record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
                ),
            );

            let (update, changes, _) = run(Env::new(styled_dom).key(key), |info| {
                default_on_virtual_key_down(state.clone(), info)
            });

            assert_eq!(update, Update::RefreshDom, "{key:?} swallowed the hook's Update");
            if matches!(key, VirtualKeyCode::Return) {
                // Single-line field: Enter must never edit the value, so the
                // handler vetoes the engine's "\n" default — and nothing else.
                assert_eq!(changes.len(), 1, "Return must veto exactly once: {changes:?}");
                assert!(
                    matches!(changes[0], CallbackChange::PreventDefault),
                    "Return must veto the engine line break, and only that: {changes:?}"
                );
            } else {
                assert!(changes.is_empty(), "{key:?} repainted the value: {changes:?}");
            }
            assert_eq!(state_of(&state).get_text(), "ab");
            assert_eq!(recorded(&probe).len(), 1, "the hook must see {key:?} too");
        }
    }

    #[test]
    fn a_rejecting_hook_vetoes_the_engines_default_and_keeps_its_update() {
        let probe = recorder(Update::RefreshDomAllWindows, TextInputValid::No);
        let (styled_dom, state) = rendered(
            TextInput::create().with_text("abc".into()).with_on_virtual_key_down(
                probe.clone(),
                record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
            ),
        );

        let (update, changes, _) = run(Env::new(styled_dom).key(VirtualKeyCode::Back), |info| {
            default_on_virtual_key_down(state.clone(), info)
        });

        assert_eq!(update, Update::RefreshDomAllWindows);
        assert_eq!(
            changes.len(),
            1,
            "a veto must push nothing but the preventDefault: {changes:?}",
        );
        assert!(matches!(changes[0], CallbackChange::PreventDefault));
        assert_eq!(state_of(&state).get_text(), "abc");
        assert_eq!(state_of(&state).cursor_pos, 3);
    }

    #[test]
    fn the_virtual_key_hook_is_shown_the_state_from_before_the_key_is_handled() {
        // The hook runs first so that it can veto; that means it necessarily sees
        // the pre-edit buffer. Pinned because "which side of the edit does the
        // hook see" is exactly the thing a refactor gets wrong.
        let probe = recorder(Update::DoNothing, TextInputValid::Yes);
        let (styled_dom, state) = rendered(
            TextInput::create().with_text("abc".into()).with_on_virtual_key_down(
                probe.clone(),
                record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
            ),
        );

        let (_, _, _) = run(Env::new(styled_dom).key(VirtualKeyCode::Back), |info| {
            default_on_virtual_key_down(state.clone(), info)
        });

        let seen = recorded(&probe);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].get_text(), "abc");
        assert_eq!(seen[0].cursor_pos, 3);
    }

    #[test]
    fn virtual_key_down_on_a_none_hit_node_skips_the_hook_entirely() {
        let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
        let (styled_dom, state) = rendered(
            TextInput::create().with_text("abc".into()).with_on_virtual_key_down(
                probe.clone(),
                record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
            ),
        );

        let (result, changes, _) = run(
            Env::new(styled_dom).hit(node_none()).key(VirtualKeyCode::Back),
            |info| default_on_virtual_key_down_inner(state.clone(), info),
        );

        assert_eq!(result, None);
        assert!(changes.is_empty());
        assert!(recorded(&probe).is_empty(), "the hook fired on a hit node that does not exist");
        assert_eq!(state_of(&state).get_text(), "abc");
    }

    #[test]
    fn virtual_key_down_with_a_foreign_payload_is_an_inert_no_op() {
        let (styled_dom, _) = rendered(TextInput::create());
        let foreign = RefAny::new(vec![1_u32, 2, 3]);
        let (update, changes, _) = run(Env::new(styled_dom).key(VirtualKeyCode::Back), |info| {
            default_on_virtual_key_down(foreign.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn repeated_key_presses_are_idempotent_on_the_widget_state() {
        // Six backspaces over a three-scalar buffer: the widget must not move a
        // single unit — every one of them belongs to the engine.
        let (_, state) = rendered(TextInput::create().with_text("abc".into()));
        for i in 0..6 {
            // The tree shape does not depend on what the buffer holds, so a fresh
            // container is enough to navigate; the live state is `state`.
            let (styled_dom, _) = rendered(TextInput::create());
            let (update, _, _) = run(Env::new(styled_dom).key(VirtualKeyCode::Back), |info| {
                default_on_virtual_key_down(state.clone(), info)
            });
            assert_eq!(update, Update::DoNothing, "backspace #{i} reported work to do");
        }
        let after = state_of(&state);
        assert_eq!(after.get_text(), "abc");
        assert_eq!(after.cursor_pos, 3);
    }

    // ==================================================================
    // default_on_mouse_hover
    // ==================================================================

    #[test]
    fn mouse_hover_is_inert_for_every_payload_and_every_hit_node() {
        let (styled_dom, state) = rendered(TextInput::create().with_text("abc".into()));
        let (update, changes, _) =
            run(Env::new(styled_dom), |info| default_on_mouse_hover(state.clone(), info));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(state_of(&state).get_text(), "abc", "hovering edited the buffer");

        let (styled_dom, _) = rendered(TextInput::create());
        let foreign = RefAny::new(0_u8);
        let (update, changes, _) = run(Env::new(styled_dom).hit(node_none()), |info| {
            default_on_mouse_hover(foreign.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }
}

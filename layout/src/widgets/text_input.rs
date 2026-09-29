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
    dom::{AttributeType, Dom, DomNodeId},
    form::{ValidityReason, ValidityState},
    refany::RefAny,
    task::OptionTimerId,
};
use unicode_segmentation::UnicodeSegmentation;
use azul_css::{css::BoxOrStatic, dynamic_selector::OptionCssPropertyWithConditionsVec};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    OptionString, *,
};

use crate::callbacks::{Callback, CallbackInfo};

const BACKGROUND_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
}; // white
const COLOR_9B9B9B: ColorU = ColorU {
    r: 155,
    g: 155,
    b: 155,
    a: 255,
}; // #9b9b9b
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

/// Minimum height of the field (border box), every platform: one line of the
/// 11 px UI font plus the chrome. With no text the value `<p>` has no line box
/// and the placeholder is positioned out of flow, so without this an EMPTY
/// field collapsed to its padding + border (4 px) — it used to be propped up
/// by the UA `<p>` margin that `widget_p` now resets.
const TEXT_INPUT_MIN_HEIGHT_PX: isize = 22;

#[cfg(target_os = "windows")]
pub(crate) static TEXT_INPUT_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(
        TEXT_INPUT_MIN_HEIGHT_PX,
    ))),
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
    // A single-line field CLIPS vertically but SCROLLS horizontally WITHOUT a
    // scrollbar, so the caret stays visible once the text runs past the right
    // edge. `overflow-x: hidden` clipped the caret (the "append-only" feel the
    // user hit); `auto` makes the container a scroll box the caret-reveal
    // (`scroll_selection_into_view`) can shift, and `scrollbar-width: none` keeps
    // a one-line input from sprouting a horizontal bar under it.
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Auto)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::ScrollbarWidth(
        LayoutScrollbarWidthValue::Exact(LayoutScrollbarWidth::None),
    )),
    // The value line is centred in the field's `min-height` — which needs the
    // field to BE a flex container. Without this the box laid out as a block,
    // `justify-content` did nothing, and the value sat at the top edge with the
    // bottom border a line's height below it (the number input on the frontpage
    // screenshot looked like it had a rule drawn through it).
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Flex,
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    // Hover and focus border states are NOT here. They live in the theme
    // modules (`flat::FIELD_BORDER_STATES`, `flora::FIELD_BORDER_STATES`) and
    // are appended by `flat::text_input` / `flora::text_input`, because the
    // dark half of each pair needs the theme's `DARK_ACC` — a colour this file
    // cannot see. Declared here, they could only ever name the light-mode
    // colours, which is why a hovered field kept its light ring on a dark
    // surface.
    //
    // The Windows-only grey hover ring (#4c4c4c) went with them: which colour
    // rings the field is the theme's decision now, and neither theme draws a
    // platform distinction.
];

#[cfg(target_os = "linux")]
pub(crate) static TEXT_INPUT_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(
        TEXT_INPUT_MIN_HEIGHT_PX,
    ))),
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
    // The value line is centred in the field's `min-height` — which needs the
    // field to BE a flex container. Without this the box laid out as a block,
    // `justify-content` did nothing, and the value sat at the top edge with the
    // bottom border a line's height below it (the number input on the frontpage
    // screenshot looked like it had a rule drawn through it).
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Flex,
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    // Hover and focus border states are NOT here. They live in the theme
    // modules (`flat::FIELD_BORDER_STATES`, `flora::FIELD_BORDER_STATES`) and
    // are appended by `flat::text_input` / `flora::text_input`, because the
    // dark half of each pair needs the theme's `DARK_ACC` — a colour this file
    // cannot see. Declared here, they could only ever name the light-mode
    // colours, which is why a hovered field kept its light ring on a dark
    // surface.
];

// Mobile (Android / iOS) inherit the macOS-style container — same flex
// box-sizing and background; touch-target padding is the user's concern.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub(crate) static TEXT_INPUT_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(
        TEXT_INPUT_MIN_HEIGHT_PX,
    ))),
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
    // The value line is centred in the field's `min-height` — which needs the
    // field to BE a flex container. Without this the box laid out as a block,
    // `justify-content` did nothing, and the value sat at the top edge with the
    // bottom border a line's height below it (the number input on the frontpage
    // screenshot looked like it had a rule drawn through it).
    CssPropertyWithConditions::simple(CssProperty::Display(LayoutDisplayValue::Exact(
        LayoutDisplay::Flex,
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexDirection(LayoutFlexDirectionValue::Exact(
        LayoutFlexDirection::Column,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    // Hover and focus border states are NOT here. They live in the theme
    // modules (`flat::FIELD_BORDER_STATES`, `flora::FIELD_BORDER_STATES`) and
    // are appended by `flat::text_input` / `flora::text_input`, because the
    // dark half of each pair needs the theme's `DARK_ACC` — a colour this file
    // cannot see. Declared here, they could only ever name the light-mode
    // colours, which is why a hovered field kept its light ring on a dark
    // surface.
];

// -- label style
//
// The label is the `<p>` block wrapping the value text, so it is the box that
// bounds the line and clips against the container's `overflow: hidden`.
// `white-space: pre` keeps a single-line field on one line and preserves the
// spaces the user typed.

#[cfg(target_os = "windows")]
pub(crate) static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    // The PROMPT's own colour, via the real `::placeholder` cascade. The
    // engine paints the prompt with the value line's style, overridden by
    // whatever `::placeholder` declares - so this is the widget's default
    // and any app rule (`.my-field::placeholder { color: ... }`) wins over
    // it exactly like normal CSS.
    CssPropertyWithConditions::on_placeholder(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    // The value line SCROLLS horizontally (no visible bar) so the caret stays
    // in view once the text runs past the right edge. This is where the scroll
    // MUST live: the container is a block box this value `<p>` fills exactly
    // (394 px in a 400 px field), so the overflowing line is INSIDE this box,
    // not the container's. `overflow-x: hidden` trapped it and the field went
    // append-only — the caret walked off the right edge and new characters were
    // invisible. `auto` makes this the horizontal scroll box the caret-reveal
    // (`scroll_selection_into_view` → `find_scrollable_ancestor`) shifts to
    // follow the caret; `scrollbar-width: none` keeps a one-line field from
    // reserving vertical space for a horizontal bar. `white-space: pre` keeps
    // the value on one line so it overflows horizontally rather than wrapping.
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Auto)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::ScrollbarWidth(
        LayoutScrollbarWidthValue::Exact(LayoutScrollbarWidth::None),
    )),
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
pub(crate) static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    // The PROMPT's own colour, via the real `::placeholder` cascade. The
    // engine paints the prompt with the value line's style, overridden by
    // whatever `::placeholder` declares - so this is the widget's default
    // and any app rule (`.my-field::placeholder { color: ... }`) wins over
    // it exactly like normal CSS.
    CssPropertyWithConditions::on_placeholder(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    // The value line SCROLLS horizontally (no visible bar) so the caret stays
    // in view once the text runs past the right edge. This is where the scroll
    // MUST live: the container is a block box this value `<p>` fills exactly
    // (394 px in a 400 px field), so the overflowing line is INSIDE this box,
    // not the container's. `overflow-x: hidden` trapped it and the field went
    // append-only — the caret walked off the right edge and new characters were
    // invisible. `auto` makes this the horizontal scroll box the caret-reveal
    // (`scroll_selection_into_view` → `find_scrollable_ancestor`) shifts to
    // follow the caret; `scrollbar-width: none` keeps a one-line field from
    // reserving vertical space for a horizontal bar. `white-space: pre` keeps
    // the value on one line so it overflows horizontally rather than wrapping.
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Auto)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::ScrollbarWidth(
        LayoutScrollbarWidthValue::Exact(LayoutScrollbarWidth::None),
    )),
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
pub(crate) static TEXT_INPUT_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    // The PROMPT's own colour, via the real `::placeholder` cascade. The
    // engine paints the prompt with the value line's style, overridden by
    // whatever `::placeholder` declares - so this is the widget's default
    // and any app rule (`.my-field::placeholder { color: ... }`) wins over
    // it exactly like normal CSS.
    CssPropertyWithConditions::on_placeholder(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    // The value line SCROLLS horizontally (no visible bar) so the caret stays
    // in view once the text runs past the right edge. This is where the scroll
    // MUST live: the container is a block box this value `<p>` fills exactly
    // (394 px in a 400 px field), so the overflowing line is INSIDE this box,
    // not the container's. `overflow-x: hidden` trapped it and the field went
    // append-only — the caret walked off the right edge and new characters were
    // invisible. `auto` makes this the horizontal scroll box the caret-reveal
    // (`scroll_selection_into_view` → `find_scrollable_ancestor`) shifts to
    // follow the caret; `scrollbar-width: none` keeps a one-line field from
    // reserving vertical space for a horizontal bar. `white-space: pre` keeps
    // the value on one line so it overflows horizontally rather than wrapping.
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Auto)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::ScrollbarWidth(
        LayoutScrollbarWidthValue::Exact(LayoutScrollbarWidth::None),
    )),
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Pre,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
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
    pub container_style: OptionCssPropertyWithConditionsVec,
    pub label_style: OptionCssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET so it knows at build time whether it was named;
    /// forwarded into the accessibility declaration it already builds.
    pub accessibility_name: OptionString,
    /// The HTML `name` this field submits its value under (see
    /// [`crate::widgets::form::Form`]). `None` keeps the field out of a form's
    /// `FormData`, like an `<input>` without a `name`.
    pub name: OptionString,
    pub theme: crate::widgets::themes::OptionUiTheme,
}

/// Which HTML `<input type=..>` a [`TextInput`] stands for.
///
/// One widget, several modes, rather than one copy of the widget per type:
/// every kind edits a single line of text through the same engine-owned
/// buffer. The kind decides what the line SHOWS (a password shows one bullet
/// per grapheme), which checks the value must pass (email and url syntax),
/// the soft keyboard the platform offers (`type` attribute, read by
/// `crate::form::input_purpose`) and what assistive technology announces.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TextInputKind {
    /// `type=text`: plain text, the default.
    #[default]
    Text,
    /// `type=password`: the line shows one bullet per grapheme, the real text
    /// lives only in the widget state; copy and cut are refused.
    Password,
    /// `type=search`: a clear button appears while the field holds text;
    /// Escape clears it too.
    Search,
    /// `type=email`: the value must be a valid e-mail address.
    Email,
    /// `type=tel`: any text; only the soft keyboard changes (a phone pad).
    Tel,
    /// `type=url`: the value must be an absolute URL.
    Url,
}

impl TextInputKind {
    /// The HTML `type` attribute value this kind stands for.
    #[must_use]
    pub const fn html_type(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Password => "password",
            Self::Search => "search",
            Self::Email => "email",
            Self::Tel => "tel",
            Self::Url => "url",
        }
    }
}

/// Editable state of a text input (text buffer, cursor position, selection).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TextInputState {
    /// The REAL value, one `u32` per Unicode scalar - for a password too:
    /// what the line shows is derived from it (see [`display_text`]).
    pub text: U32Vec, // Vec<char>
    pub placeholder: OptionString,
    pub max_len: usize,
    pub selection: OptionTextInputSelection,
    pub cursor_pos: usize,
    /// HTML `pattern`: the WHOLE value must match this regular expression
    /// (compiled as `^(?:pattern)$`). An empty value is exempt, a pattern that
    /// does not compile is ignored - both exactly as in HTML.
    pub pattern: OptionString,
    /// Which constraints the current value fails - HTML's `ValidityState`.
    /// Recomputed by the widget on every build and every edit; read it from
    /// any callback that receives this state.
    pub validity: ValidityState,
    /// Which `<input type>` this field is.
    pub kind: TextInputKind,
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

impl azul_core::host_invoker::HostOut for OnTextInputReturn {
    fn unwritten() -> Self {
        Self {
            update: Update::DoNothing,
            valid: TextInputValid::Yes,
        }
    }
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
    from_handle_byref_fn: AzTextInputOnTextInputCallback_createFromHostHandleByref,
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
    from_handle_byref_fn: AzTextInputOnVirtualKeyDownCallback_createFromHostHandleByref,
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
    from_handle_byref_fn: AzTextInputOnFocusLostCallback_createFromHostHandleByref,
    extra_args:     [ state: TextInputState ],
}
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
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
            container_style: OptionCssPropertyWithConditionsVec::None,
            label_style: OptionCssPropertyWithConditionsVec::None,
            accessibility_name: OptionString::None,
            name: OptionString::None,
            theme: None.into(),
        }
    }
}

impl Default for TextInputState {
    fn default() -> Self {
        Self {
            text: Vec::new().into(),
            placeholder: None.into(),
            // Unlimited, like a browser <input> without `maxlength`. The old
            // default was an arbitrary 50 that nothing enforced; now that
            // typing past `max_len` is vetoed (default_on_text_input), keeping
            // 50 would have silently capped every existing field.
            max_len: usize::MAX,
            selection: None.into(),
            cursor_pos: 0,
            pattern: OptionString::None,
            validity: ValidityState::valid(),
            kind: TextInputKind::Text,
        }
    }
}

impl TextInputState {
    #[must_use]
    pub fn get_text(&self) -> String {
        self.text
            .iter()
            .filter_map(|c| core::char::from_u32(*c))
            .collect()
    }
}

// ---------------------------------------------------------------------------
// type=password: the line shows a mask, the state keeps the value
// ---------------------------------------------------------------------------

/// The glyph a password field paints in place of each grapheme.
pub const PASSWORD_MASK_CHAR: char = '\u{2022}';

/// Byte length of [`PASSWORD_MASK_CHAR`]: every offset the engine reports on a
/// masked line is a multiple of it, because the line holds nothing else.
const MASK_LEN: usize = PASSWORD_MASK_CHAR.len_utf8();

/// Number of user-perceived characters (extended grapheme clusters) in `s`.
fn grapheme_count(s: &str) -> usize {
    s.graphemes(true).count()
}

/// One mask glyph per grapheme of `s`.
fn mask_for(s: &str) -> String {
    core::iter::repeat_n(PASSWORD_MASK_CHAR, grapheme_count(s)).collect()
}

/// The text the value line SHOWS for `state`: the value itself, or - for a
/// password - one [`PASSWORD_MASK_CHAR`] per grapheme of it.
///
/// The engine's buffer holds exactly this string, so every caret and selection
/// offset the engine reports is an offset into it.
#[must_use]
pub fn display_text(state: &TextInputState) -> String {
    let text = state.get_text();
    if state.kind == TextInputKind::Password {
        mask_for(&text)
    } else {
        text
    }
}

/// Byte offset in `real` of the boundary before its `index`-th grapheme, or the
/// end of `real` when it has fewer.
fn grapheme_byte_offset(real: &str, index: usize) -> usize {
    real.grapheme_indices(true)
        .nth(index)
        .map_or(real.len(), |(at, _)| at)
}

/// A byte offset into a MASKED line (one glyph per grapheme), mapped onto the
/// matching grapheme boundary of the real value `real`.
fn masked_to_real_offset(real: &str, masked_byte: usize) -> usize {
    grapheme_byte_offset(real, masked_byte / MASK_LEN)
}

/// The real value after the engine DELETED bullets from a masked line.
///
/// `masked_after` is how many bullets the line holds now, `caret` the grapheme
/// index the caret sits at after the deletion - which is where the deleted run
/// started, for Backspace, Delete and a selection alike. `None` when there is
/// nothing to mirror: nothing was removed, or the line GREW without any
/// characters to show for it (an undo re-inserting bullets), which bullets can
/// never be turned back into.
fn masked_deletion(real: &str, masked_after: usize, caret: usize) -> Option<String> {
    let before = grapheme_count(real);
    if masked_after >= before {
        return None;
    }
    let removed = before - masked_after;
    let at = caret.min(masked_after);
    let start = grapheme_byte_offset(real, at);
    let end = grapheme_byte_offset(real, at + removed);
    let mut next = String::with_capacity(real.len());
    next.push_str(&real[..start]);
    next.push_str(&real[end..]);
    Some(next)
}

/// `s` as the widget's scalar buffer.
fn to_units(s: &str) -> U32Vec {
    s.chars().map(|c| c as u32).collect::<Vec<_>>().into()
}

/// Which constraints `state`'s current value fails.
fn validity_of(state: &TextInputState) -> ValidityState {
    state.compute_validity()
}

impl TextInputState {
    /// Which constraints the current value fails, HTML's rules: the syntax
    /// its [`TextInputKind`] demands (`type=email`, `type=url`) and the
    /// `pattern`, which must match the WHOLE value. An empty value is exempt
    /// from both. A password is checked against its real text, never its
    /// mask. The widget keeps [`Self::validity`] up to date with this; call it
    /// directly on a state you built yourself.
    #[must_use]
    pub fn compute_validity(&self) -> ValidityState {
        let mut validity = ValidityState::valid();
        let value = self.get_text();
        if value.is_empty() {
            return validity;
        }
        let type_ok = match self.kind {
            TextInputKind::Email => crate::form::is_valid_email(&value),
            TextInputKind::Url => crate::form::is_valid_absolute_url(&value),
            TextInputKind::Text
            | TextInputKind::Password
            | TextInputKind::Search
            | TextInputKind::Tel => true,
        };
        if !type_ok {
            validity.insert(ValidityReason::TypeMismatch);
        }
        if let Some(pattern) = self.pattern.as_ref() {
            if crate::form::pattern_matches(pattern.as_str(), &value) == Some(false) {
                validity.insert(ValidityReason::PatternMismatch);
            }
        }
        validity
    }

    /// Can this field's value be invalid at all? Only a typed field
    /// (`email`, `url`) or one with a `pattern` has a constraint to fail.
    #[must_use]
    pub const fn is_constrained(&self) -> bool {
        matches!(self.kind, TextInputKind::Email | TextInputKind::Url) || self.pattern.is_some()
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
    /// The container style this widget renders with.
    ///
    /// `None` means no opinion, so the widget's default applies — the same
    /// answer both themes give, asked in one place.
    #[must_use]
    pub fn resolved_container_style(&self) -> CssPropertyWithConditionsVec {
        self.container_style
            .clone()
            .into_option()
            .unwrap_or_else(|| {
                CssPropertyWithConditionsVec::from_const_slice(TEXT_INPUT_CONTAINER_PROPS)
            })
    }

    /// The label style this widget renders with.
    ///
    /// `None` means no opinion, so the widget's default applies — the same
    /// answer both themes give, asked in one place.
    #[must_use]
    pub fn resolved_label_style(&self) -> CssPropertyWithConditionsVec {
        self.label_style.clone().into_option().unwrap_or_else(|| {
            CssPropertyWithConditionsVec::from_const_slice(TEXT_INPUT_LABEL_PROPS)
        })
    }

    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    #[must_use]
    pub fn create() -> Self {
        Self::default()
    }

    /// A field of the given `<input type>`.
    #[must_use]
    pub fn create_with_kind(kind: TextInputKind) -> Self {
        Self::default().with_kind(kind)
    }

    /// `<input type=password>`: shows one bullet per grapheme, keeps the real
    /// text in the state, refuses copy and cut.
    #[must_use]
    pub fn create_password() -> Self {
        Self::create_with_kind(TextInputKind::Password)
    }

    /// `<input type=search>`: a clear button while non-empty; Escape clears.
    #[must_use]
    pub fn create_search() -> Self {
        Self::create_with_kind(TextInputKind::Search)
    }

    /// `<input type=email>`: validated as an e-mail address.
    #[must_use]
    pub fn create_email() -> Self {
        Self::create_with_kind(TextInputKind::Email)
    }

    /// `<input type=tel>`: plain text with a phone-pad soft keyboard.
    #[must_use]
    pub fn create_tel() -> Self {
        Self::create_with_kind(TextInputKind::Tel)
    }

    /// `<input type=url>`: validated as an absolute URL.
    #[must_use]
    pub fn create_url() -> Self {
        Self::create_with_kind(TextInputKind::Url)
    }

    /// Switch this field to another `<input type>`.
    pub const fn set_kind(&mut self, kind: TextInputKind) {
        self.text_input_state.inner.kind = kind;
    }

    /// [`Self::set_kind`] for the builder chain.
    #[must_use]
    pub const fn with_kind(mut self, kind: TextInputKind) -> Self {
        self.set_kind(kind);
        self
    }

    /// HTML `pattern`: the whole value must match `pattern` (see
    /// [`TextInputState::pattern`]).
    pub fn set_pattern(&mut self, pattern: AzString) {
        self.text_input_state.inner.pattern = Some(pattern).into();
    }

    /// [`Self::set_pattern`] for the builder chain.
    #[must_use]
    pub fn with_pattern(mut self, pattern: AzString) -> Self {
        self.set_pattern(pattern);
        self
    }

    /// The name this field's value is submitted under in a
    /// [`crate::widgets::form::Form`].
    pub fn set_name(&mut self, name: AzString) {
        self.name = Some(name).into();
    }

    /// [`Self::set_name`] for the builder chain.
    #[must_use]
    pub fn with_name(mut self, name: AzString) -> Self {
        self.set_name(name);
        self
    }

    #[must_use]
    pub fn with_text(mut self, text: AzString) -> Self {
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

    /// Pick the widget theme. Unset (`None`), the widget follows the app
    /// theme (`AppConfig::with_theme`, flat by default).
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.text_input_state.inner.placeholder = Some(placeholder).into();
    }

    #[must_use]
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

    pub fn set_container_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.container_style = OptionCssPropertyWithConditionsVec::Some(style);
    }

    #[must_use]
    pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    pub fn set_label_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.label_style = OptionCssPropertyWithConditionsVec::Some(style);
    }

    #[must_use]
    pub fn with_label_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
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
    ///
    /// Unpinned (`theme: None`), the field follows the APP theme: built in
    /// the structure of the theme its DOM is built for, every node carrying
    /// flat's and flora's blocks (`themes::theme_blocks::follow_app_theme`).
    #[must_use]
    pub fn dom(self) -> Dom {
        use crate::widgets::themes::theme_blocks;
        match self.theme.into_option() {
            Some(theme) => self.dom_in(theme),
            None => theme_blocks::follow_app_theme(self, Self::dom_flat, Self::dom_flora),
        }
    }

    /// [`Self::dom_in`] the flat theme.
    fn dom_flat(self) -> Dom {
        self.dom_in(crate::widgets::themes::UiTheme::Flat)
    }

    /// [`Self::dom_in`] the flora theme.
    fn dom_flora(self) -> Dom {
        self.dom_in(crate::widgets::themes::UiTheme::Flora)
    }

    /// Renders the field in `theme`.
    fn dom_in(mut self, theme: crate::widgets::themes::UiTheme) -> Dom {
        // The state is built fresh from the app's value, so its validity is
        // too: an app handing in a malformed e-mail gets an invalid state
        // (and FormData) before the user has touched the field.
        self.text_input_state.inner.validity = validity_of(&self.text_input_state.inner);
        let kind = self.text_input_state.inner.kind;
        let constrained = self.text_input_state.inner.is_constrained();
        let name = self.name.clone();
        let a11y_name = self.accessibility_name.clone();
        let has_text = !self.text_input_state.inner.text.is_empty();
        let container = match theme {
            crate::widgets::themes::UiTheme::Flat => crate::widgets::themes::flat::text_input(self),
            crate::widgets::themes::UiTheme::Flora => {
                crate::widgets::themes::flora::text_input(self)
            }
        };
        let mut container = with_kind_semantics(container, kind, name, a11y_name);
        if constrained {
            // The handlers paint the invalid ring in the THEME's colours, and
            // this is how they learn which theme the field was built with.
            container.add_class(AzString::from_const_str(match theme {
                crate::widgets::themes::UiTheme::Flat => THEME_FLAT_CLASS,
                crate::widgets::themes::UiTheme::Flora => THEME_FLORA_CLASS,
            }));
        }
        if kind == TextInputKind::Search {
            search_field(container, theme, has_text)
        } else {
            container
        }
    }
}

/// Marks a constrained field built by the flat theme (see [`TextInput::dom`]).
pub const THEME_FLAT_CLASS: &str = "__azul-theme-flat";
/// Marks a constrained field built by the flora theme.
pub const THEME_FLORA_CLASS: &str = "__azul-theme-flora";

/// The class of the row a `type=search` field sits in.
pub const SEARCH_FIELD_CLASS: &str = "__azul-native-search-field";
/// The class of a `type=search` field's clear button.
pub const SEARCH_CLEAR_CLASS: &str = "__azul-native-search-clear";

/// `type=search`: the field, then its clear button, in one row.
///
/// The button is a SIBLING of the editable host, never a child: inside the host
/// its glyph would be editable content, and a click on it would place a caret
/// in the cross. It is not a Tab stop (Escape clears from the keyboard, as in
/// every browser), and it shares the field's state, so the click handler
/// mirrors the clear exactly like an edit.
fn search_field(
    container: Dom,
    theme: crate::widgets::themes::UiTheme,
    has_text: bool,
) -> Dom {
    use azul_core::{
        a11y::{AccessibilityInfo, AccessibilityRole},
        dom::{EventFilter, HoverEventFilter},
        refany::OptionRefAny,
    };
    use crate::widgets::themes::{flat, flora, UiTheme};

    let mut clear = match theme {
        UiTheme::Flat => flat::search_clear_button(has_text),
        UiTheme::Flora => flora::search_clear_button(has_text),
    };
    if let Some(state) = container.root.get_dataset().cloned() {
        clear.add_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            state,
            CoreCallback {
                cb: default_on_search_clear_click as usize,
                ctx: OptionRefAny::None,
            },
        );
    }
    let clear = clear.with_accessibility_info(AccessibilityInfo {
        role: AccessibilityRole::PushButton,
        accessibility_name: Some(AzString::from_const_str("Clear search")).into(),
        ..Default::default()
    });

    match theme {
        UiTheme::Flat => flat::search_field(container, clear),
        UiTheme::Flora => flora::search_field(container, clear),
    }
}

/// What every `<input type>` adds on top of the themed field, in ONE place so
/// the two themes cannot drift on it: the `type` and `name` attributes, the
/// accessibility declaration a password needs, and the clipboard veto.
///
/// A plain `type=text` field without a name gets nothing here, so its DOM is
/// exactly what it was before the kinds existed.
fn with_kind_semantics(
    mut container: Dom,
    kind: TextInputKind,
    name: OptionString,
    a11y_name: OptionString,
) -> Dom {
    use azul_core::{
        a11y::AccessibilityState,
        dom::{EventFilter, FocusEventFilter},
        refany::OptionRefAny,
    };

    // The soft keyboard reads the `type` attribute of the focused node
    // (`crate::form::input_purpose`), and so does the accessibility tree.
    if kind != TextInputKind::Text {
        container = container.with_attribute(AttributeType::InputType(AzString::from_const_str(
            kind.html_type(),
        )));
    }
    if let Some(name) = name.into_option() {
        container = container.with_attribute(AttributeType::Name(name));
    }

    if let Some(mut a11y) = container.root.get_accessibility_info().cloned() {
        if let Some(explicit) = a11y_name.into_option() {
            a11y.accessibility_name = Some(explicit).into();
        }
        if kind == TextInputKind::Password {
            // The value would be read out loud. HTML's password field exposes
            // no value either; the bullets are all an AT user gets, like
            // everyone else.
            a11y.accessibility_value = OptionString::None;
            let mut states = a11y.states.clone().into_library_owned_vec();
            if !states.contains(&AccessibilityState::Protected) {
                states.push(AccessibilityState::Protected);
            }
            a11y.states = states.into();
        }
        container.root.set_accessibility_info(a11y);
    }

    if kind == TextInputKind::Password {
        // The engine's buffer holds bullets, so a copy would only ever copy
        // bullets - but a password field that pretends to copy is still a
        // lie. HTML refuses both, and so does every native toolkit.
        if let Some(state) = container.root.get_dataset().cloned() {
            for filter in [FocusEventFilter::Copy, FocusEventFilter::Cut] {
                container.root.add_callback(
                    EventFilter::Focus(filter),
                    state.clone(),
                    CoreCallback {
                        cb: default_on_clipboard_veto as usize,
                        ctx: OptionRefAny::None,
                    },
                );
            }
        }
    }

    container
}

pub const TEXT_INPUT_CONTAINER_CLASS: &str = "__azul-native-text-input-container";
pub const TEXT_INPUT_LABEL_CLASS: &str = "__azul-native-text-input-label";

/// The value `<p>` - the editable line the engine paints the `placeholder`
/// attribute's prompt into while it is empty and unfocused.
///
/// Resolved through the same hierarchy hop the container's own layout
/// guarantees; a subtree of any other shape yields `None` and every handler
/// bails out. (Was `label_nodes`, returning an overlay node too, until the
/// prompt became an engine-painted attribute.)
fn value_node(info: &CallbackInfo) -> Option<DomNodeId> {
    let child = info.get_first_child(info.get_hit_node())?;
    // SHAPE GUARD: the value `<p>` always wraps exactly one text leaf, so a
    // childless first child means the hit node was NOT the container (it was
    // the value line itself, whose first child IS that leaf). The two-child
    // layout used to get this for free - `first child -> next sibling` walked
    // off the end - and a handler that mutates the buffer on a malformed
    // subtree silently diverges the model from the screen.
    let node = child.node.into_crate_internal()?;
    if info.get_children_count(child.dom, node) == 0 {
        return None;
    }
    Some(child)
}

/// Adopts the engine's text for `node` into the widget's mirror.
///
/// The engine owns the buffer, so its answer wins — except that an empty answer
/// is ambiguous: `get_text_before_textinput` also yields nothing for a node
/// whose text sits under a block wrapper it does not descend into. An empty
/// read therefore never clears a non-empty mirror.
fn adopt_engine_text(state: &mut TextInputState, info: &CallbackInfo, node: DomNodeId) {
    // A password's engine buffer holds BULLETS: adopting it would overwrite
    // the real value with its own mask. Its edits are mirrored one by one
    // instead (`masked_insertion` / `masked_notification`).
    if state.kind == TextInputKind::Password {
        return;
    }
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

/// The engine's selection in the widget's public shape, as offsets into the
/// REAL value - for a password the engine's offsets index the bullets, so they
/// are mapped onto the value's grapheme boundaries first.
fn mirror_selection(
    info: &CallbackInfo,
    node: DomNodeId,
    state: &TextInputState,
) -> OptionTextInputSelection {
    let text = state.get_text();
    if state.kind != TextInputKind::Password {
        return engine_selection(info, node, text.len()).into();
    }
    match engine_selection(info, node, grapheme_count(&text) * MASK_LEN) {
        Some(TextInputSelection::FromTo(r)) => {
            Some(TextInputSelection::FromTo(TextInputSelectionRange {
                dir_from: masked_to_real_offset(&text, r.dir_from),
                dir_to: masked_to_real_offset(&text, r.dir_to),
            }))
            .into()
        }
        other => other.into(),
    }
}

/// The engine's caret as a byte offset into the REAL value (see
/// [`mirror_selection`]).
fn mirror_caret(info: &CallbackInfo, node: DomNodeId, state: &TextInputState) -> Option<usize> {
    let caret = engine_caret(info, node)?;
    if state.kind == TextInputKind::Password {
        Some(masked_to_real_offset(&state.get_text(), caret))
    } else {
        Some(caret)
    }
}

/// Copy and Cut on a password field: refused.
///
/// Registered only on `type=password` fields, for `FocusEventFilter::Copy` and
/// `FocusEventFilter::Cut`, which fire BEFORE the clipboard default and are
/// cancellable.
#[must_use]
pub extern "C" fn default_on_clipboard_veto(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.prevent_default();
    Update::DoNothing
}

/// Click on a `type=search` field's clear button: empty the field.
///
/// The hit node is the button; the field is its previous sibling (see
/// `search_field`). The payload is the FIELD's state, shared with the field's
/// own handlers.
#[must_use]
pub extern "C" fn default_on_search_clear_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(mut wrapper) = data.downcast_mut::<TextInputStateWrapper>() else {
        return Update::DoNothing;
    };
    let Some(container) = info.get_previous_sibling(info.get_hit_node()) else {
        return Update::DoNothing;
    };
    clear_field(&mut wrapper, info, container)
}

/// Empty the field whose host is `container`, the way an edit would: the
/// `on_text_input` hook sees the empty value first and may veto it, then the
/// mirror, the engine's line and the live looks follow.
fn clear_field(
    wrapper: &mut TextInputStateWrapper,
    mut info: CallbackInfo,
    container: DomNodeId,
) -> Update {
    if wrapper.inner.text.is_empty() {
        return Update::DoNothing;
    }
    let mut preview = wrapper.inner.clone();
    preview.text = Vec::new().into();
    preview.cursor_pos = 0;
    preview.selection = None.into();
    preview.validity = validity_of(&preview);

    let result = run_text_input_hook(wrapper, info, preview.clone());
    if result.valid == TextInputValid::No {
        return result.update;
    }
    let looks_before = looks_of(&wrapper.inner);
    wrapper.inner = preview;
    replace_engine_line(&mut info, container, "");
    sync_live_looks(&mut info, container, looks_before, &wrapper.inner);
    result.update
}

/// Replace what the engine shows on the line of the field hosted at
/// `container` with `shown` (for a password: its mask). A `TextArea` has the
/// same `container > p > text` shape and is re-texted here too.
///
/// One write, the line's text leaf: `ChangeNodeText` is the app SETTING the
/// text, and the engine lets it supersede whatever the user typed there
/// (`LayoutWindow::set_node_text`) - the edit buffer (the user's uncommitted
/// typing, which otherwise outranks the DOM until the DOM catches up) is
/// retired and the caret moves across the change. No edit is raised, so no
/// handler mirrors the new value a second time.
pub(crate) fn replace_engine_line(info: &mut CallbackInfo, container: DomNodeId, shown: &str) {
    if let Some(line) = info.get_first_child(container) {
        if let Some(leaf) = info.get_first_child(line) {
            info.change_node_text(leaf, AzString::from(shown));
        }
    }
}

/// What a field's value-dependent looks were derived from, captured BEFORE an
/// edit so [`sync_live_looks`] can tell what changed.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
struct LooksBefore {
    empty: bool,
    invalid: bool,
}

fn looks_of(state: &TextInputState) -> LooksBefore {
    LooksBefore {
        empty: state.text.is_empty(),
        invalid: !state.validity.is_valid(),
    }
}

/// Bring the parts of the field that depend on its value up to date after an
/// edit, WITHOUT a rebuild:
///
/// * the `type=search` clear button appears with the first character and hides
///   with the last - written only on that TRANSITION, because a same-value
///   `display` write costs a full relayout per keystroke;
/// * the invalid ring: painted while an edit leaves the value invalid, removed
///   when an edit makes it valid again (see [`paint_invalid_ring`]). A valid
///   edit of a valid field writes nothing.
fn sync_live_looks(
    info: &mut CallbackInfo,
    container: DomNodeId,
    before: LooksBefore,
    state: &TextInputState,
) {
    let is_empty = state.text.is_empty();
    if state.kind == TextInputKind::Search && before.empty != is_empty {
        if let Some(clear) = info.get_next_sibling(container) {
            let display = if is_empty {
                LayoutDisplay::None
            } else {
                LayoutDisplay::Block
            };
            info.set_css_property(clear, CssProperty::const_display(display));
        }
    }

    let invalid = !state.validity.is_valid();
    if invalid || before.invalid {
        paint_invalid_ring(info, container, invalid);
    }
}

/// The invalid look - CSS `:user-invalid`, not `:invalid`: it follows the
/// USER's edits. An edit that leaves the value invalid rings the field in the
/// theme's invalid colour on all four edges; an edit that makes it valid
/// REMOVES the ring (an `initial` override), which brings the resting border,
/// the hover/focus ring and the dark twins back exactly as the theme declared
/// them.
///
/// An override, not an inline declaration, because only an override can be
/// taken back at run time. The price: it outranks the hover/focus ring while
/// it stands (an invalid field is red in every state, which is the intent),
/// and its colour is the light or dark one current when it was written.
/// A value the APP hands in invalid is reported in the state (and FormData)
/// at once but not painted until the user edits it or a form submit asks
/// ([`mark_user_invalid`]) - the reason browsers added `:user-invalid`.
fn paint_invalid_ring(info: &mut CallbackInfo, container: DomNodeId, invalid: bool) {
    let Some(node_id) = container.node.into_crate_internal() else {
        return;
    };
    let props: Vec<CssProperty> = if invalid {
        let flora = info
            .get_node_classes(container)
            .as_ref()
            .iter()
            .any(|c| c.as_str() == THEME_FLORA_CLASS);
        // The one light / dark decision (`resolve_window_theme`, I1), not a
        // widget's own re-implementation of it.
        let dark =
            info.get_resolved_mode() == azul_core::window::WindowTheme::DarkMode;
        if flora {
            crate::widgets::themes::flora::text_input_invalid_ring(dark)
        } else {
            crate::widgets::themes::flat::text_input_invalid_ring(dark)
        }
    } else {
        alloc::vec![
            CssProperty::initial(CssPropertyType::BorderTopColor),
            CssProperty::initial(CssPropertyType::BorderRightColor),
            CssProperty::initial(CssPropertyType::BorderBottomColor),
            CssProperty::initial(CssPropertyType::BorderLeftColor),
        ]
    };
    info.override_node_css_properties(container.dom, node_id, props.into());
}

/// Put the field hosted at `container` back to `value` - what a form reset
/// does. The mirror, the engine's line (whatever the user typed there is
/// superseded, see [`replace_engine_line`]) and the live looks all follow; no
/// hook is asked, because a reset is the app's own action, reported to it
/// through the form's `on_reset`.
///
/// The line is re-texted even when the mirror already says `value`: the
/// screen is the ENGINE's, and what it shows is not the mirror's to vouch for.
/// A line that already shows `value` costs the engine nothing (the write is a
/// no-op there).
pub(crate) fn restore_text_input(
    info: &mut CallbackInfo,
    container: DomNodeId,
    wrapper: &mut TextInputStateWrapper,
    value: &str,
) {
    let changed = wrapper.inner.get_text() != value;
    let looks_before = looks_of(&wrapper.inner);
    if changed {
        wrapper.inner.text = to_units(value);
        wrapper.inner.cursor_pos = value.len();
        wrapper.inner.selection = None.into();
        wrapper.inner.validity = validity_of(&wrapper.inner);
    }
    let shown = display_text(&wrapper.inner);
    replace_engine_line(info, container, &shown);
    if changed {
        sync_live_looks(info, container, looks_before, &wrapper.inner);
    }
}

/// Paint the invalid look on the field hosted at `container` if its current
/// value is invalid - what a failed form submit does for every field it
/// refused (see `crate::widgets::form`), the other moment `:user-invalid`
/// starts to apply.
pub fn mark_user_invalid(info: &mut CallbackInfo, container: DomNodeId, state: &TextInputState) {
    if !state.validity.is_valid() {
        paint_invalid_ring(info, container, true);
    }
}

/// The user's `on_text_input` hook, or "accept, nothing to redraw" without one.
fn run_text_input_hook(
    wrapper: &mut TextInputStateWrapper,
    info: CallbackInfo,
    preview: TextInputState,
) -> OnTextInputReturn {
    match wrapper.on_text_input.as_mut() {
        Some(TextInputOnTextInput { callback, refany }) => {
            callback.invoke(refany.clone(), info, preview)
        }
        None => OnTextInputReturn {
            update: Update::DoNothing,
            valid: TextInputValid::Yes,
        },
    }
}

/// A recorded insertion into a PASSWORD field.
///
/// The engine's line holds one bullet per grapheme, so its caret and selection
/// are mapped onto the real value, the typed text is spliced into the REAL
/// value, the user's hook sees the real result, and - if the edit stands - the
/// pending changeset is rewritten so the engine inserts bullets instead of the
/// characters. A rejected edit is vetoed exactly like on a plain field.
fn masked_insertion(
    wrapper: &mut TextInputStateWrapper,
    mut info: CallbackInfo,
    container: DomNodeId,
    inserted: &str,
) -> Update {
    let real = wrapper.inner.get_text();
    let shown = grapheme_count(&real);

    // The replaced range, in graphemes. The engine deletes the live
    // selection before it inserts; without one it inserts at the caret.
    let (from, to) = match engine_selection(&info, container, shown * MASK_LEN) {
        Some(TextInputSelection::All) => (0, shown),
        Some(TextInputSelection::FromTo(r)) => {
            let a = (r.dir_from.min(r.dir_to) / MASK_LEN).min(shown);
            let b = (r.dir_from.max(r.dir_to) / MASK_LEN).min(shown);
            (a, b)
        }
        None => {
            let at = engine_caret(&info, container)
                .map_or(shown, |c| c / MASK_LEN)
                .min(shown);
            (at, at)
        }
    };
    let start = grapheme_byte_offset(&real, from);
    let end = grapheme_byte_offset(&real, to);

    // maxlength, counted in characters of the REAL value, replacement-aware
    // like the plain path.
    let current_chars = real.chars().count();
    let prospective = current_chars
        .saturating_sub(real[start..end].chars().count())
        .saturating_add(inserted.chars().count());
    if prospective > wrapper.inner.max_len && prospective > current_chars {
        info.prevent_default();
        return Update::DoNothing;
    }

    let mut next = String::with_capacity(real.len() + inserted.len());
    next.push_str(&real[..start]);
    next.push_str(inserted);
    next.push_str(&real[end..]);

    let mut preview = wrapper.inner.clone();
    preview.text = to_units(&next);
    preview.cursor_pos = start + inserted.len();
    preview.selection = None.into();
    preview.validity = validity_of(&preview);

    let result = run_text_input_hook(wrapper, info, preview.clone());
    if result.valid == TextInputValid::No {
        info.prevent_default();
        return result.update;
    }

    // As many bullets as the line needs to show the NEW value: normally one
    // per inserted grapheme, fewer when the insertion fused with a neighbour
    // (a combining mark typed after its base letter adds no grapheme).
    let bullets = grapheme_count(&next).saturating_sub(shown - (to - from));
    if let Some(mut changeset) = info.get_text_changeset().cloned() {
        changeset.inserted_text = AzString::from(
            core::iter::repeat_n(PASSWORD_MASK_CHAR, bullets).collect::<String>(),
        );
        info.set_text_changeset(changeset);
    }
    wrapper.inner = preview;
    result.update
}

/// A post-edit notification on a PASSWORD field: the engine already deleted
/// bullets (Backspace, Delete, a cut selection) and the real value has to lose
/// the same graphemes. See [`masked_deletion`].
fn masked_notification(
    wrapper: &mut TextInputStateWrapper,
    info: CallbackInfo,
    container: DomNodeId,
) -> Option<Update> {
    let masked = info.get_node_text_content(container)?;
    let masked_after = grapheme_count(&masked);
    let caret = engine_caret(&info, container).map_or(masked_after, |c| c / MASK_LEN);
    let next = masked_deletion(&wrapper.inner.get_text(), masked_after, caret)?;

    let cursor = grapheme_byte_offset(&next, caret.min(masked_after));
    wrapper.inner.text = to_units(&next);
    wrapper.inner.cursor_pos = cursor;
    wrapper.inner.selection = None.into();
    wrapper.inner.validity = validity_of(&wrapper.inner);

    // Already applied: the hook is told, but cannot veto.
    let preview = wrapper.inner.clone();
    Some(run_text_input_hook(wrapper, info, preview).update)
}

#[must_use]
pub extern "C" fn default_on_focus_received(
    mut text_input: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let Some(mut text_input) = text_input.downcast_mut::<TextInputStateWrapper>() else {
        return Update::DoNothing;
    };

    let text_input = &mut *text_input;

    // A text input always has its placeholder as the first child; a hit node
    // without one is not a text input.
    // A text input always has its value <p> as first child; a hit node
    // without one is not a text input.
    let Some(_value) = value_node(&info) else {
        return Update::DoNothing;
    };

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_input.inner, &info, container);

    // The prompt hides itself: the engine paints it only while the line is
    // empty AND unfocused, recomputed per display list. No toggle here, and
    // so nothing that can latch.

    // The engine seeds the caret at the end of the value when focus lands on a
    // contenteditable host; the mirror follows it.
    let end_of_text = text_input.inner.text.len();
    text_input.inner.cursor_pos =
        mirror_caret(&info, container, &text_input.inner).unwrap_or(end_of_text);

    Update::DoNothing
}

#[must_use]
pub extern "C" fn default_on_focus_lost(mut text_input: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut text_input) = text_input.downcast_mut::<TextInputStateWrapper>() else {
        return Update::DoNothing;
    };

    let text_input = &mut *text_input;

    let Some(_value) = value_node(&info) else {
        return Update::DoNothing;
    };

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_input.inner, &info, container);

    // The prompt reappears on its own once the blur lands (empty + unfocused
    // is re-evaluated on the next display list).

    // rustc doesn't understand the borrowing lifetime here
    let text_input = &mut *text_input;
    let onfocuslost = &mut text_input.on_focus_lost;
    let inner = text_input.inner.clone();

    match onfocuslost.as_mut() {
        Some(TextInputOnFocusLost { callback, refany }) => {
            callback.invoke(refany.clone(), info, inner)
        }
        None => Update::DoNothing,
    }
}

#[must_use]
pub extern "C" fn default_on_text_input(text_input: RefAny, info: CallbackInfo) -> Update {
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

    let _value = value_node(&info)?;
    let container = info.get_hit_node();
    let is_password = text_input.inner.kind == TextInputKind::Password;
    let looks_before = looks_of(&text_input.inner);

    if inserted_text.is_empty() {
        if is_password {
            let update = masked_notification(&mut text_input, info, container);
            sync_live_looks(&mut info, container, looks_before, &text_input.inner);
            return update;
        }
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
        text_input.inner.validity = validity_of(&text_input.inner);
        sync_live_looks(&mut info, container, looks_before, &text_input.inner);
        let result = {
            let text_input = &mut *text_input;
            let inner_clone = text_input.inner.clone();
            match text_input.on_text_input.as_mut() {
                Some(TextInputOnTextInput { callback, refany }) => {
                    callback.invoke(refany.clone(), info, inner_clone)
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

    if is_password {
        let update = masked_insertion(&mut text_input, info, container, &inserted_text);
        sync_live_looks(&mut info, container, looks_before, &text_input.inner);
        return Some(update);
    }

    let caret = engine_caret(&info, container);
    adopt_engine_text(&mut text_input.inner, &info, container);

    // maxlength: veto an insertion that would GROW the value past `max_len`
    // (counted in characters, the stored unit). Replacement-aware: the engine
    // deletes the live selection before inserting, so the prospective length
    // is current − selected + inserted. An edit that shrinks or holds the
    // length always passes — a select-all-and-type at the limit must not be
    // blocked, and a value pushed over the limit programmatically
    // (`set_text` is deliberately NOT capped, like a browser's programmatic
    // assignment) can still be edited back down.
    {
        let current = text_input.inner.get_text();
        let current_chars = current.chars().count();
        let selected_chars = match engine_selection(&info, container, current.len()) {
            Some(TextInputSelection::All) => current_chars,
            Some(TextInputSelection::FromTo(r)) => {
                let (a, b) = (r.dir_from.min(r.dir_to), r.dir_from.max(r.dir_to));
                if b <= current.len() && current.is_char_boundary(a) && current.is_char_boundary(b)
                {
                    current[a..b].chars().count()
                } else {
                    0
                }
            }
            None => 0,
        };
        let prospective = current_chars
            .saturating_sub(selected_chars)
            .saturating_add(inserted_text.chars().count());
        if prospective > text_input.inner.max_len && prospective > current_chars {
            info.prevent_default();
            return Some(Update::DoNothing);
        }
    }

    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let ontextinput = &mut text_input.on_text_input;

        // inner_clone has the new text
        let mut inner_clone = text_input.inner.clone();
        mirror_insertion(&mut inner_clone, &inserted_text, caret);
        let len = inner_clone.get_text().len();
        inner_clone.selection = engine_selection(&info, container, len).into();
        inner_clone.validity = validity_of(&inner_clone);

        match ontextinput.as_mut() {
            Some(TextInputOnTextInput { callback, refany }) => {
                callback.invoke(refany.clone(), info, inner_clone)
            }
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };

    if result.valid == TextInputValid::Yes {
        // No placeholder bookkeeping: the first accepted character makes the
        // line non-empty, and the engine simply stops painting the prompt on
        // the next display list.
        mirror_insertion(&mut text_input.inner, &inserted_text, caret);
        let len = text_input.inner.get_text().len();
        text_input.inner.selection = engine_selection(&info, container, len).into();
        text_input.inner.validity = validity_of(&text_input.inner);
        sync_live_looks(&mut info, container, looks_before, &text_input.inner);
    } else {
        // The engine applies the recorded changeset once the callbacks return,
        // unless one of them vetoes it.
        info.prevent_default();
    }

    Some(result.update)
}

#[must_use]
pub extern "C" fn default_on_virtual_key_down(text_input: RefAny, info: CallbackInfo) -> Update {
    default_on_virtual_key_down_inner(text_input, info).unwrap_or(Update::DoNothing)
}

fn default_on_virtual_key_down_inner(
    mut text_input: RefAny,
    mut info: CallbackInfo,
) -> Option<Update> {
    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;
    let keyboard_state = info.get_current_keyboard_state();

    let keycode = keyboard_state.current_virtual_keycode.into_option()?;
    let _value = value_node(&info)?;

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_input.inner, &info, container);

    // Editing keys (Backspace, Delete, the arrows, Enter) are the engine's
    // default actions; this handler only forwards the key to the user's hook
    // and lets a rejection stop the default from running.
    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let mut inner_clone = text_input.inner.clone();
        inner_clone.selection = mirror_selection(&info, container, &inner_clone);
        match text_input.on_virtual_key_down.as_mut() {
            Some(TextInputOnVirtualKeyDown { callback, refany }) => {
                callback.invoke(refany.clone(), info, inner_clone)
            }
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };

    text_input.inner.selection = mirror_selection(&info, container, &text_input.inner);

    if result.valid == TextInputValid::No {
        info.prevent_default();
    }

    // type=search: Escape clears a non-empty field (HTML's "cancel" action)
    // and the field KEEPS focus - the default Escape would drop it. On an
    // empty field Escape keeps its default. A hook that rejected the key
    // rejected the clear with it.
    if text_input.inner.kind == TextInputKind::Search
        && keycode == azul_core::window::VirtualKeyCode::Escape
        && result.valid == TextInputValid::Yes
        && !text_input.inner.text.is_empty()
    {
        info.prevent_default();
        let cleared = clear_field(&mut text_input, info, container);
        return Some(core::cmp::max(result.update, cleared));
    }

    // Single-line field: Enter must never edit the value. The engine-side
    // default in this host (white-space:pre) is a literal "\n" insert, so it
    // is vetoed here; activation semantics stay with the user's hook above.
    if matches!(
        keycode,
        azul_core::window::VirtualKeyCode::Return | azul_core::window::VirtualKeyCode::NumpadEnter
    ) {
        info.prevent_default();
        // HTML's implicit submission: Enter in a text field submits the form
        // it sits in - unless the hook rejected the key. (The engine's own
        // Enter-to-submit never sees a text field: Enter in an editable host
        // is a line break, which the veto above just took away.)
        if result.valid == TextInputValid::Yes {
            // Release this field's state first: the submit READS every field
            // of the form through its dataset, this one included, and a
            // shared read fails while the mutable borrow is live.
            drop(text_input);
            if let Some(submitted) =
                crate::widgets::form::submit_enclosing_form(&mut info, container)
            {
                return Some(core::cmp::max(result.update, submitted));
            }
        }
    }

    Some(result.update)
}

#[must_use]
pub extern "C" fn default_on_mouse_hover(mut text_input: RefAny, _info: CallbackInfo) -> Update {
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
    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType, ThemeCondition};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        managers::text_input::PendingTextEdit,
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::theme_probe,
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
        "\0",         // a lone NUL is a perfectly good scalar
        "a\0b",       // ... and it survives in the middle of a string, too
        "\u{7f}",     // largest 1-byte scalar
        "\u{80}",     // smallest 2-byte scalar
        "\u{7ff}",    // largest 2-byte scalar
        "\u{800}",    // smallest 3-byte scalar
        "\u{ffff}",   // largest 3-byte scalar (and a non-character)
        "\u{10000}",  // smallest 4-byte scalar
        "\u{10ffff}", // the largest scalar that exists at all
        "é",
        "e\u{301}", // e + COMBINING ACUTE: 2 scalars, 1 grapheme
        "👨‍👩‍👧‍👦",       // ZWJ family: 7 scalars, 1 grapheme, 25 bytes
        "日本語",
        "مرحبا",    // RTL
        "\u{200b}", // zero-width space: invisible, but still one scalar
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
        data.downcast_mut::<Recorder>()
            .map_or(Update::RefreshDom, |mut log| {
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

    /// The container's single `<p>` child (the value line) plus its bare text
    /// leaf, resolved through the *same* API the handlers use — so no test has
    /// to hard-code a flattened index. The prompt is an ATTRIBUTE on the value
    /// line, not a node, so there is nothing else to resolve.
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    struct Nodes {
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
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let probe = CallbackInfo::new(
            &ref_data,
            &changes,
            dom_node(CONTAINER),
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        let label = probe.get_first_child(dom_node(CONTAINER));
        let label_text = label.and_then(|l| probe.get_first_child(l));
        let nodes = Nodes { label, label_text };

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
                    node_id,
                    properties,
                    ..
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
    /// The value line is the container's ONLY child now - the prompt rides
    /// on it as a `placeholder` attribute the engine paints.
    const LABEL_CHILD: usize = 0;

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
            other => panic!(
                "a label <p> wraps exactly one text node, found {}",
                other.len()
            ),
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
        let all: Vec<CssPropertyWithConditions> = TextInput::default()
            .resolved_container_style()
            .as_slice()
            .to_vec();
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
        assert_eq!(
            rendered.chars().count(),
            units.len() - NON_SCALAR_UNITS.len()
        );
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
            text: core::iter::repeat_n(u32::from('ß'), n)
                .collect::<Vec<_>>()
                .into(),
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
        assert_eq!(
            input,
            TextInput::create(),
            "clearing did not restore a fresh widget"
        );
    }

    #[test]
    fn set_text_does_not_enforce_max_len() {
        // INTENDED: `max_len` caps USER input (the text-input handler vetoes a
        // growing keystroke), but a PROGRAMMATIC assignment is stored whole —
        // the same split a browser makes: `maxlength` never truncates a value
        // set from script. The default is unlimited (`usize::MAX`), like an
        // <input> without `maxlength`.
        let long: String = "x".repeat(200);
        let mut input = TextInput::create().with_text(long.clone().into());
        assert_eq!(input.text_input_state.inner.max_len, usize::MAX);
        assert_eq!(input.text_input_state.inner.text.len(), 200);
        assert_eq!(input.text_input_state.inner.get_text(), long);
        // Even a field WITH a limit accepts a longer programmatic value.
        input.text_input_state.inner.max_len = 50;
        input.set_text("y".repeat(80).into());
        assert_eq!(input.text_input_state.inner.text.len(), 80);
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
        let mut input = TextInput::create().with_placeholder("type here".into());
        let before = input.clone();
        input.set_text("abc".into());

        assert_eq!(
            input
                .text_input_state
                .inner
                .placeholder
                .as_ref()
                .map(|s| s.as_str().to_string()),
            Some("type here".to_string()),
        );
        assert_eq!(
            input.resolved_container_style(),
            before.resolved_container_style()
        );
        assert_eq!(input.resolved_label_style(), before.resolved_label_style());
        assert_eq!(
            input.text_input_state.inner.max_len,
            before.text_input_state.inner.max_len
        );
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
        assert!(TextInput::create()
            .text_input_state
            .inner
            .placeholder
            .is_none());
    }

    #[test]
    fn with_placeholder_is_exactly_set_placeholder_and_stores_the_string_verbatim() {
        for s in HOSTILE {
            let mut a = TextInput::create();
            a.set_placeholder(s.into());
            let b = TextInput::create().with_placeholder(s.into());
            assert_eq!(
                a, b,
                "with_placeholder and set_placeholder disagree on {s:?}"
            );

            assert_eq!(
                a.text_input_state
                    .inner
                    .placeholder
                    .as_ref()
                    .map(|p| p.as_str()),
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
            input
                .text_input_state
                .inner
                .placeholder
                .as_ref()
                .map(|p| p.as_str()),
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

        let mut b = TextInput::create();
        b.set_container_style(marker.clone());
        assert_eq!(b.resolved_container_style(), marker);
        assert_eq!(
            b.resolved_label_style(),
            TextInput::create().resolved_label_style()
        );

        let mut c = TextInput::create();
        c.set_label_style(marker.clone());
        assert_eq!(c.resolved_label_style(), marker);
        assert_eq!(
            c.resolved_container_style(),
            TextInput::create().resolved_container_style()
        );
    }

    #[test]
    fn the_with_style_builders_are_exactly_their_setters() {
        let s = style(2);

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
            .with_container_style(empty.clone())
            .with_label_style(empty.clone());
        assert!(input.resolved_container_style().is_empty());

        // Stripping every declared property must not stop the widget from rendering.
        let dom = input.dom();
        assert_eq!(dom.children.as_ref().len(), 1);
    }

    #[test]
    fn style_setters_overwrite_rather_than_merge() {
        let mut input = TextInput::create();
        input.set_container_style(style(4));
        input.set_container_style(style(1));
        assert_eq!(input.resolved_container_style().len(), 1);
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
            *payload
                .downcast_ref::<u32>()
                .expect("the payload changed type"),
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
        a.set_on_text_input(
            payload.clone(),
            record_text_input as TextInputOnTextInputCallbackType,
        );
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
        c.set_on_focus_lost(
            payload.clone(),
            record_focus_lost as TextInputOnFocusLostCallbackType,
        );
        assert_eq!(
            c,
            TextInput::create().with_on_focus_lost(
                payload,
                record_focus_lost as TextInputOnFocusLostCallbackType
            ),
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

        let slot = input
            .text_input_state
            .on_text_input
            .as_ref()
            .expect("slot is empty");
        assert_eq!(
            slot.callback.cb as *const () as usize,
            record_virtual_key as TextInputOnTextInputCallbackType as *const () as usize,
            "the second assignment did not win",
        );
        let mut payload = slot.refany.clone();
        assert_eq!(
            *payload.downcast_ref::<u8>().expect("wrong payload type"),
            2
        );
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
        // Unlimited by default, like an <input> without `maxlength`.
        assert_eq!(input.text_input_state.inner.max_len, usize::MAX);
        assert!(input.text_input_state.on_text_input.as_ref().is_none());
        assert!(input
            .text_input_state
            .on_virtual_key_down
            .as_ref()
            .is_none());
        assert!(input.text_input_state.on_focus_lost.as_ref().is_none());
        assert!(input.text_input_state.cursor_animation.is_none());
        assert!(
            input
                .text_input_state
                .update_text_input_before_calling_focus_lost_fn
        );
        assert!(
            input
                .text_input_state
                .update_text_input_before_calling_vk_down_fn
        );
        assert!(!input.resolved_container_style().is_empty());
    }

    #[test]
    fn swap_with_default_returns_the_old_widget_and_leaves_a_fresh_one_behind() {
        let mut input = TextInput::create()
            .with_text("typed".into())
            .with_placeholder("hint".into());
        let old = input.swap_with_default();

        assert_eq!(old.text_input_state.inner.get_text(), "typed");
        assert_eq!(
            input,
            TextInput::create(),
            "what was left behind is not a fresh widget"
        );
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
    fn dom_builds_a_container_with_exactly_one_value_block() {
        let dom = TextInput::create().dom();

        assert_eq!(classes(&dom), vec!["__azul-native-text-input-container"]);
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "the prompt is an attribute, not a second child"
        );

        let label = &dom.children.as_ref()[LABEL_CHILD];
        assert_eq!(classes(label), vec!["__azul-native-text-input-label"]);
        assert!(matches!(label.root.get_node_type(), NodeType::P));
        assert_eq!(label.children.as_ref().len(), 1);
        assert!(matches!(
            label.children.as_ref()[0].root.get_node_type(),
            NodeType::Text(_),
        ));
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
        walk(
            &TextInput::create().with_text("typed".into()).dom(),
            &mut all,
        );
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
            TextInput::create()
                .with_text("typed".into())
                .with_placeholder("hint".into()),
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
        // The prompt must never be typed into. It cannot be: it is an
        // ATTRIBUTE the engine paints, so it owns no node inside the
        // contenteditable host and no text an edit could reach.
        let dom = TextInput::create().with_placeholder("hint".into()).dom();
        let label = &dom.children.as_ref()[LABEL_CHILD];
        assert_eq!(label.root.get_placeholder(), Some("hint"));
        assert_eq!(
            text_of(label),
            "",
            "the prompt is not part of the editable content"
        );
        assert!(!label
            .root
            .attributes()
            .as_ref()
            .iter()
            .any(|a| matches!(a, AttributeType::ContentEditable(_))));
    }

    #[test]
    fn dom_never_bakes_placeholder_visibility_into_css() {
        // The widget used to bake `display:none` onto its overlay prompt when
        // the buffer was non-empty - a build-time snapshot of a RUNTIME fact,
        // which is exactly what latched. The DOM must now be identical either
        // way: whether the prompt shows is decided per display list, by the
        // engine, from (empty && unfocused).
        let filled = TextInput::create().with_text("typed".into()).dom();
        let empty = TextInput::create().dom();

        let display = |node: &Dom| -> Option<CssProperty> {
            node.root
                .style
                .iter_inline_properties()
                .map(|(p, _)| p.clone())
                .filter(|p| matches!(p, CssProperty::Display(_)))
                .last()
        };

        assert_eq!(
            display(&filled.children.as_ref()[LABEL_CHILD]),
            display(&empty.children.as_ref()[LABEL_CHILD]),
            "the value line's css must not depend on whether it has text",
        );
        assert_eq!(
            filled.children.as_ref()[LABEL_CHILD].root.get_placeholder(),
            empty.children.as_ref()[LABEL_CHILD].root.get_placeholder(),
            "and neither must the prompt attribute",
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
            assert_eq!(
                c.refany, callbacks[0].refany,
                "a handler got its own state copy"
            );
        }
        assert_eq!(
            dom.root.get_dataset().expect("no dataset attached"),
            &callbacks[0].refany,
        );
    }

    #[test]
    fn dom_renders_the_buffer_into_the_label_and_the_prompt_into_the_attribute() {
        let dom = TextInput::create()
            .with_text("typed".into())
            .with_placeholder("hint".into())
            .dom();

        let label = &dom.children.as_ref()[LABEL_CHILD];
        assert_eq!(text_of(label), "typed");
        assert_eq!(label.root.get_placeholder(), Some("hint"));
    }

    #[test]
    fn dom_without_a_placeholder_still_carries_an_empty_attribute() {
        // The handlers navigate `container -> first child`; the attribute is
        // always present (empty when unset) and the engine paints nothing for
        // an empty prompt.
        let dom = TextInput::create().with_text("typed".into()).dom();
        assert_eq!(dom.children.as_ref().len(), 1);
        assert_eq!(
            dom.children.as_ref()[LABEL_CHILD].root.get_placeholder(),
            Some("")
        );
    }

    #[test]
    fn dom_passes_hostile_text_and_placeholder_through_unchanged() {
        for s in HOSTILE {
            let dom = TextInput::create()
                .with_text(s.into())
                .with_placeholder(s.into())
                .dom();
            assert_eq!(
                text_of(&dom.children.as_ref()[LABEL_CHILD]),
                s,
                "label mangled {s:?}"
            );
            assert_eq!(
                dom.children.as_ref()[LABEL_CHILD].root.get_placeholder(),
                Some(s),
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
        let label_style = style(2);
        let container_style = style(3);
        // One theme's field: unpinned, the field follows the app theme and
        // repeats a property the themes twin differently in each theme's block.
        let dom = TextInput::create()
            .with_label_style(label_style.clone())
            .with_container_style(container_style.clone())
            .with_theme(crate::widgets::themes::UiTheme::Flat)
            .dom();

        let declared = |v: &CssPropertyWithConditionsVec| -> Vec<CssProperty> {
            v.as_ref().iter().map(|p| p.property.clone()).collect()
        };

        // The theme appends its dark twins AND the field's hover/focus border
        // states on top of what the caller set, so only the resting,
        // unconditional half of the container's style is the caller's verbatim.
        // The label carries no state rules, so the stricter probe still holds
        // there.
        assert_eq!(theme_probe::unconditional(&dom), declared(&container_style));
        assert_eq!(
            theme_probe::unthemed(&dom.children.as_ref()[LABEL_CHILD]),
            declared(&label_style)
        );
    }

    #[test]
    fn dom_gives_the_container_and_the_label_a_dark_mode_twin() {
        // A caller-supplied style replaces the widget's own, but it must not
        // disable the theme's dark half — otherwise setting any style at all
        // leaves the field painting a light fill on a dark surface.
        let dom = TextInput::create()
            .with_container_style(style(3))
            .with_label_style(style(2))
            .dom();

        assert!(
            theme_probe::dark(&dom)
                .iter()
                .any(|p| matches!(p, CssProperty::BackgroundContent(_))),
            "the container keeps its light fill in dark mode",
        );
        assert!(
            theme_probe::dark(&dom.children.as_ref()[LABEL_CHILD])
                .iter()
                .any(|p| matches!(p, CssProperty::TextColor(_))),
            "the label keeps its light ink in dark mode",
        );
    }

    #[test]
    fn dom_carries_the_themes_hover_and_focus_border_states_with_dark_twins() {
        // The rules moved OUT of `TEXT_INPUT_CONTAINER_PROPS` and into the theme
        // modules, which is a move nothing else in this suite would notice: no
        // compiler error, and every other assertion here still passes if a
        // theme silently forgets to append them. Hence this test — and hence it
        // asks BOTH themes, since each `text_input()` appends the array itself.
        use crate::widgets::themes::{OptionUiTheme, UiTheme};

        for (name, theme) in [("flat", UiTheme::Flat), ("flora", UiTheme::Flora)] {
            let mut input = TextInput::create();
            input.theme = OptionUiTheme::Some(theme);
            let dom = input.dom();

            let conditioned = |want_dark: bool, want_focus: bool| -> usize {
                dom.root
                    .style
                    .iter_inline_properties()
                    .filter(|(p, conds)| {
                        let is_border = matches!(
                            p,
                            CssProperty::BorderTopColor(_)
                                | CssProperty::BorderBottomColor(_)
                                | CssProperty::BorderLeftColor(_)
                                | CssProperty::BorderRightColor(_)
                        );
                        let mut dark = false;
                        let mut state_matches = false;
                        for c in conds.as_ref() {
                            match c {
                                DynamicSelector::Theme(ThemeCondition::Dark) => dark = true,
                                DynamicSelector::PseudoState(PseudoStateType::Focus) => {
                                    state_matches = want_focus;
                                }
                                DynamicSelector::PseudoState(PseudoStateType::Hover) => {
                                    state_matches = !want_focus;
                                }
                                _ => {}
                            }
                        }
                        is_border && state_matches && dark == want_dark
                    })
                    .count()
            };

            for (state, want_focus) in [("hover", false), ("focus", true)] {
                assert_eq!(
                    conditioned(false, want_focus),
                    4,
                    "{name} {state}: all four border edges must take the accent, or the ring is \
                     drawn on some sides only",
                );
                assert_eq!(
                    conditioned(true, want_focus),
                    4,
                    "{name} {state}: the dark twin is missing, so the field keeps its light-mode \
                     ring on a dark surface",
                );
            }

            // And the dark declarations really are gated, not unconditional.
            assert!(
                !theme_probe::dark(&dom).is_empty(),
                "{name}: the theme contributed no dark-mode declarations at all"
            );
        }
    }

    #[test]
    fn the_rendered_tree_flattens_to_the_shape_the_handlers_navigate() {
        let (styled_dom, _) = rendered(TextInput::create());
        let ((), _, nodes) = run(Env::new(styled_dom), |_| ());

        let label = nodes.label.expect("the container has no first child");
        let label_text = nodes.label_text.expect("the value block has no text leaf");

        assert_ne!(label, label_text);
        assert_ne!(label, dom_node(CONTAINER));
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
        assert!(
            changes.is_empty(),
            "a foreign payload still produced {changes:?}"
        );
    }

    #[test]
    fn focus_received_on_a_node_with_no_children_bails_out_before_touching_css() {
        // `set_css_property` panics on a `None` node id, and every handler
        // bails when the hit node has no children. The escape must happen
        // before any write.
        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, nodes) = run(Env::new(styled_dom).hit(node_none()), |info| {
            default_on_focus_received(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(nodes.label.is_some(), "the fixture itself is malformed");
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
        assert!(
            changes.is_empty(),
            "a childless hit node still pushed {changes:?}"
        );
    }

    #[test]
    fn focus_writes_no_placeholder_css_at_all() {
        // 2026-08-31 ruling: focusing an empty field HIDES its placeholder -
        // the rule TextArea always had. The empty-editable strut caret marks
        // the focused field, so the old blank-field concern is gone;
        // `focus_lost_shows_the_placeholder_only_while_the_buffer_is_empty`
        // pins the symmetric restore.
        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, nodes) = run(Env::new(styled_dom), |info| {
            default_on_focus_received(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "focus must write NO css: the engine paints the prompt only while the line is empty \
             AND unfocused, so there is no state to toggle and nothing that can latch: {changes:?}",
        );
        let _ = nodes;

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
    fn blur_writes_no_placeholder_css_at_all() {
        let (styled_dom, state) = rendered(TextInput::create());
        let (update, changes, nodes) = run(Env::new(styled_dom), |info| {
            default_on_focus_lost(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "blur must write NO css either — the prompt reappears because the next display list \
             sees empty+unfocused: {changes:?}",
        );
        let _ = nodes;

        let (styled_dom, state) = rendered(TextInput::create().with_text("typed".into()));
        let (_, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_focus_lost(state.clone(), info)
        });
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
                .with_on_focus_lost(
                    probe.clone(),
                    record_focus_lost as TextInputOnFocusLostCallbackType,
                ),
        );

        let (update, _, _) = run(Env::new(styled_dom), |info| {
            default_on_focus_lost(state.clone(), info)
        });

        assert_eq!(
            update,
            Update::RefreshDomAllWindows,
            "the hook's Update was swallowed"
        );
        let seen = recorded(&probe);
        assert_eq!(
            seen.len(),
            1,
            "the hook fired {} times, expected once",
            seen.len()
        );
        assert_eq!(seen[0].get_text(), "typed");
        assert_eq!(
            seen[0].cursor_pos, 5,
            "the hook saw a cursor that dom() should have synced"
        );
    }

    #[test]
    fn focus_lost_without_a_hook_reports_no_work_to_do() {
        let (styled_dom, state) = rendered(TextInput::create().with_text("typed".into()));
        let (update, _, _) = run(Env::new(styled_dom), |info| {
            default_on_focus_lost(state.clone(), info)
        });
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
        assert!(
            recorded(&probe).is_empty(),
            "the hook fired on a hit node that does not exist"
        );
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
        let (update, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_text_input(state.clone(), info)
        });
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
        assert!(
            changes.is_empty(),
            "an empty insertion still repainted: {changes:?}"
        );
        assert_eq!(state_of(&state).get_text(), "abc");
        assert_eq!(state_of(&state).cursor_pos, 3);
    }

    #[test]
    fn text_input_mirrors_the_insertion_and_hides_the_placeholder() {
        let (styled_dom, state) = rendered(TextInput::create().with_placeholder("hint".into()));
        let (update, changes, nodes) = run(Env::new(styled_dom).insert("hi"), |info| {
            default_on_text_input(state.clone(), info)
        });

        assert_eq!(
            update,
            Update::DoNothing,
            "no hook is installed, so nothing needs redrawing"
        );
        assert_eq!(state_of(&state).get_text(), "hi");
        assert_eq!(state_of(&state).cursor_pos, 2);

        assert!(
            pushed_opacities(&changes).is_empty(),
            "typing must not toggle any placeholder override: {changes:?}",
        );
        let _ = nodes;
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
                .with_on_text_input(
                    probe.clone(),
                    record_text_input as TextInputOnTextInputCallbackType,
                ),
        );

        let (update, _, _) = run(Env::new(styled_dom).insert("c"), |info| {
            default_on_text_input(state.clone(), info)
        });

        assert_eq!(
            update,
            Update::RefreshDom,
            "the hook's Update was swallowed"
        );
        let seen = recorded(&probe);
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].get_text(),
            "abc",
            "the hook was shown the pre-edit buffer"
        );
        assert_eq!(seen[0].cursor_pos, 3);
    }

    #[test]
    fn text_input_rejected_by_the_hook_leaves_the_buffer_and_the_screen_untouched() {
        let probe = recorder(Update::RefreshDomAllWindows, TextInputValid::No);
        let (styled_dom, state) = rendered(
            TextInput::create()
                .with_text("ab".into())
                .with_placeholder("hint".into())
                .with_on_text_input(
                    probe.clone(),
                    record_text_input as TextInputOnTextInputCallbackType,
                ),
        );

        let (update, changes, _) = run(Env::new(styled_dom).insert("c"), |info| {
            default_on_text_input(state.clone(), info)
        });

        assert_eq!(
            update,
            Update::RefreshDomAllWindows,
            "a rejected edit still reports its Update"
        );
        assert_eq!(
            state_of(&state).get_text(),
            "ab",
            "a rejected edit was applied anyway"
        );
        assert_eq!(
            state_of(&state).cursor_pos,
            2,
            "a rejected edit still moved the cursor"
        );
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
        assert_eq!(
            after.cursor_pos, 2,
            "but the cursor moved by the two UTF-8 bytes"
        );
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

        assert_eq!(
            result, None,
            "the handler claimed to have handled a malformed tree"
        );
        assert!(changes.is_empty());
        assert_eq!(
            state_of(&state).get_text(),
            "ab",
            "the buffer changed anyway"
        );
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
    fn text_input_default_max_len_is_unlimited() {
        // The default is unlimited, like an <input> without `maxlength`: an
        // 80-char insertion into a fresh field is accepted whole.
        let (styled_dom, state) = rendered(TextInput::create());
        let filler: String = "x".repeat(80);
        let (_, _, _) = run(Env::new(styled_dom).insert(&filler), |info| {
            default_on_text_input(state.clone(), info)
        });
        let after = state_of(&state);
        assert_eq!(after.max_len, usize::MAX);
        assert_eq!(after.text.len(), 80);
    }

    #[test]
    fn typing_past_max_len_is_vetoed() {
        // maxlength: a keystroke that would GROW the value past `max_len` is
        // vetoed — the widget state keeps the old text and `PreventDefault`
        // stops the engine from applying the recorded changeset. (Programmatic
        // `set_text` stays uncapped; see `set_text_does_not_enforce_max_len`.)
        let mut input = TextInput::create().with_text("abc".into());
        input.text_input_state.inner.max_len = 3;
        let (styled_dom, state) = rendered(input);
        let (_, changes, _) = run(Env::new(styled_dom).insert("d"), |info| {
            default_on_text_input(state.clone(), info)
        });
        assert_eq!(
            state_of(&state).get_text(),
            "abc",
            "the over-limit keystroke was applied anyway",
        );
        assert!(
            changes
                .iter()
                .any(|c| matches!(c, CallbackChange::PreventDefault)),
            "the over-limit keystroke did not veto the engine changeset: {changes:?}",
        );
    }

    // ==================================================================
    // default_on_virtual_key_down
    // ==================================================================

    #[test]
    fn virtual_key_down_without_a_pressed_key_does_nothing() {
        let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
        let (styled_dom, state) = rendered(
            TextInput::create()
                .with_text("ab".into())
                .with_on_virtual_key_down(
                    probe.clone(),
                    record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
                ),
        );

        let (update, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_virtual_key_down(state.clone(), info)
        });

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(
            recorded(&probe).is_empty(),
            "the hook fired without a key being down"
        );
        assert_eq!(state_of(&state).get_text(), "ab");
    }

    #[test]
    fn backspace_is_the_engines_default_action_not_the_widgets() {
        // Deletion is `SystemChange::ApplySelectionOp` on the engine side; a
        // widget that also popped its own buffer would double-delete.
        let (styled_dom, state) = rendered(TextInput::create().with_text("abc".into()));
        let (update, changes, _) = run(Env::new(styled_dom).key(VirtualKeyCode::Back), |info| {
            default_on_virtual_key_down(state.clone(), info)
        });

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "backspace still mutated the DOM: {changes:?}"
        );
        assert_eq!(
            state_of(&state).get_text(),
            "abc",
            "the widget deleted behind the engine"
        );
    }

    #[test]
    fn every_key_reaches_the_hook_and_leaves_the_buffer_alone() {
        for key in [
            VirtualKeyCode::A,
            VirtualKeyCode::Back,
            VirtualKeyCode::Return,
        ] {
            let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
            let (styled_dom, state) = rendered(
                TextInput::create()
                    .with_text("ab".into())
                    .with_on_virtual_key_down(
                        probe.clone(),
                        record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
                    ),
            );

            let (update, changes, _) = run(Env::new(styled_dom).key(key), |info| {
                default_on_virtual_key_down(state.clone(), info)
            });

            assert_eq!(
                update,
                Update::RefreshDom,
                "{key:?} swallowed the hook's Update"
            );
            if matches!(key, VirtualKeyCode::Return) {
                // Single-line field: Enter must never edit the value, so the
                // handler vetoes the engine's "\n" default — and nothing else.
                assert_eq!(
                    changes.len(),
                    1,
                    "Return must veto exactly once: {changes:?}"
                );
                assert!(
                    matches!(changes[0], CallbackChange::PreventDefault),
                    "Return must veto the engine line break, and only that: {changes:?}"
                );
            } else {
                assert!(
                    changes.is_empty(),
                    "{key:?} repainted the value: {changes:?}"
                );
            }
            assert_eq!(state_of(&state).get_text(), "ab");
            assert_eq!(recorded(&probe).len(), 1, "the hook must see {key:?} too");
        }
    }

    #[test]
    fn a_rejecting_hook_vetoes_the_engines_default_and_keeps_its_update() {
        let probe = recorder(Update::RefreshDomAllWindows, TextInputValid::No);
        let (styled_dom, state) = rendered(
            TextInput::create()
                .with_text("abc".into())
                .with_on_virtual_key_down(
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
            TextInput::create()
                .with_text("abc".into())
                .with_on_virtual_key_down(
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
            TextInput::create()
                .with_text("abc".into())
                .with_on_virtual_key_down(
                    probe.clone(),
                    record_virtual_key as TextInputOnVirtualKeyDownCallbackType,
                ),
        );

        let (result, changes, _) = run(
            Env::new(styled_dom)
                .hit(node_none())
                .key(VirtualKeyCode::Back),
            |info| default_on_virtual_key_down_inner(state.clone(), info),
        );

        assert_eq!(result, None);
        assert!(changes.is_empty());
        assert!(
            recorded(&probe).is_empty(),
            "the hook fired on a hit node that does not exist"
        );
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
            assert_eq!(
                update,
                Update::DoNothing,
                "backspace #{i} reported work to do"
            );
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
        let (update, changes, _) = run(Env::new(styled_dom), |info| {
            default_on_mouse_hover(state.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(
            state_of(&state).get_text(),
            "abc",
            "hovering edited the buffer"
        );

        let (styled_dom, _) = rendered(TextInput::create());
        let foreign = RefAny::new(0_u8);
        let (update, changes, _) = run(Env::new(styled_dom).hit(node_none()), |info| {
            default_on_mouse_hover(foreign.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    // ==================================================================
    // <input type=password>: a masking mode of the same widget
    // ==================================================================

    mod password {
        use azul_core::a11y::{AccessibilityRole, AccessibilityState};

        use super::*;

        const BULLET: &str = "\u{2022}";

        fn bullets(n: usize) -> String {
            BULLET.repeat(n)
        }

        fn rewritten_insertions(changes: &[CallbackChange]) -> Vec<String> {
            changes
                .iter()
                .filter_map(|c| match c {
                    CallbackChange::SetTextChangeset { changeset } => {
                        Some(changeset.inserted_text.as_str().to_string())
                    }
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn a_password_input_shows_one_bullet_per_grapheme() {
            // `e` + COMBINING ACUTE is ONE grapheme, and so is the ZWJ family:
            // a user who typed three things sees three bullets, however many
            // scalars or bytes they are.
            let dom = TextInput::create_password()
                .with_text("ae\u{301}\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}".into())
                .dom();
            assert_eq!(text_of(&dom.children.as_ref()[LABEL_CHILD]), bullets(3));
        }

        #[test]
        fn a_password_input_keeps_the_real_text_in_its_state() {
            let dom = TextInput::create_password()
                .with_text("hunter2".into())
                .dom();
            let state = dataset_state(&dom);
            assert_eq!(state.get_text(), "hunter2");
            assert_eq!(state.kind, TextInputKind::Password);
        }

        #[test]
        fn a_password_input_is_announced_as_protected_text_without_its_value() {
            let dom = TextInput::create_password()
                .with_text("hunter2".into())
                .dom();
            let a11y = dom
                .root
                .get_accessibility_info()
                .expect("a password field must declare its accessibility info");
            assert_eq!(a11y.role, AccessibilityRole::Text);
            assert!(
                a11y.states
                    .as_ref()
                    .contains(&AccessibilityState::Protected),
                "a password field must carry the Protected state"
            );
            assert!(
                a11y.accessibility_value.is_none(),
                "the password leaked into the accessibility value"
            );
        }

        #[test]
        fn a_password_input_declares_its_html_type_for_the_soft_keyboard() {
            let dom = TextInput::create_password().dom();
            assert!(dom.root.attributes().as_ref().iter().any(
                |a| matches!(a, AttributeType::InputType(t) if t.as_str() == "password")
            ));
        }

        #[test]
        fn a_plain_text_input_carries_no_type_attribute() {
            let dom = TextInput::create().dom();
            assert!(!dom
                .root
                .attributes()
                .as_ref()
                .iter()
                .any(|a| matches!(a, AttributeType::InputType(_))));
        }

        #[test]
        fn a_password_input_vetoes_copy_and_cut() {
            let dom = TextInput::create_password().dom();
            let callbacks = dom.root.callbacks.as_ref();
            for filter in [FocusEventFilter::Copy, FocusEventFilter::Cut] {
                let cb = callbacks
                    .iter()
                    .find(|c| c.event == EventFilter::Focus(filter))
                    .unwrap_or_else(|| panic!("no {filter:?} handler on a password field"));
                assert_eq!(cb.callback.cb, default_on_clipboard_veto as usize);
            }

            let (styled_dom, state) = rendered(TextInput::create_password());
            let (update, changes, _) = run(Env::new(styled_dom), |info| {
                default_on_clipboard_veto(state.clone(), info)
            });
            assert_eq!(update, Update::DoNothing);
            assert!(
                changes
                    .iter()
                    .any(|c| matches!(c, CallbackChange::PreventDefault)),
                "the clipboard handler did not veto the copy"
            );
        }

        #[test]
        fn a_plain_text_input_leaves_copy_and_cut_alone() {
            let dom = TextInput::create().dom();
            assert!(!dom.root.callbacks.as_ref().iter().any(|c| matches!(
                c.event,
                EventFilter::Focus(FocusEventFilter::Copy | FocusEventFilter::Cut)
            )));
        }

        #[test]
        fn typing_into_a_password_input_stores_the_character_and_shows_a_bullet() {
            let (styled_dom, state) =
                rendered(TextInput::create_password().with_text("ab".into()));
            let (_, changes, _) = run(Env::new(styled_dom).insert("c"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert_eq!(state_of(&state).get_text(), "abc");
            assert_eq!(
                rewritten_insertions(&changes),
                vec![bullets(1)],
                "the engine must insert a bullet, never the typed character"
            );
            assert!(!changes
                .iter()
                .any(|c| matches!(c, CallbackChange::PreventDefault)));
        }

        #[test]
        fn a_multi_grapheme_paste_into_a_password_input_shows_one_bullet_each() {
            let (styled_dom, state) = rendered(TextInput::create_password());
            let (_, changes, _) = run(Env::new(styled_dom).insert("x\u{e9}e\u{301}"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert_eq!(state_of(&state).get_text(), "x\u{e9}e\u{301}");
            assert_eq!(rewritten_insertions(&changes), vec![bullets(3)]);
        }

        #[test]
        fn a_password_input_hands_its_hook_the_real_text() {
            let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
            let (styled_dom, state) = rendered(
                TextInput::create_password()
                    .with_text("ab".into())
                    .with_on_text_input(
                        probe.clone(),
                        record_text_input as TextInputOnTextInputCallbackType,
                    ),
            );
            let (update, _, _) = run(Env::new(styled_dom).insert("c"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert_eq!(update, Update::RefreshDom);
            let seen = recorded(&probe);
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].get_text(), "abc");
        }

        #[test]
        fn a_rejected_keystroke_in_a_password_input_is_vetoed_and_not_stored() {
            let probe = recorder(Update::DoNothing, TextInputValid::No);
            let (styled_dom, state) = rendered(
                TextInput::create_password()
                    .with_text("ab".into())
                    .with_on_text_input(
                        probe.clone(),
                        record_text_input as TextInputOnTextInputCallbackType,
                    ),
            );
            let (_, changes, _) = run(Env::new(styled_dom).insert("c"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert_eq!(state_of(&state).get_text(), "ab");
            assert!(changes
                .iter()
                .any(|c| matches!(c, CallbackChange::PreventDefault)));
            assert!(rewritten_insertions(&changes).is_empty());
        }

        #[test]
        fn a_password_input_still_honours_max_len() {
            let mut input = TextInput::create_password().with_text("abc".into());
            input.text_input_state.inner.max_len = 3;
            let (styled_dom, state) = rendered(input);
            let (_, changes, _) = run(Env::new(styled_dom).insert("d"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert_eq!(state_of(&state).get_text(), "abc");
            assert!(changes
                .iter()
                .any(|c| matches!(c, CallbackChange::PreventDefault)));
        }

        #[test]
        fn deleting_bullets_removes_the_same_graphemes_from_the_real_text() {
            // [a, e+acute, b]: the caret sits after bullet 1 once bullet 2 is
            // gone, so the real grapheme at index 1 is the one removed.
            assert_eq!(
                masked_deletion("ae\u{301}b", 2, 1),
                Some("ab".to_string())
            );
            // Select-all + Backspace.
            assert_eq!(masked_deletion("abc", 0, 0), Some(String::new()));
            // A Backspace at the very end.
            assert_eq!(masked_deletion("abc", 2, 2), Some("ab".to_string()));
            // Nothing removed: nothing to mirror.
            assert_eq!(masked_deletion("abc", 3, 1), None);
            // The line GREW without characters (an undo): the real text cannot
            // be reconstructed from bullets, so the mirror is left alone.
            assert_eq!(masked_deletion("abc", 4, 1), None);
        }

        #[test]
        fn the_masked_caret_maps_onto_grapheme_boundaries_of_the_real_text() {
            let real = "ae\u{301}b";
            assert_eq!(masked_to_real_offset(real, 0), 0);
            assert_eq!(masked_to_real_offset(real, BULLET.len()), 1);
            assert_eq!(masked_to_real_offset(real, 2 * BULLET.len()), 4);
            assert_eq!(masked_to_real_offset(real, 3 * BULLET.len()), real.len());
            // Past the end clamps to the end.
            assert_eq!(masked_to_real_offset(real, 99 * BULLET.len()), real.len());
        }
    }

    // ==================================================================
    // <input type=search>: a clear button while non-empty, Escape clears
    // ==================================================================

    mod search {
        use azul_core::a11y::AccessibilityRole;

        use super::*;

        // Flattened: wrapper(0) > field(1) > line <p>(2) > text(3),
        //            clear <p>(4) > text(5).
        const FIELD: usize = 1;
        const LINE_TEXT: usize = 3;
        const CLEAR: usize = 4;

        fn field_of(dom: &Dom) -> &Dom {
            &dom.children.as_ref()[0]
        }

        fn clear_of(dom: &Dom) -> &Dom {
            &dom.children.as_ref()[1]
        }

        fn rendered_search(input: TextInput) -> (StyledDom, RefAny) {
            let dom = input.dom();
            let state = field_of(&dom)
                .root
                .get_dataset()
                .cloned()
                .expect("the search FIELD carries the widget state");
            (StyledDom::create_from_dom(dom), state)
        }

        /// The resting (unconditional) `display` the node was built with.
        fn built_display(node: &Dom) -> Option<LayoutDisplay> {
            node.root
                .style
                .iter_inline_properties()
                .filter(|(_, conds)| conds.as_ref().is_empty())
                .filter_map(|(p, _)| match p {
                    CssProperty::Display(v) => v.get_property().cloned(),
                    _ => None,
                })
                .last()
        }

        /// Every `display` a handler pushed onto `node`, in push order.
        fn displays_pushed_to(changes: &[CallbackChange], node: usize) -> Vec<LayoutDisplay> {
            changes
                .iter()
                .filter_map(|c| match c {
                    CallbackChange::ChangeNodeCssProperties {
                        node_id,
                        properties,
                        ..
                    } if *node_id == NodeId::new(node) => {
                        properties.as_ref().iter().find_map(|p| match p {
                            CssProperty::Display(v) => v.get_property().cloned(),
                            _ => None,
                        })
                    }
                    _ => None,
                })
                .collect()
        }

        #[test]
        fn a_search_input_is_its_field_followed_by_a_clear_button() {
            let dom = TextInput::create_search().with_text("abc".into()).dom();
            assert_eq!(dom.children.as_ref().len(), 2);

            let field = field_of(&dom);
            assert_eq!(classes(field), vec![TEXT_INPUT_CONTAINER_CLASS.to_string()]);
            assert!(field.root.is_contenteditable());
            assert!(field.root.attributes().as_ref().iter().any(
                |a| matches!(a, AttributeType::InputType(t) if t.as_str() == "search")
            ));

            let clear = clear_of(&dom);
            let a11y = clear
                .root
                .get_accessibility_info()
                .expect("the clear button must be announced");
            assert_eq!(a11y.role, AccessibilityRole::PushButton);
            assert!(a11y.accessibility_name.is_some(), "the clear button has no name");
            assert!(
                clear
                    .root
                    .callbacks
                    .as_ref()
                    .iter()
                    .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click)
                        && c.callback.cb == default_on_search_clear_click as usize),
                "the clear button does not clear on click"
            );
            // The clear button must not sit INSIDE the editable host, or a
            // click on it would place a caret in its glyph.
            assert_eq!(field.children.as_ref().len(), 1);
        }

        #[test]
        fn the_clear_button_is_hidden_while_the_search_field_is_empty() {
            // One theme's field: the flat and flora buttons show as different
            // displays, so unpinned the shown one is written per theme.
            let flat = crate::widgets::themes::UiTheme::Flat;
            let empty = TextInput::create_search().with_theme(flat).dom();
            assert_eq!(built_display(clear_of(&empty)), Some(LayoutDisplay::None));

            let filled = TextInput::create_search()
                .with_text("abc".into())
                .with_theme(flat)
                .dom();
            let shown = built_display(clear_of(&filled));
            assert!(
                shown.is_some() && shown != Some(LayoutDisplay::None),
                "a non-empty search field hides its clear button: {shown:?}"
            );
        }

        #[test]
        fn clicking_the_clear_button_empties_the_field_and_tells_the_hook() {
            let probe = recorder(Update::RefreshDom, TextInputValid::Yes);
            let (styled_dom, state) = rendered_search(
                TextInput::create_search()
                    .with_text("abc".into())
                    .with_on_text_input(
                        probe.clone(),
                        record_text_input as TextInputOnTextInputCallbackType,
                    ),
            );
            let (update, changes, _) = run(Env::new(styled_dom).hit(dom_node(CLEAR)), |info| {
                default_on_search_clear_click(state.clone(), info)
            });

            assert_eq!(update, Update::RefreshDom, "the hook's Update was swallowed");
            assert_eq!(state_of(&state).get_text(), "");
            let seen = recorded(&probe);
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].get_text(), "", "the hook was not shown the cleared value");

            assert!(
                pushed_texts(&changes)
                    .iter()
                    .any(|(node, text)| *node == dom_node(LINE_TEXT) && text.is_empty()),
                "the field's line was not emptied: {changes:?}"
            );
            assert_eq!(
                displays_pushed_to(&changes, CLEAR),
                vec![LayoutDisplay::None],
                "the clear button must hide once the field is empty"
            );
        }

        #[test]
        fn a_hook_rejecting_the_clear_keeps_the_text() {
            let probe = recorder(Update::DoNothing, TextInputValid::No);
            let (styled_dom, state) = rendered_search(
                TextInput::create_search()
                    .with_text("abc".into())
                    .with_on_text_input(
                        probe.clone(),
                        record_text_input as TextInputOnTextInputCallbackType,
                    ),
            );
            let (_, changes, _) = run(Env::new(styled_dom).hit(dom_node(CLEAR)), |info| {
                default_on_search_clear_click(state.clone(), info)
            });
            assert_eq!(state_of(&state).get_text(), "abc");
            assert!(pushed_texts(&changes).is_empty());
        }

        #[test]
        fn escape_in_a_non_empty_search_field_clears_it_and_keeps_focus() {
            let (styled_dom, state) =
                rendered_search(TextInput::create_search().with_text("abc".into()));
            let (_, changes, _) = run(
                Env::new(styled_dom)
                    .hit(dom_node(FIELD))
                    .key(VirtualKeyCode::Escape),
                |info| default_on_virtual_key_down(state.clone(), info),
            );
            assert_eq!(state_of(&state).get_text(), "");
            assert!(
                changes
                    .iter()
                    .any(|c| matches!(c, CallbackChange::PreventDefault)),
                "Escape's default (dropping focus) must not run when it cleared the field"
            );
        }

        #[test]
        fn escape_in_an_empty_search_field_keeps_its_default() {
            let (styled_dom, state) = rendered_search(TextInput::create_search());
            let (_, changes, _) = run(
                Env::new(styled_dom)
                    .hit(dom_node(FIELD))
                    .key(VirtualKeyCode::Escape),
                |info| default_on_virtual_key_down(state.clone(), info),
            );
            assert!(!changes
                .iter()
                .any(|c| matches!(c, CallbackChange::PreventDefault)));
        }

        #[test]
        fn escape_in_a_plain_text_field_does_not_clear_it() {
            let (styled_dom, state) = rendered(TextInput::create().with_text("abc".into()));
            let _ = run(Env::new(styled_dom).key(VirtualKeyCode::Escape), |info| {
                default_on_virtual_key_down(state.clone(), info)
            });
            assert_eq!(state_of(&state).get_text(), "abc");
        }

        #[test]
        fn typing_the_first_character_shows_the_clear_button_once() {
            let (styled_dom, state) = rendered_search(TextInput::create_search());
            let (_, changes, _) = run(
                Env::new(styled_dom).hit(dom_node(FIELD)).insert("a"),
                |info| default_on_text_input(state.clone(), info),
            );
            assert_eq!(state_of(&state).get_text(), "a");
            let shown = displays_pushed_to(&changes, CLEAR);
            assert_eq!(shown.len(), 1, "the clear button was not shown: {changes:?}");
            assert_ne!(shown[0], LayoutDisplay::None);

            // The second character changes nothing about the button: no
            // same-value `display` write (each one costs a relayout).
            let (styled_dom, state) =
                rendered_search(TextInput::create_search().with_text("a".into()));
            let (_, changes, _) = run(
                Env::new(styled_dom).hit(dom_node(FIELD)).insert("b"),
                |info| default_on_text_input(state.clone(), info),
            );
            assert!(displays_pushed_to(&changes, CLEAR).is_empty());
        }
    }

    // ==================================================================
    // type=email / tel / url and `pattern`: validity the app can read
    // ==================================================================

    mod validation {
        use azul_core::form::ValidityReason;

        use super::*;

        /// Every override the handler pushed onto the field host, one entry
        /// per push.
        fn ring_writes(changes: &[CallbackChange]) -> Vec<Vec<CssProperty>> {
            changes
                .iter()
                .filter_map(|c| match c {
                    CallbackChange::OverrideNodeCssProperties {
                        node_id,
                        properties,
                        ..
                    } if *node_id == NodeId::new(CONTAINER) => {
                        Some(properties.as_ref().to_vec())
                    }
                    _ => None,
                })
                .collect()
        }

        fn is_border_colour(p: &CssProperty) -> bool {
            matches!(
                p,
                CssProperty::BorderTopColor(_)
                    | CssProperty::BorderRightColor(_)
                    | CssProperty::BorderBottomColor(_)
                    | CssProperty::BorderLeftColor(_)
            )
        }

        #[test]
        fn an_email_field_with_a_malformed_value_reports_a_type_mismatch() {
            let dom = TextInput::create_email().with_text("not-an-email".into()).dom();
            let validity = dataset_state(&dom).validity;
            assert!(validity.has(ValidityReason::TypeMismatch));
            assert!(!validity.is_valid());

            let dom = TextInput::create_email()
                .with_text("someone@example.com".into())
                .dom();
            assert!(dataset_state(&dom).validity.is_valid());
        }

        #[test]
        fn an_empty_email_or_url_field_is_valid() {
            for input in [TextInput::create_email(), TextInput::create_url()] {
                assert!(dataset_state(&input.dom()).validity.is_valid());
            }
        }

        #[test]
        fn a_url_field_accepts_only_absolute_urls() {
            let bad = TextInput::create_url().with_text("example.com".into()).dom();
            assert!(dataset_state(&bad).validity.has(ValidityReason::TypeMismatch));
            let good = TextInput::create_url()
                .with_text("https://example.com/a?b#c".into())
                .dom();
            assert!(dataset_state(&good).validity.is_valid());
        }

        #[test]
        fn a_tel_field_accepts_any_text_and_declares_its_type() {
            let dom = TextInput::create_tel().with_text("call me maybe".into()).dom();
            assert!(dataset_state(&dom).validity.is_valid());
            assert!(dom.root.attributes().as_ref().iter().any(
                |a| matches!(a, AttributeType::InputType(t) if t.as_str() == "tel")
            ));
        }

        #[test]
        fn email_and_url_fields_declare_their_type_for_the_soft_keyboard() {
            for (input, ty) in [
                (TextInput::create_email(), "email"),
                (TextInput::create_url(), "url"),
            ] {
                let dom = input.dom();
                assert!(
                    dom.root.attributes().as_ref().iter().any(
                        |a| matches!(a, AttributeType::InputType(t) if t.as_str() == ty)
                    ),
                    "no type={ty} attribute"
                );
            }
        }

        #[test]
        fn a_pattern_must_match_the_whole_value() {
            let three_digits = |text: &str| {
                dataset_state(
                    &TextInput::create()
                        .with_pattern("[0-9]{3}".into())
                        .with_text(text.into())
                        .dom(),
                )
                .validity
            };
            assert!(three_digits("123").is_valid());
            assert!(three_digits("1234").has(ValidityReason::PatternMismatch));
            assert!(three_digits("12a").has(ValidityReason::PatternMismatch));
            // Empty is exempt: that is `required`'s job, not `pattern`'s.
            assert!(three_digits("").is_valid());
        }

        #[test]
        fn an_uncompilable_pattern_is_ignored() {
            let dom = TextInput::create()
                .with_pattern("([unclosed".into())
                .with_text("anything".into())
                .dom();
            assert!(dataset_state(&dom).validity.is_valid());
        }

        #[test]
        fn a_password_is_checked_against_its_pattern_not_its_bullets() {
            let dom = TextInput::create_password()
                .with_pattern("[a-z]+[0-9]".into())
                .with_text("hunter2".into())
                .dom();
            assert!(dataset_state(&dom).validity.is_valid());
        }

        #[test]
        fn typing_updates_the_validity_the_hook_and_the_state_see() {
            let probe = recorder(Update::DoNothing, TextInputValid::Yes);
            let (styled_dom, state) = rendered(
                TextInput::create_email()
                    .with_text("a@b".into())
                    .with_on_text_input(
                        probe.clone(),
                        record_text_input as TextInputOnTextInputCallbackType,
                    ),
            );
            let _ = run(Env::new(styled_dom).insert("@"), |info| {
                default_on_text_input(state.clone(), info)
            });
            let seen = recorded(&probe);
            assert_eq!(seen.len(), 1);
            assert!(seen[0].validity.has(ValidityReason::TypeMismatch));
            assert!(state_of(&state).validity.has(ValidityReason::TypeMismatch));
        }

        #[test]
        fn typing_an_email_field_into_an_invalid_value_paints_the_invalid_ring() {
            let (styled_dom, state) = rendered(TextInput::create_email().with_text("a@b".into()));
            let (_, changes, _) = run(Env::new(styled_dom).insert("@"), |info| {
                default_on_text_input(state.clone(), info)
            });
            let writes = ring_writes(&changes);
            assert_eq!(writes.len(), 1, "one ring write expected: {changes:?}");
            assert_eq!(writes[0].len(), 4, "all four edges take the ring");
            assert!(writes[0].iter().all(is_border_colour));
            assert!(
                writes[0].iter().all(|p| !p.is_initial()),
                "the ring must paint a colour, not remove one"
            );
        }

        #[test]
        fn fixing_an_invalid_value_removes_the_invalid_ring() {
            let (styled_dom, state) = rendered(TextInput::create_email().with_text("ab".into()));
            let (_, changes, _) = run(Env::new(styled_dom).insert("@c"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert!(state_of(&state).validity.is_valid());
            let writes = ring_writes(&changes);
            assert_eq!(writes.len(), 1, "one ring removal expected: {changes:?}");
            assert_eq!(writes[0].len(), 4);
            assert!(
                writes[0].iter().all(|p| is_border_colour(p) && p.is_initial()),
                "the ring must be REMOVED (initial), so hover/focus/dark come back"
            );
        }

        #[test]
        fn a_valid_edit_of_a_valid_field_writes_no_ring() {
            let (styled_dom, state) = rendered(TextInput::create_email().with_text("a@b".into()));
            let (_, changes, _) = run(Env::new(styled_dom).insert("c"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert!(ring_writes(&changes).is_empty(), "{changes:?}");

            let (styled_dom, state) = rendered(TextInput::create().with_text("x".into()));
            let (_, changes, _) = run(Env::new(styled_dom).insert("@@"), |info| {
                default_on_text_input(state.clone(), info)
            });
            assert!(ring_writes(&changes).is_empty(), "a plain field has no constraints");
        }

        #[test]
        fn the_two_themes_paint_their_own_invalid_ring_in_both_modes() {
            use crate::widgets::themes::{flat, flora};
            for dark in [false, true] {
                assert_eq!(flat::text_input_invalid_ring(dark).len(), 4);
                assert_eq!(flora::text_input_invalid_ring(dark).len(), 4);
            }
            assert_ne!(
                flat::text_input_invalid_ring(false),
                flat::text_input_invalid_ring(true),
                "flat: the dark ring must differ from the light one"
            );
            assert_ne!(
                flora::text_input_invalid_ring(false),
                flora::text_input_invalid_ring(true),
                "flora: the dark ring must differ from the light one"
            );
        }
    }
}

/// R5: a text field's STRUCTURE (display, flex, overflow, cursor, ...) is
/// its base - declared once, outside every `@theme(<name>)` block, so it
/// holds under flat, flora and any theme to come. What a theme owns is its
/// skin: paint and metrics.
#[cfg(test)]
mod structure_tests {
    use azul_css::AzString;

    use super::{TextInput, TextInputKind};
    use crate::widgets::themes::{
        theme_blocks::checks::{under, BOTH},
        theme_checks::assert_structure_is_shared,
    };

    #[test]
    fn a_text_input_declares_its_structure_once_for_every_theme() {
        let kinds = [
            TextInputKind::Text,
            TextInputKind::Password,
            TextInputKind::Search,
            TextInputKind::Email,
            TextInputKind::Tel,
            TextInputKind::Url,
        ];
        for t in BOTH {
            for kind in kinds {
                // Empty and filled: a search field shows its clear button
                // only while it holds text, an e-mail field is invalid with
                // "abc".
                for text in ["", "abc"] {
                    let dom = under(t, || {
                        TextInput::create_with_kind(kind)
                            .with_text(AzString::from(text))
                            .dom()
                    });
                    assert_structure_is_shared(
                        &format!("{kind:?} field holding {text:?}, built for {}", t.name()),
                        &dom,
                        &[],
                    );
                }
            }
        }
    }
}

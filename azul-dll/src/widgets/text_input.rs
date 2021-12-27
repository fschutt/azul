//! Text input (demonstrates two-way data binding)

use core::ops::Range;
use azul_desktop::{
    css::*,
    css::AzString,
    styled_dom::StyledDom,
    dom::{
        Dom, NodeDataInlineCssProperty, NodeDataInlineCssPropertyVec,
        NodeDataInlineCssProperty::{Normal, Hover, Focus}
    },
    task::OptionTimerId,
    callbacks::{RefAny, Callback, CallbackInfo, Update},
};
use azul_core::{
    callbacks::{Animation, AnimationRepeatCount, InlineText, DomNodeId},
    task::SystemTimeDiff,
    window::{KeyboardState, LogicalPosition, VirtualKeyCode},
};
use alloc::vec::Vec;
use alloc::string::String;
use azul_impl::text_layout::text_layout;

const BACKGROUND_COLOR: ColorU = ColorU { r: 255,  g: 255,  b: 255,  a: 255 }; // white
const BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };
const TEXT_COLOR: StyleTextColor = StyleTextColor { inner: BLACK  }; // black
const COLOR_9B9B9B: ColorU = ColorU { r: 155, g: 155, b: 155, a: 255 }; // #9b9b9b
const COLOR_4286F4: ColorU = ColorU { r: 66, g: 134, b: 244, a: 255 }; // #4286f4
const COLOR_4C4C4C: ColorU = ColorU { r: 76, g: 76, b: 76, a: 255 }; // #4C4C4C

const CURSOR_COLOR_BLACK: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(BLACK)];
const CURSOR_COLOR: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(CURSOR_COLOR_BLACK);

const BACKGROUND_THEME_LIGHT: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(BACKGROUND_COLOR)];
const BACKGROUND_COLOR_LIGHT: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_LIGHT);

const SANS_SERIF_STR: &str = "sans-serif";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec = StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

// -- cursor style

const TEXT_CURSOR_TRANSFORM: &[StyleTransform] = &[
    StyleTransform::Translate(StyleTransformTranslate2D {
        x: PixelValue::const_px(0),
        y: PixelValue::const_px(2),
    })
];

static TEXT_CURSOR_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_position(LayoutPosition::Absolute)),
    Normal(CssProperty::const_width(LayoutWidth::const_px(1))),
    Normal(CssProperty::const_height(LayoutHeight::const_px(11))),
    Normal(CssProperty::const_background_content(CURSOR_COLOR)),
    Normal(CssProperty::const_opacity(StyleOpacity::const_new(0))),
    Normal(CssProperty::const_transform(StyleTransformVec::from_const_slice(TEXT_CURSOR_TRANSFORM))),
];

// -- container style

#[cfg(target_os = "windows")]
static TEXT_INPUT_CONTAINER_PROPS: &[NodeDataInlineCssProperty] = &[

    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_cursor(StyleCursor::Text)),
    Normal(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Normal(CssProperty::const_background_content(BACKGROUND_COLOR_LIGHT)),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),

    Normal(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
    Normal(CssProperty::const_padding_right(LayoutPaddingRight::const_px(2))),
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(1))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(1))),

    // border: 1px solid #484c52;

    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),

    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Inset })),

    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_9B9B9B })),

    Normal(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    Normal(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),

    // Hover(border-color: #4c4c4c;)

    Hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_4C4C4C })),
    Hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_4C4C4C })),
    Hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_4C4C4C })),
    Hover(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_4C4C4C })),

    // Focus(border-color: #4286f4;)

    Focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_4286F4 })),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_CONTAINER_PROPS: &[NodeDataInlineCssProperty] = &[

    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_cursor(StyleCursor::Text)),
    Normal(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Normal(CssProperty::const_background_content(BACKGROUND_COLOR_LIGHT)),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),

    Normal(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
    Normal(CssProperty::const_padding_right(LayoutPaddingRight::const_px(2))),
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(1))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(1))),

    // border: 1px solid #484c52;

    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),

    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Inset })),

    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_9B9B9B })),

    Normal(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    Normal(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    Normal(CssProperty::const_text_align(StyleTextAlign::Left)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),

    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),

    // Hover(border-color: #4286f4;)

    Hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_4286F4 })),
    Hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_4286F4 })),
    Hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_4286F4 })),
    Hover(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_4286F4 })),

    // Focus(border-color: #4286f4;)

    Focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_4286F4 })),
];

#[cfg(target_os = "macos")]
static TEXT_INPUT_CONTAINER_PROPS: &[NodeDataInlineCssProperty] = &[

    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_cursor(StyleCursor::Text)),
    Normal(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Normal(CssProperty::const_background_content(BACKGROUND_COLOR_LIGHT)),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),

    Normal(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(2))),
    Normal(CssProperty::const_padding_right(LayoutPaddingRight::const_px(2))),
    Normal(CssProperty::const_padding_top(LayoutPaddingTop::const_px(1))),
    Normal(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(1))),

    // border: 1px solid #484c52;

    Normal(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    Normal(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Normal(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    Normal(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),

    Normal(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Inset })),
    Normal(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Inset })),

    Normal(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_9B9B9B })),
    Normal(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_9B9B9B })),

    Normal(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    Normal(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
    Normal(CssProperty::const_text_align(StyleTextAlign::Left)),
    Normal(CssProperty::const_justify_content(LayoutJustifyContent::Center)),

    // Hover(border-color: #4286f4;)

    Hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_4286F4 })),
    Hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_4286F4 })),
    Hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_4286F4 })),
    Hover(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_4286F4 })),

    // Focus(border-color: #4286f4;)

    Focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: COLOR_4286F4 })),
    Focus(CssProperty::const_border_right_color(StyleBorderRightColor { inner: COLOR_4286F4 })),
];

// -- label style

#[cfg(target_os = "windows")]
static TEXT_INPUT_LABEL_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_LABEL_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(target_os = "macos")]
static TEXT_INPUT_LABEL_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

// --- placeholder

#[cfg(target_os = "windows")]
static TEXT_INPUT_PLACEHOLDER_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Block)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Absolute)),
    Normal(CssProperty::const_top(LayoutTop::const_px(2))),
    Normal(CssProperty::const_left(LayoutLeft::const_px(2))),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    Normal(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_PLACEHOLDER_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Block)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Absolute)),
    Normal(CssProperty::const_top(LayoutTop::const_px(2))),
    Normal(CssProperty::const_left(LayoutLeft::const_px(2))),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    Normal(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

#[cfg(target_os = "macos")]
static TEXT_INPUT_PLACEHOLDER_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::Block)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Absolute)),
    Normal(CssProperty::const_top(LayoutTop::const_px(2))),
    Normal(CssProperty::const_left(LayoutLeft::const_px(2))),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(11))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    Normal(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextInput {
    pub state: TextInputStateWrapper,
    pub placeholder_style: NodeDataInlineCssPropertyVec,
    pub container_style: NodeDataInlineCssPropertyVec,
    pub label_style: NodeDataInlineCssPropertyVec,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextInputState {
    pub text: U32Vec, // Vec<char>
    pub placeholder: OptionAzString,
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
pub type TextInputOnTextInputCallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo, &TextInputState) -> OnTextInputReturn;
impl_callback!(TextInputOnTextInput, OptionTextInputOnTextInput, TextInputOnTextInputCallback, TextInputOnTextInputCallbackType);

pub type TextInputOnVirtualKeyDownCallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo, &TextInputState) -> OnTextInputReturn;
impl_callback!(TextInputOnVirtualKeyDown, OptionTextInputOnVirtualKeyDown, TextInputOnVirtualKeyDownCallback, TextInputOnVirtualKeyDownCallbackType);

pub type TextInputOnFocusLostCallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo, &TextInputState) -> Update;
impl_callback!(TextInputOnFocusLost, OptionTextInputOnFocusLost, TextInputOnFocusLostCallback, TextInputOnFocusLostCallbackType);


#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C, u8)]
pub enum TextInputSelection {
    All,
    FromTo(TextInputSelectionRange),
}

impl_option!(TextInputSelection, OptionTextInputSelection, copy = false, [Debug, Clone, Hash, PartialEq, Eq]);

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
#[repr(C)]
pub struct TextInputSelectionRange {
    pub from: usize,
    pub to: usize,
}

impl Default for TextInput {
    fn default() -> Self {
        TextInput {
            state: TextInputStateWrapper::default(),
            placeholder_style: NodeDataInlineCssPropertyVec::from_const_slice(TEXT_INPUT_PLACEHOLDER_PROPS),
            container_style: NodeDataInlineCssPropertyVec::from_const_slice(TEXT_INPUT_CONTAINER_PROPS),
            label_style: NodeDataInlineCssPropertyVec::from_const_slice(TEXT_INPUT_LABEL_PROPS),
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

    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(&mut self, text: AzString) -> Self {
        let mut s = self.swap_with_default();
        s.set_text(text);
        s
    }

    pub fn set_text(&mut self, text: AzString) {
        self.state.inner.text = text
            .as_str()
            .chars()
            .map(|c| c as u32)
            .collect::<Vec<_>>()
            .into();
    }

    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.state.inner.placeholder = Some(placeholder).into();
    }

    pub fn with_placeholder(&mut self, placeholder: AzString) -> Self {
        let mut s = self.swap_with_default();
        s.set_placeholder(placeholder);
        s
    }

    pub fn set_on_text_input(&mut self,  data: RefAny, callback: TextInputOnTextInputCallbackType) {
        self.state.on_text_input = Some(TextInputOnTextInput {
            callback: TextInputOnTextInputCallback { cb: callback },
            data
        }).into();
    }

    pub fn set_on_virtual_key_down(&mut self, data: RefAny, callback: TextInputOnVirtualKeyDownCallbackType) {
        self.state.on_virtual_key_down = Some(TextInputOnVirtualKeyDown {
            callback: TextInputOnVirtualKeyDownCallback { cb: callback },
            data
        }).into();
    }

    pub fn set_on_focus_lost(&mut self, data: RefAny, callback: TextInputOnFocusLostCallbackType) {
        self.state.on_focus_lost = Some(TextInputOnFocusLost {
            callback: TextInputOnFocusLostCallback { cb: callback },
            data
        }).into();
    }

    pub fn with_on_focus_lost(mut self, data: RefAny, callback: TextInputOnFocusLostCallbackType) -> Self {
        self.set_on_focus_lost(data, callback);
        self
    }

    pub fn set_placeholder_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.placeholder_style = style;
    }

    pub fn set_container_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.container_style = style;
    }

    pub fn set_label_style(&mut self, style: NodeDataInlineCssPropertyVec) {
        self.label_style = style;
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    pub fn dom(mut self) -> Dom {

        use azul_desktop::dom::{
            CallbackData, EventFilter,
            HoverEventFilter, FocusEventFilter,
            IdOrClass::Class, TabIndex,
        };

        self.state.inner.cursor_pos = self.state.inner.text.len();

        let label_text: String = self.state.inner.text.iter().filter_map(|s| {
            core::char::from_u32(*s)
        }).collect();

        let placeholder = self.state.inner.placeholder
            .as_ref()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();

        let state_ref = RefAny::new(self.state);

        Dom::div()
        .with_ids_and_classes(vec![Class("__azul-native-text-input-container".into())].into())
        .with_inline_css_props(self.container_style)
        .with_tab_index(TabIndex::Auto)
        .with_dataset(Some(state_ref.clone()).into())
        .with_callbacks(vec![
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                data: state_ref.clone(),
                callback: Callback { cb: default_on_focus_received }
            },
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::FocusLost),
                data: state_ref.clone(),
                callback: Callback { cb: default_on_focus_lost }
            },
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::TextInput),
                data: state_ref.clone(),
                callback: Callback { cb: default_on_text_input }
            },
            CallbackData {
                event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                data: state_ref.clone(),
                callback: Callback { cb: default_on_virtual_key_down }
            },
            CallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseOver),
                data: state_ref.clone(),
                callback: Callback { cb: default_on_mouse_hover }
            },
        ].into())
        .with_children(vec![
            Dom::text(placeholder)
            .with_ids_and_classes(vec![Class("__azul-native-text-input-placeholder".into())].into())
            .with_inline_css_props(self.placeholder_style),
            Dom::text(label_text)
            .with_ids_and_classes(vec![Class("__azul-native-text-input-label".into())].into())
            .with_inline_css_props(self.label_style)
            .with_children(vec![
                Dom::div()
                .with_ids_and_classes(vec![Class("__azul-native-text-input-cursor".into())].into())
                .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(TEXT_CURSOR_PROPS))
            ].into())
        ].into())
    }
}

extern "C"
fn default_on_focus_received(
    text_input: &mut RefAny,
    info: &mut CallbackInfo
) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let mut text_input = &mut *text_input;

    let placeholder_text_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // hide the placeholder text
    if text_input.inner.text.is_empty() {
        info.set_css_property(
            placeholder_text_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(0))
        );
    }

    text_input.inner.cursor_pos = text_input.inner.text.len();

    Update::DoNothing
}

extern "C"
fn default_on_focus_lost(
    text_input: &mut RefAny,
    info: &mut CallbackInfo
) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let mut text_input = &mut *text_input;

    let placeholder_text_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // show the placeholder text
    if text_input.inner.text.is_empty() {
        info.set_css_property(
            placeholder_text_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(100))
        );
    }

    Update::DoNothing
}

extern "C"
fn default_on_text_input(
    text_input: &mut RefAny,
    info: &mut CallbackInfo
) -> Update {
    default_on_text_input_inner(text_input, info)
    .unwrap_or(Update::DoNothing)
}

fn default_on_text_input_inner(
    text_input: &mut RefAny,
    info: &mut CallbackInfo
) -> Option<Update> {

    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;
    let keyboard_state = info.get_current_keyboard_state();

    let c = keyboard_state.current_char.into_option()?;
    let placeholder_node_id = info.get_first_child(info.get_hit_node())?;
    let label_node_id = info.get_next_sibling(placeholder_node_id)?;
    let cursor_node_id = info.get_first_child(label_node_id)?;


    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let ontextinput = &mut text_input.on_text_input;

        // inner_clone has the new text
        let mut inner_clone = text_input.inner.clone();
        inner_clone.cursor_pos = inner_clone.cursor_pos.saturating_add(1);
        inner_clone.text = {
            let mut internal = inner_clone.text.clone().into_library_owned_vec();
            internal.push(c);
            internal.into()
        };

        match ontextinput.as_mut() {
            Some(TextInputOnTextInput { callback, data }) => (callback.cb)(data, info, &inner_clone),
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
            CssProperty::const_opacity(StyleOpacity::const_new(0))
        );

        // append to the text
        text_input.inner.text = {
            let mut internal = text_input.inner.text.clone().into_library_owned_vec();
            internal.push(c);
            internal.into()
        };
        text_input.inner.cursor_pos = text_input.inner.cursor_pos.saturating_add(1);

        info.set_string_contents(label_node_id, text_input.inner.get_text().into());
    }

    Some(result.update)
}

extern "C"
fn default_on_virtual_key_down(
    text_input: &mut RefAny,
    info: &mut CallbackInfo
) -> Update {
    default_on_virtual_key_down_inner(text_input, info)
    .unwrap_or(Update::DoNothing)
}

fn default_on_virtual_key_down_inner(
    text_input: &mut RefAny,
    info: &mut CallbackInfo
) -> Option<Update> {

    let mut text_input = text_input.downcast_mut::<TextInputStateWrapper>()?;
    let keyboard_state = info.get_current_keyboard_state();

    let c = keyboard_state.current_virtual_keycode.into_option()?;
    let placeholder_node_id = info.get_first_child(info.get_hit_node())?;
    let label_node_id = info.get_next_sibling(placeholder_node_id)?;
    let cursor_node_id = info.get_first_child(label_node_id)?;

    if c != VirtualKeyCode::Back {
        return None;
    }

    text_input.inner.text = {
        let mut internal = text_input.inner.text.clone().into_library_owned_vec();
        internal.pop();
        internal.into()
    };
    text_input.inner.cursor_pos = text_input.inner.cursor_pos.saturating_sub(1);

    info.set_string_contents(label_node_id, text_input.inner.get_text().into());

    None
}

extern "C"
fn default_on_mouse_hover(
  text_input: &mut RefAny,
  info: &mut CallbackInfo
) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // println!("default_on_mouse_hover");

    Update::DoNothing
}
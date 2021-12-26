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

    Normal(CssProperty::const_cursor(StyleCursor::Text)),
    Normal(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
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
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
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
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(target_os = "linux")]
static TEXT_INPUT_LABEL_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

#[cfg(target_os = "macos")]
static TEXT_INPUT_LABEL_PROPS: &[NodeDataInlineCssProperty] = &[
    Normal(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    Normal(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Normal(CssProperty::const_position(LayoutPosition::Relative)),
    Normal(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    Normal(CssProperty::const_text_color(StyleTextColor { inner: COLOR_4C4C4C })),
    Normal(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
];

impl Default for TextInput {
    fn default() -> Self {
        TextInput {
            state: TextInputStateWrapper::default(),
            placeholder_style: Vec::new().into(), // TEXT_INPUT_PLACEHOLDER_PROPS
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

    pub fn new(s: AzString) -> Self {
        Self {
            state: TextInputStateWrapper {
                inner: TextInputState {
                    text: s.as_ref().chars().map(|c| c as u32).collect::<Vec<_>>().into(),
                    .. Default::default()
                },
                .. Default::default()
            },
            .. Default::default()
        }
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
        let mut s = Self::new(AzString::from_const_str(""));
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
                event: EventFilter::Hover(HoverEventFilter::LeftMouseDown),
                data: state_ref.clone(),
                callback: Callback { cb: default_on_container_click }
            },
        ].into())
        .with_children(vec![
            Dom::text(label_text)
            .with_callbacks(vec![
                CallbackData {
                    event: EventFilter::Hover(HoverEventFilter::LeftMouseDown),
                    data: state_ref.clone(),
                    callback: Callback { cb: default_on_label_click }
                },
            ].into())
            .with_ids_and_classes(vec![Class("__azul-native-text-input-label".into())].into())
            .with_inline_css_props(self.label_style)
            .with_children(vec![
                Dom::div()
                .with_ids_and_classes(vec![Class("__azul-native-text-input-cursor".into())].into())
                .with_inline_css_props(NodeDataInlineCssPropertyVec::from_const_slice(TEXT_CURSOR_PROPS))
            ].into()),
            // let text_selection = Dom::div().with_class("__azul-native-text-input-selection".into());
        ].into())
    }
}

impl TextInputState {

    pub fn get_text(&self) -> String {
        self.text
        .iter()
        .filter_map(|c| core::char::from_u32(*c))
        .collect()
    }

    #[must_use]
    fn handle_on_text_input(&mut self, c: char) -> usize {
        match self.selection.clone().into_option() {
            None => {
                println!("handle on text input: char = {}, cursor_pos = {}", c, self.cursor_pos);
                if self.cursor_pos >= self.text.len() {
                    self.text.push(c as u32);
                } else {
                    // TODO: insert character at the cursor location!
                    self.text.insert(self.cursor_pos, c as u32);
                }
                let cursor_pos = self.cursor_pos.saturating_add(1).min(self.text.len());
                cursor_pos
            },
            Some(TextInputSelection::All) => {
                self.text = vec![c as u32].into();
                self.selection = None.into();
                let cursor_pos = 1;
                return cursor_pos;
            },
            Some(TextInputSelection::FromTo(range)) => {
                return self.delete_selection(range.from..range.to, Some(c));
            },
        }
    }

    // returns if the text has changed
    #[must_use]
    fn handle_on_virtual_key_down(
        &mut self,
        virtual_key: VirtualKeyCode,
        keyboard_state: &KeyboardState,
        info: &mut CallbackInfo,
    ) -> (bool, usize) {
        let mut cursor_pos = self.cursor_pos;

        let update = match virtual_key {
            VirtualKeyCode::Back => {
                // TODO: shift + back = delete last word
                let selection = self.selection.clone();
                match selection.as_ref() {
                    None => {
                        if self.cursor_pos == (self.text.len() - 1) {
                            self.text.pop();
                        } else {
                            self.text.remove(self.cursor_pos);
                        }
                        cursor_pos = self.cursor_pos.saturating_sub(1);
                    },
                    Some(TextInputSelection::All) => {
                        self.text = Vec::new().into();
                        cursor_pos = 0;
                        self.selection = None.into();
                    },
                    Some(TextInputSelection::FromTo(range)) => {
                        self.delete_selection(range.from..range.to, None);
                    },
                }
                true
            },
            VirtualKeyCode::Home => {
                cursor_pos = 0;
                self.selection = None.into();
                true
            },
            VirtualKeyCode::End => {
                cursor_pos = self.text.len();
                self.selection = None.into();
                true
            },
            VirtualKeyCode::Tab => {
                use azul_desktop::callbacks::FocusTarget;
                if keyboard_state.shift_down() {
                    info.set_focus(FocusTarget::Previous);
                } else {
                    info.set_focus(FocusTarget::Next);
                }
                false // no update necessary
            },
            VirtualKeyCode::Escape => {
                self.selection = None.into();
                true
            },
            VirtualKeyCode::Right => {
                cursor_pos = self.cursor_pos.saturating_add(1).min(self.text.len());
                true
            },
            VirtualKeyCode::Left => {
                cursor_pos = self.cursor_pos.saturating_sub(1).min(self.text.len());
                true
            },
            vk => {
                if keyboard_state.ctrl_down() {
                    match vk {
                        VirtualKeyCode::A => {
                            self.selection = Some(TextInputSelection::All).into();
                            true
                        },
                        VirtualKeyCode::V => {
                            use azul_desktop::app::Clipboard;

                            if let Some(clip) = Clipboard::new() {
                                let clipboard_contents = clip.get_clipboard_string().unwrap_or_default();
                                let clipboard_contents: Vec<u32> = clipboard_contents.as_str().chars().map(|c| c as u32).collect();

                                // TODO: handle selection properly
                                self.selection = None.into();
                                cursor_pos = clipboard_contents.len();
                                self.text = clipboard_contents.into();
                            }

                            true
                        },
                        _ => false,
                        // C = copy
                        // X = cut
                    }
                } else {
                    false
                }
            },
            _ => false,
        };

        (update, cursor_pos)
    }

    #[must_use]
    fn delete_selection(&mut self, selection: Range<usize>, new_text: Option<char>) -> usize {
        let mut cursor_pos = self.cursor_pos;

        let Range { start, end } = selection;
        let max = end.min(self.text.len() - 1);

        if max == (self.text.len() - 1) {
            self.text.truncate(start);
            if let Some(new) = new_text {
                self.text.push(new as u32);
                cursor_pos = start + 1;
            } else {
                cursor_pos = start;
            }
        } else {
            let end = self.text.as_ref()[max..].to_vec();
            self.text.truncate(start);
            if let Some(new) = new_text {
                self.text.push(new as u32);
                cursor_pos = start + 1;
            } else {
                cursor_pos = start;
            }
            let mut end: U32Vec = end.into();
            self.text.append(&mut end);
        }

        return cursor_pos;
    }
}

impl TextInputSelection {
    pub fn get_range(&self, text_len: usize) -> Range<usize> {
        match self {
            TextInputSelection::All => 0..text_len,
            TextInputSelection::FromTo(r) => r.from..r.to,
        }
    }
}

// handle input events for the TextInput
extern "C" fn default_on_text_input(text_input: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let keyboard_state = info.get_current_keyboard_state();

    let c = match keyboard_state.current_char.into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let c = match core::char::from_u32(c) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    println!("entered: {}", c);

    let label_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let ontextinput = &mut text_input.on_text_input;

        // inner_clone has the new text
        let mut inner_clone = text_input.inner.clone();
        let new_cursor_pos = inner_clone.handle_on_text_input(c);

        match ontextinput.as_mut() {
            Some(TextInputOnTextInput { callback, data }) => (callback.cb)(data, info, &inner_clone),
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };


    if result.valid == TextInputValid::Yes {
        // Update the string, cursor position on the screen and selection background
        let new_cursor = text_input.inner.handle_on_text_input(c);
        // text_input.inner.set_cursor_pos(new_cursor, new_text.as_ref(), label_node_id, info);

        // TODO: restart the timer for cursor blinking
        // info.update_image(selection_node_id, render_selection(self.selection));
    }

    let new_text = text_input.inner.get_text();
    println!("setting text to: {}", new_text);
    info.set_string_contents(label_node_id, new_text.into());

    result.update
}

extern "C" fn default_on_virtual_key_down(text_input: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let keyboard_state = info.get_current_keyboard_state();
    let last_keycode = match keyboard_state.current_virtual_keycode.into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let kb_state = info.get_current_keyboard_state();

    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let onvkdown = &mut text_input.on_virtual_key_down;

        // inner_clone has the new text
        let mut inner_clone = text_input.inner.clone();
        let _ = inner_clone.handle_on_virtual_key_down(last_keycode, &kb_state, info);

        match onvkdown.as_mut() {
            Some(TextInputOnVirtualKeyDown { callback, data }) => (callback.cb)(data, info, &inner_clone),
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };

    if result.valid == TextInputValid::Yes {
        let (should_update, new_cursor) = text_input.inner.handle_on_virtual_key_down(last_keycode, &kb_state, info);
        if should_update {
            if let Some(label_node_id) = info.get_first_child(info.get_hit_node()) {
                // text_input.inner.set_cursor_pos(
                //     new_cursor,
                //     &new_text,
                //     label_node_id,
                //     info,
                // );
                // set_selection(text_input.cursor)
            };
        }
    }

    if let Some(label_node_id) = info.get_first_child(info.get_hit_node()) {
        let new_text = text_input.inner.get_text();
        info.set_string_contents(label_node_id, new_text.into());
    }

    result.update
}

extern "C" fn default_on_container_click(text_input: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let label_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let new_cursor_pos = text_input.inner.text.len();
    let new_text = text_input.inner.get_text();
    // text_input.inner.set_cursor_pos(
    //     new_cursor_pos,
    //     &new_text,
    //     label_node_id,
    //     info,
    // );

    // println!("container clicked at position {:?}", info.get_cursor_relative_to_node());

    // TODO: clear selection, set cursor to text hit
    Update::DoNothing
}

extern "C" fn default_on_label_click(text_input: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let text_layout = match info.get_inline_text(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let cursor_pos = match info.get_cursor_relative_to_node().into_option() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let hits = text_layout.hit_test(cursor_pos);

    let hit = if hits.is_empty() {
        return Update::DoNothing;
    } else {
        hits[0].clone()
    };

    println!("label clicked at position {:?}", info.get_cursor_relative_to_node());
    println!("hit: {:#?}", hit);

    let label_node_id = info.get_hit_node();
    let new_text = text_input.inner.get_text();
    text_input.inner.set_cursor_pos(
        hit.hit_relative_to_inline_text.x,
        hit.char_index_relative_to_text,
        &new_text,
        label_node_id,
        info
    );

    info.stop_propagation();

    // TODO: set cursor to end or start
    Update::DoNothing
}

extern "C" fn default_on_focus_received(text_input: &mut RefAny, info: &mut CallbackInfo) -> Update {

    use azul_core::task::Duration;
    use azul_core::callbacks::AnimationRepeat;

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let cursor_node_id = match info.get_first_child(info.get_hit_node()).and_then(|f| info.get_first_child(f)) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    if text_input.cursor_animation.is_none() {
        let timer_id = info.start_animation(cursor_node_id, Animation {
            from: CssProperty::const_opacity(StyleOpacity::const_new(100)),
            to: CssProperty::const_opacity(StyleOpacity::const_new(0)),
            duration: Duration::System(SystemTimeDiff::from_millis(500)),
            repeat: AnimationRepeat::PingPong,
            repeat_times: AnimationRepeatCount::Infinite,
            easing: AnimationInterpolationFunction::EaseInOut,
            relayout_on_finish: false,
        });
        if let Some(timer_id) = timer_id {
            text_input.cursor_animation = Some(timer_id).into();
        }
    }

    // TODO: start text cursor blinking
    Update::DoNothing
}

extern "C" fn default_on_focus_lost(text_input: &mut RefAny, info: &mut CallbackInfo) -> Update {

    let mut text_input = match text_input.downcast_mut::<TextInputStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_input = &mut *text_input;
        let onfocuslost = &mut text_input.on_focus_lost;
        let inner = &text_input.inner;

        if let Some(timer_id) = text_input.cursor_animation.clone().into_option() {
            info.stop_timer(timer_id);
            text_input.cursor_animation = None.into();
        }

        match onfocuslost.as_mut() {
            Some(TextInputOnFocusLost { callback, data }) => (callback.cb)(data, info, &inner),
            None => Update::DoNothing,
        }
    };

    let cursor_node_id = match info.get_first_child(info.get_hit_node()).and_then(|f| info.get_first_child(f)) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    info.set_css_property(cursor_node_id, CssProperty::const_opacity(StyleOpacity::const_new(0))); // hide cursor

    /*
    info.set_css_property(cursor_node_id, CssProperty::Transform(StyleTransformVecValue::from_vec(vec![
        StyleTransform::Translate(StyleTransformTranslate2D {
            x
        })
    ])));
    */

    result
}

// determine the cursor position from the x position that was clicked
impl TextInputState {

    // Updates the cursor position and scrolls the text to the correct position
    pub fn set_cursor_pos(
        &mut self,
        new_cursor_x: f32,
        new_cursor_index: usize,
        new_text: &str,
        label_node_id: DomNodeId,
        info: &mut CallbackInfo,
    ) {

        use azul_core::callbacks::InlineWord;

        let cursor_node_id = match info.get_first_child(label_node_id) {
            Some(s) => s,
            None => return,
        };

        /*
        let inline_text = info.get_inline_text(label_node_id) {

        };*/

        info.set_string_contents(label_node_id, new_text.to_owned().into());
        info.set_css_property(cursor_node_id, CssProperty::const_transform(vec![
            StyleTransform::Translate(StyleTransformTranslate2D {
                x: PixelValue::px(new_cursor_x),
                y: PixelValue::px(2.0),
            })
        ].into()));

        self.cursor_pos = new_cursor_index;
    }
}

impl From<TextInput> for Dom {
    fn from(t: TextInput) -> Dom {
        t.dom()
    }
}
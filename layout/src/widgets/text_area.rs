//! Multi-line text input (text area) widget.
//!
//! A multi-line sibling of [`crate::widgets::text_input::TextInput`]: it reuses
//! the same editable-state / cursor / focus-callback machinery but stores and
//! renders multiple lines. Pressing Return/Enter inserts a newline; Backspace
//! deletes the last character; typed/pasted text (including embedded newlines)
//! is appended. The buffer is a `Vec<char>` (as `U32Vec`) exactly like
//! `TextInput`, so the `'\n'` characters round-trip through
//! [`TextAreaState::get_text`].
//!
//! The widget reuses [`TextInput`]'s [`OnTextInputReturn`] / [`TextInputValid`]
//! return types for its `on_text_input` callback so existing host bindings and
//! validation logic apply unchanged.
//!
//! TODO2: this implements the *core* of multi-line editing — multi-line value,
//! newline insertion, append/backspace, `on_text_input` (a.k.a. on_change) and
//! `on_focus_lost`. Advanced editing is intentionally NOT implemented and is
//! not verifiable without a live window: the blinking cursor is a static child
//! and does not track the caret across lines, there is no selection/range
//! editing, no mid-buffer insertion (edits append/truncate at the end), and no
//! vertical (up/down) caret navigation. Line wrapping relies on the text
//! layout honouring `white-space: pre-wrap`.
//!
//! Key types: [`TextArea`], [`TextAreaState`], [`TextAreaOnTextInput`],
//! [`TextAreaOnFocusLost`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::Dom,
    refany::RefAny,
    window::VirtualKeyCode,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{basic::*, layout::*, property::{CssProperty, *}, style::*},
    *,
};

use crate::callbacks::{Callback, CallbackInfo};
use crate::widgets::text_input::{OnTextInputReturn, TextInputValid};

// ---- colours ----
const BACKGROUND_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
}; // white
const BLACK: ColorU = ColorU { r: 0, g: 0, b: 0, a: 255 };
const COLOR_9B9B9B: ColorU = ColorU {
    r: 155,
    g: 155,
    b: 155,
    a: 255,
}; // #9b9b9b border
const COLOR_4286F4: ColorU = ColorU {
    r: 66,
    g: 134,
    b: 244,
    a: 255,
}; // #4286f4 focus/hover
const COLOR_4C4C4C: ColorU = ColorU {
    r: 76,
    g: 76,
    b: 76,
    a: 255,
}; // #4C4C4C text

const CURSOR_COLOR_BLACK: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(BLACK)];
const CURSOR_COLOR: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(CURSOR_COLOR_BLACK);

const BACKGROUND_THEME_LIGHT: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(BACKGROUND_COLOR)];
const BACKGROUND_COLOR_LIGHT: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(BACKGROUND_THEME_LIGHT);

const SANS_SERIF_STR: &str = "system:ui";
const SANS_SERIF: AzString = AzString::from_const_str(SANS_SERIF_STR);
const SANS_SERIF_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SANS_SERIF)];
const SANS_SERIF_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SANS_SERIF_FAMILIES);

/// Minimum height of the editable area (~4 lines).
const MIN_HEIGHT_PX: isize = 64;

// -- cursor style (a static child; does not track the caret — see module TODO2) --
static TEXT_CURSOR_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(1))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(11))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(CURSOR_COLOR)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(0))),
];

// -- container style (cross-platform single style) --
static TEXT_AREA_CONTAINER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_min_height(LayoutMinHeight::const_px(
        MIN_HEIGHT_PX,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(BACKGROUND_COLOR_LIGHT)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        4,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
    // border: 1px inset #9b9b9b
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
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Scroll)),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    // Preserve newlines + wrap long lines.
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::PreWrap,
    ))),
    // Hover / focus border highlight.
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: COLOR_4286F4,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: COLOR_4286F4,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: COLOR_4286F4,
        },
    )),
];

// -- label style (the rendered multi-line text) --
static TEXT_AREA_LABEL_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_4C4C4C,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::PreWrap,
    ))),
];

// -- placeholder style --
static TEXT_AREA_PLACEHOLDER_PROPS: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: COLOR_9B9B9B,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SANS_SERIF_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

/// Multi-line text input widget.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextArea {
    pub text_area_state: TextAreaStateWrapper,
    pub placeholder_style: CssPropertyWithConditionsVec,
    pub container_style: CssPropertyWithConditionsVec,
    pub label_style: CssPropertyWithConditionsVec,
}

/// Editable state of a text area (text buffer + cursor position).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextAreaState {
    /// The text buffer as `Vec<char>` (newlines included).
    pub text: U32Vec,
    pub placeholder: OptionString,
    pub max_len: usize,
    pub cursor_pos: usize,
}

/// [`TextAreaState`] together with optional user callbacks.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TextAreaStateWrapper {
    pub inner: TextAreaState,
    pub on_text_input: OptionTextAreaOnTextInput,
    pub on_focus_lost: OptionTextAreaOnFocusLost,
    pub update_text_area_before_calling_focus_lost_fn: bool,
}

// -- callbacks --

/// Invoked on each text edit. Returns whether the edit is valid (reusing
/// [`TextInput`](crate::widgets::text_input::TextInput)'s [`OnTextInputReturn`]).
pub type TextAreaOnTextInputCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextAreaState) -> OnTextInputReturn;
impl_widget_callback!(
    TextAreaOnTextInput,
    OptionTextAreaOnTextInput,
    TextAreaOnTextInputCallback,
    TextAreaOnTextInputCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TextAreaOnTextInputCallback,
    info_ty:        CallbackInfo,
    return_ty:      OnTextInputReturn,
    default_ret:    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes },
    invoker_static: TEXT_AREA_ON_TEXT_INPUT_INVOKER,
    invoker_ty:     AzTextAreaOnTextInputCallbackInvoker,
    thunk_fn:       az_text_area_on_text_input_callback_thunk,
    setter_fn:      AzApp_setTextAreaOnTextInputCallbackInvoker,
    from_handle_fn: AzTextAreaOnTextInputCallback_createFromHostHandle,
    extra_args:     [ state: TextAreaState ],
}

/// Invoked when the text area loses focus.
pub type TextAreaOnFocusLostCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextAreaState) -> Update;
impl_widget_callback!(
    TextAreaOnFocusLost,
    OptionTextAreaOnFocusLost,
    TextAreaOnFocusLostCallback,
    TextAreaOnFocusLostCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TextAreaOnFocusLostCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TEXT_AREA_ON_FOCUS_LOST_INVOKER,
    invoker_ty:     AzTextAreaOnFocusLostCallbackInvoker,
    thunk_fn:       az_text_area_on_focus_lost_callback_thunk,
    setter_fn:      AzApp_setTextAreaOnFocusLostCallbackInvoker,
    from_handle_fn: AzTextAreaOnFocusLostCallback_createFromHostHandle,
    extra_args:     [ state: TextAreaState ],
}

impl Default for TextAreaState {
    fn default() -> Self {
        TextAreaState {
            text: Vec::new().into(),
            placeholder: None.into(),
            max_len: 1000,
            cursor_pos: 0,
        }
    }
}

impl TextAreaState {
    /// Reconstructs the (multi-line) string, including `'\n'` characters.
    pub fn get_text(&self) -> String {
        self.text
            .iter()
            .filter_map(|c| core::char::from_u32(*c))
            .collect()
    }
}

impl Default for TextAreaStateWrapper {
    fn default() -> Self {
        TextAreaStateWrapper {
            inner: TextAreaState::default(),
            on_text_input: None.into(),
            on_focus_lost: None.into(),
            update_text_area_before_calling_focus_lost_fn: true,
        }
    }
}

impl Default for TextArea {
    fn default() -> Self {
        TextArea {
            text_area_state: TextAreaStateWrapper::default(),
            placeholder_style: CssPropertyWithConditionsVec::from_const_slice(
                TEXT_AREA_PLACEHOLDER_PROPS,
            ),
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                TEXT_AREA_CONTAINER_PROPS,
            ),
            label_style: CssPropertyWithConditionsVec::from_const_slice(TEXT_AREA_LABEL_PROPS),
        }
    }
}

impl TextArea {
    pub fn create() -> Self {
        Self::default()
    }

    /// Sets the (multi-line) text. Newlines in `text` are preserved.
    pub fn set_text(&mut self, text: AzString) {
        self.text_area_state.inner.text = text
            .as_str()
            .chars()
            .map(|c| c as u32)
            .collect::<Vec<_>>()
            .into();
    }

    pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.text_area_state.inner.placeholder = Some(placeholder).into();
    }

    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    pub fn set_on_text_input<C: Into<TextAreaOnTextInputCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_area_state.on_text_input = Some(TextAreaOnTextInput {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    pub fn with_on_text_input<C: Into<TextAreaOnTextInputCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_text_input(refany, callback);
        self
    }

    pub fn set_on_focus_lost<C: Into<TextAreaOnFocusLostCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_area_state.on_focus_lost = Some(TextAreaOnFocusLost {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    pub fn with_on_focus_lost<C: Into<TextAreaOnFocusLostCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_focus_lost(refany, callback);
        self
    }

    pub fn set_container_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.container_style = style;
    }

    pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    pub fn dom(mut self) -> Dom {
        use azul_core::dom::{EventFilter, FocusEventFilter, HoverEventFilter, IdOrClass::Class, TabIndex};

        self.text_area_state.inner.cursor_pos = self.text_area_state.inner.text.len();

        let label_text: String = self
            .text_area_state
            .inner
            .text
            .iter()
            .filter_map(|s| core::char::from_u32(*s))
            .collect();

        let placeholder = self
            .text_area_state
            .inner
            .placeholder
            .as_ref()
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();

        let state_ref = RefAny::new(self.text_area_state);

        Dom::create_div()
            .with_ids_and_classes(vec![Class("__azul-native-text-area-container".into())].into())
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
                ]
                .into(),
            )
            .with_children(
                vec![
                    Dom::create_text(placeholder)
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-area-placeholder".into())].into(),
                        )
                        .with_css_props(self.placeholder_style),
                    Dom::create_text(label_text)
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-area-label".into())].into(),
                        )
                        .with_css_props(self.label_style)
                        .with_children(
                            vec![Dom::create_div()
                                .with_ids_and_classes(
                                    vec![Class("__azul-native-text-area-cursor".into())].into(),
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

extern "C" fn default_on_focus_received(mut text_area: RefAny, mut info: CallbackInfo) -> Update {
    let mut text_area = match text_area.downcast_mut::<TextAreaStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let text_area = &mut *text_area;

    let placeholder_text_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // hide the placeholder text
    if text_area.inner.text.is_empty() {
        info.set_css_property(
            placeholder_text_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(0)),
        );
    }

    text_area.inner.cursor_pos = text_area.inner.text.len();

    Update::DoNothing
}

extern "C" fn default_on_focus_lost(mut text_area: RefAny, mut info: CallbackInfo) -> Update {
    let mut text_area = match text_area.downcast_mut::<TextAreaStateWrapper>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let text_area = &mut *text_area;

    let placeholder_text_node_id = match info.get_first_child(info.get_hit_node()) {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    // show the placeholder text
    if text_area.inner.text.is_empty() {
        info.set_css_property(
            placeholder_text_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(100)),
        );
    }

    let text_area = &mut *text_area;
    let onfocuslost = &mut text_area.on_focus_lost;
    let inner = text_area.inner.clone();

    match onfocuslost.as_mut() {
        Some(TextAreaOnFocusLost { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
        None => Update::DoNothing,
    }
}

extern "C" fn default_on_text_input(text_area: RefAny, info: CallbackInfo) -> Update {
    default_on_text_input_inner(text_area, info).unwrap_or(Update::DoNothing)
}

fn default_on_text_input_inner(mut text_area: RefAny, mut info: CallbackInfo) -> Option<Update> {
    let mut text_area = text_area.downcast_mut::<TextAreaStateWrapper>()?;

    let changeset = info.get_text_changeset()?;
    let inserted_text = changeset.inserted_text.as_str().to_string();

    if inserted_text.is_empty() {
        return None;
    }

    let placeholder_node_id = info.get_first_child(info.get_hit_node())?;
    let label_node_id = info.get_next_sibling(placeholder_node_id)?;
    let _cursor_node_id = info.get_first_child(label_node_id)?;

    let result = {
        let text_area = &mut *text_area;
        let ontextinput = &mut text_area.on_text_input;

        // inner_clone has the new (would-be) text
        let mut inner_clone = text_area.inner.clone();
        inner_clone.cursor_pos = inner_clone.cursor_pos.saturating_add(inserted_text.len());
        inner_clone.text = {
            let mut internal = inner_clone.text.clone().into_library_owned_vec();
            internal.extend(inserted_text.chars().map(|c| c as u32));
            internal.into()
        };

        match ontextinput.as_mut() {
            Some(TextAreaOnTextInput { callback, refany }) => {
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
        info.set_css_property(
            placeholder_node_id,
            CssProperty::const_opacity(StyleOpacity::const_new(0)),
        );

        // append to the text
        text_area.inner.text = {
            let mut internal = text_area.inner.text.clone().into_library_owned_vec();
            internal.extend(inserted_text.chars().map(|c| c as u32));
            internal.into()
        };
        text_area.inner.cursor_pos = text_area
            .inner
            .cursor_pos
            .saturating_add(inserted_text.len());

        info.change_node_text(label_node_id, text_area.inner.get_text().into());
    }

    Some(result.update)
}

extern "C" fn default_on_virtual_key_down(text_area: RefAny, info: CallbackInfo) -> Update {
    default_on_virtual_key_down_inner(text_area, info).unwrap_or(Update::DoNothing)
}

fn default_on_virtual_key_down_inner(
    mut text_area: RefAny,
    mut info: CallbackInfo,
) -> Option<Update> {
    let mut text_area = text_area.downcast_mut::<TextAreaStateWrapper>()?;
    let keyboard_state = info.get_current_keyboard_state();

    let c = keyboard_state.current_virtual_keycode.into_option()?;
    let placeholder_node_id = info.get_first_child(info.get_hit_node())?;
    let label_node_id = info.get_next_sibling(placeholder_node_id)?;
    let _cursor_node_id = info.get_first_child(label_node_id)?;

    match c {
        VirtualKeyCode::Back => {
            text_area.inner.text = {
                let mut internal = text_area.inner.text.clone().into_library_owned_vec();
                internal.pop();
                internal.into()
            };
            text_area.inner.cursor_pos = text_area.inner.cursor_pos.saturating_sub(1);
            info.change_node_text(label_node_id, text_area.inner.get_text().into());

            // re-show placeholder if the buffer is now empty
            if text_area.inner.text.is_empty() {
                info.set_css_property(
                    placeholder_node_id,
                    CssProperty::const_opacity(StyleOpacity::const_new(100)),
                );
            }
        }
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => {
            // insert a newline
            text_area.inner.text = {
                let mut internal = text_area.inner.text.clone().into_library_owned_vec();
                internal.push('\n' as u32);
                internal.into()
            };
            text_area.inner.cursor_pos = text_area.inner.cursor_pos.saturating_add(1);
            info.change_node_text(label_node_id, text_area.inner.get_text().into());
            // hide placeholder (buffer is non-empty now)
            info.set_css_property(
                placeholder_node_id,
                CssProperty::const_opacity(StyleOpacity::const_new(0)),
            );
        }
        _ => return None,
    }

    None
}

impl From<TextArea> for Dom {
    fn from(t: TextArea) -> Dom {
        t.dom()
    }
}

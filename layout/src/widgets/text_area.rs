//! Multi-line text input (text area) widget.
//!
//! A multi-line sibling of [`crate::widgets::text_input::TextInput`], built on
//! the same flow: the container is a `contenteditable` host carrying the tab
//! index and the focus callbacks, so the engine's `TextEditManager` owns the
//! caret, the selection and the buffer, and edits run through
//! `record_text_input` / `apply_text_changeset`. Caret and selection are
//! display-list items driven by that manager; the widget emits no cursor node.
//!
//! Value and placeholder are `<p>` blocks wrapping a bare text node — a
//! [`NodeType::Text`](azul_core::dom::NodeType::Text) node is always
//! inline-level and owns no rect, so box-model properties on one are inert.
//! Line wrapping relies on the text layout honouring `white-space: pre-wrap`.
//!
//! [`TextAreaState`] is a *mirror* of the engine's state, refreshed from its
//! changesets so the public callbacks keep the shape existing hosts bind
//! against. The widget reuses [`TextInput`]'s [`OnTextInputReturn`] /
//! [`TextInputValid`] return types for its `on_text_input` callback so existing
//! host bindings and validation logic apply unchanged; a `TextInputValid::No`
//! answer turns into `CallbackInfo::prevent_default`, which stops the engine
//! from applying the edit it recorded.
//!
//! KNOWN GAP: caret-relative *deletion* (Backspace/Delete) and Enter are engine
//! default actions and the mirror cannot see their result, because the engine
//! exposes no post-apply text for a node whose value sits under a block
//! wrapper. Enter in a contenteditable host records a structural block split
//! rather than inserting a `'\n'` into the buffer, so a multi-paragraph value
//! no longer round-trips through [`TextAreaState::get_text`] as newlines.
//!
//! Key types: [`TextArea`], [`TextAreaState`], [`TextAreaOnTextInput`],
//! [`TextAreaOnVirtualKeyDown`], [`TextAreaOnFocusLost`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomNodeId},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{basic::{ColorU, StyleFontFamily, StyleFontFamilyVec, StyleFontSize}, layout::{LayoutPosition, LayoutBoxSizing, LayoutFlexGrow, LayoutMinHeight, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPaddingBottom, LayoutOverflow, LayoutDisplay, LayoutTop, LayoutLeft}, property::{CssProperty, StyleWhiteSpaceValue}, style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleOpacity, StyleCursor, StyleTextColor, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleTextAlign, StyleWhiteSpace}},
    impl_option_inner, AzString, U32Vec, OptionString,
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
    // `auto`, not `scroll`: the web-model textarea shows a scrollbar only
    // once the content overflows — an EMPTY field with a permanent track +
    // full-length thumb was the reported artifact.
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Auto)),
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

// -- label style (the `<p>` block wrapping the multi-line value) --
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
//
// An absolutely-positioned `<p>` overlay inside the editable container. It is
// marked `contenteditable="false"` so the engine's inheritance walk stops at it
// and the prompt never becomes part of the buffer, and it is toggled with
// `display` as well as `opacity`: a hidden-but-laid-out overlay would still own
// the container's first inline layout, which is what
// `LayoutWindow::reshape_text_node` picks up when it looks for the IFC to write
// an edit into.
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
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TextArea {
    pub text_area_state: TextAreaStateWrapper,
    pub placeholder_style: CssPropertyWithConditionsVec,
    pub container_style: CssPropertyWithConditionsVec,
    pub label_style: CssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET so it knows at build time whether it was named;
    /// forwarded into the accessibility declaration it already builds.
    pub accessibility_name: OptionString,
}

/// Editable state of a text area (text buffer + cursor position).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TextAreaState {
    /// The text buffer as `Vec<char>` (newlines included).
    pub text: U32Vec,
    pub placeholder: OptionString,
    pub max_len: usize,
    pub cursor_pos: usize,
}

/// [`TextAreaState`] together with optional user callbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct TextAreaStateWrapper {
    pub inner: TextAreaState,
    pub on_text_input: OptionTextAreaOnTextInput,
    pub on_focus_lost: OptionTextAreaOnFocusLost,
    pub update_text_area_before_calling_focus_lost_fn: bool,
    // appended at the END of the repr(C) struct for ABI stability
    pub on_virtual_key_down: OptionTextAreaOnVirtualKeyDown,
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

/// Invoked on every virtual-key press while the text area is focused (reusing
/// [`TextInput`](crate::widgets::text_input::TextInput)'s [`OnTextInputReturn`]).
pub type TextAreaOnVirtualKeyDownCallbackType =
    extern "C" fn(RefAny, CallbackInfo, TextAreaState) -> OnTextInputReturn;
impl_widget_callback!(
    TextAreaOnVirtualKeyDown,
    OptionTextAreaOnVirtualKeyDown,
    TextAreaOnVirtualKeyDownCallback,
    TextAreaOnVirtualKeyDownCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TextAreaOnVirtualKeyDownCallback,
    info_ty:        CallbackInfo,
    return_ty:      OnTextInputReturn,
    default_ret:    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes },
    invoker_static: TEXT_AREA_ON_VIRTUAL_KEY_DOWN_INVOKER,
    invoker_ty:     AzTextAreaOnVirtualKeyDownCallbackInvoker,
    thunk_fn:       az_text_area_on_virtual_key_down_callback_thunk,
    setter_fn:      AzApp_setTextAreaOnVirtualKeyDownCallbackInvoker,
    from_handle_fn: AzTextAreaOnVirtualKeyDownCallback_createFromHostHandle,
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
        Self {
            text: Vec::new().into(),
            placeholder: None.into(),
            max_len: 1000,
            cursor_pos: 0,
        }
    }
}

impl TextAreaState {
    /// Reconstructs the (multi-line) string, including `'\n'` characters.
    #[must_use] pub fn get_text(&self) -> String {
        self.text
            .iter()
            .filter_map(|c| core::char::from_u32(*c))
            .collect()
    }
}

impl Default for TextAreaStateWrapper {
    fn default() -> Self {
        Self {
            inner: TextAreaState::default(),
            on_text_input: None.into(),
            on_focus_lost: None.into(),
            update_text_area_before_calling_focus_lost_fn: true,
            on_virtual_key_down: None.into(),
        }
    }
}

impl Default for TextArea {
    fn default() -> Self {
        Self {
            text_area_state: TextAreaStateWrapper::default(),
            placeholder_style: CssPropertyWithConditionsVec::from_const_slice(
                TEXT_AREA_PLACEHOLDER_PROPS,
            ),
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                TEXT_AREA_CONTAINER_PROPS,
            ),
            label_style: CssPropertyWithConditionsVec::from_const_slice(TEXT_AREA_LABEL_PROPS),
            accessibility_name: OptionString::None,
        }
    }
}

impl TextArea {
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    #[must_use] pub fn create() -> Self {
        Self::default()
    }

    /// Sets the (multi-line) text. Newlines in `text` are preserved.
    #[allow(clippy::needless_pass_by_value)] // public by-value setter; builder with_text moves the arg in
    pub fn set_text(&mut self, text: AzString) {
        self.text_area_state.inner.text = text
            .as_str()
            .chars()
            .map(|c| c as u32)
            .collect::<Vec<_>>()
            .into();
    }

    #[must_use] pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.text_area_state.inner.placeholder = Some(placeholder).into();
    }

    #[must_use] pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
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

    #[must_use] pub fn with_on_text_input<C: Into<TextAreaOnTextInputCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_text_input(refany, callback);
        self
    }

    pub fn set_on_virtual_key_down<C: Into<TextAreaOnVirtualKeyDownCallback>>(
        &mut self,
        refany: RefAny,
        callback: C,
    ) {
        self.text_area_state.on_virtual_key_down = Some(TextAreaOnVirtualKeyDown {
            callback: callback.into(),
            refany,
        })
        .into();
    }

    #[must_use] pub fn with_on_virtual_key_down<C: Into<TextAreaOnVirtualKeyDownCallback>>(
        mut self,
        refany: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_virtual_key_down(refany, callback);
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

    #[must_use] pub fn with_on_focus_lost<C: Into<TextAreaOnFocusLostCallback>>(
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

    #[must_use] pub fn with_container_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_container_style(style);
        self
    }

    #[must_use] pub fn swap_with_default(&mut self) -> Self {
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
    /// in particular no caret node.
    #[must_use] pub fn dom(mut self) -> Dom {
        // Read before the state is moved into the DOM below.
        let ta_name: Option<AzString> =
            self.text_area_state.inner.placeholder.as_ref().cloned();

        use azul_core::dom::{AttributeType, DomVec, EventFilter, FocusEventFilter, IdOrClass::Class, TabIndex};

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

        let mut placeholder_style = self.placeholder_style;
        if !self.text_area_state.inner.text.is_empty() {
            placeholder_style = hidden_placeholder_style(&placeholder_style);
        }

        let state_ref = RefAny::new(self.text_area_state);

        Dom::create_div()
            .with_ids_and_classes(vec![Class("__azul-native-text-area-container".into())].into())
            .with_css_props(self.container_style)
            .with_tab_index(TabIndex::Auto)
            // Same as text_input: an edit field with no name announces as
            // "edit" and the user cannot tell what it is for.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Text,
                accessibility_name: ta_name.into(),
                ..Default::default()
            })
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
                        refany: state_ref,
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
                    crate::widgets::widget_p()
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-area-placeholder".into())].into(),
                        )
                        .with_css_props(placeholder_style)
                        // appended, never `with_attributes`: that one replaces the
                        // whole vector, classes included
                        .with_attribute(AttributeType::ContentEditable(false))
                        .with_children(DomVec::from_vec(vec![Dom::create_text_do_not_use_without_block_level_wrapper(placeholder)])),
                    crate::widgets::widget_p()
                        .with_ids_and_classes(
                            vec![Class("__azul-native-text-area-label".into())].into(),
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
fn adopt_engine_text(state: &mut TextAreaState, info: &CallbackInfo, node: DomNodeId) {
    let Some(text) = info.get_node_text_content(node) else {
        return;
    };
    if text.is_empty() && !state.text.is_empty() {
        return;
    }
    state.text = text.chars().map(|c| c as u32).collect::<Vec<_>>().into();
}

/// Mirrors the insertion the engine is about to apply.
///
/// The engine inserts at the caret, so the mirror does too whenever the caret
/// is readable and lands on a character boundary; otherwise it appends, which
/// is where the caret sits for every append-only path. `cursor_pos` stays a
/// byte offset, as it has always been.
fn mirror_insertion(state: &mut TextAreaState, inserted: &str, caret: Option<usize>) {
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

extern "C" fn default_on_focus_received(mut text_area: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut text_area) = text_area.downcast_mut::<TextAreaStateWrapper>() else {
        return Update::DoNothing;
    };

    let text_area = &mut *text_area;

    let Some(placeholder_text_node_id) = info.get_first_child(info.get_hit_node()) else {
        return Update::DoNothing;
    };

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_area.inner, &info, container);

    // hide the placeholder text
    if text_area.inner.text.is_empty() {
        set_placeholder_visible(&mut info, placeholder_text_node_id, false);
    }

    // The engine seeds the caret at the end of the value when focus lands on a
    // contenteditable host; the mirror follows it.
    let end_of_text = text_area.inner.text.len();
    text_area.inner.cursor_pos = engine_caret(&info, container).unwrap_or(end_of_text);

    Update::DoNothing
}

extern "C" fn default_on_focus_lost(mut text_area: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut text_area) = text_area.downcast_mut::<TextAreaStateWrapper>() else {
        return Update::DoNothing;
    };

    let text_area = &mut *text_area;

    let Some(placeholder_text_node_id) = info.get_first_child(info.get_hit_node()) else {
        return Update::DoNothing;
    };

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_area.inner, &info, container);

    // show the placeholder text
    if text_area.inner.text.is_empty() {
        set_placeholder_visible(&mut info, placeholder_text_node_id, true);
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

    // The engine records the edit before the callbacks run and applies it after
    // them; this handler only observes it and mirrors it into the widget state.
    // An `Input` WITHOUT a pending record is a post-edit NOTIFICATION: an edit
    // committed outside the record pipeline (deletion, the Enter line break,
    // programmatic edit) that is already applied — adopt it and inform the
    // user hook; `valid` cannot veto what already happened.
    let inserted_text = info
        .get_text_changeset()
        .map(|c| c.inserted_text.as_str().to_string())
        .unwrap_or_default();

    let (placeholder_node_id, _label_node_id) = label_nodes(&info)?;
    let container = info.get_hit_node();

    if inserted_text.is_empty() {
        // Idempotent: a notification that changed nothing observable stays a
        // strict no-op, so the no-changeset pins keep holding.
        let before = text_area.inner.get_text();
        adopt_engine_text(&mut text_area.inner, &info, container);
        if text_area.inner.get_text() == before {
            return None;
        }
        let empty = text_area.inner.get_text().is_empty();
        set_placeholder_visible(&mut info, placeholder_node_id, empty);
        let result = {
            let text_area = &mut *text_area;
            let inner_clone = text_area.inner.clone();
            match text_area.on_text_input.as_mut() {
                Some(TextAreaOnTextInput { callback, refany }) => {
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

    let caret = engine_caret(&info, container);
    adopt_engine_text(&mut text_area.inner, &info, container);

    let result = {
        let text_area = &mut *text_area;
        let ontextinput = &mut text_area.on_text_input;

        // inner_clone has the new (would-be) text
        let mut inner_clone = text_area.inner.clone();
        mirror_insertion(&mut inner_clone, &inserted_text, caret);

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
        set_placeholder_visible(&mut info, placeholder_node_id, false);

        mirror_insertion(&mut text_area.inner, &inserted_text, caret);
    } else {
        // The engine applies the recorded changeset once the callbacks return,
        // unless one of them vetoes it.
        info.prevent_default();
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

    let _keycode = keyboard_state.current_virtual_keycode.into_option()?;
    let (_placeholder_node_id, _label_node_id) = label_nodes(&info)?;

    let container = info.get_hit_node();
    adopt_engine_text(&mut text_area.inner, &info, container);

    // Editing keys (Backspace, Delete, the arrows, Enter) are the engine's
    // default actions; this handler only forwards the key to the user's hook
    // and lets a rejection stop the default from running.
    let result = {
        // rustc doesn't understand the borrowing lifetime here
        let text_area = &mut *text_area;
        let inner_clone = text_area.inner.clone();
        match text_area.on_virtual_key_down.as_mut() {
            Some(TextAreaOnVirtualKeyDown { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner_clone)
            }
            None => OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            },
        }
    };

    if result.valid == TextInputValid::No {
        info.prevent_default();
    }

    Some(result.update)
}

impl From<TextArea> for Dom {
    fn from(t: TextArea) -> Self {
        t.dom()
    }
}

#[cfg(test)]
// `redundant_closure`: NOT redundant here. `run()` takes
// `impl FnOnce(RefAny, CallbackInfo) -> R`; `CallbackInfo` carries an elided
// lifetime, so the bound is higher-ranked (`for<'a> FnOnce(_, CallbackInfo<'a>)`).
// The handlers are `extern "C" fn` items, which do NOT satisfy a higher-ranked
// `FnOnce` bound — passing one bare fails to compile with E0277. The `|r, ci| f(r, ci)`
// wrapper is what makes the coercion happen and must stay.
#[allow(clippy::redundant_closure)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{
            AttributeType, DomId, DomNodeId, EventFilter, FocusEventFilter, NodeId, NodeType,
            TabIndex,
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

    /// Strings the buffer must round-trip verbatim through `set_text` ->
    /// `get_text`. Every entry is a valid Rust `str`, so none of them can be
    /// lost to the `char::from_u32` filter in `get_text` — anything that does
    /// not come back is real damage, not an encoding limit.
    const ROUND_TRIP: [&str; 22] = [
        "",
        " ",
        "a",
        "hello",
        "\n",
        "\n\n\n",
        "a\nb",
        "a\r\nb",           // CRLF: *both* units have to survive
        "trailing\n",
        "\nleading",
        "\t\ttabbed",
        "\0",               // NUL is a perfectly good `char`
        "line1\nline2\nline3",
        "ünïcödé",
        "e\u{301}",         // combining acute: 2 chars, 1 grapheme
        "😀",               // astral plane: 1 char, 4 bytes
        "👩‍👩‍👧‍👦",     // ZWJ family: 7 chars, 25 bytes
        "🇩🇪",              // regional-indicator pair
        "مرحبا",            // RTL
        "日本語",
        "a\u{200b}b",       // zero-width space wedged between two letters
        "\u{10FFFF}",       // the largest scalar value there is
    ];

    /// `u32` values that are *not* Unicode scalar values. The buffer is a
    /// `U32Vec`, not a `String`, so it can hold them — `get_text` has to drop
    /// them rather than panic.
    const NON_SCALAR: [u32; 6] = [
        0xD800,      // lone high surrogate
        0xDBFF,
        0xDC00,      // lone low surrogate
        0xDFFF,
        0x0011_0000, // one past the last scalar value
        u32::MAX,
    ];

    // ==================================================================
    // Fixtures
    // ==================================================================

    /// A state buffer built from a `&str` exactly the way `set_text` builds it.
    fn buffer(text: &str) -> U32Vec {
        text.chars().map(|c| c as u32).collect::<Vec<_>>().into()
    }

    /// A `TextAreaStateWrapper` with no user hooks, holding `text`.
    fn wrapper(text: &str) -> TextAreaStateWrapper {
        TextAreaStateWrapper {
            inner: TextAreaState {
                text: buffer(text),
                ..TextAreaState::default()
            },
            ..TextAreaStateWrapper::default()
        }
    }

    /// The state currently stored behind a `TextAreaStateWrapper` payload.
    fn read(state: &RefAny) -> TextAreaState {
        let mut handle = state.clone();
        let w = handle
            .downcast_ref::<TextAreaStateWrapper>()
            .expect("the payload must still be a TextAreaStateWrapper");
        w.inner.clone()
    }

    /// Mutates the shared state behind a payload (the borrow is released before
    /// this returns, so a handler may be invoked right afterwards).
    fn poke(state: &RefAny, f: impl FnOnce(&mut TextAreaStateWrapper)) {
        let mut handle = state.clone();
        let mut w = handle
            .downcast_mut::<TextAreaStateWrapper>()
            .expect("the payload must still be a TextAreaStateWrapper");
        f(&mut w);
    }

    /// `n` properties lifted off the default container style — an easy way to
    /// mint pairwise-distinct style vectors without hard-coding CSS.
    fn style(n: usize) -> CssPropertyWithConditionsVec {
        let all: Vec<CssPropertyWithConditions> =
            TextArea::default().container_style.as_ref().to_vec();
        assert!(n <= all.len(), "not enough default properties to slice");
        CssPropertyWithConditionsVec::from_vec(all.into_iter().take(n).collect())
    }

    /// The text a node carries, looking through the `<p>` block wrapper the
    /// widget convention mandates (`p > text`).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match node.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    // ---- recording hooks -------------------------------------------------
    //
    // NOTE: each hook below has a deliberately *different* body. Identical
    // function bodies can be folded onto a single symbol by the linker, and
    // these callbacks are compared by function-pointer identity.

    /// Records every `TextAreaState` an `on_text_input` / `on_virtual_key_down`
    /// hook is handed, and answers with a fixed verdict.
    struct EditLog {
        seen: Vec<TextAreaState>,
        ret: OnTextInputReturn,
    }

    /// Records every `TextAreaState` an `on_focus_lost` hook is handed.
    struct FocusLog {
        seen: Vec<TextAreaState>,
        ret: Update,
    }

    extern "C" fn record_text_input(
        mut data: RefAny,
        _: CallbackInfo,
        state: TextAreaState,
    ) -> OnTextInputReturn {
        let Some(mut log) = data.downcast_mut::<EditLog>() else {
            return OnTextInputReturn {
                update: Update::DoNothing,
                valid: TextInputValid::Yes,
            };
        };
        log.seen.push(state);
        log.ret
    }

    extern "C" fn record_virtual_key(
        mut data: RefAny,
        _: CallbackInfo,
        state: TextAreaState,
    ) -> OnTextInputReturn {
        match data.downcast_mut::<EditLog>() {
            Some(mut log) => {
                log.seen.push(state.clone());
                log.ret
            }
            None => OnTextInputReturn {
                update: Update::RefreshDom,
                valid: TextInputValid::Yes,
            },
        }
    }

    extern "C" fn record_focus_lost(
        mut data: RefAny,
        _: CallbackInfo,
        state: TextAreaState,
    ) -> Update {
        let mut update = Update::DoNothing;
        if let Some(mut log) = data.downcast_mut::<FocusLog>() {
            log.seen.push(state);
            update = log.ret;
        }
        update
    }

    fn edit_log(ret: OnTextInputReturn) -> RefAny {
        RefAny::new(EditLog {
            seen: Vec::new(),
            ret,
        })
    }

    fn focus_log(ret: Update) -> RefAny {
        RefAny::new(FocusLog {
            seen: Vec::new(),
            ret,
        })
    }

    fn edits_seen(log: &RefAny) -> Vec<TextAreaState> {
        let mut handle = log.clone();
        let l = handle
            .downcast_ref::<EditLog>()
            .expect("the payload must still be an EditLog");
        l.seen.clone()
    }

    fn focus_seen(log: &RefAny) -> Vec<TextAreaState> {
        let mut handle = log.clone();
        let l = handle
            .downcast_ref::<FocusLog>()
            .expect("the payload must still be a FocusLog");
        l.seen.clone()
    }

    const ACCEPT: OnTextInputReturn = OnTextInputReturn {
        update: Update::RefreshDom,
        valid: TextInputValid::Yes,
    };
    const REJECT: OnTextInputReturn = OnTextInputReturn {
        update: Update::RefreshDomAllWindows,
        valid: TextInputValid::No,
    };

    // ==================================================================
    // CallbackInfo harness
    // ==================================================================

    /// Flattened node indices of a `TextArea::dom()`.
    #[derive(Copy, Clone, Debug)]
    struct Nodes {
        container: usize,
        placeholder: usize,
        label: usize,
        label_text: usize,
    }

    /// Which node the event hit.
    #[derive(Copy, Clone, Debug)]
    enum Hit {
        /// `NodeHierarchyItemId::NONE` — no node was hit at all.
        Nothing,
        Container,
        Placeholder,
        /// The value's bare text leaf: it has no children, so every handler
        /// must bail out.
        TextLeaf,
    }

    /// Flattened indices of every node carrying `class`, in tree order.
    fn nodes_with_class(styled: &StyledDom, class: &str) -> Vec<usize> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| nd.has_class(class))
            .map(|(i, _)| i)
            .collect()
    }

    /// A styled, but never laid out, `TextArea::dom()` — the handlers only walk
    /// `styled_dom.node_hierarchy`, so no real layout (and no font) is needed.
    /// The DOM here is a pure *navigation skeleton*: the state a handler edits
    /// is always the `RefAny` passed to it, never this DOM's own dataset.
    fn skeleton() -> (StyledDom, Nodes) {
        let styled = StyledDom::create_from_dom(TextArea::create().dom());

        fn one(styled: &StyledDom, class: &str) -> usize {
            let found = nodes_with_class(styled, class);
            assert_eq!(found.len(), 1, "expected exactly one `{class}` node");
            found[0]
        }

        let label = one(&styled, "__azul-native-text-area-label");
        let nodes = Nodes {
            container: one(&styled, "__azul-native-text-area-container"),
            placeholder: one(&styled, "__azul-native-text-area-placeholder"),
            label,
            label_text: first_child(&styled, label),
        };
        (styled, nodes)
    }

    /// The flattened index of `node`'s first child.
    fn first_child(styled: &StyledDom, node: usize) -> usize {
        styled
            .node_hierarchy
            .as_ref()
            .get(node)
            .and_then(|item| item.first_child_id(NodeId::new(node)))
            .expect("expected a child node")
            .index()
    }

    fn dom_node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomLayoutResult` with an empty layout tree and no display list.
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

    /// Everything the handlers read out of the window.
    struct Env {
        /// `false` installs a `LayoutWindow` with no layout result at all — the
        /// "callback fired before the first layout" case.
        with_dom: bool,
        changeset: Option<PendingTextEdit>,
        keycode: Option<VirtualKeyCode>,
        hit: Hit,
    }

    impl Default for Env {
        fn default() -> Self {
            Self {
                with_dom: true,
                changeset: None,
                keycode: None,
                hit: Hit::Container,
            }
        }
    }

    impl Env {
        fn typed(text: &str) -> Self {
            Self {
                changeset: Some(PendingTextEdit {
                    node: dom_node(0),
                    inserted_text: AzString::from(text),
                    old_text: AzString::from(""),
                }),
                ..Self::default()
            }
        }

        fn key(code: VirtualKeyCode) -> Self {
            Self {
                keycode: Some(code),
                ..Self::default()
            }
        }

        fn hitting(mut self, hit: Hit) -> Self {
            self.hit = hit;
            self
        }
    }

    /// Invokes `call` against a `LayoutWindow` built from `env`. Returns the
    /// handler's value, every recorded `CallbackChange`, and the node indices.
    fn run<R>(
        env: Env,
        data: &RefAny,
        call: impl FnOnce(RefAny, CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>, Nodes) {
        let (styled, nodes) = skeleton();

        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if env.with_dom {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(styled));
        }
        if let Some(changeset) = env.changeset {
            layout_window.text_input_manager.set_changeset(changeset);
        }

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
            system_style: Arc::new(azul_css::system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let hit = match env.hit {
            Hit::Nothing => DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::NONE,
            },
            Hit::Container => dom_node(nodes.container),
            Hit::Placeholder => dom_node(nodes.placeholder),
            Hit::TextLeaf => dom_node(nodes.label_text),
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let out = call(data.clone(), info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded, nodes)
    }

    /// Every opacity write in the change log, as `(node index, normalized opacity)`.
    fn opacity_writes(changes: &[CallbackChange]) -> Vec<(usize, f32)> {
        let mut out = Vec::new();
        for change in changes {
            if let CallbackChange::ChangeNodeCssProperties {
                node_id, properties, ..
            } = change
            {
                for p in properties.as_ref() {
                    if let CssProperty::Opacity(v) = p {
                        if let Some(o) = v.get_property() {
                            out.push((node_id.index(), o.inner.normalized()));
                        }
                    }
                }
            }
        }
        out
    }

    /// Every text write in the change log, as `(node index, new text)`.
    fn text_writes(changes: &[CallbackChange]) -> Vec<(usize, String)> {
        changes
            .iter()
            .filter_map(|change| match change {
                CallbackChange::ChangeNodeText { node_id, text } => Some((
                    node_id
                        .node
                        .into_crate_internal()
                        .expect("a text write always targets a real node")
                        .index(),
                    text.as_str().to_string(),
                )),
                _ => None,
            })
            .collect()
    }

    // ==================================================================
    // TextAreaState::get_text
    // ==================================================================

    #[test]
    fn get_text_on_a_default_state_is_empty() {
        let state = TextAreaState::default();
        assert_eq!(state.get_text(), "");
        assert!(state.text.is_empty());
        assert_eq!(state.cursor_pos, 0);
        assert_eq!(state.max_len, 1000);
        assert!(state.placeholder.is_none());
    }

    #[test]
    fn get_text_round_trips_every_sample_string() {
        for s in ROUND_TRIP {
            let area = TextArea::create().with_text(AzString::from(s));
            assert_eq!(
                area.text_area_state.inner.get_text(),
                s,
                "set_text -> get_text must be lossless for {s:?}"
            );
            assert_eq!(
                area.text_area_state.inner.text.len(),
                s.chars().count(),
                "the buffer counts chars, not bytes, for {s:?}"
            );
        }
    }

    #[test]
    fn get_text_drops_code_units_that_are_not_scalar_values() {
        // A `U32Vec` is not a `String`: it can hold surrogates and out-of-range
        // values. `get_text` must silently drop them, never panic.
        for unit in NON_SCALAR {
            let state = TextAreaState {
                text: vec![unit].into(),
                ..TextAreaState::default()
            };
            assert_eq!(
                state.get_text(),
                "",
                "0x{unit:X} is not a scalar value and must not reach the string"
            );
            assert_eq!(state.text.len(), 1, "the raw buffer keeps the unit");
        }
    }

    #[test]
    fn get_text_keeps_the_scalars_around_dropped_units() {
        let mut units = vec!['a' as u32];
        units.extend(NON_SCALAR);
        units.push('b' as u32);
        let state = TextAreaState {
            text: units.into(),
            ..TextAreaState::default()
        };

        assert_eq!(state.get_text(), "ab", "only the non-scalars may be dropped");
        assert_eq!(state.text.len(), NON_SCALAR.len() + 2);
    }

    #[test]
    fn get_text_accepts_the_boundary_scalars() {
        // The exact edges of the two legal ranges: 0, the last code point below
        // the surrogate block, the first above it, and the very last scalar.
        let units = vec![0x0000, 0xD7FF, 0xE000, 0x0010_FFFF];
        let state = TextAreaState {
            text: units.clone().into(),
            ..TextAreaState::default()
        };
        assert_eq!(
            state.get_text().chars().count(),
            units.len(),
            "every boundary scalar must survive"
        );
    }

    #[test]
    fn get_text_handles_a_very_large_buffer() {
        let big: String = "line 😀 ünicode\n".repeat(20_000);
        let area = TextArea::create().with_text(AzString::from(big.as_str()));

        assert_eq!(area.text_area_state.inner.text.len(), big.chars().count());
        assert_eq!(area.text_area_state.inner.get_text(), big);
    }

    // ==================================================================
    // TextArea::create
    // ==================================================================

    #[test]
    fn create_equals_default() {
        assert_eq!(TextArea::create(), TextArea::default());
    }

    #[test]
    fn create_starts_empty_with_no_hooks() {
        let area = TextArea::create();
        let s = &area.text_area_state;

        assert!(s.inner.text.is_empty());
        assert!(s.inner.placeholder.is_none());
        assert_eq!(s.inner.max_len, 1000);
        assert_eq!(s.inner.cursor_pos, 0);
        assert!(s.on_text_input.is_none());
        assert!(s.on_virtual_key_down.is_none());
        assert!(s.on_focus_lost.is_none());
        assert!(s.update_text_area_before_calling_focus_lost_fn);
    }

    #[test]
    fn create_ships_all_three_style_vectors_non_empty() {
        let area = TextArea::create();
        assert!(!area.container_style.as_ref().is_empty());
        assert!(!area.label_style.as_ref().is_empty());
        assert!(!area.placeholder_style.as_ref().is_empty());
    }

    #[test]
    fn create_is_repeatable_and_unshared() {
        // Two areas must be equal but must not alias: editing one may not be
        // visible in the other.
        let mut a = TextArea::create();
        let b = TextArea::create();
        a.set_text(AzString::from("mutated"));

        assert_ne!(a, b);
        assert_eq!(b.text_area_state.inner.get_text(), "");
    }

    // ==================================================================
    // TextArea::set_text / with_text
    // ==================================================================

    #[test]
    fn set_text_preserves_newlines() {
        let mut area = TextArea::create();
        area.set_text(AzString::from("a\nb\n\nc\n"));

        assert_eq!(area.text_area_state.inner.get_text(), "a\nb\n\nc\n");
        assert_eq!(
            area.text_area_state
                .inner
                .text
                .iter()
                .filter(|c| **c == '\n' as u32)
                .count(),
            4,
            "all four newlines have to be stored"
        );
    }

    #[test]
    fn set_text_replaces_rather_than_appends() {
        let mut area = TextArea::create();
        area.set_text(AzString::from("first"));
        area.set_text(AzString::from("second"));

        assert_eq!(area.text_area_state.inner.get_text(), "second");
        assert_eq!(area.text_area_state.inner.text.len(), 6);
    }

    #[test]
    fn set_text_with_an_empty_string_clears_the_buffer() {
        let mut area = TextArea::create().with_text(AzString::from("something"));
        area.set_text(AzString::from(""));

        assert!(area.text_area_state.inner.text.is_empty());
        assert_eq!(area.text_area_state.inner.get_text(), "");
    }

    #[test]
    fn with_text_is_exactly_set_text() {
        for s in ROUND_TRIP {
            let mut a = TextArea::create();
            a.set_text(AzString::from(s));
            let b = TextArea::create().with_text(AzString::from(s));
            assert_eq!(a, b, "the builder and the setter must agree for {s:?}");
        }
    }

    #[test]
    fn set_text_ignores_max_len() {
        // `max_len` is stored but never enforced anywhere in this widget.
        // Pinning that here so a future limit check is a deliberate change and
        // not a silent behaviour flip.
        let mut area = TextArea::create();
        area.text_area_state.inner.max_len = 3;
        area.set_text(AzString::from("far past the limit"));

        assert_eq!(area.text_area_state.inner.text.len(), 18);
        assert_eq!(area.text_area_state.inner.max_len, 3);
    }

    #[test]
    fn set_text_leaves_a_stale_cursor_behind() {
        // `set_text` does not touch `cursor_pos`, so shrinking the text can
        // leave the cursor pointing past the end. `dom()` is what repairs it.
        let mut area = TextArea::create().with_text(AzString::from("0123456789"));
        area.text_area_state.inner.cursor_pos = 10;
        area.set_text(AzString::from(""));

        assert_eq!(
            area.text_area_state.inner.cursor_pos, 10,
            "the setter deliberately leaves the cursor alone"
        );
        assert!(area.text_area_state.inner.text.is_empty());

        let dom = area.dom();
        let mut dataset = dom
            .root
            .get_dataset()
            .cloned()
            .expect("dom() must attach the state");
        let w = dataset
            .downcast_ref::<TextAreaStateWrapper>()
            .expect("the dataset must be a TextAreaStateWrapper");
        assert_eq!(w.inner.cursor_pos, 0, "dom() must repair the stale cursor");
    }

    #[test]
    fn set_text_does_not_disturb_the_other_fields() {
        let area = TextArea::create()
            .with_placeholder(AzString::from("type here"))
            .with_container_style(style(3))
            .with_text(AzString::from("body"));

        assert_eq!(
            area.text_area_state.inner.placeholder.as_ref().map(AzString::as_str),
            Some("type here")
        );
        assert_eq!(area.container_style.len(), 3);
        assert_eq!(area.text_area_state.inner.get_text(), "body");
    }

    // ==================================================================
    // TextArea::set_placeholder / with_placeholder
    // ==================================================================

    #[test]
    fn placeholder_round_trips_every_sample_string() {
        for s in ROUND_TRIP {
            let area = TextArea::create().with_placeholder(AzString::from(s));
            assert_eq!(
                area.text_area_state.inner.placeholder.as_ref().map(AzString::as_str),
                Some(s)
            );
        }
    }

    #[test]
    fn an_empty_placeholder_is_some_not_none() {
        // `Some("")` and `None` are different states: only the former means
        // "the user explicitly asked for no placeholder text".
        let area = TextArea::create().with_placeholder(AzString::from(""));
        assert!(area.text_area_state.inner.placeholder.is_some());
        assert_eq!(
            area.text_area_state.inner.placeholder.as_ref().map(AzString::as_str),
            Some("")
        );
    }

    #[test]
    fn set_placeholder_overwrites_the_previous_one() {
        let mut area = TextArea::create();
        area.set_placeholder(AzString::from("one"));
        area.set_placeholder(AzString::from("two"));

        assert_eq!(
            area.text_area_state.inner.placeholder.as_ref().map(AzString::as_str),
            Some("two")
        );
    }

    #[test]
    fn with_placeholder_is_exactly_set_placeholder() {
        let mut a = TextArea::create();
        a.set_placeholder(AzString::from("hint"));
        let b = TextArea::create().with_placeholder(AzString::from("hint"));
        assert_eq!(a, b);
    }

    #[test]
    fn set_placeholder_does_not_touch_the_text() {
        let area = TextArea::create()
            .with_text(AzString::from("body\ntext"))
            .with_placeholder(AzString::from("hint"));

        assert_eq!(area.text_area_state.inner.get_text(), "body\ntext");
        assert_eq!(area.text_area_state.inner.cursor_pos, 0);
    }

    // ==================================================================
    // TextArea::set_on_* / with_on_*
    // ==================================================================

    #[test]
    fn each_hook_setter_touches_only_its_own_slot() {
        let text_in = TextArea::create()
            .with_on_text_input(RefAny::new(1u32), record_text_input as TextAreaOnTextInputCallbackType);
        assert!(text_in.text_area_state.on_text_input.is_some());
        assert!(text_in.text_area_state.on_virtual_key_down.is_none());
        assert!(text_in.text_area_state.on_focus_lost.is_none());

        let key_down = TextArea::create().with_on_virtual_key_down(
            RefAny::new(2u32),
            record_virtual_key as TextAreaOnVirtualKeyDownCallbackType,
        );
        assert!(key_down.text_area_state.on_text_input.is_none());
        assert!(key_down.text_area_state.on_virtual_key_down.is_some());
        assert!(key_down.text_area_state.on_focus_lost.is_none());

        let focus = TextArea::create()
            .with_on_focus_lost(RefAny::new(3u32), record_focus_lost as TextAreaOnFocusLostCallbackType);
        assert!(focus.text_area_state.on_text_input.is_none());
        assert!(focus.text_area_state.on_virtual_key_down.is_none());
        assert!(focus.text_area_state.on_focus_lost.is_some());
    }

    #[test]
    fn hook_setters_keep_the_user_payload_reachable() {
        let payload = RefAny::new(0xDEAD_BEEF_u32);
        let area = TextArea::create()
            .with_on_text_input(payload.clone(), record_text_input as TextAreaOnTextInputCallbackType);

        let mut stored = area
            .text_area_state
            .on_text_input
            .as_ref()
            .expect("the hook must be stored")
            .refany
            .clone();
        assert_eq!(
            *stored.downcast_ref::<u32>().expect("payload type must survive"),
            0xDEAD_BEEF_u32
        );
    }

    #[test]
    fn setting_a_hook_twice_replaces_it_and_keeps_the_old_payload_alive() {
        let first = RefAny::new(11u32);
        let second = RefAny::new(22u32);
        let mut area = TextArea::create();
        area.set_on_text_input(first.clone(), record_text_input as TextAreaOnTextInputCallbackType);
        area.set_on_text_input(second, record_text_input as TextAreaOnTextInputCallbackType);

        let mut stored = area
            .text_area_state
            .on_text_input
            .as_ref()
            .expect("the hook must be stored")
            .refany
            .clone();
        assert_eq!(*stored.downcast_ref::<u32>().expect("payload"), 22);

        // The replaced handle must not have been freed out from under us.
        let mut first = first;
        assert_eq!(*first.downcast_ref::<u32>().expect("payload"), 11);
    }

    #[test]
    fn with_on_hooks_are_exactly_their_setters() {
        let payload = RefAny::new(7u32);

        let mut a = TextArea::create();
        a.set_on_focus_lost(payload.clone(), record_focus_lost as TextAreaOnFocusLostCallbackType);
        let b = TextArea::create()
            .with_on_focus_lost(payload, record_focus_lost as TextAreaOnFocusLostCallbackType);
        assert_eq!(a, b);
    }

    #[test]
    fn all_three_hooks_can_coexist() {
        let area = TextArea::create()
            .with_on_text_input(RefAny::new(1u32), record_text_input as TextAreaOnTextInputCallbackType)
            .with_on_virtual_key_down(
                RefAny::new(2u32),
                record_virtual_key as TextAreaOnVirtualKeyDownCallbackType,
            )
            .with_on_focus_lost(RefAny::new(3u32), record_focus_lost as TextAreaOnFocusLostCallbackType)
            .with_text(AzString::from("still here"));

        assert!(area.text_area_state.on_text_input.is_some());
        assert!(area.text_area_state.on_virtual_key_down.is_some());
        assert!(area.text_area_state.on_focus_lost.is_some());
        assert_eq!(area.text_area_state.inner.get_text(), "still here");
    }

    #[test]
    fn hook_setters_leave_a_zero_sized_payload_usable() {
        // A `RefAny` over a ZST is the degenerate case for the refcount /
        // destructor plumbing.
        struct Zst;
        let area = TextArea::create()
            .with_on_text_input(RefAny::new(Zst), record_text_input as TextAreaOnTextInputCallbackType);

        let mut stored = area
            .text_area_state
            .on_text_input
            .as_ref()
            .expect("the hook must be stored")
            .refany
            .clone();
        assert!(stored.downcast_ref::<Zst>().is_some());
        assert!(stored.downcast_ref::<u32>().is_none(), "the type tag must still discriminate");
    }

    // ==================================================================
    // TextArea::set_container_style / with_container_style
    // ==================================================================

    #[test]
    fn set_container_style_replaces_the_whole_vector() {
        let mut area = TextArea::create();
        let before = area.container_style.len();
        area.set_container_style(style(2));

        assert_eq!(area.container_style.len(), 2);
        assert_ne!(before, 2, "the fixture has to actually change something");
    }

    #[test]
    fn an_empty_container_style_is_accepted() {
        let area = TextArea::create()
            .with_container_style(CssPropertyWithConditionsVec::from_vec(Vec::new()));
        assert!(area.container_style.as_ref().is_empty());

        // ...and still produces a DOM.
        let dom = area.dom();
        assert_eq!(dom.children.as_ref().len(), 2);
    }

    #[test]
    fn container_style_does_not_leak_into_the_other_style_slots() {
        let default_label = TextArea::create().label_style;
        let default_placeholder = TextArea::create().placeholder_style;
        let area = TextArea::create().with_container_style(style(1));

        assert_eq!(area.label_style, default_label);
        assert_eq!(area.placeholder_style, default_placeholder);
    }

    // ==================================================================
    // TextArea::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_hands_back_the_old_value_and_resets_self() {
        let mut area = TextArea::create()
            .with_text(AzString::from("keep\nme"))
            .with_placeholder(AzString::from("hint"));

        let old = area.swap_with_default();

        assert_eq!(old.text_area_state.inner.get_text(), "keep\nme");
        assert_eq!(
            old.text_area_state.inner.placeholder.as_ref().map(AzString::as_str),
            Some("hint")
        );
        assert_eq!(area, TextArea::default(), "self must be a fresh default");
    }

    #[test]
    fn swap_with_default_twice_yields_a_default_the_second_time() {
        let mut area = TextArea::create().with_text(AzString::from("x"));
        let first = area.swap_with_default();
        let second = area.swap_with_default();

        assert_eq!(first.text_area_state.inner.get_text(), "x");
        assert_eq!(second, TextArea::default());
        assert_eq!(area, TextArea::default());
    }

    #[test]
    fn swap_with_default_carries_the_hooks_out_with_it() {
        let payload = RefAny::new(99u32);
        let mut area = TextArea::create()
            .with_on_focus_lost(payload, record_focus_lost as TextAreaOnFocusLostCallbackType);

        let old = area.swap_with_default();

        assert!(old.text_area_state.on_focus_lost.is_some());
        assert!(area.text_area_state.on_focus_lost.is_none());

        let mut stored = old
            .text_area_state
            .on_focus_lost
            .as_ref()
            .expect("hook")
            .refany
            .clone();
        assert_eq!(*stored.downcast_ref::<u32>().expect("payload"), 99);
    }

    // ==================================================================
    // TextArea::dom
    // ==================================================================

    #[test]
    fn dom_has_the_shape_the_handlers_navigate() {
        let dom = TextArea::create().dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 2, "a text area is exactly [placeholder, label]");
        assert!(dom.root.has_class("__azul-native-text-area-container"));
        assert!(children[0].root.has_class("__azul-native-text-area-placeholder"));
        assert!(children[1].root.has_class("__azul-native-text-area-label"));

        for block in children {
            assert!(matches!(block.root.get_node_type(), NodeType::P));
            assert_eq!(block.children.as_ref().len(), 1, "a label wraps one text node");
            let leaf = &block.children.as_ref()[0];
            assert!(matches!(leaf.root.get_node_type(), NodeType::Text(_)));
            assert!(leaf.children.as_ref().is_empty(), "the text node is a leaf");
        }
    }

    #[test]
    fn dom_emits_no_cursor_node() {
        // The caret and the selection are display-list items driven by the
        // engine's TextEditManager; a widget-owned cursor div resolved against
        // the container and never tracked the caret.
        let styled = StyledDom::create_from_dom(TextArea::create().dom());
        assert!(nodes_with_class(&styled, "__azul-native-text-area-cursor").is_empty());
    }

    #[test]
    fn dom_carries_no_state_on_any_text_node() {
        // A NodeType::Text node is unconditionally inline-level and owns no
        // rect, so css props / callbacks / a tab index / a dataset / children on
        // one are all inert. Every text node must be a bare leaf under a <p>.
        fn walk(node: &Dom, parent_is_p: bool, bad: &mut Vec<String>) {
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
                walk(c, is_p, bad);
            }
        }

        for area in [
            TextArea::create(),
            TextArea::create()
                .with_text(AzString::from("a\nb"))
                .with_placeholder(AzString::from("hint")),
        ] {
            let mut bad = Vec::new();
            walk(&area.dom(), false, &mut bad);
            assert!(bad.is_empty(), "text nodes carrying inert state: {bad:?}");
        }
    }

    #[test]
    fn dom_marks_the_container_as_keyboard_focusable_and_editable() {
        // Focus events do not bubble and the engine records an edit against the
        // FOCUSED node, so the tab index and the contenteditable flag have to
        // sit on the same node the handlers are attached to.
        let dom = TextArea::create().dom();
        assert_eq!(dom.root.get_tab_index(), Some(TabIndex::Auto));
        assert!(dom.root.is_contenteditable());
    }

    #[test]
    fn dom_keeps_the_placeholder_out_of_the_editable_content() {
        // Everything inside a contenteditable host is editable content unless a
        // node blocks the inheritance walk; the prompt must never be typed into.
        let dom = TextArea::create().with_placeholder(AzString::from("hint")).dom();
        let children = dom.children.as_ref();
        assert!(
            children[0]
                .root
                .attributes()
                .as_ref()
                .iter()
                .any(|a| matches!(a, AttributeType::ContentEditable(false))),
            "the placeholder is inside the editable host and does not opt out",
        );
        assert!(!children[1]
            .root
            .attributes()
            .as_ref()
            .iter()
            .any(|a| matches!(a, AttributeType::ContentEditable(_))));
    }

    #[test]
    fn dom_renders_the_text_into_the_label_and_the_placeholder_into_its_own_node() {
        let dom = TextArea::create()
            .with_text(AzString::from("body\nlines"))
            .with_placeholder(AzString::from("hint"))
            .dom();
        let children = dom.children.as_ref();

        assert_eq!(text_of(&children[0]), Some("hint"));
        assert_eq!(text_of(&children[1]), Some("body\nlines"));
    }

    #[test]
    fn dom_renders_an_empty_placeholder_node_when_none_was_set() {
        // The node must still exist: every handler navigates *through* it to
        // reach the label.
        let dom = TextArea::create().with_text(AzString::from("x")).dom();
        assert_eq!(text_of(&dom.children.as_ref()[0]), Some(""));
    }

    #[test]
    fn dom_round_trips_every_sample_string_into_the_label() {
        for s in ROUND_TRIP {
            let dom = TextArea::create().with_text(AzString::from(s)).dom();
            assert_eq!(
                text_of(&dom.children.as_ref()[1]),
                Some(s),
                "the label must render {s:?} verbatim"
            );
        }
    }

    #[test]
    fn dom_drops_non_scalar_units_from_the_label() {
        let mut area = TextArea::create().with_text(AzString::from("ab"));
        let mut units = area.text_area_state.inner.text.clone().into_library_owned_vec();
        units.extend(NON_SCALAR);
        area.text_area_state.inner.text = units.into();

        let dom = area.dom();
        assert_eq!(
            text_of(&dom.children.as_ref()[1]),
            Some("ab"),
            "the rendered label may only contain real scalars"
        );
    }

    #[test]
    fn dom_snaps_the_cursor_to_the_end_of_the_buffer() {
        for (text, expected) in [("", 0), ("abc", 3), ("😀😀", 2), ("a\nb", 3)] {
            let mut area = TextArea::create().with_text(AzString::from(text));
            area.text_area_state.inner.cursor_pos = usize::MAX;

            let dom = area.dom();
            let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
            let w = dataset
                .downcast_ref::<TextAreaStateWrapper>()
                .expect("the dataset must be a TextAreaStateWrapper");
            assert_eq!(
                w.inner.cursor_pos, expected,
                "dom() must clamp the cursor to the buffer for {text:?}"
            );
        }
    }

    #[test]
    fn dom_wires_up_all_four_focus_callbacks() {
        let dom = TextArea::create().dom();
        let callbacks = dom.root.get_callbacks();
        assert_eq!(callbacks.len(), 4);

        let expected = [
            (
                EventFilter::Focus(FocusEventFilter::FocusReceived),
                default_on_focus_received as usize,
            ),
            (
                EventFilter::Focus(FocusEventFilter::FocusLost),
                default_on_focus_lost as usize,
            ),
            (
                EventFilter::Focus(FocusEventFilter::TextInput),
                default_on_text_input as usize,
            ),
            (
                EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                default_on_virtual_key_down as usize,
            ),
        ];

        for (cd, (event, cb)) in callbacks.as_ref().iter().zip(expected) {
            assert_eq!(cd.event, event);
            assert_eq!(cd.callback.cb, cb, "wrong handler wired to {event:?}");
        }
    }

    #[test]
    fn dom_shares_one_state_handle_between_the_dataset_and_every_callback() {
        let dom = TextArea::create().with_text(AzString::from("seed")).dom();
        let dataset = dom.root.get_dataset().cloned().expect("dataset");
        poke(&dataset, |w| w.inner.max_len = 7);

        for cd in dom.root.get_callbacks().as_ref() {
            let mut handle = cd.refany.clone();
            let w = handle
                .downcast_ref::<TextAreaStateWrapper>()
                .expect("every callback must carry the state wrapper");
            assert_eq!(
                w.inner.max_len, 7,
                "every callback must see the *same* state object as the dataset"
            );
        }
    }

    #[test]
    fn dom_survives_a_very_large_buffer() {
        let big: String = "wide 😀 line\n".repeat(20_000);
        let dom = TextArea::create().with_text(AzString::from(big.as_str())).dom();
        assert_eq!(text_of(&dom.children.as_ref()[1]), Some(big.as_str()));
    }

    #[test]
    fn styled_dom_navigation_matches_what_the_handlers_assume() {
        // Every handler walks container -> first child (placeholder) -> next
        // sibling (label). If that walk ever stops matching the DOM, all of
        // them silently no-op.
        let (styled, nodes) = skeleton();
        let hierarchy = styled.node_hierarchy.as_container();

        let placeholder = hierarchy[NodeId::new(nodes.container)]
            .first_child_id(NodeId::new(nodes.container))
            .expect("the container must have a first child");
        assert_eq!(placeholder.index(), nodes.placeholder);

        let label = hierarchy[placeholder]
            .next_sibling_id()
            .expect("the placeholder must have a next sibling");
        assert_eq!(label.index(), nodes.label);

        let leaf = hierarchy[label]
            .first_child_id(label)
            .expect("the value block must wrap a text node");
        assert_eq!(leaf.index(), nodes.label_text);
        assert!(hierarchy[leaf].first_child_id(leaf).is_none(), "the text node is a leaf");
    }

    // ==================================================================
    // default_on_focus_received
    // ==================================================================

    #[test]
    fn focus_received_ignores_a_foreign_payload() {
        let data = RefAny::new(0u8);
        let (update, changes, _) = run(Env::default(), &data, |r, ci| default_on_focus_received(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a foreign payload must not touch the DOM");
    }

    #[test]
    fn focus_received_bails_out_without_a_hit_node() {
        let data = RefAny::new(wrapper(""));
        poke(&data, |w| w.inner.cursor_pos = 42);

        let (update, changes, _) = run(
            Env::default().hitting(Hit::Nothing),
            &data,
            |r, ci| default_on_focus_received(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(
            read(&data).cursor_pos,
            42,
            "the early return happens *before* the cursor is repaired"
        );
    }

    #[test]
    fn focus_received_bails_out_on_a_childless_hit_node() {
        let data = RefAny::new(wrapper("text"));
        let (update, changes, _) = run(
            Env::default().hitting(Hit::TextLeaf),
            &data,
            |r, ci| default_on_focus_received(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn focus_received_hides_the_placeholder_only_while_the_buffer_is_empty() {
        let empty = RefAny::new(wrapper(""));
        let (update, changes, nodes) = run(Env::default(), &empty, |r, ci| default_on_focus_received(r, ci));
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            opacity_writes(&changes),
            vec![(nodes.placeholder, 0.0)],
            "an empty area hides its placeholder on focus"
        );

        let filled = RefAny::new(wrapper("typed"));
        let (update, changes, _) = run(Env::default(), &filled, |r, ci| default_on_focus_received(r, ci));
        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a non-empty area has nothing to hide — the placeholder is already gone"
        );
    }

    #[test]
    fn focus_received_repairs_a_stale_cursor() {
        for (text, expected) in [("", 0usize), ("abc", 3), ("😀 x", 3)] {
            let data = RefAny::new(wrapper(text));
            poke(&data, |w| w.inner.cursor_pos = usize::MAX);

            let (_, _, _) = run(Env::default(), &data, |r, ci| default_on_focus_received(r, ci));
            assert_eq!(
                read(&data).cursor_pos,
                expected,
                "focus must snap the cursor to the end for {text:?}"
            );
        }
    }

    #[test]
    fn focus_received_does_not_edit_the_buffer() {
        let data = RefAny::new(wrapper("untouched\ntext"));
        let (_, _, _) = run(Env::default(), &data, |r, ci| default_on_focus_received(r, ci));
        assert_eq!(read(&data).get_text(), "untouched\ntext");
    }

    // ==================================================================
    // default_on_focus_lost
    // ==================================================================

    #[test]
    fn focus_lost_ignores_a_foreign_payload() {
        let data = RefAny::new("not a text area".to_string());
        let (update, changes, _) = run(Env::default(), &data, |r, ci| default_on_focus_lost(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn focus_lost_shows_the_placeholder_only_while_the_buffer_is_empty() {
        let empty = RefAny::new(wrapper(""));
        let (update, changes, nodes) = run(Env::default(), &empty, |r, ci| default_on_focus_lost(r, ci));
        assert_eq!(update, Update::DoNothing);
        assert_eq!(opacity_writes(&changes), vec![(nodes.placeholder, 1.0)]);

        let filled = RefAny::new(wrapper("typed"));
        let (update, changes, _) = run(Env::default(), &filled, |r, ci| default_on_focus_lost(r, ci));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn focus_lost_forwards_the_state_to_the_user_hook() {
        let log = focus_log(Update::RefreshDomAllWindows);
        let mut state = wrapper("saved\ntext");
        state.on_focus_lost = Some(TextAreaOnFocusLost {
            callback: (record_focus_lost as TextAreaOnFocusLostCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (update, _, _) = run(Env::default(), &data, |r, ci| default_on_focus_lost(r, ci));

        assert_eq!(
            update,
            Update::RefreshDomAllWindows,
            "the hook's Update must be propagated verbatim"
        );
        let seen = focus_seen(&log);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].get_text(), "saved\ntext");
    }

    #[test]
    fn focus_lost_skips_the_user_hook_when_the_dom_has_no_children() {
        // The DOM walk happens *before* the hook is dispatched, so a text area
        // whose node has no children never notifies its owner. Pinned as the
        // current contract, not endorsed as ideal.
        let log = focus_log(Update::RefreshDom);
        let mut state = wrapper("x");
        state.on_focus_lost = Some(TextAreaOnFocusLost {
            callback: (record_focus_lost as TextAreaOnFocusLostCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (update, changes, _) = run(
            Env::default().hitting(Hit::TextLeaf),
            &data,
            |r, ci| default_on_focus_lost(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(focus_seen(&log).is_empty(), "the hook must not have run");
    }

    #[test]
    fn focus_lost_hands_the_hook_a_snapshot_it_cannot_write_back_through() {
        // The hook receives a *clone* of the inner state; mutating it (which the
        // signature allows, it is by value) must not reach the widget.
        let log = focus_log(Update::DoNothing);
        let mut state = wrapper("original");
        state.on_focus_lost = Some(TextAreaOnFocusLost {
            callback: (record_focus_lost as TextAreaOnFocusLostCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (_, _, _) = run(Env::default(), &data, |r, ci| default_on_focus_lost(r, ci));

        let mut seen = focus_seen(&log);
        assert_eq!(seen.len(), 1);
        seen[0].text = buffer("clobbered");
        assert_eq!(read(&data).get_text(), "original");
    }

    #[test]
    fn focus_lost_without_a_hook_reports_do_nothing() {
        let data = RefAny::new(wrapper("text"));
        let (update, _, _) = run(Env::default(), &data, |r, ci| default_on_focus_lost(r, ci));
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn focus_lost_does_not_move_the_cursor() {
        let data = RefAny::new(wrapper("abcdef"));
        poke(&data, |w| w.inner.cursor_pos = 2);

        let (_, _, _) = run(Env::default(), &data, |r, ci| default_on_focus_lost(r, ci));

        assert_eq!(
            read(&data).cursor_pos,
            2,
            "only focus-received re-snaps the cursor"
        );
    }

    // ==================================================================
    // default_on_text_input / default_on_text_input_inner
    // ==================================================================

    #[test]
    fn text_input_without_a_changeset_does_nothing() {
        let data = RefAny::new(wrapper("abc"));
        let (out, changes, _) = run(Env::default(), &data, default_on_text_input_inner);

        assert_eq!(out, None);
        assert!(changes.is_empty());
        assert_eq!(read(&data).get_text(), "abc");
    }

    #[test]
    fn text_input_with_an_empty_insertion_does_nothing() {
        let data = RefAny::new(wrapper("abc"));
        let (out, changes, _) = run(Env::typed(""), &data, default_on_text_input_inner);

        assert_eq!(out, None, "an empty insertion is not an edit");
        assert!(changes.is_empty());
        assert_eq!(read(&data).get_text(), "abc");
    }

    #[test]
    fn text_input_ignores_a_foreign_payload() {
        let data = RefAny::new(1234u64);
        let (out, changes, _) = run(Env::typed("x"), &data, default_on_text_input_inner);

        assert_eq!(out, None);
        assert!(changes.is_empty());
    }

    #[test]
    fn text_input_bails_out_on_a_childless_hit_node() {
        let data = RefAny::new(wrapper("abc"));
        let (out, changes, _) = run(
            Env::typed("x").hitting(Hit::TextLeaf),
            &data,
            default_on_text_input_inner,
        );

        assert_eq!(out, None);
        assert!(changes.is_empty());
        assert_eq!(read(&data).get_text(), "abc", "no DOM, no edit");
    }

    #[test]
    fn text_input_bails_out_when_the_hit_node_has_no_sibling_chain() {
        // Hitting the placeholder: its own text leaf has no next sibling, so
        // the walk stops one step in.
        let data = RefAny::new(wrapper("abc"));
        let (out, changes, _) = run(
            Env::typed("x").hitting(Hit::Placeholder),
            &data,
            default_on_text_input_inner,
        );

        assert_eq!(out, None);
        assert!(changes.is_empty());
        assert_eq!(read(&data).get_text(), "abc");
    }

    #[test]
    fn text_input_mirrors_the_insertion_and_hides_the_placeholder() {
        let data = RefAny::new(wrapper("ab"));
        let (out, changes, nodes) = run(Env::typed("cd"), &data, default_on_text_input_inner);

        assert_eq!(out, Some(Update::DoNothing), "no hook means no refresh");
        assert_eq!(read(&data).get_text(), "abcd");
        assert_eq!(
            opacity_writes(&changes),
            vec![(nodes.placeholder, 0.0)],
            "typing hides the placeholder"
        );
        assert!(
            text_writes(&changes).is_empty(),
            "the widget repainted the value itself; the engine owns the buffer"
        );
    }

    #[test]
    fn text_input_preserves_embedded_newlines() {
        let data = RefAny::new(wrapper("first"));
        let (out, changes, _) = run(Env::typed("\nsecond\n"), &data, default_on_text_input_inner);

        assert_eq!(out, Some(Update::DoNothing));
        assert_eq!(read(&data).get_text(), "first\nsecond\n");
        assert!(text_writes(&changes).is_empty());
    }

    #[test]
    fn text_input_stores_pasted_unicode_by_char() {
        for s in ROUND_TRIP {
            if s.is_empty() {
                continue; // an empty insertion is a documented no-op
            }
            let data = RefAny::new(wrapper(""));
            let (out, _, _) = run(Env::typed(s), &data, default_on_text_input_inner);

            assert_eq!(out, Some(Update::DoNothing), "insertion of {s:?}");
            let state = read(&data);
            assert_eq!(state.get_text(), s, "insertion of {s:?} must be lossless");
            assert_eq!(state.text.len(), s.chars().count());
        }
    }

    #[test]
    fn text_input_advances_the_cursor_by_bytes_not_chars() {
        // KNOWN QUIRK: the buffer grows by `chars`, but `cursor_pos` is advanced
        // by `inserted_text.len()`, which is a *byte* count. For any non-ASCII
        // insertion the cursor therefore ends up past the end of the buffer.
        // `dom()` and `default_on_focus_received` both re-snap it, which is why
        // this is survivable — pinned here so the divergence is visible.
        let data = RefAny::new(wrapper(""));
        let (_, _, _) = run(Env::typed("😀"), &data, default_on_text_input_inner);

        let state = read(&data);
        assert_eq!(state.text.len(), 1, "one char went into the buffer");
        assert_eq!(state.cursor_pos, 4, "but the cursor moved by four bytes");
        assert!(
            state.cursor_pos > state.text.len(),
            "the cursor is left past the end of the buffer"
        );

        // ASCII is the case where the two counts happen to agree.
        let ascii = RefAny::new(wrapper(""));
        let (_, _, _) = run(Env::typed("abcd"), &ascii, default_on_text_input_inner);
        let ascii_state = read(&ascii);
        assert_eq!(ascii_state.cursor_pos, ascii_state.text.len());
    }

    #[test]
    fn text_input_does_not_enforce_max_len() {
        // KNOWN GAP: `max_len` is never consulted by the edit path. Typing past
        // it is accepted silently.
        let data = RefAny::new(wrapper("ab"));
        poke(&data, |w| w.inner.max_len = 2);

        let (out, _, _) = run(Env::typed("cdefgh"), &data, default_on_text_input_inner);

        assert_eq!(out, Some(Update::DoNothing));
        assert_eq!(read(&data).get_text(), "abcdefgh");
        assert_eq!(read(&data).max_len, 2, "the limit is stored, just not applied");
    }

    #[test]
    fn text_input_shows_the_hook_the_would_be_text_before_committing() {
        let log = edit_log(ACCEPT);
        let mut state = wrapper("old");
        state.on_text_input = Some(TextAreaOnTextInput {
            callback: (record_text_input as TextAreaOnTextInputCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (out, _, _) = run(Env::typed("+new"), &data, default_on_text_input_inner);

        assert_eq!(out, Some(Update::RefreshDom), "the hook's Update wins");
        let seen = edits_seen(&log);
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].get_text(),
            "old+new",
            "the hook is shown the text as it *would* be after the edit"
        );
        assert_eq!(read(&data).get_text(), "old+new");
    }

    #[test]
    fn text_input_rejected_by_the_hook_changes_nothing() {
        let log = edit_log(REJECT);
        let mut state = wrapper("locked");
        state.on_text_input = Some(TextAreaOnTextInput {
            callback: (record_text_input as TextAreaOnTextInputCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (out, changes, _) = run(Env::typed("nope"), &data, default_on_text_input_inner);

        assert_eq!(
            out,
            Some(Update::RefreshDomAllWindows),
            "a rejected edit still returns the hook's Update"
        );
        assert_eq!(
            changes.len(),
            1,
            "a rejected edit must push nothing but the preventDefault: {changes:?}"
        );
        assert!(matches!(changes[0], CallbackChange::PreventDefault));
        let state = read(&data);
        assert_eq!(state.get_text(), "locked");
        assert_eq!(state.cursor_pos, 0, "and must not move the cursor");
        assert_eq!(edits_seen(&log).len(), 1, "the hook still ran exactly once");
    }

    #[test]
    fn text_input_accumulates_across_edits() {
        let data = RefAny::new(wrapper(""));
        for chunk in ["a", "b\n", "c"] {
            let (out, _, _) = run(Env::typed(chunk), &data, default_on_text_input_inner);
            assert_eq!(out, Some(Update::DoNothing));
        }

        let state = read(&data);
        assert_eq!(state.get_text(), "ab\nc");
        assert_eq!(state.cursor_pos, 4);
    }

    #[test]
    fn text_input_drops_non_scalar_units_the_engine_could_never_hold() {
        // The mirror is rebuilt from the *string* the engine works in, so
        // non-scalar units planted directly into the buffer do not survive an
        // edit. Nothing can render them either, so there is nothing to lose.
        let data = RefAny::new(wrapper("a"));
        poke(&data, |w| {
            let mut units = w.inner.text.clone().into_library_owned_vec();
            units.extend(NON_SCALAR);
            w.inner.text = units.into();
        });

        let (out, changes, _) = run(Env::typed("b"), &data, default_on_text_input_inner);

        assert_eq!(out, Some(Update::DoNothing));
        assert!(text_writes(&changes).is_empty());
        assert_eq!(read(&data).get_text(), "ab");
        assert_eq!(read(&data).text.len(), 2);
    }

    #[test]
    fn text_input_extern_wrapper_maps_none_onto_do_nothing() {
        let data = RefAny::new(wrapper("abc"));
        let (update, changes, _) = run(Env::default(), &data, |r, ci| default_on_text_input(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn text_input_survives_a_very_large_insertion() {
        let big: String = "chunk 😀\n".repeat(10_000);
        let data = RefAny::new(wrapper(""));

        let (out, changes, _) = run(Env::typed(&big), &data, default_on_text_input_inner);

        assert_eq!(out, Some(Update::DoNothing));
        assert_eq!(read(&data).text.len(), big.chars().count());
        assert_eq!(read(&data).get_text(), big);
        assert!(text_writes(&changes).is_empty());
    }

    // ==================================================================
    // default_on_virtual_key_down / default_on_virtual_key_down_inner
    // ==================================================================

    #[test]
    fn virtual_key_down_without_a_keycode_does_nothing() {
        let data = RefAny::new(wrapper("abc"));
        let (out, changes, _) = run(Env::default(), &data, default_on_virtual_key_down_inner);

        assert_eq!(out, None);
        assert!(changes.is_empty());
        assert_eq!(read(&data).get_text(), "abc");
    }

    #[test]
    fn virtual_key_down_ignores_a_foreign_payload() {
        let data = RefAny::new(0i64);
        let (out, changes, _) = run(
            Env::key(VirtualKeyCode::Back),
            &data,
            default_on_virtual_key_down_inner,
        );

        assert_eq!(out, None);
        assert!(changes.is_empty());
    }

    #[test]
    fn virtual_key_down_bails_out_on_a_childless_hit_node() {
        let log = edit_log(ACCEPT);
        let mut state = wrapper("abc");
        state.on_virtual_key_down = Some(TextAreaOnVirtualKeyDown {
            callback: (record_virtual_key as TextAreaOnVirtualKeyDownCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (out, changes, _) = run(
            Env::key(VirtualKeyCode::Back).hitting(Hit::TextLeaf),
            &data,
            default_on_virtual_key_down_inner,
        );

        assert_eq!(out, None);
        assert!(changes.is_empty());
        assert_eq!(read(&data).get_text(), "abc");
        assert!(
            edits_seen(&log).is_empty(),
            "the DOM walk precedes the hook, so it never ran"
        );
    }

    #[test]
    fn no_key_edits_the_buffer_behind_the_engine() {
        // Backspace, Delete and the arrows are `SystemChange::ApplySelectionOp`
        // and Enter records a structural block split — all of them engine
        // default actions. A widget that also edited its own buffer would
        // double-apply every one of them.
        for key in [
            VirtualKeyCode::Back,
            VirtualKeyCode::Delete,
            VirtualKeyCode::Return,
            VirtualKeyCode::NumpadEnter,
            VirtualKeyCode::Left,
            VirtualKeyCode::A,
            VirtualKeyCode::Space,
            VirtualKeyCode::Tab,
            VirtualKeyCode::Escape,
        ] {
            for text in ["", "abc", "a\nb"] {
                let data = RefAny::new(wrapper(text));
                let before = read(&data);
                let (out, changes, _) =
                    run(Env::key(key), &data, default_on_virtual_key_down_inner);

                assert_eq!(out, Some(Update::DoNothing), "{key:?} on {text:?}");
                assert!(changes.is_empty(), "{key:?} on {text:?} mutated the DOM: {changes:?}");
                assert_eq!(read(&data), before, "{key:?} on {text:?} edited the mirror");
            }
        }
    }

    #[test]
    fn the_hook_runs_even_for_keys_that_do_not_edit() {
        let log = edit_log(ACCEPT);
        let mut state = wrapper("abc");
        state.on_virtual_key_down = Some(TextAreaOnVirtualKeyDown {
            callback: (record_virtual_key as TextAreaOnVirtualKeyDownCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (out, changes, _) = run(
            Env::key(VirtualKeyCode::F1),
            &data,
            default_on_virtual_key_down_inner,
        );

        assert_eq!(out, Some(Update::RefreshDom), "the hook's Update is returned");
        assert!(changes.is_empty());
        let seen = edits_seen(&log);
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].get_text(), "abc", "the hook sees the pre-edit state");
    }

    #[test]
    fn a_rejecting_hook_vetoes_the_engines_default_action() {
        for key in [VirtualKeyCode::Back, VirtualKeyCode::Return] {
            let log = edit_log(REJECT);
            let mut state = wrapper("frozen");
            state.on_virtual_key_down = Some(TextAreaOnVirtualKeyDown {
                callback: (record_virtual_key as TextAreaOnVirtualKeyDownCallbackType).into(),
                refany: log.clone(),
            })
            .into();
            let data = RefAny::new(state);

            let (out, changes, _) = run(Env::key(key), &data, default_on_virtual_key_down_inner);

            assert_eq!(out, Some(Update::RefreshDomAllWindows), "{key:?}");
            assert_eq!(changes.len(), 1, "{key:?} pushed more than the veto: {changes:?}");
            assert!(matches!(changes[0], CallbackChange::PreventDefault), "{key:?}");
            assert_eq!(read(&data).get_text(), "frozen", "{key:?} must not edit");
            assert_eq!(edits_seen(&log).len(), 1);
        }
    }

    #[test]
    fn an_accepting_hook_leaves_the_default_action_alone_and_still_sets_the_update() {
        let log = edit_log(ACCEPT);
        let mut state = wrapper("ab");
        state.on_virtual_key_down = Some(TextAreaOnVirtualKeyDown {
            callback: (record_virtual_key as TextAreaOnVirtualKeyDownCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (out, changes, _) = run(
            Env::key(VirtualKeyCode::Back),
            &data,
            default_on_virtual_key_down_inner,
        );

        assert_eq!(out, Some(Update::RefreshDom));
        assert!(changes.is_empty(), "an accepted key must push nothing: {changes:?}");
        assert_eq!(read(&data).get_text(), "ab", "the engine owns the deletion");
    }

    #[test]
    fn virtual_key_down_extern_wrapper_maps_none_onto_do_nothing() {
        let data = RefAny::new(wrapper("abc"));
        let (update, changes, _) = run(Env::default(), &data, |r, ci| default_on_virtual_key_down(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn successive_insertions_accumulate_in_the_mirror_without_touching_the_dom() {
        // The engine repaints the value; the widget only tracks what was
        // inserted so its callbacks can hand the host a current state.
        let data = RefAny::new(wrapper(""));

        for (chunk, expected) in [
            ("hello", "hello"),
            ("\n", "hello\n"),
            ("world", "hello\nworld"),
        ] {
            let (_, changes, _) = run(Env::typed(chunk), &data, default_on_text_input_inner);
            assert!(
                text_writes(&changes).is_empty(),
                "the widget repainted the value for {chunk:?}"
            );
            assert_eq!(read(&data).get_text(), expected);
        }

        assert_eq!(read(&data).cursor_pos, "hello\nworld".len());
    }

    // ==================================================================
    // Every handler, fired before the first layout
    // ==================================================================

    /// An `Env` whose `LayoutWindow` holds no layout result at all — the state a
    /// callback sees if it is dispatched before the DOM has ever been laid out.
    fn before_first_layout() -> Env {
        Env {
            with_dom: false,
            ..Env::default()
        }
    }

    #[test]
    fn focus_received_is_inert_before_the_first_layout() {
        let data = RefAny::new(wrapper(""));
        poke(&data, |w| w.inner.cursor_pos = 5);

        let (update, changes, _) = run(before_first_layout(), &data, |r, ci| default_on_focus_received(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(read(&data).cursor_pos, 5, "the cursor is not repaired either");
    }

    #[test]
    fn focus_lost_is_inert_before_the_first_layout() {
        let log = focus_log(Update::RefreshDom);
        let mut state = wrapper("");
        state.on_focus_lost = Some(TextAreaOnFocusLost {
            callback: (record_focus_lost as TextAreaOnFocusLostCallbackType).into(),
            refany: log.clone(),
        })
        .into();
        let data = RefAny::new(state);

        let (update, changes, _) = run(before_first_layout(), &data, |r, ci| default_on_focus_lost(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(focus_seen(&log).is_empty());
    }

    #[test]
    fn text_input_is_inert_before_the_first_layout() {
        let data = RefAny::new(wrapper("abc"));
        let env = Env {
            with_dom: false,
            ..Env::typed("xyz")
        };

        let (out, changes, _) = run(env, &data, default_on_text_input_inner);

        assert_eq!(out, None);
        assert!(changes.is_empty());
        assert_eq!(read(&data).get_text(), "abc", "no DOM to walk, no edit");
    }

    #[test]
    fn virtual_key_down_is_inert_before_the_first_layout() {
        for key in [VirtualKeyCode::Back, VirtualKeyCode::Return] {
            let data = RefAny::new(wrapper("abc"));
            let env = Env {
                with_dom: false,
                ..Env::key(key)
            };

            let (out, changes, _) = run(env, &data, default_on_virtual_key_down_inner);

            assert_eq!(out, None, "{key:?}");
            assert!(changes.is_empty(), "{key:?}");
            assert_eq!(read(&data).get_text(), "abc", "{key:?}");
        }
    }
}

//! Combobox widget — an editable text field with a click-toggled drop-down list
//! of options. A blend of [`crate::widgets::drop_down::DropDown`] (the list of
//! options + click-to-select-by-index + `on_select` callback) and
//! [`crate::widgets::text_input::TextInput`] (the editable text field on top: the
//! user may type a free value, with `get_text_changeset` insertion + backspace
//! deletion). The open/close show-hide mirrors
//! [`crate::widgets::popover::Popover`] (an absolutely-positioned panel toggled
//! via `set_css_property(display)`), but the panel here holds a list of clickable
//! options rather than a single native menu popup.
//!
//! Structure: a `position: relative` wrapper containing a focusable *input field*
//! (a text node + a drop-down arrow) followed by an absolutely-positioned
//! *options list*, hidden by default (`display: none`). A single shared
//! [`RefAny`] holding the [`ComboBoxStateWrapper`] is attached to every callback
//! (the field's toggle/text-input/key-down handlers and each option's click
//! handler) so all of them read and mutate the *same* state — clicking the field
//! flips `open` and shows/hides the list; clicking an option fills the field with
//! the option's label (`change_node_text`), sets `selected`, closes the list, and
//! invokes the optional user `on_select(state)` with the new [`ComboBoxState`].
//! The clicked option's index is derived from its position (counting previous
//! siblings), exactly like the index-by-position approach used elsewhere.
//!
//! TODO2 — type-to-filter is NOT implemented. Live "filter-as-you-type" requires
//! the option list to be RE-RENDERED (a DOM rebuild) from the typed text on every
//! keystroke. Azul widget handlers can only patch *live* state through
//! `info.set_css_property` / `info.change_node_text` (show/hide/restyle/retext an
//! existing node) — they cannot add/remove DOM nodes, so the visible option set
//! cannot be re-filtered from a handler with the tools the other widgets use. The
//! field is therefore genuinely *editable* (you can type a free value, which is
//! reported in [`ComboBoxState::text`]), and selecting from the *full* list works
//! — but the list does not shrink as you type. A future revision could rebuild
//! the list via a full relayout (`Update::RefreshDom`) driven by a user callback
//! that owns the items, once that is runtime-verifiable.
//!
//! TODO2 — like [`Popover`], the list is placed at a fixed offset below the field
//! (it does not measure the field's height, flip near a screen edge, escape an
//! `overflow: hidden` ancestor, or raise its z-order — it relies on being the
//! later sibling to paint on top). There is no click-outside / blur dismissal
//! (closing on focus-lost races the option click and could swallow the
//! selection); the list closes on selection or on clicking the field again.
//!
//! Key types: [`ComboBox`], [`ComboBoxState`], [`ComboBoxOnSelect`].

use alloc::{string::String, vec::Vec};

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, FocusEventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class,
        IdOrClassVec, TabIndex,
    },
    refany::{OptionRefAny, RefAny},
    window::VirtualKeyCode,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, StyleFontSize},
        layout::{LayoutDisplay, LayoutPosition, LayoutFlexGrow, LayoutMinWidth, LayoutFlexDirection, LayoutAlignItems, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutTop, LayoutLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleCursor, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextColor, StyleTextAlign, StyleUserSelect},
    },
    impl_option_inner, AzString, StringVec,
};

use crate::callbacks::{Callback, CallbackInfo};

static COMBOBOX_WRAPPER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-combobox"))];
static COMBOBOX_INPUT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-combobox-input",
))];
static COMBOBOX_TEXT_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-combobox-text"))];
static COMBOBOX_ARROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-combobox-arrow",
))];
static COMBOBOX_LIST_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-combobox-list"))];
static COMBOBOX_OPTION_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-combobox-option",
))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// ---- layout (logical px) ----
/// Fixed vertical offset of the list below the wrapper's top edge (a
/// simplification - see the module-level `TODO2`; the field is ~26px tall).
const LIST_OFFSET_Y: isize = 28;
/// Minimum width of the field and the list.
const MIN_WIDTH: isize = 160;
const RADIUS: isize = 4;
const ARROW_FONT_SIZE_PX: isize = 18;

// ---- colours ----
const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
const BORDER_COLOR: ColorU = ColorU { r: 172, g: 172, b: 172, a: 255 }; // #acacac
const BORDER_FOCUS: ColorU = ColorU { r: 66, g: 134, b: 244, a: 255 }; // #4286f4
const TEXT_COLOR: ColorU = ColorU { r: 51, g: 51, b: 51, a: 255 }; // #333333
const OPTION_HOVER_BG: ColorU = ColorU { r: 234, g: 244, b: 252, a: 255 }; // #eaf4fc

const WHITE_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(WHITE)];
const WHITE_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(WHITE_BG_ITEMS);
const OPTION_HOVER_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(OPTION_HOVER_BG)];
const OPTION_HOVER_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(OPTION_HOVER_BG_ITEMS);

/// Callback invoked when an option is chosen. The [`ComboBoxState`] carries the
/// new `selected` index and the field `text` (set to the chosen label).
pub type ComboBoxOnSelectCallbackType = extern "C" fn(RefAny, CallbackInfo, ComboBoxState) -> Update;
impl_widget_callback!(
    ComboBoxOnSelect,
    OptionComboBoxOnSelect,
    ComboBoxOnSelectCallback,
    ComboBoxOnSelectCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ComboBoxOnSelectCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: COMBOBOX_ON_SELECT_INVOKER,
    invoker_ty:     AzComboBoxOnSelectCallbackInvoker,
    thunk_fn:       az_combobox_on_select_callback_thunk,
    setter_fn:      AzApp_setComboBoxOnSelectCallbackInvoker,
    from_handle_fn: AzComboBoxOnSelectCallback_createFromHostHandle,
    extra_args:     [ state: ComboBoxState ],
}

/// An editable filtered-select widget: a text field plus a click-toggled list of
/// options.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ComboBox {
    /// Runtime state (`open`/`selected`/`text`) plus the item list and the
    /// optional select callback.
    pub combo_state: ComboBoxStateWrapper,
    /// Greyed text shown in the field when no value has been typed/selected.
    pub placeholder: AzString,
    /// Style of the outer wrapper (the `position: relative` context).
    pub wrapper_style: CssPropertyWithConditionsVec,
    /// Style of the clickable, focusable, editable input field.
    pub field_style: CssPropertyWithConditionsVec,
    /// Style of the text inside the field.
    pub text_style: CssPropertyWithConditionsVec,
    /// Style of the drop-down arrow icon on the right of the field.
    pub arrow_style: CssPropertyWithConditionsVec,
    /// Style of each option row inside the list panel.
    pub option_style: CssPropertyWithConditionsVec,
    /// Extra properties appended to the options-list panel style. The
    /// open/close `display` toggle stays widget-managed; anything here wins
    /// over the built-in panel style (inline properties resolve last-wins).
    pub list_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ComboBoxStateWrapper {
    /// The mutable per-interaction state passed to `on_select`.
    pub inner: ComboBoxState,
    /// The full set of selectable options (rendered into the list).
    pub items: StringVec,
    /// Optional: function to call when an option is selected.
    pub on_select: OptionComboBoxOnSelect,
}

impl Default for ComboBoxStateWrapper {
    fn default() -> Self {
        Self {
            inner: ComboBoxState::default(),
            items: StringVec::from_const_slice(&[]),
            on_select: None.into(),
        }
    }
}

/// The live state of a [`ComboBox`]: whether the list is open, the currently
/// selected index, and the current (editable) field text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ComboBoxState {
    /// `true` = list shown, `false` (default) = list hidden.
    pub open: bool,
    /// Zero-based index of the most recently selected option.
    pub selected: usize,
    /// The current text shown in the field (typed or set from a selection).
    pub text: AzString,
}

impl Default for ComboBoxState {
    fn default() -> Self {
        Self {
            open: false,
            selected: 0,
            text: AzString::from_const_str(""),
        }
    }
}

// ---- styles ----

/// Wrapper: an inline-block positioning context so the absolutely-positioned list
/// is placed relative to it.
static COMBOBOX_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
        MIN_WIDTH,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
];

/// The clickable, focusable, editable input field (text + arrow).
static COMBOBOX_INPUT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Text)),
    // padding: 3px 4px
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(3))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(3),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        4,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(4),
    )),
    // border: 1px solid #acacac
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
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_COLOR,
        },
    )),
    // border-radius: 4px
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(WHITE_BG_VEC)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
    // focus: highlight border
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_FOCUS,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_FOCUS,
        },
    )),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_FOCUS,
    })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_FOCUS,
        },
    )),
];

/// The editable text inside the field - takes the remaining horizontal space.
static COMBOBOX_TEXT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(4),
    )),
];

/// The drop-down arrow icon on the right of the field.
static COMBOBOX_ARROW_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
        ARROW_FONT_SIZE_PX,
    ))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// Builds the floating options-list style. Only the `display` (open vs closed)
/// differs; all positioning/visual props are present in both so the runtime
/// `set_css_property(display)` toggle has everything it needs (mirroring the
/// popover/accordion approach).
fn build_list_style(open: bool) -> CssPropertyWithConditionsVec {
    let display = if open {
        LayoutDisplay::Block
    } else {
        LayoutDisplay::None
    };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(display)),
        CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
        CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(LIST_OFFSET_Y))),
        CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
            MIN_WIDTH,
        ))),
        // border: 1px solid #acacac
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
            inner: BorderStyle::Solid,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
            StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: BORDER_COLOR,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor {
                inner: BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
            inner: BORDER_COLOR,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor {
                inner: BORDER_COLOR,
            },
        )),
        // border-radius: 4px
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(WHITE_BG_VEC)),
    ])
}

/// Per-option row style: a padded, pointer-cursor block highlighted on hover.
static COMBOBOX_OPTION_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(6))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        10,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(10),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(OPTION_HOVER_BG_VEC)),
];

impl ComboBox {
    /// Creates a new combobox with the given options (no callback, nothing typed).
    #[must_use] pub fn new(items: StringVec) -> Self {
        Self {
            combo_state: ComboBoxStateWrapper {
                inner: ComboBoxState::default(),
                items,
                on_select: None.into(),
            },
            placeholder: AzString::from_const_str(""),
            wrapper_style: CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_WRAPPER_STYLE),
            field_style: CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_INPUT_STYLE),
            text_style: CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_TEXT_STYLE),
            arrow_style: CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_ARROW_STYLE),
            option_style: CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_OPTION_STYLE),
            list_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
        }
    }

    /// Creates an empty combobox.
    #[must_use] pub fn create() -> Self {
        Self::new(StringVec::from_const_slice(&[]))
    }

    /// Sets the initially-selected option index.
    #[inline]
    pub const fn set_selected(&mut self, selected: usize) {
        self.combo_state.inner.selected = selected;
    }

    /// Builder-style setter for the initially-selected index.
    #[inline]
    #[must_use] pub const fn with_selected(mut self, selected: usize) -> Self {
        self.set_selected(selected);
        self
    }

    /// Sets the initial (editable) field text.
    #[inline]
    pub fn set_text(&mut self, text: AzString) {
        self.combo_state.inner.text = text;
    }

    /// Builder-style setter for the initial field text.
    #[inline]
    #[must_use] pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// Sets the greyed placeholder shown when the field is empty.
    #[inline]
    pub fn set_placeholder(&mut self, placeholder: AzString) {
        self.placeholder = placeholder;
    }

    /// Builder-style setter for the placeholder.
    #[inline]
    #[must_use] pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
        self.set_placeholder(placeholder);
        self
    }

    /// Sets the callback invoked when an option is selected.
    #[inline]
    pub fn set_on_select<C: Into<ComboBoxOnSelectCallback>>(&mut self, data: RefAny, on_select: C) {
        self.combo_state.on_select = Some(ComboBoxOnSelect {
            callback: on_select.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the select callback.
    #[inline]
    #[must_use] pub fn with_on_select<C: Into<ComboBoxOnSelectCallback>>(
        mut self,
        data: RefAny,
        on_select: C,
    ) -> Self {
        self.set_on_select(data, on_select);
        self
    }

    /// Replaces `self` with a default (empty) combobox and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the combobox into a [`Dom`] subtree with the `__azul-native-combobox`
    /// class.
    #[must_use] pub fn dom(self) -> Dom {
        // Initial field text: the typed/selected text if present, else the
        // placeholder (a simplification — there is no separate placeholder node,
        // so the placeholder is just the initial label and is replaced on the
        // first keystroke or selection).
        let field_text = if self.combo_state.inner.text.as_str().is_empty() {
            self.placeholder.clone()
        } else {
            self.combo_state.inner.text.clone()
        };

        let open = self.combo_state.inner.open;
        let items = self.combo_state.items.clone();

        // ONE shared RefAny: the field handlers and every option handler all
        // read/mutate the same ComboBoxStateWrapper (the text_input shared-state
        // pattern), so open/selected/text stay in sync across interactions.
        let state_ref = RefAny::new(self.combo_state);

        let text_node = Dom::create_p_with_text(field_text)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_TEXT_CLASS))
            .with_css_props(self.text_style);

        let arrow = Dom::create_icon(AzString::from_const_str("arrow_drop_down"))
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_ARROW_CLASS))
            .with_css_props(self.arrow_style);

        // The focusable, editable input field. Clicking it toggles the list
        // (Hover::MouseUp) and focuses it; typing edits the text node
        // (Focus::TextInput / VirtualKeyDown), mirroring text_input.
        let field = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_INPUT_CLASS))
            .with_css_props(self.field_style)
            .with_tab_index(TabIndex::Auto)
            .with_callbacks(
                alloc::vec![
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: on_combobox_toggle as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: state_ref.clone(),
                    },
                    CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::TextInput),
                        callback: CoreCallback {
                            cb: on_combobox_text_input as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: state_ref.clone(),
                    },
                    CoreCallbackData {
                        event: EventFilter::Focus(FocusEventFilter::VirtualKeyDown),
                        callback: CoreCallback {
                            cb: on_combobox_key_down as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: state_ref.clone(),
                    },
                ]
                .into(),
            )
            .with_children(DomVec::from_vec(alloc::vec![text_node, arrow]));

        // Build the option rows. Each carries a CLONE of the shared state so its
        // click handler can mutate selected/open and read the chosen label.
        let mut option_doms: Vec<Dom> = Vec::with_capacity(items.as_ref().len());
        for option in items.as_ref() {
            option_doms.push(
                Dom::create_p_with_text(option.clone())
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_OPTION_CLASS))
                    .with_css_props(self.option_style.clone())
                    .with_tab_index(TabIndex::Auto)
                    .with_callbacks(
                        alloc::vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::MouseUp),
                            callback: CoreCallback {
                                cb: on_combobox_option_click as usize,
                                ctx: OptionRefAny::None,
                            },
                            refany: state_ref.clone(),
                        }]
                        .into(),
                    ),
            );
        }

        // Widget-managed panel style (open/close display toggle) + caller
        // extras appended last so they win (inline resolution is last-wins).
        let list_style = if self.list_style.is_empty() {
            build_list_style(open)
        } else {
            let mut merged = build_list_style(open).into_library_owned_vec();
            merged.extend(self.list_style.as_ref().iter().cloned());
            CssPropertyWithConditionsVec::from_vec(merged)
        };

        let list = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_LIST_CLASS))
            .with_css_props(list_style)
            .with_children(DomVec::from_vec(option_doms));

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_WRAPPER_CLASS))
            .with_css_props(self.wrapper_style)
            // children: [field, list] — the list is the field's next sibling.
            .with_children(DomVec::from_vec(alloc::vec![field, list]))
    }
}

impl Default for ComboBox {
    fn default() -> Self {
        Self::create()
    }
}

/// Field click handler. The hit node is the field; its next sibling is the list.
/// Flips `open` on the shared state and shows/hides the list via `display`.
extern "C" fn on_combobox_toggle(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let field = info.get_hit_node();
    let Some(list) = info.get_next_sibling(field) else {
        return Update::DoNothing;
    };

    let now_open = {
        let Some(mut combo) = data.downcast_mut::<ComboBoxStateWrapper>() else {
            return Update::DoNothing;
        };
        combo.inner.open = !combo.inner.open;
        combo.inner.open
    };

    // TODO2: shows/hides the list by toggling `display` via set_css_property; the
    // display:none/block relayout itself is not GUI-verified in this build.
    let display = if now_open {
        LayoutDisplay::Block
    } else {
        LayoutDisplay::None
    };
    info.set_css_property(list, CssProperty::const_display(display));

    Update::DoNothing
}

/// Field text-input handler - appends the typed character(s) to the editable
/// field text (mirroring `text_input`). Does NOT re-filter the list (see the
/// module-level type-to-filter `TODO2`).
extern "C" fn on_combobox_text_input(data: RefAny, info: CallbackInfo) -> Update {
    on_combobox_text_input_inner(data, info).unwrap_or(Update::DoNothing)
}

fn on_combobox_text_input_inner(mut data: RefAny, mut info: CallbackInfo) -> Option<Update> {
    let field = info.get_hit_node();
    // field -> label `<p>` -> bare text leaf: the label convention keeps the
    // styling on the block box, so the re-textable node is one level deeper.
    let text_node = info.get_first_child(info.get_first_child(field)?)?;

    let changeset = info.get_text_changeset()?;
    let inserted_text = changeset.inserted_text.as_str().to_string();
    if inserted_text.is_empty() {
        return None;
    }

    let new_text = {
        let mut combo = data.downcast_mut::<ComboBoxStateWrapper>()?;
        let mut s: String = combo.inner.text.as_str().into();
        s.push_str(&inserted_text);
        combo.inner.text = s.clone().into();
        s
    };

    info.change_node_text(text_node, new_text.into());
    Some(Update::DoNothing)
}

/// Field key-down handler - implements backspace deletion (mirroring `text_input`).
extern "C" fn on_combobox_key_down(data: RefAny, info: CallbackInfo) -> Update {
    on_combobox_key_down_inner(data, info).unwrap_or(Update::DoNothing)
}

fn on_combobox_key_down_inner(mut data: RefAny, mut info: CallbackInfo) -> Option<Update> {
    let field = info.get_hit_node();
    // field -> label `<p>` -> bare text leaf (see `on_combobox_text_input_inner`).
    let text_node = info.get_first_child(info.get_first_child(field)?)?;

    let keyboard_state = info.get_current_keyboard_state();
    let c = keyboard_state.current_virtual_keycode.into_option()?;
    if c != VirtualKeyCode::Back {
        return None;
    }

    let new_text = {
        let mut combo = data.downcast_mut::<ComboBoxStateWrapper>()?;
        let mut s: String = combo.inner.text.as_str().into();
        s.pop();
        combo.inner.text = s.clone().into();
        s
    };

    info.change_node_text(text_node, new_text.into());
    Some(Update::DoNothing)
}

/// Option click handler. The hit node is the clicked option's `<p>`; its index is
/// the number of previous siblings. Its parent is the list; the list's parent is
/// the wrapper, whose first child is the field, whose first child is the label
/// `<p>`, whose only child is the text node.
/// Fills the field with the option's label, sets `selected`, closes the list, and
/// invokes the optional user callback.
extern "C" fn on_combobox_option_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let option = info.get_hit_node();

    // index = number of previous siblings.
    let mut index = 0usize;
    let mut cursor = option;
    while let Some(prev) = info.get_previous_sibling(cursor) {
        index += 1;
        cursor = prev;
    }

    let Some(list) = info.get_parent(option) else {
        return Update::DoNothing;
    };
    let Some(wrapper) = info.get_parent(list) else {
        return Update::DoNothing;
    };
    let Some(field) = info.get_first_child(wrapper) else {
        return Update::DoNothing;
    };
    let Some(text_box) = info.get_first_child(field) else {
        return Update::DoNothing;
    };
    let Some(text_node) = info.get_first_child(text_box) else {
        return Update::DoNothing;
    };

    let (label, inner, result) = {
        let Some(mut combo) = data.downcast_mut::<ComboBoxStateWrapper>() else {
            return Update::DoNothing;
        };
        let Some(label) = combo.items.as_ref().get(index).cloned() else {
            return Update::DoNothing;
        };
        combo.inner.selected = index;
        combo.inner.text = label.clone();
        combo.inner.open = false;
        let inner = combo.inner.clone();
        let combo = &mut *combo;
        let result = match combo.on_select.as_mut() {
            Some(ComboBoxOnSelect { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner.clone())
            }
            None => Update::DoNothing,
        };
        (label, inner, result)
    };
    drop(inner);

    // Fill the field with the chosen label and close the list.
    info.change_node_text(text_node, label);
    info.set_css_property(list, CssProperty::const_display(LayoutDisplay::None));

    result
}

impl From<ComboBox> for Dom {
    fn from(c: ComboBox) -> Self {
        c.dom()
    }
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::too_many_lines)]
// `redundant_closure`: NOT redundant here. `run()` takes
// `impl FnOnce(RefAny, CallbackInfo) -> R`; `CallbackInfo` carries an elided
// lifetime, so the bound is higher-ranked (`for<'a> FnOnce(_, CallbackInfo<'a>)`).
// The handlers are `extern "C" fn` items, which do NOT satisfy a higher-ranked
// `FnOnce` bound — passing one bare fails to compile with E0277. The `|r, ci| f(r, ci)`
// wrapper is what makes the coercion happen and must stay.
#[allow(clippy::redundant_closure)]
mod autotest_generated {
    use std::{
        cell::{Cell, RefCell},
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
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

    // ------------------------------------------------------------------
    // Fixtures / helpers
    // ------------------------------------------------------------------

    /// A `StringVec` of options from string literals.
    fn sv(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
    }

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
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

    /// The icon name of a `NodeType::Icon` node.
    fn icon_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Icon(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    /// The `display` value in a node's *inline* style, if it sets one.
    fn inline_display(node: &Dom) -> Option<LayoutDisplay> {
        node.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::Display(v) => v.get_property().copied(),
                _ => None,
            })
    }

    /// The `display` declared in a built style vec.
    fn display_of(props: &CssPropertyWithConditionsVec) -> Option<LayoutDisplay> {
        props.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Display(v) => v.get_property().copied(),
            _ => None,
        })
    }

    /// The `position` declared in a built style vec.
    fn position_of(props: &CssPropertyWithConditionsVec) -> Option<LayoutPosition> {
        props.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Position(v) => v.get_property().copied(),
            _ => None,
        })
    }

    /// `(field, list)` of a rendered combobox DOM.
    fn parts(dom: &Dom) -> (&Dom, &Dom) {
        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2, "a combobox is exactly [field, list]");
        (&children[0], &children[1])
    }

    /// Flattened indices of every node carrying `class`, in tree order. Used
    /// instead of hard-coded indices so the tests do not encode the DOM
    /// flattening order.
    fn nodes_with_class(styled: &StyledDom, class: &str) -> Vec<usize> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// A styled `ComboBox::new(items).dom()` plus the flattened index of every
    /// node the handlers navigate to.
    struct Fixture {
        styled: StyledDom,
        wrapper: usize,
        field: usize,
        text: usize,
        list: usize,
        options: Vec<usize>,
    }

    fn fixture(items: &[&str]) -> Fixture {
        let styled = StyledDom::create_from_dom(ComboBox::new(sv(items)).dom());

        fn one(styled: &StyledDom, class: &str) -> usize {
            let found = nodes_with_class(styled, class);
            assert_eq!(found.len(), 1, "expected exactly one `{class}` node");
            found[0]
        }

        let wrapper = one(&styled, "__azul-native-combobox");
        let field = one(&styled, "__azul-native-combobox-input");
        // The class sits on the label `<p>`; the node the handlers re-text is
        // the bare text leaf inside it, which pre-order flattening puts next.
        let text_box = one(&styled, "__azul-native-combobox-text");
        let text = text_box + 1;
        assert!(
            matches!(styled.node_data.as_ref()[text].get_node_type(), NodeType::Text(_)),
            "the combobox field label must be `p > text`"
        );
        let list = one(&styled, "__azul-native-combobox-list");
        let options = nodes_with_class(&styled, "__azul-native-combobox-option");
        assert_eq!(options.len(), items.len());

        Fixture {
            styled,
            wrapper,
            field,
            text,
            list,
            options,
        }
    }

    /// A `DomNodeId` in the root DOM pointing at flattened node `idx`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomLayoutResult` with an *empty* layout tree: these handlers only walk
    /// `styled_dom.node_hierarchy`, so no real layout (and no font) is needed.
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

    /// Everything the combobox handlers read out of the window: the styled DOM
    /// they navigate, the pending text changeset, and the pressed key.
    #[derive(Default)]
    struct Env {
        styled: Option<StyledDom>,
        changeset: Option<PendingTextEdit>,
        keycode: Option<VirtualKeyCode>,
    }

    /// Invokes `call` against a `LayoutWindow` built from `env`, with `hit` as the
    /// hit node. Returns the handler's value plus every recorded `CallbackChange`.
    fn run<R>(
        env: Env,
        hit: usize,
        data: RefAny,
        call: impl FnOnce(RefAny, CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = env.styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd));
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
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            node(hit),
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let out = call(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    /// Every `display` write in the change log, as `(node index, display)`.
    fn display_writes(changes: &[CallbackChange]) -> Vec<(usize, LayoutDisplay)> {
        let mut out = Vec::new();
        for change in changes {
            if let CallbackChange::ChangeNodeCssProperties {
                node_id, properties, ..
            } = change
            {
                for p in properties.as_ref() {
                    if let CssProperty::Display(v) = p {
                        if let Some(d) = v.get_property() {
                            out.push((node_id.index(), *d));
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

    /// A `ComboBoxStateWrapper` payload with no user callback.
    fn state(items: &[&str], text: &str, open: bool, selected: usize) -> RefAny {
        RefAny::new(ComboBoxStateWrapper {
            inner: ComboBoxState {
                open,
                selected,
                text: AzString::from(text),
            },
            items: sv(items),
            on_select: None.into(),
        })
    }

    /// Reads the (still shared) `ComboBoxState` back out of a payload.
    fn inner_of(data: &mut RefAny) -> ComboBoxState {
        data.downcast_ref::<ComboBoxStateWrapper>()
            .expect("payload must still be a ComboBoxStateWrapper")
            .inner
            .clone()
    }

    fn on_select_cb(f: ComboBoxOnSelectCallbackType) -> ComboBoxOnSelectCallback {
        f.into()
    }

    /// Records every `ComboBoxState` a user `on_select` was handed.
    struct SelectLog {
        calls: Vec<ComboBoxState>,
    }

    extern "C" fn record_select(mut data: RefAny, _: CallbackInfo, s: ComboBoxState) -> Update {
        if let Some(mut log) = data.downcast_mut::<SelectLog>() {
            log.calls.push(s);
        }
        Update::RefreshDom
    }

    extern "C" fn select_do_nothing(_: RefAny, _: CallbackInfo, _: ComboBoxState) -> Update {
        Update::DoNothing
    }

    thread_local! {
        /// A clone of the shared state handle, smuggled into `probe_reborrow`
        /// without building a self-referential `RefAny` cycle.
        static SHARED_ALIAS: RefCell<Option<RefAny>> = const { RefCell::new(None) };
        /// `Some(true)` once `probe_reborrow` has seen the re-borrow refused.
        static REBORROW_REFUSED: Cell<Option<bool>> = const { Cell::new(None) };
    }

    extern "C" fn probe_reborrow(_: RefAny, _: CallbackInfo, _: ComboBoxState) -> Update {
        let refused = SHARED_ALIAS.with(|alias| {
            alias
                .borrow_mut()
                .as_mut()
                .expect("alias installed by the test")
                .downcast_mut::<ComboBoxStateWrapper>()
                .is_none()
        });
        REBORROW_REFUSED.with(|c| c.set(Some(refused)));
        Update::DoNothing
    }

    // ------------------------------------------------------------------
    // build_list_style
    // ------------------------------------------------------------------

    #[test]
    fn build_list_style_differs_only_in_display() {
        let closed = build_list_style(false);
        let open = build_list_style(true);
        let (c, o) = (closed.as_ref(), open.as_ref());

        assert!(!c.is_empty(), "the list style must not be empty");
        assert_eq!(
            c.len(),
            o.len(),
            "open and closed must declare the same property set so the runtime \
             `set_css_property(display)` toggle has everything it needs"
        );

        let differing: Vec<usize> = (0..c.len()).filter(|&i| c[i] != o[i]).collect();
        assert_eq!(
            differing.len(),
            1,
            "exactly one property may differ between open and closed"
        );
        assert!(matches!(
            &c[differing[0]].property,
            CssProperty::Display(_)
        ));
    }

    #[test]
    fn build_list_style_display_follows_the_flag() {
        assert_eq!(
            display_of(&build_list_style(false)),
            Some(LayoutDisplay::None),
            "a closed list is hidden"
        );
        assert_eq!(
            display_of(&build_list_style(true)),
            Some(LayoutDisplay::Block),
            "an open list is shown"
        );
    }

    #[test]
    fn build_list_style_always_positions_absolutely() {
        // Positioning must be present in BOTH states — the toggle only rewrites
        // `display`, so a missing `position` in the closed style would leave the
        // list statically positioned once opened.
        for open in [false, true] {
            let props = build_list_style(open);
            assert_eq!(
                position_of(&props),
                Some(LayoutPosition::Absolute),
                "open={open}"
            );
        }
    }

    #[test]
    fn build_list_style_is_deterministic_and_unshared() {
        // Two calls with the same flag must be equal, and neither may alias the
        // other (it allocates a fresh vec every call).
        assert_eq!(build_list_style(true), build_list_style(true));
        assert_eq!(build_list_style(false), build_list_style(false));
        assert_ne!(build_list_style(true), build_list_style(false));
    }

    // ------------------------------------------------------------------
    // ComboBox::new / create / Default
    // ------------------------------------------------------------------

    #[test]
    fn new_stores_items_and_starts_at_documented_defaults() {
        let combo = ComboBox::new(sv(&["a", "b", "c"]));

        assert_eq!(combo.combo_state.items.as_ref().len(), 3);
        assert_eq!(combo.combo_state.items.as_ref()[2].as_str(), "c");
        assert_eq!(combo.combo_state.inner, ComboBoxState::default());
        assert!(!combo.combo_state.inner.open, "the list starts closed");
        assert_eq!(combo.combo_state.inner.selected, 0);
        assert!(combo.combo_state.inner.text.as_str().is_empty());
        assert!(combo.placeholder.as_str().is_empty());
        assert!(
            combo.combo_state.on_select.is_none(),
            "ComboBox::new sets no callback"
        );
    }

    #[test]
    fn new_survives_extreme_item_lists() {
        let long = "ab".repeat(50_000);
        let cases: Vec<Vec<AzString>> = alloc::vec![
            Vec::new(),
            alloc::vec![AzString::from("")],
            alloc::vec![AzString::from("a\0b"), AzString::from("")],
            alloc::vec![AzString::from(
                "👨‍👩‍👧‍👦 e\u{0301}\u{0327} مرحبا שלום 🇩🇪"
            )],
            alloc::vec![AzString::from("\u{feff}\u{202e}rtl-override")],
            alloc::vec![AzString::from(long.as_str())],
        ];

        for items in cases {
            let combo = ComboBox::new(StringVec::from_vec(items.clone()));
            assert_eq!(combo.combo_state.items.as_ref(), items.as_slice());

            // ...and every option survives the trip through the DOM byte-for-byte
            let dom = combo.dom();
            let (_, list) = parts(&dom);
            assert_eq!(list.children.as_ref().len(), items.len());
            for (i, item) in items.iter().enumerate() {
                assert_eq!(text_of(&list.children.as_ref()[i]), Some(item.as_str()));
            }
        }
    }

    #[test]
    fn new_handles_many_duplicate_items() {
        // Duplicates are legal: selection is by index, not by label.
        let items = sv(&["same"; 512]);
        let combo = ComboBox::new(items);
        assert_eq!(combo.combo_state.items.as_ref().len(), 512);

        let dom = combo.dom();
        let (_, list) = parts(&dom);
        assert_eq!(list.children.as_ref().len(), 512);
    }

    #[test]
    fn create_is_empty_and_equals_default() {
        let combo = ComboBox::create();
        assert!(combo.combo_state.items.as_ref().is_empty());
        assert!(combo.combo_state.on_select.is_none());
        assert_eq!(combo.combo_state.inner, ComboBoxState::default());
        assert_eq!(combo, ComboBox::default());
        // repeated calls are independent, equal values
        assert_eq!(ComboBox::create(), ComboBox::create());
    }

    // ------------------------------------------------------------------
    // set_selected / with_selected  (numeric)
    // ------------------------------------------------------------------

    #[test]
    fn set_selected_stores_every_index_verbatim() {
        // The setter is documented as a plain store: no clamping to items.len(),
        // no saturation, no wrap — assert exactly that at both ends of usize.
        for value in [0usize, 1, 2, usize::MAX - 1, usize::MAX] {
            let mut combo = ComboBox::new(sv(&["a", "b"]));
            combo.set_selected(value);
            assert_eq!(combo.combo_state.inner.selected, value);
            // nothing else moved
            assert!(!combo.combo_state.inner.open);
            assert!(combo.combo_state.inner.text.as_str().is_empty());
            assert_eq!(combo.combo_state.items.as_ref().len(), 2);
        }
    }

    #[test]
    fn set_selected_last_write_wins() {
        let mut combo = ComboBox::create();
        combo.set_selected(usize::MAX);
        combo.set_selected(0);
        assert_eq!(combo.combo_state.inner.selected, 0);
        combo.set_selected(7);
        combo.set_selected(7);
        assert_eq!(combo.combo_state.inner.selected, 7, "idempotent re-set");
    }

    #[test]
    fn with_selected_matches_set_selected() {
        for value in [0usize, 3, usize::MAX] {
            let built = ComboBox::new(sv(&["a"])).with_selected(value);
            let mut mutated = ComboBox::new(sv(&["a"]));
            mutated.set_selected(value);
            assert_eq!(built, mutated);
        }
    }

    #[test]
    fn out_of_range_selected_still_renders_without_panicking() {
        // `dom()` never indexes `items` by `selected`, so an out-of-range index
        // (including usize::MAX on an EMPTY item list) must render fine.
        for (items, selected) in [
            (alloc::vec![], usize::MAX),
            (alloc::vec!["a"], 99),
            (alloc::vec!["a", "b"], usize::MAX - 1),
        ] {
            let combo = ComboBox::new(sv(&items)).with_selected(selected);
            assert_eq!(combo.combo_state.inner.selected, selected);
            let dom = combo.dom();
            let (_, list) = parts(&dom);
            assert_eq!(list.children.as_ref().len(), items.len());
        }
    }

    // ------------------------------------------------------------------
    // set_text / with_text
    // ------------------------------------------------------------------

    #[test]
    fn set_text_stores_every_string_verbatim() {
        let long = "x".repeat(100_000);
        let cases = [
            "",
            " ",
            "a\0b",
            "line\nbreak\ttab",
            "👨‍👩‍👧‍👦 e\u{0301}\u{0327} مرحبا שלום 🇩🇪",
            "\u{feff}\u{202e}rtl",
            long.as_str(),
        ];

        for case in cases {
            let mut combo = ComboBox::create();
            combo.set_text(AzString::from(case));
            assert_eq!(combo.combo_state.inner.text.as_str(), case);
            assert_eq!(
                combo.combo_state.inner.text.as_str().len(),
                case.len(),
                "no truncation at the NUL or anywhere else"
            );
        }
    }

    #[test]
    fn with_text_matches_set_text_and_last_write_wins() {
        let built = ComboBox::create().with_text("a".into()).with_text("b".into());
        let mut mutated = ComboBox::create();
        mutated.set_text("a".into());
        mutated.set_text("b".into());
        assert_eq!(built, mutated);
        assert_eq!(built.combo_state.inner.text.as_str(), "b");
    }

    // ------------------------------------------------------------------
    // set_placeholder / with_placeholder
    // ------------------------------------------------------------------

    #[test]
    fn set_placeholder_stores_verbatim_and_does_not_touch_text() {
        let mut combo = ComboBox::new(sv(&["a"]));
        combo.set_placeholder("Pick one…\u{0}".into());
        assert_eq!(combo.placeholder.as_str(), "Pick one…\u{0}");
        assert!(
            combo.combo_state.inner.text.as_str().is_empty(),
            "the placeholder is not the value"
        );

        let built = ComboBox::new(sv(&["a"])).with_placeholder("Pick one…\u{0}".into());
        assert_eq!(built, combo);
    }

    #[test]
    fn placeholder_is_the_field_label_only_while_text_is_empty() {
        // Documented simplification: there is no separate placeholder node, so the
        // field label is `text` if non-empty, else `placeholder`.
        let dom = ComboBox::create().with_placeholder("ph".into()).dom();
        let (field, _) = parts(&dom);
        assert_eq!(text_of(&field.children.as_ref()[0]), Some("ph"));

        // a single SPACE is non-empty, so it must win over the placeholder
        let spaced = ComboBox::create()
            .with_placeholder("ph".into())
            .with_text(" ".into());
        let dom = spaced.dom();
        let (field, _) = parts(&dom);
        assert_eq!(text_of(&field.children.as_ref()[0]), Some(" "));

        // ...and with no placeholder and no text the label is the empty string
        let bare = ComboBox::create().dom();
        let (field, _) = parts(&bare);
        assert_eq!(text_of(&field.children.as_ref()[0]), Some(""));
    }

    // ------------------------------------------------------------------
    // set_on_select / with_on_select
    // ------------------------------------------------------------------

    #[test]
    fn set_on_select_last_call_wins() {
        let mut combo = ComboBox::create();

        combo.set_on_select(RefAny::new(1u8), on_select_cb(select_do_nothing));
        assert!(combo.combo_state.on_select.is_some());

        // a second call must *replace* (not append / leak / panic)
        combo.set_on_select(RefAny::new(9i64), on_select_cb(record_select));
        let set = combo.combo_state.on_select.as_ref().expect("still Some");
        assert_eq!(set.refany.get_type_id(), RefAny::new(0i64).get_type_id());
        assert_eq!(set.callback, on_select_cb(record_select));
        assert_ne!(set.callback, on_select_cb(select_do_nothing));
    }

    #[test]
    fn with_on_select_matches_set_on_select() {
        let built = ComboBox::new(sv(&["a"]))
            .with_on_select(RefAny::new(7u32), on_select_cb(record_select));

        let mut mutated = ComboBox::new(sv(&["a"]));
        mutated.set_on_select(RefAny::new(7u32), on_select_cb(record_select));

        assert_eq!(
            built.combo_state.on_select.as_ref().unwrap().callback,
            mutated.combo_state.on_select.as_ref().unwrap().callback
        );
        // the builder form must not disturb the items or the state
        assert_eq!(built.combo_state.items.as_ref().len(), 1);
        assert_eq!(built.combo_state.inner, ComboBoxState::default());
    }

    // ------------------------------------------------------------------
    // swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_moves_all_state_out() {
        let mut combo = ComboBox::new(sv(&["a", "b"]))
            .with_selected(1)
            .with_text("typed".into())
            .with_placeholder("ph".into())
            .with_on_select(RefAny::new(5u8), on_select_cb(record_select));

        let original = combo.swap_with_default();

        assert_eq!(original.combo_state.items.as_ref().len(), 2);
        assert_eq!(original.combo_state.inner.selected, 1);
        assert_eq!(original.combo_state.inner.text.as_str(), "typed");
        assert_eq!(original.placeholder.as_str(), "ph");
        assert!(original.combo_state.on_select.is_some());

        assert_eq!(combo, ComboBox::create(), "self must be left empty");

        // swapping an already-empty combobox is a no-op, not a panic
        let second = combo.swap_with_default();
        assert_eq!(second, ComboBox::create());
        assert_eq!(combo, ComboBox::create());
    }

    // ------------------------------------------------------------------
    // ComboBox::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_empty_combobox_still_has_field_and_empty_list() {
        let dom = ComboBox::create().dom();
        assert!(has_class(&dom, "__azul-native-combobox"));

        let (field, list) = parts(&dom);
        assert!(has_class(field, "__azul-native-combobox-input"));
        assert!(has_class(list, "__azul-native-combobox-list"));
        assert!(
            list.children.as_ref().is_empty(),
            "no items -> no option rows"
        );
        // the field still has its text node + arrow
        assert_eq!(field.children.as_ref().len(), 2);
    }

    #[test]
    fn dom_structure_classes_and_callbacks() {
        let dom = ComboBox::new(sv(&["one", "two"])).dom();
        let (field, list) = parts(&dom);

        let text_node = &field.children.as_ref()[0];
        let arrow = &field.children.as_ref()[1];
        assert!(has_class(text_node, "__azul-native-combobox-text"));
        assert!(has_class(arrow, "__azul-native-combobox-arrow"));
        assert_eq!(icon_of(arrow), Some("arrow_drop_down"));

        // the field is focusable and wires exactly toggle / text-input / key-down
        assert!(matches!(field.root.get_tab_index(), Some(TabIndex::Auto)));
        let cbs = field.root.get_callbacks();
        assert_eq!(cbs.len(), 3);
        assert_eq!(
            cbs.as_ref()[0].event,
            EventFilter::Hover(HoverEventFilter::MouseUp)
        );
        assert_eq!(cbs.as_ref()[0].callback.cb, on_combobox_toggle as usize);
        assert_eq!(
            cbs.as_ref()[1].event,
            EventFilter::Focus(FocusEventFilter::TextInput)
        );
        assert_eq!(cbs.as_ref()[1].callback.cb, on_combobox_text_input as usize);
        assert_eq!(
            cbs.as_ref()[2].event,
            EventFilter::Focus(FocusEventFilter::VirtualKeyDown)
        );
        assert_eq!(cbs.as_ref()[2].callback.cb, on_combobox_key_down as usize);

        // every option is focusable and carries exactly one click handler
        for (i, option) in list.children.as_ref().iter().enumerate() {
            assert!(has_class(option, "__azul-native-combobox-option"));
            assert_eq!(text_of(option), Some(["one", "two"][i]));
            assert!(matches!(option.root.get_tab_index(), Some(TabIndex::Auto)));
            let cbs = option.root.get_callbacks();
            assert_eq!(cbs.len(), 1);
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp)
            );
            assert_eq!(
                cbs.as_ref()[0].callback.cb,
                on_combobox_option_click as usize
            );
        }
    }

    #[test]
    fn dom_list_display_follows_open() {
        let closed = ComboBox::new(sv(&["a"])).dom();
        assert_eq!(inline_display(parts(&closed).1), Some(LayoutDisplay::None));

        let mut open = ComboBox::new(sv(&["a"]));
        open.combo_state.inner.open = true;
        let open = open.dom();
        assert_eq!(inline_display(parts(&open).1), Some(LayoutDisplay::Block));
    }

    #[test]
    fn dom_shares_exactly_one_refany_across_every_callback() {
        // The module doc promises ONE shared RefAny: a write through the field's
        // handle must be visible through every option's handle.
        let dom = ComboBox::new(sv(&["a", "b", "c"])).dom();
        let (field, list) = parts(&dom);

        let field_refany = &field.root.get_callbacks().as_ref()[0].refany;
        for cb in field.root.get_callbacks().as_ref() {
            assert_eq!(&cb.refany, field_refany, "field handlers share one state");
        }
        for option in list.children.as_ref() {
            assert_eq!(
                &option.root.get_callbacks().as_ref()[0].refany,
                field_refany,
                "option handlers share the field's state"
            );
        }

        // ...and it is actually the same allocation, not just an equal one
        let mut writer = field_refany.clone();
        {
            let mut w = writer
                .downcast_mut::<ComboBoxStateWrapper>()
                .expect("the shared payload is a ComboBoxStateWrapper");
            w.inner.selected = 2;
            w.inner.open = true;
        }
        let mut reader = list.children.as_ref()[0].root.get_callbacks().as_ref()[0]
            .refany
            .clone();
        let seen = inner_of(&mut reader);
        assert_eq!(seen.selected, 2);
        assert!(seen.open);
    }

    #[test]
    fn dom_round_trips_items_and_state_into_the_shared_payload() {
        let combo = ComboBox::new(sv(&["α", "β", "\0"]))
            .with_selected(2)
            .with_text("typed".into());
        let expected = combo.combo_state.clone();

        let dom = combo.dom();
        let mut shared = parts(&dom).0.root.get_callbacks().as_ref()[0]
            .refany
            .clone();
        let decoded = shared
            .downcast_ref::<ComboBoxStateWrapper>()
            .expect("payload type is preserved");

        assert_eq!(decoded.inner, expected.inner);
        assert_eq!(decoded.items.as_ref(), expected.items.as_ref());
        assert!(decoded.on_select.is_none());
    }

    #[test]
    fn dom_child_count_cache_stays_consistent() {
        // A wrong `estimated_total_children` under-allocates the compact-DOM
        // arena and panics much later.
        for items in [
            alloc::vec![],
            alloc::vec!["a"],
            alloc::vec!["a", "", "\u{1F600}"],
        ] {
            let dom = ComboBox::new(sv(&items))
                .with_placeholder("ph".into())
                .dom();
            assert_eq!(
                dom.estimated_total_children,
                dom.recompute_estimated_total_children(),
                "cached descendant count desynced for {} item(s)",
                items.len()
            );
        }
    }

    #[test]
    fn from_combobox_for_dom_renders_the_same_tree() {
        // `Dom::from` delegates to `dom()`; the trees are structurally identical
        // (they are NOT `==`, because each render mints a fresh shared `RefAny`).
        let combo = ComboBox::new(sv(&["a", "b"])).with_text("t".into());
        let via_from = Dom::from(combo.clone());
        let via_dom = combo.dom();

        assert_eq!(
            via_from.estimated_total_children,
            via_dom.estimated_total_children
        );
        let (ff, fl) = parts(&via_from);
        let (df, dl) = parts(&via_dom);
        assert_eq!(text_of(&ff.children.as_ref()[0]), text_of(&df.children.as_ref()[0]));
        assert_eq!(inline_display(fl), inline_display(dl));
        assert_eq!(fl.children.as_ref().len(), dl.children.as_ref().len());
        assert_ne!(
            ff.root.get_callbacks().as_ref()[0].refany,
            df.root.get_callbacks().as_ref()[0].refany,
            "each render owns its own state allocation"
        );
    }

    // ------------------------------------------------------------------
    // on_combobox_toggle
    // ------------------------------------------------------------------

    #[test]
    fn toggle_without_any_layout_result_is_a_noop() {
        let mut data = state(&["a"], "", false, 0);
        let (update, changes) = run(Env::default(), 0, data.clone(), |r, ci| on_combobox_toggle(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(!inner_of(&mut data).open, "state must not flip");
    }

    #[test]
    fn toggle_with_a_stale_hit_node_is_a_noop() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            9_999,
            data.clone(),
            |r, ci| on_combobox_toggle(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(!inner_of(&mut data).open);
    }

    #[test]
    fn toggle_on_a_node_without_a_next_sibling_does_not_flip_state() {
        // The list is the wrapper's LAST child: hitting it finds no sibling, and
        // crucially `open` must NOT have been toggled on the way out.
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "", true, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.list,
            data.clone(),
            |r, ci| on_combobox_toggle(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(inner_of(&mut data).open, "state must be untouched");
    }

    #[test]
    fn toggle_with_a_foreign_payload_does_not_restyle() {
        let fx = fixture(&["a"]);
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.field,
            data,
            |r, ci| on_combobox_toggle(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload must not show or hide the list"
        );
    }

    #[test]
    fn toggle_flips_open_and_shows_then_hides_the_list() {
        let fx = fixture(&["a", "b"]);
        let mut data = state(&["a", "b"], "", false, 0);

        // closed -> open
        let (update, changes) = run(
            Env {
                styled: Some(fx.styled.clone()),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_toggle(r, ci),
        );
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(fx.list, LayoutDisplay::Block)],
            "the field's next sibling (the list) is the node that is shown"
        );
        assert!(inner_of(&mut data).open);

        // open -> closed (same payload, so the flip must be stateful)
        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_toggle(r, ci),
        );
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(fx.list, LayoutDisplay::None)]
        );
        assert!(!inner_of(&mut data).open);
    }

    // ------------------------------------------------------------------
    // on_combobox_text_input / on_combobox_text_input_inner
    // ------------------------------------------------------------------

    #[test]
    fn text_input_without_a_changeset_is_a_noop() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "abc", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_text_input(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(inner_of(&mut data).text.as_str(), "abc", "text untouched");
    }

    #[test]
    fn text_input_with_an_empty_insertion_is_a_noop() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "abc", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                changeset: Some(PendingTextEdit {
                    node: node(fx.text),
                    inserted_text: AzString::from(""),
                    old_text: AzString::from("abc"),
                }),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_text_input(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "an empty insertion must not re-text the node"
        );
        assert_eq!(inner_of(&mut data).text.as_str(), "abc");
    }

    #[test]
    fn text_input_on_a_childless_node_is_a_noop() {
        // A bare text leaf has no children of its own: `get_first_child`
        // returns None before any state is touched.
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "abc", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                changeset: Some(PendingTextEdit {
                    node: node(fx.text),
                    inserted_text: AzString::from("z"),
                    old_text: AzString::from("abc"),
                }),
                ..Env::default()
            },
            fx.text,
            data.clone(),
            |r, ci| on_combobox_text_input(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(inner_of(&mut data).text.as_str(), "abc");
    }

    #[test]
    fn text_input_appends_to_state_and_retexts_the_field() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "ab", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                changeset: Some(PendingTextEdit {
                    node: node(fx.text),
                    inserted_text: AzString::from("c"),
                    old_text: AzString::from("ab"),
                }),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_text_input(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            text_writes(&changes),
            alloc::vec![(fx.text, String::from("abc"))],
            "the text leaf inside the field's label <p> is the node that is re-texted"
        );
        assert_eq!(inner_of(&mut data).text.as_str(), "abc");
        // selection/open state is not disturbed by typing
        assert!(!inner_of(&mut data).open);
        assert_eq!(inner_of(&mut data).selected, 0);
    }

    #[test]
    fn text_input_accumulates_across_keystrokes() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "", false, 0);

        for (i, ch) in ["h", "é", "🌍", "\0"].iter().enumerate() {
            let (update, changes) = run(
                Env {
                    styled: Some(fx.styled.clone()),
                    changeset: Some(PendingTextEdit {
                        node: node(fx.text),
                        inserted_text: AzString::from(*ch),
                        old_text: AzString::from(""),
                    }),
                    ..Env::default()
                },
                fx.field,
                data.clone(),
                |r, ci| on_combobox_text_input(r, ci),
            );
            assert_eq!(update, Update::DoNothing);
            assert_eq!(changes.len(), 1, "keystroke {i} produced one text write");
        }

        assert_eq!(inner_of(&mut data).text.as_str(), "hé🌍\0");
    }

    #[test]
    fn text_input_ignores_the_changesets_own_target_node() {
        // Quirk worth pinning: the handler re-texts the HIT node's first child and
        // never looks at `changeset.node`. A changeset naming a nonexistent node
        // is applied to the field anyway (rather than being dropped or panicking).
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                changeset: Some(PendingTextEdit {
                    // usize::MAX - 1 is the largest index the 1-based
                    // `NodeHierarchyItemId` encoding accepts without overflowing.
                    node: node(usize::MAX - 1),
                    inserted_text: AzString::from("q"),
                    old_text: AzString::from("ignored"),
                }),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_text_input(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert_eq!(text_writes(&changes), alloc::vec![(fx.text, String::from("q"))]);
        assert_eq!(
            inner_of(&mut data).text.as_str(),
            "q",
            "`old_text` is ignored: the append is against the widget's own state"
        );
    }

    #[test]
    fn text_input_with_a_foreign_payload_leaves_the_dom_untouched() {
        let fx = fixture(&["a"]);
        let data = RefAny::new("not a combobox");

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                changeset: Some(PendingTextEdit {
                    node: node(fx.text),
                    inserted_text: AzString::from("x"),
                    old_text: AzString::from(""),
                }),
                ..Env::default()
            },
            fx.field,
            data,
            |r, ci| on_combobox_text_input(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn text_input_survives_a_huge_insertion() {
        let fx = fixture(&["a"]);
        let huge = "y".repeat(100_000);
        let mut data = state(&["a"], "", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                changeset: Some(PendingTextEdit {
                    node: node(fx.text),
                    inserted_text: AzString::from(huge.as_str()),
                    old_text: AzString::from(""),
                }),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_text_input(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert_eq!(changes.len(), 1);
        assert_eq!(inner_of(&mut data).text.as_str().len(), 100_000);
    }

    #[test]
    fn text_input_inner_reports_none_when_it_does_nothing() {
        // The `_inner` half distinguishes "nothing to do" (None) from "handled"
        // (Some) — the extern wrapper collapses both to DoNothing.
        let fx = fixture(&["a"]);

        let (out, _) = run(
            Env {
                styled: Some(fx.styled.clone()),
                ..Env::default()
            },
            fx.field,
            state(&["a"], "", false, 0),
            on_combobox_text_input_inner,
        );
        assert_eq!(out, None, "no changeset -> None");

        let (out, _) = run(
            Env {
                styled: Some(fx.styled),
                changeset: Some(PendingTextEdit {
                    node: node(fx.text),
                    inserted_text: AzString::from("k"),
                    old_text: AzString::from(""),
                }),
                ..Env::default()
            },
            fx.field,
            state(&["a"], "", false, 0),
            on_combobox_text_input_inner,
        );
        assert_eq!(out, Some(Update::DoNothing), "handled -> Some");
    }

    // ------------------------------------------------------------------
    // on_combobox_key_down / on_combobox_key_down_inner
    // ------------------------------------------------------------------

    #[test]
    fn key_down_without_a_keycode_is_a_noop() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "abc", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_key_down(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(inner_of(&mut data).text.as_str(), "abc");
    }

    #[test]
    fn key_down_ignores_every_key_except_backspace() {
        let fx = fixture(&["a"]);

        for key in [
            VirtualKeyCode::A,
            VirtualKeyCode::Return,
            VirtualKeyCode::Escape,
            VirtualKeyCode::Delete,
            VirtualKeyCode::Space,
        ] {
            let mut data = state(&["a"], "abc", false, 0);
            let (update, changes) = run(
                Env {
                    styled: Some(fx.styled.clone()),
                    keycode: Some(key),
                    ..Env::default()
                },
                fx.field,
                data.clone(),
                |r, ci| on_combobox_key_down(r, ci),
            );

            assert_eq!(update, Update::DoNothing);
            assert!(changes.is_empty(), "{key:?} must not edit the text");
            assert_eq!(inner_of(&mut data).text.as_str(), "abc");
        }
    }

    #[test]
    fn key_down_backspace_pops_one_char_not_one_byte() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "hé🌍", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled.clone()),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_key_down(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            text_writes(&changes),
            alloc::vec![(fx.text, String::from("hé"))],
            "the 4-byte 🌍 is removed whole — no UTF-8 boundary panic"
        );
        assert_eq!(inner_of(&mut data).text.as_str(), "hé");

        // the two-byte é goes next, still whole
        let (_, changes) = run(
            Env {
                styled: Some(fx.styled),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_key_down(r, ci),
        );
        assert_eq!(text_writes(&changes), alloc::vec![(fx.text, String::from("h"))]);
        assert_eq!(inner_of(&mut data).text.as_str(), "h");
    }

    #[test]
    fn key_down_backspace_deletes_by_codepoint_not_by_grapheme() {
        // Documented consequence of `String::pop`: a combining mark and a ZWJ
        // emoji sequence lose ONE codepoint per press, not the whole cluster.
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "e\u{0301}", false, 0);

        let (_, changes) = run(
            Env {
                styled: Some(fx.styled.clone()),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_key_down(r, ci),
        );
        assert_eq!(text_writes(&changes), alloc::vec![(fx.text, String::from("e"))]);
        assert_eq!(inner_of(&mut data).text.as_str(), "e");

        // Expected value is derived, not spelled out: the ZWJ joiners the family
        // sequence is built from are invisible in source.
        let family_str = "👨‍👩‍👧";
        let all_but_last: String = {
            let mut s = String::from(family_str);
            s.pop();
            s
        };
        assert_eq!(
            family_str.chars().count(),
            5,
            "man ZWJ woman ZWJ girl — 5 codepoints, 1 grapheme"
        );

        let mut family = state(&["a"], family_str, false, 0);
        let (_, _) = run(
            Env {
                styled: Some(fx.styled),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.field,
            family.clone(),
            |r, ci| on_combobox_key_down(r, ci),
        );
        let after = inner_of(&mut family).text.as_str().to_string();
        assert_eq!(
            after, all_but_last,
            "only the trailing codepoint is dropped, not the whole cluster"
        );
        assert_eq!(
            after.chars().count(),
            4,
            "the cluster is still visually broken — one press removed one codepoint"
        );
    }

    #[test]
    fn key_down_backspace_on_empty_text_is_safe() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.field,
            data.clone(),
            |r, ci| on_combobox_key_down(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            text_writes(&changes),
            alloc::vec![(fx.text, String::new())],
            "popping an empty string is a no-op write, not a panic"
        );
        assert!(inner_of(&mut data).text.as_str().is_empty());
    }

    #[test]
    fn key_down_on_a_childless_node_is_a_noop() {
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "abc", false, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.options[0],
            data.clone(),
            |r, ci| on_combobox_key_down(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(inner_of(&mut data).text.as_str(), "abc");
    }

    #[test]
    fn key_down_with_a_foreign_payload_is_a_noop() {
        let fx = fixture(&["a"]);
        let data = RefAny::new(7u16);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.field,
            data,
            |r, ci| on_combobox_key_down(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn key_down_inner_reports_none_when_it_does_nothing() {
        let fx = fixture(&["a"]);

        let (out, _) = run(
            Env {
                styled: Some(fx.styled.clone()),
                keycode: Some(VirtualKeyCode::A),
                ..Env::default()
            },
            fx.field,
            state(&["a"], "abc", false, 0),
            on_combobox_key_down_inner,
        );
        assert_eq!(out, None, "a non-backspace key -> None");

        let (out, _) = run(
            Env {
                styled: Some(fx.styled),
                keycode: Some(VirtualKeyCode::Back),
                ..Env::default()
            },
            fx.field,
            state(&["a"], "abc", false, 0),
            on_combobox_key_down_inner,
        );
        assert_eq!(out, Some(Update::DoNothing), "backspace -> Some");
    }

    // ------------------------------------------------------------------
    // on_combobox_option_click
    // ------------------------------------------------------------------

    #[test]
    fn option_click_without_any_layout_result_is_a_noop() {
        let mut data = state(&["a"], "", true, 0);
        let (update, changes) = run(Env::default(), 0, data.clone(), |r, ci| on_combobox_option_click(r, ci));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(inner_of(&mut data).open, "state must not change");
    }

    #[test]
    fn option_click_on_a_parentless_node_is_a_noop() {
        // The wrapper is the root: it has no parent, so the walk bails out.
        let fx = fixture(&["a"]);
        let mut data = state(&["a"], "", true, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.wrapper,
            data.clone(),
            |r, ci| on_combobox_option_click(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(inner_of(&mut data).open);
    }

    #[test]
    fn option_click_selects_by_previous_sibling_count() {
        let labels = ["zero", "one", "two", "three"];
        let fx = fixture(&labels);

        for (i, label) in labels.iter().enumerate() {
            let mut data = state(&labels, "", true, 999);
            let (update, changes) = run(
                Env {
                    styled: Some(fx.styled.clone()),
                    ..Env::default()
                },
                fx.options[i],
                data.clone(),
                |r, ci| on_combobox_option_click(r, ci),
            );

            assert_eq!(update, Update::DoNothing, "no user callback -> DoNothing");

            let inner = inner_of(&mut data);
            assert_eq!(inner.selected, i, "index = number of previous siblings");
            assert_eq!(inner.text.as_str(), *label, "the field takes the label");
            assert!(!inner.open, "selecting closes the list");

            assert_eq!(
                text_writes(&changes),
                alloc::vec![(fx.text, String::from(*label))]
            );
            assert_eq!(
                display_writes(&changes),
                alloc::vec![(fx.list, LayoutDisplay::None)]
            );
        }
    }

    #[test]
    fn option_click_index_walk_scales_to_a_long_list() {
        // The index is derived by walking previous siblings one at a time; make
        // sure a long list terminates and lands on the right (last) index.
        let labels: Vec<String> = (0..200).map(|i| alloc::format!("item{i}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let fx = fixture(&refs);
        let mut data = state(&refs, "", true, 0);

        let (update, _) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.options[199],
            data.clone(),
            |r, ci| on_combobox_option_click(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        let inner = inner_of(&mut data);
        assert_eq!(inner.selected, 199);
        assert_eq!(inner.text.as_str(), "item199");
    }

    #[test]
    fn option_click_with_an_out_of_range_index_changes_nothing() {
        // The rendered list has 3 rows but the payload only knows 1 item — the
        // `items.get(index)` miss must abort BEFORE any state or DOM write.
        let fx = fixture(&["a", "b", "c"]);
        let mut data = state(&["only"], "keep", true, 42);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.options[2],
            data.clone(),
            |r, ci| on_combobox_option_click(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "no partial write may escape");
        let inner = inner_of(&mut data);
        assert_eq!(inner.selected, 42, "selected must not move");
        assert_eq!(inner.text.as_str(), "keep");
        assert!(inner.open, "the list must not be closed either");
    }

    #[test]
    fn option_click_with_an_empty_item_list_changes_nothing() {
        // Same miss, taken from the other side: a DOM with rows, a payload with
        // no items at all.
        let fx = fixture(&["a"]);
        let mut data = state(&[], "keep", true, 0);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.options[0],
            data.clone(),
            |r, ci| on_combobox_option_click(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(inner_of(&mut data).text.as_str(), "keep");
    }

    #[test]
    fn option_click_with_a_foreign_payload_is_a_noop() {
        let fx = fixture(&["a"]);
        let data = RefAny::new(1u8);

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.options[0],
            data,
            |r, ci| on_combobox_option_click(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn option_click_selects_labels_with_nul_and_emoji_verbatim() {
        let labels = ["a\0b", "👨‍👩‍👧‍👦", ""];
        let fx = fixture(&labels);

        for (i, label) in labels.iter().enumerate() {
            let mut data = state(&labels, "", true, 0);
            let (_, changes) = run(
                Env {
                    styled: Some(fx.styled.clone()),
                    ..Env::default()
                },
                fx.options[i],
                data.clone(),
                |r, ci| on_combobox_option_click(r, ci),
            );

            assert_eq!(inner_of(&mut data).text.as_str(), *label);
            assert_eq!(
                text_writes(&changes),
                alloc::vec![(fx.text, String::from(*label))]
            );
        }
    }

    #[test]
    fn option_click_invokes_the_user_callback_and_propagates_its_update() {
        let fx = fixture(&["a", "b"]);
        let mut log = RefAny::new(SelectLog { calls: Vec::new() });
        let data = RefAny::new(ComboBoxStateWrapper {
            inner: ComboBoxState {
                open: true,
                selected: 0,
                text: AzString::from(""),
            },
            items: sv(&["a", "b"]),
            on_select: Some(ComboBoxOnSelect {
                callback: on_select_cb(record_select),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.options[1],
            data,
            |r, ci| on_combobox_option_click(r, ci),
        );

        // the user's return value wins over the internal DoNothing
        assert_eq!(update, Update::RefreshDom);
        // ...and the field/list are still updated, even though the user ran
        assert_eq!(
            text_writes(&changes),
            alloc::vec![(fx.text, String::from("b"))]
        );
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(fx.list, LayoutDisplay::None)]
        );

        let logged = log
            .downcast_ref::<SelectLog>()
            .expect("log payload survived");
        assert_eq!(logged.calls.len(), 1);
        assert_eq!(logged.calls[0].selected, 1, "the callback sees the NEW index");
        assert_eq!(logged.calls[0].text.as_str(), "b", "...and the NEW text");
        assert!(!logged.calls[0].open, "...and an already-closed list");
    }

    #[test]
    fn option_click_holds_the_state_borrow_across_the_user_callback() {
        // The handler invokes `on_select` while its own `downcast_mut` guard is
        // still alive, so a re-entrant borrow of the shared state from inside the
        // user callback is REFUSED (returns None) rather than aliasing or
        // deadlocking. Pinning this documents the constraint on user callbacks.
        let fx = fixture(&["a"]);
        let data = RefAny::new(ComboBoxStateWrapper {
            inner: ComboBoxState::default(),
            items: sv(&["a"]),
            on_select: Some(ComboBoxOnSelect {
                callback: on_select_cb(probe_reborrow),
                refany: RefAny::new(0u8),
            })
            .into(),
        });

        SHARED_ALIAS.with(|a| *a.borrow_mut() = Some(data.clone()));
        REBORROW_REFUSED.with(|c| c.set(None));

        let (update, _) = run(
            Env {
                styled: Some(fx.styled),
                ..Env::default()
            },
            fx.options[0],
            data,
            |r, ci| on_combobox_option_click(r, ci),
        );

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            REBORROW_REFUSED.with(|c| c.get()),
            Some(true),
            "a re-entrant downcast_mut must fail cleanly, not alias or hang"
        );

        SHARED_ALIAS.with(|a| *a.borrow_mut() = None);
    }
}

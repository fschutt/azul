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
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, *},
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
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
/// simplification — see the module-level `TODO2`; the field is ~26px tall).
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
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ComboBox {
    /// Runtime state (`open`/`selected`/`text`) plus the item list and the
    /// optional select callback.
    pub combo_state: ComboBoxStateWrapper,
    /// Greyed text shown in the field when no value has been typed/selected.
    pub placeholder: AzString,
}

#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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

/// The editable text inside the field — takes the remaining horizontal space.
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
    pub fn new(items: StringVec) -> Self {
        Self {
            combo_state: ComboBoxStateWrapper {
                inner: ComboBoxState::default(),
                items,
                on_select: None.into(),
            },
            placeholder: AzString::from_const_str(""),
        }
    }

    /// Creates an empty combobox.
    pub fn create() -> Self {
        Self::new(StringVec::from_const_slice(&[]))
    }

    /// Sets the initially-selected option index.
    #[inline]
    pub fn set_selected(&mut self, selected: usize) {
        self.combo_state.inner.selected = selected;
    }

    /// Builder-style setter for the initially-selected index.
    #[inline]
    pub fn with_selected(mut self, selected: usize) -> Self {
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
    pub fn with_text(mut self, text: AzString) -> Self {
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
    pub fn with_placeholder(mut self, placeholder: AzString) -> Self {
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
    pub fn with_on_select<C: Into<ComboBoxOnSelectCallback>>(
        mut self,
        data: RefAny,
        on_select: C,
    ) -> Self {
        self.set_on_select(data, on_select);
        self
    }

    /// Replaces `self` with a default (empty) combobox and returns the original.
    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the combobox into a [`Dom`] subtree with the `__azul-native-combobox`
    /// class.
    pub fn dom(self) -> Dom {
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

        let text_node = Dom::create_text(field_text)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_TEXT_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_TEXT_STYLE));

        let arrow = Dom::create_icon(AzString::from_const_str("arrow_drop_down"))
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_ARROW_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_ARROW_STYLE));

        // The focusable, editable input field. Clicking it toggles the list
        // (Hover::MouseUp) and focuses it; typing edits the text node
        // (Focus::TextInput / VirtualKeyDown), mirroring text_input.
        let field = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_INPUT_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_INPUT_STYLE))
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
        for option in items.as_ref().iter() {
            option_doms.push(
                Dom::create_text(option.clone())
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_OPTION_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                        COMBOBOX_OPTION_STYLE,
                    ))
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

        let list = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_LIST_CLASS))
            .with_css_props(build_list_style(open))
            .with_children(DomVec::from_vec(option_doms));

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(COMBOBOX_WRAPPER_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(COMBOBOX_WRAPPER_STYLE))
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
    let list = match info.get_next_sibling(field) {
        Some(l) => l,
        None => return Update::DoNothing,
    };

    let now_open = {
        let mut combo = match data.downcast_mut::<ComboBoxStateWrapper>() {
            Some(s) => s,
            None => return Update::DoNothing,
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

/// Field text-input handler — appends the typed character(s) to the editable
/// field text (mirroring text_input). Does NOT re-filter the list (see the
/// module-level type-to-filter `TODO2`).
extern "C" fn on_combobox_text_input(data: RefAny, info: CallbackInfo) -> Update {
    on_combobox_text_input_inner(data, info).unwrap_or(Update::DoNothing)
}

fn on_combobox_text_input_inner(mut data: RefAny, mut info: CallbackInfo) -> Option<Update> {
    let field = info.get_hit_node();
    let text_node = info.get_first_child(field)?;

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

/// Field key-down handler — implements backspace deletion (mirroring text_input).
extern "C" fn on_combobox_key_down(data: RefAny, info: CallbackInfo) -> Update {
    on_combobox_key_down_inner(data, info).unwrap_or(Update::DoNothing)
}

fn on_combobox_key_down_inner(mut data: RefAny, mut info: CallbackInfo) -> Option<Update> {
    let field = info.get_hit_node();
    let text_node = info.get_first_child(field)?;

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

/// Option click handler. The hit node is the clicked option; its index is the
/// number of previous siblings. Its parent is the list; the list's parent is the
/// wrapper, whose first child is the field, whose first child is the text node.
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

    let list = match info.get_parent(option) {
        Some(l) => l,
        None => return Update::DoNothing,
    };
    let wrapper = match info.get_parent(list) {
        Some(w) => w,
        None => return Update::DoNothing,
    };
    let field = match info.get_first_child(wrapper) {
        Some(f) => f,
        None => return Update::DoNothing,
    };
    let text_node = match info.get_first_child(field) {
        Some(t) => t,
        None => return Update::DoNothing,
    };

    let (label, inner, result) = {
        let mut combo = match data.downcast_mut::<ComboBoxStateWrapper>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        let label = match combo.items.as_ref().get(index) {
            Some(l) => l.clone(),
            None => return Update::DoNothing,
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
    let _ = inner;

    // Fill the field with the chosen label and close the list.
    info.change_node_text(text_node, label);
    info.set_css_property(list, CssProperty::const_display(LayoutDisplay::None));

    result
}

impl From<ComboBox> for Dom {
    fn from(c: ComboBox) -> Dom {
        c.dom()
    }
}

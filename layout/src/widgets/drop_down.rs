//! Native drop-down / select widget.
//!
//! Renders a clickable trigger (label + arrow icon) that opens a native
//! menu popup for item selection.  Depends on [`azul_core::menu`] for
//! popup rendering.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, FocusEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    menu::{Menu, MenuItem, MenuPopupPosition, StringMenuItem},
    refany::RefAny,
    window::ContextMenuMouseButton,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorU, ColorOrSystem},
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::CssProperty,
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

// -- Callback type via macro --

/// Callback signature invoked when the user selects a new choice.
///
/// The `usize` argument is the zero-based index of the chosen item.
pub type DropDownOnChoiceChangeCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    DropDownOnChoiceChange,
    OptionDropDownOnChoiceChange,
    DropDownOnChoiceChangeCallback,
    DropDownOnChoiceChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        DropDownOnChoiceChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: DROP_DOWN_ON_CHOICE_CHANGE_INVOKER,
    invoker_ty:     AzDropDownOnChoiceChangeCallbackInvoker,
    thunk_fn:       az_drop_down_on_choice_change_callback_thunk,
    setter_fn:      AzApp_setDropDownOnChoiceChangeCallbackInvoker,
    from_handle_fn: AzDropDownOnChoiceChangeCallback_createFromHostHandle,
    extra_args:     [ choice_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- Colors --

const BORDER_NORMAL: ColorU = ColorU { r: 172, g: 172, b: 172, a: 255 };
const BORDER_HOVER: ColorU = ColorU { r: 126, g: 180, b: 234, a: 255 };
const BORDER_FOCUS: ColorU = ColorU { r: 86, g: 157, b: 229, a: 255 };

const BG_GRADIENT_TOP: ColorU = ColorU { r: 245, g: 245, b: 245, a: 255 };
const BG_GRADIENT_BOTTOM: ColorU = ColorU { r: 235, g: 235, b: 235, a: 255 };
const BG_HOVER_TOP: ColorU = ColorU { r: 234, g: 244, b: 252, a: 255 };
const BG_HOVER_BOTTOM: ColorU = ColorU { r: 218, g: 236, b: 252, a: 255 };
const BG_ACTIVE_TOP: ColorU = ColorU { r: 218, g: 236, b: 252, a: 255 };
const BG_ACTIVE_BOTTOM: ColorU = ColorU { r: 202, g: 226, b: 248, a: 255 };

const NORMAL_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(&[
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(0),
                color: ColorOrSystem::color(BG_GRADIENT_TOP),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(100),
                color: ColorOrSystem::color(BG_GRADIENT_BOTTOM),
            },
        ]),
    })];

const HOVER_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(&[
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(0),
                color: ColorOrSystem::color(BG_HOVER_TOP),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(100),
                color: ColorOrSystem::color(BG_HOVER_BOTTOM),
            },
        ]),
    })];

const ACTIVE_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(&[
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(0),
                color: ColorOrSystem::color(BG_ACTIVE_TOP),
            },
            NormalizedLinearColorStop {
                offset: PercentageValue::const_new(100),
                color: ColorOrSystem::color(BG_ACTIVE_BOTTOM),
            },
        ]),
    })];

// -- Dropdown wrapper styles (the clickable trigger) --

static DROPDOWN_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    // Layout
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineFlex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    // Font
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    // Padding
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(2))),
    // Border
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1))),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1))),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1))),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor { inner: BORDER_NORMAL })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: BORDER_NORMAL })),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: BORDER_NORMAL })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(StyleBorderRightColor { inner: BORDER_NORMAL })),
    // Background
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(NORMAL_BG_ITEMS),
    )),
    // Hover
    CssPropertyWithConditions::on_hover(CssProperty::const_border_top_color(StyleBorderTopColor { inner: BORDER_HOVER })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: BORDER_HOVER })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: BORDER_HOVER })),
    CssPropertyWithConditions::on_hover(CssProperty::const_border_right_color(StyleBorderRightColor { inner: BORDER_HOVER })),
    CssPropertyWithConditions::on_hover(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(HOVER_BG_ITEMS),
    )),
    // Active
    CssPropertyWithConditions::on_active(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(ACTIVE_BG_ITEMS),
    )),
    // Focus
    CssPropertyWithConditions::on_focus(CssProperty::const_border_top_color(StyleBorderTopColor { inner: BORDER_FOCUS })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_bottom_color(StyleBorderBottomColor { inner: BORDER_FOCUS })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_left_color(StyleBorderLeftColor { inner: BORDER_FOCUS })),
    CssPropertyWithConditions::on_focus(CssProperty::const_border_right_color(StyleBorderRightColor { inner: BORDER_FOCUS })),
];

// -- Label text style --

static DROPDOWN_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(LayoutPaddingRight::const_px(8))),
];

// -- Arrow icon style --

static DROPDOWN_ARROW_ICON_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

// ============================================================================
// Widget struct and API
// ============================================================================

/// A drop-down / select widget that displays the currently selected item
/// and opens a native menu popup when focused.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct DropDown {
    /// The list of choices presented in the popup menu.
    pub choices: StringVec,
    /// Zero-based index of the currently selected choice.
    pub selected: usize,
    /// Optional callback invoked when the user picks a different choice.
    pub on_choice_change: OptionDropDownOnChoiceChange,
}

impl Default for DropDown {
    fn default() -> Self {
        Self {
            choices: StringVec::from_const_slice(&[]),
            selected: 0,
            on_choice_change: None.into(),
        }
    }
}

impl DropDown {
    /// Creates a new `DropDown` with the given choices and no callback.
    pub fn new(choices: StringVec) -> Self {
        Self {
            choices,
            selected: 0,
            on_choice_change: None.into(),
        }
    }

    /// Sets the callback invoked when the user selects a different choice.
    pub fn set_on_choice_change<C: Into<DropDownOnChoiceChangeCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_choice_change = Some(DropDownOnChoiceChange {
            callback: callback.into(),
            refany: data,
        }).into();
    }

    /// Builder variant of [`Self::set_on_choice_change`].
    pub fn with_on_choice_change<C: Into<DropDownOnChoiceChangeCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_choice_change(data, callback);
        self
    }

    /// Replaces `self` with the default value and returns the original.
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = DropDown::default();
        core::mem::swap(&mut m, self);
        m
    }

    /// Builds the DOM tree for this drop-down widget.
    pub fn dom(self) -> Dom {
        let selected_text = self.choices
            .as_slice()
            .get(self.selected)
            .cloned()
            .unwrap_or_else(|| AzString::from_const_str(""));

        let refany = RefAny::new(self);

        const DROPDOWN_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-dropdown"))];

        // Wrapper: focusable trigger that opens popup on focus
        let wrapper = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(DROPDOWN_WRAPPER_STYLE))
            .with_ids_and_classes(IdOrClassVec::from_const_slice(DROPDOWN_CLASS))
            .with_tab_index(TabIndex::Auto)
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Focus(FocusEventFilter::FocusReceived),
                    refany: refany.clone(),
                    callback: CoreCallback {
                        cb: on_dropdown_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                }]
                .into(),
            )
            .with_children(DomVec::from_vec(vec![
                // Selected text label wrapped in <p> for proper block formatting
                Dom::create_p()
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(DROPDOWN_LABEL_STYLE))
                    .with_children(DomVec::from_vec(vec![
                        Dom::create_text(selected_text),
                    ])),
                // Arrow icon (resolved via Material Icons)
                Dom::create_icon(AzString::from_const_str("arrow_drop_down"))
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(DROPDOWN_ARROW_ICON_STYLE)),
            ]));

        wrapper
    }
}

// ============================================================================
// Internal callback data types
// ============================================================================

struct ChoiceCallbackData {
    choice_id: usize,
    on_choice_change: OptionDropDownOnChoiceChange,
}

// ============================================================================
// Callbacks
// ============================================================================

extern "C" fn on_dropdown_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let refany = match refany.downcast_ref::<DropDown>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let menu_items: Vec<MenuItem> = refany
        .choices
        .iter()
        .enumerate()
        .map(|(idx, choice)| {
            MenuItem::String(StringMenuItem::create(choice.clone()).with_callback(
                RefAny::new(ChoiceCallbackData {
                    choice_id: idx,
                    on_choice_change: refany.on_choice_change.clone(),
                }),
                on_choice_selected as usize,
            ))
        })
        .collect();

    let menu = Menu {
        items: menu_items.into(),
        position: MenuPopupPosition::BottomOfHitRect,
        context_mouse_btn: ContextMenuMouseButton::Right,
    };

    info.open_menu_for_hit_node(menu);
    Update::DoNothing
}

extern "C" fn on_choice_selected(mut refany: RefAny, info: CallbackInfo) -> Update {
    let mut refany = match refany.downcast_mut::<ChoiceCallbackData>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };

    let choice_id = refany.choice_id;

    match refany.on_choice_change.as_mut() {
        Some(DropDownOnChoiceChange { refany, callback }) => {
            (callback.cb)(refany.clone(), info.clone(), choice_id)
        }
        None => Update::DoNothing,
    }
}

impl From<DropDown> for Dom {
    fn from(b: DropDown) -> Dom {
        b.dom()
    }
}

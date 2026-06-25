//! Radio-group widget — a vertical (or horizontal) group of mutually-exclusive
//! options where exactly one is selected. Combines the sibling-navigation +
//! `selected_index` state of [`crate::widgets::segmented::Segmented`] with the
//! circular filled/empty indicator visual of
//! [`crate::widgets::check_box::CheckBox`].
//!
//! Each option is a row: a circular indicator (an outer ring containing an inner
//! dot whose opacity is `100` when selected, `0` otherwise) followed by a text
//! label. Clicking any row selects it: the internal handler computes the clicked
//! row's index from its position among its siblings, updates `selected_index`,
//! invokes the user's `on_change(index)`, and live-restyles every row's dot via
//! `set_css_property`.
//!
//! Key types: [`RadioGroup`], [`RadioGroupState`], [`RadioGroupOnChange`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutJustifyContent, LayoutAlignItems, LayoutFlexGrow, LayoutWidth, LayoutHeight, LayoutAlignSelf, LayoutMarginRight, LayoutMarginBottom, LayoutMarginLeft},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleOpacity, StyleCursor, StyleUserSelect},
    },
    impl_option_inner, AzString, StringVec,
};

use crate::callbacks::{Callback, CallbackInfo};

static RADIO_GROUP_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-radio-group"))];
static RADIO_GROUP_ROW_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-radio-group-row",
))];
static RADIO_GROUP_CIRCLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-radio-group-circle",
))];
static RADIO_GROUP_DOT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-radio-group-dot",
))];
static RADIO_GROUP_LABEL_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-radio-group-label",
))];

/// Callback function type invoked when the selected option changes.
pub type RadioGroupOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, RadioGroupState) -> Update;
impl_widget_callback!(
    RadioGroupOnChange,
    OptionRadioGroupOnChange,
    RadioGroupOnChangeCallback,
    RadioGroupOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RadioGroupOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RADIO_GROUP_ON_CHANGE_INVOKER,
    invoker_ty:     AzRadioGroupOnChangeCallbackInvoker,
    thunk_fn:       az_radio_group_on_change_callback_thunk,
    setter_fn:      AzApp_setRadioGroupOnChangeCallbackInvoker,
    from_handle_fn: AzRadioGroupOnChangeCallback_createFromHostHandle,
    extra_args:     [ state: RadioGroupState ],
}

/// A group of mutually-exclusive radio options with a selection callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct RadioGroup {
    pub radio_group_state: RadioGroupStateWrapper,
    /// The label of each option, in order.
    pub options: StringVec,
    /// Style for the group container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct RadioGroupStateWrapper {
    /// The current selection.
    pub inner: RadioGroupState,
    /// `true` lays the options out in a horizontal row, `false` (default) stacks
    /// them vertically.
    pub horizontal: bool,
    /// Optional: function to call when the selection changes.
    pub on_change: OptionRadioGroupOnChange,
}

/// State of a [`RadioGroup`]: the index of the currently selected option.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct RadioGroupState {
    /// Zero-based index of the selected option.
    pub selected_index: usize,
}

// ---- dimensions (logical px) ----
const CIRCLE_SIZE: isize = 16;
const CIRCLE_RADIUS: isize = 8;
const CIRCLE_BORDER: isize = 1;
const DOT_SIZE: isize = 8;
const DOT_RADIUS: isize = 4;
/// Gap between stacked rows (vertical) / between side-by-side rows (horizontal).
const ROW_GAP: isize = 6;
/// Gap between the indicator circle and its label.
const LABEL_GAP: isize = 8;

// ---- colours ----
/// Indicator ring colour (#9b9b9b).
const CIRCLE_BORDER_COLOR: ColorU = ColorU {
    r: 155,
    g: 155,
    b: 155,
    a: 255,
};
/// Selected dot fill (#0d6efd, accent blue).
const DOT_COLOR: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};

const DOT_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(DOT_COLOR)];
const DOT_BG: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(DOT_BG_ITEMS);

/// Outer ring of one option's indicator (parameter-independent → const slice).
/// A flex box that centres its inner dot.
static RADIO_GROUP_CIRCLE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_justify_content(
        LayoutJustifyContent::Center,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(CIRCLE_SIZE))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(CIRCLE_SIZE))),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(CIRCLE_BORDER),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(CIRCLE_BORDER),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(CIRCLE_BORDER),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_width(
        LayoutBorderRightWidth::const_px(CIRCLE_BORDER),
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
        inner: CIRCLE_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: CIRCLE_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: CIRCLE_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: CIRCLE_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(CIRCLE_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(CIRCLE_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(CIRCLE_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(CIRCLE_RADIUS),
    )),
];

/// Inner filled dot when the option is SELECTED (opacity 100).
static RADIO_GROUP_DOT_STYLE_SELECTED: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(DOT_SIZE))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(DOT_SIZE))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(DOT_BG)),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(100))),
];

/// Inner filled dot when the option is UNSELECTED (opacity 0 — hidden but laid out).
static RADIO_GROUP_DOT_STYLE_UNSELECTED: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(DOT_SIZE))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(DOT_SIZE))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(DOT_BG)),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(DOT_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(0))),
];

/// Builds the container style. Orientation (row vs column) is the only
/// parameter-dependent property, so the style is built at runtime.
fn build_container_style(horizontal: bool) -> CssPropertyWithConditionsVec {
    let direction = if horizontal {
        LayoutFlexDirection::Row
    } else {
        LayoutFlexDirection::Column
    };
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(direction)),
        CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
    ])
}

/// Builds one option's row style. The orientation decides whether the inter-row
/// gap is applied to the bottom (vertical) or the right (horizontal).
fn build_row_style(horizontal: bool) -> CssPropertyWithConditionsVec {
    let mut v: Vec<CssPropertyWithConditions> = alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    ];
    if horizontal {
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_margin_right(LayoutMarginRight::const_px(ROW_GAP * 2)),
        ));
    } else {
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_margin_bottom(LayoutMarginBottom::const_px(ROW_GAP)),
        ));
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The label-text style: a small left gap from the indicator.
static RADIO_GROUP_LABEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(
        LABEL_GAP,
    ))),
];

impl RadioGroup {
    /// Creates a radio group from the given options, with the first one selected.
    #[must_use] pub fn create(options: StringVec) -> Self {
        Self {
            radio_group_state: RadioGroupStateWrapper {
                inner: RadioGroupState { selected_index: 0 },
                horizontal: false,
                ..Default::default()
            },
            options,
            container_style: build_container_style(false),
        }
    }

    /// Sets the currently selected option index.
    #[inline]
    pub const fn set_selected_index(&mut self, selected_index: usize) {
        self.radio_group_state.inner.selected_index = selected_index;
    }

    /// Builder-style setter for the selected option index.
    #[inline]
    #[must_use] pub const fn with_selected_index(mut self, selected_index: usize) -> Self {
        self.set_selected_index(selected_index);
        self
    }

    /// Lays the options out horizontally (default is vertical).
    #[inline]
    pub fn set_horizontal(&mut self, horizontal: bool) {
        self.radio_group_state.horizontal = horizontal;
        self.container_style = build_container_style(horizontal);
    }

    /// Builder-style setter for the horizontal layout flag.
    #[inline]
    #[must_use] pub fn with_horizontal(mut self, horizontal: bool) -> Self {
        self.set_horizontal(horizontal);
        self
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_change<C: Into<RadioGroupOnChangeCallback>>(
        &mut self,
        data: RefAny,
        on_change: C,
    ) {
        self.radio_group_state.on_change = Some(RadioGroupOnChange {
            callback: on_change.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use] pub fn with_on_change<C: Into<RadioGroupOnChangeCallback>>(
        mut self,
        data: RefAny,
        on_change: C,
    ) -> Self {
        self.set_on_change(data, on_change);
        self
    }

    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let selected = self.radio_group_state.inner.selected_index;
        let horizontal = self.radio_group_state.horizontal;
        let count = self.options.as_ref().len();

        let row_style = build_row_style(horizontal);

        // One shared RefAny across every row's callback (RefAny::clone shares
        // the underlying state — same pattern as segmented/tabs/map).
        let state = RefAny::new(self.radio_group_state);

        let mut children: Vec<Dom> = Vec::with_capacity(count);
        for (i, label) in self.options.as_ref().iter().enumerate() {
            let dot_style = if i == selected {
                CssPropertyWithConditionsVec::from_const_slice(RADIO_GROUP_DOT_STYLE_SELECTED)
            } else {
                CssPropertyWithConditionsVec::from_const_slice(RADIO_GROUP_DOT_STYLE_UNSELECTED)
            };

            let circle = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(RADIO_GROUP_CIRCLE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    RADIO_GROUP_CIRCLE_STYLE,
                ))
                .with_children(
                    vec![Dom::create_div()
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(
                            RADIO_GROUP_DOT_CLASS,
                        ))
                        .with_css_props(dot_style)]
                    .into(),
                );

            let label_node = Dom::create_text(label.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(RADIO_GROUP_LABEL_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    RADIO_GROUP_LABEL_STYLE,
                ));

            children.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(RADIO_GROUP_ROW_CLASS))
                    .with_css_props(row_style.clone())
                    .with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::MouseUp),
                            callback: CoreCallback {
                                cb: on_radio_row_click as usize,
                                ctx: OptionRefAny::None,
                            },
                            refany: state.clone(),
                        }]
                        .into(),
                    )
                    .with_tab_index(TabIndex::Auto)
                    .with_children(vec![circle, label_node].into()),
            );
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(RADIO_GROUP_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for RadioGroup {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

/// Click handler shared by all rows. Determines the clicked row's index from its
/// position among its siblings (the hit node resolves to the row the callback is
/// registered on — currentTarget semantics — regardless of whether the dot,
/// circle or label was clicked), updates the selection, invokes the user
/// callback, and live-restyles every row's indicator dot.
extern "C" fn on_radio_row_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::dom::DomNodeId;

    let clicked = info.get_hit_node();
    let Some(parent) = info.get_parent(clicked) else {
        return Update::DoNothing;
    };

    // Collect the option rows in document order.
    let mut rows: Vec<DomNodeId> = Vec::new();
    let mut cur = info.get_first_child(parent);
    while let Some(node) = cur {
        rows.push(node);
        cur = info.get_next_sibling(node);
    }

    let Some(selected) = rows.iter().position(|n| *n == clicked) else {
        return Update::DoNothing;
    };

    let result = {
        let Some(mut rg) = data.downcast_mut::<RadioGroupStateWrapper>() else {
            return Update::DoNothing;
        };
        rg.inner.selected_index = selected;
        let inner = rg.inner;
        let rg = &mut *rg;
        match rg.on_change.as_mut() {
            Some(RadioGroupOnChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        }
    };

    // Live-restyle every row's dot: the selected option's dot becomes visible
    // (opacity 100), the rest are hidden (opacity 0). Each row is
    // `row → circle (first child) → dot (first child)`.
    for (i, row) in rows.iter().enumerate() {
        let Some(circle) = info.get_first_child(*row) else {
            continue;
        };
        let Some(dot) = info.get_first_child(circle) else {
            continue;
        };
        let opacity = if i == selected { 100 } else { 0 };
        info.set_css_property(dot, CssProperty::const_opacity(StyleOpacity::const_new(opacity)));
    }

    result
}

impl From<RadioGroup> for Dom {
    fn from(r: RadioGroup) -> Self {
        r.dom()
    }
}

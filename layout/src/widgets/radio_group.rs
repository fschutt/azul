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
use azul_css::{OptionString, 
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
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET rather than patched onto the finished `Dom`: that
    /// is what lets the widget know at build time whether it was named, so its
    /// warning fires only when nobody supplied one. Forwarded into the
    /// accessibility declaration the widget builds anyway, beside its role and
    /// state.
    pub accessibility_name: OptionString,
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
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    #[must_use] pub fn create(options: StringVec) -> Self {
        Self {
            radio_group_state: RadioGroupStateWrapper {
                inner: RadioGroupState { selected_index: 0 },
                horizontal: false,
                ..Default::default()
            },
            options,
            container_style: build_container_style(false),
            accessibility_name: OptionString::None,
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
        // Read before the widget's fields are moved into the DOM below.
        let rg_name = self.accessibility_name.clone();
        crate::widgets::warn_widget_needs_a_name(
            "radio_group",
            rg_name.is_some(),
        );

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
        // Read once, BEFORE the state is moved into the shared RefAny below.
        let selected_now = self.radio_group_state.inner.selected_index;
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

            let label_node = crate::widgets::widget_p_with_text(label.clone())
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
                    // Each row is its own radio button and must say whether IT
                    // is the chosen one. A group where every row announces the
                    // same thing is unusable: the user cannot tell which is
                    // selected without seeing the dot.
                    .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                        role: azul_core::a11y::AccessibilityRole::RadioButton,
                        states: azul_core::a11y::AccessibilityStateVec::from_vec(vec![
                            if i == selected_now {
                                azul_core::a11y::AccessibilityState::CheckedTrue
                            } else {
                                azul_core::a11y::AccessibilityState::CheckedFalse
                            },
                        ]),
                        ..Default::default()
                    })
                    .with_children(vec![circle, label_node].into()),
            );
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(RADIO_GROUP_CLASS))
            .with_css_props(self.container_style)
            // The name belongs to the GROUP, not to each row. Every row already
            // has its own option text, which azul derives a name from; stamping
            // the group's name onto all of them would make them announce
            // identically and hide the very thing the user is choosing between.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Grouping,
                accessibility_name: rg_name,
                ..Default::default()
            })
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
// `assertions_on_constants`: these are deliberate invariant guards over sibling
// `const`s in this module. They are const-foldable *today*, which is exactly the
// point — they must go red the moment someone edits one of those constants into an
// inconsistent value. Deleting them (clippy's suggestion) would delete the check.
#[allow(clippy::assertions_on_constants)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        mem::discriminant,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{
        props::basic::{length::SizeMetric, pixel::PixelValue},
        system::SystemStyle,
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    fn labels(v: &[&str]) -> StringVec {
        StringVec::from_vec(v.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    /// `n` distinct labels: `o0, o1, … o{n-1}`.
    fn n_labels(n: usize) -> StringVec {
        StringVec::from_vec(
            (0..n)
                .map(|i| AzString::from(format!("o{i}")))
                .collect::<Vec<_>>(),
        )
    }

    fn group(v: &[&str]) -> RadioGroup {
        RadioGroup::create(labels(v))
    }

    // ------------------------------------------------------------------
    // Style probes
    // ------------------------------------------------------------------

    fn props(style: &[CssPropertyWithConditions]) -> Vec<CssProperty> {
        style.iter().map(|p| p.property.clone()).collect()
    }

    fn has_property(style: &[CssPropertyWithConditions], wanted: &CssProperty) -> bool {
        style.iter().any(|p| p.property == *wanted)
    }

    /// Every style in this file is unconditional — a stray `@media`/`:hover`
    /// condition would make the property silently not apply.
    fn all_unconditional(style: &[CssPropertyWithConditions]) -> bool {
        style.iter().all(|p| p.apply_if.as_ref().is_empty())
    }

    fn no_duplicate_properties(name: &str, style: &[CssPropertyWithConditions]) {
        let mut seen = Vec::new();
        for p in style {
            let d = discriminant(&p.property);
            assert!(
                !seen.contains(&d),
                "{name} declares {:?} twice — the later declaration silently wins",
                p.property,
            );
            seen.push(d);
        }
    }

    /// The opacity declared by a property list, normalized to `0.0..=1.0`.
    /// `StyleOpacity::const_new` takes a *percentage*, so `const_new(1)` would be
    /// 1% — a dot that is technically there but invisible.
    fn opacity_of(properties: &[CssProperty]) -> Option<f32> {
        properties.iter().find_map(|p| match p {
            CssProperty::Opacity(o) => o.get_property().map(|o| o.inner.normalized()),
            _ => None,
        })
    }

    fn flex_direction(properties: &[CssProperty]) -> Option<LayoutFlexDirection> {
        properties.iter().find_map(|p| match p {
            CssProperty::FlexDirection(d) => d.get_property().copied(),
            _ => None,
        })
    }

    fn cursor(properties: &[CssProperty]) -> Option<StyleCursor> {
        properties.iter().find_map(|p| match p {
            CssProperty::Cursor(c) => c.get_property().copied(),
            _ => None,
        })
    }

    fn user_select(properties: &[CssProperty]) -> Option<StyleUserSelect> {
        properties.iter().find_map(|p| match p {
            CssProperty::UserSelect(u) => u.get_property().copied(),
            _ => None,
        })
    }

    fn margin_bottom(properties: &[CssProperty]) -> Option<PixelValue> {
        properties.iter().find_map(|p| match p {
            CssProperty::MarginBottom(m) => m.get_property().map(|m| m.inner),
            _ => None,
        })
    }

    fn margin_right(properties: &[CssProperty]) -> Option<PixelValue> {
        properties.iter().find_map(|p| match p {
            CssProperty::MarginRight(m) => m.get_property().map(|m| m.inner),
            _ => None,
        })
    }

    fn margin_left(properties: &[CssProperty]) -> Option<PixelValue> {
        properties.iter().find_map(|p| match p {
            CssProperty::MarginLeft(m) => m.get_property().map(|m| m.inner),
            _ => None,
        })
    }

    fn width(properties: &[CssProperty]) -> Option<PixelValue> {
        properties.iter().find_map(|p| match p {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(*pv),
                _ => None,
            },
            _ => None,
        })
    }

    fn height(properties: &[CssProperty]) -> Option<PixelValue> {
        properties.iter().find_map(|p| match p {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(*pv),
                _ => None,
            },
            _ => None,
        })
    }

    fn border_top_left_radius(properties: &[CssProperty]) -> Option<PixelValue> {
        properties.iter().find_map(|p| match p {
            CssProperty::BorderTopLeftRadius(r) => r.get_property().map(|r| r.inner),
            _ => None,
        })
    }

    /// Asserts the length is an absolute `px` and returns its magnitude. An `em`
    /// or `%` slipping into this widget's geometry would resolve against the
    /// parent font/box, so a 16px indicator could render at any size at all.
    fn px(pv: PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "radio-group geometry must be absolute px, got {:?}",
            pv.metric,
        );
        pv.number.get()
    }

    // ------------------------------------------------------------------
    // Dom probes
    // ------------------------------------------------------------------

    fn classes(node: &Dom) -> Vec<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_props(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
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

    fn row_of(dom: &Dom, i: usize) -> &Dom {
        &dom.children.as_ref()[i]
    }

    /// `row → circle (first child) → dot (first child)` — the path the click
    /// handler itself walks.
    fn dot_of(dom: &Dom, i: usize) -> &Dom {
        &row_of(dom, i).children.as_ref()[0].children.as_ref()[0]
    }

    fn label_of(dom: &Dom, i: usize) -> &Dom {
        &row_of(dom, i).children.as_ref()[1]
    }

    /// The `RefAny` row `i`'s click callback carries.
    fn row_state(dom: &Dom, i: usize) -> RefAny {
        row_of(dom, i)
            .root
            .get_callbacks()
            .as_ref()
            .first()
            .expect("every option row must carry the click callback")
            .refany
            .clone()
    }

    // ------------------------------------------------------------------
    // Callback harness
    // ------------------------------------------------------------------

    /// Flattened (pre-order) node id of option row `i`: the tree is
    /// `root, [row, circle, dot, label <p>, label text] * n` — the label is a
    /// `<p>` wrapping a bare text node, per the widget label convention.
    fn row_node(i: usize) -> DomNodeId {
        node(1 + 5 * i)
    }

    /// Flattened node id of option `i`'s indicator dot.
    fn dot_node(i: usize) -> NodeId {
        NodeId::new(3 + 5 * i)
    }

    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was
    /// hit" case. `CallbackInfo::set_css_property` *panics* on such an id, so the
    /// handler must bail out long before reaching it.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` with an *empty* layout tree: `on_radio_row_click` only
    /// walks `styled_dom.node_hierarchy`, so no real layout (and no font) is needed.
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

    /// Renders `rg`, then hands back both the flattened DOM *and* the very
    /// `RefAny` the widget registered on row 0's mouse-up callback. Driving the
    /// handler with these two is the real wiring — nothing is re-created by hand,
    /// so a mismatch between what `dom()` stores and what the handler expects
    /// cannot hide behind the fixture. Requires at least one option.
    fn flatten(rg: RadioGroup) -> (StyledDom, RefAny) {
        let dom = rg.dom();
        let state = row_state(&dom, 0);
        (StyledDom::create_from_dom(dom), state)
    }

    /// Invokes `on_radio_row_click` against a `LayoutWindow` holding `styled` (or
    /// nothing at all, when `styled` is `None`), with `hit` as the hit node.
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(
        styled: Option<StyledDom>,
        hit: DomNodeId,
        data: RefAny,
    ) -> (Update, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd));
        }

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
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
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = on_radio_row_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// The opacity overrides pushed onto individual nodes, in push order.
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

    /// What a correct restyle of an `n`-option group with option `selected` looks
    /// like: every dot touched exactly once, only the selected one opaque.
    fn expected_opacities(n: usize, selected: usize) -> Vec<(NodeId, f32)> {
        (0..n)
            .map(|i| (dot_node(i), if i == selected { 1.0 } else { 0.0 }))
            .collect()
    }

    fn selected_index_of(data: &mut RefAny) -> usize {
        data.downcast_ref::<RadioGroupStateWrapper>()
            .expect("payload must still be a RadioGroupStateWrapper")
            .inner
            .selected_index
    }

    /// A `RefAny` payload recording every index a user `on_change` sees.
    struct ChangeLog {
        seen: Vec<usize>,
    }

    extern "C" fn record_change(
        mut data: RefAny,
        _: CallbackInfo,
        state: RadioGroupState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ChangeLog>() {
            log.seen.push(state.selected_index);
        }
        Update::RefreshDom
    }

    extern "C" fn change_do_nothing(_: RefAny, _: CallbackInfo, _: RadioGroupState) -> Update {
        Update::DoNothing
    }

    extern "C" fn change_refresh_all(_: RefAny, _: CallbackInfo, _: RadioGroupState) -> Update {
        Update::RefreshDomAllWindows
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn change_cb(f: RadioGroupOnChangeCallbackType) -> RadioGroupOnChangeCallback {
        f.into()
    }

    fn log_refany() -> RefAny {
        RefAny::new(ChangeLog { seen: Vec::new() })
    }

    fn log_indices(data: &mut RefAny) -> Vec<usize> {
        data.downcast_ref::<ChangeLog>()
            .expect("payload must still be a ChangeLog")
            .seen
            .clone()
    }

    // ==================================================================
    // build_container_style
    // ==================================================================

    #[test]
    fn container_style_switches_only_the_flex_direction() {
        // Orientation is the *only* thing this function is allowed to vary; if it
        // also flipped, say, align-self, a horizontal group would stretch across
        // the parent while a vertical one hugs its content.
        let vertical = build_container_style(false);
        let horizontal = build_container_style(true);

        let v = props(vertical.as_ref());
        let h = props(horizontal.as_ref());
        assert_eq!(
            v.len(),
            h.len(),
            "the two orientations declare a different number of properties",
        );

        let differing: Vec<_> = v
            .iter()
            .zip(h.iter())
            .filter(|(a, b)| a != b)
            .map(|(a, _)| discriminant(a))
            .collect();
        assert_eq!(
            differing,
            vec![discriminant(&CssProperty::const_flex_direction(
                LayoutFlexDirection::Row
            ))],
            "the vertical/horizontal container styles differ in more than the flex direction",
        );

        assert_eq!(
            flex_direction(&v),
            Some(LayoutFlexDirection::Column),
            "a vertical radio group must stack its options",
        );
        assert_eq!(
            flex_direction(&h),
            Some(LayoutFlexDirection::Row),
            "a horizontal radio group must lay its options side by side",
        );
    }

    #[test]
    fn container_style_is_pure_unconditional_and_declares_nothing_twice() {
        for horizontal in [false, true] {
            let a = build_container_style(horizontal);
            let b = build_container_style(horizontal);
            assert_eq!(
                a.as_ref(),
                b.as_ref(),
                "build_container_style({horizontal}) is not a pure function",
            );
            no_duplicate_properties("the container style", a.as_ref());
            assert!(
                all_unconditional(a.as_ref()),
                "the container style must apply unconditionally",
            );
        }
    }

    #[test]
    fn container_style_is_a_non_growing_flex_box_in_both_orientations() {
        // `flex-grow: 0` + `align-self: start` is what keeps the group hugging its
        // options instead of being stretched by the parent flex line.
        for horizontal in [false, true] {
            let style = build_container_style(horizontal);
            assert!(
                has_property(
                    style.as_ref(),
                    &CssProperty::const_display(LayoutDisplay::Flex)
                ),
                "horizontal={horizontal}: the container is not a flex box",
            );
            assert!(
                has_property(
                    style.as_ref(),
                    &CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))
                ),
                "horizontal={horizontal}: the container would be stretched by its parent",
            );
            assert!(
                has_property(
                    style.as_ref(),
                    &CssProperty::align_self(LayoutAlignSelf::Start)
                ),
                "horizontal={horizontal}: the container lost its align-self:start",
            );
        }
    }

    // ==================================================================
    // build_row_style
    // ==================================================================

    #[test]
    fn row_style_puts_the_inter_row_gap_on_the_stacking_axis() {
        // Vertical groups stack downwards -> the gap belongs on the bottom;
        // horizontal groups run rightwards -> it belongs on the right. Putting it
        // on the wrong axis leaves the options touching along the axis they are
        // actually laid out on.
        let vertical = props(build_row_style(false).as_ref());
        assert_eq!(
            margin_bottom(&vertical).map(px),
            Some(ROW_GAP as f32),
            "a vertically stacked row must separate itself from the next one",
        );
        assert_eq!(
            margin_right(&vertical),
            None,
            "a vertically stacked row must not push its neighbours sideways",
        );

        let horizontal = props(build_row_style(true).as_ref());
        assert_eq!(
            margin_right(&horizontal).map(px),
            Some((ROW_GAP * 2) as f32),
            "a horizontal row must separate itself from the next one",
        );
        assert_eq!(
            margin_bottom(&horizontal),
            None,
            "a horizontal row must not add vertical spacing",
        );
    }

    #[test]
    fn row_style_is_always_an_inner_row_regardless_of_the_group_orientation() {
        // The *group* orientation must not leak into the row: a row is always
        // `circle | label` left-to-right, even inside a column group. A naive
        // "pass horizontal through" would render the label under the indicator.
        for horizontal in [false, true] {
            let style = props(build_row_style(horizontal).as_ref());
            assert_eq!(
                flex_direction(&style),
                Some(LayoutFlexDirection::Row),
                "horizontal={horizontal}: the indicator/label pair is not laid out in a row",
            );
            assert!(
                has_property(
                    build_row_style(horizontal).as_ref(),
                    &CssProperty::const_align_items(LayoutAlignItems::Center)
                ),
                "horizontal={horizontal}: the label is not vertically centred on the indicator",
            );
        }
    }

    #[test]
    fn row_style_marks_the_whole_row_as_a_click_target() {
        // The row is what carries the mouse-up handler, so it must *look*
        // clickable and must not start a text selection when dragged.
        for horizontal in [false, true] {
            let style = props(build_row_style(horizontal).as_ref());
            assert_eq!(
                cursor(&style),
                Some(StyleCursor::Pointer),
                "horizontal={horizontal}: the row does not look clickable",
            );
            assert_eq!(
                user_select(&style),
                Some(StyleUserSelect::None),
                "horizontal={horizontal}: dragging a row would select its label text",
            );
        }
    }

    #[test]
    fn row_style_is_pure_unconditional_and_declares_nothing_twice() {
        for horizontal in [false, true] {
            let a = build_row_style(horizontal);
            let b = build_row_style(horizontal);
            assert_eq!(
                a.as_ref(),
                b.as_ref(),
                "build_row_style({horizontal}) is not a pure function",
            );
            no_duplicate_properties("the row style", a.as_ref());
            assert!(
                all_unconditional(a.as_ref()),
                "the row style must apply unconditionally",
            );
        }
    }

    // ==================================================================
    // The const style tables
    // ==================================================================

    #[test]
    fn the_const_style_tables_declare_nothing_twice_and_apply_unconditionally() {
        for (name, style) in [
            ("the circle style", RADIO_GROUP_CIRCLE_STYLE),
            ("the selected dot style", RADIO_GROUP_DOT_STYLE_SELECTED),
            ("the unselected dot style", RADIO_GROUP_DOT_STYLE_UNSELECTED),
            ("the label style", RADIO_GROUP_LABEL_STYLE),
        ] {
            no_duplicate_properties(name, style);
            assert!(all_unconditional(style), "{name} must apply unconditionally");
        }
    }

    #[test]
    fn the_two_dot_styles_differ_in_opacity_and_nothing_else() {
        // Opacity is the *only* thing that may distinguish a selected option from
        // an unselected one: a size or colour difference would reflow (or recolour)
        // the row as the selection moves.
        let selected = props(RADIO_GROUP_DOT_STYLE_SELECTED);
        let unselected = props(RADIO_GROUP_DOT_STYLE_UNSELECTED);
        assert_eq!(
            selected.len(),
            unselected.len(),
            "the two dot styles declare a different number of properties",
        );

        let differing: Vec<_> = selected
            .iter()
            .zip(unselected.iter())
            .filter(|(a, b)| a != b)
            .map(|(a, _)| discriminant(a))
            .collect();
        assert_eq!(
            differing,
            vec![discriminant(&CssProperty::const_opacity(
                StyleOpacity::const_new(0)
            ))],
            "the selected/unselected dot styles differ in something other than opacity",
        );

        assert_eq!(
            opacity_of(&selected),
            Some(1.0),
            "the selected dot is not fully opaque (const_new takes a *percentage*)",
        );
        assert_eq!(
            opacity_of(&unselected),
            Some(0.0),
            "the unselected dot is still visible",
        );
    }

    #[test]
    fn the_indicator_geometry_is_absolute_px_and_actually_circular() {
        // `border-radius = size / 2` on all four corners is what makes the ring and
        // the dot circles rather than rounded squares; and the dot plus the ring's
        // two borders must fit inside the ring.
        assert_eq!(CIRCLE_RADIUS * 2, CIRCLE_SIZE, "the ring is not a circle");
        assert_eq!(DOT_RADIUS * 2, DOT_SIZE, "the dot is not a circle");
        assert!(
            DOT_SIZE + 2 * CIRCLE_BORDER <= CIRCLE_SIZE,
            "the dot ({DOT_SIZE}px) does not fit inside the ring ({CIRCLE_SIZE}px + \
             {CIRCLE_BORDER}px borders)",
        );

        let circle = props(RADIO_GROUP_CIRCLE_STYLE);
        assert_eq!(width(&circle).map(px), Some(CIRCLE_SIZE as f32));
        assert_eq!(height(&circle).map(px), Some(CIRCLE_SIZE as f32));
        assert_eq!(
            border_top_left_radius(&circle).map(px),
            Some(CIRCLE_RADIUS as f32),
        );

        for style in [RADIO_GROUP_DOT_STYLE_SELECTED, RADIO_GROUP_DOT_STYLE_UNSELECTED] {
            let dot = props(style);
            assert_eq!(width(&dot).map(px), Some(DOT_SIZE as f32));
            assert_eq!(height(&dot).map(px), Some(DOT_SIZE as f32));
            assert_eq!(
                border_top_left_radius(&dot).map(px),
                Some(DOT_RADIUS as f32),
            );
        }

        assert_eq!(
            margin_left(&props(RADIO_GROUP_LABEL_STYLE)).map(px),
            Some(LABEL_GAP as f32),
            "the label lost its gap from the indicator",
        );
    }

    // ==================================================================
    // RadioGroup::create
    // ==================================================================

    #[test]
    fn create_preserves_the_options_verbatim_and_defaults_the_state() {
        for case in [
            vec![],
            vec!["only"],
            vec!["a", "b"],
            vec!["dup", "dup", "dup"],
            vec!["Yes", "No", "Maybe", "Ask again later"],
        ] {
            let rg = group(&case);

            let got: Vec<&str> = rg.options.as_ref().iter().map(AzString::as_str).collect();
            assert_eq!(got, case, "create must not reorder/drop/rewrite options");
            assert_eq!(
                rg.radio_group_state.inner.selected_index, 0,
                "a fresh radio group selects its first option",
            );
            assert!(
                !rg.radio_group_state.horizontal,
                "a fresh radio group is vertical",
            );
            assert!(
                rg.radio_group_state.on_change.as_ref().is_none(),
                "create must not invent a callback",
            );
            assert_eq!(
                rg.container_style.as_ref(),
                build_container_style(false).as_ref(),
                "create must build the *vertical* container style",
            );
        }
    }

    #[test]
    fn create_survives_pathological_labels() {
        // empty string, whitespace-only, emoji + ZWJ, RTL, stacked combining marks,
        // an embedded NUL, invisible formatting chars, and a 100k-char label.
        let huge = "x".repeat(100_000);
        let case = vec![
            "",
            "   ",
            "a\u{0}b",
            "👨‍👩‍👧‍👦",
            "مرحبا",
            "e\u{0301}\u{0301}\u{0301}",
            "\u{200b}\u{feff}",
            huge.as_str(),
        ];
        let rg = group(&case);

        let got: Vec<&str> = rg.options.as_ref().iter().map(AzString::as_str).collect();
        assert_eq!(got, case, "options must survive byte-for-byte");
        assert_eq!(rg.options.as_ref()[7].as_str().len(), 100_000);

        // … and they must survive the trip through the DOM unchanged.
        let dom = rg.dom();
        let texts: Vec<&str> = (0..case.len()).filter_map(|i| text_of(label_of(&dom, i))).collect();
        assert_eq!(texts, case, "a label was mangled on its way into the DOM");
    }

    #[test]
    fn create_with_a_huge_option_list_does_not_panic() {
        let n = 10_000;
        let rg = RadioGroup::create(n_labels(n));
        assert_eq!(rg.options.as_ref().len(), n);
        assert_eq!(rg.options.as_ref()[n - 1].as_str(), "o9999");
    }

    #[test]
    fn default_equals_create_with_no_options() {
        assert_eq!(
            RadioGroup::default(),
            RadioGroup::create(StringVec::from_const_slice(&[])),
        );
    }

    // ==================================================================
    // set_selected_index / with_selected_index
    // ==================================================================

    #[test]
    fn selected_index_is_stored_verbatim_at_every_boundary() {
        // The setter is documented as a plain store — no clamping to the option
        // count — so the extremes must round-trip exactly rather than saturate,
        // wrap, or panic in a debug build.
        for idx in [0, 1, 2, 3, 1_000, usize::MAX - 1, usize::MAX] {
            let mut rg = group(&["a", "b", "c"]);
            rg.set_selected_index(idx);
            assert_eq!(
                rg.radio_group_state.inner.selected_index, idx,
                "set_selected_index({idx}) did not store what it was given",
            );

            let built = group(&["a", "b", "c"]).with_selected_index(idx);
            assert_eq!(
                built, rg,
                "with_selected_index({idx}) disagrees with the mutating setter",
            );
        }
    }

    #[test]
    fn setting_the_index_repeatedly_keeps_only_the_last_value() {
        let mut rg = group(&["a", "b"]);
        for idx in [1, 0, usize::MAX, 1, 0] {
            rg.set_selected_index(idx);
        }
        assert_eq!(rg.radio_group_state.inner.selected_index, 0);
    }

    #[test]
    fn with_selected_index_touches_nothing_but_the_index() {
        let before = group(&["a", "b", "c"]).with_horizontal(true);
        let after = before.clone().with_selected_index(2);

        assert_eq!(after.options.as_ref(), before.options.as_ref());
        assert_eq!(after.container_style.as_ref(), before.container_style.as_ref());
        assert_eq!(
            after.radio_group_state.horizontal,
            before.radio_group_state.horizontal,
            "changing the selection must not change the layout direction",
        );
        assert_eq!(after.radio_group_state.inner.selected_index, 2);
    }

    #[test]
    fn an_out_of_range_selection_renders_every_dot_hidden() {
        // Nothing clamps `selected_index`, so `dom()` has to cope with an index no
        // option owns: it must render the full option list with *no* dot lit
        // rather than panicking or highlighting a wrapped-around row.
        for idx in [3, 4, 1_000, usize::MAX - 1, usize::MAX] {
            let dom = group(&["a", "b", "c"]).with_selected_index(idx).dom();
            assert_eq!(
                dom.children.as_ref().len(),
                3,
                "idx={idx}: an out-of-range selection changed the option count",
            );
            for i in 0..3 {
                assert_eq!(
                    opacity_of(&inline_props(dot_of(&dom, i))),
                    Some(0.0),
                    "idx={idx}: option {i} is lit even though nothing is selected",
                );
            }
        }
    }

    #[test]
    fn selecting_an_option_lights_exactly_that_one() {
        for selected in 0..4 {
            let dom = group(&["a", "b", "c", "d"])
                .with_selected_index(selected)
                .dom();
            let lit: Vec<usize> = (0..4)
                .filter(|i| opacity_of(&inline_props(dot_of(&dom, *i))) == Some(1.0))
                .collect();
            assert_eq!(
                lit,
                vec![selected],
                "selecting option {selected} lit {lit:?} instead",
            );
        }
    }

    // ==================================================================
    // set_horizontal / with_horizontal
    // ==================================================================

    #[test]
    fn the_horizontal_flag_and_the_container_style_never_disagree() {
        // Two sources of truth for one fact: the flag drives the *rendered* row
        // style, the style drives the container. If the setter updated only one of
        // them, a group would stack vertically while spacing itself horizontally.
        for horizontal in [false, true] {
            let mut rg = group(&["a", "b"]);
            rg.set_horizontal(horizontal);
            assert_eq!(rg.radio_group_state.horizontal, horizontal);
            assert_eq!(
                rg.container_style.as_ref(),
                build_container_style(horizontal).as_ref(),
                "horizontal={horizontal}: the container style was not rebuilt",
            );

            assert_eq!(
                group(&["a", "b"]).with_horizontal(horizontal),
                rg,
                "with_horizontal({horizontal}) disagrees with the mutating setter",
            );
        }
    }

    #[test]
    fn toggling_the_orientation_never_accumulates_properties() {
        // The style is *rebuilt*, not appended to: flipping the flag a hundred
        // times must leave a four-property vec, not a four-hundred-property one
        // (where every later duplicate silently overrides the earlier).
        let mut rg = group(&["a", "b"]);
        let original = rg.clone();
        let len = rg.container_style.as_ref().len();

        for i in 0..100 {
            rg.set_horizontal(i % 2 == 0);
            assert_eq!(
                rg.container_style.as_ref().len(),
                len,
                "toggle #{i}: the container style grew",
            );
        }

        rg.set_horizontal(false);
        assert_eq!(
            rg, original,
            "an even number of toggles did not return the group to its original state",
        );
    }

    #[test]
    fn the_orientation_reaches_the_rendered_container() {
        for (horizontal, expected) in [
            (false, LayoutFlexDirection::Column),
            (true, LayoutFlexDirection::Row),
        ] {
            let dom = group(&["a", "b"]).with_horizontal(horizontal).dom();
            assert_eq!(
                flex_direction(&inline_props(&dom)),
                Some(expected),
                "horizontal={horizontal}: the rendered container flows the wrong way",
            );
        }
    }

    #[test]
    fn the_orientation_reaches_the_rendered_rows() {
        // `dom()` reads the *flag*, not the container style, to build the row gap —
        // so the flag has to be what `set_horizontal` stored.
        for horizontal in [false, true] {
            let dom = group(&["a", "b"]).with_horizontal(horizontal).dom();
            let row = inline_props(row_of(&dom, 0));
            assert_eq!(
                margin_bottom(&row),
                margin_bottom(&props(build_row_style(horizontal).as_ref())),
            );
            assert_eq!(
                margin_right(&row),
                margin_right(&props(build_row_style(horizontal).as_ref())),
            );
        }
    }

    // ==================================================================
    // swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_hands_back_the_old_value_and_leaves_a_default_behind() {
        let mut rg = group(&["a", "b", "c"])
            .with_selected_index(2)
            .with_horizontal(true);
        let original = rg.clone();

        let taken = rg.swap_with_default();

        assert_eq!(taken, original, "the caller did not get the old value back");
        assert_eq!(
            rg,
            RadioGroup::default(),
            "the widget was not reset to its default",
        );
        assert!(rg.options.as_ref().is_empty());
        assert_eq!(rg.radio_group_state.inner.selected_index, 0);
        assert!(!rg.radio_group_state.horizontal);
    }

    #[test]
    fn swapping_twice_restores_the_original() {
        let mut rg = group(&["a", "b"]).with_selected_index(1);
        let original = rg.clone();

        let mut taken = rg.swap_with_default();
        let back = taken.swap_with_default();

        assert_eq!(back, original, "swap is not its own inverse");
        assert_eq!(taken, RadioGroup::default());
    }

    #[test]
    fn swapping_a_default_group_is_a_no_op() {
        let mut rg = RadioGroup::default();
        let taken = rg.swap_with_default();
        assert_eq!(taken, RadioGroup::default());
        assert_eq!(rg, RadioGroup::default());
    }

    #[test]
    fn swap_with_default_drops_the_installed_callback_from_the_widget() {
        // The callback belongs to the value that was taken, not to the husk left
        // behind — otherwise the "default" group would still fire the old handler.
        let mut rg = group(&["a"]).with_on_change(RefAny::new(0u8), change_cb(change_do_nothing));
        let taken = rg.swap_with_default();

        assert!(taken.radio_group_state.on_change.as_ref().is_some());
        assert!(
            rg.radio_group_state.on_change.as_ref().is_none(),
            "the emptied widget kept the old on_change callback",
        );
    }

    // ==================================================================
    // set_on_change / with_on_change
    // ==================================================================

    #[test]
    fn with_on_change_installs_the_callback_and_keeps_the_rest_of_the_state() {
        let before = group(&["a", "b", "c"])
            .with_selected_index(2)
            .with_horizontal(true);
        // One shared payload: `RefAny` equality is instance identity, so the
        // builder/setter comparison below only means something with the same one.
        let payload = RefAny::new(7u32);
        let after = before
            .clone()
            .with_on_change(payload.clone(), change_cb(record_change));

        assert!(after.radio_group_state.on_change.as_ref().is_some());
        assert_eq!(after.options.as_ref(), before.options.as_ref());
        assert_eq!(after.container_style.as_ref(), before.container_style.as_ref());
        assert_eq!(after.radio_group_state.inner, before.radio_group_state.inner);
        assert_eq!(
            after.radio_group_state.horizontal,
            before.radio_group_state.horizontal,
        );

        // … and it matches the mutating setter.
        let mut mutated = before;
        mutated.set_on_change(payload, change_cb(record_change));
        assert_eq!(mutated, after);
    }

    #[test]
    fn setting_on_change_twice_replaces_it_rather_than_stacking() {
        let mut rg = group(&["a"]);
        rg.set_on_change(RefAny::new(1u8), change_cb(record_change));
        rg.set_on_change(RefAny::new(2u8), change_cb(change_refresh_all));

        let installed = rg
            .radio_group_state
            .on_change
            .as_ref()
            .expect("a callback must still be installed");
        assert_eq!(
            installed.callback,
            change_cb(change_refresh_all),
            "the first callback survived the second install",
        );
        let mut payload = installed.refany.clone();
        assert_eq!(
            *payload.downcast_ref::<u8>().expect("the payload changed type"),
            2,
            "the first payload survived the second install",
        );
    }

    #[test]
    fn installing_a_callback_never_invokes_it() {
        // Building a widget is not a user interaction: nothing may fire until a
        // click actually happens.
        let mut probe = log_refany();
        let rg = group(&["a", "b"]).with_on_change(probe.clone(), change_cb(record_change));
        let dom = rg.dom();
        let _ = StyledDom::create_from_dom(dom);

        assert!(
            log_indices(&mut probe).is_empty(),
            "the on_change callback fired during construction",
        );
    }

    // ==================================================================
    // RadioGroup::dom
    // ==================================================================

    #[test]
    fn dom_renders_one_row_per_option_with_the_documented_structure() {
        let dom = group(&["a", "b", "c"]).dom();

        assert_eq!(classes(&dom), vec!["__azul-native-radio-group"]);
        assert_eq!(dom.children.as_ref().len(), 3, "one row per option");

        for i in 0..3 {
            let row = row_of(&dom, i);
            assert_eq!(classes(row), vec!["__azul-native-radio-group-row"]);
            assert_eq!(
                row.children.as_ref().len(),
                2,
                "row {i} must be `circle, label`",
            );
            assert_eq!(
                row.root.get_tab_index(),
                Some(TabIndex::Auto),
                "row {i} is not keyboard reachable",
            );

            let circle = &row.children.as_ref()[0];
            assert_eq!(classes(circle), vec!["__azul-native-radio-group-circle"]);
            assert_eq!(circle.children.as_ref().len(), 1, "the circle holds the dot");
            assert_eq!(
                classes(dot_of(&dom, i)),
                vec!["__azul-native-radio-group-dot"],
            );
            assert_eq!(
                classes(label_of(&dom, i)),
                vec!["__azul-native-radio-group-label"],
            );
            assert_eq!(text_of(label_of(&dom, i)), Some(["a", "b", "c"][i]));
        }
    }

    #[test]
    fn every_row_carries_exactly_one_mouse_up_handler_pointing_at_the_row_handler() {
        let dom = group(&["a", "b", "c"]).dom();

        for i in 0..3 {
            let cbs = row_of(&dom, i).root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 1, "row {i} must have one callback");
            let cb = &cbs.as_ref()[0];
            assert_eq!(
                cb.event,
                EventFilter::Hover(HoverEventFilter::MouseUp),
                "row {i} listens for the wrong event",
            );
            assert_eq!(
                cb.callback.cb,
                on_radio_row_click as usize,
                "row {i} is wired to the wrong handler",
            );
        }

        // The inner nodes must stay inert: a handler on the dot or the label would
        // resolve its index against the *wrong* sibling set.
        for i in 0..3 {
            assert!(row_of(&dom, i).children.as_ref()[0]
                .root
                .get_callbacks()
                .as_ref()
                .is_empty());
            assert!(dot_of(&dom, i).root.get_callbacks().as_ref().is_empty());
            assert!(label_of(&dom, i).root.get_callbacks().as_ref().is_empty());
        }
    }

    #[test]
    fn all_rows_share_one_state_refany() {
        // Mutual exclusion depends on it: if each row owned its own copy of the
        // state, clicking row 2 would leave row 0 still believing it is selected.
        let dom = group(&["a", "b", "c", "d"]).dom();
        let first = row_state(&dom, 0);
        for i in 1..4 {
            assert_eq!(
                row_state(&dom, i).get_data_ptr(),
                first.get_data_ptr(),
                "row {i} carries its own state instead of the shared one",
            );
        }
    }

    #[test]
    fn dom_of_an_empty_group_is_an_empty_container() {
        let dom = RadioGroup::default().dom();
        assert!(
            dom.children.as_ref().is_empty(),
            "a group with no options invented a row",
        );
        assert!(dom.root.get_callbacks().as_ref().is_empty());
        assert_eq!(classes(&dom), vec!["__azul-native-radio-group"]);
    }

    #[test]
    fn dom_of_an_empty_group_with_a_selection_does_not_panic() {
        // `create` sets index 0 even with zero options, so the "selected option" is
        // out of range from the start — the render path must not index into it.
        for idx in [0, 1, usize::MAX] {
            let dom = RadioGroup::default().with_selected_index(idx).dom();
            assert!(dom.children.as_ref().is_empty());
        }
    }

    #[test]
    fn dom_flattens_to_five_nodes_per_option() {
        // root + (row, circle, dot, label <p>, label text) per option. The click
        // handler's live restyle walks exactly this shape, and the callback tests
        // below address nodes by this formula.
        for n in [0, 1, 2, 7] {
            let styled = StyledDom::create_from_dom(RadioGroup::create(n_labels(n)).dom());
            assert_eq!(
                styled.node_hierarchy.as_ref().len(),
                1 + 5 * n,
                "an {n}-option group flattened to an unexpected node count",
            );
        }
    }

    #[test]
    fn a_large_group_renders_without_panicking() {
        let n = 500;
        let dom = RadioGroup::create(n_labels(n))
            .with_selected_index(n - 1)
            .dom();
        assert_eq!(dom.children.as_ref().len(), n);
        assert_eq!(text_of(label_of(&dom, n - 1)), Some("o499"));
        assert_eq!(opacity_of(&inline_props(dot_of(&dom, n - 1))), Some(1.0));
        assert_eq!(opacity_of(&inline_props(dot_of(&dom, 0))), Some(0.0));
    }

    // ==================================================================
    // on_radio_row_click
    // ==================================================================

    #[test]
    fn clicking_a_row_selects_it_and_restyles_every_dot() {
        for clicked in 0..4 {
            let (styled, state) = flatten(group(&["a", "b", "c", "d"]));
            let mut state_probe = state.clone();

            let (update, changes) = run_click(Some(styled), row_node(clicked), state);

            assert_eq!(
                update,
                Update::DoNothing,
                "with no on_change installed the handler reports nothing to redraw",
            );
            assert_eq!(
                selected_index_of(&mut state_probe),
                clicked,
                "clicking row {clicked} selected the wrong option",
            );
            assert_eq!(
                pushed_opacities(&changes),
                expected_opacities(4, clicked),
                "clicking row {clicked} did not light exactly that row's dot",
            );
        }
    }

    #[test]
    fn clicking_the_already_selected_row_is_idempotent() {
        let (styled, state) = flatten(group(&["a", "b", "c"]).with_selected_index(1));
        let mut probe = state.clone();

        let (_, changes) = run_click(Some(styled), row_node(1), state);

        assert_eq!(selected_index_of(&mut probe), 1);
        assert_eq!(
            pushed_opacities(&changes),
            expected_opacities(3, 1),
            "a redundant click must still leave every dot in a consistent state",
        );
    }

    #[test]
    fn clicking_repairs_an_out_of_range_selection() {
        // The widget can be handed an index no option owns; the first click must
        // bring it back into range instead of leaving a group with nothing lit.
        let (styled, state) = flatten(group(&["a", "b", "c"]).with_selected_index(usize::MAX));
        let mut probe = state.clone();

        let (_, changes) = run_click(Some(styled), row_node(2), state);

        assert_eq!(selected_index_of(&mut probe), 2);
        assert_eq!(pushed_opacities(&changes), expected_opacities(3, 2));
    }

    #[test]
    fn clicking_a_single_option_group_selects_option_zero() {
        let (styled, state) = flatten(group(&["only"]));
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), row_node(0), state);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(selected_index_of(&mut probe), 0);
        assert_eq!(pushed_opacities(&changes), vec![(dot_node(0), 1.0)]);
    }

    #[test]
    fn the_reported_index_always_addresses_a_real_option() {
        let n = 32;
        for clicked in [0, 1, n / 2, n - 2, n - 1] {
            let (styled, state) = flatten(RadioGroup::create(n_labels(n)));
            let mut probe = state.clone();

            let (_, changes) = run_click(Some(styled), row_node(clicked), state);

            let idx = selected_index_of(&mut probe);
            assert!(idx < n, "row {clicked} reported out-of-range index {idx}");
            assert_eq!(idx, clicked);

            let pushed = pushed_opacities(&changes);
            assert_eq!(pushed.len(), n, "every dot must be restyled exactly once");
            assert_eq!(
                pushed.iter().filter(|(_, o)| *o == 1.0).count(),
                1,
                "exactly one option may be lit at a time",
            );
        }
    }

    #[test]
    fn the_user_callback_sees_the_new_index_and_its_update_is_forwarded() {
        // Order matters: the selection is written *before* the user callback runs,
        // so the callback observes the state the user just asked for.
        let mut probe = log_refany();
        let rg = group(&["a", "b", "c"]).with_on_change(probe.clone(), change_cb(record_change));
        let (styled, state) = flatten(rg);

        let (update, changes) = run_click(Some(styled), row_node(2), state.clone());

        assert_eq!(log_indices(&mut probe), vec![2], "the callback ran once with the new index");
        assert_eq!(update, Update::RefreshDom, "the user's Update was swallowed");
        // … and the restyle still happens *after* the user callback returns.
        assert_eq!(pushed_opacities(&changes), expected_opacities(3, 2));

        // A second click updates the shared state again — the index is not sticky,
        // and the user hears about every click, not just the first.
        let (styled2, _) = flatten(group(&["a", "b", "c"]));
        let (_, _) = run_click(Some(styled2), row_node(0), state.clone());
        assert_eq!(log_indices(&mut probe), vec![2, 0]);
        let mut state = state;
        assert_eq!(
            selected_index_of(&mut state),
            0,
            "the state must hold the *last* clicked index",
        );
    }

    #[test]
    fn a_callback_that_declines_the_update_still_gets_the_dots_restyled() {
        // A user callback returning DoNothing must not suppress the widget's own
        // visual bookkeeping — otherwise the state says "option 1" while option 0
        // stays lit.
        let rg = group(&["a", "b"]).with_on_change(RefAny::new(0u8), change_cb(change_do_nothing));
        let (styled, state) = flatten(rg);
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), row_node(1), state);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(selected_index_of(&mut probe), 1);
        assert_eq!(
            pushed_opacities(&changes),
            expected_opacities(2, 1),
            "a DoNothing user callback suppressed the dot restyle",
        );
    }

    #[test]
    fn every_update_variant_is_propagated_unchanged() {
        for (cb, expected) in [
            (change_cb(change_do_nothing), Update::DoNothing),
            (change_cb(change_refresh_all), Update::RefreshDomAllWindows),
            (change_cb(record_change), Update::RefreshDom),
        ] {
            let rg = group(&["a", "b"]).with_on_change(log_refany(), cb);
            let (styled, state) = flatten(rg);
            let (update, _) = run_click(Some(styled), row_node(1), state);
            assert_eq!(update, expected);
        }
    }

    #[test]
    fn clicking_the_root_container_does_nothing() {
        // The root has no parent -> the handler must bail before indexing into
        // nothing.
        let (styled, state) = flatten(group(&["a", "b"]));
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), node(0), state);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a parentless hit pushed a DOM change");
        assert_eq!(selected_index_of(&mut probe), 0, "the state must be untouched");
    }

    #[test]
    fn clicking_a_stale_or_absent_node_does_nothing() {
        // Stale hit ids reach callbacks after a DOM mutation, and
        // `set_css_property` *panics* on a None node id — so the handler has to
        // bail out well before the restyle loop.
        for hit in [node(9999), node(usize::MAX - 1), node_none()] {
            let (styled, state) = flatten(group(&["a", "b"]).with_selected_index(1));
            let mut probe = state.clone();

            let (update, changes) = run_click(Some(styled), hit, state);

            assert_eq!(update, Update::DoNothing, "{hit:?}: a stale hit was acted on");
            assert!(changes.is_empty(), "{hit:?}: a stale hit pushed a DOM change");
            assert_eq!(
                selected_index_of(&mut probe),
                1,
                "{hit:?}: a stale hit moved the selection",
            );
        }
    }

    #[test]
    fn clicking_with_no_layout_result_does_nothing() {
        let dom = group(&["a", "b"]).dom();
        let state = row_state(&dom, 0);

        let (update, changes) = run_click(None, row_node(0), state);

        assert_eq!(
            update,
            Update::DoNothing,
            "an empty LayoutWindow must be handled, not unwrapped",
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn clicking_with_a_foreign_payload_does_nothing_and_leaves_it_intact() {
        // The handler downcasts blind; a foreign RefAny must bail out, not
        // reinterpret the bytes as a RadioGroupStateWrapper.
        let (styled, _) = flatten(group(&["a", "b"]));
        let foreign = RefAny::new(0xDEAD_BEEF_u32);

        let (update, changes) = run_click(Some(styled), row_node(1), foreign.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "the handler restyled the DOM through a RefAny it could not read",
        );
        let mut foreign = foreign;
        assert_eq!(
            *foreign
                .downcast_ref::<u32>()
                .expect("the foreign payload was reinterpreted"),
            0xDEAD_BEEF,
            "the handler corrupted a RefAny it did not understand",
        );
    }

    #[test]
    fn clicking_while_the_state_is_already_borrowed_does_nothing() {
        let (styled, state) = flatten(group(&["a", "b"]));

        // A live mutable borrow on a sibling clone: `downcast_mut` inside the
        // handler must fail (returning DoNothing) instead of aliasing `&mut`.
        let mut held = state.clone();
        let guard = held
            .downcast_mut::<RadioGroupStateWrapper>()
            .expect("first borrow succeeds");

        let (update, changes) = run_click(Some(styled), row_node(1), state);

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "the handler restyled the DOM after failing to update the state",
        );
        drop(guard);
    }

    #[test]
    fn a_hit_inside_a_row_resolves_against_its_own_siblings() {
        // The handler documents `currentTarget` semantics: the hit node is the row
        // the callback is registered on, and only rows carry callbacks. Should an
        // inner node ever reach it anyway, it must stay memory-safe and push no
        // half-finished restyle — the sibling walk simply finds no dots to update.
        // (`dot` is its circle's only child -> position 0; the label `<p>` is its
        // row's second child -> position 1, regardless of which row it belongs to.)
        for (hit, expected) in [(node(3), 0usize), (node(13), 0), (node(4), 1), (node(14), 1)] {
            let (styled, state) = flatten(group(&["a", "b", "c"]));
            let mut probe = state.clone();

            let (update, changes) = run_click(Some(styled), hit, state);

            assert_eq!(update, Update::DoNothing);
            assert_eq!(selected_index_of(&mut probe), expected, "{hit:?}");
            assert!(
                changes.is_empty(),
                "{hit:?}: an inner-node hit pushed a partial restyle",
            );
        }
    }

    #[test]
    fn many_clicks_keep_the_state_and_the_pushed_opacities_in_agreement() {
        // A drift between the stored index and the pushed opacity is exactly the
        // class of bug that makes a radio group render a selection it does not
        // hold. 60 clicks cycling through a 5-option group.
        let (_, state) = flatten(group(&["a", "b", "c", "d", "e"]));

        for click in 0..60usize {
            let expected = click % 5;
            let (styled, _) = flatten(group(&["a", "b", "c", "d", "e"]));
            let (_, changes) = run_click(Some(styled), row_node(expected), state.clone());

            let mut probe = state.clone();
            assert_eq!(
                selected_index_of(&mut probe),
                expected,
                "click #{click}: the stored index drifted",
            );
            assert_eq!(
                pushed_opacities(&changes),
                expected_opacities(5, expected),
                "click #{click}: the pushed opacities disagree with the stored index",
            );
        }
    }
}

//! Segmented control / button-group widget — a joined row of mutually-exclusive
//! buttons where exactly one is selected. A blend of the `tabs::TabHeader` row of
//! clickable labels and `button.rs`'s styling, with the stateful 3-type split
//! (state / state-wrapper / widget) of the other interactive widgets.
//!
//! Clicking a segment selects it: the internal handler computes the clicked
//! segment's index from its position among its siblings, updates the
//! `selected_index`, invokes the user's `on_change(index)`, and live-restyles
//! every segment (selected vs unselected) via `set_css_property`.
//!
//! Key types: [`Segmented`], [`SegmentedState`], [`SegmentedOnChange`].

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
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutJustifyContent, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderRightColor, StyleCursor, StyleTextAlign, StyleUserSelect, StyleTextColor, LayoutBorderLeftWidth, StyleBorderLeftStyle, StyleBorderLeftColor, StyleBorderTopLeftRadius, StyleBorderBottomLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomRightRadius},
    },
    impl_option_inner, AzString, StringVec,
};

use crate::callbacks::{Callback, CallbackInfo};

static SEGMENTED_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-segmented"))];
static SEGMENT_ITEM_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-segmented-item"))];

/// Callback function type invoked when the selected segment changes.
pub type SegmentedOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, SegmentedState) -> Update;
impl_widget_callback!(
    SegmentedOnChange,
    OptionSegmentedOnChange,
    SegmentedOnChangeCallback,
    SegmentedOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        SegmentedOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: SEGMENTED_ON_CHANGE_INVOKER,
    invoker_ty:     AzSegmentedOnChangeCallbackInvoker,
    thunk_fn:       az_segmented_on_change_callback_thunk,
    setter_fn:      AzApp_setSegmentedOnChangeCallbackInvoker,
    from_handle_fn: AzSegmentedOnChangeCallback_createFromHostHandle,
    extra_args:     [ state: SegmentedState ],
}

/// A joined row of mutually-exclusive segments with a selection callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Segmented {
    pub segmented_state: SegmentedStateWrapper,
    /// The label of each segment, in order.
    pub labels: StringVec,
    /// Style for the row container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct SegmentedStateWrapper {
    /// The current selection.
    pub inner: SegmentedState,
    /// Optional: function to call when the selection changes.
    pub on_change: OptionSegmentedOnChange,
}

/// State of a [`Segmented`]: the index of the currently selected segment.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct SegmentedState {
    /// Zero-based index of the selected segment.
    pub selected_index: usize,
}

// ---- colours ----
/// Segment border colour (#ced4da).
const SEG_BORDER_COLOR: ColorU = ColorU {
    r: 206,
    g: 212,
    b: 218,
    a: 255,
};
/// Selected-segment background (#0d6efd, accent blue).
const SEG_SELECTED_BG_COLOR: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};
/// Unselected-segment background (white).
const SEG_UNSELECTED_BG_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Selected-segment text colour (white).
const SEG_SELECTED_TEXT: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Unselected-segment text colour (#212529, dark).
const SEG_UNSELECTED_TEXT: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};

const SEG_SELECTED_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(SEG_SELECTED_BG_COLOR)];
const SEG_SELECTED_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(SEG_SELECTED_BG_ITEMS);
const SEG_UNSELECTED_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(SEG_UNSELECTED_BG_COLOR)];
const SEG_UNSELECTED_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(SEG_UNSELECTED_BG_ITEMS);

const SEG_RADIUS: isize = 6;

/// Row container: a horizontal flex row that hugs its content.
static SEGMENTED_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// Builds the style for one segment. The selected/unselected colours and the
/// rounding of the outer corners (only the first segment is rounded on the left,
/// only the last on the right) are the position-dependent properties, so the
/// style is built at runtime.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn build_segment_style(selected: bool, is_first: bool, is_last: bool) -> CssPropertyWithConditionsVec {
    let (bg, text) = if selected {
        (SEG_SELECTED_BG, SEG_SELECTED_TEXT)
    } else {
        (SEG_UNSELECTED_BG, SEG_UNSELECTED_TEXT)
    };

    let mut v: Vec<CssPropertyWithConditions> = alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_justify_content(
            LayoutJustifyContent::Center,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(
            0,
        ))),
        // padding: 6px 12px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
            6,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(6),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(12),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(12),
        )),
        // top/bottom/right borders (the left border is added only for the first segment,
        // so adjacent segments share a single 1px separator)
        CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
            LayoutBorderTopWidth::const_px(1),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
            LayoutBorderBottomWidth::const_px(1),
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
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
            inner: SEG_BORDER_COLOR,
        })),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor {
                inner: SEG_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor {
                inner: SEG_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(13))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg)),
        CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
            inner: text,
        })),
    ];

    if is_first {
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1)),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_left_style(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_left_color(StyleBorderLeftColor {
                inner: SEG_BORDER_COLOR,
            }),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(SEG_RADIUS)),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(
                SEG_RADIUS,
            )),
        ));
    }
    if is_last {
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(
                SEG_RADIUS,
            )),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(
                SEG_RADIUS,
            )),
        ));
    }

    CssPropertyWithConditionsVec::from_vec(v)
}

impl Segmented {
    /// Creates a segmented control from the given labels, with the first segment selected.
    #[must_use] pub fn create(labels: StringVec) -> Self {
        Self {
            segmented_state: SegmentedStateWrapper {
                inner: SegmentedState { selected_index: 0 },
                ..Default::default()
            },
            labels,
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                SEGMENTED_CONTAINER_STYLE,
            ),
        }
    }

    /// Sets the currently selected segment index.
    #[inline]
    pub const fn set_selected_index(&mut self, selected_index: usize) {
        self.segmented_state.inner.selected_index = selected_index;
    }

    /// Builder-style setter for the selected segment index.
    #[inline]
    #[must_use] pub const fn with_selected_index(mut self, selected_index: usize) -> Self {
        self.set_selected_index(selected_index);
        self
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_change<C: Into<SegmentedOnChangeCallback>>(
        &mut self,
        data: RefAny,
        on_change: C,
    ) {
        self.segmented_state.on_change = Some(SegmentedOnChange {
            callback: on_change.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use] pub fn with_on_change<C: Into<SegmentedOnChangeCallback>>(
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

        let selected = self.segmented_state.inner.selected_index;
        let count = self.labels.as_ref().len();

        // One shared RefAny across every segment's callback (RefAny::clone shares
        // the underlying state — same pattern as tabs/map).
        let state = RefAny::new(self.segmented_state);

        let mut children: Vec<Dom> = Vec::with_capacity(count);
        for (i, label) in self.labels.as_ref().iter().enumerate() {
            let is_first = i == 0;
            let is_last = i + 1 == count;
            let seg_style = build_segment_style(i == selected, is_first, is_last);

            children.push(
                Dom::create_text(label.clone())
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(SEGMENT_ITEM_CLASS))
                    .with_css_props(seg_style)
                    .with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::MouseUp),
                            callback: CoreCallback {
                                cb: on_segment_click as usize,
                                ctx: OptionRefAny::None,
                            },
                            refany: state.clone(),
                        }]
                        .into(),
                    )
                    .with_tab_index(TabIndex::Auto),
            );
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(SEGMENTED_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for Segmented {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

/// Click handler shared by all segments. Determines the clicked segment's index
/// from its position among its siblings, updates the selection, invokes the user
/// callback, and live-restyles every segment.
extern "C" fn on_segment_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::dom::DomNodeId;

    let clicked = info.get_hit_node();
    let Some(parent) = info.get_parent(clicked) else {
        return Update::DoNothing;
    };

    // Collect the segment siblings in document order.
    let mut segments: Vec<DomNodeId> = Vec::new();
    let mut cur = info.get_first_child(parent);
    while let Some(node) = cur {
        segments.push(node);
        cur = info.get_next_sibling(node);
    }

    let Some(selected) = segments.iter().position(|n| *n == clicked) else {
        return Update::DoNothing;
    };

    let result = {
        let Some(mut seg) = data.downcast_mut::<SegmentedStateWrapper>() else {
            return Update::DoNothing;
        };
        seg.inner.selected_index = selected;
        let inner = seg.inner;
        let seg = &mut *seg;
        match seg.on_change.as_mut() {
            Some(SegmentedOnChange { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
            None => Update::DoNothing,
        }
    };

    // Live-restyle: selected segment gets the accent fill + light text,
    // the rest get the neutral fill + dark text.
    for (i, node) in segments.iter().enumerate() {
        if i == selected {
            info.set_css_property(*node, CssProperty::const_background_content(SEG_SELECTED_BG));
            info.set_css_property(
                *node,
                CssProperty::const_text_color(StyleTextColor {
                    inner: SEG_SELECTED_TEXT,
                }),
            );
        } else {
            info.set_css_property(
                *node,
                CssProperty::const_background_content(SEG_UNSELECTED_BG),
            );
            info.set_css_property(
                *node,
                CssProperty::const_text_color(StyleTextColor {
                    inner: SEG_UNSELECTED_TEXT,
                }),
            );
        }
    }

    result
}

impl From<Segmented> for Dom {
    fn from(s: Segmented) -> Self {
        s.dom()
    }
}

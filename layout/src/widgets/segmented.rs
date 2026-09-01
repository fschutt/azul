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
    impl_option_inner,
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{
            LayoutAlignItems, LayoutAlignSelf, LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow,
            LayoutJustifyContent, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight,
            LayoutPaddingTop,
        },
        property::{CssProperty, *},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderTopStyle, StyleCursor,
            StyleTextAlign, StyleTextColor, StyleUserSelect,
        },
    },
    AzString, StringVec,
};

use crate::callbacks::{Callback, CallbackInfo};

static SEGMENTED_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-segmented"))];
static SEGMENT_ITEM_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-segmented-item",
))];

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
fn build_segment_style(
    selected: bool,
    is_first: bool,
    is_last: bool,
) -> CssPropertyWithConditionsVec {
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
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(6,)
        )),
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
        CssPropertyWithConditions::simple(CssProperty::const_border_top_style(
            StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }
        )),
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
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(
            StyleBorderTopColor {
                inner: SEG_BORDER_COLOR,
            }
        )),
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
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13
        ))),
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
            CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(
                SEG_RADIUS,
            )),
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
    #[must_use]
    pub fn create(labels: StringVec) -> Self {
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
    #[must_use]
    pub const fn with_selected_index(mut self, selected_index: usize) -> Self {
        self.set_selected_index(selected_index);
        self
    }

    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
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
    #[must_use]
    pub fn with_on_change<C: Into<SegmentedOnChangeCallback>>(
        mut self,
        data: RefAny,
        on_change: C,
    ) -> Self {
        self.set_on_change(data, on_change);
        self
    }

    #[must_use]
    pub fn dom(self) -> Dom {
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
                crate::widgets::widget_p_with_text(label.clone())
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(SEGMENT_ITEM_CLASS))
                    .with_css_props(seg_style)
                    .with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::Click),
                            callback: CoreCallback {
                                cb: on_segment_click as usize,
                                ctx: OptionRefAny::None,
                            },
                            refany: state.clone(),
                        }]
                        .into(),
                    )
                    .with_tab_index(TabIndex::Auto)
            // Role so the accessibility tree knows what this IS:
            // a row of mutually exclusive choices. The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::PageTabList,
                ..Default::default()
            }),
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
            Some(SegmentedOnChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        }
    };

    // Live-restyle: selected segment gets the accent fill + light text,
    // the rest get the neutral fill + dark text.
    for (i, node) in segments.iter().enumerate() {
        if i == selected {
            info.set_css_property(
                *node,
                CssProperty::const_background_content(SEG_SELECTED_BG),
            );
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

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap, HashSet},
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
    // Helpers
    // ------------------------------------------------------------------

    fn labels(v: &[&str]) -> StringVec {
        StringVec::from_vec(v.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    /// `n` distinct labels: `s0, s1, … s{n-1}`.
    fn n_labels(n: usize) -> StringVec {
        StringVec::from_vec(
            (0..n)
                .map(|i| AzString::from(format!("s{i}")))
                .collect::<Vec<_>>(),
        )
    }

    /// The eight possible `(selected, is_first, is_last)` argument triples —
    /// the complete input domain of `build_segment_style`.
    const ALL_FLAGS: [(bool, bool, bool); 8] = [
        (false, false, false),
        (false, false, true),
        (false, true, false),
        (false, true, true),
        (true, false, false),
        (true, false, true),
        (true, true, false),
        (true, true, true),
    ];

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The *kind* of every declared property, in order (ignores the values).
    fn property_kinds(
        v: &CssPropertyWithConditionsVec,
    ) -> Vec<core::mem::Discriminant<CssProperty>> {
        v.as_ref()
            .iter()
            .map(|p| core::mem::discriminant(&p.property))
            .collect()
    }

    fn declares(v: &CssPropertyWithConditionsVec, pred: impl Fn(&CssProperty) -> bool) -> usize {
        v.as_ref().iter().filter(|p| pred(&p.property)).count()
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length — an
    /// `em`/`%` slipping into the segment geometry would resolve against the
    /// parent font/box instead of the intended fixed padding, border or radius.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "segment geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    /// The four paddings in `(top, bottom, left, right)` order.
    fn padding_px(
        v: &CssPropertyWithConditionsVec,
    ) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| {
            v.as_ref().iter().find_map(|p| f(&p.property))
        };
        (
            find(&|p| match p {
                CssProperty::PaddingTop(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::PaddingBottom(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::PaddingLeft(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::PaddingRight(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
        )
    }

    /// The four corner radii as `(top_left, top_right, bottom_left, bottom_right)`.
    fn radii_px(
        v: &CssPropertyWithConditionsVec,
    ) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| {
            v.as_ref().iter().find_map(|p| f(&p.property))
        };
        (
            find(&|p| match p {
                CssProperty::BorderTopLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderTopRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderBottomLeftRadius(r) => r.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderBottomRightRadius(r) => r.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
        )
    }

    /// The four border widths as `(top, bottom, left, right)`.
    fn border_widths_px(
        v: &CssPropertyWithConditionsVec,
    ) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| {
            v.as_ref().iter().find_map(|p| f(&p.property))
        };
        (
            find(&|p| match p {
                CssProperty::BorderTopWidth(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderBottomWidth(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderLeftWidth(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderRightWidth(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
        )
    }

    fn font_size_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FontSize(f) => f.get_property().map(|f| px(&f.inner)),
            _ => None,
        })
    }

    fn text_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::TextColor(c) => c.get_property().map(|c| c.inner),
            _ => None,
        })
    }

    /// The single background layer of a style vec, asserting there is exactly one
    /// and that it is a flat colour (a gradient would not be a `Color`).
    fn background_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        let bg = v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(b) => b.get_property(),
            _ => None,
        })?;
        assert_eq!(
            bg.as_ref().len(),
            1,
            "a segment must declare exactly one background layer"
        );
        match &bg.as_ref()[0] {
            StyleBackgroundContent::Color(c) => Some(*c),
            other => panic!("segment background is not a flat colour: {other:?}"),
        }
    }

    /// Perceived brightness (0..=255) of an sRGB colour, Rec.709 weights. Kept to
    /// plain `+`/`*` (no gamma expansion) so the readability assertions stay exact
    /// and toolchain-independent.
    fn luma(c: ColorU) -> f32 {
        0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b)
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

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The true recursive descendant count of a `Dom` — what
    /// `estimated_total_children` is documented to cache.
    fn recursive_descendants(node: &Dom) -> usize {
        node.children
            .as_ref()
            .iter()
            .map(|c| 1 + recursive_descendants(c))
            .sum()
    }

    /// Boundary + "negative" selection indices. `usize` has no negative values, so
    /// a `-1` handed in through FFI arrives here as `usize::MAX`; both wrapped
    /// forms are included so the setter is exercised at the two's-complement ends.
    fn boundary_indices() -> Vec<usize> {
        vec![
            0,
            1,
            2,
            usize::MAX / 2,
            usize::MAX / 2 + 1,
            usize::MAX - 1,
            usize::MAX,
            (-1i64) as usize,
            i64::MIN as usize,
            u32::MAX as usize,
        ]
    }

    /// Adversarial segment labels: empty, whitespace, combining marks, ZWJ emoji,
    /// RTL, embedded NULs (`AzString` is length-based, so a NUL must not
    /// truncate), control characters, and a string far longer than any plausible
    /// segment caption.
    fn adversarial_strings() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            "Day",
            " ",
            "e\u{0301}",                                   // e + combining acute
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family emoji
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",                // RTL Hebrew
            "\0",                                          // a single NUL
            "a\0b",                                        // embedded NUL
            "\u{FFFD}\u{202E}\u{200B}",                    // replacement, RTL override, ZWSP
            "line\nbreak\ttab",                            // control characters
            "-9223372036854775808",                        // i64::MIN as a caption
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(100_000));
        v
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn change_cb(f: SegmentedOnChangeCallbackType) -> SegmentedOnChangeCallback {
        f.into()
    }

    /// A `RefAny` payload recording every index a user `on_change` sees.
    struct IndexLog {
        seen: Vec<usize>,
    }

    extern "C" fn record_index(mut data: RefAny, _: CallbackInfo, state: SegmentedState) -> Update {
        if let Some(mut log) = data.downcast_mut::<IndexLog>() {
            log.seen.push(state.selected_index);
        }
        Update::RefreshDom
    }

    extern "C" fn change_do_nothing(_: RefAny, _: CallbackInfo, _: SegmentedState) -> Update {
        Update::DoNothing
    }

    extern "C" fn change_refresh_all(_: RefAny, _: CallbackInfo, state: SegmentedState) -> Update {
        // `selected_index` is read (and discarded) purely so this body cannot be
        // identical-code-folded onto another handler; the tests below compare
        // callback function pointers for equality/inequality.
        let _ = state.selected_index;
        Update::RefreshDomAllWindows
    }

    /// A payload whose callback tries to read the *same* `SegmentedStateWrapper`
    /// `RefAny` that the handler is currently holding a mutable borrow on.
    struct ReentrantProbe {
        /// A clone of the state `RefAny` the handler was invoked with.
        state: RefAny,
        /// `Some(index)` if the re-entrant read succeeded, `None` if it was
        /// refused. Starts as `Some(usize::MAX)` so "never ran" is distinguishable.
        saw_index: Option<usize>,
        calls: usize,
    }

    extern "C" fn probe_state_reentrantly(
        mut data: RefAny,
        _: CallbackInfo,
        _: SegmentedState,
    ) -> Update {
        if let Some(mut probe) = data.downcast_mut::<ReentrantProbe>() {
            probe.calls += 1;
            let mut state = probe.state.clone();
            probe.saw_index = state
                .downcast_ref::<SegmentedStateWrapper>()
                .map(|w| w.inner.selected_index);
        }
        Update::DoNothing
    }

    fn log_indices(data: &mut RefAny) -> Vec<usize> {
        data.downcast_ref::<IndexLog>()
            .expect("payload must still be an IndexLog")
            .seen
            .clone()
    }

    fn selected_index_of(data: &mut RefAny) -> usize {
        data.downcast_ref::<SegmentedStateWrapper>()
            .expect("payload must still be a SegmentedStateWrapper")
            .inner
            .selected_index
    }

    /// The `RefAny` carried by segment `i`'s click callback.
    fn segment_state(dom: &Dom, i: usize) -> RefAny {
        let cbs = dom.children.as_ref()[i].root.get_callbacks();
        cbs.as_ref()
            .first()
            .expect("every segment must carry the click callback")
            .refany
            .clone()
    }

    /// A `DomLayoutResult` with an *empty* layout tree: `on_segment_click` only
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

    /// Flattens `seg.dom()` and hands back the shared state `RefAny` the segment
    /// callbacks carry. Requires at least one label.
    fn flatten(seg: Segmented) -> (StyledDom, RefAny) {
        let dom = seg.dom();
        let state = segment_state(&dom, 0);
        (StyledDom::create_from_dom(dom), state)
    }

    /// Flattened (pre-order) node id of segment `i`. Every segment is a `<p>`
    /// wrapping one bare text node, so the tree is
    /// `0 root / 1 seg0 <p> / 2 seg0 text / 3 seg1 <p> / …` and the callback
    /// sits on the `<p>`.
    const fn seg_node(i: usize) -> usize {
        2 * i + 1
    }

    /// Invokes `on_segment_click` against a `LayoutWindow` holding `styled` (or
    /// nothing at all, when `styled` is `None`), with node `hit` as the hit node.
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(
        styled: Option<StyledDom>,
        hit: usize,
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
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(hit))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = on_segment_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// Every colour the live restyle wrote, as `(node index, "bg" | "text", colour)`
    /// in emission order. Panics on any property other than the two the handler is
    /// documented to write.
    fn restyle_writes(changes: &[CallbackChange]) -> Vec<(usize, &'static str, ColorU)> {
        let mut out = Vec::new();
        for change in changes {
            let CallbackChange::ChangeNodeCssProperties {
                node_id,
                properties,
                ..
            } = change
            else {
                panic!("the restyle must only emit ChangeNodeCssProperties, got {change:?}");
            };
            for p in properties.as_ref() {
                match p {
                    CssProperty::BackgroundContent(v) => {
                        let layers = v
                            .get_property()
                            .expect("restyle must write an exact background");
                        assert_eq!(layers.as_ref().len(), 1, "a segment fill is a single layer");
                        match &layers.as_ref()[0] {
                            StyleBackgroundContent::Color(c) => {
                                out.push((node_id.index(), "bg", *c));
                            }
                            other => panic!("segment background is not a flat colour: {other:?}"),
                        }
                    }
                    CssProperty::TextColor(v) => {
                        let c = v
                            .get_property()
                            .expect("restyle must write an exact text colour");
                        out.push((node_id.index(), "text", c.inner));
                    }
                    other => panic!("unexpected restyle property: {other:?}"),
                }
            }
        }
        out
    }

    // ------------------------------------------------------------------
    // build_segment_style
    // ------------------------------------------------------------------

    #[test]
    fn build_segment_style_handles_all_eight_flag_combinations() {
        // 24 shared declarations, +5 for the first segment (left border triple +
        // the two left radii), +2 for the last (the two right radii).
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            let expected = 24 + if first { 5 } else { 0 } + if last { 2 } else { 0 };
            assert_eq!(
                style.as_ref().len(),
                expected,
                "({selected}, {first}, {last}): unexpected declaration count"
            );
        }
    }

    #[test]
    fn build_segment_style_colours_depend_only_on_selected() {
        // Position must not leak into the palette: a "first" segment and a
        // "middle" segment with the same selection must paint identically.
        for selected in [false, true] {
            let reference = build_segment_style(selected, false, false);
            let bg = background_color(&reference).expect("a segment must declare a background");
            let fg = text_color(&reference).expect("a segment must declare a text colour");

            for (first, last) in [(false, false), (false, true), (true, false), (true, true)] {
                let style = build_segment_style(selected, first, last);
                assert_eq!(
                    background_color(&style),
                    Some(bg),
                    "selected={selected}: background moved with position"
                );
                assert_eq!(
                    text_color(&style),
                    Some(fg),
                    "selected={selected}: text colour moved with position"
                );
            }
        }

        assert_eq!(
            background_color(&build_segment_style(true, false, false)),
            Some(SEG_SELECTED_BG_COLOR)
        );
        assert_eq!(
            text_color(&build_segment_style(true, false, false)),
            Some(SEG_SELECTED_TEXT)
        );
        assert_eq!(
            background_color(&build_segment_style(false, false, false)),
            Some(SEG_UNSELECTED_BG_COLOR)
        );
        assert_eq!(
            text_color(&build_segment_style(false, false, false)),
            Some(SEG_UNSELECTED_TEXT)
        );
    }

    #[test]
    fn build_segment_style_adds_the_left_border_only_to_the_first_segment() {
        // Every segment paints its own right border, so a non-first segment that
        // also painted a left one would render a 2px seam between neighbours.
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            let want = usize::from(first);

            assert_eq!(
                declares(&style, |p| matches!(p, CssProperty::BorderLeftWidth(_))),
                want,
                "({selected}, {first}, {last}): left border width"
            );
            assert_eq!(
                declares(&style, |p| matches!(p, CssProperty::BorderLeftStyle(_))),
                want,
                "({selected}, {first}, {last}): left border style"
            );
            assert_eq!(
                declares(&style, |p| matches!(p, CssProperty::BorderLeftColor(_))),
                want,
                "({selected}, {first}, {last}): left border colour"
            );
        }
    }

    #[test]
    fn build_segment_style_rounds_only_the_outer_corners() {
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            let (tl, tr, bl, br) = radii_px(&style);
            let r = SEG_RADIUS as f32;

            assert_eq!(
                tl,
                first.then_some(r),
                "({selected}, {first}, {last}): top-left radius"
            );
            assert_eq!(
                bl,
                first.then_some(r),
                "({selected}, {first}, {last}): bottom-left radius"
            );
            assert_eq!(
                tr,
                last.then_some(r),
                "({selected}, {first}, {last}): top-right radius"
            );
            assert_eq!(
                br,
                last.then_some(r),
                "({selected}, {first}, {last}): bottom-right radius"
            );
        }

        // A lone segment is a fully rounded pill; an interior segment is square.
        let solo = build_segment_style(true, true, true);
        let r = SEG_RADIUS as f32;
        assert_eq!(radii_px(&solo), (Some(r), Some(r), Some(r), Some(r)));
        let middle = build_segment_style(true, false, false);
        assert_eq!(radii_px(&middle), (None, None, None, None));
    }

    #[test]
    fn build_segment_style_always_paints_the_shared_separator_edges() {
        // Top/bottom/right must be declared unconditionally — dropping the right
        // border on the last segment would leave the group open on one side.
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            let (top, bottom, left, right) = border_widths_px(&style);

            assert_eq!(top, Some(1.0), "({selected}, {first}, {last}): top border");
            assert_eq!(
                bottom,
                Some(1.0),
                "({selected}, {first}, {last}): bottom border"
            );
            assert_eq!(
                right,
                Some(1.0),
                "({selected}, {first}, {last}): right border"
            );
            assert_eq!(
                left,
                first.then_some(1.0),
                "({selected}, {first}, {last}): left border"
            );

            // A width without a matching style/colour renders as no border at all.
            for count in [
                declares(&style, |p| matches!(p, CssProperty::BorderTopStyle(_))),
                declares(&style, |p| matches!(p, CssProperty::BorderBottomStyle(_))),
                declares(&style, |p| matches!(p, CssProperty::BorderRightStyle(_))),
                declares(&style, |p| matches!(p, CssProperty::BorderTopColor(_))),
                declares(&style, |p| matches!(p, CssProperty::BorderBottomColor(_))),
                declares(&style, |p| matches!(p, CssProperty::BorderRightColor(_))),
            ] {
                assert_eq!(
                    count, 1,
                    "({selected}, {first}, {last}): a shared edge lost its style/colour"
                );
            }
        }
    }

    #[test]
    fn build_segment_style_border_colours_are_the_single_neutral_grey() {
        // A width without a matching colour (or a stray second grey) shows up as
        // an inconsistent seam between neighbouring segments.
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            for p in style.as_ref() {
                let found = match &p.property {
                    CssProperty::BorderTopColor(c) => c.get_property().map(|c| c.inner),
                    CssProperty::BorderBottomColor(c) => c.get_property().map(|c| c.inner),
                    CssProperty::BorderLeftColor(c) => c.get_property().map(|c| c.inner),
                    CssProperty::BorderRightColor(c) => c.get_property().map(|c| c.inner),
                    _ => None,
                };
                if let Some(c) = found {
                    assert_eq!(
                        c, SEG_BORDER_COLOR,
                        "({selected}, {first}, {last}): border colour {c:?} is not the shared grey"
                    );
                }
            }
        }
    }

    #[test]
    fn build_segment_style_geometry_is_absolute_and_symmetric() {
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            // padding: 6px 12px — `px()` asserts the metric on every value it reads.
            assert_eq!(
                padding_px(&style),
                (Some(6.0), Some(6.0), Some(12.0), Some(12.0)),
                "({selected}, {first}, {last}): padding is not 6px 12px"
            );
            assert_eq!(
                font_size_px(&style),
                Some(13.0),
                "({selected}, {first}, {last}): font size"
            );
        }
    }

    #[test]
    fn build_segment_style_declares_every_property_unconditionally() {
        // `simple()` means an empty `apply_if`. A stray condition here would make
        // the segment silently unstyled until some selector state happened to match.
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            for p in style.as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "({selected}, {first}, {last}): {:?} is conditional",
                    p.property
                );
            }
        }
    }

    #[test]
    fn build_segment_style_never_declares_the_same_property_twice() {
        // Duplicates are silently last-wins, so a doubled declaration hides a
        // genuine value conflict instead of failing loudly.
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            let mut seen = HashSet::new();
            for kind in property_kinds(&style) {
                assert!(
                    seen.insert(kind),
                    "({selected}, {first}, {last}): duplicate property kind in the style vec"
                );
            }
            assert_eq!(seen.len(), style.as_ref().len());
        }
    }

    #[test]
    fn build_segment_style_is_pure() {
        // Called once per segment on every `dom()`; a hidden `static mut` cache or
        // an accumulating vec would show up as drift between two identical calls.
        for (selected, first, last) in ALL_FLAGS {
            let a = build_segment_style(selected, first, last);
            let b = build_segment_style(selected, first, last);
            assert_eq!(
                properties(&a),
                properties(&b),
                "({selected}, {first}, {last}): not pure"
            );
        }
    }

    #[test]
    fn build_segment_style_keeps_the_label_readable_and_opaque() {
        for selected in [false, true] {
            let style = build_segment_style(selected, false, false);
            let bg = background_color(&style).expect("background");
            let fg = text_color(&style).expect("text colour");

            assert_eq!(
                bg.a, 255,
                "selected={selected}: a translucent fill lets the page bleed through"
            );
            assert_eq!(fg.a, 255, "selected={selected}: translucent label text");
            assert_ne!(
                bg, fg,
                "selected={selected}: an invisible label is not a segment"
            );
            assert!(
                (luma(bg) - luma(fg)).abs() >= 60.0,
                "selected={selected}: brightness gap {:.1} is too low to read",
                (luma(bg) - luma(fg)).abs()
            );
        }

        // The two states must be visually distinguishable — that is the entire
        // point of a segmented control.
        let sel = build_segment_style(true, false, false);
        let unsel = build_segment_style(false, false, false);
        assert_ne!(background_color(&sel), background_color(&unsel));
        assert_ne!(text_color(&sel), text_color(&unsel));
    }

    #[test]
    fn build_segment_style_declares_the_interaction_affordances() {
        for (selected, first, last) in ALL_FLAGS {
            let style = build_segment_style(selected, first, last);
            let ctx = format!("({selected}, {first}, {last})");

            assert!(
                style.as_ref().iter().any(|p| matches!(
                    &p.property,
                    CssProperty::Cursor(c) if c.get_property() == Some(&StyleCursor::Pointer)
                )),
                "{ctx}: a clickable segment must show the pointer cursor"
            );
            assert!(
                style.as_ref().iter().any(|p| matches!(
                    &p.property,
                    CssProperty::UserSelect(u) if u.get_property() == Some(&StyleUserSelect::None)
                )),
                "{ctx}: click-dragging a segment must not select its caption"
            );
            assert!(
                style.as_ref().iter().any(|p| matches!(
                    &p.property,
                    CssProperty::TextAlign(t) if t.get_property() == Some(&StyleTextAlign::Center)
                )),
                "{ctx}: captions are centred"
            );
            assert!(
                style.as_ref().iter().any(|p| matches!(
                    &p.property,
                    CssProperty::FlexGrow(f) if f.get_property().map(|f| f.inner.get()) == Some(0.0)
                )),
                "{ctx}: segments hug their caption, they do not stretch"
            );
        }
    }

    // ------------------------------------------------------------------
    // Segmented::create
    // ------------------------------------------------------------------

    #[test]
    fn create_preserves_labels_verbatim() {
        for case in [
            vec![],
            vec!["only"],
            vec!["Day", "Week"],
            vec!["Day", "Week", "Month", "Year"],
            vec!["dup", "dup", "dup"],
        ] {
            let seg = Segmented::create(labels(&case));
            let got: Vec<&str> = seg.labels.as_ref().iter().map(AzString::as_str).collect();
            assert_eq!(
                got, case,
                "create must not reorder/drop/dedupe/rewrite labels"
            );
        }
    }

    #[test]
    fn create_preserves_adversarial_labels_byte_for_byte() {
        for s in adversarial_strings() {
            let seg = Segmented::create(labels(&[s.as_str()]));
            let stored = seg.labels.as_ref()[0].as_str();
            assert_eq!(
                stored,
                s.as_str(),
                "the caption changed on its way into the widget"
            );
            assert_eq!(
                seg.labels.as_ref()[0].as_ref().len(),
                s.len(),
                "byte length changed (NUL truncation?)"
            );
        }
    }

    #[test]
    fn create_selects_the_first_segment_and_installs_no_callback() {
        for n in [0usize, 1, 2, 7] {
            let seg = Segmented::create(n_labels(n));
            assert_eq!(
                seg.segmented_state.inner.selected_index, 0,
                "n={n}: a fresh control starts on segment 0"
            );
            assert!(
                seg.segmented_state.on_change.as_ref().is_none(),
                "n={n}: create must not wire a callback"
            );
        }
    }

    #[test]
    fn create_installs_the_shared_container_style() {
        let seg = Segmented::create(labels(&["a", "b"]));
        assert_eq!(
            seg.container_style.as_ref(),
            SEGMENTED_CONTAINER_STYLE,
            "create must install the shared container style"
        );

        // Decode the semantics too, so a silent edit of the const is caught here
        // rather than only in a screenshot: a horizontal, content-hugging row.
        let style = &seg.container_style;
        assert_eq!(
            declares(style, |p| matches!(
                p, CssProperty::Display(d) if d.get_property() == Some(&LayoutDisplay::Flex))),
            1,
            "the row container must be a flex box"
        );
        assert_eq!(
            declares(style, |p| matches!(
                p, CssProperty::FlexDirection(d) if d.get_property() == Some(&LayoutFlexDirection::Row))),
            1,
            "segments are joined horizontally"
        );
        assert_eq!(
            declares(style, |p| matches!(
                p, CssProperty::AlignItems(a) if a.get_property() == Some(&LayoutAlignItems::Center))),
            1
        );
        assert_eq!(
            declares(style, |p| matches!(
                p, CssProperty::AlignSelf(a) if a.get_property() == Some(&LayoutAlignSelf::Start))),
            1
        );
        assert_eq!(
            declares(style, |p| matches!(
                p, CssProperty::FlexGrow(f) if f.get_property().map(|f| f.inner.get()) == Some(0.0))),
            1,
            "the group hugs its segments instead of filling the parent"
        );

        for p in seg.container_style.as_ref() {
            assert!(
                p.apply_if.as_ref().is_empty(),
                "{:?} is conditional",
                p.property
            );
        }
    }

    #[test]
    fn create_with_no_labels_equals_default() {
        let empty = Segmented::create(StringVec::from_const_slice(&[]));
        assert_eq!(
            empty,
            Segmented::default(),
            "Default must be the empty control"
        );
        assert_eq!(empty.labels.as_ref().len(), 0);
        assert!(Segmented::default()
            .segmented_state
            .on_change
            .as_ref()
            .is_none());
    }

    #[test]
    fn create_scales_to_a_very_long_label_list() {
        let n = 4096;
        let seg = Segmented::create(n_labels(n));
        assert_eq!(seg.labels.as_ref().len(), n);
        assert_eq!(seg.labels.as_ref()[n - 1].as_str(), format!("s{}", n - 1));
        assert_eq!(seg.segmented_state.inner.selected_index, 0);
    }

    // ------------------------------------------------------------------
    // Segmented::set_selected_index  /  with_selected_index
    // ------------------------------------------------------------------

    #[test]
    fn set_selected_index_stores_every_boundary_value_verbatim() {
        // The setter is a plain field write: no clamping, no wrapping, no panic —
        // not even at `usize::MAX` or at a `-1` that arrived through FFI.
        for i in boundary_indices() {
            let mut seg = Segmented::create(labels(&["a", "b", "c"]));
            seg.set_selected_index(i);
            assert_eq!(
                seg.segmented_state.inner.selected_index, i,
                "index {i} was not stored as-is"
            );
        }
    }

    #[test]
    fn set_selected_index_does_not_clamp_to_the_label_count() {
        // Documenting the actual contract: an out-of-range index is *accepted*
        // and simply selects nothing when rendered (see the `dom_` tests below).
        let mut seg = Segmented::create(labels(&["a", "b"]));
        for i in [2usize, 3, 1_000, usize::MAX] {
            seg.set_selected_index(i);
            assert_eq!(seg.segmented_state.inner.selected_index, i);
            assert_eq!(
                seg.labels.as_ref().len(),
                2,
                "the setter must not touch the labels"
            );
        }
    }

    #[test]
    fn set_selected_index_is_idempotent_and_last_write_wins() {
        let mut seg = Segmented::create(labels(&["a", "b", "c"]));
        for i in [1usize, 1, 1] {
            seg.set_selected_index(i);
        }
        assert_eq!(seg.segmented_state.inner.selected_index, 1);

        for i in [0usize, usize::MAX, 2, 0] {
            seg.set_selected_index(i);
        }
        assert_eq!(
            seg.segmented_state.inner.selected_index, 0,
            "the last write must win"
        );
    }

    #[test]
    fn set_selected_index_leaves_every_other_field_alone() {
        let mut seg = Segmented::create(labels(&["a", "b"]))
            .with_on_change(RefAny::new(7u8), change_cb(change_do_nothing));
        let before = seg.clone();

        seg.set_selected_index(usize::MAX);

        assert_eq!(seg.labels, before.labels, "labels changed");
        assert_eq!(
            seg.container_style, before.container_style,
            "container style changed"
        );
        assert_eq!(
            seg.segmented_state.on_change, before.segmented_state.on_change,
            "the callback was disturbed"
        );
    }

    #[test]
    fn with_selected_index_round_trips_through_the_setter() {
        for i in boundary_indices() {
            let via_builder = Segmented::create(labels(&["a", "b"])).with_selected_index(i);
            let mut via_setter = Segmented::create(labels(&["a", "b"]));
            via_setter.set_selected_index(i);

            assert_eq!(
                via_builder, via_setter,
                "index {i}: builder and setter diverge"
            );
            assert_eq!(via_builder.segmented_state.inner.selected_index, i);
        }
    }

    #[test]
    fn with_selected_index_preserves_the_rest_of_the_widget() {
        let base = Segmented::create(labels(&["a", "b", "c"]));
        let built = base.clone().with_selected_index(2);

        assert_eq!(built.labels, base.labels);
        assert_eq!(built.container_style, base.container_style);
        assert_eq!(
            built.labels.as_ref().len(),
            3,
            "len/contents must stay consistent"
        );
        assert!(built.segmented_state.on_change.as_ref().is_none());
    }

    #[test]
    fn with_selected_index_chains_with_last_wins() {
        let seg = Segmented::create(labels(&["a", "b", "c"]))
            .with_selected_index(usize::MAX)
            .with_selected_index(0)
            .with_selected_index(2);
        assert_eq!(seg.segmented_state.inner.selected_index, 2);
    }

    // ------------------------------------------------------------------
    // Segmented::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_default_behind() {
        let mut seg = Segmented::create(labels(&["Day", "Week", "Month"])).with_selected_index(2);
        let expected = seg.clone();

        let taken = seg.swap_with_default();

        assert_eq!(
            taken, expected,
            "the caller must get the original widget back"
        );
        assert_eq!(
            seg,
            Segmented::default(),
            "a default must be left in its place"
        );
        assert_eq!(seg.labels.as_ref().len(), 0);
        assert_eq!(seg.segmented_state.inner.selected_index, 0);
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_with_the_widget() {
        let mut seg = Segmented::create(labels(&["a", "b"]))
            .with_on_change(RefAny::new(1u8), change_cb(record_index));

        let taken = seg.swap_with_default();

        assert!(
            taken.segmented_state.on_change.as_ref().is_some(),
            "the callback must travel with the taken widget"
        );
        assert!(
            seg.segmented_state.on_change.as_ref().is_none(),
            "the leftover default must not keep a handle on the callback"
        );
    }

    #[test]
    fn swap_with_default_on_a_default_is_a_no_op() {
        let mut seg = Segmented::default();
        let taken = seg.swap_with_default();
        assert_eq!(taken, Segmented::default());
        assert_eq!(seg, Segmented::default());
    }

    #[test]
    fn swap_with_default_twice_yields_a_default_the_second_time() {
        let mut seg = Segmented::create(labels(&["a", "b"])).with_selected_index(1);
        let first = seg.swap_with_default();
        let second = seg.swap_with_default();

        assert_eq!(first.labels.as_ref().len(), 2);
        assert_eq!(first.segmented_state.inner.selected_index, 1);
        assert_eq!(
            second,
            Segmented::default(),
            "the second take is the default we left behind"
        );
        assert_eq!(seg, Segmented::default());
    }

    #[test]
    fn swap_with_default_does_not_truncate_a_large_label_list() {
        let n = 1024;
        let mut seg = Segmented::create(n_labels(n)).with_selected_index(n - 1);
        let taken = seg.swap_with_default();

        assert_eq!(taken.labels.as_ref().len(), n);
        assert_eq!(taken.labels.as_ref()[n - 1].as_str(), format!("s{}", n - 1));
        assert_eq!(taken.segmented_state.inner.selected_index, n - 1);
    }

    // ------------------------------------------------------------------
    // Segmented::set_on_change  /  with_on_change
    // ------------------------------------------------------------------

    #[test]
    fn set_on_change_installs_the_callback_and_its_payload() {
        let mut seg = Segmented::create(labels(&["a", "b"]));
        let mut payload = RefAny::new(IndexLog { seen: Vec::new() });
        seg.set_on_change(payload.clone(), change_cb(record_index));

        let installed = seg
            .segmented_state
            .on_change
            .as_ref()
            .expect("set_on_change must install a callback");
        assert_eq!(
            installed.callback.cb as usize, record_index as usize,
            "wrong function installed"
        );
        assert!(
            matches!(installed.callback.ctx, OptionRefAny::None),
            "a native Rust callback carries no FFI context"
        );

        // The stored `RefAny` must be a *share* of the caller's, not a copy:
        // writing through the widget's handle must be visible to the caller.
        let mut stored = installed.refany.clone();
        {
            let mut log = stored
                .downcast_mut::<IndexLog>()
                .expect("payload type must survive");
            log.seen.push(42);
        }
        assert_eq!(
            log_indices(&mut payload),
            vec![42],
            "the payload was copied, not shared"
        );
    }

    #[test]
    fn set_on_change_overwrites_a_previously_installed_callback() {
        let mut seg = Segmented::create(labels(&["a", "b"]));
        seg.set_on_change(RefAny::new(1u8), change_cb(record_index));
        seg.set_on_change(RefAny::new(2u8), change_cb(change_refresh_all));

        let installed = seg.segmented_state.on_change.as_ref().expect("callback");
        assert_eq!(
            installed.callback.cb as usize, change_refresh_all as usize,
            "the last setter must win"
        );
        assert_ne!(installed.callback.cb as usize, record_index as usize);
    }

    #[test]
    fn set_on_change_does_not_disturb_labels_or_selection() {
        let mut seg = Segmented::create(labels(&["a", "b", "c"])).with_selected_index(2);
        seg.set_on_change(RefAny::new(0u8), change_cb(change_do_nothing));

        assert_eq!(seg.labels.as_ref().len(), 3);
        assert_eq!(
            seg.segmented_state.inner.selected_index, 2,
            "installing a callback moved the selection"
        );
    }

    #[test]
    fn with_on_change_matches_the_setter_exactly() {
        let payload = RefAny::new(9u8);
        let via_builder = Segmented::create(labels(&["a", "b"]))
            .with_on_change(payload.clone(), change_cb(change_do_nothing));
        let mut via_setter = Segmented::create(labels(&["a", "b"]));
        via_setter.set_on_change(payload, change_cb(change_do_nothing));

        assert_eq!(
            via_builder, via_setter,
            "builder and setter must produce the same widget"
        );
    }

    #[test]
    fn with_on_change_holds_its_invariants_after_construction() {
        let seg = Segmented::create(n_labels(5))
            .with_selected_index(3)
            .with_on_change(RefAny::new(0u8), change_cb(change_refresh_all));

        assert_eq!(
            seg.labels.as_ref().len(),
            5,
            "label count must survive the builder chain"
        );
        assert_eq!(
            seg.segmented_state.inner.selected_index, 3,
            "the selection must survive"
        );
        assert_eq!(
            seg.container_style.as_ref(),
            SEGMENTED_CONTAINER_STYLE,
            "the container style must survive"
        );
        let installed = seg.segmented_state.on_change.as_ref().expect("callback");
        assert_eq!(installed.callback.cb as usize, change_refresh_all as usize);
    }

    #[test]
    fn with_on_change_chains_with_last_wins() {
        let seg = Segmented::create(labels(&["a"]))
            .with_on_change(RefAny::new(0u8), change_cb(record_index))
            .with_on_change(RefAny::new(0u8), change_cb(change_do_nothing));
        let installed = seg.segmented_state.on_change.as_ref().expect("callback");
        assert_eq!(installed.callback.cb as usize, change_do_nothing as usize);
    }

    // ------------------------------------------------------------------
    // Segmented::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_emits_one_text_child_per_label_in_order() {
        let case = ["Day", "Week", "Month", "Year"];
        let dom = Segmented::create(labels(&case)).dom();

        assert!(
            matches!(dom.root.get_node_type(), NodeType::Div),
            "the group is a div"
        );
        assert!(dom.root.has_class("__azul-native-segmented"));
        assert!(
            dom.root.get_callbacks().as_ref().is_empty(),
            "the container itself is not clickable"
        );

        let children = dom.children.as_ref();
        assert_eq!(children.len(), case.len());
        for (i, child) in children.iter().enumerate() {
            assert_eq!(
                text_of(child),
                Some(case[i]),
                "segment {i} shows the wrong caption"
            );
            assert!(
                child.root.has_class("__azul-native-segmented-item"),
                "segment {i} lost its class"
            );
        }
    }

    #[test]
    fn dom_of_an_empty_control_is_a_childless_container() {
        // `count == 0` must not underflow `i + 1 == count` or emit a stray child.
        let dom = Segmented::create(StringVec::from_const_slice(&[])).dom();
        assert_eq!(dom.children.as_ref().len(), 0);
        assert_eq!(dom.estimated_total_children, 0);
        assert!(dom.root.has_class("__azul-native-segmented"));

        let styled = StyledDom::create_from_dom(dom);
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            1,
            "just the container"
        );
    }

    #[test]
    fn dom_styles_each_segment_by_its_position_and_selection() {
        for n in [1usize, 2, 3, 5] {
            for selected in 0..n {
                let dom = Segmented::create(n_labels(n))
                    .with_selected_index(selected)
                    .dom();
                let children = dom.children.as_ref();
                assert_eq!(children.len(), n);

                for (i, child) in children.iter().enumerate() {
                    let expected =
                        properties(&build_segment_style(i == selected, i == 0, i + 1 == n));
                    assert_eq!(
                        inline_properties(child),
                        expected,
                        "n={n} selected={selected}: segment {i} carries the wrong style"
                    );
                }
            }
        }
    }

    #[test]
    fn dom_marks_exactly_one_segment_as_selected() {
        for n in [1usize, 2, 4] {
            for selected in 0..n {
                let dom = Segmented::create(n_labels(n))
                    .with_selected_index(selected)
                    .dom();
                let marked: Vec<usize> = dom
                    .children
                    .as_ref()
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        inline_properties(c).iter().any(|p| {
                            matches!(
                            p, CssProperty::TextColor(t)
                                if t.get_property().map(|t| t.inner) == Some(SEG_SELECTED_TEXT))
                        })
                    })
                    .map(|(i, _)| i)
                    .collect();
                assert_eq!(marked, vec![selected], "n={n}: mutual exclusivity broken");
            }
        }
    }

    #[test]
    fn dom_with_an_out_of_range_selection_marks_nothing_and_does_not_panic() {
        // `set_selected_index` accepts any `usize`; rendering must degrade to
        // "nothing selected" rather than panicking or wrapping onto a real segment.
        let n = 3;
        for selected in [n, n + 1, 1_000, usize::MAX, usize::MAX - 1] {
            let dom = Segmented::create(n_labels(n))
                .with_selected_index(selected)
                .dom();
            assert_eq!(
                dom.children.as_ref().len(),
                n,
                "selected={selected}: child count changed"
            );

            for (i, child) in dom.children.as_ref().iter().enumerate() {
                let expected = properties(&build_segment_style(false, i == 0, i + 1 == n));
                assert_eq!(
                    inline_properties(child),
                    expected,
                    "selected={selected}: segment {i} must render unselected"
                );
            }
        }
    }

    #[test]
    fn dom_rounds_only_the_two_outer_segments() {
        let n = 4;
        let dom = Segmented::create(n_labels(n)).dom();
        let r = SEG_RADIUS as f32;

        let radii_of = |child: &Dom| -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
            let props = inline_properties(child);
            let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| props.iter().find_map(f);
            (
                find(&|p| match p {
                    CssProperty::BorderTopLeftRadius(x) => x.get_property().map(|x| px(&x.inner)),
                    _ => None,
                }),
                find(&|p| match p {
                    CssProperty::BorderTopRightRadius(x) => x.get_property().map(|x| px(&x.inner)),
                    _ => None,
                }),
                find(&|p| match p {
                    CssProperty::BorderBottomLeftRadius(x) => {
                        x.get_property().map(|x| px(&x.inner))
                    }
                    _ => None,
                }),
                find(&|p| match p {
                    CssProperty::BorderBottomRightRadius(x) => {
                        x.get_property().map(|x| px(&x.inner))
                    }
                    _ => None,
                }),
            )
        };

        let children = dom.children.as_ref();
        assert_eq!(
            radii_of(&children[0]),
            (Some(r), None, Some(r), None),
            "first: left corners only"
        );
        assert_eq!(
            radii_of(&children[1]),
            (None, None, None, None),
            "interior segments are square"
        );
        assert_eq!(
            radii_of(&children[2]),
            (None, None, None, None),
            "interior segments are square"
        );
        assert_eq!(
            radii_of(&children[3]),
            (None, Some(r), None, Some(r)),
            "last: right corners only"
        );
    }

    #[test]
    fn dom_of_a_single_segment_is_rounded_on_both_ends() {
        let dom = Segmented::create(labels(&["only"])).dom();
        let children = dom.children.as_ref();
        assert_eq!(children.len(), 1);

        let expected = properties(&build_segment_style(true, true, true));
        assert_eq!(
            inline_properties(&children[0]),
            expected,
            "a lone segment is simultaneously first and last"
        );
    }

    #[test]
    fn dom_makes_every_segment_clickable_and_keyboard_reachable() {
        let n = 3;
        let dom = Segmented::create(n_labels(n)).dom();
        for (i, child) in dom.children.as_ref().iter().enumerate() {
            let cbs = child.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 1, "segment {i}: exactly one handler");
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::Click)
            );
            assert_eq!(cbs.as_ref()[0].callback.cb, on_segment_click as usize);
            assert!(matches!(cbs.as_ref()[0].callback.ctx, OptionRefAny::None));
            assert_eq!(
                child.root.get_tab_index(),
                Some(TabIndex::Auto),
                "segment {i} must be tab-reachable"
            );
        }
    }

    #[test]
    fn dom_shares_one_state_refany_across_every_segment() {
        // The handler resolves the clicked index from the DOM, so all segments
        // *must* observe the same state — a per-segment copy would let two
        // segments believe they are both selected.
        let dom = Segmented::create(n_labels(4)).dom();

        let mut first = segment_state(&dom, 0);
        {
            let mut w = first
                .downcast_mut::<SegmentedStateWrapper>()
                .expect("segment state must be a SegmentedStateWrapper");
            w.inner.selected_index = 3;
        }

        for i in 1..4 {
            let mut other = segment_state(&dom, i);
            assert_eq!(
                selected_index_of(&mut other),
                3,
                "segment {i} does not share segment 0's state"
            );
        }
    }

    #[test]
    fn dom_carries_the_installed_callback_into_the_shared_state() {
        let dom = Segmented::create(labels(&["a", "b"]))
            .with_on_change(RefAny::new(0u8), change_cb(change_refresh_all))
            .dom();
        let mut state = segment_state(&dom, 0);
        let wrapper = state
            .downcast_ref::<SegmentedStateWrapper>()
            .expect("SegmentedStateWrapper");
        let installed = wrapper
            .on_change
            .as_ref()
            .expect("the user callback must reach the DOM");
        assert_eq!(installed.callback.cb as usize, change_refresh_all as usize);
    }

    #[test]
    fn dom_preserves_adversarial_labels_verbatim() {
        for s in adversarial_strings() {
            let dom = Segmented::create(labels(&[s.as_str(), "other"])).dom();
            let children = dom.children.as_ref();
            assert_eq!(children.len(), 2);
            match children[0].children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(t) => {
                        assert_eq!(
                            t.as_ref().as_str(),
                            s.as_str(),
                            "the caption changed inside dom()"
                        );
                        assert_eq!(
                            t.as_ref().len(),
                            s.len(),
                            "byte length changed (NUL truncation?)"
                        );
                    }
                    other => panic!("expected a text node, got {other:?}"),
                },
                other => panic!("expected `p > text`, got {} children", other.len()),
            }
        }
    }

    #[test]
    fn dom_keeps_estimated_total_children_in_sync() {
        // `estimated_total_children` is a cached count; if it under-counts,
        // `convert_dom_into_compact_dom` under-allocates and panics.
        for n in [0usize, 1, 2, 3, 5, 64, 257] {
            let dom = Segmented::create(n_labels(n)).dom();
            assert_eq!(dom.children.as_ref().len(), n, "child count for n={n}");
            assert_eq!(
                dom.estimated_total_children,
                recursive_descendants(&dom),
                "cached descendant count desynced for n={n}"
            );
            assert_eq!(
                dom.estimated_total_children,
                2 * n,
                "for n={n} (each segment is a <p> wrapping one text node)"
            );
        }
    }

    #[test]
    fn dom_of_many_segments_flattens_without_panicking() {
        let n = 512;
        let styled = StyledDom::create_from_dom(Segmented::create(n_labels(n)).dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            2 * n + 1,
            "root + n segments, each a <p> wrapping one text node"
        );
    }

    #[test]
    fn dom_via_from_matches_dom_exactly() {
        let build = || Segmented::create(n_labels(3)).with_selected_index(1);
        let via_into: Dom = build().into();
        let via_dom = build().dom();

        assert_eq!(
            via_into.children.as_ref().len(),
            via_dom.children.as_ref().len()
        );
        assert_eq!(
            via_into.estimated_total_children,
            via_dom.estimated_total_children
        );
        for i in 0..via_dom.children.as_ref().len() {
            assert_eq!(
                inline_properties(&via_into.children.as_ref()[i]),
                inline_properties(&via_dom.children.as_ref()[i]),
                "`From` diverges from `dom()` at segment {i}"
            );
            assert_eq!(
                text_of(&via_into.children.as_ref()[i]),
                text_of(&via_dom.children.as_ref()[i])
            );
        }
    }

    #[test]
    fn dom_with_duplicate_labels_still_produces_distinct_positional_segments() {
        // Selection is positional, not by caption: three identical captions must
        // still give exactly one selected segment, at the requested position.
        let dom = Segmented::create(labels(&["same", "same", "same"]))
            .with_selected_index(1)
            .dom();
        let children = dom.children.as_ref();
        for (i, child) in children.iter().enumerate() {
            assert_eq!(text_of(child), Some("same"));
            let expected = properties(&build_segment_style(i == 1, i == 0, i == 2));
            assert_eq!(inline_properties(child), expected, "segment {i}");
        }
    }

    // ------------------------------------------------------------------
    // on_segment_click
    // ------------------------------------------------------------------

    #[test]
    fn click_selects_the_segment_at_the_clicked_position() {
        let n = 4;
        let (styled, state) = flatten(Segmented::create(n_labels(n)));
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            2 * n + 1,
            "fixture: root + n segments, each a <p> wrapping one text node"
        );

        for i in 0..n {
            let mut state = state.clone();
            let (update, changes) = run_click(Some(styled.clone()), seg_node(i), state.clone());

            assert_eq!(
                update,
                Update::DoNothing,
                "with no on_change installed the handler reports nothing to redraw"
            );
            assert_eq!(
                selected_index_of(&mut state),
                i,
                "node {} must select segment {i}",
                seg_node(i)
            );
            assert_eq!(
                restyle_writes(&changes).len(),
                2 * n,
                "every segment must be restyled"
            );
        }
    }

    #[test]
    fn click_restyle_agrees_with_a_freshly_built_style() {
        // The live restyle and a full rebuild must not drift apart, or a click
        // followed by a `RefreshDom` would visibly change the widget twice.
        let n = 4;
        let (styled, state) = flatten(Segmented::create(n_labels(n)));

        for clicked in 0..n {
            let (_, changes) = run_click(Some(styled.clone()), seg_node(clicked), state.clone());
            let writes = restyle_writes(&changes);
            assert_eq!(writes.len(), 2 * n);

            for i in 0..n {
                let fresh = build_segment_style(i == clicked, i == 0, i + 1 == n);
                assert_eq!(
                    writes[2 * i],
                    (
                        seg_node(i),
                        "bg",
                        background_color(&fresh).expect("background")
                    ),
                    "clicked={clicked}: segment {i} background"
                );
                assert_eq!(
                    writes[2 * i + 1],
                    (
                        seg_node(i),
                        "text",
                        text_color(&fresh).expect("text colour")
                    ),
                    "clicked={clicked}: segment {i} text colour"
                );
            }
        }
    }

    #[test]
    fn click_invokes_the_user_callback_with_the_updated_state() {
        let mut log = RefAny::new(IndexLog { seen: Vec::new() });
        let seg =
            Segmented::create(n_labels(4)).with_on_change(log.clone(), change_cb(record_index));
        let (styled, state) = flatten(seg);

        let (update, changes) = run_click(Some(styled.clone()), seg_node(2), state.clone());
        assert_eq!(
            update,
            Update::RefreshDom,
            "the user's Update must propagate"
        );
        assert_eq!(
            log_indices(&mut log),
            vec![2],
            "the callback sees the *new* index"
        );
        assert_eq!(
            restyle_writes(&changes).len(),
            8,
            "the restyle must still run"
        );

        // A second click updates the shared state again — the index is not sticky.
        let (_, _) = run_click(Some(styled), seg_node(0), state.clone());
        assert_eq!(log_indices(&mut log), vec![2, 0]);

        let mut state = state;
        assert_eq!(
            selected_index_of(&mut state),
            0,
            "the state holds the *last* clicked index"
        );
    }

    #[test]
    fn click_propagates_every_update_variant_unchanged() {
        for (cb, expected) in [
            (change_cb(change_do_nothing), Update::DoNothing),
            (change_cb(change_refresh_all), Update::RefreshDomAllWindows),
        ] {
            let seg = Segmented::create(labels(&["a", "b"])).with_on_change(RefAny::new(0u8), cb);
            let (styled, state) = flatten(seg);
            let (update, changes) = run_click(Some(styled), seg_node(1), state);
            assert_eq!(update, expected);
            assert_eq!(
                restyle_writes(&changes).len(),
                4,
                "the restyle runs regardless of what the user returns"
            );
        }
    }

    #[test]
    fn click_restyles_even_without_a_user_callback() {
        let (styled, state) = flatten(Segmented::create(labels(&["a", "b"])));
        let (update, changes) = run_click(Some(styled), seg_node(0), state);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            restyle_writes(&changes),
            vec![
                (seg_node(0), "bg", SEG_SELECTED_BG_COLOR),
                (seg_node(0), "text", SEG_SELECTED_TEXT),
                (seg_node(1), "bg", SEG_UNSELECTED_BG_COLOR),
                (seg_node(1), "text", SEG_UNSELECTED_TEXT),
            ],
            "selection feedback must not depend on the user wiring a callback"
        );
    }

    #[test]
    fn click_on_a_single_segment_control_stays_at_zero() {
        let (styled, state) = flatten(Segmented::create(labels(&["only"])));
        let mut probe = state.clone();
        let (update, changes) = run_click(Some(styled), seg_node(0), state);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(selected_index_of(&mut probe), 0);
        assert_eq!(
            restyle_writes(&changes),
            vec![
                (seg_node(0), "bg", SEG_SELECTED_BG_COLOR),
                (seg_node(0), "text", SEG_SELECTED_TEXT)
            ]
        );
    }

    #[test]
    fn click_on_the_root_node_does_nothing() {
        // The container has no parent -> the handler must bail, not index into nothing.
        let (styled, state) = flatten(Segmented::create(labels(&["a", "b"])));
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), 0, state);

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "nothing may be restyled when the click is not on a segment"
        );
        assert_eq!(selected_index_of(&mut probe), 0, "state must be untouched");
    }

    #[test]
    fn click_on_an_out_of_range_node_does_nothing() {
        let (styled, state) = flatten(Segmented::create(labels(&["a", "b"])));
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), 9999, state);

        assert_eq!(
            update,
            Update::DoNothing,
            "a hit node outside the tree must not panic"
        );
        assert!(changes.is_empty());
        assert_eq!(selected_index_of(&mut probe), 0);
    }

    #[test]
    fn click_with_no_layout_result_does_nothing() {
        let dom = Segmented::create(labels(&["a", "b"])).dom();
        let state = segment_state(&dom, 0);

        let (update, changes) = run_click(None, 1, state);

        assert_eq!(
            update,
            Update::DoNothing,
            "an empty LayoutWindow must be handled, not unwrapped"
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn click_with_a_foreign_payload_does_nothing() {
        // Wrong type in the RefAny: the downcast fails, so the handler must bail
        // *before* restyling — otherwise the DOM would show a selection the state
        // never recorded.
        let (styled, _) = flatten(Segmented::create(labels(&["a", "b"])));
        let (update, changes) = run_click(Some(styled), 1, RefAny::new(0u32));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a failed downcast must not leave a half-applied restyle"
        );
    }

    #[test]
    fn click_with_the_state_already_borrowed_does_nothing() {
        let (styled, state) = flatten(Segmented::create(labels(&["a", "b"])));

        // A live mutable borrow on a sibling clone: `downcast_mut` inside the
        // handler must fail (returning DoNothing) instead of aliasing `&mut`.
        let mut held = state.clone();
        let guard = held
            .downcast_mut::<SegmentedStateWrapper>()
            .expect("first borrow succeeds");

        let (update, changes) = run_click(Some(styled), 1, state);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        drop(guard);
    }

    #[test]
    fn click_holds_the_state_borrow_across_the_user_callback() {
        // The handler invokes the user callback while its own `downcast_mut` on
        // the state is still live. A user callback that re-enters the *same*
        // state `RefAny` is therefore refused — it must get `None` back rather
        // than a second aliasing borrow (or a panic).
        //
        // NOTE: probe <-> state form a RefAny reference cycle, so this fixture
        // leaks. That is deliberate and harmless for a single test.
        let mut probe = RefAny::new(ReentrantProbe {
            state: RefAny::new(0u8),
            saw_index: Some(usize::MAX),
            calls: 0,
        });
        let state = RefAny::new(SegmentedStateWrapper {
            inner: SegmentedState { selected_index: 0 },
            on_change: Some(SegmentedOnChange {
                callback: change_cb(probe_state_reentrantly),
                refany: probe.clone(),
            })
            .into(),
        });
        {
            let mut p = probe
                .downcast_mut::<ReentrantProbe>()
                .expect("ReentrantProbe");
            p.state = state.clone();
        }

        let styled = StyledDom::create_from_dom(Segmented::create(labels(&["a", "b"])).dom());
        let (update, changes) = run_click(Some(styled), seg_node(1), state.clone());

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            restyle_writes(&changes).len(),
            4,
            "the restyle must still run afterwards"
        );

        let p = probe
            .downcast_ref::<ReentrantProbe>()
            .expect("ReentrantProbe");
        assert_eq!(p.calls, 1, "the user callback must have run exactly once");
        assert_eq!(
            p.saw_index, None,
            "a re-entrant read of the state must be refused, not aliased"
        );
    }

    #[test]
    fn click_indices_stay_within_the_label_count() {
        // The index is derived from the sibling position, so it can never address
        // past the last rendered segment however many there are.
        let n = 128;
        let (styled, state) = flatten(Segmented::create(n_labels(n)));

        for i in [0usize, 1, n / 2 - 1, n - 2, n - 1] {
            let mut probe = state.clone();
            let (_, changes) = run_click(Some(styled.clone()), seg_node(i), state.clone());
            let idx = selected_index_of(&mut probe);
            assert_eq!(idx, i, "node {} sits at sibling position {i}", seg_node(i));
            assert!(
                idx < n,
                "the reported index must always address a real label"
            );
            assert_eq!(restyle_writes(&changes).len(), 2 * n);
        }
    }

    #[test]
    fn click_recovers_a_state_left_out_of_range_by_the_setter() {
        // `set_selected_index(usize::MAX)` renders nothing selected; the first
        // click must snap the state back to a real, in-range segment.
        let seg = Segmented::create(n_labels(3)).with_selected_index(usize::MAX);
        let (styled, state) = flatten(seg);
        let mut probe = state.clone();

        let (_, changes) = run_click(Some(styled), seg_node(1), state);

        assert_eq!(selected_index_of(&mut probe), 1);
        assert_eq!(
            restyle_writes(&changes),
            vec![
                (seg_node(0), "bg", SEG_UNSELECTED_BG_COLOR),
                (seg_node(0), "text", SEG_UNSELECTED_TEXT),
                (seg_node(1), "bg", SEG_SELECTED_BG_COLOR),
                (seg_node(1), "text", SEG_SELECTED_TEXT),
                (seg_node(2), "bg", SEG_UNSELECTED_BG_COLOR),
                (seg_node(2), "text", SEG_UNSELECTED_TEXT),
            ]
        );
    }
}

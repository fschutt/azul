//! Pagination widget — a page-number navigator: `Prev`, a joined row of
//! page-number buttons, then `Next`. A near-clone of
//! [`crate::widgets::segmented::Segmented`] (a joined button bar whose clicked
//! item is derived from sibling position and whose active item is live-restyled
//! via `set_css_property`), specialised to page navigation.
//!
//! State is `{ current_page, total_pages }` (`current_page` is 1-based). Clicking
//! a page button selects it; clicking `Prev`/`Next` steps one page within
//! `[1, total_pages]`. Any change updates `current_page`, invokes the optional
//! `on_change(state)`, and live-restyles every button (the active page gets the
//! accent fill + white text; the others the neutral fill). `Prev`/`Next` show a
//! muted "disabled" text colour (style only) when `current_page` is at the
//! respective end; clicking a disabled end (or the already-current page) is a
//! no-op (returns `Update::DoNothing`, fires no callback).
//!
//! Index derivation: the children are `[Prev, page1 … pageN, Next]`, so page `p`
//! sits at sibling position `p`, `Prev` at position `0` and `Next` at the last
//! position. The handler reads the clicked node's position and the live child
//! count, so it stays correct regardless of `total_pages` drift.
//!
//! Key types: [`Pagination`], [`PaginationState`], [`PaginationOnChange`].

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
            LayoutJustifyContent, LayoutMinWidth, LayoutPaddingBottom, LayoutPaddingLeft,
            LayoutPaddingRight, LayoutPaddingTop,
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
    AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static PAGINATION_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-pagination"))];
static PAGINATION_PAGE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-pagination-page",
))];
static PAGINATION_NAV_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-pagination-nav",
))];

const PREV_LABEL: AzString = AzString::from_const_str("Prev");
const NEXT_LABEL: AzString = AzString::from_const_str("Next");

/// Callback function type invoked when the current page changes.
pub type PaginationOnChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, PaginationState) -> Update;
impl_widget_callback!(
    PaginationOnChange,
    OptionPaginationOnChange,
    PaginationOnChangeCallback,
    PaginationOnChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        PaginationOnChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: PAGINATION_ON_CHANGE_INVOKER,
    invoker_ty:     AzPaginationOnChangeCallbackInvoker,
    thunk_fn:       az_pagination_on_change_callback_thunk,
    setter_fn:      AzApp_setPaginationOnChangeCallbackInvoker,
    from_handle_fn: AzPaginationOnChangeCallback_createFromHostHandle,
    extra_args:     [ state: PaginationState ],
}

/// A `Prev` / page-numbers / `Next` page navigator with a change callback.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Pagination {
    pub pagination_state: PaginationStateWrapper,
    /// Style for the row container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct PaginationStateWrapper {
    /// The current page + total page count.
    pub inner: PaginationState,
    /// Optional: function to call when the current page changes.
    pub on_change: OptionPaginationOnChange,
}

/// State of a [`Pagination`]: the current (1-based) page and the total page count.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct PaginationState {
    /// The 1-based index of the current page.
    pub current_page: usize,
    /// The total number of pages.
    pub total_pages: usize,
}

// ---- colours (mirroring segmented's palette) ----
/// Page border colour (#ced4da).
const PAGE_BORDER_COLOR: ColorU = ColorU {
    r: 206,
    g: 212,
    b: 218,
    a: 255,
};
/// Active-page background (#0d6efd, accent blue).
const ACCENT_BG_COLOR: ColorU = ColorU {
    r: 13,
    g: 110,
    b: 253,
    a: 255,
};
/// Neutral (inactive) background (white).
const NEUTRAL_BG_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Active-page text colour (white).
const ACTIVE_TEXT: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Neutral text colour (#212529, dark).
const NEUTRAL_TEXT: ColorU = ColorU {
    r: 33,
    g: 37,
    b: 41,
    a: 255,
};
/// Disabled (Prev/Next at a bound) text colour (#adb5bd, muted grey).
const DISABLED_TEXT: ColorU = ColorU {
    r: 173,
    g: 181,
    b: 189,
    a: 255,
};

const ACCENT_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ACCENT_BG_COLOR)];
const ACCENT_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(ACCENT_BG_ITEMS);
const NEUTRAL_BG_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(NEUTRAL_BG_COLOR)];
const NEUTRAL_BG: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(NEUTRAL_BG_ITEMS);

const PAGE_RADIUS: isize = 6;

/// Row container: a horizontal flex row that hugs its content.
static PAGINATION_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// Builds the style for one button. The active/disabled colours and the rounding
/// of the outer corners (only the first button — `Prev` — is rounded on the left,
/// only the last — `Next` — on the right) are position-dependent, so the style is
/// built at runtime (mirroring `segmented::build_segment_style`).
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[allow(clippy::fn_params_excessive_bools)] // independent boolean render flags, not a state enum
fn build_button_style(
    active: bool,
    disabled: bool,
    is_first: bool,
    is_last: bool,
) -> CssPropertyWithConditionsVec {
    let bg = if active { ACCENT_BG } else { NEUTRAL_BG };
    let text = if active {
        ACTIVE_TEXT
    } else if disabled {
        DISABLED_TEXT
    } else {
        NEUTRAL_TEXT
    };

    let mut v: Vec<CssPropertyWithConditions> = vec![
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
        // Keep single-digit page buttons from collapsing too narrow.
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
            36,
        ))),
        // padding: 6px 12px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(6),
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
        // top/bottom/right borders (the left border is added only for the first
        // button, so adjacent buttons share a single 1px separator)
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
            },
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
                inner: PAGE_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor {
                inner: PAGE_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor {
                inner: PAGE_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            13,
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
                inner: PAGE_BORDER_COLOR,
            }),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(
                PAGE_RADIUS,
            )),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(
                PAGE_RADIUS,
            )),
        ));
    }
    if is_last {
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(
                PAGE_RADIUS,
            )),
        ));
        v.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(
                PAGE_RADIUS,
            )),
        ));
    }

    CssPropertyWithConditionsVec::from_vec(v)
}

impl Pagination {
    /// Creates a pager for `total_pages` pages with `current_page` (1-based)
    /// selected. `current_page` is clamped into `[1, total_pages.max(1)]`.
    #[must_use]
    pub fn create(current_page: usize, total_pages: usize) -> Self {
        let total_pages = total_pages.max(1);
        let current_page = current_page.clamp(1, total_pages);
        Self {
            pagination_state: PaginationStateWrapper {
                inner: PaginationState {
                    current_page,
                    total_pages,
                },
                ..Default::default()
            },
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                PAGINATION_CONTAINER_STYLE,
            ),
        }
    }

    /// Sets the current (1-based) page, clamped into `[1, total_pages]`.
    #[inline]
    pub fn set_current_page(&mut self, current_page: usize) {
        let total = self.pagination_state.inner.total_pages.max(1);
        self.pagination_state.inner.current_page = current_page.clamp(1, total);
    }

    /// Builder-style setter for the current page.
    #[inline]
    #[must_use]
    pub fn with_current_page(mut self, current_page: usize) -> Self {
        self.set_current_page(current_page);
        self
    }

    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(1, 1);
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_change<C: Into<PaginationOnChangeCallback>>(
        &mut self,
        data: RefAny,
        on_change: C,
    ) {
        self.pagination_state.on_change = Some(PaginationOnChange {
            callback: on_change.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use]
    pub fn with_on_change<C: Into<PaginationOnChangeCallback>>(
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

        let current = self.pagination_state.inner.current_page;
        let total = self.pagination_state.inner.total_pages;

        // One shared RefAny across every button's callback (RefAny::clone shares
        // the underlying state — same pattern as segmented/tabs/map).
        let state = RefAny::new(self.pagination_state);

        let make_button =
            |label: AzString, class: &'static [IdOrClass], style: CssPropertyWithConditionsVec| {
                crate::widgets::widget_p_with_text(label)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(class))
                    .with_css_props(style)
                    .with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::Click),
                            callback: CoreCallback {
                                cb: on_page_click as usize,
                                ctx: OptionRefAny::None,
                            },
                            refany: state.clone(),
                        }]
                        .into(),
                    )
                    .with_tab_index(TabIndex::Auto)
            // Role so the accessibility tree knows what this IS:
            // each page control is a button. The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::PushButton,
                ..Default::default()
            })
            };

        let mut children: Vec<Dom> = Vec::with_capacity(total.saturating_add(2));

        // Prev (first, left-rounded; disabled-look at page 1).
        children.push(make_button(
            PREV_LABEL,
            PAGINATION_NAV_CLASS,
            build_button_style(false, current <= 1, true, false),
        ));

        // Page-number buttons 1..=total.
        for page in 1..=total {
            children.push(make_button(
                AzString::from(format!("{page}").as_str()),
                PAGINATION_PAGE_CLASS,
                build_button_style(page == current, false, false, false),
            ));
        }

        // Next (last, right-rounded; disabled-look at the final page).
        children.push(make_button(
            NEXT_LABEL,
            PAGINATION_NAV_CLASS,
            build_button_style(false, current >= total, false, true),
        ));

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(PAGINATION_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for Pagination {
    fn default() -> Self {
        Self::create(1, 1)
    }
}

/// Click handler shared by all buttons. Resolves the clicked button from its
/// sibling position (`Prev`=0, page `p`=`p`, `Next`=last), computes the new page
/// within bounds, and — only if it actually changed — updates the state, invokes
/// the user callback, and live-restyles every button.
extern "C" fn on_page_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::dom::DomNodeId;

    let clicked = info.get_hit_node();
    let Some(parent) = info.get_parent(clicked) else {
        return Update::DoNothing;
    };

    // Collect the buttons in document order: [Prev, page1 … pageN, Next].
    let mut buttons: Vec<DomNodeId> = Vec::new();
    let mut cur = info.get_first_child(parent);
    while let Some(node) = cur {
        buttons.push(node);
        cur = info.get_next_sibling(node);
    }
    let n = buttons.len();
    if n < 2 {
        return Update::DoNothing;
    }
    // Page buttons occupy positions 1..=total; Prev=0, Next=n-1.
    let total = n - 2;

    let Some(pos) = buttons.iter().position(|b| *b == clicked) else {
        return Update::DoNothing;
    };

    let current = {
        let Some(pg) = data.downcast_ref::<PaginationStateWrapper>() else {
            return Update::DoNothing;
        };
        pg.inner.current_page
    };

    let new_page = if pos == 0 {
        // Prev
        if current > 1 {
            current - 1
        } else {
            current
        }
    } else if pos == n - 1 {
        // Next
        if current < total {
            current + 1
        } else {
            current
        }
    } else {
        // A page-number button: its 1-based page equals its sibling position.
        pos
    };

    if new_page == current {
        // Clicked the current page, or a disabled Prev/Next at a bound.
        return Update::DoNothing;
    }

    let result = {
        let Some(mut pg) = data.downcast_mut::<PaginationStateWrapper>() else {
            return Update::DoNothing;
        };
        pg.inner.current_page = new_page;
        let inner = pg.inner;
        let pg = &mut *pg;
        match pg.on_change.as_mut() {
            Some(PaginationOnChange { callback, refany }) => {
                (callback.cb)(refany.clone(), info, inner)
            }
            None => Update::DoNothing,
        }
    };

    // Live-restyle: active page gets the accent fill + light text; Prev/Next show
    // the muted disabled text at their bounds; everything else is neutral.
    for (i, node) in buttons.iter().enumerate() {
        let (bg, text) = if i == 0 {
            // Prev
            let disabled = new_page <= 1;
            (
                NEUTRAL_BG,
                if disabled {
                    DISABLED_TEXT
                } else {
                    NEUTRAL_TEXT
                },
            )
        } else if i == n - 1 {
            // Next
            let disabled = new_page >= total;
            (
                NEUTRAL_BG,
                if disabled {
                    DISABLED_TEXT
                } else {
                    NEUTRAL_TEXT
                },
            )
        } else if i == new_page {
            (ACCENT_BG, ACTIVE_TEXT)
        } else {
            (NEUTRAL_BG, NEUTRAL_TEXT)
        };
        info.set_css_property(*node, CssProperty::const_background_content(bg));
        info.set_css_property(
            *node,
            CssProperty::const_text_color(StyleTextColor { inner: text }),
        );
    }

    result
}

impl From<Pagination> for Dom {
    fn from(p: Pagination) -> Self {
        p.dom()
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
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
        props::{
            basic::{length::SizeMetric, pixel::PixelValue},
            property::CssPropertyType,
        },
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

    /// The declared properties of a style vec, in declaration order.
    fn declared(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The declared properties of a rendered node's inline style, in declaration
    /// order (`with_css_props` folds the vec into a `Css`; this reads it back).
    fn inline_props(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

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

    /// The one text colour a button declares (asserting there is at most one — a
    /// second declaration would silently shadow the first at cascade time).
    fn text_color(props: &[CssProperty]) -> Option<ColorU> {
        let found: Vec<ColorU> = props
            .iter()
            .filter_map(|p| match p {
                CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
                _ => None,
            })
            .collect();
        assert!(
            found.len() <= 1,
            "a button must declare at most one text colour"
        );
        found.first().copied()
    }

    /// The one flat background colour a button declares.
    fn background_color(props: &[CssProperty]) -> Option<ColorU> {
        let found: Vec<&StyleBackgroundContentVec> = props
            .iter()
            .filter_map(|p| match p {
                CssProperty::BackgroundContent(v) => v.get_property(),
                _ => None,
            })
            .collect();
        assert!(
            found.len() <= 1,
            "a button must declare at most one background"
        );
        let bg: &StyleBackgroundContentVec = *found.first()?;
        assert_eq!(
            bg.as_ref().len(),
            1,
            "a button must declare exactly one background layer"
        );
        match &bg.as_ref()[0] {
            StyleBackgroundContent::Color(c) => Some(*c),
            other => panic!("pagination background is not a flat colour: {other:?}"),
        }
    }

    /// The `f32` of a `PixelValue`, asserting the length is an absolute `px`. An
    /// `em`/`%` slipping into the button chrome would resolve against the parent
    /// font/box instead of the intended fixed padding, border or radius.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "pagination geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    /// Every length a style declares (min-width, paddings, border widths, font
    /// size, corner radii).
    fn pixel_values(props: &[CssProperty]) -> Vec<PixelValue> {
        props
            .iter()
            .filter_map(|p| match p {
                CssProperty::MinWidth(v) => v.get_property().map(|x| x.inner),
                CssProperty::PaddingTop(v) => v.get_property().map(|x| x.inner),
                CssProperty::PaddingBottom(v) => v.get_property().map(|x| x.inner),
                CssProperty::PaddingLeft(v) => v.get_property().map(|x| x.inner),
                CssProperty::PaddingRight(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderTopWidth(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderBottomWidth(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderLeftWidth(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderRightWidth(v) => v.get_property().map(|x| x.inner),
                CssProperty::FontSize(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderTopLeftRadius(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderBottomLeftRadius(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderTopRightRadius(v) => v.get_property().map(|x| x.inner),
                CssProperty::BorderBottomRightRadius(v) => v.get_property().map(|x| x.inner),
                _ => None,
            })
            .collect()
    }

    /// The four corner radii a style declares, as `(top-left, bottom-left,
    /// top-right, bottom-right)`; `None` where the corner is left square.
    fn radii(props: &[CssProperty]) -> (Option<f32>, Option<f32>, Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| props.iter().find_map(f);
        (
            find(&|p| match p {
                CssProperty::BorderTopLeftRadius(v) => v.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderBottomLeftRadius(v) => v.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderTopRightRadius(v) => v.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::BorderBottomRightRadius(v) => v.get_property().map(|r| px(&r.inner)),
                _ => None,
            }),
        )
    }

    /// Whether the style declares a *left* border on all three of width/style/colour
    /// — the "I am the first button in the joined bar" marker.
    fn has_left_border(props: &[CssProperty]) -> (bool, bool, bool) {
        (
            props
                .iter()
                .any(|p| matches!(p, CssProperty::BorderLeftWidth(_))),
            props
                .iter()
                .any(|p| matches!(p, CssProperty::BorderLeftStyle(_))),
            props
                .iter()
                .any(|p| matches!(p, CssProperty::BorderLeftColor(_))),
        )
    }

    /// Every `(active, disabled, is_first, is_last)` the builder can be handed.
    fn all_flag_combinations() -> Vec<(bool, bool, bool, bool)> {
        let mut out = Vec::with_capacity(16);
        for a in [false, true] {
            for d in [false, true] {
                for f in [false, true] {
                    for l in [false, true] {
                        out.push((a, d, f, l));
                    }
                }
            }
        }
        out
    }

    /// A `RefAny` payload recording every state a user `on_change` observes.
    struct ChangeLog {
        seen: Vec<PaginationState>,
    }

    extern "C" fn record_change(
        mut data: RefAny,
        _: CallbackInfo,
        state: PaginationState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ChangeLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn change_do_nothing(_: RefAny, _: CallbackInfo, _: PaginationState) -> Update {
        Update::DoNothing
    }

    extern "C" fn change_refresh_all(_: RefAny, _: CallbackInfo, _: PaginationState) -> Update {
        Update::RefreshDomAllWindows
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn cb(f: PaginationOnChangeCallbackType) -> PaginationOnChangeCallback {
        f.into()
    }

    fn logged(data: &mut RefAny) -> Vec<PaginationState> {
        data.downcast_ref::<ChangeLog>()
            .expect("payload must still be a ChangeLog")
            .seen
            .clone()
    }

    fn current_page_of(data: &mut RefAny) -> usize {
        data.downcast_ref::<PaginationStateWrapper>()
            .expect("payload must still be a PaginationStateWrapper")
            .inner
            .current_page
    }

    /// The shared state `RefAny` carried by button `i`'s click callback.
    fn button_state(dom: &Dom, i: usize) -> RefAny {
        dom.children.as_ref()[i]
            .root
            .get_callbacks()
            .as_ref()
            .first()
            .expect("every pagination button carries a click callback")
            .refany
            .clone()
    }

    /// Flattened node ids. Every button is a `<p>` wrapping one bare text node,
    /// so depth-first pre-order lays them out as
    /// `0 root / 1 Prev <p> / 2 Prev text / 3 page1 <p> / 4 page1 text / …`.
    /// The callbacks sit on the `<p>`s.
    const PREV_NODE: usize = 1;
    /// Flattened node id of page `p` (1-based).
    const fn page_node(p: usize) -> usize {
        2 * p + 1
    }
    /// Flattened node id of the `Next` button for a `total`-page pager.
    const fn next_node(total: usize) -> usize {
        2 * total + 3
    }

    /// A `DomLayoutResult` with an *empty* layout tree: `on_page_click` only walks
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

    /// Flattens `p.dom()` and hands back the shared state `RefAny` its buttons carry.
    fn flatten(p: Pagination) -> (StyledDom, RefAny) {
        let dom = p.dom();
        let state = button_state(&dom, 0);
        (StyledDom::create_from_dom(dom), state)
    }

    /// Invokes `on_page_click` against a `LayoutWindow` holding `styled` (or nothing
    /// at all, when `styled` is `None`), with flattened node `hit` as the hit node.
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

        let update = on_page_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// Decodes a live-restyle transaction into `(node index, background, text)`
    /// triples — the handler pushes exactly one background and one colour write per
    /// button, in that order.
    fn restyle(changes: &[CallbackChange]) -> Vec<(usize, ColorU, ColorU)> {
        assert_eq!(
            changes.len() % 2,
            0,
            "a restyle pass writes background+colour in pairs"
        );
        let decode = |c: &CallbackChange| -> (usize, CssProperty) {
            match c {
                CallbackChange::ChangeNodeCssProperties {
                    dom_id,
                    node_id,
                    properties,
                } => {
                    assert_eq!(*dom_id, DomId::ROOT_ID, "restyle must stay in the root DOM");
                    assert_eq!(
                        properties.as_ref().len(),
                        1,
                        "set_css_property writes exactly one property per change"
                    );
                    (node_id.index(), properties.as_ref()[0].clone())
                }
                other => panic!("unexpected change pushed by on_page_click: {other:?}"),
            }
        };
        changes
            .chunks(2)
            .map(|pair| {
                let (bg_node, bg_prop) = decode(&pair[0]);
                let (fg_node, fg_prop) = decode(&pair[1]);
                assert_eq!(bg_node, fg_node, "both writes must target the same button");
                let bg = background_color(&[bg_prop])
                    .expect("the first write of each pair is the background");
                let fg =
                    text_color(&[fg_prop]).expect("the second write of each pair is the colour");
                (bg_node, bg, fg)
            })
            .collect()
    }

    // ==================================================================
    // build_button_style
    // ==================================================================

    #[test]
    fn build_button_style_never_panics_and_is_deterministic() {
        for (a, d, f, l) in all_flag_combinations() {
            let style = build_button_style(a, d, f, l);
            assert!(
                !style.as_ref().is_empty(),
                "({a},{d},{f},{l}) produced an empty style"
            );
            assert_eq!(
                declared(&style),
                declared(&build_button_style(a, d, f, l)),
                "({a},{d},{f},{l}) is not a pure function of its flags"
            );
        }
    }

    #[test]
    fn build_button_style_declares_every_property_at_most_once() {
        // A duplicate declaration would silently shadow the earlier one at cascade
        // time, making the widget's look depend on declaration order.
        for (a, d, f, l) in all_flag_combinations() {
            let props = declared(&build_button_style(a, d, f, l));
            let mut types: Vec<CssPropertyType> = props.iter().map(CssProperty::get_type).collect();
            let len = types.len();
            types.sort_unstable();
            types.dedup();
            assert_eq!(
                types.len(),
                len,
                "duplicate declaration for ({a},{d},{f},{l})"
            );
        }
    }

    #[test]
    fn build_button_style_is_unconditional() {
        // Every declaration is `simple` — a stray `apply_if` would make the button
        // silently lose its fill under some pseudo-state.
        for (a, d, f, l) in all_flag_combinations() {
            for p in build_button_style(a, d, f, l).as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "({a},{d},{f},{l}) declared a conditional property: {:?}",
                    p.property
                );
            }
        }
    }

    #[test]
    fn build_button_style_active_wins_over_disabled_for_the_text_colour() {
        // `active` is checked first, so an active-*and*-disabled button reads as
        // active; only a non-active disabled button gets the muted grey.
        for (flags, expected) in [
            ((true, true), ACTIVE_TEXT),
            ((true, false), ACTIVE_TEXT),
            ((false, true), DISABLED_TEXT),
            ((false, false), NEUTRAL_TEXT),
        ] {
            let (a, d) = flags;
            let props = declared(&build_button_style(a, d, false, false));
            assert_eq!(
                text_color(&props),
                Some(expected),
                "active={a} disabled={d} picked the wrong text colour"
            );
        }
    }

    #[test]
    fn build_button_style_paints_the_accent_fill_only_when_active() {
        for (a, d, f, l) in all_flag_combinations() {
            let props = declared(&build_button_style(a, d, f, l));
            let expected = if a { ACCENT_BG_COLOR } else { NEUTRAL_BG_COLOR };
            assert_eq!(
                background_color(&props),
                Some(expected),
                "({a},{d},{f},{l}) has the wrong fill — `disabled` must not tint the background"
            );
        }
    }

    #[test]
    fn build_button_style_rounds_only_the_outer_corners() {
        for (a, d, f, l) in all_flag_combinations() {
            let props = declared(&build_button_style(a, d, f, l));
            let (tl, bl, tr, br) = radii(&props);
            let r = PAGE_RADIUS as f32;

            assert_eq!(
                tl,
                if f { Some(r) } else { None },
                "top-left for is_first={f}"
            );
            assert_eq!(
                bl,
                if f { Some(r) } else { None },
                "bottom-left for is_first={f}"
            );
            assert_eq!(
                tr,
                if l { Some(r) } else { None },
                "top-right for is_last={l}"
            );
            assert_eq!(
                br,
                if l { Some(r) } else { None },
                "bottom-right for is_last={l}"
            );
        }
    }

    #[test]
    fn build_button_style_gives_only_the_first_button_a_left_border() {
        // Adjacent buttons must share a single 1px separator: everyone draws a right
        // border, only the leftmost also draws a left one.
        for (a, d, f, l) in all_flag_combinations() {
            let props = declared(&build_button_style(a, d, f, l));
            assert_eq!(
                has_left_border(&props),
                (f, f, f),
                "left border must be present iff is_first ({a},{d},{f},{l})"
            );
            assert!(
                props
                    .iter()
                    .any(|p| matches!(p, CssProperty::BorderRightWidth(_))),
                "every button draws its own right border"
            );
        }
    }

    #[test]
    fn build_button_style_property_count_is_purely_position_dependent() {
        // The colour flags must not add or drop declarations — only the position
        // flags do (left border + 2 radii for first, 2 radii for last).
        let base = build_button_style(false, false, false, false)
            .as_ref()
            .len();
        for (a, d) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(
                build_button_style(a, d, false, false).as_ref().len(),
                base,
                "colour flags changed the declaration count"
            );
            assert_eq!(
                build_button_style(a, d, true, false).as_ref().len(),
                base + 5,
                "is_first must add exactly left width/style/colour + 2 radii"
            );
            assert_eq!(
                build_button_style(a, d, false, true).as_ref().len(),
                base + 2,
                "is_last must add exactly 2 radii"
            );
            assert_eq!(
                build_button_style(a, d, true, true).as_ref().len(),
                base + 7,
                "a single-button bar is first *and* last"
            );
        }
    }

    #[test]
    fn build_button_style_uses_only_absolute_px_lengths() {
        for (a, d, f, l) in all_flag_combinations() {
            let props = declared(&build_button_style(a, d, f, l));
            let lengths = pixel_values(&props);
            assert!(!lengths.is_empty(), "a button must declare some geometry");
            for pv in &lengths {
                let v = px(pv); // asserts SizeMetric::Px
                assert!(v.is_finite(), "non-finite length {v} in ({a},{d},{f},{l})");
                assert!(v >= 0.0, "negative length {v} in ({a},{d},{f},{l})");
            }
        }
    }

    #[test]
    fn button_palette_is_opaque_and_the_states_are_visually_distinct() {
        for (name, c) in [
            ("page border", PAGE_BORDER_COLOR),
            ("accent bg", ACCENT_BG_COLOR),
            ("neutral bg", NEUTRAL_BG_COLOR),
            ("active text", ACTIVE_TEXT),
            ("neutral text", NEUTRAL_TEXT),
            ("disabled text", DISABLED_TEXT),
        ] {
            assert_eq!(c.a, 255, "{name} must be fully opaque");
        }
        assert_ne!(
            ACCENT_BG_COLOR, NEUTRAL_BG_COLOR,
            "the active page must stand out"
        );
        assert_ne!(
            ACTIVE_TEXT, NEUTRAL_TEXT,
            "active text must read on the accent fill"
        );
        assert_ne!(
            NEUTRAL_TEXT, DISABLED_TEXT,
            "a disabled end must look disabled"
        );
        // The active-page text sits on the accent fill and must not equal it.
        assert_ne!(
            ACTIVE_TEXT, ACCENT_BG_COLOR,
            "active text would be invisible"
        );
        assert_ne!(
            NEUTRAL_TEXT, NEUTRAL_BG_COLOR,
            "neutral text would be invisible"
        );
        assert_ne!(
            DISABLED_TEXT, NEUTRAL_BG_COLOR,
            "disabled text would be invisible"
        );
    }

    // ==================================================================
    // Pagination::create
    // ==================================================================

    #[test]
    fn create_clamps_current_page_into_range() {
        for (cur, total, want_cur, want_total) in [
            // (input page, input total, expected page, expected total)
            (0usize, 0usize, 1usize, 1usize),
            (0, 1, 1, 1),
            (1, 0, 1, 1),
            (0, 5, 1, 5),
            (1, 5, 1, 5),
            (3, 5, 3, 5),
            (5, 5, 5, 5),
            (6, 5, 5, 5),
            (usize::MAX, 5, 5, 5),
            (usize::MAX, 1, 1, 1),
            (usize::MAX, 0, 1, 1),
            (5, usize::MAX, 5, usize::MAX),
            (0, usize::MAX, 1, usize::MAX),
            (usize::MAX, usize::MAX, usize::MAX, usize::MAX),
        ] {
            let p = Pagination::create(cur, total);
            assert_eq!(
                p.pagination_state.inner,
                PaginationState {
                    current_page: want_cur,
                    total_pages: want_total,
                },
                "create({cur}, {total})"
            );
        }
    }

    #[test]
    fn create_never_yields_a_zero_or_out_of_range_page() {
        // The 1-based invariant is what every consumer (dom(), the click handler,
        // the restyle pass) relies on; a 0 page would mean "no page is current".
        for total in [0usize, 1, 2, 3, 7, 64, 1024, usize::MAX - 1, usize::MAX] {
            for cur in [0usize, 1, 2, 63, 1023, usize::MAX - 1, usize::MAX] {
                let s = Pagination::create(cur, total).pagination_state.inner;
                assert!(s.total_pages >= 1, "create({cur}, {total}) left 0 pages");
                assert!(
                    s.current_page >= 1,
                    "create({cur}, {total}) produced page 0"
                );
                assert!(
                    s.current_page <= s.total_pages,
                    "create({cur}, {total}) escaped the upper bound"
                );
                // Clamping must never *invent* a page: an in-range input survives.
                if cur >= 1 && cur <= total {
                    assert_eq!(s.current_page, cur, "an in-range page must pass through");
                }
            }
        }
    }

    #[test]
    fn create_installs_no_callback_and_the_shared_const_container_style() {
        let p = Pagination::create(2, 4);
        assert!(
            p.pagination_state.on_change.as_ref().is_none(),
            "create must not install a callback"
        );
        assert_eq!(
            p.container_style.as_ref(),
            PAGINATION_CONTAINER_STYLE,
            "create must reuse the const container style"
        );
    }

    #[test]
    fn default_equals_create_one_one() {
        assert_eq!(Pagination::default(), Pagination::create(1, 1));
        let s = Pagination::default().pagination_state.inner;
        assert_eq!(s.current_page, 1);
        assert_eq!(s.total_pages, 1);
    }

    // ==================================================================
    // Pagination::set_current_page / with_current_page
    // ==================================================================

    #[test]
    fn set_current_page_clamps_into_range() {
        for (input, want) in [
            (0usize, 1usize),
            (1, 1),
            (4, 4),
            (7, 7),
            (8, 7),
            (usize::MAX, 7),
            (usize::MAX - 1, 7),
        ] {
            let mut p = Pagination::create(1, 7);
            p.set_current_page(input);
            assert_eq!(
                p.pagination_state.inner.current_page, want,
                "set_current_page({input}) on a 7-page pager"
            );
            assert_eq!(
                p.pagination_state.inner.total_pages, 7,
                "set_current_page must not touch total_pages"
            );
        }
    }

    #[test]
    fn set_current_page_survives_a_hand_corrupted_zero_total() {
        // `total_pages` is a pub field, so it can be zeroed behind the constructor's
        // back. `clamp(1, 0)` would panic (min > max); the `.max(1)` guard is what
        // prevents that.
        let mut p = Pagination::create(1, 3);
        p.pagination_state.inner.total_pages = 0;
        for input in [0usize, 1, 5, usize::MAX] {
            p.set_current_page(input);
            assert_eq!(
                p.pagination_state.inner.current_page, 1,
                "a 0-page pager must collapse every page to 1"
            );
        }
        assert_eq!(
            p.pagination_state.inner.total_pages, 0,
            "the setter must not silently repair total_pages"
        );
    }

    #[test]
    fn set_current_page_is_idempotent_and_reversible() {
        let mut p = Pagination::create(1, 10);
        for page in [1usize, 10, 5, 5, 1, 10] {
            p.set_current_page(page);
            let once = p.pagination_state.inner.current_page;
            p.set_current_page(page);
            assert_eq!(
                p.pagination_state.inner.current_page, once,
                "not idempotent"
            );
            assert_eq!(once, page);
        }
    }

    #[test]
    fn set_current_page_at_the_usize_max_total_does_not_overflow() {
        let mut p = Pagination::create(1, usize::MAX);
        p.set_current_page(usize::MAX);
        assert_eq!(p.pagination_state.inner.current_page, usize::MAX);
        p.set_current_page(0);
        assert_eq!(p.pagination_state.inner.current_page, 1);
    }

    #[test]
    fn with_current_page_matches_set_current_page_and_keeps_the_callback() {
        for input in [0usize, 1, 3, 9, usize::MAX] {
            let mut expected = Pagination::create(1, 6);
            expected.set_current_page(input);

            let got = Pagination::create(1, 6).with_current_page(input);
            assert_eq!(got, expected, "builder and setter must agree for {input}");
        }

        // The builder form must not drop an already-installed callback.
        let p = Pagination::create(1, 6)
            .with_on_change(
                RefAny::new(ChangeLog { seen: Vec::new() }),
                cb(record_change),
            )
            .with_current_page(4);
        assert_eq!(p.pagination_state.inner.current_page, 4);
        assert!(
            p.pagination_state.on_change.as_ref().is_some(),
            "with_current_page must not disturb the callback"
        );
    }

    // ==================================================================
    // Pagination::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_old_value_and_resets_self() {
        let mut p = Pagination::create(3, 9);
        let old = p.swap_with_default();

        assert_eq!(old.pagination_state.inner.current_page, 3);
        assert_eq!(old.pagination_state.inner.total_pages, 9);
        assert_eq!(
            p,
            Pagination::default(),
            "self must be left as a 1-of-1 pager"
        );
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_of_self() {
        let mut p = Pagination::create(1, 3).with_on_change(
            RefAny::new(ChangeLog { seen: Vec::new() }),
            cb(record_change),
        );

        let old = p.swap_with_default();
        assert!(
            old.pagination_state.on_change.as_ref().is_some(),
            "the callback must travel with the returned value"
        );
        assert!(
            p.pagination_state.on_change.as_ref().is_none(),
            "self must not keep a dangling reference to the moved-out callback"
        );
    }

    #[test]
    fn swap_with_default_is_stable_when_repeated() {
        let mut p = Pagination::create(2, 4);
        let _ = p.swap_with_default();
        for _ in 0..100 {
            let out = p.swap_with_default();
            assert_eq!(out, Pagination::default());
            assert_eq!(p, Pagination::default());
        }
    }

    #[test]
    fn swap_with_default_preserves_a_hand_corrupted_state_verbatim() {
        let mut p = Pagination::create(1, 4);
        p.pagination_state.inner.current_page = usize::MAX;
        p.pagination_state.inner.total_pages = 0;

        let old = p.swap_with_default();
        assert_eq!(
            old.pagination_state.inner,
            PaginationState {
                current_page: usize::MAX,
                total_pages: 0,
            },
            "the swap must move state out untouched (no clamping/rewriting)"
        );
        assert_eq!(p.pagination_state.inner.current_page, 1);
        assert_eq!(p.pagination_state.inner.total_pages, 1);
    }

    // ==================================================================
    // Pagination::set_on_change / with_on_change
    // ==================================================================

    #[test]
    fn with_on_change_installs_the_callback_and_touches_nothing_else() {
        let before = Pagination::create(2, 5);
        let after = Pagination::create(2, 5).with_on_change(
            RefAny::new(ChangeLog { seen: Vec::new() }),
            cb(record_change),
        );

        assert_eq!(
            after.pagination_state.inner, before.pagination_state.inner,
            "installing a callback must not disturb the page state"
        );
        assert_eq!(
            after.container_style, before.container_style,
            "installing a callback must not disturb the container style"
        );

        let installed = after
            .pagination_state
            .on_change
            .as_ref()
            .expect("with_on_change must install Some(..)");
        assert_eq!(installed.callback.cb as usize, record_change as usize);
    }

    #[test]
    fn set_on_change_overwrites_the_previous_callback_and_data() {
        let mut p = Pagination::create(1, 3);
        p.set_on_change(
            RefAny::new(ChangeLog { seen: Vec::new() }),
            cb(record_change),
        );
        p.set_on_change(RefAny::new(42u32), cb(change_do_nothing));

        let installed = p
            .pagination_state
            .on_change
            .as_mut()
            .expect("still Some after the overwrite");
        assert_eq!(
            installed.callback.cb as usize, change_do_nothing as usize,
            "the last set_on_change must win"
        );
        assert_eq!(
            installed.refany.downcast_ref::<u32>().map(|v| *v),
            Some(42),
            "the payload must be replaced along with the fn pointer"
        );
        assert!(
            installed.refany.downcast_ref::<ChangeLog>().is_none(),
            "the stale payload must be gone"
        );
    }

    #[test]
    fn generic_callback_conversion_round_trips_the_fn_pointer() {
        // The FFI path (`From<Callback>`) transmutes the fn pointer. The value must
        // round-trip bit-for-bit — a corrupted pointer would be an unconditional
        // jump into garbage at click time. (Never invoked here.)
        let raw = record_change as usize;
        let generic = Callback {
            cb: unsafe { core::mem::transmute::<usize, crate::callbacks::CallbackType>(raw) },
            ctx: OptionRefAny::None,
        };
        let converted: PaginationOnChangeCallback = generic.into();
        assert_eq!(converted.cb as usize, raw);
    }

    // ==================================================================
    // Pagination::dom
    // ==================================================================

    #[test]
    fn dom_lays_out_prev_then_pages_then_next() {
        for total in [1usize, 2, 3, 10, 99] {
            let dom = Pagination::create(1, total).dom();
            let children = dom.children.as_ref();

            assert_eq!(children.len(), total + 2, "Prev + {total} pages + Next");
            assert_eq!(text_of(&children[0]), Some("Prev"));
            assert_eq!(text_of(&children[total + 1]), Some("Next"));
            for (p, child) in children.iter().enumerate().take(total + 1).skip(1) {
                assert_eq!(
                    text_of(child),
                    Some(p.to_string().as_str()),
                    "page {p} must sit at sibling position {p}"
                );
            }

            assert!(dom.root.has_class("__azul-native-pagination"));
            assert!(
                dom.root.is_node_type(NodeType::Div),
                "the bar must be a div"
            );
            assert_eq!(classes(&children[0]), ["__azul-native-pagination-nav"]);
            assert_eq!(
                classes(&children[total + 1]),
                ["__azul-native-pagination-nav"]
            );
            assert_eq!(classes(&children[1]), ["__azul-native-pagination-page"]);
        }
    }

    #[test]
    fn dom_page_labels_are_plain_ascii_decimal() {
        let total = 1000;
        let dom = Pagination::create(1, total).dom();
        let children = dom.children.as_ref();
        for p in [1usize, 9, 10, 99, 100, 999, 1000] {
            let label = text_of(&children[p]).expect("page buttons are text nodes");
            assert_eq!(label, p.to_string(), "page {p} label");
            assert!(
                label.bytes().all(|b| b.is_ascii_digit()),
                "page {p} label {label:?} is not plain decimal"
            );
        }
    }

    #[test]
    fn dom_wires_one_mouseup_handler_per_button() {
        let total = 4;
        let dom = Pagination::create(2, total).dom();
        for (i, child) in dom.children.as_ref().iter().enumerate() {
            let cbs = child.root.get_callbacks();
            assert_eq!(
                cbs.as_ref().len(),
                1,
                "button {i} must carry exactly one handler"
            );
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::Click)
            );
            assert_eq!(cbs.as_ref()[0].callback.cb, on_page_click as usize);
            assert_eq!(
                child.root.get_tab_index(),
                Some(TabIndex::Auto),
                "button {i} must be keyboard-reachable"
            );
        }
        assert!(
            dom.root.get_callbacks().as_ref().is_empty(),
            "the container itself must not be clickable"
        );
    }

    #[test]
    fn dom_shares_one_state_refany_across_every_button() {
        let dom = Pagination::create(1, 4).dom();

        // Write through Prev's handle…
        let mut first = button_state(&dom, 0);
        {
            let mut w = first
                .downcast_mut::<PaginationStateWrapper>()
                .expect("button state must be a PaginationStateWrapper");
            w.inner.current_page = 3;
        }
        // …and read it back through Next's handle.
        let mut last = button_state(&dom, 5);
        assert_eq!(
            current_page_of(&mut last),
            3,
            "every button must observe the same shared state"
        );
    }

    #[test]
    fn dom_gives_separate_pagers_separate_state() {
        let a = Pagination::create(1, 3).dom();
        let b = Pagination::create(1, 3).dom();

        let mut a0 = button_state(&a, 0);
        {
            let mut w = a0.downcast_mut::<PaginationStateWrapper>().unwrap();
            w.inner.current_page = 3;
        }

        let mut b0 = button_state(&b, 0);
        assert_eq!(
            current_page_of(&mut b0),
            1,
            "two pagers must not alias one another's state"
        );
    }

    #[test]
    fn dom_marks_exactly_the_current_page_active() {
        for total in [1usize, 2, 5, 12] {
            for current in 1..=total {
                let dom = Pagination::create(current, total).dom();
                let children = dom.children.as_ref();

                let active: Vec<usize> = (0..children.len())
                    .filter(|i| {
                        background_color(&inline_props(&children[*i])) == Some(ACCENT_BG_COLOR)
                    })
                    .collect();
                assert_eq!(
                    active,
                    vec![current],
                    "exactly the current page carries the accent fill (total={total})"
                );
                assert_eq!(
                    text_color(&inline_props(&children[current])),
                    Some(ACTIVE_TEXT),
                    "the active page must use the light text"
                );
            }
        }
    }

    #[test]
    fn dom_mutes_prev_at_the_first_page_and_next_at_the_last() {
        let total = 5;
        for current in 1..=total {
            let dom = Pagination::create(current, total).dom();
            let children = dom.children.as_ref();

            let prev = text_color(&inline_props(&children[0]));
            let next = text_color(&inline_props(&children[total + 1]));

            assert_eq!(
                prev,
                Some(if current == 1 {
                    DISABLED_TEXT
                } else {
                    NEUTRAL_TEXT
                }),
                "Prev at page {current}/{total}"
            );
            assert_eq!(
                next,
                Some(if current == total {
                    DISABLED_TEXT
                } else {
                    NEUTRAL_TEXT
                }),
                "Next at page {current}/{total}"
            );
            // A muted end is a *style-only* signal — it stays clickable.
            assert_eq!(children[0].root.get_callbacks().as_ref().len(), 1);
            assert_eq!(children[total + 1].root.get_callbacks().as_ref().len(), 1);
        }
    }

    #[test]
    fn dom_rounds_only_the_two_outer_ends_of_the_bar() {
        let total = 4;
        let dom = Pagination::create(1, total).dom();
        let children = dom.children.as_ref();
        let r = PAGE_RADIUS as f32;

        assert_eq!(
            radii(&inline_props(&children[0])),
            (Some(r), Some(r), None, None),
            "Prev is rounded on the left only"
        );
        assert_eq!(
            radii(&inline_props(&children[total + 1])),
            (None, None, Some(r), Some(r)),
            "Next is rounded on the right only"
        );
        for (p, child) in children.iter().enumerate().take(total + 1).skip(1) {
            assert_eq!(
                radii(&inline_props(child)),
                (None, None, None, None),
                "interior page {p} must stay square"
            );
            assert_eq!(
                has_left_border(&inline_props(child)),
                (false, false, false),
                "only Prev draws a left border, so buttons share one separator"
            );
        }
        assert_eq!(
            has_left_border(&inline_props(&children[0])),
            (true, true, true),
            "Prev closes the left edge of the bar"
        );
    }

    #[test]
    fn dom_carries_the_container_style_and_the_button_styles_verbatim() {
        let p = Pagination::create(2, 3);
        let dom = p.dom();
        assert_eq!(
            inline_props(&dom),
            PAGINATION_CONTAINER_STYLE
                .iter()
                .map(|p| p.property.clone())
                .collect::<Vec<_>>(),
            "the container style must survive the Dom round-trip"
        );

        let children = dom.children.as_ref();
        assert_eq!(
            inline_props(&children[2]),
            declared(&build_button_style(true, false, false, false)),
            "the active page's style must match the builder output verbatim"
        );
        assert_eq!(
            inline_props(&children[0]),
            declared(&build_button_style(false, false, true, false)),
            "Prev's style must match the builder output verbatim"
        );
        // page 2 of 3, so Next is *not* at its bound and stays un-muted.
        assert_eq!(
            inline_props(&children[4]),
            declared(&build_button_style(false, false, false, true)),
            "Next's style must match the builder output verbatim"
        );
    }

    #[test]
    fn dom_estimated_total_children_matches_the_real_child_count() {
        // `estimated_total_children` is a cached count; if it under-counts,
        // `convert_dom_into_compact_dom` under-allocates.
        for total in [1usize, 2, 3, 8, 64, 257] {
            let dom = Pagination::create(1, total).dom();
            assert_eq!(dom.children.as_ref().len(), total + 2);
            assert_eq!(
                dom.estimated_total_children,
                2 * (total + 2),
                "cached descendant count desynced for total={total}"
            );
        }
    }

    #[test]
    fn dom_of_a_hand_zeroed_total_has_only_prev_and_next() {
        // `total_pages` is a pub field: zeroing it must yield an inert two-button
        // bar, not an empty/negative range or a panic.
        let mut p = Pagination::create(1, 3);
        p.pagination_state.inner.total_pages = 0;
        let dom = p.dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 2, "no pages => just Prev and Next");
        assert_eq!(text_of(&children[0]), Some("Prev"));
        assert_eq!(text_of(&children[1]), Some("Next"));
        // current(1) >= total(0), so Next reads as disabled too.
        assert_eq!(text_color(&inline_props(&children[1])), Some(DISABLED_TEXT));
    }

    #[test]
    fn dom_of_an_out_of_range_current_page_marks_nothing_active() {
        // Reachable only by writing the pub field directly. The bar must still
        // render deterministically instead of indexing out of bounds.
        let mut p = Pagination::create(1, 4);
        p.pagination_state.inner.current_page = 99;
        let dom = p.dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 6);
        assert!(
            (0..children.len())
                .all(|i| background_color(&inline_props(&children[i])) == Some(NEUTRAL_BG_COLOR)),
            "no page matches 99, so nothing is painted active"
        );
        assert_eq!(
            text_color(&inline_props(&children[0])),
            Some(NEUTRAL_TEXT),
            "Prev is live (99 > 1)"
        );
        assert_eq!(
            text_color(&inline_props(&children[5])),
            Some(DISABLED_TEXT),
            "Next is muted (99 >= 4)"
        );
    }

    #[test]
    fn dom_of_many_pages_flattens_without_panicking() {
        let total = 500;
        let styled = StyledDom::create_from_dom(Pagination::create(250, total).dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            2 * total + 5,
            "root + (Prev + {total} pages + Next), each a <p> wrapping one text node"
        );
    }

    #[test]
    fn from_pagination_for_dom_equals_dom() {
        let dom: Dom = Pagination::create(2, 3).into();
        assert_eq!(dom.children.as_ref().len(), 5);
        assert_eq!(text_of(&dom.children.as_ref()[0]), Some("Prev"));
    }

    // ==================================================================
    // on_page_click
    // ==================================================================

    #[test]
    fn click_next_advances_exactly_one_page() {
        let total = 5;
        let (styled, state) = flatten(Pagination::create(2, total));
        let mut state2 = state.clone();

        let (update, changes) = run_click(Some(styled), next_node(total), state);
        assert_eq!(
            update,
            Update::DoNothing,
            "no user callback => nothing to redraw"
        );
        assert_eq!(current_page_of(&mut state2), 3, "Next must step 2 -> 3");
        assert_eq!(
            restyle(&changes).len(),
            total + 2,
            "every button is restyled after a real change"
        );
    }

    #[test]
    fn click_prev_steps_back_exactly_one_page() {
        let total = 5;
        let (styled, state) = flatten(Pagination::create(4, total));
        let mut state2 = state.clone();

        let (_, _) = run_click(Some(styled), PREV_NODE, state);
        assert_eq!(current_page_of(&mut state2), 3, "Prev must step 4 -> 3");
    }

    #[test]
    fn click_a_page_number_jumps_straight_to_it() {
        let total = 8;
        for target in [1usize, 2, 7, 8] {
            let (styled, state) = flatten(Pagination::create(4, total));
            let mut state2 = state.clone();

            let (_, changes) = run_click(Some(styled), page_node(target), state);
            assert_eq!(
                current_page_of(&mut state2),
                target,
                "page button {target} sits at sibling position {target}"
            );
            assert!(!changes.is_empty(), "a real jump must restyle the bar");
        }
    }

    #[test]
    fn click_the_current_page_is_a_no_op() {
        let total = 6;
        let (styled, state) = flatten(Pagination::create(3, total));
        let mut state2 = state.clone();

        let (update, changes) = run_click(Some(styled), page_node(3), state);
        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "an unchanged page must not push a restyle transaction"
        );
        assert_eq!(current_page_of(&mut state2), 3);
    }

    #[test]
    fn click_prev_at_the_first_page_is_a_no_op() {
        let total = 4;
        let (styled, state) = flatten(Pagination::create(1, total));
        let mut state2 = state.clone();

        let (update, changes) = run_click(Some(styled), PREV_NODE, state);
        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a disabled end must fire nothing at all"
        );
        assert_eq!(
            current_page_of(&mut state2),
            1,
            "page 1 must not underflow to 0"
        );
    }

    #[test]
    fn click_next_at_the_last_page_is_a_no_op() {
        let total = 4;
        let (styled, state) = flatten(Pagination::create(total, total));
        let mut state2 = state.clone();

        let (update, changes) = run_click(Some(styled), next_node(total), state);
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(
            current_page_of(&mut state2),
            total,
            "must not run past the end"
        );
    }

    #[test]
    fn click_walks_the_whole_range_without_escaping_its_bounds() {
        let total = 6;
        let (styled, state) = flatten(Pagination::create(1, total));
        let mut probe = state.clone();

        // Press Next more often than there are pages.
        for step in 0..(total + 2) {
            let (_, _) = run_click(Some(styled.clone()), next_node(total), state.clone());
            let page = current_page_of(&mut probe);
            assert!(
                (1..=total).contains(&page),
                "page {page} escaped [1, {total}] after {step} Next presses"
            );
            assert_eq!(
                page,
                (step + 2).min(total),
                "Next must advance one at a time"
            );
        }
        assert_eq!(current_page_of(&mut probe), total);

        // …then all the way back down.
        for step in 0..(total + 2) {
            let (_, _) = run_click(Some(styled.clone()), PREV_NODE, state.clone());
            let page = current_page_of(&mut probe);
            assert!(
                (1..=total).contains(&page),
                "page {page} escaped [1, {total}]"
            );
            assert_eq!(page, total.saturating_sub(step + 1).max(1));
        }
        assert_eq!(current_page_of(&mut probe), 1);
    }

    #[test]
    fn click_on_a_one_page_pager_is_completely_inert() {
        let (styled, state) = flatten(Pagination::create(1, 1));
        let mut probe = state.clone();

        for hit in [PREV_NODE, page_node(1), next_node(1)] {
            let (update, changes) = run_click(Some(styled.clone()), hit, state.clone());
            assert_eq!(update, Update::DoNothing, "node {hit} on a 1-page pager");
            assert!(changes.is_empty(), "node {hit} must not restyle anything");
            assert_eq!(current_page_of(&mut probe), 1);
        }
    }

    #[test]
    fn click_on_a_hand_zeroed_pager_is_inert() {
        // total_pages = 0 => children are just [Prev, Next] (n == 2, total == 0).
        // The `n < 2` guard is not hit, so both ends must fall through the bounds
        // checks instead of computing a 0/underflowing page.
        let mut p = Pagination::create(1, 3);
        p.pagination_state.inner.total_pages = 0;
        let (styled, state) = flatten(p);
        let mut probe = state.clone();

        for hit in [1usize, 2] {
            let (update, changes) = run_click(Some(styled.clone()), hit, state.clone());
            assert_eq!(update, Update::DoNothing, "node {hit} on a 0-page pager");
            assert!(changes.is_empty());
            assert_eq!(current_page_of(&mut probe), 1);
        }
    }

    #[test]
    fn click_invokes_the_user_callback_with_the_new_state() {
        let total = 5;
        let mut log = RefAny::new(ChangeLog { seen: Vec::new() });
        let p = Pagination::create(1, total).with_on_change(log.clone(), cb(record_change));
        let (styled, state) = flatten(p);

        let (update, _) = run_click(Some(styled.clone()), page_node(4), state.clone());
        assert_eq!(
            update,
            Update::RefreshDom,
            "the user's Update must propagate"
        );
        assert_eq!(
            logged(&mut log),
            vec![PaginationState {
                current_page: 4,
                total_pages: total,
            }],
            "the callback sees the *new* page, with total_pages intact"
        );

        // A no-op click must not fire the callback again.
        let (update, _) = run_click(Some(styled.clone()), page_node(4), state.clone());
        assert_eq!(update, Update::DoNothing);
        assert_eq!(logged(&mut log).len(), 1, "an unchanged page fires nothing");

        let (_, _) = run_click(Some(styled), PREV_NODE, state);
        assert_eq!(
            logged(&mut log).len(),
            2,
            "a real change fires the callback again"
        );
        assert_eq!(logged(&mut log)[1].current_page, 3);
    }

    #[test]
    fn click_propagates_every_update_variant_unchanged() {
        for (callback, expected) in [
            (cb(change_do_nothing), Update::DoNothing),
            (cb(change_refresh_all), Update::RefreshDomAllWindows),
        ] {
            let p = Pagination::create(1, 3).with_on_change(RefAny::new(0u8), callback);
            let (styled, state) = flatten(p);
            let (update, _) = run_click(Some(styled), page_node(2), state);
            assert_eq!(update, expected);
        }
    }

    #[test]
    fn click_restyles_every_button_and_marks_only_the_new_page() {
        let total = 5;
        let new_page = 4;
        let (styled, state) = flatten(Pagination::create(1, total));
        let (_, changes) = run_click(Some(styled), page_node(new_page), state);

        let pass = restyle(&changes);
        assert_eq!(pass.len(), total + 2, "one pair of writes per button");

        for (i, (node, bg, fg)) in pass.iter().enumerate() {
            assert_eq!(
                *node,
                2 * i + 1,
                "buttons must be restyled in document order"
            );
            let (want_bg, want_fg) = if i == 0 {
                // Prev: live, because the new page is not 1.
                (NEUTRAL_BG_COLOR, NEUTRAL_TEXT)
            } else if i == total + 1 {
                // Next: live, because the new page is not the last.
                (NEUTRAL_BG_COLOR, NEUTRAL_TEXT)
            } else if i == new_page {
                (ACCENT_BG_COLOR, ACTIVE_TEXT)
            } else {
                (NEUTRAL_BG_COLOR, NEUTRAL_TEXT)
            };
            assert_eq!((*bg, *fg), (want_bg, want_fg), "button {i} after the jump");
        }
    }

    #[test]
    fn click_restyle_mutes_the_end_it_lands_on() {
        let total = 4;

        // Landing on page 1 must mute Prev…
        let (styled, state) = flatten(Pagination::create(3, total));
        let (_, changes) = run_click(Some(styled), page_node(1), state);
        let pass = restyle(&changes);
        assert_eq!(pass[0].2, DISABLED_TEXT, "Prev must go muted at page 1");
        assert_eq!(pass[total + 1].2, NEUTRAL_TEXT, "Next stays live at page 1");

        // …and landing on the last page must mute Next.
        let (styled, state) = flatten(Pagination::create(1, total));
        let (_, changes) = run_click(Some(styled), page_node(total), state);
        let pass = restyle(&changes);
        assert_eq!(pass[0].2, NEUTRAL_TEXT, "Prev is live at the last page");
        assert_eq!(
            pass[total + 1].2,
            DISABLED_TEXT,
            "Next must go muted at the end"
        );
    }

    #[test]
    fn click_on_the_root_node_does_nothing() {
        // The root has no parent -> the handler must bail before touching anything.
        let (styled, state) = flatten(Pagination::create(2, 4));
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), 0, state);
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(current_page_of(&mut probe), 2, "state must be untouched");
    }

    #[test]
    fn click_on_an_out_of_range_node_does_nothing() {
        let (styled, state) = flatten(Pagination::create(2, 4));
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled), 9999, state);
        assert_eq!(
            update,
            Update::DoNothing,
            "a hit node that isn't in the tree must not panic"
        );
        assert!(changes.is_empty());
        assert_eq!(current_page_of(&mut probe), 2);
    }

    #[test]
    fn click_with_no_layout_result_does_nothing() {
        let dom = Pagination::create(2, 4).dom();
        let state = button_state(&dom, 0);

        let (update, changes) = run_click(None, PREV_NODE, state);
        assert_eq!(
            update,
            Update::DoNothing,
            "an empty LayoutWindow must be handled, not unwrapped"
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn click_with_a_foreign_payload_does_nothing() {
        let (styled, _) = flatten(Pagination::create(2, 4));
        let (update, changes) = run_click(Some(styled), page_node(3), RefAny::new(0u32));
        assert_eq!(
            update,
            Update::DoNothing,
            "a failed downcast must bail cleanly"
        );
        assert!(
            changes.is_empty(),
            "no state change => no restyle, even for a foreign payload"
        );
    }

    #[test]
    fn click_with_the_state_already_borrowed_does_nothing() {
        let (styled, state) = flatten(Pagination::create(2, 4));

        // A live mutable borrow on a sibling clone: the handler's own `downcast_ref`
        // must fail (returning DoNothing) instead of aliasing `&mut`.
        let mut held = state.clone();
        let guard = held
            .downcast_mut::<PaginationStateWrapper>()
            .expect("first borrow succeeds");

        let (update, changes) = run_click(Some(styled), page_node(3), state);
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        drop(guard);
    }

    #[test]
    fn click_derives_the_page_count_from_the_live_dom_not_from_the_state() {
        // The doc promises the handler "stays correct regardless of total_pages
        // drift": it reads the child count, so a stale `total_pages` in the state
        // must not change where Next stops.
        let total = 3;
        let dom = Pagination::create(1, total).dom();
        let mut state = button_state(&dom, 0);

        // Corrupt the *shared* state after the DOM was built: the rendered bar
        // still has 3 page buttons.
        {
            let mut w = state
                .downcast_mut::<PaginationStateWrapper>()
                .expect("the shared payload is a PaginationStateWrapper");
            w.inner.total_pages = 999;
        }

        let styled = StyledDom::create_from_dom(dom);
        let mut probe = state.clone();

        for _ in 0..(total + 3) {
            let (_, _) = run_click(Some(styled.clone()), next_node(total), state.clone());
        }
        assert_eq!(
            current_page_of(&mut probe),
            total,
            "Next must stop at the last *rendered* page, not at the stale total"
        );
        assert_eq!(
            state
                .downcast_ref::<PaginationStateWrapper>()
                .expect("still a PaginationStateWrapper")
                .inner
                .total_pages,
            999,
            "the handler must not rewrite total_pages behind the caller's back"
        );
    }

    #[test]
    fn click_prev_from_an_out_of_range_page_steps_down_by_one() {
        // Only reachable by writing the pub `current_page` field. The handler has no
        // clamp of its own, so it walks down one step at a time rather than snapping
        // back into range — assert that documented-by-construction behaviour rather
        // than a silent repair.
        let total = 4;
        let mut p = Pagination::create(1, total);
        p.pagination_state.inner.current_page = 99;
        let (styled, state) = flatten(p);
        let mut probe = state.clone();

        let (update, changes) = run_click(Some(styled.clone()), PREV_NODE, state.clone());
        assert_eq!(update, Update::DoNothing, "no callback installed");
        assert_eq!(current_page_of(&mut probe), 98, "Prev steps 99 -> 98");
        // Nothing is in range, so the restyle marks no page active.
        let pass = restyle(&changes);
        assert_eq!(pass.len(), total + 2);
        assert!(
            pass.iter().all(|(_, bg, _)| *bg == NEUTRAL_BG_COLOR),
            "an out-of-range page cannot be painted active"
        );

        // Next, by contrast, is a no-op: 98 is already past the last page.
        let (update, changes) = run_click(Some(styled), next_node(total), state);
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(current_page_of(&mut probe), 98);
    }

    #[test]
    fn click_a_page_button_snaps_an_out_of_range_page_back_into_range() {
        let total = 4;
        let mut p = Pagination::create(1, total);
        p.pagination_state.inner.current_page = usize::MAX;
        let (styled, state) = flatten(p);
        let mut probe = state.clone();

        let (_, _) = run_click(Some(styled), page_node(2), state);
        assert_eq!(
            current_page_of(&mut probe),
            2,
            "an explicit page click always lands in range"
        );
    }
}

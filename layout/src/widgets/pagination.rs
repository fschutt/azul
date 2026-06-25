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
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutJustifyContent, LayoutMinWidth, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderRightColor, StyleCursor, StyleTextAlign, StyleUserSelect, StyleTextColor, LayoutBorderLeftWidth, StyleBorderLeftStyle, StyleBorderLeftColor, StyleBorderTopLeftRadius, StyleBorderBottomLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomRightRadius},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static PAGINATION_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-pagination"))];
static PAGINATION_PAGE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-pagination-page"))];
static PAGINATION_NAV_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-pagination-nav"))];

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
const PAGE_BORDER_COLOR: ColorU = ColorU { r: 206, g: 212, b: 218, a: 255 };
/// Active-page background (#0d6efd, accent blue).
const ACCENT_BG_COLOR: ColorU = ColorU { r: 13, g: 110, b: 253, a: 255 };
/// Neutral (inactive) background (white).
const NEUTRAL_BG_COLOR: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
/// Active-page text colour (white).
const ACTIVE_TEXT: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
/// Neutral text colour (#212529, dark).
const NEUTRAL_TEXT: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 };
/// Disabled (Prev/Next at a bound) text colour (#adb5bd, muted grey).
const DISABLED_TEXT: ColorU = ColorU { r: 173, g: 181, b: 189, a: 255 };

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
            inner: PAGE_BORDER_COLOR,
        })),
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
    #[must_use] pub fn create(current_page: usize, total_pages: usize) -> Self {
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
    #[must_use] pub fn with_current_page(mut self, current_page: usize) -> Self {
        self.set_current_page(current_page);
        self
    }

    #[inline]
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
    pub fn with_on_change<C: Into<PaginationOnChangeCallback>>(
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

        let current = self.pagination_state.inner.current_page;
        let total = self.pagination_state.inner.total_pages;

        // One shared RefAny across every button's callback (RefAny::clone shares
        // the underlying state — same pattern as segmented/tabs/map).
        let state = RefAny::new(self.pagination_state);

        let make_button =
            |label: AzString, class: &'static [IdOrClass], style: CssPropertyWithConditionsVec| {
                Dom::create_text(label)
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(class))
                    .with_css_props(style)
                    .with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::MouseUp),
                            callback: CoreCallback {
                                cb: on_page_click as usize,
                                ctx: OptionRefAny::None,
                            },
                            refany: state.clone(),
                        }]
                        .into(),
                    )
                    .with_tab_index(TabIndex::Auto)
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
    let parent = match info.get_parent(clicked) {
        Some(p) => p,
        None => return Update::DoNothing,
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

    let pos = match buttons.iter().position(|b| *b == clicked) {
        Some(p) => p,
        None => return Update::DoNothing,
    };

    let current = {
        let pg = match data.downcast_ref::<PaginationStateWrapper>() {
            Some(s) => s,
            None => return Update::DoNothing,
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
        let mut pg = match data.downcast_mut::<PaginationStateWrapper>() {
            Some(s) => s,
            None => return Update::DoNothing,
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
            (NEUTRAL_BG, if disabled { DISABLED_TEXT } else { NEUTRAL_TEXT })
        } else if i == n - 1 {
            // Next
            let disabled = new_page >= total;
            (NEUTRAL_BG, if disabled { DISABLED_TEXT } else { NEUTRAL_TEXT })
        } else if i == new_page {
            (ACCENT_BG, ACTIVE_TEXT)
        } else {
            (NEUTRAL_BG, NEUTRAL_TEXT)
        };
        info.set_css_property(*node, CssProperty::const_background_content(bg));
        info.set_css_property(*node, CssProperty::const_text_color(StyleTextColor { inner: text }));
    }

    result
}

impl From<Pagination> for Dom {
    fn from(p: Pagination) -> Self {
        p.dom()
    }
}

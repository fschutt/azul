//! Breadcrumb widget — a horizontal path-navigation trail: a row of clickable
//! crumb links separated by a "/" glyph, where the last crumb is the current
//! page (non-clickable, muted + bold). A blend of [`crate::widgets::segmented::Segmented`]
//! (the horizontal row of clickable text nodes whose clicked index is derived
//! from sibling position) and [`crate::widgets::button::Button`]'s `Link` look
//! (blue, pointer cursor) for the crumb links.
//!
//! Clicking a crumb invokes the user's `on_navigate(index)` carrying the clicked
//! crumb's index in [`BreadcrumbState`] (exactly how segmented carries its
//! selected index). Unlike segmented there is no persistent selection to
//! re-style: navigating a breadcrumb is expected to rebuild the page, so the
//! handler only reports the index and does not live-restyle.
//!
//! Index derivation: the children alternate `crumb, separator, crumb, separator,
//! …, current`, so crumb `i` sits at sibling position `2*i`; the handler computes
//! `index = position / 2`. Separators and the final current crumb carry no
//! callback, so the hit node is always a clickable crumb (an even position).
//!
//! Key types: [`Breadcrumb`], [`BreadcrumbState`], [`BreadcrumbOnNavigate`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize, StyleFontWeight},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutAlignItems, LayoutAlignSelf, LayoutFlexGrow, LayoutMarginLeft, LayoutMarginRight},
        property::{CssProperty, *},
        style::{StyleCursor, StyleUserSelect, StyleTextColor},
    },
    impl_option_inner, AzString, StringVec,
};

use crate::callbacks::{Callback, CallbackInfo};

static BREADCRUMB_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-breadcrumb"))];
static BREADCRUMB_ITEM_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-breadcrumb-item"))];
static BREADCRUMB_CURRENT_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-breadcrumb-current"))];
static BREADCRUMB_SEPARATOR_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-breadcrumb-separator"))];

/// Separator glyph rendered between crumbs.
const SEPARATOR_GLYPH: AzString = AzString::from_const_str("/");

/// Callback function type invoked when a (non-current) crumb is clicked.
pub type BreadcrumbOnNavigateCallbackType =
    extern "C" fn(RefAny, CallbackInfo, BreadcrumbState) -> Update;
impl_widget_callback!(
    BreadcrumbOnNavigate,
    OptionBreadcrumbOnNavigate,
    BreadcrumbOnNavigateCallback,
    BreadcrumbOnNavigateCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        BreadcrumbOnNavigateCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: BREADCRUMB_ON_NAVIGATE_INVOKER,
    invoker_ty:     AzBreadcrumbOnNavigateCallbackInvoker,
    thunk_fn:       az_breadcrumb_on_navigate_callback_thunk,
    setter_fn:      AzApp_setBreadcrumbOnNavigateCallbackInvoker,
    from_handle_fn: AzBreadcrumbOnNavigateCallback_createFromHostHandle,
    extra_args:     [ state: BreadcrumbState ],
}

/// A horizontal trail of clickable crumb links ending in the current page.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Breadcrumb {
    pub breadcrumb_state: BreadcrumbStateWrapper,
    /// The crumb labels, in order (the last is the current, non-clickable page).
    pub labels: StringVec,
    /// Style for the row container.
    pub container_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct BreadcrumbStateWrapper {
    /// The last-clicked crumb index.
    pub inner: BreadcrumbState,
    /// Optional: function to call when a crumb is clicked.
    pub on_navigate: OptionBreadcrumbOnNavigate,
}

/// State of a [`Breadcrumb`]: the index of the most recently clicked crumb.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct BreadcrumbState {
    /// Zero-based index of the clicked crumb.
    pub selected_index: usize,
}

// ---- colours ----
/// Crumb-link colour (#0d6efd, Bootstrap link blue).
const LINK_COLOR: ColorU = ColorU { r: 13, g: 110, b: 253, a: 255 };
/// Current-crumb colour (#495057, muted dark grey).
const CURRENT_COLOR: ColorU = ColorU { r: 73, g: 80, b: 87, a: 255 };
/// Separator colour (#6c757d, grey).
const SEPARATOR_COLOR: ColorU = ColorU { r: 108, g: 117, b: 125, a: 255 };

/// Row container: a horizontal flex row that hugs its content.
static BREADCRUMB_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::align_self(LayoutAlignSelf::Start)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
];

/// Clickable crumb-link style (blue, pointer cursor). A hover underline is
/// omitted to keep the style a const slice (`TextDecoration::Underline.into()`
/// is not const); the link colour + pointer already read clearly as a link.
static BREADCRUMB_ITEM_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: LINK_COLOR,
    })),
];

/// Current (last) crumb style: muted dark, bold, not clickable.
static BREADCRUMB_CURRENT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::font_weight(StyleFontWeight::Bold)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: CURRENT_COLOR,
    })),
];

/// Separator-glyph style: grey, with a small horizontal gap on each side.
static BREADCRUMB_SEPARATOR_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_margin_left(LayoutMarginLeft::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_margin_right(LayoutMarginRight::const_px(
        8,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: SEPARATOR_COLOR,
    })),
];

impl Breadcrumb {
    /// Creates a breadcrumb from the given labels (the last is the current page).
    #[must_use] pub fn create(labels: StringVec) -> Self {
        Self {
            breadcrumb_state: BreadcrumbStateWrapper::default(),
            labels,
            container_style: CssPropertyWithConditionsVec::from_const_slice(
                BREADCRUMB_CONTAINER_STYLE,
            ),
        }
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(StringVec::from_const_slice(&[]));
        core::mem::swap(&mut s, self);
        s
    }

    #[inline]
    pub fn set_on_navigate<C: Into<BreadcrumbOnNavigateCallback>>(
        &mut self,
        data: RefAny,
        on_navigate: C,
    ) {
        self.breadcrumb_state.on_navigate = Some(BreadcrumbOnNavigate {
            callback: on_navigate.into(),
            refany: data,
        })
        .into();
    }

    #[inline]
    #[must_use] pub fn with_on_navigate<C: Into<BreadcrumbOnNavigateCallback>>(
        mut self,
        data: RefAny,
        on_navigate: C,
    ) -> Self {
        self.set_on_navigate(data, on_navigate);
        self
    }

    #[must_use] pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{EventFilter, HoverEventFilter},
            refany::OptionRefAny,
        };

        let count = self.labels.as_ref().len();

        // One shared RefAny across every crumb callback (RefAny::clone shares the
        // underlying state — same pattern as segmented/tabs/map).
        let state = RefAny::new(self.breadcrumb_state);

        let mut children: Vec<Dom> = Vec::with_capacity(count.saturating_mul(2));
        for (i, label) in self.labels.as_ref().iter().enumerate() {
            let is_last = i + 1 == count;

            if is_last {
                // The current page: muted + bold, non-clickable (no callback).
                children.push(
                    Dom::create_text(label.clone())
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(
                            BREADCRUMB_CURRENT_CLASS,
                        ))
                        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                            BREADCRUMB_CURRENT_STYLE,
                        )),
                );
            } else {
                // A clickable crumb link.
                children.push(
                    Dom::create_text(label.clone())
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(BREADCRUMB_ITEM_CLASS))
                        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                            BREADCRUMB_ITEM_STYLE,
                        ))
                        .with_callbacks(
                            vec![CoreCallbackData {
                                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                                callback: CoreCallback {
                                    cb: on_crumb_click as usize,
                                    ctx: OptionRefAny::None,
                                },
                                refany: state.clone(),
                            }]
                            .into(),
                        )
                        .with_tab_index(TabIndex::Auto),
                );
                // Separator after every non-last crumb.
                children.push(
                    Dom::create_text(SEPARATOR_GLYPH)
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(
                            BREADCRUMB_SEPARATOR_CLASS,
                        ))
                        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                            BREADCRUMB_SEPARATOR_STYLE,
                        )),
                );
            }
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(BREADCRUMB_CLASS))
            .with_css_props(self.container_style)
            .with_children(children.into())
    }
}

impl Default for Breadcrumb {
    fn default() -> Self {
        Self::create(StringVec::from_const_slice(&[]))
    }
}

/// Click handler shared by all crumb links. Determines the clicked crumb's index
/// from its position among its siblings (`index = position / 2`, since the
/// children alternate crumb/separator), updates the state, and invokes the user
/// `on_navigate` callback. No live restyle — navigating is expected to rebuild
/// the page.
extern "C" fn on_crumb_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::dom::DomNodeId;

    let clicked = info.get_hit_node();
    let Some(parent) = info.get_parent(clicked) else {
        return Update::DoNothing;
    };

    // Collect the children in document order, then find the clicked crumb's slot.
    let mut siblings: Vec<DomNodeId> = Vec::new();
    let mut cur = info.get_first_child(parent);
    while let Some(node) = cur {
        siblings.push(node);
        cur = info.get_next_sibling(node);
    }

    let Some(pos) = siblings.iter().position(|n| *n == clicked) else {
        return Update::DoNothing;
    };
    // Crumbs sit at even positions (crumb, separator, crumb, separator, …).
    let index = pos / 2;

    let Some(mut bc) = data.downcast_mut::<BreadcrumbStateWrapper>() else {
        return Update::DoNothing;
    };
    bc.inner.selected_index = index;
    let inner = bc.inner;
    let bc = &mut *bc;
    match bc.on_navigate.as_mut() {
        Some(BreadcrumbOnNavigate { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
        None => Update::DoNothing,
    }
}

impl From<Breadcrumb> for Dom {
    fn from(b: Breadcrumb) -> Self {
        b.dom()
    }
}

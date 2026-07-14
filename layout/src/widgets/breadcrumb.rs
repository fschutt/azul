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

#[cfg(test)]
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
    use azul_css::system::SystemStyle;
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

    /// `n` distinct labels: `c0, c1, … c{n-1}`.
    fn n_labels(n: usize) -> StringVec {
        StringVec::from_vec((0..n).map(|i| AzString::from(format!("c{i}"))).collect::<Vec<_>>())
    }

    /// The text of a `NodeType::Text` node (`None` for any other node type).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
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

    /// The `color` (text colour) declared by a style slice.
    fn text_color(style: &[CssPropertyWithConditions]) -> Option<ColorU> {
        style.iter().find_map(|p| match &p.property {
            CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
            _ => None,
        })
    }

    fn has_property(style: &[CssPropertyWithConditions], wanted: &CssProperty) -> bool {
        style.iter().any(|p| p.property == *wanted)
    }

    fn has_cursor(style: &[CssPropertyWithConditions]) -> bool {
        style
            .iter()
            .any(|p| matches!(&p.property, CssProperty::Cursor(_)))
    }

    /// A `RefAny` payload recording every index a user `on_navigate` sees.
    struct NavLog {
        seen: Vec<usize>,
    }

    extern "C" fn record_nav(mut data: RefAny, _: CallbackInfo, state: BreadcrumbState) -> Update {
        if let Some(mut log) = data.downcast_mut::<NavLog>() {
            log.seen.push(state.selected_index);
        }
        Update::RefreshDom
    }

    extern "C" fn nav_do_nothing(_: RefAny, _: CallbackInfo, _: BreadcrumbState) -> Update {
        Update::DoNothing
    }

    extern "C" fn nav_refresh_all(_: RefAny, _: CallbackInfo, _: BreadcrumbState) -> Update {
        Update::RefreshDomAllWindows
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn nav_cb(f: BreadcrumbOnNavigateCallbackType) -> BreadcrumbOnNavigateCallback {
        f.into()
    }

    fn log_indices(data: &mut RefAny) -> Vec<usize> {
        data.downcast_ref::<NavLog>()
            .expect("payload must still be a NavLog")
            .seen
            .clone()
    }

    fn selected_index_of(data: &mut RefAny) -> usize {
        data.downcast_ref::<BreadcrumbStateWrapper>()
            .expect("payload must still be a BreadcrumbStateWrapper")
            .inner
            .selected_index
    }

    /// The `RefAny` carried by crumb `i`'s click callback (crumbs sit at even
    /// child positions).
    fn crumb_state(dom: &Dom, crumb: usize) -> RefAny {
        let cbs = dom.children.as_ref()[crumb * 2].root.get_callbacks();
        cbs.as_ref()
            .first()
            .expect("a non-last crumb must carry the click callback")
            .refany
            .clone()
    }

    /// A `DomLayoutResult` with an *empty* layout tree: `on_crumb_click` only
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
            display_list: DisplayList::default(),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Flattens `bc.dom()` and hands back the shared state `RefAny` the crumb
    /// callbacks carry. Requires >= 2 labels (so that crumb 0 is clickable).
    fn flatten(bc: Breadcrumb) -> (StyledDom, RefAny) {
        let dom = bc.dom();
        let state = crumb_state(&dom, 0);
        (StyledDom::create_from_dom(dom), state)
    }

    /// Invokes `on_crumb_click` against a `LayoutWindow` holding `styled` (or
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

        let update = on_crumb_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    // ------------------------------------------------------------------
    // Breadcrumb::create
    // ------------------------------------------------------------------

    #[test]
    fn create_preserves_labels_verbatim_and_defaults_the_state() {
        for case in [
            vec![],
            vec!["only"],
            vec!["Home", "Docs"],
            vec!["Home", "Docs", "Widgets", "Breadcrumb"],
        ] {
            let bc = Breadcrumb::create(labels(&case));

            let got: Vec<&str> = bc.labels.as_ref().iter().map(AzString::as_str).collect();
            assert_eq!(got, case, "create must not reorder/drop/rewrite labels");

            assert_eq!(
                bc.breadcrumb_state.inner.selected_index, 0,
                "a fresh breadcrumb starts at crumb 0"
            );
            assert!(
                bc.breadcrumb_state.on_navigate.as_ref().is_none(),
                "create must not install a callback"
            );
            assert_eq!(
                bc.container_style.as_ref(),
                BREADCRUMB_CONTAINER_STYLE,
                "create must use the shared const container style"
            );
        }
    }

    #[test]
    fn create_survives_pathological_labels() {
        // empty string, whitespace-only, a label that *is* the separator glyph,
        // emoji + ZWJ, RTL, combining marks, NUL, and a 100k-char label.
        let huge = "x".repeat(100_000);
        let case = vec![
            "",
            "   ",
            "/",
            "//",
            "a\u{0}b",
            "👨‍👩‍👧‍👦",
            "مرحبا",
            "e\u{0301}\u{0301}\u{0301}",
            "\u{200b}\u{feff}",
            huge.as_str(),
        ];
        let bc = Breadcrumb::create(labels(&case));

        let got: Vec<&str> = bc.labels.as_ref().iter().map(AzString::as_str).collect();
        assert_eq!(got, case, "labels must survive byte-for-byte");
        assert_eq!(bc.labels.as_ref()[9].as_str().len(), 100_000);

        // …and they must survive the trip through the DOM unchanged.
        let dom = bc.dom();
        let texts: Vec<&str> = dom
            .children
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0) // crumbs sit at even positions
            .filter_map(|(_, c)| text_of(c))
            .collect();
        assert_eq!(texts, case);
    }

    #[test]
    fn create_with_many_labels_does_not_panic() {
        let n = 10_000;
        let bc = Breadcrumb::create(n_labels(n));
        assert_eq!(bc.labels.as_ref().len(), n);
        assert_eq!(bc.labels.as_ref()[n - 1].as_str(), "c9999");
    }

    #[test]
    fn default_equals_create_with_no_labels() {
        assert_eq!(
            Breadcrumb::default(),
            Breadcrumb::create(StringVec::from_const_slice(&[]))
        );
        assert!(Breadcrumb::default().labels.as_ref().is_empty());
    }

    // ------------------------------------------------------------------
    // Breadcrumb::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_old_value_and_resets_self() {
        let mut bc = Breadcrumb::create(labels(&["Home", "Docs", "Here"]));
        let old = bc.swap_with_default();

        let old_labels: Vec<&str> = old.labels.as_ref().iter().map(AzString::as_str).collect();
        assert_eq!(old_labels, ["Home", "Docs", "Here"], "the old value moves out");
        assert!(
            bc.labels.as_ref().is_empty(),
            "self must be left as a default (empty) breadcrumb"
        );
        assert_eq!(bc, Breadcrumb::default());
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_of_self() {
        let mut bc = Breadcrumb::create(labels(&["a", "b"]))
            .with_on_navigate(RefAny::new(NavLog { seen: Vec::new() }), nav_cb(record_nav));

        let old = bc.swap_with_default();
        assert!(
            old.breadcrumb_state.on_navigate.as_ref().is_some(),
            "the callback must travel with the returned value"
        );
        assert!(
            bc.breadcrumb_state.on_navigate.as_ref().is_none(),
            "self must not keep a dangling reference to the moved-out callback"
        );
    }

    #[test]
    fn swap_with_default_is_stable_when_repeated() {
        let mut bc = Breadcrumb::create(labels(&["a"]));
        let _ = bc.swap_with_default();
        // Now `bc` is already a default — swapping again must keep returning
        // defaults, not panic or corrupt state.
        for _ in 0..100 {
            let out = bc.swap_with_default();
            assert_eq!(out, Breadcrumb::default());
            assert_eq!(bc, Breadcrumb::default());
        }
    }

    #[test]
    fn swap_with_default_preserves_a_customised_state() {
        let mut bc = Breadcrumb::create(labels(&["a", "b"]));
        bc.breadcrumb_state.inner.selected_index = usize::MAX;

        let old = bc.swap_with_default();
        assert_eq!(
            old.breadcrumb_state.inner.selected_index,
            usize::MAX,
            "an out-of-range index must move out untouched (no clamping/rewrite)"
        );
        assert_eq!(bc.breadcrumb_state.inner.selected_index, 0);
    }

    // ------------------------------------------------------------------
    // Breadcrumb::set_on_navigate / with_on_navigate
    // ------------------------------------------------------------------

    #[test]
    fn with_on_navigate_sets_the_callback_and_touches_nothing_else() {
        let before = Breadcrumb::create(labels(&["Home", "Docs"]));
        let after = Breadcrumb::create(labels(&["Home", "Docs"]))
            .with_on_navigate(RefAny::new(NavLog { seen: Vec::new() }), nav_cb(record_nav));

        assert_eq!(
            after.labels, before.labels,
            "installing a callback must not disturb the labels"
        );
        assert_eq!(
            after.container_style, before.container_style,
            "installing a callback must not disturb the container style"
        );
        assert_eq!(after.breadcrumb_state.inner.selected_index, 0);

        let installed = after
            .breadcrumb_state
            .on_navigate
            .as_ref()
            .expect("with_on_navigate must install Some(..)");
        assert_eq!(installed.callback.cb as usize, record_nav as usize);
    }

    #[test]
    fn set_on_navigate_overwrites_the_previous_callback_and_data() {
        let mut bc = Breadcrumb::create(labels(&["a", "b"]));
        bc.set_on_navigate(RefAny::new(NavLog { seen: Vec::new() }), nav_cb(record_nav));
        bc.set_on_navigate(RefAny::new(42u32), nav_cb(nav_do_nothing));

        let installed = bc
            .breadcrumb_state
            .on_navigate
            .as_mut()
            .expect("still Some after the overwrite");
        assert_eq!(
            installed.callback.cb as usize,
            nav_do_nothing as usize,
            "the last set_on_navigate must win"
        );
        assert_eq!(
            installed.refany.downcast_ref::<u32>().map(|v| *v),
            Some(42),
            "the payload must be replaced along with the fn pointer"
        );
        assert!(
            installed.refany.downcast_ref::<NavLog>().is_none(),
            "the stale payload must be gone"
        );
    }

    #[test]
    fn set_on_navigate_accepts_a_generic_callback_without_corrupting_the_fn_pointer() {
        // The FFI path (`From<Callback>`) transmutes the fn pointer. The value
        // must round-trip bit-for-bit — a corrupted pointer would be an
        // unconditional jump into garbage at click time. (Never invoked here.)
        let raw = record_nav as usize;
        let generic = Callback {
            cb: unsafe { core::mem::transmute::<usize, crate::callbacks::CallbackType>(raw) },
            ctx: OptionRefAny::None,
        };
        let converted: BreadcrumbOnNavigateCallback = generic.into();
        assert_eq!(converted.cb as usize, raw);
    }

    #[test]
    fn with_on_navigate_on_an_empty_breadcrumb_yields_a_dom_with_no_callbacks() {
        // 0 labels => no crumbs => the installed callback is simply never wired up.
        let dom = Breadcrumb::create(StringVec::from_const_slice(&[]))
            .with_on_navigate(RefAny::new(NavLog { seen: Vec::new() }), nav_cb(record_nav))
            .dom();

        assert!(dom.children.as_ref().is_empty());
        assert!(dom.root.get_callbacks().as_ref().is_empty());
    }

    // ------------------------------------------------------------------
    // Breadcrumb::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_no_labels_is_an_empty_container() {
        let dom = Breadcrumb::create(StringVec::from_const_slice(&[])).dom();

        assert!(
            dom.children.as_ref().is_empty(),
            "no labels => no crumbs and no separators"
        );
        assert_eq!(dom.estimated_total_children, 0);
        assert!(dom.root.has_class("__azul-native-breadcrumb"));
    }

    #[test]
    fn dom_of_a_single_label_has_no_separator_and_no_clickable_crumb() {
        let dom = Breadcrumb::create(labels(&["Home"])).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 1, "a lone label is the current page, nothing else");
        assert_eq!(text_of(&children[0]), Some("Home"));
        assert!(children[0].root.has_class("__azul-native-breadcrumb-current"));
        assert!(
            children[0].root.get_callbacks().as_ref().is_empty(),
            "the current page must not be clickable"
        );
        assert_eq!(children[0].root.get_tab_index(), None);
    }

    #[test]
    fn dom_alternates_crumb_separator_and_ends_on_the_current_page() {
        let case = ["Home", "Docs", "Widgets", "Breadcrumb"];
        let dom = Breadcrumb::create(labels(&case)).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 2 * case.len() - 1, "n crumbs + (n-1) separators");

        for (pos, child) in children.iter().enumerate() {
            let is_last_child = pos + 1 == children.len();
            if pos % 2 == 1 {
                // separator
                assert_eq!(text_of(child), Some("/"), "odd positions are separators");
                assert!(child.root.has_class("__azul-native-breadcrumb-separator"));
                assert!(
                    child.root.get_callbacks().as_ref().is_empty(),
                    "separators must not be clickable"
                );
                assert_eq!(child.root.get_tab_index(), None);
            } else if is_last_child {
                // current page
                assert_eq!(text_of(child), Some(case[pos / 2]));
                assert!(child.root.has_class("__azul-native-breadcrumb-current"));
                assert!(
                    child.root.get_callbacks().as_ref().is_empty(),
                    "the current page must not be clickable"
                );
                assert_eq!(child.root.get_tab_index(), None);
            } else {
                // clickable crumb link
                assert_eq!(text_of(child), Some(case[pos / 2]));
                assert!(child.root.has_class("__azul-native-breadcrumb-item"));
                let cbs = child.root.get_callbacks();
                assert_eq!(cbs.as_ref().len(), 1, "one MouseUp handler per crumb");
                assert_eq!(
                    cbs.as_ref()[0].event,
                    EventFilter::Hover(HoverEventFilter::MouseUp)
                );
                assert_eq!(cbs.as_ref()[0].callback.cb, on_crumb_click as usize);
                assert_eq!(
                    child.root.get_tab_index(),
                    Some(TabIndex::Auto),
                    "crumb links must be keyboard-reachable"
                );
            }
        }
    }

    #[test]
    fn dom_estimated_total_children_matches_the_real_descendant_count() {
        // `estimated_total_children` is a cached count; if it under-counts,
        // `convert_dom_into_compact_dom` under-allocates and panics.
        for n in [0usize, 1, 2, 3, 5, 64, 257] {
            let dom = Breadcrumb::create(n_labels(n)).dom();
            let expected = if n == 0 { 0 } else { 2 * n - 1 };

            assert_eq!(dom.children.as_ref().len(), expected, "child count for n={n}");
            assert_eq!(
                dom.estimated_total_children,
                recursive_descendants(&dom),
                "cached descendant count desynced for n={n}"
            );
            assert_eq!(dom.estimated_total_children, expected, "for n={n}");
        }
    }

    #[test]
    fn dom_of_many_labels_flattens_without_panicking() {
        let n = 200;
        let styled = StyledDom::create_from_dom(Breadcrumb::create(n_labels(n)).dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            2 * n,
            "root + (2n-1) children"
        );
    }

    #[test]
    fn dom_separator_is_told_apart_by_class_not_by_text() {
        // A label that is literally "/" must still be a crumb, not a separator.
        let dom = Breadcrumb::create(labels(&["/", "b"])).dom();
        let children = dom.children.as_ref();

        assert_eq!(text_of(&children[0]), Some("/"));
        assert!(children[0].root.has_class("__azul-native-breadcrumb-item"));
        assert!(!children[0].root.has_class("__azul-native-breadcrumb-separator"));
        assert!(!children[0].root.get_callbacks().as_ref().is_empty());

        assert_eq!(text_of(&children[1]), Some("/"));
        assert!(children[1].root.has_class("__azul-native-breadcrumb-separator"));
        assert!(children[1].root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn dom_shares_one_state_refany_across_every_crumb() {
        let dom = Breadcrumb::create(labels(&["a", "b", "c", "d"])).dom();

        // Write through crumb 0's handle…
        let mut first = crumb_state(&dom, 0);
        {
            let mut w = first
                .downcast_mut::<BreadcrumbStateWrapper>()
                .expect("crumb state must be a BreadcrumbStateWrapper");
            w.inner.selected_index = 7;
        }
        // …and read it back through crumb 2's handle.
        let mut third = crumb_state(&dom, 2);
        assert_eq!(
            selected_index_of(&mut third),
            7,
            "every crumb must observe the same shared state"
        );
    }

    #[test]
    fn dom_gives_separate_breadcrumbs_separate_state() {
        let a = Breadcrumb::create(labels(&["a", "b"])).dom();
        let b = Breadcrumb::create(labels(&["a", "b"])).dom();

        let mut a0 = crumb_state(&a, 0);
        {
            let mut w = a0.downcast_mut::<BreadcrumbStateWrapper>().unwrap();
            w.inner.selected_index = 3;
        }

        let mut b0 = crumb_state(&b, 0);
        assert_eq!(
            selected_index_of(&mut b0),
            0,
            "two breadcrumbs must not alias one another's state"
        );
    }

    #[test]
    fn dom_carries_the_container_style_through() {
        let bc = Breadcrumb::create(labels(&["a", "b"]));
        assert_eq!(bc.container_style.as_ref(), BREADCRUMB_CONTAINER_STYLE);

        let dom = bc.dom();
        assert!(dom.root.has_class("__azul-native-breadcrumb"));
        assert!(
            dom.root.is_node_type(NodeType::Div),
            "the row container must be a div"
        );
    }

    // ------------------------------------------------------------------
    // Style constants (invariants the widget's look depends on)
    // ------------------------------------------------------------------

    #[test]
    fn crumb_styles_are_opaque_and_visually_distinct() {
        for (name, c) in [
            ("link", LINK_COLOR),
            ("current", CURRENT_COLOR),
            ("separator", SEPARATOR_COLOR),
        ] {
            assert_eq!(c.a, 255, "{name} colour must be fully opaque");
        }
        assert_ne!(
            LINK_COLOR, CURRENT_COLOR,
            "the current page must not look like a link"
        );

        assert_eq!(text_color(BREADCRUMB_ITEM_STYLE), Some(LINK_COLOR));
        assert_eq!(text_color(BREADCRUMB_CURRENT_STYLE), Some(CURRENT_COLOR));
        assert_eq!(text_color(BREADCRUMB_SEPARATOR_STYLE), Some(SEPARATOR_COLOR));
    }

    #[test]
    fn only_the_clickable_crumb_style_declares_a_pointer_cursor() {
        assert!(has_property(
            BREADCRUMB_ITEM_STYLE,
            &CssProperty::const_cursor(StyleCursor::Pointer)
        ));
        assert!(
            !has_cursor(BREADCRUMB_CURRENT_STYLE),
            "the current page is not clickable, so it must not advertise a pointer"
        );
        assert!(
            !has_cursor(BREADCRUMB_SEPARATOR_STYLE),
            "separators are not clickable"
        );
    }

    #[test]
    fn separator_style_has_symmetric_horizontal_margins() {
        assert!(has_property(
            BREADCRUMB_SEPARATOR_STYLE,
            &CssProperty::const_margin_left(LayoutMarginLeft::const_px(8))
        ));
        assert!(has_property(
            BREADCRUMB_SEPARATOR_STYLE,
            &CssProperty::const_margin_right(LayoutMarginRight::const_px(8))
        ));
    }

    #[test]
    fn every_crumb_style_disables_text_selection_and_flex_growth() {
        for (name, style) in [
            ("item", BREADCRUMB_ITEM_STYLE),
            ("current", BREADCRUMB_CURRENT_STYLE),
            ("separator", BREADCRUMB_SEPARATOR_STYLE),
        ] {
            assert!(
                has_property(style, &CssProperty::user_select(StyleUserSelect::None)),
                "{name}: dragging across a breadcrumb must not select its text"
            );
            assert!(
                has_property(
                    style,
                    &CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))
                ),
                "{name}: crumbs must hug their content"
            );
        }
        assert!(has_property(
            BREADCRUMB_CURRENT_STYLE,
            &CssProperty::font_weight(StyleFontWeight::Bold)
        ));
        assert_eq!(SEPARATOR_GLYPH.as_str(), "/");
    }

    // ------------------------------------------------------------------
    // on_crumb_click
    // ------------------------------------------------------------------

    #[test]
    fn click_reports_the_index_of_each_crumb() {
        // children: crumb0(1) sep(2) crumb1(3) sep(4) crumb2(5) sep(6) current(7)
        let (styled, state) = flatten(Breadcrumb::create(labels(&["a", "b", "c", "d"])));
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            8,
            "fixture must flatten to root + 7 children"
        );

        for (hit, expected) in [(1usize, 0usize), (3, 1), (5, 2)] {
            let mut state = state.clone();
            let (update, changes) = run_click(Some(styled.clone()), hit, state.clone());

            assert_eq!(
                update,
                Update::DoNothing,
                "with no on_navigate installed the handler reports nothing to redraw"
            );
            assert_eq!(
                selected_index_of(&mut state),
                expected,
                "node {hit} sits at sibling position {} => index {expected}",
                hit - 1
            );
            assert!(
                changes.is_empty(),
                "the handler must not live-restyle (navigation rebuilds the page)"
            );
        }
    }

    #[test]
    fn click_invokes_the_user_callback_with_the_clicked_index() {
        let mut log = RefAny::new(NavLog { seen: Vec::new() });
        let bc = Breadcrumb::create(labels(&["a", "b", "c", "d"]))
            .with_on_navigate(log.clone(), nav_cb(record_nav));
        let (styled, state) = flatten(bc);

        let (update, _) = run_click(Some(styled.clone()), 5, state.clone());
        assert_eq!(update, Update::RefreshDom, "the user's Update must propagate");
        assert_eq!(log_indices(&mut log), vec![2]);

        // A second click updates the shared state again — the index is not sticky.
        let (_, _) = run_click(Some(styled), 1, state.clone());
        assert_eq!(log_indices(&mut log), vec![2, 0]);

        let mut state = state;
        assert_eq!(
            selected_index_of(&mut state),
            0,
            "the state must hold the *last* clicked index"
        );
    }

    #[test]
    fn click_propagates_every_update_variant_unchanged() {
        for (cb, expected) in [
            (nav_cb(nav_do_nothing), Update::DoNothing),
            (nav_cb(nav_refresh_all), Update::RefreshDomAllWindows),
        ] {
            let bc =
                Breadcrumb::create(labels(&["a", "b"])).with_on_navigate(RefAny::new(0u8), cb);
            let (styled, state) = flatten(bc);
            let (update, _) = run_click(Some(styled), 1, state);
            assert_eq!(update, expected);
        }
    }

    #[test]
    fn click_on_the_root_node_does_nothing() {
        // The root has no parent -> the handler must bail, not index into nothing.
        let (styled, state) = flatten(Breadcrumb::create(labels(&["a", "b"])));
        let mut state2 = state.clone();

        let (update, changes) = run_click(Some(styled), 0, state);
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(selected_index_of(&mut state2), 0, "state must be untouched");
    }

    #[test]
    fn click_on_an_out_of_range_node_does_nothing() {
        let (styled, state) = flatten(Breadcrumb::create(labels(&["a", "b"])));
        let (update, changes) = run_click(Some(styled), 9999, state);
        assert_eq!(
            update,
            Update::DoNothing,
            "a hit node that isn't in the tree must not panic"
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn click_with_no_layout_result_does_nothing() {
        let bc = Breadcrumb::create(labels(&["a", "b"]));
        let dom = bc.dom();
        let state = crumb_state(&dom, 0);

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
        let (styled, _) = flatten(Breadcrumb::create(labels(&["a", "b"])));
        // Wrong type in the RefAny: downcast fails, handler must bail cleanly.
        let (update, changes) = run_click(Some(styled), 1, RefAny::new(0u32));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn click_with_the_state_already_borrowed_does_nothing() {
        let (styled, state) = flatten(Breadcrumb::create(labels(&["a", "b"])));

        // A live mutable borrow on a sibling clone: `downcast_mut` inside the
        // handler must fail (returning DoNothing) instead of aliasing `&mut`.
        let mut held = state.clone();
        let guard = held
            .downcast_mut::<BreadcrumbStateWrapper>()
            .expect("first borrow succeeds");

        let (update, changes) = run_click(Some(styled), 1, state);
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        drop(guard);
    }

    #[test]
    fn click_on_a_separator_or_the_current_page_maps_to_pos_over_two() {
        // Neither carries a callback, so this is unreachable in practice — but
        // the documented `index = position / 2` must still hold (and not panic)
        // if the handler is ever invoked on one.
        let (styled, state) = flatten(Breadcrumb::create(labels(&["a", "b", "c", "d"])));

        for (hit, expected) in [(2usize, 0usize), (4, 1), (6, 2), (7, 3)] {
            let mut state = state.clone();
            let (update, _) = run_click(Some(styled.clone()), hit, state.clone());
            assert_eq!(update, Update::DoNothing);
            assert_eq!(
                selected_index_of(&mut state),
                expected,
                "node {hit} => position {} => index {expected}",
                hit - 1
            );
        }
    }

    #[test]
    fn click_indices_stay_in_range_for_a_long_trail() {
        let n = 64;
        let (styled, state) = flatten(Breadcrumb::create(n_labels(n)));
        assert_eq!(styled.node_hierarchy.as_ref().len(), 2 * n);

        // Last clickable crumb: index n-2, at child position 2*(n-2) => node 2n-3.
        let hit = 2 * n - 3;
        let mut state = state;
        let (_, _) = run_click(Some(styled), hit, state.clone());

        let idx = selected_index_of(&mut state);
        assert_eq!(idx, n - 2);
        assert!(
            idx < n,
            "the reported index must always address a real label"
        );
    }
}

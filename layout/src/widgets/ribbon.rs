//! Microsoft Office-style ribbon widget.
//!
//! A [`Ribbon`] organizes controls into a tabbed toolbar where each tab
//! contains one or more [`RibbonSection`]s, each with a title and arbitrary
//! content.  Unlike the simpler [`super::tabs`] widget, each tab is further
//! subdivided into titled, visually separated sections — matching the ribbon
//! pattern found in Office applications.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions as Cond, CssPropertyWithConditionsVec},
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, *},
        layout::*,
        property::CssProperty as P,
        style::*,
    },
    *,
};

use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_partialeq, impl_vec_mut};

use crate::callbacks::{Callback, CallbackInfo};

// -- Callback --

/// Callback signature invoked when a ribbon tab is clicked.
pub type RibbonOnTabClickCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    RibbonOnTabClick, OptionRibbonOnTabClick,
    RibbonOnTabClickCallback, RibbonOnTabClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RibbonOnTabClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RIBBON_ON_TAB_CLICK_INVOKER,
    invoker_ty:     AzRibbonOnTabClickCallbackInvoker,
    thunk_fn:       az_ribbon_on_tab_click_callback_thunk,
    setter_fn:      AzApp_setRibbonOnTabClickCallbackInvoker,
    from_handle_fn: AzRibbonOnTabClickCallback_createFromHostHandle,
    extra_args:     [ tab_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- Colors --

const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
const LIGHT_GRAY: ColorU = ColorU { r: 240, g: 240, b: 240, a: 255 };
const BORDER_GRAY: ColorU = ColorU { r: 200, g: 200, b: 200, a: 255 };
const TEXT_GRAY: ColorU = ColorU { r: 100, g: 100, b: 100, a: 255 };
const ACTIVE_BLUE: ColorU = ColorU { r: 0, g: 114, b: 198, a: 255 };
const BG_WHITE: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(WHITE)];
const BG_LIGHT_GRAY: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(LIGHT_GRAY)];

static RIBBON_CONTAINER_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
    Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
];

static TAB_BAR_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_background_content(StyleBackgroundContentVec::from_const_slice(BG_LIGHT_GRAY))),
    Cond::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Cond::simple(P::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_bottom_color(StyleBorderBottomColor { inner: BORDER_GRAY })),
];

static TAB_INACTIVE_STYLE: &[Cond] = &[
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(12))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(12))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(6))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(6))),
    Cond::simple(P::const_cursor(StyleCursor::Pointer)),
    Cond::simple(P::const_text_color(StyleTextColor { inner: TEXT_GRAY })),
];

static TAB_ACTIVE_STYLE: &[Cond] = &[
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(12))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(12))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(6))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(6))),
    Cond::simple(P::const_cursor(StyleCursor::Pointer)),
    Cond::simple(P::const_background_content(StyleBackgroundContentVec::from_const_slice(BG_WHITE))),
    Cond::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(2))),
    Cond::simple(P::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_bottom_color(StyleBorderBottomColor { inner: ACTIVE_BLUE })),
];

static SECTIONS_CONTAINER_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Cond::simple(P::const_background_content(StyleBackgroundContentVec::from_const_slice(BG_WHITE))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(4))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(4))),
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(4))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(4))),
    Cond::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1))),
    Cond::simple(P::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_bottom_color(StyleBorderBottomColor { inner: BORDER_GRAY })),
];

static SECTION_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(6))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(6))),
    Cond::simple(P::const_border_right_width(LayoutBorderRightWidth::const_px(1))),
    Cond::simple(P::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })),
    Cond::simple(P::const_border_right_color(StyleBorderRightColor { inner: BORDER_GRAY })),
];

static SECTION_CONTENT_STYLE: &[Cond] = &[
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

static SECTION_TITLE_STYLE: &[Cond] = &[
    Cond::simple(P::const_font_size(StyleFontSize::const_px(11))),
    Cond::simple(P::const_text_color(StyleTextColor { inner: TEXT_GRAY })),
    Cond::simple(P::const_text_align(StyleTextAlign::Center)),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(2))),
];

/// Top-level ribbon widget containing multiple tabs.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Ribbon {
    /// Tabs displayed in the ribbon tab bar.
    pub tabs: RibbonTabVec,
    /// Index of the currently active tab.
    pub active_tab: usize,
    /// Optional callback fired when a tab is clicked.
    pub on_tab_click: OptionRibbonOnTabClick,
}

/// A single tab within a [`Ribbon`], containing a label and sections.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonTab {
    /// Display label shown in the tab bar.
    pub label: AzString,
    /// Sections rendered when this tab is active.
    pub sections: RibbonSectionVec,
}

/// A titled section within a [`RibbonTab`], holding arbitrary content.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonSection {
    /// Title displayed below the section content.
    pub title: AzString,
    /// Content DOM rendered inside this section.
    pub content: Dom,
}

impl_option!(RibbonSection, OptionRibbonSection, copy = false, [Debug, Clone]);
impl_vec!(RibbonSection, RibbonSectionVec, RibbonSectionVecDestructor, RibbonSectionVecDestructorType, RibbonSectionVecSlice, OptionRibbonSection);
impl_vec_clone!(RibbonSection, RibbonSectionVec, RibbonSectionVecDestructor);
impl_vec_debug!(RibbonSection, RibbonSectionVec);
impl_vec_mut!(RibbonSection, RibbonSectionVec);

impl_option!(RibbonTab, OptionRibbonTab, copy = false, [Debug, Clone]);
impl_vec!(RibbonTab, RibbonTabVec, RibbonTabVecDestructor, RibbonTabVecDestructorType, RibbonTabVecSlice, OptionRibbonTab);
impl_vec_clone!(RibbonTab, RibbonTabVec, RibbonTabVecDestructor);
impl_vec_debug!(RibbonTab, RibbonTabVec);
impl_vec_mut!(RibbonTab, RibbonTabVec);

impl RibbonTab {
    /// Creates a new tab with the given label and no sections.
    #[must_use] pub const fn new(label: AzString) -> Self {
        Self { label, sections: RibbonSectionVec::from_const_slice(&[]) }
    }

    /// Appends a section to this tab.
    pub fn add_section(&mut self, section: RibbonSection) {
        self.sections.push(section);
    }

    /// Builder method: appends a section and returns `self`.
    #[must_use] pub fn with_section(mut self, section: RibbonSection) -> Self {
        self.add_section(section);
        self
    }
}

impl RibbonSection {
    /// Creates a new section with the given title and content DOM.
    #[must_use] pub const fn new(title: AzString, content: Dom) -> Self {
        Self { title, content }
    }
}

impl Ribbon {
    /// Creates a new ribbon with the given tabs, defaulting to the first tab active.
    #[must_use] pub fn new(tabs: RibbonTabVec) -> Self {
        Self { tabs, active_tab: 0, on_tab_click: None.into() }
    }

    /// Sets the active tab by index, clamping to the last valid tab.
    pub const fn set_active_tab(&mut self, index: usize) {
        let max = self.tabs.len().saturating_sub(1);
        self.active_tab = if index > max { max } else { index };
    }

    /// Registers a callback invoked when a tab is clicked.
    pub fn set_on_tab_click<C: Into<RibbonOnTabClickCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_tab_click = Some(RibbonOnTabClick {
            callback: cb.into(), refany: data,
        }).into();
    }

    /// Builder method: registers a tab-click callback and returns `self`.
    #[must_use]
    pub fn with_on_tab_click<C: Into<RibbonOnTabClickCallback>>(mut self, data: RefAny, cb: C) -> Self {
        self.set_on_tab_click(data, cb);
        self
    }

    /// Builds the ribbon DOM, rendering the tab bar and the active tab's sections.
    #[must_use] pub fn dom(self) -> Dom {
        let active_tab = self.active_tab;
        let has_callback = self.on_tab_click.is_some();

        let tab_items: Vec<Dom> = self.tabs.as_slice().iter().enumerate().map(|(idx, tab)| {
            let style = if idx == active_tab { TAB_ACTIVE_STYLE } else { TAB_INACTIVE_STYLE };
            let mut d = Dom::create_text(tab.label.clone())
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(style));
            if has_callback {
                d = d.with_callbacks(vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: on_ribbon_tab_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(TabClickData {
                        tab_idx: idx, on_tab_click: self.on_tab_click.clone(),
                    }),
                }].into());
            }
            d
        }).collect();

        let tab_bar = Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(TAB_BAR_STYLE))
            .with_children(DomVec::from_vec(tab_items));

        let sections_dom = if let Some(active) = self.tabs.into_library_owned_vec().into_iter().nth(active_tab) {
            let items: Vec<Dom> = active.sections.into_library_owned_vec().into_iter().map(|s| {
                let content = Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTION_CONTENT_STYLE))
                    .with_children(DomVec::from_vec(vec![s.content]));
                let title = Dom::create_text(s.title)
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTION_TITLE_STYLE));
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTION_STYLE))
                    .with_children(DomVec::from_vec(vec![content, title]))
            }).collect();
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTIONS_CONTAINER_STYLE))
                .with_children(DomVec::from_vec(items))
        } else {
            Dom::create_div()
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(SECTIONS_CONTAINER_STYLE))
        };

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(RIBBON_CONTAINER_STYLE))
            .with_ids_and_classes({
                const CLS: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-ribbon"))];
                IdOrClassVec::from_const_slice(CLS)
            })
            .with_children(DomVec::from_vec(vec![tab_bar, sections_dom]))
    }
}

struct TabClickData {
    tab_idx: usize,
    on_tab_click: OptionRibbonOnTabClick,
}

extern "C" fn on_ribbon_tab_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    let Some(mut data) = refany.downcast_mut::<TabClickData>() else {
        return Update::DoNothing;
    };
    let idx = data.tab_idx;
    match data.on_tab_click.as_mut() {
        Some(RibbonOnTabClick { refany, callback }) => {
            (callback.cb)(refany.clone(), info, idx)
        }
        None => Update::DoNothing,
    }
}

impl From<Ribbon> for Dom {
    fn from(r: Ribbon) -> Self { r.dom() }
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{props::property::CssProperty, system::SystemStyle};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Pathological label/title inputs reused across the string tests: empty,
    /// whitespace-only, interior NUL, emoji-with-ZWJ, RTL, stacked combining
    /// marks, zero-width + BOM + RTL-override, and a 100k-char string.
    fn nasty_strings() -> Vec<String> {
        vec![
            String::new(),
            "   ".to_string(),
            "a\u{0}b".to_string(),
            "👨‍👩‍👧‍👦🇩🇪".to_string(),
            "مرحبا שלום".to_string(),
            "e\u{0301}\u{0327}\u{0301}".to_string(),
            "\u{200b}\u{feff}\u{202e}rtl-override".to_string(),
            "x".repeat(100_000),
        ]
    }

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == name))
    }

    /// The text of a `NodeType::Text` node (`None` for any other node type).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    /// The node's inline style, flattened back to the property list that
    /// `with_css_props` was handed.
    fn inline_props(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The property list of one of this module's `static &[Cond]` style tables.
    fn style_props(style: &[Cond]) -> Vec<CssProperty> {
        style.iter().map(|c| c.property.clone()).collect()
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

    /// `(tab bar, sections container)` of a rendered ribbon DOM.
    fn parts(dom: &Dom) -> (&Dom, &Dom) {
        let ch = dom.children.as_ref();
        assert_eq!(ch.len(), 2, "a ribbon DOM is exactly [tab bar, sections]");
        (&ch[0], &ch[1])
    }

    /// `(content wrapper, title)` of the `n`-th rendered section.
    fn section_parts(sections: &Dom, n: usize) -> (&Dom, &Dom) {
        let sec = &sections.children.as_ref()[n];
        let ch = sec.children.as_ref();
        assert_eq!(ch.len(), 2, "a section is exactly [content, title]");
        (&ch[0], &ch[1])
    }

    /// `n` tabs labelled `t0 … t{n-1}`, each with `sections_per_tab` sections
    /// titled `t{i}s{j}` wrapping a text node `t{i}c{j}`.
    fn tabs(n: usize, sections_per_tab: usize) -> RibbonTabVec {
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            let mut tab = RibbonTab::new(AzString::from(format!("t{i}")));
            for j in 0..sections_per_tab {
                tab.add_section(RibbonSection::new(
                    AzString::from(format!("t{i}s{j}")),
                    Dom::create_text(format!("t{i}c{j}")),
                ));
            }
            v.push(tab);
        }
        RibbonTabVec::from_vec(v)
    }

    /// A `RefAny` payload recording every tab index a user `on_tab_click` sees.
    struct TabLog {
        seen: Vec<usize>,
    }

    extern "C" fn record_tab(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
        if let Some(mut log) = data.downcast_mut::<TabLog>() {
            log.seen.push(index);
        }
        Update::RefreshDom
    }

    extern "C" fn tab_do_nothing(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }

    extern "C" fn tab_refresh_all(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::RefreshDomAllWindows
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn tab_cb(f: RibbonOnTabClickCallbackType) -> RibbonOnTabClickCallback {
        f.into()
    }

    fn log_indices(data: &mut RefAny) -> Vec<usize> {
        data.downcast_ref::<TabLog>()
            .expect("payload must still be a TabLog")
            .seen
            .clone()
    }

    /// Invokes `on_ribbon_tab_click` with `hit` as the hit node. The handler
    /// never reads the DOM, so the `LayoutWindow` deliberately holds no layout
    /// results at all — if it ever starts touching them, these tests notice.
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(hit: usize, data: RefAny) -> (Update, Vec<CallbackChange>) {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");

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

        let update = on_ribbon_tab_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    // ------------------------------------------------------------------
    // RibbonTab::new  (constructor: no_panic + invariants)
    // ------------------------------------------------------------------

    #[test]
    fn tab_new_stores_label_verbatim_and_starts_section_less() {
        for label in nasty_strings() {
            let tab = RibbonTab::new(AzString::from(label.clone()));

            assert_eq!(
                tab.label.as_str(),
                label.as_str(),
                "the label must survive byte-for-byte"
            );
            assert!(tab.sections.is_empty(), "a fresh tab has no sections");
            assert_eq!(tab.sections.len(), 0);
            assert_eq!(
                tab.sections.capacity(),
                0,
                "the const-slice-backed empty vec must report cap 0"
            );
            assert!(tab.sections.as_ref().is_empty());
        }
    }

    #[test]
    fn tab_new_with_a_100k_char_label_keeps_every_byte() {
        let huge = "ab".repeat(50_000);
        let tab = RibbonTab::new(AzString::from(huge.clone()));
        assert_eq!(tab.label.as_str().len(), 100_000);
        assert_eq!(tab.label.as_str(), huge);
    }

    // ------------------------------------------------------------------
    // RibbonTab::add_section / with_section
    // ------------------------------------------------------------------

    #[test]
    fn add_section_grows_the_const_backed_vec_without_freeing_static_memory() {
        // `RibbonTab::new` seeds `sections` from `from_const_slice(&[])`
        // (destructor = NoDestructor, ptr = &'static). The very first `push`
        // therefore has to take the "fresh allocation" branch rather than
        // realloc'ing static memory. If it took the realloc path this test
        // aborts inside the allocator.
        let mut tab = RibbonTab::new(AzString::from("t"));
        for i in 0..1000usize {
            tab.add_section(RibbonSection::new(
                AzString::from(format!("s{i}")),
                Dom::create_text(format!("c{i}")),
            ));
            assert_eq!(tab.sections.len(), i + 1);
            assert!(
                tab.sections.capacity() >= tab.sections.len(),
                "capacity must never fall below len"
            );
        }

        for (i, s) in tab.sections.as_ref().iter().enumerate() {
            assert_eq!(s.title.as_str(), format!("s{i}"), "push must append in order");
        }

        // ...and the grown buffer is now genuinely owned: cloning it must deep
        // copy, so a drop of both halves cannot double-free.
        let cloned = tab.clone();
        assert_eq!(cloned.sections.len(), 1000);
        assert_ne!(
            cloned.sections.as_ptr(),
            tab.sections.as_ptr(),
            "a library-owned vec must deep-clone, not alias"
        );
        drop(cloned);
        assert_eq!(tab.sections.as_ref()[999].title.as_str(), "s999");
    }

    #[test]
    fn add_section_accepts_extreme_titles_and_deeply_nested_content() {
        let mut deep = Dom::create_text("leaf");
        for _ in 0..256 {
            deep = Dom::create_div().with_child(deep);
        }

        let mut tab = RibbonTab::new(AzString::from(""));
        tab.add_section(RibbonSection::new(AzString::from(""), Dom::create_div()));
        tab.add_section(RibbonSection::new(
            AzString::from("x".repeat(100_000)),
            deep.clone(),
        ));

        assert_eq!(tab.sections.len(), 2);
        assert_eq!(tab.sections.as_ref()[0].title.as_str(), "");
        assert_eq!(tab.sections.as_ref()[1].title.as_str().len(), 100_000);
        assert_eq!(tab.sections.as_ref()[1].content, deep);
    }

    #[test]
    fn with_section_matches_add_section() {
        let make = || RibbonSection::new(AzString::from("s"), Dom::create_text("c"));

        let built = RibbonTab::new(AzString::from("t"))
            .with_section(make())
            .with_section(make());

        let mut mutated = RibbonTab::new(AzString::from("t"));
        mutated.add_section(make());
        mutated.add_section(make());

        assert_eq!(built.sections.len(), mutated.sections.len());
        assert_eq!(built.label.as_str(), mutated.label.as_str());
        for (a, b) in built.sections.as_ref().iter().zip(mutated.sections.as_ref()) {
            assert_eq!(a.title.as_str(), b.title.as_str());
            assert_eq!(a.content, b.content);
        }
        // the builder form must not disturb the label
        assert_eq!(built.label.as_str(), "t");
    }

    // ------------------------------------------------------------------
    // RibbonSection::new  (constructor: no_panic + invariants)
    // ------------------------------------------------------------------

    #[test]
    fn section_new_stores_both_args_unchanged() {
        let content = Dom::create_div()
            .with_child(Dom::create_text("a"))
            .with_child(Dom::create_text("b"));

        for title in nasty_strings() {
            let sec = RibbonSection::new(AzString::from(title.clone()), content.clone());
            assert_eq!(sec.title.as_str(), title.as_str());
            assert_eq!(sec.content, content, "content must be stored verbatim");
        }
    }

    #[test]
    fn section_new_accepts_an_empty_and_a_pathologically_deep_content_dom() {
        let empty = RibbonSection::new(AzString::from("t"), Dom::create_div());
        assert!(empty.content.children.as_ref().is_empty());
        assert_eq!(empty.content.estimated_total_children, 0);

        let mut deep = Dom::create_text("leaf");
        for _ in 0..512 {
            deep = Dom::create_div().with_child(deep);
        }
        let sec = RibbonSection::new(AzString::from("t"), deep);
        assert_eq!(
            sec.content.estimated_total_children,
            recursive_descendants(&sec.content),
            "the cached descendant count must survive the move into the section"
        );
    }

    // ------------------------------------------------------------------
    // Ribbon::new  (constructor: no_panic + invariants)
    // ------------------------------------------------------------------

    #[test]
    fn ribbon_new_defaults_to_tab_zero_and_installs_no_callback() {
        for count in [0usize, 1, 2, 7, 500] {
            let r = Ribbon::new(tabs(count, 1));

            assert_eq!(r.tabs.len(), count, "new must not drop or duplicate tabs");
            assert_eq!(r.active_tab, 0, "a fresh ribbon starts on tab 0");
            assert!(
                r.on_tab_click.is_none(),
                "Ribbon::new must not install a callback"
            );
            for (i, t) in r.tabs.as_ref().iter().enumerate() {
                assert_eq!(t.label.as_str(), format!("t{i}"));
                assert_eq!(t.sections.len(), 1);
            }
        }
    }

    #[test]
    fn ribbon_new_on_an_empty_vec_leaves_a_zero_active_tab_that_dom_survives() {
        // active_tab == 0 is *out of range* for a tab-less ribbon. That is the
        // documented default; the invariant that matters is that `dom()` does
        // not index with it.
        let r = Ribbon::new(RibbonTabVec::from_vec(Vec::new()));
        assert_eq!(r.active_tab, 0);
        assert!(r.tabs.is_empty());

        let dom = r.dom();
        let (bar, sections) = parts(&dom);
        assert!(bar.children.as_ref().is_empty());
        assert!(sections.children.as_ref().is_empty());
    }

    // ------------------------------------------------------------------
    // Ribbon::set_active_tab  (numeric: zero / min / max / overflow)
    // ------------------------------------------------------------------

    #[test]
    fn set_active_tab_on_an_empty_ribbon_always_lands_on_zero() {
        // `len().saturating_sub(1)` is 0 for an empty vec — the clamp must not
        // underflow-panic in a debug build.
        let mut r = Ribbon::new(RibbonTabVec::from_vec(Vec::new()));
        for index in [0usize, 1, 2, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            r.set_active_tab(index);
            assert_eq!(r.active_tab, 0, "empty ribbon must clamp {index} to 0");
        }
    }

    #[test]
    fn set_active_tab_clamps_to_the_last_valid_index() {
        for count in [1usize, 2, 3, 8] {
            let last = count - 1;
            let mut r = Ribbon::new(tabs(count, 0));

            for index in 0..count {
                r.set_active_tab(index);
                assert_eq!(r.active_tab, index, "in-range index must pass through");
            }
            for index in [count, count + 1, count * 2, usize::MAX] {
                r.set_active_tab(index);
                assert_eq!(
                    r.active_tab, last,
                    "out-of-range {index} must clamp to the last tab"
                );
            }
        }
    }

    #[test]
    fn set_active_tab_at_usize_min_and_max_does_not_overflow() {
        let mut r = Ribbon::new(tabs(3, 0));

        r.set_active_tab(usize::MIN);
        assert_eq!(r.active_tab, 0);

        r.set_active_tab(usize::MAX);
        assert_eq!(r.active_tab, 2);

        // `usize::MAX` is also what a negative index looks like after the
        // wrap-around a C / FFI caller would perform, so it must clamp too.
        r.set_active_tab(-1i64 as usize);
        assert_eq!(r.active_tab, 2);
        r.set_active_tab(-3i32 as usize);
        assert_eq!(r.active_tab, 2);
    }

    #[test]
    fn set_active_tab_is_idempotent_and_never_touches_the_tabs() {
        let mut r = Ribbon::new(tabs(4, 2));

        for _ in 0..3 {
            r.set_active_tab(usize::MAX);
            assert_eq!(r.active_tab, 3);
        }
        for _ in 0..3 {
            r.set_active_tab(1);
            assert_eq!(r.active_tab, 1);
        }

        assert_eq!(r.tabs.len(), 4, "clamping must not resize the tab list");
        for (i, t) in r.tabs.as_ref().iter().enumerate() {
            assert_eq!(t.label.as_str(), format!("t{i}"));
            assert_eq!(t.sections.len(), 2);
        }
    }

    // ------------------------------------------------------------------
    // Ribbon::set_on_tab_click / with_on_tab_click
    // ------------------------------------------------------------------

    #[test]
    fn set_on_tab_click_last_call_wins() {
        let mut r = Ribbon::new(tabs(2, 0));

        r.set_on_tab_click(RefAny::new(1u8), tab_cb(tab_do_nothing));
        assert!(r.on_tab_click.is_some());
        assert_eq!(
            r.on_tab_click.as_ref().unwrap().callback,
            tab_cb(tab_do_nothing)
        );

        // a second call must *replace*, not append / leak / panic
        r.set_on_tab_click(RefAny::new(9i64), tab_cb(record_tab));
        let set = r.on_tab_click.as_ref().expect("still Some");
        assert_eq!(set.callback, tab_cb(record_tab));
        assert_ne!(set.callback, tab_cb(tab_do_nothing));
        assert_eq!(set.refany.get_type_id(), RefAny::new(0i64).get_type_id());

        // ...and it leaves the rest of the widget alone
        assert_eq!(r.tabs.len(), 2);
        assert_eq!(r.active_tab, 0);
    }

    #[test]
    fn set_on_tab_click_shares_rather_than_copies_the_caller_payload() {
        let mut kept = RefAny::new(TabLog { seen: Vec::new() });
        let mut r = Ribbon::new(tabs(1, 0));
        r.set_on_tab_click(kept.clone(), tab_cb(record_tab));

        // writing through the widget's handle is visible through the caller's
        *r.on_tab_click
            .as_mut()
            .unwrap()
            .refany
            .downcast_mut::<TabLog>()
            .expect("payload type preserved") = TabLog { seen: vec![42] };

        assert_eq!(log_indices(&mut kept), vec![42]);
    }

    #[test]
    fn with_on_tab_click_matches_set_on_tab_click() {
        let built = Ribbon::new(tabs(3, 1)).with_on_tab_click(RefAny::new(7u32), tab_cb(record_tab));

        let mut mutated = Ribbon::new(tabs(3, 1));
        mutated.set_on_tab_click(RefAny::new(7u32), tab_cb(record_tab));

        assert_eq!(
            built.on_tab_click.as_ref().unwrap().callback,
            mutated.on_tab_click.as_ref().unwrap().callback
        );
        // the builder form must not disturb the tabs or the active index
        assert_eq!(built.tabs.len(), 3);
        assert_eq!(built.active_tab, 0);
    }

    #[test]
    fn with_on_tab_click_preserves_a_previously_clamped_active_tab() {
        let mut r = Ribbon::new(tabs(4, 0));
        r.set_active_tab(usize::MAX);
        let r = r.with_on_tab_click(RefAny::new(0u8), tab_cb(record_tab));

        assert_eq!(r.active_tab, 3, "installing a callback must not reset state");
        assert!(r.on_tab_click.is_some());
    }

    // ------------------------------------------------------------------
    // Ribbon::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_an_empty_ribbon_is_a_classed_container_with_two_empty_children() {
        let dom = Ribbon::new(RibbonTabVec::from_vec(Vec::new())).dom();

        assert!(has_class(&dom, "__azul-native-ribbon"));
        assert_eq!(inline_props(&dom), style_props(RIBBON_CONTAINER_STYLE));

        let (bar, sections) = parts(&dom);
        assert_eq!(inline_props(bar), style_props(TAB_BAR_STYLE));
        assert_eq!(
            inline_props(sections),
            style_props(SECTIONS_CONTAINER_STYLE)
        );
        assert!(bar.children.as_ref().is_empty());
        assert!(sections.children.as_ref().is_empty());
        assert_eq!(dom.estimated_total_children, 2);
    }

    #[test]
    fn dom_styles_exactly_the_active_tab() {
        let count = 5usize;
        for active in 0..count {
            let mut r = Ribbon::new(tabs(count, 0));
            r.set_active_tab(active);
            let dom = r.dom();
            let (bar, _) = parts(&dom);

            assert_eq!(bar.children.as_ref().len(), count);
            for (i, tab) in bar.children.as_ref().iter().enumerate() {
                let expected = if i == active {
                    style_props(TAB_ACTIVE_STYLE)
                } else {
                    style_props(TAB_INACTIVE_STYLE)
                };
                assert_eq!(inline_props(tab), expected, "tab {i} (active = {active})");
                let want = format!("t{i}");
                assert_eq!(text_of(tab), Some(want.as_str()));
            }
        }
    }

    #[test]
    fn dom_with_an_out_of_range_active_tab_highlights_nothing_and_renders_no_sections() {
        // `active_tab` is a public field, so it can hold a value `set_active_tab`
        // would have clamped away. `dom()` must not index with it.
        for active in [3usize, 4, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let mut r = Ribbon::new(tabs(3, 2));
            r.active_tab = active;

            let dom = r.dom();
            let (bar, sections) = parts(&dom);

            assert_eq!(bar.children.as_ref().len(), 3, "every tab is still shown");
            for tab in bar.children.as_ref() {
                assert_eq!(
                    inline_props(tab),
                    style_props(TAB_INACTIVE_STYLE),
                    "no tab may be styled active when active_tab is out of range"
                );
            }
            assert!(
                sections.children.as_ref().is_empty(),
                "an out-of-range active tab renders an empty section container"
            );
        }
    }

    #[test]
    fn dom_survives_a_stale_active_tab_after_the_tab_list_shrinks() {
        let mut r = Ribbon::new(tabs(3, 1));
        r.set_active_tab(2);
        // the public `tabs` field is swapped out from under the clamped index
        r.tabs = tabs(1, 1);

        let dom = r.dom();
        let (bar, sections) = parts(&dom);
        assert_eq!(bar.children.as_ref().len(), 1);
        assert!(sections.children.as_ref().is_empty());
    }

    #[test]
    fn dom_renders_only_the_active_tabs_sections_in_content_then_title_order() {
        let mut r = Ribbon::new(tabs(3, 4));
        r.set_active_tab(1);
        let dom = r.dom();
        let (_, sections) = parts(&dom);

        assert_eq!(sections.children.as_ref().len(), 4);
        for j in 0..4 {
            let section = &sections.children.as_ref()[j];
            assert_eq!(inline_props(section), style_props(SECTION_STYLE));

            let (content, title) = section_parts(sections, j);
            assert_eq!(inline_props(content), style_props(SECTION_CONTENT_STYLE));
            assert_eq!(inline_props(title), style_props(SECTION_TITLE_STYLE));

            // the title is rendered *after* the content, as documented
            let want_title = format!("t1s{j}");
            let want_content = format!("t1c{j}");
            assert_eq!(text_of(title), Some(want_title.as_str()));
            let inner = content.children.as_ref();
            assert_eq!(inner.len(), 1, "the wrapper holds exactly the user content");
            assert_eq!(text_of(&inner[0]), Some(want_content.as_str()));
        }
    }

    #[test]
    fn dom_round_trips_pathological_labels_and_titles_byte_for_byte() {
        let strings = nasty_strings();
        let mut v = Vec::new();
        for s in &strings {
            v.push(
                RibbonTab::new(AzString::from(s.clone())).with_section(RibbonSection::new(
                    AzString::from(s.clone()),
                    Dom::create_text(s.clone()),
                )),
            );
        }
        let dom = Ribbon::new(RibbonTabVec::from_vec(v)).dom();
        let (bar, sections) = parts(&dom);

        assert_eq!(bar.children.as_ref().len(), strings.len());
        for (i, s) in strings.iter().enumerate() {
            assert_eq!(
                text_of(&bar.children.as_ref()[i]),
                Some(s.as_str()),
                "tab label {i} must survive the DOM round trip"
            );
        }

        // active_tab defaults to 0 -> only the first tab's section is rendered
        assert_eq!(sections.children.as_ref().len(), 1);
        let (content, title) = section_parts(sections, 0);
        assert_eq!(text_of(title), Some(strings[0].as_str()));
        assert_eq!(
            text_of(&content.children.as_ref()[0]),
            Some(strings[0].as_str())
        );
    }

    #[test]
    fn dom_without_a_callback_attaches_no_callbacks_at_all() {
        let dom = Ribbon::new(tabs(6, 2)).dom();
        let (bar, sections) = parts(&dom);

        for tab in bar.children.as_ref() {
            assert!(
                tab.root.get_callbacks().as_ref().is_empty(),
                "no user callback -> no MouseUp handler"
            );
        }
        for section in sections.children.as_ref() {
            assert!(section.root.get_callbacks().as_ref().is_empty());
        }
        assert!(dom.root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn dom_gives_every_tab_one_mouseup_callback_carrying_its_own_index() {
        let count = 64usize;
        let dom = Ribbon::new(tabs(count, 1))
            .with_on_tab_click(RefAny::new(TabLog { seen: Vec::new() }), tab_cb(record_tab))
            .dom();
        let (bar, _) = parts(&dom);

        assert_eq!(bar.children.as_ref().len(), count);
        for (i, tab) in bar.children.as_ref().iter().enumerate() {
            let cbs = tab.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 1, "exactly one callback per tab");
            assert_eq!(
                cbs.as_ref()[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp)
            );
            assert_eq!(cbs.as_ref()[0].callback.cb, on_ribbon_tab_click as usize);

            let mut payload = cbs.as_ref()[0].refany.clone();
            let data = payload
                .downcast_ref::<TabClickData>()
                .expect("tab payload is a TabClickData");
            assert_eq!(data.tab_idx, i, "each tab must know its own index");
            assert!(data.on_tab_click.is_some());
        }
    }

    #[test]
    fn dom_shares_the_user_payload_with_every_tab_and_keeps_the_caller_handle_alive() {
        let mut kept = RefAny::new(TabLog { seen: Vec::new() });
        let dom = Ribbon::new(tabs(3, 0))
            .with_on_tab_click(kept.clone(), tab_cb(record_tab))
            .dom();
        let (bar, _) = parts(&dom);

        // write through tab 2's copy of the shared payload...
        let mut payload = bar.children.as_ref()[2].root.get_callbacks().as_ref()[0]
            .refany
            .clone();
        {
            let mut data = payload
                .downcast_mut::<TabClickData>()
                .expect("tab payload is a TabClickData");
            data.on_tab_click
                .as_mut()
                .unwrap()
                .refany
                .downcast_mut::<TabLog>()
                .expect("user payload type preserved")
                .seen
                .push(7);
        }

        // ...and the caller's handle sees it (RefAny shares, it does not deep-copy)
        assert_eq!(log_indices(&mut kept), vec![7]);
    }

    #[test]
    fn dom_child_count_cache_stays_consistent_for_deeply_nested_content() {
        let mut deep = Dom::create_text("leaf");
        for _ in 0..128 {
            deep = Dom::create_div().with_child(deep);
        }

        let tab = RibbonTab::new(AzString::from("t"))
            .with_section(RibbonSection::new(AzString::from("deep"), deep))
            .with_section(RibbonSection::new(AzString::from(""), Dom::create_div()));

        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom();

        // a too-small cache makes `convert_dom_into_compact_dom` under-allocate
        // its arenas and panic on an out-of-bounds write later
        assert_eq!(
            dom.estimated_total_children,
            recursive_descendants(&dom),
            "cached descendant count desynced from the real tree"
        );
        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children()
        );
    }

    #[test]
    fn dom_with_many_tabs_and_sections_does_not_panic() {
        let mut r = Ribbon::new(tabs(500, 20));
        r.set_active_tab(499);
        let dom = r.dom();
        let (bar, sections) = parts(&dom);

        assert_eq!(bar.children.as_ref().len(), 500);
        assert_eq!(sections.children.as_ref().len(), 20);
        assert_eq!(
            inline_props(&bar.children.as_ref()[499]),
            style_props(TAB_ACTIVE_STYLE)
        );
    }

    #[test]
    fn dom_of_a_tab_with_no_sections_yields_an_empty_but_styled_container() {
        let dom = Ribbon::new(tabs(2, 0)).dom();
        let (bar, sections) = parts(&dom);

        assert_eq!(bar.children.as_ref().len(), 2);
        assert!(sections.children.as_ref().is_empty());
        assert_eq!(
            inline_props(sections),
            style_props(SECTIONS_CONTAINER_STYLE),
            "the empty branch must still carry the container style"
        );
    }

    #[test]
    fn from_ribbon_for_dom_matches_dom() {
        // Only meaningful without a callback: every `dom()` call mints fresh
        // per-tab `RefAny`s and two distinct `RefAny`s never compare equal.
        assert_eq!(Dom::from(Ribbon::new(tabs(3, 2))), Ribbon::new(tabs(3, 2)).dom());
        assert_eq!(
            Dom::from(Ribbon::new(RibbonTabVec::from_vec(Vec::new()))),
            Ribbon::new(RibbonTabVec::from_vec(Vec::new())).dom()
        );
    }

    // ------------------------------------------------------------------
    // on_ribbon_tab_click
    // ------------------------------------------------------------------

    #[test]
    fn tab_click_with_a_foreign_payload_is_a_noop() {
        let (update, changes) = run_click(0, RefAny::new(0xdead_beef_u64));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload must not touch the window"
        );
    }

    #[test]
    fn tab_click_without_a_user_callback_is_a_noop() {
        let data = RefAny::new(TabClickData {
            tab_idx: 3,
            on_tab_click: None.into(),
        });

        let (update, changes) = run_click(0, data);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "the handler never restyles by itself");
    }

    #[test]
    fn tab_click_forwards_the_index_and_propagates_the_user_update() {
        let mut log = RefAny::new(TabLog { seen: Vec::new() });
        let data = RefAny::new(TabClickData {
            tab_idx: 17,
            on_tab_click: Some(RibbonOnTabClick {
                callback: tab_cb(record_tab),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) = run_click(0, data.clone());

        assert_eq!(update, Update::RefreshDom, "the user's Update must win");
        assert!(changes.is_empty());
        assert_eq!(log_indices(&mut log), vec![17]);

        // the handler is stateless: a second click reports the same index again
        let (update, _) = run_click(0, data);
        assert_eq!(update, Update::RefreshDom);
        assert_eq!(log_indices(&mut log), vec![17, 17]);
    }

    #[test]
    fn tab_click_forwards_extreme_indices_verbatim() {
        let mut log = RefAny::new(TabLog { seen: Vec::new() });
        let indices = [0usize, 1, usize::MAX / 2, usize::MAX - 1, usize::MAX];

        for idx in indices {
            let data = RefAny::new(TabClickData {
                tab_idx: idx,
                on_tab_click: Some(RibbonOnTabClick {
                    callback: tab_cb(record_tab),
                    refany: log.clone(),
                })
                .into(),
            });
            let (update, _) = run_click(0, data);
            assert_eq!(update, Update::RefreshDom);
        }

        assert_eq!(
            log_indices(&mut log),
            indices.to_vec(),
            "indices must reach the user callback without clamping or wrapping"
        );
    }

    #[test]
    fn tab_click_propagates_every_update_variant() {
        for (cb, expected) in [
            (tab_cb(tab_do_nothing), Update::DoNothing),
            (tab_cb(tab_refresh_all), Update::RefreshDomAllWindows),
        ] {
            let data = RefAny::new(TabClickData {
                tab_idx: 0,
                on_tab_click: Some(RibbonOnTabClick {
                    callback: cb,
                    refany: RefAny::new(0u8),
                })
                .into(),
            });
            let (update, changes) = run_click(0, data);
            assert_eq!(update, expected);
            assert!(changes.is_empty());
        }
    }

    #[test]
    fn tab_click_ignores_the_hit_node_entirely() {
        // the handler is a pure forwarder — a hit node that does not exist in
        // any layout result must behave exactly like a valid one.
        let mut log = RefAny::new(TabLog { seen: Vec::new() });
        for hit in [0usize, 1, 999, usize::MAX / 4] {
            let data = RefAny::new(TabClickData {
                tab_idx: 5,
                on_tab_click: Some(RibbonOnTabClick {
                    callback: tab_cb(record_tab),
                    refany: log.clone(),
                })
                .into(),
            });
            let (update, changes) = run_click(hit, data);
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }
        assert_eq!(log_indices(&mut log), vec![5, 5, 5, 5]);
    }

    #[test]
    fn tab_click_from_a_real_dom_payload_reports_the_clicked_tab() {
        let mut log = RefAny::new(TabLog { seen: Vec::new() });
        let dom = Ribbon::new(tabs(4, 1))
            .with_on_tab_click(log.clone(), tab_cb(record_tab))
            .dom();
        let (bar, _) = parts(&dom);

        for i in [3usize, 0, 2, 1] {
            let payload = bar.children.as_ref()[i].root.get_callbacks().as_ref()[0]
                .refany
                .clone();
            let (update, changes) = run_click(i, payload);
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }

        assert_eq!(
            log_indices(&mut log),
            vec![3, 0, 2, 1],
            "each tab's payload must report that tab's own index"
        );
    }
}

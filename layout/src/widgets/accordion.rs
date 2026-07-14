//! Accordion / expander widget — one or more collapsible titled sections. Each
//! section is a clickable header row plus a body that shows or hides. Combines
//! the expand/collapse state of [`crate::widgets::tree_view::TreeView`] with a
//! flat list of sections (each carrying an arbitrary content [`Dom`]).
//!
//! Sections toggle independently (any number may be open at once). Clicking a
//! header flips that section's `is_open` flag in a per-header [`RefAny`] (the
//! self-contained per-row data pattern of `tree_view`), invokes the optional
//! user `on_toggle(section_index)`, and shows/hides the section body by setting
//! `display: block | none` on it via `set_css_property` (mirroring tree_view /
//! check_box live restyling).
//!
//! TODO2: the header is a plain styled clickable bar with no animated disclosure
//! chevron — a glyph cannot be re-textured via `set_css_property` without a
//! relayout, so an indicator that flips on toggle is deferred. The `display`
//! toggle itself follows the proven live-restyle pattern but the `display:none`
//! relayout is not GUI-verified in this build.
//!
//! Key types: [`Accordion`], [`AccordionSection`], [`AccordionOnToggle`].

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        TabIndex,
    },
    refany::{OptionRefAny, RefAny},
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, impl_vec_partialeq,
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, StyleFontSize},
        layout::{LayoutDisplay, LayoutFlexDirection, LayoutFlexGrow, LayoutOverflow, LayoutAlignItems, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight},
        property::{CssProperty, *},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleTextColor, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleCursor, StyleUserSelect, StyleTextAlign},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static ACCORDION_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-accordion"))];
static ACCORDION_SECTION_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-accordion-section",
))];
static ACCORDION_HEADER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-accordion-header",
))];
static ACCORDION_TITLE_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-accordion-title",
))];
static ACCORDION_BODY_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-accordion-body"))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

/// Callback invoked when a section header is clicked. The `usize` is the
/// zero-based index of the toggled section.
pub type AccordionOnToggleCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    AccordionOnToggle,
    OptionAccordionOnToggle,
    AccordionOnToggleCallback,
    AccordionOnToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        AccordionOnToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ACCORDION_ON_TOGGLE_INVOKER,
    invoker_ty:     AzAccordionOnToggleCallbackInvoker,
    thunk_fn:       az_accordion_on_toggle_callback_thunk,
    setter_fn:      AzApp_setAccordionOnToggleCallbackInvoker,
    from_handle_fn: AzAccordionOnToggleCallback_createFromHostHandle,
    extra_args:     [ section_index: usize ],
}

// ---- colours ----
const BORDER_COLOR: ColorU = ColorU { r: 222, g: 226, b: 230, a: 255 }; // #dee2e6
const HEADER_BG: ColorU = ColorU { r: 248, g: 249, b: 250, a: 255 }; // #f8f9fa
const TEXT_COLOR: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 }; // #212529

const HEADER_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(HEADER_BG)];
const HEADER_BG_VEC: StyleBackgroundContentVec =
    StyleBackgroundContentVec::from_const_slice(HEADER_BG_ITEMS);

/// One collapsible section: a header title and an arbitrary content body.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct AccordionSection {
    /// The header text shown for this section.
    pub title: AzString,
    /// The body content revealed when the section is open.
    pub content: Dom,
    /// Whether this section starts open (body visible).
    pub is_open: bool,
}

impl AccordionSection {
    /// Creates a new collapsed section with the given title and content.
    pub fn new<S: Into<AzString>>(title: S, content: Dom) -> Self {
        Self {
            title: title.into(),
            content,
            is_open: false,
        }
    }

    /// Builder method: sets the initial open state.
    #[must_use] pub const fn with_open(mut self, open: bool) -> Self {
        self.is_open = open;
        self
    }
}

impl_option!(AccordionSection, OptionAccordionSection, copy = false, [Debug, Clone, PartialEq, Eq]);
impl_vec!(
    AccordionSection,
    AccordionSectionVec,
    AccordionSectionVecDestructor,
    AccordionSectionVecDestructorType,
    AccordionSectionVecSlice,
    OptionAccordionSection
);
impl_vec_clone!(AccordionSection, AccordionSectionVec, AccordionSectionVecDestructor);
impl_vec_debug!(AccordionSection, AccordionSectionVec);
impl_vec_partialeq!(AccordionSection, AccordionSectionVec);
impl_vec_mut!(AccordionSection, AccordionSectionVec);

/// A vertical stack of collapsible titled sections.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct Accordion {
    /// The sections, in display order.
    pub sections: AccordionSectionVec,
    /// Optional callback fired when any section header is toggled.
    pub on_toggle: OptionAccordionOnToggle,
}

// ---- styles ----

static ACCORDION_CONTAINER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TEXT_COLOR,
    })),
    // border: 1px solid #dee2e6
    CssPropertyWithConditions::simple(CssProperty::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
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
    CssPropertyWithConditions::simple(CssProperty::const_border_left_style(StyleBorderLeftStyle {
        inner: BorderStyle::Solid,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_color(StyleBorderTopColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: BORDER_COLOR,
        },
    )),
    // rounded corners, clipping the per-section separators
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(6),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_x(LayoutOverflow::Hidden)),
    CssPropertyWithConditions::simple(CssProperty::const_overflow_y(LayoutOverflow::Hidden)),
];

static ACCORDION_SECTION_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
        LayoutFlexDirection::Column,
    )),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    // a thin separator between stacked sections
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: BORDER_COLOR,
        },
    )),
];

static ACCORDION_HEADER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        10,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(10),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(HEADER_BG_VEC)),
];

static ACCORDION_TITLE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
];

/// Body style when the section is OPEN: a padded block with a top separator.
static ACCORDION_BODY_STYLE_OPEN: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        12,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(12),
    )),
];

/// Body style when the section is CLOSED: not laid out at all.
static ACCORDION_BODY_STYLE_CLOSED: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::None)),
];

impl Accordion {
    /// Creates a new accordion from the given sections, with no toggle callback.
    #[must_use] pub fn new(sections: AccordionSectionVec) -> Self {
        Self {
            sections,
            on_toggle: None.into(),
        }
    }

    /// Creates an empty accordion.
    #[must_use] pub fn create() -> Self {
        Self::new(AccordionSectionVec::from_const_slice(&[]))
    }

    /// Sets the callback invoked when any section header is toggled.
    pub fn set_on_toggle<C: Into<AccordionOnToggleCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_toggle = Some(AccordionOnToggle {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder method: sets the toggle callback.
    #[must_use] pub fn with_on_toggle<C: Into<AccordionOnToggleCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_toggle(data, callback);
        self
    }

    /// Replaces `self` with an empty default accordion and returns the original.
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the accordion into a [`Dom`] subtree.
    #[must_use] pub fn dom(self) -> Dom {
        let on_toggle = self.on_toggle;
        let sections = self.sections;

        let mut section_doms: Vec<Dom> = Vec::with_capacity(sections.as_ref().len());

        for (index, section) in sections.as_ref().iter().enumerate() {
            let title = Dom::create_text(section.title.clone())
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_TITLE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    ACCORDION_TITLE_STYLE,
                ));

            // Per-header self-contained click data (mirrors tree_view's NodeClickData).
            let header_data = HeaderClickData {
                index,
                is_open: section.is_open,
                on_toggle: clone_option_on_toggle(&on_toggle),
            };

            let header = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_HEADER_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    ACCORDION_HEADER_STYLE,
                ))
                .with_tab_index(TabIndex::Auto)
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: on_accordion_header_click as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: RefAny::new(header_data),
                    }]
                    .into(),
                )
                .with_children(DomVec::from_vec(alloc::vec![title]));

            let body_style = if section.is_open {
                ACCORDION_BODY_STYLE_OPEN
            } else {
                ACCORDION_BODY_STYLE_CLOSED
            };
            let body = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_BODY_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(body_style))
                .with_children(DomVec::from_vec(alloc::vec![section.content.clone()]));

            section_doms.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_SECTION_CLASS))
                    .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                        ACCORDION_SECTION_STYLE,
                    ))
                    .with_children(DomVec::from_vec(alloc::vec![header, body])),
            );
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ACCORDION_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                ACCORDION_CONTAINER_STYLE,
            ))
            .with_children(DomVec::from_vec(section_doms))
    }
}

impl Default for Accordion {
    fn default() -> Self {
        Self::create()
    }
}

/// Clones an `OptionAccordionOnToggle` (the callback wrapper is not `Copy`).
fn clone_option_on_toggle(opt: &OptionAccordionOnToggle) -> OptionAccordionOnToggle {
    match opt.as_ref() {
        Some(AccordionOnToggle { callback, refany }) => Some(AccordionOnToggle {
            callback: callback.clone(),
            refany: refany.clone(),
        })
        .into(),
        None => None.into(),
    }
}

/// Per-header callback payload (kept internal, like `tree_view::NodeClickData`).
struct HeaderClickData {
    index: usize,
    is_open: bool,
    on_toggle: OptionAccordionOnToggle,
}

/// Header click handler. The hit node is the header (the callback-bearing node,
/// per `currentTarget` semantics — see `radio_group`); its next sibling is the
/// body. Flips this section's `is_open`, invokes the optional user callback with
/// the section index, then shows/hides the body via `display`.
extern "C" fn on_accordion_header_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let header = info.get_hit_node();
    let Some(body) = info.get_next_sibling(header) else {
        return Update::DoNothing;
    };

    let (now_open, result) = {
        let Some(mut hd) = data.downcast_mut::<HeaderClickData>() else {
            return Update::DoNothing;
        };
        hd.is_open = !hd.is_open;
        let now_open = hd.is_open;
        let index = hd.index;
        let result = match hd.on_toggle.as_mut() {
            Some(AccordionOnToggle { callback, refany }) => {
                (callback.cb)(refany.clone(), info, index)
            }
            None => Update::DoNothing,
        };
        (now_open, result)
    };

    let display = if now_open {
        LayoutDisplay::Block
    } else {
        LayoutDisplay::None
    };
    info.set_css_property(body, CssProperty::const_display(display));

    result
}

impl From<Accordion> for Dom {
    fn from(a: Accordion) -> Self {
        a.dom()
    }
}

#[cfg(all(test, feature = "std"))]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
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

    /// The `display` value in a node's *inline* style, if it sets one.
    fn inline_display(node: &Dom) -> Option<LayoutDisplay> {
        node.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::Display(v) => v.get_property().copied(),
                _ => None,
            })
    }

    /// `(header, body)` of the `n`-th section of a rendered accordion DOM.
    fn section_parts(dom: &Dom, n: usize) -> (&Dom, &Dom) {
        let section = &dom.children.as_ref()[n];
        assert!(has_class(section, "__azul-native-accordion-section"));
        let children = section.children.as_ref();
        assert_eq!(children.len(), 2, "a section is exactly [header, body]");
        (&children[0], &children[1])
    }

    /// A three-node styled DOM — `root(0)` with children `header(1)` and
    /// `body(2)` — i.e. the exact hierarchy `on_accordion_header_click` walks
    /// (`hit node` -> `next sibling`).
    fn header_body_dom() -> StyledDom {
        let styled = StyledDom::create_from_dom(
            Dom::create_div()
                .with_child(Dom::create_div())
                .with_child(Dom::create_div()),
        );
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            3,
            "fixture must flatten to exactly root/header/body"
        );
        styled
    }

    /// A `DomLayoutResult` with an *empty* layout tree: the click handler only
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

    /// Invokes `on_accordion_header_click` against a `LayoutWindow` holding
    /// `styled` (or nothing at all, when `styled` is `None`), with `hit` as the
    /// hit node. Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(styled: Option<StyledDom>, hit: usize, data: RefAny) -> (Update, Vec<CallbackChange>) {
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

        let update = on_accordion_header_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    /// Every `display` write recorded in the change log, as `(node index, display)`.
    fn display_writes(changes: &[CallbackChange]) -> Vec<(usize, LayoutDisplay)> {
        let mut out = Vec::new();
        for change in changes {
            if let CallbackChange::ChangeNodeCssProperties {
                node_id, properties, ..
            } = change
            {
                for p in properties.as_ref() {
                    if let CssProperty::Display(v) = p {
                        if let Some(d) = v.get_property() {
                            out.push((node_id.index(), *d));
                        }
                    }
                }
            }
        }
        out
    }

    /// `is_open` of a `HeaderClickData` payload.
    fn payload_is_open(data: &mut RefAny) -> bool {
        data.downcast_ref::<HeaderClickData>()
            .expect("payload must still be a HeaderClickData")
            .is_open
    }

    /// Records the section indices it is invoked with; used as a user `on_toggle`.
    struct ToggleLog {
        calls: Vec<usize>,
    }

    extern "C" fn record_toggle(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
        if let Some(mut log) = data.downcast_mut::<ToggleLog>() {
            log.calls.push(index);
        }
        Update::RefreshDom
    }

    extern "C" fn toggle_do_nothing(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }

    fn toggle_cb(f: AccordionOnToggleCallbackType) -> AccordionOnToggleCallback {
        f.into()
    }

    // ------------------------------------------------------------------
    // AccordionSection::new / with_open  (constructor, invariants)
    // ------------------------------------------------------------------

    #[test]
    fn section_new_stores_args_and_starts_closed() {
        let content = Dom::create_div().with_child(Dom::create_text("body"));
        let sec = AccordionSection::new("Title", content.clone());

        assert_eq!(sec.title.as_str(), "Title");
        assert_eq!(sec.content, content);
        assert!(!sec.is_open, "a fresh section must start collapsed");
    }

    #[test]
    fn section_new_survives_extreme_titles() {
        // empty, interior NUL, emoji + combining marks + RTL, and a 100k-char title
        let long = "ab".repeat(50_000);
        let cases: Vec<AzString> = alloc::vec![
            AzString::from(""),
            AzString::from("a\0b"),
            AzString::from("👨‍👩‍👧‍👦 e\u{0301}\u{0327} مرحبا שלום 🇩🇪"),
            AzString::from("\u{feff}\u{202e}rtl-override"),
            AzString::from(long.as_str()),
        ];

        for title in cases {
            let sec = AccordionSection::new(title.clone(), Dom::create_div());
            assert_eq!(sec.title.as_str(), title.as_str());
            assert!(!sec.is_open);

            // and the title survives the trip through the DOM unchanged
            let dom = Accordion::new(AccordionSectionVec::from_vec(alloc::vec![sec])).dom();
            let (header, _) = section_parts(&dom, 0);
            let title_node = &header.children.as_ref()[0];
            assert_eq!(text_of(title_node), Some(title.as_str()));
        }
    }

    #[test]
    fn section_with_open_sets_flag_without_touching_other_fields() {
        let content = Dom::create_text("x");
        let base = AccordionSection::new("t", content.clone());

        let opened = base.clone().with_open(true);
        assert!(opened.is_open);
        assert_eq!(opened.title.as_str(), "t");
        assert_eq!(opened.content, content);

        // last write wins; applying the same value twice is idempotent
        assert!(!base.clone().with_open(true).with_open(false).is_open);
        assert!(base.clone().with_open(false).with_open(true).is_open);
        assert!(base.clone().with_open(true).with_open(true).is_open);
        assert!(!base.with_open(false).is_open);
    }

    // ------------------------------------------------------------------
    // Accordion::new / create / Default
    // ------------------------------------------------------------------

    #[test]
    fn accordion_new_preserves_section_count_and_has_no_callback() {
        for count in [0usize, 1, 3, 1000] {
            let mut sections = Vec::with_capacity(count);
            for i in 0..count {
                sections.push(
                    AccordionSection::new(alloc::format!("s{i}"), Dom::create_div())
                        .with_open(i % 2 == 0),
                );
            }
            let acc = Accordion::new(AccordionSectionVec::from_vec(sections));

            assert_eq!(acc.sections.len(), count);
            assert!(acc.on_toggle.is_none(), "Accordion::new sets no callback");
            for (i, s) in acc.sections.as_ref().iter().enumerate() {
                assert_eq!(s.title.as_str(), alloc::format!("s{i}"));
                assert_eq!(s.is_open, i % 2 == 0);
            }
        }
    }

    #[test]
    fn accordion_create_is_empty_and_equals_default() {
        let acc = Accordion::create();
        assert!(acc.sections.is_empty());
        assert!(acc.on_toggle.is_none());
        assert_eq!(acc, Accordion::default());
    }

    // ------------------------------------------------------------------
    // set_on_toggle / with_on_toggle / swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn set_on_toggle_last_call_wins() {
        let mut acc = Accordion::create();

        acc.set_on_toggle(RefAny::new(1u8), toggle_cb(toggle_do_nothing));
        assert!(acc.on_toggle.is_some());
        assert_eq!(
            acc.on_toggle.as_ref().unwrap().refany.get_type_id(),
            RefAny::new(1u8).get_type_id()
        );

        // a second call must *replace* (not append / leak / panic)
        acc.set_on_toggle(RefAny::new(9i64), toggle_cb(record_toggle));
        let set = acc.on_toggle.as_ref().expect("still Some");
        assert_eq!(set.refany.get_type_id(), RefAny::new(0i64).get_type_id());
        assert_eq!(set.callback, toggle_cb(record_toggle));
        assert_ne!(set.callback, toggle_cb(toggle_do_nothing));
    }

    #[test]
    fn with_on_toggle_matches_set_on_toggle() {
        let built = Accordion::create().with_on_toggle(RefAny::new(7u32), toggle_cb(record_toggle));

        let mut mutated = Accordion::create();
        mutated.set_on_toggle(RefAny::new(7u32), toggle_cb(record_toggle));

        assert!(built.on_toggle.is_some());
        assert_eq!(
            built.on_toggle.as_ref().unwrap().callback,
            mutated.on_toggle.as_ref().unwrap().callback
        );
        // the builder form must not disturb the sections
        assert!(built.sections.is_empty());
    }

    #[test]
    fn swap_with_default_moves_all_state_out() {
        let sections = AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("a", Dom::create_div()),
            AccordionSection::new("b", Dom::create_div()).with_open(true),
        ]);
        let mut acc = Accordion::new(sections).with_on_toggle(RefAny::new(5u8), toggle_cb(record_toggle));

        let original = acc.swap_with_default();

        assert_eq!(original.sections.len(), 2);
        assert!(original.on_toggle.is_some());
        assert!(original.sections.as_ref()[1].is_open);

        assert!(acc.sections.is_empty(), "self must be left empty");
        assert!(acc.on_toggle.is_none(), "self must lose the callback");
        assert_eq!(acc, Accordion::create());

        // swapping an already-empty accordion is a no-op, not a panic
        let second = acc.swap_with_default();
        assert_eq!(second, Accordion::create());
        assert_eq!(acc, Accordion::create());
    }

    // ------------------------------------------------------------------
    // Accordion::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_empty_accordion_has_no_children() {
        let dom = Accordion::create().dom();
        assert!(has_class(&dom, "__azul-native-accordion"));
        assert!(dom.children.as_ref().is_empty());
        assert_eq!(dom.estimated_total_children, 0);
    }

    #[test]
    fn dom_display_follows_is_open() {
        let acc = Accordion::new(AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("closed", Dom::create_text("c0")),
            AccordionSection::new("open", Dom::create_text("c1")).with_open(true),
        ]));
        let dom = acc.dom();
        assert_eq!(dom.children.as_ref().len(), 2);

        let (h0, b0) = section_parts(&dom, 0);
        let (h1, b1) = section_parts(&dom, 1);

        assert!(has_class(h0, "__azul-native-accordion-header"));
        assert!(has_class(b0, "__azul-native-accordion-body"));

        // a closed section is `display: none`, an open one `display: block`
        assert_eq!(inline_display(b0), Some(LayoutDisplay::None));
        assert_eq!(inline_display(b1), Some(LayoutDisplay::Block));

        // the body wraps exactly the caller's content
        assert_eq!(text_of(&b0.children.as_ref()[0]), Some("c0"));
        assert_eq!(text_of(&b1.children.as_ref()[0]), Some("c1"));

        // the header is focusable and carries exactly one MouseUp callback
        for h in [h0, h1] {
            assert!(matches!(h.root.get_tab_index(), Some(TabIndex::Auto)));
            let cbs = h.root.get_callbacks();
            assert_eq!(cbs.len(), 1);
            assert_eq!(cbs.as_ref()[0].event, EventFilter::Hover(HoverEventFilter::MouseUp));
            assert_eq!(
                cbs.as_ref()[0].callback.cb,
                on_accordion_header_click as usize
            );
        }
    }

    #[test]
    fn dom_header_payload_carries_the_section_index_and_open_state() {
        let count = 64usize;
        let mut sections = Vec::with_capacity(count);
        for i in 0..count {
            sections.push(
                AccordionSection::new(alloc::format!("s{i}"), Dom::create_div())
                    .with_open(i % 3 == 0),
            );
        }
        let dom = Accordion::new(AccordionSectionVec::from_vec(sections)).dom();

        for i in 0..count {
            let (header, body) = section_parts(&dom, i);
            let mut payload = header.root.get_callbacks().as_ref()[0].refany.clone();
            let hd = payload
                .downcast_ref::<HeaderClickData>()
                .expect("header payload is a HeaderClickData");

            assert_eq!(hd.index, i, "each header must know its own section index");
            assert_eq!(hd.is_open, i % 3 == 0);
            assert!(hd.on_toggle.is_none(), "no user callback was set");
            assert_eq!(
                inline_display(body),
                Some(if i % 3 == 0 {
                    LayoutDisplay::Block
                } else {
                    LayoutDisplay::None
                })
            );
        }
    }

    #[test]
    fn dom_child_count_cache_stays_consistent() {
        // deeply nested content + many sections: `estimated_total_children` must
        // still equal the real descendant count, otherwise the compact-DOM arena
        // under-allocates and panics later.
        let mut deep = Dom::create_text("leaf");
        for _ in 0..64 {
            deep = Dom::create_div().with_child(deep);
        }

        let sections = AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("deep", deep),
            AccordionSection::new("flat", Dom::create_div()).with_open(true),
            AccordionSection::new("", Dom::create_div()),
        ]);
        let dom = Accordion::new(sections).dom();

        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children(),
            "cached descendant count desynced from the real tree"
        );
    }

    #[test]
    fn from_accordion_for_dom_matches_dom() {
        // Only meaningful for a section-less accordion: every `dom()` call mints
        // fresh per-header `RefAny`s, and two distinct `RefAny`s never compare equal.
        assert_eq!(Dom::from(Accordion::create()), Accordion::create().dom());
    }

    #[test]
    fn dom_leaves_the_original_on_toggle_payload_alive() {
        let log = RefAny::new(ToggleLog { calls: Vec::new() });
        let mut kept = log.clone();

        let acc = Accordion::new(AccordionSectionVec::from_vec(alloc::vec![
            AccordionSection::new("a", Dom::create_div()),
            AccordionSection::new("b", Dom::create_div()),
        ]))
        .with_on_toggle(log, toggle_cb(record_toggle));

        let dom = acc.dom();

        // every header got its own clone of the callback...
        for i in 0..2 {
            let (header, _) = section_parts(&dom, i);
            let mut payload = header.root.get_callbacks().as_ref()[0].refany.clone();
            let hd = payload.downcast_ref::<HeaderClickData>().unwrap();
            assert!(hd.on_toggle.is_some());
        }

        // ...and the caller's handle to the shared payload is still valid (no free)
        assert!(kept.downcast_ref::<ToggleLog>().unwrap().calls.is_empty());
    }

    // ------------------------------------------------------------------
    // clone_option_on_toggle
    // ------------------------------------------------------------------

    #[test]
    fn clone_option_on_toggle_of_none_is_none() {
        let none: OptionAccordionOnToggle = None.into();
        assert!(clone_option_on_toggle(&none).is_none());
        // cloning the clone stays None
        assert!(clone_option_on_toggle(&clone_option_on_toggle(&none)).is_none());
    }

    #[test]
    fn clone_option_on_toggle_shares_the_payload() {
        let mut some: OptionAccordionOnToggle = Some(AccordionOnToggle {
            callback: toggle_cb(record_toggle),
            refany: RefAny::new(0usize),
        })
        .into();

        let mut cloned = clone_option_on_toggle(&some);
        let cloned_inner = cloned.as_mut().expect("clone of Some must be Some");
        assert_eq!(cloned_inner.callback, toggle_cb(record_toggle));

        // the RefAny is shared, not deep-copied: a write through the clone is
        // visible through the original.
        *cloned_inner
            .refany
            .downcast_mut::<usize>()
            .expect("payload type is preserved") = 42;

        let original_inner = some.as_mut().unwrap();
        assert_eq!(*original_inner.refany.downcast_ref::<usize>().unwrap(), 42);
    }

    // ------------------------------------------------------------------
    // on_accordion_header_click
    // ------------------------------------------------------------------

    #[test]
    fn header_click_without_any_layout_result_is_a_noop() {
        let mut data = RefAny::new(HeaderClickData {
            index: 0,
            is_open: false,
            on_toggle: None.into(),
        });

        let (update, changes) = run_click(None, 0, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "nothing may be restyled without a body");
        assert!(!payload_is_open(&mut data), "state must not flip");
    }

    #[test]
    fn header_click_without_next_sibling_does_not_flip_state() {
        // node 2 is the *last* child -> no next sibling -> early return, and
        // crucially `is_open` must NOT have been toggled.
        let mut data = RefAny::new(HeaderClickData {
            index: 3,
            is_open: true,
            on_toggle: None.into(),
        });

        let (update, changes) = run_click(Some(header_body_dom()), 2, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(payload_is_open(&mut data), "state must be untouched");
    }

    #[test]
    fn header_click_with_stale_hit_node_is_a_noop() {
        let mut data = RefAny::new(HeaderClickData {
            index: 0,
            is_open: false,
            on_toggle: None.into(),
        });

        // node 999 does not exist in the 3-node fixture
        let (update, changes) = run_click(Some(header_body_dom()), 999, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(!payload_is_open(&mut data));
    }

    #[test]
    fn header_click_with_foreign_payload_is_a_noop() {
        // the callback-bearing node carries a RefAny of the *wrong* type
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign payload must not restyle the body"
        );
    }

    #[test]
    fn header_click_toggles_body_display_and_flips_state() {
        let mut data = RefAny::new(HeaderClickData {
            index: 0,
            is_open: false,
            on_toggle: None.into(),
        });

        // closed -> open
        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());
        assert_eq!(update, Update::DoNothing, "no user callback -> DoNothing");
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(2usize, LayoutDisplay::Block)]
        );
        assert!(payload_is_open(&mut data));

        // open -> closed (same payload, so the flip must be stateful)
        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());
        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(2usize, LayoutDisplay::None)]
        );
        assert!(!payload_is_open(&mut data));
    }

    #[test]
    fn header_click_invokes_user_callback_and_propagates_its_update() {
        let mut log = RefAny::new(ToggleLog { calls: Vec::new() });
        let data = RefAny::new(HeaderClickData {
            index: 17,
            is_open: false,
            on_toggle: Some(AccordionOnToggle {
                callback: toggle_cb(record_toggle),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) = run_click(Some(header_body_dom()), 1, data.clone());

        // the user's return value wins over the internal DoNothing
        assert_eq!(update, Update::RefreshDom);
        // ...and the body is still restyled, even though the user callback ran
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(2usize, LayoutDisplay::Block)]
        );
        assert_eq!(
            log.downcast_ref::<ToggleLog>().unwrap().calls.as_slice(),
            &[17],
            "the user callback must receive this section's index"
        );

        // a second click reports the same index again
        let (_, _) = run_click(Some(header_body_dom()), 1, data);
        assert_eq!(
            log.downcast_ref::<ToggleLog>().unwrap().calls.as_slice(),
            &[17, 17]
        );
    }
}

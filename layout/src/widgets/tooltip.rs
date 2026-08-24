//! Tooltip widget — wraps an arbitrary anchor [`Dom`] and shows a small text
//! popup near it while the pointer hovers, hiding it again on leave.
//!
//! ## Implementation note (CSS-based, see `TODO2` below)
//!
//! The drop-down popup path (`open_menu_for_hit_node` / `MenuPopupPosition`) is
//! built for *menus* — a list of clickable `MenuItem`s — not arbitrary text
//! shown next to an anchor, and it would also require a live window/hit-test to
//! verify. This widget therefore takes the simpler, fully-compilable and
//! self-contained CSS route the recipe allows: the tip is an absolutely-
//! positioned child of a `position: relative` wrapper, hidden by default
//! (`opacity: 0`) and revealed on `MouseEnter` / hidden on `MouseLeave` via
//! `set_css_property`. No user callbacks are needed — the show/hide handlers are
//! internal.
//!
//! TODO2: this is a CSS simplification of a "real" floating popover. The tip is
//! placed at a fixed offset below the anchor (it does not measure the anchor's
//! height, flip when near a screen edge, or escape an `overflow: hidden`
//! ancestor). A future revision could route through the window-popup / menu
//! popup path for true screen-anchored positioning once that is runtime-
//! verifiable.
//!
//! Key types: [`Tooltip`].

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::{OptionRefAny, RefAny},
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{LayoutDisplay, LayoutPosition, LayoutFlexGrow, LayoutTop, LayoutLeft, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPaddingBottom},
        property::{CssProperty, StyleWhiteSpaceValue},
        style::{StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextColor, StyleWhiteSpace, StyleOpacity},
    },
    AzString,
};

use crate::callbacks::CallbackInfo;

static TOOLTIP_WRAPPER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tooltip"))];
static TOOLTIP_TIP_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tooltip-tip"))];

// ---- layout (logical px) ----
/// Fixed vertical offset of the tip below the wrapper's top edge. A
/// simplification — see the module-level `TODO2`.
const TIP_OFFSET_Y: isize = 22;
const TIP_RADIUS: isize = 4;

// ---- colours ----
/// Tip background (#333333, dark).
const TIP_BG_COLOR: ColorU = ColorU {
    r: 51,
    g: 51,
    b: 51,
    a: 240,
};
/// Tip text colour (white).
const TIP_TEXT_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};

const TIP_BG_ITEMS: &[StyleBackgroundContent] = &[StyleBackgroundContent::Color(TIP_BG_COLOR)];
const TIP_BG: StyleBackgroundContentVec = StyleBackgroundContentVec::from_const_slice(TIP_BG_ITEMS);

/// Wrapper around the anchor: an inline-block positioning context so the
/// absolutely-positioned tip is placed relative to it.
static TOOLTIP_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// The tip itself: absolutely positioned, hidden by default (`opacity: 0`).
static TOOLTIP_TIP_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(TIP_OFFSET_Y))),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        8,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(4))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(4),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(TIP_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(TIP_BG)),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TIP_TEXT_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(12))),
    // Preserve the tip on one line so it does not wrap into the anchor's width.
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Nowrap,
    ))),
    // Hidden until hovered.
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(0))),
];

/// A tooltip: an anchor [`Dom`] plus the text shown on hover.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Tooltip {
    /// The element the tooltip is attached to.
    pub anchor: Dom,
    /// The text shown in the tip popup.
    pub text: AzString,
    /// Style of the positioning wrapper around the anchor.
    pub wrapper_style: CssPropertyWithConditionsVec,
    /// Style of the tip popup.
    pub tip_style: CssPropertyWithConditionsVec,
}

impl Default for Tooltip {
    fn default() -> Self {
        Self::new(Dom::default(), AzString::from_const_str(""))
    }
}

impl Tooltip {
    /// Creates a tooltip wrapping `anchor` that shows `text` on hover.
    #[must_use] pub fn new(anchor: Dom, text: AzString) -> Self {
        Self {
            anchor,
            text,
            wrapper_style: CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_WRAPPER_STYLE),
            tip_style: CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_TIP_STYLE),
        }
    }

    /// Sets the tip text.
    #[inline]
    pub fn set_text(&mut self, text: AzString) {
        self.text = text;
    }

    /// Builder-style setter for the tip text.
    #[inline]
    #[must_use] pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// Overrides the tip popup style.
    #[inline]
    pub fn set_tip_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.tip_style = style;
    }

    /// Builder-style setter for the tip popup style.
    #[inline]
    #[must_use] pub fn with_tip_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_tip_style(style);
        self
    }

    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    #[must_use] pub fn dom(self) -> Dom {
        // The hover handlers only navigate the DOM (the tip is found relative to
        // the hovered wrapper), so no per-tooltip state is needed.
        let marker = RefAny::new(());

        let tip = crate::widgets::widget_p_with_text(self.text)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOOLTIP_TIP_CLASS))
            .with_css_props(self.tip_style);

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOOLTIP_WRAPPER_CLASS))
            .with_css_props(self.wrapper_style)
            .with_callbacks(
                vec![
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseEnter),
                        callback: CoreCallback {
                            cb: on_tooltip_enter as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: marker.clone(),
                    },
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseLeave),
                        callback: CoreCallback {
                            cb: on_tooltip_leave as usize,
                            ctx: OptionRefAny::None,
                        },
                        refany: marker,
                    },
                ]
                .into(),
            )
            // children: [anchor, tip] — the tip is the anchor's next sibling.
            .with_children(vec![self.anchor, tip].into())
    }
}

/// Returns the tip node (the second child) of the hovered wrapper.
fn tip_of_wrapper(info: &CallbackInfo) -> Option<azul_core::dom::DomNodeId> {
    let wrapper = info.get_hit_node();
    let anchor = info.get_first_child(wrapper)?;
    info.get_next_sibling(anchor)
}

/// Pointer entered the wrapper → reveal the tip.
extern "C" fn on_tooltip_enter(_data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(tip) = tip_of_wrapper(&info) {
        info.set_css_property(tip, CssProperty::const_opacity(StyleOpacity::const_new(100)));
    }
    Update::DoNothing
}

/// Pointer left the wrapper → hide the tip.
extern "C" fn on_tooltip_leave(_data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(tip) = tip_of_wrapper(&info) {
        info.set_css_property(tip, CssProperty::const_opacity(StyleOpacity::const_new(0)));
    }
    Update::DoNothing
}

impl From<Tooltip> for Dom {
    fn from(t: Tooltip) -> Self {
        t.dom()
    }
}

#[cfg(test)]
// `assertions_on_constants`: these are deliberate invariant guards over sibling
// `const`s in this module. They are const-foldable *today*, which is exactly the
// point — they must go red the moment someone edits one of those constants into an
// inconsistent value. Deleting them (clippy's suggestion) would delete the check.
#[allow(clippy::assertions_on_constants)]
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
    use azul_css::{props::property::CssPropertyType, system::SystemStyle};
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
    // Helpers — DOM inspection
    // ------------------------------------------------------------------

    const WRAPPER_CLASS_NAME: &str = "__azul-native-tooltip";
    const TIP_CLASS_NAME: &str = "__azul-native-tooltip-tip";

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
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

    /// The *inline* properties a rendered node carries, in declaration order.
    fn inline_properties(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The property *types* of a style vec, in declaration order.
    fn prop_types(style: &CssPropertyWithConditionsVec) -> Vec<CssPropertyType> {
        style
            .as_ref()
            .iter()
            .map(|p| p.property.get_type())
            .collect()
    }

    /// *Every* opacity declared in a style vec, normalized to `0.0..=1.0`, in
    /// declaration order. More than one entry means the later one silently
    /// shadows the earlier — the tip would then not be hidden by default.
    fn declared_opacities(style: &CssPropertyWithConditionsVec) -> Vec<f32> {
        style
            .as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Opacity(v) => v.get_property().map(|o| o.inner.normalized()),
                _ => None,
            })
            .collect()
    }

    fn declared_positions(style: &CssPropertyWithConditionsVec) -> Vec<LayoutPosition> {
        style
            .as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Position(v) => v.get_property().copied(),
                _ => None,
            })
            .collect()
    }

    fn declared_displays(style: &CssPropertyWithConditionsVec) -> Vec<LayoutDisplay> {
        style
            .as_ref()
            .iter()
            .filter_map(|p| match &p.property {
                CssProperty::Display(v) => v.get_property().copied(),
                _ => None,
            })
            .collect()
    }

    /// The `CssPropertyType` of `opacity`, without hard-coding the enum variant.
    fn opacity_ty() -> CssPropertyType {
        CssProperty::const_opacity(StyleOpacity::const_new(0)).get_type()
    }

    /// A style vec built from an owned `Vec` (the shape a *user* override has).
    fn style_of(props: Vec<CssProperty>) -> CssPropertyWithConditionsVec {
        props
            .into_iter()
            .map(CssPropertyWithConditions::simple)
            .collect::<Vec<_>>()
            .into()
    }

    /// A `Dom` nested `depth` levels deep — a stress input for the recursive
    /// child bookkeeping `dom()` relies on.
    fn nested_anchor(depth: usize) -> Dom {
        let mut d = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("leaf"));
        for _ in 0..depth {
            d = Dom::create_div().with_child(d);
        }
        d
    }

    /// Inputs chosen to break text handling: empty, whitespace-only, interior
    /// NULs, control characters, astral-plane emoji, ZWJ sequences, stacked
    /// combining marks, a bidi override, a BOM, and two very large strings.
    fn adversarial_texts() -> Vec<String> {
        vec![
            String::new(),
            " ".to_string(),
            "\0".to_string(),
            "a\0b\0".to_string(),
            "\n\r\t\u{0b}\u{0c}".to_string(),
            "🦀".to_string(),
            "👨‍👩‍👧‍👦".to_string(),
            "e\u{0301}\u{0301}\u{0301}\u{0301}".to_string(),
            "\u{202e}gnirts detrevni".to_string(),
            "\u{feff}bom-prefixed".to_string(),
            "ｆｕｌｌｗｉｄｔｈ".to_string(),
            "\u{fdfa}".to_string(),
            "line\nbreak".to_string(),
            "a".repeat(100_000),
            "🦀".repeat(50_000),
        ]
    }

    // ------------------------------------------------------------------
    // Helpers — callback harness
    // ------------------------------------------------------------------

    /// A `DomLayoutResult` with an *empty* layout tree: the hover handlers only
    /// walk `styled_dom.node_hierarchy`, so no real layout (and no font) is
    /// needed.
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

    fn node(index: usize) -> NodeHierarchyItemId {
        NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index)))
    }

    /// Runs `f` against a `CallbackInfo` whose hit node is `hit`, backed by a
    /// `LayoutWindow` holding `styled` (or holding nothing at all, when `styled`
    /// is `None`). Returns `f`'s result plus every recorded `CallbackChange`.
    fn with_info<R>(
        styled: Option<StyledDom>,
        hit: NodeHierarchyItemId,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
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
                node: hit,
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let out = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    /// Every recorded CSS write, as `(node index, properties)`.
    fn css_writes(changes: &[CallbackChange]) -> Vec<(usize, Vec<CssProperty>)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeCssProperties {
                    node_id, properties, ..
                } => Some((node_id.index(), properties.as_ref().to_vec())),
                _ => None,
            })
            .collect()
    }

    /// Every recorded opacity write, as `(node index, normalized opacity)`.
    fn opacity_writes(changes: &[CallbackChange]) -> Vec<(usize, f32)> {
        let mut out = Vec::new();
        for (idx, props) in css_writes(changes) {
            for p in &props {
                if let CssProperty::Opacity(v) = p {
                    if let Some(o) = v.get_property() {
                        out.push((idx, o.inner.normalized()));
                    }
                }
            }
        }
        out
    }

    /// Index of the first node carrying `class` in a flattened `StyledDom`.
    fn index_of_class(styled: &StyledDom, class: &str) -> Option<usize> {
        styled.node_data.as_ref().iter().position(|nd| {
            nd.get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, Class(s) if s.as_str() == class))
        })
    }

    /// A three-node styled DOM — `root(0)` with children `anchor(1)` and
    /// `tip(2)` — i.e. the exact hierarchy `tip_of_wrapper` walks.
    fn anchor_tip_dom() -> StyledDom {
        let styled = StyledDom::create_from_dom(
            Dom::create_div()
                .with_child(Dom::create_div())
                .with_child(Dom::create_div()),
        );
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            3,
            "fixture must flatten to exactly wrapper/anchor/tip"
        );
        styled
    }

    // ------------------------------------------------------------------
    // Tooltip::new / Default
    // ------------------------------------------------------------------

    #[test]
    fn new_stores_anchor_and_text_verbatim() {
        let anchor = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("anchor"));
        let text = AzString::from("tip".to_string());
        let t = Tooltip::new(anchor.clone(), text.clone());

        assert_eq!(t.anchor, anchor, "the anchor must be stored unmodified");
        assert_eq!(t.text, text, "the text must be stored unmodified");
    }

    #[test]
    fn new_uses_the_static_style_tables() {
        let t = Tooltip::new(Dom::create_div(), AzString::from_const_str("x"));

        assert_eq!(
            t.wrapper_style,
            CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_WRAPPER_STYLE)
        );
        assert_eq!(
            t.tip_style,
            CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_TIP_STYLE)
        );
        assert_eq!(t.wrapper_style.len(), TOOLTIP_WRAPPER_STYLE.len());
        assert_eq!(t.tip_style.len(), TOOLTIP_TIP_STYLE.len());
    }

    #[test]
    fn new_is_pure_and_independent_of_the_arguments() {
        // The style tables must not vary with the anchor/text — a widget whose
        // styling depended on its content would be unstyleable.
        let a = Tooltip::new(Dom::create_div(), AzString::from_const_str(""));
        let b = Tooltip::new(
            nested_anchor(8),
            AzString::from("🦀".repeat(1000)),
        );

        assert_eq!(a.wrapper_style, b.wrapper_style);
        assert_eq!(a.tip_style, b.tip_style);
    }

    #[test]
    fn new_survives_adversarial_text() {
        for s in adversarial_texts() {
            let t = Tooltip::new(Dom::create_div(), AzString::from(s.clone()));
            assert_eq!(
                t.text.as_str(),
                s.as_str(),
                "text must round-trip byte-for-byte through AzString"
            );
            assert_eq!(
                t.text.as_str().len(),
                s.len(),
                "byte length must be preserved (no re-encoding / truncation at NUL)"
            );
        }
    }

    #[test]
    fn new_with_a_deeply_nested_anchor_keeps_the_child_count_consistent() {
        let anchor = nested_anchor(64);
        let expected = anchor.estimated_total_children;
        let t = Tooltip::new(anchor.clone(), AzString::from_const_str("deep"));

        assert_eq!(t.anchor, anchor);
        assert_eq!(
            t.anchor.estimated_total_children, expected,
            "the constructor must not disturb the anchor's cached descendant count"
        );
    }

    #[test]
    fn new_with_a_very_wide_anchor_does_not_panic() {
        let anchor = Dom::create_div()
            .with_children((0..2000).map(|_| Dom::create_div()).collect::<Vec<_>>().into());
        let t = Tooltip::new(anchor, AzString::from_const_str("wide"));

        assert_eq!(t.anchor.children.as_ref().len(), 2000);
    }

    #[test]
    fn default_is_an_empty_body_anchor_with_empty_text() {
        let d = Tooltip::default();

        assert_eq!(d.text.as_str(), "");
        assert_eq!(d.anchor, Dom::default());
        assert_eq!(
            d,
            Tooltip::new(Dom::default(), AzString::from_const_str("")),
            "Default must agree with the documented constructor call"
        );
    }

    // ------------------------------------------------------------------
    // set_text / with_text
    // ------------------------------------------------------------------

    #[test]
    fn set_text_and_with_text_agree_and_touch_nothing_else() {
        for s in adversarial_texts() {
            let base = Tooltip::new(
                Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a")),
                AzString::from_const_str("initial"),
            );

            let mut mutated = base.clone();
            mutated.set_text(AzString::from(s.clone()));
            let built = base.clone().with_text(AzString::from(s.clone()));

            assert_eq!(mutated, built, "with_text must be set_text + self");
            assert_eq!(mutated.text.as_str(), s.as_str());
            assert_eq!(mutated.anchor, base.anchor, "the anchor must be untouched");
            assert_eq!(mutated.wrapper_style, base.wrapper_style);
            assert_eq!(mutated.tip_style, base.tip_style);
        }
    }

    #[test]
    fn set_text_is_last_write_wins() {
        let mut t = Tooltip::default();
        let huge = "x".repeat(200_000);

        t.set_text(AzString::from(huge.clone()));
        assert_eq!(t.text.as_str().len(), huge.len());

        t.set_text(AzString::from_const_str(""));
        assert_eq!(t.text.as_str(), "", "a later empty write must win");

        t.set_text(AzString::from("🦀".to_string()));
        assert_eq!(t.text.as_str(), "🦀");
    }

    #[test]
    fn text_and_tip_style_setters_commute() {
        let style = style_of(vec![CssProperty::const_opacity(StyleOpacity::const_new(42))]);
        let text = AzString::from("both".to_string());

        let a = Tooltip::default()
            .with_text(text.clone())
            .with_tip_style(style.clone());
        let b = Tooltip::default()
            .with_tip_style(style)
            .with_text(text);

        assert_eq!(a, b, "the two builder setters must be independent");
    }

    // ------------------------------------------------------------------
    // set_tip_style / with_tip_style
    // ------------------------------------------------------------------

    #[test]
    fn set_tip_style_and_with_tip_style_agree() {
        let style = style_of(vec![
            CssProperty::const_position(LayoutPosition::Fixed),
            CssProperty::const_opacity(StyleOpacity::const_new(100)),
        ]);

        let mut mutated = Tooltip::default();
        mutated.set_tip_style(style.clone());
        let built = Tooltip::default().with_tip_style(style.clone());

        assert_eq!(mutated, built);
        assert_eq!(mutated.tip_style, style, "the style must be stored verbatim");
    }

    #[test]
    fn set_tip_style_does_not_touch_the_wrapper_style() {
        // The wrapper carries `position: relative`; losing it would make the
        // absolutely-positioned tip escape to the nearest positioned ancestor.
        let base = Tooltip::default();
        let mut t = base.clone();
        t.set_tip_style(CssPropertyWithConditionsVec::from_const_slice(&[]));

        assert_eq!(t.wrapper_style, base.wrapper_style);
        assert_eq!(t.text, base.text);
        assert_eq!(t.anchor, base.anchor);
    }

    #[test]
    fn tip_style_can_be_emptied_and_the_widget_still_builds() {
        let t = Tooltip::new(Dom::create_div(), AzString::from_const_str("naked"))
            .with_tip_style(CssPropertyWithConditionsVec::from_const_slice(&[]));
        assert_eq!(t.tip_style.len(), 0);

        let dom = t.dom();
        let tip = &dom.children.as_ref()[1];
        assert!(
            inline_properties(tip).is_empty(),
            "an empty override must produce an unstyled tip, not the default table"
        );
        assert_eq!(text_of(tip), Some("naked"));
    }

    #[test]
    fn a_huge_tip_style_is_stored_verbatim() {
        let props: Vec<CssProperty> = (0..10_000)
            .map(|i| CssProperty::const_opacity(StyleOpacity::const_new(i % 101)))
            .collect();
        let style = style_of(props);

        let t = Tooltip::default().with_tip_style(style.clone());
        assert_eq!(t.tip_style.len(), 10_000);
        assert_eq!(t.tip_style, style);

        let dom = t.dom();
        assert_eq!(
            inline_properties(&dom.children.as_ref()[1]).len(),
            10_000,
            "every declaration must survive the DOM build"
        );
    }

    // ------------------------------------------------------------------
    // swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_old_value_and_leaves_a_default() {
        let original = Tooltip::new(
            Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("anchor")),
            AzString::from("tip".to_string()),
        )
        .with_tip_style(style_of(vec![CssProperty::const_opacity(
            StyleOpacity::const_new(7),
        )]));

        let mut t = original.clone();
        let taken = t.swap_with_default();

        assert_eq!(taken, original, "the previous value must be handed back");
        assert_eq!(t, Tooltip::default(), "self must be left as a default");
    }

    #[test]
    fn swap_with_default_is_stable_under_repetition() {
        let mut t = Tooltip::default().with_text(AzString::from("a".repeat(50_000)));

        let first = t.swap_with_default();
        assert_eq!(first.text.as_str().len(), 50_000);

        for _ in 0..10 {
            let again = t.swap_with_default();
            assert_eq!(again, Tooltip::default());
            assert_eq!(t, Tooltip::default());
        }
    }

    #[test]
    fn swap_with_default_on_a_default_is_an_identity() {
        let mut t = Tooltip::default();
        let taken = t.swap_with_default();

        assert_eq!(taken, Tooltip::default());
        assert_eq!(t, Tooltip::default());
    }

    // ------------------------------------------------------------------
    // Static style tables
    // ------------------------------------------------------------------

    #[test]
    fn tip_style_starts_hidden_with_exactly_one_opacity_declaration() {
        // Two opacity declarations would make the last one win and could leave
        // the tip permanently visible.
        assert_eq!(
            declared_opacities(&CssPropertyWithConditionsVec::from_const_slice(
                TOOLTIP_TIP_STYLE
            )),
            vec![0.0],
            "the tip must be hidden by default via a single opacity declaration"
        );
    }

    #[test]
    fn tip_style_is_absolutely_positioned_and_does_not_wrap() {
        let style = CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_TIP_STYLE);

        assert_eq!(declared_positions(&style), vec![LayoutPosition::Absolute]);
        assert!(
            style.as_ref().contains(&CssPropertyWithConditions::simple(
                CssProperty::const_top(LayoutTop::const_px(TIP_OFFSET_Y))
            )),
            "the documented vertical offset must be declared"
        );
        assert!(
            style.as_ref().contains(&CssPropertyWithConditions::simple(
                CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap))
            )),
            "the tip must stay on one line"
        );
    }

    #[test]
    fn wrapper_style_is_an_inline_block_positioning_context() {
        let style = CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_WRAPPER_STYLE);

        assert_eq!(declared_displays(&style), vec![LayoutDisplay::InlineBlock]);
        assert_eq!(
            declared_positions(&style),
            vec![LayoutPosition::Relative],
            "without `position: relative` the tip would anchor to some ancestor"
        );
        assert!(
            style.as_ref().contains(&CssPropertyWithConditions::simple(
                CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))
            )),
            "the wrapper must not grow past the anchor"
        );
    }

    #[test]
    fn neither_style_table_declares_a_property_type_twice() {
        for (name, table) in [
            ("wrapper", TOOLTIP_WRAPPER_STYLE),
            ("tip", TOOLTIP_TIP_STYLE),
        ] {
            let style = CssPropertyWithConditionsVec::from_const_slice(table);
            let mut types = prop_types(&style);
            let declared = types.len();
            assert!(declared > 0, "{name} style must not be empty");
            types.sort_unstable();
            types.dedup();
            assert_eq!(
                types.len(),
                declared,
                "{name}: a duplicated property type would make the later declaration \
                 silently win"
            );
        }
    }

    #[test]
    fn both_style_tables_apply_unconditionally() {
        for table in [TOOLTIP_WRAPPER_STYLE, TOOLTIP_TIP_STYLE] {
            assert!(
                table.iter().all(|p| p.apply_if.as_ref().is_empty()),
                "a stray condition would leave the tooltip unstyled"
            );
        }
    }

    #[test]
    fn tip_colours_are_opaque_enough_to_read() {
        assert!(
            TIP_BG_COLOR.a > 200,
            "a near-transparent tip background would be unreadable"
        );
        assert_eq!(TIP_TEXT_COLOR.a, 255);
        assert!(TIP_RADIUS >= 0 && TIP_OFFSET_Y > 0);
    }

    // ------------------------------------------------------------------
    // Tooltip::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_builds_a_wrapper_with_the_anchor_then_the_tip() {
        let anchor = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("anchor"));
        let dom = Tooltip::new(anchor.clone(), AzString::from_const_str("tip")).dom();

        assert!(has_class(&dom, WRAPPER_CLASS_NAME));
        assert_eq!(dom.root.get_node_type(), &NodeType::Div);

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2, "children must be exactly [anchor, tip]");
        assert_eq!(children[0], anchor, "child 0 must be the anchor, verbatim");
        assert!(
            has_class(&children[1], TIP_CLASS_NAME),
            "child 1 must be the tip"
        );
        assert_eq!(text_of(&children[1]), Some("tip"));
        assert_eq!(
            inline_properties(&children[1]).len(),
            TOOLTIP_TIP_STYLE.len(),
            "the tip must carry the full tip style"
        );
        assert_eq!(
            inline_properties(&dom).len(),
            TOOLTIP_WRAPPER_STYLE.len(),
            "the wrapper must carry the full wrapper style"
        );
    }

    #[test]
    fn dom_preserves_adversarial_text_byte_for_byte() {
        for s in adversarial_texts() {
            let dom = Tooltip::new(Dom::create_div(), AzString::from(s.clone())).dom();
            let tip = &dom.children.as_ref()[1];
            assert_eq!(
                text_of(tip),
                Some(s.as_str()),
                "the tip text must survive the DOM build unchanged"
            );
        }
    }

    #[test]
    fn dom_applies_a_custom_tip_style_to_the_tip_only() {
        let custom = style_of(vec![CssProperty::const_opacity(StyleOpacity::const_new(
            100,
        ))]);
        let dom = Tooltip::new(Dom::create_div(), AzString::from_const_str("t"))
            .with_tip_style(custom.clone())
            .dom();

        assert_eq!(
            inline_properties(&dom.children.as_ref()[1]),
            vec![CssProperty::const_opacity(StyleOpacity::const_new(100))],
            "the override must replace the default tip table"
        );
        assert_eq!(
            inline_properties(&dom).len(),
            TOOLTIP_WRAPPER_STYLE.len(),
            "the wrapper must keep its own style"
        );
    }

    #[test]
    fn dom_binds_exactly_mouse_enter_and_mouse_leave_on_the_wrapper() {
        let dom = Tooltip::new(Dom::create_div(), AzString::from_const_str("t")).dom();
        let callbacks = dom.root.callbacks.as_ref();

        assert_eq!(callbacks.len(), 2, "exactly two hover handlers are expected");
        assert_eq!(
            callbacks[0].event,
            EventFilter::Hover(HoverEventFilter::MouseEnter)
        );
        assert_eq!(
            callbacks[1].event,
            EventFilter::Hover(HoverEventFilter::MouseLeave)
        );
        assert_eq!(callbacks[0].callback.cb, on_tooltip_enter as usize);
        assert_eq!(callbacks[1].callback.cb, on_tooltip_leave as usize);
        assert!(matches!(callbacks[0].callback.ctx, OptionRefAny::None));
        assert!(matches!(callbacks[1].callback.ctx, OptionRefAny::None));
        assert_eq!(
            callbacks[0].refany, callbacks[1].refany,
            "both handlers must share one marker RefAny (they are stateless)"
        );
    }

    #[test]
    fn dom_binds_no_callbacks_on_the_anchor_or_the_tip() {
        let dom = Tooltip::new(
            Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a")),
            AzString::from_const_str("t"),
        )
        .dom();

        for (i, child) in dom.children.as_ref().iter().enumerate() {
            assert!(
                child.root.callbacks.as_ref().is_empty(),
                "child {i} must not carry hover handlers of its own"
            );
        }
    }

    #[test]
    fn from_impl_matches_dom_structurally() {
        // `dom()` mints a fresh marker `RefAny` per call, so the two DOMs are
        // deliberately compared field-by-field rather than with `==`.
        let make = || {
            Tooltip::new(
                Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a")),
                AzString::from_const_str("tip"),
            )
        };
        let via_from = Dom::from(make());
        let via_dom = make().dom();

        assert_eq!(via_from.root.get_node_type(), via_dom.root.get_node_type());
        assert_eq!(
            via_from.root.get_ids_and_classes().as_ref(),
            via_dom.root.get_ids_and_classes().as_ref()
        );
        assert_eq!(via_from.root.style, via_dom.root.style);
        assert_eq!(via_from.children.as_ref(), via_dom.children.as_ref());
        assert_eq!(
            via_from.estimated_total_children,
            via_dom.estimated_total_children
        );

        let (a, b) = (
            via_from.root.callbacks.as_ref(),
            via_dom.root.callbacks.as_ref(),
        );
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(b.iter()) {
            assert_eq!(x.event, y.event);
            assert_eq!(x.callback.cb, y.callback.cb);
        }
    }

    #[test]
    fn dom_keeps_the_estimated_child_count_consistent_with_the_flattened_tree() {
        // A too-small `estimated_total_children` makes the arena conversion
        // under-allocate and panic on out-of-bounds writes.
        for depth in [0, 1, 8, 64] {
            let dom = Tooltip::new(nested_anchor(depth), AzString::from_const_str("t")).dom();
            let estimated = dom.estimated_total_children;
            let flattened = StyledDom::create_from_dom(dom).node_hierarchy.as_ref().len();
            assert_eq!(
                flattened,
                estimated + 1,
                "depth {depth}: the cached descendant count disagrees with the flattened tree"
            );
        }
    }

    #[test]
    fn dom_of_a_very_wide_anchor_flattens_without_panicking() {
        let anchor = Dom::create_div()
            .with_children((0..2000).map(|_| Dom::create_div()).collect::<Vec<_>>().into());
        let dom = Tooltip::new(anchor, AzString::from_const_str("wide")).dom();

        let styled = StyledDom::create_from_dom(dom);
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            1 + 1 + 2000 + 2,
            "wrapper + anchor + 2000 grandchildren + tip <p> + tip text"
        );
    }

    #[test]
    fn dom_of_nested_tooltips_keeps_each_tip_as_the_second_child() {
        let inner = Tooltip::new(Dom::create_div(), AzString::from_const_str("inner")).dom();
        let outer = Tooltip::new(inner, AzString::from_const_str("outer")).dom();

        let outer_children = outer.children.as_ref();
        assert_eq!(outer_children.len(), 2);
        assert_eq!(text_of(&outer_children[1]), Some("outer"));

        let inner_children = outer_children[0].children.as_ref();
        assert_eq!(inner_children.len(), 2);
        assert_eq!(text_of(&inner_children[1]), Some("inner"));
    }

    // ------------------------------------------------------------------
    // tip_of_wrapper
    // ------------------------------------------------------------------

    #[test]
    fn tip_of_wrapper_without_a_layout_result_is_none() {
        let (tip, changes) = with_info(None, node(0), |info| tip_of_wrapper(&info));
        assert_eq!(tip, None);
        assert!(changes.is_empty());
    }

    #[test]
    fn tip_of_wrapper_with_a_stale_hit_node_is_none() {
        for stale in [3usize, 999, usize::MAX / 2] {
            let (tip, _) = with_info(Some(anchor_tip_dom()), node(stale), |info| {
                tip_of_wrapper(&info)
            });
            assert_eq!(tip, None, "node {stale} does not exist in the 3-node fixture");
        }
    }

    #[test]
    fn tip_of_wrapper_with_a_none_hit_node_is_none() {
        let (tip, _) = with_info(
            Some(anchor_tip_dom()),
            NodeHierarchyItemId::NONE,
            |info| tip_of_wrapper(&info),
        );
        assert_eq!(tip, None, "an unset hit node must not resolve to a tip");
    }

    #[test]
    fn tip_of_wrapper_on_a_childless_node_is_none() {
        // node 1 is a leaf -> no first child -> no tip.
        let (tip, _) = with_info(Some(anchor_tip_dom()), node(1), |info| tip_of_wrapper(&info));
        assert_eq!(tip, None);
    }

    #[test]
    fn tip_of_wrapper_without_a_second_child_is_none() {
        let styled = StyledDom::create_from_dom(Dom::create_div().with_child(Dom::create_div()));
        let (tip, _) = with_info(Some(styled), node(0), |info| tip_of_wrapper(&info));
        assert_eq!(
            tip, None,
            "a wrapper with a single child has no tip to reveal"
        );
    }

    #[test]
    fn tip_of_wrapper_returns_the_second_child() {
        let (tip, _) = with_info(Some(anchor_tip_dom()), node(0), |info| tip_of_wrapper(&info));
        assert_eq!(
            tip.and_then(|t| t.node.into_crate_internal()).map(|n| n.index()),
            Some(2)
        );
    }

    #[test]
    fn tip_of_wrapper_finds_the_tip_of_a_real_tooltip_dom() {
        // The anchor has a subtree of its own, so the tip is *not* simply
        // `hit + 1` — the handler must walk first-child -> next-sibling.
        let dom = Tooltip::new(nested_anchor(3), AzString::from_const_str("tip")).dom();
        let styled = StyledDom::create_from_dom(dom);
        let wrapper = index_of_class(&styled, WRAPPER_CLASS_NAME).expect("wrapper class missing");
        let expected = index_of_class(&styled, TIP_CLASS_NAME).expect("tip class missing");
        assert!(
            expected > wrapper + 1,
            "fixture must have a non-trivial anchor subtree between wrapper and tip"
        );

        let (tip, _) = with_info(Some(styled), node(wrapper), |info| tip_of_wrapper(&info));
        assert_eq!(
            tip.and_then(|t| t.node.into_crate_internal()).map(|n| n.index()),
            Some(expected)
        );
    }

    // ------------------------------------------------------------------
    // on_tooltip_enter / on_tooltip_leave
    // ------------------------------------------------------------------

    #[test]
    fn enter_reveals_and_leave_hides_exactly_the_tip() {
        for (name, handler, expected) in [
            (
                "enter",
                on_tooltip_enter as extern "C" fn(RefAny, CallbackInfo) -> Update,
                1.0_f32,
            ),
            (
                "leave",
                on_tooltip_leave as extern "C" fn(RefAny, CallbackInfo) -> Update,
                0.0_f32,
            ),
        ] {
            let (update, changes) = with_info(Some(anchor_tip_dom()), node(0), |info| {
                handler(RefAny::new(()), info)
            });

            assert_eq!(update, Update::DoNothing, "{name} must not relayout");
            assert_eq!(
                opacity_writes(&changes),
                vec![(2, expected)],
                "{name} must write exactly one opacity, on the tip node"
            );
            let writes = css_writes(&changes);
            assert_eq!(
                writes.len(),
                changes.len(),
                "{name} must only record CSS writes"
            );
            assert_eq!(writes[0].1.len(), 1, "{name} must write a single property");
        }
    }

    #[test]
    fn leave_restores_the_opacity_declared_in_the_static_tip_style() {
        // Round-trip: what the handler writes on leave must be exactly what the
        // stylesheet declares, otherwise the tip would not return to its
        // initial rendering.
        let declared = declared_opacities(&CssPropertyWithConditionsVec::from_const_slice(
            TOOLTIP_TIP_STYLE,
        ));
        let (_, changes) = with_info(Some(anchor_tip_dom()), node(0), |info| {
            on_tooltip_leave(RefAny::new(()), info)
        });

        assert_eq!(
            opacity_writes(&changes).iter().map(|(_, o)| *o).collect::<Vec<_>>(),
            declared
        );
    }

    #[test]
    fn enter_then_leave_is_a_round_trip() {
        let enter = with_info(Some(anchor_tip_dom()), node(0), |info| {
            on_tooltip_enter(RefAny::new(()), info)
        })
        .1;
        let leave = with_info(Some(anchor_tip_dom()), node(0), |info| {
            on_tooltip_leave(RefAny::new(()), info)
        })
        .1;

        let (e, l) = (opacity_writes(&enter), opacity_writes(&leave));
        assert_eq!(e.len(), 1);
        assert_eq!(l.len(), 1);
        assert_eq!(e[0].0, l[0].0, "both must target the same node");
        assert!(
            e[0].1 > l[0].1,
            "enter must make the tip more visible than leave ({} vs {})",
            e[0].1,
            l[0].1
        );
        assert_eq!((e[0].1, l[0].1), (1.0, 0.0));
    }

    #[test]
    fn handlers_are_noops_when_there_is_no_tip() {
        let fixtures: Vec<(&str, Option<StyledDom>, NodeHierarchyItemId)> = vec![
            ("no layout result", None, node(0)),
            ("stale hit node", Some(anchor_tip_dom()), node(999)),
            ("none hit node", Some(anchor_tip_dom()), NodeHierarchyItemId::NONE),
            ("leaf hit node", Some(anchor_tip_dom()), node(1)),
            (
                "single child",
                Some(StyledDom::create_from_dom(
                    Dom::create_div().with_child(Dom::create_div()),
                )),
                node(0),
            ),
        ];

        for (name, styled, hit) in fixtures {
            for handler in [
                on_tooltip_enter as extern "C" fn(RefAny, CallbackInfo) -> Update,
                on_tooltip_leave as extern "C" fn(RefAny, CallbackInfo) -> Update,
            ] {
                let (update, changes) =
                    with_info(styled.clone(), hit, |info| handler(RefAny::new(()), info));
                assert_eq!(update, Update::DoNothing, "{name}");
                assert!(
                    changes.is_empty(),
                    "{name}: nothing may be restyled without a tip"
                );
            }
        }
    }

    #[test]
    fn handlers_ignore_their_payload() {
        // The handlers are stateless — a foreign (or even empty) payload must
        // not change what they do.
        for data in [RefAny::new(()), RefAny::new(0xdead_beef_u64), RefAny::new(())] {
            let (update, changes) = with_info(Some(anchor_tip_dom()), node(0), |info| {
                on_tooltip_enter(data.clone(), info)
            });
            assert_eq!(update, Update::DoNothing);
            assert_eq!(opacity_writes(&changes), vec![(2, 1.0)]);
        }
    }

    #[test]
    fn repeated_enter_is_idempotent() {
        let mut all = Vec::new();
        for _ in 0..64 {
            let (update, changes) = with_info(Some(anchor_tip_dom()), node(0), |info| {
                on_tooltip_enter(RefAny::new(()), info)
            });
            assert_eq!(update, Update::DoNothing);
            all.push(opacity_writes(&changes));
        }
        assert!(
            all.iter().all(|w| *w == vec![(2, 1.0)]),
            "repeated hovers must keep producing the same single write"
        );
    }

    #[test]
    fn handlers_never_restyle_the_wrapper_or_the_anchor() {
        let dom = Tooltip::new(nested_anchor(2), AzString::from_const_str("tip")).dom();
        let styled = StyledDom::create_from_dom(dom);
        let wrapper = index_of_class(&styled, WRAPPER_CLASS_NAME).expect("wrapper class missing");
        let tip = index_of_class(&styled, TIP_CLASS_NAME).expect("tip class missing");

        for handler in [
            on_tooltip_enter as extern "C" fn(RefAny, CallbackInfo) -> Update,
            on_tooltip_leave as extern "C" fn(RefAny, CallbackInfo) -> Update,
        ] {
            let (_, changes) = with_info(Some(styled.clone()), node(wrapper), |info| {
                handler(RefAny::new(()), info)
            });
            let touched: Vec<usize> = css_writes(&changes).into_iter().map(|(i, _)| i).collect();
            assert_eq!(
                touched,
                vec![tip],
                "only the tip may be restyled, never the wrapper or the anchor subtree"
            );
        }
    }

    #[test]
    fn hovering_the_tip_itself_does_nothing() {
        let dom = Tooltip::new(Dom::create_div(), AzString::from_const_str("tip")).dom();
        let styled = StyledDom::create_from_dom(dom);
        let tip = index_of_class(&styled, TIP_CLASS_NAME).expect("tip class missing");

        let (update, changes) = with_info(Some(styled), node(tip), |info| {
            on_tooltip_enter(RefAny::new(()), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "the tip is a leaf text node — it has no tip of its own"
        );
    }

    #[test]
    fn every_written_property_is_an_opacity() {
        for handler in [
            on_tooltip_enter as extern "C" fn(RefAny, CallbackInfo) -> Update,
            on_tooltip_leave as extern "C" fn(RefAny, CallbackInfo) -> Update,
        ] {
            let (_, changes) = with_info(Some(anchor_tip_dom()), node(0), |info| {
                handler(RefAny::new(()), info)
            });
            for (_, props) in css_writes(&changes) {
                for p in props {
                    assert_eq!(
                        p.get_type(),
                        opacity_ty(),
                        "the hover handlers must only toggle opacity"
                    );
                }
            }
        }
    }
}

//! Tooltip widget — wraps an arbitrary anchor [`Dom`] and shows a small text
//! popup near it while the pointer hovers, hiding it again on leave.
//!
//! The tip is shown in the operating system's tooltip window
//! (`CallbackInfo::show_tooltip_at`, a transient window of the platform's
//! tooltip kind), opened under the anchor on `MouseEnter` and closed on
//! `MouseLeave`. A window is not clipped by the anchor's ancestors and does
//! not move the page. The tip's styled node stays in the DOM, hidden: it is
//! the anchor's accessible description and the look the window's text
//! follows where a platform draws the tooltip itself.
//!
//! Key types: [`Tooltip`].

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        AttributeType, Dom, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class,
        IdOrClassVec,
    },
    refany::{OptionRefAny, RefAny},
};
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec,
    },
    props::{
        basic::{color::ColorU, StyleFontSize},
        layout::{
            LayoutDisplay, LayoutFlexGrow, LayoutLeft, LayoutPaddingBottom, LayoutPaddingLeft,
            LayoutPaddingRight, LayoutPaddingTop, LayoutPosition, LayoutTop,
        },
        property::{CssProperty, StyleWhiteSpaceValue},
        style::{
            StyleBackgroundContent, StyleBackgroundContentVec, StyleBorderBottomLeftRadius,
            StyleBorderBottomRightRadius, StyleBorderTopLeftRadius, StyleBorderTopRightRadius,
            StyleOpacity, StyleTextColor, StyleWhiteSpace,
        },
    },
    AzString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::themes::{style_kit, OptionUiTheme, UiTheme},
};

static TOOLTIP_WRAPPER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tooltip"))];
static TOOLTIP_TIP_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tooltip-tip"))];

// ---- layout (logical px) ----
/// Fixed vertical offset of the tip below the wrapper's top edge. A
/// simplification — see the module-level `TODO2`.
pub(crate) const TIP_OFFSET_Y: isize = 22;
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
pub(crate) static TOOLTIP_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

// ---- R5: the tip's BASE - what every theme's tip shares ----
//
// A theme's tip is `TIP_BASE`, THEN its skin (paint and metrics):
// `TOOLTIP_TIP_STYLE` for flat, `themes::flora::tooltip_skin` for flora. The
// base comes first in every theme. The wrapper has no skin:
// `TOOLTIP_WRAPPER_STYLE` is its whole style in every theme.

/// The tip's structure, placement and starting state in every theme:
/// absolutely placed [`TIP_OFFSET_Y`] below the wrapper, on one line, and
/// hidden (`opacity: 0`) - the value the leave handler writes back; the
/// enter / leave handlers write the opacity and nothing else.
pub(crate) static TIP_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(TIP_OFFSET_Y))),
    CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
    // Preserve the tip on one line so it does not wrap into the anchor's width.
    CssPropertyWithConditions::simple(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
        StyleWhiteSpace::Nowrap,
    ))),
    // Hidden until hovered.
    CssPropertyWithConditions::simple(CssProperty::const_opacity(StyleOpacity::const_new(0))),
];

/// The tip: flat's dark chip, on [`TIP_BASE`].
pub(crate) static TOOLTIP_TIP_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(
        LayoutPaddingLeft::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(8),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
        4,
    ))),
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
];

/// A tooltip: an anchor [`Dom`] plus the text shown on hover.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Tooltip {
    /// The element the tooltip is attached to.
    pub anchor: Dom,
    /// The text shown in the tip popup.
    pub text: AzString,
    /// Style for the positioning wrapper, or `None` for "no opinion" — in which
    /// case the widget's default applies.
    ///
    /// `None` and `Some(empty)` are different answers: the first means the
    /// widget picks, the second means the caller asked for no properties at all
    /// and gets none.
    pub wrapper_style: OptionCssPropertyWithConditionsVec,
    /// Style of the tip popup.
    pub tip_style: OptionCssPropertyWithConditionsVec,
    /// The widget theme, or `None` to follow the app theme (`AppConfig::with_theme`). A
    /// theme is a DOM-level choice: it picks the skin the tip is built from,
    /// so switching it rebuilds the tooltip.
    pub theme: OptionUiTheme,
}

/// What a theme supplies for a tooltip: the wrapper and the tip. Every
/// theme's tip is absolutely placed [`TIP_OFFSET_Y`] below the wrapper and
/// starts hidden (`opacity: 0`) - the enter / leave handlers write its
/// opacity and nothing else. Built by `themes::flat::tooltip_skin` /
/// `themes::flora::tooltip_skin`.
pub(crate) struct TooltipSkin {
    pub theme: UiTheme,
    pub wrapper: CssPropertyWithConditionsVec,
    pub tip: CssPropertyWithConditionsVec,
}

/// The skin `theme` draws tooltips with.
#[must_use]
pub(crate) fn skin_for(theme: UiTheme) -> TooltipSkin {
    match theme {
        UiTheme::Flat => crate::widgets::themes::flat::tooltip_skin(),
        UiTheme::Flora => crate::widgets::themes::flora::tooltip_skin(),
    }
}

/// The skin a tooltip carrying `theme` renders with: the pinned theme's, or
/// - unpinned - the one of the theme the DOM is built for. What the render
/// and the resolvers both ask.
#[must_use]
pub(crate) fn skin_of(theme: OptionUiTheme) -> TooltipSkin {
    skin_for(crate::widgets::themes::theme_blocks::live_theme(theme))
}

impl Default for Tooltip {
    fn default() -> Self {
        Self::new(Dom::default(), AzString::from_const_str(""))
    }
}

impl Tooltip {
    /// The wrapper CSS this tooltip renders with.
    ///
    /// `None` means no opinion, so the built-in wrapper style applies. Named so
    /// the tests can ask what is rendered rather than reading the field, which
    /// is `None` until a caller sets one.
    #[must_use]
    pub fn resolved_wrapper_style(&self) -> CssPropertyWithConditionsVec {
        self.wrapper_style.clone().into_option().unwrap_or_else(|| {
            CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_WRAPPER_STYLE)
        })
    }

    /// The tip CSS this tooltip renders with.
    ///
    /// `None` means no opinion, so the theme's tip applies - asked of the same
    /// skin the render uses, so the two cannot drift.
    #[must_use]
    pub fn resolved_tip_style(&self) -> CssPropertyWithConditionsVec {
        self.tip_style
            .clone()
            .into_option()
            .unwrap_or_else(|| skin_of(self.theme).tip)
    }

    /// Creates a tooltip wrapping `anchor` that shows `text` on hover.
    #[must_use]
    pub const fn new(anchor: Dom, text: AzString) -> Self {
        Self {
            anchor,
            text,
            wrapper_style: OptionCssPropertyWithConditionsVec::None,
            tip_style: OptionCssPropertyWithConditionsVec::None,
            theme: OptionUiTheme::None,
        }
    }

    /// Pick the widget theme. Unset (`None`), the tooltip follows the
    /// app theme (`AppConfig::with_theme`, flat by default).
    #[inline]
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[inline]
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Sets the tip text.
    #[inline]
    pub fn set_text(&mut self, text: AzString) {
        self.text = text;
    }

    /// Builder-style setter for the tip text.
    #[inline]
    #[must_use]
    pub fn with_text(mut self, text: AzString) -> Self {
        self.set_text(text);
        self
    }

    /// Overrides the tip popup style.
    #[inline]
    pub fn set_tip_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.tip_style = OptionCssPropertyWithConditionsVec::Some(style);
    }

    /// Builder-style setter for the tip popup style.
    #[inline]
    #[must_use]
    pub fn with_tip_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_tip_style(style);
        self
    }

    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the tooltip. Rendering goes through the theme modules (as
    /// `Button::dom` does): each hands [`Self::build`] its skin. Unpinned
    /// (`theme: None`), the tooltip follows the APP theme: built in the look
    /// of the theme its DOM is built for.
    #[must_use]
    pub fn dom(self) -> Dom {
        match crate::widgets::themes::theme_blocks::live_theme(self.theme) {
            UiTheme::Flora => crate::widgets::themes::flora::tooltip(self),
            UiTheme::Flat => crate::widgets::themes::flat::tooltip(self),
        }
    }

    /// Renders the tooltip with `skin` styling the wrapper and the tip - what
    /// `themes::flat::tooltip` / `themes::flora::tooltip` call.
    #[must_use]
    pub(crate) fn build(self, skin: TooltipSkin) -> Dom {
        // What the tooltip window shows: the handlers get it with the event.
        let marker = RefAny::new(TooltipText(self.text.clone()));

        // Resolved before `self.text` is moved out below.
        let tip_css = self.tip_style.into_option().unwrap_or(skin.tip);
        let wrapper_css = self
            .wrapper_style

            .into_option()
            .unwrap_or(skin.wrapper);
        // A screen reader reads the tip's text as the anchor's description
        // ("Save, button, Saves the file"), as for `aria-describedby`. The tip
        // itself is drawn on hover only; read on its own it would be a stray
        // text after the anchor, so it is hidden from assistive technology.
        let mut anchor = self.anchor;
        let mut anchor_info = anchor
            .root
            .get_accessibility_info()
            .cloned()
            .unwrap_or_default();
        if anchor_info.description.is_none() && !self.text.as_str().trim().is_empty() {
            anchor_info.description = Some(self.text.clone()).into();
            anchor.root.set_accessibility_info(anchor_info);
        }
        let tip = crate::widgets::widget_p_with_text(self.text)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOOLTIP_TIP_CLASS))
            .with_attribute(AttributeType::Hidden)
            .with_css_props(tip_css);

        let mut classes: Vec<IdOrClass> = TOOLTIP_WRAPPER_CLASS.to_vec();
        classes.push(style_kit::marker(skin.theme));
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css_props(wrapper_css)
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
            .with_children(vec![anchor, tip].into())
    }
}

/// The text a tooltip's hover handlers show, carried as their payload.
struct TooltipText(
    /// The tip's text, as the widget was built with it.
    AzString,
);

/// How far below its anchor the tooltip window opens, in logical px.
const TIP_GAP: f32 = 4.0;

/// Pointer entered the wrapper: open the tooltip window under the anchor
/// with the tip's text.
extern "C" fn on_tooltip_enter(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(text) = data.downcast_ref::<TooltipText>().map(|t| t.0.clone()) else {
        return Update::DoNothing;
    };
    if text.as_str().trim().is_empty() {
        return Update::DoNothing;
    }
    match info.get_hit_node_rect() {
        Some(rect) => info.show_tooltip_at(
            text,
            azul_core::geom::LogicalPosition::new(
                rect.origin.x,
                rect.origin.y + rect.size.height + TIP_GAP,
            ),
        ),
        None => info.show_tooltip(text),
    }
    Update::DoNothing
}

/// Pointer left the wrapper: close the tooltip window.
extern "C" fn on_tooltip_leave(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.hide_tooltip();
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
        let mut d = Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("leaf"),
        );
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
            }.into(),
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
            ctx: core::cell::RefCell::new(OptionRefAny::None),
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
                    node_id,
                    properties,
                    ..
                } => Some((node_id.index(), properties.as_ref().to_vec())),
                _ => None,
            })
            .collect()
    }

    /// A three-node styled DOM — `root(0)` with children `anchor(1)` and
    /// `tip(2)`, the hierarchy a built tooltip has.
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
        let anchor = Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("anchor"),
        );
        let text = AzString::from("tip".to_string());
        let t = Tooltip::new(anchor.clone(), text.clone());

        assert_eq!(t.anchor, anchor, "the anchor must be stored unmodified");
        assert_eq!(t.text, text, "the text must be stored unmodified");
    }

    #[test]
    fn new_uses_the_static_style_tables() {
        // Flat's tables.
        let t = Tooltip::new(Dom::create_div(), AzString::from_const_str("x"))
            .with_theme(UiTheme::Flat);

        assert_eq!(
            t.resolved_wrapper_style(),
            CssPropertyWithConditionsVec::from_const_slice(TOOLTIP_WRAPPER_STYLE)
        );
        // R5: the tip is the widget's `TIP_BASE`, then flat's table.
        let flat_tip: Vec<CssPropertyWithConditions> =
            TIP_BASE.iter().chain(TOOLTIP_TIP_STYLE.iter()).cloned().collect();
        assert_eq!(
            t.resolved_tip_style(),
            CssPropertyWithConditionsVec::from_vec(flat_tip.clone())
        );
        assert_eq!(
            t.resolved_wrapper_style().len(),
            TOOLTIP_WRAPPER_STYLE.len()
        );
        assert_eq!(t.resolved_tip_style().as_slice().len(), flat_tip.len());
    }

    #[test]
    fn new_is_pure_and_independent_of_the_arguments() {
        // The style tables must not vary with the anchor/text — a widget whose
        // styling depended on its content would be unstyleable.
        let a = Tooltip::new(Dom::create_div(), AzString::from_const_str(""));
        let b = Tooltip::new(nested_anchor(8), AzString::from("🦀".repeat(1000)));

        assert_eq!(a.resolved_wrapper_style(), b.resolved_wrapper_style());
        assert_eq!(a.resolved_tip_style(), b.resolved_tip_style());
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
        let anchor = Dom::create_div().with_children(
            (0..2000)
                .map(|_| Dom::create_div())
                .collect::<Vec<_>>()
                .into(),
        );
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
                Dom::create_div()
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a")),
                AzString::from_const_str("initial"),
            );

            let mut mutated = base.clone();
            mutated.set_text(AzString::from(s.clone()));
            let built = base.clone().with_text(AzString::from(s.clone()));

            assert_eq!(mutated, built, "with_text must be set_text + self");
            assert_eq!(mutated.text.as_str(), s.as_str());
            assert_eq!(mutated.anchor, base.anchor, "the anchor must be untouched");
            assert_eq!(
                mutated.resolved_wrapper_style(),
                base.resolved_wrapper_style()
            );
            assert_eq!(mutated.resolved_tip_style(), base.resolved_tip_style());
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
        let style = style_of(vec![CssProperty::const_opacity(StyleOpacity::const_new(
            42,
        ))]);
        let text = AzString::from("both".to_string());

        let a = Tooltip::default()
            .with_text(text.clone())
            .with_tip_style(style.clone());
        let b = Tooltip::default().with_tip_style(style).with_text(text);

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
        assert_eq!(
            mutated.resolved_tip_style(),
            style,
            "the style must be stored verbatim"
        );
    }

    #[test]
    fn set_tip_style_does_not_touch_the_wrapper_style() {
        // The wrapper carries `position: relative`; losing it would make the
        // absolutely-positioned tip escape to the nearest positioned ancestor.
        let base = Tooltip::default();
        let mut t = base.clone();
        t.set_tip_style(CssPropertyWithConditionsVec::from_const_slice(&[]));

        assert_eq!(t.resolved_wrapper_style(), base.resolved_wrapper_style());
        assert_eq!(t.text, base.text);
        assert_eq!(t.anchor, base.anchor);
    }

    #[test]
    fn tip_style_can_be_emptied_and_the_widget_still_builds() {
        let t = Tooltip::new(Dom::create_div(), AzString::from_const_str("naked"))
            .with_tip_style(CssPropertyWithConditionsVec::from_const_slice(&[]));
        assert_eq!(t.resolved_tip_style().as_slice().len(), 0);

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
        assert_eq!(t.resolved_tip_style().as_slice().len(), 10_000);
        assert_eq!(t.resolved_tip_style(), style);

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
            Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "anchor",
            )),
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
        // the tip permanently visible. R5: the hidden start is the widget's
        // `TIP_BASE`; the whole flat tip is that base, then flat's table.
        let flat_tip = CssPropertyWithConditionsVec::from_vec(
            TIP_BASE.iter().chain(TOOLTIP_TIP_STYLE.iter()).cloned().collect(),
        );
        assert_eq!(
            declared_opacities(&flat_tip),
            vec![0.0],
            "the tip must be hidden by default via a single opacity declaration"
        );
    }

    #[test]
    fn tip_style_is_absolutely_positioned_and_does_not_wrap() {
        // R5: the placement and the single line are the widget's `TIP_BASE`,
        // the same in every theme.
        let style = CssPropertyWithConditionsVec::from_const_slice(TIP_BASE);

        assert_eq!(declared_positions(&style), vec![LayoutPosition::Absolute]);
        assert!(
            style
                .as_ref()
                .contains(&CssPropertyWithConditions::simple(CssProperty::const_top(
                    LayoutTop::const_px(TIP_OFFSET_Y)
                ))),
            "the documented vertical offset must be declared"
        );
        assert!(
            style
                .as_ref()
                .contains(&CssPropertyWithConditions::simple(CssProperty::WhiteSpace(
                    StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap)
                ))),
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
        // R5: the tip as flat builds it - `TIP_BASE`, then flat's table.
        let flat_tip: Vec<CssPropertyWithConditions> =
            TIP_BASE.iter().chain(TOOLTIP_TIP_STYLE.iter()).cloned().collect();
        for (name, table) in [
            ("wrapper", TOOLTIP_WRAPPER_STYLE),
            ("tip", flat_tip.as_slice()),
        ] {
            let style = CssPropertyWithConditionsVec::from_vec(table.to_vec());
            let mut types = prop_types(&style);
            let declared = types.len();
            assert!(declared > 0, "{name} style must not be empty");
            types.sort_unstable();
            types.dedup();
            assert_eq!(
                types.len(),
                declared,
                "{name}: a duplicated property type would make the later declaration silently win"
            );
        }
    }

    #[test]
    fn both_style_tables_apply_unconditionally() {
        for table in [TOOLTIP_WRAPPER_STYLE, TIP_BASE, TOOLTIP_TIP_STYLE] {
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
        let anchor = Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("anchor"),
        );
        // Flat's tables.
        let dom = Tooltip::new(anchor.clone(), AzString::from_const_str("tip"))
            .with_theme(UiTheme::Flat)
            .dom();

        assert!(has_class(&dom, WRAPPER_CLASS_NAME));
        assert_eq!(dom.root.get_node_type(), &NodeType::Div);

        let children = dom.children.as_ref();
        assert_eq!(children.len(), 2, "children must be exactly [anchor, tip]");
        let mut described = anchor.clone();
        described.root.set_accessibility_info(azul_core::dom::AccessibilityInfo {
            description: Some(AzString::from_const_str("tip")).into(),
            ..Default::default()
        });
        assert_eq!(
            children[0], described,
            "child 0 must be the anchor, described by the tip for a screen reader"
        );
        assert!(
            has_class(&children[1], TIP_CLASS_NAME),
            "child 1 must be the tip"
        );
        assert_eq!(text_of(&children[1]), Some("tip"));
        assert_eq!(
            inline_properties(&children[1]).len(),
            TIP_BASE.len() + TOOLTIP_TIP_STYLE.len(),
            "the tip must carry the full tip style (the widget's base, then flat's table)"
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

        assert_eq!(
            callbacks.len(),
            2,
            "exactly two hover handlers are expected"
        );
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
            Dom::create_div()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a")),
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
                Dom::create_div()
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a")),
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
            let flattened = StyledDom::create_from_dom(dom)
                .node_hierarchy
                .as_ref()
                .len();
            assert_eq!(
                flattened,
                estimated + 1,
                "depth {depth}: the cached descendant count disagrees with the flattened tree"
            );
        }
    }

    #[test]
    fn dom_of_a_very_wide_anchor_flattens_without_panicking() {
        let anchor = Dom::create_div().with_children(
            (0..2000)
                .map(|_| Dom::create_div())
                .collect::<Vec<_>>()
                .into(),
        );
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
    // the native tooltip window
    // ------------------------------------------------------------------

    /// The payload a built tooltip hands its `event` handler.
    fn payload_of(dom: &Dom, event: HoverEventFilter) -> RefAny {
        dom.root
            .get_callbacks()
            .as_ref()
            .iter()
            .find(|cb| cb.event == EventFilter::Hover(event))
            .map(|cb| cb.refany.clone())
            .expect("the wrapper takes the event")
    }

    /// The user's report: the AzWidgets tooltip showcase never opens the
    /// operating system's tooltip window - the tip is a styled child of the
    /// page, clipped by its ancestors. Hovering the anchor opens the native
    /// tooltip window with the tip's text; leaving it closes the window. The
    /// page is not restyled for it.
    #[test]
    fn hovering_a_tooltip_opens_the_native_tooltip_window_with_its_text() {
        let dom = Tooltip::new(Dom::create_div(), AzString::from("Saves the file")).dom();
        let (update, entered) = with_info(Some(anchor_tip_dom()), node(0), |info| {
            on_tooltip_enter(payload_of(&dom, HoverEventFilter::MouseEnter), info)
        });
        assert_eq!(update, Update::DoNothing);
        assert!(
            entered.iter().any(|c| matches!(
                c,
                CallbackChange::ShowTooltip { text, .. } if text.as_str() == "Saves the file"
            )),
            "entering the anchor opens the tooltip window with the text, {entered:?}"
        );
        assert!(css_writes(&entered).is_empty(), "nothing on the page is restyled");

        let (_, left) = with_info(Some(anchor_tip_dom()), node(0), |info| {
            on_tooltip_leave(payload_of(&dom, HoverEventFilter::MouseLeave), info)
        });
        assert!(
            left.iter().any(|c| matches!(c, CallbackChange::HideTooltip)),
            "leaving the anchor closes the tooltip window, {left:?}"
        );
        assert!(css_writes(&left).is_empty(), "nothing on the page is restyled");
    }

}

#[cfg(test)]
mod theme_tests {
    //! The tooltip's theme is a DOM-level choice: the tip is built from the
    //! skin of the theme the tooltip carries, flat by default. In both the
    //! tip's node starts hidden (`opacity: 0`): the tooltip window shows it.

    use azul_core::dom::Dom;
    use azul_css::props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
    };

    use super::*;
    use crate::widgets::themes::{theme_checks as tc, OptionUiTheme, UiTheme};

    const FLAT: &str = "__azul-theme-flat";
    const FLORA: &str = "__azul-theme-flora";

    fn tooltip(theme: Option<UiTheme>) -> Dom {
        let t = Tooltip::new(Dom::create_div(), AzString::from("Explains it"));
        match theme {
            Some(th) => t.with_theme(th).dom(),
            None => t.dom(),
        }
    }

    fn tip(dom: &Dom) -> &Dom {
        &dom.children.as_ref()[1]
    }

    fn bg(node: &Dom, dark: bool) -> Option<ColorU> {
        tc::background(node, dark).and_then(|p| tc::bg_color(&p))
    }

    #[test]
    fn a_tooltip_without_a_theme_follows_the_app_theme_flat_by_default() {
        let t = Tooltip::new(Dom::create_div(), AzString::from("x"));
        assert_eq!(t.theme, OptionUiTheme::None);
        assert!(tc::has_class(&tooltip(None), FLAT));
        let dom = {
            let _app = azul_core::app_theme::ThemeScope::enter(AzString::from_const_str("flora"));
            tooltip(None)
        };
        assert!(tc::has_class(&dom, FLORA), "built for flora, it is flora's");
        assert!(!tc::has_class(&dom, FLAT));
    }

    #[test]
    fn set_theme_and_with_theme_agree() {
        let mut a = Tooltip::new(Dom::create_div(), AzString::from("x"));
        a.set_theme(UiTheme::Flora);
        assert_eq!(a.theme, OptionUiTheme::Some(UiTheme::Flora));
        assert_eq!(
            a,
            Tooltip::new(Dom::create_div(), AzString::from("x")).with_theme(UiTheme::Flora)
        );
    }

    #[test]
    fn a_flat_tip_is_the_established_dark_chip_in_both_modes() {
        let dom = tooltip(Some(UiTheme::Flat));
        for dark in [false, true] {
            assert_eq!(bg(tip(&dom), dark), Some(TIP_BG_COLOR), "dark={dark}");
            assert_eq!(tc::text_color(tip(&dom), dark), Some(TIP_TEXT_COLOR), "dark={dark}");
        }
    }

    #[test]
    fn a_flora_tip_is_dark_oak_never_black_by_day_and_by_night() {
        use crate::widgets::themes::flora::{TOOLTIP_OAK, TOOLTIP_OAK_INK, TOOLTIP_OAK_RULE};
        let dom = tooltip(Some(UiTheme::Flora));
        assert!(tc::has_class(&dom, FLORA));
        let t = tip(&dom);
        assert_eq!(bg(t, false), Some(TOOLTIP_OAK.0));
        assert_eq!(bg(t, true), Some(TOOLTIP_OAK.1));
        assert_ne!(TOOLTIP_OAK.0, ColorU::rgb(0, 0, 0), "never black");
        assert_eq!(tc::text_color(t, false), Some(TOOLTIP_OAK_INK.0));
        assert_eq!(tc::text_color(t, true), Some(TOOLTIP_OAK_INK.1));
        assert_eq!(tc::border_top_color(t, false, None), Some(TOOLTIP_OAK_RULE.0));
        assert_eq!(tc::border_top_color(t, true, None), Some(TOOLTIP_OAK_RULE.1));
        tc::assert_theme_invariants("tooltip Flora", &dom);
    }

    #[test]
    fn every_theme_s_tip_starts_hidden_and_keeps_its_placement() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let dom = tooltip(Some(theme));
            let t = tip(&dom);
            assert_eq!(
                tc::resolve(t, CssPropertyType::Opacity, false, None),
                Some(CssProperty::const_opacity(StyleOpacity::const_new(0))),
                "{theme:?}: the enter / leave handlers toggle opacity"
            );
            assert_eq!(
                tc::resolve(t, CssPropertyType::Position, false, None),
                Some(CssProperty::const_position(LayoutPosition::Absolute)),
                "{theme:?}"
            );
            assert_eq!(
                tc::resolve(t, CssPropertyType::Top, false, None),
                Some(CssProperty::const_top(LayoutTop::const_px(TIP_OFFSET_Y))),
                "{theme:?}"
            );
        }
    }

    #[test]
    fn the_tip_resolver_answers_for_the_theme() {
        let t = Tooltip::new(Dom::create_div(), AzString::from("x")).with_theme(UiTheme::Flora);
        assert!(t
            .resolved_tip_style()
            .as_ref()
            .iter()
            .any(|p| tc::bg_color(&p.property)
                == Some(crate::widgets::themes::flora::TOOLTIP_OAK.0)));
    }

    #[test]
    fn the_theme_changes_the_look_not_the_accessibility_tree() {
        assert_eq!(
            tc::a11y_outline(&tooltip(Some(UiTheme::Flat))),
            tc::a11y_outline(&tooltip(Some(UiTheme::Flora)))
        );
    }
}

/// R5: a tooltip's STRUCTURE (display, position, white-space, ...) is its
/// base - declared once, outside every `@theme(<name>)` block, so it holds
/// under flat, flora and any theme to come. What a theme owns is its skin:
/// paint and metrics.
#[cfg(test)]
mod structure_tests {
    use azul_core::dom::Dom;
    use azul_css::AzString;

    use super::Tooltip;
    use crate::widgets::themes::{
        theme_blocks::checks::{under, BOTH},
        theme_checks::assert_structure_is_shared,
    };

    #[test]
    fn a_tooltip_declares_its_structure_once_for_every_theme() {
        for t in BOTH {
            let dom = under(t, || {
                Tooltip::new(Dom::create_div(), AzString::from("Save the file")).dom()
            });
            assert_structure_is_shared(&format!("tooltip, built for {}", t.name()), &dom, &[]);
        }
    }
}

//! Popover widget — HTML `popover="auto"`: a floating panel holding
//! arbitrary `content: Dom`, shown below its anchor when the anchor is
//! clicked, and closed by a second click, a press outside it, its window
//! losing focus, or Escape.
//!
//! A front-end over [`crate::widgets::dialog`]: the panel is a
//! `<transient-window>` (a real OS popup, so it is never clipped and sits
//! above everything), non-modal, with `closedby="any"`. Everything a dialog
//! does, a popover does - focus moves into it and comes back to the anchor,
//! its panel is announced as a dialog - it only fixes the options. Use
//! [`crate::widgets::dialog::Dialog`] with an invoker for a titled popover,
//! a close button, a `cancel` / `close` callback or a return value.
//!
//! Why it is not a `display`-toggled sibling any more (2026-09-28): the old
//! popover kept "open" in its trigger's callback payload, which the app's
//! rebuild re-minted from `with_open(false)`, while the panel was shown by a
//! runtime `display: block` override that survives rebuilds. After any
//! rebuild the two disagreed, and every click "opened" the popover again -
//! it could not be closed. Now the engine's popup set is the only record of
//! whether it is open.
//!
//! Key types: [`Popover`], [`PopoverState`], [`PopoverOnToggle`].

use azul_core::{
    callbacks::Update,
    dom::{Dom, IdOrClass, IdOrClass::Class},
    refany::RefAny,
    transient::TransientAnchor,
};
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec,
    },
    props::{
        basic::{color::ColorU, *},
        layout::{
            LayoutDisplay, LayoutFlexGrow, LayoutMinWidth, LayoutPaddingBottom,
            LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutPosition,
        },
        property::{CssProperty, *},
        style::{
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBackgroundContent, StyleBackgroundContentVec,
            StyleBorderBottomColor, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius,
            StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
            StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
            StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderTopStyle,
        },
    },
    AzString,
};

use crate::{
    callbacks::CallbackInfo,
    widgets::{
        dialog::{
            build_dialog, follow_skins, DialogClasses, DialogClosedBy, DialogCompat, DialogParts,
            DialogSkin, OptionDialogOnCancel, OptionDialogOnClose,
        },
        themes::{OptionUiTheme, UiTheme},
    },
};

static POPOVER_WRAPPER_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-popover"))];
static POPOVER_TRIGGER_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-popover-trigger",
))];
static POPOVER_CONTENT_CLASS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-popover-content",
))];

// ---- layout (logical px) ----
/// Minimum width of the floating panel.
const CONTENT_MIN_WIDTH: isize = 160;
const CONTENT_RADIUS: isize = 6;

// ---- colours ----
/// Panel background (white).
const CONTENT_BG_COLOR: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
/// Panel border (#cccccc).
const CONTENT_BORDER_COLOR: ColorU = ColorU {
    r: 204,
    g: 204,
    b: 204,
    a: 255,
};

/// Callback function type invoked when a popover opens or closes. The
/// [`PopoverState`] carries the *new* open/closed value.
pub type PopoverOnToggleCallbackType = extern "C" fn(RefAny, CallbackInfo, PopoverState) -> Update;
impl_widget_callback!(
    PopoverOnToggle,
    OptionPopoverOnToggle,
    PopoverOnToggleCallback,
    PopoverOnToggleCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        PopoverOnToggleCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: POPOVER_ON_TOGGLE_INVOKER,
    invoker_ty:     AzPopoverOnToggleCallbackInvoker,
    thunk_fn:       az_popover_on_toggle_callback_thunk,
    setter_fn:      AzApp_setPopoverOnToggleCallbackInvoker,
    from_handle_fn: AzPopoverOnToggleCallback_createFromHostHandle,
    from_handle_byref_fn: AzPopoverOnToggleCallback_createFromHostHandleByref,
    extra_args:     [ state: PopoverState ],
}

/// A click-triggered floating panel anchored to an arbitrary [`Dom`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Popover {
    /// The declared `open` plus the optional toggle callback.
    pub popover_state: PopoverStateWrapper,
    /// The element that, when clicked, shows (or hides) the panel.
    pub anchor: Dom,
    /// The content shown inside the floating panel.
    pub content: Dom,
    /// Style for the positioning wrapper around the anchor, or `None` for
    /// "no opinion" — in which case the widget's default applies.
    ///
    /// `None` and `Some(empty)` are different answers: the first means the
    /// widget picks, the second means the caller asked for no properties at all
    /// and gets none.
    pub wrapper_style: OptionCssPropertyWithConditionsVec,
    /// Style for the floating panel, or `None` for "no opinion" — in which
    /// case the widget's default panel applies. The panel is a window of its
    /// own, so it carries no `display` toggle: open and closed are the
    /// window's.
    ///
    /// `None` and `Some(empty)` are different answers: the first means the
    /// widget picks, the second means the caller asked for no properties at all
    /// and gets none.
    pub content_style: OptionCssPropertyWithConditionsVec,
    /// The widget theme, or `None` to follow the app theme (`AppConfig::with_theme`). A
    /// theme is a DOM-level choice: it picks the skin the panel is built
    /// from, so switching it rebuilds the popover.
    pub theme: OptionUiTheme,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct PopoverStateWrapper {
    /// Whether the panel is declared open (the HTML `open` state).
    pub inner: PopoverState,
    /// Optional: function to call when the popover opens or closes.
    pub on_toggle: OptionPopoverOnToggle,
}

/// The open/closed state of a [`Popover`].
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct PopoverState {
    /// `true` = panel shown, `false` (default) = panel hidden.
    pub open: bool,
}

/// Wrapper around the anchor: an inline-block, so the anchor rect the panel
/// opens below is the anchor's own.
static POPOVER_WRAPPER_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::InlineBlock)),
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
];

/// The floating panel's BASE: how it lays out, the same in every theme (R5)
/// - the positioning context of what it holds. Every theme's panel starts
/// with it - flat's [`build_panel_style`], `themes::flora::popover_panel_style`
/// - and adds its skin after it: size, padding, border, radius, surface,
/// shadow.
///
/// Declared once here, it is declared once in a popover that follows the app
/// theme too (`themes::theme_blocks`): outside every `@theme` block, so it
/// holds under an app theme no widget knows.
pub(crate) static POPOVER_PANEL_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
];

/// The floating panel: the [`POPOVER_PANEL_BASE`], then a small bordered,
/// rounded surface in the window's own colours. The flat theme's panel.
pub(crate) fn build_panel_style() -> CssPropertyWithConditionsVec {
    let bg_vec = StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(
        CONTENT_BG_COLOR
    )]);
    let mut v = POPOVER_PANEL_BASE.to_vec();
    v.extend([
        CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
            CONTENT_MIN_WIDTH,
        ))),
        // padding: 8px
        CssPropertyWithConditions::simple(CssProperty::const_padding_top(
            LayoutPaddingTop::const_px(8,)
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
            LayoutPaddingBottom::const_px(8),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_left(
            LayoutPaddingLeft::const_px(8),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_padding_right(
            LayoutPaddingRight::const_px(8),
        )),
        // border: 1px solid #cccccc
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
        CssPropertyWithConditions::simple(CssProperty::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_color(
            StyleBorderTopColor {
                inner: CONTENT_BORDER_COLOR,
            }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
            StyleBorderBottomColor {
                inner: CONTENT_BORDER_COLOR,
            },
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_left_color(
            StyleBorderLeftColor {
                inner: CONTENT_BORDER_COLOR,
            }
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
            StyleBorderRightColor {
                inner: CONTENT_BORDER_COLOR,
            },
        )),
        // border-radius: 6px
        CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
            StyleBorderTopLeftRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
            StyleBorderTopRightRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius::const_px(CONTENT_RADIUS),
        )),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
        // Dark theme: the panel floats on the desktop's window surface with
        // its separator as the outline, so the content inside it - which
        // inherits the themed ink - stays legible.
        crate::widgets::themes::system_palette::DARK_WINDOW_BACKGROUND,
        crate::widgets::themes::system_palette::DARK_SEPARATOR_BORDER_TOP,
        crate::widgets::themes::system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
        crate::widgets::themes::system_palette::DARK_SEPARATOR_BORDER_LEFT,
        crate::widgets::themes::system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    ]);
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The skin an UNPINNED popover is built with, so it follows the app theme:
/// each theme's dialog skin with that theme's popover panel swapped in (what
/// `themes::flat::popover` / `themes::flora::popover` do), merged part by
/// part under `structure` (`dialog::follow_skins`).
#[must_use]
fn follow_popover_skin(structure: UiTheme) -> DialogSkin {
    use crate::widgets::themes::{flat, flora};
    let mut flat_skin = flat::dialog_skin();
    flat_skin.panel = flat::popover_panel_style();
    let mut flora_skin = flora::dialog_skin();
    flora_skin.panel = flora::popover_panel_style();
    follow_skins(structure, flat_skin, flora_skin)
}

impl Popover {
    /// Creates a popover whose `anchor`, when clicked, shows a panel holding
    /// `content`. The panel starts closed.
    #[must_use]
    pub fn new(anchor: Dom, content: Dom) -> Self {
        Self {
            popover_state: PopoverStateWrapper::default(),
            anchor,
            content,
            wrapper_style: OptionCssPropertyWithConditionsVec::None,
            content_style: OptionCssPropertyWithConditionsVec::None,
            theme: OptionUiTheme::None,
        }
    }

    /// Pick the widget theme. Unset (`None`), the popover follows the
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

    /// The wrapper CSS this popover renders with.
    ///
    /// `None` means no opinion, so the widget's default applies — the same
    /// answer both themes give, asked in one place so they cannot drift.
    #[must_use]
    pub fn resolved_wrapper_style(&self) -> CssPropertyWithConditionsVec {
        self.wrapper_style.clone().into_option().unwrap_or_else(|| {
            CssPropertyWithConditionsVec::from_const_slice(POPOVER_WRAPPER_STYLE)
        })
    }

    /// The panel CSS this popover renders with.
    ///
    /// `None` means no opinion, so the theme's panel applies. Open or
    /// closed is not a style: the panel is a window of its own.
    #[must_use]
    pub fn resolved_content_style(&self) -> CssPropertyWithConditionsVec {
        self.content_style.clone().into_option().unwrap_or_else(|| {
            use crate::widgets::themes::{flat, flora, theme_blocks};
            match self.theme.into_option() {
                Some(UiTheme::Flat) => flat::popover_panel_style(),
                Some(UiTheme::Flora) => flora::popover_panel_style(),
                // Unpinned: every theme's panel, as `follow_popover_skin`
                // puts it on the render.
                None => theme_blocks::follow_props(
                    flat::popover_panel_style().as_slice(),
                    flora::popover_panel_style().as_slice(),
                ),
            }
        })
    }

    /// Declares the panel open (the HTML `open` state). A CHANGE of it
    /// shows or hides the panel; while it stays `true`, a panel the user
    /// closed stays closed until it goes `false` and `true` again. A popover
    /// the app leaves at `false` is opened and closed by its anchor alone.
    #[inline]
    pub const fn set_open(&mut self, open: bool) {
        self.popover_state.inner.open = open;
    }

    /// Builder-style setter for the declared open state.
    #[inline]
    #[must_use]
    pub const fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// Sets the toggle callback (invoked with the new state whenever the
    /// popover opens or closes by the user's hand).
    #[inline]
    pub fn set_on_toggle<C: Into<PopoverOnToggleCallback>>(&mut self, data: RefAny, on_toggle: C) {
        self.popover_state.on_toggle = Some(PopoverOnToggle {
            callback: on_toggle.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the toggle callback.
    #[inline]
    #[must_use]
    pub fn with_on_toggle<C: Into<PopoverOnToggleCallback>>(
        mut self,
        data: RefAny,
        on_toggle: C,
    ) -> Self {
        self.set_on_toggle(data, on_toggle);
        self
    }

    /// Replaces `self` with a default (empty) popover and returns the original.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(Dom::default(), Dom::default());
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the popover: the `__azul-native-popover` wrapper holding the
    /// clickable anchor (`__azul-native-popover-trigger`) and the panel's
    /// `<transient-window>` (the panel is `__azul-native-popover-content`).
    ///
    /// Rendering goes through the theme modules (as `Button::dom` does):
    /// each hands [`Self::build`] its skin. Unpinned (`theme: None`), the
    /// popover follows the APP theme: built in the structure of the theme
    /// its DOM is built for, carrying every theme's blocks
    /// (`follow_popover_skin`).
    #[must_use]
    pub fn dom(self) -> Dom {
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::popover(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::popover(self),
            None => self.build(follow_popover_skin(UiTheme::current())),
        }
    }

    /// Renders the popover with `skin` supplying every part it does not
    /// style itself - what `themes::flat::popover` / `themes::flora::popover`
    /// call.
    #[must_use]
    pub(crate) fn build(self, skin: DialogSkin) -> Dom {
        // Resolved before the fields are moved out below. The panel is the
        // skin's unless the caller brought one: the theme module that called
        // this is the authority on its own panel.
        let wrapper_style = self.resolved_wrapper_style();
        let content_style = self
            .content_style
            .clone()
            .into_option()
            .unwrap_or_else(|| skin.panel.clone());
        build_dialog(DialogParts {
            declared_open: self.popover_state.inner.open,
            modal: false,
            return_value: AzString::from_const_str(""),
            // HTML `popover="auto"`: a press outside, focus loss and Escape
            // close it.
            closed_by: DialogClosedBy::Any,
            on_cancel: OptionDialogOnCancel::None,
            on_close: OptionDialogOnClose::None,
            compat: DialogCompat::Popover(self.popover_state.on_toggle),
            title: AzString::from_const_str(""),
            content: self.content,
            invoker: Some(self.anchor),
            show_close_button: false,
            anchor: TransientAnchor::Bottom,
            wrapper_style: Some(wrapper_style),
            panel_style: Some(content_style),
            backdrop_style: None,
            classes: DialogClasses {
                wrapper: POPOVER_WRAPPER_CLASS,
                invoker: POPOVER_TRIGGER_CLASS,
                window: &[],
                panel: POPOVER_CONTENT_CLASS,
                title: &[],
                content: &[],
                close: &[],
            },
            skin,
        })
    }
}

impl Default for Popover {
    fn default() -> Self {
        Self::new(Dom::default(), Dom::default())
    }
}

impl From<Popover> for Dom {
    fn from(p: Popover) -> Self {
        p.dom()
    }
}

#[cfg(test)]
mod tests {
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
        transient::TransientDismiss,
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
        widgets::dialog::{on_dialog_dismissed, on_dialog_invoker_click},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    fn index_of_class(styled: &StyledDom, class: &str) -> usize {
        styled
            .node_data
            .as_ref()
            .iter()
            .position(|nd| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == class))
            })
            .unwrap_or_else(|| panic!("no node with class {class} in the flattened DOM"))
    }

    fn node(i: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        }
    }

    /// A `DomLayoutResult` with an EMPTY layout tree: the handlers only walk
    /// the node hierarchy.
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

    /// Runs `f` on a `CallbackInfo` over `styled` (hit node `hit`), after
    /// `prepare` ran on the `LayoutWindow`; returns `f`'s result and the
    /// changes it queued.
    fn with_info<R>(
        styled: StyledDom,
        hit: usize,
        prepare: impl FnOnce(&mut LayoutWindow),
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled));
        prepare(&mut layout_window);

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
            node(hit),
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        let r = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (r, recorded)
    }

    /// Every `SetTransientWindowOpen` in `changes`, as `(node index, open)`.
    fn opens(changes: &[CallbackChange]) -> Vec<(usize, bool)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::SetTransientWindowOpen { node, open } => {
                    node.node.into_crate_internal().map(|n| (n.index(), *open))
                }
                _ => None,
            })
            .collect()
    }

    /// The rendered popover, flattened, with the state its handlers share.
    fn rendered(pop: Popover) -> (StyledDom, RefAny) {
        let dom = pop.dom();
        let data = dom
            .root
            .get_dataset()
            .cloned()
            .expect("the wrapper carries the popover's state");
        (StyledDom::create_from_dom(dom), data)
    }

    /// Records the states it is invoked with; used as a user `on_toggle`.
    /// No asserts in an `extern "C" fn`: a panic there aborts the binary.
    struct ToggleLog {
        calls: Vec<bool>,
    }

    extern "C" fn record_toggle(mut data: RefAny, _: CallbackInfo, state: PopoverState) -> Update {
        if let Some(mut log) = data.downcast_mut::<ToggleLog>() {
            log.calls.push(state.open);
        }
        Update::RefreshDom
    }

    extern "C" fn toggle_do_nothing(_: RefAny, _: CallbackInfo, _: PopoverState) -> Update {
        Update::DoNothing
    }

    fn toggle_cb(f: PopoverOnToggleCallbackType) -> PopoverOnToggleCallback {
        f.into()
    }

    fn calls(log: &RefAny) -> Vec<bool> {
        let mut log = log.clone();
        let v = log
            .downcast_ref::<ToggleLog>()
            .map(|l| l.calls.clone())
            .unwrap_or_default();
        v
    }

    // ------------------------------------------------------------------
    // The panel style
    // ------------------------------------------------------------------

    /// The panel is a window of its own: its style carries no `display`
    /// toggle (the old in-window panel's), and no property twice.
    #[test]
    fn the_panel_style_has_no_display_toggle_and_no_duplicates() {
        let style = build_panel_style();
        assert!(
            !style
                .as_ref()
                .iter()
                .any(|p| matches!(p.property, CssProperty::Display(_))),
            "open and closed are the window's, not a display value"
        );
        let mut types: Vec<CssPropertyType> = style
            .as_ref()
            .iter()
            .filter(|p| p.apply_if.as_ref().is_empty())
            .map(|p| p.property.get_type())
            .collect();
        let declared = types.len();
        types.sort_unstable();
        types.dedup();
        assert_eq!(types.len(), declared, "a duplicated property type");
        assert!(
            style.as_ref().iter().any(|p| p.is_dark_twin()
                && matches!(p.property, CssProperty::BackgroundContent(_))),
            "the panel has a dark-theme surface"
        );
    }

    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    #[test]
    fn new_stores_both_doms_and_starts_closed() {
        let anchor = Dom::create_p_with_text("anchor");
        let content = Dom::create_p_with_text("panel");
        let pop = Popover::new(anchor.clone(), content.clone());
        assert_eq!(pop.anchor, anchor);
        assert_eq!(pop.content, content);
        assert!(!pop.popover_state.inner.open);
        assert!(pop.popover_state.on_toggle.is_none());
        // Flat's panel is the established one (an unpinned popover answers
        // with every theme's blocks - `follow_popover_skin`).
        assert_eq!(
            pop.clone().with_theme(UiTheme::Flat).resolved_content_style(),
            build_panel_style()
        );
        assert_eq!(
            pop.resolved_wrapper_style(),
            CssPropertyWithConditionsVec::from_const_slice(POPOVER_WRAPPER_STYLE)
        );
        assert_eq!(Popover::default(), Popover::new(Dom::default(), Dom::default()));
    }

    #[test]
    fn with_open_matches_set_open_and_the_last_write_wins() {
        for open in [false, true] {
            let mut mutated = Popover::default();
            mutated.set_open(open);
            assert_eq!(Popover::default().with_open(open), mutated);
        }
        assert!(!Popover::default()
            .with_open(true)
            .with_open(false)
            .popover_state
            .inner
            .open);
    }

    #[test]
    fn set_on_toggle_last_call_wins() {
        let mut pop = Popover::default();
        pop.set_on_toggle(RefAny::new(1u8), toggle_cb(toggle_do_nothing));
        pop.set_on_toggle(RefAny::new(9i64), toggle_cb(record_toggle));
        let set = pop.popover_state.on_toggle.as_ref().expect("still Some");
        assert_eq!(set.refany.get_type_id(), RefAny::new(0i64).get_type_id());
        assert_eq!(set.callback, toggle_cb(record_toggle));
    }

    #[test]
    fn swap_with_default_moves_all_state_out() {
        let mut pop = Popover::new(
            Dom::create_p_with_text("a"),
            Dom::create_p_with_text("c"),
        )
        .with_open(true)
        .with_on_toggle(RefAny::new(5u8), toggle_cb(record_toggle));
        let original = pop.swap_with_default();
        assert!(original.popover_state.inner.open);
        assert!(original.popover_state.on_toggle.is_some());
        assert_eq!(pop, Popover::default());
    }

    // ------------------------------------------------------------------
    // Structure
    // ------------------------------------------------------------------

    /// The popover is a dialog: `[trigger, <transient-window>]` in the
    /// `__azul-native-popover` wrapper; the window opens below the anchor,
    /// light-dismisses (`closedby="any"`), and holds the
    /// `__azul-native-popover-content` panel with the caller's content.
    #[test]
    fn the_popover_is_a_trigger_and_a_window_below_it() {
        for open in [false, true] {
            let dom = Popover::new(
                Dom::create_p_with_text("anchor"),
                Dom::create_p_with_text("panel"),
            )
            .with_open(open)
            .dom();
            assert!(has_class(&dom, "__azul-native-popover"));
            let kids = dom.children.as_ref();
            assert_eq!(kids.len(), 2, "[trigger, window]");
            assert!(has_class(&kids[0], "__azul-native-popover-trigger"));
            assert!(kids[0]
                .root
                .get_callbacks()
                .as_ref()
                .iter()
                .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click)));
            let NodeType::TransientWindow(cfg) = kids[1].root.get_node_type() else {
                panic!("the trigger's next sibling is the panel's window");
            };
            assert_eq!(cfg.open, open, "the declared open state");
            assert_eq!(cfg.anchor, TransientAnchor::Bottom);
            assert_eq!(cfg.dismiss, TransientDismiss::OutsideOnly);
            let panel = &kids[1].children.as_ref()[0];
            assert!(has_class(panel, "__azul-native-popover-content"));
            assert_eq!(
                panel.children.as_ref().len(),
                1,
                "no title row and no close button: just the content"
            );
        }
    }

    #[test]
    fn each_dom_gets_its_own_state_and_the_child_count_cache_holds() {
        let a = Popover::default().dom();
        let b = Popover::default().dom();
        assert_ne!(
            a.root.get_dataset().cloned(),
            b.root.get_dataset().cloned(),
            "two popovers must not share state"
        );
        let mut deep = Dom::create_p_with_text("leaf");
        for _ in 0..64 {
            deep = Dom::create_div().with_child(deep);
        }
        let wide = Dom::create_div()
            .with_children((0..256).map(|_| Dom::create_div()).collect::<Vec<_>>().into());
        let dom = Popover::new(deep, wide).dom();
        assert_eq!(
            dom.estimated_total_children,
            dom.recompute_estimated_total_children()
        );
    }

    // ------------------------------------------------------------------
    // Opening and closing
    // ------------------------------------------------------------------

    /// A click on the trigger of a closed popover asks the engine to show
    /// its window and tells `on_toggle` it opened.
    #[test]
    fn a_trigger_click_opens_the_popover_and_tells_on_toggle() {
        let log = RefAny::new(ToggleLog { calls: Vec::new() });
        let (styled, data) = rendered(
            Popover::new(Dom::create_p_with_text("a"), Dom::create_p_with_text("c"))
                .with_on_toggle(log.clone(), toggle_cb(record_toggle)),
        );
        let trigger = index_of_class(&styled, "__azul-native-popover-trigger");
        let window = index_of_class(&styled, "__azul-native-dialog-window");
        let (update, changes) = with_info(styled, trigger, |_| {}, |info| {
            on_dialog_invoker_click(data.clone(), info)
        });
        assert_eq!(opens(&changes), vec![(window, true)]);
        assert_eq!(calls(&log), vec![true]);
        assert_eq!(update, Update::RefreshDom, "the user callback's update");
    }

    /// THE bug (2026-09-28): the demo rebuilds `with_open(false)` after
    /// every toggle. The trigger asks the ENGINE whether the popover is open,
    /// so a click while it shows closes it, whatever the rebuild minted.
    #[test]
    fn a_trigger_click_on_a_showing_popover_closes_it_after_a_rebuild() {
        let log = RefAny::new(ToggleLog { calls: Vec::new() });
        let (styled, data) = rendered(
            Popover::new(Dom::create_p_with_text("a"), Dom::create_p_with_text("c"))
                .with_open(false)
                .with_on_toggle(log.clone(), toggle_cb(record_toggle)),
        );
        let trigger = index_of_class(&styled, "__azul-native-popover-trigger");
        let window = index_of_class(&styled, "__azul-native-dialog-window");
        let (_, changes) = with_info(
            styled,
            trigger,
            |lw| {
                let _ = lw
                    .transient_windows
                    .set_forced_open(NodeId::new(window), true);
            },
            |info| on_dialog_invoker_click(data.clone(), info),
        );
        assert_eq!(opens(&changes), vec![(window, false)]);
        assert_eq!(calls(&log), vec![false]);
    }

    /// The engine closed it (a press outside, focus loss, Escape):
    /// `on_toggle` hears it closed.
    #[test]
    fn a_dismissal_tells_on_toggle_the_popover_closed() {
        let log = RefAny::new(ToggleLog { calls: Vec::new() });
        let (styled, data) = rendered(
            Popover::new(Dom::create_p_with_text("a"), Dom::create_p_with_text("c"))
                .with_on_toggle(log.clone(), toggle_cb(record_toggle)),
        );
        let window = index_of_class(&styled, "__azul-native-dialog-window");
        let (update, changes) = with_info(styled, window, |_| {}, |info| {
            on_dialog_dismissed(data.clone(), info)
        });
        assert!(changes.is_empty());
        assert_eq!(calls(&log), vec![false]);
        assert_eq!(update, Update::RefreshDom);
    }
}

#[cfg(test)]
mod base_and_skin_tests {
    //! R5: a popover's structure is its base, declared once for every app
    //! theme - never inside a `@theme(<name>)` block.

    use azul_core::dom::Dom;

    use super::Popover;
    use crate::widgets::themes::{
        theme_blocks::checks::{under, BOTH},
        theme_checks::assert_structure_is_shared,
    };

    #[test]
    fn a_popover_declares_its_structure_once_for_every_theme() {
        for t in BOTH {
            for open in [false, true] {
                let dom = under(t, || {
                    Popover::new(
                        Dom::create_p_with_text("anchor"),
                        Dom::create_p_with_text("panel"),
                    )
                    .with_open(open)
                    .dom()
                });
                assert_structure_is_shared(
                    &format!("popover (open: {open}) built for {}", t.name()),
                    &dom,
                    &[],
                );
            }
        }
    }
}

//! Modal / dialog widget — an in-app overlay dialog (NOT the native OS file/
//! message dialogs, which live in the `dialog` module; this is the custom in-app
//! variant). A blend of [`crate::widgets::frame::Frame`] (the bordered, elevated
//! content panel) and [`crate::widgets::popover::Popover`] (overlay show/hide via
//! `set_css_property(display)` driven by a toggled state).
//!
//! Structure: a full-area *backdrop* (`position: absolute`, covering its parent,
//! semi-transparent black) that centres a *panel* holding an optional title, an
//! optional "x" close button (absolutely positioned in the panel's top-right
//! corner), and the arbitrary `content: Dom`. The whole thing is hidden by
//! default (`display: none`) and shown by building it with `with_open(true)` (or
//! by the host flipping it). Clicking the close button flips `open` to `false`,
//! invokes the optional user `on_close(state)`, and hides the backdrop via
//! `set_css_property(display: none)` (mirroring popover's live restyle).
//!
//! TODO2 — several "real modal" behaviours are NOT reachable from a widget
//! handler and are deliberately omitted (be honest rather than fake them):
//!   * **Focus-trap** (confining keyboard focus to the dialog while open) depends
//!     on the focus model and is not controllable from a widget handler.
//!   * **Escape-to-close** depends on a global key handler the widget does not own
//!     (the panel/backdrop are not keyboard-focused), so it is not wired.
//!   * **Backdrop-click-to-close** is NOT wired: with `currentTarget` hit
//!     semantics (see `popover`), a click handler on the backdrop reports the
//!     backdrop as the hit node even when the *panel* (a descendant) was clicked,
//!     so it cannot distinguish an outside click from an inside click — wiring it
//!     would close the dialog when clicking its own content. Only the explicit "x"
//!     closes it.
//!   * **Covering sibling widgets**: the backdrop is `position: absolute` and
//!     relies on paint order (being a later sibling) to overlay other content;
//!     there is no real stacking-context / z-index. Place the modal as the LAST
//!     child of a positioned, full-size container for a correct overlay.
//!   * The `display:none/flex` relayout itself is not GUI-verified in this build.
//!
//! Key types: [`Modal`], [`ModalState`], [`ModalOnClose`].

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec, TabIndex},
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
use azul_css::{
    props::{
        basic::{color::ColorU, font::{StyleFontFamily, StyleFontFamilyVec}, PixelValue, StyleFontSize},
        layout::{LayoutDisplay, LayoutPosition, LayoutTop, LayoutLeft, LayoutWidth, LayoutHeight, LayoutFlexDirection, LayoutJustifyContent, LayoutAlignItems, LayoutFlexGrow, LayoutMinWidth, LayoutMaxWidth, LayoutPaddingTop, LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutRight},
        property::{CssProperty, *},
        style::{StyleBackgroundContentVec, StyleBackgroundContent, LayoutBorderTopWidth, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth, StyleBorderTopStyle, BorderStyle, StyleBorderBottomStyle, StyleBorderLeftStyle, StyleBorderRightStyle, StyleBorderTopColor, StyleBorderBottomColor, StyleBorderLeftColor, StyleBorderRightColor, StyleBorderTopLeftRadius, StyleBorderTopRightRadius, StyleBorderBottomLeftRadius, StyleBorderBottomRightRadius, StyleTextColor, StyleTextAlign, StyleUserSelect, StyleCursor},
    },
    impl_option_inner, AzString,
};

use crate::callbacks::{Callback, CallbackInfo};

static MODAL_BACKDROP_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal"))];
static MODAL_PANEL_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal-panel"))];
static MODAL_TITLE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal-title"))];
static MODAL_CLOSE_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal-close"))];
static MODAL_CONTENT_CLASS: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-modal-content"))];

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// ---- layout (logical px) ----
const PANEL_MIN_WIDTH: isize = 280;
const PANEL_MAX_WIDTH: isize = 520;
const PANEL_RADIUS: isize = 8;

// ---- colours ----
/// Semi-transparent black backdrop (rgba(0,0,0,0.5)).
const BACKDROP_COLOR: ColorU = ColorU { r: 0, g: 0, b: 0, a: 128 };
const PANEL_BG_COLOR: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
const PANEL_BORDER_COLOR: ColorU = ColorU { r: 204, g: 204, b: 204, a: 255 }; // #cccccc
const TITLE_COLOR: ColorU = ColorU { r: 33, g: 37, b: 41, a: 255 }; // #212529
const CLOSE_COLOR: ColorU = ColorU { r: 108, g: 117, b: 125, a: 255 }; // #6c757d

/// Callback invoked when the modal's "x" close button is clicked. The
/// [`ModalState`] carries the *new* (`false`) open value.
pub type ModalOnCloseCallbackType = extern "C" fn(RefAny, CallbackInfo, ModalState) -> Update;
impl_widget_callback!(
    ModalOnClose,
    OptionModalOnClose,
    ModalOnCloseCallback,
    ModalOnCloseCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ModalOnCloseCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: MODAL_ON_CLOSE_INVOKER,
    invoker_ty:     AzModalOnCloseCallbackInvoker,
    thunk_fn:       az_modal_on_close_callback_thunk,
    setter_fn:      AzApp_setModalOnCloseCallbackInvoker,
    from_handle_fn: AzModalOnCloseCallback_createFromHostHandle,
    extra_args:     [ state: ModalState ],
}

/// An in-app overlay dialog holding arbitrary content, with an optional title and
/// close button.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct Modal {
    /// Runtime state (`open`) plus the optional close callback.
    pub modal_state: ModalStateWrapper,
    /// The dialog title (empty = no title bar).
    pub title: AzString,
    /// The arbitrary content shown inside the panel.
    pub content: Dom,
    /// Whether to render the "x" close button.
    pub show_close_button: bool,
    /// Style of the full-area backdrop (includes its current `display`).
    pub backdrop_style: CssPropertyWithConditionsVec,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ModalStateWrapper {
    /// Whether the dialog is currently open (shown).
    pub inner: ModalState,
    /// Optional: function to call when the dialog is closed.
    pub on_close: OptionModalOnClose,
}

/// The open/closed state of a [`Modal`].
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ModalState {
    /// `true` = dialog shown, `false` (default) = dialog hidden.
    pub open: bool,
}

/// Builds the backdrop style. Only the `display` (open vs closed) differs; all
/// other props are present in both so the runtime `set_css_property(display)`
/// toggle has everything it needs (mirroring popover/accordion).
fn build_backdrop_style(open: bool) -> CssPropertyWithConditionsVec {
    let display = if open {
        LayoutDisplay::Flex
    } else {
        LayoutDisplay::None
    };
    let bg_vec = StyleBackgroundContentVec::from_vec(alloc::vec![StyleBackgroundContent::Color(
        BACKDROP_COLOR
    )]);
    CssPropertyWithConditionsVec::from_vec(alloc::vec![
        CssPropertyWithConditions::simple(CssProperty::const_display(display)),
        CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
        CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(0))),
        CssPropertyWithConditions::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        // Cover the full parent (see the z-order TODO2 — depends on a full-size,
        // positioned parent).
        CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::Px(
            PixelValue::const_percent(100),
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::Px(
            PixelValue::const_percent(100),
        ))),
        // Centre the panel.
        CssPropertyWithConditions::simple(CssProperty::const_flex_direction(
            LayoutFlexDirection::Row,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_justify_content(
            LayoutJustifyContent::Center,
        )),
        CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
        CssPropertyWithConditions::simple(CssProperty::const_background_content(bg_vec)),
    ])
}

/// The centred dialog panel: a bordered, rounded white box (frame-like). Elevation
/// is conveyed by the dimmed backdrop behind it + the border/radius; a drop
/// `box-shadow` is intentionally omitted (it requires a runtime-heap shadow value
/// — see `progressbar.rs` — and is not needed for a clear modal read).
static MODAL_PANEL_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Relative)),
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Column)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_min_width(LayoutMinWidth::const_px(
        PANEL_MIN_WIDTH,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_max_width(LayoutMaxWidth::const_px(
        PANEL_MAX_WIDTH,
    ))),
    // padding: 20px
    CssPropertyWithConditions::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(20))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(20),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
        20,
    ))),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(20),
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
        inner: PANEL_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: PANEL_BORDER_COLOR,
        },
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_left_color(StyleBorderLeftColor {
        inner: PANEL_BORDER_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_border_right_color(
        StyleBorderRightColor {
            inner: PANEL_BORDER_COLOR,
        },
    )),
    // border-radius: 8px
    CssPropertyWithConditions::simple(CssProperty::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(PANEL_RADIUS),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_font_family(SYSTEM_UI_FAMILY)),
    CssPropertyWithConditions::simple(CssProperty::const_background_content(
        StyleBackgroundContentVec::from_const_slice(&[StyleBackgroundContent::Color(
            PANEL_BG_COLOR,
        )]),
    )),
];

/// Title style: larger, bold-ish dark text with a bottom gap; right padding keeps
/// it clear of the absolutely-positioned "x".
static MODAL_TITLE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(18))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: TITLE_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Left)),
    CssPropertyWithConditions::simple(CssProperty::const_padding_right(
        LayoutPaddingRight::const_px(24),
    )),
    CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
        LayoutPaddingBottom::const_px(12),
    )),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// "x" close-button style: an absolutely-positioned pointer-cursor glyph in the
/// panel's top-right corner.
static MODAL_CLOSE_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_position(LayoutPosition::Absolute)),
    CssPropertyWithConditions::simple(CssProperty::const_top(LayoutTop::const_px(8))),
    CssPropertyWithConditions::simple(CssProperty::const_right(LayoutRight::const_px(12))),
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(22))),
    CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: CLOSE_COLOR,
    })),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// Content-wrapper style: takes the remaining vertical space.
static MODAL_CONTENT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

impl Modal {
    /// Creates a new (closed) modal holding `content`, with a "x" close button and
    /// no title.
    #[must_use] pub fn create(content: Dom) -> Self {
        Self {
            modal_state: ModalStateWrapper::default(),
            title: AzString::from_const_str(""),
            content,
            show_close_button: true,
            backdrop_style: build_backdrop_style(false),
        }
    }

    /// Sets the dialog title (empty = no title).
    #[inline]
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }

    /// Builder-style setter for the title.
    #[inline]
    #[must_use] pub fn with_title(mut self, title: AzString) -> Self {
        self.set_title(title);
        self
    }

    /// Replaces the content shown inside the panel.
    #[inline]
    pub fn set_content(&mut self, content: Dom) {
        self.content = content;
    }

    /// Builder-style setter for the content.
    #[inline]
    #[must_use] pub fn with_content(mut self, content: Dom) -> Self {
        self.set_content(content);
        self
    }

    /// Sets whether the dialog is currently open, recomputing the backdrop style.
    #[inline]
    pub fn set_open(&mut self, open: bool) {
        self.modal_state.inner.open = open;
        self.backdrop_style = build_backdrop_style(open);
    }

    /// Builder-style setter for the initial open state.
    #[inline]
    #[must_use] pub fn with_open(mut self, open: bool) -> Self {
        self.set_open(open);
        self
    }

    /// Sets whether the "x" close button is shown.
    #[inline]
    pub const fn set_close_button(&mut self, show: bool) {
        self.show_close_button = show;
    }

    /// Builder-style setter for the close-button flag.
    #[inline]
    #[must_use] pub const fn with_close_button(mut self, show: bool) -> Self {
        self.set_close_button(show);
        self
    }

    /// Sets the close callback (invoked with the new state when "x" is clicked).
    #[inline]
    pub fn set_on_close<C: Into<ModalOnCloseCallback>>(&mut self, data: RefAny, on_close: C) {
        self.modal_state.on_close = Some(ModalOnClose {
            callback: on_close.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style setter for the close callback.
    #[inline]
    #[must_use] pub fn with_on_close<C: Into<ModalOnCloseCallback>>(
        mut self,
        data: RefAny,
        on_close: C,
    ) -> Self {
        self.set_on_close(data, on_close);
        self
    }

    /// Replaces `self` with a default (empty, closed) modal and returns the original.
    #[inline]
    #[must_use] pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(Dom::default());
        core::mem::swap(&mut s, self);
        s
    }

    /// Renders the modal into a [`Dom`] subtree with the `__azul-native-modal`
    /// class (the backdrop).
    #[must_use] pub fn dom(self) -> Dom {
        // Panel children: [close?, title?, content]. The close button is
        // absolutely positioned (top-right), so its document order does not affect
        // the title/content stacking.
        let mut panel_children = Vec::new();

        if self.show_close_button {
            let close = Dom::create_p_with_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(MODAL_CLOSE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(MODAL_CLOSE_STYLE))
                .with_tab_index(TabIndex::Auto)
            // Role so the accessibility tree knows what this IS:
            // a dialog traps focus and is announced as one. The NAME comes from the widget's own text,
            // which azul derives when a readable label is present.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::Dialog,
                ..Default::default()
            })
                .with_callbacks(
                    alloc::vec![CoreCallbackData {
                        event: azul_core::dom::EventFilter::Hover(
                            azul_core::dom::HoverEventFilter::MouseUp,
                        ),
                        callback: CoreCallback {
                            cb: on_modal_close as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(self.modal_state),
                    }]
                    .into(),
                );
            panel_children.push(close);
        }

        if !self.title.as_str().is_empty() {
            let title = Dom::create_p_with_text(self.title)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(MODAL_TITLE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(MODAL_TITLE_STYLE));
            panel_children.push(title);
        }

        let content = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(MODAL_CONTENT_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(MODAL_CONTENT_STYLE))
            .with_children(DomVec::from_vec(alloc::vec![self.content]));
        panel_children.push(content);

        let panel = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(MODAL_PANEL_CLASS))
            .with_css_props(CssPropertyWithConditionsVec::from_const_slice(MODAL_PANEL_STYLE))
            .with_children(DomVec::from_vec(panel_children));

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(MODAL_BACKDROP_CLASS))
            .with_css_props(self.backdrop_style)
            .with_children(DomVec::from_vec(alloc::vec![panel]))
    }
}

impl Default for Modal {
    fn default() -> Self {
        Self::create(Dom::default())
    }
}

/// "x" close-button click handler. The hit node is the close button (the
/// callback-bearing node, per `currentTarget` semantics — see `popover`); its
/// parent is the panel and the panel's parent is the backdrop. Flips `open` to
/// `false`, invokes the optional user callback, then hides the backdrop via
/// `display: none`.
extern "C" fn on_modal_close(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let close_node = info.get_hit_node();
    let Some(panel) = info.get_parent(close_node) else {
        return Update::DoNothing;
    };
    let Some(backdrop) = info.get_parent(panel) else {
        return Update::DoNothing;
    };

    let result = {
        let Some(mut modal) = data.downcast_mut::<ModalStateWrapper>() else {
            return Update::DoNothing;
        };
        modal.inner.open = false;
        let inner = modal.inner;
        let modal = &mut *modal;
        match modal.on_close.as_mut() {
            Some(ModalOnClose { callback, refany }) => (callback.cb)(refany.clone(), info, inner),
            None => Update::DoNothing,
        }
    };

    // TODO2: hides the whole dialog by toggling `display: none` via
    // set_css_property (the proven live-restyle pattern of popover/alert); the
    // relayout itself is not GUI-verified in this build.
    info.set_css_property(backdrop, CssProperty::const_display(LayoutDisplay::None));

    result
}

impl From<Modal> for Dom {
    fn from(m: Modal) -> Self {
        m.dom()
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

    /// Titles a caller can realistically hand to a modal. The widget never parses
    /// or normalises its title — every one of these has to reach the DOM
    /// byte-for-byte, and every *non-empty* one has to produce a title node (the
    /// `is_empty()` gate is byte-length based, so a zero-width space counts).
    const ADVERSARIAL_TEXT: [&str; 8] = [
        "",
        " ",
        "a\0b",
        "e\u{0301}\u{0301}\u{0301}",
        "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{202E}gnirts desrever\u{202C}",
        "\u{FFFD}\u{FEFF}\t\n",
        "\u{200B}",
    ];

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

    /// The properties of a style vec, in declaration order.
    fn style_props(style: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        style.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The *kind* of every declared property, in order (ignores the values).
    fn property_types(
        style: &CssPropertyWithConditionsVec,
    ) -> Vec<core::mem::Discriminant<CssProperty>> {
        style
            .as_ref()
            .iter()
            .map(|p| core::mem::discriminant(&p.property))
            .collect()
    }

    /// A node's *inline* style properties, in declaration order.
    fn inline_props(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The `display` value declared in a style vec.
    fn display_of(style: &CssPropertyWithConditionsVec) -> Option<LayoutDisplay> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Display(v) => v.get_property().copied(),
            _ => None,
        })
    }

    /// The `display` value in a node's *inline* style.
    fn inline_display(node: &Dom) -> Option<LayoutDisplay> {
        node.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::Display(v) => v.get_property().copied(),
                _ => None,
            })
    }

    /// The `background-color` of a style vec (first background layer only).
    fn background_color(style: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        style.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(v) => match v.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    /// The one and only child of the backdrop: the panel.
    fn panel(dom: &Dom) -> &Dom {
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 1, "the backdrop must have exactly one child");
        assert!(
            has_class(&kids[0], "__azul-native-modal-panel"),
            "the backdrop's only child must be the panel"
        );
        &kids[0]
    }

    fn panel_kids(dom: &Dom) -> &[Dom] {
        panel(dom).children.as_ref()
    }

    /// The first node in the tree carrying `class` (pre-order).
    fn find_class<'a>(dom: &'a Dom, class: &str) -> Option<&'a Dom> {
        if has_class(dom, class) {
            return Some(dom);
        }
        dom.children
            .as_ref()
            .iter()
            .find_map(|c| find_class(c, class))
    }

    /// Total number of nodes in a `Dom` tree.
    fn node_count(dom: &Dom) -> usize {
        1 + dom
            .children
            .as_ref()
            .iter()
            .map(node_count)
            .sum::<usize>()
    }

    /// Total number of callbacks registered anywhere in a `Dom` tree.
    fn count_callbacks(dom: &Dom) -> usize {
        dom.root.get_callbacks().as_ref().len()
            + dom
                .children
                .as_ref()
                .iter()
                .map(count_callbacks)
                .sum::<usize>()
    }

    /// A `depth`-deep chain of nested divs (adversarial content).
    fn nested_content(depth: usize) -> Dom {
        let mut d = Dom::create_div();
        for _ in 0..depth {
            d = Dom::create_div().with_child(d);
        }
        d
    }

    /// The exact backdrop style the widget documents, for a given `display`.
    fn expected_backdrop(display: LayoutDisplay) -> Vec<CssProperty> {
        alloc::vec![
            CssProperty::const_display(display),
            CssProperty::const_position(LayoutPosition::Absolute),
            CssProperty::const_top(LayoutTop::const_px(0)),
            CssProperty::const_left(LayoutLeft::const_px(0)),
            CssProperty::const_width(LayoutWidth::Px(PixelValue::const_percent(100))),
            CssProperty::const_height(LayoutHeight::Px(PixelValue::const_percent(100))),
            CssProperty::const_flex_direction(LayoutFlexDirection::Row),
            CssProperty::const_justify_content(LayoutJustifyContent::Center),
            CssProperty::const_align_items(LayoutAlignItems::Center),
            CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0)),
            CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(
                alloc::vec![StyleBackgroundContent::Color(BACKDROP_COLOR)]
            )),
        ]
    }

    /// A `RefAny` payload recording every `ModalState` a user `on_close` sees.
    struct CloseLog {
        calls: Vec<bool>,
    }

    extern "C" fn record_close(mut data: RefAny, _: CallbackInfo, state: ModalState) -> Update {
        if let Some(mut log) = data.downcast_mut::<CloseLog>() {
            log.calls.push(state.open);
        }
        Update::RefreshDom
    }

    extern "C" fn close_do_nothing(_: RefAny, _: CallbackInfo, _: ModalState) -> Update {
        Update::DoNothing
    }

    fn close_cb(f: ModalOnCloseCallbackType) -> ModalOnCloseCallback {
        f.into()
    }

    /// `open` of a `ModalStateWrapper` payload.
    fn wrapper_open(data: &mut RefAny) -> bool {
        data.downcast_ref::<ModalStateWrapper>()
            .expect("payload must still be a ModalStateWrapper")
            .inner
            .open
    }

    /// The `open` flags recorded by a `CloseLog` payload.
    fn log_calls(data: &mut RefAny) -> Vec<bool> {
        data.downcast_ref::<CloseLog>()
            .expect("payload must still be a CloseLog")
            .calls
            .clone()
    }

    /// A `DomLayoutResult` with an *empty* layout tree: the close handler only
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

    /// The flattened DOM of a default modal: `backdrop(0)`, `panel(1)`,
    /// `close <p>(2)`, `close text(3)`, `content-wrapper(4)`, `content(5)` —
    /// i.e. exactly the hierarchy `on_modal_close` walks (hit node -> parent ->
    /// parent). The callback lives on the `<p>`, never on the text node.
    fn modal_styled_dom() -> StyledDom {
        let styled = StyledDom::create_from_dom(Modal::create(Dom::create_div()).dom());
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            6,
            "fixture must flatten to backdrop/panel/close <p> + text/wrapper/content"
        );
        styled
    }

    /// Invokes `on_modal_close` against a `LayoutWindow` holding `styled` (or
    /// nothing at all, when `styled` is `None`), with `hit` as the hit node.
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_close(
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

        let update = on_modal_close(data, info);
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

    // ------------------------------------------------------------------
    // build_backdrop_style
    // ------------------------------------------------------------------

    #[test]
    fn build_backdrop_style_emits_the_documented_property_list() {
        assert_eq!(
            style_props(&build_backdrop_style(true)),
            expected_backdrop(LayoutDisplay::Flex)
        );
        assert_eq!(
            style_props(&build_backdrop_style(false)),
            expected_backdrop(LayoutDisplay::None)
        );
    }

    #[test]
    fn build_backdrop_style_differs_only_in_the_display() {
        // The doc comment promises the toggle has *everything* it needs: both
        // variants must declare the same properties, in the same order, with the
        // same values — except `display`.
        let open = style_props(&build_backdrop_style(true));
        let closed = style_props(&build_backdrop_style(false));

        assert_eq!(open.len(), closed.len());
        let diffs: Vec<usize> = (0..open.len()).filter(|i| open[*i] != closed[*i]).collect();
        assert_eq!(diffs, alloc::vec![0usize], "only entry 0 (display) may differ");

        assert_eq!(display_of(&build_backdrop_style(true)), Some(LayoutDisplay::Flex));
        assert_eq!(display_of(&build_backdrop_style(false)), Some(LayoutDisplay::None));
    }

    #[test]
    fn build_backdrop_style_declares_no_property_twice() {
        // a duplicated property would silently shadow the earlier declaration —
        // and would make the runtime `display` toggle ambiguous
        for open in [true, false] {
            let types = property_types(&build_backdrop_style(open));
            for (i, a) in types.iter().enumerate() {
                for b in &types[i + 1..] {
                    assert_ne!(a, b, "open={open}: the backdrop declares the same property twice");
                }
            }
        }
    }

    #[test]
    fn build_backdrop_style_is_unconditional() {
        // nothing may be gated behind :hover/@media/... or the closed modal could
        // become visible under the wrong conditions
        for open in [true, false] {
            for p in build_backdrop_style(open).as_ref() {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "open={open}: {:?} must be unconditional",
                    p.property
                );
            }
        }
    }

    #[test]
    fn build_backdrop_style_is_pure_and_repeatable() {
        for open in [true, false] {
            assert_eq!(build_backdrop_style(open), build_backdrop_style(open));
        }
        assert_ne!(build_backdrop_style(true), build_backdrop_style(false));
    }

    #[test]
    fn build_backdrop_style_dims_with_half_transparent_black() {
        for open in [true, false] {
            let style = build_backdrop_style(open);
            assert_eq!(
                background_color(&style),
                Some(ColorU { r: 0, g: 0, b: 0, a: 128 }),
                "the backdrop must stay rgba(0,0,0,0.5) in both states"
            );
        }
        // a fully opaque or fully transparent backdrop would be a regression
        let alpha = BACKDROP_COLOR.a;
        assert!(alpha > 0 && alpha < 255, "backdrop alpha {alpha} must dim, not erase");
    }

    // ------------------------------------------------------------------
    // Modal::create / Default
    // ------------------------------------------------------------------

    #[test]
    fn create_is_a_closed_modal_with_a_close_button_and_no_title() {
        let content = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("hi"));
        let m = Modal::create(content.clone());

        assert!(!m.modal_state.inner.open, "a fresh modal must start closed");
        assert!(m.modal_state.on_close.is_none());
        assert_eq!(m.title.as_str(), "");
        assert_eq!(m.content, content);
        assert!(m.show_close_button, "the 'x' is on by default");
        assert_eq!(display_of(&m.backdrop_style), Some(LayoutDisplay::None));
    }

    #[test]
    fn default_equals_create_with_a_default_dom() {
        assert_eq!(Modal::default(), Modal::create(Dom::default()));
        assert_eq!(Modal::default(), Modal::default());
        assert!(!Modal::default().modal_state.inner.open);
    }

    #[test]
    fn create_survives_extreme_content() {
        // deeply nested, very wide, and empty content must all be stored verbatim
        assert_eq!(node_count(&Modal::create(nested_content(200)).content), 201);

        let wide = Dom::create_div().with_children(DomVec::from_vec(
            (0..5_000).map(|_| Dom::create_div()).collect::<Vec<_>>(),
        ));
        assert_eq!(node_count(&Modal::create(wide).content), 5_001);

        // a modal whose content is *another* modal's DOM
        let inner = Modal::create(Dom::create_div()).with_title(AzString::from("inner"));
        let nested = Modal::create(inner.dom());
        assert_eq!(nested.content.children.as_ref().len(), 1);
    }

    #[test]
    fn create_is_clone_and_value_comparable() {
        let m = Modal::create(Dom::create_div()).with_title(AzString::from("t"));
        assert_eq!(m.clone(), m);
        assert_ne!(m, Modal::create(Dom::create_div()));
    }

    // ------------------------------------------------------------------
    // Modal::set_title / with_title
    // ------------------------------------------------------------------

    #[test]
    fn set_title_stores_every_adversarial_string_byte_for_byte() {
        for s in ADVERSARIAL_TEXT {
            let mut m = Modal::create(Dom::create_div());
            m.set_title(AzString::from(s));
            assert_eq!(m.title.as_str(), s, "title {s:?} must round-trip unchanged");
            assert_eq!(m.title.as_str().len(), s.len(), "no normalisation may happen");
        }
    }

    #[test]
    fn set_title_survives_a_100k_char_title() {
        let long = "ä\u{0301}".repeat(50_000);
        let mut m = Modal::create(Dom::create_div());
        m.set_title(AzString::from(long.as_str()));
        assert_eq!(m.title.as_str(), long);

        let dom = m.dom();
        assert_eq!(
            text_of(&panel_kids(&dom)[1]),
            Some(long.as_str()),
            "the huge title must reach the DOM intact"
        );
    }

    #[test]
    fn with_title_matches_set_title_and_last_write_wins() {
        for s in ADVERSARIAL_TEXT {
            let built = Modal::create(Dom::create_div()).with_title(AzString::from(s));
            let mut mutated = Modal::create(Dom::create_div());
            mutated.set_title(AzString::from(s));
            assert_eq!(built, mutated);
        }

        let m = Modal::create(Dom::create_div())
            .with_title(AzString::from("first"))
            .with_title(AzString::from("second"))
            .with_title(AzString::from(""));
        assert_eq!(m.title.as_str(), "", "the last write must win, even an empty one");
    }

    #[test]
    fn set_title_touches_nothing_else() {
        let content = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("body"));
        let mut m = Modal::create(content.clone()).with_open(true);
        let before = m.backdrop_style.clone();

        m.set_title(AzString::from("Title"));

        assert!(m.modal_state.inner.open, "the title must not close the dialog");
        assert_eq!(m.backdrop_style, before, "the title must not rebuild the backdrop");
        assert_eq!(m.content, content);
        assert!(m.show_close_button);
    }

    #[test]
    fn only_a_byte_empty_title_suppresses_the_title_node() {
        for s in ADVERSARIAL_TEXT {
            let dom = Modal::create(Dom::create_div())
                .with_title(AzString::from(s))
                .dom();
            let title = find_class(&dom, "__azul-native-modal-title");

            if s.is_empty() {
                assert!(title.is_none(), "an empty title must emit no title node");
            } else {
                let title = title.expect("a non-empty title must emit a title node");
                assert_eq!(
                    text_of(title),
                    Some(s),
                    "the title node must carry the string verbatim"
                );
            }
        }
        // documented consequence: a zero-width space is "non-empty", so it emits a
        // title node that renders as nothing but still consumes the title slot
        let zwsp = Modal::create(Dom::create_div())
            .with_title(AzString::from("\u{200B}"))
            .dom();
        assert!(find_class(&zwsp, "__azul-native-modal-title").is_some());
        assert_eq!(panel_kids(&zwsp).len(), 3, "close + (invisible) title + content");
    }

    // ------------------------------------------------------------------
    // Modal::set_content / with_content
    // ------------------------------------------------------------------

    #[test]
    fn set_content_replaces_and_last_write_wins() {
        let a = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("a"));
        let b = Dom::create_text_do_not_use_without_block_level_wrapper("b");

        let mut m = Modal::create(a.clone());
        assert_eq!(m.content, a);
        m.set_content(b.clone());
        assert_eq!(m.content, b, "content must be replaced, not merged");
        assert_eq!(node_count(&m.content), 1);
    }

    #[test]
    fn with_content_matches_set_content_and_keeps_everything_else() {
        let content = nested_content(32);

        let built = Modal::create(Dom::create_div())
            .with_title(AzString::from("t"))
            .with_open(true)
            .with_close_button(false)
            .with_content(content.clone());

        let mut mutated = Modal::create(Dom::create_div())
            .with_title(AzString::from("t"))
            .with_open(true)
            .with_close_button(false);
        mutated.set_content(content.clone());

        assert_eq!(built, mutated);
        assert_eq!(built.content, content);
        assert_eq!(built.title.as_str(), "t");
        assert!(built.modal_state.inner.open);
        assert!(!built.show_close_button);
    }

    #[test]
    fn content_reaches_the_dom_under_the_content_wrapper() {
        let content = nested_content(64);
        let dom = Modal::create(content.clone()).dom();

        let wrapper = find_class(&dom, "__azul-native-modal-content")
            .expect("the content wrapper must exist");
        let kids = wrapper.children.as_ref();
        assert_eq!(kids.len(), 1, "the wrapper holds exactly the user content");
        assert_eq!(kids[0], content, "the content must be handed through untouched");
    }

    // ------------------------------------------------------------------
    // Modal::set_open / with_open
    // ------------------------------------------------------------------

    #[test]
    fn set_open_flips_the_state_and_the_backdrop_display_together() {
        let mut m = Modal::create(Dom::create_div());

        m.set_open(true);
        assert!(m.modal_state.inner.open);
        assert_eq!(display_of(&m.backdrop_style), Some(LayoutDisplay::Flex));

        m.set_open(false);
        assert!(!m.modal_state.inner.open);
        assert_eq!(display_of(&m.backdrop_style), Some(LayoutDisplay::None));
    }

    #[test]
    fn set_open_rebuilds_rather_than_appends() {
        // an append-instead-of-rebuild bug would grow the vec on every call and
        // leave two conflicting `display` declarations behind
        let mut m = Modal::create(Dom::create_div());
        let len = m.backdrop_style.as_ref().len();

        for open in [true, true, false, false, true] {
            m.set_open(open);
            assert_eq!(
                m.backdrop_style.as_ref().len(),
                len,
                "set_open must rebuild the style, not extend it"
            );
            let displays: Vec<_> = m
                .backdrop_style
                .as_ref()
                .iter()
                .filter(|p| matches!(p.property, CssProperty::Display(_)))
                .collect();
            assert_eq!(displays.len(), 1, "exactly one `display` may be declared");
        }
        assert!(m.modal_state.inner.open, "the last write must win");
    }

    #[test]
    fn set_open_is_idempotent() {
        let mut once = Modal::create(Dom::create_div());
        once.set_open(true);
        let mut twice = Modal::create(Dom::create_div());
        twice.set_open(true);
        twice.set_open(true);
        assert_eq!(once, twice);
    }

    #[test]
    fn with_open_matches_set_open_and_keeps_the_other_fields() {
        for open in [true, false] {
            let built = Modal::create(Dom::create_div())
                .with_title(AzString::from("t"))
                .with_open(open);
            let mut mutated = Modal::create(Dom::create_div()).with_title(AzString::from("t"));
            mutated.set_open(open);
            assert_eq!(built, mutated);
            assert_eq!(built.title.as_str(), "t");
            assert!(built.show_close_button);
        }
    }

    #[test]
    fn open_state_reaches_the_rendered_backdrop() {
        for open in [true, false] {
            let dom = Modal::create(Dom::create_div()).with_open(open).dom();
            assert_eq!(
                inline_display(&dom),
                Some(if open { LayoutDisplay::Flex } else { LayoutDisplay::None }),
                "open={open}: the backdrop's inline display must match"
            );
            assert!(has_class(&dom, "__azul-native-modal"));
        }
    }

    // ------------------------------------------------------------------
    // Modal::set_close_button / with_close_button
    // ------------------------------------------------------------------

    #[test]
    fn set_close_button_last_write_wins_and_touches_nothing_else() {
        let mut m = Modal::create(Dom::create_div())
            .with_title(AzString::from("t"))
            .with_open(true);
        let before = m.backdrop_style.clone();

        for show in [false, true, false] {
            m.set_close_button(show);
            assert_eq!(m.show_close_button, show);
        }
        assert!(!m.show_close_button);
        assert_eq!(m.title.as_str(), "t");
        assert!(m.modal_state.inner.open);
        assert_eq!(m.backdrop_style, before);
    }

    #[test]
    fn with_close_button_matches_set_close_button() {
        for show in [true, false] {
            let built = Modal::create(Dom::create_div()).with_close_button(show);
            let mut mutated = Modal::create(Dom::create_div());
            mutated.set_close_button(show);
            assert_eq!(built, mutated);
        }
    }

    #[test]
    fn hiding_the_close_button_leaves_the_modal_with_no_way_to_close_itself() {
        // TODO2 in the module docs: only the explicit "x" closes the dialog. With
        // the button off there is no callback in the whole tree at all.
        let dom = Modal::create(Dom::create_div())
            .with_close_button(false)
            .with_title(AzString::from("stuck"))
            .dom();

        assert!(find_class(&dom, "__azul-native-modal-close").is_none());
        assert_eq!(count_callbacks(&dom), 0, "no handler is wired anywhere");
        assert_eq!(panel_kids(&dom).len(), 2, "panel is [title, content] only");
    }

    // ------------------------------------------------------------------
    // Modal::set_on_close / with_on_close
    // ------------------------------------------------------------------

    #[test]
    fn set_on_close_stores_the_callback_and_replaces_rather_than_appends() {
        let mut m = Modal::create(Dom::create_div());
        assert!(m.modal_state.on_close.is_none());

        let first = RefAny::new(1u32);
        m.set_on_close(first.clone(), close_cb(record_close));
        assert!(m.modal_state.on_close.is_some());
        assert_eq!(
            m.modal_state.on_close.as_ref().unwrap().callback,
            close_cb(record_close)
        );

        let second = RefAny::new(2u32);
        m.set_on_close(second.clone(), close_cb(close_do_nothing));
        let stored = m.modal_state.on_close.as_ref().expect("still set");
        assert_eq!(stored.callback, close_cb(close_do_nothing), "last write wins");
        assert_eq!(stored.refany, second, "the payload is replaced too");
        assert_ne!(stored.refany, first);
    }

    #[test]
    fn with_on_close_matches_set_on_close_and_keeps_everything_else() {
        let data = RefAny::new(7u64);

        let built = Modal::create(Dom::create_div())
            .with_title(AzString::from("t"))
            .with_open(true)
            .with_on_close(data.clone(), close_cb(record_close));

        let mut mutated = Modal::create(Dom::create_div())
            .with_title(AzString::from("t"))
            .with_open(true);
        mutated.set_on_close(data.clone(), close_cb(record_close));

        assert_eq!(built, mutated);
        assert_eq!(built.title.as_str(), "t");
        assert!(built.modal_state.inner.open);
        assert!(built.show_close_button);
    }

    #[test]
    fn set_on_close_does_not_switch_the_close_button_on() {
        // Unlike `Alert::set_on_dismiss`, this setter does NOT imply a close
        // button — a caller that turned the "x" off gets a callback that can
        // never fire, and `dom()` silently drops it.
        let data = RefAny::new(CloseLog { calls: Vec::new() });
        let m = Modal::create(Dom::create_div())
            .with_close_button(false)
            .with_on_close(data.clone(), close_cb(record_close));

        assert!(m.modal_state.on_close.is_some(), "the callback is stored");
        assert!(!m.show_close_button, "but the button stays off");
        assert_eq!(count_callbacks(&m.dom()), 0, "so nothing is wired into the DOM");
    }

    // ------------------------------------------------------------------
    // Modal::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_resets_self() {
        let content = Dom::create_div().with_child(Dom::create_text_do_not_use_without_block_level_wrapper("body"));
        let mut m = Modal::create(content.clone())
            .with_title(AzString::from("Title"))
            .with_open(true)
            .with_close_button(false);

        let old = m.swap_with_default();

        assert_eq!(old.title.as_str(), "Title");
        assert_eq!(old.content, content);
        assert!(old.modal_state.inner.open);
        assert!(!old.show_close_button);

        assert_eq!(m, Modal::default(), "self must be a pristine modal");
        assert_eq!(m.title.as_str(), "");
        assert!(!m.modal_state.inner.open);
        assert!(m.show_close_button);
        assert_eq!(display_of(&m.backdrop_style), Some(LayoutDisplay::None));
    }

    #[test]
    fn swap_with_default_is_stable_when_repeated() {
        let mut m = Modal::create(Dom::create_div()).with_title(AzString::from("t"));
        let _first = m.swap_with_default();
        let second = m.swap_with_default();
        assert_eq!(second, Modal::default());
        assert_eq!(m, Modal::default());
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_of_self() {
        let data = RefAny::new(0u8);
        let mut m = Modal::create(Dom::create_div())
            .with_on_close(data.clone(), close_cb(record_close));

        let old = m.swap_with_default();

        assert!(old.modal_state.on_close.is_some(), "the callback moves out");
        assert!(m.modal_state.on_close.is_none(), "and must not stay behind");
    }

    // ------------------------------------------------------------------
    // Modal::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_shape_is_backdrop_panel_close_content() {
        let dom = Modal::create(Dom::create_div()).dom();

        assert!(has_class(&dom, "__azul-native-modal"));
        let kids = panel_kids(&dom);
        assert_eq!(kids.len(), 2, "no title -> [close, content]");
        assert!(has_class(&kids[0], "__azul-native-modal-close"));
        assert!(has_class(&kids[1], "__azul-native-modal-content"));
        assert_eq!(node_count(&dom), 6); // close "x" is a <p> + its text leaf
    }

    #[test]
    fn dom_with_a_title_puts_the_close_button_first() {
        // document order is [close, title, content]: the "x" is absolutely
        // positioned, so it may come first without affecting the layout.
        let dom = Modal::create(Dom::create_div())
            .with_title(AzString::from("Title"))
            .dom();

        let kids = panel_kids(&dom);
        assert_eq!(kids.len(), 3);
        assert!(has_class(&kids[0], "__azul-native-modal-close"));
        assert!(has_class(&kids[1], "__azul-native-modal-title"));
        assert!(has_class(&kids[2], "__azul-native-modal-content"));
        assert_eq!(text_of(&kids[1]), Some("Title"));
        assert_eq!(node_count(&dom), 8); // close + title each: <p> + text leaf
    }

    #[test]
    fn dom_close_button_is_a_focusable_multiplication_sign_with_one_mouseup_handler() {
        let dom = Modal::create(Dom::create_div()).dom();
        let close = find_class(&dom, "__azul-native-modal-close").expect("close node");

        assert_eq!(
            text_of(close),
            Some("\u{00D7}"),
            "the glyph must be U+00D7 MULTIPLICATION SIGN, not ASCII 'x'"
        );
        assert_eq!(close.root.get_tab_index(), Some(TabIndex::Auto));

        let cbs = close.root.get_callbacks();
        assert_eq!(cbs.as_ref().len(), 1, "exactly one handler on the 'x'");
        let entry = &cbs.as_ref()[0];
        assert_eq!(
            entry.event,
            EventFilter::Hover(HoverEventFilter::MouseUp),
            "the modal closes on mouse-up, not mouse-down"
        );
        assert_eq!(entry.callback.cb, on_modal_close as usize);
        assert_eq!(count_callbacks(&dom), 1, "and nowhere else in the tree");
    }

    #[test]
    fn dom_hands_a_snapshot_of_the_modal_state_to_the_close_button() {
        for open in [true, false] {
            let dom = Modal::create(Dom::create_div()).with_open(open).dom();
            let close = find_class(&dom, "__azul-native-modal-close").expect("close node");
            let mut payload = close.root.get_callbacks().as_ref()[0].refany.clone();
            assert_eq!(
                wrapper_open(&mut payload),
                open,
                "the payload must carry the open state at build time"
            );
        }
    }

    #[test]
    fn dom_payloads_of_two_clones_are_independent() {
        // the payload is a *snapshot*: flipping one rendered modal's state must
        // not reach through to another render of the same builder
        let m = Modal::create(Dom::create_div()).with_open(true);
        let a = m.clone().dom();
        let b = m.dom();

        let mut pa = find_class(&a, "__azul-native-modal-close")
            .expect("close")
            .root
            .get_callbacks()
            .as_ref()[0]
            .refany
            .clone();
        let mut pb = find_class(&b, "__azul-native-modal-close")
            .expect("close")
            .root
            .get_callbacks()
            .as_ref()[0]
            .refany
            .clone();

        assert_ne!(pa, pb, "two renders must not share one RefAny allocation");
        let (_update, _changes) = run_close(Some(StyledDom::create_from_dom(a)), 2, pa.clone());
        assert!(!wrapper_open(&mut pa));
        assert!(wrapper_open(&mut pb), "the other render must be untouched");
    }

    #[test]
    fn dom_applies_the_static_styles_verbatim() {
        let dom = Modal::create(Dom::create_div())
            .with_title(AzString::from("t"))
            .dom();
        let kids = panel_kids(&dom);

        let want = |s: &[CssPropertyWithConditions]| {
            s.iter().map(|p| p.property.clone()).collect::<Vec<_>>()
        };
        assert_eq!(inline_props(panel(&dom)), want(MODAL_PANEL_STYLE));
        assert_eq!(inline_props(&kids[0]), want(MODAL_CLOSE_STYLE));
        assert_eq!(inline_props(&kids[1]), want(MODAL_TITLE_STYLE));
        assert_eq!(inline_props(&kids[2]), want(MODAL_CONTENT_STYLE));
    }

    #[test]
    fn dom_backdrop_style_is_exactly_the_builders_backdrop_style() {
        for open in [true, false] {
            let m = Modal::create(Dom::create_div()).with_open(open);
            let expected = style_props(&m.backdrop_style);
            assert_eq!(inline_props(&m.dom()), expected);
        }
    }

    #[test]
    fn panel_geometry_matches_the_documented_constants() {
        let props = style_props(&CssPropertyWithConditionsVec::from_const_slice(
            MODAL_PANEL_STYLE,
        ));
        for want in [
            CssProperty::const_min_width(LayoutMinWidth::const_px(PANEL_MIN_WIDTH)),
            CssProperty::const_max_width(LayoutMaxWidth::const_px(PANEL_MAX_WIDTH)),
            CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(
                PANEL_RADIUS,
            )),
            CssProperty::const_position(LayoutPosition::Relative),
            CssProperty::const_display(LayoutDisplay::Flex),
        ] {
            assert!(props.contains(&want), "panel style is missing {want:?}");
        }
        let (min, max) = (PANEL_MIN_WIDTH, PANEL_MAX_WIDTH);
        assert!(min <= max, "min-width {min} must not exceed max-width {max}");
    }

    #[test]
    fn every_static_style_declares_each_property_at_most_once() {
        for (name, style) in [
            ("panel", MODAL_PANEL_STYLE),
            ("title", MODAL_TITLE_STYLE),
            ("close", MODAL_CLOSE_STYLE),
            ("content", MODAL_CONTENT_STYLE),
        ] {
            let types = property_types(&CssPropertyWithConditionsVec::from_const_slice(style));
            for (i, a) in types.iter().enumerate() {
                for b in &types[i + 1..] {
                    assert_ne!(a, b, "{name}: the same property is declared twice");
                }
            }
            for p in style {
                assert!(
                    p.apply_if.as_ref().is_empty(),
                    "{name}: {:?} must be unconditional",
                    p.property
                );
            }
        }
    }

    #[test]
    fn dom_survives_extreme_content_and_titles() {
        // deep nesting, a huge sibling list and an adversarial title at once
        let content = Dom::create_div().with_children(DomVec::from_vec(
            (0..2_000).map(|_| nested_content(4)).collect::<Vec<_>>(),
        ));
        let dom = Modal::create(content)
            .with_title(AzString::from("\u{202E}\u{1F469}\u{200D}\u{1F467}\0"))
            .with_open(true)
            .dom();

        // backdrop + panel + close(<p>+text) + title(<p>+text) + wrapper +
        // content root + 2000*5
        assert_eq!(node_count(&dom), 8 + 2_000 * 5);
        assert_eq!(count_callbacks(&dom), 1);
    }

    #[test]
    fn from_modal_for_dom_equals_dom() {
        // no RefAny is involved once the close button is off, so the two renders
        // are value-comparable
        let m = Modal::create(Dom::create_div())
            .with_title(AzString::from("t"))
            .with_close_button(false)
            .with_open(true);
        assert_eq!(Dom::from(m.clone()), m.dom());
    }

    #[test]
    fn two_renders_with_a_close_button_differ_only_in_the_refany_identity() {
        // RefAny equality is allocation identity, so two renders of the *same*
        // modal are NOT equal — but they are once the button (and its payload)
        // is gone.
        let m = Modal::create(Dom::create_div());
        assert_ne!(m.clone().dom(), m.clone().dom());

        let q = m.with_close_button(false);
        assert_eq!(q.clone().dom(), q.dom());
    }

    // ------------------------------------------------------------------
    // on_modal_close
    // ------------------------------------------------------------------

    #[test]
    fn close_hides_the_backdrop_and_flips_open() {
        let mut data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: OptionModalOnClose::None,
        });

        // node 2 == the close button; its parent is the panel(1), whose parent is
        // the backdrop(0)
        let (update, changes) = run_close(Some(modal_styled_dom()), 2, data.clone());

        assert_eq!(update, Update::DoNothing, "no user callback -> DoNothing");
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)],
            "the *backdrop* (not the panel or the button) must be hidden"
        );
        assert!(!wrapper_open(&mut data), "state must flip to closed");
    }

    #[test]
    fn close_invokes_the_user_callback_with_the_already_flipped_state() {
        let mut log = RefAny::new(CloseLog { calls: Vec::new() });
        let mut data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: Some(ModalOnClose {
                callback: close_cb(record_close),
                refany: log.clone(),
            })
            .into(),
        });

        let (update, changes) = run_close(Some(modal_styled_dom()), 2, data.clone());

        assert_eq!(update, Update::RefreshDom, "the user callback's Update is returned");
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false],
            "the callback must see `open == false` (already closed)"
        );
        assert!(!wrapper_open(&mut data));
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)],
            "the backdrop is hidden even after a user callback ran"
        );
    }

    #[test]
    fn close_hides_the_backdrop_even_when_the_user_callback_does_nothing() {
        let data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: Some(ModalOnClose {
                callback: close_cb(close_do_nothing),
                refany: RefAny::new(0u8),
            })
            .into(),
        });

        let (update, changes) = run_close(Some(modal_styled_dom()), 2, data);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(display_writes(&changes), alloc::vec![(0usize, LayoutDisplay::None)]);
    }

    #[test]
    fn close_twice_is_idempotent() {
        let mut log = RefAny::new(CloseLog { calls: Vec::new() });
        let mut data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: Some(ModalOnClose {
                callback: close_cb(record_close),
                refany: log.clone(),
            })
            .into(),
        });

        for _ in 0..2 {
            let (update, changes) = run_close(Some(modal_styled_dom()), 2, data.clone());
            assert_eq!(update, Update::RefreshDom);
            assert_eq!(display_writes(&changes), alloc::vec![(0usize, LayoutDisplay::None)]);
        }

        assert!(!wrapper_open(&mut data), "a second close must not re-open");
        assert_eq!(
            log_calls(&mut log),
            alloc::vec![false, false],
            "each click fires the callback exactly once, always with open == false"
        );
    }

    #[test]
    fn close_on_the_panel_is_a_noop_because_the_backdrop_has_no_parent() {
        // hit node 1 == the panel: parent is the backdrop(0), which has no parent
        // of its own, so the handler bails *before* touching the state
        let mut data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: OptionModalOnClose::None,
        });

        let (update, changes) = run_close(Some(modal_styled_dom()), 1, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "nothing may be restyled without a grandparent");
        assert!(wrapper_open(&mut data), "state must not flip");
    }

    #[test]
    fn close_on_the_root_is_a_noop() {
        let mut data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: OptionModalOnClose::None,
        });

        let (update, changes) = run_close(Some(modal_styled_dom()), 0, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_open(&mut data));
    }

    #[test]
    fn close_with_a_stale_hit_node_is_a_noop() {
        // node 999 does not exist in the 6-node fixture
        let mut data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: OptionModalOnClose::None,
        });

        let (update, changes) = run_close(Some(modal_styled_dom()), 999, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_open(&mut data));
    }

    #[test]
    fn close_without_any_layout_result_is_a_noop() {
        let mut data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: OptionModalOnClose::None,
        });

        let (update, changes) = run_close(None, 2, data.clone());

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(wrapper_open(&mut data), "state must not flip");
    }

    #[test]
    fn close_with_a_foreign_payload_is_a_noop() {
        // the callback-bearing node carries a RefAny of the *wrong* type: the
        // downcast fails before the restyle, so nothing is hidden
        let data = RefAny::new(0xdead_beef_u64);

        let (update, changes) = run_close(Some(modal_styled_dom()), 2, data);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a foreign payload must not hide the backdrop");
    }

    #[test]
    fn close_with_a_plain_modal_state_payload_is_a_noop() {
        // `ModalState` and `ModalStateWrapper` are different types — handing the
        // inner state alone must not be silently accepted
        let data = RefAny::new(ModalState { open: true });

        let (update, changes) = run_close(Some(modal_styled_dom()), 2, data);

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn close_hides_the_grandparent_whatever_it_is() {
        // The handler hides `parent(parent(hit))` unconditionally. On a deeper
        // tree that is NOT the root — documenting why the close button has to
        // stay a direct child of the panel.
        let deep = StyledDom::create_from_dom(nested_content(3));
        assert_eq!(deep.node_hierarchy.as_ref().len(), 4);

        let data = RefAny::new(ModalStateWrapper {
            inner: ModalState { open: true },
            on_close: OptionModalOnClose::None,
        });
        let (update, changes) = run_close(Some(deep), 3, data);

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(1usize, LayoutDisplay::None)],
            "node 1 (the grandparent), not the root, gets hidden"
        );
    }

    #[test]
    fn close_end_to_end_through_the_real_dom_payload() {
        // Take the *actual* RefAny the widget wired into its close button and
        // drive the *actual* handler the widget registered against it.
        let modal = Modal::create(Dom::create_text_do_not_use_without_block_level_wrapper("body"))
            .with_title(AzString::from("Title"))
            .with_open(true);
        let dom = modal.dom();

        let close = find_class(&dom, "__azul-native-modal-close").expect("close node");
        let entry = &close.root.get_callbacks().as_ref()[0];
        assert_eq!(entry.callback.cb, on_modal_close as usize);
        let mut payload = entry.refany.clone();
        assert!(wrapper_open(&mut payload), "the snapshot starts open");

        let styled = StyledDom::create_from_dom(dom);
        // backdrop(0), panel(1), close <p>(2) + text(3), title <p>(4) + text(5),
        // wrapper(6), content(7)
        assert_eq!(styled.node_hierarchy.as_ref().len(), 8);

        let (update, changes) = run_close(Some(styled), 2, payload.clone());

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );
        assert!(
            !wrapper_open(&mut payload),
            "the state living in the DOM must be flipped to closed"
        );
    }

    #[test]
    fn close_end_to_end_invokes_a_user_callback_wired_through_the_builder() {
        let mut log = RefAny::new(CloseLog { calls: Vec::new() });
        let dom = Modal::create(Dom::create_div())
            .with_open(true)
            .with_on_close(log.clone(), close_cb(record_close))
            .dom();

        let payload = find_class(&dom, "__azul-native-modal-close")
            .expect("close node")
            .root
            .get_callbacks()
            .as_ref()[0]
            .refany
            .clone();

        let (update, changes) = run_close(Some(StyledDom::create_from_dom(dom)), 2, payload);

        assert_eq!(update, Update::RefreshDom);
        assert_eq!(log_calls(&mut log), alloc::vec![false]);
        assert_eq!(
            display_writes(&changes),
            alloc::vec![(0usize, LayoutDisplay::None)]
        );
    }

    // ------------------------------------------------------------------
    // ModalState / ModalStateWrapper invariants
    // ------------------------------------------------------------------

    #[test]
    fn modal_state_defaults_to_closed_with_no_callback() {
        assert!(!ModalState::default().open);
        let w = ModalStateWrapper::default();
        assert!(!w.inner.open);
        assert!(w.on_close.is_none());
        assert_eq!(w, ModalStateWrapper::default());
    }

    #[test]
    fn modal_state_is_copy_and_value_comparable() {
        let a = ModalState { open: true };
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(a, ModalState { open: false });
    }
}

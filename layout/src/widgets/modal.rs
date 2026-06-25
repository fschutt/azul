//! Modal / dialog widget — an in-app overlay dialog (NOT the native OS file/
//! message dialogs, which live in the `dialog` module; this is the custom in-app
//! variant). A blend of [`crate::widgets::frame::Frame`] (the bordered, elevated
//! content panel) and [`crate::widgets::popover::Popover`] (overlay show/hide via
//! `set_css_property(display)` driven by a toggled state).
//!
//! Structure: a full-area *backdrop* (`position: absolute`, covering its parent,
//! semi-transparent black) that centres a *panel* holding an optional title, an
//! optional "×" close button (absolutely positioned in the panel's top-right
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
//!     would close the dialog when clicking its own content. Only the explicit "×"
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

/// Callback invoked when the modal's "×" close button is clicked. The
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
    /// Whether to render the "×" close button.
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
/// it clear of the absolutely-positioned "×".
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

/// "×" close-button style: an absolutely-positioned pointer-cursor glyph in the
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
    /// Creates a new (closed) modal holding `content`, with a "×" close button and
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

    /// Sets whether the "×" close button is shown.
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

    /// Sets the close callback (invoked with the new state when "×" is clicked).
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
            let close = Dom::create_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(MODAL_CLOSE_CLASS))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(MODAL_CLOSE_STYLE))
                .with_tab_index(TabIndex::Auto)
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
            let title = Dom::create_text(self.title)
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

/// "×" close-button click handler. The hit node is the close button (the
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

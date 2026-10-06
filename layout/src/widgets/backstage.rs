//! Microsoft Office-style backstage view widget (the full-window "FILE"
//! screen, the Office-2013-era look look by default).
//!
//! Models the component hierarchy of the Office backstage:
//!
//! ```text
//! Backstage ─ nav column (dark accent, full height)
//!           │    ├─ back button (white ring + left arrow)
//!           │    └─ nav items ("Info", "New", "Open", …, "Account", "Options")
//!           └─ right side
//!                ├─ title strip (optional, app-provided: window title/buttons)
//!                └─ content pane (app-provided Dom for the active item)
//! ```
//!
//! The widget owns the CHROME: nav column, back button, item highlight and
//! the content host. The per-item pane content ("Open" recent list, "Info"
//! properties, …) is application composition, injected through
//! [`Backstage::content`] - the backstage does not model document state.
//!
//! The back button expands to the existing [`super::button::Button`] widget
//! with backstage part styles injected (the ribbon's composition rule), and
//! its arrow uses `Dom::create_icon("arrow_back")` so glyphs resolve through
//! the registered icon provider (Material Icons by default).
//!
//! All visual parts are exposed on [`BackstageStyle`] (defaults = the Office-2013-era look
//! look, [`BackstageStyle::office_2013`]); replace any field to re-theme
//! without touching widget code. [`BackstageBehavior`] holds the
//! interactions the backstage performs by itself (currently: Escape invokes
//! the back callback, like classic office suites).
//!
//! The backstage has a widget theme ([`Backstage::theme`]): flat is the
//! Office look [`BackstageStyle`] describes; flora is the flyout navigation
//! drawer of flora.css (`themes::flora::backstage_style`) on the same
//! column. Unpinned (`None`) it follows the app theme; its back button is
//! built in the backstage's theme.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        WindowEventFilter,
    },
    refany::RefAny,
    window::VirtualKeyCode,
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions as Cond, CssPropertyWithConditionsVec,
        OptionCssPropertyWithConditionsVec,
    },
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::CssProperty as P,
        style::*,
    },
    *,
};
use azul_css::{
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut, system::SystemStyle,
};

use super::{
    button::{Button, ButtonOnClick, OptionButtonOnClick},
    themes::{flat, style_kit, OptionUiTheme, UiTheme},
};
use crate::callbacks::CallbackInfo;

// -- Callbacks --

/// Callback signature invoked when a nav item is clicked (receives the item
/// index).
pub type BackstageOnNavSelectCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    BackstageOnNavSelect,
    OptionBackstageOnNavSelect,
    BackstageOnNavSelectCallback,
    BackstageOnNavSelectCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        BackstageOnNavSelectCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: BACKSTAGE_ON_NAV_SELECT_INVOKER,
    invoker_ty:     AzBackstageOnNavSelectCallbackInvoker,
    thunk_fn:       az_backstage_on_nav_select_callback_thunk,
    setter_fn:      AzApp_setBackstageOnNavSelectCallbackInvoker,
    from_handle_fn: AzBackstageOnNavSelectCallback_createFromHostHandle,
    from_handle_byref_fn: AzBackstageOnNavSelectCallback_createFromHostHandleByref,
    extra_args:     [ item_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- the Office-2013-era look palette (seeds BackstageTheme::office_2013) --

const WHITE: ColorU = ColorU {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const TRANSPARENT: ColorU = ColorU {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};
/// Office 2013 accent blue (#2B579A): the nav column fill.
const W13_BLUE: ColorU = ColorU {
    r: 43,
    g: 87,
    b: 154,
    a: 255,
};
/// Hover fill on nav items (#3465AC).
const W13_NAV_HOVER: ColorU = ColorU {
    r: 52,
    g: 101,
    b: 172,
    a: 255,
};
/// Active nav item fill (#3E6DB5).
const W13_NAV_ACTIVE: ColorU = ColorU {
    r: 62,
    g: 109,
    b: 181,
    a: 255,
};

// -- Metrics (the Office-2013-era look, logical px) --

/// Nav column width.
const NAV_WIDTH: isize = 126;
/// Height of one nav item.
const NAV_ITEM_H: isize = 38;
/// Nav item text size.
const NAV_TEXT_PX: isize = 13;
/// Extra gap above a `gap_before` item (office-2013: before "Account").
const NAV_GAP_H: isize = 22;
/// Back button ring diameter.
const BACK_D: isize = 38;

// -- Theme --

/// Color palette from which a full [`BackstageStyle`] is derived via
/// [`BackstageStyle::from_theme`]. All fields are plain colors, so themes
/// are trivially constructible over FFI. Preset:
/// [`BackstageTheme::office_2013`] (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct BackstageTheme {
    /// Nav column fill (office-2013: accent blue).
    pub nav_bg: ColorU,
    /// Nav item text and back-arrow color.
    pub nav_text: ColorU,
    /// Hover fill on nav items.
    pub nav_hover_bg: ColorU,
    /// Fill of the active nav item.
    pub nav_active_bg: ColorU,
    /// Content pane fill.
    pub content_bg: ColorU,
    /// Back button ring color.
    pub back_ring: ColorU,
}

impl BackstageTheme {
    /// The the Office-2013-era look palette: #2B579A nav, white text, lighter-blue
    /// highlights, white content.
    #[must_use]
    pub const fn office_2013() -> Self {
        Self {
            nav_bg: W13_BLUE,
            nav_text: WHITE,
            nav_hover_bg: W13_NAV_HOVER,
            nav_active_bg: W13_NAV_ACTIVE,
            content_bg: WHITE,
            back_ring: WHITE,
        }
    }

    /// Extracts a backstage palette from the OS theme.
    ///
    /// The nav column is an ACCENT-FILLED band and the pane behind it is the
    /// window surface, so those two system colours carry the whole screen. A
    /// colour the platform does not report falls back to that field's own
    /// Office-2013 value, never to another derived value; same discipline as
    /// [`super::ribbon::RibbonTheme::from_system`], no colour arithmetic.
    ///
    /// Takes the style by value (FFI constructor convention).
    #[must_use]
    pub fn from_system(style: SystemStyle) -> Self {
        let d = Self::office_2013();
        let c = &style.colors;
        let on_accent = c.accent_text.into_option();
        Self {
            nav_bg: c.accent.into_option().unwrap_or(d.nav_bg),
            nav_text: on_accent.unwrap_or(d.nav_text),
            nav_hover_bg: c
                .selection_background
                .into_option()
                .unwrap_or(d.nav_hover_bg),
            nav_active_bg: c
                .selection_background_inactive
                .into_option()
                .unwrap_or(d.nav_active_bg),
            content_bg: c.window_background.into_option().unwrap_or(d.content_bg),
            back_ring: on_accent.unwrap_or(d.back_ring),
        }
    }
}

impl Default for BackstageTheme {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Theme -> property-list builders --

fn bg_vec(c: ColorU) -> StyleBackgroundContentVec {
    StyleBackgroundContentVec::from_vec(vec![StyleBackgroundContent::Color(c)])
}

fn cond_bg(c: ColorU) -> Cond {
    Cond::simple(P::const_background_content(bg_vec(c)))
}

const fn cond_text_color(c: ColorU) -> Cond {
    Cond::simple(P::const_text_color(StyleTextColor { inner: c }))
}

const fn cond_border_box() -> Cond {
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox))
}

fn push_ring_border(v: &mut Vec<Cond>, c: ColorU, width: isize, radius: isize) {
    v.push(Cond::simple(P::const_border_top_width(
        LayoutBorderTopWidth::const_px(width),
    )));
    v.push(Cond::simple(P::const_border_left_width(
        LayoutBorderLeftWidth::const_px(width),
    )));
    v.push(Cond::simple(P::const_border_right_width(
        LayoutBorderRightWidth::const_px(width),
    )));
    v.push(Cond::simple(P::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(width),
    )));
    v.push(Cond::simple(P::const_border_top_style(
        StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        },
    )));
    v.push(Cond::simple(P::const_border_left_style(
        StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        },
    )));
    v.push(Cond::simple(P::const_border_right_style(
        StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        },
    )));
    v.push(Cond::simple(P::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )));
    v.push(Cond::simple(P::const_border_top_color(
        StyleBorderTopColor { inner: c },
    )));
    v.push(Cond::simple(P::const_border_left_color(
        StyleBorderLeftColor { inner: c },
    )));
    v.push(Cond::simple(P::const_border_right_color(
        StyleBorderRightColor { inner: c },
    )));
    v.push(Cond::simple(P::const_border_bottom_color(
        StyleBorderBottomColor { inner: c },
    )));
    v.push(Cond::simple(P::const_border_top_left_radius(
        StyleBorderTopLeftRadius::const_px(radius),
    )));
    v.push(Cond::simple(P::const_border_top_right_radius(
        StyleBorderTopRightRadius::const_px(radius),
    )));
    v.push(Cond::simple(P::const_border_bottom_left_radius(
        StyleBorderBottomLeftRadius::const_px(radius),
    )));
    v.push(Cond::simple(P::const_border_bottom_right_radius(
        StyleBorderBottomRightRadius::const_px(radius),
    )));
}

/// The hover fill of the two controls on the nav column (back button, nav
/// item), light AND dark.
///
/// Built by the theme module rather than declared here, so the pair cannot be
/// split: the dark half needs a palette this file cannot see, and a rule
/// written here could only ever name the light colour — which is how every
/// interactive state in this toolkit came to paint its light fill onto a dark
/// surface.
///
/// The dark twin is the SAME colour, on purpose. The nav column is an ACCENT
/// fill in either mode (`theme_nav` paints `nav_bg` with no dark variant;
/// `from_system` reads it from the desktop's accent), and `nav_hover_bg` is a
/// shade of that accent. The theme's neutral `DARK_HT` on a blue band would be
/// worse than today's light-only rule — see `themes::flat::hover_bg_both` for
/// the rule, and `flat::button_states` for the same call on a Primary button.
fn push_nav_hover(v: &mut Vec<Cond>, t: &BackstageTheme) {
    v.extend(flat::hover_bg_both(t.nav_hover_bg, t.nav_hover_bg));
}

/// The page (the root, the right side, the pane): the palette's `content_bg`
/// by day, flat's page (`DARK_PG`) at night. The page is a page-neutral
/// surface (the rule in `themes::flat`'s chrome states): the caller's pane is
/// written in the window's ink, which is light at night whatever the palette
/// says, so a palette's day colour there put white text on a white page.
fn page_bg(t: &BackstageTheme) -> [Cond; 2] {
    super::themes::decl::themed_fill(t.content_bg, flat::DARK_PG)
}

fn theme_root(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(NAV_TEXT_PX))),
    ];
    v.extend(page_bg(t));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_nav(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_width(LayoutWidth::const_px(NAV_WIDTH))),
        cond_bg(t.nav_bg),
    ])
}

/// The circled back arrow. office-2013: a 2px white ring, transparent fill,
/// hover fills like a nav item.
fn theme_back_button(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_justify_content(LayoutJustifyContent::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_width(LayoutWidth::const_px(BACK_D))),
        Cond::simple(P::const_height(LayoutHeight::const_px(BACK_D))),
        Cond::simple(P::const_margin_top(LayoutMarginTop::const_px(16))),
        Cond::simple(P::const_margin_left(LayoutMarginLeft::const_px(20))),
        Cond::simple(P::const_margin_bottom(LayoutMarginBottom::const_px(18))),
        Cond::simple(P::const_cursor(StyleCursor::Pointer)),
        Cond::simple(P::user_select(StyleUserSelect::None)),
        cond_bg(TRANSPARENT),
    ];
    push_nav_hover(&mut v, t);
    push_ring_border(&mut v, t.back_ring, 2, BACK_D / 2);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_back_icon(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(20))),
        cond_text_color(t.nav_text),
    ])
}

fn theme_nav_item(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_height(LayoutHeight::const_px(NAV_ITEM_H))),
        Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(24))),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(NAV_TEXT_PX))),
        Cond::simple(P::const_cursor(StyleCursor::Pointer)),
        Cond::simple(P::user_select(StyleUserSelect::None)),
        cond_text_color(t.nav_text),
        cond_bg(TRANSPARENT),
    ];
    push_nav_hover(&mut v, t);
    CssPropertyWithConditionsVec::from_vec(v)
}

/// APPENDED to the active nav item.
fn theme_nav_item_active(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![cond_bg(t.nav_active_bg)])
}

/// APPENDED to a `gap_before` nav item (office-2013: the gap before "Account").
fn theme_nav_item_gap(_t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![Cond::simple(P::const_margin_top(
        LayoutMarginTop::const_px(NAV_GAP_H),
    ))])
}

fn theme_right(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    ];
    v.extend(page_bg(t));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_content(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    ];
    v.extend(page_bg(t));
    CssPropertyWithConditionsVec::from_vec(v)
}

// -- Style --

/// All part styles of the backstage. Every part defaults to the the Office-2013-era look
/// look; replace any field for finer control (the same override API as
/// [`super::ribbon::RibbonStyle`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct BackstageStyle {
    /// The palette this style bundle was derived from. Kept for consumers
    /// deriving matching custom parts.
    pub theme: BackstageTheme,
    /// Root container (horizontal: nav column beside the right side).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub root_style: OptionCssPropertyWithConditionsVec,
    /// The nav column.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub nav_style: OptionCssPropertyWithConditionsVec,
    /// Container style injected into the back [`Button`] (the ring).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub back_button_style: OptionCssPropertyWithConditionsVec,
    /// Icon style injected into the back [`Button`] (the arrow).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub back_icon_style: OptionCssPropertyWithConditionsVec,
    /// One nav item.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub nav_item_style: OptionCssPropertyWithConditionsVec,
    /// APPENDED to the active nav item.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub nav_item_active_style: OptionCssPropertyWithConditionsVec,
    /// APPENDED to a `gap_before` nav item.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub nav_item_gap_style: OptionCssPropertyWithConditionsVec,
    /// The right side (title strip over content).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub right_style: OptionCssPropertyWithConditionsVec,
    /// The content host for the active pane.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub content_style: OptionCssPropertyWithConditionsVec,
}

impl BackstageStyle {
    /// The the Office-2013-era look look (#2B579A nav, white content) - the default.
    #[must_use]
    pub const fn office_2013() -> Self {
        Self::from_theme(BackstageTheme::office_2013())
    }

    /// Every part style, derived from the OS theme - see
    /// [`BackstageTheme::from_system`]. Takes the style by value (FFI
    /// constructor convention).
    #[must_use]
    pub fn from_system(style: SystemStyle) -> Self {
        Self::from_theme(BackstageTheme::from_system(style))
    }

    /// Derives every part style from the given palette.
    #[must_use]
    pub const fn from_theme(theme: BackstageTheme) -> Self {
        Self {
            theme,
            root_style: OptionCssPropertyWithConditionsVec::None,
            nav_style: OptionCssPropertyWithConditionsVec::None,
            back_button_style: OptionCssPropertyWithConditionsVec::None,
            back_icon_style: OptionCssPropertyWithConditionsVec::None,
            nav_item_style: OptionCssPropertyWithConditionsVec::None,
            nav_item_active_style: OptionCssPropertyWithConditionsVec::None,
            nav_item_gap_style: OptionCssPropertyWithConditionsVec::None,
            right_style: OptionCssPropertyWithConditionsVec::None,
            content_style: OptionCssPropertyWithConditionsVec::None,
        }
    }

    /// The `root_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_root_style(&self) -> CssPropertyWithConditionsVec {
        self.root_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_root(&self.theme))
    }

    /// The `nav_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_nav_style(&self) -> CssPropertyWithConditionsVec {
        self.nav_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_nav(&self.theme))
    }

    /// The `back_button_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_back_button_style(&self) -> CssPropertyWithConditionsVec {
        self.back_button_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_back_button(&self.theme))
    }

    /// The `back_icon_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_back_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.back_icon_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_back_icon(&self.theme))
    }

    /// The `nav_item_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_nav_item_style(&self) -> CssPropertyWithConditionsVec {
        self.nav_item_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_nav_item(&self.theme))
    }

    /// The `nav_item_active_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_nav_item_active_style(&self) -> CssPropertyWithConditionsVec {
        self.nav_item_active_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_nav_item_active(&self.theme))
    }

    /// The `nav_item_gap_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_nav_item_gap_style(&self) -> CssPropertyWithConditionsVec {
        self.nav_item_gap_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_nav_item_gap(&self.theme))
    }

    /// The `right_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_right_style(&self) -> CssPropertyWithConditionsVec {
        self.right_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_right(&self.theme))
    }

    /// The `content_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_content_style(&self) -> CssPropertyWithConditionsVec {
        self.content_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_content(&self.theme))
    }
}

impl Default for BackstageStyle {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Behavior --

/// The interactions the backstage performs BY ITSELF. Each is the classic
/// default and each can be turned off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct BackstageBehavior {
    /// Pressing Escape invokes the back callback (office-2013: Esc leaves the
    /// backstage). Attached as a window-level key handler on the root, so
    /// it fires regardless of focus. Requires [`Backstage::on_back`].
    pub close_on_escape: bool,
}

impl BackstageBehavior {
    /// All classic office-suite behaviors enabled - the default.
    #[must_use]
    pub const fn office_2013() -> Self {
        Self {
            close_on_escape: true,
        }
    }

    /// Every self-driven behavior off.
    #[must_use]
    pub const fn inert() -> Self {
        Self {
            close_on_escape: false,
        }
    }
}

impl Default for BackstageBehavior {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Data model --

/// One backstage nav item ("Info", "Open", …).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct BackstageNavItem {
    /// The item label.
    pub label: AzString,
    /// Renders an extra gap above this item (office-2013: before "Account").
    pub gap_before: bool,
}

impl BackstageNavItem {
    /// Creates a nav item without a gap.
    #[must_use]
    pub const fn new(label: AzString) -> Self {
        Self {
            label,
            gap_before: false,
        }
    }

    /// Builder method: marks this item as starting a new group.
    #[must_use]
    pub const fn with_gap_before(mut self) -> Self {
        self.gap_before = true;
        self
    }
}

impl_option!(
    BackstageNavItem,
    OptionBackstageNavItem,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    BackstageNavItem,
    BackstageNavItemVec,
    BackstageNavItemVecDestructor,
    BackstageNavItemVecDestructorType,
    BackstageNavItemVecSlice,
    OptionBackstageNavItem
);
impl_vec_clone!(
    BackstageNavItem,
    BackstageNavItemVec,
    BackstageNavItemVecDestructor
);
impl_vec_debug!(BackstageNavItem, BackstageNavItemVec);
impl_vec_mut!(BackstageNavItem, BackstageNavItemVec);

/// Top-level backstage widget: nav column + app-provided content pane.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Backstage {
    /// Nav items, top to bottom.
    pub nav_items: BackstageNavItemVec,
    /// Index of the active (highlighted) nav item.
    pub active_item: usize,
    /// Optional callback fired when a nav item is clicked (receives the
    /// item index).
    pub on_nav_select: OptionBackstageOnNavSelect,
    /// Optional callback fired by the back button (and by Escape, if
    /// [`BackstageBehavior::close_on_escape`] is set).
    pub on_back: OptionButtonOnClick,
    /// Optional strip rendered above the content, right of the nav column
    /// (office-2013: the white title bar area with the window buttons).
    pub title_strip: azul_core::dom::OptionDom,
    /// The active item's pane content (application composition).
    pub content: azul_core::dom::OptionDom,
    /// Which interactions the backstage handles by itself (defaults to
    /// Word).
    pub behavior: BackstageBehavior,
    /// All part styles (defaults to the the Office-2013-era look look).
    pub style: BackstageStyle,
    /// The widget theme, or `None` to follow the app theme
    /// (`AppConfig::with_theme`). Flat is the Office look [`Self::style`]
    /// describes; flora lays the flyout drawer of flora.css - a leaf of
    /// paper, inset keys, the selected item a sunken stone - on the same
    /// column. A part the caller set in [`Self::style`] wins in either theme.
    pub theme: OptionUiTheme,
}

// -- CSS classes --

static CLS_BACKSTAGE: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-backstage"))];
static CLS_NAV: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-backstage-nav",
))];
static CLS_NAV_ITEM: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-backstage-nav-item",
))];
static CLS_NAV_ITEM_ACTIVE: &[IdOrClass] = &[
    Class(AzString::from_const_str("__azul-native-backstage-nav-item")),
    Class(AzString::from_const_str(
        "__azul-native-backstage-nav-item-active",
    )),
];
static CLS_RIGHT: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-backstage-right",
))];
static CLS_CONTENT: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-backstage-content",
))];

/// The default the Office-2013-era look nav labels, in order.
pub const OFFICE_2013_NAV_LABELS: &[&str] = &[
    "Info", "New", "Open", "Save", "Save As", "Print", "Share", "Export", "Close",
];

// -- Constructors / builders --

impl Backstage {
    /// Creates a backstage with the given nav items, item 0 active, no
    /// callbacks and no content, in the the Office-2013-era look style.
    #[must_use]
    pub fn new(nav_items: BackstageNavItemVec) -> Self {
        Self {
            nav_items,
            active_item: 0,
            on_nav_select: None.into(),
            on_back: None.into(),
            title_strip: None.into(),
            content: None.into(),
            behavior: BackstageBehavior::office_2013(),
            style: BackstageStyle::office_2013(),
            theme: OptionUiTheme::None,
        }
    }

    /// The the Office-2013-era look nav: Info / New / Open / Save / Save As / Print /
    /// Share / Export / Close, then a gap, then Account / Options.
    #[must_use]
    pub fn office_2013() -> Self {
        let mut items: Vec<BackstageNavItem> = OFFICE_2013_NAV_LABELS
            .iter()
            .map(|l| BackstageNavItem::new(AzString::from(*l)))
            .collect();
        items.push(BackstageNavItem::new(AzString::from_const_str("Account")).with_gap_before());
        items.push(BackstageNavItem::new(AzString::from_const_str("Options")));
        Self::new(BackstageNavItemVec::from_vec(items))
    }

    /// Sets the active nav item.
    pub const fn set_active_item(&mut self, active_item: usize) {
        self.active_item = active_item;
    }

    /// Builder method: sets the active nav item and returns `self`.
    #[must_use]
    pub const fn with_active_item(mut self, active_item: usize) -> Self {
        self.set_active_item(active_item);
        self
    }

    /// Sets the pane content for the active item.
    pub fn set_content(&mut self, content: Dom) {
        self.content = Some(content).into();
    }

    /// Builder method: sets the pane content and returns `self`.
    #[must_use]
    pub fn with_content(mut self, content: Dom) -> Self {
        self.set_content(content);
        self
    }

    /// Sets the title strip rendered above the content.
    pub fn set_title_strip(&mut self, title_strip: Dom) {
        self.title_strip = Some(title_strip).into();
    }

    /// Builder method: sets the title strip and returns `self`.
    #[must_use]
    pub fn with_title_strip(mut self, title_strip: Dom) -> Self {
        self.set_title_strip(title_strip);
        self
    }

    /// Sets the nav-select callback.
    pub fn set_on_nav_select<C: Into<BackstageOnNavSelectCallback>>(
        &mut self,
        data: RefAny,
        on_nav_select: C,
    ) {
        self.on_nav_select = Some(BackstageOnNavSelect {
            refany: data,
            callback: on_nav_select.into(),
        })
        .into();
    }

    /// Builder method: sets the nav-select callback and returns `self`.
    #[must_use]
    pub fn with_on_nav_select<C: Into<BackstageOnNavSelectCallback>>(
        mut self,
        data: RefAny,
        on_nav_select: C,
    ) -> Self {
        self.set_on_nav_select(data, on_nav_select);
        self
    }

    /// Sets the back callback (back button + Escape).
    pub fn set_on_back<C: Into<super::button::ButtonOnClickCallback>>(
        &mut self,
        data: RefAny,
        on_back: C,
    ) {
        self.on_back = Some(ButtonOnClick {
            refany: data,
            callback: on_back.into(),
        })
        .into();
    }

    /// Builder method: sets the back callback and returns `self`.
    #[must_use]
    pub fn with_on_back<C: Into<super::button::ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        on_back: C,
    ) -> Self {
        self.set_on_back(data, on_back);
        self
    }

    /// Builder method: replaces the behavior set.
    #[must_use]
    pub const fn with_behavior(mut self, behavior: BackstageBehavior) -> Self {
        self.behavior = behavior;
        self
    }

    /// Builder method: replaces the style bundle.
    #[must_use]
    pub fn with_style(mut self, style: BackstageStyle) -> Self {
        self.style = style;
        self
    }

    /// Pick the widget theme: the backstage and its back button keep this
    /// look whatever the app theme is. Unset (`None`), the backstage follows
    /// the app theme (`AppConfig::with_theme`, flat by default).
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Renders the backstage in its theme: a pinned theme is that look; no
    /// theme follows the app theme (every part carries both looks, each
    /// inside its `@theme(<name>)` block, in the structure of the theme the
    /// DOM is built for). The tree is built ONCE either way, so the caller's
    /// title strip and pane content are never cloned.
    #[must_use]
    pub fn dom(self) -> Dom {
        let Self {
            nav_items,
            active_item,
            on_nav_select,
            on_back,
            title_strip,
            content,
            behavior,
            style,
            theme,
        } = self;

        // The parts this build paints with, and the theme whose structure
        // (the root's marker) it has. The back button takes the backstage's
        // own theme: pinned with it, following the app theme with it.
        let (style, structure) = match theme.into_option() {
            Some(UiTheme::Flat) => (style, UiTheme::Flat),
            Some(UiTheme::Flora) => (
                crate::widgets::themes::flora::backstage_style(style),
                UiTheme::Flora,
            ),
            None => (follow_style(style), UiTheme::current()),
        };

        // Every part resolved up front: the resolvers borrow `&style`, and
        // `root_style` is moved out of it at the end of this function.
        let part_root = style.resolved_root_style();
        let part_nav = style.resolved_nav_style();
        let part_back_button = style.resolved_back_button_style();
        let part_back_icon = style.resolved_back_icon_style();
        let part_nav_item = style.resolved_nav_item_style();
        let part_nav_item_active = style.resolved_nav_item_active_style();
        let part_nav_item_gap = style.resolved_nav_item_gap_style();
        let part_right = style.resolved_right_style();
        let part_content = style.resolved_content_style();

        // -- nav column --
        let mut nav_children: Vec<Dom> = Vec::with_capacity(nav_items.len() + 1);

        {
            let mut b = Button::create(AzString::from_const_str(""));
            // The backstage's theme: pinned, the button wears the same look;
            // `None`, it follows the app theme with the backstage (its part
            // styles below already carry both looks' blocks).
            b.theme = theme;
            b.icon = AzString::from_const_str("arrow_back");
            b.container_style = OptionCssPropertyWithConditionsVec::Some(part_back_button);
            b.icon_style = OptionCssPropertyWithConditionsVec::Some(part_back_icon);
            b.on_click = on_back.clone();
            nav_children.push(b.dom());
        }

        for (idx, item) in nav_items.into_library_owned_vec().into_iter().enumerate() {
            let (classes, mut part_style) = if idx == active_item {
                (
                    CLS_NAV_ITEM_ACTIVE,
                    merged_style(&part_nav_item, &part_nav_item_active),
                )
            } else {
                (CLS_NAV_ITEM, part_nav_item.clone())
            };
            if item.gap_before {
                part_style = merged_style(&part_style, &part_nav_item_gap);
            }
            // The nav item div is display:flex — a raw text run cannot be a
            // flex item (no anonymous-block wrapping in azul), so the label
            // gets its `<p>` per the label convention. Caught by `dom_lint`
            // on its very first run.
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
                .with_css_props(part_style)
                .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                    item.label,
                )]));
            if let Some(cb) = on_nav_select.as_ref() {
                d = d.with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: on_backstage_nav_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(NavClickData {
                            item_idx: idx,
                            on_nav_select: cb.clone(),
                        }),
                    }]
                    .into(),
                );
            }
            nav_children.push(d);
        }

        let nav = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_NAV))
            .with_css_props(part_nav)
            .with_children(DomVec::from_vec(nav_children));

        // -- right side --
        let mut right_children: Vec<Dom> = Vec::with_capacity(2);
        if let Some(strip) = title_strip.into_option() {
            right_children.push(strip);
        }
        let pane = match content.into_option() {
            Some(c) => c,
            None => Dom::create_div(),
        };
        right_children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_CONTENT))
                .with_css_props(part_content)
                .with_children(DomVec::from_vec(vec![pane])),
        );

        let right = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_RIGHT))
            .with_css_props(part_right)
            .with_children(DomVec::from_vec(right_children));

        let mut root = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![
                CLS_BACKSTAGE[0].clone(),
                style_kit::marker(structure),
            ]))
            .with_css_props(part_root)
            .with_children(DomVec::from_vec(vec![nav, right]));

        // Escape leaves the backstage (window-level, focus-independent).
        if behavior.close_on_escape {
            if let Some(back) = on_back.into_option() {
                root = root.with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Window(WindowEventFilter::VirtualKeyDown),
                        callback: CoreCallback {
                            cb: on_backstage_key_down as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(EscCloseData { on_back: back }),
                    }]
                    .into(),
                );
            }
        }

        root
    }
}

impl Default for Backstage {
    fn default() -> Self {
        Self::office_2013()
    }
}

impl From<Backstage> for Dom {
    fn from(b: Backstage) -> Self {
        b.dom()
    }
}

fn merged_style(
    base: &CssPropertyWithConditionsVec,
    extra: &CssPropertyWithConditionsVec,
) -> CssPropertyWithConditionsVec {
    crate::widgets::themes::theme_blocks::stack_parts(base, extra)
}

/// The parts an UNPINNED backstage renders with, so it follows the app
/// theme: every part in BOTH looks - the flat one `style` describes and
/// flora's (`themes::flora::backstage_style`) - through the one merge
/// (`themes::theme_blocks::follow_props`). What both looks declare alike (the
/// column's layout, the flex boxes, the fonts) is declared once, outside any
/// theme block; the rest sits in its theme's `@theme(<name>)` block, and the
/// cascade keeps the live theme's. A part the caller set is the same in both
/// looks, so it comes back as it is.
fn follow_style(style: BackstageStyle) -> BackstageStyle {
    use crate::widgets::themes::theme_blocks::follow_props;
    let flora = crate::widgets::themes::flora::backstage_style(style.clone());
    let flat = style;
    let both = |a: CssPropertyWithConditionsVec, b: CssPropertyWithConditionsVec| {
        OptionCssPropertyWithConditionsVec::Some(follow_props(a.as_slice(), b.as_slice()))
    };
    BackstageStyle {
        theme: flat.theme,
        root_style: both(flat.resolved_root_style(), flora.resolved_root_style()),
        nav_style: both(flat.resolved_nav_style(), flora.resolved_nav_style()),
        back_button_style: both(
            flat.resolved_back_button_style(),
            flora.resolved_back_button_style(),
        ),
        back_icon_style: both(
            flat.resolved_back_icon_style(),
            flora.resolved_back_icon_style(),
        ),
        nav_item_style: both(
            flat.resolved_nav_item_style(),
            flora.resolved_nav_item_style(),
        ),
        nav_item_active_style: both(
            flat.resolved_nav_item_active_style(),
            flora.resolved_nav_item_active_style(),
        ),
        nav_item_gap_style: both(
            flat.resolved_nav_item_gap_style(),
            flora.resolved_nav_item_gap_style(),
        ),
        right_style: both(flat.resolved_right_style(), flora.resolved_right_style()),
        content_style: both(
            flat.resolved_content_style(),
            flora.resolved_content_style(),
        ),
    }
}

// -- Nav-click / Escape plumbing --

/// Payload of one nav item: the item index plus the user's nav callback.
struct NavClickData {
    item_idx: usize,
    on_nav_select: BackstageOnNavSelect,
}

extern "C" fn on_backstage_nav_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(payload) = data.downcast_ref::<NavClickData>() else {
        return Update::DoNothing;
    };
    let idx = payload.item_idx;
    let cb = payload.on_nav_select.callback.cb;
    let refany = payload.on_nav_select.refany.clone();
    drop(payload);
    (cb)(refany, info, idx)
}

/// Payload of the window-level Escape handler: the user's back callback.
struct EscCloseData {
    on_back: ButtonOnClick,
}

extern "C" fn on_backstage_key_down(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(payload) = data.downcast_ref::<EscCloseData>() else {
        return Update::DoNothing;
    };
    let is_escape = matches!(
        info.get_current_keyboard_state()
            .current_virtual_keycode
            .into_option(),
        Some(VirtualKeyCode::Escape)
    );
    if !is_escape {
        return Update::DoNothing;
    }
    let cb = payload.on_back.callback.cb;
    let refany = payload.on_back.refany.clone();
    drop(payload);
    (cb)(refany, info)
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn nav_cb(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }

    extern "C" fn back_cb(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    #[test]
    fn from_system_with_no_reported_colors_falls_back_to_office_2013() {
        // SystemStyle::default() may pre-fill platform colors; the fallback
        // contract is about a system that reports NO colors at all.
        let mut sys = azul_css::system::SystemStyle::default();
        sys.colors = azul_css::system::SystemColors::default();
        assert_eq!(
            BackstageTheme::from_system(sys.clone()),
            BackstageTheme::office_2013()
        );
        assert_eq!(
            BackstageStyle::from_system(sys),
            BackstageStyle::office_2013()
        );
    }

    #[test]
    fn from_system_separates_the_accent_nav_band_from_the_window_surface() {
        let accent = ColorU {
            r: 61,
            g: 174,
            b: 233,
            a: 255,
        };
        let surface = ColorU {
            r: 35,
            g: 38,
            b: 41,
            a: 255,
        };
        let mut sys = azul_css::system::SystemStyle::default();
        sys.colors = azul_css::system::SystemColors::default();
        sys.colors.accent = Some(accent).into();
        sys.colors.window_background = Some(surface).into();

        let t = BackstageTheme::from_system(sys);
        assert_eq!(t.nav_bg, accent, "the nav column is the accent band");
        assert_eq!(t.content_bg, surface, "the pane is the window surface");
        assert_ne!(
            t.nav_bg, t.content_bg,
            "a dark surface must not swallow the nav column"
        );
        assert_eq!(
            t.nav_text,
            BackstageTheme::office_2013().nav_text,
            "an unreported colour falls back to its OWN office value"
        );
    }

    // ------------------------------------------------------------------
    // Constructors and invariants
    // ------------------------------------------------------------------

    #[test]
    fn backstage_office_2013_has_eleven_items_with_account_gapped() {
        let b = Backstage::office_2013();
        assert_eq!(b.nav_items.len(), 11);
        assert_eq!(b.active_item, 0);
        let items = b.nav_items.as_slice();
        assert_eq!(items[0].label.as_str(), "Info");
        assert_eq!(items[8].label.as_str(), "Close");
        assert_eq!(items[9].label.as_str(), "Account");
        assert!(items[9].gap_before);
        assert_eq!(items[10].label.as_str(), "Options");
        assert!(!items[10].gap_before);
    }

    #[test]
    fn backstage_style_default_is_office_2013() {
        assert_eq!(BackstageStyle::default(), BackstageStyle::office_2013());
    }

    #[test]
    fn backstage_behavior_default_closes_on_escape() {
        assert_eq!(
            BackstageBehavior::default(),
            BackstageBehavior::office_2013()
        );
        assert!(BackstageBehavior::office_2013().close_on_escape);
        assert!(!BackstageBehavior::inert().close_on_escape);
    }

    // ------------------------------------------------------------------
    // DOM shape
    // ------------------------------------------------------------------

    #[test]
    fn dom_renders_nav_and_right_side() {
        let dom = Backstage::office_2013().dom();
        assert_eq!(dom.children.as_ref().len(), 2);
        // Nav: back button + 11 items.
        let nav = &dom.children.as_ref()[0];
        assert_eq!(nav.children.as_ref().len(), 12);
    }

    #[test]
    fn dom_places_the_title_strip_above_the_content() {
        let strip = Dom::create_div();
        let dom = Backstage::office_2013().with_title_strip(strip).dom();
        let right = &dom.children.as_ref()[1];
        assert_eq!(right.children.as_ref().len(), 2);
    }

    #[test]
    fn nav_items_get_click_callbacks_only_with_a_select_handler() {
        // Without a handler: inert items.
        let dom = Backstage::office_2013().dom();
        let nav = &dom.children.as_ref()[0];
        for item in nav.children.as_ref().iter().skip(1) {
            assert!(item.root.callbacks.as_ref().is_empty());
        }
        // With a handler: every item carries one.
        let dom = Backstage::office_2013()
            .with_on_nav_select(RefAny::new(()), nav_cb as BackstageOnNavSelectCallbackType)
            .dom();
        let nav = &dom.children.as_ref()[0];
        for item in nav.children.as_ref().iter().skip(1) {
            assert_eq!(item.root.callbacks.as_ref().len(), 1);
        }
    }

    #[test]
    fn escape_handler_is_attached_only_with_behavior_and_back_callback() {
        // Behavior on, no back callback: nothing to invoke, no handler.
        let dom = Backstage::office_2013().dom();
        assert!(dom.root.callbacks.as_ref().is_empty());
        // Behavior on + back callback: window-level key handler on the root.
        let dom = Backstage::office_2013()
            .with_on_back(
                RefAny::new(()),
                back_cb as super::super::button::ButtonOnClickCallbackType,
            )
            .dom();
        assert_eq!(dom.root.callbacks.as_ref().len(), 1);
        assert_eq!(
            dom.root.callbacks.as_ref()[0].event,
            EventFilter::Window(WindowEventFilter::VirtualKeyDown)
        );
        // Behavior off: no handler even with a back callback.
        let dom = Backstage::office_2013()
            .with_on_back(
                RefAny::new(()),
                back_cb as super::super::button::ButtonOnClickCallbackType,
            )
            .with_behavior(BackstageBehavior::inert())
            .dom();
        assert!(dom.root.callbacks.as_ref().is_empty());
    }

    #[test]
    fn active_item_gets_the_active_class() {
        let dom = Backstage::office_2013().with_active_item(2).dom();
        let nav = &dom.children.as_ref()[0];
        // Nav child 0 is the back button; item i is child i+1.
        let active = &nav.children.as_ref()[3];
        let classes = active.root.get_ids_and_classes();
        assert!(classes.as_ref().iter().any(|c| match c {
            Class(s) => s.as_str().contains("nav-item-active"),
            IdOrClass::Id(_) => false,
        }));
    }
}

/// The backstage's flora look (W5c): flora.css's flyout navigation drawer
/// (`.mobile-menu`) on the Office column - a leaf of paper laid on the page,
/// its items bare keys in the house ink that lift under the pointer, the
/// selected one the sunken stone in a brass edge, the back button the accent
/// stone in a brass collar - by day and by night; and the theme option
/// around it.
#[cfg(test)]
mod flora_tests {
    use azul_css::{
        dynamic_selector::{DynamicSelector, DynamicSelectorVec, PseudoStateType, ThemeCondition},
        props::property::CssPropertyType,
    };

    use super::*;
    use crate::widgets::themes::{flora, theme_blocks::checks, theme_checks as tc};

    const NAV: &str = "__azul-native-backstage-nav";
    const ITEM: &str = "__azul-native-backstage-nav-item";
    const ACTIVE: &str = "__azul-native-backstage-nav-item-active";
    const RIGHT: &str = "__azul-native-backstage-right";
    const CONTENT: &str = "__azul-native-backstage-content";
    const BUTTON: &str = "__azul-native-button";

    extern "C" fn back(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    /// The Office nav ("Open" selected, "Account" after the gap), a back
    /// handler and a pane of the caller's.
    fn fixture() -> Backstage {
        Backstage::office_2013()
            .with_active_item(2)
            .with_on_back(
                RefAny::new(()),
                back as super::super::button::ButtonOnClickCallbackType,
            )
            .with_content(Dom::create_p_with_text("Recent documents"))
    }

    fn pinned(theme: UiTheme) -> Dom {
        fixture().with_theme(theme).dom()
    }

    fn node<'a>(dom: &'a Dom, class: &str) -> &'a Dom {
        tc::find(dom, class).unwrap_or_else(|| panic!("the backstage renders a {class}"))
    }

    /// The first nav item that is not the selected one.
    fn plain_item(dom: &Dom) -> &Dom {
        tc::find_all(dom, ITEM)
            .into_iter()
            .find(|n| !tc::has_class(n, ACTIVE))
            .expect("the backstage renders an unselected item")
    }

    /// `node`'s background in the light or dark mode and `state` (`None`: at
    /// rest), as its layers.
    fn face(node: &Dom, dark: bool, state: Option<PseudoStateType>) -> Vec<StyleBackgroundContent> {
        tc::resolve(node, CssPropertyType::BackgroundContent, dark, state)
            .map(|p| tc::bg_layers(&p))
            .unwrap_or_default()
    }

    fn fill(color: ColorU) -> Vec<StyleBackgroundContent> {
        vec![StyleBackgroundContent::Color(color)]
    }

    /// The colour of one border edge (`ty`) in the light or dark mode and
    /// `state`.
    fn edge(
        node: &Dom,
        ty: CssPropertyType,
        dark: bool,
        state: Option<PseudoStateType>,
    ) -> Option<ColorU> {
        tc::resolve(node, ty, dark, state)
            .as_ref()
            .and_then(tc::border_color)
    }

    #[test]
    fn a_new_backstage_follows_the_app_theme_and_set_theme_and_with_theme_agree() {
        assert_eq!(
            Backstage::office_2013().theme,
            OptionUiTheme::None,
            "no opinion until the app picks a theme"
        );
        let mut a = Backstage::office_2013();
        a.set_theme(UiTheme::Flora);
        assert_eq!(a.theme, OptionUiTheme::Some(UiTheme::Flora));
        assert_eq!(
            Backstage::office_2013().with_theme(UiTheme::Flora).theme,
            a.theme
        );
    }

    /// The drawer (`.mobile-menu`) is a leaf (`--fl-sur`) with a `--fl-bd2`
    /// hairline along the edge that faces the page; the page behind it is
    /// the ground (`--fl-pg`), written in the house ink (`--color-text`,
    /// `--fl-ink`) so the caller's pane reads in either mode.
    #[test]
    fn a_flora_backstage_lays_a_drawer_of_paper_on_the_page_in_both_modes() {
        let dom = pinned(UiTheme::Flora);
        let nav = node(&dom, NAV);
        for (dark, page, ink, leaf, hairline) in [
            (false, flora::LIGHT_PG, flora::LIGHT_INK, flora::LIGHT_SUR, flora::LIGHT_BD2),
            (true, flora::DARK_PG, flora::DARK_INK, flora::DARK_SUR, flora::DARK_BD2),
        ] {
            for part in [&dom, node(&dom, RIGHT), node(&dom, CONTENT)] {
                assert_eq!(face(part, dark, None), fill(page), "the page (dark: {dark})");
            }
            assert_eq!(tc::text_color(&dom, dark), Some(ink), "the page's ink (dark: {dark})");
            assert_eq!(face(nav, dark, None), fill(leaf), "the drawer (dark: {dark})");
            assert_eq!(
                edge(nav, CssPropertyType::BorderRightColor, dark, None),
                Some(hairline),
                "the drawer's edge (dark: {dark})"
            );
            assert!(
                tc::resolve(nav, CssPropertyType::BorderRightWidth, dark, None).is_some(),
                "the drawer's edge has a width (dark: {dark})"
            );
        }
    }

    /// `.mobile-menu a`: the house ink on a bare key; under the pointer the
    /// hover face (`--fl-hT` -> `--fl-hB`) in a `--fl-bd` hairline, held the
    /// pressed face (`.nav-links a:active`).
    #[test]
    fn a_flora_nav_item_is_a_bare_key_in_house_ink_that_lifts_under_the_pointer() {
        let dom = pinned(UiTheme::Flora);
        let item = plain_item(&dom);
        for (dark, ink, hover, rim, pressed) in [
            (
                false,
                flora::LIGHT_INK,
                flora::HOVER_FACE_LIGHT,
                flora::LIGHT_BD,
                flora::PRESSED_FACE_LIGHT,
            ),
            (
                true,
                flora::DARK_INK,
                flora::HOVER_FACE_DARK,
                flora::DARK_BD,
                flora::PRESSED_FACE_DARK,
            ),
        ] {
            assert_eq!(tc::text_color(item, dark), Some(ink), "the item's ink (dark: {dark})");
            assert_eq!(
                face(item, dark, None),
                fill(ColorU::TRANSPARENT),
                "bare at rest (dark: {dark})"
            );
            assert_eq!(
                face(item, dark, Some(PseudoStateType::Hover)),
                vec![hover],
                "the hover face (dark: {dark})"
            );
            assert_eq!(
                edge(item, CssPropertyType::BorderTopColor, dark, Some(PseudoStateType::Hover)),
                Some(rim),
                "the hover hairline (dark: {dark})"
            );
            assert_eq!(
                face(item, dark, Some(PseudoStateType::Active)),
                vec![pressed],
                "the pressed face (dark: {dark})"
            );
        }
    }

    /// `.mobile-menu a.active`: the sunken stone (`--fl-gem-sunken`, lit from
    /// below) written in `--fl-on-acc`, in a brass edge (the leaf border,
    /// `--fl-metal-turn`) - its own colour by day and by night, and still the
    /// stone under the pointer and while held.
    #[test]
    fn the_selected_flora_nav_item_is_the_sunken_stone_in_a_brass_edge_in_both_modes() {
        let dom = pinned(UiTheme::Flora);
        let picked = node(&dom, ACTIVE);
        for dark in [false, true] {
            for state in [
                None,
                Some(PseudoStateType::Hover),
                Some(PseudoStateType::Active),
            ] {
                assert_eq!(
                    face(picked, dark, state),
                    flora::selected_stone(),
                    "the stone (dark: {dark}, {state:?})"
                );
                assert_eq!(
                    edge(picked, CssPropertyType::BorderTopColor, dark, state),
                    Some(flora::TAB_METAL),
                    "the brass edge (dark: {dark}, {state:?})"
                );
            }
            assert_eq!(
                tc::text_color(picked, dark),
                Some(flora::LIGHT_ON_ACC),
                "the ink on the stone (dark: {dark})"
            );
        }
    }

    /// The back button closes the drawer the FILE stone opened: the raised
    /// accent stone (the ribbon's application button) seated in a brass
    /// collar (`.fl-orb-collar`), its arrow in `--fl-on-acc`; the streak
    /// brightens under the pointer, the stone sinks while held, and it rings
    /// on focus (`.fl-orb:focus-visible`) - in both modes.
    #[test]
    fn the_flora_back_button_is_the_accent_stone_in_a_brass_collar_and_rings_on_focus() {
        let dom = pinned(UiTheme::Flora);
        let button = node(node(&dom, NAV), BUTTON);
        let arrow = &button.children.as_ref()[0];
        for dark in [false, true] {
            assert_eq!(
                face(button, dark, None).first(),
                Some(&StyleBackgroundContent::Color(flora::LIGHT_ACC)),
                "the accent stone (dark: {dark})"
            );
            assert_eq!(
                face(button, dark, Some(PseudoStateType::Hover)).last(),
                Some(&flora::STONE_STREAK_HOVER),
                "the streak brightens under the pointer (dark: {dark})"
            );
            assert_eq!(
                face(button, dark, Some(PseudoStateType::Active)).first(),
                Some(&StyleBackgroundContent::Color(flora::LIGHT_DEEP)),
                "the stone sinks while held (dark: {dark})"
            );
            assert_eq!(
                edge(button, CssPropertyType::BorderTopColor, dark, None),
                Some(flora::TAB_METAL),
                "the brass collar (dark: {dark})"
            );
            assert_eq!(
                tc::text_color(arrow, dark),
                Some(flora::LIGHT_ON_ACC),
                "the arrow (dark: {dark})"
            );
            assert!(
                tc::has_focus_ring(button, dark),
                "the back button rings on focus (dark: {dark})"
            );
        }
    }

    /// The drawer's items are keys inset from its edges (`.mobile-menu`:
    /// `padding: 14px 12px; gap: 2px`, each item a 1px ring): 12px in from
    /// either side, 2px apart. The label keeps the flat look's indent (24px
    /// from the column's edge) and the items the flat pitch (38px), so a
    /// theme switch moves no word.
    #[test]
    fn a_flora_nav_key_is_inset_like_the_drawers_and_its_label_stays_where_the_flat_one_is() {
        let at_rest = |n: &Dom, ty: CssPropertyType| tc::resolve(n, ty, false, None);
        let flat_dom = pinned(UiTheme::Flat);
        let flat = plain_item(&flat_dom);
        assert_eq!(
            at_rest(flat, CssPropertyType::PaddingLeft),
            Some(P::const_padding_left(LayoutPaddingLeft::const_px(24)))
        );
        assert_eq!(
            at_rest(flat, CssPropertyType::Height),
            Some(P::const_height(LayoutHeight::const_px(38)))
        );
        let flora_dom = pinned(UiTheme::Flora);
        let key = plain_item(&flora_dom);
        // 12px inset + the 1px ring + 11px padding: the flat 24px indent.
        assert_eq!(
            at_rest(key, CssPropertyType::MarginLeft),
            Some(P::const_margin_left(LayoutMarginLeft::const_px(12)))
        );
        assert_eq!(
            at_rest(key, CssPropertyType::MarginRight),
            Some(P::const_margin_right(LayoutMarginRight::const_px(12)))
        );
        assert_eq!(
            at_rest(key, CssPropertyType::BorderLeftWidth),
            Some(P::const_border_left_width(LayoutBorderLeftWidth::const_px(1)))
        );
        assert_eq!(
            at_rest(key, CssPropertyType::PaddingLeft),
            Some(P::const_padding_left(LayoutPaddingLeft::const_px(11)))
        );
        // A 36px key with 1px above and below: the flat 38px pitch, 2px
        // between two keys.
        assert_eq!(
            at_rest(key, CssPropertyType::Height),
            Some(P::const_height(LayoutHeight::const_px(36)))
        );
        assert_eq!(
            at_rest(key, CssPropertyType::MarginTop),
            Some(P::const_margin_top(LayoutMarginTop::const_px(1)))
        );
        assert_eq!(
            at_rest(key, CssPropertyType::MarginBottom),
            Some(P::const_margin_bottom(LayoutMarginBottom::const_px(1)))
        );
    }

    #[test]
    fn the_flora_backstage_keeps_every_theme_invariant() {
        // The selected item plain, and the selected item after the gap.
        for active in [2usize, 9] {
            let dom = fixture()
                .with_active_item(active)
                .with_theme(UiTheme::Flora)
                .dom();
            tc::assert_theme_invariants(&format!("flora backstage, item {active} active"), &dom);
        }
    }

    #[test]
    fn a_pinned_backstage_builds_its_back_button_in_its_theme_under_any_app_theme() {
        for (theme, other) in [
            (UiTheme::Flat, UiTheme::Flora),
            (UiTheme::Flora, UiTheme::Flat),
        ] {
            let marker = match theme {
                UiTheme::Flat => style_kit::FLAT_CLASS,
                UiTheme::Flora => style_kit::FLORA_CLASS,
            };
            // Built for the OTHER app theme: the pin holds.
            let dom = checks::under(other, || pinned(theme));
            assert!(tc::has_class(&dom, marker), "{theme:?}: the root carries its marker");
            assert!(
                tc::has_class(node(&dom, BUTTON), marker),
                "{theme:?}: the back button is built in the backstage's theme"
            );
            assert!(
                checks::theme_names(&dom).is_empty(),
                "{theme:?}: a pinned backstage carries theme blocks {:?}",
                checks::theme_names(&dom)
            );
        }
    }

    #[test]
    fn an_unpinned_backstage_follows_the_app_theme() {
        for active in [2usize, 9] {
            checks::assert_follows_the_app_theme(
                &format!("backstage, item {active} active"),
                || fixture().with_active_item(active).dom(),
                |t| fixture().with_active_item(active).with_theme(t).dom(),
            );
        }
    }

    /// GENERIC rules go outside the theme blocks: what both looks declare
    /// alike - the column's layout, the flex boxes, the font, the back
    /// button's circle - an unpinned backstage declares ONCE, unconditioned,
    /// under either app theme.
    #[test]
    fn a_following_backstage_declares_what_both_looks_share_once_outside_the_theme_blocks() {
        use CssPropertyType as T;
        fn once_unconditioned(node: &Dom, ty: CssPropertyType) -> bool {
            let decls: Vec<(&P, &DynamicSelectorVec)> = node
                .root
                .style
                .iter_inline_properties()
                .filter(|(p, _)| p.get_type() == ty)
                .collect();
            decls.len() == 1
                && decls[0].1.as_ref().iter().all(|c| {
                    !matches!(c, DynamicSelector::Theme(ThemeCondition::Custom(_)))
                })
        }
        for app in [UiTheme::Flat, UiTheme::Flora] {
            let dom = checks::under(app, || fixture().dom());
            let shared: [(&str, &Dom, &[CssPropertyType]); 6] = [
                (
                    "root",
                    &dom,
                    &[
                        T::BoxSizing,
                        T::Display,
                        T::FlexDirection,
                        T::FlexGrow,
                        T::FontFamily,
                        T::FontSize,
                    ],
                ),
                (
                    "nav",
                    node(&dom, NAV),
                    &[T::Display, T::FlexDirection, T::FlexShrink, T::Width],
                ),
                (
                    "nav item",
                    plain_item(&dom),
                    &[T::Display, T::FlexDirection, T::AlignItems, T::FontSize, T::Cursor],
                ),
                (
                    "back button",
                    node(&dom, BUTTON),
                    &[
                        T::Width,
                        T::Height,
                        T::MarginLeft,
                        T::BorderTopWidth,
                        T::BorderTopLeftRadius,
                    ],
                ),
                (
                    "right side",
                    node(&dom, RIGHT),
                    &[T::Display, T::FlexDirection, T::FlexGrow],
                ),
                (
                    "content host",
                    node(&dom, CONTENT),
                    &[T::Display, T::FlexDirection, T::FlexGrow],
                ),
            ];
            for (part, n, types) in shared {
                for ty in types {
                    assert!(
                        once_unconditioned(n, *ty),
                        "built for {app:?}: the {part}'s {ty:?} is not declared once outside \
                         the theme blocks"
                    );
                }
            }
        }
    }

    /// R5: the backstage's layout - the root's row, the column, the right
    /// side and the content host, every nav item's row with its pointer and
    /// unselectable label, the back button's centred circle - is its BASE:
    /// flora paints the flat part's geometry (`chrome_geometry`), so every
    /// structure declaration is declared once, outside every `@theme` block.
    /// The first, a middle and the item after the gap selected.
    #[test]
    fn a_backstage_declares_its_structure_once_for_every_theme() {
        for t in checks::BOTH {
            for active in [0usize, 2, 9] {
                let dom = checks::under(t, || fixture().with_active_item(active).dom());
                tc::assert_structure_is_shared(
                    &format!("backstage, item {active} active, built for {}", t.name()),
                    &dom,
                    &[],
                );
            }
        }
    }

    #[test]
    fn a_part_the_caller_set_is_the_callers_in_both_looks() {
        let custom = CssPropertyWithConditionsVec::from_vec(vec![Cond::simple(P::const_width(
            LayoutWidth::const_px(200),
        ))]);
        let want: azul_css::css::Css = custom.clone().into();
        for theme in [None, Some(UiTheme::Flat), Some(UiTheme::Flora)] {
            let mut style = BackstageStyle::office_2013();
            style.nav_style = OptionCssPropertyWithConditionsVec::Some(custom.clone());
            let b = fixture().with_style(style);
            let dom = match theme {
                Some(t) => b.with_theme(t).dom(),
                None => b.dom(),
            };
            assert_eq!(node(&dom, NAV).root.style, want, "{theme:?}");
        }
    }

    /// Flora is a second look, not a change to the first: the flat backstage
    /// is the Office palette's parts, declaration for declaration.
    #[test]
    fn the_flat_backstage_is_the_office_look_it_always_was() {
        let office = BackstageStyle::office_2013();
        let css = |v: CssPropertyWithConditionsVec| -> azul_css::css::Css { v.into() };
        let dom = pinned(UiTheme::Flat);
        assert_eq!(dom.root.style, css(office.resolved_root_style()));
        assert_eq!(node(&dom, NAV).root.style, css(office.resolved_nav_style()));
        assert_eq!(node(&dom, RIGHT).root.style, css(office.resolved_right_style()));
        assert_eq!(
            node(&dom, CONTENT).root.style,
            css(office.resolved_content_style())
        );
        assert_eq!(
            plain_item(&dom).root.style,
            css(office.resolved_nav_item_style())
        );
        assert_eq!(
            node(&dom, ACTIVE).root.style,
            css(merged_style(
                &office.resolved_nav_item_style(),
                &office.resolved_nav_item_active_style()
            ))
        );
        let button = node(&dom, BUTTON);
        assert_eq!(button.root.style, css(office.resolved_back_button_style()));
        assert_eq!(
            button.children.as_ref()[0].root.style,
            css(office.resolved_back_icon_style())
        );
    }

    /// The flat page (the root, the right side and the pane) is the Office
    /// white by day and flat's dark page at night: the caller's pane is
    /// written in the window's ink, and AzCalendar's dark backstage showed
    /// white headings and buttons on a white page (WIDGETS7, prebuilt
    /// AzCalendar `--mode dark`, FILE > Calendars).
    #[test]
    fn the_flat_backstage_page_is_dark_in_the_dark_mode() {
        let dom = pinned(UiTheme::Flat);
        for part in [&dom, node(&dom, RIGHT), node(&dom, CONTENT)] {
            assert_eq!(face(part, false, None), fill(WHITE), "the page by day");
            assert_eq!(
                face(part, true, None),
                fill(flat::DARK_PG),
                "the page at night"
            );
        }
    }
}

/// A backstage page taller than the window scrolls inside the backstage: the backstage (a
/// flex item of the window's column) and its page column take the height they are given, not
/// their content's - CSS's automatic minimum size of a flex item is its content's, so every
/// flex level between the window and the page's scroller declares `min-height: 0`. AzMail's
/// File > Account Settings (E2E-A, 2026-10-06): the backstage was 1016 px in an 832 px shell,
/// the settings never scrolled, and "Create a key" stood below the window's edge.
#[cfg(test)]
mod a_backstage_page_scrolls_inside_the_window_tests {
    use azul_core::{
        dom::{Dom, DomId, NodeId},
        geom::LogicalSize,
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use rust_fontconfig::FcFontCache;

    use super::Backstage;
    use crate::{
        callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
    };

    /// The used size of the first node carrying the class `class`, in a 800 x 400 window.
    fn height_of_class(mut dom: Dom, class: &str) -> f32 {
        let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(800.0, 400.0);
        lw.current_window_state = ws.clone();
        lw.layout_and_generate_display_list(
            styled,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
        let lr = &lw.layout_results[&DomId::ROOT_ID];
        let node = lr
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|n| n.has_class(class))
            .unwrap_or_else(|| panic!("no node .{class}"));
        let index = *lr
            .layout_tree
            .dom_to_layout
            .get(&NodeId::new(node))
            .and_then(|v| v.first())
            .expect("the node is laid out");
        lr.layout_tree
            .get(index)
            .and_then(|n| n.used_size)
            .expect("the node has a size")
            .height
    }

    /// The window's column (as OfficeShell's backstage slot: `min-height: 0`) holding a
    /// backstage whose page is a scroller over 2000 px of content.
    fn window_with_a_tall_page() -> Dom {
        let page = Dom::create_div()
            .with_css(
                "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                 overflow-y: auto;",
            )
            .with_child(Dom::create_div().with_css("height: 2000px; flex-shrink: 0;"));
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; margin: 0px; height: 100%;")
            .with_child(
                Dom::create_div()
                    .with_css(
                        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;",
                    )
                    .with_child(Backstage::office_2013().with_content(page).dom()),
            )
    }

    #[test]
    fn a_tall_page_scrolls_inside_the_backstage_instead_of_growing_it() {
        for class in [
            "__azul-native-backstage",
            "__azul-native-backstage-right",
            "__azul-native-backstage-content",
        ] {
            let h = height_of_class(window_with_a_tall_page(), class);
            assert!(
                h <= 400.5,
                ".{class} is {h} px tall in a 400 px window: it grew to its page's content"
            );
        }
    }
}

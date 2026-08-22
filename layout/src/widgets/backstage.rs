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
//! [`Backstage::content`] — the backstage does not model document state.
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

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec,
        WindowEventFilter,
    },
    refany::RefAny,
    window::VirtualKeyCode,
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

use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut};

use crate::callbacks::CallbackInfo;

use super::button::{Button, ButtonOnClick, OptionButtonOnClick};

// -- Callbacks --

/// Callback signature invoked when a nav item is clicked (receives the item
/// index).
pub type BackstageOnNavSelectCallbackType =
    extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    BackstageOnNavSelect, OptionBackstageOnNavSelect,
    BackstageOnNavSelectCallback, BackstageOnNavSelectCallbackType
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
    extra_args:     [ item_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- the Office-2013-era look palette (seeds BackstageTheme::office_2013) --

const WHITE: ColorU = ColorU { r: 255, g: 255, b: 255, a: 255 };
const TRANSPARENT: ColorU = ColorU { r: 0, g: 0, b: 0, a: 0 };
/// Office 2013 accent blue (#2B579A): the nav column fill.
const W13_BLUE: ColorU = ColorU { r: 43, g: 87, b: 154, a: 255 };
/// Hover fill on nav items (#3465AC).
const W13_NAV_HOVER: ColorU = ColorU { r: 52, g: 101, b: 172, a: 255 };
/// Active nav item fill (#3E6DB5).
const W13_NAV_ACTIVE: ColorU = ColorU { r: 62, g: 109, b: 181, a: 255 };

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

fn cond_bg_hover(c: ColorU) -> Cond {
    Cond::on_hover(P::const_background_content(bg_vec(c)))
}

const fn cond_text_color(c: ColorU) -> Cond {
    Cond::simple(P::const_text_color(StyleTextColor { inner: c }))
}

const fn cond_border_box() -> Cond {
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox))
}

fn push_ring_border(v: &mut Vec<Cond>, c: ColorU, width: isize, radius: isize) {
    v.push(Cond::simple(P::const_border_top_width(LayoutBorderTopWidth::const_px(width))));
    v.push(Cond::simple(P::const_border_left_width(LayoutBorderLeftWidth::const_px(width))));
    v.push(Cond::simple(P::const_border_right_width(LayoutBorderRightWidth::const_px(width))));
    v.push(Cond::simple(P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(width))));
    v.push(Cond::simple(P::const_border_top_style(StyleBorderTopStyle { inner: BorderStyle::Solid })));
    v.push(Cond::simple(P::const_border_left_style(StyleBorderLeftStyle { inner: BorderStyle::Solid })));
    v.push(Cond::simple(P::const_border_right_style(StyleBorderRightStyle { inner: BorderStyle::Solid })));
    v.push(Cond::simple(P::const_border_bottom_style(StyleBorderBottomStyle { inner: BorderStyle::Solid })));
    v.push(Cond::simple(P::const_border_top_color(StyleBorderTopColor { inner: c })));
    v.push(Cond::simple(P::const_border_left_color(StyleBorderLeftColor { inner: c })));
    v.push(Cond::simple(P::const_border_right_color(StyleBorderRightColor { inner: c })));
    v.push(Cond::simple(P::const_border_bottom_color(StyleBorderBottomColor { inner: c })));
    v.push(Cond::simple(P::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(radius))));
    v.push(Cond::simple(P::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(radius))));
    v.push(Cond::simple(P::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(radius))));
    v.push(Cond::simple(P::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(radius))));
}

fn theme_root(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(NAV_TEXT_PX))),
        cond_bg(t.content_bg),
    ])
}

fn theme_nav(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink { inner: FloatValue::const_new(0) })),
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
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink { inner: FloatValue::const_new(0) })),
        Cond::simple(P::const_width(LayoutWidth::const_px(BACK_D))),
        Cond::simple(P::const_height(LayoutHeight::const_px(BACK_D))),
        Cond::simple(P::const_margin_top(LayoutMarginTop::const_px(16))),
        Cond::simple(P::const_margin_left(LayoutMarginLeft::const_px(20))),
        Cond::simple(P::const_margin_bottom(LayoutMarginBottom::const_px(18))),
        Cond::simple(P::const_cursor(StyleCursor::Pointer)),
        Cond::simple(P::user_select(StyleUserSelect::None)),
        cond_bg(TRANSPARENT),
        cond_bg_hover(t.nav_hover_bg),
    ];
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
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink { inner: FloatValue::const_new(0) })),
        Cond::simple(P::const_height(LayoutHeight::const_px(NAV_ITEM_H))),
        Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(24))),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(NAV_TEXT_PX))),
        Cond::simple(P::const_cursor(StyleCursor::Pointer)),
        Cond::simple(P::user_select(StyleUserSelect::None)),
        cond_text_color(t.nav_text),
        cond_bg(TRANSPARENT),
        cond_bg_hover(t.nav_hover_bg),
    ])
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
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        cond_bg(t.content_bg),
    ])
}

fn theme_content(t: &BackstageTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        cond_bg(t.content_bg),
    ])
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
    pub root_style: CssPropertyWithConditionsVec,
    /// The nav column.
    pub nav_style: CssPropertyWithConditionsVec,
    /// Container style injected into the back [`Button`] (the ring).
    pub back_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into the back [`Button`] (the arrow).
    pub back_icon_style: CssPropertyWithConditionsVec,
    /// One nav item.
    pub nav_item_style: CssPropertyWithConditionsVec,
    /// APPENDED to the active nav item.
    pub nav_item_active_style: CssPropertyWithConditionsVec,
    /// APPENDED to a `gap_before` nav item.
    pub nav_item_gap_style: CssPropertyWithConditionsVec,
    /// The right side (title strip over content).
    pub right_style: CssPropertyWithConditionsVec,
    /// The content host for the active pane.
    pub content_style: CssPropertyWithConditionsVec,
}

impl BackstageStyle {
    /// The the Office-2013-era look look (#2B579A nav, white content) - the default.
    #[must_use]
    pub fn office_2013() -> Self {
        Self::from_theme(BackstageTheme::office_2013())
    }

    /// Derives every part style from the given palette.
    #[must_use]
    pub fn from_theme(theme: BackstageTheme) -> Self {
        let t = &theme;
        Self {
            theme,
            root_style: theme_root(t),
            nav_style: theme_nav(t),
            back_button_style: theme_back_button(t),
            back_icon_style: theme_back_icon(t),
            nav_item_style: theme_nav_item(t),
            nav_item_active_style: theme_nav_item_active(t),
            nav_item_gap_style: theme_nav_item_gap(t),
            right_style: theme_right(t),
            content_style: theme_content(t),
        }
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
        Self { close_on_escape: true }
    }

    /// Every self-driven behavior off.
    #[must_use]
    pub const fn inert() -> Self {
        Self { close_on_escape: false }
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
        Self { label, gap_before: false }
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
impl_vec_clone!(BackstageNavItem, BackstageNavItemVec, BackstageNavItemVecDestructor);
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
}

// -- CSS classes --

static CLS_BACKSTAGE: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-backstage"))];
static CLS_NAV: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-backstage-nav"))];
static CLS_NAV_ITEM: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-backstage-nav-item"))];
static CLS_NAV_ITEM_ACTIVE: &[IdOrClass] = &[
    Class(AzString::from_const_str("__azul-native-backstage-nav-item")),
    Class(AzString::from_const_str("__azul-native-backstage-nav-item-active")),
];
static CLS_RIGHT: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-backstage-right"))];
static CLS_CONTENT: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-backstage-content"))];

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

    /// Renders the backstage.
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
        } = self;

        // -- nav column --
        let mut nav_children: Vec<Dom> = Vec::with_capacity(nav_items.len() + 1);

        {
            let mut b = Button::create(AzString::from_const_str(""));
            b.icon = AzString::from_const_str("arrow_back");
            b.container_style = style.back_button_style.clone();
            b.icon_style = style.back_icon_style.clone();
            b.on_click = on_back.clone();
            nav_children.push(b.dom());
        }

        for (idx, item) in nav_items.into_library_owned_vec().into_iter().enumerate() {
            let (classes, mut part_style) = if idx == active_item {
                (
                    CLS_NAV_ITEM_ACTIVE,
                    merged_style(&style.nav_item_style, &style.nav_item_active_style),
                )
            } else {
                (CLS_NAV_ITEM, style.nav_item_style.clone())
            };
            if item.gap_before {
                part_style = merged_style(&part_style, &style.nav_item_gap_style);
            }
            // The nav item div is display:flex — a raw text run cannot be a
            // flex item (no anonymous-block wrapping in azul), so the label
            // gets its `<p>` per the label convention. Caught by `dom_lint`
            // on its very first run.
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
                .with_css_props(part_style)
                .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(item.label)]));
            if let Some(cb) = on_nav_select.as_ref() {
                d = d.with_callbacks(vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    callback: CoreCallback {
                        cb: on_backstage_nav_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(NavClickData {
                        item_idx: idx,
                        on_nav_select: cb.clone(),
                    }),
                }].into());
            }
            nav_children.push(d);
        }

        let nav = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_NAV))
            .with_css_props(style.nav_style.clone())
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
                .with_css_props(style.content_style.clone())
                .with_children(DomVec::from_vec(vec![pane])),
        );

        let right = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_RIGHT))
            .with_css_props(style.right_style.clone())
            .with_children(DomVec::from_vec(right_children));

        let mut root = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_BACKSTAGE))
            .with_css_props(style.root_style)
            .with_children(DomVec::from_vec(vec![nav, right]));

        // Escape leaves the backstage (window-level, focus-independent).
        if behavior.close_on_escape {
            if let Some(back) = on_back.into_option() {
                root = root.with_callbacks(vec![CoreCallbackData {
                    event: EventFilter::Window(WindowEventFilter::VirtualKeyDown),
                    callback: CoreCallback {
                        cb: on_backstage_key_down as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(EscCloseData { on_back: back }),
                }].into());
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
    if extra.as_ref().is_empty() {
        return base.clone();
    }
    let mut v: Vec<Cond> = base.as_ref().to_vec();
    v.extend_from_slice(extra.as_ref());
    CssPropertyWithConditionsVec::from_vec(v)
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
        info.get_current_keyboard_state().current_virtual_keycode.into_option(),
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
        assert_eq!(BackstageBehavior::default(), BackstageBehavior::office_2013());
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
            .with_on_back(RefAny::new(()), back_cb as super::super::button::ButtonOnClickCallbackType)
            .dom();
        assert_eq!(dom.root.callbacks.as_ref().len(), 1);
        assert_eq!(
            dom.root.callbacks.as_ref()[0].event,
            EventFilter::Window(WindowEventFilter::VirtualKeyDown)
        );
        // Behavior off: no handler even with a back callback.
        let dom = Backstage::office_2013()
            .with_on_back(RefAny::new(()), back_cb as super::super::button::ButtonOnClickCallbackType)
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

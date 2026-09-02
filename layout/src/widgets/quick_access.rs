//! Microsoft Office-style title band with a Quick Access Toolbar
//! (the Office-2013-era look look by default).
//!
//! Models the top chrome band of an Office document window:
//!
//! ```text
//! QuickAccessBar ─ leading slot            (app logo, any user Dom)
//!                ─ quick-access actions    QuickAccessAction (save / undo / redo)
//!                ─ customize arrow         ("▾" menu glyph, optional)
//!                ─ window title            (centered, "Document1 - AzWriter")
//!                ─ trailing actions        QuickAccessAction (help, ribbon options)
//!                ─ window buttons          minimize / maximize / close
//! ```
//!
//! Buttons are not re-implemented: every action and window button expands
//! to the existing [`super::button::Button`] widget with title-band part
//! styles injected through `Button`'s public style fields (the same
//! composition rule the ribbon uses). Icons resolve through the registered
//! icon provider (Material Icons by default): "save", "undo", "redo",
//! "help_outline", "minimize", "crop_square", "close".
//!
//! This widget draws WINDOW CHROME AS DOM - pair it with borderless window
//! decorations, or use [`super::titlebar::Titlebar`] when the OS should
//! draw its native caption instead. Window-button clicks are forwarded to
//! application callbacks; the band never calls `modify_window_state`
//! itself, so it stays inert in mockups and screenshot harnesses.
//!
//! All visual parts are exposed on [`QuickAccessStyle`] (defaults =
//! the Office-2013-era look look, [`QuickAccessStyle::office_2013`]); replace any field to
//! re-theme without touching widget code. There is no behavior struct: the
//! band has no self-driven chrome interactions.

use azul_core::{
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions as Cond, CssPropertyWithConditionsVec},
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

use azul_css::system::SystemStyle;
use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut};

use super::button::{Button, OptionButtonOnClick};

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- the Office-2013-era look palette (seeds QuickAccessTheme::office_2013) --

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
/// Title text gray (#5D5D5D).
const W13_TITLE_TEXT: ColorU = ColorU {
    r: 93,
    g: 93,
    b: 93,
    a: 255,
};
/// Action glyph gray (#6A6A6A).
const W13_ICON_GRAY: ColorU = ColorU {
    r: 106,
    g: 106,
    b: 106,
    a: 255,
};
/// Hover fill on band controls (#E5E5E5).
const W13_HOVER_BG: ColorU = ColorU {
    r: 229,
    g: 229,
    b: 229,
    a: 255,
};
/// Pressed fill (#CCCCCC).
const W13_PRESSED_BG: ColorU = ColorU {
    r: 204,
    g: 204,
    b: 204,
    a: 255,
};
/// Close button hover fill (#E81123, the Windows caption red).
const W13_CLOSE_HOVER: ColorU = ColorU {
    r: 232,
    g: 17,
    b: 35,
    a: 255,
};

// -- Metrics (the Office-2013-era look, logical px) --

/// Band height.
const BAR_HEIGHT: isize = 28;
/// Quick-access glyph size.
const QAT_ICON_PX: isize = 15;
/// Window-button glyph size.
const WIN_ICON_PX: isize = 14;
/// Width of one quick-access button.
const QAT_BUTTON_W: isize = 26;
/// Width of one window button (office-2013: wide flat caption buttons).
const WIN_BUTTON_W: isize = 34;
/// Title text size.
const TITLE_PX: isize = 12;

// -- Theme --

/// Color palette from which a full [`QuickAccessStyle`] is derived via
/// [`QuickAccessStyle::from_theme`]. All fields are plain colors, so themes
/// are trivially constructible over FFI. Preset:
/// [`QuickAccessTheme::office_2013`] (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct QuickAccessTheme {
    /// Band fill (office-2013: white).
    pub bg: ColorU,
    /// Title text color.
    pub text: ColorU,
    /// Action and window-button glyph color.
    pub icon: ColorU,
    /// Hover fill on band controls.
    pub hover_bg: ColorU,
    /// Pressed fill on band controls.
    pub pressed_bg: ColorU,
    /// Close button hover fill (caption red).
    pub close_hover_bg: ColorU,
    /// Close button glyph color while hovered.
    pub close_hover_icon: ColorU,
}

impl QuickAccessTheme {
    /// The the Office-2013-era look palette: white band, gray glyphs, red close hover.
    #[must_use]
    pub const fn office_2013() -> Self {
        Self {
            bg: WHITE,
            text: W13_TITLE_TEXT,
            icon: W13_ICON_GRAY,
            hover_bg: W13_HOVER_BG,
            pressed_bg: W13_PRESSED_BG,
            close_hover_bg: W13_CLOSE_HOVER,
            close_hover_icon: WHITE,
        }
    }

    /// Extracts a band palette from the OS theme.
    ///
    /// The quick-access band IS the window's titlebar in an app that draws its
    /// own chrome, so it reads the OS TITLEBAR colours first
    /// (`SystemMetrics::titlebar`, which every desktop keeps separately from
    /// the window background and which is what the user compares against the
    /// native window next to it) and falls back to the general palette, then
    /// to this field's own Office-2013 value. Same discipline as
    /// [`super::ribbon::RibbonTheme::from_system`]: one system colour per
    /// field, no colour arithmetic, so the FFI-observable behaviour stays
    /// trivial to reason about.
    ///
    /// Takes the style by value (FFI constructor convention).
    #[must_use]
    pub fn from_system(style: SystemStyle) -> Self {
        let d = Self::office_2013();
        let c = &style.colors;
        let tb = &style.metrics.titlebar;
        let secondary = c.secondary_text.into_option();
        let title_text = tb.text_active.into_option().or(c.text.into_option());
        Self {
            bg: tb
                .background_active
                .into_option()
                .or(c.window_background.into_option())
                .unwrap_or(d.bg),
            text: title_text.unwrap_or(d.text),
            icon: secondary.or(title_text).unwrap_or(d.icon),
            hover_bg: tb
                .button_hover_background
                .into_option()
                .or(c.selection_background_inactive.into_option())
                .unwrap_or(d.hover_bg),
            pressed_bg: c.selection_background.into_option().unwrap_or(d.pressed_bg),
            // The close button keeps its own colour on every platform (Breeze
            // and Windows both go red), which is why it is a separate metric
            // and never falls back to the ordinary hover fill.
            close_hover_bg: tb
                .close_button_hover_background
                .into_option()
                .unwrap_or(d.close_hover_bg),
            close_hover_icon: c
                .accent_text
                .into_option()
                .unwrap_or(d.close_hover_icon),
        }
    }
}

impl Default for QuickAccessTheme {
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

fn cond_bg_active(c: ColorU) -> Cond {
    Cond::on_active(P::const_background_content(bg_vec(c)))
}

const fn cond_text_color(c: ColorU) -> Cond {
    Cond::simple(P::const_text_color(StyleTextColor { inner: c }))
}

const fn cond_border_box() -> Cond {
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox))
}

fn push_row_center(v: &mut Vec<Cond>) {
    v.push(Cond::simple(P::const_display(LayoutDisplay::Flex)));
    v.push(Cond::simple(P::const_flex_direction(
        LayoutFlexDirection::Row,
    )));
    v.push(Cond::simple(P::const_align_items(LayoutAlignItems::Center)));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })));
}

/// Flat, hover-highlighted button chassis shared by every band control.
/// The explicit TRANSPARENT border overrides the [`Button`] widget's
/// default frame (the classic office-suite band controls are frameless until hovered).
fn push_flat_button(v: &mut Vec<Cond>, t: &QuickAccessTheme) {
    v.push(cond_border_box());
    v.push(Cond::simple(P::const_cursor(StyleCursor::Default)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    v.push(cond_bg(TRANSPARENT));
    push_box_border(v, TRANSPARENT);
    v.push(cond_bg_hover(t.hover_bg));
    v.push(cond_bg_active(t.pressed_bg));
}

/// 1px solid border on all four sides in the given color.
fn push_box_border(v: &mut Vec<Cond>, c: ColorU) {
    v.push(Cond::simple(P::const_border_top_width(
        LayoutBorderTopWidth::const_px(1),
    )));
    v.push(Cond::simple(P::const_border_left_width(
        LayoutBorderLeftWidth::const_px(1),
    )));
    v.push(Cond::simple(P::const_border_right_width(
        LayoutBorderRightWidth::const_px(1),
    )));
    v.push(Cond::simple(P::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
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
}

fn theme_bar(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    v.push(cond_border_box());
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT,
    ))));
    v.push(Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)));
    v.push(Cond::simple(P::const_font_size(StyleFontSize::const_px(
        TITLE_PX,
    ))));
    v.push(cond_bg(t.bg));
    v.push(Cond::simple(P::const_padding_left(
        LayoutPaddingLeft::const_px(8),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_leading(_t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    v.push(Cond::simple(P::const_margin_right(
        LayoutMarginRight::const_px(4),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_action_button(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    push_flat_button(&mut v, t);
    v.push(Cond::simple(P::const_justify_content(
        LayoutJustifyContent::Center,
    )));
    v.push(Cond::simple(P::const_width(LayoutWidth::const_px(
        QAT_BUTTON_W,
    ))));
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT - 4,
    ))));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_action_icon(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(QAT_ICON_PX))),
        cond_text_color(t.icon),
    ])
}

/// The small "customize quick access toolbar" chevron after the actions.
fn theme_menu_arrow(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
        cond_text_color(t.icon),
        Cond::simple(P::const_margin_left(LayoutMarginLeft::const_px(1))),
        Cond::simple(P::const_margin_right(LayoutMarginRight::const_px(4))),
    ])
}

fn theme_title(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::const_text_align(StyleTextAlign::Center)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(TITLE_PX))),
        cond_text_color(t.text),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_window_button(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    push_flat_button(&mut v, t);
    v.push(Cond::simple(P::const_justify_content(
        LayoutJustifyContent::Center,
    )));
    v.push(Cond::simple(P::const_width(LayoutWidth::const_px(
        WIN_BUTTON_W,
    ))));
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT,
    ))));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_window_icon(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(WIN_ICON_PX))),
        cond_text_color(t.icon),
    ])
}

/// APPENDED to the close button: the caption-red hover.
fn theme_close_button(t: &QuickAccessTheme) -> CssPropertyWithConditionsVec {
    // ROUND, because that is what the desktop draws. A close button's red is a
    // hover FILL behind a neutral glyph, and every current desktop draws that
    // fill as a circle - a full-height red rectangle in the corner of the
    // titlebar is the one detail that gives a hand-drawn titlebar away.
    // Half the bar height makes the fill as round as the button allows.
    let radius = PixelValue::px(BAR_HEIGHT as f32 / 2.0);
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_bg_hover(t.close_hover_bg),
        Cond::on_hover(P::const_border_top_left_radius(StyleBorderTopLeftRadius {
            inner: radius,
        })),
        Cond::on_hover(P::const_border_top_right_radius(
            StyleBorderTopRightRadius { inner: radius },
        )),
        Cond::on_hover(P::const_border_bottom_left_radius(
            StyleBorderBottomLeftRadius { inner: radius },
        )),
        Cond::on_hover(P::const_border_bottom_right_radius(
            StyleBorderBottomRightRadius { inner: radius },
        )),
    ])
}

// -- Style --

/// All part styles of the title band. Every part defaults to the the Office-2013-era look
/// look; replace any field for finer control (the same override API as
/// [`super::ribbon::RibbonStyle`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct QuickAccessStyle {
    /// The palette this style bundle was derived from. Kept for consumers
    /// deriving matching custom parts.
    pub theme: QuickAccessTheme,
    /// The band itself (horizontal row).
    pub bar_style: CssPropertyWithConditionsVec,
    /// Wrapper around the leading slot.
    pub leading_style: CssPropertyWithConditionsVec,
    /// Container style injected into one quick-access [`Button`].
    pub action_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into the quick-access [`Button`]s.
    pub action_icon_style: CssPropertyWithConditionsVec,
    /// The customize chevron after the quick-access actions.
    pub menu_arrow_style: CssPropertyWithConditionsVec,
    /// The centered window title.
    pub title_style: CssPropertyWithConditionsVec,
    /// Container style injected into the window [`Button`]s.
    pub window_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into the window [`Button`]s.
    pub window_icon_style: CssPropertyWithConditionsVec,
    /// APPENDED to the close [`Button`] (caption-red hover).
    pub close_button_style: CssPropertyWithConditionsVec,
}

impl QuickAccessStyle {
    /// The the Office-2013-era look look (white band, gray glyphs) - the default.
    #[must_use]
    pub fn office_2013() -> Self {
        Self::from_theme(QuickAccessTheme::office_2013())
    }

    /// Every part style, derived from the OS theme - see
    /// [`QuickAccessTheme::from_system`]. Takes the style by value (FFI
    /// constructor convention).
    #[must_use]
    pub fn from_system(style: SystemStyle) -> Self {
        Self::from_theme(QuickAccessTheme::from_system(style))
    }

    /// Derives every part style from the given palette.
    #[must_use]
    pub fn from_theme(theme: QuickAccessTheme) -> Self {
        let t = &theme;
        Self {
            theme,
            bar_style: theme_bar(t),
            leading_style: theme_leading(t),
            action_button_style: theme_action_button(t),
            action_icon_style: theme_action_icon(t),
            menu_arrow_style: theme_menu_arrow(t),
            title_style: theme_title(t),
            window_button_style: theme_window_button(t),
            window_icon_style: theme_window_icon(t),
            close_button_style: theme_close_button(t),
        }
    }
}

impl Default for QuickAccessStyle {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Data model --

/// One icon action on the band (quick-access or trailing).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct QuickAccessAction {
    /// Icon name, resolved through the registered icon provider.
    pub icon: AzString,
    /// Optional click handler; without one the action is inert.
    pub on_click: OptionButtonOnClick,
}

impl QuickAccessAction {
    /// Creates an inert action with the given icon name.
    #[must_use]
    pub fn new(icon: AzString) -> Self {
        Self {
            icon,
            on_click: None.into(),
        }
    }

    /// Sets the click callback.
    pub fn set_on_click<C: Into<super::button::ButtonOnClickCallback>>(
        &mut self,
        data: RefAny,
        on_click: C,
    ) {
        self.on_click = Some(super::button::ButtonOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    /// Builder method: sets the click callback and returns `self`.
    #[must_use]
    pub fn with_on_click<C: Into<super::button::ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(data, on_click);
        self
    }
}

impl_option!(
    QuickAccessAction,
    OptionQuickAccessAction,
    copy = false,
    [Debug, Clone]
);
impl_vec!(
    QuickAccessAction,
    QuickAccessActionVec,
    QuickAccessActionVecDestructor,
    QuickAccessActionVecDestructorType,
    QuickAccessActionVecSlice,
    OptionQuickAccessAction
);
impl_vec_clone!(
    QuickAccessAction,
    QuickAccessActionVec,
    QuickAccessActionVecDestructor
);
impl_vec_debug!(QuickAccessAction, QuickAccessActionVec);
impl_vec_mut!(QuickAccessAction, QuickAccessActionVec);

/// Top-level title band: leading slot, quick-access actions, customize
/// arrow, centered title, trailing actions and the window buttons.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct QuickAccessBar {

    /// All part styles (defaults to the the Office-2013-era look look).
    pub style: QuickAccessStyle,

    /// Optional leading content (office-2013: the app logo square).
    pub leading: azul_core::dom::OptionDom,

    /// Optional minimize handler.
    pub on_minimize: OptionButtonOnClick,

    /// Optional maximize/restore handler.
    pub on_maximize: OptionButtonOnClick,

    /// Optional close handler.
    pub on_close: OptionButtonOnClick,

    /// The centered window title ("Document1 - `AzWriter`").
    pub title: AzString,

    /// Quick-access actions (office-2013: save / undo / redo).
    pub actions: QuickAccessActionVec,

    /// Actions between the title and the window buttons (office-2013: help,
    /// ribbon display options).
    pub trailing_actions: QuickAccessActionVec,

    /// Renders the "customize quick access toolbar" chevron.
    pub show_menu_arrow: bool,

    /// Renders the minimize window button.
    pub show_minimize: bool,

    /// Renders the maximize/restore window button.
    pub show_maximize: bool,

    /// Renders the close window button.
    pub show_close: bool,
}

// -- CSS classes --

static CLS_QAB: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-quick-access",
))];
static CLS_LEADING: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-quick-access-leading",
))];
static CLS_TITLE: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-quick-access-title",
))];

// -- Constructors / builders --

impl QuickAccessBar {
    /// Creates a band with the given title, no actions and all three
    /// window buttons, in the the Office-2013-era look style.
    #[must_use]
    pub fn new(title: AzString) -> Self {
        Self {
            leading: None.into(),
            actions: QuickAccessActionVec::from_vec(Vec::new()),
            show_menu_arrow: false,
            title,
            trailing_actions: QuickAccessActionVec::from_vec(Vec::new()),
            show_minimize: true,
            show_maximize: true,
            show_close: true,
            on_minimize: None.into(),
            on_maximize: None.into(),
            on_close: None.into(),
            style: QuickAccessStyle::office_2013(),
        }
    }

    /// The the Office-2013-era look band: save / undo / redo quick-access actions (inert
    /// until callbacks are set), the customize chevron, and help before the
    /// window buttons.
    #[must_use]
    pub fn office_2013(title: AzString) -> Self {
        let mut band = Self::new(title);
        band.actions = QuickAccessActionVec::from_vec(vec![
            QuickAccessAction::new(AzString::from_const_str("save")),
            QuickAccessAction::new(AzString::from_const_str("undo")),
            QuickAccessAction::new(AzString::from_const_str("redo")),
        ]);
        band.show_menu_arrow = true;
        band.trailing_actions = QuickAccessActionVec::from_vec(vec![QuickAccessAction::new(
            AzString::from_const_str("help_outline"),
        )]);
        band
    }

    /// Builder method: sets the leading content.
    #[must_use]
    pub fn with_leading(mut self, leading: Dom) -> Self {
        self.leading = Some(leading).into();
        self
    }

    /// Builder method: replaces the quick-access actions.
    #[must_use]
    pub fn with_actions(mut self, actions: QuickAccessActionVec) -> Self {
        self.actions = actions;
        self
    }

    /// Builder method: replaces the trailing actions.
    #[must_use]
    pub fn with_trailing_actions(mut self, trailing_actions: QuickAccessActionVec) -> Self {
        self.trailing_actions = trailing_actions;
        self
    }

    /// Builder method: replaces the style bundle.
    #[must_use]
    pub fn with_style(mut self, style: QuickAccessStyle) -> Self {
        self.style = style;
        self
    }

    /// Renders the band.
    #[must_use]
    pub fn dom(self) -> Dom {
        let Self {
            leading,
            actions,
            show_menu_arrow,
            title,
            trailing_actions,
            show_minimize,
            show_maximize,
            show_close,
            on_minimize,
            on_maximize,
            on_close,
            style,
        } = self;

        let mut children: Vec<Dom> = Vec::with_capacity(actions.len() + 8);

        if let Some(lead) = leading.into_option() {
            children.push(
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_LEADING))
                    .with_css_props(style.leading_style.clone())
                    .with_children(DomVec::from_vec(vec![lead])),
            );
        }

        for action in actions.into_library_owned_vec() {
            children.push(action_button(action, &style.action_button_style, &style));
        }

        if show_menu_arrow {
            children.push(
                Dom::create_icon(AzString::from_const_str("arrow_drop_down"))
                    .with_css_props(style.menu_arrow_style.clone()),
            );
        }

        children.push(
            crate::widgets::widget_p_with_text(title)
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_TITLE))
                .with_css_props(style.title_style.clone()),
        );

        for action in trailing_actions.into_library_owned_vec() {
            children.push(action_button(action, &style.window_button_style, &style));
        }

        // The window controls come from the DESKTOP's icon theme first
        // (`system:` = the pack the shell fills from the user's icon theme -
        // Breeze, Adwaita, whatever is installed), with the bundled Material
        // glyph as the fallback for a desktop that provides none. A bar that
        // draws its own titlebar and then paints Material chevrons next to
        // the session's real windows is the one detail that gives client-side
        // decoration away; the same chain the CSD titlebar widget uses
        // (`super::titlebar`) keeps them identical.
        if show_minimize {
            children.push(window_button(
                AzString::from_const_str("system:window-minimize,minimize"),
                style.window_button_style.clone(),
                &style,
                on_minimize,
            ));
        }
        if show_maximize {
            children.push(window_button(
                AzString::from_const_str("system:window-maximize,crop_square"),
                style.window_button_style.clone(),
                &style,
                on_maximize,
            ));
        }
        if show_close {
            children.push(window_button(
                AzString::from_const_str("system:titlebar-close,system:window-close,close"),
                merged_style(&style.window_button_style, &style.close_button_style),
                &style,
                on_close,
            ));
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_QAB))
            .with_css_props(style.bar_style)
            .with_children(DomVec::from_vec(children))
    }
}

impl Default for QuickAccessBar {
    fn default() -> Self {
        Self::new(AzString::from_const_str(""))
    }
}

impl From<QuickAccessBar> for Dom {
    fn from(q: QuickAccessBar) -> Self {
        q.dom()
    }
}

// -- DOM builders --

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

/// Expands one action to the existing [`Button`] widget with the given
/// container style injected.
fn action_button(
    action: QuickAccessAction,
    container: &CssPropertyWithConditionsVec,
    style: &QuickAccessStyle,
) -> Dom {
    let mut b = Button::create(AzString::from_const_str(""));
    b.icon = action.icon;
    b.container_style = container.clone();
    b.icon_style = style.action_icon_style.clone();
    b.on_click = action.on_click;
    b.dom()
}

fn window_button(
    icon: AzString,
    container: CssPropertyWithConditionsVec,
    style: &QuickAccessStyle,
    on_click: OptionButtonOnClick,
) -> Dom {
    let mut b = Button::create(AzString::from_const_str(""));
    b.icon = icon;
    b.container_style = container;
    b.icon_style = style.window_icon_style.clone();
    b.on_click = on_click;
    b.dom()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Constructors and invariants
    // ------------------------------------------------------------------

    /// The window controls must ask the DESKTOP first and fall back to the
    /// bundled glyph - the same chain the CSD titlebar uses. Asserted as a
    /// CHAIN, not as a literal name: pinning one icon name is how this last
    /// broke, and an unresolved icon renders as an empty div rather than
    /// failing loudly.
    #[test]
    fn the_window_controls_ask_the_desktop_icon_theme_first() {
        let dom = QuickAccessBar::new(AzString::from("t")).dom();
        let icons: Vec<String> = collect_icon_names(&dom);
        for (system, fallback) in [
            ("system:window-minimize", "minimize"),
            ("system:window-maximize", "crop_square"),
            ("system:titlebar-close", "close"),
        ] {
            let chain = icons
                .iter()
                .find(|i| i.starts_with(system))
                .unwrap_or_else(|| panic!("no window button asks for {system}; got {icons:?}"));
            let mut parts = chain.split(',');
            assert_eq!(
                parts.next(),
                Some(system),
                "the desktop's icon must come FIRST in the chain"
            );
            assert!(
                parts.any(|p| p == fallback),
                "{chain} must fall back to the bundled {fallback} glyph"
            );
        }
    }

    /// Every icon name in a DOM tree, in document order.
    fn collect_icon_names(dom: &Dom) -> Vec<String> {
        fn walk(node: &Dom, out: &mut Vec<String>) {
            if let azul_core::dom::NodeType::Icon(icon) = node.root.get_node_type() {
                out.push(icon.as_str().to_string());
            }
            for child in node.children.as_ref() {
                walk(child, out);
            }
        }
        let mut out = Vec::new();
        walk(dom, &mut out);
        out
    }

    #[test]
    fn from_system_with_no_reported_colors_falls_back_to_office_2013() {
        // SystemStyle::default() may pre-fill platform colors; the fallback
        // contract is about a system that reports NO colors at all.
        let mut sys = azul_css::system::SystemStyle::default();
        sys.colors = azul_css::system::SystemColors::default();
        sys.metrics.titlebar = azul_css::system::TitlebarMetrics::default();
        assert_eq!(
            QuickAccessTheme::from_system(sys.clone()),
            QuickAccessTheme::office_2013()
        );
        assert_eq!(
            QuickAccessStyle::from_system(sys),
            QuickAccessStyle::office_2013()
        );
    }

    #[test]
    fn from_system_prefers_the_os_titlebar_colours_over_the_window_palette() {
        let header = ColorU {
            r: 61,
            g: 174,
            b: 233,
            a: 255,
        };
        let window_bg = ColorU {
            r: 239,
            g: 240,
            b: 241,
            a: 255,
        };
        let mut sys = azul_css::system::SystemStyle::default();
        sys.colors = azul_css::system::SystemColors::default();
        sys.metrics.titlebar = azul_css::system::TitlebarMetrics::default();
        sys.colors.window_background = Some(window_bg).into();

        // Window background only: the band takes it.
        assert_eq!(QuickAccessTheme::from_system(sys.clone()).bg, window_bg);

        // A reported HEADER colour outranks it — the band is the titlebar.
        sys.metrics.titlebar.background_active = Some(header).into();
        assert_eq!(QuickAccessTheme::from_system(sys).bg, header);
    }

    #[test]
    fn from_system_keeps_the_close_hover_out_of_the_ordinary_hover_fill() {
        let hover = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        };
        let mut sys = azul_css::system::SystemStyle::default();
        sys.colors = azul_css::system::SystemColors::default();
        sys.metrics.titlebar = azul_css::system::TitlebarMetrics::default();
        sys.metrics.titlebar.button_hover_background = Some(hover).into();

        let t = QuickAccessTheme::from_system(sys);
        assert_eq!(t.hover_bg, hover);
        assert_eq!(
            t.close_hover_bg,
            QuickAccessTheme::office_2013().close_hover_bg,
            "the close button keeps its own colour: a desktop that reports a \
             button hover but no close hover must NOT paint the close button \
             in the ordinary fill"
        );
    }

    #[test]
    fn quick_access_new_has_no_actions_and_all_window_buttons() {
        let q = QuickAccessBar::new(AzString::from("t"));
        assert_eq!(q.actions.len(), 0);
        assert_eq!(q.trailing_actions.len(), 0);
        assert!(!q.show_menu_arrow);
        assert!(q.show_minimize && q.show_maximize && q.show_close);
        assert_eq!(q.style, QuickAccessStyle::office_2013());
    }

    #[test]
    fn quick_access_office_2013_has_save_undo_redo_and_help() {
        let q = QuickAccessBar::office_2013(AzString::from("Document1 - AzWriter"));
        let icons: Vec<&str> = q
            .actions
            .as_slice()
            .iter()
            .map(|a| a.icon.as_str())
            .collect();
        assert_eq!(icons, ["save", "undo", "redo"]);
        assert!(q.show_menu_arrow);
        assert_eq!(q.trailing_actions.len(), 1);
    }

    #[test]
    fn quick_access_style_default_is_office_2013() {
        assert_eq!(QuickAccessStyle::default(), QuickAccessStyle::office_2013());
    }

    // ------------------------------------------------------------------
    // DOM shape
    // ------------------------------------------------------------------

    #[test]
    fn dom_renders_actions_arrow_title_trailing_and_window_buttons_in_order() {
        let dom = QuickAccessBar::office_2013(AzString::from("t")).dom();
        // 3 actions + arrow + title + 1 trailing + min + max + close
        assert_eq!(dom.children.as_ref().len(), 9);
    }

    #[test]
    fn dom_without_window_buttons_renders_title_only() {
        let mut q = QuickAccessBar::new(AzString::from("t"));
        q.show_minimize = false;
        q.show_maximize = false;
        q.show_close = false;
        let dom = q.dom();
        assert_eq!(dom.children.as_ref().len(), 1);
    }

    #[test]
    fn dom_wraps_the_leading_slot_first() {
        let q = QuickAccessBar::new(AzString::from("t")).with_leading(Dom::create_div());
        let dom = q.dom();
        let first = &dom.children.as_ref()[0];
        let classes = first.root.get_ids_and_classes();
        assert!(classes.as_ref().iter().any(|c| match c {
            Class(s) => s.as_str().contains("quick-access-leading"),
            IdOrClass::Id(_) => false,
        }));
    }
}

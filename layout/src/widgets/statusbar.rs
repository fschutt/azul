//! Microsoft Office-style status bar widget (the Office-2013-era look look by default).
//!
//! Models the bottom chrome band of an Office document window:
//!
//! ```text
//! StatusBar ─ segments (left)             StatusBarSegment (icon? + label, clickable?)
//!           ─ filler
//!           ─ view switcher               StatusBarViewSwitcher (icon buttons, one active)
//!           ─ zoom cluster                StatusBarZoom (− button, slider, + button, "100%")
//! ```
//!
//! the Office-2013-era look reference (bottom of the window):
//! `PAGE 1 OF 1   0 WORDS   [spellcheck]  ENGLISH (UNITED STATES) …
//!  [read mode] [print layout] [web layout]   − ──────╂────── +  100%`
//!
//! Buttons are not re-implemented: the view-switcher buttons and the zoom
//! −/+ buttons expand to the existing [`super::button::Button`] widget with
//! status-bar part styles injected through `Button`'s public style fields
//! (the same composition rule the ribbon uses). The zoom slider embeds the
//! existing [`super::slider::Slider`] widget, restyled through its public
//! `track_style` / `thumb_style` fields, so dragging it reports values
//! through the regular [`super::slider::SliderOnValueChange`] callback.
//!
//! All visual parts are exposed on [`StatusBarStyle`] (defaults = the Office-2013-era look
//! look, [`StatusBarStyle::office_2013`]); replace any field to re-theme
//! without touching widget code. There is no behavior struct: the status bar
//! has no self-driven chrome interactions — every event is forwarded to the
//! application callbacks.

use azul_core::{
    callbacks::Update,
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

use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut};

use crate::callbacks::CallbackInfo;

use super::{
    button::{Button, OptionButtonOnClick},
    slider::{OptionSliderOnValueChange, Slider},
};

// -- Callbacks --

/// Callback signature invoked when a view-switcher button is clicked
/// (receives the view index).
pub type StatusBarOnViewSelectCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    StatusBarOnViewSelect,
    OptionStatusBarOnViewSelect,
    StatusBarOnViewSelectCallback,
    StatusBarOnViewSelectCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        StatusBarOnViewSelectCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: STATUS_BAR_ON_VIEW_SELECT_INVOKER,
    invoker_ty:     AzStatusBarOnViewSelectCallbackInvoker,
    thunk_fn:       az_status_bar_on_view_select_callback_thunk,
    setter_fn:      AzApp_setStatusBarOnViewSelectCallbackInvoker,
    from_handle_fn: AzStatusBarOnViewSelectCallback_createFromHostHandle,
    extra_args:     [ view_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- the Office-2013-era look palette (seeds StatusBarTheme::office_2013) --

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
/// Office 2013 accent blue (#2B579A): the bar fill.
const W13_BLUE: ColorU = ColorU {
    r: 43,
    g: 87,
    b: 154,
    a: 255,
};
/// Hover fill on bar controls (lighter blue, #3E6DB5).
const W13_BAR_HOVER: ColorU = ColorU {
    r: 62,
    g: 109,
    b: 181,
    a: 255,
};
/// Pressed / active-view fill (darker blue, #1E3E6F).
const W13_BAR_PRESSED: ColorU = ColorU {
    r: 30,
    g: 62,
    b: 111,
    a: 255,
};
/// Zoom slider rail (semi-light blue-gray, #A5BDDE).
const W13_RAIL: ColorU = ColorU {
    r: 165,
    g: 189,
    b: 222,
    a: 255,
};
/// Zoom slider thumb border (#8E9BB3).
const W13_THUMB_BORDER: ColorU = ColorU {
    r: 142,
    g: 155,
    b: 179,
    a: 255,
};

// -- Metrics (the Office-2013-era look, logical px) --

/// Bar height.
const BAR_HEIGHT: isize = 23;
/// Status text size.
const TEXT_PX: isize = 11;
/// View-switcher / zoom glyph size.
const ICON_PX: isize = 15;
/// Width of one view-switcher button.
const VIEW_BUTTON_W: isize = 29;
/// Width of the zoom −/+ buttons.
const ZOOM_BUTTON_W: isize = 21;
/// Total width of the zoom slider track.
const ZOOM_TRACK_W: isize = 100;
/// Zoom thumb size (office-2013: a small vertical bar).
const ZOOM_THUMB_W: isize = 6;
const ZOOM_THUMB_H: isize = 11;
/// Width reserved for the "100%" label.
const ZOOM_LABEL_W: isize = 42;

// -- Theme --

/// Color palette from which a full [`StatusBarStyle`] is derived via
/// [`StatusBarStyle::from_theme`]. All fields are plain colors, so themes
/// are trivially constructible over FFI. Preset: [`StatusBarTheme::office_2013`]
/// (the default).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct StatusBarTheme {
    /// Bar fill (office-2013: accent blue).
    pub bar_bg: ColorU,
    /// Text and glyph color on the bar.
    pub text: ColorU,
    /// Hover fill on clickable bar controls.
    pub hover_bg: ColorU,
    /// Pressed fill on bar controls.
    pub pressed_bg: ColorU,
    /// Fill of the active view-switcher button.
    pub view_active_bg: ColorU,
    /// Zoom slider rail line.
    pub rail: ColorU,
    /// Zoom slider thumb fill.
    pub thumb: ColorU,
    /// Zoom slider thumb border.
    pub thumb_border: ColorU,
}

impl StatusBarTheme {
    /// The the Office-2013-era look palette: #2B579A bar, white text, lighter-blue hovers.
    #[must_use]
    pub const fn office_2013() -> Self {
        Self {
            bar_bg: W13_BLUE,
            text: WHITE,
            hover_bg: W13_BAR_HOVER,
            pressed_bg: W13_BAR_PRESSED,
            view_active_bg: W13_BAR_PRESSED,
            rail: W13_RAIL,
            thumb: WHITE,
            thumb_border: W13_THUMB_BORDER,
        }
    }
}

impl Default for StatusBarTheme {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Theme -> property-list builders --
//
// Every themed status-bar part is built from `StatusBarTheme` colors by the
// functions below; `StatusBarStyle::office_2013()` is just
// `from_theme(StatusBarTheme::office_2013())`, so there is exactly one source
// of truth for each part's property list.

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

/// Transparent, hover-highlighted flat button chassis shared by every
/// clickable status-bar control. The explicit TRANSPARENT border overrides
/// the [`Button`] widget's default frame (the bar's controls are flat).
fn push_flat_button(v: &mut Vec<Cond>, t: &StatusBarTheme) {
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

fn theme_bar(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    v.push(cond_border_box());
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT,
    ))));
    v.push(Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)));
    v.push(Cond::simple(P::const_font_size(StyleFontSize::const_px(
        TEXT_PX,
    ))));
    v.push(cond_bg(t.bar_bg));
    v.push(cond_text_color(t.text));
    v.push(Cond::simple(P::const_padding_left(
        LayoutPaddingLeft::const_px(6),
    )));
    v.push(Cond::simple(P::const_padding_right(
        LayoutPaddingRight::const_px(4),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_segment(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    push_flat_button(&mut v, t);
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT,
    ))));
    v.push(Cond::simple(P::const_padding_left(
        LayoutPaddingLeft::const_px(7),
    )));
    v.push(Cond::simple(P::const_padding_right(
        LayoutPaddingRight::const_px(7),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_segment_icon(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(ICON_PX - 1))),
        cond_text_color(t.text),
    ])
}

fn theme_segment_label(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(TEXT_PX))),
        cond_text_color(t.text),
    ])
}

fn theme_filler(_t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![Cond::simple(P::const_flex_grow(
        LayoutFlexGrow::const_new(1),
    ))])
}

fn theme_views(_t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    v.push(Cond::simple(P::const_margin_right(
        LayoutMarginRight::const_px(6),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_view_button(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    push_flat_button(&mut v, t);
    v.push(Cond::simple(P::const_justify_content(
        LayoutJustifyContent::Center,
    )));
    v.push(Cond::simple(P::const_width(LayoutWidth::const_px(
        VIEW_BUTTON_W,
    ))));
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT,
    ))));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// APPENDED to the active view-switcher button.
fn theme_view_button_active(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![cond_bg(t.view_active_bg)])
}

fn theme_view_icon(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(ICON_PX))),
        cond_text_color(t.text),
    ])
}

fn theme_zoom(_t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_zoom_button(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    push_flat_button(&mut v, t);
    v.push(Cond::simple(P::const_justify_content(
        LayoutJustifyContent::Center,
    )));
    v.push(Cond::simple(P::const_width(LayoutWidth::const_px(
        ZOOM_BUTTON_W,
    ))));
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT,
    ))));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_zoom_icon(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(ICON_PX - 2))),
        cond_text_color(t.text),
    ])
}

/// Positioning context for the rail line, the center tick and the slider.
fn theme_zoom_track_host(_t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_position(LayoutPosition::Relative)),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_width(LayoutWidth::const_px(ZOOM_TRACK_W))),
        Cond::simple(P::const_height(LayoutHeight::const_px(BAR_HEIGHT))),
    ])
}

/// The 1px horizontal rail line behind the slider.
fn theme_zoom_rail(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_position(LayoutPosition::Absolute)),
        Cond::simple(P::const_left(LayoutLeft::const_px(0))),
        Cond::simple(P::const_top(LayoutTop::const_px(BAR_HEIGHT / 2))),
        Cond::simple(P::const_width(LayoutWidth::const_px(ZOOM_TRACK_W))),
        Cond::simple(P::const_height(LayoutHeight::const_px(1))),
        cond_bg(t.rail),
    ])
}

/// The small vertical tick marking the 100% center of the rail.
fn theme_zoom_tick(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_position(LayoutPosition::Absolute)),
        Cond::simple(P::const_left(LayoutLeft::const_px(ZOOM_TRACK_W / 2))),
        Cond::simple(P::const_top(LayoutTop::const_px(BAR_HEIGHT / 2 - 3))),
        Cond::simple(P::const_width(LayoutWidth::const_px(1))),
        Cond::simple(P::const_height(LayoutHeight::const_px(7))),
        cond_bg(t.rail),
    ])
}

/// Injected into the embedded [`Slider`]'s `track_style`: a transparent
/// full-height hit area (the visible rail is drawn by the host).
fn theme_slider_track(_t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_width(LayoutWidth::const_px(ZOOM_TRACK_W))),
        Cond::simple(P::const_height(LayoutHeight::const_px(BAR_HEIGHT))),
        cond_bg(TRANSPARENT),
        Cond::simple(P::const_cursor(StyleCursor::Default)),
    ])
}

/// Injected into the embedded [`Slider`]'s `thumb_style` (position-independent
/// part; `StatusBar::dom` appends the `margin-left` computed from the zoom
/// percent).
fn theme_slider_thumb(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_width(LayoutWidth::const_px(ZOOM_THUMB_W))),
        Cond::simple(P::const_height(LayoutHeight::const_px(ZOOM_THUMB_H))),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        cond_bg(t.thumb),
    ];
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
        StyleBorderTopColor {
            inner: t.thumb_border,
        },
    )));
    v.push(Cond::simple(P::const_border_left_color(
        StyleBorderLeftColor {
            inner: t.thumb_border,
        },
    )));
    v.push(Cond::simple(P::const_border_right_color(
        StyleBorderRightColor {
            inner: t.thumb_border,
        },
    )));
    v.push(Cond::simple(P::const_border_bottom_color(
        StyleBorderBottomColor {
            inner: t.thumb_border,
        },
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_zoom_label(t: &StatusBarTheme) -> CssPropertyWithConditionsVec {
    let mut v = Vec::new();
    push_row_center(&mut v);
    push_flat_button(&mut v, t);
    v.push(Cond::simple(P::const_justify_content(
        LayoutJustifyContent::End,
    )));
    v.push(Cond::simple(P::const_width(LayoutWidth::const_px(
        ZOOM_LABEL_W,
    ))));
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(
        BAR_HEIGHT,
    ))));
    v.push(Cond::simple(P::const_padding_right(
        LayoutPaddingRight::const_px(6),
    )));
    v.push(Cond::simple(P::const_font_size(StyleFontSize::const_px(
        TEXT_PX,
    ))));
    v.push(cond_text_color(t.text));
    CssPropertyWithConditionsVec::from_vec(v)
}

// -- Style --

/// All part styles of the status bar. Every part defaults to the the Office-2013-era look
/// look; replace any field for finer control (the same override API as
/// [`super::ribbon::RibbonStyle`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct StatusBarStyle {
    /// The palette this style bundle was derived from. Kept for consumers
    /// deriving matching custom parts.
    pub theme: StatusBarTheme,
    /// The bar itself (horizontal row).
    pub bar_style: CssPropertyWithConditionsVec,
    /// One left-hand segment (page count, word count, language, …).
    pub segment_style: CssPropertyWithConditionsVec,
    /// Icon inside a segment.
    pub segment_icon_style: CssPropertyWithConditionsVec,
    /// Text label inside a segment.
    pub segment_label_style: CssPropertyWithConditionsVec,
    /// Flexible spacer between the left segments and the right clusters.
    pub filler_style: CssPropertyWithConditionsVec,
    /// The view-switcher cluster.
    pub views_style: CssPropertyWithConditionsVec,
    /// Container style injected into one view-switcher [`Button`].
    pub view_button_style: CssPropertyWithConditionsVec,
    /// APPENDED to the active view-switcher button.
    pub view_button_active_style: CssPropertyWithConditionsVec,
    /// Icon style injected into the view-switcher [`Button`]s.
    pub view_icon_style: CssPropertyWithConditionsVec,
    /// The zoom cluster (− slider + label).
    pub zoom_style: CssPropertyWithConditionsVec,
    /// Container style injected into the zoom −/+ [`Button`]s.
    pub zoom_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into the zoom −/+ [`Button`]s.
    pub zoom_icon_style: CssPropertyWithConditionsVec,
    /// Positioning host around the rail, tick and embedded slider.
    pub zoom_track_host_style: CssPropertyWithConditionsVec,
    /// The 1px rail line.
    pub zoom_rail_style: CssPropertyWithConditionsVec,
    /// The center (100%) tick on the rail.
    pub zoom_tick_style: CssPropertyWithConditionsVec,
    /// Track style injected into the embedded [`Slider`].
    pub slider_track_style: CssPropertyWithConditionsVec,
    /// Thumb style injected into the embedded [`Slider`] (without the
    /// position; `dom()` appends the computed `margin-left`).
    pub slider_thumb_style: CssPropertyWithConditionsVec,
    /// The "100%" zoom percent label.
    pub zoom_label_style: CssPropertyWithConditionsVec,
}

impl StatusBarStyle {
    /// The the Office-2013-era look look (#2B579A bar, white text) - the default.
    #[must_use]
    pub fn office_2013() -> Self {
        Self::from_theme(StatusBarTheme::office_2013())
    }

    /// Derives every part style from the given palette.
    #[must_use]
    pub fn from_theme(theme: StatusBarTheme) -> Self {
        let t = &theme;
        Self {
            theme,
            bar_style: theme_bar(t),
            segment_style: theme_segment(t),
            segment_icon_style: theme_segment_icon(t),
            segment_label_style: theme_segment_label(t),
            filler_style: theme_filler(t),
            views_style: theme_views(t),
            view_button_style: theme_view_button(t),
            view_button_active_style: theme_view_button_active(t),
            view_icon_style: theme_view_icon(t),
            zoom_style: theme_zoom(t),
            zoom_button_style: theme_zoom_button(t),
            zoom_icon_style: theme_zoom_icon(t),
            zoom_track_host_style: theme_zoom_track_host(t),
            zoom_rail_style: theme_zoom_rail(t),
            zoom_tick_style: theme_zoom_tick(t),
            slider_track_style: theme_slider_track(t),
            slider_thumb_style: theme_slider_thumb(t),
            zoom_label_style: theme_zoom_label(t),
        }
    }
}

impl Default for StatusBarStyle {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Data model --

/// One left-hand status segment ("PAGE 1 OF 1", "0 WORDS", the language, …).
///
/// An empty `icon` means "no icon". With an `on_click` the segment renders
/// as a flat hover-highlighted button (office-2013: the segments open panes /
/// toggle counters); without one it is inert text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct StatusBarSegment {
    /// Optional leading icon name, resolved through the registered icon
    /// provider (e.g. the builtin Material Icons pack: "spellcheck").
    pub icon: AzString,
    /// The segment text.
    pub label: AzString,
    /// Optional click handler.
    pub on_click: OptionButtonOnClick,
}

impl StatusBarSegment {
    /// Creates a text-only, inert segment.
    #[must_use]
    pub fn new(label: AzString) -> Self {
        Self {
            icon: AzString::from_const_str(""),
            label,
            on_click: None.into(),
        }
    }

    /// Builder method: sets the leading icon name.
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.icon = icon;
        self
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
    StatusBarSegment,
    OptionStatusBarSegment,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    StatusBarSegment,
    StatusBarSegmentVec,
    StatusBarSegmentVecDestructor,
    StatusBarSegmentVecDestructorType,
    StatusBarSegmentVecSlice,
    OptionStatusBarSegment
);
impl_vec_clone!(
    StatusBarSegment,
    StatusBarSegmentVec,
    StatusBarSegmentVecDestructor
);
impl_vec_debug!(StatusBarSegment, StatusBarSegmentVec);
impl_vec_mut!(StatusBarSegment, StatusBarSegmentVec);

/// One view-switcher button (office-2013: read mode / print layout / web layout).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct StatusBarView {
    /// Icon name, resolved through the registered icon provider.
    pub icon: AzString,
}

impl StatusBarView {
    /// Creates a view button with the given icon name.
    #[must_use]
    pub const fn new(icon: AzString) -> Self {
        Self { icon }
    }
}

impl_option!(
    StatusBarView,
    OptionStatusBarView,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    StatusBarView,
    StatusBarViewVec,
    StatusBarViewVecDestructor,
    StatusBarViewVecDestructorType,
    StatusBarViewVecSlice,
    OptionStatusBarView
);
impl_vec_clone!(StatusBarView, StatusBarViewVec, StatusBarViewVecDestructor);
impl_vec_debug!(StatusBarView, StatusBarViewVec);
impl_vec_mut!(StatusBarView, StatusBarViewVec);

/// The view-switcher cluster: a row of icon buttons with one active view.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct StatusBarViewSwitcher {
    /// The view buttons, left to right.
    pub views: StatusBarViewVec,
    /// Index of the active view (highlighted).
    pub active_view: usize,
    /// Optional callback fired when a view is clicked (receives the index).
    pub on_select: OptionStatusBarOnViewSelect,
}

impl StatusBarViewSwitcher {
    /// Creates a view switcher with the given views; view 0 is active.
    #[must_use]
    pub fn new(views: StatusBarViewVec) -> Self {
        Self {
            views,
            active_view: 0,
            on_select: None.into(),
        }
    }

    /// The the Office-2013-era look trio: read mode, print layout (active), web layout.
    #[must_use]
    pub fn office_2013() -> Self {
        let views = StatusBarViewVec::from_vec(vec![
            StatusBarView::new(AzString::from_const_str("menu_book")),
            StatusBarView::new(AzString::from_const_str("description")),
            StatusBarView::new(AzString::from_const_str("public")),
        ]);
        Self {
            views,
            active_view: 1,
            on_select: None.into(),
        }
    }

    /// Builder method: sets the active view index.
    #[must_use]
    pub const fn with_active_view(mut self, active_view: usize) -> Self {
        self.active_view = active_view;
        self
    }

    /// Sets the view-select callback.
    pub fn set_on_select<C: Into<StatusBarOnViewSelectCallback>>(
        &mut self,
        data: RefAny,
        on_select: C,
    ) {
        self.on_select = Some(StatusBarOnViewSelect {
            refany: data,
            callback: on_select.into(),
        })
        .into();
    }

    /// Builder method: sets the view-select callback and returns `self`.
    #[must_use]
    pub fn with_on_select<C: Into<StatusBarOnViewSelectCallback>>(
        mut self,
        data: RefAny,
        on_select: C,
    ) -> Self {
        self.set_on_select(data, on_select);
        self
    }
}

impl_option!(
    StatusBarViewSwitcher,
    OptionStatusBarViewSwitcher,
    copy = false,
    [Debug, Clone]
);

/// The zoom cluster: − button, slider, + button and the percent label.
///
/// The slider is the existing [`Slider`] widget with `value = percent` over
/// the linear `[min, max]` window. The the Office-2013-era look default window is
/// `[10, 190]` so the 100% default rests exactly on the center tick (the
/// original office-suite slider maps 10–100–500 piecewise; a linear window keeps the
/// widget dumb — the application decides what a slider value means).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct StatusBarZoom {
    /// Current zoom percent, shown in the label and placing the thumb.
    pub percent: f32,
    /// Slider window minimum (thumb far left).
    pub min: f32,
    /// Slider window maximum (thumb far right).
    pub max: f32,
    /// Optional − button callback.
    pub on_zoom_out: OptionButtonOnClick,
    /// Optional + button callback.
    pub on_zoom_in: OptionButtonOnClick,
    /// Optional slider drag/press callback (reports the raw slider value).
    pub on_slider_change: OptionSliderOnValueChange,
    /// Renders the "100%" label after the + button.
    pub show_label: bool,
}

impl StatusBarZoom {
    /// 100% zoom over the the Office-2013-era look `[10, 190]` window, label shown.
    #[must_use]
    pub fn office_2013() -> Self {
        Self {
            percent: 100.0,
            min: 10.0,
            max: 190.0,
            on_zoom_out: None.into(),
            on_zoom_in: None.into(),
            on_slider_change: None.into(),
            show_label: true,
        }
    }

    /// Builder method: sets the zoom percent.
    #[must_use]
    pub const fn with_percent(mut self, percent: f32) -> Self {
        self.percent = percent;
        self
    }
}

impl Default for StatusBarZoom {
    fn default() -> Self {
        Self::office_2013()
    }
}

impl_option!(
    StatusBarZoom,
    OptionStatusBarZoom,
    copy = false,
    [Debug, Clone, PartialEq]
);

/// Top-level status bar widget: left segments, a filler, then the
/// view switcher and zoom cluster on the right.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct StatusBar {
    /// Left-hand segments, in order.
    pub segments: StatusBarSegmentVec,
    /// Optional view-switcher cluster.
    pub views: OptionStatusBarViewSwitcher,
    /// Optional zoom cluster.
    pub zoom: OptionStatusBarZoom,
    /// All part styles (defaults to the the Office-2013-era look look).
    pub style: StatusBarStyle,
}

// -- CSS classes --

static CLS_STATUSBAR: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-statusbar"))];
static CLS_SEGMENT: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-segment",
))];
static CLS_FILLER: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-filler",
))];
static CLS_VIEWS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-views",
))];
static CLS_ZOOM: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-zoom",
))];
static CLS_ZOOM_TRACK: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-zoom-track",
))];
static CLS_ZOOM_RAIL: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-zoom-rail",
))];
static CLS_ZOOM_TICK: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-zoom-tick",
))];
static CLS_ZOOM_LABEL: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-statusbar-zoom-label",
))];

// -- Constructors / builders --

impl StatusBar {
    /// Creates a status bar with the given left segments, no view switcher
    /// and no zoom cluster, in the the Office-2013-era look style.
    #[must_use]
    pub fn new(segments: StatusBarSegmentVec) -> Self {
        Self {
            segments,
            views: None.into(),
            zoom: None.into(),
            style: StatusBarStyle::office_2013(),
        }
    }

    /// Sets the view-switcher cluster.
    pub fn set_views(&mut self, views: StatusBarViewSwitcher) {
        self.views = Some(views).into();
    }

    /// Builder method: sets the view-switcher cluster and returns `self`.
    #[must_use]
    pub fn with_views(mut self, views: StatusBarViewSwitcher) -> Self {
        self.set_views(views);
        self
    }

    /// Sets the zoom cluster.
    pub fn set_zoom(&mut self, zoom: StatusBarZoom) {
        self.zoom = Some(zoom).into();
    }

    /// Builder method: sets the zoom cluster and returns `self`.
    #[must_use]
    pub fn with_zoom(mut self, zoom: StatusBarZoom) -> Self {
        self.set_zoom(zoom);
        self
    }

    /// Builder method: replaces the style bundle.
    #[must_use]
    pub fn with_style(mut self, style: StatusBarStyle) -> Self {
        self.style = style;
        self
    }

    /// Renders the status bar.
    #[must_use]
    pub fn dom(self) -> Dom {
        let Self {
            segments,
            views,
            zoom,
            style,
        } = self;
        let mut children: Vec<Dom> = Vec::with_capacity(segments.len() + 3);

        for seg in segments.into_library_owned_vec() {
            children.push(segment_dom(seg, &style));
        }

        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_FILLER))
                .with_css_props(style.filler_style.clone()),
        );

        if let Some(switcher) = views.into_option() {
            children.push(views_dom(switcher, &style));
        }

        if let Some(zoom) = zoom.into_option() {
            children.push(zoom_dom(zoom, &style));
        }

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_STATUSBAR))
            .with_css_props(style.bar_style)
            .with_children(DomVec::from_vec(children))
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new(StatusBarSegmentVec::from_vec(Vec::new()))
    }
}

impl From<StatusBar> for Dom {
    fn from(s: StatusBar) -> Self {
        s.dom()
    }
}

// -- DOM builders --

/// Expands widget config to the existing [`Button`] widget with the
/// status-bar part styles injected (the ribbon's composition rule).
fn styled_button(
    icon: AzString,
    container_style: CssPropertyWithConditionsVec,
    icon_style: CssPropertyWithConditionsVec,
    on_click: OptionButtonOnClick,
) -> Dom {
    let mut b = Button::create(AzString::from_const_str(""));
    b.icon = icon;
    b.container_style = container_style;
    b.icon_style = icon_style;
    b.on_click = on_click;
    b.dom()
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

fn segment_dom(seg: StatusBarSegment, style: &StatusBarStyle) -> Dom {
    let StatusBarSegment {
        icon,
        label,
        on_click,
    } = seg;
    if !icon.as_str().is_empty() || on_click.is_some() {
        // Icon and/or clickable: expand to a Button (flat chassis).
        let mut b = Button::create(label);
        b.icon = icon;
        b.container_style = style.segment_style.clone();
        b.icon_style = style.segment_icon_style.clone();
        b.label_style = style.segment_label_style.clone();
        b.on_click = on_click;
        return b
            .dom()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_SEGMENT));
    }
    // Inert text segment.
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_SEGMENT))
        .with_css_props(style.segment_style.clone())
        .with_children(DomVec::from_vec(vec![
            // `<p>` so the inert segment has the same `div > p > text` shape as
            // the clickable one (Button puts `label_style` on a `<p>` too).
            crate::widgets::widget_p_with_text(label)
                .with_css_props(style.segment_label_style.clone()),
        ]))
}

fn views_dom(switcher: StatusBarViewSwitcher, style: &StatusBarStyle) -> Dom {
    let StatusBarViewSwitcher {
        views,
        active_view,
        on_select,
    } = switcher;
    let mut children: Vec<Dom> = Vec::with_capacity(views.len());
    for (idx, view) in views.into_library_owned_vec().into_iter().enumerate() {
        let container = if idx == active_view {
            merged_style(&style.view_button_style, &style.view_button_active_style)
        } else {
            style.view_button_style.clone()
        };
        let on_click: OptionButtonOnClick = match on_select.as_ref() {
            Some(cb) => Some(super::button::ButtonOnClick {
                refany: RefAny::new(ViewClickData {
                    view_idx: idx,
                    on_select: cb.clone(),
                }),
                callback: super::button::ButtonOnClickCallback {
                    cb: on_status_bar_view_click,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            })
            .into(),
            None => None.into(),
        };
        children.push(styled_button(
            view.icon,
            container,
            style.view_icon_style.clone(),
            on_click,
        ));
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_VIEWS))
        .with_css_props(style.views_style.clone())
        .with_children(DomVec::from_vec(children))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // bounded layout numeric cast
fn zoom_dom(zoom: StatusBarZoom, style: &StatusBarStyle) -> Dom {
    let StatusBarZoom {
        percent,
        min,
        max,
        on_zoom_out,
        on_zoom_in,
        on_slider_change,
        show_label,
    } = zoom;

    let mut children: Vec<Dom> = Vec::with_capacity(4);

    children.push(styled_button(
        AzString::from_const_str("remove"),
        style.zoom_button_style.clone(),
        style.zoom_icon_style.clone(),
        on_zoom_out,
    ));

    // Rail + tick + embedded Slider, layered inside the positioning host.
    let fraction = if max > min {
        ((percent - min) / (max - min)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let margin = (fraction * (ZOOM_TRACK_W - ZOOM_THUMB_W) as f32).round() as isize;
    let thumb_style = merged_style(
        &style.slider_thumb_style,
        &CssPropertyWithConditionsVec::from_vec(vec![Cond::simple(P::const_margin_left(
            LayoutMarginLeft::const_px(margin),
        ))]),
    );
    let mut slider = Slider::create(percent, min, max);
    slider.track_style = style.slider_track_style.clone();
    slider.thumb_style = thumb_style;
    slider.slider_state.on_value_change = on_slider_change;

    children.push(
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_ZOOM_TRACK))
            .with_css_props(style.zoom_track_host_style.clone())
            .with_children(DomVec::from_vec(vec![
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_ZOOM_RAIL))
                    .with_css_props(style.zoom_rail_style.clone()),
                Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_ZOOM_TICK))
                    .with_css_props(style.zoom_tick_style.clone()),
                slider.dom(),
            ])),
    );

    children.push(styled_button(
        AzString::from_const_str("add"),
        style.zoom_button_style.clone(),
        style.zoom_icon_style.clone(),
        on_zoom_in,
    ));

    if show_label {
        let label = format!("{}%", percent.round() as i64);
        children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_ZOOM_LABEL))
                .with_css_props(style.zoom_label_style.clone())
                .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                    AzString::from(label),
                )])),
        );
    }

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_ZOOM))
        .with_css_props(style.zoom_style.clone())
        .with_children(DomVec::from_vec(children))
}

// -- View-click plumbing --

/// Payload of one view-switcher button: the view index plus the user's
/// view-select callback.
struct ViewClickData {
    view_idx: usize,
    on_select: StatusBarOnViewSelect,
}

extern "C" fn on_status_bar_view_click(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(payload) = data.downcast_ref::<ViewClickData>() else {
        return Update::DoNothing;
    };
    let idx = payload.view_idx;
    let cb = payload.on_select.callback.cb;
    let refany = payload.on_select.refany.clone();
    drop(payload);
    (cb)(refany, info, idx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(label: &str) -> StatusBarSegment {
        StatusBarSegment::new(AzString::from(label))
    }

    fn segs(count: usize) -> StatusBarSegmentVec {
        StatusBarSegmentVec::from_vec((0..count).map(|i| seg(&format!("s{i}"))).collect())
    }

    // ------------------------------------------------------------------
    // Constructors and invariants
    // ------------------------------------------------------------------

    #[test]
    fn status_bar_new_defaults_to_office_2013_with_no_clusters() {
        for count in [0usize, 1, 3] {
            let s = StatusBar::new(segs(count));
            assert_eq!(s.segments.len(), count);
            assert!(s.views.is_none());
            assert!(s.zoom.is_none());
            assert_eq!(s.style, StatusBarStyle::office_2013());
        }
    }

    #[test]
    fn status_bar_style_default_is_office_2013() {
        assert_eq!(StatusBarStyle::default(), StatusBarStyle::office_2013());
    }

    #[test]
    fn zoom_office_2013_centers_the_default_percent() {
        let z = StatusBarZoom::office_2013();
        // 100% must rest exactly on the center tick of the [10, 190] window.
        let fraction = (z.percent - z.min) / (z.max - z.min);
        assert!((fraction - 0.5).abs() < 1e-6);
        assert!(z.show_label);
    }

    #[test]
    fn view_switcher_office_2013_has_three_views_with_print_layout_active() {
        let v = StatusBarViewSwitcher::office_2013();
        assert_eq!(v.views.len(), 3);
        assert_eq!(v.active_view, 1);
        assert!(v.on_select.is_none());
    }

    // ------------------------------------------------------------------
    // DOM shape
    // ------------------------------------------------------------------

    #[test]
    fn dom_renders_segments_filler_views_and_zoom_in_order() {
        let bar = StatusBar::new(segs(2))
            .with_views(StatusBarViewSwitcher::office_2013())
            .with_zoom(StatusBarZoom::office_2013());
        let dom = bar.dom();
        // 2 segments + filler + views + zoom
        assert_eq!(dom.children.as_ref().len(), 5);
    }

    #[test]
    fn dom_without_clusters_renders_segments_and_filler_only() {
        let dom = StatusBar::new(segs(3)).dom();
        assert_eq!(dom.children.as_ref().len(), 4);
    }

    #[test]
    fn zoom_cluster_without_label_has_three_children() {
        let mut zoom = StatusBarZoom::office_2013();
        zoom.show_label = false;
        let dom = StatusBar::new(segs(0)).with_zoom(zoom).dom();
        // filler + zoom
        assert_eq!(dom.children.as_ref().len(), 2);
        let zoom_dom = &dom.children.as_ref()[1];
        // − button, track host, + button (no label)
        assert_eq!(zoom_dom.children.as_ref().len(), 3);
    }

    #[test]
    fn zoom_track_host_layers_rail_tick_and_slider() {
        let dom = StatusBar::new(segs(0))
            .with_zoom(StatusBarZoom::office_2013())
            .dom();
        let zoom_dom = &dom.children.as_ref()[1];
        let track_host = &zoom_dom.children.as_ref()[1];
        assert_eq!(track_host.children.as_ref().len(), 3);
    }

    #[test]
    fn view_buttons_get_click_callbacks_only_with_a_select_handler() {
        extern "C" fn on_select(_: RefAny, _: CallbackInfo, _: usize) -> Update {
            Update::DoNothing
        }
        // Without a handler: no callbacks on the buttons.
        let dom = StatusBar::new(segs(0))
            .with_views(StatusBarViewSwitcher::office_2013())
            .dom();
        let views = &dom.children.as_ref()[1];
        for btn in views.children.as_ref() {
            assert!(btn.root.callbacks.as_ref().is_empty());
        }
        // With a handler: every button carries one.
        let mut switcher = StatusBarViewSwitcher::office_2013();
        switcher.set_on_select(
            RefAny::new(()),
            on_select as StatusBarOnViewSelectCallbackType,
        );
        let dom = StatusBar::new(segs(0)).with_views(switcher).dom();
        let views = &dom.children.as_ref()[1];
        for btn in views.children.as_ref() {
            assert_eq!(btn.root.callbacks.as_ref().len(), 1);
        }
    }
}

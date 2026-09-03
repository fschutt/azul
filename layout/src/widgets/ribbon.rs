//! Microsoft Office-style ribbon widget.
//!
//! Models the component hierarchy of the MS Ribbon (Office "Fluent" ribbon /
//! RibbonX `customUI` markup / Windows Ribbon Framework):
//!
//! ```text
//! Ribbon ─ app button ("FILE")            RibbonAppButton
//!        ─ tabs                           RibbonTab
//!            └─ groups                    RibbonGroup (label + dialog launcher)
//!                 └─ items                RibbonItem
//!                      ├─ LargeButton     RibbonButton (icon-over-label, full height)
//!                      ├─ SmallButton     RibbonButton (16px icon row)
//!                      ├─ Column / Row    RibbonColumn / RibbonRow (packing boxes)
//!                      ├─ Combo           embeds [`super::combobox::ComboBox`]
//!                      ├─ Drop            embeds [`super::drop_down::DropDown`]
//!                      ├─ Check           embeds [`super::check_box::CheckBox`]
//!                      ├─ Gallery         RibbonGallery (in-ribbon gallery + spinner)
//!                      ├─ Separator       thin vertical rule
//!                      └─ Custom          any user [`Dom`]
//! ```
//!
//! Mapping from RibbonX elements: `button[size=large]` → `LargeButton`,
//! `button`/`toggleButton` → `SmallButton` (+ `toggled`), `splitButton`/`menu`
//! → `RibbonArrow::Split`/`Menu`, `box`/`buttonGroup` → `Row`/`Column`,
//! `comboBox` → `Combo`, `dropDown` → `Drop`, `checkBox` → `Check`,
//! `gallery` → `Gallery`, `separator` → `Separator`,
//! `dialogBoxLauncher` → [`RibbonGroup::launcher`]. Contextual tabs, KeyTips,
//! the backstage view and automatic size collapsing are out of scope.
//!
//! Buttons are not re-implemented: every ribbon button (including the group
//! dialog launcher and the gallery spinner buttons) expands to the existing
//! [`super::button::Button`] widget with ribbon part styles injected through
//! `Button`'s public style fields. Embedded `Combo`/`Drop`/`Check` widgets
//! render exactly as configured — restyle them via their own public
//! `*_style` fields (see the ribbon example for an office-2013-style combobox).
//!
//! All visual parts of the ribbon itself are exposed on [`RibbonStyle`]
//! (defaults = the Office-2013-era look look, [`RibbonStyle::office_2013`]); replace any field
//! to re-theme without touching widget code.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class,
        IdOrClassVec,
    },
    refany::RefAny,
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions as Cond, CssPropertyWithConditionsVec, DynamicSelector,
        MinMaxRange,
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

use azul_css::system::{Handedness, SystemStyle};
use azul_css::{impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut};

use crate::callbacks::{Callback, CallbackInfo};

use super::{
    button::{Button, OptionButtonOnClick},
    check_box::CheckBox,
    combobox::ComboBox,
    drop_down::DropDown,
};

// -- Callbacks --

/// Callback signature invoked when a ribbon tab is clicked.
pub type RibbonOnTabClickCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    RibbonOnTabClick,
    OptionRibbonOnTabClick,
    RibbonOnTabClickCallback,
    RibbonOnTabClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RibbonOnTabClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RIBBON_ON_TAB_CLICK_INVOKER,
    invoker_ty:     AzRibbonOnTabClickCallbackInvoker,
    thunk_fn:       az_ribbon_on_tab_click_callback_thunk,
    setter_fn:      AzApp_setRibbonOnTabClickCallbackInvoker,
    from_handle_fn: AzRibbonOnTabClickCallback_createFromHostHandle,
    extra_args:     [ tab_index: usize ],
}

/// Callback signature invoked when a gallery cell is clicked (cell index).
pub type RibbonGalleryOnSelectCallbackType = extern "C" fn(RefAny, CallbackInfo, usize) -> Update;
impl_widget_callback!(
    RibbonGalleryOnSelect,
    OptionRibbonGalleryOnSelect,
    RibbonGalleryOnSelectCallback,
    RibbonGalleryOnSelectCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        RibbonGalleryOnSelectCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: RIBBON_GALLERY_ON_SELECT_INVOKER,
    invoker_ty:     AzRibbonGalleryOnSelectCallbackInvoker,
    thunk_fn:       az_ribbon_gallery_on_select_callback_thunk,
    setter_fn:      AzApp_setRibbonGalleryOnSelectCallbackInvoker,
    from_handle_fn: AzRibbonGalleryOnSelectCallback_createFromHostHandle,
    extra_args:     [ cell_index: usize ],
}

// -- Font --

const SYSTEM_UI_STR: AzString = AzString::from_const_str("system:ui");
const SYSTEM_UI_FAMILIES: &[StyleFontFamily] = &[StyleFontFamily::System(SYSTEM_UI_STR)];
const SYSTEM_UI_FAMILY: StyleFontFamilyVec =
    StyleFontFamilyVec::from_const_slice(SYSTEM_UI_FAMILIES);

// -- the Office-2013-era look palette (seeds RibbonTheme::office_2013) --

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
/// Office 2013 accent blue (#2B579A): FILE tab, active tab text.
const W13_BLUE: ColorU = ColorU {
    r: 43,
    g: 87,
    b: 154,
    a: 255,
};
/// FILE tab hover fill (darker blue).
const W13_BLUE_HOVER: ColorU = ColorU {
    r: 30,
    g: 62,
    b: 111,
    a: 255,
};
/// Regular control text (#444444).
const W13_TEXT: ColorU = ColorU {
    r: 68,
    g: 68,
    b: 68,
    a: 255,
};
/// Group caption + secondary glyph gray (#676767).
const W13_LABEL_GRAY: ColorU = ColorU {
    r: 103,
    g: 103,
    b: 103,
    a: 255,
};
/// Monochrome icon gray.
const W13_ICON_GRAY: ColorU = ColorU {
    r: 80,
    g: 80,
    b: 80,
    a: 255,
};
/// Chrome border gray (#D4D4D4): tab underline, ribbon bottom border.
const W13_BORDER: ColorU = ColorU {
    r: 212,
    g: 212,
    b: 212,
    a: 255,
};
/// Group/segment separator gray (#E1E1E1).
const W13_SEP: ColorU = ColorU {
    r: 225,
    g: 225,
    b: 225,
    a: 255,
};
/// Hover fill (#CDE6F7).
const W13_HOVER_BG: ColorU = ColorU {
    r: 205,
    g: 230,
    b: 247,
    a: 255,
};
/// Hover/checked border (#92C0E0).
const W13_HOVER_BORDER: ColorU = ColorU {
    r: 146,
    g: 192,
    b: 224,
    a: 255,
};
/// Pressed fill (#B0D0EC).
const W13_PRESSED_BG: ColorU = ColorU {
    r: 176,
    g: 208,
    b: 236,
    a: 255,
};
/// Toggled-on fill (#C6DDF0).
const W13_CHECKED_BG: ColorU = ColorU {
    r: 198,
    g: 221,
    b: 240,
    a: 255,
};
/// Selected gallery cell fill (#EAF3FC).
const W13_SELECTED_BG: ColorU = ColorU {
    r: 234,
    g: 243,
    b: 252,
    a: 255,
};
/// Flat editable-field border gray (#ABABAB).
const W13_FIELD_BORDER: ColorU = ColorU {
    r: 171,
    g: 171,
    b: 171,
    a: 255,
};

// -- Theme --

/// Color palette from which a full [`RibbonStyle`] is derived via
/// [`RibbonStyle::from_theme`]. All fields are plain colors, so themes are
/// trivially constructible over FFI. Presets: [`RibbonTheme::office_2013`]
/// (the default) and [`RibbonTheme::from_system`], which extracts the
/// colors from the OS theme (accent color, selection color, separators).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct RibbonTheme {
    /// Chrome background: the ribbon's own surfaces (root, tab bar, tabs) and
    /// the control fills that match them (gallery frame, combo field).
    pub chrome_bg: ColorU,
    /// The tab-content band behind the groups, separately from
    /// [`Self::chrome_bg`] so an app can paint the two differently: a window
    /// whose background is a gradient wants the chrome TRANSPARENT, so the
    /// gradient runs unbroken from the title bar down, and the content band as
    /// a translucent overlay on it rather than an opaque slab that cuts it.
    ///
    /// Defaults to `chrome_bg` in every constructor, so a theme that says
    /// nothing looks exactly as it did.
    pub content_bg: ColorU,
    /// Accent: application button fill, active tab text.
    pub accent: ColorU,
    /// Application button hover fill.
    pub accent_hover: ColorU,
    /// Text on accent fills (application button label).
    pub accent_text: ColorU,
    /// Regular control text.
    pub text: ColorU,
    /// Group captions and secondary glyphs.
    pub label: ColorU,
    /// Monochrome icon glyphs.
    pub icon: ColorU,
    /// Chrome borders: tab underline, ribbon bottom border, gallery frame.
    pub border: ColorU,
    /// Group and segment separators.
    pub separator: ColorU,
    /// Hover fill on ribbon controls.
    pub hover_bg: ColorU,
    /// Hover and toggled-on border.
    pub hover_border: ColorU,
    /// Pressed fill.
    pub pressed_bg: ColorU,
    /// Toggled-on fill.
    pub checked_bg: ColorU,
    /// Selected gallery cell fill.
    pub selected_bg: ColorU,
    /// Editable field border (embedded comboboxes).
    pub field_border: ColorU,
}

impl RibbonTheme {
    /// The the Office-2013-era look palette: white chrome, #2B579A accents, #CDE6F7 hovers.
    #[must_use]
    pub const fn office_2013() -> Self {
        Self {
            chrome_bg: WHITE,
            content_bg: WHITE,
            accent: W13_BLUE,
            accent_hover: W13_BLUE_HOVER,
            accent_text: WHITE,
            text: W13_TEXT,
            label: W13_LABEL_GRAY,
            icon: W13_ICON_GRAY,
            border: W13_BORDER,
            separator: W13_SEP,
            hover_bg: W13_HOVER_BG,
            hover_border: W13_HOVER_BORDER,
            pressed_bg: W13_PRESSED_BG,
            checked_bg: W13_CHECKED_BG,
            selected_bg: W13_SELECTED_BG,
            field_border: W13_FIELD_BORDER,
        }
    }

    /// Extracts a ribbon palette from the OS theme (accent color, selection
    /// colors, separators). Colors the platform does not report fall back to
    /// the the Office-2013-era look palette. Pass `SystemStyle::detect()` for the live
    /// system theme, or a preset `SystemStyle` for platform mockups.
    /// Takes the style by value (FFI constructor convention).
    #[must_use]
    pub fn from_system(style: SystemStyle) -> Self {
        let d = Self::office_2013();
        let c = &style.colors;
        // Each ribbon field maps to ONE system color; a color the platform
        // does not report falls back to that field's own the Office-2013-era look value
        // (never to another derived value). No color arithmetic on purpose:
        // FFI-observable behavior stays trivial to reason about.
        let accent = c.accent.into_option();
        let selection = c.selection_background.into_option();
        let separator = c.separator.into_option();
        let inactive_selection = c.selection_background_inactive.into_option();
        let secondary_text = c.secondary_text.into_option();
        Self {
            chrome_bg: c.window_background.into_option().unwrap_or(d.chrome_bg),
            content_bg: c.window_background.into_option().unwrap_or(d.content_bg),
            accent: accent.unwrap_or(d.accent),
            accent_hover: selection.unwrap_or(d.accent_hover),
            accent_text: c.accent_text.into_option().unwrap_or(d.accent_text),
            text: c.text.into_option().unwrap_or(d.text),
            label: secondary_text.unwrap_or(d.label),
            icon: secondary_text.unwrap_or(d.icon),
            border: separator.unwrap_or(d.border),
            separator: separator.unwrap_or(d.separator),
            hover_bg: inactive_selection.unwrap_or(d.hover_bg),
            hover_border: accent.unwrap_or(d.hover_border),
            pressed_bg: selection.unwrap_or(d.pressed_bg),
            checked_bg: selection.unwrap_or(d.checked_bg),
            selected_bg: inactive_selection.unwrap_or(d.selected_bg),
            field_border: separator.unwrap_or(d.field_border),
        }
    }
}

impl Default for RibbonTheme {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Colorless const part styles (shared by every theme) --

static GROUP_ITEMS_STYLE: &[Cond] = &[
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_height(LayoutHeight::const_px(68))),
    Cond::simple(P::const_align_items(LayoutAlignItems::Start)),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

static GROUP_FOOTER_STYLE: &[Cond] = &[
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_height(LayoutHeight::const_px(18))),
    Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

static FOOTER_SPACER_STYLE: &[Cond] = &[
    Cond::simple(P::const_width(LayoutWidth::const_px(18))),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

static COLUMN_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_align_items(LayoutAlignItems::Start)),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

static ROW_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
    Cond::simple(P::const_margin_bottom(LayoutMarginBottom::const_px(5))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

static GALLERY_STRIP_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Cond::simple(P::const_overflow_x(LayoutOverflow::Hidden)),
    Cond::simple(P::const_overflow_y(LayoutOverflow::Hidden)),
];

static RIBBON_COMBO_TEXT_STYLE: &[Cond] = &[
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Cond::simple(P::const_text_align(StyleTextAlign::Left)),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(2))),
];

// -- Responsive (@media) conditions --
//
// The mobile ribbon keeps the SEMANTICS of the desktop one - the same tabs,
// groups and items - and changes only presentation, so both chromes are
// emitted once and the viewport decides which is visible. That is how a
// responsive HTML page behaves, and it means no second widget tree, no
// duplicated callbacks and no state to keep in sync.
//
// `MOBILE_MAX_PX` is the breakpoint: at or below it the touch chrome shows.

/// Widest viewport that still gets the touch layout (a large phone in
/// landscape is still a phone).
pub const MOBILE_MAX_PX: f32 = 720.0;

static COND_MOBILE: &[DynamicSelector] = &[DynamicSelector::ViewportWidth(MinMaxRange {
    min: f32::NAN,
    max: MOBILE_MAX_PX,
})];

static COND_DESKTOP: &[DynamicSelector] = &[DynamicSelector::ViewportWidth(MinMaxRange {
    min: MOBILE_MAX_PX,
    max: f32::NAN,
})];

/// `display: none` unless the viewport is a phone.
fn only_on_mobile(prop: P) -> Cond {
    Cond::with_single_condition(prop, COND_MOBILE)
}

/// `display: none` unless the viewport is a desktop.
fn only_on_desktop(prop: P) -> Cond {
    Cond::with_single_condition(prop, COND_DESKTOP)
}

/// Hidden by default, shown on phones.
fn mobile_only_visibility(display: LayoutDisplay) -> [Cond; 2] {
    [
        Cond::simple(P::const_display(LayoutDisplay::None)),
        only_on_mobile(P::const_display(display)),
    ]
}

/// Visible by default, hidden on phones.
fn desktop_only_visibility(display: LayoutDisplay) -> [Cond; 2] {
    [
        Cond::simple(P::const_display(display)),
        only_on_mobile(P::const_display(LayoutDisplay::None)),
    ]
}

// -- Theme -> property-list builders --
//
// Every themed ribbon part is built from `RibbonTheme` colors by the
// functions below; `RibbonStyle::office_2013()` is just
// `from_theme(&RibbonTheme::office_2013())`, so there is exactly one source
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

/// the classic office-suite control metrics (22px small button, 66px large button, 26px tab)
/// are BORDER-BOX numbers: they include the padding and the 1px hover
/// border. CSS defaults to content-box, which inflated every control by its
/// padding+border - three 22px rows became 78px and overflowed the 68px item
/// area, painting over the group caption.
const fn cond_border_box() -> Cond {
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox))
}

fn push_padding(v: &mut Vec<Cond>, top: isize, right: isize, bottom: isize, left: isize) {
    v.push(Cond::simple(P::const_padding_top(
        LayoutPaddingTop::const_px(top),
    )));
    v.push(Cond::simple(P::const_padding_right(
        LayoutPaddingRight::const_px(right),
    )));
    v.push(Cond::simple(P::const_padding_bottom(
        LayoutPaddingBottom::const_px(bottom),
    )));
    v.push(Cond::simple(P::const_padding_left(
        LayoutPaddingLeft::const_px(left),
    )));
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
    push_border_colors(v, c);
}

fn push_border_colors(v: &mut Vec<Cond>, c: ColorU) {
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

fn push_hover_border_colors(v: &mut Vec<Cond>, c: ColorU) {
    v.push(Cond::on_hover(P::const_border_top_color(
        StyleBorderTopColor { inner: c },
    )));
    v.push(Cond::on_hover(P::const_border_left_color(
        StyleBorderLeftColor { inner: c },
    )));
    v.push(Cond::on_hover(P::const_border_right_color(
        StyleBorderRightColor { inner: c },
    )));
    v.push(Cond::on_hover(P::const_border_bottom_color(
        StyleBorderBottomColor { inner: c },
    )));
}

/// Bottom border only (tab underline / ribbon bottom edge).
fn push_bottom_border(v: &mut Vec<Cond>, c: ColorU) {
    v.push(Cond::simple(P::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )));
    v.push(Cond::simple(P::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )));
    v.push(Cond::simple(P::const_border_bottom_color(
        StyleBorderBottomColor { inner: c },
    )));
}

/// Transparent-bordered, hover-highlighted button chassis shared by large
/// and small ribbon buttons.
fn push_button_chassis(v: &mut Vec<Cond>, t: &RibbonTheme) {
    v.push(cond_border_box());
    v.push(Cond::simple(P::const_cursor(StyleCursor::Default)));
    v.push(cond_bg(TRANSPARENT));
    push_box_border(v, TRANSPARENT);
    v.push(cond_bg_hover(t.hover_bg));
    push_hover_border_colors(v, t.hover_border);
    v.push(cond_bg_active(t.pressed_bg));
}

fn theme_container(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
        cond_bg(t.chrome_bg),
    ];
    push_bottom_border(&mut v, t.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_tab_bar(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        // Replaced by the full-width mobile tab button on phones. The
        // conditional MUST come after the unconditional value: inline
        // properties resolve last-match-wins.
        only_on_mobile(P::const_display(LayoutDisplay::None)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_height(LayoutHeight::const_px(26))),
        cond_bg(t.chrome_bg),
    ])
}

fn theme_app_button(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    ];
    v.push(Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })));
    push_padding(&mut v, 7, 17, 7, 17);
    v.push(cond_bg(t.accent));
    v.push(cond_text_color(t.accent_text));
    v.push(Cond::simple(P::const_font_size(StyleFontSize::const_px(
        12,
    ))));
    v.push(Cond::simple(P::const_cursor(StyleCursor::Pointer)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    v.push(cond_bg_hover(t.accent_hover));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_tab(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    ];
    v.push(Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })));
    push_padding(&mut v, 7, 13, 6, 13);
    v.push(Cond::simple(P::const_cursor(StyleCursor::Pointer)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    v.push(cond_text_color(t.text));
    v.push(cond_bg(t.chrome_bg));
    push_bottom_border(&mut v, t.border);
    v.push(Cond::on_hover(P::const_text_color(StyleTextColor {
        inner: t.accent,
    })));
    // A tab header is ONE line. Without this "PAGE LAYOUT" wrapped, and its
    // second line was drawn below the 26px tab strip, over the ribbon content
    // - invisible only because the content band was opaque and painted over
    // it. It stops being invisible the moment an app makes that band
    // translucent, which is what `content_bg` is for.
    v.push(Cond::simple(P::WhiteSpace(
        azul_css::props::property::StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_tab_active(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    ];
    v.push(Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })));
    push_padding(&mut v, 6, 12, 6, 12);
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    v.push(cond_text_color(t.accent));
    v.push(cond_bg(t.chrome_bg));
    push_box_border(&mut v, t.border);
    // Erase the underline below the active tab: the bottom border matches
    // the chrome so the tab visually merges with the ribbon content.
    v.push(Cond::simple(P::const_border_bottom_color(
        StyleBorderBottomColor { inner: t.content_bg },
    )));
    // One line, like every other tab - see `theme_tab`.
    v.push(Cond::simple(P::WhiteSpace(
        azul_css::props::property::StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_tab_filler(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        1,
    )))];
    push_bottom_border(&mut v, t.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_content(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_height(LayoutHeight::const_px(92))),
        cond_bg(t.content_bg),
    ])
}

fn theme_group(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(3))),
        Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(2))),
        Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(2))),
        Cond::simple(P::const_border_right_width(
            LayoutBorderRightWidth::const_px(1),
        )),
        Cond::simple(P::const_border_right_style(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        })),
        Cond::simple(P::const_border_right_color(StyleBorderRightColor {
            inner: t.separator,
        })),
    ])
}

fn theme_group_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::const_text_align(StyleTextAlign::Center)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(11))),
        cond_text_color(t.label),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_launcher_button(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
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
        Cond::simple(P::const_width(LayoutWidth::const_px(16))),
        Cond::simple(P::const_height(LayoutHeight::const_px(14))),
    ];
    push_padding(&mut v, 0, 0, 0, 0);
    v.push(Cond::simple(P::const_cursor(StyleCursor::Default)));
    v.push(cond_bg(TRANSPARENT));
    push_box_border(&mut v, TRANSPARENT);
    v.push(cond_bg_hover(t.hover_bg));
    push_hover_border_colors(&mut v, t.hover_border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_launcher_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(11))),
        cond_text_color(t.label),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_separator(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_width(LayoutWidth::const_px(1))),
        Cond::simple(P::const_height(LayoutHeight::const_px(22))),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_margin_left(LayoutMarginLeft::const_px(3))),
        Cond::simple(P::const_margin_right(LayoutMarginRight::const_px(3))),
        cond_bg(t.separator),
    ])
}

fn theme_large_button(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_height(LayoutHeight::const_px(66))),
        Cond::simple(P::const_min_width(LayoutMinWidth::const_px(44))),
    ];
    push_padding(&mut v, 3, 7, 3, 7);
    v.push(Cond::simple(P::const_margin_right(
        LayoutMarginRight::const_px(1),
    )));
    push_button_chassis(&mut v, t);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_large_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(32))),
        cond_text_color(t.icon),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_large_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
        cond_text_color(t.text),
        Cond::simple(P::const_text_align(StyleTextAlign::Center)),
        Cond::simple(P::const_margin_top(LayoutMarginTop::const_px(3))),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_small_button(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_height(LayoutHeight::const_px(22))),
    ];
    push_padding(&mut v, 1, 3, 1, 3);
    push_button_chassis(&mut v, t);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_small_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(16))),
        cond_text_color(t.icon),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_small_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
        cond_text_color(t.text),
        Cond::simple(P::const_margin_left(LayoutMarginLeft::const_px(5))),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_arrow_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(14))),
        cond_text_color(t.label),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

/// Appended to a button's container style when [`RibbonButton::toggled`] is
/// set. Inline properties resolve last-wins, so these override the base.
fn theme_checked(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![cond_bg(t.checked_bg)];
    push_border_colors(&mut v, t.hover_border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_frame(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_min_width(LayoutMinWidth::const_px(137))),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::const_height(LayoutHeight::const_px(68))),
        // The frame IS the gallery viewport (like classic office suites): overflow hidden
        // both clips partially-visible cells and zeroes the frame's
        // automatic minimum size so it yields space to rigid groups.
        // (taffy 0.10 only collapses the minimum for DIRECT scroll
        // containers — see layout/tests/flex_intrinsic_text.rs.)
        Cond::simple(P::const_overflow_x(LayoutOverflow::Hidden)),
        Cond::simple(P::const_overflow_y(LayoutOverflow::Hidden)),
        cond_bg(t.chrome_bg),
    ];
    push_box_border(&mut v, t.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_cell(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_justify_content(LayoutJustifyContent::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_width(LayoutWidth::const_px(120))),
    ];
    push_padding(&mut v, 2, 6, 2, 6);
    v.push(Cond::simple(P::const_cursor(StyleCursor::Default)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    push_box_border(&mut v, TRANSPARENT);
    // Cells are divided by a thin rule on their right edge.
    v.push(Cond::simple(P::const_border_right_color(
        StyleBorderRightColor { inner: t.separator },
    )));
    v.push(cond_bg_hover(t.hover_bg));
    push_hover_border_colors(&mut v, t.hover_border);
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Appended to [`RibbonStyle::gallery_cell_style`] for the selected cell.
fn theme_gallery_cell_selected(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![cond_bg(t.selected_bg)];
    push_border_colors(&mut v, t.hover_border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_cell_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(11))),
        cond_text_color(t.text),
        Cond::simple(P::const_margin_top(LayoutMarginTop::const_px(2))),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

fn theme_gallery_spinner(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_width(LayoutWidth::const_px(15))),
        Cond::simple(P::const_border_left_width(LayoutBorderLeftWidth::const_px(
            1,
        ))),
        Cond::simple(P::const_border_left_style(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        })),
        Cond::simple(P::const_border_left_color(StyleBorderLeftColor {
            inner: t.separator,
        })),
    ])
}

/// The gallery wrapper is the positioning context for the expansion panel;
/// it is otherwise transparent and behaves exactly like the bare frame.
static GALLERY_WRAPPER_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_position(LayoutPosition::Relative)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Cond::simple(P::const_min_width(LayoutMinWidth::const_px(137))),
];

/// The "More" expansion panel: an absolutely-positioned wrapped grid of every
/// gallery cell, hidden until the More button toggles its `display`.
fn theme_gallery_panel(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        Cond::simple(P::const_display(LayoutDisplay::None)),
        Cond::simple(P::const_position(LayoutPosition::Absolute)),
        Cond::simple(P::const_top(LayoutTop::const_px(68))),
        Cond::simple(P::const_left(LayoutLeft::const_px(0))),
        Cond::simple(P::const_width(LayoutWidth::const_px(612))),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_wrap(LayoutFlexWrap::Wrap)),
        cond_bg(t.chrome_bg),
    ];
    push_box_border(&mut v, t.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_spinner_button(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_justify_content(LayoutJustifyContent::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::const_width(LayoutWidth::const_px(14))),
    ];
    push_padding(&mut v, 0, 0, 0, 0);
    v.push(Cond::simple(P::const_cursor(StyleCursor::Default)));
    v.push(cond_bg(TRANSPARENT));
    v.push(Cond::simple(P::const_border_top_width(
        LayoutBorderTopWidth::const_px(0),
    )));
    v.push(Cond::simple(P::const_border_left_width(
        LayoutBorderLeftWidth::const_px(0),
    )));
    v.push(Cond::simple(P::const_border_right_width(
        LayoutBorderRightWidth::const_px(0),
    )));
    v.push(Cond::simple(P::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(0),
    )));
    v.push(cond_bg_hover(t.hover_bg));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_spinner_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
        cond_text_color(t.label),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

/// Office-2013-look combobox parts, injected by [`RibbonStyle::styled_combo_box`].
fn theme_combo_wrapper_base(_t: &RibbonTheme) -> Vec<Cond> {
    vec![
        Cond::simple(P::const_display(LayoutDisplay::InlineBlock)),
        Cond::simple(P::const_position(LayoutPosition::Relative)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_margin_right(LayoutMarginRight::const_px(2))),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
        Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
    ]
}

fn theme_combo_field(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_height(LayoutHeight::const_px(22))),
    ];
    push_padding(&mut v, 0, 2, 0, 5);
    v.push(Cond::simple(P::const_cursor(StyleCursor::Text)));
    v.push(cond_bg(t.chrome_bg));
    v.push(cond_text_color(t.text));
    push_box_border(&mut v, t.field_border);
    v.push(Cond::on_focus(P::const_border_top_color(
        StyleBorderTopColor { inner: t.accent },
    )));
    v.push(Cond::on_focus(P::const_border_left_color(
        StyleBorderLeftColor { inner: t.accent },
    )));
    v.push(Cond::on_focus(P::const_border_right_color(
        StyleBorderRightColor { inner: t.accent },
    )));
    v.push(Cond::on_focus(P::const_border_bottom_color(
        StyleBorderBottomColor { inner: t.accent },
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_combo_arrow(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(14))),
        cond_text_color(t.label),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

// -- Mobile part styles --
//
// Touch targets follow the platform minimum (44px). The desktop tab strip
// and the mobile tab button are mutually exclusive via the viewport
// condition, so exactly one is ever visible.

/// The full-width tab button that replaces the tab strip on phones. Shows the
/// ACTIVE tab's label plus a chevron that opens the tab overlay; double
/// tapping it collapses the ribbon exactly like double clicking a desktop tab.
fn theme_mobile_tab_button(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = mobile_only_visibility(LayoutDisplay::Flex).to_vec();
    v.push(cond_border_box());
    v.push(Cond::simple(P::const_flex_direction(
        LayoutFlexDirection::Row,
    )));
    v.push(Cond::simple(P::const_align_items(LayoutAlignItems::Center)));
    v.push(Cond::simple(P::const_height(LayoutHeight::const_px(48))));
    v.push(Cond::simple(P::const_width(LayoutWidth::Px(
        PixelValue::const_percent(100),
    ))));
    push_padding(&mut v, 0, 12, 0, 16);
    v.push(Cond::simple(P::const_font_size(StyleFontSize::const_px(
        17,
    ))));
    v.push(cond_text_color(t.accent));
    v.push(cond_bg(t.chrome_bg));
    push_bottom_border(&mut v, t.border);
    v.push(Cond::simple(P::const_cursor(StyleCursor::Pointer)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The active tab's label inside the mobile tab button.
fn theme_mobile_tab_label(_t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

/// Chevron on the mobile tab button.
fn theme_mobile_tab_arrow(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![
        Cond::simple(P::const_font_size(StyleFontSize::const_px(24))),
        cond_text_color(t.accent),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ])
}

/// Full-screen overlay listing every tab; opened by the mobile tab button.
fn theme_mobile_tab_overlay(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        // Hidden until the button opens it (on ANY viewport: the overlay is
        // only reachable through the mobile button).
        Cond::simple(P::const_display(LayoutDisplay::None)),
        cond_border_box(),
        Cond::simple(P::const_position(LayoutPosition::Absolute)),
        Cond::simple(P::const_top(LayoutTop::const_px(0))),
        Cond::simple(P::const_left(LayoutLeft::const_px(0))),
        Cond::simple(P::const_width(LayoutWidth::Px(PixelValue::const_percent(
            100,
        )))),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        cond_bg(t.chrome_bg),
    ];
    push_box_border(&mut v, t.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

/// One row of the mobile tab overlay - a full-width 48px touch target.
fn theme_mobile_tab_overlay_item(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_height(LayoutHeight::const_px(48))),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(17))),
        cond_text_color(t.text),
        Cond::simple(P::const_cursor(StyleCursor::Pointer)),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ];
    push_padding(&mut v, 0, 16, 0, 16);
    push_bottom_border(&mut v, t.separator);
    v.push(cond_bg_hover(t.hover_bg));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The scrollable list of GROUP names shown beside the visible group on
/// phones. Sits on the user's dominant-hand side (see [`Handedness`]).
fn theme_mobile_group_list(t: &RibbonTheme, left_handed: bool) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = mobile_only_visibility(LayoutDisplay::Flex).to_vec();
    v.push(cond_border_box());
    v.push(Cond::simple(P::const_flex_direction(
        LayoutFlexDirection::Column,
    )));
    v.push(Cond::simple(P::const_width(LayoutWidth::const_px(116))));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })));
    v.push(Cond::simple(P::const_overflow_y(LayoutOverflow::Scroll)));
    v.push(Cond::simple(P::const_overflow_x(LayoutOverflow::Hidden)));
    v.push(cond_bg(t.chrome_bg));
    // The list hugs the dominant hand: a border on the side that faces the
    // content, so the divider reads correctly whichever side it is on.
    if left_handed {
        v.push(Cond::simple(P::const_border_right_width(
            LayoutBorderRightWidth::const_px(1),
        )));
        v.push(Cond::simple(P::const_border_right_style(
            StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            },
        )));
        v.push(Cond::simple(P::const_border_right_color(
            StyleBorderRightColor { inner: t.separator },
        )));
    } else {
        v.push(Cond::simple(P::const_border_left_width(
            LayoutBorderLeftWidth::const_px(1),
        )));
        v.push(Cond::simple(P::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            },
        )));
        v.push(Cond::simple(P::const_border_left_color(
            StyleBorderLeftColor { inner: t.separator },
        )));
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

/// One entry of the mobile group list.
fn theme_mobile_group_list_item(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
        Cond::simple(P::const_height(LayoutHeight::const_px(44))),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(15))),
        cond_text_color(t.text),
        Cond::simple(P::const_cursor(StyleCursor::Pointer)),
        Cond::simple(P::user_select(StyleUserSelect::None)),
    ];
    push_padding(&mut v, 0, 10, 0, 12);
    push_bottom_border(&mut v, t.separator);
    v.push(cond_bg_hover(t.hover_bg));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The selected entry of the mobile group list (appended, last-wins).
fn theme_mobile_group_list_item_selected(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::from_vec(vec![cond_bg(t.selected_bg), cond_text_color(t.accent)])
}

// -- Classes --

static CLS_RIBBON: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-ribbon"))];
static CLS_TAB_BAR: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-tabbar",
))];
static CLS_APP_BUTTON: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-appbutton",
))];
static CLS_TAB: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-ribbon-tab"))];
static CLS_TAB_ACTIVE: &[IdOrClass] = &[
    Class(AzString::from_const_str("__azul-native-ribbon-tab")),
    Class(AzString::from_const_str("__azul-native-ribbon-tab-active")),
];
static CLS_TAB_FILLER: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-tab-filler",
))];
static CLS_CONTENT: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-content",
))];
static CLS_GROUP: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-group",
))];
static CLS_GROUP_ITEMS: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-group-items",
))];
static CLS_GROUP_FOOTER: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-group-footer",
))];
static CLS_GROUP_LABEL: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-group-label",
))];
static CLS_FOOTER_SPACER: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-footer-spacer",
))];
static CLS_COLUMN: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-column",
))];
static CLS_ROW: &[IdOrClass] = &[Class(AzString::from_const_str("__azul-native-ribbon-row"))];
static CLS_SEPARATOR: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-separator",
))];
static CLS_GALLERY: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery",
))];
static CLS_GALLERY_STRIP: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-strip",
))];
static CLS_GALLERY_CELL: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-cell",
))];
static CLS_GALLERY_CELL_SELECTED: &[IdOrClass] = &[
    Class(AzString::from_const_str(
        "__azul-native-ribbon-gallery-cell",
    )),
    Class(AzString::from_const_str(
        "__azul-native-ribbon-gallery-cell-selected",
    )),
];
/// Class names the handlers resolve their targets by (see
/// `ancestor_with_class`), kept next to the `IdOrClass` tables that emit them.
const GALLERY_WRAPPER_CLASS: &str = "__azul-native-ribbon-gallery-wrapper";
const GALLERY_CELL_CLASS: &str = "__azul-native-ribbon-gallery-cell";
const RIBBON_TAB_CLASS: &str = "__azul-native-ribbon-tab";

const MOBILE_TAB_BUTTON_CLASS: &str = "__azul-native-ribbon-mobile-tab";
const RIBBON_CONTAINER_CLASS: &str = "__azul-native-ribbon";
const MOBILE_GROUP_LIST_ITEM_CLASS: &str = "__azul-native-ribbon-mobile-group-list-item";
const RIBBON_CONTENT_CLASS: &str = "__azul-native-ribbon-content";
static CLS_MOBILE_TAB_BUTTON: &[IdOrClass] =
    &[Class(AzString::from_const_str(MOBILE_TAB_BUTTON_CLASS))];
static CLS_MOBILE_TAB_OVERLAY: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-mobile-tab-overlay",
))];
static CLS_MOBILE_TAB_OVERLAY_ITEM: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-mobile-tab-overlay-item",
))];
static CLS_MOBILE_GROUP_LIST: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-mobile-group-list",
))];
static CLS_MOBILE_BAND: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-mobile-band",
))];
static CLS_MOBILE_GROUP_LIST_ITEM: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-mobile-group-list-item",
))];

static CLS_GALLERY_MORE: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-more",
))];
static CLS_GALLERY_WRAPPER: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-wrapper",
))];
static CLS_GALLERY_PANEL: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-panel",
))];
static CLS_GALLERY_SPINNER: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-spinner",
))];

// -- Style bundle --

/// Every visual part of the ribbon chrome as a replaceable property list.
///
/// The default ([`RibbonStyle::office_2013`]) reproduces the the Office-2013-era look look:
/// white chrome, #2B579A accents, #CDE6F7 hover fills. Each field is applied
/// to exactly one DOM part; replace any of them to re-theme that part.
/// Fields named `*_style` fully replace the part's style; [`Self::checked_style`]
/// and [`Self::gallery_cell_selected_style`] are *appended* to the base button /
/// cell style (inline CSS resolves last-wins, so appended properties override).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct RibbonStyle {
    /// The palette this style bundle was derived from. Kept for
    /// [`Self::styled_combo_box`] and for consumers deriving matching
    /// custom parts.
    pub theme: RibbonTheme,
    /// Root container (vertical: tab bar over content).
    pub container_style: CssPropertyWithConditionsVec,
    /// The horizontal tab strip.
    pub tab_bar_style: CssPropertyWithConditionsVec,
    /// The blue application button ("FILE").
    pub app_button_style: CssPropertyWithConditionsVec,
    /// An inactive tab header.
    pub tab_style: CssPropertyWithConditionsVec,
    /// The active tab header.
    pub tab_active_style: CssPropertyWithConditionsVec,
    /// The filler segment after the last tab (carries the underline).
    pub tab_filler_style: CssPropertyWithConditionsVec,
    /// The content band below the tab strip (horizontal group list).
    pub content_style: CssPropertyWithConditionsVec,
    /// One group (vertical: items over footer), incl. the right separator.
    pub group_style: CssPropertyWithConditionsVec,
    /// The item area of a group.
    pub group_items_style: CssPropertyWithConditionsVec,
    /// The footer row of a group (label + dialog launcher).
    pub group_footer_style: CssPropertyWithConditionsVec,
    /// The centered group caption.
    pub group_label_style: CssPropertyWithConditionsVec,
    /// Invisible spacer balancing the launcher so the caption stays centered.
    pub footer_spacer_style: CssPropertyWithConditionsVec,
    /// Container style injected into the dialog-launcher [`Button`].
    pub launcher_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into the dialog-launcher [`Button`].
    pub launcher_icon_style: CssPropertyWithConditionsVec,
    /// A [`RibbonColumn`] packing box.
    pub column_style: CssPropertyWithConditionsVec,
    /// A [`RibbonRow`] packing box.
    pub row_style: CssPropertyWithConditionsVec,
    /// A [`RibbonItem::Separator`] rule.
    pub separator_style: CssPropertyWithConditionsVec,
    /// Container style injected into large-button [`Button`]s.
    pub large_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into large-button [`Button`]s.
    pub large_icon_style: CssPropertyWithConditionsVec,
    /// Label style injected into large-button [`Button`]s.
    pub large_label_style: CssPropertyWithConditionsVec,
    /// Container style injected into small-button [`Button`]s.
    pub small_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into small-button [`Button`]s.
    pub small_icon_style: CssPropertyWithConditionsVec,
    /// Label style injected into small-button [`Button`]s.
    pub small_label_style: CssPropertyWithConditionsVec,
    /// Style of the drop-down arrow glyph on Menu/Split buttons.
    pub arrow_icon_style: CssPropertyWithConditionsVec,
    /// APPENDED to the button container when [`RibbonButton::toggled`] is set.
    pub checked_style: CssPropertyWithConditionsVec,
    /// The gallery outer frame.
    pub gallery_frame_style: CssPropertyWithConditionsVec,
    /// The horizontal cell strip inside the gallery frame.
    pub gallery_strip_style: CssPropertyWithConditionsVec,
    /// One gallery cell.
    pub gallery_cell_style: CssPropertyWithConditionsVec,
    /// APPENDED to the selected gallery cell.
    pub gallery_cell_selected_style: CssPropertyWithConditionsVec,
    /// The name label under a gallery cell preview.
    pub gallery_cell_label_style: CssPropertyWithConditionsVec,
    /// The vertical spinner column on the gallery's right edge.
    pub gallery_spinner_style: CssPropertyWithConditionsVec,
    /// Positioning context wrapping the gallery frame + expansion panel.
    pub gallery_wrapper_style: CssPropertyWithConditionsVec,
    /// The expansion panel shown by the gallery's "More" button.
    pub gallery_panel_style: CssPropertyWithConditionsVec,
    /// Full-width tab button shown INSTEAD of the tab strip on phones.
    pub mobile_tab_button_style: CssPropertyWithConditionsVec,
    /// Active-tab label inside the mobile tab button.
    pub mobile_tab_label_style: CssPropertyWithConditionsVec,
    /// Chevron on the mobile tab button.
    pub mobile_tab_arrow_style: CssPropertyWithConditionsVec,
    /// Full-screen tab picker opened by the mobile tab button.
    pub mobile_tab_overlay_style: CssPropertyWithConditionsVec,
    /// One row of the mobile tab picker.
    pub mobile_tab_overlay_item_style: CssPropertyWithConditionsVec,
    /// Scrollable group list shown beside the visible group on phones.
    pub mobile_group_list_style: CssPropertyWithConditionsVec,
    /// One entry of the mobile group list.
    pub mobile_group_list_item_style: CssPropertyWithConditionsVec,
    /// APPENDED to the selected mobile group-list entry.
    pub mobile_group_list_item_selected_style: CssPropertyWithConditionsVec,
    /// Container style injected into the three spinner [`Button`]s.
    pub gallery_spinner_button_style: CssPropertyWithConditionsVec,
    /// Icon style injected into the three spinner [`Button`]s.
    pub gallery_spinner_icon_style: CssPropertyWithConditionsVec,
}

impl RibbonStyle {
    /// The the Office-2013-era look look (white chrome, #2B579A accents) - the default.
    #[must_use]
    pub fn office_2013() -> Self {
        Self::from_theme(RibbonTheme::office_2013())
    }

    /// Derives every part style from the given palette. This is the styling
    /// override API: build a [`RibbonTheme`] (or start from a preset), then
    /// replace individual `*_style` fields for finer control.
    #[must_use]
    pub fn from_theme(theme: RibbonTheme) -> Self {
        Self::from_theme_handed(theme, Handedness::RightHanded)
    }

    /// [`Self::from_theme`] with an explicit hand: the mobile group list sits
    /// on the dominant-hand side so the thumb reaches it. Independent of text
    /// direction - see [`Handedness`].
    #[must_use]
    pub fn from_theme_handed(theme: RibbonTheme, handedness: Handedness) -> Self {
        let left_handed = matches!(handedness, Handedness::LeftHanded);
        let theme = &theme;
        Self {
            theme: *theme,
            container_style: theme_container(theme),
            tab_bar_style: theme_tab_bar(theme),
            app_button_style: theme_app_button(theme),
            tab_style: theme_tab(theme),
            tab_active_style: theme_tab_active(theme),
            tab_filler_style: theme_tab_filler(theme),
            content_style: theme_content(theme),
            group_style: theme_group(theme),
            group_items_style: CssPropertyWithConditionsVec::from_const_slice(GROUP_ITEMS_STYLE),
            group_footer_style: CssPropertyWithConditionsVec::from_const_slice(GROUP_FOOTER_STYLE),
            group_label_style: theme_group_label(theme),
            footer_spacer_style: CssPropertyWithConditionsVec::from_const_slice(
                FOOTER_SPACER_STYLE,
            ),
            launcher_button_style: theme_launcher_button(theme),
            launcher_icon_style: theme_launcher_icon(theme),
            column_style: CssPropertyWithConditionsVec::from_const_slice(COLUMN_STYLE),
            row_style: CssPropertyWithConditionsVec::from_const_slice(ROW_STYLE),
            separator_style: theme_separator(theme),
            large_button_style: theme_large_button(theme),
            large_icon_style: theme_large_icon(theme),
            large_label_style: theme_large_label(theme),
            small_button_style: theme_small_button(theme),
            small_icon_style: theme_small_icon(theme),
            small_label_style: theme_small_label(theme),
            arrow_icon_style: theme_arrow_icon(theme),
            checked_style: theme_checked(theme),
            gallery_frame_style: theme_gallery_frame(theme),
            gallery_strip_style: CssPropertyWithConditionsVec::from_const_slice(
                GALLERY_STRIP_STYLE,
            ),
            gallery_cell_style: theme_gallery_cell(theme),
            gallery_cell_selected_style: theme_gallery_cell_selected(theme),
            gallery_cell_label_style: theme_gallery_cell_label(theme),
            gallery_spinner_style: theme_gallery_spinner(theme),
            gallery_wrapper_style: CssPropertyWithConditionsVec::from_const_slice(
                GALLERY_WRAPPER_STYLE,
            ),
            gallery_panel_style: theme_gallery_panel(theme),
            mobile_tab_button_style: theme_mobile_tab_button(theme),
            mobile_tab_label_style: theme_mobile_tab_label(theme),
            mobile_tab_arrow_style: theme_mobile_tab_arrow(theme),
            mobile_tab_overlay_style: theme_mobile_tab_overlay(theme),
            mobile_tab_overlay_item_style: theme_mobile_tab_overlay_item(theme),
            mobile_group_list_style: theme_mobile_group_list(theme, left_handed),
            mobile_group_list_item_style: theme_mobile_group_list_item(theme),
            mobile_group_list_item_selected_style: theme_mobile_group_list_item_selected(theme),
            gallery_spinner_button_style: theme_gallery_spinner_button(theme),
            gallery_spinner_icon_style: theme_gallery_spinner_icon(theme),
        }
    }

    /// Derives the ribbon style from the OS theme (see
    /// [`RibbonTheme::from_system`]). Pass `SystemStyle::detect()` for the
    /// live system look, e.g. to render a "system native" ribbon.
    #[must_use]
    pub fn from_system(style: SystemStyle) -> Self {
        let handedness = style.handedness;
        Self::from_theme_handed(RibbonTheme::from_system(style), handedness)
    }

    /// Returns a [`ComboBox`] with this ribbon's field look injected through
    /// the combobox's public style fields (flat 1px border, 22px field, 12px
    /// text - the the Office-2013-era look font-name/font-size pickers). `width` is the
    /// total field width in px. Demonstrates (and exercises) the widget
    /// style-injection API; tweak the returned combobox further by replacing
    /// any of its `*_style` fields.
    #[must_use]
    pub fn styled_combo_box(&self, items: StringVec, text: AzString, width: isize) -> ComboBox {
        let mut combo = ComboBox::new(items).with_text(text);
        let mut wrapper: Vec<Cond> = theme_combo_wrapper_base(&self.theme);
        wrapper.push(Cond::simple(P::const_width(LayoutWidth::const_px(width))));
        combo.wrapper_style = CssPropertyWithConditionsVec::from_vec(wrapper);
        combo.field_style = theme_combo_field(&self.theme);
        combo.text_style = CssPropertyWithConditionsVec::from_const_slice(RIBBON_COMBO_TEXT_STYLE);
        combo.arrow_style = theme_combo_arrow(&self.theme);
        combo
    }
}

impl Default for RibbonStyle {
    fn default() -> Self {
        Self::office_2013()
    }
}

// -- Data model --

/// The interactive behaviors the ribbon performs BY ITSELF, without any
/// application state. Each is the classic default and each can be turned off,
/// in which case the corresponding event is still forwarded to the app
/// callback (if any) but the ribbon does not touch its own chrome.
///
/// The state these behaviors need (collapsed / peeked / selected cell) lives
/// in a private `RefAny` minted inside [`Ribbon::dom`] — the application's
/// own data model is never involved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct RibbonBehavior {
    /// Double-clicking a tab header collapses the content band; double
    /// clicking again restores it (office-2013: "Collapse the Ribbon").
    pub collapsible: bool,
    /// While collapsed, hovering a tab header peeks the content band and
    /// leaving it hides the band again.
    pub peek_on_hover: bool,
    /// Clicking a gallery cell moves the selection highlight without waiting
    /// for the app to re-render.
    pub auto_select_gallery: bool,
    /// The gallery's third spinner button ("More") toggles an expansion
    /// panel showing every cell.
    pub expandable_gallery: bool,
    /// On phones, tapping the tab button opens the full-screen tab picker.
    /// With this off the button is inert and the application drives tab
    /// switching itself.
    pub mobile_tab_overlay: bool,
}

impl RibbonBehavior {
    /// All classic office-suite behaviors enabled - the default.
    #[must_use]
    pub const fn office_2013() -> Self {
        Self {
            collapsible: true,
            peek_on_hover: true,
            auto_select_gallery: true,
            expandable_gallery: true,
            mobile_tab_overlay: true,
        }
    }

    /// Every self-driven behavior off: the ribbon only forwards events to
    /// the application callbacks and never patches its own chrome.
    #[must_use]
    pub const fn inert() -> Self {
        Self {
            collapsible: false,
            peek_on_hover: false,
            auto_select_gallery: false,
            expandable_gallery: false,
            mobile_tab_overlay: false,
        }
    }
}

impl Default for RibbonBehavior {
    fn default() -> Self {
        Self::office_2013()
    }
}

/// Top-level ribbon widget: an optional application button, a tab strip and
/// the active tab's groups.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct Ribbon {
    /// Optional application button rendered before the first tab ("FILE").
    pub app_button: OptionRibbonAppButton,
    /// Tabs displayed in the ribbon tab bar.
    pub tabs: RibbonTabVec,
    /// Index of the currently active tab.
    pub active_tab: usize,
    /// Optional callback fired when a tab is clicked (receives the tab index).
    pub on_tab_click: OptionRibbonOnTabClick,
    /// All part styles (defaults to the the Office-2013-era look look).
    pub style: RibbonStyle,
    /// Which interactions the ribbon handles by itself (defaults to the classic behavior).
    pub behavior: RibbonBehavior,
}

/// The application button at the far left of the tab strip ("FILE").
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonAppButton {
    /// Display label of the application button.
    pub label: AzString,
    /// Optional click callback.
    pub on_click: OptionButtonOnClick,
}

/// A single tab within a [`Ribbon`], containing a label and groups.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonTab {
    /// Display label shown in the tab bar.
    pub label: AzString,
    /// Groups rendered when this tab is active.
    pub groups: RibbonGroupVec,
    /// Extra properties APPENDED to this tab header's style, after the
    /// shared [`RibbonStyle::tab_style`] / [`RibbonStyle::tab_active_style`]
    /// — so they win, and they apply in BOTH states.
    ///
    /// Empty (the default) leaves the tab looking like every other one.
    /// This is the only per-tab hook: `RibbonStyle` describes the tab
    /// STRIP, so without it a single tab could not be tinted, badged or
    /// given its own border, and telling two tabs apart in a screenshot
    /// meant reading their labels.
    pub style: CssPropertyWithConditionsVec,
}

/// A captioned group of controls within a [`RibbonTab`].
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonGroup {
    /// Caption shown centered under the group content.
    pub label: AzString,
    /// The controls of this group, laid out left-to-right.
    pub items: RibbonItemVec,
    /// Optional dialog-box-launcher callback; when set, a small launcher
    /// button is rendered at the right end of the caption row.
    pub launcher: OptionButtonOnClick,
    /// When set, this group absorbs the remaining ribbon width (the classic office-suite
    /// Styles gallery group stretches; the other groups are content-sized).
    pub fills_space: bool,
}

/// One control slot inside a [`RibbonGroup`].
// `#[repr(C, u8)]` — this enum crosses the C ABI and is mirrored in api.json.
// Boxing the large variant to equalise sizes would change the generated
// bindings for every language, so the size spread is deliberate.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum RibbonItem {
    /// Full-height button: icon over label (`RibbonX` `button[size=large]`).
    LargeButton(RibbonButton),
    /// One-row button: icon beside optional label (`RibbonX` `button`).
    SmallButton(RibbonButton),
    /// Vertical packing box (`RibbonX` `box[boxStyle=vertical]`).
    Column(RibbonColumn),
    /// Horizontal packing box (`RibbonX` `box`/`buttonGroup`).
    Row(RibbonRow),
    /// Embeds the existing [`ComboBox`] widget (`RibbonX` `comboBox`).
    Combo(ComboBox),
    /// Embeds the existing [`DropDown`] widget (`RibbonX` `dropDown`).
    Drop(DropDown),
    /// Embeds the existing [`CheckBox`] widget (`RibbonX` `checkBox`).
    Check(CheckBox),
    /// In-ribbon gallery with spinner column (`RibbonX` `gallery`).
    Gallery(RibbonGallery),
    /// Thin vertical rule (`RibbonX` `separator`).
    Separator,
    /// Arbitrary user content.
    Custom(Dom),
}

/// Vertical stack of items (e.g. the Cut/Copy/Format-Painter column).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonColumn {
    /// Items stacked top-to-bottom.
    pub items: RibbonItemVec,
}

/// Horizontal cluster of items (e.g. the Bold/Italic/Underline row).
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonRow {
    /// Items packed left-to-right.
    pub items: RibbonItemVec,
}

/// Declarative description of one ribbon button; expands to the existing
/// [`Button`] widget with ribbon styles injected.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonButton {
    /// Icon name resolved via the icon provider (Material Icons ships
    /// builtin, e.g. "`content_paste`"). Empty string = no icon.
    pub icon: AzString,
    /// Button label. Empty string = icon-only button.
    pub label: AzString,
    /// Drop-down decoration: none, menu arrow or split-button arrow.
    pub arrow: RibbonArrow,
    /// Renders the button in the toggled-on state (`RibbonX` `toggleButton`).
    pub toggled: bool,
    /// Optional click callback (same family as [`Button::on_click`]).
    pub on_click: OptionButtonOnClick,
}

/// Drop-down decoration of a [`RibbonButton`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum RibbonArrow {
    /// Plain button without an arrow.
    #[default]
    None,
    /// The whole button opens a menu (`RibbonX` `menu`).
    Menu,
    /// Primary action + separate arrow region (`RibbonX` `splitButton`).
    /// Rendered identically to `Menu`; the split behavior is the caller's.
    Split,
}

/// In-ribbon gallery: a strip of preview cells plus a 3-button spinner column.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonGallery {
    /// The visible cells.
    pub cells: RibbonGalleryCellVec,
    /// Index of the selected cell.
    pub selected: usize,
    /// Optional callback fired when a cell is clicked (receives cell index).
    pub on_select: OptionRibbonGalleryOnSelect,
}

/// One gallery cell: an arbitrary preview [`Dom`] over a name label.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonGalleryCell {
    /// Preview content rendered above the label (e.g. styled sample text).
    pub preview: Dom,
    /// Name label rendered under the preview.
    pub label: AzString,
}

impl_option!(
    RibbonAppButton,
    OptionRibbonAppButton,
    copy = false,
    [Debug, Clone]
);
impl_option!(RibbonTab, OptionRibbonTab, copy = false, [Debug, Clone]);
impl_option!(RibbonGroup, OptionRibbonGroup, copy = false, [Debug, Clone]);
impl_option!(RibbonItem, OptionRibbonItem, copy = false, [Debug, Clone]);
impl_option!(
    RibbonGalleryCell,
    OptionRibbonGalleryCell,
    copy = false,
    [Debug, Clone]
);

impl_vec!(
    RibbonTab,
    RibbonTabVec,
    RibbonTabVecDestructor,
    RibbonTabVecDestructorType,
    RibbonTabVecSlice,
    OptionRibbonTab
);
impl_vec_clone!(RibbonTab, RibbonTabVec, RibbonTabVecDestructor);
impl_vec_debug!(RibbonTab, RibbonTabVec);
impl_vec_mut!(RibbonTab, RibbonTabVec);

impl_vec!(
    RibbonGroup,
    RibbonGroupVec,
    RibbonGroupVecDestructor,
    RibbonGroupVecDestructorType,
    RibbonGroupVecSlice,
    OptionRibbonGroup
);
impl_vec_clone!(RibbonGroup, RibbonGroupVec, RibbonGroupVecDestructor);
impl_vec_debug!(RibbonGroup, RibbonGroupVec);
impl_vec_mut!(RibbonGroup, RibbonGroupVec);

impl_vec!(
    RibbonItem,
    RibbonItemVec,
    RibbonItemVecDestructor,
    RibbonItemVecDestructorType,
    RibbonItemVecSlice,
    OptionRibbonItem
);
impl_vec_clone!(RibbonItem, RibbonItemVec, RibbonItemVecDestructor);
impl_vec_debug!(RibbonItem, RibbonItemVec);
impl_vec_mut!(RibbonItem, RibbonItemVec);

impl_vec!(
    RibbonGalleryCell,
    RibbonGalleryCellVec,
    RibbonGalleryCellVecDestructor,
    RibbonGalleryCellVecDestructorType,
    RibbonGalleryCellVecSlice,
    OptionRibbonGalleryCell
);
impl_vec_clone!(
    RibbonGalleryCell,
    RibbonGalleryCellVec,
    RibbonGalleryCellVecDestructor
);
impl_vec_debug!(RibbonGalleryCell, RibbonGalleryCellVec);
impl_vec_mut!(RibbonGalleryCell, RibbonGalleryCellVec);

// -- Constructors / builders --

impl RibbonAppButton {
    /// Creates an application button with the given label and no callback.
    #[must_use]
    pub fn new(label: AzString) -> Self {
        Self {
            label,
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

impl RibbonTab {
    /// Creates a new tab with the given label and no groups.
    #[must_use]
    pub const fn new(label: AzString) -> Self {
        Self {
            label,
            groups: RibbonGroupVec::from_const_slice(&[]),
            style: CssPropertyWithConditionsVec::from_const_slice(&[]),
        }
    }

    /// Appends per-tab style properties to this tab's header (see
    /// [`RibbonTab::style`]).
    pub fn set_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.style = style;
    }

    /// Builder method: sets the per-tab header style and returns `self`.
    #[must_use]
    pub fn with_style(mut self, style: CssPropertyWithConditionsVec) -> Self {
        self.set_style(style);
        self
    }

    /// Appends a group to this tab.
    pub fn add_group(&mut self, group: RibbonGroup) {
        self.groups.push(group);
    }

    /// Builder method: appends a group and returns `self`.
    #[must_use]
    pub fn with_group(mut self, group: RibbonGroup) -> Self {
        self.add_group(group);
        self
    }
}

impl RibbonGroup {
    /// Creates a new group with the given caption and no items.
    #[must_use]
    pub const fn new(label: AzString) -> Self {
        Self {
            label,
            items: RibbonItemVec::from_const_slice(&[]),
            launcher: OptionButtonOnClick::None,
            fills_space: false,
        }
    }

    /// Builder method: makes this group absorb the remaining ribbon width.
    #[must_use]
    pub const fn with_fills_space(mut self, fills_space: bool) -> Self {
        self.fills_space = fills_space;
        self
    }

    /// Appends an item to this group.
    pub fn add_item(&mut self, item: RibbonItem) {
        self.items.push(item);
    }

    /// Builder method: appends an item and returns `self`.
    #[must_use]
    pub fn with_item(mut self, item: RibbonItem) -> Self {
        self.add_item(item);
        self
    }

    /// Sets the dialog-box-launcher callback (renders the launcher button).
    pub fn set_launcher<C: Into<super::button::ButtonOnClickCallback>>(
        &mut self,
        data: RefAny,
        on_click: C,
    ) {
        self.launcher = Some(super::button::ButtonOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    /// Builder method: sets the launcher callback and returns `self`.
    #[must_use]
    pub fn with_launcher<C: Into<super::button::ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
        self.set_launcher(data, on_click);
        self
    }
}

impl RibbonColumn {
    /// Creates an empty column.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: RibbonItemVec::from_const_slice(&[]),
        }
    }

    /// Appends an item to this column.
    pub fn add_item(&mut self, item: RibbonItem) {
        self.items.push(item);
    }

    /// Builder method: appends an item and returns `self`.
    #[must_use]
    pub fn with_item(mut self, item: RibbonItem) -> Self {
        self.add_item(item);
        self
    }
}

impl Default for RibbonColumn {
    fn default() -> Self {
        Self::new()
    }
}

impl RibbonRow {
    /// Creates an empty row.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            items: RibbonItemVec::from_const_slice(&[]),
        }
    }

    /// Appends an item to this row.
    pub fn add_item(&mut self, item: RibbonItem) {
        self.items.push(item);
    }

    /// Builder method: appends an item and returns `self`.
    #[must_use]
    pub fn with_item(mut self, item: RibbonItem) -> Self {
        self.add_item(item);
        self
    }
}

impl Default for RibbonRow {
    fn default() -> Self {
        Self::new()
    }
}

impl RibbonButton {
    /// Creates a plain button with an icon and a label (both may be empty).
    #[must_use]
    pub const fn new(icon: AzString, label: AzString) -> Self {
        Self {
            icon,
            label,
            arrow: RibbonArrow::None,
            toggled: false,
            on_click: OptionButtonOnClick::None,
        }
    }

    /// Builder method: sets the arrow decoration and returns `self`.
    #[must_use]
    pub const fn with_arrow(mut self, arrow: RibbonArrow) -> Self {
        self.arrow = arrow;
        self
    }

    /// Builder method: sets the toggled state and returns `self`.
    #[must_use]
    pub const fn with_toggled(mut self, toggled: bool) -> Self {
        self.toggled = toggled;
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

impl RibbonGallery {
    /// Creates a gallery from its cells; cell 0 is selected.
    #[must_use]
    pub fn new(cells: RibbonGalleryCellVec) -> Self {
        Self {
            cells,
            selected: 0,
            on_select: None.into(),
        }
    }

    /// Builder method: sets the selected cell index and returns `self`.
    #[must_use]
    pub const fn with_selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    /// Sets the cell-click callback.
    pub fn set_on_select<C: Into<RibbonGalleryOnSelectCallback>>(
        &mut self,
        data: RefAny,
        on_select: C,
    ) {
        self.on_select = Some(RibbonGalleryOnSelect {
            refany: data,
            callback: on_select.into(),
        })
        .into();
    }

    /// Builder method: sets the cell-click callback and returns `self`.
    #[must_use]
    pub fn with_on_select<C: Into<RibbonGalleryOnSelectCallback>>(
        mut self,
        data: RefAny,
        on_select: C,
    ) -> Self {
        self.set_on_select(data, on_select);
        self
    }
}

impl RibbonGalleryCell {
    /// Creates a cell from a preview subtree and a name label.
    #[must_use]
    pub const fn new(preview: Dom, label: AzString) -> Self {
        Self { preview, label }
    }
}

impl Ribbon {
    /// Creates a new ribbon with the given tabs, the first tab active and the
    /// the Office-2013-era look default style.
    #[must_use]
    pub fn new(tabs: RibbonTabVec) -> Self {
        Self {
            app_button: None.into(),
            tabs,
            active_tab: 0,
            on_tab_click: None.into(),
            style: RibbonStyle::office_2013(),
            behavior: RibbonBehavior::office_2013(),
        }
    }

    /// Sets the application button ("FILE").
    pub fn set_app_button(&mut self, app_button: RibbonAppButton) {
        self.app_button = Some(app_button).into();
    }

    /// Builder method: sets the application button and returns `self`.
    #[must_use]
    pub fn with_app_button(mut self, app_button: RibbonAppButton) -> Self {
        self.set_app_button(app_button);
        self
    }

    /// Replaces the whole style bundle.
    pub fn set_style(&mut self, style: RibbonStyle) {
        self.style = style;
    }

    /// Builder method: replaces the style bundle and returns `self`.
    #[must_use]
    pub fn with_style(mut self, style: RibbonStyle) -> Self {
        self.set_style(style);
        self
    }

    /// Replaces the self-driven behavior set (collapse, peek, gallery).
    pub const fn set_behavior(&mut self, behavior: RibbonBehavior) {
        self.behavior = behavior;
    }

    /// Builder method: replaces the behavior set and returns `self`.
    #[must_use]
    pub const fn with_behavior(mut self, behavior: RibbonBehavior) -> Self {
        self.set_behavior(behavior);
        self
    }

    /// Sets the active tab by index, clamping to the last valid tab.
    pub const fn set_active_tab(&mut self, index: usize) {
        let max = self.tabs.len().saturating_sub(1);
        self.active_tab = if index > max { max } else { index };
    }

    /// Builder method: sets the active tab (clamped) and returns `self`.
    #[must_use]
    pub const fn with_active_tab(mut self, index: usize) -> Self {
        self.set_active_tab(index);
        self
    }

    /// Registers a callback invoked when a tab is clicked.
    pub fn set_on_tab_click<C: Into<RibbonOnTabClickCallback>>(&mut self, data: RefAny, cb: C) {
        self.on_tab_click = Some(RibbonOnTabClick {
            callback: cb.into(),
            refany: data,
        })
        .into();
    }

    /// Builder method: registers a tab-click callback and returns `self`.
    #[must_use]
    pub fn with_on_tab_click<C: Into<RibbonOnTabClickCallback>>(
        mut self,
        data: RefAny,
        cb: C,
    ) -> Self {
        self.set_on_tab_click(data, cb);
        self
    }

    /// Builds the ADAPTIVE ribbon DOM: both the desktop chrome (tab strip)
    /// and the touch chrome (full-width tab button, group list) live in the
    /// tree, and inline viewport conditions decide which is visible. Use
    /// this when one tree must serve every window size without re-running
    /// `layout()` logic.
    #[must_use]
    pub fn dom(self) -> Dom {
        self.build_chrome(RibbonChromeMode::Adaptive)
    }

    /// Builds ONLY the desktop chrome (tab strip + content band), with no
    /// mobile nodes and no viewport conditions. Pair with
    /// [`Self::dom_mobile`] by branching on
    /// `LayoutCallbackInfo::viewport_bigger_than` in `layout()` - the
    /// framework re-invokes `layout()` on every resize, so crossing the
    /// breakpoint swaps the structure.
    #[must_use]
    pub fn dom_desktop(self) -> Dom {
        self.build_chrome(RibbonChromeMode::Desktop)
    }

    /// Builds ONLY the touch chrome: the full-width active-tab button (tap
    /// opens the fullscreen tab picker, double-tap collapses the band), the
    /// scrollable group list on the dominant-hand side, and ONE visible
    /// group at a time (tapping a list entry swaps it in with no app
    /// relayout). See [`Self::dom_desktop`] for the pairing contract.
    #[must_use]
    pub fn dom_mobile(self) -> Dom {
        self.build_chrome(RibbonChromeMode::Mobile)
    }

    fn build_chrome(self, mode: RibbonChromeMode) -> Dom {
        let Self {
            app_button,
            tabs,
            active_tab,
            on_tab_click,
            style,
            behavior,
        } = self;
        let has_callback = on_tab_click.is_some();

        // Labels are needed by both chromes; `tabs` is consumed below.
        let tab_labels: Vec<AzString> = tabs.as_slice().iter().map(|t| t.label.clone()).collect();
        let group_labels: Vec<AzString> = tabs
            .as_slice()
            .get(active_tab)
            .map(|t| {
                t.groups
                    .as_slice()
                    .iter()
                    .map(|g| g.label.clone())
                    .collect()
            })
            .unwrap_or_default();

        let mut bar_children: Vec<Dom> = Vec::with_capacity(tabs.len() + 2);

        if let Some(ab) = app_button.into_option() {
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_APP_BUTTON))
                .with_css_props(style.app_button_style.clone())
                .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                    ab.label,
                )]));
            if let Some(oc) = ab.on_click.into_option() {
                d = d.with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: oc.callback.cb as *const () as usize,
                            ctx: oc.callback.ctx,
                        },
                        refany: oc.refany,
                    }]
                    .into(),
                );
            }
            bar_children.push(d);
        }

        // Private chrome state shared by every tab header: the collapse and
        // hover-peek behaviors are driven from here, so the application's own
        // data model never has to model ribbon chrome.
        let chrome = RefAny::new(RibbonChromeState { collapsed: false });

        for (idx, tab) in tabs.as_slice().iter().enumerate() {
            let (classes, part_style) = if idx == active_tab {
                (CLS_TAB_ACTIVE, style.tab_active_style.clone())
            } else {
                (CLS_TAB, style.tab_style.clone())
            };
            // Per-tab properties go AFTER the shared ones so they win, and
            // they are applied in both the active and inactive state.
            let part_style = if tab.style.as_ref().is_empty() {
                part_style
            } else {
                let mut merged = part_style.into_library_owned_vec();
                merged.extend(tab.style.as_ref().iter().cloned());
                CssPropertyWithConditionsVec::from_vec(merged)
            };
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
                .with_css_props(part_style)
                .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                    tab.label.clone(),
                )]));

            let mut cbs: Vec<CoreCallbackData> = Vec::with_capacity(4);
            if has_callback {
                cbs.push(CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Click),
                    callback: CoreCallback {
                        cb: on_ribbon_tab_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(TabClickData {
                        tab_idx: idx,
                        on_tab_click: on_tab_click.clone(),
                    }),
                });
            }
            if behavior.collapsible {
                cbs.push(CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                    callback: CoreCallback {
                        cb: on_ribbon_tab_double_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: chrome.clone(),
                });
                if behavior.peek_on_hover {
                    cbs.push(CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseEnter),
                        callback: CoreCallback {
                            cb: on_ribbon_tab_peek_enter as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: chrome.clone(),
                    });
                    cbs.push(CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseLeave),
                        callback: CoreCallback {
                            cb: on_ribbon_tab_peek_leave as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: chrome.clone(),
                    });
                }
            }
            if !cbs.is_empty() {
                d = d.with_callbacks(cbs.into());
            }
            bar_children.push(d);
        }

        bar_children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_TAB_FILLER))
                .with_css_props(style.tab_filler_style.clone()),
        );

        let tab_bar = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_TAB_BAR))
            .with_css_props(style.tab_bar_style.clone())
            .with_children(DomVec::from_vec(bar_children));

        let mut group_doms: Vec<Dom> =
            match tabs.into_library_owned_vec().into_iter().nth(active_tab) {
                Some(active) => active
                    .groups
                    .into_library_owned_vec()
                    .into_iter()
                    .map(|g| group_dom(g, &style, behavior))
                    .collect(),
                None => Vec::new(),
            };

        // Structural mobile chrome shows ONE group at a time; the group list
        // beside the content swaps them in (a runtime display patch - no app
        // relayout, same mechanism as the gallery panel).
        if matches!(mode, RibbonChromeMode::Mobile) {
            for (idx, g) in group_doms.iter_mut().enumerate() {
                if idx != 0 {
                    g.root
                        .upsert_inline_css_property(P::const_display(LayoutDisplay::None));
                }
            }
        }

        // ---- mobile chrome -------------------------------------------
        // Same tabs and groups, touch presentation. Both chromes live in the
        // tree and the viewport condition decides which is visible, so there
        // is no second widget tree and no state to keep in sync.
        let active_label = tab_labels
            .get(active_tab)
            .cloned()
            .unwrap_or_else(|| AzString::from_const_str(""));

        let mut mobile_tab_button = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_MOBILE_TAB_BUTTON))
            .with_css_props(style.mobile_tab_button_style.clone())
            .with_children(DomVec::from_vec(vec![
                crate::widgets::widget_p()
                    .with_css_props(style.mobile_tab_label_style.clone())
                    .with_children(DomVec::from_vec(vec![
                        Dom::create_text_do_not_use_without_block_level_wrapper(active_label),
                    ])),
                Dom::create_icon(AzString::from_const_str("expand_more"))
                    .with_css_props(style.mobile_tab_arrow_style.clone()),
            ]));

        let mut mobile_cbs: Vec<CoreCallbackData> = Vec::with_capacity(2);
        if behavior.mobile_tab_overlay {
            mobile_cbs.push(CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Click),
                callback: CoreCallback {
                    cb: on_ribbon_mobile_tab_click as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(MobileTabData { open: false }),
            });
        }
        // Double tap collapses the band, exactly like a desktop double click.
        if behavior.collapsible {
            mobile_cbs.push(CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                callback: CoreCallback {
                    cb: on_ribbon_tab_double_click as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: chrome.clone(),
            });
        }
        if !mobile_cbs.is_empty() {
            mobile_tab_button = mobile_tab_button.with_callbacks(mobile_cbs.into());
        }

        // Full-screen tab picker, hidden until the button opens it.
        let overlay_items: Vec<Dom> = tab_labels
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                let mut item = Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(
                        CLS_MOBILE_TAB_OVERLAY_ITEM,
                    ))
                    .with_css_props(style.mobile_tab_overlay_item_style.clone())
                    .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                        label.clone(),
                    )]));
                if has_callback {
                    item = item.with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::Click),
                            callback: CoreCallback {
                                cb: on_ribbon_tab_click as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(TabClickData {
                                tab_idx: idx,
                                on_tab_click: on_tab_click.clone(),
                            }),
                        }]
                        .into(),
                    );
                }
                item
            })
            .collect();
        let mobile_tab_overlay = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_MOBILE_TAB_OVERLAY))
            .with_css_props(style.mobile_tab_overlay_style.clone())
            .with_children(DomVec::from_vec(overlay_items));

        // Group list: on phones ONE group is visible and the rest are a
        // scrollable list on the dominant-hand side.
        let group_list_items: Vec<Dom> = group_labels
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                let item_style = if idx == 0 {
                    merged_style(
                        &style.mobile_group_list_item_style,
                        &style.mobile_group_list_item_selected_style,
                    )
                } else {
                    style.mobile_group_list_item_style.clone()
                };
                let mut item = Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(
                        CLS_MOBILE_GROUP_LIST_ITEM,
                    ))
                    .with_css_props(item_style)
                    .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                        label.clone(),
                    )]));
                if matches!(mode, RibbonChromeMode::Mobile) {
                    item = item.with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::Click),
                            callback: CoreCallback {
                                cb: on_ribbon_mobile_group_click as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(GroupListClickData {
                                group_idx: idx,
                                selected_style: style.mobile_group_list_item_selected_style.clone(),
                                base_style: style.mobile_group_list_item_style.clone(),
                            }),
                        }]
                        .into(),
                    );
                }
                item
            })
            .collect();
        let mobile_group_list = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_MOBILE_GROUP_LIST))
            .with_css_props(style.mobile_group_list_style.clone())
            .with_children(DomVec::from_vec(group_list_items));

        let content = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_CONTENT))
            .with_css_props(style.content_style.clone())
            .with_children(DomVec::from_vec(group_doms));

        // Structural modes pin the chrome's visibility unconditionally
        // (inline resolution is last-wins, so the appended value overrides
        // the baked viewport condition): the STRUCTURE is the breakpoint
        // switch, not the stylesheet.
        let children = match mode {
            RibbonChromeMode::Adaptive => vec![
                tab_bar,
                mobile_tab_button,
                mobile_tab_overlay,
                mobile_group_list,
                content,
            ],
            RibbonChromeMode::Desktop => {
                let mut tab_bar = tab_bar;
                tab_bar
                    .root
                    .upsert_inline_css_property(P::const_display(LayoutDisplay::Flex));
                vec![tab_bar, content]
            }
            RibbonChromeMode::Mobile => {
                let mut mobile_tab_button = mobile_tab_button;
                let mut mobile_group_list = mobile_group_list;
                mobile_tab_button
                    .root
                    .upsert_inline_css_property(P::const_display(LayoutDisplay::Flex));
                mobile_group_list
                    .root
                    .upsert_inline_css_property(P::const_display(LayoutDisplay::Flex));
                // The group list and the ONE visible group share a ROW band
                // ("scrollable list beside the content", the touch spec) -
                // structurally, because the container is a column. List side
                // = leading (right-handed default); a Handedness-driven
                // row-reverse via a future mobile_band_style field can flip
                // it without changing this structure.
                let band = Dom::create_div()
                    .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_MOBILE_BAND))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        Cond::simple(P::const_display(LayoutDisplay::Flex)),
                        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
                        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
                        cond_border_box(),
                    ]))
                    .with_children(DomVec::from_vec(vec![mobile_group_list, content]));
                vec![mobile_tab_button, mobile_tab_overlay, band]
            }
        };
        let mut container = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_RIBBON))
            .with_css_props(style.container_style)
            .with_children(DomVec::from_vec(children));
        // The chrome state (collapse flag) lives on the container as a
        // DATASET so it follows node identity across RefreshDom rebuilds
        // (see keep_old_ribbon_chrome). Without this, every rebuild reset
        // collapsed=false: double-clicking a tab collapsed the band and the
        // tab-click's own RefreshDom immediately forgot it. Only attached
        // when a chrome behavior is active - an inert ribbon has no chrome
        // state to persist (and `Dom` equality stays meaningful for it).
        if behavior.collapsible || behavior.peek_on_hover {
            container
                .root
                .set_dataset(azul_core::refany::OptionRefAny::Some(chrome));
            // The `as` is NOT trivial: it coerces the fn ITEM to a fn POINTER,
            // which is what `DatasetMergeCallback: From<...>` is implemented for.
            // Dropping it fails to satisfy the bound.
            #[allow(trivial_casts)]
            container.root.set_merge_callback(
                keep_old_ribbon_chrome as azul_core::dom::DatasetMergeCallbackType,
            );
        }
        container
    }
}

// -- DOM assembly helpers --

/// `base` with `extra` appended (inline CSS resolves last-wins, so `extra`
/// overrides `base` where they collide).
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

/// Expands ribbon button config to the existing [`Button`] widget with the
/// given part styles injected through `Button`'s public style fields.
fn styled_button(
    icon: AzString,
    label: AzString,
    trailing_icon: AzString,
    container_style: CssPropertyWithConditionsVec,
    icon_style: CssPropertyWithConditionsVec,
    label_style: CssPropertyWithConditionsVec,
    trailing_icon_style: CssPropertyWithConditionsVec,
    on_click: OptionButtonOnClick,
) -> Dom {
    let mut b = Button::create(label);
    b.icon = icon;
    b.trailing_icon = trailing_icon;
    b.container_style = container_style;
    b.icon_style = icon_style;
    b.label_style = label_style;
    b.trailing_icon_style = trailing_icon_style;
    b.on_click = on_click;
    b.dom()
}

fn expand_ribbon_button(rb: RibbonButton, large: bool, s: &RibbonStyle) -> Dom {
    let base = if large {
        &s.large_button_style
    } else {
        &s.small_button_style
    };
    let container = if rb.toggled {
        merged_style(base, &s.checked_style)
    } else {
        base.clone()
    };
    let trailing = match rb.arrow {
        RibbonArrow::None => AzString::from_const_str(""),
        RibbonArrow::Menu | RibbonArrow::Split => AzString::from_const_str("arrow_drop_down"),
    };
    let (icon_style, label_style) = if large {
        (s.large_icon_style.clone(), s.large_label_style.clone())
    } else {
        (s.small_icon_style.clone(), s.small_label_style.clone())
    };
    styled_button(
        rb.icon,
        rb.label,
        trailing,
        container,
        icon_style,
        label_style,
        s.arrow_icon_style.clone(),
        rb.on_click,
    )
}

fn item_dom(item: RibbonItem, s: &RibbonStyle, b: RibbonBehavior) -> Dom {
    match item {
        RibbonItem::LargeButton(rb) => expand_ribbon_button(rb, true, s),
        RibbonItem::SmallButton(rb) => expand_ribbon_button(rb, false, s),
        RibbonItem::Column(col) => Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_COLUMN))
            .with_css_props(s.column_style.clone())
            .with_children(DomVec::from_vec(
                col.items
                    .into_library_owned_vec()
                    .into_iter()
                    .map(|it| item_dom(it, s, b))
                    .collect(),
            )),
        RibbonItem::Row(row) => Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_ROW))
            .with_css_props(s.row_style.clone())
            .with_children(DomVec::from_vec(
                row.items
                    .into_library_owned_vec()
                    .into_iter()
                    .map(|it| item_dom(it, s, b))
                    .collect(),
            )),
        RibbonItem::Combo(combo) => combo.dom(),
        RibbonItem::Drop(drop) => drop.dom(),
        RibbonItem::Check(check) => check.dom(),
        RibbonItem::Gallery(gallery) => gallery_dom(gallery, s, b),
        RibbonItem::Separator => Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_SEPARATOR))
            .with_css_props(s.separator_style.clone()),
        RibbonItem::Custom(dom) => dom,
    }
}

/// Appended to the group style when [`RibbonGroup::fills_space`] is set:
/// the group absorbs leftover width AND yields it under pressure, down to
/// an explicit floor. The explicit `min-width` is load-bearing — it
/// replaces the flex automatic minimum size, which taffy 0.10 does not
/// collapse across the nested group > items > gallery-frame chain (see
/// `layout/tests/flex_intrinsic_text.rs`).
static GROUP_FILL_STYLE: &[Cond] = &[
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(1),
    })),
    Cond::simple(P::const_min_width(LayoutMinWidth::const_px(160))),
];

fn group_dom(group: RibbonGroup, s: &RibbonStyle, b: RibbonBehavior) -> Dom {
    let RibbonGroup {
        label,
        items,
        launcher,
        fills_space,
    } = group;

    let item_doms: Vec<Dom> = items
        .into_library_owned_vec()
        .into_iter()
        .map(|it| item_dom(it, s, b))
        .collect();

    let items_row = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_ITEMS))
        .with_css_props(s.group_items_style.clone())
        .with_children(DomVec::from_vec(item_doms));

    let has_launcher = launcher.is_some();
    let mut footer_children: Vec<Dom> = Vec::with_capacity(3);
    if has_launcher {
        // Balances the launcher's width so the caption stays centered.
        footer_children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_FOOTER_SPACER))
                .with_css_props(s.footer_spacer_style.clone()),
        );
    }
    footer_children.push(
        crate::widgets::widget_p()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_LABEL))
            .with_css_props(s.group_label_style.clone())
            .with_children(DomVec::from_vec(vec![
                Dom::create_text_do_not_use_without_block_level_wrapper(label),
            ])),
    );
    if let Some(l) = launcher.into_option() {
        footer_children.push(styled_button(
            AzString::from_const_str("south_east"),
            AzString::from_const_str(""),
            AzString::from_const_str(""),
            s.launcher_button_style.clone(),
            s.launcher_icon_style.clone(),
            s.small_label_style.clone(),
            s.arrow_icon_style.clone(),
            Some(l).into(),
        ));
    }
    let footer = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_FOOTER))
        .with_css_props(s.group_footer_style.clone())
        .with_children(DomVec::from_vec(footer_children));

    let group_style = if fills_space {
        merged_style(
            &s.group_style,
            &CssPropertyWithConditionsVec::from_const_slice(GROUP_FILL_STYLE),
        )
    } else {
        s.group_style.clone()
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP))
        .with_css_props(group_style)
        .with_children(DomVec::from_vec(vec![items_row, footer]))
}

fn gallery_dom(gallery: RibbonGallery, s: &RibbonStyle, b: RibbonBehavior) -> Dom {
    let RibbonGallery {
        cells,
        selected,
        on_select,
    } = gallery;
    let has_callback = on_select.is_some();
    let cells = cells.into_library_owned_vec();

    // The cells are built twice: once for the in-ribbon strip and once for
    // the expansion panel, so "More" can show every cell without a relayout.
    let build_cells = |in_panel: bool| -> Vec<Dom> {
        let mut out: Vec<Dom> = Vec::with_capacity(cells.len());
        for (idx, cell) in cells.iter().enumerate() {
            let (classes, cell_style) = if idx == selected {
                (
                    CLS_GALLERY_CELL_SELECTED,
                    merged_style(&s.gallery_cell_style, &s.gallery_cell_selected_style),
                )
            } else {
                (CLS_GALLERY_CELL, s.gallery_cell_style.clone())
            };
            let label = crate::widgets::widget_p()
                .with_css_props(s.gallery_cell_label_style.clone())
                .with_children(DomVec::from_vec(vec![
                    Dom::create_text_do_not_use_without_block_level_wrapper(cell.label.clone()),
                ]));
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
                .with_css_props(cell_style)
                .with_children(DomVec::from_vec(vec![cell.preview.clone(), label]));
            if has_callback || b.auto_select_gallery {
                d = d.with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: on_ribbon_gallery_cell_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(GalleryCellClickData {
                            cell_idx: idx,
                            on_select: on_select.clone(),
                            auto_select: b.auto_select_gallery,
                            in_panel,
                            selected_style: s.gallery_cell_selected_style.clone(),
                            base_style: s.gallery_cell_style.clone(),
                        }),
                    }]
                    .into(),
                );
            }
            out.push(d);
        }
        out
    };

    let strip = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_STRIP))
        .with_css_props(s.gallery_strip_style.clone())
        .with_children(DomVec::from_vec(build_cells(false)));

    // Spinner column: scroll-up, scroll-down, and the "More" button that
    // toggles the expansion panel (the classic office-suite "More" chevron-over-bar).
    let spinner_icons = ["expand_less", "expand_more", "arrow_drop_down"];
    let spinner_buttons: Vec<Dom> = spinner_icons
        .iter()
        .enumerate()
        .map(|(i, icon)| {
            let mut btn = styled_button(
                AzString::from(*icon),
                AzString::from_const_str(""),
                AzString::from_const_str(""),
                s.gallery_spinner_button_style.clone(),
                s.gallery_spinner_icon_style.clone(),
                s.small_label_style.clone(),
                s.arrow_icon_style.clone(),
                OptionButtonOnClick::None,
            );
            // The third button is "More": it expands the panel.
            if i == 2 && b.expandable_gallery {
                btn = btn.with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_MORE));
                btn = btn.with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: on_ribbon_gallery_more_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(GalleryMoreData { open: false }),
                    }]
                    .into(),
                );
            }
            btn
        })
        .collect();

    let spinner = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_SPINNER))
        .with_css_props(s.gallery_spinner_style.clone())
        .with_children(DomVec::from_vec(spinner_buttons));

    let frame = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY))
        .with_css_props(s.gallery_frame_style.clone())
        .with_children(DomVec::from_vec(vec![strip, spinner]));

    if !b.expandable_gallery {
        return frame;
    }

    // The expansion panel is an absolutely-positioned wrapped grid of every
    // cell, hidden until "More" is clicked (the popover/combobox pattern).
    let panel = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_PANEL))
        .with_css_props(s.gallery_panel_style.clone())
        .with_children(DomVec::from_vec(build_cells(true)));

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_WRAPPER))
        .with_css_props(s.gallery_wrapper_style.clone())
        .with_children(DomVec::from_vec(vec![frame, panel]))
}

// -- Trampolines --

/// Which chrome [`Ribbon::dom`]-family builder emits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RibbonChromeMode {
    /// Both chromes in one tree; inline viewport conditions pick one.
    Adaptive,
    /// Desktop chrome only (tab strip + content).
    Desktop,
    /// Touch chrome only (tab button + overlay + group list + one group).
    Mobile,
}

/// Per-list-entry payload for the mobile group switcher.
struct GroupListClickData {
    group_idx: usize,
    selected_style: CssPropertyWithConditionsVec,
    base_style: CssPropertyWithConditionsVec,
}

/// Tapping a group-list entry shows THAT group in the content band and moves
/// the highlight - a runtime display patch, no app relayout (the same
/// chokepoint mechanism the gallery panel uses). Targets resolve BY CLASS
/// from the ribbon container, per the widget convention.
extern "C" fn on_ribbon_mobile_group_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(mut data) = refany.downcast_mut::<GroupListClickData>() else {
        return Update::DoNothing;
    };
    let group_idx = data.group_idx;
    let selected_style = data.selected_style.clone();
    let base_style = data.base_style.clone();
    drop(data);

    let Some(ribbon) = ancestor_with_class(&info, hit, RIBBON_CONTAINER_CLASS) else {
        return Update::DoNothing;
    };

    // Swap the visible group inside the content band (the content sits
    // INSIDE the mobile band wrapper, so resolve by descendant search).
    if let Some(content) = descendant_with_class(&info, ribbon, RIBBON_CONTENT_CLASS) {
        let mut group = info.get_first_child(content);
        let mut idx = 0_usize;
        while let Some(g) = group {
            let display = if idx == group_idx {
                LayoutDisplay::Flex
            } else {
                LayoutDisplay::None
            };
            info.set_css_property(g, P::const_display(display));
            group = info.get_next_sibling(g);
            idx += 1;
        }
    }

    // Move the highlight along the list (unconditional props only, like the
    // gallery cell highlight). For a DE-selected entry, property types the
    // selected style sets but the base style does not are reset to Initial -
    // set_css_property(Initial) REMOVES the runtime override, so the entry
    // falls back to its inline style instead of keeping the stale highlight.
    let item = ancestor_with_class(&info, hit, MOBILE_GROUP_LIST_ITEM_CLASS).unwrap_or(hit);
    if let Some(list) = info.get_parent(item) {
        let mut sibling = info.get_first_child(list);
        while let Some(entry) = sibling {
            if entry == item {
                for prop in selected_style.as_ref() {
                    if prop.apply_if.as_ref().is_empty() {
                        info.set_css_property(entry, prop.property.clone());
                    }
                }
            } else {
                for prop in base_style.as_ref() {
                    if prop.apply_if.as_ref().is_empty() {
                        info.set_css_property(entry, prop.property.clone());
                    }
                }
                for prop in selected_style.as_ref() {
                    if !prop.apply_if.as_ref().is_empty() {
                        continue;
                    }
                    let ty = prop.property.get_type();
                    let in_base = base_style
                        .as_ref()
                        .iter()
                        .any(|b| b.apply_if.as_ref().is_empty() && b.property.get_type() == ty);
                    if !in_base {
                        info.set_css_property(entry, props::property::CssProperty::initial(ty));
                    }
                }
            }
            sibling = info.get_next_sibling(entry);
        }
    }

    Update::DoNothing
}

/// Dataset merge for the ribbon container: chrome state (collapse) must
/// survive app-driven rebuilds (any callback returning `RefreshDom` - the
/// ribbon's own tab switch does), so keep the OLD allocation wholesale.
/// `diff::transfer_states` then re-points every tab callback refany (they
/// are clones of this dataset) onto the kept allocation, so the handlers
/// keep reading the persistent state with no further wiring.
extern "C" fn keep_old_ribbon_chrome(_new: RefAny, old: RefAny) -> RefAny {
    old
}

struct TabClickData {
    tab_idx: usize,
    on_tab_click: OptionRibbonOnTabClick,
}

extern "C" fn on_ribbon_tab_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    let Some(mut data) = refany.downcast_mut::<TabClickData>() else {
        return Update::DoNothing;
    };
    let idx = data.tab_idx;
    match data.on_tab_click.as_mut() {
        Some(RibbonOnTabClick { refany, callback }) => (callback.cb)(refany.clone(), info, idx),
        None => Update::DoNothing,
    }
}

/// Private chrome state for the collapse / hover-peek behaviors. Lives in a
/// `RefAny` minted inside [`Ribbon::dom`] and shared by every tab header, so
/// the ribbon can drive its own chrome without any application state.
struct RibbonChromeState {
    collapsed: bool,
}

/// The content band is the tab bar's next sibling; from a tab header that is
/// `parent(tab) -> next_sibling`.
/// The content band of the ribbon that owns `hit`'s tab header, resolved BY
/// CLASS: walk up to the ribbon container, then scan its children for the
/// content class. Positional navigation (`next_sibling(parent(tab))`) broke
/// the moment the container grew more children — the mobile chrome sits
/// between the tab bar and the content band, so a double-click "collapsed"
/// the (already hidden) mobile tab button while the content stayed visible.
fn content_node_of_tab(info: &CallbackInfo, hit: DomNodeId) -> Option<DomNodeId> {
    let tab = ancestor_with_class(info, hit, RIBBON_TAB_CLASS)
        .or_else(|| ancestor_with_class(info, hit, MOBILE_TAB_BUTTON_CLASS))?;
    let ribbon = ancestor_with_class(info, tab, RIBBON_CONTAINER_CLASS)?;
    descendant_with_class(info, ribbon, RIBBON_CONTENT_CLASS)
}

fn set_content_visible(info: &mut CallbackInfo, content: DomNodeId, visible: bool) {
    let display = if visible {
        LayoutDisplay::Flex
    } else {
        LayoutDisplay::None
    };
    info.set_css_property(content, P::const_display(display));
}

/// Double-click on a tab header toggles the collapsed state of the content
/// band (the classic office-suite "Collapse the Ribbon").
extern "C" fn on_ribbon_tab_double_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(content) = content_node_of_tab(&info, hit) else {
        return Update::DoNothing;
    };
    let Some(mut state) = refany.downcast_mut::<RibbonChromeState>() else {
        return Update::DoNothing;
    };
    state.collapsed = !state.collapsed;
    let collapsed = state.collapsed;
    drop(state);
    set_content_visible(&mut info, content, !collapsed);
    Update::DoNothing
}

/// While collapsed, hovering a tab header peeks the content band.
extern "C" fn on_ribbon_tab_peek_enter(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(content) = content_node_of_tab(&info, hit) else {
        return Update::DoNothing;
    };
    let Some(state) = refany.downcast_ref::<RibbonChromeState>() else {
        return Update::DoNothing;
    };
    let collapsed = state.collapsed;
    drop(state);
    if collapsed {
        set_content_visible(&mut info, content, true);
    }
    Update::DoNothing
}

/// Leaving the tab header hides the peeked band again.
extern "C" fn on_ribbon_tab_peek_leave(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(content) = content_node_of_tab(&info, hit) else {
        return Update::DoNothing;
    };
    let Some(state) = refany.downcast_ref::<RibbonChromeState>() else {
        return Update::DoNothing;
    };
    let collapsed = state.collapsed;
    drop(state);
    if collapsed {
        set_content_visible(&mut info, content, false);
    }
    Update::DoNothing
}

/// Per-mobile-tab-button state: whether the tab overlay is open.
struct MobileTabData {
    open: bool,
}

/// The mobile tab button opens/closes the full-screen tab picker, which is
/// its next sibling in the ribbon container.
extern "C" fn on_ribbon_mobile_tab_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(button) = ancestor_with_class(&info, hit, MOBILE_TAB_BUTTON_CLASS) else {
        return Update::DoNothing;
    };
    let Some(overlay) = info.get_next_sibling(button) else {
        return Update::DoNothing;
    };
    let Some(mut data) = refany.downcast_mut::<MobileTabData>() else {
        return Update::DoNothing;
    };
    data.open = !data.open;
    let open = data.open;
    drop(data);
    info.set_css_property(
        overlay,
        P::const_display(if open {
            LayoutDisplay::Flex
        } else {
            LayoutDisplay::None
        }),
    );
    Update::DoNothing
}

/// Per-"More"-button state: whether the expansion panel is open.
struct GalleryMoreData {
    open: bool,
}

/// Walks up from `start` (inclusive) to the first ancestor carrying `class`.
///
/// Hit nodes are not stable: a click on a button can report the button or
/// the icon/label node inside it, and widgets may gain wrapper levels. So
/// the ribbon's handlers locate their targets by CLASS rather than by
/// counting `get_parent` hops - the same identifiers the public CSS API is
/// built on. The walk is bounded so a malformed tree cannot spin.
fn ancestor_with_class(info: &CallbackInfo, start: DomNodeId, class: &str) -> Option<DomNodeId> {
    let mut current = Some(start);
    for _ in 0..16 {
        let node = current?;
        if info
            .get_node_classes(node)
            .as_ref()
            .iter()
            .any(|c| c.as_str() == class)
        {
            return Some(node);
        }
        current = info.get_parent(node);
    }
    None
}

/// Breadth-first search for the first descendant of `root` carrying `class`,
/// bounded to 64 visited nodes. The ribbon's structural chromes nest parts
/// at different depths (the mobile band wraps group list + content), so
/// class resolution must not assume direct children.
fn descendant_with_class(info: &CallbackInfo, root: DomNodeId, class: &str) -> Option<DomNodeId> {
    let mut queue: Vec<DomNodeId> = Vec::with_capacity(8);
    let mut child = info.get_first_child(root);
    while let Some(n) = child {
        queue.push(n);
        child = info.get_next_sibling(n);
    }
    let mut visited = 0_usize;
    let mut i = 0_usize;
    while i < queue.len() && visited < 64 {
        let node = queue[i];
        i += 1;
        visited += 1;
        if info
            .get_node_classes(node)
            .as_ref()
            .iter()
            .any(|cl| cl.as_str() == class)
        {
            return Some(node);
        }
        let mut child = info.get_first_child(node);
        while let Some(n) = child {
            queue.push(n);
            child = info.get_next_sibling(n);
        }
    }
    None
}

/// The "More" button toggles the expansion panel (the gallery wrapper's
/// last child).
extern "C" fn on_ribbon_gallery_more_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(wrapper) = ancestor_with_class(&info, hit, GALLERY_WRAPPER_CLASS) else {
        return Update::DoNothing;
    };
    let Some(panel) = info.get_last_child(wrapper) else {
        return Update::DoNothing;
    };
    let Some(mut data) = refany.downcast_mut::<GalleryMoreData>() else {
        return Update::DoNothing;
    };
    data.open = !data.open;
    let open = data.open;
    drop(data);
    let display = if open {
        LayoutDisplay::Flex
    } else {
        LayoutDisplay::None
    };
    info.set_css_property(panel, P::const_display(display));
    Update::DoNothing
}

struct GalleryCellClickData {
    cell_idx: usize,
    on_select: OptionRibbonGalleryOnSelect,
    /// Move the selection highlight without an app relayout.
    auto_select: bool,
    /// Cells in the expansion panel also close the panel when picked.
    in_panel: bool,
    selected_style: CssPropertyWithConditionsVec,
    base_style: CssPropertyWithConditionsVec,
}

extern "C" fn on_ribbon_gallery_cell_click(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(mut data) = refany.downcast_mut::<GalleryCellClickData>() else {
        return Update::DoNothing;
    };
    let idx = data.cell_idx;
    let auto_select = data.auto_select;
    let in_panel = data.in_panel;
    let selected_style = data.selected_style.clone();
    let base_style = data.base_style.clone();
    let user = data.on_select.clone();
    drop(data);

    // Default behavior: move the highlight to the clicked cell immediately,
    // so the gallery feels live even if the app does not re-render. The hit
    // node may be the cell's preview or label, so resolve the cell by class.
    let cell = ancestor_with_class(&info, hit, GALLERY_CELL_CLASS).unwrap_or(hit);
    if auto_select {
        if let Some(strip) = info.get_parent(cell) {
            let mut sibling = info.get_first_child(strip);
            while let Some(cell_node) = sibling {
                let style = if cell_node == cell {
                    &selected_style
                } else {
                    &base_style
                };
                for prop in style.as_ref() {
                    if prop.apply_if.as_ref().is_empty() {
                        info.set_css_property(cell_node, prop.property.clone());
                    }
                }
                sibling = info.get_next_sibling(cell_node);
            }
        }
        // Picking from the expansion panel closes it.
        if in_panel {
            if let Some(panel) = info.get_parent(cell) {
                info.set_css_property(panel, P::const_display(LayoutDisplay::None));
            }
        }
    }

    match user.into_option() {
        Some(RibbonGalleryOnSelect { refany, callback }) => (callback.cb)(refany, info, idx),
        None => Update::DoNothing,
    }
}

impl From<Ribbon> for Dom {
    fn from(r: Ribbon) -> Self {
        r.dom()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::{props::property::CssProperty, system::SystemStyle};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        window::LayoutWindow,
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

    /// Text of a label node, looking through the `<p>` block wrapper the
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

    /// USER convention (2026-08-12): widget-emitted text is never a raw
    /// `create_text` child — every label is `<p>` wrapping exactly one text
    /// node. Raw text as a direct flex child takes the anonymous-box path
    /// that made "PAGE LAYOUT" wrap and the group captions de-center live.
    /// Walks the full desktop + mobile chrome (tabs, captions, buttons,
    /// gallery, launcher) and flags every Text node under a non-P parent.
    #[test]
    fn every_ribbon_label_is_block_formatted_no_raw_text_children() {
        extern "C" fn noop_launcher_click(_data: RefAny, _info: CallbackInfo) -> Update {
            Update::DoNothing
        }
        fn walk(node: &Dom, parent_is_p: bool, bad: &mut Vec<String>) {
            if let NodeType::Text(t) = node.root.get_node_type() {
                if !parent_is_p {
                    bad.push(t.as_ref().as_str().to_string());
                }
            }
            // An icon's text leaf is the glyph slot icon resolution fills
            // (the icon becomes an inline <span> around it): by convention.
            let is_p = matches!(node.root.get_node_type(), NodeType::P | NodeType::Icon(_));
            for c in node.children.as_ref() {
                walk(c, is_p, bad);
            }
        }
        let cells = vec![RibbonGalleryCell::new(
            Dom::create_div(), // user preview content — exempt from the convention
            "Style 0".into(),
        )];
        let tab = RibbonTab::new("HOME".into())
            .with_group(
                RibbonGroup::new("Clipboard".into())
                    .with_item(RibbonItem::LargeButton(RibbonButton::new(
                        "content_paste".into(),
                        "Paste".into(),
                    )))
                    .with_launcher(
                        RefAny::new(0usize),
                        noop_launcher_click as crate::widgets::button::ButtonOnClickCallbackType,
                    ),
            )
            .with_group(
                RibbonGroup::new("Styles".into())
                    .with_item(RibbonItem::Gallery(RibbonGallery::new(cells.into()))),
            );
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![
            tab,
            RibbonTab::new("PAGE LAYOUT".into()),
        ]))
        .with_app_button(RibbonAppButton::new("FILE".into()))
        .dom();
        let mut bad = Vec::new();
        walk(&dom, false, &mut bad);
        assert!(
            bad.is_empty(),
            "raw text nodes outside a <p> wrapper: {bad:?}"
        );
    }

    /// The text of a box's single label child (tabs / app button).
    fn label_text(node: &Dom) -> Option<&str> {
        node.children.as_ref().first().and_then(text_of)
    }

    fn icon_name_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Icon(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    fn inline_props(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    fn style_props(style: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        style.as_ref().iter().map(|c| c.property.clone()).collect()
    }

    fn recursive_descendants(node: &Dom) -> usize {
        node.children
            .as_ref()
            .iter()
            .map(|c| 1 + recursive_descendants(c))
            .sum()
    }

    /// `(tab bar, content)` of a rendered ribbon DOM.
    ///
    /// The root also carries the mobile chrome (tab button, tab overlay,
    /// group list), which the viewport conditions hide on desktop - so the
    /// parts are located by CLASS, not by index.
    fn parts(dom: &Dom) -> (&Dom, &Dom) {
        let by_class = |name: &str| {
            dom.children
                .as_ref()
                .iter()
                .find(|c| has_class(c, name))
                .unwrap_or_else(|| panic!("a ribbon DOM has a {name} child"))
        };
        (
            by_class("__azul-native-ribbon-tabbar"),
            by_class("__azul-native-ribbon-content"),
        )
    }

    /// `(items row, footer)` of the `n`-th rendered group.
    fn group_parts(content: &Dom, n: usize) -> (&Dom, &Dom) {
        let group = &content.children.as_ref()[n];
        let ch = group.children.as_ref();
        assert_eq!(ch.len(), 2, "a group is exactly [items, footer]");
        (&ch[0], &ch[1])
    }

    fn tabs(n: usize) -> RibbonTabVec {
        let mut v = Vec::with_capacity(n);
        for i in 0..n {
            v.push(RibbonTab::new(AzString::from(format!("t{i}"))));
        }
        RibbonTabVec::from_vec(v)
    }

    fn small_btn(icon: &str, label: &str) -> RibbonButton {
        RibbonButton::new(AzString::from(icon), AzString::from(label))
    }

    struct IndexLog {
        seen: Vec<usize>,
    }

    extern "C" fn record_index(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
        if let Some(mut log) = data.downcast_mut::<IndexLog>() {
            log.seen.push(index);
        }
        Update::RefreshDom
    }

    fn log_indices(data: &mut RefAny) -> Vec<usize> {
        data.downcast_ref::<IndexLog>()
            .expect("payload must still be an IndexLog")
            .seen
            .clone()
    }

    /// Invokes `cb` (a ribbon trampoline) with a minimal `CallbackInfo`. The
    /// trampolines never read the DOM, so the `LayoutWindow` holds no layout
    /// results - if they ever start touching them, these tests notice.
    fn run_trampoline(
        cb: extern "C" fn(RefAny, CallbackInfo) -> Update,
        data: RefAny,
    ) -> (Update, Vec<CallbackChange>) {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");

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
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = cb(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    // ------------------------------------------------------------------
    // Constructors and invariants
    // ------------------------------------------------------------------

    #[test]
    fn ribbon_new_defaults_to_office_2013_with_tab_zero_active() {
        for count in [0usize, 1, 2, 9] {
            let r = Ribbon::new(tabs(count));
            assert_eq!(r.tabs.len(), count);
            assert_eq!(r.active_tab, 0);
            assert!(r.on_tab_click.is_none());
            assert!(r.app_button.is_none());
            assert_eq!(r.style, RibbonStyle::office_2013());
        }
    }

    #[test]
    fn ribbon_style_default_is_office_2013() {
        assert_eq!(RibbonStyle::default(), RibbonStyle::office_2013());
    }

    #[test]
    fn set_active_tab_clamps_to_the_last_valid_index() {
        let mut r = Ribbon::new(RibbonTabVec::from_vec(Vec::new()));
        for index in [0usize, 1, usize::MAX / 2, usize::MAX] {
            r.set_active_tab(index);
            assert_eq!(r.active_tab, 0, "empty ribbon must clamp {index} to 0");
        }

        let mut r = Ribbon::new(tabs(4));
        for index in 0..4 {
            r.set_active_tab(index);
            assert_eq!(r.active_tab, index);
        }
        for index in [4usize, 5, usize::MAX - 1, usize::MAX] {
            r.set_active_tab(index);
            assert_eq!(r.active_tab, 3, "{index} must clamp to the last tab");
        }
    }

    #[test]
    fn group_and_tab_builders_append_in_order() {
        let group = RibbonGroup::new(AzString::from("Font"))
            .with_item(RibbonItem::SmallButton(small_btn("format_bold", "")))
            .with_item(RibbonItem::Separator);
        assert_eq!(group.items.len(), 2);
        assert!(group.launcher.is_none());

        let tab = RibbonTab::new(AzString::from("HOME"))
            .with_group(group.clone())
            .with_group(RibbonGroup::new(AzString::from("Editing")));
        assert_eq!(tab.groups.len(), 2);
        assert_eq!(tab.groups.as_ref()[0].label.as_str(), "Font");
        assert_eq!(tab.groups.as_ref()[1].label.as_str(), "Editing");
    }

    // ------------------------------------------------------------------
    // Tab bar
    // ------------------------------------------------------------------

    #[test]
    fn dom_of_an_empty_ribbon_has_only_the_filler_in_the_bar() {
        let dom = Ribbon::new(RibbonTabVec::from_vec(Vec::new())).dom();
        assert!(has_class(&dom, "__azul-native-ribbon"));

        let (bar, content) = parts(&dom);
        assert!(has_class(bar, "__azul-native-ribbon-tabbar"));
        assert_eq!(
            bar.children.as_ref().len(),
            1,
            "empty ribbon bar = filler only"
        );
        assert!(has_class(
            &bar.children.as_ref()[0],
            "__azul-native-ribbon-tab-filler"
        ));
        assert!(content.children.as_ref().is_empty());
    }

    #[test]
    fn dom_desktop_emits_only_the_desktop_chrome() {
        let r = Ribbon::new(tabs(3));
        let dom = r.dom_desktop();
        let ch = dom.children.as_ref();
        assert_eq!(ch.len(), 2, "desktop chrome = [tab bar, content]");
        assert!(has_class(&ch[0], "__azul-native-ribbon-tabbar"));
        assert!(has_class(&ch[1], "__azul-native-ribbon-content"));
    }

    #[test]
    fn dom_mobile_emits_only_the_touch_chrome_with_one_visible_group() {
        use azul_css::props::property::CssPropertyType;

        let mut tab = RibbonTab::new(AzString::from_const_str("HOME"));
        for label in ["Clipboard", "Font", "Paragraph"] {
            tab = tab.with_group(RibbonGroup::new(AzString::from(label)));
        }
        let r = Ribbon::new(RibbonTabVec::from_vec(vec![tab]));
        let dom = r.dom_mobile();
        let ch = dom.children.as_ref();
        assert_eq!(ch.len(), 3, "mobile chrome = [tab button, overlay, band]");
        assert!(has_class(&ch[0], "__azul-native-ribbon-mobile-tab"));
        assert!(has_class(&ch[1], "__azul-native-ribbon-mobile-tab-overlay"));
        assert!(has_class(&ch[2], "__azul-native-ribbon-mobile-band"));
        let band = ch[2].children.as_ref();
        assert_eq!(band.len(), 2, "band = [group list, content] side by side");
        assert!(has_class(
            &band[0],
            "__azul-native-ribbon-mobile-group-list"
        ));
        assert!(has_class(&band[1], "__azul-native-ribbon-content"));

        // Exactly the FIRST group is visible; the others carry an appended
        // unconditional display:none (the group list swaps them at runtime).
        let last_uncond_display = |d: &Dom| {
            d.root
                .style
                .iter_inline_properties()
                .filter(|(p, conds)| {
                    conds.as_ref().is_empty() && p.get_type() == CssPropertyType::Display
                })
                .last()
                .map(|(p, _)| p.clone())
        };
        let groups = ch[2].children.as_ref()[1].children.as_ref();
        assert_eq!(groups.len(), 3);
        assert_ne!(
            last_uncond_display(&groups[0]),
            Some(P::const_display(LayoutDisplay::None)),
            "first group stays visible"
        );
        for g in &groups[1..] {
            assert_eq!(
                last_uncond_display(g),
                Some(P::const_display(LayoutDisplay::None)),
                "non-initial groups start hidden in the mobile chrome"
            );
        }

        // Every group-list entry carries the swap callback.
        for item in ch[2].children.as_ref()[0].children.as_ref() {
            assert_eq!(
                item.root.callbacks.as_ref().len(),
                1,
                "group-list entry has the swap callback"
            );
        }
    }

    #[test]
    fn dom_renders_app_button_tabs_and_filler_in_order() {
        let r = Ribbon::new(tabs(3))
            .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
            .with_active_tab(1);
        let dom = r.dom();
        let (bar, _) = parts(&dom);
        let ch = bar.children.as_ref();

        assert_eq!(ch.len(), 5, "[app, t0, t1, t2, filler]");
        assert!(has_class(&ch[0], "__azul-native-ribbon-appbutton"));
        // The app button and the tabs are BOXES holding a label text child
        // (a raw text node is an inline box, whose border paints around the
        // text run instead of the padded tab).
        assert_eq!(label_text(&ch[0]), Some("FILE"));
        for i in 0..3 {
            assert_eq!(label_text(&ch[1 + i]), Some(format!("t{i}").as_str()));
            assert!(has_class(&ch[1 + i], "__azul-native-ribbon-tab"));
            assert_eq!(
                has_class(&ch[1 + i], "__azul-native-ribbon-tab-active"),
                i == 1,
                "only tab 1 is active"
            );
        }
        assert!(has_class(&ch[4], "__azul-native-ribbon-tab-filler"));

        // the active tab carries the active style, the others the plain style
        let s = RibbonStyle::office_2013();
        assert_eq!(inline_props(&ch[1]), style_props(&s.tab_style));
        assert_eq!(inline_props(&ch[2]), style_props(&s.tab_active_style));
    }

    #[test]
    fn dom_with_an_out_of_range_active_tab_highlights_nothing_and_renders_no_groups() {
        let mut r = Ribbon::new(tabs(3));
        r.active_tab = usize::MAX; // public field bypasses the clamp
        let dom = r.dom();
        let (bar, content) = parts(&dom);
        for tab in &bar.children.as_ref()[..3] {
            assert!(!has_class(tab, "__azul-native-ribbon-tab-active"));
        }
        assert!(content.children.as_ref().is_empty());
    }

    #[test]
    fn dom_without_a_callback_attaches_no_user_tab_handler() {
        let dom = Ribbon::new(tabs(4)).dom();
        let (bar, _) = parts(&dom);
        for tab in bar.children.as_ref() {
            assert!(
                !tab.root
                    .get_callbacks()
                    .as_ref()
                    .iter()
                    .any(|c| c.event == EventFilter::Hover(HoverEventFilter::Click)),
                "no user callback -> no MouseUp handler (chrome handlers may still be present)"
            );
        }
    }

    #[test]
    fn dom_gives_every_tab_a_mouseup_callback_with_its_own_index() {
        let dom = Ribbon::new(tabs(5))
            .with_on_tab_click(
                RefAny::new(IndexLog { seen: Vec::new() }),
                record_index as RibbonOnTabClickCallbackType,
            )
            .dom();
        let (bar, _) = parts(&dom);
        for (i, tab) in bar.children.as_ref()[..5].iter().enumerate() {
            let cbs = tab.root.get_callbacks();
            // Default behavior also attaches the collapse/peek chrome
            // handlers; the USER callback is the MouseUp one.
            let click = cbs
                .as_ref()
                .iter()
                .find(|c| c.event == EventFilter::Hover(HoverEventFilter::Click))
                .expect("every tab has a MouseUp user handler");
            let mut payload = click.refany.clone();
            let data = payload
                .downcast_ref::<TabClickData>()
                .expect("tab payload is a TabClickData");
            assert_eq!(data.tab_idx, i);
        }
        // the filler has no callback
        assert!(bar.children.as_ref()[5]
            .root
            .get_callbacks()
            .as_ref()
            .is_empty());
    }

    #[test]
    fn app_button_callback_is_attached_directly() {
        extern "C" fn noop(_: RefAny, _: CallbackInfo) -> Update {
            Update::DoNothing
        }
        let ab = RibbonAppButton::new(AzString::from("FILE")).with_on_click(
            RefAny::new(0u8),
            noop as super::super::button::ButtonOnClickCallbackType,
        );
        let dom = Ribbon::new(tabs(1)).with_app_button(ab).dom();
        let (bar, _) = parts(&dom);
        let cbs = bar.children.as_ref()[0].root.get_callbacks();
        assert_eq!(cbs.as_ref().len(), 1);
        assert_eq!(cbs.as_ref()[0].callback.cb, noop as usize);
    }

    // ------------------------------------------------------------------
    // Trampolines
    // ------------------------------------------------------------------

    #[test]
    fn tab_click_forwards_the_index_and_propagates_the_update() {
        let mut log = RefAny::new(IndexLog { seen: Vec::new() });
        for idx in [0usize, 7, usize::MAX] {
            let data = RefAny::new(TabClickData {
                tab_idx: idx,
                on_tab_click: Some(RibbonOnTabClick {
                    callback: (record_index as RibbonOnTabClickCallbackType).into(),
                    refany: log.clone(),
                })
                .into(),
            });
            let (update, changes) = run_trampoline(on_ribbon_tab_click, data);
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }
        assert_eq!(log_indices(&mut log), vec![0, 7, usize::MAX]);
    }

    #[test]
    fn gallery_click_forwards_the_index_and_propagates_the_update() {
        let mut log = RefAny::new(IndexLog { seen: Vec::new() });
        let data = RefAny::new(GalleryCellClickData {
            cell_idx: 3,
            on_select: Some(RibbonGalleryOnSelect {
                callback: (record_index as RibbonGalleryOnSelectCallbackType).into(),
                refany: log.clone(),
            })
            .into(),
            // The auto-select branch needs live layout results; this test
            // drives the forwarding path only.
            auto_select: false,
            in_panel: false,
            selected_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
            base_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
        });
        let (update, changes) = run_trampoline(on_ribbon_gallery_cell_click, data);
        assert_eq!(update, Update::RefreshDom);
        assert!(changes.is_empty());
        assert_eq!(log_indices(&mut log), vec![3]);
    }

    #[test]
    fn trampolines_with_foreign_or_empty_payloads_are_noops() {
        let (update, changes) = run_trampoline(on_ribbon_tab_click, RefAny::new(0xdead_u64));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());

        let data = RefAny::new(GalleryCellClickData {
            cell_idx: 0,
            on_select: None.into(),
            auto_select: false,
            in_panel: false,
            selected_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
            base_style: CssPropertyWithConditionsVec::from_const_slice(&[]),
        });
        let (update, _) = run_trampoline(on_ribbon_gallery_cell_click, data);
        assert_eq!(update, Update::DoNothing);
    }

    // ------------------------------------------------------------------
    // Groups
    // ------------------------------------------------------------------

    #[test]
    fn group_renders_items_over_a_footer_with_the_caption() {
        let tab = RibbonTab::new(AzString::from("HOME")).with_group(
            RibbonGroup::new(AzString::from("Clipboard"))
                .with_item(RibbonItem::SmallButton(small_btn("content_cut", "Cut"))),
        );
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom();
        let (_, content) = parts(&dom);
        assert_eq!(content.children.as_ref().len(), 1);
        assert!(has_class(
            &content.children.as_ref()[0],
            "__azul-native-ribbon-group"
        ));

        let (items, footer) = group_parts(content, 0);
        assert!(has_class(items, "__azul-native-ribbon-group-items"));
        assert_eq!(items.children.as_ref().len(), 1);
        assert!(has_class(footer, "__azul-native-ribbon-group-footer"));
        // no launcher: the footer is exactly [caption]
        assert_eq!(footer.children.as_ref().len(), 1);
        assert_eq!(text_of(&footer.children.as_ref()[0]), Some("Clipboard"));
    }

    #[test]
    fn group_with_launcher_renders_spacer_caption_launcher() {
        extern "C" fn noop(_: RefAny, _: CallbackInfo) -> Update {
            Update::DoNothing
        }
        let group = RibbonGroup::new(AzString::from("Font")).with_launcher(
            RefAny::new(0u8),
            noop as super::super::button::ButtonOnClickCallbackType,
        );
        let tab = RibbonTab::new(AzString::from("HOME")).with_group(group);
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom();
        let (_, content) = parts(&dom);
        let (_, footer) = group_parts(content, 0);

        let ch = footer.children.as_ref();
        assert_eq!(ch.len(), 3, "[spacer, caption, launcher]");
        assert!(has_class(&ch[0], "__azul-native-ribbon-footer-spacer"));
        assert_eq!(text_of(&ch[1]), Some("Font"));
        // the launcher is a real Button widget with the south_east icon
        assert!(matches!(ch[2].root.get_node_type(), NodeType::Button));
        assert_eq!(
            icon_name_of(&ch[2].children.as_ref()[0]),
            Some("south_east")
        );
        assert_eq!(ch[2].root.get_callbacks().as_ref().len(), 1);
    }

    // ------------------------------------------------------------------
    // Items
    // ------------------------------------------------------------------

    /// Renders one item into a throwaway single-group ribbon and returns the
    /// rendered item node.
    fn render_item(item: RibbonItem) -> Dom {
        let tab = RibbonTab::new(AzString::from("t"))
            .with_group(RibbonGroup::new(AzString::from("g")).with_item(item));
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom();
        let (_, content) = parts(&dom);
        let (items, _) = group_parts(content, 0);
        assert_eq!(items.children.as_ref().len(), 1);
        items.children.as_ref()[0].clone()
    }

    #[test]
    fn large_button_expands_to_a_button_widget_with_icon_label_and_arrow() {
        let rb = RibbonButton::new(AzString::from("content_paste"), AzString::from("Paste"))
            .with_arrow(RibbonArrow::Split);
        let node = render_item(RibbonItem::LargeButton(rb));

        assert!(matches!(node.root.get_node_type(), NodeType::Button));
        assert!(
            has_class(&node, "__azul-native-button"),
            "reuses the Button widget"
        );
        let ch = node.children.as_ref();
        assert_eq!(ch.len(), 3, "[icon, label, arrow]");
        assert_eq!(icon_name_of(&ch[0]), Some("content_paste"));
        assert_eq!(text_of(&ch[1]), Some("Paste"));
        assert_eq!(icon_name_of(&ch[2]), Some("arrow_drop_down"));

        let s = RibbonStyle::office_2013();
        assert_eq!(inline_props(&node), style_props(&s.large_button_style));
        assert_eq!(inline_props(&ch[0]), style_props(&s.large_icon_style));
        assert_eq!(inline_props(&ch[1]), style_props(&s.large_label_style));
        assert_eq!(inline_props(&ch[2]), style_props(&s.arrow_icon_style));
    }

    #[test]
    fn icon_only_small_button_skips_the_empty_label() {
        let node = render_item(RibbonItem::SmallButton(small_btn("format_bold", "")));
        let ch = node.children.as_ref();
        assert_eq!(ch.len(), 1, "icon only — no empty text node");
        assert_eq!(icon_name_of(&ch[0]), Some("format_bold"));
    }

    #[test]
    fn toggled_button_appends_the_checked_style_last() {
        let rb = small_btn("format_align_left", "").with_toggled(true);
        let node = render_item(RibbonItem::SmallButton(rb));

        let s = RibbonStyle::office_2013();
        let mut expected = style_props(&s.small_button_style);
        expected.extend(style_props(&s.checked_style));
        assert_eq!(
            inline_props(&node),
            expected,
            "checked props must come last so they win (inline CSS is last-wins)"
        );
    }

    #[test]
    fn columns_and_rows_nest_items_recursively() {
        let column = RibbonColumn::new()
            .with_item(RibbonItem::SmallButton(small_btn("content_cut", "Cut")))
            .with_item(RibbonItem::Row(
                RibbonRow::new()
                    .with_item(RibbonItem::SmallButton(small_btn("format_bold", "")))
                    .with_item(RibbonItem::Separator),
            ));
        let node = render_item(RibbonItem::Column(column));

        assert!(has_class(&node, "__azul-native-ribbon-column"));
        let ch = node.children.as_ref();
        assert_eq!(ch.len(), 2);
        assert!(matches!(ch[0].root.get_node_type(), NodeType::Button));
        assert!(has_class(&ch[1], "__azul-native-ribbon-row"));
        let row_ch = ch[1].children.as_ref();
        assert_eq!(row_ch.len(), 2);
        assert!(has_class(&row_ch[1], "__azul-native-ribbon-separator"));
    }

    #[test]
    fn embedded_widgets_render_with_their_own_classes() {
        use azul_css::StringVec;

        let combo = ComboBox::new(StringVec::from_vec(vec![AzString::from("Calibri")]));
        let node = render_item(RibbonItem::Combo(combo));
        assert!(has_class(&node, "__azul-native-combobox"));

        let drop = DropDown::new(StringVec::from_vec(vec![AzString::from("11")]));
        let node = render_item(RibbonItem::Drop(drop));
        assert!(has_class(&node, "__azul-native-dropdown"));

        let check = CheckBox::create(true);
        let node = render_item(RibbonItem::Check(check));
        assert!(has_class(&node, "__azul-native-checkbox-container"));
    }

    #[test]
    fn custom_items_pass_through_verbatim() {
        let custom = Dom::create_text_do_not_use_without_block_level_wrapper("¶");
        let node = render_item(RibbonItem::Custom(custom.clone()));
        assert_eq!(node, custom);
    }

    // ------------------------------------------------------------------
    // Gallery
    // ------------------------------------------------------------------

    fn gallery(cells: usize) -> RibbonGallery {
        let v: Vec<RibbonGalleryCell> = (0..cells)
            .map(|i| {
                RibbonGalleryCell::new(
                    Dom::create_text_do_not_use_without_block_level_wrapper(format!("AaBbCc{i}")),
                    AzString::from(format!("Style {i}")),
                )
            })
            .collect();
        RibbonGallery::new(RibbonGalleryCellVec::from_vec(v))
    }

    #[test]
    fn gallery_renders_strip_cells_and_three_spinner_buttons() {
        let wrapper = render_item(RibbonItem::Gallery(gallery(4).with_selected(2)));
        assert!(has_class(&wrapper, "__azul-native-ribbon-gallery-wrapper"));
        let node = &wrapper.children.as_ref()[0];
        assert!(has_class(node, "__azul-native-ribbon-gallery"));

        let ch = node.children.as_ref();
        assert_eq!(ch.len(), 2, "[strip, spinner]");
        let (strip, spinner) = (&ch[0], &ch[1]);

        assert!(has_class(strip, "__azul-native-ribbon-gallery-strip"));
        let cells = strip.children.as_ref();
        assert_eq!(cells.len(), 4);
        for (i, cell) in cells.iter().enumerate() {
            assert!(has_class(cell, "__azul-native-ribbon-gallery-cell"));
            assert_eq!(
                has_class(cell, "__azul-native-ribbon-gallery-cell-selected"),
                i == 2,
                "only cell 2 is selected"
            );
            // [preview, label]
            let cc = cell.children.as_ref();
            assert_eq!(cc.len(), 2);
            assert_eq!(text_of(&cc[0]), Some(format!("AaBbCc{i}").as_str()));
            assert_eq!(text_of(&cc[1]), Some(format!("Style {i}").as_str()));
        }

        // selected cell style = base + selected extras appended
        let s = RibbonStyle::office_2013();
        let mut expected = style_props(&s.gallery_cell_style);
        expected.extend(style_props(&s.gallery_cell_selected_style));
        assert_eq!(inline_props(&cells[2]), expected);

        assert!(has_class(spinner, "__azul-native-ribbon-gallery-spinner"));
        let buttons = spinner.children.as_ref();
        assert_eq!(buttons.len(), 3);
        let expected_icons = ["expand_less", "expand_more", "arrow_drop_down"];
        for (b, expected_icon) in buttons.iter().zip(expected_icons) {
            assert!(matches!(b.root.get_node_type(), NodeType::Button));
            assert_eq!(icon_name_of(&b.children.as_ref()[0]), Some(expected_icon));
        }
    }

    #[test]
    fn gallery_cells_carry_their_own_index_in_the_click_payload() {
        let g = gallery(2).with_on_select(
            RefAny::new(IndexLog { seen: Vec::new() }),
            record_index as RibbonGalleryOnSelectCallbackType,
        );
        let wrapper = render_item(RibbonItem::Gallery(g));
        let frame = &wrapper.children.as_ref()[0];
        for (i, cell) in frame.children.as_ref()[0]
            .children
            .as_ref()
            .iter()
            .enumerate()
        {
            let cbs = cell.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 1);
            let mut payload = cbs.as_ref()[0].refany.clone();
            let data = payload
                .downcast_ref::<GalleryCellClickData>()
                .expect("cell payload is a GalleryCellClickData");
            assert_eq!(data.cell_idx, i);
        }
    }

    // ------------------------------------------------------------------
    // Style injection
    // ------------------------------------------------------------------

    #[test]
    fn replacing_a_part_style_restyles_the_expanded_buttons() {
        let injected = CssPropertyWithConditionsVec::from_vec(vec![Cond::simple(
            P::const_font_size(StyleFontSize::const_px(99)),
        )]);

        let tab = RibbonTab::new(AzString::from("t")).with_group(
            RibbonGroup::new(AzString::from("g"))
                .with_item(RibbonItem::SmallButton(small_btn("format_bold", ""))),
        );
        let mut r = Ribbon::new(RibbonTabVec::from_vec(vec![tab]));
        r.style.small_button_style = injected.clone();
        let dom = r.dom();
        let (_, content) = parts(&dom);
        let (items, _) = group_parts(content, 0);

        assert_eq!(
            inline_props(&items.children.as_ref()[0]),
            style_props(&injected),
            "the injected style must reach the expanded Button verbatim"
        );
    }

    // ------------------------------------------------------------------
    // Whole-tree invariants
    // ------------------------------------------------------------------

    #[test]
    fn estimated_child_count_cache_stays_consistent_for_a_full_ribbon() {
        let tab = RibbonTab::new(AzString::from("HOME"))
            .with_group(
                RibbonGroup::new(AzString::from("Clipboard"))
                    .with_item(RibbonItem::LargeButton(
                        RibbonButton::new(AzString::from("content_paste"), AzString::from("Paste"))
                            .with_arrow(RibbonArrow::Split),
                    ))
                    .with_item(RibbonItem::Column(
                        RibbonColumn::new()
                            .with_item(RibbonItem::SmallButton(small_btn("content_cut", "Cut")))
                            .with_item(RibbonItem::SmallButton(small_btn("content_copy", "Copy"))),
                    )),
            )
            .with_group(
                RibbonGroup::new(AzString::from("Styles"))
                    .with_item(RibbonItem::Gallery(gallery(6))),
            );
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab]))
            .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
            .dom();

        assert_eq!(
            dom.estimated_total_children,
            recursive_descendants(&dom),
            "cached descendant count desynced from the real tree"
        );
    }

    #[test]
    fn from_ribbon_for_dom_matches_dom() {
        // Only meaningful without callbacks: every `dom()` call mints fresh
        // per-tab RefAny payloads and two RefAnys never compare equal.
        // Inert: the default behaviors mint a fresh chrome `RefAny` per call
        // and two RefAnys never compare equal.
        let inert = || Ribbon::new(tabs(3)).with_behavior(RibbonBehavior::inert());
        assert_eq!(Dom::from(inert()), inert().dom());
    }

    #[test]
    fn styled_combo_box_injects_the_ribbon_field_look() {
        use azul_css::StringVec;

        let s = RibbonStyle::office_2013();
        let combo = s.styled_combo_box(
            StringVec::from_vec(vec![AzString::from("Calibri")]),
            AzString::from("Calibri (Body)"),
            133,
        );

        let default = ComboBox::create();
        assert_ne!(
            combo.wrapper_style, default.wrapper_style,
            "wrapper restyled"
        );
        assert_ne!(combo.field_style, default.field_style, "field restyled");
        assert_eq!(combo.combo_state.inner.text.as_str(), "Calibri (Body)");

        // the width is the LAST wrapper property, so it wins over any base width
        let last = combo
            .wrapper_style
            .as_ref()
            .last()
            .expect("wrapper style is non-empty");
        assert!(
            matches!(&last.property, CssProperty::Width(_)),
            "styled_combo_box must append the width last, got {:?}",
            last.property
        );
    }

    // ------------------------------------------------------------------
    // Theming
    // ------------------------------------------------------------------

    #[test]
    fn from_theme_recolors_the_accent_carrying_parts() {
        let neon = ColorU {
            r: 255,
            g: 0,
            b: 128,
            a: 255,
        };
        let mut theme = RibbonTheme::office_2013();
        theme.accent = neon;

        let s = RibbonStyle::from_theme(theme);
        assert_eq!(s.theme, theme, "the style bundle records its palette");
        assert_ne!(s, RibbonStyle::office_2013());

        // The app button's fill is the accent color.
        let app_bg = s
            .app_button_style
            .as_ref()
            .iter()
            .find_map(|c| match &c.property {
                CssProperty::BackgroundContent(b) => b.get_property().cloned(),
                _ => None,
            })
            .expect("app button declares a background");
        assert_eq!(
            app_bg.as_ref(),
            &[StyleBackgroundContent::Color(neon)],
            "the app button fill must follow the theme accent"
        );

        // The active tab's text is the accent color.
        let active_text = s
            .tab_active_style
            .as_ref()
            .iter()
            .find_map(|c| match &c.property {
                CssProperty::TextColor(t) => t.get_property().copied(),
                _ => None,
            })
            .expect("active tab declares a text color");
        assert_eq!(active_text.inner, neon);
    }

    #[test]
    fn office_2013_is_exactly_from_theme_of_the_office_2013_palette() {
        assert_eq!(
            RibbonStyle::office_2013(),
            RibbonStyle::from_theme(RibbonTheme::office_2013()),
            "one source of truth: the named preset is just from_theme"
        );
    }

    #[test]
    fn from_system_with_no_reported_colors_falls_back_to_office_2013() {
        // SystemStyle::default() may pre-fill platform colors; the fallback
        // contract is about a system that reports NO colors at all.
        let mut sys = SystemStyle::default();
        sys.colors = system::SystemColors::default();
        assert_eq!(
            RibbonTheme::from_system(sys.clone()),
            RibbonTheme::office_2013()
        );
        assert_eq!(RibbonStyle::from_system(sys), RibbonStyle::office_2013());
    }

    #[test]
    fn from_system_extracts_reported_colors_and_falls_back_for_the_rest() {
        let reported = ColorU {
            r: 9,
            g: 99,
            b: 199,
            a: 255,
        };
        let mut sys = SystemStyle::default();
        sys.colors.accent = Some(reported).into();

        let t = RibbonTheme::from_system(sys);
        assert_eq!(t.accent, reported, "reported accent must be extracted");
        assert_eq!(t.hover_border, reported, "hover border follows the accent");
        assert_eq!(
            t.text,
            RibbonTheme::office_2013().text,
            "unreported colors fall back to the the Office-2013-era look palette"
        );
    }

    // ------------------------------------------------------------------
    // Behaviors
    // ------------------------------------------------------------------

    #[test]
    fn default_behavior_is_office_2013_and_inert_disables_everything() {
        assert_eq!(RibbonBehavior::default(), RibbonBehavior::office_2013());
        let w = RibbonBehavior::office_2013();
        assert!(w.collapsible && w.peek_on_hover && w.auto_select_gallery && w.expandable_gallery);
        assert!(w.mobile_tab_overlay);
        let i = RibbonBehavior::inert();
        assert!(
            !i.collapsible && !i.peek_on_hover && !i.auto_select_gallery && !i.expandable_gallery
        );
        assert!(!i.mobile_tab_overlay);
        assert_eq!(Ribbon::new(tabs(1)).behavior, RibbonBehavior::office_2013());
    }

    #[test]
    fn collapsible_tabs_carry_double_click_and_peek_handlers() {
        let dom = Ribbon::new(tabs(3)).dom();
        let (bar, _) = parts(&dom);
        for tab in &bar.children.as_ref()[..3] {
            let events: Vec<EventFilter> = tab
                .root
                .get_callbacks()
                .as_ref()
                .iter()
                .map(|c| c.event)
                .collect();
            assert!(
                events.contains(&EventFilter::Hover(HoverEventFilter::DoubleClick)),
                "a collapsible ribbon must listen for DoubleClick, got {events:?}"
            );
            assert!(events.contains(&EventFilter::Hover(HoverEventFilter::MouseEnter)));
            assert!(events.contains(&EventFilter::Hover(HoverEventFilter::MouseLeave)));
        }
    }

    #[test]
    fn inert_behavior_attaches_no_chrome_handlers() {
        let dom = Ribbon::new(tabs(2))
            .with_behavior(RibbonBehavior::inert())
            .dom();
        let (bar, _) = parts(&dom);
        for tab in &bar.children.as_ref()[..2] {
            assert!(
                tab.root.get_callbacks().as_ref().is_empty(),
                "an inert ribbon with no user callback must attach nothing"
            );
        }
    }

    #[test]
    fn peek_can_be_disabled_while_collapse_stays_on() {
        let behavior = RibbonBehavior {
            peek_on_hover: false,
            ..RibbonBehavior::office_2013()
        };
        let dom = Ribbon::new(tabs(1)).with_behavior(behavior).dom();
        let (bar, _) = parts(&dom);
        let events: Vec<EventFilter> = bar.children.as_ref()[0]
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|c| c.event)
            .collect();
        assert_eq!(
            events,
            vec![EventFilter::Hover(HoverEventFilter::DoubleClick)]
        );
    }

    #[test]
    fn expandable_gallery_wraps_the_frame_and_adds_a_hidden_panel() {
        let node = render_item(RibbonItem::Gallery(gallery(3)));
        assert!(has_class(&node, "__azul-native-ribbon-gallery-wrapper"));

        let ch = node.children.as_ref();
        assert_eq!(ch.len(), 2, "[frame, panel]");
        assert!(has_class(&ch[0], "__azul-native-ribbon-gallery"));
        assert!(has_class(&ch[1], "__azul-native-ribbon-gallery-panel"));

        // The panel holds EVERY cell and starts hidden.
        assert_eq!(ch[1].children.as_ref().len(), 3);
        let display = inline_props(&ch[1]).into_iter().find_map(|p| match p {
            CssProperty::Display(d) => d.get_property().copied(),
            _ => None,
        });
        assert_eq!(
            display,
            Some(LayoutDisplay::None),
            "the panel starts hidden"
        );

        // The third spinner button is the "More" toggle.
        let spinner = &ch[0].children.as_ref()[1];
        let more = &spinner.children.as_ref()[2];
        assert_eq!(more.root.get_callbacks().as_ref().len(), 1);
        assert_eq!(
            more.root.get_callbacks().as_ref()[0].event,
            EventFilter::Hover(HoverEventFilter::Click)
        );
    }

    #[test]
    fn non_expandable_gallery_is_the_bare_frame() {
        let tab = RibbonTab::new(AzString::from("t")).with_group(
            RibbonGroup::new(AzString::from("g")).with_item(RibbonItem::Gallery(gallery(2))),
        );
        let behavior = RibbonBehavior {
            expandable_gallery: false,
            ..RibbonBehavior::office_2013()
        };
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab]))
            .with_behavior(behavior)
            .dom();
        let (_, content) = parts(&dom);
        let (items, _) = group_parts(content, 0);
        let node = &items.children.as_ref()[0];
        assert!(has_class(node, "__azul-native-ribbon-gallery"));
        assert!(!has_class(node, "__azul-native-ribbon-gallery-wrapper"));
    }

    #[test]
    fn auto_select_attaches_cell_handlers_even_without_a_user_callback() {
        // The classic behavior moves the highlight on click regardless of the app; with
        // auto_select off and no user callback, nothing is attached.
        let node = render_item(RibbonItem::Gallery(gallery(2)));
        let strip = &node.children.as_ref()[0].children.as_ref()[0];
        for cell in strip.children.as_ref() {
            assert_eq!(cell.root.get_callbacks().as_ref().len(), 1);
        }

        let tab = RibbonTab::new(AzString::from("t")).with_group(
            RibbonGroup::new(AzString::from("g")).with_item(RibbonItem::Gallery(gallery(2))),
        );
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab]))
            .with_behavior(RibbonBehavior::inert())
            .dom();
        let (_, content) = parts(&dom);
        let (items, _) = group_parts(content, 0);
        let strip = &items.children.as_ref()[0].children.as_ref()[0];
        for cell in strip.children.as_ref() {
            assert!(cell.root.get_callbacks().as_ref().is_empty());
        }
    }

    // ------------------------------------------------------------------
    // Responsive / mobile
    // ------------------------------------------------------------------

    /// Both chromes are emitted once and the VIEWPORT decides which shows,
    /// so the mobile ribbon keeps the desktop semantics (same tabs, same
    /// groups) without a second widget tree.
    #[test]
    fn mobile_chrome_is_emitted_alongside_the_desktop_chrome() {
        let dom = Ribbon::new(tabs(3)).with_active_tab(1).dom();
        let ch = dom.children.as_ref();

        for class in [
            "__azul-native-ribbon-tabbar",
            "__azul-native-ribbon-mobile-tab",
            "__azul-native-ribbon-mobile-tab-overlay",
            "__azul-native-ribbon-mobile-group-list",
            "__azul-native-ribbon-content",
        ] {
            assert!(
                ch.iter().any(|c| has_class(c, class)),
                "the ribbon must emit a {class} child"
            );
        }

        // The mobile button shows the ACTIVE tab's label.
        let btn = ch
            .iter()
            .find(|c| has_class(c, "__azul-native-ribbon-mobile-tab"))
            .expect("mobile tab button");
        assert_eq!(label_text(btn), Some("t1"));
        assert_eq!(
            icon_name_of(&btn.children.as_ref()[1]),
            Some("expand_more"),
            "the mobile tab button carries the picker chevron"
        );

        // The overlay lists every tab.
        let overlay = ch
            .iter()
            .find(|c| has_class(c, "__azul-native-ribbon-mobile-tab-overlay"))
            .expect("tab overlay");
        assert_eq!(overlay.children.as_ref().len(), 3);
    }

    /// The breakpoint is expressed as a real viewport condition, and the
    /// conditional value comes LAST so it wins (inline CSS is last-match).
    #[test]
    fn the_desktop_tab_strip_is_hidden_under_the_mobile_breakpoint() {
        let s = RibbonStyle::office_2013();

        let displays: Vec<(&LayoutDisplay, bool)> = s
            .tab_bar_style
            .as_ref()
            .iter()
            .filter_map(|c| match &c.property {
                CssProperty::Display(d) => d
                    .get_property()
                    .map(|d| (d, !c.apply_if.as_ref().is_empty())),
                _ => None,
            })
            .collect();
        assert_eq!(
            displays.len(),
            2,
            "the tab strip declares an unconditional and a mobile display"
        );
        assert_eq!(*displays[0].0, LayoutDisplay::Flex);
        assert!(!displays[0].1, "the desktop value is unconditional");
        assert_eq!(*displays[1].0, LayoutDisplay::None);
        assert!(
            displays[1].1,
            "the mobile value is conditional and comes last"
        );

        // ...and the mobile button is the mirror image.
        let mobile: Vec<(&LayoutDisplay, bool)> = s
            .mobile_tab_button_style
            .as_ref()
            .iter()
            .filter_map(|c| match &c.property {
                CssProperty::Display(d) => d
                    .get_property()
                    .map(|d| (d, !c.apply_if.as_ref().is_empty())),
                _ => None,
            })
            .collect();
        assert_eq!(*mobile[0].0, LayoutDisplay::None);
        assert_eq!(*mobile[1].0, LayoutDisplay::Flex);
        assert!(mobile[1].1);
    }

    /// Handedness moves the mobile group list to the reachable side. It is
    /// independent of text direction, so it is its own system setting.
    #[test]
    fn handedness_flips_the_mobile_group_list_divider() {
        let right =
            RibbonStyle::from_theme_handed(RibbonTheme::office_2013(), Handedness::RightHanded);
        let left =
            RibbonStyle::from_theme_handed(RibbonTheme::office_2013(), Handedness::LeftHanded);
        assert_ne!(right.mobile_group_list_style, left.mobile_group_list_style);

        let has = |s: &CssPropertyWithConditionsVec, want_left: bool| {
            s.as_ref().iter().any(|c| {
                if want_left {
                    matches!(c.property, CssProperty::BorderLeftWidth(_))
                } else {
                    matches!(c.property, CssProperty::BorderRightWidth(_))
                }
            })
        };
        assert!(
            has(&right.mobile_group_list_style, true),
            "a right-handed list sits at the right edge, so its divider is on its LEFT"
        );
        assert!(
            has(&left.mobile_group_list_style, false),
            "a left-handed list sits at the left edge, so its divider is on its RIGHT"
        );
    }

    #[test]
    fn from_system_picks_up_the_system_handedness() {
        let mut sys = SystemStyle::default();
        sys.handedness = Handedness::LeftHanded;
        let from_sys = RibbonStyle::from_system(sys.clone());
        let expected =
            RibbonStyle::from_theme_handed(RibbonTheme::from_system(sys), Handedness::LeftHanded);
        assert_eq!(
            from_sys.mobile_group_list_style,
            expected.mobile_group_list_style
        );
    }

    #[test]
    fn inert_behavior_leaves_the_mobile_tab_button_without_a_toggle() {
        let dom = Ribbon::new(tabs(2))
            .with_behavior(RibbonBehavior::inert())
            .dom();
        let btn = dom
            .children
            .as_ref()
            .iter()
            .find(|c| has_class(c, "__azul-native-ribbon-mobile-tab"))
            .expect("mobile tab button");
        assert!(btn.root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn styled_combo_box_follows_the_style_bundles_theme() {
        let neon = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 255,
        };
        let mut theme = RibbonTheme::office_2013();
        theme.field_border = neon;

        let combo = RibbonStyle::from_theme(theme).styled_combo_box(
            StringVec::from_vec(vec![]),
            AzString::from("x"),
            50,
        );
        let border_color = combo
            .field_style
            .as_ref()
            .iter()
            .find_map(|c| match &c.property {
                CssProperty::BorderTopColor(b) => b.get_property().copied(),
                _ => None,
            })
            .expect("combo field declares a border color");
        assert_eq!(
            border_color.inner, neon,
            "combo field border follows the theme"
        );
    }
}

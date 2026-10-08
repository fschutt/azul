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
//! `dialogBoxLauncher` → [`RibbonGroup::launcher`]. Contextual tabs, KeyTips
//! and the backstage view are out of scope.
//!
//! A tab wider than the ribbon scales down as Office's does (its "ribbon
//! scaling"), step by step and the rightmost group first: large buttons
//! become small ones (the label beside a 16 px icon, three to a column),
//! galleries show fewer cells, buttons show their icons alone - and only
//! then does a group collapse into one button (its icon over its label and
//! ▾) that opens the whole group in a popup. Every control stays reachable
//! at any width. The width is that of the window the DOM is built for,
//! asked through the recorded size queries (a resize across a step rebuilds
//! the ribbon, any other re-flows it), or [`Ribbon::available_width`].
//!
//! The controls are laid out as Office 2010 lays them out: a large button's
//! label of two words or more is set on two balanced lines with a menu's ▾
//! after the last word ("New / Items ▾"); a split button with
//! [`RibbonButton::on_arrow_click`] is two hit targets (the icon, the label
//! and ▾); small items at a group's top level stack three to a column; a
//! gallery with [`RibbonGallery::columns`] is a list of commands (Outlook's
//! Quick Steps).
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
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{
        ComponentEventFilter, Dom, DomNodeId, DomVec, EventFilter, HoverEventFilter, IdOrClass,
        IdOrClass::Class, IdOrClassVec, NodeData, NodeType, OptionDom, TabIndex,
    },
    refany::RefAny,
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
    window::WindowBackgroundMaterial,
};
use azul_css::{
    dynamic_selector::OptionCssPropertyWithConditionsVec,
    impl_option, impl_vec, impl_vec_clone, impl_vec_debug, impl_vec_mut,
    system::{Handedness, SystemStyle},
};
// widget/render module pulls in the css property/value types it builds with
#[allow(clippy::wildcard_imports)]
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

use super::{
    button::{styled_button, ButtonOnClickCallbackType, OptionButtonOnClick},
    check_box::CheckBox,
    combobox::ComboBox,
    drop_down::DropDown,
    themes::{flat, style_kit, OptionUiTheme, UiTheme},
    titlebar::{OptionTabsInTitlebar, TabsInTitlebar},
};
use crate::callbacks::{Callback, CallbackInfo};

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
    from_handle_byref_fn: AzRibbonOnTabClickCallback_createFromHostHandleByref,
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
    from_handle_byref_fn: AzRibbonGalleryOnSelectCallback_createFromHostHandleByref,
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

// -- the Office 2010 palette (seeds RibbonTheme::office_2010) --
//
// Sampled from Outlook 2010 (Silver): the rest of it is the flat theme's
// tokens, which ARE Office 2010.

/// The ribbon's foot and the tab strip's rule (#BAC0C9): a shade darker than
/// the panes' hairline, as the band's edge against the window.
const O10_BORDER: ColorU = ColorU {
    r: 186,
    g: 192,
    b: 201,
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

    /// The Office 2010 palette (Silver), the flat theme's ribbon and the
    /// default: white paper fading to silver in the band, Outlook's orange
    /// File tab, grey captions, the warm yellow hover in a gold rim, the
    /// orange pressed face, the deeper yellow of a toggled-on command. A
    /// palette that keeps these hover and pressed colours is painted with
    /// Office 2010's two-stop faces (the band's fade, the yellow / orange
    /// gradients); any other is painted in its plain colours.
    #[must_use]
    pub const fn office_2010() -> Self {
        Self {
            chrome_bg: WHITE,
            content_bg: WHITE,
            accent: flat::FILE_TAB,
            accent_hover: flat::FILE_TAB_HOVER,
            accent_text: WHITE,
            text: flat::LIGHT_INK,
            label: flat::LIGHT_SOFT1,
            icon: flat::LIGHT_ICON,
            border: O10_BORDER,
            separator: flat::LIGHT_BD,
            hover_bg: flat::LIGHT_HB,
            hover_border: flat::LIGHT_HOVER_BORDER,
            pressed_bg: flat::LIGHT_PT,
            checked_bg: flat::LIGHT_CHECKED_BOTTOM,
            selected_bg: flat::LIGHT_CHECKED_BOTTOM,
            field_border: flat::LIGHT_BD,
        }
    }

    /// Whether this palette carries Office 2010's states (its hover and
    /// pressed colours), so its parts take Office 2010's faces.
    #[must_use]
    pub(crate) const fn has_office_2010_faces(&self) -> bool {
        const fn same(a: ColorU, b: ColorU) -> bool {
            a.r == b.r && a.g == b.g && a.b == b.b && a.a == b.a
        }
        same(self.hover_bg, flat::LIGHT_HB) && same(self.pressed_bg, flat::LIGHT_PT)
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

impl RibbonTheme {
    /// The dark twin of this palette: what the ribbon paints on a dark window.
    ///
    /// Derived, not stored — the struct is the FFI palette and stays a set of
    /// LIGHT colours; every part builder emits each colour as a light/dark
    /// pair (`bg_both` & co.) and this is where the dark half comes from.
    ///
    /// Page-neutral fields — the white chrome and content band, the grey
    /// text, labels, icons, borders and separators, the hover/pressed/checked/
    /// selected greys, the field border — take the flat theme's dark tokens
    /// (`themes::flat`, the single source of dark values every widget shares),
    /// so a ribbon control and the button next to it agree on what "dark"
    /// is. A field that is its own colour — the accent, its hover, the label
    /// on it — keeps the light value: an accent fill does not change with the
    /// mode (`flat::button_states` makes the same call for a Primary button).
    /// A transparent light value stays transparent: an app that made the
    /// chrome see-through for a window gradient wants it see-through in dark
    /// mode too, not an opaque slab.
    ///
    /// The states the theme appends (`push_chrome_hover_fill` & co.) already
    /// use these same tokens, so a hovered control and the surface under it
    /// come from one palette.
    #[must_use]
    pub(crate) const fn dark_counterpart(&self) -> Self {
        const fn neutral(light: ColorU, dark: ColorU) -> ColorU {
            if light.a == 0 {
                light
            } else {
                dark
            }
        }
        Self {
            chrome_bg: neutral(self.chrome_bg, flat::DARK_SUR),
            content_bg: neutral(self.content_bg, flat::DARK_SUR),
            accent: self.accent,
            accent_hover: self.accent_hover,
            accent_text: self.accent_text,
            text: neutral(self.text, flat::DARK_INK),
            label: neutral(self.label, flat::DARK_INK2),
            icon: neutral(self.icon, flat::DARK_ICON),
            border: neutral(self.border, flat::DARK_BD),
            separator: neutral(self.separator, flat::DARK_SEP),
            hover_bg: neutral(self.hover_bg, flat::DARK_HT),
            hover_border: neutral(self.hover_border, flat::DARK_BD),
            pressed_bg: neutral(self.pressed_bg, flat::DARK_PT),
            checked_bg: neutral(self.checked_bg, flat::DARK_PT),
            selected_bg: neutral(self.selected_bg, flat::DARK_HT),
            field_border: neutral(self.field_border, flat::DARK_BD3),
        }
    }
}

impl Default for RibbonTheme {
    fn default() -> Self {
        Self::office_2010()
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
    push_box_border_frame(v);
    push_border_colors(v, c);
}

/// The widths and styles of [`push_box_border`], without the colours.
fn push_box_border_frame(v: &mut Vec<Cond>) {
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

// -- Resting colours, light and dark --
//
// Every colour a part declares at rest is a PAIR: the light value from the
// palette, then its dark twin from `RibbonTheme::dark_counterpart`, in that
// order because inline declarations resolve last-match-wins — a twin pushed
// first would lose to the light value in dark mode. `Cond::themed` builds the
// pair so half of one cannot ship; `widgets::theme_pairs` checks whatever is
// still built by hand. Before this existed the ribbon declared its resting
// surfaces in light only (the states had twins, the chrome under them did
// not), which is a white Office-2013 bar on a dark window.
//
// Exempt, and declared with the plain `cond_bg` / `cond_text_color`: the
// surfaces that are their own colour in both modes — the application button's
// accent fill and the white label on it — and the transparent fills/borders
// that a hover paints over, which have no dark value to take.

/// A palette field in both modes: the light value from `t`, the dark from
/// [`RibbonTheme::dark_counterpart`].
fn both(t: &RibbonTheme, field: fn(&RibbonTheme) -> ColorU) -> (ColorU, ColorU) {
    (field(t), field(&t.dark_counterpart()))
}

/// A background fill, light and dark.
fn bg_both(t: &RibbonTheme, field: fn(&RibbonTheme) -> ColorU) -> [Cond; 2] {
    let (light, dark) = both(t, field);
    Cond::themed(
        P::const_background_content(bg_vec(light)),
        P::const_background_content(bg_vec(dark)),
    )
}

/// A text colour, light and dark.
fn text_both(t: &RibbonTheme, field: fn(&RibbonTheme) -> ColorU) -> [Cond; 2] {
    let (light, dark) = both(t, field);
    Cond::themed(
        P::const_text_color(StyleTextColor { inner: light }),
        P::const_text_color(StyleTextColor { inner: dark }),
    )
}

/// The ACCENT as a TEXT colour on the neutral chrome — the active tab's
/// label, the mobile tab button and its chevron, the selected group-list
/// entry — light and dark.
///
/// The light half is the palette's accent, untouched. The dark half is the
/// flat theme's `DARK_ACC`, the same call `theme_tab` makes for its hover text
/// and `theme_combo_field` for its focus ring: the accent is a FILL colour,
/// and `#2B579A` as text on a dark surface is unreadable. Accent fills keep
/// their value in both modes (`theme_app_button`); this is for text.
fn accent_text_both(t: &RibbonTheme) -> [Cond; 2] {
    Cond::themed(
        P::const_text_color(StyleTextColor { inner: t.accent }),
        P::const_text_color(StyleTextColor {
            inner: flat::DARK_ACC,
        }),
    )
}

#[derive(Clone, Copy)]
enum Edge {
    Top,
    Left,
    Right,
    Bottom,
}

const fn edge_color(edge: Edge, c: ColorU) -> P {
    match edge {
        Edge::Top => P::const_border_top_color(StyleBorderTopColor { inner: c }),
        Edge::Left => P::const_border_left_color(StyleBorderLeftColor { inner: c }),
        Edge::Right => P::const_border_right_color(StyleBorderRightColor { inner: c }),
        Edge::Bottom => P::const_border_bottom_color(StyleBorderBottomColor { inner: c }),
    }
}

/// One edge's border colour, light and dark.
fn push_edge_color_both(
    v: &mut Vec<Cond>,
    edge: Edge,
    t: &RibbonTheme,
    field: fn(&RibbonTheme) -> ColorU,
) {
    let (light, dark) = both(t, field);
    v.extend(Cond::themed(
        edge_color(edge, light),
        edge_color(edge, dark),
    ));
}

/// [`push_border_colors`] in both modes: the four light values first, in the
/// same order, then the four twins (the `hover_border_both` shape).
fn push_border_colors_both(v: &mut Vec<Cond>, t: &RibbonTheme, field: fn(&RibbonTheme) -> ColorU) {
    const EDGES: [Edge; 4] = [Edge::Top, Edge::Left, Edge::Right, Edge::Bottom];
    let (light, dark) = both(t, field);
    for edge in EDGES {
        v.push(Cond::simple(edge_color(edge, light)));
    }
    for edge in EDGES {
        v.push(Cond::dark_mode(edge_color(edge, dark)));
    }
}

/// [`push_box_border`] with the colour in both modes.
fn push_box_border_both(v: &mut Vec<Cond>, t: &RibbonTheme, field: fn(&RibbonTheme) -> ColorU) {
    push_box_border_frame(v);
    push_border_colors_both(v, t, field);
}

/// [`push_bottom_border`] with the colour in both modes.
fn push_bottom_border_both(v: &mut Vec<Cond>, t: &RibbonTheme, field: fn(&RibbonTheme) -> ColorU) {
    push_bottom_border_frame(v);
    push_edge_color_both(v, Edge::Bottom, t, field);
}

// -- Interactive states --
//
// Hover, pressed and focus are NOT declared in this file. They are built by
// the theme module (`themes::flat::hover_bg_both` and friends), which returns
// each rule together with its dark twin, because the dark half needs that
// module's palette — `DARK_HT`, `DARK_PT`, `DARK_BD`, `DARK_ACC` — which this
// file cannot see. Declared here, a state could only ever name the light
// colour, which is how every ribbon control kept its light-blue hover on a
// dark surface.
//
// Which dark colour a rule takes depends on the SURFACE it sits on:
//
//   * the chrome — tab strip, buttons, launcher, gallery, the mobile lists — is page-neutral: white
//     in the Office look, the window background from `from_system`. Its dark twins are the theme's
//     tokens, exactly what `flat::button_states` gives the neutral button;
//   * the application button is an ACCENT fill in either mode, so its hover keeps the palette's own
//     `accent_hover` (`theme_app_button`);
//   * a hovered tab's text and a focused field's ring take the ACCENT, the one state colour with a
//     genuine per-mode value in both palettes, so their twins are `DARK_ACC` (`theme_tab`,
//     `theme_combo_field`).

/// Hover fill of a control on the neutral chrome, light and dark: Office
/// 2010's yellow face for a palette with its states
/// ([`RibbonTheme::has_office_2010_faces`]), else the palette's colour.
fn push_chrome_hover_fill(v: &mut Vec<Cond>, t: &RibbonTheme) {
    if t.has_office_2010_faces() {
        v.extend(flat::hover_face_both());
    } else {
        v.extend(flat::hover_bg_both(t.hover_bg, flat::DARK_HT));
    }
}

/// Hover border of a control on the neutral chrome, all four edges, light and
/// dark (by night Office 2010's rim is the amber one).
fn push_chrome_hover_border(v: &mut Vec<Cond>, t: &RibbonTheme) {
    let dark = if t.has_office_2010_faces() {
        flat::DARK_HOVER_BORDER
    } else {
        flat::DARK_BD
    };
    v.extend(flat::hover_border_both(t.hover_border, dark));
}

/// Pressed fill of a control on the neutral chrome, light and dark: Office
/// 2010's orange face, or the palette's colour.
fn push_chrome_pressed_fill(v: &mut Vec<Cond>, t: &RibbonTheme) {
    if t.has_office_2010_faces() {
        v.extend(flat::active_face_both());
    } else {
        v.extend(flat::active_bg_both(t.pressed_bg, flat::DARK_PT));
    }
}

/// The resting face of a toggled-on command or a picked gallery cell, and
/// its rim: Office 2010's deeper yellow in its own rim, or the palette's
/// `fill` in its hover border.
fn push_checked_face(v: &mut Vec<Cond>, t: &RibbonTheme, fill: fn(&RibbonTheme) -> ColorU) {
    if t.has_office_2010_faces() {
        v.extend(flat::checked_face_both());
        v.extend(super::themes::decl::themed_border_color(
            flat::LIGHT_CHECKED_BORDER,
            flat::DARK_CHECKED_BORDER,
        ));
    } else {
        v.extend(bg_both(t, fill));
        push_border_colors_both(v, t, |p| p.hover_border);
    }
    // The states again: this face is APPENDED after the part's own states,
    // and a resting face after them would shadow them (last match wins) - a
    // toggled button lit no hover and no press.
    push_chrome_hover_fill(v, t);
    push_chrome_hover_border(v, t);
    push_chrome_pressed_fill(v, t);
}

/// Bottom border only (tab underline / ribbon bottom edge).
fn push_bottom_border(v: &mut Vec<Cond>, c: ColorU) {
    push_bottom_border_frame(v);
    v.push(Cond::simple(P::const_border_bottom_color(
        StyleBorderBottomColor { inner: c },
    )));
}

/// The width and style of [`push_bottom_border`], without the colour.
fn push_bottom_border_frame(v: &mut Vec<Cond>) {
    v.push(Cond::simple(P::const_border_bottom_width(
        LayoutBorderBottomWidth::const_px(1),
    )));
    v.push(Cond::simple(P::const_border_bottom_style(
        StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        },
    )));
}

/// Transparent-bordered, hover-highlighted button chassis shared by large
/// and small ribbon buttons.
fn push_button_chassis(v: &mut Vec<Cond>, t: &RibbonTheme) {
    v.push(cond_border_box());
    v.push(Cond::simple(P::const_cursor(StyleCursor::Default)));
    v.push(cond_bg(TRANSPARENT));
    push_box_border(v, TRANSPARENT);
    // Office 2010's lit buttons are softly rounded (2 px); 2013's are square.
    if t.has_office_2010_faces() {
        v.extend(super::themes::decl::radius(2));
    }
    push_chrome_hover_fill(v, t);
    push_chrome_hover_border(v, t);
    // Pressed: page-neutral chrome, so the dark twin is the theme's pressed
    // face — see the `Interactive states` note above.
    push_chrome_pressed_fill(v, t);
}

fn theme_container(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
    ];
    v.extend(bg_both(t, |p| p.chrome_bg));
    push_bottom_border_both(&mut v, t, |p| p.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_tab_bar(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        // Replaced by the full-width mobile tab button on phones. The
        // conditional MUST come after the unconditional value: inline
        // properties resolve last-match-wins.
        only_on_mobile(P::const_display(LayoutDisplay::None)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_height(LayoutHeight::const_px(26))),
    ];
    v.extend(bg_both(t, |p| p.chrome_bg));
    CssPropertyWithConditionsVec::from_vec(v)
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
    // Its own colour in both modes: an accent fill and the label on it are
    // the two resting surfaces with NO dark twin (see `dark_counterpart`).
    v.push(cond_bg(t.accent));
    v.push(cond_text_color(t.accent_text));
    v.push(Cond::simple(P::const_font_size(StyleFontSize::const_px(
        12,
    ))));
    v.push(Cond::simple(P::const_cursor(StyleCursor::Pointer)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    // Hover, light and dark. The application button is an ACCENT fill in
    // either mode (`cond_bg(t.accent)` above has no dark variant), so the dark
    // twin is the palette's own `accent_hover` too: the theme's neutral grey
    // on a blue button would be wrong, and inventing a second blue is a design
    // decision this refactor has no business making — `flat::button_states`
    // makes the same call for a Primary button.
    v.extend(flat::hover_bg_both(t.accent_hover, t.accent_hover));
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
    v.extend(text_both(t, |p| p.text));
    v.extend(bg_both(t, |p| p.chrome_bg));
    push_bottom_border_both(&mut v, t, |p| p.border);
    // Hover text, light and dark. The tab strip is page-neutral chrome and a
    // hovered tab's text takes the accent, so the dark twin is the theme's
    // `DARK_ACC` — the accent is the one state colour with a genuine per-mode
    // value in both palettes.
    v.extend(flat::hover_text_color_both(t.accent, flat::DARK_ACC));
    // A tab header is ONE line. Without this "PAGE LAYOUT" wrapped, and its
    // second line was drawn below the 26px tab strip, over the ribbon content
    // - invisible only because the content band was opaque and painted over
    // it. It stops being invisible the moment an app makes that band
    // translucent, which is what `content_bg` is for.
    v.push(Cond::simple(P::WhiteSpace(
        props::property::StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap),
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
    v.extend(accent_text_both(t));
    v.extend(bg_both(t, |p| p.chrome_bg));
    push_box_border_both(&mut v, t, |p| p.border);
    // Erase the underline below the active tab: the bottom border matches
    // the chrome so the tab visually merges with the ribbon content — in
    // both modes, so the pair follows the box border's pair (last wins).
    push_edge_color_both(&mut v, Edge::Bottom, t, |p| p.content_bg);
    // One line, like every other tab - see `theme_tab`.
    v.push(Cond::simple(P::WhiteSpace(
        props::property::StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap),
    )));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_tab_filler(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        1,
    )))];
    push_bottom_border_both(&mut v, t, |p| p.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_content(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_height(LayoutHeight::const_px(92))),
    ];
    // Office 2010's band is paper at its top (where the active tab merges in)
    // fading to silver at its foot.
    if t.has_office_2010_faces() {
        v.extend(flat::band_face_both());
    } else {
        v.extend(bg_both(t, |p| p.content_bg));
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_group(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
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
    ];
    push_edge_color_both(&mut v, Edge::Right, t, |p| p.separator);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_group_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
        Cond::simple(P::const_text_align(StyleTextAlign::Center)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(11))),
    ];
    v.extend(text_both(t, |p| p.label));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
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
    push_chrome_hover_fill(&mut v, t);
    push_chrome_hover_border(&mut v, t);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_launcher_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        11,
    )))];
    v.extend(text_both(t, |p| p.label));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_separator(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        Cond::simple(P::const_width(LayoutWidth::const_px(1))),
        Cond::simple(P::const_height(LayoutHeight::const_px(22))),
        Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
        Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        })),
        Cond::simple(P::const_margin_left(LayoutMarginLeft::const_px(3))),
        Cond::simple(P::const_margin_right(LayoutMarginRight::const_px(3))),
    ];
    v.extend(bg_both(t, |p| p.separator));
    CssPropertyWithConditionsVec::from_vec(v)
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
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        32,
    )))];
    v.extend(text_both(t, |p| p.icon));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_large_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        12,
    )))];
    v.extend(text_both(t, |p| p.text));
    v.push(Cond::simple(P::const_text_align(StyleTextAlign::Center)));
    v.push(Cond::simple(P::const_margin_top(
        LayoutMarginTop::const_px(3),
    )));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
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
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        16,
    )))];
    v.extend(text_both(t, |p| p.icon));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_small_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        12,
    )))];
    v.extend(text_both(t, |p| p.text));
    v.push(Cond::simple(P::const_margin_left(
        LayoutMarginLeft::const_px(5),
    )));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_arrow_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        14,
    )))];
    v.extend(text_both(t, |p| p.label));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Appended to a button's container style when [`RibbonButton::toggled`] is
/// set. Inline properties resolve last-wins, so these override the base.
fn theme_checked(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = Vec::new();
    push_checked_face(&mut v, t, |p| p.checked_bg);
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
    ];
    v.extend(bg_both(t, |p| p.chrome_bg));
    push_box_border_both(&mut v, t, |p| p.border);
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
    push_edge_color_both(&mut v, Edge::Right, t, |p| p.separator);
    push_chrome_hover_fill(&mut v, t);
    push_chrome_hover_border(&mut v, t);
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Appended to [`RibbonStyle::gallery_cell_style`] for the selected cell.
fn theme_gallery_cell_selected(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = Vec::new();
    push_checked_face(&mut v, t, |p| p.selected_bg);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_cell_label(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        11,
    )))];
    v.extend(text_both(t, |p| p.text));
    v.push(Cond::simple(P::const_margin_top(
        LayoutMarginTop::const_px(2),
    )));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_spinner(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
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
    ];
    push_edge_color_both(&mut v, Edge::Left, t, |p| p.separator);
    CssPropertyWithConditionsVec::from_vec(v)
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
        // Same reason as the mobile overlay: absolute is not "on top".
        Cond::simple(P::const_z_index(LayoutZIndex::Integer(100))),
        Cond::simple(P::const_top(LayoutTop::const_px(68))),
        Cond::simple(P::const_left(LayoutLeft::const_px(0))),
        Cond::simple(P::const_width(LayoutWidth::const_px(612))),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_flex_wrap(LayoutFlexWrap::Wrap)),
    ];
    v.extend(bg_both(t, |p| p.chrome_bg));
    push_box_border_both(&mut v, t, |p| p.border);
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The panel a collapsed group's popup shows the group on: a piece of the
/// content band - its face, Office 2010's fade for a palette with Office
/// 2010's states - closed by the chrome's border, the group laid on it as it
/// is in the band. The ribbon's font is declared again: the popup is a
/// window of its own, and its root inherits nothing from the ribbon.
fn theme_group_popup(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        cond_border_box(),
        Cond::simple(P::const_display(LayoutDisplay::Flex)),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
        Cond::simple(P::const_align_items(LayoutAlignItems::Stretch)),
        Cond::simple(P::const_font_family(SYSTEM_UI_FAMILY)),
        Cond::simple(P::const_font_size(StyleFontSize::const_px(12))),
    ];
    if t.has_office_2010_faces() {
        v.extend(flat::band_face_both());
    } else {
        v.extend(bg_both(t, |p| p.content_bg));
    }
    push_box_border_both(&mut v, t, |p| p.border);
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
    push_chrome_hover_fill(&mut v, t);
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_gallery_spinner_icon(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        12,
    )))];
    v.extend(text_both(t, |p| p.label));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
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
    v.extend(bg_both(t, |p| p.chrome_bg));
    v.extend(text_both(t, |p| p.text));
    push_box_border_both(&mut v, t, |p| p.field_border);
    // Focus ring, light and dark. The field sits on the neutral chrome and its
    // ring is the accent, so the dark twin is the theme's `DARK_ACC`. All four
    // edges: a ring that sets only some leaves the rest at their resting
    // colour.
    v.extend(flat::focus_border_both(t.accent, flat::DARK_ACC));
    CssPropertyWithConditionsVec::from_vec(v)
}

fn theme_combo_arrow(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        14,
    )))];
    v.extend(text_both(t, |p| p.label));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
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
    v.extend(accent_text_both(t));
    v.extend(bg_both(t, |p| p.chrome_bg));
    push_bottom_border_both(&mut v, t, |p| p.border);
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
    let mut v = vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(
        24,
    )))];
    v.extend(accent_text_both(t));
    v.push(Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(
        0,
    ))));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// Full-screen overlay listing every tab; opened by the mobile tab button.
fn theme_mobile_tab_overlay(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v = vec![
        // Hidden until the button opens it (on ANY viewport: the overlay is
        // only reachable through the mobile button).
        Cond::simple(P::const_display(LayoutDisplay::None)),
        cond_border_box(),
        Cond::simple(P::const_position(LayoutPosition::Absolute)),
        // ABOVE its siblings. `position: absolute` only takes a node out of
        // flow — it does not lift it in paint order, so with no z-index the
        // overlay was painted and then covered by the mobile band, which is a
        // LATER sibling in the same container. Nothing in the widget set
        // rendered above anything, because no widget set z-index at all.
        Cond::simple(P::const_z_index(LayoutZIndex::Integer(100))),
        Cond::simple(P::const_top(LayoutTop::const_px(0))),
        Cond::simple(P::const_left(LayoutLeft::const_px(0))),
        Cond::simple(P::const_width(LayoutWidth::Px(PixelValue::const_percent(
            100,
        )))),
        Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    ];
    v.extend(bg_both(t, |p| p.chrome_bg));
    push_box_border_both(&mut v, t, |p| p.border);
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
    ];
    v.extend(text_both(t, |p| p.text));
    v.push(Cond::simple(P::const_cursor(StyleCursor::Pointer)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    push_padding(&mut v, 0, 16, 0, 16);
    push_bottom_border_both(&mut v, t, |p| p.separator);
    push_chrome_hover_fill(&mut v, t);
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
    v.extend(bg_both(t, |p| p.chrome_bg));
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
        push_edge_color_both(&mut v, Edge::Right, t, |p| p.separator);
    } else {
        v.push(Cond::simple(P::const_border_left_width(
            LayoutBorderLeftWidth::const_px(1),
        )));
        v.push(Cond::simple(P::const_border_left_style(
            StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            },
        )));
        push_edge_color_both(&mut v, Edge::Left, t, |p| p.separator);
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
    ];
    v.extend(text_both(t, |p| p.text));
    v.push(Cond::simple(P::const_cursor(StyleCursor::Pointer)));
    v.push(Cond::simple(P::user_select(StyleUserSelect::None)));
    push_padding(&mut v, 0, 10, 0, 12);
    push_bottom_border_both(&mut v, t, |p| p.separator);
    push_chrome_hover_fill(&mut v, t);
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The selected entry of the mobile group list (appended, last-wins).
fn theme_mobile_group_list_item_selected(t: &RibbonTheme) -> CssPropertyWithConditionsVec {
    let mut v: Vec<Cond> = bg_both(t, |p| p.selected_bg).to_vec();
    v.extend(accent_text_both(t));
    CssPropertyWithConditionsVec::from_vec(v)
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
/// The mobile band: the group list AND the one visible group, side by side.
/// Collapsing on mobile has to hide THIS, not just the content — hiding the
/// content alone leaves the group list behind as a floating strip with nothing
/// beside it.
const MOBILE_BAND_CLASS: &str = "__azul-native-ribbon-mobile-band";
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
    /// The handedness the mobile parts were derived for.
    ///
    /// Stored for the same reason as [`Self::theme`]: it is an INPUT to one of
    /// the parts (`mobile_group_list_style` puts the divider on the far side for
    /// a left-handed layout), and `from_theme_handed` used to bake it into that
    /// vec and then throw the flag away — so the bundle could not re-derive the
    /// part, and nothing downstream could tell which hand it had been built for.
    pub handedness: Handedness,
    /// Root container (vertical: tab bar over content).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub container_style: OptionCssPropertyWithConditionsVec,
    /// The horizontal tab strip.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub tab_bar_style: OptionCssPropertyWithConditionsVec,
    /// The blue application button ("FILE").
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub app_button_style: OptionCssPropertyWithConditionsVec,
    /// An inactive tab header.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub tab_style: OptionCssPropertyWithConditionsVec,
    /// The active tab header.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub tab_active_style: OptionCssPropertyWithConditionsVec,
    /// The filler segment after the last tab (carries the underline).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub tab_filler_style: OptionCssPropertyWithConditionsVec,
    /// The content band below the tab strip (horizontal group list).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub content_style: OptionCssPropertyWithConditionsVec,
    /// One group (vertical: items over footer), incl. the right separator.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub group_style: OptionCssPropertyWithConditionsVec,
    /// The item area of a group.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub group_items_style: OptionCssPropertyWithConditionsVec,
    /// The footer row of a group (label + dialog launcher).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub group_footer_style: OptionCssPropertyWithConditionsVec,
    /// The centered group caption.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub group_label_style: OptionCssPropertyWithConditionsVec,
    /// Invisible spacer balancing the launcher so the caption stays centered.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub footer_spacer_style: OptionCssPropertyWithConditionsVec,
    /// Container style injected into the dialog-launcher [`Button`](crate::widgets::button::Button).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub launcher_button_style: OptionCssPropertyWithConditionsVec,
    /// Icon style injected into the dialog-launcher [`Button`](crate::widgets::button::Button).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub launcher_icon_style: OptionCssPropertyWithConditionsVec,
    /// A [`RibbonColumn`] packing box.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub column_style: OptionCssPropertyWithConditionsVec,
    /// A [`RibbonRow`] packing box.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub row_style: OptionCssPropertyWithConditionsVec,
    /// A [`RibbonItem::Separator`] rule.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub separator_style: OptionCssPropertyWithConditionsVec,
    /// Container style injected into large-button [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub large_button_style: OptionCssPropertyWithConditionsVec,
    /// Icon style injected into large-button [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub large_icon_style: OptionCssPropertyWithConditionsVec,
    /// Label style injected into large-button [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub large_label_style: OptionCssPropertyWithConditionsVec,
    /// Container style injected into small-button [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub small_button_style: OptionCssPropertyWithConditionsVec,
    /// Icon style injected into small-button [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub small_icon_style: OptionCssPropertyWithConditionsVec,
    /// Label style injected into small-button [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub small_label_style: OptionCssPropertyWithConditionsVec,
    /// Style of the drop-down arrow glyph on Menu/Split buttons.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub arrow_icon_style: OptionCssPropertyWithConditionsVec,
    /// APPENDED to the button container when [`RibbonButton::toggled`] is set.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub checked_style: OptionCssPropertyWithConditionsVec,
    /// The gallery outer frame.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_frame_style: OptionCssPropertyWithConditionsVec,
    /// The horizontal cell strip inside the gallery frame.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_strip_style: OptionCssPropertyWithConditionsVec,
    /// One gallery cell.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_cell_style: OptionCssPropertyWithConditionsVec,
    /// APPENDED to the selected gallery cell.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_cell_selected_style: OptionCssPropertyWithConditionsVec,
    /// The name label under a gallery cell preview.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_cell_label_style: OptionCssPropertyWithConditionsVec,
    /// The vertical spinner column on the gallery's right edge.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_spinner_style: OptionCssPropertyWithConditionsVec,
    /// Positioning context wrapping the gallery frame + expansion panel.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_wrapper_style: OptionCssPropertyWithConditionsVec,
    /// The expansion panel shown by the gallery's "More" button.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_panel_style: OptionCssPropertyWithConditionsVec,
    /// Full-width tab button shown INSTEAD of the tab strip on phones.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_tab_button_style: OptionCssPropertyWithConditionsVec,
    /// Active-tab label inside the mobile tab button.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_tab_label_style: OptionCssPropertyWithConditionsVec,
    /// Chevron on the mobile tab button.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_tab_arrow_style: OptionCssPropertyWithConditionsVec,
    /// Full-screen tab picker opened by the mobile tab button.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_tab_overlay_style: OptionCssPropertyWithConditionsVec,
    /// One row of the mobile tab picker.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_tab_overlay_item_style: OptionCssPropertyWithConditionsVec,
    /// Scrollable group list shown beside the visible group on phones.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_group_list_style: OptionCssPropertyWithConditionsVec,
    /// One entry of the mobile group list.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_group_list_item_style: OptionCssPropertyWithConditionsVec,
    /// APPENDED to the selected mobile group-list entry.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub mobile_group_list_item_selected_style: OptionCssPropertyWithConditionsVec,
    /// Container style injected into the three spinner [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_spinner_button_style: OptionCssPropertyWithConditionsVec,
    /// Icon style injected into the three spinner [`Button`](crate::widgets::button::Button)s.
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub gallery_spinner_icon_style: OptionCssPropertyWithConditionsVec,
    /// The panel a collapsed group's popup shows the whole group on (a tab
    /// too wide for the ribbon collapses its groups, see [`Ribbon`]).
    ///
    /// `None` means "no opinion": the part is derived from [`Self::theme`] at
    /// render time. `Some` is an override the caller chose, and `Some(empty)` is
    /// a real answer — "no properties at all" — which the pre-filled field could
    /// not express.
    pub group_popup_style: OptionCssPropertyWithConditionsVec,
}

impl RibbonStyle {
    /// The the Office-2013-era look look (white chrome, #2B579A accents).
    #[must_use]
    pub const fn office_2013() -> Self {
        Self::from_theme(RibbonTheme::office_2013())
    }

    /// The Office 2010 look (see [`RibbonTheme::office_2010`]) - the default,
    /// and the flat theme's ribbon.
    #[must_use]
    pub const fn office_2010() -> Self {
        Self::from_theme(RibbonTheme::office_2010())
    }

    /// Derives every part style from the given palette. This is the styling
    /// override API: build a [`RibbonTheme`] (or start from a preset), then
    /// replace individual `*_style` fields for finer control.
    #[must_use]
    pub const fn from_theme(theme: RibbonTheme) -> Self {
        Self::from_theme_handed(theme, Handedness::RightHanded)
    }

    /// [`Self::from_theme`] with an explicit hand: the mobile group list sits
    /// on the dominant-hand side so the thumb reaches it. Independent of text
    /// direction - see [`Handedness`].
    #[must_use]
    pub const fn from_theme_handed(theme: RibbonTheme, handedness: Handedness) -> Self {
        Self {
            theme,
            handedness,
            container_style: OptionCssPropertyWithConditionsVec::None,
            tab_bar_style: OptionCssPropertyWithConditionsVec::None,
            app_button_style: OptionCssPropertyWithConditionsVec::None,
            tab_style: OptionCssPropertyWithConditionsVec::None,
            tab_active_style: OptionCssPropertyWithConditionsVec::None,
            tab_filler_style: OptionCssPropertyWithConditionsVec::None,
            content_style: OptionCssPropertyWithConditionsVec::None,
            group_style: OptionCssPropertyWithConditionsVec::None,
            group_items_style: OptionCssPropertyWithConditionsVec::None,
            group_footer_style: OptionCssPropertyWithConditionsVec::None,
            group_label_style: OptionCssPropertyWithConditionsVec::None,
            footer_spacer_style: OptionCssPropertyWithConditionsVec::None,
            launcher_button_style: OptionCssPropertyWithConditionsVec::None,
            launcher_icon_style: OptionCssPropertyWithConditionsVec::None,
            column_style: OptionCssPropertyWithConditionsVec::None,
            row_style: OptionCssPropertyWithConditionsVec::None,
            separator_style: OptionCssPropertyWithConditionsVec::None,
            large_button_style: OptionCssPropertyWithConditionsVec::None,
            large_icon_style: OptionCssPropertyWithConditionsVec::None,
            large_label_style: OptionCssPropertyWithConditionsVec::None,
            small_button_style: OptionCssPropertyWithConditionsVec::None,
            small_icon_style: OptionCssPropertyWithConditionsVec::None,
            small_label_style: OptionCssPropertyWithConditionsVec::None,
            arrow_icon_style: OptionCssPropertyWithConditionsVec::None,
            checked_style: OptionCssPropertyWithConditionsVec::None,
            gallery_frame_style: OptionCssPropertyWithConditionsVec::None,
            gallery_strip_style: OptionCssPropertyWithConditionsVec::None,
            gallery_cell_style: OptionCssPropertyWithConditionsVec::None,
            gallery_cell_selected_style: OptionCssPropertyWithConditionsVec::None,
            gallery_cell_label_style: OptionCssPropertyWithConditionsVec::None,
            gallery_spinner_style: OptionCssPropertyWithConditionsVec::None,
            gallery_wrapper_style: OptionCssPropertyWithConditionsVec::None,
            gallery_panel_style: OptionCssPropertyWithConditionsVec::None,
            mobile_tab_button_style: OptionCssPropertyWithConditionsVec::None,
            mobile_tab_label_style: OptionCssPropertyWithConditionsVec::None,
            mobile_tab_arrow_style: OptionCssPropertyWithConditionsVec::None,
            mobile_tab_overlay_style: OptionCssPropertyWithConditionsVec::None,
            mobile_tab_overlay_item_style: OptionCssPropertyWithConditionsVec::None,
            mobile_group_list_style: OptionCssPropertyWithConditionsVec::None,
            mobile_group_list_item_style: OptionCssPropertyWithConditionsVec::None,
            mobile_group_list_item_selected_style: OptionCssPropertyWithConditionsVec::None,
            gallery_spinner_button_style: OptionCssPropertyWithConditionsVec::None,
            gallery_spinner_icon_style: OptionCssPropertyWithConditionsVec::None,
            group_popup_style: OptionCssPropertyWithConditionsVec::None,
        }
    }

    /// Whether the mobile parts are laid out for a left-handed grip.
    #[must_use]
    pub const fn is_left_handed(&self) -> bool {
        matches!(self.handedness, Handedness::LeftHanded)
    }

    /// The `container_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_container_style(&self) -> CssPropertyWithConditionsVec {
        self.container_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_container(&self.theme))
    }

    /// The `tab_bar_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_tab_bar_style(&self) -> CssPropertyWithConditionsVec {
        self.tab_bar_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_tab_bar(&self.theme))
    }

    /// The `app_button_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_app_button_style(&self) -> CssPropertyWithConditionsVec {
        self.app_button_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_app_button(&self.theme))
    }

    /// The `tab_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_tab_style(&self) -> CssPropertyWithConditionsVec {
        self.tab_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_tab(&self.theme))
    }

    /// The `tab_active_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_tab_active_style(&self) -> CssPropertyWithConditionsVec {
        self.tab_active_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_tab_active(&self.theme))
    }

    /// The `tab_filler_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_tab_filler_style(&self) -> CssPropertyWithConditionsVec {
        self.tab_filler_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_tab_filler(&self.theme))
    }

    /// The `content_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_content_style(&self) -> CssPropertyWithConditionsVec {
        self.content_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_content(&self.theme))
    }

    /// The `group_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_group_style(&self) -> CssPropertyWithConditionsVec {
        self.group_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_group(&self.theme))
    }

    /// The `group_items_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_group_items_style(&self) -> CssPropertyWithConditionsVec {
        self.group_items_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_const_slice(GROUP_ITEMS_STYLE))
    }

    /// The `group_footer_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_group_footer_style(&self) -> CssPropertyWithConditionsVec {
        self.group_footer_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_const_slice(GROUP_FOOTER_STYLE))
    }

    /// The `group_label_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_group_label_style(&self) -> CssPropertyWithConditionsVec {
        self.group_label_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_group_label(&self.theme))
    }

    /// The `footer_spacer_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_footer_spacer_style(&self) -> CssPropertyWithConditionsVec {
        self.footer_spacer_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_const_slice(FOOTER_SPACER_STYLE))
    }

    /// The `launcher_button_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_launcher_button_style(&self) -> CssPropertyWithConditionsVec {
        self.launcher_button_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_launcher_button(&self.theme))
    }

    /// The `launcher_icon_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_launcher_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.launcher_icon_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_launcher_icon(&self.theme))
    }

    /// The `column_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_column_style(&self) -> CssPropertyWithConditionsVec {
        self.column_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_const_slice(COLUMN_STYLE))
    }

    /// The `row_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_row_style(&self) -> CssPropertyWithConditionsVec {
        self.row_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_const_slice(ROW_STYLE))
    }

    /// The `separator_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_separator_style(&self) -> CssPropertyWithConditionsVec {
        self.separator_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_separator(&self.theme))
    }

    /// The `large_button_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_large_button_style(&self) -> CssPropertyWithConditionsVec {
        self.large_button_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_large_button(&self.theme))
    }

    /// The `large_icon_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_large_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.large_icon_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_large_icon(&self.theme))
    }

    /// The `large_label_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_large_label_style(&self) -> CssPropertyWithConditionsVec {
        self.large_label_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_large_label(&self.theme))
    }

    /// The `small_button_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_small_button_style(&self) -> CssPropertyWithConditionsVec {
        self.small_button_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_small_button(&self.theme))
    }

    /// The `small_icon_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_small_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.small_icon_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_small_icon(&self.theme))
    }

    /// The `small_label_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_small_label_style(&self) -> CssPropertyWithConditionsVec {
        self.small_label_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_small_label(&self.theme))
    }

    /// The `arrow_icon_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_arrow_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.arrow_icon_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_arrow_icon(&self.theme))
    }

    /// The `checked_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_checked_style(&self) -> CssPropertyWithConditionsVec {
        self.checked_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_checked(&self.theme))
    }

    /// The `gallery_frame_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_frame_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_frame_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_frame(&self.theme))
    }

    /// The `gallery_strip_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_strip_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_strip_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_const_slice(GALLERY_STRIP_STYLE))
    }

    /// The `gallery_cell_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_cell_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_cell_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_cell(&self.theme))
    }

    /// The `gallery_cell_selected_style` this bundle renders with: the caller's override if there
    /// is one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_cell_selected_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_cell_selected_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_cell_selected(&self.theme))
    }

    /// The `gallery_cell_label_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_cell_label_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_cell_label_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_cell_label(&self.theme))
    }

    /// The `gallery_spinner_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_spinner_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_spinner_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_spinner(&self.theme))
    }

    /// The `gallery_wrapper_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_wrapper_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_wrapper_style
            .clone()
            .into_option()
            .unwrap_or_else(|| {
                CssPropertyWithConditionsVec::from_const_slice(GALLERY_WRAPPER_STYLE)
            })
    }

    /// The `gallery_panel_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_panel_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_panel_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_panel(&self.theme))
    }

    /// The `mobile_tab_button_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_tab_button_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_tab_button_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_tab_button(&self.theme))
    }

    /// The `mobile_tab_label_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_tab_label_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_tab_label_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_tab_label(&self.theme))
    }

    /// The `mobile_tab_arrow_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_tab_arrow_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_tab_arrow_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_tab_arrow(&self.theme))
    }

    /// The `mobile_tab_overlay_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_tab_overlay_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_tab_overlay_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_tab_overlay(&self.theme))
    }

    /// The `mobile_tab_overlay_item_style` this bundle renders with: the caller's override if there
    /// is one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_tab_overlay_item_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_tab_overlay_item_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_tab_overlay_item(&self.theme))
    }

    /// The `mobile_group_list_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_group_list_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_group_list_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_group_list(&self.theme, self.is_left_handed()))
    }

    /// The `mobile_group_list_item_style` this bundle renders with: the caller's override if there
    /// is one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_group_list_item_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_group_list_item_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_group_list_item(&self.theme))
    }

    /// The `mobile_group_list_item_selected_style` this bundle renders with: the caller's override
    /// if there is one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_mobile_group_list_item_selected_style(&self) -> CssPropertyWithConditionsVec {
        self.mobile_group_list_item_selected_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_mobile_group_list_item_selected(&self.theme))
    }

    /// The `gallery_spinner_button_style` this bundle renders with: the caller's override if there
    /// is one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_spinner_button_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_spinner_button_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_spinner_button(&self.theme))
    }

    /// The `gallery_spinner_icon_style` this bundle renders with: the caller's override if there is
    /// one, else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_gallery_spinner_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.gallery_spinner_icon_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_gallery_spinner_icon(&self.theme))
    }

    /// The `group_popup_style` this bundle renders with: the caller's override if there is one,
    /// else derived from [`Self::theme`].
    #[must_use]
    pub fn resolved_group_popup_style(&self) -> CssPropertyWithConditionsVec {
        self.group_popup_style
            .clone()
            .into_option()
            .unwrap_or_else(|| theme_group_popup(&self.theme))
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
        combo.wrapper_style = OptionCssPropertyWithConditionsVec::Some(
            CssPropertyWithConditionsVec::from_vec(wrapper),
        );
        combo.field_style =
            OptionCssPropertyWithConditionsVec::Some(theme_combo_field(&self.theme));
        combo.text_style = OptionCssPropertyWithConditionsVec::Some(
            CssPropertyWithConditionsVec::from_const_slice(RIBBON_COMBO_TEXT_STYLE),
        );
        combo.arrow_style =
            OptionCssPropertyWithConditionsVec::Some(theme_combo_arrow(&self.theme));
        combo
    }
}

impl Default for RibbonStyle {
    fn default() -> Self {
        Self::office_2010()
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
    /// The tab strip as the window's title bar ([`TabsInTitlebar`]), or
    /// `None` for a ribbon under a title row of its own.
    pub tabs_in_titlebar: OptionTabsInTitlebar,
    /// All part styles (defaults to the Office 2010 look).
    pub style: RibbonStyle,
    /// Which interactions the ribbon handles by itself (defaults to the classic behavior).
    pub behavior: RibbonBehavior,
    /// The widget theme, or `None` to follow the app theme
    /// (`AppConfig::with_theme`). Flat is the Office look [`Self::style`]
    /// describes; flora lays flora's paper, hairlines and stones on the same
    /// metrics. A part the caller set in [`Self::style`] wins in either theme.
    pub theme: OptionUiTheme,
    /// The width the ribbon is laid out in, px, which the active tab's
    /// groups scale down to when they do not fit (Office's ribbon scaling,
    /// see the module docs). 0 = the width of the window the DOM is built
    /// for, asked of the window so a resize across a scaling step rebuilds
    /// the ribbon; set it for a ribbon in a narrower box than its window.
    pub available_width: f32,
}

/// The application button at the far left of the tab strip ("FILE").
#[derive(Debug, Clone)]
#[repr(C)]
pub struct RibbonAppButton {
    /// Display label of the application button.
    pub label: AzString,
    /// Optional click callback.
    pub on_click: OptionButtonOnClick,
    /// The drop-down the button opens - Windows 8 Explorer's File menu
    /// ([`crate::widgets::ribbon_file_menu::RibbonFileMenu::dom`]), or any
    /// DOM, which then shows in a window of its own: hung on the button, it
    /// opens under the File tab and its click opens and closes it (next to
    /// `on_click`, if any). `None`: the button only runs `on_click` (Office's
    /// full-window backstage is the app's to show).
    pub menu: OptionDom,
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
    /// `None` (the default) leaves the tab looking like every other one; `Some`
    /// APPENDS, so a caller cannot drop the shared tab style the strip depends
    /// on. An empty vec used to be the sentinel for "no extras", which is the
    /// confusion this option removes.
    /// This is the only per-tab hook: `RibbonStyle` describes the tab
    /// STRIP, so without it a single tab could not be tinted, badged or
    /// given its own border, and telling two tabs apart in a screenshot
    /// meant reading their labels.
    pub style: OptionCssPropertyWithConditionsVec,
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
    /// The icon of the one button the group becomes when its tab is too
    /// wide for the ribbon and the group collapses (Office's group image):
    /// the button opens the whole group in a popup. Empty = the icon of the
    /// group's first button.
    pub icon: AzString,
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
/// [`Button`](crate::widgets::button::Button) widget with ribbon styles injected.
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
    /// Why the command cannot run now ("Select a file to rename"); empty =
    /// enabled. A disabled button keeps its place and its keyboard stop, is
    /// dimmed, never runs `on_click`, is announced unavailable with this
    /// reason as its description, and shows the reason as a tooltip on hover
    /// and on click (Office's greyed commands and their tooltips).
    pub disabled_reason: AzString,
    /// The name of an icon-only button (empty `label`): what a screen reader
    /// says ("Bold"). Office's Font and Alignment groups are rows of such
    /// buttons; without a name each was announced as "button". Empty = the
    /// label names the button.
    pub alt: AzString,
    /// The ARROW part of a split button (`RibbonArrow::Split`): what a click
    /// on the ▾ runs - the app opens the button's menu there - while
    /// [`Self::on_click`] stays the primary command (Office's Paste: the
    /// clipboard icon pastes, "Paste ▾" under it opens the paste options).
    /// With this set a split button is two hit targets, each with its own
    /// hover; unset it is one button with an arrow, like `Menu`.
    pub on_arrow_click: OptionButtonOnClick,
}

/// Drop-down decoration of a [`RibbonButton`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum RibbonArrow {
    /// Plain button without an arrow.
    #[default]
    None,
    /// The whole button opens a menu (`RibbonX` `menu`): its `on_click`
    /// opens it. A large one draws the ▾ under its label, a small one after.
    Menu,
    /// Primary action + separate arrow region (`RibbonX` `splitButton`). With
    /// [`RibbonButton::on_arrow_click`] set the two are separate buttons (a
    /// large one: the icon over the label and ▾; a small one: icon and label
    /// beside the ▾); without it, it renders like `Menu`.
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
    /// How many cells the in-ribbon strip shows (0: every cell). Office's
    /// galleries show one row of a few cells - the row holding the selected
    /// one - and "More" opens all of them; a gallery of every cell inline
    /// pushed the groups after it off a 1280 px window (AzShow's Layout).
    pub visible: usize,
    /// 0: the classic strip - preview cells over their names, one row (Word's
    /// Styles). N: a LIST gallery of commands in N columns three rows high,
    /// each cell its preview (an icon) beside its name - Outlook's Quick Steps
    /// ("Move to: ?", "Team E-mail", "Reply & Delete" | "To Manager", "Done",
    /// "Create New"). A list gallery keeps no selection: a cell is a command
    /// (`on_select` runs it), so no cell stays lit. `visible` 0 shows N x 3
    /// cells; the rest are behind "More".
    pub columns: usize,
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
            menu: OptionDom::None,
        }
    }

    /// Sets the drop-down the button opens (see [`Self::menu`]).
    pub fn set_menu(&mut self, menu: Dom) {
        self.menu = OptionDom::Some(menu);
    }

    /// Builder method: sets the drop-down the button opens and returns `self`.
    #[must_use]
    pub fn with_menu(mut self, menu: Dom) -> Self {
        self.set_menu(menu);
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

impl RibbonTab {
    /// Creates a new tab with the given label and no groups.
    #[must_use]
    pub const fn new(label: AzString) -> Self {
        Self {
            label,
            groups: RibbonGroupVec::from_const_slice(&[]),
            style: OptionCssPropertyWithConditionsVec::None,
        }
    }

    /// Appends per-tab style properties to this tab's header (see
    /// [`RibbonTab::style`]).
    pub fn set_style(&mut self, style: CssPropertyWithConditionsVec) {
        self.style = OptionCssPropertyWithConditionsVec::Some(style);
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

    /// Appends every group of `groups`, in order.
    pub fn add_groups(&mut self, groups: RibbonGroupVec) {
        for group in groups.as_ref() {
            self.groups.push(group.clone());
        }
    }

    /// Builder method: appends every group of `groups` and returns `self`.
    #[must_use]
    pub fn with_groups(mut self, groups: RibbonGroupVec) -> Self {
        self.add_groups(groups);
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
            icon: AzString::from_const_str(""),
        }
    }

    /// Builder method: makes this group absorb the remaining ribbon width.
    #[must_use]
    pub const fn with_fills_space(mut self, fills_space: bool) -> Self {
        self.fills_space = fills_space;
        self
    }

    /// Sets the icon the group's button shows when the group collapses
    /// (see [`Self::icon`]).
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// Builder method: [`Self::set_icon`].
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
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

    /// Appends every item of `items`, in order (what folding
    /// [`Self::with_item`] over a list does).
    pub fn add_items(&mut self, items: RibbonItemVec) {
        for item in items.as_ref() {
            self.items.push(item.clone());
        }
    }

    /// Builder method: appends every item of `items` and returns `self`.
    #[must_use]
    pub fn with_items(mut self, items: RibbonItemVec) -> Self {
        self.add_items(items);
        self
    }

    /// Sets the dialog-box-launcher callback (renders the launcher button).
    pub fn set_launcher<C: Into<super::button::ButtonOnClickCallback>>(
        &mut self,
        data: RefAny,
        on_click: C,
    ) {
        self.launcher = Some(super::button::ButtonOnClick::create(data, on_click)).into();
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

    /// Appends every item of `items`, in order (what folding
    /// [`Self::with_item`] over a list does).
    pub fn add_items(&mut self, items: RibbonItemVec) {
        for item in items.as_ref() {
            self.items.push(item.clone());
        }
    }

    /// Builder method: appends every item of `items` and returns `self`.
    #[must_use]
    pub fn with_items(mut self, items: RibbonItemVec) -> Self {
        self.add_items(items);
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

    /// Appends every item of `items`, in order (what folding
    /// [`Self::with_item`] over a list does).
    pub fn add_items(&mut self, items: RibbonItemVec) {
        for item in items.as_ref() {
            self.items.push(item.clone());
        }
    }

    /// Builder method: appends every item of `items` and returns `self`.
    #[must_use]
    pub fn with_items(mut self, items: RibbonItemVec) -> Self {
        self.add_items(items);
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
            disabled_reason: AzString::from_const_str(""),
            alt: AzString::from_const_str(""),
            on_arrow_click: OptionButtonOnClick::None,
        }
    }

    /// Makes this a split button: `on_arrow_click` runs when its arrow part
    /// is clicked (see [`Self::on_arrow_click`]); the arrow becomes
    /// [`RibbonArrow::Split`].
    pub fn set_on_arrow_click<C: Into<super::button::ButtonOnClickCallback>>(
        &mut self,
        data: RefAny,
        on_arrow_click: C,
    ) {
        self.arrow = RibbonArrow::Split;
        self.on_arrow_click =
            Some(super::button::ButtonOnClick::create(data, on_arrow_click)).into();
    }

    /// Builder method: [`Self::set_on_arrow_click`].
    #[must_use]
    pub fn with_on_arrow_click<C: Into<super::button::ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        on_arrow_click: C,
    ) -> Self {
        self.set_on_arrow_click(data, on_arrow_click);
        self
    }

    /// Whether this button is drawn as two parts (a split button with its
    /// own arrow callback).
    #[must_use]
    pub fn is_split(&self) -> bool {
        self.arrow == RibbonArrow::Split && self.on_arrow_click.is_some()
    }

    /// Names an icon-only button for assistive technology (see
    /// [`Self::alt`]).
    pub fn set_alt(&mut self, alt: AzString) {
        self.alt = alt;
    }

    /// Builder method: [`Self::set_alt`].
    #[must_use]
    pub fn with_alt(mut self, alt: AzString) -> Self {
        self.set_alt(alt);
        self
    }

    /// Disables the button: `reason` says why the command cannot run now
    /// (an empty reason enables it again). See [`Self::disabled_reason`].
    pub fn set_disabled(&mut self, reason: AzString) {
        self.disabled_reason = reason;
    }

    /// Builder method: disables the button with `reason` and returns `self`.
    #[must_use]
    pub fn with_disabled(mut self, reason: AzString) -> Self {
        self.set_disabled(reason);
        self
    }

    /// Whether the button is disabled (it has a reason).
    #[must_use]
    pub fn is_disabled(&self) -> bool {
        !self.disabled_reason.as_str().is_empty()
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
            visible: 0,
            columns: 0,
        }
    }

    /// Lays the gallery out as a list of commands in `columns` columns
    /// (0: the classic strip); see [`Self::columns`].
    pub const fn set_columns(&mut self, columns: usize) {
        self.columns = columns;
    }

    /// Builder method: [`Self::set_columns`].
    #[must_use]
    pub const fn with_columns(mut self, columns: usize) -> Self {
        self.set_columns(columns);
        self
    }

    /// Shows `visible` cells in the ribbon (0: every cell); see
    /// [`Self::visible`].
    pub const fn set_visible(&mut self, visible: usize) {
        self.visible = visible;
    }

    /// Builder method: [`Self::set_visible`].
    #[must_use]
    pub const fn with_visible(mut self, visible: usize) -> Self {
        self.set_visible(visible);
        self
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
    /// Office 2010 default style ([`RibbonStyle::office_2010`]).
    #[must_use]
    pub fn new(tabs: RibbonTabVec) -> Self {
        Self {
            app_button: None.into(),
            tabs,
            active_tab: 0,
            on_tab_click: None.into(),
            tabs_in_titlebar: OptionTabsInTitlebar::None,
            style: RibbonStyle::office_2010(),
            behavior: RibbonBehavior::office_2013(),
            theme: OptionUiTheme::None,
            available_width: 0.0,
        }
    }

    /// The width the ribbon is laid out in, px (see
    /// [`Self::available_width`]); 0 or a width that is not a number: the
    /// window's.
    pub fn set_available_width(&mut self, width: f32) {
        self.available_width = if width.is_finite() {
            width.max(0.0)
        } else {
            0.0
        };
    }

    /// [`Self::set_available_width`] for the builder chain.
    #[must_use]
    pub fn with_available_width(mut self, width: f32) -> Self {
        self.set_available_width(width);
        self
    }

    /// Pick the widget theme: the ribbon, its buttons and every embedded
    /// widget without a theme of its own keep this look whatever the app
    /// theme is. Unset (`None`), the ribbon follows the app theme
    /// (`AppConfig::with_theme`, flat by default).
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
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

    /// Makes the tab strip the window's title bar (Firefox's tabs in the
    /// titlebar): the strip runs up to the window's top edge and holds its
    /// tabs `chrome`'s offsets in from it, clear of the window controls, and
    /// everything around the tabs moves the window - a double click
    /// maximizes it (zooms it on macOS). The tabs and the application button
    /// stay controls. For a `WindowDecorations::NoTitle` window that shows no
    /// title row above the ribbon.
    pub const fn set_tabs_in_titlebar(&mut self, chrome: TabsInTitlebar) {
        self.tabs_in_titlebar = OptionTabsInTitlebar::Some(chrome);
    }

    /// [`Self::set_tabs_in_titlebar`] for the builder chain.
    #[must_use]
    pub const fn with_tabs_in_titlebar(mut self, chrome: TabsInTitlebar) -> Self {
        self.set_tabs_in_titlebar(chrome);
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
        self.themed(RibbonChromeMode::Adaptive)
    }

    /// Builds ONLY the desktop chrome (tab strip + content band), with no
    /// mobile nodes and no viewport conditions. Pair with
    /// [`Self::dom_mobile`] by branching on
    /// `LayoutCallbackInfo::viewport_bigger_than` in `layout()` - the
    /// framework re-invokes `layout()` on every resize, so crossing the
    /// breakpoint swaps the structure.
    #[must_use]
    pub fn dom_desktop(self) -> Dom {
        self.themed(RibbonChromeMode::Desktop)
    }

    /// Builds ONLY the touch chrome: the full-width active-tab button (tap
    /// opens the fullscreen tab picker, double-tap collapses the band), the
    /// scrollable group list on the dominant-hand side, and ONE visible
    /// group at a time (tapping a list entry swaps it in with no app
    /// relayout). See [`Self::dom_desktop`] for the pairing contract.
    #[must_use]
    pub fn dom_mobile(self) -> Dom {
        self.themed(RibbonChromeMode::Mobile)
    }

    /// `mode`'s chrome in the ribbon's theme: a pinned theme is that look;
    /// no theme follows the app theme - the ribbon built in both looks and
    /// merged into ONE tree in the structure of the theme the DOM is built
    /// for (`UiTheme::current()`), every node carrying each look's
    /// declarations in its `@theme(<name>)` block.
    ///
    /// The active tab's scaling is decided once, before either look is
    /// built: both lay their parts out on the same metrics, so both build the
    /// same structure - and the window is asked about each width once.
    fn themed(self, mode: RibbonChromeMode) -> Dom {
        let scales = self.scales_for(mode);
        match self.theme.into_option() {
            Some(theme) => self.build_in(theme, mode, &scales),
            None => {
                let flat = self.clone().build_in(UiTheme::Flat, mode, &scales);
                let flora = self.build_in(UiTheme::Flora, mode, &scales);
                crate::widgets::themes::theme_blocks::follow_dom(UiTheme::current(), flat, flora)
            }
        }
    }

    /// How the active tab's groups are drawn (Office's ribbon scaling,
    /// [`walk_scaling_steps`]): the first step - every group as the app built
    /// it, then narrower and narrower - whose width fits the ribbon's. The
    /// width is [`Self::available_width`] when the app set one, else the
    /// window's the DOM is built for, asked through the recorded size queries
    /// (`build_window_width_less_than`): only the widths of the steps tried,
    /// widest first, so a resize rebuilds the ribbon exactly when it crosses
    /// one of them. Outside a window's build, with no width set, every group
    /// is drawn as built. The touch chrome shows one group at a time and is
    /// never scaled.
    fn scales_for(&self, mode: RibbonChromeMode) -> Vec<GroupScale> {
        let Some(tab) = self.tabs.as_slice().get(self.active_tab) else {
            return Vec::new();
        };
        let groups = tab.groups.as_slice();
        if matches!(mode, RibbonChromeMode::Mobile) {
            return vec![GroupScale::FULL; groups.len()];
        }
        let available = self.available_width;
        if available > 0.0 {
            chosen_scales(groups, |total| total + FIT_SLACK_PX <= available)
        } else {
            chosen_scales(groups, |total| {
                !matches!(
                    azul_core::callbacks::build_window_width_less_than(total + FIT_SLACK_PX),
                    Some(true)
                )
            })
        }
    }

    /// `mode`'s chrome in exactly `theme`'s look: flat is the palette's own
    /// parts; flora fills every part the caller left `None` with flora's
    /// paint on the same geometry (`themes::flora::ribbon_style`) - and its
    /// own tab row, which it cuts as Firefox's (Australis): its application
    /// button and selected tab carry curves, unless the caller's part
    /// replaced flora's. The active tab's groups are drawn at `scales`.
    fn build_in(mut self, theme: UiTheme, mode: RibbonChromeMode, scales: &[GroupScale]) -> Dom {
        let mut curves = TabRowCurves::default();
        if theme == UiTheme::Flora {
            use crate::widgets::themes::flora;
            if self.style.app_button_style.is_none() {
                curves.app_button = Some(flora::tab_curves(true));
            }
            if self.style.tab_active_style.is_none() {
                curves.selected_tab = Some(flora::tab_curves(false));
            }
            self.style = flora::ribbon_style(self.style);
        }
        self.build_chrome(mode, theme, &curves, scales)
    }

    fn build_chrome(
        self,
        mode: RibbonChromeMode,
        theme: UiTheme,
        curves: &TabRowCurves,
        scales: &[GroupScale],
    ) -> Dom {
        let Self {
            app_button,
            tabs,
            active_tab,
            on_tab_click,
            tabs_in_titlebar,
            style,
            behavior,
            // The look to build is `theme`: the field is the caller's pin,
            // already resolved by `themed`.
            theme: _,
            // Already turned into `scales` by `themed`.
            available_width: _,
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
        // The first tab stands right after the application button when there
        // is one: a stone of its own, over the strip's rule.
        let after_the_app_button = app_button.is_some();

        if let Some(ab) = app_button.into_option() {
            let RibbonAppButton {
                label,
                on_click,
                menu,
            } = ab;
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_APP_BUTTON))
                .with_css_props(as_control(
                    style.resolved_app_button_style(),
                    tabs_in_titlebar,
                ))
                .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                    label,
                )]));
            if let Some(look) = curves.app_button.as_ref() {
                for curve in crate::widgets::tabs::australis_curves(look) {
                    d.add_child(curve);
                }
            }
            if let Some(oc) = on_click.into_option() {
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
            // The File menu: hung on the button (its anchor) as a window the
            // button's click opens and closes - after its other parts, so the
            // label stays the button's first child.
            if let Some(menu) = menu.into_option() {
                crate::widgets::ribbon_file_menu::hang_on_app_button(&mut d, menu);
            }
            bar_children.push(d);
        }

        // Private chrome state shared by every tab header: the collapse and
        // hover-peek behaviors are driven from here, so the application's own
        // data model never has to model ribbon chrome.
        // Mobile starts COLLAPSED. On a phone the content band eats most of the
        // viewport and the document is what the user came for; the classic
        // office suites do the same thing at this size. The seed has to agree
        // with the initial DOM below — `set_content_visible` only ever runs
        // from a callback, so a state that says "collapsed" over a band that
        // renders expanded would make the first double-tap a no-op.
        let chrome = RefAny::new(RibbonChromeState {
            collapsed: matches!(mode, RibbonChromeMode::Mobile),
        });

        for (idx, tab) in tabs.as_slice().iter().enumerate() {
            let (classes, part_style) = if idx == active_tab {
                (CLS_TAB_ACTIVE, style.resolved_tab_active_style())
            } else {
                (CLS_TAB, style.resolved_tab_style())
            };
            // Per-tab properties go AFTER the shared ones so they win, and
            // they are applied in both the active and inactive state.
            let part_style = match tab.style.as_ref() {
                None => part_style,
                Some(extra) => {
                    let mut merged = part_style.into_library_owned_vec();
                    merged.extend(extra.as_ref().iter().cloned());
                    CssPropertyWithConditionsVec::from_vec(merged)
                }
            };
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
                .with_css_props(as_control(part_style, tabs_in_titlebar))
                .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_with_text(
                    tab.label.clone(),
                )]));
            // The selected tab's curves go after its label: the label stays
            // the tab's first child in every look. Right after the
            // application button it hangs no run-out on that side: the
            // button's stone covers the rule there.
            if idx == active_tab {
                if let Some(look) = curves.selected_tab.as_ref() {
                    let parts = if idx == 0 && after_the_app_button {
                        crate::widgets::tabs::australis_curves(&look.after_a_stone())
                    } else {
                        crate::widgets::tabs::australis_curves(look)
                    };
                    for curve in parts {
                        d.add_child(curve);
                    }
                }
            }

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
                .with_css_props(style.resolved_tab_filler_style()),
        );

        // In the titlebar the strip runs up to the window's edges, holds its
        // tabs clear of the window controls and moves the window.
        let tab_bar_style = match tabs_in_titlebar.into_option() {
            Some(chrome) => chrome.strip_style(&style.resolved_tab_bar_style()),
            None => style.resolved_tab_bar_style(),
        };
        let tab_bar = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_TAB_BAR))
            .with_css_props(tab_bar_style)
            .with_children(DomVec::from_vec(bar_children));

        let mut group_doms: Vec<Dom> =
            match tabs.into_library_owned_vec().into_iter().nth(active_tab) {
                Some(active) => active
                    .groups
                    .into_library_owned_vec()
                    .into_iter()
                    .enumerate()
                    .map(|(i, g)| {
                        let scale = scales.get(i).copied().unwrap_or(GroupScale::FULL);
                        group_dom(g, &style, behavior, theme, scale)
                    })
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
            .with_css_props(style.resolved_mobile_tab_button_style())
            .with_children(DomVec::from_vec(vec![
                crate::widgets::widget_p_chrome()
                    .with_css_props(style.resolved_mobile_tab_label_style())
                    .with_children(DomVec::from_vec(vec![
                        Dom::create_text_do_not_use_without_block_level_wrapper(active_label),
                    ])),
                Dom::create_icon(AzString::from_const_str("expand_more"))
                    .with_css_props(style.resolved_mobile_tab_arrow_style()),
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
                    .with_css_props(style.resolved_mobile_tab_overlay_item_style())
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
            .with_css_props(style.resolved_mobile_tab_overlay_style())
            .with_children(DomVec::from_vec(overlay_items));

        // Group list: on phones ONE group is visible and the rest are a
        // scrollable list on the dominant-hand side.
        let group_list_items: Vec<Dom> = group_labels
            .iter()
            .enumerate()
            .map(|(idx, label)| {
                let item_style = if idx == 0 {
                    merged_style(
                        &style.resolved_mobile_group_list_item_style(),
                        &style.resolved_mobile_group_list_item_selected_style(),
                    )
                } else {
                    style.resolved_mobile_group_list_item_style()
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
                                selected_style: style
                                    .resolved_mobile_group_list_item_selected_style(),
                                base_style: style.resolved_mobile_group_list_item_style(),
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
            .with_css_props(style.resolved_mobile_group_list_style())
            .with_children(DomVec::from_vec(group_list_items));

        let content = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_CONTENT))
            .with_css_props(style.resolved_content_style())
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
                // Collapsed to start — the other half of the `chrome` seed
                // above. Only the tab button shows; tapping it expands.
                let mut band = band;
                band.root
                    .upsert_inline_css_property(P::const_display(LayoutDisplay::None));
                vec![mobile_tab_button, mobile_tab_overlay, band]
            }
        };
        // The root carries the theme marker (`__azul-theme-<name>`), like
        // every themed widget: the look it was BUILT in.
        let mut container = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![
                CLS_RIBBON[0].clone(),
                style_kit::marker(theme),
            ]))
            .with_css_props(style.resolved_container_style())
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

/// The Australis curves a look hangs off the tab row's stones: flora's own
/// application button and selected tab (`themes::flora::tab_curves`).
/// `None` where the look draws none - flat's Office tabs are boxes - or
/// where the caller's own part replaced the look's.
#[derive(Debug, Clone, Default)]
struct TabRowCurves {
    app_button: Option<crate::widgets::tabs::TabCurveLook>,
    selected_tab: Option<crate::widgets::tabs::TabCurveLook>,
}

/// A tab's or the application button's part, with
/// [`TabsInTitlebar::control`] appended when the strip is the window's title
/// bar: pressing the control presses it, and dragging it does not move the
/// window.
fn as_control(
    part: CssPropertyWithConditionsVec,
    chrome: OptionTabsInTitlebar,
) -> CssPropertyWithConditionsVec {
    if chrome.is_none() {
        return part;
    }
    let mut v = part.into_library_owned_vec();
    v.push(TabsInTitlebar::control());
    CssPropertyWithConditionsVec::from_vec(v)
}

/// `base` with `extra` appended (inline CSS resolves last-wins, so `extra`
/// overrides `base` where they collide).
fn merged_style(
    base: &CssPropertyWithConditionsVec,
    extra: &CssPropertyWithConditionsVec,
) -> CssPropertyWithConditionsVec {
    crate::widgets::themes::theme_blocks::stack_parts(base, extra)
}

/// Added to a disabled ribbon button ([`RibbonButton::disabled_reason`]),
/// next to the Button's own `BUTTON_DISABLED_CLASS`: the disabled state
/// itself (dimmed, inert, unavailable, the reason as tooltip) is the
/// Button's (`Button::with_disabled`).
pub const RIBBON_DISABLED_CLASS: &str = "__azul-native-ribbon-button-disabled";

// -- Office's control kinds --
//
// A large button is Office's 32 px icon over its label. A label of two or
// more words is set on two balanced lines ("New / E-mail", "Send/Receive /
// All Folders") and a menu's ▾ follows its last word on the second line; a
// one-word label stays on one line with the ▾ under it ("Move / ▾"). A split
// button with its own arrow callback is two buttons: a large one the icon
// over "Paste ▾", a small one the icon and label beside the ▾. Small items -
// small buttons, check boxes, combo and drop-down boxes - stack three to a
// column. A list gallery (`RibbonGallery::columns`) is Outlook's Quick
// Steps: its commands, icon beside name, in columns of three.

/// The class of a split button's wrapper (its main part and its arrow part).
pub const RIBBON_SPLIT_CLASS: &str = "__azul-native-ribbon-split";
/// Added to a split button's main part.
pub const RIBBON_SPLIT_MAIN_CLASS: &str = "__azul-native-ribbon-split-main";
/// Added to a split button's arrow part.
pub const RIBBON_SPLIT_ARROW_CLASS: &str = "__azul-native-ribbon-split-arrow";
static CLS_SPLIT: &[IdOrClass] = &[Class(AzString::from_const_str(RIBBON_SPLIT_CLASS))];
static CLS_LARGE_CONTENT: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-large-content",
))];
static CLS_LARGE_LABEL: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-large-label",
))];
static CLS_GALLERY_COLUMN: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-column",
))];
static CLS_GALLERY_CELL_ICON: &[IdOrClass] = &[Class(AzString::from_const_str(
    "__azul-native-ribbon-gallery-cell-icon",
))];
/// Added to the strip of a list gallery (`RibbonGallery::columns` > 0).
pub const RIBBON_GALLERY_LIST_CLASS: &str = "__azul-native-ribbon-gallery-list";

/// Added to a group the ribbon collapsed into one button (a tab too wide for
/// the ribbon, see the module docs), next to the group's own class.
pub const RIBBON_GROUP_COLLAPSED_CLASS: &str = "__azul-native-ribbon-group-collapsed";
/// The button a collapsed group is: its icon over its label and ▾.
pub const RIBBON_GROUP_BUTTON_CLASS: &str = "__azul-native-ribbon-group-button";
/// The `<transient-window>` a collapsed group's button opens: the group's
/// last child, so the group is its anchor.
pub const RIBBON_GROUP_POPUP_WINDOW_CLASS: &str = "__azul-native-ribbon-group-popup-window";
/// The panel inside it that holds the whole group.
pub const RIBBON_GROUP_POPUP_CLASS: &str = "__azul-native-ribbon-group-popup";
static CLS_GROUP_POPUP_WINDOW: &[IdOrClass] = &[Class(AzString::from_const_str(
    RIBBON_GROUP_POPUP_WINDOW_CLASS,
))];
static CLS_GROUP_POPUP: &[IdOrClass] = &[Class(AzString::from_const_str(RIBBON_GROUP_POPUP_CLASS))];

/// How many small items Office stacks in one column, and how many rows a
/// list gallery has: three 22 px rows fill the 68 px item area.
const ROWS_PER_COLUMN: usize = 3;

/// The no-break space that joins the words of one line of a large label.
const NO_BREAK_SPACE: char = '\u{a0}';

/// A large button's content when its label wraps: the icon over the label
/// block, centred.
static LARGE_CONTENT_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The 32 px icon of a wrapped large button: one icon tall, so the icon and
/// two label lines fit the 66 px button.
static LARGE_ICON_LINE_STYLE: &[Cond] = &[Cond::simple(P::const_line_height(
    StyleLineHeight::Length(PixelValue::const_px(32)),
))];

/// The block around a wrapped label: a block formatting context, so the
/// label inside it can be as wide as its longer line (a flex item cannot
/// be `width: min-content`). Its own line is the label's 13 px, so its strut
/// adds nothing under the label.
static LARGE_LABEL_BLOCK_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Block)),
    Cond::simple(P::const_text_align(StyleTextAlign::Center)),
    Cond::simple(P::const_line_height(StyleLineHeight::Length(
        PixelValue::const_px(13),
    ))),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// APPENDED to the large label style for a wrapped label: the label is as
/// wide as its longer line (its words are joined by no-break spaces but at
/// the one break) and its lines are 13 px, two of them under the 32 px icon:
/// 58 px, the content box of the 66 px button.
static LARGE_LABEL_LINES_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::InlineBlock)),
    Cond::simple(P::const_width(LayoutWidth::MinContent)),
    Cond::simple(P::const_text_align(StyleTextAlign::Center)),
    Cond::simple(P::const_vertical_align(StyleVerticalAlign::Top)),
    Cond::simple(P::const_margin_top(LayoutMarginTop::const_px(0))),
    Cond::simple(P::const_line_height(StyleLineHeight::Length(
        PixelValue::const_px(13),
    ))),
];

/// A split button's wrapper: the two parts stacked (large) or side by side
/// (small), each part a full button with its own hover.
static SPLIT_LARGE_STYLE: &[Cond] = &[
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_align_items(LayoutAlignItems::Stretch)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    Cond::simple(P::const_height(LayoutHeight::const_px(66))),
    Cond::simple(P::const_margin_right(LayoutMarginRight::const_px(1))),
];

static SPLIT_SMALL_STYLE: &[Cond] = &[
    Cond::simple(P::const_box_sizing(LayoutBoxSizing::BorderBox)),
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_align_items(LayoutAlignItems::Stretch)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
    Cond::simple(P::const_height(LayoutHeight::const_px(22))),
];

/// APPENDED to the large button style for a split button's main part: the
/// icon alone, the top 38 px of the 66 px button.
static SPLIT_LARGE_MAIN_STYLE: &[Cond] = &[
    Cond::simple(P::const_height(LayoutHeight::const_px(38))),
    Cond::simple(P::const_margin_right(LayoutMarginRight::const_px(0))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(1))),
    Cond::simple(P::const_justify_content(LayoutJustifyContent::Center)),
];

/// APPENDED to the large button style for a split button's arrow part: the
/// label and ▾ in the rest of the button.
static SPLIT_LARGE_ARROW_STYLE: &[Cond] = &[
    Cond::simple(P::const_height(LayoutHeight::Auto)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(1))),
    Cond::simple(P::const_margin_right(LayoutMarginRight::const_px(0))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(0))),
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(3))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(3))),
    Cond::simple(P::const_justify_content(LayoutJustifyContent::Start)),
];

/// APPENDED to the small button style for a split button's main part.
static SPLIT_SMALL_MAIN_STYLE: &[Cond] = &[Cond::simple(P::const_padding_right(
    LayoutPaddingRight::const_px(2),
))];

/// APPENDED to the small button style for a split button's arrow part: a
/// 14 px column for the ▾.
static SPLIT_SMALL_ARROW_STYLE: &[Cond] = &[
    Cond::simple(P::const_width(LayoutWidth::const_px(14))),
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(0))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(0))),
    Cond::simple(P::const_justify_content(LayoutJustifyContent::Center)),
];

/// One column of a list gallery: three cells, as wide as the widest.
static GALLERY_LIST_COLUMN_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Column)),
    Cond::simple(P::const_align_items(LayoutAlignItems::Stretch)),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// APPENDED to the gallery cell style for a list gallery's cell: a 22 px
/// row, the icon beside the name, no rule between the cells.
static GALLERY_LIST_CELL_STYLE: &[Cond] = &[
    Cond::simple(P::const_flex_direction(LayoutFlexDirection::Row)),
    Cond::simple(P::const_justify_content(LayoutJustifyContent::Start)),
    Cond::simple(P::const_width(LayoutWidth::Auto)),
    Cond::simple(P::const_height(LayoutHeight::const_px(22))),
    Cond::simple(P::const_padding_top(LayoutPaddingTop::const_px(1))),
    Cond::simple(P::const_padding_bottom(LayoutPaddingBottom::const_px(1))),
    Cond::simple(P::const_padding_left(LayoutPaddingLeft::const_px(3))),
    Cond::simple(P::const_padding_right(LayoutPaddingRight::const_px(8))),
    Cond::simple(P::const_border_right_width(LayoutBorderRightWidth::const_px(0))),
];

/// The 16 px box a list gallery cell's preview (its icon) sits in.
static GALLERY_LIST_ICON_STYLE: &[Cond] = &[
    Cond::simple(P::const_display(LayoutDisplay::Flex)),
    Cond::simple(P::const_align_items(LayoutAlignItems::Center)),
    Cond::simple(P::const_justify_content(LayoutJustifyContent::Center)),
    Cond::simple(P::const_width(LayoutWidth::const_px(16))),
    Cond::simple(P::const_flex_grow(LayoutFlexGrow::const_new(0))),
    Cond::simple(P::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })),
];

/// The word a balanced two-line label breaks before: the boundary that makes
/// the longer line shortest - a tie keeps the longer FIRST line, as Office's
/// "Run Rules / Now". `None` for a label of one word.
fn balanced_break(words: &[&str]) -> Option<usize> {
    if words.len() < 2 {
        return None;
    }
    let widths: Vec<usize> = words.iter().map(|w| w.chars().count()).collect();
    let total = widths.iter().sum::<usize>() + words.len() - 1;
    let mut best: Option<(usize, usize)> = None;
    let mut first = 0_usize;
    for k in 1..words.len() {
        first += widths[k - 1] + usize::from(k > 1);
        let longer = first.max(total - first - 1);
        if best.map_or(true, |(b, _)| longer <= b) {
            best = Some((longer, k));
        }
    }
    best.map(|(_, k)| k)
}

/// A large button's label text as Office sets it: the one break of
/// [`balanced_break`] a plain space and every other space a no-break one, so
/// the label's min-content width IS its longer line; with a menu, a
/// no-break space glues the ▾ (the next inline) to the last word - or, for
/// a one-word label, a plain space puts it on a line of its own.
fn large_label_text(label: &str, arrow: bool) -> String {
    let words: Vec<&str> = label.split_whitespace().collect();
    let nbsp = NO_BREAK_SPACE.to_string();
    let mut out = String::with_capacity(label.len() + 2);
    match balanced_break(&words) {
        Some(k) => {
            out.push_str(&words[..k].join(&nbsp));
            out.push(' ');
            out.push_str(&words[k..].join(&nbsp));
            if arrow {
                out.push(NO_BREAK_SPACE);
            }
        }
        None => {
            out.push_str(label.trim());
            if arrow {
                out.push(' ');
            }
        }
    }
    out
}

/// Whether a large button's label is set on two lines (two or more words).
fn label_wraps(label: &str) -> bool {
    label.split_whitespace().nth(1).is_some()
}

/// A large button's label block: a centred inline-block `<p>` as wide as its
/// longer line ([`large_label_text`]), the ▾ inline at its end.
fn large_label_block(label: &str, arrow: bool, s: &RibbonStyle) -> Dom {
    let lines = merged_style(
        &s.resolved_large_label_style(),
        &CssPropertyWithConditionsVec::from_const_slice(LARGE_LABEL_LINES_STYLE),
    );
    let mut children = vec![Dom::create_text_do_not_use_without_block_level_wrapper(
        AzString::from(large_label_text(label, arrow)),
    )];
    if arrow {
        children.push(
            Dom::create_icon(AzString::from_const_str("arrow_drop_down"))
                .with_css_props(s.resolved_arrow_icon_style()),
        );
    }
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_LARGE_LABEL))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
            LARGE_LABEL_BLOCK_STYLE,
        ))
        .with_children(DomVec::from_vec(vec![crate::widgets::widget_p_chrome()
            .with_css_props(lines)
            .with_children(DomVec::from_vec(children))]))
}

/// The content of a large button whose label wraps: the 32 px icon over the
/// label block. Handed to the Button as its icon DOM; the Button's own label
/// stays empty and its name is the label (`alt`).
fn large_content(icon: &AzString, label: &str, arrow: bool, s: &RibbonStyle) -> Dom {
    let mut children = Vec::with_capacity(2);
    if !icon.as_str().is_empty() {
        children.push(Dom::create_icon(icon.clone()).with_css_props(merged_style(
            &s.resolved_large_icon_style(),
            &CssPropertyWithConditionsVec::from_const_slice(LARGE_ICON_LINE_STYLE),
        )));
    }
    children.push(large_label_block(label, arrow, s));
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_LARGE_CONTENT))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(LARGE_CONTENT_STYLE))
        .with_children(DomVec::from_vec(children))
}

/// The name a button is announced by: its `alt`, else its label.
fn button_name(rb: &RibbonButton) -> AzString {
    if rb.alt.as_str().is_empty() {
        rb.label.clone()
    } else {
        rb.alt.clone()
    }
}

/// A built button node, marked disabled for the ribbon's own class.
fn finish_button(button: crate::widgets::button::Button, disabled: bool) -> Dom {
    let mut dom = button.dom();
    if disabled {
        dom.root.add_class(AzString::from_const_str(RIBBON_DISABLED_CLASS));
    }
    dom
}

fn expand_ribbon_button(rb: RibbonButton, large: bool, s: &RibbonStyle, theme: UiTheme) -> Dom {
    if rb.is_split() {
        return split_button_dom(rb, large, s, theme);
    }
    let disabled = rb.is_disabled();
    let base = if large {
        &s.resolved_large_button_style()
    } else {
        &s.resolved_small_button_style()
    };
    let container = if rb.toggled {
        merged_style(base, &s.resolved_checked_style())
    } else {
        base.clone()
    };
    let has_arrow = rb.arrow != RibbonArrow::None;
    let trailing = if has_arrow {
        AzString::from_const_str("arrow_drop_down")
    } else {
        AzString::from_const_str("")
    };
    let (icon_style, label_style) = if large {
        (
            s.resolved_large_icon_style(),
            s.resolved_large_label_style(),
        )
    } else {
        (
            s.resolved_small_icon_style(),
            s.resolved_small_label_style(),
        )
    };
    // A large label of two or more words is set on two lines (and its ▾ on
    // the second): the Button carries the icon and label block as its icon
    // DOM, and the label as its name.
    if large && label_wraps(rb.label.as_str()) {
        let name = button_name(&rb);
        let content = large_content(&rb.icon, rb.label.as_str(), has_arrow, s);
        let mut button = styled_button(
            rb.icon,
            AzString::from_const_str(""),
            AzString::from_const_str(""),
            container,
            icon_style,
            label_style,
            s.resolved_arrow_icon_style(),
            rb.on_click,
            rb.disabled_reason,
            name,
            OptionUiTheme::Some(theme),
        );
        button.icon_dom = OptionDom::Some(content);
        return finish_button(button, disabled);
    }
    // The Button drops a disabled command's click, dims it and says why.
    finish_button(
        styled_button(
            rb.icon,
            rb.label,
            trailing,
            container,
            icon_style,
            label_style,
            s.resolved_arrow_icon_style(),
            rb.on_click,
            rb.disabled_reason,
            rb.alt,
            OptionUiTheme::Some(theme),
        ),
        disabled,
    )
}

/// A split button with its own arrow callback: two Buttons in a wrapper. A
/// large one is the icon (the primary command) over its label and ▾ (the
/// arrow); a small one is the icon and label beside a 14 px ▾. A disabled
/// split button is disabled in both parts; a toggled one lights its main
/// part.
fn split_button_dom(rb: RibbonButton, large: bool, s: &RibbonStyle, theme: UiTheme) -> Dom {
    let disabled = rb.is_disabled();
    let name = button_name(&rb);
    let arrow_name = AzString::from(alloc::format!("{} options", name.as_str()));
    let (base, part_main, part_arrow) = if large {
        (
            s.resolved_large_button_style(),
            SPLIT_LARGE_MAIN_STYLE,
            SPLIT_LARGE_ARROW_STYLE,
        )
    } else {
        (
            s.resolved_small_button_style(),
            SPLIT_SMALL_MAIN_STYLE,
            SPLIT_SMALL_ARROW_STYLE,
        )
    };
    let mut main_style =
        merged_style(&base, &CssPropertyWithConditionsVec::from_const_slice(part_main));
    if rb.toggled {
        main_style = merged_style(&main_style, &s.resolved_checked_style());
    }
    let arrow_style =
        merged_style(&base, &CssPropertyWithConditionsVec::from_const_slice(part_arrow));
    let (icon_style, label_style) = if large {
        (s.resolved_large_icon_style(), s.resolved_large_label_style())
    } else {
        (s.resolved_small_icon_style(), s.resolved_small_label_style())
    };
    let label = rb.label.as_str().to_string();
    let main = styled_button(
        rb.icon,
        if large {
            AzString::from_const_str("")
        } else {
            rb.label.clone()
        },
        AzString::from_const_str(""),
        main_style,
        icon_style.clone(),
        label_style.clone(),
        s.resolved_arrow_icon_style(),
        rb.on_click,
        rb.disabled_reason.clone(),
        name,
        OptionUiTheme::Some(theme),
    );
    let mut arrow = styled_button(
        if large {
            AzString::from_const_str("")
        } else {
            AzString::from_const_str("arrow_drop_down")
        },
        AzString::from_const_str(""),
        AzString::from_const_str(""),
        arrow_style,
        if large {
            icon_style
        } else {
            s.resolved_arrow_icon_style()
        },
        label_style,
        s.resolved_arrow_icon_style(),
        rb.on_arrow_click,
        rb.disabled_reason,
        arrow_name,
        OptionUiTheme::Some(theme),
    );
    if large {
        arrow.icon_dom = OptionDom::Some(large_label_block(&label, true, s));
    }
    let mut main = finish_button(main, disabled);
    main.root.add_class(AzString::from_const_str(RIBBON_SPLIT_MAIN_CLASS));
    let mut arrow = finish_button(arrow, disabled);
    arrow.root.add_class(AzString::from_const_str(RIBBON_SPLIT_ARROW_CLASS));
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_SPLIT))
        .with_css_props(CssPropertyWithConditionsVec::from_const_slice(if large {
            SPLIT_LARGE_STYLE
        } else {
            SPLIT_SMALL_STYLE
        }))
        .with_children(DomVec::from_vec(vec![main, arrow]))
}

/// One item in the ribbon's theme. An embedded widget the caller left
/// without a theme (`None`) is part of the ribbon's look and is built in
/// `theme`; one the caller pinned keeps its pin. A gallery shows at most
/// `cells` cells in the ribbon (`None`: as many as the app asked for), the
/// cap a tab too wide for the ribbon puts on its group's galleries.
fn item_dom(
    item: RibbonItem,
    s: &RibbonStyle,
    b: RibbonBehavior,
    theme: UiTheme,
    cells: Option<usize>,
) -> Dom {
    match item {
        RibbonItem::LargeButton(rb) => expand_ribbon_button(rb, true, s, theme),
        RibbonItem::SmallButton(rb) => expand_ribbon_button(rb, false, s, theme),
        RibbonItem::Column(col) => Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_COLUMN))
            .with_css_props(s.resolved_column_style())
            .with_children(DomVec::from_vec(
                col.items
                    .into_library_owned_vec()
                    .into_iter()
                    .map(|it| item_dom(it, s, b, theme, cells))
                    .collect(),
            )),
        RibbonItem::Row(row) => Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_ROW))
            .with_css_props(s.resolved_row_style())
            .with_children(DomVec::from_vec(
                row.items
                    .into_library_owned_vec()
                    .into_iter()
                    .map(|it| item_dom(it, s, b, theme, cells))
                    .collect(),
            )),
        RibbonItem::Combo(mut combo) => {
            if combo.theme.is_none() {
                combo.set_theme(theme);
            }
            combo.dom()
        }
        RibbonItem::Drop(mut drop) => {
            if drop.theme.is_none() {
                drop.set_theme(theme);
            }
            drop.dom()
        }
        RibbonItem::Check(mut check) => {
            if check.theme.is_none() {
                check.set_theme(theme);
            }
            check.dom()
        }
        RibbonItem::Gallery(gallery) => gallery_dom(gallery, s, b, theme, cells),
        RibbonItem::Separator => Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_SEPARATOR))
            .with_css_props(s.resolved_separator_style()),
        RibbonItem::Custom(dom) => dom,
    }
}

/// Whether Office stacks `item` with its neighbours: a small button, a check
/// box, a combo or a drop-down box - or a row of them, which is one control
/// tall (RibbonX's `box`): Explorer's Show/hide is three check boxes, each
/// with its label, over each other.
const fn is_small_item(item: &RibbonItem) -> bool {
    matches!(
        item,
        RibbonItem::SmallButton(_)
            | RibbonItem::Check(_)
            | RibbonItem::Combo(_)
            | RibbonItem::Drop(_)
            | RibbonItem::Row(_)
    )
}

/// A group's top-level items with every run of two or more small items
/// ([`is_small_item`]) packed into [`RibbonColumn`]s of three, top first -
/// Office's Delete group (Ignore / Clean Up / Junk over each other), its
/// Find group (the contact box, Address Book, Filter E-mail). A small item
/// alone stays as it is (it sits at the top either way); a row keeps its
/// items side by side and the columns a caller built are its own.
fn stack_small_items(items: Vec<RibbonItem>) -> Vec<RibbonItem> {
    fn flush(run: &mut Vec<RibbonItem>, out: &mut Vec<RibbonItem>) {
        if run.len() < 2 {
            out.append(run);
            return;
        }
        let mut column: Vec<RibbonItem> = Vec::with_capacity(ROWS_PER_COLUMN);
        for item in run.drain(..) {
            column.push(item);
            if column.len() == ROWS_PER_COLUMN {
                out.push(RibbonItem::Column(RibbonColumn {
                    items: RibbonItemVec::from_vec(core::mem::take(&mut column)),
                }));
            }
        }
        if !column.is_empty() {
            out.push(RibbonItem::Column(RibbonColumn {
                items: RibbonItemVec::from_vec(column),
            }));
        }
    }
    let mut out = Vec::with_capacity(items.len());
    let mut run: Vec<RibbonItem> = Vec::new();
    for item in items {
        if is_small_item(&item) {
            run.push(item);
        } else {
            flush(&mut run, &mut out);
            out.push(item);
        }
    }
    flush(&mut run, &mut out);
    out
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

/// One group of the active tab, drawn at `scale` (Office's ribbon scaling,
/// [`walk_scaling_steps`]): as the app built it, with its buttons small or
/// their icons alone ([`scaled_items`]) and its galleries showing fewer
/// cells - or collapsed into one button that opens the whole group in a
/// popup ([`collapsed_group_dom`]).
fn group_dom(
    group: RibbonGroup,
    s: &RibbonStyle,
    b: RibbonBehavior,
    theme: UiTheme,
    scale: GroupScale,
) -> Dom {
    if scale.size == GroupSize::Collapsed {
        return collapsed_group_dom(group, s, b, theme);
    }
    let RibbonGroup {
        label,
        items,
        launcher,
        fills_space,
        // The collapsed group's button shows the icon; a group drawn whole
        // shows its items.
        icon: _,
    } = group;

    let items = scaled_items(items.into_library_owned_vec(), scale.size);
    let item_doms: Vec<Dom> = stack_small_items(items)
        .into_iter()
        .map(|it| item_dom(it, s, b, theme, scale.gallery_cells))
        .collect();

    let items_row = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_ITEMS))
        .with_css_props(s.resolved_group_items_style())
        .with_children(DomVec::from_vec(item_doms));

    let has_launcher = launcher.is_some();
    let mut footer_children: Vec<Dom> = Vec::with_capacity(3);
    if has_launcher {
        // Balances the launcher's width so the caption stays centered.
        footer_children.push(
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_FOOTER_SPACER))
                .with_css_props(s.resolved_footer_spacer_style()),
        );
    }
    footer_children.push(
        crate::widgets::widget_p_chrome()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_LABEL))
            .with_css_props(s.resolved_group_label_style())
            .with_children(DomVec::from_vec(vec![
                Dom::create_text_do_not_use_without_block_level_wrapper(label),
            ])),
    );
    if let Some(l) = launcher.into_option() {
        footer_children.push(styled_button(
            AzString::from_const_str("south_east"),
            AzString::from_const_str(""),
            AzString::from_const_str(""),
            s.resolved_launcher_button_style(),
            s.resolved_launcher_icon_style(),
            s.resolved_small_label_style(),
            s.resolved_arrow_icon_style(),
            Some(l).into(),
            AzString::from_const_str(""),
            AzString::from_const_str("More options"),
            OptionUiTheme::Some(theme),
        ).dom());
    }
    let footer = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_FOOTER))
        .with_css_props(s.resolved_group_footer_style())
        .with_children(DomVec::from_vec(footer_children));

    let group_style = if fills_space {
        merged_style(
            &s.resolved_group_style(),
            &CssPropertyWithConditionsVec::from_const_slice(GROUP_FILL_STYLE),
        )
    } else {
        s.resolved_group_style()
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP))
        .with_css_props(group_style)
        .with_children(DomVec::from_vec(vec![items_row, footer]))
}

/// The cells the in-ribbon strip shows: the row of `visible` cells holding
/// `selected` (the last row filled from the end; no selection - an index
/// past the cells - is the first row), or every cell when `visible` is 0 or
/// covers them all.
fn gallery_window(len: usize, selected: usize, visible: usize) -> core::ops::Range<usize> {
    if visible == 0 || visible >= len {
        return 0..len;
    }
    let selected = if selected < len { selected } else { 0 };
    let start = ((selected / visible) * visible).min(len - visible);
    start..start + visible
}

/// How many cells a gallery's in-ribbon strip shows: a classic strip the
/// `visible` cells around the selected one (every cell when `visible` is 0
/// or covers them all), a list gallery its first `columns` x 3 commands (or
/// its `visible` ones) - and at most `cap` of them, the cap a tab too wide
/// for the ribbon puts on its group's galleries.
fn gallery_shown(gallery: &RibbonGallery, cap: Option<usize>) -> usize {
    let len = gallery.cells.len();
    let asked = if gallery.columns > 0 {
        let shown = if gallery.visible == 0 {
            gallery.columns.saturating_mul(ROWS_PER_COLUMN)
        } else {
            gallery.visible
        };
        shown.min(len)
    } else if gallery.visible == 0 || gallery.visible >= len {
        len
    } else {
        gallery.visible
    };
    cap.map_or(asked, |cap| asked.min(cap))
}

fn gallery_dom(
    gallery: RibbonGallery,
    s: &RibbonStyle,
    b: RibbonBehavior,
    theme: UiTheme,
    cap: Option<usize>,
) -> Dom {
    let shown = gallery_shown(&gallery, cap);
    let RibbonGallery {
        cells,
        selected,
        on_select,
        // Counted into `shown`.
        visible: _,
        columns,
    } = gallery;
    // A list gallery is commands (Quick Steps): no cell stays selected, and
    // its strip is the first `columns` x 3 cells.
    let list = columns > 0;
    let has_callback = on_select.is_some();
    let auto_select = b.auto_select_gallery && !list;
    let cells = cells.into_library_owned_vec();
    let (strip_cells, selected) = if list {
        (0..shown, usize::MAX)
    } else {
        (gallery_window(cells.len(), selected, shown), selected)
    };
    let cell_base = if list {
        merged_style(
            &s.resolved_gallery_cell_style(),
            &CssPropertyWithConditionsVec::from_const_slice(GALLERY_LIST_CELL_STYLE),
        )
    } else {
        s.resolved_gallery_cell_style()
    };

    // The cells are built twice: once for the in-ribbon strip and once for
    // the expansion panel, so "More" can show every cell without a relayout.
    let build_cells = |in_panel: bool| -> Vec<Dom> {
        let mut out: Vec<Dom> = Vec::with_capacity(cells.len());
        for (idx, cell) in cells.iter().enumerate() {
            // The strip shows its window of cells; the panel every cell.
            if !in_panel && !strip_cells.contains(&idx) {
                continue;
            }
            let (classes, cell_style) = if idx == selected {
                (
                    CLS_GALLERY_CELL_SELECTED,
                    merged_style(&cell_base, &s.resolved_gallery_cell_selected_style()),
                )
            } else {
                (CLS_GALLERY_CELL, cell_base.clone())
            };
            let children = if list {
                // Quick Steps: the icon beside the name, a small button's.
                vec![
                    Dom::create_div()
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(
                            CLS_GALLERY_CELL_ICON,
                        ))
                        .with_css_props(merged_style(
                            &s.resolved_small_icon_style(),
                            &CssPropertyWithConditionsVec::from_const_slice(
                                GALLERY_LIST_ICON_STYLE,
                            ),
                        ))
                        .with_children(DomVec::from_vec(vec![cell.preview.clone()])),
                    crate::widgets::widget_p_chrome()
                        .with_css_props(s.resolved_small_label_style())
                        .with_children(DomVec::from_vec(vec![
                            Dom::create_text_do_not_use_without_block_level_wrapper(
                                cell.label.clone(),
                            ),
                        ])),
                ]
            } else {
                vec![
                    cell.preview.clone(),
                    crate::widgets::widget_p_chrome()
                        .with_css_props(s.resolved_gallery_cell_label_style())
                        .with_children(DomVec::from_vec(vec![
                            Dom::create_text_do_not_use_without_block_level_wrapper(
                                cell.label.clone(),
                            ),
                        ])),
                ]
            };
            let mut d = Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
                .with_css_props(cell_style)
                .with_children(DomVec::from_vec(children));
            if has_callback || auto_select {
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
                            auto_select,
                            in_panel,
                            selected_style: s.resolved_gallery_cell_selected_style(),
                            base_style: cell_base.clone(),
                        }),
                    }]
                    .into(),
                );
            }
            out.push(d);
        }
        out
    };

    // A list gallery's strip is its columns of three; a classic one is one
    // row of cells.
    let strip_children = if list {
        let mut columns_out: Vec<Dom> = Vec::new();
        let mut column: Vec<Dom> = Vec::with_capacity(ROWS_PER_COLUMN);
        let list_column = |cells: Vec<Dom>| {
            Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_COLUMN))
                .with_css_props(CssPropertyWithConditionsVec::from_const_slice(
                    GALLERY_LIST_COLUMN_STYLE,
                ))
                .with_children(DomVec::from_vec(cells))
        };
        for cell in build_cells(false) {
            column.push(cell);
            if column.len() == ROWS_PER_COLUMN {
                columns_out.push(list_column(core::mem::take(&mut column)));
            }
        }
        if !column.is_empty() {
            columns_out.push(list_column(column));
        }
        columns_out
    } else {
        build_cells(false)
    };
    let mut strip = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_STRIP))
        .with_css_props(s.resolved_gallery_strip_style())
        .with_children(DomVec::from_vec(strip_children));
    if list {
        strip
            .root
            .add_class(AzString::from_const_str(RIBBON_GALLERY_LIST_CLASS));
    }

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
                s.resolved_gallery_spinner_button_style(),
                s.resolved_gallery_spinner_icon_style(),
                s.resolved_small_label_style(),
                s.resolved_arrow_icon_style(),
                OptionButtonOnClick::None,
                AzString::from_const_str(""),
                AzString::from_const_str(["Previous row", "Next row", "More"][i]),
                OptionUiTheme::Some(theme),
            ).dom();
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
        .with_css_props(s.resolved_gallery_spinner_style())
        .with_children(DomVec::from_vec(spinner_buttons));

    let frame = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY))
        .with_css_props(s.resolved_gallery_frame_style())
        .with_children(DomVec::from_vec(vec![strip, spinner]));

    if !b.expandable_gallery {
        return frame;
    }

    // The expansion panel is an absolutely-positioned wrapped grid of every
    // cell, hidden until "More" is clicked (the popover/combobox pattern).
    let panel = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_PANEL))
        .with_css_props(s.resolved_gallery_panel_style())
        .with_children(DomVec::from_vec(build_cells(true)));

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GALLERY_WRAPPER))
        .with_css_props(s.resolved_gallery_wrapper_style())
        .with_children(DomVec::from_vec(vec![frame, panel]))
}

// -- Ribbon scaling --
//
// Office scales a tab that does not fit its ribbon down step by step (its
// "ribbon scaling": the size definitions and scaling policy of Windows'
// Ribbon framework). First the large buttons become small ones, the label
// beside a 16 px icon, three to a column with the group's small items; then
// the galleries show fewer cells; then every button shows its icon alone,
// still three to a column; and only then does a group collapse into one
// large button - its icon over its label and ▾ - that opens the whole group
// in a popup. Each step is one group's, the rightmost group first (Office's
// order when a tab names none), and a group takes it only where it makes the
// group narrower: three large buttons with short labels are narrower than a
// column of small ones.
//
// Choosing a step needs every group's width at every step BEFORE anything is
// laid out, so the ribbon measures its parts from the metrics their styles
// declare - the paddings, borders and gaps of `theme_large_button`,
// `theme_small_button`, `theme_group` and the gallery's parts, the 32 px and
// 16 px icons, the 120 px gallery cells - and a label from its characters'
// advances in a sans-serif UI face at the part's font size. The estimate
// errs wide (`TEXT_WIDTH_FACTOR`, `FIT_SLACK_PX`): a group gives way a
// little early rather than overflowing its window. Flora repaints the ribbon
// on the same metrics, so one measure serves both looks.

/// A large button's border box across its content: padding 7 + 7 and the
/// chassis' 1 px border on each side (`theme_large_button`).
const LARGE_BUTTON_FRAME_PX: f32 = 16.0;
/// A large button's minimum width (`theme_large_button`).
const LARGE_BUTTON_MIN_PX: f32 = 44.0;
/// The margin after a large button (`theme_large_button`).
const LARGE_BUTTON_GAP_PX: f32 = 1.0;
/// A large button's icon (`theme_large_icon`).
const LARGE_ICON_PX: f32 = 32.0;
/// A large split button's arrow part across its label: padding 3 + 3 and
/// its border (`SPLIT_LARGE_ARROW_STYLE`).
const SPLIT_LARGE_ARROW_FRAME_PX: f32 = 8.0;
/// A small button's padding 3 + 3 and border 1 + 1 (`theme_small_button`).
const SMALL_BUTTON_FRAME_PX: f32 = 8.0;
/// A small button's icon (`theme_small_icon`).
const SMALL_ICON_PX: f32 = 16.0;
/// The space before a small button's label (`theme_small_label`).
const SMALL_LABEL_GAP_PX: f32 = 5.0;
/// A small split button's arrow part (`SPLIT_SMALL_ARROW_STYLE`).
const SPLIT_SMALL_ARROW_PX: f32 = 14.0;
/// A menu's ▾ (`theme_arrow_icon`).
const ARROW_PX: f32 = 14.0;
/// A button label's font size (`theme_large_label`, `theme_small_label`).
const LABEL_FONT_PX: f32 = 12.0;
/// A group caption's font size (`theme_group_label`).
const CAPTION_FONT_PX: f32 = 11.0;
/// A drop-down box's font size (the drop-down widget's own).
const DROP_DOWN_FONT_PX: f32 = 13.0;
/// What a drop-down box puts around its longest choice: its padding, its
/// border and its ▾.
const DROP_DOWN_FRAME_PX: f32 = 34.0;
/// The margin after a ribbon combo box (`theme_combo_wrapper_base`).
const COMBO_GAP_PX: f32 = 2.0;
/// A check box: 14 px, padding 2 + 2 and border 1 + 1 (`check_box`).
const CHECK_BOX_PX: f32 = 20.0;
/// A separator and its margins (`theme_separator`).
const SEPARATOR_PX: f32 = 7.0;
/// What an app's own control is taken to put around its texts and icons.
const CUSTOM_FRAME_PX: f32 = 8.0;
/// A group's padding 2 + 2 and its 1 px separator (`theme_group`).
const GROUP_FRAME_PX: f32 = 5.0;
/// A dialog launcher and the spacer that balances it
/// (`theme_launcher_button`, `FOOTER_SPACER_STYLE`).
const LAUNCHER_PX: f32 = 34.0;
/// A group that fills the space shrinks to its floor: `GROUP_FILL_STYLE`'s
/// min-width and the group's frame.
const FILL_GROUP_MIN_PX: f32 = 165.0;
/// A classic gallery cell (`theme_gallery_cell`).
const GALLERY_CELL_PX: f32 = 120.0;
/// A gallery's spinner column - 15 px and its 1 px rule - and its frame's
/// border (`theme_gallery_spinner`, `theme_gallery_frame`).
const GALLERY_FRAME_PX: f32 = 18.0;
/// A gallery frame's minimum width (`theme_gallery_frame`).
const GALLERY_MIN_PX: f32 = 137.0;
/// A list gallery cell around its name: its border, its padding 3 + 8 and
/// its 16 px icon box (`GALLERY_LIST_CELL_STYLE`, `GALLERY_LIST_ICON_STYLE`).
const GALLERY_LIST_CELL_FRAME_PX: f32 = 28.0;
/// The fewest cells a classic gallery shows in the ribbon as its tab
/// narrows (a list gallery keeps one column): Office's in-ribbon galleries
/// keep a few, and More shows them all.
const GALLERY_MIN_CELLS: usize = 2;
/// How much wider than Helvetica's advances a label is taken to be: the UI
/// faces (San Francisco, Segoe UI, Cantarell) run a little wider.
const TEXT_WIDTH_FACTOR: f32 = 1.1;
/// Room left at the ribbon's edge for what the estimate misses.
const FIT_SLACK_PX: f32 = 4.0;
/// A collapsed group's button: the group's item area and its caption row
/// (`GROUP_ITEMS_STYLE`, `GROUP_FOOTER_STYLE`) - Office's spans the group.
const COLLAPSED_BUTTON_HEIGHT_PX: isize = 86;
/// The icon of a collapsed group none of whose buttons has one.
const GROUP_FALLBACK_ICON: &str = "apps";

/// A character's advance in a sans-serif UI face, in thousandths of an em:
/// Helvetica's, which the UI faces of macOS, Windows and Linux are close to
/// (`TEXT_WIDTH_FACTOR` covers the rest). Ideographs, kana and hangul are an
/// em wide; any other character is taken to be a wide letter.
const fn advance_milli_em(c: char) -> u32 {
    match c {
        '\'' => 191,
        'i' | 'j' | 'l' => 222,
        '|' => 260,
        ' ' | '\u{a0}' | '!' | ',' | '.' | '/' | ':' | ';' | '[' | '\\' | ']' | 'I' | 'f' | 't' => {
            278
        }
        'r' | '(' | ')' | '-' | '`' => 333,
        '{' | '}' => 334,
        '"' => 355,
        '*' => 389,
        '^' => 469,
        'J' | 'c' | 'k' | 's' | 'v' | 'x' | 'y' | 'z' => 500,
        '0'..='9'
        | '#'
        | '$'
        | '?'
        | '_'
        | 'L'
        | 'a'
        | 'b'
        | 'd'
        | 'e'
        | 'g'
        | 'h'
        | 'n'..='q'
        | 'u' => 556,
        '+' | '<' | '=' | '>' | '~' => 584,
        'F' | 'T' | 'Z' => 611,
        '&' | 'A' | 'B' | 'E' | 'K' | 'P' | 'S' | 'V' | 'X' | 'Y' => 667,
        'C' | 'D' | 'H' | 'N' | 'R' | 'U' | 'w' => 722,
        'G' | 'O' | 'Q' => 778,
        'M' | 'm' => 833,
        '%' => 889,
        'W' => 944,
        '@' => 1015,
        '\u{1100}'..='\u{11ff}'
        | '\u{2e80}'..='\u{a4cf}'
        | '\u{ac00}'..='\u{d7a3}'
        | '\u{f900}'..='\u{faff}'
        | '\u{ff00}'..='\u{ff60}' => 1000,
        _ => 600,
    }
}

/// The width of `text` set at `font_px`, as the ribbon estimates it.
fn text_px(text: &str, font_px: f32) -> f32 {
    let milli: u64 = text.chars().map(|c| u64::from(advance_milli_em(c))).sum();
    milli as f32 / 1000.0 * font_px * TEXT_WIDTH_FACTOR
}

/// A button label's width.
fn label_px(text: &str) -> f32 {
    text_px(text, LABEL_FONT_PX)
}

/// A large button's label block as [`large_label_text`] sets it: the longer
/// of its two balanced lines, the ▾ glued to the last word - or its one
/// word over the ▾.
fn large_label_px(label: &str, arrow: bool) -> f32 {
    let words: Vec<&str> = label.split_whitespace().collect();
    match balanced_break(&words) {
        Some(k) => {
            let glued = if arrow {
                label_px("\u{a0}") + ARROW_PX
            } else {
                0.0
            };
            label_px(&words[..k].join(" ")).max(label_px(&words[k..].join(" ")) + glued)
        }
        None => {
            let word = label_px(label.trim());
            if arrow {
                word.max(ARROW_PX)
            } else {
                word
            }
        }
    }
}

/// A large button across, its margin included.
fn large_button_px(rb: &RibbonButton) -> f32 {
    if rb.is_split() {
        // The icon over the label and its ▾: two parts, as wide as the wider.
        let main = LARGE_ICON_PX + LARGE_BUTTON_FRAME_PX;
        let arrow_part = large_label_px(rb.label.as_str(), true) + SPLIT_LARGE_ARROW_FRAME_PX;
        return main.max(arrow_part).max(LARGE_BUTTON_MIN_PX) + LARGE_BUTTON_GAP_PX;
    }
    let icon = if rb.icon.as_str().is_empty() {
        0.0
    } else {
        LARGE_ICON_PX
    };
    let label = large_label_px(rb.label.as_str(), rb.arrow != RibbonArrow::None);
    (icon.max(label) + LARGE_BUTTON_FRAME_PX).max(LARGE_BUTTON_MIN_PX) + LARGE_BUTTON_GAP_PX
}

/// A small button across; `icon_only` drops the label of a button that has
/// an icon ([`icon_only`]).
fn small_button_px(rb: &RibbonButton, icon_only: bool) -> f32 {
    let has_icon = !rb.icon.as_str().is_empty();
    let icon = if has_icon { SMALL_ICON_PX } else { 0.0 };
    let label = if (icon_only && has_icon) || rb.label.as_str().is_empty() {
        0.0
    } else {
        SMALL_LABEL_GAP_PX + label_px(rb.label.as_str())
    };
    if rb.is_split() {
        // The main part - padded 2 on its right, not 3 - beside the ▾'s part.
        return SMALL_BUTTON_FRAME_PX - 1.0 + icon + label + SPLIT_SMALL_ARROW_PX;
    }
    let arrow = if rb.arrow == RibbonArrow::None {
        0.0
    } else {
        ARROW_PX
    };
    SMALL_BUTTON_FRAME_PX + icon + label + arrow
}

/// A combo box across: the width the app gave its wrapper
/// ([`RibbonStyle::styled_combo_box`]), else the combo box's own minimum.
fn combo_px(combo: &ComboBox) -> f32 {
    let declared = combo.wrapper_style.as_ref().and_then(|style| {
        style.as_ref().iter().rev().find_map(|declaration| {
            if !declaration.apply_if.as_ref().is_empty() {
                return None;
            }
            match &declaration.property {
                P::Width(azul_css::css::CssPropertyValue::Exact(LayoutWidth::Px(px))) => {
                    px.to_pixels_absolute().into_option()
                }
                _ => None,
            }
        })
    });
    declared.unwrap_or(super::combobox::MIN_WIDTH as f32) + COMBO_GAP_PX
}

/// A drop-down box across: its longest choice and what it puts around it.
fn drop_px(drop: &DropDown) -> f32 {
    drop.choices
        .as_ref()
        .iter()
        .map(|choice| text_px(choice.as_str(), DROP_DOWN_FONT_PX))
        .fold(0.0, f32::max)
        + DROP_DOWN_FRAME_PX
}

/// An app's own control across, roughly: its texts at a label's size and its
/// icons side by side, and a little room around them.
fn custom_px(dom: &Dom) -> f32 {
    fn content(dom: &Dom) -> f32 {
        let own = match dom.root.get_node_type() {
            NodeType::Text(text) => label_px(text.as_str()),
            NodeType::Icon(_) => SMALL_ICON_PX,
            _ => 0.0,
        };
        own + dom.children.as_ref().iter().map(content).sum::<f32>()
    }
    content(dom) + CUSTOM_FRAME_PX
}

/// A gallery across: its strip of cells - at most `fit.cells` of them, or
/// as narrow as its frame lets it in a group that fills the space - its
/// spinner and its frame.
fn gallery_px(gallery: &RibbonGallery, fit: Fit) -> f32 {
    if fit.elastic {
        return GALLERY_MIN_PX;
    }
    let shown = gallery_shown(gallery, fit.cells);
    let strip = if gallery.columns > 0 {
        // Columns of three commands, each as wide as its widest.
        let cells: Vec<f32> = gallery
            .cells
            .as_ref()
            .iter()
            .take(shown)
            .map(|cell| {
                GALLERY_LIST_CELL_FRAME_PX + SMALL_LABEL_GAP_PX + label_px(cell.label.as_str())
            })
            .collect();
        cells
            .chunks(ROWS_PER_COLUMN)
            .map(|column| column.iter().copied().fold(0.0, f32::max))
            .sum::<f32>()
    } else {
        shown as f32 * GALLERY_CELL_PX
    };
    (strip + GALLERY_FRAME_PX).max(GALLERY_MIN_PX)
}

/// How far a group is scaled down, in the order a tab that does not fit
/// takes the sizes (Office's group sizes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum GroupSize {
    /// As the app built it.
    Large,
    /// Its large buttons are small ones - the label beside a 16 px icon -
    /// stacked three to a column with its small items.
    Medium,
    /// Every button is its icon alone, named by its label, three to a
    /// column.
    Small,
    /// One button - the group's icon over its label and ▾ - that opens the
    /// whole group in a popup.
    Collapsed,
}

/// One group's scaling: its size, and the most cells its galleries show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GroupScale {
    size: GroupSize,
    /// The most cells a gallery of the group shows in the ribbon; `None`:
    /// as many as the app asked for.
    gallery_cells: Option<usize>,
}

impl GroupScale {
    /// The group as the app built it.
    const FULL: Self = Self {
        size: GroupSize::Large,
        gallery_cells: None,
    };
}

/// How a group at a scale lays out one of its items ([`item_extent`]).
#[derive(Debug, Clone, Copy)]
struct Fit {
    size: GroupSize,
    /// The cap on the group's galleries.
    cells: Option<usize>,
    /// The group fills the space: its galleries clip, down to their frame's
    /// minimum.
    elastic: bool,
}

/// One item as a group at `fit` lays it out: whether Office stacks it three
/// to a column with its neighbours (one control tall: [`is_small_item`] once
/// [`scaled_items`] made it small) and how wide it is.
fn item_extent(item: &RibbonItem, fit: Fit) -> (bool, f32) {
    match item {
        RibbonItem::LargeButton(rb) if fit.size == GroupSize::Large => (false, large_button_px(rb)),
        RibbonItem::LargeButton(rb) | RibbonItem::SmallButton(rb) => {
            (true, small_button_px(rb, fit.size >= GroupSize::Small))
        }
        RibbonItem::Column(col) => (
            false,
            col.items
                .as_ref()
                .iter()
                .map(|it| item_extent(it, fit).1)
                .fold(0.0, f32::max),
        ),
        RibbonItem::Row(row) => (
            true,
            row.items
                .as_ref()
                .iter()
                .map(|it| item_extent(it, fit).1)
                .sum::<f32>(),
        ),
        RibbonItem::Combo(combo) => (true, combo_px(combo)),
        RibbonItem::Drop(drop) => (true, drop_px(drop)),
        RibbonItem::Check(_) => (true, CHECK_BOX_PX),
        RibbonItem::Gallery(gallery) => (false, gallery_px(gallery, fit)),
        RibbonItem::Separator => (false, SEPARATOR_PX),
        RibbonItem::Custom(dom) => (false, custom_px(dom)),
    }
}

/// A group's item row across: its items side by side, every run of two or
/// more that stack packed three to a column, as [`stack_small_items`] packs
/// them.
fn items_px(items: &[RibbonItem], fit: Fit) -> f32 {
    fn flush(run: &mut Vec<f32>, total: &mut f32) {
        if run.len() < 2 {
            *total += run.iter().sum::<f32>();
        } else {
            for column in run.chunks(ROWS_PER_COLUMN) {
                *total += column.iter().copied().fold(0.0, f32::max);
            }
        }
        run.clear();
    }
    let mut total = 0.0;
    let mut run: Vec<f32> = Vec::new();
    for item in items {
        let (stacks, width) = item_extent(item, fit);
        if stacks {
            run.push(width);
        } else {
            flush(&mut run, &mut total);
            total += width;
        }
    }
    flush(&mut run, &mut total);
    total
}

/// A group across at `scale`, its padding and its separator included: its
/// item row or its caption, whichever is wider - a group that fills the
/// space no narrower than its floor - or its one button, collapsed.
fn group_px(group: &RibbonGroup, scale: GroupScale) -> f32 {
    if scale.size == GroupSize::Collapsed {
        return GROUP_FRAME_PX + large_button_px(&collapsed_button(group));
    }
    let fit = Fit {
        size: scale.size,
        cells: scale.gallery_cells,
        elastic: group.fills_space,
    };
    let items = items_px(group.items.as_ref(), fit);
    let launcher = if group.launcher.is_some() {
        LAUNCHER_PX
    } else {
        0.0
    };
    let caption = text_px(group.label.as_str(), CAPTION_FONT_PX) + launcher;
    let width = GROUP_FRAME_PX + items.max(caption);
    if group.fills_space {
        width.max(FILL_GROUP_MIN_PX)
    } else {
        width
    }
}

/// The most and the fewest cells the galleries among `items` show in the
/// ribbon - the range a tab too wide for it steps them through, a cell at a
/// time: a classic gallery keeps [`GALLERY_MIN_CELLS`], a list gallery one
/// column. `None` for items without a gallery.
fn gallery_cell_range(items: &[RibbonItem]) -> Option<(usize, usize)> {
    let mut range: Option<(usize, usize)> = None;
    for item in items {
        let found = match item {
            RibbonItem::Gallery(gallery) => {
                let most = gallery_shown(gallery, None);
                let fewest = if gallery.columns > 0 {
                    ROWS_PER_COLUMN
                } else {
                    GALLERY_MIN_CELLS
                };
                Some((most, fewest.min(most)))
            }
            RibbonItem::Column(col) => gallery_cell_range(col.items.as_ref()),
            RibbonItem::Row(row) => gallery_cell_range(row.items.as_ref()),
            _ => None,
        };
        if let Some((most, fewest)) = found {
            range = Some(match range {
                Some((m, f)) => (m.max(most), f.max(fewest)),
                None => (most, fewest),
            });
        }
    }
    range
}

/// A tab's groups while the scaling walk ([`walk_scaling_steps`]) scales
/// them down: each group's scale, and its width there.
struct ScalingWalk<'a> {
    groups: &'a [RibbonGroup],
    scales: Vec<GroupScale>,
    widths: Vec<f32>,
}

impl<'a> ScalingWalk<'a> {
    fn new(groups: &'a [RibbonGroup]) -> Self {
        Self {
            groups,
            scales: vec![GroupScale::FULL; groups.len()],
            widths: groups
                .iter()
                .map(|g| group_px(g, GroupScale::FULL))
                .collect(),
        }
    }

    /// The tab across at the groups' scales.
    fn total(&self) -> f32 {
        self.widths.iter().sum()
    }

    /// Scales group `i` to `candidate` if that makes it narrower (by more
    /// than a rounding error); whether it did.
    fn take(&mut self, i: usize, candidate: GroupScale) -> bool {
        let width = group_px(&self.groups[i], candidate);
        if width + 0.5 < self.widths[i] {
            self.scales[i] = candidate;
            self.widths[i] = width;
            true
        } else {
            false
        }
    }
}

/// Walks Office's ribbon scaling for a tab of `groups`, widest first: every
/// group as the app built it, then one group's step at a time - every step
/// that makes a group narrower, the rightmost group first, in four rounds:
/// (1) its large buttons small, (2) its galleries a cell narrower each, down
/// to their fewest cells, (3) every button its icon alone, (4) the group
/// collapsed into one button. `stop` sees the groups' scales and the tab's
/// width at each step and says whether to stop there.
fn walk_scaling_steps(groups: &[RibbonGroup], mut stop: impl FnMut(&[GroupScale], f32) -> bool) {
    let mut walk = ScalingWalk::new(groups);
    if stop(&walk.scales, walk.total()) {
        return;
    }
    let right_to_left = (0..groups.len()).rev();
    for i in right_to_left.clone() {
        let medium = GroupScale {
            size: GroupSize::Medium,
            ..walk.scales[i]
        };
        if walk.take(i, medium) && stop(&walk.scales, walk.total()) {
            return;
        }
    }
    for i in right_to_left.clone() {
        // A group that fills the space clips its gallery as it shrinks.
        if groups[i].fills_space {
            continue;
        }
        let Some((most, fewest)) = gallery_cell_range(groups[i].items.as_ref()) else {
            continue;
        };
        let mut cells = most;
        while cells > fewest {
            cells -= 1;
            let fewer = GroupScale {
                gallery_cells: Some(cells),
                ..walk.scales[i]
            };
            if walk.take(i, fewer) && stop(&walk.scales, walk.total()) {
                return;
            }
        }
    }
    for i in right_to_left.clone() {
        let small = GroupScale {
            size: GroupSize::Small,
            ..walk.scales[i]
        };
        if walk.take(i, small) && stop(&walk.scales, walk.total()) {
            return;
        }
    }
    for i in right_to_left {
        let collapsed = GroupScale {
            size: GroupSize::Collapsed,
            ..walk.scales[i]
        };
        if walk.take(i, collapsed) && stop(&walk.scales, walk.total()) {
            return;
        }
    }
}

/// The groups' scales at the first step of [`walk_scaling_steps`] whose
/// width `fits` - or at the narrowest step, when none does.
fn chosen_scales(groups: &[RibbonGroup], mut fits: impl FnMut(f32) -> bool) -> Vec<GroupScale> {
    let mut chosen = vec![GroupScale::FULL; groups.len()];
    walk_scaling_steps(groups, |scales, total| {
        chosen.clear();
        chosen.extend_from_slice(scales);
        fits(total)
    });
    chosen
}

/// `items` as a group at `size` draws them (Office's group sizes): from
/// medium on every large button a small one, at small every button its icon
/// alone - in the app's own rows and columns too.
fn scaled_items(items: Vec<RibbonItem>, size: GroupSize) -> Vec<RibbonItem> {
    if size == GroupSize::Large {
        return items;
    }
    items
        .into_iter()
        .map(|item| scaled_item(item, size))
        .collect()
}

fn scaled_item(item: RibbonItem, size: GroupSize) -> RibbonItem {
    match item {
        RibbonItem::LargeButton(rb) | RibbonItem::SmallButton(rb) => {
            RibbonItem::SmallButton(if size >= GroupSize::Small {
                icon_only(rb)
            } else {
                rb
            })
        }
        RibbonItem::Column(col) => RibbonItem::Column(RibbonColumn {
            items: RibbonItemVec::from_vec(scaled_items(col.items.into_library_owned_vec(), size)),
        }),
        RibbonItem::Row(row) => RibbonItem::Row(RibbonRow {
            items: RibbonItemVec::from_vec(scaled_items(row.items.into_library_owned_vec(), size)),
        }),
        other => other,
    }
}

/// A button as its icon alone, named by its label - what a screen reader
/// says, as for any icon-only button. A button without an icon keeps its
/// label.
fn icon_only(mut rb: RibbonButton) -> RibbonButton {
    if rb.icon.as_str().is_empty() || rb.label.as_str().is_empty() {
        return rb;
    }
    if rb.alt.as_str().is_empty() {
        rb.alt = rb.label.clone();
    }
    rb.label = AzString::from_const_str("");
    rb
}

// -- A collapsed group and its popup --

/// The button a collapsed group is (Office's group button): the group's
/// icon - its own, else its first button's - over its label and ▾.
fn collapsed_button(group: &RibbonGroup) -> RibbonButton {
    let icon = if group.icon.as_str().is_empty() {
        first_icon(group.items.as_ref())
            .unwrap_or_else(|| AzString::from_const_str(GROUP_FALLBACK_ICON))
    } else {
        group.icon.clone()
    };
    RibbonButton::new(icon, group.label.clone()).with_arrow(RibbonArrow::Menu)
}

/// The icon of the first button among `items` that has one, depth first.
fn first_icon(items: &[RibbonItem]) -> Option<AzString> {
    items.iter().find_map(|item| match item {
        RibbonItem::LargeButton(rb) | RibbonItem::SmallButton(rb)
            if !rb.icon.as_str().is_empty() =>
        {
            Some(rb.icon.clone())
        }
        RibbonItem::Column(col) => first_icon(col.items.as_ref()),
        RibbonItem::Row(row) => first_icon(row.items.as_ref()),
        _ => None,
    })
}

/// A collapsed group (module docs): one large button spanning the group -
/// its icon over its label and ▾ - whose click opens the whole group, as a
/// window wide enough draws it, in a popup under it
/// ([`group_popup_window`], the group's last child: the group is the
/// popup's anchor).
fn collapsed_group_dom(
    group: RibbonGroup,
    s: &RibbonStyle,
    b: RibbonBehavior,
    theme: UiTheme,
) -> Dom {
    let shared = RefAny::new(GroupPopupShared { pending: None });
    let mut button = collapsed_button(&group);
    button.on_click = OptionButtonOnClick::Some(super::button::ButtonOnClick::create(
        shared.clone(),
        on_ribbon_group_button_click as ButtonOnClickCallbackType,
    ));
    let mut button = expand_ribbon_button(button, true, s, theme);
    button
        .root
        .add_class(AzString::from_const_str(RIBBON_GROUP_BUTTON_CLASS));
    // The button is the whole group - its item area and its caption row -
    // and its label is the group's caption.
    button
        .root
        .upsert_inline_css_property(P::const_height(LayoutHeight::const_px(
            COLLAPSED_BUTTON_HEIGHT_PX,
        )));
    let popup = group_popup_window(group, s, b, theme, shared);
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![
            CLS_GROUP[0].clone(),
            Class(AzString::from_const_str(RIBBON_GROUP_COLLAPSED_CLASS)),
        ]))
        .with_css_props(s.resolved_group_style())
        .with_children(DomVec::from_vec(vec![button, popup]))
}

/// The window a collapsed group's button opens: closed, under the group,
/// closed by a press outside or Escape, with per-pixel alpha so a look's
/// rounded panel keeps its corners.
const fn group_popup_config() -> TransientWindowConfig {
    TransientWindowConfig::closed()
        .with_anchor(TransientAnchor::Bottom)
        .with_dismiss(TransientDismiss::Outside)
        .with_material(WindowBackgroundMaterial::Transparent)
}

/// A collapsed group's popup: the `<transient-window>` holding the whole
/// group - drawn as a window wide enough draws it - on the look's popup
/// panel. A command there (a button, a split button's main part, a gallery
/// cell) is noted and closes the popup, and the app runs it in its own
/// window once the popup has closed ([`deferred_in_popup`],
/// [`on_ribbon_group_popup_dismissed`]), as the File menu hands over its
/// picks; everything else runs where it is clicked, a rebuild it asks for
/// reaching the app's window too ([`run_in_popup_window`]).
fn group_popup_window(
    group: RibbonGroup,
    s: &RibbonStyle,
    b: RibbonBehavior,
    theme: UiTheme,
    shared: RefAny,
) -> Dom {
    let label = group.label.clone();
    let mut whole = group_dom(
        deferred_in_popup(group, &shared),
        s,
        b,
        theme,
        GroupScale::FULL,
    );
    // Alone on its panel, the group wants no separator to its right.
    whole
        .root
        .upsert_inline_css_property(P::const_border_right_width(
            LayoutBorderRightWidth::const_px(0),
        ));
    let mut panel = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_POPUP))
        .with_css_props(s.resolved_group_popup_style())
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Grouping,
            accessibility_name: Some(label).into(),
            ..Default::default()
        })
        .with_children(DomVec::from_vec(vec![whole]));
    run_in_popup_window(&mut panel);
    let mut window = NodeData::create_node(NodeType::TransientWindow(group_popup_config()));
    // The popup's root holds no Tab stop of its own: its first control takes
    // the focus.
    window.set_tab_index(TabIndex::NoKeyboardFocus);
    window.add_callback(
        EventFilter::Component(ComponentEventFilter::Dismissed),
        shared,
        Callback::from_ptr(on_ribbon_group_popup_dismissed).to_core(),
    );
    Dom::create_from_data(window)
        .with_ids_and_classes(IdOrClassVec::from_const_slice(CLS_GROUP_POPUP_WINDOW))
        .with_children(DomVec::from_vec(vec![panel]))
}

/// `group` as its popup holds it: the click of each command - a plain or a
/// toggled button, a split button's main part, a gallery cell - is noted
/// ([`on_ribbon_group_popup_command`], [`on_ribbon_group_popup_select`])
/// rather than run, and the app runs it in its own window once the popup has
/// closed: run in the popup's window, a command's thread, timer or new
/// window would belong to a window about to close. A button whose whole face
/// opens a menu keeps its click, for its menu to open beside it.
fn deferred_in_popup(mut group: RibbonGroup, shared: &RefAny) -> RibbonGroup {
    let items = core::mem::replace(&mut group.items, RibbonItemVec::from_const_slice(&[]));
    group.items = RibbonItemVec::from_vec(deferred_items(items.into_library_owned_vec(), shared));
    group
}

fn deferred_items(items: Vec<RibbonItem>, shared: &RefAny) -> Vec<RibbonItem> {
    items
        .into_iter()
        .map(|item| deferred_item(item, shared))
        .collect()
}

fn deferred_item(item: RibbonItem, shared: &RefAny) -> RibbonItem {
    match item {
        RibbonItem::LargeButton(rb) => RibbonItem::LargeButton(deferred_button(rb, shared)),
        RibbonItem::SmallButton(rb) => RibbonItem::SmallButton(deferred_button(rb, shared)),
        RibbonItem::Column(col) => RibbonItem::Column(RibbonColumn {
            items: RibbonItemVec::from_vec(deferred_items(
                col.items.into_library_owned_vec(),
                shared,
            )),
        }),
        RibbonItem::Row(row) => RibbonItem::Row(RibbonRow {
            items: RibbonItemVec::from_vec(deferred_items(
                row.items.into_library_owned_vec(),
                shared,
            )),
        }),
        RibbonItem::Gallery(mut gallery) => {
            let on_select =
                core::mem::replace(&mut gallery.on_select, OptionRibbonGalleryOnSelect::None);
            if let Some(on_select) = on_select.into_option() {
                gallery.on_select =
                    OptionRibbonGalleryOnSelect::Some(RibbonGalleryOnSelect::create(
                        RefAny::new(PopupSelect {
                            shared: shared.clone(),
                            on_select,
                        }),
                        on_ribbon_group_popup_select as RibbonGalleryOnSelectCallbackType,
                    ));
            }
            RibbonItem::Gallery(gallery)
        }
        other => other,
    }
}

fn deferred_button(mut rb: RibbonButton, shared: &RefAny) -> RibbonButton {
    // A Menu - or a split button without an arrow part, drawn like one -
    // opens its menu with its whole face: it runs in the popup, beside it.
    if rb.arrow != RibbonArrow::None && !rb.is_split() {
        return rb;
    }
    let on_click = core::mem::replace(&mut rb.on_click, OptionButtonOnClick::None);
    if let Some(on_click) = on_click.into_option() {
        rb.on_click = OptionButtonOnClick::Some(super::button::ButtonOnClick::create(
            RefAny::new(PopupCommand {
                shared: shared.clone(),
                on_click,
            }),
            on_ribbon_group_popup_command as ButtonOnClickCallbackType,
        ));
    }
    rb
}

/// What a callback of a collapsed group's popup was built with.
struct InPopupWindow {
    callback: CoreCallback,
    refany: RefAny,
}

/// Every callback under `dom` through [`on_ribbon_group_popup_event`].
fn run_in_popup_window(dom: &mut Dom) {
    let callbacks = dom.root.get_callbacks().clone().into_library_owned_vec();
    if !callbacks.is_empty() {
        let wrapped: Vec<CoreCallbackData> = callbacks
            .into_iter()
            .map(|c| CoreCallbackData {
                event: c.event,
                callback: CoreCallback {
                    cb: on_ribbon_group_popup_event as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(InPopupWindow {
                    callback: c.callback,
                    refany: c.refany,
                }),
            })
            .collect();
        dom.root.set_callbacks(wrapped.into());
    }
    let children: &mut [Dom] = dom.children.as_mut();
    for child in children {
        run_in_popup_window(child);
    }
}

/// What a collapsed group's popup and its `<transient-window>` share: the
/// command a click in the popup picked, waiting for the popup to close.
struct GroupPopupShared {
    pending: Option<PopupPick>,
}

/// A command picked in a collapsed group's popup.
enum PopupPick {
    /// A button's click.
    Click(super::button::ButtonOnClick),
    /// A gallery cell, by its index.
    Select(RibbonGalleryOnSelect, usize),
}

/// A command button of a collapsed group's popup: the popup's shared part
/// and the app's click.
struct PopupCommand {
    shared: RefAny,
    on_click: super::button::ButtonOnClick,
}

/// A gallery of a collapsed group's popup: the popup's shared part and the
/// app's pick.
struct PopupSelect {
    shared: RefAny,
    on_select: RibbonGalleryOnSelect,
}

/// A rebuild asked for in a popup's window must reach the app's window too:
/// the popup shows a subtree of the app's DOM, which only the app's window
/// rebuilds.
const fn everywhere(update: Update) -> Update {
    match update {
        Update::RefreshDom => Update::RefreshDomAllWindows,
        other => other,
    }
}

/// A collapsed group's button was clicked: its popup opens - or closes, when
/// it shows. The engine knows whether it shows: a press outside or Escape
/// closes it too, and a rebuild of the app keeps it up.
extern "C" fn on_ribbon_group_button_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let Some(group) = ancestor_with_class(&info, hit, RIBBON_GROUP_COLLAPSED_CLASS) else {
        return Update::DoNothing;
    };
    let Some(popup) = info.get_last_child(group) else {
        return Update::DoNothing;
    };
    let open = !info.is_transient_window_open(popup);
    if open {
        // A fresh showing forgets a pick an earlier one left.
        if let Some(mut shared) = data.downcast_mut::<GroupPopupShared>() {
            shared.pending = None;
        }
    }
    info.set_transient_window_open(popup, open);
    Update::DoNothing
}

/// A callback of a collapsed group's popup - a check box, a combo box, a
/// button that opens a menu, the app's own control - run where it was
/// clicked, in the popup's window, with a rebuild it asks for widened to
/// every window ([`everywhere`]).
extern "C" fn on_ribbon_group_popup_event(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((callback, refany)) = data
        .downcast_ref::<InPopupWindow>()
        .map(|p| (p.callback.clone(), p.refany.clone()))
    else {
        return Update::DoNothing;
    };
    everywhere(Callback::from_core(callback).invoke(refany, info))
}

/// A command of a collapsed group's popup was clicked: it is noted and the
/// popup closes; the app runs it in its own window once the popup has
/// closed ([`on_ribbon_group_popup_dismissed`]).
extern "C" fn on_ribbon_group_popup_command(mut data: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();
    let Some((mut shared, on_click)) = data
        .downcast_ref::<PopupCommand>()
        .map(|c| (c.shared.clone(), c.on_click.clone()))
    else {
        return Update::DoNothing;
    };
    note_and_close(&mut shared, PopupPick::Click(on_click), &mut info);
    Update::DoNothing
}

/// A gallery cell of a collapsed group's popup was clicked: noted, as a
/// command is ([`on_ribbon_group_popup_command`]).
extern "C" fn on_ribbon_group_popup_select(
    mut data: RefAny,
    mut info: CallbackInfo,
    cell_index: usize,
) -> Update {
    let Some((mut shared, on_select)) = data
        .downcast_ref::<PopupSelect>()
        .map(|c| (c.shared.clone(), c.on_select.clone()))
    else {
        return Update::DoNothing;
    };
    note_and_close(
        &mut shared,
        PopupPick::Select(on_select, cell_index),
        &mut info,
    );
    Update::DoNothing
}

/// Notes `pick` in the popup's shared part and closes the popup the click
/// was in.
fn note_and_close(shared: &mut RefAny, pick: PopupPick, info: &mut CallbackInfo) {
    if let Some(mut s) = shared.downcast_mut::<GroupPopupShared>() {
        s.pending = Some(pick);
    }
    let hit = info.get_hit_node();
    if let Some(window) = popup_window_of(info, hit) {
        info.set_transient_window_open(window, false);
    }
}

/// The `<transient-window>` of the collapsed group whose popup `node` is in:
/// in the popup's own window its root (the window node, its classes kept),
/// in the app's window the window node itself. The walk is bounded so a
/// malformed tree cannot spin.
fn popup_window_of(info: &CallbackInfo, node: DomNodeId) -> Option<DomNodeId> {
    let mut current = Some(node);
    for _ in 0..64 {
        let n = current?;
        if info
            .get_node_classes(n)
            .as_ref()
            .iter()
            .any(|c| c.as_str() == RIBBON_GROUP_POPUP_WINDOW_CLASS)
        {
            return Some(n);
        }
        current = info.get_parent(n);
    }
    None
}

/// A collapsed group's popup closed - a command closed it, or a press
/// outside, or Escape - in the app's window: the command it noted, if any,
/// runs now, once.
extern "C" fn on_ribbon_group_popup_dismissed(mut data: RefAny, info: CallbackInfo) -> Update {
    let pick = match data.downcast_mut::<GroupPopupShared>() {
        Some(mut shared) => shared.pending.take(),
        None => return Update::DoNothing,
    };
    match pick {
        Some(PopupPick::Click(super::button::ButtonOnClick { refany, callback })) => {
            callback.invoke(refany, info)
        }
        Some(PopupPick::Select(RibbonGalleryOnSelect { refany, callback }, cell)) => {
            callback.invoke(refany, info, cell)
        }
        None => Update::DoNothing,
    }
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
        Some(RibbonOnTabClick { refany, callback }) => callback.invoke(refany.clone(), info, idx),
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
    // Mobile first: the band is what "collapsed" means there. Desktop has no
    // band, so it falls through to the content div as before.
    descendant_with_class(info, ribbon, MOBILE_BAND_CLASS)
        .or_else(|| descendant_with_class(info, ribbon, RIBBON_CONTENT_CLASS))
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
    }
    // Picking from the expansion panel closes it - a list gallery's command
    // (which moves no highlight) too.
    if in_panel {
        if let Some(panel) = info.get_parent(cell) {
            info.set_css_property(panel, P::const_display(LayoutDisplay::None));
        }
    }

    match user.into_option() {
        Some(RibbonGalleryOnSelect { refany, callback }) => callback.invoke(refany, info, idx),
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
    use azul_css::{
        dynamic_selector::{DynamicSelector, DynamicSelectorVec, PseudoStateType, ThemeCondition},
        props::property::{CssProperty, CssPropertyType},
        system::SystemStyle,
    };
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
            ctx: core::cell::RefCell::new(OptionRefAny::None),
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
    fn ribbon_new_defaults_to_office_2010_with_tab_zero_active() {
        for count in [0usize, 1, 2, 9] {
            let r = Ribbon::new(tabs(count));
            assert_eq!(r.tabs.len(), count);
            assert_eq!(r.active_tab, 0);
            assert!(r.on_tab_click.is_none());
            assert!(r.app_button.is_none());
            assert_eq!(r.style, RibbonStyle::office_2010());
        }
    }

    #[test]
    fn ribbon_style_default_is_office_2010() {
        assert_eq!(RibbonStyle::default(), RibbonStyle::office_2010());
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
            .with_theme(UiTheme::Flat)
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
        let s = RibbonStyle::default();
        assert_eq!(inline_props(&ch[1]), style_props(&s.resolved_tab_style()));
        assert_eq!(
            inline_props(&ch[2]),
            style_props(&s.resolved_tab_active_style())
        );
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
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab]))
            .with_theme(UiTheme::Flat)
            .dom();
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

        let s = RibbonStyle::default();
        // Verbatim, states included: the ribbon injects a complete part style
        // (its hover/pressed pairs now come from `flat::hover_bg_both` etc.),
        // and `Button::dom` appends nothing over an injected container style —
        // a caller who supplied one chose every property in it.
        assert_eq!(
            inline_props(&node),
            style_props(&s.resolved_large_button_style())
        );
        assert_eq!(
            inline_props(&ch[0]),
            style_props(&s.resolved_large_icon_style())
        );
        assert_eq!(
            inline_props(&ch[1]),
            style_props(&s.resolved_large_label_style())
        );
        assert_eq!(
            inline_props(&ch[2]),
            style_props(&s.resolved_arrow_icon_style())
        );
    }

    /// AzSheets' Font and Alignment groups become rows of icon-only buttons
    /// (Excel's); each must still say what it is.
    #[test]
    fn an_icon_only_button_is_named_by_its_alt() {
        let rb = small_btn("format_bold", "").with_alt(AzString::from_const_str("Bold"));
        let node = render_item(RibbonItem::SmallButton(rb));
        let info = node.root.get_accessibility_info().expect("a button role");
        assert_eq!(info.accessibility_name.as_ref().map(|n| n.as_str()), Some("Bold"));
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

        let s = RibbonStyle::default();
        let mut expected = style_props(&s.resolved_small_button_style());
        expected.extend(style_props(&s.resolved_checked_style()));
        // Verbatim: the injected style is the whole inline style (see
        // `large_button_expands_to_a_button_widget_with_icon_label_and_arrow`).
        assert_eq!(
            inline_props(&node),
            expected,
            "checked props must come last so they win (inline CSS is last-wins)"
        );
    }

    /// Office greys a command that cannot run and its tooltip says why
    /// ("Paste: copy or cut something first"). A disabled ribbon button keeps
    /// its place and its keyboard stop, never runs the app's callback, drops
    /// the hover / pressed paint, is dimmed, is announced as unavailable with
    /// the reason as its description, and shows the reason as a tooltip when
    /// the pointer rests on it or it is clicked.
    #[test]
    fn a_disabled_button_is_dimmed_inert_and_says_why() {
        extern "C" fn app_click(_: RefAny, _: CallbackInfo) -> Update {
            Update::RefreshDom
        }
        let reason = "Nothing to paste: copy or cut something first";
        let rb = small_btn("content_paste", "Paste")
            .with_on_click(RefAny::new(7u32), app_click as crate::widgets::button::ButtonOnClickCallbackType)
            .with_disabled(AzString::from(reason));
        assert!(rb.is_disabled());
        assert_eq!(rb.disabled_reason.as_str(), reason);
        assert!(!small_btn("content_paste", "Paste").is_disabled());

        let node = render_item(RibbonItem::SmallButton(rb));
        assert!(has_class(&node, "__azul-native-button"), "still the Button widget");
        assert!(has_class(&node, RIBBON_DISABLED_CLASS));

        let a11y = node
            .root
            .get_accessibility_info()
            .expect("a ribbon button is announced");
        assert!(
            a11y.states
                .as_ref()
                .contains(&azul_core::a11y::AccessibilityState::Unavailable),
            "announced as unavailable"
        );
        assert_eq!(
            a11y.description.as_ref().map(|d| d.as_str()),
            Some(reason),
            "the reason is the description"
        );

        // Dimmed, and no hover / pressed paint left.
        let mut dimmed = false;
        for (prop, conditions) in node.root.style.iter_inline_properties() {
            let states: Vec<&DynamicSelector> = conditions
                .as_ref()
                .iter()
                .filter(|c| {
                    matches!(
                        c,
                        DynamicSelector::PseudoState(PseudoStateType::Hover | PseudoStateType::Active)
                    )
                })
                .collect();
            assert!(states.is_empty(), "a disabled button has no {states:?} paint: {prop:?}");
            if let CssProperty::Opacity(o) = prop {
                if o.get_property().map_or(false, |o| o.inner.normalized() < 0.75) {
                    dimmed = true;
                }
            }
        }
        assert!(dimmed, "a disabled button is dimmed");

        // The app's callback is gone; what is left shows the reason.
        let callbacks = node.root.get_callbacks().as_ref();
        assert!(!callbacks.is_empty(), "hover and click show the reason");
        for cb in callbacks {
            let mut data = cb.refany.clone();
            assert!(
                data.downcast_ref::<u32>().is_none(),
                "the app's click data is not attached to a disabled button"
            );
            assert!(
                data.downcast_ref::<crate::widgets::button::DisabledReason>().is_some(),
                "every callback of a disabled button carries its reason"
            );
        }
        let events: Vec<EventFilter> = callbacks.iter().map(|cb| cb.event).collect();
        for wanted in [
            EventFilter::Hover(HoverEventFilter::MouseEnter),
            EventFilter::Hover(HoverEventFilter::MouseLeave),
            EventFilter::Hover(HoverEventFilter::Click),
        ] {
            assert!(events.contains(&wanted), "{wanted:?} in {events:?}");
        }
    }

    // ------------------------------------------------------------------
    // Interactive states (declared by the theme module, with dark twins)
    // ------------------------------------------------------------------

    /// Every hover / pressed / focus declaration in `decls` has a twin gated
    /// on `Theme(Dark)` for the same property — and there is at least one.
    ///
    /// The states moved OUT of this file into `themes::flat` (phase 2 of the
    /// widget theme migration), which is a move nothing else in this suite
    /// would notice: it compiles either way, and every other assertion here
    /// passes if the theme silently drops them or ships the light half alone.
    fn assert_every_state_rule_has_a_dark_twin<'a>(
        what: &str,
        decls: impl Iterator<Item = (&'a CssProperty, &'a DynamicSelectorVec)>,
    ) {
        let mut light: Vec<(CssPropertyType, PseudoStateType)> = Vec::new();
        let mut dark: Vec<(CssPropertyType, PseudoStateType)> = Vec::new();
        for (p, conds) in decls {
            let conds = conds.as_ref();
            let state = conds.iter().find_map(|c| match c {
                DynamicSelector::PseudoState(
                    s @ (PseudoStateType::Hover | PseudoStateType::Active | PseudoStateType::Focus),
                ) => Some(*s),
                _ => None,
            });
            let Some(state) = state else { continue };
            let is_dark = conds
                .iter()
                .any(|c| matches!(c, DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)));
            if is_dark {
                dark.push((p.get_type(), state));
            } else {
                light.push((p.get_type(), state));
            }
        }
        assert!(
            !light.is_empty(),
            "{what}: carries no hover/pressed/focus rule at all — the theme forgot to append them"
        );
        for (ty, state) in &light {
            assert!(
                dark.contains(&(*ty, *state)),
                "{what}: `{ty:?}` on {state:?} has no dark twin, so its light-mode value is \
                 painted on a dark surface"
            );
        }
    }

    /// The first background fill on `node` gated on `state`, in the light or
    /// the dark half.
    fn state_fill(node: &Dom, state: PseudoStateType, want_dark: bool) -> CssProperty {
        node.root
            .style
            .iter_inline_properties()
            .find(|(p, conds)| {
                let conds = conds.as_ref();
                let gated_on_state = conds
                    .iter()
                    .any(|c| matches!(c, DynamicSelector::PseudoState(s) if *s == state));
                let is_dark = conds
                    .iter()
                    .any(|c| matches!(c, DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)));
                matches!(p, CssProperty::BackgroundContent(_))
                    && gated_on_state
                    && is_dark == want_dark
            })
            .map(|(p, _)| p.clone())
            .unwrap_or_else(|| panic!("no {state:?} fill on the node (dark: {want_dark})"))
    }

    #[test]
    fn every_ribbon_state_rule_has_a_dark_twin_chosen_for_its_surface() {
        use azul_css::StringVec;

        let dom = Ribbon::new(tabs(2))
            .with_theme(UiTheme::Flat)
            .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
            .dom();
        let (bar, _) = parts(&dom);
        let ch = bar.children.as_ref();
        // [app, t0 (active), t1, filler]
        let (app, tab) = (&ch[0], &ch[2]);

        // The application button is an ACCENT fill in either mode, so its
        // hover keeps the palette's `accent_hover` in dark mode too.
        assert_every_state_rule_has_a_dark_twin(
            "application button",
            app.root.style.iter_inline_properties(),
        );
        assert_eq!(
            state_fill(app, PseudoStateType::Hover, true),
            state_fill(app, PseudoStateType::Hover, false),
            "the application button does not go grey in dark mode, so neither may its hover"
        );

        // An inactive tab's text takes the accent on hover; the dark twin is
        // the theme's accent, not the palette's light-mode blue. (The HOVER
        // twin: the tab's resting text has a dark twin of its own now.)
        assert_every_state_rule_has_a_dark_twin("tab", tab.root.style.iter_inline_properties());
        let dark_hover_text = tab
            .root
            .style
            .iter_inline_properties()
            .find(|(p, conds)| {
                let conds = conds.as_ref();
                matches!(p, CssProperty::TextColor(_))
                    && conds
                        .iter()
                        .any(|c| matches!(c, DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)))
                    && conds
                        .iter()
                        .any(|c| matches!(c, DynamicSelector::PseudoState(PseudoStateType::Hover)))
            })
            .map(|(p, _)| p.clone());
        assert_eq!(
            dark_hover_text,
            Some(P::const_text_color(StyleTextColor {
                inner: flat::DARK_ACC
            })),
            "a hovered tab's text is the accent, so its dark twin is the theme's accent"
        );

        // A small button (the chassis: hover fill, hover border, pressed fill)
        // and a gallery cell sit on the page-neutral chrome, so their dark
        // twins are the theme's tokens.
        let button = render_item(RibbonItem::SmallButton(small_btn("format_bold", "Bold")));
        assert_every_state_rule_has_a_dark_twin(
            "small button",
            button.root.style.iter_inline_properties(),
        );
        let wrapper = render_item(RibbonItem::Gallery(gallery(1)));
        let cell = &wrapper.children.as_ref()[0].children.as_ref()[0]
            .children
            .as_ref()[0];
        assert_every_state_rule_has_a_dark_twin(
            "gallery cell",
            cell.root.style.iter_inline_properties(),
        );
        // The default palette is Office 2010's: the hover is its yellow face,
        // and by night the theme's amber one.
        assert_eq!(
            state_fill(cell, PseudoStateType::Hover, false),
            super::super::themes::decl::layers(vec![super::super::themes::decl::face(
                flat::LIGHT_HT,
                flat::LIGHT_HB
            )]),
            "the light half is Office 2010's hover face"
        );
        assert_eq!(
            state_fill(cell, PseudoStateType::Hover, true),
            super::super::themes::decl::layers(vec![super::super::themes::decl::face(
                flat::DARK_HT,
                flat::DARK_HB
            )]),
            "a cell sits on the neutral chrome, so its dark hover is the theme's hover face"
        );

        // The combo field's focus ring, from the style the ribbon injects.
        let combo = RibbonStyle::office_2013().styled_combo_box(
            StringVec::from_vec(vec![AzString::from("Calibri")]),
            AzString::from("Calibri"),
            120,
        );
        let field = combo
            .field_style
            .into_option()
            .expect("the ribbon injects a field style");
        assert_every_state_rule_has_a_dark_twin(
            "combo field",
            field.as_ref().iter().map(|c| (&c.property, &c.apply_if)),
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

    /// `with_items` / `with_groups` append a whole list, in order - what
    /// six apps folded `with_item` over (DEDUP_OFFICE D9 / A4).
    #[test]
    fn with_items_appends_the_list_in_order() {
        let items = || {
            RibbonItemVec::from_vec(vec![
                RibbonItem::SmallButton(small_btn("content_cut", "Cut")),
                RibbonItem::Separator,
            ])
        };
        let column = RibbonColumn::new()
            .with_item(RibbonItem::SmallButton(small_btn("content_copy", "Copy")))
            .with_items(items());
        let labels: Vec<&str> = column
            .items
            .as_ref()
            .iter()
            .map(|it| match it {
                RibbonItem::SmallButton(b) => b.label.as_str(),
                RibbonItem::Separator => "|",
                _ => "?",
            })
            .collect();
        assert_eq!(labels, vec!["Copy", "Cut", "|"]);
        assert_eq!(RibbonRow::new().with_items(items()).items.len(), 2);
        let group = RibbonGroup::new(AzString::from("Clipboard")).with_items(items());
        assert_eq!(group.items.len(), 2);
        let tab = RibbonTab::new(AzString::from("Home")).with_groups(RibbonGroupVec::from_vec(vec![
            group.clone(),
            RibbonGroup::new(AzString::from("Font")),
        ]));
        let names: Vec<&str> = tab.groups.as_ref().iter().map(|g| g.label.as_str()).collect();
        assert_eq!(names, vec!["Clipboard", "Font"]);
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
        let s = RibbonStyle::default();
        let mut expected = style_props(&s.resolved_gallery_cell_style());
        expected.extend(style_props(&s.resolved_gallery_cell_selected_style()));
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

    /// AzShow's HOME put all seven layouts inline (863 px) and pushed Font,
    /// Paragraph and Editing off a 1280 px window. A gallery shows one row
    /// of `visible` cells - the row holding the selected cell - and "More"
    /// every cell; a click still reports the cell's own index.
    #[test]
    fn a_gallery_shows_the_row_of_the_selected_cell_and_more_shows_every_cell() {
        let wrapper = render_item(RibbonItem::Gallery(gallery(7).with_selected(4).with_visible(3)));
        let frame = &wrapper.children.as_ref()[0];
        let strip = &frame.children.as_ref()[0];
        let labels: Vec<String> = strip
            .children
            .as_ref()
            .iter()
            .map(|c| text_of(&c.children.as_ref()[1]).unwrap_or_default().to_string())
            .collect();
        assert_eq!(labels, vec!["Style 3", "Style 4", "Style 5"], "the row of cell 4");
        let panel = &wrapper.children.as_ref()[1];
        assert_eq!(panel.children.as_ref().len(), 7, "More shows every cell");

        // The last row is filled from the end, not left short.
        let last = render_item(RibbonItem::Gallery(gallery(7).with_selected(6).with_visible(3)));
        let strip = &last.children.as_ref()[0].children.as_ref()[0];
        assert_eq!(text_of(&strip.children.as_ref()[0].children.as_ref()[1]), Some("Style 4"));
        assert_eq!(strip.children.as_ref().len(), 3);

        // No selection (AzShow's New Slide gallery): the first row.
        let none = render_item(RibbonItem::Gallery(gallery(7).with_selected(usize::MAX).with_visible(3)));
        let strip = &none.children.as_ref()[0].children.as_ref()[0];
        assert_eq!(text_of(&strip.children.as_ref()[0].children.as_ref()[1]), Some("Style 0"));

        // 0 (the default) shows every cell.
        let all = render_item(RibbonItem::Gallery(gallery(5)));
        assert_eq!(all.children.as_ref()[0].children.as_ref()[0].children.as_ref().len(), 5);
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
        let mut r = Ribbon::new(RibbonTabVec::from_vec(vec![tab]))
            .with_theme(UiTheme::Flat);
        r.style.small_button_style = OptionCssPropertyWithConditionsVec::Some(injected.clone());
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
        assert_ne!(
            combo.resolved_field_style(),
            default.resolved_field_style(),
            "field restyled"
        );
        assert_eq!(combo.combo_state.inner.text.as_str(), "Calibri (Body)");

        // the width is the LAST wrapper property, so it wins over any base width
        let wrapper = combo.resolved_wrapper_style();
        let last = wrapper.as_ref().last().expect("wrapper style is non-empty");
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
            .resolved_app_button_style()
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
            .resolved_tab_active_style()
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

        let tab_bar = s.resolved_tab_bar_style();
        let displays: Vec<(&LayoutDisplay, bool)> = tab_bar
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
        let mobile_tab = s.resolved_mobile_tab_button_style();
        let mobile: Vec<(&LayoutDisplay, bool)> = mobile_tab
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
        // The bundle stores the HANDEDNESS now and derives the part from it, so
        // comparing the fields would compare two `None`s. Both facts are worth
        // pinning: the input differs, and so does what it resolves to.
        assert_ne!(right.handedness, left.handedness);
        assert_ne!(
            right.resolved_mobile_group_list_style(),
            left.resolved_mobile_group_list_style()
        );

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
            has(&right.resolved_mobile_group_list_style(), true),
            "a right-handed list sits at the right edge, so its divider is on its LEFT"
        );
        assert!(
            has(&left.resolved_mobile_group_list_style(), false),
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
            .resolved_field_style()
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
    // ------------------------------------------------------------------
    // Resting colours, light and dark
    // ------------------------------------------------------------------
    //
    // The ribbon's resting surfaces are declared from `RibbonTheme`, whose
    // fields are LIGHT values; the dark twins come from `dark_counterpart`.
    // These pins say two things: the light look is byte-for-byte what it was
    // before the twins existed, and every twin is where last-match-wins can
    // see it.

    /// The colour a declaration carries, if it is a colour property. A
    /// gradient or image background is not a colour (none in the ribbon).
    fn colour_of(p: &CssProperty) -> Option<ColorU> {
        match p {
            CssProperty::BackgroundContent(b) => b
                .get_property()
                .and_then(|v| v.as_ref().first())
                .and_then(|c| match c {
                    StyleBackgroundContent::Color(c) => Some(*c),
                    _ => None,
                }),
            CssProperty::TextColor(t) => t.get_property().map(|t| t.inner),
            CssProperty::BorderTopColor(c) => c.get_property().map(|c| c.inner),
            CssProperty::BorderLeftColor(c) => c.get_property().map(|c| c.inner),
            CssProperty::BorderRightColor(c) => c.get_property().map(|c| c.inner),
            CssProperty::BorderBottomColor(c) => c.get_property().map(|c| c.inner),
            _ => None,
        }
    }

    /// `(type, colour)` of every unconditional colour declaration on `node`,
    /// in declaration order — the light look of the node.
    fn light_colours(node: &Dom) -> Vec<(CssPropertyType, ColorU)> {
        crate::widgets::theme_probe::unconditional(node)
            .iter()
            .filter_map(|p| colour_of(p).map(|c| (p.get_type(), c)))
            .collect()
    }

    #[test]
    fn the_light_look_of_the_chrome_is_unchanged_by_the_dark_twins() {
        use crate::widgets::theme_probe::unconditional;

        let dom = Ribbon::new(tabs(2))
            .with_theme(UiTheme::Flat)
            // The Office 2013 palette: the twins of plain colours.
            .with_style(RibbonStyle::office_2013())
            .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
            .dom();
        let (bar, content) = parts(&dom);

        // The root: white chrome over a #D4D4D4 bottom rule, exactly as the
        // Office palette declares it.
        assert_eq!(
            unconditional(&dom),
            vec![
                P::const_display(LayoutDisplay::Flex),
                P::const_flex_direction(LayoutFlexDirection::Column),
                P::const_flex_grow(LayoutFlexGrow::const_new(0)),
                P::const_font_family(SYSTEM_UI_FAMILY),
                P::const_font_size(StyleFontSize::const_px(12)),
                P::const_background_content(bg_vec(WHITE)),
                P::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1)),
                P::const_border_bottom_style(StyleBorderBottomStyle {
                    inner: BorderStyle::Solid,
                }),
                P::const_border_bottom_color(StyleBorderBottomColor { inner: W13_BORDER }),
            ],
            "root: the unconditional declarations are the light look, in order"
        );
        assert_eq!(
            unconditional(bar),
            vec![
                P::const_box_sizing(LayoutBoxSizing::BorderBox),
                P::const_display(LayoutDisplay::Flex),
                P::const_flex_direction(LayoutFlexDirection::Row),
                P::const_flex_grow(LayoutFlexGrow::const_new(0)),
                P::const_height(LayoutHeight::const_px(26)),
                P::const_background_content(bg_vec(WHITE)),
            ],
            "tab bar"
        );
        assert_eq!(
            unconditional(content),
            vec![
                P::const_box_sizing(LayoutBoxSizing::BorderBox),
                P::const_display(LayoutDisplay::Flex),
                P::const_flex_direction(LayoutFlexDirection::Row),
                P::const_flex_grow(LayoutFlexGrow::const_new(0)),
                P::const_flex_shrink(LayoutFlexShrink {
                    inner: FloatValue::const_new(0),
                }),
                P::const_height(LayoutHeight::const_px(92)),
                P::const_background_content(bg_vec(WHITE)),
            ],
            "content band"
        );
    }

    #[test]
    fn the_light_colours_of_the_tab_strip_are_unchanged_by_the_dark_twins() {
        use CssPropertyType as T;

        let dom = Ribbon::new(tabs(2))
            .with_theme(UiTheme::Flat)
            // The Office 2013 palette: the twins of plain colours.
            .with_style(RibbonStyle::office_2013())
            .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
            .dom();
        let (bar, _) = parts(&dom);
        let ch = bar.children.as_ref();
        // [app, t0 (active), t1, filler]
        let (app, active, tab, filler) = (&ch[0], &ch[1], &ch[2], &ch[3]);

        assert_eq!(
            light_colours(app),
            vec![(T::BackgroundContent, W13_BLUE), (T::TextColor, WHITE)],
            "application button: accent fill, white label"
        );
        assert_eq!(
            light_colours(tab),
            vec![
                (T::TextColor, W13_TEXT),
                (T::BackgroundContent, WHITE),
                (T::BorderBottomColor, W13_BORDER),
            ],
            "inactive tab"
        );
        assert_eq!(
            light_colours(active),
            vec![
                (T::TextColor, W13_BLUE),
                (T::BackgroundContent, WHITE),
                (T::BorderTopColor, W13_BORDER),
                (T::BorderLeftColor, W13_BORDER),
                (T::BorderRightColor, W13_BORDER),
                (T::BorderBottomColor, W13_BORDER),
                // The underline erased: the bottom edge matches the content band.
                (T::BorderBottomColor, WHITE),
            ],
            "active tab"
        );
        assert_eq!(
            light_colours(filler),
            vec![(T::BorderBottomColor, W13_BORDER)],
            "tab filler"
        );
    }
    /// Every part builder, by name, over one palette (and the left-handed
    /// group list, which is the one part that takes a second input).
    fn every_builder(t: &RibbonTheme) -> Vec<(&'static str, CssPropertyWithConditionsVec)> {
        vec![
            ("container", theme_container(t)),
            ("tab_bar", theme_tab_bar(t)),
            ("app_button", theme_app_button(t)),
            ("tab", theme_tab(t)),
            ("tab_active", theme_tab_active(t)),
            ("tab_filler", theme_tab_filler(t)),
            ("content", theme_content(t)),
            ("group", theme_group(t)),
            ("group_label", theme_group_label(t)),
            ("launcher_button", theme_launcher_button(t)),
            ("launcher_icon", theme_launcher_icon(t)),
            ("separator", theme_separator(t)),
            ("large_button", theme_large_button(t)),
            ("large_icon", theme_large_icon(t)),
            ("large_label", theme_large_label(t)),
            ("small_button", theme_small_button(t)),
            ("small_icon", theme_small_icon(t)),
            ("small_label", theme_small_label(t)),
            ("arrow_icon", theme_arrow_icon(t)),
            ("checked", theme_checked(t)),
            ("gallery_frame", theme_gallery_frame(t)),
            ("gallery_cell", theme_gallery_cell(t)),
            ("gallery_cell_selected", theme_gallery_cell_selected(t)),
            ("gallery_cell_label", theme_gallery_cell_label(t)),
            ("gallery_spinner", theme_gallery_spinner(t)),
            ("gallery_panel", theme_gallery_panel(t)),
            ("gallery_spinner_button", theme_gallery_spinner_button(t)),
            ("gallery_spinner_icon", theme_gallery_spinner_icon(t)),
            (
                "combo_wrapper",
                CssPropertyWithConditionsVec::from_vec(theme_combo_wrapper_base(t)),
            ),
            ("combo_field", theme_combo_field(t)),
            ("combo_arrow", theme_combo_arrow(t)),
            ("mobile_tab_button", theme_mobile_tab_button(t)),
            ("mobile_tab_label", theme_mobile_tab_label(t)),
            ("mobile_tab_arrow", theme_mobile_tab_arrow(t)),
            ("mobile_tab_overlay", theme_mobile_tab_overlay(t)),
            ("mobile_tab_overlay_item", theme_mobile_tab_overlay_item(t)),
            (
                "mobile_group_list (right)",
                theme_mobile_group_list(t, false),
            ),
            ("mobile_group_list (left)", theme_mobile_group_list(t, true)),
            ("mobile_group_list_item", theme_mobile_group_list_item(t)),
            (
                "mobile_group_list_item_selected",
                theme_mobile_group_list_item_selected(t),
            ),
        ]
    }

    fn is_state_gated(conds: &DynamicSelectorVec) -> bool {
        conds
            .as_ref()
            .iter()
            .any(|c| matches!(c, DynamicSelector::PseudoState(_)))
    }

    fn is_dark_gated(conds: &DynamicSelectorVec) -> bool {
        conds
            .as_ref()
            .iter()
            .any(|c| matches!(c, DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)))
    }

    /// Invariant I8 for the ribbon's RESTING colours: every opaque colour a
    /// builder declares unconditionally has a dark twin of the same property,
    /// declared AFTER it (last-match-wins), and no builder declares a resting
    /// dark twin for a property it does not declare in light. The one
    /// exception is the application button — an accent fill and the label on
    /// it are their own colour in both modes. States are the other test's
    /// business (`every_ribbon_state_rule_has_a_dark_twin_chosen_for_its_surface`).
    #[test]
    fn every_resting_colour_of_every_builder_has_a_dark_twin_after_it_and_nothing_else_does() {
        use std::collections::BTreeSet;

        let mut twins_seen = 0usize;
        let palettes = [RibbonTheme::office_2013(), RibbonTheme::office_2010()];
        for (name, part) in palettes.iter().flat_map(every_builder) {
            let mut light: Vec<(CssPropertyType, usize)> = Vec::new();
            let mut dark: Vec<(CssPropertyType, usize)> = Vec::new();
            for (i, c) in part.as_ref().iter().enumerate() {
                let Some(colour) = colour_of(&c.property) else {
                    continue;
                };
                // Transparent fills/borders (a hover paints over them) have no
                // dark value to take; states have their own twins.
                if colour.a == 0 || is_state_gated(&c.apply_if) {
                    continue;
                }
                if c.apply_if.as_ref().is_empty() {
                    light.push((c.property.get_type(), i));
                } else if is_dark_gated(&c.apply_if) {
                    dark.push((c.property.get_type(), i));
                }
            }
            let own_colour = name == "app_button";
            let expect: BTreeSet<CssPropertyType> = if own_colour {
                BTreeSet::new()
            } else {
                light.iter().map(|(t, _)| *t).collect()
            };
            let got: BTreeSet<CssPropertyType> = dark.iter().map(|(t, _)| *t).collect();
            assert_eq!(
                got, expect,
                "{name}: the resting dark twins must cover exactly the resting light colours \
                 (light: {light:?}, dark: {dark:?})"
            );
            for (ty, dark_at) in &dark {
                let light_at = light
                    .iter()
                    .find(|(t, _)| t == ty)
                    .map(|(_, i)| *i)
                    .expect("covered by the set comparison above");
                assert!(
                    light_at < *dark_at,
                    "{name}: the dark twin of {ty:?} (#{dark_at}) precedes its light value \
                     (#{light_at}), so the light value wins in dark mode"
                );
            }
            twins_seen += dark.len();
        }
        assert!(
            twins_seen > 40,
            "only {twins_seen} resting twins: the walk found nothing"
        );
    }

    /// The dark half of the palette is the flat theme's tokens for every
    /// page-neutral field, and the light value for every own-colour field.
    #[test]
    fn the_dark_palette_is_the_flat_themes_tokens_and_keeps_the_accent() {
        let t = RibbonTheme::office_2013();
        let d = t.dark_counterpart();
        assert_eq!(d.chrome_bg, flat::DARK_SUR, "chrome");
        assert_eq!(d.content_bg, flat::DARK_SUR, "content band");
        assert_eq!(d.text, flat::DARK_INK, "control text");
        assert_eq!(d.label, flat::DARK_INK2, "captions and secondary glyphs");
        assert_eq!(d.icon, flat::DARK_ICON, "icon glyphs");
        assert_eq!(d.border, flat::DARK_BD, "chrome borders");
        assert_eq!(d.separator, flat::DARK_SEP, "separators");
        assert_eq!(d.hover_bg, flat::DARK_HT, "hover fill");
        assert_eq!(d.hover_border, flat::DARK_BD, "hover / toggled border");
        assert_eq!(d.pressed_bg, flat::DARK_PT, "pressed fill");
        assert_eq!(d.checked_bg, flat::DARK_PT, "toggled-on fill");
        assert_eq!(d.selected_bg, flat::DARK_HT, "selected cell fill");
        assert_eq!(d.field_border, flat::DARK_BD3, "field border");
        // Own colour: the accent, its hover and the label on it.
        assert_eq!(
            (d.accent, d.accent_hover, d.accent_text),
            (t.accent, t.accent_hover, t.accent_text),
            "an accent fill does not change with the mode"
        );
        // The resting hover/pressed tokens are the ones the state twins use,
        // so a hovered control and the surface under it come from one palette.
        assert_eq!(
            (d.hover_bg, d.hover_border, d.pressed_bg),
            (flat::DARK_HT, flat::DARK_BD, flat::DARK_PT)
        );
        // A palette that reports its own accent keeps it in the dark too.
        let mut sys = SystemStyle::default();
        sys.colors.accent = Some(ColorU {
            r: 9,
            g: 99,
            b: 199,
            a: 255,
        })
        .into();
        let os = RibbonTheme::from_system(sys);
        assert_eq!(os.dark_counterpart().accent, os.accent);
        // A transparent light value (see-through chrome over a window
        // gradient) stays transparent instead of becoming an opaque slab.
        let mut see_through = t;
        see_through.chrome_bg = TRANSPARENT;
        see_through.content_bg = TRANSPARENT;
        let d = see_through.dark_counterpart();
        assert_eq!(d.chrome_bg, TRANSPARENT);
        assert_eq!(d.content_bg, TRANSPARENT);
        assert_eq!(d.text, flat::DARK_INK, "the other fields still go dark");
    }

    /// The accent stays blue in dark mode: the application button keeps its
    /// fill and label untouched (no resting twin at all), and the active
    /// tab's label — the accent as TEXT on the neutral chrome — takes the
    /// flat theme's accent, the same blue its hover text already took.
    #[test]
    fn the_accent_keeps_its_blue_in_dark_mode() {
        use crate::widgets::theme_probe::dark;

        let dom = Ribbon::new(tabs(2))
            .with_theme(UiTheme::Flat)
            // The Office 2013 palette: the twins of plain colours.
            .with_style(RibbonStyle::office_2013())
            .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
            .dom();
        let (bar, _) = parts(&dom);
        let ch = bar.children.as_ref();
        // [app, t0 (active), t1, filler]
        let (app, active) = (&ch[0], &ch[1]);

        let app_dark: Vec<CssProperty> = dark(app);
        assert_eq!(
            app_dark.iter().filter_map(colour_of).collect::<Vec<_>>(),
            vec![W13_BLUE_HOVER],
            "the application button's only dark declaration is its hover fill, and that is the \
             palette's own accent_hover: {app_dark:?}"
        );
        assert!(
            !app_dark
                .iter()
                .any(|p| matches!(p, CssProperty::TextColor(_))),
            "the white label on the accent fill has no dark twin"
        );

        let active_dark_text: Vec<ColorU> = dark(active)
            .iter()
            .filter(|p| matches!(p, CssProperty::TextColor(_)))
            .filter_map(colour_of)
            .collect();
        assert_eq!(
            active_dark_text,
            vec![flat::DARK_ACC],
            "the active tab's label takes the theme's accent in dark mode"
        );
        // (`DARK_ACC` is #3b82f6 — still a blue.)
        let active_dark_bg: Vec<ColorU> = dark(active)
            .iter()
            .filter(|p| matches!(p, CssProperty::BackgroundContent(_)))
            .filter_map(colour_of)
            .collect();
        assert_eq!(
            active_dark_bg,
            vec![flat::DARK_SUR],
            "and the chrome under it went dark"
        );
    }

    /// The chrome itself — root, tab bar, content band — goes dark with the
    /// window: this is the bar that stayed a white Office-2013 strip on a dark
    /// window before the resting surfaces had twins.
    #[test]
    fn the_chrome_goes_dark_with_the_window() {
        use crate::widgets::theme_probe::dark;

        let dom = Ribbon::new(tabs(2)).with_theme(UiTheme::Flat)
            // The Office 2013 palette: the twins of plain colours.
            .with_style(RibbonStyle::office_2013()).dom();
        let (bar, content) = parts(&dom);
        let colours = |node: &Dom| dark(node).iter().filter_map(colour_of).collect::<Vec<_>>();
        assert_eq!(
            colours(&dom),
            vec![flat::DARK_SUR, flat::DARK_BD],
            "root: dark chrome over a dark bottom rule"
        );
        assert_eq!(colours(bar), vec![flat::DARK_SUR], "tab bar");
        assert_eq!(colours(content), vec![flat::DARK_SUR], "content band");
        // And every one of those twins is a dark-mode-only declaration; the
        // light look is pinned separately
        // (`the_light_look_of_the_chrome_is_unchanged_by_the_dark_twins`).
        for node in [&dom, bar, content] {
            for p in dark(node) {
                assert!(
                    !crate::widgets::theme_probe::unconditional(node).contains(&p),
                    "a dark value leaked into the unconditional style: {p:?}"
                );
            }
        }
    }
}

/// The ribbon's flora look (W5a): flora's toolbar strip over a leaf, the
/// selected tab cut as the sunken accent stone, controls that are bare paper
/// until the pointer lifts them - on exactly the flat ribbon's metrics.
#[cfg(test)]
mod flora_tests {
    use azul_css::{dynamic_selector::PseudoStateType, props::property::CssPropertyType, StringVec};

    use super::*;
    use crate::widgets::themes::{flora, theme_checks as tc};

    extern "C" fn noop(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    /// A ribbon with every part a look paints: the application button, a
    /// selected and an unselected tab, a large, a small and a toggled button,
    /// a separator, a dialog launcher and a gallery with a selected cell.
    fn fixture() -> Ribbon {
        let cells: Vec<RibbonGalleryCell> = (0..3)
            .map(|i| RibbonGalleryCell::new(Dom::create_div(), AzString::from(format!("Style {i}"))))
            .collect();
        let clipboard = RibbonGroup::new("Clipboard".into())
            .with_item(RibbonItem::LargeButton(
                RibbonButton::new("content_paste".into(), "Paste".into())
                    .with_arrow(RibbonArrow::Split),
            ))
            .with_item(RibbonItem::Column(
                RibbonColumn::new()
                    .with_item(RibbonItem::SmallButton(RibbonButton::new(
                        "content_cut".into(),
                        "Cut".into(),
                    )))
                    .with_item(RibbonItem::SmallButton(
                        RibbonButton::new("format_bold".into(), "".into()).with_toggled(true),
                    )),
            ))
            .with_item(RibbonItem::Separator)
            .with_launcher(
                RefAny::new(0u8),
                noop as crate::widgets::button::ButtonOnClickCallbackType,
            );
        let styles = RibbonGroup::new("Styles".into()).with_item(RibbonItem::Gallery(
            RibbonGallery::new(cells.into()).with_selected(1),
        ));
        Ribbon::new(RibbonTabVec::from_vec(vec![
            RibbonTab::new("HOME".into())
                .with_group(clipboard)
                .with_group(styles),
            RibbonTab::new("INSERT".into()),
        ]))
        .with_app_button(RibbonAppButton::new("FILE".into()))
    }

    fn ribbon(theme: UiTheme) -> Dom {
        fixture().with_theme(theme).dom()
    }

    /// The adaptive, desktop and touch chromes of the fixture in `theme`.
    fn every_chrome(theme: UiTheme) -> [(&'static str, Dom); 3] {
        [
            ("adaptive", fixture().with_theme(theme).dom()),
            ("desktop", fixture().with_theme(theme).dom_desktop()),
            ("mobile", fixture().with_theme(theme).dom_mobile()),
        ]
    }

    fn node<'a>(dom: &'a Dom, class: &str) -> &'a Dom {
        tc::find(dom, class).unwrap_or_else(|| panic!("the ribbon renders a {class}"))
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

    /// Every button of the fixture, in tree order: Paste, Cut, the toggled
    /// Bold, the launcher, then the gallery's three spinner buttons.
    fn buttons(dom: &Dom) -> Vec<&Dom> {
        tc::find_all(dom, "__azul-native-button")
    }

    #[test]
    fn a_flora_ribbon_is_flora_s_toolbar_strip_over_a_leaf_in_both_modes() {
        let dom = ribbon(UiTheme::Flora);
        let bar = node(&dom, "__azul-native-ribbon-tabbar");
        let content = node(&dom, "__azul-native-ribbon-content");
        for (dark, strip, leaf) in [
            (false, flora::LIGHT_STRIP, flora::LIGHT_SUR),
            (true, flora::DARK_STRIP, flora::DARK_SUR),
        ] {
            assert_eq!(face(&dom, dark, None), fill(strip), "the chrome (dark: {dark})");
            // The tab strip is closed by the website's rule (`.navbar::after`,
            // `--fl-rule-metal-bg`), seen through its transparent 2px foot
            // under the strip's paper on the padding box.
            assert_eq!(
                face(bar, dark, None),
                vec![tc::flora_css_rule_metal(), StyleBackgroundContent::Color(strip)],
                "the tab strip (dark: {dark})"
            );
            assert_eq!(
                tc::background_clips(bar, dark, None),
                vec![StyleBackgroundClip::BorderBox, StyleBackgroundClip::PaddingBox],
                "the rule on the border box, the paper on the padding box (dark: {dark})"
            );
            assert_eq!(
                tc::resolve(bar, CssPropertyType::BorderBottomColor, dark, None)
                    .as_ref()
                    .and_then(tc::border_color),
                Some(ColorU::TRANSPARENT),
                "the strip's foot is transparent, so the metal shows (dark: {dark})"
            );
            assert_eq!(face(content, dark, None), fill(leaf), "the content leaf (dark: {dark})");
        }
    }

    #[test]
    fn a_flora_ribbon_s_selected_tab_is_the_sunken_accent_stone_and_the_others_lift_under_the_pointer(
    ) {
        let dom = ribbon(UiTheme::Flora);
        let active = node(&dom, "__azul-native-ribbon-tab-active");
        let tabs = tc::find_all(&dom, "__azul-native-ribbon-tab");
        let other = tabs
            .iter()
            .find(|t| !tc::has_class(t, "__azul-native-ribbon-tab-active"))
            .expect("an unselected tab");
        for (dark, soft, hover) in [
            (false, flora::LIGHT_SOFT1, flora::HOVER_FACE_LIGHT),
            (true, flora::DARK_SOFT1, flora::HOVER_FACE_DARK),
        ] {
            let mut cut = vec![tc::flora_css_rolled_tab()];
            cut.extend(flora::australis_face(flora::STONE_STREAK));
            assert_eq!(
                face(active, dark, None),
                cut,
                "the selected tab is the sunken stone stood upright over the rolled metal, its \
                 own colour (dark: {dark})"
            );
            assert_eq!(tc::text_color(active, dark), Some(flora::LIGHT_ON_ACC));
            assert_eq!(tc::text_color(other, dark), Some(soft), "an unselected tab (dark: {dark})");
            assert_eq!(
                face(other, dark, Some(PseudoStateType::Hover)),
                vec![hover],
                "an unselected tab lifts to the hover face (dark: {dark})"
            );
        }
    }

    #[test]
    fn the_flora_application_button_is_the_accent_stone_written_on_accent() {
        let dom = ribbon(UiTheme::Flora);
        let app = node(&dom, "__azul-native-ribbon-appbutton");
        for dark in [false, true] {
            let layers = face(app, dark, None);
            assert_eq!(
                layers.first(),
                Some(&tc::flora_css_rolled_tab()),
                "the stone is set in the rolled metal (dark: {dark})"
            );
            assert_eq!(
                layers.get(1),
                Some(&StyleBackgroundContent::Color(flora::LIGHT_ACC)),
                "a stone is its own colour in both modes (dark: {dark})"
            );
            assert_eq!(tc::text_color(app, dark), Some(flora::LIGHT_ON_ACC));
        }
    }

    #[test]
    fn a_flora_ribbon_button_is_bare_paper_that_lifts_under_the_pointer_and_rings_on_focus() {
        let dom = ribbon(UiTheme::Flora);
        let paste = buttons(&dom)[0];
        for (dark, hover, pressed) in [
            (false, flora::HOVER_FACE_LIGHT, flora::PRESSED_FACE_LIGHT),
            (true, flora::HOVER_FACE_DARK, flora::PRESSED_FACE_DARK),
        ] {
            assert_eq!(
                face(paste, dark, None),
                fill(ColorU::TRANSPARENT),
                "at rest the strip shows through (dark: {dark})"
            );
            assert_eq!(face(paste, dark, Some(PseudoStateType::Hover)), vec![hover]);
            assert_eq!(face(paste, dark, Some(PseudoStateType::Active)), vec![pressed]);
            assert!(tc::has_focus_ring(paste, dark), "a Tab stop is ringed (dark: {dark})");
        }
        assert_eq!(tc::focus_ring_color(paste, false), Some(flora::LIGHT_ACC));
        assert_eq!(tc::focus_ring_color(paste, true), Some(flora::DARK_GLOW));
    }

    #[test]
    fn a_toggled_flora_button_is_pushed_in_paper() {
        let dom = ribbon(UiTheme::Flora);
        let bold = buttons(&dom)[2];
        assert_eq!(face(bold, false, None), vec![flora::PRESSED_FACE_LIGHT]);
        assert_eq!(face(bold, true, None), vec![flora::PRESSED_FACE_DARK]);
    }

    #[test]
    fn the_flora_ribbon_keeps_every_theme_invariant_in_every_chrome() {
        for (chrome, dom) in every_chrome(UiTheme::Flora) {
            tc::assert_theme_invariants(&format!("flora ribbon ({chrome})"), &dom);
        }
    }

    /// `dom` without its tab strip: the ribbon's root with every other child.
    fn without_tab_strip(dom: &Dom) -> Dom {
        let mut d = dom.clone();
        d.children = DomVec::from_vec(
            dom.children
                .as_ref()
                .iter()
                .filter(|c| !tc::has_class(c, "__azul-native-ribbon-tabbar"))
                .cloned()
                .collect(),
        );
        d
    }

    /// Flora repaints the ribbon; it does not re-measure it. The ribbon's
    /// layout was measured for its heights, paddings and borders (the 68px
    /// item row), so the flora look keeps every one of them - except in the
    /// tab strip, which flora cuts as Firefox's tab row (the Australis tab,
    /// `themes::flora`): taller, the tabs standing on its rule, the selected
    /// tab and the application button hung with curves.
    #[test]
    fn a_flora_ribbon_keeps_every_metric_of_the_flat_ribbon_below_its_tab_strip() {
        let flat = every_chrome(UiTheme::Flat);
        let flora_chromes = every_chrome(UiTheme::Flora);
        for ((chrome, a), (_, b)) in flat.iter().zip(flora_chromes.iter()) {
            let moved =
                flora::chrome_metric_findings(&without_tab_strip(a), &without_tab_strip(b));
            assert!(
                moved.is_empty(),
                "the flora ribbon ({chrome}) moves:\n  {}",
                moved.join("\n  ")
            );
        }
    }

    #[test]
    fn a_part_the_caller_set_wins_over_the_flora_look() {
        let own = CssPropertyWithConditionsVec::from_vec(vec![Cond::simple(P::const_font_size(
            StyleFontSize::const_px(99),
        ))]);
        let mut r = fixture().with_theme(UiTheme::Flora);
        r.style.large_button_style = OptionCssPropertyWithConditionsVec::Some(own);
        let dom = r.dom();
        let paste = buttons(&dom)[0];
        assert_eq!(
            paste
                .root
                .style
                .iter_inline_properties()
                .map(|(p, _)| p.clone())
                .collect::<Vec<_>>(),
            vec![P::const_font_size(StyleFontSize::const_px(99))],
            "the caller's part reaches the button verbatim"
        );
    }

    #[test]
    fn a_pinned_ribbon_builds_its_buttons_and_its_unpinned_embedded_widgets_in_its_theme() {
        let marker = |t: UiTheme| match t {
            UiTheme::Flat => style_kit::FLAT_CLASS,
            UiTheme::Flora => style_kit::FLORA_CLASS,
        };
        let combo = |label: &str| ComboBox::new(StringVec::from_vec(vec![AzString::from(label)]));
        for (theme, other) in [(UiTheme::Flat, UiTheme::Flora), (UiTheme::Flora, UiTheme::Flat)] {
            let tab = RibbonTab::new("t".into()).with_group(
                RibbonGroup::new("g".into())
                    .with_item(RibbonItem::SmallButton(RibbonButton::new(
                        "format_bold".into(),
                        "".into(),
                    )))
                    .with_item(RibbonItem::Combo(combo("a")))
                    .with_item(RibbonItem::Combo(combo("b").with_theme(other))),
            );
            let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab]))
                .with_theme(theme)
                .dom();
            assert!(tc::has_class(&dom, marker(theme)), "the root carries its theme marker");
            assert!(
                tc::has_class(buttons(&dom)[0], marker(theme)),
                "a ribbon button is built in the ribbon's theme"
            );
            let combos = tc::find_all(&dom, "__azul-native-combobox");
            assert_eq!(combos.len(), 2);
            assert!(
                tc::has_class(combos[0], marker(theme)),
                "an embedded widget with no theme of its own is built in the ribbon's"
            );
            assert!(
                tc::has_class(combos[1], marker(other)),
                "an embedded widget the caller pinned keeps its pin"
            );
        }
    }

    /// R5: the ribbon's structure is its base - the flat part's geometry,
    /// which the flora look keeps (`themes::flora::ribbon_style`) - declared
    /// once for every app theme, never inside a `@theme(<name>)` block. The
    /// fixture embeds no foreign widget (a combo box, a drop-down, a check
    /// box): built in the ribbon's look, those carry their OWN widget's
    /// structure, which their own tests hold to this rule.
    #[test]
    fn a_ribbon_declares_its_structure_once_for_every_theme() {
        use crate::widgets::themes::{
            theme_blocks::checks::{under, BOTH},
            theme_checks::assert_structure_is_shared,
        };
        for t in BOTH {
            // Every chrome, with the first tab selected (its groups, the
            // toggled button and the picked gallery cell shown) and with the
            // second one selected (an empty tab).
            for active in [0, 1] {
                let chromes = under(t, || {
                    [
                        ("adaptive", fixture().with_active_tab(active).dom()),
                        ("desktop", fixture().with_active_tab(active).dom_desktop()),
                        ("mobile", fixture().with_active_tab(active).dom_mobile()),
                    ]
                });
                for (chrome, dom) in chromes {
                    assert_structure_is_shared(
                        &format!("ribbon ({chrome}, tab {active}) built for {}", t.name()),
                        &dom,
                        &[
                            (
                                "__azul-native-ribbon-tab-active",
                                CssPropertyType::Position,
                                "flora hangs the Australis curves off its selected tab",
                            ),
                            (
                                "__azul-native-ribbon-appbutton",
                                CssPropertyType::Position,
                                "flora hangs the Australis curves off its application button",
                            ),
                        ],
                    );
                }
            }
        }
    }

    /// `node`'s resting border-top colour in the light mode.
    fn top_edge(node: &Dom) -> Option<ColorU> {
        match tc::resolve(node, CssPropertyType::BorderTopColor, false, None)? {
            P::BorderTopColor(azul_css::css::CssPropertyValue::Exact(c)) => Some(c.inner),
            _ => None,
        }
    }

    /// `node`'s `-azul-app-region`, if it declares one.
    fn app_region(node: &Dom) -> Option<azul_css::props::style::transform::StyleAppRegion> {
        match tc::resolve(node, CssPropertyType::AppRegion, false, None)? {
            P::AppRegion(azul_css::css::CssPropertyValue::Exact(r)) => Some(r),
            _ => None,
        }
    }

    #[test]
    fn flora_cuts_its_selected_tab_and_application_button_as_australis_tabs_in_the_rule_s_metal() {
        use azul_core::dom::SvgNodeData;

        use crate::widgets::tabs::{
            CURVE_FILL_CLASS, CURVE_LEFT_CLASS, CURVE_RIGHT_CLASS, CURVE_STROKE_CLASS,
        };

        let dom = ribbon(UiTheme::Flora);
        let active = node(&dom, "__azul-native-ribbon-tab-active");
        let app = node(&dom, "__azul-native-ribbon-appbutton");
        // The fixture's selected tab is its first, right after the
        // application button: that side has no rule to ease (the button's
        // stone stands over it), so it hangs only the right run-out.
        let tab_runouts: &[(usize, &str, bool)] = &[(3, "__azul-native-tab-runout-right", false)];
        let app_runouts: &[(usize, &str, bool)] = &[
            (3, "__azul-native-tab-runout-left", true),
            (4, "__azul-native-tab-runout-right", false),
        ];
        for (what, stone, runouts) in [
            ("the selected tab", active, tab_runouts),
            ("the application button", app, app_runouts),
        ] {
            assert_eq!(
                stone.children.as_ref().len(),
                3 + runouts.len(),
                "{what}: its label, its two curves, the run-outs beside its feet"
            );
            for (i, side) in [(1, CURVE_LEFT_CLASS), (2, CURVE_RIGHT_CLASS)] {
                let curve = &stone.children.as_ref()[i];
                assert!(tc::has_class(curve, side), "{what}: child {i} is its {side}");
                assert!(
                    matches!(curve.root.get_svg_data(), Some(SvgNodeData::ViewBox { .. })),
                    "{what}: a curve is a user space of its own, one unit per px"
                );
                let fill = tc::find(curve, CURVE_FILL_CLASS).expect("the face inside the S");
                let metal = tc::find(curve, CURVE_STROKE_CLASS).expect("the S");
                assert!(matches!(fill.root.get_svg_data(), Some(SvgNodeData::Path(_))));
                assert!(matches!(metal.root.get_svg_data(), Some(SvgNodeData::Path(_))));
                assert_eq!(
                    face(metal, false, None),
                    vec![tc::flora_css_rolled_tab()],
                    "{what}: the S is cut from the rolled metal its head is cut from"
                );
            }
            for &(i, side, left) in runouts {
                let runout = &stone.children.as_ref()[i];
                assert!(tc::has_class(runout, side), "{what}: child {i} is its {side}");
                assert_eq!(face(runout, false, None), vec![tc::flora_css_runout(left)]);
            }
            assert_eq!(
                top_edge(stone),
                Some(ColorU::TRANSPARENT),
                "{what}: its head is transparent, the rolled metal under the stone shows there"
            );
            assert_eq!(
                face(stone, false, None).first(),
                Some(&tc::flora_css_rolled_tab()),
                "{what}: the metal on its border box"
            );
        }
        let unselected = tc::find_all(&dom, "__azul-native-ribbon-tab")
            .into_iter()
            .find(|t| !tc::has_class(t, "__azul-native-ribbon-tab-active"))
            .expect("an unselected tab");
        assert_eq!(unselected.children.as_ref().len(), 1, "an unselected tab is its label");
    }

    #[test]
    fn a_flat_ribbon_keeps_its_office_tabs_square() {
        let dom = ribbon(UiTheme::Flat);
        assert!(tc::find(&dom, crate::widgets::tabs::CURVE_LEFT_CLASS).is_none());
        assert!(tc::find(&dom, crate::widgets::tabs::CURVE_RIGHT_CLASS).is_none());
    }

    #[test]
    fn a_selected_tab_the_caller_styled_keeps_its_own_shape() {
        let mut r = fixture().with_theme(UiTheme::Flora);
        r.style.tab_active_style =
            OptionCssPropertyWithConditionsVec::Some(CssPropertyWithConditionsVec::from_vec(
                vec![Cond::simple(P::const_font_size(StyleFontSize::const_px(15)))],
            ));
        let dom = r.dom();
        let active = node(&dom, "__azul-native-ribbon-tab-active");
        assert_eq!(active.children.as_ref().len(), 1, "no curves on the caller's tab");
        let app = node(&dom, "__azul-native-ribbon-appbutton");
        assert_eq!(
            app.children.as_ref().len(),
            5,
            "flora's own application button keeps its curves and run-outs"
        );
    }

    #[test]
    fn a_flora_tab_is_set_in_the_ui_hand_s_capitals() {
        let dom = ribbon(UiTheme::Flora);
        for tab in tc::find_all(&dom, "__azul-native-ribbon-tab") {
            assert_eq!(
                tc::resolve(tab, CssPropertyType::FontFamily, false, None),
                Some(P::const_font_family(flora::FONT_CAPS))
            );
            assert_eq!(
                tc::resolve(tab, CssPropertyType::TextTransform, false, None),
                Some(P::TextTransform(azul_css::props::property::StyleTextTransformValue::Exact(
                    StyleTextTransform::Uppercase
                )))
            );
        }
    }

    #[test]
    fn a_tab_strip_in_the_titlebar_moves_the_window_and_its_tabs_do_not() {
        use azul_css::props::style::transform::StyleAppRegion;

        let chrome = TabsInTitlebar::create(8.0, 78.0, 0.0);
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let dom = fixture()
                .with_theme(theme)
                .with_tabs_in_titlebar(chrome)
                .dom_desktop();
            assert_eq!(
                app_region(node(&dom, "__azul-native-ribbon-tabbar")),
                Some(StyleAppRegion::Drag),
                "{theme:?}: the strip around the tabs moves the window"
            );
            for tab in tc::find_all(&dom, "__azul-native-ribbon-tab") {
                assert_eq!(
                    app_region(tab),
                    Some(StyleAppRegion::NoDrag),
                    "{theme:?}: a tab stays a tab"
                );
            }
            assert_eq!(
                app_region(node(&dom, "__azul-native-ribbon-appbutton")),
                Some(StyleAppRegion::NoDrag),
                "{theme:?}: FILE stays a button"
            );
            let before_first = tc::resolve(
                node(&dom, "__azul-native-ribbon-tabbar"),
                CssPropertyType::PaddingLeft,
                false,
                None,
            )
            .and_then(|p| match p {
                P::PaddingLeft(azul_css::css::CssPropertyValue::Exact(pad)) => {
                    Some(pad.inner.number.get())
                }
                _ => None,
            });
            assert!(
                before_first.is_some_and(|px| px >= 78.0),
                "{theme:?}: the first tab starts past the window controls ({before_first:?})"
            );
        }
        let plain = fixture().with_theme(UiTheme::Flat).dom_desktop();
        assert_eq!(
            app_region(node(&plain, "__azul-native-ribbon-tabbar")),
            None,
            "a ribbon under its own title row moves nothing"
        );
    }
}

/// Office 2010's control kinds: two-line large labels, split buttons with
/// their own arrow part, small items stacked three to a column and the list
/// gallery of Quick Steps.
#[cfg(test)]
mod office_2010_kinds_tests {
    use azul_core::dom::NodeType;

    use super::*;

    extern "C" fn noop(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    extern "C" fn pick(_: RefAny, _: CallbackInfo, _: usize) -> Update {
        Update::DoNothing
    }

    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// Every non-empty text node under `node`, depth first.
    fn texts(node: &Dom) -> Vec<String> {
        let mut out = Vec::new();
        if let NodeType::Text(t) = node.root.get_node_type() {
            if !t.as_str().is_empty() {
                out.push(t.as_str().to_string());
            }
        }
        for c in node.children.as_ref() {
            out.extend(texts(c));
        }
        out
    }

    /// Every icon name under `node`, depth first.
    fn icons(node: &Dom) -> Vec<String> {
        let mut out = Vec::new();
        if let NodeType::Icon(i) = node.root.get_node_type() {
            out.push(i.as_ref().as_str().to_string());
        }
        for c in node.children.as_ref() {
            out.extend(icons(c));
        }
        out
    }

    fn name_of(node: &Dom) -> Option<String> {
        node.root
            .get_accessibility_info()
            .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string()))
    }

    /// The items row of the only group of a one-tab flat ribbon.
    fn items_of(group: RibbonGroup) -> Dom {
        let dom = Ribbon::new(RibbonTabVec::from_vec(vec![
            RibbonTab::new(AzString::from("Home")).with_group(group),
        ]))
        .with_theme(UiTheme::Flat)
        .dom_desktop();
        let content = dom
            .children
            .as_ref()
            .iter()
            .find(|c| has_class(c, "__azul-native-ribbon-content"))
            .expect("the content band")
            .clone();
        content.children.as_ref()[0].children.as_ref()[0].clone()
    }

    fn button(icon: &str, label: &str) -> RibbonButton {
        RibbonButton::new(AzString::from(icon), AzString::from(label))
    }

    #[test]
    fn a_large_label_breaks_where_its_longer_line_is_shortest() {
        let split = |label: &str| {
            let words: Vec<&str> = label.split_whitespace().collect();
            balanced_break(&words).map(|k| (words[..k].join(" "), words[k..].join(" ")))
        };
        let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        assert_eq!(split("New E-mail"), pair("New", "E-mail"));
        assert_eq!(split("Send/Receive All Folders"), pair("Send/Receive", "All Folders"));
        assert_eq!(split("Recover Deleted Items"), pair("Recover", "Deleted Items"));
        assert_eq!(split("Mark All as Read"), pair("Mark All", "as Read"));
        assert_eq!(split("Clean Up Folder"), pair("Clean Up", "Folder"));
        // A tie keeps the longer first line (Outlook's "Run Rules / Now").
        assert_eq!(split("Run Rules Now"), pair("Run Rules", "Now"));
        assert_eq!(split("Delete"), None);
    }

    #[test]
    fn a_large_labels_one_break_is_a_plain_space_and_the_rest_no_break_ones() {
        assert_eq!(large_label_text("New E-mail", false), "New E-mail");
        assert_eq!(
            large_label_text("Send/Receive All Folders", false),
            "Send/Receive All\u{a0}Folders"
        );
        // A menu's arrow is glued to the last word; a one-word label puts it
        // on a line of its own.
        assert_eq!(large_label_text("New Items", true), "New Items\u{a0}");
        assert_eq!(large_label_text("Move", true), "Move ");
    }

    #[test]
    fn a_two_word_large_button_keeps_its_label_whole_and_named() {
        let items = items_of(
            RibbonGroup::new(AzString::from("New"))
                .with_item(RibbonItem::LargeButton(button("mail", "New E-mail"))),
        );
        let node = &items.children.as_ref()[0];
        assert!(matches!(node.root.get_node_type(), NodeType::Button));
        // One text node, the label as the user reads it: scripts find the
        // button by "New E-mail".
        assert_eq!(texts(node), vec!["New E-mail".to_string()]);
        assert_eq!(name_of(node).as_deref(), Some("New E-mail"));
        let content = &node.children.as_ref()[0];
        assert!(has_class(content, "__azul-native-ribbon-large-content"));
        assert_eq!(icons(content), vec!["mail".to_string()]);
        assert!(has_class(
            &content.children.as_ref()[1],
            "__azul-native-ribbon-large-label"
        ));
    }

    #[test]
    fn a_large_menu_button_puts_its_arrow_after_the_last_word() {
        let items = items_of(RibbonGroup::new(AzString::from("New")).with_item(
            RibbonItem::LargeButton(button("mail", "New Items").with_arrow(RibbonArrow::Menu)),
        ));
        let node = &items.children.as_ref()[0];
        assert_eq!(
            icons(node),
            vec!["mail".to_string(), "arrow_drop_down".to_string()]
        );
        assert_eq!(texts(node), vec!["New Items\u{a0}".to_string()]);
        // A one-word menu keeps the Button's own column: icon, label, arrow.
        let items = items_of(RibbonGroup::new(AzString::from("Move")).with_item(
            RibbonItem::LargeButton(
                button("drive_file_move", "Move").with_arrow(RibbonArrow::Menu),
            ),
        ));
        let ch = items.children.as_ref()[0].children.as_ref();
        assert_eq!(ch.len(), 3, "[icon, label, arrow]");
    }

    #[test]
    fn a_split_button_with_an_arrow_callback_is_two_buttons() {
        type Cb = crate::widgets::button::ButtonOnClickCallbackType;
        for large in [true, false] {
            let rb = button("content_paste", "Paste")
                .with_on_click(RefAny::new(1u8), noop as Cb)
                .with_on_arrow_click(RefAny::new(2u8), noop as Cb);
            assert_eq!(rb.arrow, RibbonArrow::Split);
            let item = if large {
                RibbonItem::LargeButton(rb)
            } else {
                RibbonItem::SmallButton(rb)
            };
            let items = items_of(RibbonGroup::new(AzString::from("Clipboard")).with_item(item));
            let split = &items.children.as_ref()[0];
            assert!(has_class(split, RIBBON_SPLIT_CLASS), "large: {large}");
            let parts = split.children.as_ref();
            assert_eq!(parts.len(), 2, "[main, arrow]");
            assert!(has_class(&parts[0], RIBBON_SPLIT_MAIN_CLASS));
            assert!(has_class(&parts[1], RIBBON_SPLIT_ARROW_CLASS));
            for part in parts {
                assert!(matches!(part.root.get_node_type(), NodeType::Button));
                assert_eq!(
                    part.root.get_callbacks().as_ref().len(),
                    1,
                    "each part its own click"
                );
            }
            assert_eq!(name_of(&parts[0]).as_deref(), Some("Paste"));
            assert_eq!(name_of(&parts[1]).as_deref(), Some("Paste options"));
            assert!(icons(&parts[1]).contains(&"arrow_drop_down".to_string()));
        }
        // Without an arrow callback a split button is still one button.
        let items = items_of(RibbonGroup::new(AzString::from("Clipboard")).with_item(
            RibbonItem::LargeButton(
                button("content_paste", "Paste").with_arrow(RibbonArrow::Split),
            ),
        ));
        assert!(matches!(
            items.children.as_ref()[0].root.get_node_type(),
            NodeType::Button
        ));
    }

    #[test]
    fn small_items_at_a_groups_top_level_stack_three_to_a_column() {
        let small = |label: &str| RibbonItem::SmallButton(button("label", label));
        let group = RibbonGroup::new(AzString::from("Delete"))
            .with_item(small("Ignore"))
            .with_item(small("Clean Up"))
            .with_item(small("Junk"))
            .with_item(small("Spam"))
            .with_item(RibbonItem::LargeButton(button("delete", "Delete")))
            .with_item(small("Alone"));
        let items = items_of(group);
        let ch = items.children.as_ref();
        assert_eq!(ch.len(), 4, "[column of 3, column of 1, large, small]");
        assert!(has_class(&ch[0], "__azul-native-ribbon-column"));
        assert_eq!(ch[0].children.as_ref().len(), 3);
        assert!(has_class(&ch[1], "__azul-native-ribbon-column"));
        assert_eq!(ch[1].children.as_ref().len(), 1);
        assert!(matches!(ch[2].root.get_node_type(), NodeType::Button));
        assert!(
            matches!(ch[3].root.get_node_type(), NodeType::Button),
            "one small item alone stays"
        );
        assert_eq!(texts(&ch[0]), vec!["Ignore", "Clean Up", "Junk"]);
    }

    /// A row at a group's top level is one control tall - RibbonX's `box` -
    /// and Office stacks it three to a column with the small items, as
    /// Explorer's Show/hide stacks its three check boxes. AzDrive's three
    /// check rows stood side by side and pushed Options out of a 1280 px
    /// window.
    #[test]
    fn a_row_at_a_groups_top_level_stacks_with_the_small_items() {
        let check_row = |label: &str| {
            RibbonItem::Row(
                RibbonRow::new()
                    .with_item(RibbonItem::Check(CheckBox::create(false)))
                    .with_item(RibbonItem::Custom(crate::widgets::widget_p_with_text(
                        AzString::from(label),
                    ))),
            )
        };
        let group = RibbonGroup::new(AzString::from("Show/hide"))
            .with_item(check_row("Item check boxes"))
            .with_item(check_row("File name extensions"))
            .with_item(check_row("Hidden items"))
            .with_item(RibbonItem::LargeButton(button(
                "visibility_off",
                "Hide selected items",
            )));
        let items = items_of(group);
        let ch = items.children.as_ref();
        assert_eq!(
            ch.len(),
            2,
            "[the three rows in a column, the large button]"
        );
        assert!(has_class(&ch[0], "__azul-native-ribbon-column"));
        let rows = ch[0].children.as_ref();
        assert_eq!(rows.len(), 3);
        assert!(
            rows.iter()
                .all(|r| has_class(r, "__azul-native-ribbon-row")),
            "each row as the app built it, one under the other"
        );
        assert_eq!(
            texts(&ch[0]),
            vec!["Item check boxes", "File name extensions", "Hidden items"]
        );
        assert!(matches!(ch[1].root.get_node_type(), NodeType::Button));
    }

    #[test]
    fn a_list_gallery_shows_its_commands_icon_beside_name_in_columns_of_three() {
        let names = [
            "Move to: ?",
            "Team E-mail",
            "Reply & Delete",
            "To Manager",
            "Done",
            "Create New",
            "Hidden",
        ];
        let cells: Vec<RibbonGalleryCell> = names
            .iter()
            .map(|n| RibbonGalleryCell::new(Dom::create_icon("label"), AzString::from(*n)))
            .collect();
        let gallery = RibbonGallery::new(RibbonGalleryCellVec::from_vec(cells))
            .with_columns(2)
            .with_on_select(RefAny::new(0u8), pick as RibbonGalleryOnSelectCallbackType);
        let items = items_of(
            RibbonGroup::new(AzString::from("Quick Steps"))
                .with_item(RibbonItem::Gallery(gallery)),
        );
        let wrapper = &items.children.as_ref()[0];
        let frame = &wrapper.children.as_ref()[0];
        let strip = &frame.children.as_ref()[0];
        assert!(has_class(strip, RIBBON_GALLERY_LIST_CLASS));
        let columns = strip.children.as_ref();
        assert_eq!(columns.len(), 2, "two columns of three; the seventh is behind More");
        assert_eq!(
            texts(&columns[0]),
            vec!["Move to: ?", "Team E-mail", "Reply & Delete"]
        );
        assert_eq!(texts(&columns[1]), vec!["To Manager", "Done", "Create New"]);
        for column in columns {
            for cell in column.children.as_ref() {
                assert!(
                    !has_class(cell, "__azul-native-ribbon-gallery-cell-selected"),
                    "a command stays unlit"
                );
                assert!(has_class(
                    &cell.children.as_ref()[0],
                    "__azul-native-ribbon-gallery-cell-icon"
                ));
                assert_eq!(cell.root.get_callbacks().as_ref().len(), 1);
            }
        }
        // More shows every command.
        let panel = &wrapper.children.as_ref()[1];
        assert_eq!(panel.children.as_ref().len(), names.len());
    }
}

/// Office's ribbon scaling: the steps a tab too wide for its ribbon takes,
/// the collapsed group's popup and the width the ribbon asks its window.
#[cfg(test)]
#[path = "ribbon_scaling_tests.rs"]
mod ribbon_scaling_tests;

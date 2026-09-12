//! Button widget with Bootstrap-inspired type-based styling (`ButtonType`).

use std::vec::Vec;

use azul_core::{
    callbacks::{CoreCallbackData, Update},
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec, NodeType, OptionDom, TabIndex},
    refany::RefAny,
    resources::{ImageRef, OptionImageRef},
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, OptionCssPropertyWithConditionsVec,
    },
    props::{
        basic::{
            color::ColorU,
            font::{StyleFontFamily, StyleFontFamilyVec},
            *,
        },
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    system::SystemFontType,
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

/// The semantic type/role of a button.
///
/// Each type has distinct styling to indicate its purpose to the user.
/// Colors are based on Bootstrap's button variants for familiarity.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ButtonType {
    /// Default button style - neutral/gray appearance
    #[default]
    Default,
    /// Primary action button - blue, uses system accent color on macOS
    Primary,
    /// Secondary button - gray, less prominent than primary
    Secondary,
    /// Success/confirmation button - green with white text
    Success,
    /// Danger/destructive button - red with white text
    Danger,
    /// Warning button - yellow with BLACK text
    Warning,
    /// Informational button - teal/cyan with white text
    Info,
    /// Link-style button - appears as a hyperlink, no background
    Link,
}

/// What a button type's face IS, which decides what a theme may do to it
/// in dark mode. The migration's rule: a page-neutral surface takes the
/// theme's dark tokens; a surface that is its own colour keeps it in both
/// modes; a light value never moves.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ButtonSurface {
    /// The standard command: light grey paper, dark text. Page-neutral —
    /// the theme paints it in its dark face and ink.
    Neutral,
    /// A coloured command (primary, success, danger, ...): the colour is the
    /// meaning. The theme leaves face, text and border alone.
    OwnColour,
    /// The link button: no face at all, only text.
    NoSurface,
}

impl ButtonType {
    /// See [`ButtonSurface`].
    #[must_use]
    pub const fn surface(self) -> ButtonSurface {
        match self {
            Self::Default => ButtonSurface::Neutral,
            Self::Link => ButtonSurface::NoSurface,
            Self::Primary
            | Self::Secondary
            | Self::Success
            | Self::Danger
            | Self::Warning
            | Self::Info => ButtonSurface::OwnColour,
        }
    }

    /// Get the CSS class name for this button type
    #[must_use]
    pub const fn class_name(&self) -> &'static str {
        match self {
            Self::Default => "__azul-btn-default",
            Self::Primary => "__azul-btn-primary",
            Self::Secondary => "__azul-btn-secondary",
            Self::Success => "__azul-btn-success",
            Self::Danger => "__azul-btn-danger",
            Self::Warning => "__azul-btn-warning",
            Self::Info => "__azul-btn-info",
            Self::Link => "__azul-btn-link",
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub struct Button {
    /// Content (image or text) of this button, centered by default
    pub label: AzString,
    /// Optional image that is displayed next to the label
    pub image: OptionImageRef,
    /// Optional leading icon name, resolved through the registered icon
    /// provider (e.g. the builtin Material Icons pack: "`content_copy`").
    /// An empty string means "no icon".
    pub icon: AzString,
    /// A ready-made DOM to use AS the leading icon, instead of resolving
    /// [`icon`](Self::icon) into one.
    ///
    /// The escape hatch for an icon that is not a fixed picture: a control
    /// whose glyph depends on live state (the window control that draws
    /// "maximize" or "restore") wraps itself in a `VirtualView` so it can
    /// re-render in place, and hands the view here. `icon` is still read for
    /// the ACCESSIBLE NAME, so set both: the name is what a screen reader
    /// announces and a view cannot supply it.
    pub icon_dom: OptionDom,
    /// Optional trailing icon name (e.g. "`arrow_drop_down`" for menu/split
    /// buttons). An empty string means "no trailing icon".
    pub trailing_icon: AzString,
    /// The semantic type of this button (Primary, Success, Danger, etc.)
    pub button_type: ButtonType,
    /// Style for this button container, or `None` for "no opinion" — in which
    /// case the style is derived from `button_type` at render time.
    ///
    /// `None` and `Some(empty)` are different answers throughout this struct:
    /// the first means the widget picks, the second means the caller asked for
    /// no properties at all and gets none.
    pub container_style: OptionCssPropertyWithConditionsVec,
    /// Style of the label, or `None` for the widget's default.
    pub label_style: OptionCssPropertyWithConditionsVec,
    /// Style of the image, or `None` for the widget's default.
    pub image_style: OptionCssPropertyWithConditionsVec,
    /// Style of the leading icon, or `None` for the widget's default.
    pub icon_style: OptionCssPropertyWithConditionsVec,
    /// Style of the trailing icon, or `None` for the widget's default.
    pub trailing_icon_style: OptionCssPropertyWithConditionsVec,
    /// Optional: Function to call when the button is clicked
    pub on_click: OptionButtonOnClick,
    pub theme: crate::widgets::themes::OptionUiTheme,
}

pub type ButtonOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo) -> Update;
impl_widget_callback!(
    ButtonOnClick,
    OptionButtonOnClick,
    ButtonOnClickCallback,
    ButtonOnClickCallbackType
);

// Host-invoker plumbing for managed-FFI bindings — see core/src/host_invoker.rs.
azul_core::impl_managed_callback! {
    wrapper:        ButtonOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: BUTTON_ON_CLICK_INVOKER,
    invoker_ty:     AzButtonOnClickCallbackInvoker,
    thunk_fn:       az_button_on_click_callback_thunk,
    setter_fn:      AzApp_setButtonOnClickCallbackInvoker,
    from_handle_fn: AzButtonOnClickCallback_createFromHostHandle,
}

// ButtonType-specific styling
// ============================================================

/// Get the background color for a button type
pub(crate) const fn get_button_colors(button_type: ButtonType) -> (ColorU, ColorU, ColorU) {
    // Returns (normal, hover, active) colors
    match button_type {
        ButtonType::Default => (
            ColorU::rgb(248, 249, 250), // Light gray
            ColorU::rgb(233, 236, 239), // Darker gray on hover
            ColorU::rgb(218, 222, 226), // Even darker on active
        ),
        ButtonType::Primary => (
            ColorU::bootstrap_primary(),
            ColorU::bootstrap_primary_hover(),
            ColorU::bootstrap_primary_active(),
        ),
        ButtonType::Secondary => (
            ColorU::bootstrap_secondary(),
            ColorU::bootstrap_secondary_hover(),
            ColorU::bootstrap_secondary_active(),
        ),
        ButtonType::Success => (
            ColorU::bootstrap_success(),
            ColorU::bootstrap_success_hover(),
            ColorU::bootstrap_success_active(),
        ),
        ButtonType::Danger => (
            ColorU::bootstrap_danger(),
            ColorU::bootstrap_danger_hover(),
            ColorU::bootstrap_danger_active(),
        ),
        ButtonType::Warning => (
            ColorU::bootstrap_warning(),
            ColorU::bootstrap_warning_hover(),
            ColorU::bootstrap_warning_active(),
        ),
        ButtonType::Info => (
            ColorU::bootstrap_info(),
            ColorU::bootstrap_info_hover(),
            ColorU::bootstrap_info_active(),
        ),
        ButtonType::Link => (
            ColorU::TRANSPARENT,
            ColorU::TRANSPARENT,
            ColorU::TRANSPARENT,
        ),
    }
}

/// Get the text color for a button type
const fn get_button_text_color(button_type: ButtonType) -> ColorU {
    match button_type {
        ButtonType::Default => ColorU::rgb(33, 37, 41), // Dark text
        ButtonType::Warning => ColorU::BLACK,           // Black text on yellow
        ButtonType::Link => ColorU::bootstrap_link(),   // Blue link color
        _ => ColorU::WHITE,                             // White text on colored buttons
    }
}

/// Build container style properties for a button type
fn build_button_container_style(button_type: ButtonType) -> Vec<CssPropertyWithConditions> {
    // The 2026-06-02 BISECTION PROBE used to `return` a minimal 3-property style
    // HERE, unconditionally — which made every branch below dead code, so all 8
    // button types rendered identically (no background, no border, no hover /
    // active / focus). It was a web-lift experiment ("Remove this `return` to
    // restore the real button styling") that shipped to every target and was
    // never reverted; Default / Primary / Danger were visually indistinguishable.
    //
    // The probe's two findings still hold for the WEB LIFT only: the container's
    // complex props tripped a cascade OOB, and `display: inline-flex` hits the
    // (deferred) taffy flex-algorithm lift gap, so block layout is the
    // known-good shape there. Keep that minimal style — but behind the
    // `web_lift` feature that actually needs it, instead of on every platform.
    if cfg!(feature = "web_lift") {
        return alloc::vec![
            CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
            CssPropertyWithConditions::simple(CssProperty::const_padding_top(
                LayoutPaddingTop::const_px(6)
            )),
            CssPropertyWithConditions::simple(CssProperty::const_padding_bottom(
                LayoutPaddingBottom::const_px(6)
            )),
        ];
    }

    // Only the RESTING colour is needed here now: the hover and pressed values
    // from this triple, and the focus accent, are read by the theme modules.
    let (bg_normal, _, _) = get_button_colors(button_type);
    let text_color = get_button_text_color(button_type);

    let mut props = Vec::with_capacity(40);

    // Basic layout - use InlineFlex so flex properties (justify-content, align-items) work
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_display(LayoutDisplay::InlineFlex),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_flex_direction(LayoutFlexDirection::Row),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_justify_content(LayoutJustifyContent::Center),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_align_items(LayoutAlignItems::Center),
    ));
    // Prevent stretching when inside a flex column container
    props.push(CssPropertyWithConditions::simple(CssProperty::align_self(
        LayoutAlignSelf::Start,
    )));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_cursor(StyleCursor::Pointer),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0)),
    ));

    // Text color
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_text_color(StyleTextColor { inner: text_color }),
    ));

    // Padding (Bootstrap-like)
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_padding_top(LayoutPaddingTop::const_px(6)),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_padding_bottom(LayoutPaddingBottom::const_px(6)),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_padding_left(LayoutPaddingLeft::const_px(12)),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_padding_right(LayoutPaddingRight::const_px(12)),
    ));

    // Border radius
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_border_top_left_radius(StyleBorderTopLeftRadius::const_px(4)),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_border_top_right_radius(StyleBorderTopRightRadius::const_px(4)),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_border_bottom_left_radius(StyleBorderBottomLeftRadius::const_px(4)),
    ));
    props.push(CssPropertyWithConditions::simple(
        CssProperty::const_border_bottom_right_radius(StyleBorderBottomRightRadius::const_px(4)),
    ));

    if button_type == ButtonType::Link {
        // Link buttons have no background or border
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_background_content(StyleBackgroundContentVec::from_const_slice(&[
                StyleBackgroundContent::Color(ColorU::TRANSPARENT),
            ])),
        ));

        // The underline-on-hover lives in the theme modules with the rest of
        // this button's states — see `flat::button_states`.
    } else {
        // Normal background
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
                StyleBackgroundContent::Color(bg_normal),
            ])),
        ));

        // Border (subtle for Default, transparent for others to maintain size)
        let border_color = if button_type == ButtonType::Default {
            ColorU::rgb(206, 212, 218)
        } else {
            bg_normal
        };
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_top_width(LayoutBorderTopWidth::const_px(1)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_bottom_width(LayoutBorderBottomWidth::const_px(1)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_left_width(LayoutBorderLeftWidth::const_px(1)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_right_width(LayoutBorderRightWidth::const_px(1)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_top_style(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            }),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_bottom_style(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            }),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_left_style(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            }),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_right_style(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            }),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_top_color(StyleBorderTopColor {
                inner: border_color,
            }),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_bottom_color(StyleBorderBottomColor {
                inner: border_color,
            }),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_left_color(StyleBorderLeftColor {
                inner: border_color,
            }),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_border_right_color(StyleBorderRightColor {
                inner: border_color,
            }),
        ));

        // Hover, pressed and focus are NOT here. They live in the theme
        // modules (`flat::button_states`, `flora::button_states`) and are
        // appended by `flat::button` / `flora::button`, because the dark half of
        // each pair needs a palette this file cannot see. Declared here they
        // could only ever name the light-mode colour, which is how a hovered
        // button came to keep its light fill on a dark surface.
    }

    props
}

/// Default style for the optional leading/trailing icon nodes: a fixed-size,
/// non-selectable glyph that does not flex.
static BUTTON_ICON_DEFAULT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(16))),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
];

/// Build label style properties
fn build_button_label_style() -> Vec<CssPropertyWithConditions> {
    // Use system UI font
    let font_family =
        StyleFontFamilyVec::from_vec(vec![StyleFontFamily::SystemType(SystemFontType::Ui)]);

    vec![
        CssPropertyWithConditions::simple(CssProperty::const_font_size(StyleFontSize::const_px(
            14,
        ))),
        CssPropertyWithConditions::simple(CssProperty::const_text_align(StyleTextAlign::Center)),
        CssPropertyWithConditions::simple(CssProperty::const_font_family(font_family)),
        CssPropertyWithConditions::simple(CssProperty::user_select(StyleUserSelect::None)),
    ]
}

impl Button {
    /// Create a button with `ButtonType::Default` styling.
    #[inline]
    #[must_use]
    pub fn create(label: AzString) -> Self {
        Self::with_type(label, ButtonType::Default)
    }

    /// Create a button with a specific type (Primary, Success, Danger, etc.)
    #[inline]
    #[must_use]
    pub fn with_type(label: AzString, button_type: ButtonType) -> Self {
        Self {
            label,
            image: None.into(),
            icon: AzString::from_const_str(""),
            icon_dom: None.into(),
            trailing_icon: AzString::from_const_str(""),
            button_type,
            on_click: None.into(),
            container_style: OptionCssPropertyWithConditionsVec::None,
            label_style: OptionCssPropertyWithConditionsVec::None,
            image_style: OptionCssPropertyWithConditionsVec::None,
            theme: crate::widgets::themes::OptionUiTheme::Some(
                crate::widgets::themes::UiTheme::Flat,
            ),
            icon_style: OptionCssPropertyWithConditionsVec::None,
            trailing_icon_style: OptionCssPropertyWithConditionsVec::None,
        }
    }

    /// The container CSS this button renders with.
    ///
    /// `None` means no opinion, so `button_type` decides — the same answer both
    /// themes give, asked in one place so they cannot drift. It is also why
    /// `set_button_type` is a plain field write: the type and the colours derived
    /// from it were two facts stored apart, and a caller writing the public
    /// `button_type` field left a Primary button painted like a Default one.
    #[must_use]
    pub fn resolved_container_style(&self) -> CssPropertyWithConditionsVec {
        self.container_style
            .clone()
            .into_option()
            .unwrap_or_else(|| {
                CssPropertyWithConditionsVec::from_vec(build_button_container_style(
                    self.button_type,
                ))
            })
    }

    /// The label CSS this button renders with.
    #[must_use]
    pub fn resolved_label_style(&self) -> CssPropertyWithConditionsVec {
        self.label_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_vec(build_button_label_style()))
    }

    /// The image CSS this button renders with.
    ///
    /// Defaults to the label's style: an image sits where the label would and
    /// inherits the same box.
    #[must_use]
    pub fn resolved_image_style(&self) -> CssPropertyWithConditionsVec {
        self.image_style
            .clone()
            .into_option()
            .unwrap_or_else(|| CssPropertyWithConditionsVec::from_vec(build_button_label_style()))
    }

    /// The leading-icon CSS this button renders with.
    #[must_use]
    pub fn resolved_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.icon_style.clone().into_option().unwrap_or_else(|| {
            CssPropertyWithConditionsVec::from_const_slice(BUTTON_ICON_DEFAULT_STYLE)
        })
    }

    /// The trailing-icon CSS this button renders with.
    #[must_use]
    pub fn resolved_trailing_icon_style(&self) -> CssPropertyWithConditionsVec {
        self.trailing_icon_style
            .clone()
            .into_option()
            .unwrap_or_else(|| {
                CssPropertyWithConditionsVec::from_const_slice(BUTTON_ICON_DEFAULT_STYLE)
            })
    }

    /// Set the button type.
    ///
    /// Does not touch `container_style`: the type-specific colours are resolved
    /// from `button_type` when the DOM is built.
    #[inline]
    pub const fn set_button_type(&mut self, button_type: ButtonType) {
        self.button_type = button_type;
    }

    /// Builder method to set the button type
    #[inline]
    #[must_use]
    pub const fn with_button_type(mut self, button_type: ButtonType) -> Self {
        self.set_button_type(button_type);
        self
    }

    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut m = Self::create(AzString::from_const_str(""));
        core::mem::swap(&mut m, self);
        m
    }

    /// Pick the widget theme. Unset (`None`), the widget renders in the
    /// default theme (`crate::widgets::themes::UiTheme::default()`).
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    #[inline]
    pub fn set_image(&mut self, image: ImageRef) {
        self.image = Some(image).into();
    }

    /// Sets the leading icon name (empty string clears it).
    #[inline]
    pub fn set_icon(&mut self, icon: AzString) {
        self.icon = icon;
    }

    /// Builder method to set the leading icon name.
    #[inline]
    #[must_use]
    pub fn with_icon(mut self, icon: AzString) -> Self {
        self.set_icon(icon);
        self
    }

    /// Sets the trailing icon name (empty string clears it).
    #[inline]
    pub fn set_trailing_icon(&mut self, icon: AzString) {
        self.trailing_icon = icon;
    }

    /// Builder method to set the trailing icon name.
    #[inline]
    #[must_use]
    pub fn with_trailing_icon(mut self, icon: AzString) -> Self {
        self.set_trailing_icon(icon);
        self
    }

    #[inline]
    pub fn set_on_click<C: Into<ButtonOnClickCallback>>(&mut self, data: RefAny, on_click: C) {
        self.on_click = Some(ButtonOnClick {
            refany: data,
            callback: on_click.into(),
        })
        .into();
    }

    #[inline]
    #[must_use]
    pub fn with_on_click<C: Into<ButtonOnClickCallback>>(
        mut self,
        data: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(data, on_click);
        self
    }

    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        // Rendering lives in the theme modules, where the palette is in scope:
        // the dark-mode colours, the interactive states and (for flora) the
        // raised face are all appended there. This used to build its own copy of
        // the tree and never reach `flat::button` / `flora::button`, which left
        // both functions dead and the product's buttons without any of that.
        //
        // `UiTheme::default()` is Flat, and so is every other widget's fallback.
        match self.theme.into_option() {
            Some(crate::widgets::themes::UiTheme::Flora) => {
                crate::widgets::themes::flora::button(self)
            }
            Some(crate::widgets::themes::UiTheme::Flat) | None => {
                crate::widgets::themes::flat::button(self)
            }
        }
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::collections::HashSet;

    use azul_core::{
        dom::{EventFilter, HoverEventFilter},
        resources::RawImageFormat,
    };
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};

    use super::*;
    use crate::widgets::theme_probe;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Every variant of `ButtonType` - the complete input domain of `class_name`,
    /// `get_button_colors`, `get_button_text_color` and `build_button_container_style`.
    const ALL_TYPES: [ButtonType; 8] = [
        ButtonType::Default,
        ButtonType::Primary,
        ButtonType::Secondary,
        ButtonType::Success,
        ButtonType::Danger,
        ButtonType::Warning,
        ButtonType::Info,
        ButtonType::Link,
    ];

    const WHITE: ColorU = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    const BLACK: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    const TRANSPARENT: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    /// The "dark text" of the Default button (Bootstrap `$gray-900`).
    const DARK: ColorU = ColorU {
        r: 33,
        g: 37,
        b: 41,
        a: 255,
    };

    extern "C" fn test_click(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    extern "C" fn other_click(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::RefreshDom
    }

    fn btn(label: &str, button_type: ButtonType) -> Button {
        Button::with_type(AzString::from(label), button_type)
    }

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length - an
    /// `em`/`%` slipping into the button geometry would resolve against the parent
    /// font/box instead of the intended fixed padding or font size.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "button geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    fn padding_top_bottom_px(v: &CssPropertyWithConditionsVec) -> (Option<f32>, Option<f32>) {
        let find = |f: &dyn Fn(&CssProperty) -> Option<f32>| {
            v.as_ref().iter().find_map(|p| f(&p.property))
        };
        (
            find(&|p| match p {
                CssProperty::PaddingTop(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
            find(&|p| match p {
                CssProperty::PaddingBottom(x) => x.get_property().map(|x| px(&x.inner)),
                _ => None,
            }),
        )
    }

    fn font_size_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FontSize(f) => f.get_property().map(|f| px(&f.inner)),
            _ => None,
        })
    }

    /// The CSS classes of a rendered node, in declaration order.
    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The label text behind the block wrapper: `<p>` wrapping one text node.
    fn p_label_of(dom: &Dom) -> Option<&str> {
        if !matches!(dom.root.get_node_type(), NodeType::P) {
            return None;
        }
        let inner = dom.children.as_ref();
        if inner.len() != 1 {
            return None;
        }
        text_of(&inner[0])
    }

    fn text_of(dom: &Dom) -> Option<&str> {
        match dom.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    /// The recursive descendant count - `Dom::estimated_total_children` is a *cached*
    /// value that, if too small, makes `convert_dom_into_compact_dom` under-allocate
    /// its arenas and panic on out-of-bounds writes.
    fn count_descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + count_descendants(c))
            .sum()
    }

    /// Perceived brightness (0..=255) of an sRGB colour, Rec.709 weights. Kept to
    /// plain `+`/`*` (no gamma expansion) so the readability assertions stay exact
    /// and toolchain-independent.
    fn luma(c: ColorU) -> f32 {
        0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b)
    }

    /// Adversarial button labels: empty, whitespace, combining marks, ZWJ emoji, RTL,
    /// embedded NULs (`AzString` is length-based, so a NUL must not truncate), bidi
    /// overrides and a string far longer than any plausible button label.
    fn adversarial_labels() -> Vec<String> {
        let mut v: Vec<String> = [
            "",
            " ",
            "OK",
            "e\u{0301}",                                   // e + combining acute
            "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}", // ZWJ family emoji
            "\u{5E9}\u{5DC}\u{5D5}\u{5DD}",                // RTL Hebrew
            "\0",                                          // a single NUL
            "a\0b",                                        // embedded NUL
            "\u{FFFD}\u{202E}\u{200B}",                    // replacement char, RTL override, ZWSP
            "…\t\r\n",                                     // control chars in a label
            "__azul-btn-primary",                          // a label that looks like a class name
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
        v.push("x".repeat(100_000));
        v
    }

    // ------------------------------------------------------------------
    // ButtonType::class_name  (getter)
    // ------------------------------------------------------------------

    #[test]
    fn class_name_returns_the_documented_class_for_every_type() {
        let expected = [
            (ButtonType::Default, "__azul-btn-default"),
            (ButtonType::Primary, "__azul-btn-primary"),
            (ButtonType::Secondary, "__azul-btn-secondary"),
            (ButtonType::Success, "__azul-btn-success"),
            (ButtonType::Danger, "__azul-btn-danger"),
            (ButtonType::Warning, "__azul-btn-warning"),
            (ButtonType::Info, "__azul-btn-info"),
            (ButtonType::Link, "__azul-btn-link"),
        ];
        for (ty, class) in expected {
            assert_eq!(ty.class_name(), class, "{ty:?}: wrong CSS class");
        }
        assert_eq!(
            expected.len(),
            ALL_TYPES.len(),
            "a ButtonType variant is missing from this table"
        );
    }

    #[test]
    fn class_name_is_unique_per_type() {
        // Two types sharing a class make the semantic variant unstylable: the
        // stylesheet could not tell a Danger button from a Success one.
        let mut seen = HashSet::new();
        for ty in ALL_TYPES {
            assert!(
                seen.insert(ty.class_name()),
                "{ty:?}: duplicate class name {}",
                ty.class_name()
            );
        }
        assert_eq!(seen.len(), ALL_TYPES.len());
    }

    #[test]
    fn class_name_is_a_well_formed_css_identifier() {
        // A space, quote or `.` would silently split/escape into a *different*
        // selector once written into a stylesheet.
        for ty in ALL_TYPES {
            let c = ty.class_name();
            assert!(!c.is_empty(), "{ty:?}: empty class name");
            assert!(
                c.starts_with("__azul-btn-"),
                "{ty:?}: class {c} lost the widget prefix"
            );
            assert!(c.is_ascii(), "{ty:?}: non-ASCII class name {c}");
            assert!(
                c.chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_'),
                "{ty:?}: class {c} contains a character that needs CSS escaping",
            );
        }
    }

    #[test]
    fn class_name_is_pure_and_const_evaluable() {
        // Declared `const fn` returning `&'static str`: the same variant must yield
        // the *same* static, call after call (no per-call allocation).
        const DEFAULT_CLASS: &str = ButtonType::Default.class_name();
        const LINK_CLASS: &str = ButtonType::Link.class_name();
        assert_eq!(DEFAULT_CLASS, "__azul-btn-default");
        assert_eq!(LINK_CLASS, "__azul-btn-link");

        for ty in ALL_TYPES {
            let a = ty.class_name();
            let b = ty.class_name();
            assert_eq!(
                a.as_ptr(),
                b.as_ptr(),
                "{ty:?}: class_name is not returning a stable static"
            );
        }
    }

    #[test]
    fn class_name_of_the_default_type_matches_derived_default() {
        // `#[default] Default` — a reordering of the enum that moves `#[default]`
        // would silently restyle every `Button::create`.
        assert_eq!(ButtonType::default(), ButtonType::Default);
        assert_eq!(ButtonType::default().class_name(), "__azul-btn-default");
    }

    // ------------------------------------------------------------------
    // get_button_colors  (private)
    // ------------------------------------------------------------------

    #[test]
    fn get_button_colors_returns_the_documented_bootstrap_triples() {
        let expected = [
            (
                ButtonType::Default,
                ColorU::rgb(248, 249, 250),
                ColorU::rgb(233, 236, 239),
                ColorU::rgb(218, 222, 226),
            ),
            (
                ButtonType::Primary,
                ColorU::rgb(13, 110, 253),
                ColorU::rgb(11, 94, 215),
                ColorU::rgb(10, 88, 202),
            ),
            (
                ButtonType::Secondary,
                ColorU::rgb(108, 117, 125),
                ColorU::rgb(92, 99, 106),
                ColorU::rgb(86, 94, 100),
            ),
            (
                ButtonType::Success,
                ColorU::rgb(25, 135, 84),
                ColorU::rgb(21, 115, 71),
                ColorU::rgb(20, 108, 67),
            ),
            (
                ButtonType::Danger,
                ColorU::rgb(220, 53, 69),
                ColorU::rgb(187, 45, 59),
                ColorU::rgb(176, 42, 55),
            ),
            (
                ButtonType::Warning,
                ColorU::rgb(255, 193, 7),
                ColorU::rgb(255, 202, 44),
                ColorU::rgb(255, 205, 57),
            ),
            (
                ButtonType::Info,
                ColorU::rgb(13, 202, 240),
                ColorU::rgb(49, 210, 242),
                ColorU::rgb(61, 213, 243),
            ),
            (ButtonType::Link, TRANSPARENT, TRANSPARENT, TRANSPARENT),
        ];
        for (ty, normal, hover, active) in expected {
            assert_eq!(
                get_button_colors(ty),
                (normal, hover, active),
                "{ty:?}: wrong colour triple"
            );
        }
    }

    #[test]
    fn get_button_colors_is_pure() {
        for ty in ALL_TYPES {
            assert_eq!(
                get_button_colors(ty),
                get_button_colors(ty),
                "{ty:?}: colours are not deterministic"
            );
        }
    }

    #[test]
    fn get_button_colors_keeps_link_fully_transparent_and_every_other_type_opaque() {
        // A Link button that paints *any* background stops looking like a hyperlink;
        // a translucent solid button lets the page bleed through and destroys the
        // contrast the type was chosen for.
        for ty in ALL_TYPES {
            let (normal, hover, active) = get_button_colors(ty);
            if ty == ButtonType::Link {
                assert_eq!(
                    (normal, hover, active),
                    (TRANSPARENT, TRANSPARENT, TRANSPARENT),
                    "Link must paint nothing"
                );
            } else {
                for (state, c) in [("normal", normal), ("hover", hover), ("active", active)] {
                    assert_eq!(c.a, 255, "{ty:?}: {state} background {c:?} is not opaque");
                }
            }
        }
    }

    #[test]
    fn get_button_colors_gives_every_state_a_visible_delta() {
        // hover == normal means the button gives no feedback on mouse-over;
        // active == hover means the press is invisible.
        for ty in ALL_TYPES {
            if ty == ButtonType::Link {
                continue; // deliberately identical: a link has no background at all
            }
            let (normal, hover, active) = get_button_colors(ty);
            assert_ne!(
                normal, hover,
                "{ty:?}: hover state is indistinguishable from normal"
            );
            assert_ne!(
                hover, active,
                "{ty:?}: active state is indistinguishable from hover"
            );
            assert_ne!(
                normal, active,
                "{ty:?}: active state is indistinguishable from normal"
            );
        }
    }

    #[test]
    fn get_button_colors_gives_every_type_a_distinguishable_background() {
        // Two types that render identically make the semantic variant useless.
        let mut seen = HashSet::new();
        for ty in ALL_TYPES {
            let (normal, _, _) = get_button_colors(ty);
            assert!(
                seen.insert((normal.r, normal.g, normal.b, normal.a)),
                "{ty:?}: duplicate background {normal:?}"
            );
        }
        assert_eq!(seen.len(), ALL_TYPES.len());
    }

    // ------------------------------------------------------------------
    // get_button_text_color  (private)
    // ------------------------------------------------------------------

    #[test]
    fn get_button_text_color_returns_the_documented_colour_for_every_type() {
        let expected = [
            (ButtonType::Default, DARK),
            (ButtonType::Primary, WHITE),
            (ButtonType::Secondary, WHITE),
            (ButtonType::Success, WHITE),
            (ButtonType::Danger, WHITE),
            (ButtonType::Warning, BLACK), // doc: "Warning button - yellow with BLACK text"
            (ButtonType::Info, WHITE),
            (ButtonType::Link, ColorU::rgb(13, 110, 253)),
        ];
        for (ty, text) in expected {
            assert_eq!(get_button_text_color(ty), text, "{ty:?}: wrong text colour");
        }
        // The `_ => WHITE` catch-all is easy to widen by accident: only these three
        // variants may deviate from white.
        for ty in ALL_TYPES {
            let is_special = matches!(
                ty,
                ButtonType::Default | ButtonType::Warning | ButtonType::Link
            );
            assert_eq!(
                get_button_text_color(ty) != WHITE,
                is_special,
                "{ty:?}: text colour contradicts the documented variant"
            );
        }
    }

    #[test]
    fn get_button_text_color_is_pure_and_opaque() {
        for ty in ALL_TYPES {
            let c = get_button_text_color(ty);
            assert_eq!(
                c,
                get_button_text_color(ty),
                "{ty:?}: text colour is not deterministic"
            );
            assert_eq!(
                c.a, 255,
                "{ty:?}: invisible (translucent) text colour {c:?}"
            );
        }
    }

    #[test]
    fn get_button_text_color_stays_readable_on_its_own_background() {
        // The one real invariant of the pair: label must be legible on the fill.
        // NOTE: `Info` (white on #0dcaf0) is by far the weakest pairing at ~90 luma
        // of separation — Bootstrap and azul's own `Badge` widget both put *dark*
        // text on Info. The bound below is the current floor, not an endorsement;
        // moving Info to dark text raises its separation to ~128 and still passes.
        for ty in ALL_TYPES {
            if ty == ButtonType::Link {
                continue; // no fill: a link is drawn on the page background
            }
            let (bg, _, _) = get_button_colors(ty);
            let text = get_button_text_color(ty);
            let separation = (luma(bg) - luma(text)).abs();
            assert!(
                separation >= 85.0,
                "{ty:?}: text {text:?} on {bg:?} is unreadable (luma separation {separation:.1})"
            );

            // ... and the *more* readable of the two candidates was chosen.
            let alt = if text == WHITE { DARK } else { WHITE };
            let alt_separation = (luma(bg) - luma(alt)).abs();
            if ty != ButtonType::Info {
                assert!(
                    separation >= alt_separation,
                    "{ty:?}: {alt:?} would be more readable than {text:?} on {bg:?}"
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // build_button_container_style  (private)
    // ------------------------------------------------------------------

    #[test]
    fn build_button_container_style_never_panics_and_is_pure() {
        for ty in ALL_TYPES {
            let a = build_button_container_style(ty);
            let b = build_button_container_style(ty);
            assert!(
                !a.is_empty(),
                "{ty:?}: a button with no container style is invisible"
            );
            assert_eq!(a, b, "{ty:?}: container style is not deterministic");
        }
    }

    #[test]
    fn build_button_container_style_always_declares_display_and_symmetric_vertical_padding() {
        // Without a `display` the button falls back to the UA default; asymmetric
        // vertical padding makes the label sit off-centre.
        for ty in ALL_TYPES {
            let v = CssPropertyWithConditionsVec::from_vec(build_button_container_style(ty));
            assert!(
                v.as_ref()
                    .iter()
                    .any(|p| matches!(p.property, CssProperty::Display(_))),
                "{ty:?}: container declares no `display`",
            );
            let (top, bottom) = padding_top_bottom_px(&v);
            assert_eq!(top, Some(6.0), "{ty:?}: wrong padding-top");
            assert_eq!(bottom, Some(6.0), "{ty:?}: wrong padding-bottom");
            assert_eq!(
                top, bottom,
                "{ty:?}: vertical padding is asymmetric — the label will not be centred"
            );
        }
    }

    /// Every button type must be VISUALLY DISTINGUISHABLE.
    ///
    /// Replaces the 2026-06-02 characterisation test that pinned the opposite:
    /// a bisection probe `return`ed a 3-property minimal style before any of the
    /// type-dependent styling ran, so Default / Primary / Danger rendered
    /// identically on every platform for months. The probe now applies only
    /// under the `web_lift` feature that needed it (see
    /// `build_button_container_style`), and this test is the guard that it never
    /// escapes again.
    #[test]
    fn build_button_container_style_gives_each_type_its_own_look() {
        // The probe shape is what web_lift deliberately keeps; there the
        // distinction genuinely does not exist, so assert the native contract.
        if cfg!(feature = "web_lift") {
            return;
        }

        fn background_of(v: &CssPropertyWithConditionsVec) -> Option<StyleBackgroundContentVec> {
            v.as_ref().iter().find_map(|p| match &p.property {
                CssProperty::BackgroundContent(b) if p.apply_if.as_ref().is_empty() => {
                    b.get_property().cloned()
                }
                _ => None,
            })
        }

        // A background and a text colour exist at all — the probe emitted neither.
        for ty in ALL_TYPES {
            let v = CssPropertyWithConditionsVec::from_vec(build_button_container_style(ty));
            assert!(
                v.as_ref()
                    .iter()
                    .any(|p| matches!(p.property, CssProperty::TextColor(_))),
                "{ty:?}: no text colour — is the bisection probe back?",
            );
            // The conditionals are no longer HERE: hover, pressed and focus moved
            // to the theme modules, which is where the dark half of each pair can
            // be written. What this half of the guard still checks is that the
            // base style did not quietly acquire some other conditional rule.
            assert!(
                v.as_ref().iter().all(|p| p.apply_if.as_ref().is_empty()),
                "{ty:?}: the base style declares a conditional property; states belong in the \
                 theme modules",
            );
            // ...and the guard's real subject — that SOMETHING reacts to the
            // pointer — is asked of the rendered button, where the theme has
            // appended its states.
            let rendered = Button::with_type(AzString::from_const_str("x"), ty).dom();
            assert!(
                rendered
                    .root
                    .style
                    .iter_inline_properties()
                    .any(|(_, conds)| !conds.as_ref().is_empty()),
                "{ty:?}: the rendered button has no conditional properties at all, so it gives no \
                 feedback on hover, press or focus",
            );
        }

        // The three the user sees side by side must not collide.
        let default_bg = background_of(&CssPropertyWithConditionsVec::from_vec(
            build_button_container_style(ButtonType::Default),
        ));
        let primary_bg = background_of(&CssPropertyWithConditionsVec::from_vec(
            build_button_container_style(ButtonType::Primary),
        ));
        let danger_bg = background_of(&CssPropertyWithConditionsVec::from_vec(
            build_button_container_style(ButtonType::Danger),
        ));
        assert!(default_bg.is_some(), "Default has no background");
        assert_ne!(default_bg, primary_bg, "Default and Primary look identical");
        assert_ne!(default_bg, danger_bg, "Default and Danger look identical");
        assert_ne!(primary_bg, danger_bg, "Primary and Danger look identical");
    }

    // ------------------------------------------------------------------
    // build_button_label_style  (private)
    // ------------------------------------------------------------------

    #[test]
    fn build_button_label_style_declares_the_four_documented_properties_unconditionally() {
        let v = CssPropertyWithConditionsVec::from_vec(build_button_label_style());
        assert_eq!(
            v.len(),
            4,
            "label style gained/lost a property: {:?}",
            properties(&v)
        );

        assert_eq!(font_size_px(&v), Some(14.0), "wrong label font size");

        let align = v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::TextAlign(t) => t.get_property().copied(),
            _ => None,
        });
        assert_eq!(
            align,
            Some(StyleTextAlign::Center),
            "a button label must be centred"
        );

        let family = v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FontFamily(f) => f.get_property().cloned(),
            _ => None,
        });
        let family = family.expect("label style declares no font-family");
        assert_eq!(
            family.as_ref(),
            [StyleFontFamily::SystemType(SystemFontType::Ui)].as_slice(),
            "the label must use the system UI font",
        );

        let user_select = v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::UserSelect(u) => u.get_property().copied(),
            _ => None,
        });
        assert_eq!(
            user_select,
            Some(StyleUserSelect::None),
            "a button label must not be text-selectable"
        );

        // Every declaration is unconditional — a stray `:hover` here would make the
        // label font/size flicker on mouse-over.
        assert!(
            v.as_ref().iter().all(|p| p.apply_if.as_ref().is_empty()),
            "label style must be unconditional"
        );
    }

    #[test]
    fn build_button_label_style_is_pure() {
        assert_eq!(
            build_button_label_style(),
            build_button_label_style(),
            "label style is not deterministic"
        );
    }

    // ------------------------------------------------------------------
    // Button::create / Button::with_type  (constructors)
    // ------------------------------------------------------------------

    #[test]
    fn create_is_exactly_with_type_default() {
        for label in adversarial_labels() {
            let a = Button::create(AzString::from(label.as_str()));
            let b = Button::with_type(AzString::from(label.as_str()), ButtonType::Default);
            assert_eq!(
                a,
                b,
                "create() diverged from with_type(_, Default) for a {}-byte label",
                label.len()
            );
            assert_eq!(a.button_type, ButtonType::Default);
        }
    }

    #[test]
    fn with_type_holds_its_post_construction_invariants_for_every_type() {
        for ty in ALL_TYPES {
            for label in adversarial_labels() {
                let b = btn(&label, ty);

                // Fields match the arguments, byte for byte (a NUL must not truncate).
                assert_eq!(
                    b.label.as_str(),
                    label.as_str(),
                    "{ty:?}: label was mangled"
                );
                assert_eq!(
                    b.label.as_str().len(),
                    label.len(),
                    "{ty:?}: label length changed"
                );
                assert_eq!(
                    b.button_type, ty,
                    "{ty:?}: button_type field does not match the argument"
                );

                // Optionals start empty.
                assert!(
                    b.image.is_none(),
                    "{ty:?}: a freshly built button must have no image"
                );
                assert!(
                    b.on_click.is_none(),
                    "{ty:?}: a freshly built button must have no callback"
                );

                // Styles are the ones the builders produce, and the vec lengths are
                // consistent with what was handed to `from_vec`.
                let container = build_button_container_style(ty);
                let label_style = build_button_label_style();
                assert_eq!(
                    b.resolved_container_style().len(),
                    container.len(),
                    "{ty:?}: container_style length is inconsistent"
                );
                assert_eq!(
                    b.resolved_container_style().as_ref(),
                    container.as_slice(),
                    "{ty:?}: container_style does not match the builder"
                );
                assert_eq!(
                    b.resolved_label_style().as_ref(),
                    label_style.as_slice(),
                    "{ty:?}: label_style does not match the builder"
                );
                // `with_type` deliberately reuses the label style for the image.
                assert_eq!(
                    b.resolved_image_style().as_ref(),
                    b.resolved_label_style().as_ref(),
                    "{ty:?}: image_style diverged from label_style"
                );
            }
        }
    }

    #[test]
    fn with_type_survives_a_multi_megabyte_label() {
        // 4 MiB of label: no quadratic copy, no truncation, no panic.
        let huge = "\u{1F600}".repeat(1_000_000); // 4 bytes/char
        let b = btn(&huge, ButtonType::Danger);
        assert_eq!(b.label.as_str().len(), 4_000_000);
        assert_eq!(b.label.as_str(), huge.as_str());
    }

    #[test]
    fn buttons_are_cloneable_and_clones_compare_equal() {
        for ty in ALL_TYPES {
            let b = btn("Clone me", ty);
            let c = b.clone();
            assert_eq!(b, c, "{ty:?}: clone() produced a different button");
            assert_eq!(
                format!("{b:?}"),
                format!("{c:?}"),
                "{ty:?}: Debug output diverged between clones"
            );
        }
    }

    // ------------------------------------------------------------------
    // Button::set_button_type / with_button_type
    // ------------------------------------------------------------------

    #[test]
    fn set_button_type_updates_both_the_field_and_the_container_style() {
        let mut b = btn("Save", ButtonType::Default);
        for ty in ALL_TYPES {
            b.set_button_type(ty);
            assert_eq!(b.button_type, ty, "{ty:?}: field not updated");
            assert_eq!(
                b.resolved_container_style().as_ref(),
                build_button_container_style(ty).as_slice(),
                "{ty:?}: container_style was not rebuilt for the new type",
            );
        }
    }

    #[test]
    fn set_button_type_is_idempotent_and_never_accumulates_style() {
        // Re-setting the same type must *replace*, never append: an appending
        // implementation would grow the style vec without bound.
        let mut b = btn("Save", ButtonType::Primary);
        let len = b.resolved_container_style().len();
        for _ in 0..100 {
            b.set_button_type(ButtonType::Primary);
        }
        assert_eq!(
            b.resolved_container_style().len(),
            len,
            "container_style grew across repeated set_button_type calls"
        );
        assert_eq!(
            b,
            btn("Save", ButtonType::Primary),
            "set_button_type(same) is not idempotent"
        );
    }

    #[test]
    fn set_button_type_leaves_the_label_and_the_other_styles_untouched() {
        let mut b = btn("Delete", ButtonType::Default);
        let label_style = b.resolved_label_style().clone();
        let image_style = b.resolved_image_style().clone();
        b.set_button_type(ButtonType::Danger);
        assert_eq!(
            b.label.as_str(),
            "Delete",
            "set_button_type clobbered the label"
        );
        assert_eq!(
            b.resolved_label_style(),
            label_style,
            "set_button_type clobbered label_style"
        );
        assert_eq!(
            b.resolved_image_style(),
            image_style,
            "set_button_type clobbered image_style"
        );
    }

    #[test]
    fn with_button_type_round_trips_to_with_type() {
        // create(l).with_button_type(t) must be indistinguishable from with_type(l, t).
        for ty in ALL_TYPES {
            for label in ["", "OK", "\u{1F600}\0"] {
                let built = Button::create(AzString::from(label)).with_button_type(ty);
                let direct = Button::with_type(AzString::from(label), ty);
                assert_eq!(
                    built, direct,
                    "{ty:?}: builder path diverged from with_type for {label:?}"
                );
            }
        }
    }

    #[test]
    fn with_button_type_chains_take_the_last_type() {
        let b = btn("x", ButtonType::Default)
            .with_button_type(ButtonType::Primary)
            .with_button_type(ButtonType::Link)
            .with_button_type(ButtonType::Warning);
        assert_eq!(b.button_type, ButtonType::Warning);
        assert_eq!(b, btn("x", ButtonType::Warning));
    }

    // ------------------------------------------------------------------
    // Button::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_original_and_leaves_a_default_button_behind() {
        let mut b = btn("Delete", ButtonType::Danger);
        b.set_image(ImageRef::null_image(
            1,
            1,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        b.set_on_click(RefAny::new(7u32), test_click as ButtonOnClickCallbackType);

        let taken = b.swap_with_default();

        // The returned value is the old button, whole.
        assert_eq!(taken.label.as_str(), "Delete");
        assert_eq!(taken.button_type, ButtonType::Danger);
        assert!(
            taken.image.is_some(),
            "the image did not travel with the swapped-out button"
        );
        assert!(
            taken.on_click.is_some(),
            "the callback did not travel with the swapped-out button"
        );

        // ... and what is left behind is a pristine empty Default button.
        assert_eq!(b.label.as_str(), "");
        assert_eq!(b.button_type, ButtonType::Default);
        assert!(
            b.image.is_none(),
            "the swapped-in default still carries an image"
        );
        assert!(
            b.on_click.is_none(),
            "the swapped-in default still carries a callback"
        );
        assert_eq!(
            b,
            Button::create(AzString::from("")),
            "swap_with_default left a non-default button"
        );
    }

    #[test]
    fn swap_with_default_is_stable_under_repetition() {
        // Repeated swapping must not double-free or drift: after the first call the
        // button is already default, so every further call is a no-op swap.
        let mut b = btn("x", ButtonType::Info);
        let first = b.swap_with_default();
        assert_eq!(first.label.as_str(), "x");
        for _ in 0..1000 {
            let taken = b.swap_with_default();
            assert_eq!(taken, Button::create(AzString::from("")));
            assert_eq!(b, Button::create(AzString::from("")));
        }
    }

    // ------------------------------------------------------------------
    // Button::set_image
    // ------------------------------------------------------------------

    #[test]
    fn set_image_stores_the_image_and_replaces_a_previous_one() {
        let mut b = btn("With icon", ButtonType::Primary);
        assert!(b.image.is_none());

        b.set_image(ImageRef::null_image(
            16,
            16,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        assert!(b.image.is_some(), "set_image did not store the image");
        let first_id = b
            .image
            .as_ref()
            .map(|i| i.id)
            .expect("image must be present");

        b.set_image(ImageRef::null_image(
            32,
            32,
            RawImageFormat::RGB8,
            Vec::new(),
        ));
        let second_id = b
            .image
            .as_ref()
            .map(|i| i.id)
            .expect("image must be present");
        assert_ne!(
            first_id, second_id,
            "set_image did not replace the previous image"
        );
    }

    #[test]
    fn set_image_accepts_degenerate_and_extreme_dimensions() {
        // A null image carries only its metadata, so these must not allocate,
        // overflow (`w * h * bpp`) or panic.
        let extremes = [
            (0usize, 0usize),
            (0, 4096),
            (4096, 0),
            (1, usize::MAX),
            (usize::MAX, usize::MAX),
        ];
        for (w, h) in extremes {
            let mut b = btn("x", ButtonType::Default);
            b.set_image(ImageRef::null_image(
                w,
                h,
                RawImageFormat::RGBA8,
                Vec::new(),
            ));
            assert!(b.image.is_some(), "{w}x{h}: image was dropped");
            let dom = b.dom();
            assert_eq!(
                dom.children.as_ref().len(),
                2,
                "{w}x{h}: expected an image child and a label child"
            );
        }
    }

    // ------------------------------------------------------------------
    // Button::set_on_click / with_on_click
    // ------------------------------------------------------------------

    #[test]
    fn set_on_click_stores_the_callback_and_replaces_a_previous_one() {
        let mut b = btn("Click", ButtonType::Primary);
        assert!(b.on_click.is_none());

        b.set_on_click(RefAny::new(1u32), test_click as ButtonOnClickCallbackType);
        assert!(
            b.on_click.is_some(),
            "set_on_click did not store the callback"
        );

        b.set_on_click(RefAny::new(2u32), other_click as ButtonOnClickCallbackType);
        let stored = b.on_click.as_ref().expect("callback must be present");
        assert_eq!(
            stored.callback.cb as *const () as usize,
            other_click as ButtonOnClickCallbackType as *const () as usize,
            "the second set_on_click did not replace the first",
        );

        // ... and it replaced rather than accumulated: the DOM still fires once.
        let dom = b.dom();
        assert_eq!(
            dom.root.callbacks.as_ref().len(),
            1,
            "a re-set callback was appended instead of replaced"
        );
    }

    #[test]
    fn with_on_click_round_trips_the_function_pointer_and_the_payload_into_the_dom() {
        let cb: ButtonOnClickCallbackType = test_click;
        let expected_ptr = cb as *const () as usize;

        let dom = btn("Click", ButtonType::Success)
            .with_on_click(RefAny::new(0xDEAD_BEEF_u32), cb)
            .dom();

        let callbacks = dom.root.callbacks.as_ref();
        assert_eq!(callbacks.len(), 1, "exactly one click callback is expected");
        assert_eq!(
            callbacks[0].event,
            EventFilter::Hover(HoverEventFilter::Click),
            "the button must fire on mouse-up, not on any other filter",
        );
        assert_eq!(
            callbacks[0].callback.cb, expected_ptr,
            "the fn pointer was corrupted on the way into the DOM"
        );

        // The RefAny payload survives the move into the DOM (shared, not copied).
        let mut data = callbacks[0].refany.clone();
        assert_eq!(
            *data.downcast_ref::<u32>().expect("payload changed type"),
            0xDEAD_BEEF,
            "payload was corrupted"
        );
        assert!(
            data.downcast_ref::<u64>().is_none(),
            "downcast to the wrong type must fail, not reinterpret"
        );
    }

    #[test]
    fn with_on_click_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm transmutes the fn pointer — this is the FFI path
        // (Python/C) into the same slot, so the pointer must come out untouched.
        let generic = Callback {
            cb: test_click,
            ctx: azul_core::refany::OptionRefAny::None,
        };
        let raw: ButtonOnClickCallbackType = test_click;
        let expected_ptr = raw as *const () as usize;

        let dom = btn("Click", ButtonType::Info)
            .with_on_click(RefAny::new(1u8), generic)
            .dom();
        let callbacks = dom.root.callbacks.as_ref();
        assert_eq!(callbacks.len(), 1);
        assert_eq!(
            callbacks[0].callback.cb, expected_ptr,
            "the Callback -> ButtonOnClickCallback transmute mangled the pointer"
        );
    }

    #[test]
    fn a_button_without_a_callback_registers_no_callbacks() {
        for ty in ALL_TYPES {
            let dom = btn("Inert", ty).dom();
            assert!(
                dom.root.callbacks.as_ref().is_empty(),
                "{ty:?}: a callback appeared out of nowhere"
            );
        }
    }

    // ------------------------------------------------------------------
    // Button::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_builds_a_focusable_button_node_with_the_base_and_type_class() {
        for ty in ALL_TYPES {
            let dom = btn("OK", ty).dom();

            assert!(
                matches!(dom.root.get_node_type(), NodeType::Button),
                "{ty:?}: root is not a Button node"
            );
            assert_eq!(
                dom.root.flags.get_tab_index(),
                Some(TabIndex::Auto),
                "{ty:?}: the button is not keyboard-focusable",
            );
            assert_eq!(
                classes(&dom),
                vec![
                    "__azul-native-button".to_string(),
                    ty.class_name().to_string(),
                    // The theme marker the render path adds — `Button::dom` renders through
                    // `flat::button` / `flora::button`, which tag the node with their theme.
                    "__azul-theme-flat".to_string(),
                ],
                "{ty:?}: wrong classes (base class first, then the type class)",
            );
            assert!(
                !dom.root.style.is_empty(),
                "{ty:?}: the container style did not reach the node"
            );
        }
    }

    /// `ButtonType::surface` decides what the theme may do in dark mode: the
    /// neutral (Default) face takes the theme's dark face; a coloured command
    /// keeps its own colour — no dark background twin at all — and the link
    /// has no face. A blue primary button on a dark window stays blue.
    #[test]
    fn only_the_neutral_surface_takes_a_dark_face() {
        use azul_css::props::property::CssPropertyType;

        // RESTING dark twins only — `[Theme(Dark)]` and nothing else. The
        // hover/pressed twins from `button_states` carry a pseudo-state too,
        // and a coloured command legitimately has those (in its own colour).
        let dark_backgrounds = |ty: ButtonType| {
            use azul_css::dynamic_selector::{DynamicSelector, ThemeCondition};
            btn("OK", ty)
                .dom()
                .root
                .style
                .iter_inline_properties()
                .filter(|(p, c)| {
                    p.get_type() == CssPropertyType::BackgroundContent
                        && !c.as_ref().is_empty()
                        && c.as_ref()
                            .iter()
                            .all(|s| matches!(s, DynamicSelector::Theme(ThemeCondition::Dark)))
                })
                .count()
        };
        assert!(
            dark_backgrounds(ButtonType::Default) >= 1,
            "the neutral face takes the theme's dark face"
        );
        for ty in [
            ButtonType::Primary,
            ButtonType::Secondary,
            ButtonType::Success,
            ButtonType::Danger,
            ButtonType::Warning,
            ButtonType::Info,
        ] {
            assert_eq!(ty.surface(), ButtonSurface::OwnColour);
            assert_eq!(
                dark_backgrounds(ty),
                0,
                "{ty:?}: a coloured command keeps its colour in dark mode"
            );
        }
        assert_eq!(ButtonType::Link.surface(), ButtonSurface::NoSurface);
        assert_eq!(ButtonType::Default.surface(), ButtonSurface::Neutral);
    }

    #[test]
    fn dom_carries_the_container_style_on_the_root_and_the_label_style_on_the_child() {
        for ty in ALL_TYPES {
            let b = btn("OK", ty);
            let container = properties(&b.resolved_container_style());
            let label_style = properties(&b.resolved_label_style());
            let dom = b.dom();

            // Only the RESTING declarations: the theme appends hover, pressed
            // and focus rules on top of the container style, and those are not
            // part of what the widget declared.
            assert_eq!(
                theme_probe::unconditional(&dom),
                container,
                "{ty:?}: the root inline style is not the container style"
            );

            let children = dom.children.as_ref();
            assert_eq!(
                children.len(),
                1,
                "{ty:?}: an image-less button is a Button node with exactly one <p> label child"
            );
            assert_eq!(
                p_label_of(&children[0]),
                Some("OK"),
                "{ty:?}: the label was mangled"
            );
            assert_eq!(
                inline_properties(&children[0]),
                label_style,
                "{ty:?}: the label style is not on the label node"
            );
        }
    }

    #[test]
    fn dom_puts_the_image_before_the_label_and_keeps_the_child_count_cache_honest() {
        let mut b = btn("Save", ButtonType::Primary);
        b.set_image(ImageRef::null_image(
            16,
            16,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        let image_style = properties(&b.resolved_image_style());
        let dom = b.dom();

        let children = dom.children.as_ref();
        assert_eq!(
            children.len(),
            2,
            "an image button renders the image and the label"
        );
        assert!(
            matches!(children[0].root.get_node_type(), NodeType::Image(_)),
            "the image must come first (left of the label)"
        );
        assert_eq!(
            p_label_of(&children[1]),
            Some("Save"),
            "the label must be the second child"
        );
        assert_eq!(
            inline_properties(&children[0]),
            image_style,
            "the image style is not on the image node"
        );

        // `estimated_total_children` is a cache that, if wrong, makes the compact-DOM
        // conversion under-allocate its arenas and write out of bounds.
        assert_eq!(
            dom.estimated_total_children,
            count_descendants(&dom),
            "estimated_total_children is out of sync with the real subtree",
        );
    }

    #[test]
    fn dom_child_count_cache_is_honest_without_an_image_too() {
        for ty in ALL_TYPES {
            let dom = btn("x", ty).dom();
            assert_eq!(
                dom.estimated_total_children,
                count_descendants(&dom),
                "{ty:?}: stale estimated_total_children"
            );
            assert_eq!(
                dom.estimated_total_children, 2,
                "{ty:?}: an image-less button has the <p> label and its text node"
            );
        }
    }

    #[test]
    fn dom_preserves_adversarial_labels_verbatim() {
        for label in adversarial_labels() {
            let dom = btn(&label, ButtonType::Default).dom();
            let children = dom.children.as_ref();
            assert_eq!(children.len(), 1);
            let text =
                p_label_of(&children[0]).expect("the label child is not a <p>-wrapped text node");
            assert_eq!(
                text,
                label.as_str(),
                "a {}-byte label was mangled",
                label.len()
            );
            assert_eq!(
                text.len(),
                label.len(),
                "a NUL or a wide char truncated the label"
            );
        }
    }

    #[test]
    fn dom_of_a_button_whose_label_looks_like_a_class_name_does_not_leak_into_the_classes() {
        // The label is user data — it must never be able to add a CSS class.
        let dom = btn("__azul-btn-danger", ButtonType::Primary).dom();
        assert_eq!(
            classes(&dom),
            vec![
                "__azul-native-button".to_string(),
                "__azul-btn-primary".to_string(),
                // The theme marker the render path adds — `Button::dom` renders through
                // `flat::button` / `flora::button`, which tag the node with their theme.
                "__azul-theme-flat".to_string(),
            ],
            "the label leaked into the class list",
        );
    }

    #[test]
    fn dom_renders_the_type_the_button_was_last_set_to() {
        for ty in ALL_TYPES {
            let dom = btn("x", ButtonType::Default).with_button_type(ty).dom();
            assert!(
                classes(&dom).contains(&ty.class_name().to_string()),
                "{ty:?}: the DOM still carries the old type class",
            );
        }
    }

    #[test]
    fn dom_is_deterministic_across_identical_buttons() {
        // Same inputs, same tree: the node type, classes and inline styles must all
        // be reproducible (only the ImageRef/RefAny identities may differ).
        for ty in ALL_TYPES {
            let a = btn("Same", ty).dom();
            let b = btn("Same", ty).dom();
            assert_eq!(
                a.root.get_node_type(),
                b.root.get_node_type(),
                "{ty:?}: node type differs"
            );
            assert_eq!(classes(&a), classes(&b), "{ty:?}: classes differ");
            assert_eq!(
                inline_properties(&a),
                inline_properties(&b),
                "{ty:?}: inline style differs"
            );
            assert_eq!(
                a.children.as_ref().len(),
                b.children.as_ref().len(),
                "{ty:?}: child count differs"
            );
        }
    }
}

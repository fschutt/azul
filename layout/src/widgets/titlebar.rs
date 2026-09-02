//! Titlebar widget for custom window chrome (CSD and title-only modes).
//!
//! Key type: [`Titlebar`]

use azul_core::{
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClass::Id, IdOrClassVec},
    refany::RefAny,
};
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelector, PseudoStateType,
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
    system::{SystemFontType, SystemStyle, TitlebarButtonSide, TitlebarButtons, TitlebarMetrics},
    *,
};

// ── Compile-time defaults (used when no SystemStyle is available) ─────────

// Verified: macOS 11 Big Sur – macOS 15 Sequoia (2020–2025)
#[cfg(target_os = "macos")]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 28.0;
#[cfg(target_os = "windows")]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 32.0;
#[cfg(target_os = "linux")]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 30.0;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const DEFAULT_TITLEBAR_HEIGHT: f32 = 32.0;

#[cfg(target_os = "macos")]
const DEFAULT_TITLE_FONT_SIZE: f32 = 13.0;
#[cfg(target_os = "windows")]
const DEFAULT_TITLE_FONT_SIZE: f32 = 12.0;
#[cfg(target_os = "linux")]
const DEFAULT_TITLE_FONT_SIZE: f32 = 13.0;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const DEFAULT_TITLE_FONT_SIZE: f32 = 13.0;

// Verified: macOS 11–15 traffic-light geometry = 78px including gaps
#[cfg(target_os = "macos")]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 78.0;
// Windows 10/11: 3 buttons x 46px = 138px
#[cfg(target_os = "windows")]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 138.0;
#[cfg(target_os = "linux")]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 100.0;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 100.0;

// macOS: traffic lights on the left.  All others: right.
#[cfg(target_os = "macos")]
const DEFAULT_BUTTON_SIDE_LEFT: bool = true;
#[cfg(not(target_os = "macos"))]
const DEFAULT_BUTTON_SIDE_LEFT: bool = false;

// Default title text color for light / dark fallback
const DEFAULT_TITLE_COLOR_LIGHT: ColorU = ColorU {
    r: 76,
    g: 76,
    b: 76,
    a: 255,
}; // #4c4c4c
const DEFAULT_TITLE_COLOR_DARK: ColorU = ColorU {
    r: 229,
    g: 229,
    b: 229,
    a: 255,
}; // #e5e5e5

// ── Titlebar ─────────────────────────────────────────────────────────────

/// A titlebar widget with optional close / minimize / maximize
/// buttons, drag-to-move, and double-click-to-maximize.
///
/// # Two modes
///
/// 1. **Title-only** ([`Titlebar::dom`], the default for
///    `WindowDecorations::NoTitleAutoInject`):
///    The OS still draws the native window-control buttons (traffic lights on
///    macOS, caption buttons on Windows).  The titlebar reserves
///    `padding_left` / `padding_right` so the title text doesn't overlap them.
///
/// 2. **Full CSD** ([`Titlebar::dom_with_buttons`], used when
///    `WindowDecorations::None` + `has_decorations`):
///    The titlebar renders its own close / minimize / maximize buttons as
///    regular DOM nodes.  Each button carries a plain `MouseDown` callback
///    that calls `CallbackInfo::modify_window_state()` - exactly the same
///    mechanism used for window dragging.  No special event-system hooks.
///
/// Window-control buttons use `Dom::create_icon("system:titlebar-close,…")` — an
/// icon spec is a fallback chain, so the DESKTOP's own control icons win where
/// the session registered them and the engine's glyphs cover everywhere else — so that
/// icons are resolved through the icon provider system (Material Icons
/// by default) and can be swapped out by registering a different icon pack.
///
/// # Button layout
///
/// `button_side` controls where the buttons appear:
/// - `Left` - macOS traffic-light style (buttons before title)
/// - `Right` - Windows / Linux style (title then buttons)
///
/// # Styling
///
/// The DOM uses CSS classes `.csd-titlebar`, `.csd-title`, `.csd-buttons`,
/// `.csd-button`, `.csd-close`, `.csd-minimize`, `.csd-maximize`.
/// These match the output of `SystemStyle::create_csd_stylesheet()`.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct Titlebar {
    /// The title text to display.
    pub title: AzString,
    /// Height of the titlebar in CSS pixels.
    pub height: f32,
    /// Font size for the title text in CSS pixels.
    pub font_size: f32,
    /// Extra padding on the **left** side (px).
    pub padding_left: f32,
    /// Extra padding on the **right** side (px).
    pub padding_right: f32,
    /// Title text color (resolved from SystemStyle.colors.text or platform default).
    pub title_color: ColorU,
    /// The titlebar's own background.
    ///
    /// `None` = the caller styles it (the historical behaviour: apps passed
    /// `background:` through `.with_css()`). `from_system_style` fills it with
    /// the DESKTOP's titlebar colour, so a client-side decoration is the right
    /// colour without every app having to know what KDE's `Colors:Header` is.
    pub background_color: OptionColorU,
    /// The titlebar's background while the window does NOT have focus.
    ///
    /// Emitted under `:backdrop`, the pseudo-class that means exactly "this
    /// window is unfocused". Every desktop dims its titlebar on focus loss,
    /// and a decoration that does not is the one window on screen that always
    /// looks active.
    pub background_inactive: OptionColorU,
    /// Title text colour while the window is unfocused, same mechanism.
    pub title_color_inactive: OptionColorU,
    /// Background a window-control button takes on hover.
    pub button_hover_color: OptionColorU,
    /// Background the CLOSE button takes on hover — its own colour, because
    /// Breeze and Windows both turn it red while the others stay neutral.
    pub close_hover_color: OptionColorU,
}

impl Titlebar {
    /// Create a titlebar with compile-time platform defaults.
    ///
    /// Use [`Titlebar::from_system_style`] when you have a
    /// `SystemStyle` available for pixel-perfect metrics.
    #[inline]
    #[must_use]
    pub fn new(title: AzString) -> Self {
        // Equal padding on both sides keeps text-align:center at the window midpoint.
        // The button-side half prevents overlap; the opposite half balances it.
        let half = DEFAULT_BUTTON_AREA_WIDTH / 2.0;
        let (padding_left, padding_right) = (half, half);
        Self {
            title,
            height: DEFAULT_TITLEBAR_HEIGHT,
            font_size: DEFAULT_TITLE_FONT_SIZE,
            padding_left,
            padding_right,
            title_color: DEFAULT_TITLE_COLOR_LIGHT,
            background_color: OptionColorU::None,
            background_inactive: OptionColorU::None,
            title_color_inactive: OptionColorU::None,
            button_hover_color: OptionColorU::None,
            close_hover_color: OptionColorU::None,
        }
    }

    /// FFI-compatible alias for [`Titlebar::new`].
    #[inline]
    #[must_use]
    pub fn create(title: AzString) -> Self {
        Self::new(title)
    }

    /// Create a titlebar with a custom height.
    #[inline]
    #[must_use]
    pub fn with_height(title: AzString, height: f32) -> Self {
        let mut tb = Self::new(title);
        tb.height = height;
        tb
    }

    /// Set the titlebar height.
    #[inline]
    pub const fn set_height(&mut self, height: f32) {
        self.height = height;
    }

    /// Set the title text.
    #[inline]
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }

    /// Swap this titlebar with a default instance, returning the old value.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::new(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Create from a live [`SystemStyle`] (for title-only mode, padding
    /// reserves space for OS-drawn buttons).
    #[must_use]
    pub fn from_system_style(title: AzString, system_style: &SystemStyle) -> Self {
        let tm = &system_style.metrics.titlebar;
        let height = tm.height.as_ref().map_or(DEFAULT_TITLEBAR_HEIGHT, |pv| {
            pv.to_pixels_internal(0.0, 0.0, 0.0)
        });
        let font_size = tm
            .title_font_size
            .into_option()
            .unwrap_or(DEFAULT_TITLE_FONT_SIZE);
        let button_area = tm
            .button_area_width
            .as_ref()
            .map_or(DEFAULT_BUTTON_AREA_WIDTH, |pv| {
                pv.to_pixels_internal(0.0, 0.0, 0.0)
            });
        let safe_left = tm
            .safe_area
            .left
            .as_ref()
            .map_or(0.0, |pv| pv.to_pixels_internal(0.0, 0.0, 0.0));
        let safe_right = tm
            .safe_area
            .right
            .as_ref()
            .map_or(0.0, |pv| pv.to_pixels_internal(0.0, 0.0, 0.0));
        // Apply padding_horizontal from TitlebarMetrics
        let pad_h = tm
            .padding_horizontal
            .as_ref()
            .map_or(0.0, |pv| pv.to_pixels_internal(0.0, 0.0, 0.0));

        // Equal padding on both sides so text-align:center stays at the window midpoint.
        // button_area/2 on each side: the button-side half clears the traffic-lights/caption
        // buttons, the opposite half balances the centering offset.
        let half_btn = button_area / 2.0;
        let (padding_left, padding_right) =
            (half_btn + safe_left + pad_h, half_btn + safe_right + pad_h);

        // The TITLEBAR's own text colour first: a titlebar is not painted in
        // the window palette, and every desktop gives it its own pair
        // (KDE's `Colors:Header`). Falling straight through to the window
        // text made a client-side decoration the right shape in the wrong
        // colour, which is exactly what makes it read as foreign beside a
        // native neighbour. Window text, then the theme default, remain the
        // fallbacks for a platform that states no titlebar colour.
        let title_color = tm
            .text_active
            .into_option()
            .or_else(|| system_style.colors.text.into_option())
            .unwrap_or(match system_style.theme {
                system::Theme::Dark => DEFAULT_TITLE_COLOR_DARK,
                system::Theme::Light => DEFAULT_TITLE_COLOR_LIGHT,
            });

        Self {
            title,
            height,
            font_size,
            padding_left,
            padding_right,
            title_color,
            // The DESKTOP's titlebar colour, so a CSD decoration matches the
            // native windows beside it instead of the window background.
            background_color: tm.background_active,
            background_inactive: tm.background_inactive,
            title_color_inactive: tm.text_inactive,
            button_hover_color: tm.button_hover_background,
            close_hover_color: tm.close_button_hover_background,
        }
    }

    /// Create from [`SystemStyle`] for **full CSD** mode (no padding - the
    /// buttons are rendered as DOM children).
    #[must_use]
    pub fn from_system_style_csd(title: AzString, system_style: &SystemStyle) -> Self {
        let tm = &system_style.metrics.titlebar;
        let height = tm.height.as_ref().map_or(DEFAULT_TITLEBAR_HEIGHT, |pv| {
            pv.to_pixels_internal(0.0, 0.0, 0.0)
        });
        let font_size = tm
            .title_font_size
            .into_option()
            .unwrap_or(DEFAULT_TITLE_FONT_SIZE);
        let title_color =
            system_style
                .colors
                .text
                .into_option()
                .unwrap_or(match system_style.theme {
                    system::Theme::Dark => DEFAULT_TITLE_COLOR_DARK,
                    system::Theme::Light => DEFAULT_TITLE_COLOR_LIGHT,
                });
        Self {
            title,
            height,
            font_size,
            padding_left: 0.0,
            padding_right: 0.0,
            title_color,
            // The DESKTOP's titlebar colour, so a CSD decoration matches the
            // native windows beside it instead of the window background.
            background_color: tm.background_active,
            background_inactive: tm.background_inactive,
            title_color_inactive: tm.text_inactive,
            button_hover_color: tm.button_hover_background,
            close_hover_color: tm.close_button_hover_background,
        }
    }

    /// Build inline CSS for the container div.
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    fn build_container_style(&self, show_buttons: bool) -> CssPropertyWithConditionsVec {
        let mut props = Vec::with_capacity(8);
        if show_buttons {
            // CSD mode: flex layout to place buttons + title side by side
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_display(LayoutDisplay::Flex),
            ));
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_flex_direction(LayoutFlexDirection::Row),
            ));
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_align_items(LayoutAlignItems::Center),
            ));
        } else {
            // Title-only mode: block layout — title fills width automatically.
            // Avoids flex-grow complexity; text centers via text-align.
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_display(LayoutDisplay::Block),
            ));
        }
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_height(LayoutHeight::const_px(self.height as isize)),
        ));
        // The titlebar's own background, when the platform stated one. Emitted
        // as a normal declaration so an app's `.with_css("background: …")`
        // still overrides it — the widget supplies the native default, it does
        // not take the decision away.
        if let OptionColorU::Some(bg) = self.background_color {
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(
                    vec![StyleBackgroundContent::Color(bg)],
                )),
            ));
        }
        // …and the dimmed one for when focus leaves. `:backdrop` is the
        // pseudo-class for exactly that (`DynamicSelectorContext::window_focused`
        // drives it), so this needs no focus plumbing of its own — it is a
        // conditional declaration like `:hover`.
        if let OptionColorU::Some(bg) = self.background_inactive {
            props.push(CssPropertyWithConditions::with_single_condition(
                CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(
                    vec![StyleBackgroundContent::Color(bg)],
                )),
                &[DynamicSelector::PseudoState(PseudoStateType::Backdrop)],
            ));
        }
        // Titlebar should show grab cursor and prevent text selection
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_cursor(StyleCursor::Grab),
        ));
        props.push(CssPropertyWithConditions::simple(CssProperty::user_select(
            StyleUserSelect::None,
        )));
        if self.padding_left > 0.0 {
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_padding_left(LayoutPaddingLeft::const_px(
                    self.padding_left as isize,
                )),
            ));
        }
        if self.padding_right > 0.0 {
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_padding_right(LayoutPaddingRight::const_px(
                    self.padding_right as isize,
                )),
            ));
        }
        CssPropertyWithConditionsVec::from_vec(props)
    }

    /// Build inline CSS for the title text node.
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    fn build_title_style(&self, show_buttons: bool) -> CssPropertyWithConditionsVec {
        let font_family = StyleFontFamilyVec::from_vec(vec![StyleFontFamily::SystemType(
            SystemFontType::TitleBold,
        )]);
        let mut props = Vec::with_capacity(10);
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_size(StyleFontSize::const_px(self.font_size as isize)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_family(font_family),
        ));
        // Use resolved title color from SystemStyle (adapts to dark mode)
        // The dimmed title for an unfocused window, same `:backdrop` mechanism
        // as the container's background above. Pushed BEFORE the active colour
        // so the conditional declaration is the one that wins when it matches.
        if let OptionColorU::Some(dim) = self.title_color_inactive {
            props.push(CssPropertyWithConditions::with_single_condition(
                CssProperty::const_text_color(StyleTextColor { inner: dim }),
                &[DynamicSelector::PseudoState(PseudoStateType::Backdrop)],
            ));
        }
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_text_color(StyleTextColor {
                inner: self.title_color,
            }),
        ));
        // In CSD mode (flex container), title must grow to fill remaining space
        if show_buttons {
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1)),
            ));
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_min_width(LayoutMinWidth::const_px(0)),
            ));
        }
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_text_align(StyleTextAlign::Center),
        ));
        props.push(CssPropertyWithConditions::simple(CssProperty::WhiteSpace(
            StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap),
        )));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_overflow_x(LayoutOverflow::Hidden),
        ));
        // Vertically center the text: pad from top by (height - font_size) / 2
        let v_pad = ((self.height - self.font_size) / 2.0).max(0.0);
        if v_pad > 0.0 {
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_padding_top(LayoutPaddingTop::const_px(v_pad as isize)),
            ));
        }
        CssPropertyWithConditionsVec::from_vec(props)
    }

    /// Title-only DOM (for `NoTitleAutoInject`).
    ///
    /// The OS draws the native window-control buttons; this just renders
    /// a centred title with drag support.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        self.dom_inner(
            false,
            &TitlebarButtons::default(),
            TitlebarButtonSide::Right,
        )
    }

    /// Full-CSD DOM with close / minimize / maximize buttons.
    ///
    /// Each button is a div with a `MouseDown` callback that calls
    /// `modify_window_state()` - no special hooks needed.
    #[must_use]
    pub fn dom_with_buttons(
        self,
        buttons: &TitlebarButtons,
        button_side: TitlebarButtonSide,
    ) -> Dom {
        self.dom_inner(true, buttons, button_side)
    }

    /// Inner builder for both modes.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    fn dom_inner(
        self,
        show_buttons: bool,
        buttons: &TitlebarButtons,
        button_side: TitlebarButtonSide,
    ) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter},
        };

        #[derive(Debug, Clone, Copy)]
        struct DragMarker;

        // Build styles BEFORE moving self.title
        let title_style = self.build_title_style(show_buttons);
        let container_style = self.build_container_style(show_buttons);

        // ── Title node with drag callbacks ──
        let title_classes = IdOrClassVec::from_vec(vec![Class("csd-title".into())]);

        let title_node = Dom::create_div()
            .with_ids_and_classes(title_classes)
            .with_css_props(title_style)
            .with_child(crate::widgets::widget_p_with_text(self.title)) // moves self.title
            .with_callbacks(vec![
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DragStart),
                    callback: CoreCallback {
                        cb: callbacks::titlebar_drag_start as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(DragMarker),
                },
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Drag),
                    callback: CoreCallback {
                        cb: callbacks::titlebar_drag as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(DragMarker),
                },
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                    callback: CoreCallback {
                        cb: callbacks::titlebar_double_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(DragMarker),
                },
            ].into());

        // ── Button container (CSD mode only) ──
        let button_container = if show_buttons {
            Some(build_button_container(
                buttons,
                self.button_hover_color,
                self.close_hover_color,
            ))
        } else {
            None
        };

        // ── Root ──
        let container_classes = IdOrClassVec::from_vec(vec![
            Class("csd-titlebar".into()),
            Class("__azul-native-titlebar".into()),
        ]);
        let mut root = Dom::create_div()
            .with_ids_and_classes(container_classes)
            .with_css_props(container_style);

        // Button side determines child order:
        //   Left  (macOS):   [buttons] [title]
        //   Right (Win/Lin): [title] [buttons]
        match button_side {
            TitlebarButtonSide::Left => {
                if let Some(btn) = button_container {
                    root = root.with_child(btn);
                }
                root = root.with_child(title_node);
            }
            TitlebarButtonSide::Right => {
                root = root.with_child(title_node);
                if let Some(btn) = button_container {
                    root = root.with_child(btn);
                }
            }
        }

        root
    }
}

/// Build the `.csd-buttons` container with close/min/max button DOM nodes.
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn build_button_container(
    buttons: &TitlebarButtons,
    hover: OptionColorU,
    close_hover: OptionColorU,
) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter},
    };

    // The hover background a control takes, as an inline `:hover` declaration.
    // Emitted per button rather than as one class rule because CLOSE has its
    // own colour on Breeze and Windows alike (red), and the others do not.
    let hover_style = |c: OptionColorU| -> CssPropertyWithConditionsVec {
        match c {
            OptionColorU::Some(c) => {
                CssPropertyWithConditionsVec::from_vec(vec![CssPropertyWithConditions::on_hover(
                    CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(
                        vec![StyleBackgroundContent::Color(c)],
                    )),
                )])
            }
            // Nothing stated: declare nothing, so an app's own `.csd-button`
            // styling keeps full control.
            OptionColorU::None => CssPropertyWithConditionsVec::from_vec(Vec::new()),
        }
    };

    let mut children = Vec::new();

    if buttons.has_minimize {
        let classes = IdOrClassVec::from_vec(vec![
            Id("csd-button-minimize".into()),
            Class("csd-button".into()),
            Class("csd-minimize".into()),
        ]);
        children.push(
            Dom::create_div()
                .with_ids_and_classes(classes)
                .with_css_props(hover_style(hover))
                .with_child(Dom::create_icon("system:window-minimize,minimize"))
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseDown),
                        callback: CoreCallback {
                            cb: callbacks::csd_minimize as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(()),
                    }]
                    .into(),
                ),
        );
    }

    if buttons.has_maximize {
        let classes = IdOrClassVec::from_vec(vec![
            Id("csd-button-maximize".into()),
            Class("csd-button".into()),
            Class("csd-maximize".into()),
        ]);
        children.push(
            Dom::create_div()
                .with_ids_and_classes(classes)
                .with_css_props(hover_style(hover))
                .with_child(Dom::create_icon("system:window-maximize,maximize"))
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseDown),
                        callback: CoreCallback {
                            cb: callbacks::csd_maximize as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(()),
                    }]
                    .into(),
                ),
        );
    }

    if buttons.has_close {
        let classes = IdOrClassVec::from_vec(vec![
            Id("csd-button-close".into()),
            Class("csd-button".into()),
            Class("csd-close".into()),
        ]);
        children.push(
            Dom::create_div()
                .with_ids_and_classes(classes)
                .with_css_props(hover_style(close_hover))
                .with_child(Dom::create_icon("system:titlebar-close,system:window-close,close"))
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseDown),
                        callback: CoreCallback {
                            cb: callbacks::csd_close as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(()),
                    }]
                    .into(),
                ),
        );
    }

    let classes = IdOrClassVec::from_vec(vec![Class("csd-buttons".into())]);
    Dom::create_div()
        .with_ids_and_classes(classes)
        .with_children(DomVec::from_vec(children))
}

impl From<Titlebar> for Dom {
    fn from(t: Titlebar) -> Self {
        t.dom()
    }
}

impl Default for Titlebar {
    fn default() -> Self {
        Self::new(AzString::from_const_str(""))
    }
}

// ── Titlebar callbacks ───────────────────────────────────────────────────

/// All titlebar callbacks: drag, double-click, close, minimize, maximize.
///
/// Every callback is a plain `extern "C"` function that uses
/// `CallbackInfo::modify_window_state()`.  No special hooks needed.
pub mod callbacks {
    use crate::callbacks::CallbackInfo;
    use azul_core::callbacks::Update;
    use azul_core::refany::RefAny;

    /// `DragStart` - on Wayland, initiate compositor-managed move immediately.
    /// On other platforms, just acknowledge (movement happens in `titlebar_drag`).
    #[must_use]
    pub extern "C" fn titlebar_drag_start(_data: RefAny, mut info: CallbackInfo) -> Update {
        // On Wayland, window position is Uninitialized (compositor hides it).
        // We must use xdg_toplevel_move via begin_interactive_move().
        // MWA-B9 (D2): macOS ALSO takes the native path — the backend maps
        // begin_interactive_move to performWindowDragWithEvent:, which is
        // OS-smooth / snap-aware / multi-monitor-correct; the manual
        // per-event position loop below remains for X11/Windows and as the
        // programmatic fallback.
        let ws = info.get_current_window_state().clone();
        let native_move = matches!(
            ws.position,
            azul_core::window::WindowPosition::Uninitialized
        ) || cfg!(target_os = "macos");
        if native_move {
            info.begin_interactive_move();
        } else {
            // MWA-C-csd: reset the fractional-residual accumulator for the
            // manual move loop (see titlebar_drag).
            RESIDUAL_X_BITS.store(0f32.to_bits(), core::sync::atomic::Ordering::Relaxed);
            RESIDUAL_Y_BITS.store(0f32.to_bits(), core::sync::atomic::Ordering::Relaxed);
            // MWA-C-csd: dragging a maximized window restores it first —
            // the native paths get this from the OS drag loop, but the
            // manual loop moved the still-maximized frame around.
            if ws.flags.frame == azul_core::window::WindowFrame::Maximized {
                let mut s = ws;
                s.flags.frame = azul_core::window::WindowFrame::Normal;
                info.modify_window_state(s);
            }
        }
        Update::DoNothing
    }

    /// MWA-C-csd: fractional-residual carry for the manual drag loop -
    /// rounding alone still loses up to half a pixel per event in a
    /// consistent direction, so very slow trackpad drags crawled. Only one
    /// interactive drag exists at a time and callbacks run on the UI
    /// thread; atomics keep this no_std-friendly (f32 stored as bits).
    static RESIDUAL_X_BITS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
    static RESIDUAL_Y_BITS: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

    /// Drag - apply incremental screen-space delta to the CURRENT window position.
    ///
    /// Uses `get_drag_delta_screen_incremental()` (frame-to-frame delta) instead of
    /// `get_drag_delta_screen()` (total delta since drag start). Combined with
    /// the current window position from the OS, this approach is robust against
    /// external position changes during the drag (DPI change, OS clamping,
    /// compositor resize).
    ///
    /// On Wayland: this is a no-op because the compositor manages the move
    /// (initiated by `begin_interactive_move()` in `titlebar_drag_start`).
    #[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
    #[must_use]
    pub extern "C" fn titlebar_drag(_data: RefAny, mut info: CallbackInfo) -> Update {
        use azul_core::geom::PhysicalPositionI32;
        use azul_core::window::WindowPosition;

        let delta = info.get_drag_delta_screen_incremental();
        let current_pos = info.get_current_window_state().position;

        if let (azul_core::geom::OptionDragDelta::Some(d), WindowPosition::Initialized(pos)) =
            (delta, current_pos)
        {
            use core::sync::atomic::Ordering;
            // MWA-C-csd: full fractional-residual carry (upgrades MWA-B9's
            // round-only fix). Each event applies the integer part of
            // delta + residual and carries the remainder, so arbitrarily
            // slow drags advance losslessly.
            let total_x = d.dx + f32::from_bits(RESIDUAL_X_BITS.load(Ordering::Relaxed));
            let total_y = d.dy + f32::from_bits(RESIDUAL_Y_BITS.load(Ordering::Relaxed));
            let apply_x = total_x.round();
            let apply_y = total_y.round();
            RESIDUAL_X_BITS.store((total_x - apply_x).to_bits(), Ordering::Relaxed);
            RESIDUAL_Y_BITS.store((total_y - apply_y).to_bits(), Ordering::Relaxed);
            let new_pos = WindowPosition::Initialized(PhysicalPositionI32::new(
                pos.x + apply_x as i32,
                pos.y + apply_y as i32,
            ));
            let mut ws = info.get_current_window_state().clone();
            ws.position = new_pos;
            info.modify_window_state(ws);
        }
        // On Wayland: current_pos is Uninitialized, so the if-let doesn't match → no-op.
        Update::DoNothing
    }

    /// `DoubleClick` - toggle Maximized ↔ Normal.
    #[must_use]
    pub extern "C" fn titlebar_double_click(_data: RefAny, mut info: CallbackInfo) -> Update {
        use azul_core::window::WindowFrame;
        let mut s = info.get_current_window_state().clone();
        s.flags.frame = if s.flags.frame == WindowFrame::Maximized {
            WindowFrame::Normal
        } else {
            WindowFrame::Maximized
        };
        info.modify_window_state(s);
        Update::DoNothing
    }

    /// Close button - `close_requested = true`.
    pub(super) extern "C" fn csd_close(_data: RefAny, mut info: CallbackInfo) -> Update {
        let mut s = info.get_current_window_state().clone();
        s.flags.close_requested = true;
        info.modify_window_state(s);
        Update::DoNothing
    }

    /// Minimize button - `frame = Minimized`.
    pub(super) extern "C" fn csd_minimize(_data: RefAny, mut info: CallbackInfo) -> Update {
        use azul_core::window::WindowFrame;
        let mut s = info.get_current_window_state().clone();
        s.flags.frame = WindowFrame::Minimized;
        info.modify_window_state(s);
        Update::DoNothing
    }

    /// Maximize button - toggle Maximized ↔ Normal.
    pub(super) extern "C" fn csd_maximize(_data: RefAny, mut info: CallbackInfo) -> Update {
        use azul_core::window::WindowFrame;
        let mut s = info.get_current_window_state().clone();
        s.flags.frame = if s.flags.frame == WindowFrame::Maximized {
            WindowFrame::Normal
        } else {
            WindowFrame::Maximized
        };
        info.modify_window_state(s);
        Update::DoNothing
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::unreadable_literal
)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        callbacks::Update,
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{OptionLogicalPosition, PhysicalPositionI32},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle, WindowFrame, WindowPosition},
    };
    use azul_css::{
        props::basic::{length::SizeMetric, pixel::PixelValue},
        system::SafeAreaInsets,
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ==================================================================
    // Helpers
    // ==================================================================

    /// Titles a caller can realistically hand to a titlebar. The widget never
    /// parses, trims or normalises its title, so every one of these has to reach
    /// the DOM byte-for-byte — `AzString` is length-based, so an embedded NUL
    /// must not truncate, and a ZWJ emoji cluster must not be split.
    const ADVERSARIAL_TITLES: [&str; 10] = [
        "",
        " ",
        "My Window",
        "a\0b",
        "\0",
        "e\u{0301}\u{0301}\u{0301}",
        "\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "\u{202E}gnirts desrever\u{202C}",
        "\u{FFFD}\u{FEFF}\t\n",
        "\u{200B}",
    ];

    /// Every `f32` the numeric surface (`height` / `font_size`) has to survive
    /// *without* tipping the fixed-point encoding over — see
    /// `heights_outside_the_encodable_range_are_not_saturated` for the ones that do.
    const TAME_FLOATS: [f32; 14] = [
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        30.0,
        1000.0,
        -1000.0,
        0.999,
        f32::EPSILON,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::NAN,
    ];

    /// The magnitudes that overflow `PixelValue`'s `value * 1000` encoding on
    /// every pointer width: `as isize` saturates them to `isize::MIN`/`MAX`.
    const UNENCODABLE_FLOATS: [f32; 4] = [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN];

    /// Every `TitlebarButtonSide`.
    const BOTH_SIDES: [TitlebarButtonSide; 2] =
        [TitlebarButtonSide::Left, TitlebarButtonSide::Right];

    /// Every `WindowFrame` a titlebar callback can be invoked against.
    const ALL_FRAMES: [WindowFrame; 4] = [
        WindowFrame::Normal,
        WindowFrame::Minimized,
        WindowFrame::Maximized,
        WindowFrame::Fullscreen,
    ];

    fn tb(title: &str) -> Titlebar {
        Titlebar::new(AzString::from(title))
    }

    /// The declared properties of a style vec, in declaration order.
    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    /// Every declaration must be unconditional: a titlebar built with a
    /// `@media`/`:hover` guard would silently not apply.
    fn all_unconditional(v: &CssPropertyWithConditionsVec) -> bool {
        v.as_ref().iter().all(|p| p.apply_if.as_ref().is_empty())
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. An
    /// `em`/`%` height would resolve against the parent font/box instead of the
    /// fixed chrome geometry the titlebar is supposed to reserve.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "titlebar geometry must be absolute px, got {:?}",
            pv.metric
        );
        pv.number.get()
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn padding_left_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::PaddingLeft(x) => x.get_property().map(|x| px(&x.inner)),
            _ => None,
        })
    }

    fn padding_right_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::PaddingRight(x) => x.get_property().map(|x| px(&x.inner)),
            _ => None,
        })
    }

    fn padding_top_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::PaddingTop(x) => x.get_property().map(|x| px(&x.inner)),
            _ => None,
        })
    }

    fn font_size_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::FontSize(f) => f.get_property().map(|f| px(&f.inner)),
            _ => None,
        })
    }

    fn text_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::TextColor(c) => c.get_property().map(|c| c.inner),
            _ => None,
        })
    }

    /// The exact container declarations the widget documents, for a given mode.
    fn expected_container(t: &Titlebar, show_buttons: bool) -> Vec<CssProperty> {
        let mut v = Vec::new();
        if show_buttons {
            v.push(CssProperty::const_display(LayoutDisplay::Flex));
            v.push(CssProperty::const_flex_direction(LayoutFlexDirection::Row));
            v.push(CssProperty::const_align_items(LayoutAlignItems::Center));
        } else {
            v.push(CssProperty::const_display(LayoutDisplay::Block));
        }
        v.push(CssProperty::const_height(LayoutHeight::const_px(
            t.height as isize,
        )));
        v.push(CssProperty::const_cursor(StyleCursor::Grab));
        v.push(CssProperty::user_select(StyleUserSelect::None));
        if t.padding_left > 0.0 {
            v.push(CssProperty::const_padding_left(
                LayoutPaddingLeft::const_px(t.padding_left as isize),
            ));
        }
        if t.padding_right > 0.0 {
            v.push(CssProperty::const_padding_right(
                LayoutPaddingRight::const_px(t.padding_right as isize),
            ));
        }
        v
    }

    /// The exact title declarations the widget documents, for a given mode.
    fn expected_title(t: &Titlebar, show_buttons: bool) -> Vec<CssProperty> {
        let font_family = StyleFontFamilyVec::from_vec(vec![StyleFontFamily::SystemType(
            SystemFontType::TitleBold,
        )]);
        let mut v = vec![
            CssProperty::const_font_size(StyleFontSize::const_px(t.font_size as isize)),
            CssProperty::const_font_family(font_family),
            CssProperty::const_text_color(StyleTextColor {
                inner: t.title_color,
            }),
        ];
        if show_buttons {
            v.push(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1)));
            v.push(CssProperty::const_min_width(LayoutMinWidth::const_px(0)));
        }
        v.push(CssProperty::const_text_align(StyleTextAlign::Center));
        v.push(CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
            StyleWhiteSpace::Nowrap,
        )));
        v.push(CssProperty::const_overflow_x(LayoutOverflow::Hidden));
        let v_pad = ((t.height - t.font_size) / 2.0).max(0.0);
        if v_pad > 0.0 {
            v.push(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
                v_pad as isize,
            )));
        }
        v
    }

    /// True if `node` carries the CSS class `name`.
    fn has_class(node: &Dom, name: &str) -> bool {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, Class(s) if s.as_str() == name))
    }

    /// Every id declared on `node`, in order.
    fn ids(node: &Dom) -> Vec<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Id(s) => Some(s.as_str().to_string()),
                Class(_) => None,
            })
            .collect()
    }

    /// Every class declared on `node`, in order.
    fn classes(node: &Dom) -> Vec<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                Id(_) => None,
            })
            .collect()
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

    /// The icon name of a `NodeType::Icon` node.
    fn icon_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Icon(s) => Some(s.as_ref().as_str()),
            _ => None,
        }
    }

    /// A node's *inline* style properties, in declaration order.
    fn inline_props(node: &Dom) -> Vec<CssProperty> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// `(event, callback fn address)` for every callback on `node`, in order.
    fn callbacks_of(node: &Dom) -> Vec<(EventFilter, usize)> {
        node.root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|c| (c.event, c.callback.cb))
            .collect()
    }

    /// The recursive descendant count. `Dom::estimated_total_children` is a
    /// *cached* value that, if too small, makes `convert_dom_into_compact_dom`
    /// under-allocate its arenas and panic on out-of-bounds writes — so it has to
    /// match this exactly for every button combination.
    fn count_descendants(dom: &Dom) -> usize {
        dom.children
            .as_ref()
            .iter()
            .map(|c| 1 + count_descendants(c))
            .sum()
    }

    /// A pre-order, structural fingerprint of a DOM: node type, ids/classes,
    /// `(event, fn address)` per callback and inline declarations. Used instead of
    /// `Dom: PartialEq` because the drag callbacks carry freshly allocated
    /// `RefAny`s, which compare by pointer and so are never equal across builds.
    fn fingerprint(dom: &Dom) -> Vec<String> {
        fn walk(d: &Dom, depth: usize, out: &mut Vec<String>) {
            out.push(format!(
                "{depth}|{:?}|{:?}|{:?}|{:?}|{:?}",
                d.root.get_node_type(),
                ids(d),
                classes(d),
                callbacks_of(d),
                inline_props(d),
            ));
            for c in d.children.as_ref() {
                walk(c, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(dom, 0, &mut out);
        out
    }

    /// The title node of a rendered titlebar (the `.csd-title` div).
    fn title_node(dom: &Dom) -> &Dom {
        dom.children
            .as_ref()
            .iter()
            .find(|c| has_class(c, "csd-title"))
            .expect("every titlebar must render a .csd-title node")
    }

    /// The `.csd-buttons` node of a rendered titlebar, if there is one.
    fn buttons_node(dom: &Dom) -> Option<&Dom> {
        dom.children
            .as_ref()
            .iter()
            .find(|c| has_class(c, "csd-buttons"))
    }

    fn all_button_combinations() -> Vec<TitlebarButtons> {
        let mut out = Vec::new();
        for &close in &[false, true] {
            for &min in &[false, true] {
                for &max in &[false, true] {
                    for &full in &[false, true] {
                        out.push(TitlebarButtons {
                            has_close: close,
                            has_minimize: min,
                            has_maximize: max,
                            has_fullscreen: full,
                        });
                    }
                }
            }
        }
        out
    }

    /// A `SystemStyle` whose titlebar metrics are all "not detected" — the state
    /// `SystemStyle::default()` ships and the one the fallbacks exist for.
    fn blank_system_style() -> SystemStyle {
        SystemStyle::default()
    }

    /// Runs `f` against a `CallbackInfo` backed by `state`, returning `f`'s result
    /// plus every recorded `CallbackChange`. No layout result is inserted: none of
    /// the titlebar callbacks walk the DOM.
    fn with_callback_info<R>(
        state: FullWindowState,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = state;
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

        let out = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    /// The window states pushed through `modify_window_state`, in order.
    fn state_writes(changes: &[CallbackChange]) -> Vec<FullWindowState> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ModifyWindowState { state } => Some(state.clone()),
                _ => None,
            })
            .collect()
    }

    fn interactive_moves(changes: &[CallbackChange]) -> usize {
        changes
            .iter()
            .filter(|c| matches!(c, CallbackChange::BeginInteractiveMove))
            .count()
    }

    fn state_with(frame: WindowFrame, position: WindowPosition) -> FullWindowState {
        let mut s = FullWindowState::default();
        s.flags.frame = frame;
        s.position = position;
        s
    }

    // ==================================================================
    // Titlebar::new / Titlebar::create / Default
    // ==================================================================

    #[test]
    fn new_uses_the_compile_time_platform_defaults() {
        let t = tb("hello");

        assert_eq!(t.title.as_str(), "hello");
        assert_eq!(t.height, DEFAULT_TITLEBAR_HEIGHT);
        assert_eq!(t.font_size, DEFAULT_TITLE_FONT_SIZE);
        assert_eq!(t.title_color, DEFAULT_TITLE_COLOR_LIGHT);
        assert_eq!(t.padding_left, DEFAULT_BUTTON_AREA_WIDTH / 2.0);
        assert_eq!(t.padding_right, DEFAULT_BUTTON_AREA_WIDTH / 2.0);
    }

    #[test]
    fn new_pads_both_sides_equally_so_centering_lands_at_the_window_midpoint() {
        // The doc comment is explicit: the button-side half clears the OS buttons,
        // the opposite half balances `text-align: center`. Asymmetric padding would
        // push the title off the window midpoint.
        let t = tb("x");
        assert_eq!(
            t.padding_left, t.padding_right,
            "title-only padding must stay symmetric",
        );
        assert!(
            t.padding_left >= 0.0,
            "negative reserved space is meaningless"
        );
        assert!(t.height > 0.0 && t.height.is_finite());
        assert!(t.font_size > 0.0 && t.font_size.is_finite());
        assert!(
            t.font_size < t.height,
            "the default font must fit inside the default titlebar height",
        );
    }

    #[test]
    fn new_stores_pathological_titles_byte_for_byte() {
        for title in ADVERSARIAL_TITLES {
            let t = tb(title);
            assert_eq!(
                t.title.as_str(),
                title,
                "the title was mangled or normalised"
            );
            assert_eq!(
                t.title.as_str().len(),
                title.len(),
                "the title was truncated (an embedded NUL must not terminate it)",
            );
        }
    }

    #[test]
    fn new_accepts_a_title_far_longer_than_any_real_window_caption() {
        let huge = "a".repeat(1_000_000);
        let t = Titlebar::new(AzString::from(huge.clone()));
        assert_eq!(t.title.as_str().len(), 1_000_000);
        // ... and it survives the trip into the DOM without being re-encoded.
        let dom = t.dom();
        assert_eq!(
            text_of(&title_node(&dom).children.as_ref()[0]),
            Some(huge.as_str())
        );
    }

    #[test]
    fn create_is_indistinguishable_from_new() {
        for title in ADVERSARIAL_TITLES {
            assert_eq!(
                Titlebar::create(AzString::from(title)),
                Titlebar::new(AzString::from(title)),
                "the FFI alias drifted away from Titlebar::new",
            );
        }
    }

    #[test]
    fn default_is_new_with_an_empty_title() {
        let d = Titlebar::default();
        assert_eq!(d, tb(""));
        assert_eq!(d.title.as_str(), "");
    }

    // ==================================================================
    // Titlebar::with_height / Titlebar::set_height
    // ==================================================================

    #[test]
    fn with_height_stores_every_float_bit_exactly_and_touches_nothing_else() {
        let base = tb("t");
        for h in TAME_FLOATS.into_iter().chain(UNENCODABLE_FLOATS) {
            let t = Titlebar::with_height(AzString::from("t"), h);

            // to_bits, not `==`: NaN != NaN, and -0.0 == 0.0 would hide a sign flip.
            assert_eq!(
                t.height.to_bits(),
                h.to_bits(),
                "with_height({h}) did not store the value verbatim",
            );
            assert_eq!(t.title.as_str(), "t");
            assert_eq!(
                t.font_size, base.font_size,
                "with_height({h}) moved the font size"
            );
            assert_eq!(
                t.padding_left, base.padding_left,
                "with_height({h}) moved the padding"
            );
            assert_eq!(
                t.padding_right, base.padding_right,
                "with_height({h}) moved the padding"
            );
            assert_eq!(
                t.title_color, base.title_color,
                "with_height({h}) moved the colour"
            );
        }
    }

    #[test]
    fn set_height_is_a_bit_exact_last_write_wins_store() {
        let mut t = tb("t");
        let base = tb("t");

        for h in TAME_FLOATS.into_iter().chain(UNENCODABLE_FLOATS) {
            t.set_height(h);
            assert_eq!(
                t.height.to_bits(),
                h.to_bits(),
                "set_height({h}) was not verbatim"
            );
            assert_eq!(t.font_size, base.font_size);
            assert_eq!(t.padding_left, base.padding_left);
            assert_eq!(t.padding_right, base.padding_right);
            assert_eq!(t.title.as_str(), "t", "set_height({h}) disturbed the title");
        }

        // The last write is the one that survives; nothing accumulates.
        t.set_height(7.5);
        t.set_height(9.25);
        assert_eq!(t.height, 9.25);
    }

    #[test]
    fn set_height_zero_and_negative_are_stored_not_clamped() {
        // The setter is documented as a plain store — it is `build_container_style`
        // that has to survive the result, not the setter.
        let mut t = tb("t");

        t.set_height(0.0);
        assert_eq!(t.height.to_bits(), 0_f32.to_bits(), "0.0 must stay +0.0");

        t.set_height(-0.0);
        assert_eq!(
            t.height.to_bits(),
            (-0.0_f32).to_bits(),
            "-0.0 must not be normalised"
        );

        t.set_height(-42.0);
        assert_eq!(
            t.height, -42.0,
            "a negative height must not be clamped by the setter"
        );
    }

    // ==================================================================
    // Titlebar::set_title
    // ==================================================================

    #[test]
    fn set_title_replaces_the_title_and_leaves_the_geometry_alone() {
        let mut t = Titlebar::with_height(AzString::from("first"), 44.0);
        for title in ADVERSARIAL_TITLES {
            t.set_title(AzString::from(title));
            assert_eq!(t.title.as_str(), title);
            assert_eq!(t.title.as_str().len(), title.len());
            assert_eq!(t.height, 44.0, "set_title moved the height");
            assert_eq!(
                t.padding_left,
                tb("").padding_left,
                "set_title moved the padding"
            );
        }
    }

    // ==================================================================
    // Titlebar::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_hands_back_the_old_value_and_leaves_a_default() {
        let mut t = Titlebar::with_height(AzString::from("payload"), 99.5);
        t.title_color = ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        };

        let taken = t.swap_with_default();

        assert_eq!(
            taken.title.as_str(),
            "payload",
            "the title did not travel out"
        );
        assert_eq!(taken.height, 99.5, "the height did not travel out");
        assert_eq!(
            taken.title_color,
            ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 4
            }
        );

        assert_eq!(
            t,
            Titlebar::default(),
            "what was left behind is not a default titlebar"
        );
        assert_eq!(t.title.as_str(), "");
    }

    #[test]
    fn swap_with_default_moves_a_nan_height_out_without_losing_its_bits() {
        // `Titlebar` derives PartialEq, so a NaN height makes the struct
        // self-unequal — the swap still has to move the exact bit pattern.
        let mut t = tb("x");
        t.set_height(f32::NAN);

        let taken = t.swap_with_default();

        assert!(taken.height.is_nan(), "the NaN height did not travel out");
        assert_eq!(
            t.height, DEFAULT_TITLEBAR_HEIGHT,
            "the leftover kept the NaN"
        );
        assert_eq!(t, Titlebar::default());
    }

    #[test]
    fn repeated_swap_with_default_never_accumulates_state() {
        let mut t = Titlebar::with_height(AzString::from("x"), 1.0);
        let _first = t.swap_with_default();

        for i in 0..8 {
            let taken = t.swap_with_default();
            assert_eq!(
                taken,
                Titlebar::default(),
                "swap #{i} handed back a non-default"
            );
            assert_eq!(
                t,
                Titlebar::default(),
                "swap #{i} left a non-default behind"
            );
        }

        // The drained titlebar still renders a well-formed DOM.
        let dom = t.dom();
        assert_eq!(dom.children.as_ref().len(), 1);
        assert_eq!(text_of(&title_node(&dom).children.as_ref()[0]), Some(""));
    }

    // ==================================================================
    // Titlebar::from_system_style
    // ==================================================================

    #[test]
    fn from_system_style_falls_back_to_the_compile_time_defaults_when_nothing_is_detected() {
        let ss = blank_system_style();
        let t = Titlebar::from_system_style(AzString::from("sys"), &ss);

        assert_eq!(t.title.as_str(), "sys");
        assert_eq!(
            t.height, DEFAULT_TITLEBAR_HEIGHT,
            "an undetected height must fall back"
        );
        // `TitlebarMetrics::default()` *does* carry a font size (13.0), so the
        // compile-time default is only reachable when it is explicitly None.
        assert_eq!(t.font_size, 13.0);
        assert_eq!(t.padding_left, DEFAULT_BUTTON_AREA_WIDTH / 2.0);
        assert_eq!(t.padding_right, DEFAULT_BUTTON_AREA_WIDTH / 2.0);
        assert_eq!(t.title_color, DEFAULT_TITLE_COLOR_LIGHT);
    }

    #[test]
    fn from_system_style_with_no_font_size_falls_back_to_the_platform_constant() {
        let mut ss = blank_system_style();
        ss.metrics.titlebar.title_font_size = OptionF32::None;
        let t = Titlebar::from_system_style(AzString::from("x"), &ss);
        assert_eq!(t.font_size, DEFAULT_TITLE_FONT_SIZE);
    }

    #[test]
    fn from_system_style_adds_the_safe_area_and_padding_to_each_side_separately() {
        let mut ss = blank_system_style();
        ss.metrics.titlebar.button_area_width = OptionPixelValue::Some(PixelValue::px(100.0));
        ss.metrics.titlebar.padding_horizontal = OptionPixelValue::Some(PixelValue::px(5.0));
        ss.metrics.titlebar.safe_area = SafeAreaInsets {
            top: OptionPixelValue::None,
            bottom: OptionPixelValue::None,
            left: OptionPixelValue::Some(PixelValue::px(10.0)),
            right: OptionPixelValue::Some(PixelValue::px(20.0)),
        };

        let t = Titlebar::from_system_style(AzString::from("x"), &ss);

        assert_eq!(t.padding_left, 50.0 + 10.0 + 5.0);
        assert_eq!(t.padding_right, 50.0 + 20.0 + 5.0);
        // A notch on one side is exactly the documented case where the padding is
        // deliberately *not* symmetric.
        assert_ne!(t.padding_left, t.padding_right);
    }

    #[test]
    fn from_system_style_reads_the_height_and_font_size_it_was_given() {
        let mut ss = blank_system_style();
        ss.metrics.titlebar.height = OptionPixelValue::Some(PixelValue::px(41.0));
        ss.metrics.titlebar.title_font_size = OptionF32::Some(17.5);

        let t = Titlebar::from_system_style(AzString::from("x"), &ss);

        assert_eq!(t.height, 41.0);
        assert_eq!(t.font_size, 17.5);
    }

    #[test]
    fn from_system_style_converts_absolute_units_but_collapses_relative_ones_to_zero() {
        // `to_pixels_internal(0.0, 0.0, 0.0)` is called with *zero* resolution
        // bases, so anything relative (em/rem/%/vw) silently resolves to 0px —
        // a titlebar height declared in `em` collapses the whole chrome.
        let cases: [(PixelValue, f32); 6] = [
            (PixelValue::px(30.0), 30.0),
            (PixelValue::pt(30.0), 30.0 * (96.0 / 72.0)),
            (PixelValue::em(2.0), 0.0),
            (PixelValue::rem(2.0), 0.0),
            (PixelValue::percent(50.0), 0.0),
            (PixelValue::const_from_metric(SizeMetric::Vh, 50), 0.0),
        ];

        for (pv, expected) in cases {
            let mut ss = blank_system_style();
            ss.metrics.titlebar.height = OptionPixelValue::Some(pv);
            let t = Titlebar::from_system_style(AzString::from("x"), &ss);
            assert!(
                (t.height - expected).abs() < 0.01,
                "{pv:?} resolved to {} px, expected {expected} px",
                t.height,
            );
        }
    }

    #[test]
    fn from_system_style_saturates_a_non_finite_metric_instead_of_propagating_it() {
        // `PixelValue::px(inf)` encodes as `f32_to_isize(inf * 1000) == isize::MAX`,
        // so what comes back out is huge but *finite*: an infinity reaching the
        // layout solver would poison every downstream size computation.
        for bogus in [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::NAN,
        ] {
            let mut ss = blank_system_style();
            ss.metrics.titlebar.height = OptionPixelValue::Some(PixelValue::px(bogus));
            ss.metrics.titlebar.button_area_width = OptionPixelValue::Some(PixelValue::px(bogus));

            let t = Titlebar::from_system_style(AzString::from("x"), &ss);

            assert!(
                t.height.is_finite(),
                "{bogus} produced a non-finite height {}",
                t.height
            );
            assert!(
                t.padding_left.is_finite() && t.padding_right.is_finite(),
                "{bogus} produced non-finite padding",
            );
        }

        // NaN specifically collapses to zero rather than staying NaN.
        let mut ss = blank_system_style();
        ss.metrics.titlebar.height = OptionPixelValue::Some(PixelValue::px(f32::NAN));
        assert_eq!(
            Titlebar::from_system_style(AzString::from("x"), &ss).height,
            0.0
        );
    }

    #[test]
    fn from_system_style_prefers_the_detected_text_colour_over_both_theme_fallbacks() {
        let detected = ColorU {
            r: 9,
            g: 8,
            b: 7,
            a: 6,
        };
        for theme in [system::Theme::Light, system::Theme::Dark] {
            let mut ss = blank_system_style();
            ss.theme = theme;
            ss.colors.text = OptionColorU::Some(detected);
            assert_eq!(
                Titlebar::from_system_style(AzString::from("x"), &ss).title_color,
                detected,
                "{theme:?}: the detected system text colour must win",
            );
        }
    }

    #[test]
    fn from_system_style_picks_the_theme_appropriate_fallback_colour() {
        let mut light = blank_system_style();
        light.theme = system::Theme::Light;
        light.colors.text = OptionColorU::None;
        assert_eq!(
            Titlebar::from_system_style(AzString::from("x"), &light).title_color,
            DEFAULT_TITLE_COLOR_LIGHT,
        );

        let mut dark = blank_system_style();
        dark.theme = system::Theme::Dark;
        dark.colors.text = OptionColorU::None;
        assert_eq!(
            Titlebar::from_system_style(AzString::from("x"), &dark).title_color,
            DEFAULT_TITLE_COLOR_DARK,
        );

        // The two fallbacks must actually differ, or dark mode renders unreadably.
        assert_ne!(DEFAULT_TITLE_COLOR_LIGHT, DEFAULT_TITLE_COLOR_DARK);
    }

    #[test]
    fn from_system_style_carries_pathological_titles_through_untouched() {
        let ss = blank_system_style();
        for title in ADVERSARIAL_TITLES {
            let t = Titlebar::from_system_style(AzString::from(title), &ss);
            assert_eq!(t.title.as_str(), title);
            let csd = Titlebar::from_system_style_csd(AzString::from(title), &ss);
            assert_eq!(csd.title.as_str(), title);
        }
    }

    // ==================================================================
    // Titlebar::from_system_style_csd
    // ==================================================================

    #[test]
    fn from_system_style_csd_zeroes_the_padding_and_keeps_everything_else() {
        let mut ss = blank_system_style();
        ss.metrics.titlebar.height = OptionPixelValue::Some(PixelValue::px(41.0));
        ss.metrics.titlebar.title_font_size = OptionF32::Some(17.5);
        ss.theme = system::Theme::Dark;

        let title_only = Titlebar::from_system_style(AzString::from("x"), &ss);
        let csd = Titlebar::from_system_style_csd(AzString::from("x"), &ss);

        assert_eq!(csd.height, title_only.height);
        assert_eq!(csd.font_size, title_only.font_size);
        assert_eq!(csd.title_color, title_only.title_color);
        assert_eq!(csd.title_color, DEFAULT_TITLE_COLOR_DARK);

        // The buttons are DOM children in CSD mode, so no space is reserved.
        assert_eq!(csd.padding_left.to_bits(), 0_f32.to_bits());
        assert_eq!(csd.padding_right.to_bits(), 0_f32.to_bits());
    }

    #[test]
    fn from_system_style_csd_ignores_the_button_area_and_safe_area_entirely() {
        let mut ss = blank_system_style();
        ss.metrics.titlebar.button_area_width = OptionPixelValue::Some(PixelValue::px(500.0));
        ss.metrics.titlebar.padding_horizontal = OptionPixelValue::Some(PixelValue::px(77.0));
        ss.metrics.titlebar.safe_area = SafeAreaInsets {
            top: OptionPixelValue::Some(PixelValue::px(1.0)),
            bottom: OptionPixelValue::Some(PixelValue::px(2.0)),
            left: OptionPixelValue::Some(PixelValue::px(3.0)),
            right: OptionPixelValue::Some(PixelValue::px(4.0)),
        };

        let csd = Titlebar::from_system_style_csd(AzString::from("x"), &ss);
        assert_eq!(csd.padding_left, 0.0);
        assert_eq!(csd.padding_right, 0.0);
    }

    // ==================================================================
    // Titlebar::build_container_style
    // ==================================================================

    #[test]
    fn build_container_style_emits_the_documented_declarations_in_both_modes() {
        let t = tb("x");
        for show_buttons in [false, true] {
            let style = t.build_container_style(show_buttons);
            assert_eq!(
                properties(&style),
                expected_container(&t, show_buttons),
                "container declarations drifted (show_buttons = {show_buttons})",
            );
            assert!(
                all_unconditional(&style),
                "a container declaration became conditional"
            );
        }
    }

    #[test]
    fn build_container_style_switches_flex_only_for_the_csd_mode() {
        let t = tb("x");

        let block = t.build_container_style(false);
        let flex = t.build_container_style(true);

        assert!(properties(&block).contains(&CssProperty::const_display(LayoutDisplay::Block)));
        assert!(properties(&flex).contains(&CssProperty::const_display(LayoutDisplay::Flex)));
        // Title-only mode must *not* declare flex layout — the doc comment says it
        // deliberately avoids flex-grow complexity.
        assert!(
            !properties(&block).iter().any(|p| matches!(
                p,
                CssProperty::FlexDirection(_) | CssProperty::AlignItems(_)
            )),
            "title-only mode leaked flex declarations",
        );
        // Everything else is identical.
        assert_eq!(height_px(&block), height_px(&flex));
        assert_eq!(padding_left_px(&block), padding_left_px(&flex));
        assert_eq!(padding_right_px(&block), padding_right_px(&flex));
    }

    #[test]
    /// A CSD titlebar paints the DESKTOP's titlebar colour, not the window
    /// background — that is the whole point of carrying it in `SystemStyle`.
    /// Emitted only when the platform stated one, so an app that styles its
    /// own titlebar is unaffected and the historical `.with_css()` path still
    /// wins (a later declaration overrides an earlier one).
    ///
    /// NEGATIVE CONTROL: drop the `background_color` arm from
    /// `build_container_style` and the first assertion fails.
    #[test]
    fn build_container_style_paints_the_platform_titlebar_background() {
        use azul_css::props::basic::color::{ColorU, OptionColorU};

        let mut t = tb("x");
        t.background_color = OptionColorU::Some(ColorU::new_rgb(0x31, 0x36, 0x3b));
        let styled = t.build_container_style(true);
        assert!(
            styled.as_ref().iter().any(|p| matches!(
                p.property,
                azul_css::props::property::CssProperty::BackgroundContent(_)
            )),
            "a stated titlebar colour must reach the container's declarations"
        );

        let mut plain = tb("x");
        plain.background_color = OptionColorU::None;
        assert!(
            !plain
                .build_container_style(true)
                .as_ref()
                .iter()
                .any(|p| matches!(
                    p.property,
                    azul_css::props::property::CssProperty::BackgroundContent(_)
                )),
            "an unstated colour must declare NOTHING, so the caller keeps control"
        );
    }

    /// An unfocused window dims its titlebar on every desktop. `:backdrop` is
    /// the pseudo-class for exactly that state, so the dimmed colours ride the
    /// normal conditional-declaration path — no focus plumbing of their own.
    ///
    /// NEGATIVE CONTROL: drop the `background_inactive` arm and the first
    /// assertion fails; a decoration then stays "active"-coloured forever.
    #[test]
    fn build_container_style_dims_the_titlebar_when_the_window_is_unfocused() {
        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
        use azul_css::props::basic::color::{ColorU, OptionColorU};

        let mut t = tb("x");
        t.background_inactive = OptionColorU::Some(ColorU::new_rgb(0x2a, 0x2e, 0x32));
        let styled = t.build_container_style(true);
        assert!(
            styled.as_ref().iter().any(|p| {
                p.apply_if.as_ref().iter().any(|c| {
                    matches!(c, DynamicSelector::PseudoState(PseudoStateType::Backdrop))
                })
            }),
            "the unfocused colour must be declared under :backdrop"
        );

        let mut plain = tb("x");
        plain.background_inactive = OptionColorU::None;
        assert!(
            !plain.build_container_style(true).as_ref().iter().any(|p| {
                p.apply_if.as_ref().iter().any(|c| {
                    matches!(c, DynamicSelector::PseudoState(PseudoStateType::Backdrop))
                })
            }),
            "an unstated colour must declare NOTHING"
        );
    }

    /// CLOSE hovers in its OWN colour — red on Breeze and Windows alike —
    /// while the other controls take the neutral hover. One shared rule could
    /// not express that, which is why the hover style is per button.
    #[test]
    fn the_close_button_hovers_in_its_own_colour() {
        use azul_css::props::basic::color::{ColorU, OptionColorU};

        let neutral = ColorU::new_rgb(0x3d, 0xae, 0xe9);
        let red = ColorU::new_rgb(0xda, 0x44, 0x53);
        let dom = build_button_container(
            &TitlebarButtons::default(),
            OptionColorU::Some(neutral),
            OptionColorU::Some(red),
        );
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 3, "minimize + maximize + close");

        // Inline declarations live in `NodeData::style`; compare what each
        // button actually carries.
        let style_of = |node: &Dom| alloc::format!("{:?}", node.root.style);
        let (min_style, close_style) = (style_of(&kids[0]), style_of(&kids[2]));
        let hex = |c: ColorU| alloc::format!("r: {}, g: {}, b: {}", c.r, c.g, c.b);

        assert!(
            min_style.contains(&hex(neutral)),
            "minimize takes the neutral hover: {min_style}"
        );
        assert!(
            close_style.contains(&hex(red)),
            "close takes its OWN hover colour: {close_style}"
        );
        assert!(
            !close_style.contains(&hex(neutral)),
            "close must not also take the neutral hover"
        );

        // Nothing stated: nothing declared, so an app's own `.csd-button`
        // styling keeps full control.
        let bare = build_button_container(
            &TitlebarButtons::default(),
            OptionColorU::None,
            OptionColorU::None,
        );
        assert!(
            !style_of(&bare.children.as_ref()[2]).contains(&hex(red)),
            "an unstated hover must declare nothing"
        );
    }

    #[test]
    fn build_container_style_always_declares_the_grab_cursor_and_disables_selection() {
        // Without these a drag selects the title text instead of moving the window.
        for show_buttons in [false, true] {
            let style = tb("x").build_container_style(show_buttons);
            let props = properties(&style);
            assert!(props.contains(&CssProperty::const_cursor(StyleCursor::Grab)));
            assert!(props.contains(&CssProperty::user_select(StyleUserSelect::None)));
        }
    }

    #[test]
    fn build_container_style_truncates_the_height_toward_zero() {
        // `height as isize` truncates; a 30.9px titlebar is encoded as 30px.
        for (h, expected) in [
            (30.0_f32, 30.0_f32),
            (30.9, 30.0),
            (-30.9, -30.0),
            (0.0, 0.0),
            (-0.0, 0.0),
            (0.5, 0.0),
            (-0.5, 0.0),
            (0.999, 0.0),
        ] {
            let mut t = tb("x");
            t.set_height(h);
            assert_eq!(
                height_px(&t.build_container_style(false)),
                Some(expected),
                "height {h} encoded wrongly",
            );
        }
    }

    #[test]
    fn build_container_style_encodes_a_nan_height_as_zero_pixels() {
        // `NaN as isize` saturates to 0, so the encoding is defined rather than
        // propagating NaN into the layout solver.
        let mut t = tb("x");
        t.set_height(f32::NAN);
        assert_eq!(height_px(&t.build_container_style(false)), Some(0.0));
        assert_eq!(height_px(&t.build_container_style(true)), Some(0.0));
    }

    #[test]
    fn build_container_style_omits_padding_that_is_not_strictly_positive() {
        for pad in [0.0_f32, -0.0, -1.0, -1e30, f32::NAN, f32::NEG_INFINITY] {
            let mut t = tb("x");
            t.padding_left = pad;
            t.padding_right = pad;
            let style = t.build_container_style(false);
            assert_eq!(
                padding_left_px(&style),
                None,
                "padding-left {pad} must not be declared at all",
            );
            assert_eq!(
                padding_right_px(&style),
                None,
                "padding-right {pad} was declared"
            );
        }
    }

    #[test]
    fn build_container_style_emits_sub_pixel_padding_as_a_zero_px_declaration() {
        // The `> 0.0` gate lets 0.4px through, and `as isize` then truncates it to
        // 0px: the declaration exists but reserves nothing.
        let mut t = tb("x");
        t.padding_left = 0.4;
        t.padding_right = 0.6;
        let style = t.build_container_style(false);
        assert_eq!(padding_left_px(&style), Some(0.0));
        assert_eq!(padding_right_px(&style), Some(0.0));
    }

    #[test]
    fn build_container_style_keeps_the_two_paddings_independent() {
        let mut t = tb("x");
        t.padding_left = 12.0;
        t.padding_right = 0.0;
        let style = t.build_container_style(true);
        assert_eq!(padding_left_px(&style), Some(12.0));
        assert_eq!(padding_right_px(&style), None);
    }

    // ==================================================================
    // Titlebar::build_title_style
    // ==================================================================

    #[test]
    fn build_title_style_emits_the_documented_declarations_in_both_modes() {
        let t = tb("x");
        for show_buttons in [false, true] {
            let style = t.build_title_style(show_buttons);
            assert_eq!(
                properties(&style),
                expected_title(&t, show_buttons),
                "title declarations drifted (show_buttons = {show_buttons})",
            );
            assert!(
                all_unconditional(&style),
                "a title declaration became conditional"
            );
        }
    }

    #[test]
    fn build_title_style_only_grows_the_title_in_csd_mode() {
        // In the flex container the title must claim the space left by the buttons,
        // and `min-width: 0` is what lets it actually shrink below its text width.
        let t = tb("x");
        let flex = properties(&t.build_title_style(true));
        assert!(flex.contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))));
        assert!(flex.contains(&CssProperty::const_min_width(LayoutMinWidth::const_px(0))));

        let block = properties(&t.build_title_style(false));
        assert!(
            !block
                .iter()
                .any(|p| matches!(p, CssProperty::FlexGrow(_) | CssProperty::MinWidth(_))),
            "title-only mode leaked flex-grow / min-width",
        );
    }

    #[test]
    fn build_title_style_always_centres_clips_and_never_wraps() {
        for show_buttons in [false, true] {
            let props = properties(&tb("x").build_title_style(show_buttons));
            assert!(props.contains(&CssProperty::const_text_align(StyleTextAlign::Center)));
            assert!(
                props.contains(&CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(
                    StyleWhiteSpace::Nowrap
                )))
            );
            assert!(props.contains(&CssProperty::const_overflow_x(LayoutOverflow::Hidden)));
        }
    }

    #[test]
    fn build_title_style_forwards_the_resolved_title_colour_verbatim() {
        for c in [
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 0,
            },
            ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
            ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            DEFAULT_TITLE_COLOR_DARK,
        ] {
            let mut t = tb("x");
            t.title_color = c;
            assert_eq!(text_color(&t.build_title_style(false)), Some(c));
            assert_eq!(text_color(&t.build_title_style(true)), Some(c));
        }
    }

    #[test]
    fn build_title_style_centres_vertically_with_half_the_leftover_height() {
        for (h, fs, expected) in [
            (30.0_f32, 13.0_f32, Some(8.0_f32)), // (30-13)/2 = 8.5 -> 8px
            (32.0, 12.0, Some(10.0)),
            (40.0, 20.0, Some(10.0)),
            (14.0, 13.0, Some(0.0)), // 0.5 -> declared, but 0px
        ] {
            let mut t = tb("x");
            t.set_height(h);
            t.font_size = fs;
            assert_eq!(
                padding_top_px(&t.build_title_style(false)),
                expected,
                "h={h} fs={fs} produced the wrong vertical padding",
            );
        }
    }

    #[test]
    fn build_title_style_omits_the_vertical_padding_when_the_text_does_not_fit() {
        // `.max(0.0)` must swallow the negative gap: a negative padding-top would
        // push the title above the titlebar.
        for (h, fs) in [
            (13.0_f32, 13.0_f32),
            (10.0, 20.0),
            (0.0, 13.0),
            (-100.0, 13.0),
            (f32::NEG_INFINITY, 13.0),
            (f32::NAN, 13.0),
            (13.0, f32::NAN),
        ] {
            let mut t = tb("x");
            t.set_height(h);
            t.font_size = fs;
            assert_eq!(
                padding_top_px(&t.build_title_style(false)),
                None,
                "h={h} fs={fs} declared a vertical padding it should have clamped away",
            );
        }
    }

    #[test]
    fn build_title_style_encodes_a_nan_font_size_as_zero_pixels() {
        let mut t = tb("x");
        t.font_size = f32::NAN;
        assert_eq!(font_size_px(&t.build_title_style(false)), Some(0.0));
    }

    #[test]
    fn build_title_style_truncates_the_font_size_toward_zero() {
        for (fs, expected) in [
            (13.0_f32, 13.0_f32),
            (13.9, 13.0),
            (0.5, 0.0),
            (-13.9, -13.0),
        ] {
            let mut t = tb("x");
            t.font_size = fs;
            assert_eq!(font_size_px(&t.build_title_style(false)), Some(expected));
        }
    }

    // ==================================================================
    // The fixed-point encoding boundary
    // ==================================================================

    #[cfg(panic = "unwind")]
    #[test]
    fn heights_outside_the_encodable_range_are_not_saturated() {
        use std::{
            hint::black_box,
            panic::{catch_unwind, AssertUnwindSafe},
        };

        // LATENT BUG, pinned: `PixelValue::const_px` multiplies by 1000 with a
        // plain `*`, so any height/font-size whose `as isize` truncation exceeds
        // `isize::MAX / 1000` either panics (overflow checks on: a debug build
        // dies) or wraps to a garbage length (checks off) — it never saturates.
        // Asserted against a probe of the *current* profile so the test is
        // profile-independent; adding saturation flips it loudly.
        let profile_traps_overflow = catch_unwind(AssertUnwindSafe(|| {
            let big = black_box(isize::MAX);
            let _ = black_box(big * black_box(1000_isize));
        }))
        .is_err();

        for bogus in UNENCODABLE_FLOATS {
            let mut t = tb("x");
            t.set_height(bogus);
            let panicked =
                catch_unwind(AssertUnwindSafe(|| drop(t.build_container_style(false)))).is_err();
            assert_eq!(
                panicked, profile_traps_overflow,
                "height {bogus}: the fixed-point encoding no longer behaves like a raw multiply",
            );

            let mut f = tb("x");
            f.font_size = bogus;
            let panicked =
                catch_unwind(AssertUnwindSafe(|| drop(f.build_title_style(false)))).is_err();
            assert_eq!(
                panicked, profile_traps_overflow,
                "font size {bogus}: the fixed-point encoding no longer behaves like a raw multiply",
            );
        }
    }

    #[cfg(panic = "unwind")]
    #[test]
    fn an_unencodable_vertical_gap_reaches_the_padding_encoder_unclamped() {
        use std::{
            hint::black_box,
            panic::{catch_unwind, AssertUnwindSafe},
        };

        let profile_traps_overflow = catch_unwind(AssertUnwindSafe(|| {
            let big = black_box(isize::MAX);
            let _ = black_box(big * black_box(1000_isize));
        }))
        .is_err();

        // A *positive* unencodable height also blows up through `padding-top`,
        // because `(h - fs) / 2` is still unencodable. The negative ones are
        // clamped away by `.max(0.0)` and are therefore safe — asserted here so
        // the asymmetry is not mistaken for full coverage.
        for bogus in [f32::INFINITY, f32::MAX] {
            let mut t = tb("x");
            t.set_height(bogus);
            let panicked =
                catch_unwind(AssertUnwindSafe(|| drop(t.build_title_style(false)))).is_err();
            assert_eq!(
                panicked, profile_traps_overflow,
                "height {bogus} via padding-top"
            );
        }

        for safe in [f32::NEG_INFINITY, f32::MIN] {
            let mut t = tb("x");
            t.set_height(safe);
            assert_eq!(
                padding_top_px(&t.build_title_style(false)),
                None,
                "height {safe} must be clamped away by .max(0.0)",
            );
        }
    }

    // ==================================================================
    // Titlebar::dom (title-only)
    // ==================================================================

    #[test]
    fn dom_builds_the_documented_title_only_tree() {
        let dom = tb("caption").dom();

        assert_eq!(
            classes(&dom),
            vec!["csd-titlebar", "__azul-native-titlebar"]
        );
        assert!(ids(&dom).is_empty(), "the container must not claim an id");
        assert_eq!(
            dom.children.as_ref().len(),
            1,
            "title-only mode has exactly one child"
        );

        let title = title_node(&dom);
        assert_eq!(classes(title), vec!["csd-title"]);
        assert_eq!(title.children.as_ref().len(), 1);
        assert_eq!(text_of(&title.children.as_ref()[0]), Some("caption"));
        assert!(
            buttons_node(&dom).is_none(),
            "title-only mode must render no buttons"
        );
    }

    #[test]
    fn dom_puts_the_container_and_title_styles_on_the_right_nodes() {
        let t = tb("caption");
        let dom = t.clone().dom();

        assert_eq!(inline_props(&dom), expected_container(&t, false));
        assert_eq!(inline_props(title_node(&dom)), expected_title(&t, false));
        // The text node itself carries no styling of its own.
        assert!(inline_props(&title_node(&dom).children.as_ref()[0]).is_empty());
    }

    #[test]
    fn dom_registers_exactly_the_three_drag_callbacks_on_the_title_node() {
        let dom = tb("x").dom();

        assert!(
            callbacks_of(&dom).is_empty(),
            "the container must carry no callbacks — the title node owns the drag",
        );
        assert_eq!(
            callbacks_of(title_node(&dom)),
            vec![
                (
                    EventFilter::Hover(HoverEventFilter::DragStart),
                    callbacks::titlebar_drag_start as usize,
                ),
                (
                    EventFilter::Hover(HoverEventFilter::Drag),
                    callbacks::titlebar_drag as usize
                ),
                (
                    EventFilter::Hover(HoverEventFilter::DoubleClick),
                    callbacks::titlebar_double_click as usize,
                ),
            ],
        );
    }

    #[test]
    fn dom_carries_pathological_titles_into_the_text_node_verbatim() {
        for title in ADVERSARIAL_TITLES {
            let dom = tb(title).dom();
            let text = &title_node(&dom).children.as_ref()[0];
            assert_eq!(
                text_of(text),
                Some(title),
                "the title was mangled on the way in"
            );
            // Even an empty title still gets a text node, so the drag target exists.
            assert_eq!(title_node(&dom).children.as_ref().len(), 1);
        }
    }

    #[test]
    fn dom_keeps_the_cached_child_count_in_sync() {
        // A too-small `estimated_total_children` makes `convert_dom_into_compact_dom`
        // under-allocate and panic on an out-of-bounds write.
        let dom = tb("x").dom();
        assert_eq!(dom.estimated_total_children, count_descendants(&dom));
        assert_eq!(
            dom.estimated_total_children, 3,
            "title div + label <p> + text node"
        );
    }

    #[test]
    fn the_dom_conversion_is_exactly_dom() {
        for title in ADVERSARIAL_TITLES {
            let via_from: Dom = tb(title).into();
            assert_eq!(
                fingerprint(&via_from),
                fingerprint(&tb(title).dom()),
                "From<Titlebar> for Dom drifted away from Titlebar::dom",
            );
        }
    }

    // ==================================================================
    // Titlebar::dom_with_buttons / build_button_container
    // ==================================================================

    #[test]
    fn dom_with_buttons_orders_the_children_by_button_side() {
        let buttons = TitlebarButtons::default();

        let left = tb("x").dom_with_buttons(&buttons, TitlebarButtonSide::Left);
        let left_kids = left.children.as_ref();
        assert_eq!(left_kids.len(), 2);
        assert!(
            has_class(&left_kids[0], "csd-buttons"),
            "macOS puts the buttons first"
        );
        assert!(has_class(&left_kids[1], "csd-title"));

        let right = tb("x").dom_with_buttons(&buttons, TitlebarButtonSide::Right);
        let right_kids = right.children.as_ref();
        assert_eq!(right_kids.len(), 2);
        assert!(
            has_class(&right_kids[0], "csd-title"),
            "Windows/Linux put the title first"
        );
        assert!(has_class(&right_kids[1], "csd-buttons"));
    }

    #[test]
    fn dom_with_buttons_emits_one_node_per_enabled_button_in_minimize_maximize_close_order() {
        for buttons in all_button_combinations() {
            for side in BOTH_SIDES {
                let dom = tb("x").dom_with_buttons(&buttons, side);
                let container = buttons_node(&dom).expect("the CSD button container is mandatory");

                let mut expected: Vec<&str> = Vec::new();
                if buttons.has_minimize {
                    expected.push("csd-button-minimize");
                }
                if buttons.has_maximize {
                    expected.push("csd-button-maximize");
                }
                if buttons.has_close {
                    expected.push("csd-button-close");
                }

                let actual: Vec<String> =
                    container.children.as_ref().iter().flat_map(ids).collect();
                assert_eq!(
                    actual, expected,
                    "{buttons:?} on {side:?} produced the wrong buttons"
                );
            }
        }
    }

    #[test]
    fn has_fullscreen_is_never_rendered() {
        // The flag exists in `TitlebarButtons` but the widget has no fullscreen
        // button; toggling it must not change a single node.
        for &(close, min, max) in &[
            (true, true, true),
            (false, false, false),
            (true, false, true),
        ] {
            let off = TitlebarButtons {
                has_close: close,
                has_minimize: min,
                has_maximize: max,
                has_fullscreen: false,
            };
            let on = TitlebarButtons {
                has_fullscreen: true,
                ..off
            };
            assert_eq!(
                fingerprint(&build_button_container(&off, OptionColorU::None, OptionColorU::None)),
                fingerprint(&build_button_container(&on, OptionColorU::None, OptionColorU::None)),
                "has_fullscreen changed the rendered buttons",
            );
        }
    }

    #[test]
    fn all_buttons_disabled_still_emits_an_empty_button_container() {
        let none = TitlebarButtons {
            has_close: false,
            has_minimize: false,
            has_maximize: false,
            has_fullscreen: false,
        };
        let container = build_button_container(&none, OptionColorU::None, OptionColorU::None);

        assert_eq!(classes(&container), vec!["csd-buttons"]);
        assert!(container.children.as_ref().is_empty());
        assert_eq!(container.estimated_total_children, 0);

        // ... and the full DOM still has both children in the documented order.
        let dom = tb("x").dom_with_buttons(&none, TitlebarButtonSide::Right);
        assert_eq!(dom.children.as_ref().len(), 2);
        assert!(buttons_node(&dom).is_some());
    }

    #[test]
    fn every_button_carries_one_mousedown_callback_and_the_matching_icon() {
        // The icon SPEC is a fallback chain (`system:<freedesktop>,<material>`):
        // the desktop's own control icon where the session registered one, the
        // engine's glyph everywhere else. Asserted as a chain rather than as a
        // literal so the platform half can grow, while still pinning that each
        // button carries ITS icon and ends in the portable fallback.
        let expected: [(&str, &str, &str, usize); 3] = [
            (
                "csd-button-minimize",
                "system:window-minimize",
                "minimize",
                callbacks::csd_minimize as usize,
            ),
            (
                "csd-button-maximize",
                "system:window-maximize",
                "maximize",
                callbacks::csd_maximize as usize,
            ),
            (
                // The close button asks for the TITLEBAR glyph first: a
                // theme's `window-close` is the red circled X of the "close
                // document" ACTION, and a titlebar built from it reads as
                // permanently alarmed. The action icon stays in the chain
                // behind it, as the fallback for a session that provides no
                // titlebar glyph.
                "csd-button-close",
                "system:titlebar-close,system:window-close",
                "close",
                callbacks::csd_close as usize,
            ),
        ];

        let container = build_button_container(&TitlebarButtons::default(), OptionColorU::None, OptionColorU::None);
        let kids = container.children.as_ref();
        assert_eq!(kids.len(), 3);

        for (node, (id, native, fallback, cb)) in kids.iter().zip(expected) {
            assert_eq!(ids(node), vec![id]);
            assert_eq!(
                callbacks_of(node),
                vec![(EventFilter::Hover(HoverEventFilter::MouseDown), cb)],
                "{id} must carry exactly one MouseDown callback",
            );
            assert_eq!(node.children.as_ref().len(), 1);
            let spec = icon_of(&node.children.as_ref()[0])
                .unwrap_or_else(|| panic!("{id} rendered no icon at all"));
            assert_eq!(
                spec,
                alloc::format!("{native},{fallback}"),
                "{id} rendered the wrong icon",
            );
        }
    }

    #[test]
    fn every_button_carries_the_shared_and_the_specific_class() {
        let container = build_button_container(&TitlebarButtons::default(), OptionColorU::None, OptionColorU::None);
        for (node, specific) in
            container
                .children
                .as_ref()
                .iter()
                .zip(["csd-minimize", "csd-maximize", "csd-close"])
        {
            assert_eq!(
                classes(node),
                vec!["csd-button".to_string(), specific.to_string()],
                "the stylesheet hooks documented on Titlebar are missing",
            );
        }
    }

    #[test]
    fn dom_with_buttons_keeps_the_cached_child_count_in_sync_for_every_combination() {
        for buttons in all_button_combinations() {
            for side in BOTH_SIDES {
                let dom = tb("x").dom_with_buttons(&buttons, side);
                assert_eq!(
                    dom.estimated_total_children,
                    count_descendants(&dom),
                    "{buttons:?} on {side:?} desynced the cached child count",
                );

                let enabled = usize::from(buttons.has_close)
                    + usize::from(buttons.has_minimize)
                    + usize::from(buttons.has_maximize);
                // title + label <p> + text + button container
                // + 3 nodes per enabled button (button, icon, its glyph slot)
                assert_eq!(dom.estimated_total_children, 4 + 3 * enabled);
            }
        }
    }

    #[test]
    fn dom_with_buttons_uses_the_csd_container_and_title_styles() {
        let t = tb("x");
        let dom = t
            .clone()
            .dom_with_buttons(&TitlebarButtons::default(), TitlebarButtonSide::Right);

        assert_eq!(inline_props(&dom), expected_container(&t, true));
        assert_eq!(inline_props(title_node(&dom)), expected_title(&t, true));
        // The button container is styled entirely from the stylesheet.
        assert!(inline_props(buttons_node(&dom).unwrap()).is_empty());
    }

    #[test]
    fn the_button_side_changes_only_the_child_order() {
        let buttons = TitlebarButtons::default();
        let left = tb("x").dom_with_buttons(&buttons, TitlebarButtonSide::Left);
        let right = tb("x").dom_with_buttons(&buttons, TitlebarButtonSide::Right);

        assert_eq!(inline_props(&left), inline_props(&right));
        assert_eq!(classes(&left), classes(&right));
        assert_eq!(
            fingerprint(title_node(&left)),
            fingerprint(title_node(&right)),
            "the title node must not depend on the button side",
        );
        assert_eq!(
            fingerprint(buttons_node(&left).unwrap()),
            fingerprint(buttons_node(&right).unwrap()),
            "the button container must not depend on the button side",
        );
    }

    #[test]
    fn dom_with_buttons_carries_pathological_titles_verbatim() {
        for title in ADVERSARIAL_TITLES {
            let dom =
                tb(title).dom_with_buttons(&TitlebarButtons::default(), TitlebarButtonSide::Left);
            let text = &title_node(&dom).children.as_ref()[0];
            assert_eq!(text_of(text), Some(title));
        }
    }

    // ==================================================================
    // callbacks::titlebar_drag_start
    // ==================================================================

    #[test]
    fn drag_start_with_an_unknown_position_hands_the_move_to_the_compositor() {
        // Wayland hides the window position, so the *only* way to move is
        // `xdg_toplevel_move` — the manual loop would be a silent no-op there.
        let (update, changes) = with_callback_info(
            state_with(WindowFrame::Normal, WindowPosition::Uninitialized),
            |info| callbacks::titlebar_drag_start(RefAny::new(()), info),
        );

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            interactive_moves(&changes),
            1,
            "the compositor move was not requested"
        );
        assert!(
            state_writes(&changes).is_empty(),
            "the native path must not write state"
        );
    }

    #[test]
    fn drag_start_with_a_known_position_takes_the_platform_appropriate_path() {
        // macOS is documented to take the native path even with a known position
        // (performWindowDragWithEvent: is snap- and multi-monitor-aware); X11 and
        // Windows fall through to the manual per-event loop. `cfg!` rather than
        // `#[cfg]` so both branches keep type-checking on every target.
        let (update, changes) = with_callback_info(
            state_with(
                WindowFrame::Normal,
                WindowPosition::Initialized(PhysicalPositionI32::new(100, 200)),
            ),
            |info| callbacks::titlebar_drag_start(RefAny::new(()), info),
        );

        assert_eq!(update, Update::DoNothing);
        if cfg!(target_os = "macos") {
            assert_eq!(interactive_moves(&changes), 1);
        } else {
            assert_eq!(
                interactive_moves(&changes),
                0,
                "the manual path must not ask the OS"
            );
            assert!(
                changes.is_empty(),
                "a normal-frame manual drag start must record nothing at all",
            );
        }
    }

    #[test]
    fn drag_start_restores_a_maximized_window_before_the_manual_move() {
        // Dragging a maximized window has to un-maximize first, otherwise the
        // manual loop slides the still-maximized frame around the screen.
        let before = state_with(
            WindowFrame::Maximized,
            WindowPosition::Initialized(PhysicalPositionI32::new(0, 0)),
        );
        let (update, changes) = with_callback_info(before.clone(), |info| {
            callbacks::titlebar_drag_start(RefAny::new(()), info)
        });

        assert_eq!(update, Update::DoNothing);
        if cfg!(target_os = "macos") {
            assert_eq!(interactive_moves(&changes), 1);
            assert!(state_writes(&changes).is_empty());
        } else {
            let writes = state_writes(&changes);
            assert_eq!(writes.len(), 1, "the un-maximize write is missing");
            assert_eq!(writes[0].flags.frame, WindowFrame::Normal);

            // Nothing else may be touched on the way through.
            let mut expected = before;
            expected.flags.frame = WindowFrame::Normal;
            assert_eq!(
                writes[0], expected,
                "drag start changed more than the frame"
            );
        }
    }

    #[test]
    fn drag_start_leaves_a_fullscreen_or_minimized_frame_alone() {
        for frame in [
            WindowFrame::Fullscreen,
            WindowFrame::Minimized,
            WindowFrame::Normal,
        ] {
            let (_, changes) = with_callback_info(
                state_with(
                    frame,
                    WindowPosition::Initialized(PhysicalPositionI32::new(1, 1)),
                ),
                |info| callbacks::titlebar_drag_start(RefAny::new(()), info),
            );
            if !cfg!(target_os = "macos") {
                assert!(
                    state_writes(&changes).is_empty(),
                    "{frame:?} must not be rewritten — only Maximized is restored",
                );
            }
        }
    }

    // ==================================================================
    // callbacks::titlebar_drag
    // ==================================================================

    #[test]
    fn drag_without_an_active_gesture_is_a_no_op() {
        // No drag is in flight, so `get_drag_delta_screen_incremental()` is None and
        // the if-let must not match — a callback that moved the window anyway would
        // teleport it on the first stray Drag event.
        for position in [
            WindowPosition::Uninitialized,
            WindowPosition::Initialized(PhysicalPositionI32::new(-5, 7)),
        ] {
            let (update, changes) =
                with_callback_info(state_with(WindowFrame::Normal, position), |info| {
                    callbacks::titlebar_drag(RefAny::new(()), info)
                });
            assert_eq!(update, Update::DoNothing);
            assert!(
                changes.is_empty(),
                "{position:?}: a no-delta drag recorded a change"
            );
        }
    }

    #[test]
    fn drag_is_idempotent_when_repeated_without_a_gesture() {
        for _ in 0..4 {
            let (update, changes) = with_callback_info(
                state_with(
                    WindowFrame::Maximized,
                    WindowPosition::Initialized(PhysicalPositionI32::new(i32::MAX, i32::MIN)),
                ),
                |info| callbacks::titlebar_drag(RefAny::new(()), info),
            );
            assert_eq!(update, Update::DoNothing);
            // Extreme coordinates must not tempt the callback into arithmetic it
            // was never asked to do.
            assert!(changes.is_empty());
        }
    }

    // ==================================================================
    // callbacks::titlebar_double_click / csd_maximize
    // ==================================================================

    #[test]
    fn double_click_toggles_maximized_and_normalises_every_other_frame() {
        for frame in ALL_FRAMES {
            let before = state_with(frame, WindowPosition::Uninitialized);
            let (update, changes) = with_callback_info(before.clone(), |info| {
                callbacks::titlebar_double_click(RefAny::new(()), info)
            });

            assert_eq!(update, Update::DoNothing);
            let writes = state_writes(&changes);
            assert_eq!(
                writes.len(),
                1,
                "{frame:?}: exactly one state write expected"
            );

            let expected_frame = if frame == WindowFrame::Maximized {
                WindowFrame::Normal
            } else {
                WindowFrame::Maximized
            };
            assert_eq!(
                writes[0].flags.frame, expected_frame,
                "{frame:?} toggled wrongly"
            );

            let mut expected = before;
            expected.flags.frame = expected_frame;
            assert_eq!(
                writes[0], expected,
                "{frame:?}: more than the frame changed"
            );
        }
    }

    #[test]
    fn double_clicking_twice_returns_to_the_original_frame() {
        let (_, changes) = with_callback_info(
            state_with(WindowFrame::Normal, WindowPosition::Uninitialized),
            |info| callbacks::titlebar_double_click(RefAny::new(()), info),
        );
        let once = state_writes(&changes).remove(0);
        assert_eq!(once.flags.frame, WindowFrame::Maximized);

        let (_, changes) = with_callback_info(once, |info| {
            callbacks::titlebar_double_click(RefAny::new(()), info)
        });
        assert_eq!(state_writes(&changes)[0].flags.frame, WindowFrame::Normal);
    }

    #[test]
    fn the_maximize_button_agrees_with_the_double_click_for_every_frame() {
        for frame in ALL_FRAMES {
            let before = state_with(frame, WindowPosition::Uninitialized);
            let (_, via_button) = with_callback_info(before.clone(), |info| {
                callbacks::csd_maximize(RefAny::new(()), info)
            });
            let (_, via_double) = with_callback_info(before, |info| {
                callbacks::titlebar_double_click(RefAny::new(()), info)
            });
            assert_eq!(
                state_writes(&via_button),
                state_writes(&via_double),
                "{frame:?}: the maximize button and the double-click diverged",
            );
        }
    }

    // ==================================================================
    // callbacks::csd_close / csd_minimize
    // ==================================================================

    #[test]
    fn close_sets_close_requested_and_nothing_else() {
        for frame in ALL_FRAMES {
            let before = state_with(frame, WindowPosition::Uninitialized);
            assert!(
                !before.flags.close_requested,
                "fixture must start un-closed"
            );

            let (update, changes) = with_callback_info(before.clone(), |info| {
                callbacks::csd_close(RefAny::new(()), info)
            });

            assert_eq!(update, Update::DoNothing);
            let writes = state_writes(&changes);
            assert_eq!(writes.len(), 1);
            assert!(writes[0].flags.close_requested);
            assert_eq!(
                writes[0].flags.frame, frame,
                "close must not move the frame"
            );

            let mut expected = before;
            expected.flags.close_requested = true;
            assert_eq!(
                writes[0], expected,
                "close changed more than close_requested"
            );
        }
    }

    #[test]
    fn close_is_idempotent_on_an_already_closing_window() {
        let mut before = state_with(WindowFrame::Normal, WindowPosition::Uninitialized);
        before.flags.close_requested = true;

        let (_, changes) = with_callback_info(before.clone(), |info| {
            callbacks::csd_close(RefAny::new(()), info)
        });

        assert_eq!(
            state_writes(&changes),
            vec![before],
            "a second close must be a re-assert"
        );
    }

    #[test]
    fn minimize_always_minimizes_regardless_of_the_current_frame() {
        for frame in ALL_FRAMES {
            let before = state_with(frame, WindowPosition::Uninitialized);
            let (update, changes) = with_callback_info(before.clone(), |info| {
                callbacks::csd_minimize(RefAny::new(()), info)
            });

            assert_eq!(update, Update::DoNothing);
            let writes = state_writes(&changes);
            assert_eq!(writes.len(), 1);
            assert_eq!(
                writes[0].flags.frame,
                WindowFrame::Minimized,
                "{frame:?} was not minimized"
            );

            let mut expected = before;
            expected.flags.frame = WindowFrame::Minimized;
            assert_eq!(
                writes[0], expected,
                "{frame:?}: minimize changed more than the frame"
            );
        }
    }

    #[test]
    fn minimize_never_requests_a_close() {
        let (_, changes) = with_callback_info(
            state_with(WindowFrame::Normal, WindowPosition::Uninitialized),
            |info| callbacks::csd_minimize(RefAny::new(()), info),
        );
        assert!(!state_writes(&changes)[0].flags.close_requested);
        assert_eq!(interactive_moves(&changes), 0);
    }
}

/// Make any `Dom` behave as a window-drag region — azul's answer to Electron's
/// `-webkit-app-region: drag`.
///
/// Dragging anywhere inside the returned node moves the WINDOW, and
/// double-clicking it toggles maximize/restore, exactly as a native title bar
/// does. That is what lets an application turn decorations off and draw its own
/// title bar without losing either behaviour:
///
/// ```ignore
/// window.window_state.flags.decorations = WindowDecorations::None;
/// // ...then, in layout():
/// let bar = Dom::create_div()
///     .with_css("height: 38px; background: #1f1f28;")
///     .with_child(Dom::create_span_with_text("My Document"));
/// window_drag_region(bar)
/// ```
///
/// It reuses the three callbacks the built-in `Titlebar` already uses, rather
/// than a parallel implementation: `DragStart` begins a native interactive move
/// where the platform offers one (Wayland has no other way — the compositor
/// hides the window position), `Drag` carries the fallback path for X11 and
/// Windows, and `DoubleClick` toggles the frame.
///
/// INTERACTIVE CHILDREN STILL WORK. A button inside the region keeps its own
/// callbacks; this attaches to the node you pass, and a child that handles the
/// event first is unaffected — the same way a title bar's close button is not
/// swallowed by the bar behind it.
#[must_use]
pub fn window_drag_region(dom: Dom) -> Dom {
    use azul_core::{
        callbacks::CoreCallbackData,
        dom::{EventFilter, HoverEventFilter},
        refany::{OptionRefAny, RefAny},
    };

    dom.with_callbacks(
        alloc::vec![
            CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::DragStart),
                callback: azul_core::callbacks::CoreCallback {
                    cb: callbacks::titlebar_drag_start as usize,
                    ctx: OptionRefAny::None,
                },
                refany: RefAny::new(()),
            },
            CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::Drag),
                callback: azul_core::callbacks::CoreCallback {
                    cb: callbacks::titlebar_drag as usize,
                    ctx: OptionRefAny::None,
                },
                refany: RefAny::new(()),
            },
            CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                callback: azul_core::callbacks::CoreCallback {
                    cb: callbacks::titlebar_double_click as usize,
                    ctx: OptionRefAny::None,
                },
                refany: RefAny::new(()),
            },
        ]
        .into(),
    )
}

/// F11 toggles fullscreen; Escape leaves it. Call from a key-up callback.
///
/// Returns `true` when it consumed the key, so a caller can fall through to its
/// own handling otherwise:
///
/// ```ignore
/// extern "C" fn on_key(_: RefAny, mut info: CallbackInfo) -> Update {
///     if handle_fullscreen_keys(&mut info) { return Update::DoNothing; }
///     // ... the app's own shortcuts
/// }
/// ```
///
/// Why this exists next to `window_drag_region`: an application that turns
/// decorations off to draw its own title bar also loses the window manager's
/// fullscreen affordance, and F11 is what users reach for. Escape only leaves
/// fullscreen — it never minimizes or closes, because a stray Escape in a text
/// field must not throw the window away.
#[must_use]
pub fn handle_fullscreen_keys(info: &mut crate::callbacks::CallbackInfo) -> bool {
    use azul_core::window::{VirtualKeyCode, WindowFrame};

    let ws = info.get_current_window_state();
    let Some(key) = ws.keyboard_state.current_virtual_keycode.into_option() else {
        return false;
    };

    let is_fullscreen = ws.flags.frame == WindowFrame::Fullscreen;
    let next = match key {
        VirtualKeyCode::F11 => {
            if is_fullscreen {
                // Restore to MAXIMIZED rather than Normal: a window that was
                // maximized before going fullscreen should come back maximized,
                // and one that was not is only a click away from either.
                WindowFrame::Maximized
            } else {
                WindowFrame::Fullscreen
            }
        }
        VirtualKeyCode::Escape if is_fullscreen => WindowFrame::Maximized,
        _ => return false,
    };

    let mut s = ws.clone();
    s.flags.frame = next;
    info.modify_window_state(s);
    true
}

#[cfg(test)]
mod drag_region_tests {
    use super::*;
    use azul_core::dom::{Dom, EventFilter, HoverEventFilter};

    /// The region must carry all THREE behaviours a native title bar has.
    /// Missing any one is a bar that looks right and behaves wrong: no drag, a
    /// drag that starts and never moves, or no double-click-to-maximize.
    #[test]
    fn a_drag_region_carries_drag_start_drag_and_double_click() {
        let dom = window_drag_region(Dom::create_div());
        let events: Vec<EventFilter> = dom
            .root
            .callbacks
            .as_ref()
            .iter()
            .map(|c| c.event)
            .collect();

        for want in [
            HoverEventFilter::DragStart,
            HoverEventFilter::Drag,
            HoverEventFilter::DoubleClick,
        ] {
            assert!(
                events.contains(&EventFilter::Hover(want)),
                "a window-drag region must handle {want:?}; it has {events:?}"
            );
        }
    }

    /// It must not disturb what the caller already put on the node — an app
    /// bar has its own buttons and its own styling.
    #[test]
    fn a_drag_region_keeps_the_callers_children_and_classes() {
        let inner = Dom::create_span_with_text("My Document");
        let dom = window_drag_region(
            Dom::create_div()
                .with_ids_and_classes(
                    vec![azul_core::dom::IdOrClass::Class("app-bar".into())].into(),
                )
                .with_child(inner),
        );
        assert_eq!(dom.children.as_ref().len(), 1, "the child must survive");
        assert!(
            dom.root.get_ids_and_classes().iter().any(
                |c| matches!(c, azul_core::dom::IdOrClass::Class(s) if s.as_str() == "app-bar")
            ),
            "the caller's class must survive"
        );
    }
}

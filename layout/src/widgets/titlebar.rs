use azul_core::{
    dom::{Dom, DomVec, IdOrClass, IdOrClass::Class, IdOrClass::Id, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::{
            color::{ColorU, ColorOrSystem, SystemColorRef},
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
// Windows 10/11: 3 buttons × 46px = 138px
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
const DEFAULT_TITLE_COLOR_LIGHT: ColorU = ColorU { r: 76, g: 76, b: 76, a: 255 };  // #4c4c4c
const DEFAULT_TITLE_COLOR_DARK: ColorU = ColorU { r: 229, g: 229, b: 229, a: 255 }; // #e5e5e5

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
///    that calls `CallbackInfo::modify_window_state()` — exactly the same
///    mechanism used for window dragging.  No special event-system hooks.
///
/// Window-control buttons use `Dom::create_icon("close")` etc. so that
/// icons are resolved through the icon provider system (Material Icons
/// by default) and can be swapped out by registering a different icon pack.
///
/// # Button layout
///
/// `button_side` controls where the buttons appear:
/// - `Left` — macOS traffic-light style (buttons before title)
/// - `Right` — Windows / Linux style (title then buttons)
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
}

impl Titlebar {
    /// Create a titlebar with compile-time platform defaults.
    ///
    /// Use [`Titlebar::from_system_style`] when you have a
    /// `SystemStyle` available for pixel-perfect metrics.
    #[inline]
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
        }
    }

    /// FFI-compatible alias for [`Titlebar::new`].
    #[inline]
    pub fn create(title: AzString) -> Self {
        Self::new(title)
    }

    /// Create a titlebar with a custom height.
    #[inline]
    pub fn with_height(title: AzString, height: f32) -> Self {
        let mut tb = Self::new(title);
        tb.height = height;
        tb
    }

    /// Set the titlebar height.
    #[inline]
    pub fn set_height(&mut self, height: f32) {
        self.height = height;
    }

    /// Set the title text.
    #[inline]
    pub fn set_title(&mut self, title: AzString) {
        self.title = title;
    }

    /// Swap this titlebar with a default instance, returning the old value.
    #[inline]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Titlebar::new(AzString::from_const_str(""));
        core::mem::swap(&mut s, self);
        s
    }

    /// Create from a live [`SystemStyle`] (for title-only mode, padding
    /// reserves space for OS-drawn buttons).
    pub fn from_system_style(title: AzString, system_style: &SystemStyle) -> Self {
        let tm = &system_style.metrics.titlebar;
        let height = tm.height.as_ref()
            .map(|pv| pv.to_pixels_internal(0.0, 0.0))
            .unwrap_or(DEFAULT_TITLEBAR_HEIGHT);
        let font_size = tm.title_font_size
            .into_option()
            .unwrap_or(DEFAULT_TITLE_FONT_SIZE);
        let button_area = tm.button_area_width.as_ref()
            .map(|pv| pv.to_pixels_internal(0.0, 0.0))
            .unwrap_or(DEFAULT_BUTTON_AREA_WIDTH);
        let safe_left = tm.safe_area.left.as_ref()
            .map(|pv| pv.to_pixels_internal(0.0, 0.0))
            .unwrap_or(0.0);
        let safe_right = tm.safe_area.right.as_ref()
            .map(|pv| pv.to_pixels_internal(0.0, 0.0))
            .unwrap_or(0.0);
        // Bug 9: apply padding_horizontal from TitlebarMetrics
        let pad_h = tm.padding_horizontal.as_ref()
            .map(|pv| pv.to_pixels_internal(0.0, 0.0))
            .unwrap_or(0.0);

        // Equal padding on both sides so text-align:center stays at the window midpoint.
        // button_area/2 on each side: the button-side half clears the traffic-lights/caption
        // buttons, the opposite half balances the centering offset.
        let half_btn = button_area / 2.0;
        let (padding_left, padding_right) = (
            half_btn + safe_left + pad_h,
            half_btn + safe_right + pad_h,
        );

        // Bug 8: resolve title color from system style, with dark/light fallback
        let title_color = system_style.colors.text.into_option().unwrap_or(
            match system_style.theme {
                azul_css::system::Theme::Dark => DEFAULT_TITLE_COLOR_DARK,
                azul_css::system::Theme::Light => DEFAULT_TITLE_COLOR_LIGHT,
            }
        );

        Self { title, height, font_size, padding_left, padding_right, title_color }
    }

    /// Create from [`SystemStyle`] for **full CSD** mode (no padding — the
    /// buttons are rendered as DOM children).
    pub fn from_system_style_csd(title: AzString, system_style: &SystemStyle) -> Self {
        let tm = &system_style.metrics.titlebar;
        let height = tm.height.as_ref()
            .map(|pv| pv.to_pixels_internal(0.0, 0.0))
            .unwrap_or(DEFAULT_TITLEBAR_HEIGHT);
        let font_size = tm.title_font_size
            .into_option()
            .unwrap_or(DEFAULT_TITLE_FONT_SIZE);
        let title_color = system_style.colors.text.into_option().unwrap_or(
            match system_style.theme {
                azul_css::system::Theme::Dark => DEFAULT_TITLE_COLOR_DARK,
                azul_css::system::Theme::Light => DEFAULT_TITLE_COLOR_LIGHT,
            }
        );
        Self { title, height, font_size, padding_left: 0.0, padding_right: 0.0, title_color }
    }

    /// Build inline CSS for the container div.
    /// Build inline CSS for the container div.
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
            // Bug 12: avoids flex-grow complexity; text centers via text-align.
            props.push(CssPropertyWithConditions::simple(
                CssProperty::const_display(LayoutDisplay::Block),
            ));
        }
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_height(LayoutHeight::const_px(self.height as isize)),
        ));
        // Titlebar should show grab cursor and prevent text selection
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_cursor(StyleCursor::Grab),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::user_select(StyleUserSelect::None),
        ));
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
    fn build_title_style(&self, show_buttons: bool) -> CssPropertyWithConditionsVec {
        let font_family = StyleFontFamilyVec::from_vec(vec![
            StyleFontFamily::SystemType(SystemFontType::TitleBold),
        ]);
        let mut props = Vec::with_capacity(10);
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_size(StyleFontSize::const_px(self.font_size as isize)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_family(font_family),
        ));
        // Bug 8: use resolved title color from SystemStyle (adapts to dark mode)
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_text_color(StyleTextColor { inner: self.title_color }),
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
        props.push(CssPropertyWithConditions::simple(
            CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap)),
        ));
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
    pub fn dom(self) -> Dom {
        self.dom_inner(false, &TitlebarButtons::default(), TitlebarButtonSide::Right)
    }

    /// Full-CSD DOM with close / minimize / maximize buttons.
    ///
    /// Each button is a div with a `MouseDown` callback that calls
    /// `modify_window_state()` — no special hooks needed.
    pub fn dom_with_buttons(
        self,
        buttons: &TitlebarButtons,
        button_side: TitlebarButtonSide,
    ) -> Dom {
        self.dom_inner(true, buttons, button_side)
    }

    /// Inner builder for both modes.
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
            .with_child(Dom::create_text(self.title)) // moves self.title
            .with_callbacks(vec![
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DragStart),
                    callback: CoreCallback {
                        cb: self::callbacks::titlebar_drag_start as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(DragMarker),
                },
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Drag),
                    callback: CoreCallback {
                        cb: self::callbacks::titlebar_drag as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(DragMarker),
                },
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                    callback: CoreCallback {
                        cb: self::callbacks::titlebar_double_click as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(DragMarker),
                },
            ].into());

        // ── Button container (CSD mode only) ──
        let button_container = if show_buttons {
            Some(build_button_container(buttons))
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
                if let Some(btn) = button_container { root = root.with_child(btn); }
                root = root.with_child(title_node);
            }
            TitlebarButtonSide::Right => {
                root = root.with_child(title_node);
                if let Some(btn) = button_container { root = root.with_child(btn); }
            }
        }

        root
    }
}

/// Build the `.csd-buttons` container with close/min/max button DOM nodes.
fn build_button_container(buttons: &TitlebarButtons) -> Dom {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackData},
        dom::{EventFilter, HoverEventFilter},
    };

    let mut children = Vec::new();

    if buttons.has_minimize {
        let classes = IdOrClassVec::from_vec(vec![
            Id("csd-button-minimize".into()),
            Class("csd-button".into()),
            Class("csd-minimize".into()),
        ]);
        children.push(Dom::create_div()
            .with_ids_and_classes(classes)
            .with_child(Dom::create_icon("minimize"))
            .with_callbacks(vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseDown),
                callback: CoreCallback {
                    cb: self::callbacks::csd_minimize as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(()),
            }].into()));
    }

    if buttons.has_maximize {
        let classes = IdOrClassVec::from_vec(vec![
            Id("csd-button-maximize".into()),
            Class("csd-button".into()),
            Class("csd-maximize".into()),
        ]);
        children.push(Dom::create_div()
            .with_ids_and_classes(classes)
            .with_child(Dom::create_icon("maximize"))
            .with_callbacks(vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseDown),
                callback: CoreCallback {
                    cb: self::callbacks::csd_maximize as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(()),
            }].into()));
    }

    if buttons.has_close {
        let classes = IdOrClassVec::from_vec(vec![
            Id("csd-button-close".into()),
            Class("csd-button".into()),
            Class("csd-close".into()),
        ]);
        children.push(Dom::create_div()
            .with_ids_and_classes(classes)
            .with_child(Dom::create_icon("close"))
            .with_callbacks(vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseDown),
                callback: CoreCallback {
                    cb: self::callbacks::csd_close as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(()),
            }].into()));
    }

    let classes = IdOrClassVec::from_vec(vec![Class("csd-buttons".into())]);
    Dom::create_div()
        .with_ids_and_classes(classes)
        .with_children(DomVec::from_vec(children))
}

impl From<Titlebar> for Dom {
    fn from(t: Titlebar) -> Dom { t.dom() }
}

impl Default for Titlebar {
    fn default() -> Self {
        Titlebar::new(AzString::from_const_str(""))
    }
}

// ── Titlebar callbacks ───────────────────────────────────────────────────

/// All titlebar callbacks: drag, double-click, close, minimize, maximize.
///
/// Every callback is a plain `extern "C"` function that uses
/// `CallbackInfo::modify_window_state()`.  No special hooks needed.
pub(crate) mod callbacks {
    use azul_core::callbacks::Update;
    use azul_core::refany::RefAny;
    use crate::callbacks::CallbackInfo;

    /// DragStart — on Wayland, initiate compositor-managed move immediately.
    /// On other platforms, just acknowledge (movement happens in titlebar_drag).
    pub extern "C" fn titlebar_drag_start(
        _data: RefAny, mut info: CallbackInfo,
    ) -> Update {
        // On Wayland, window position is Uninitialized (compositor hides it).
        // We must use xdg_toplevel_move via begin_interactive_move().
        let ws = info.get_current_window_state();
        if matches!(ws.position, azul_core::window::WindowPosition::Uninitialized) {
            info.begin_interactive_move();
        }
        Update::DoNothing
    }

    /// Drag — apply incremental screen-space delta to the CURRENT window position.
    ///
    /// Uses `get_drag_delta_screen_incremental()` (frame-to-frame delta) instead of
    /// `get_drag_delta_screen()` (total delta since drag start). Combined with
    /// the current window position from the OS, this approach is robust against
    /// external position changes during the drag (DPI change, OS clamping,
    /// compositor resize).
    ///
    /// On Wayland: this is a no-op because the compositor manages the move
    /// (initiated by `begin_interactive_move()` in `titlebar_drag_start`).
    pub extern "C" fn titlebar_drag(
        _data: RefAny, mut info: CallbackInfo,
    ) -> Update {
        use azul_core::window::WindowPosition;
        use azul_core::geom::PhysicalPositionI32;

        let delta = info.get_drag_delta_screen_incremental();
        let current_pos = info.get_current_window_state().position;

        if let (azul_core::geom::OptionDragDelta::Some(d), WindowPosition::Initialized(pos)) = (delta, current_pos) {
            let new_pos = WindowPosition::Initialized(PhysicalPositionI32::new(
                pos.x + d.dx as i32,
                pos.y + d.dy as i32,
            ));
            let mut ws = info.get_current_window_state().clone();
            ws.position = new_pos;
            info.modify_window_state(ws);
        }
        // On Wayland: current_pos is Uninitialized, so the if-let doesn't match → no-op.
        Update::DoNothing
    }

    /// DoubleClick — toggle Maximized ↔ Normal.
    pub extern "C" fn titlebar_double_click(
        _data: RefAny, mut info: CallbackInfo,
    ) -> Update {
        use azul_core::window::WindowFrame;
        let mut s = info.get_current_window_state().clone();
        s.flags.frame = if s.flags.frame == WindowFrame::Maximized {
            WindowFrame::Normal } else { WindowFrame::Maximized };
        info.modify_window_state(s);
        Update::DoNothing
    }

    /// Close button — `close_requested = true`.
    pub extern "C" fn csd_close(
        _data: RefAny, mut info: CallbackInfo,
    ) -> Update {
        let mut s = info.get_current_window_state().clone();
        s.flags.close_requested = true;
        info.modify_window_state(s);
        Update::DoNothing
    }

    /// Minimize button — `frame = Minimized`.
    pub extern "C" fn csd_minimize(
        _data: RefAny, mut info: CallbackInfo,
    ) -> Update {
        use azul_core::window::WindowFrame;
        let mut s = info.get_current_window_state().clone();
        s.flags.frame = WindowFrame::Minimized;
        info.modify_window_state(s);
        Update::DoNothing
    }

    /// Maximize button — toggle Maximized ↔ Normal.
    pub extern "C" fn csd_maximize(
        _data: RefAny, mut info: CallbackInfo,
    ) -> Update {
        use azul_core::window::WindowFrame;
        let mut s = info.get_current_window_state().clone();
        s.flags.frame = if s.flags.frame == WindowFrame::Maximized {
            WindowFrame::Normal } else { WindowFrame::Maximized };
        info.modify_window_state(s);
        Update::DoNothing
    }
}


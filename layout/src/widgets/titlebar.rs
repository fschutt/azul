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

#[cfg(target_os = "macos")]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 78.0;
#[cfg(target_os = "windows")]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 138.0;
#[cfg(target_os = "linux")]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 100.0;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
const DEFAULT_BUTTON_AREA_WIDTH: f32 = 100.0;

#[cfg(target_os = "macos")]
const DEFAULT_BUTTON_SIDE_LEFT: bool = true;
#[cfg(not(target_os = "macos"))]
const DEFAULT_BUTTON_SIDE_LEFT: bool = false;

// ── SoftwareTitlebar ─────────────────────────────────────────────────────

/// A software-rendered titlebar with optional close / minimize / maximize
/// buttons, drag-to-move, and double-click-to-maximize.
///
/// # Two modes
///
/// 1. **Title-only** ([`SoftwareTitlebar::dom`], the default for
///    `WindowDecorations::NoTitleAutoInject`):
///    The OS still draws the native window-control buttons (traffic lights on
///    macOS, caption buttons on Windows).  The titlebar reserves
///    `padding_left` / `padding_right` so the title text doesn't overlap them.
///
/// 2. **Full CSD** ([`SoftwareTitlebar::dom_with_buttons`], used when
///    `WindowDecorations::None` + `has_decorations`):
///    The titlebar renders its own close / minimize / maximize buttons as
///    regular DOM nodes.  Each button carries a plain `MouseDown` callback
///    that calls `CallbackInfo::modify_window_state()` — exactly the same
///    mechanism used for window dragging.  No special event-system hooks.
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
pub struct SoftwareTitlebar {
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
}

impl SoftwareTitlebar {
    /// Create a titlebar with compile-time platform defaults.
    ///
    /// Use [`SoftwareTitlebar::from_system_style`] when you have a
    /// `SystemStyle` available for pixel-perfect metrics.
    #[inline]
    pub fn new(title: AzString) -> Self {
        let (padding_left, padding_right) = if DEFAULT_BUTTON_SIDE_LEFT {
            (DEFAULT_BUTTON_AREA_WIDTH, 0.0)
        } else {
            (0.0, DEFAULT_BUTTON_AREA_WIDTH)
        };
        Self {
            title,
            height: DEFAULT_TITLEBAR_HEIGHT,
            font_size: DEFAULT_TITLE_FONT_SIZE,
            padding_left,
            padding_right,
        }
    }

    /// FFI-compatible alias for [`SoftwareTitlebar::new`].
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
        let mut s = SoftwareTitlebar::new(AzString::from_const_str(""));
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

        let (padding_left, padding_right) = match tm.button_side {
            TitlebarButtonSide::Left => (button_area + safe_left, safe_right),
            TitlebarButtonSide::Right => (safe_left, button_area + safe_right),
        };

        Self { title, height, font_size, padding_left, padding_right }
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
        Self { title, height, font_size, padding_left: 0.0, padding_right: 0.0 }
    }

    /// Build inline CSS for the container div.
    fn build_container_style(&self) -> CssPropertyWithConditionsVec {
        let mut props = Vec::with_capacity(8);
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_display(LayoutDisplay::Flex),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_flex_direction(LayoutFlexDirection::Row),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_align_items(LayoutAlignItems::Center),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_height(LayoutHeight::const_px(self.height as isize)),
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
    fn build_title_style(&self) -> CssPropertyWithConditionsVec {
        let font_family = StyleFontFamilyVec::from_vec(vec![
            StyleFontFamily::SystemType(SystemFontType::TitleBold),
        ]);
        let mut props = Vec::with_capacity(8);
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_size(StyleFontSize::const_px(self.font_size as isize)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_family(font_family),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_text_align(StyleTextAlign::Center),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_overflow_x(LayoutOverflow::Hidden),
        ));
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
        let title_style = self.build_title_style();
        let container_style = self.build_container_style();

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
            .with_child(Dom::create_text("\u{2212}"))  // −
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
            .with_child(Dom::create_text("\u{25A1}"))  // □
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
            .with_child(Dom::create_text("\u{00D7}"))  // ×
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

impl From<SoftwareTitlebar> for Dom {
    fn from(t: SoftwareTitlebar) -> Dom { t.dom() }
}

impl Default for SoftwareTitlebar {
    fn default() -> Self { SoftwareTitlebar::new(AzString::from_const_str("")) }
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

    /// DragStart — framework tracks position, we just acknowledge.
    pub extern "C" fn titlebar_drag_start(
        _data: RefAny, _info: CallbackInfo,
    ) -> Update { Update::DoNothing }

    /// Drag — move window via gesture manager.
    pub extern "C" fn titlebar_drag(
        _data: RefAny, mut info: CallbackInfo,
    ) -> Update {
        let gm = info.get_gesture_drag_manager();
        if let Some(new_pos) = gm.get_window_position_from_drag() {
            let mut ws = info.get_current_window_state().clone();
            ws.position = new_pos;
            info.modify_window_state(ws);
        }
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


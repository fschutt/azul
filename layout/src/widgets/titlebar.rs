use azul_core::{
    dom::{Dom, IdOrClass, IdOrClass::Class, IdOrClassVec},
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
    system::{SystemFontType, SystemStyle, TitlebarButtonSide, TitlebarMetrics},
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

/// A software-rendered titlebar that reads layout metrics from [`TitlebarMetrics`]
/// to reserve the correct padding for the OS-drawn window-control buttons
/// (close / minimize / maximize / fullscreen).
///
/// # How it works
///
/// When `WindowDecorations::NoTitle` is active the operating system still draws
/// the "traffic-light" buttons (macOS) or the caption buttons (Windows) over
/// the content area. The application must avoid placing title text underneath
/// those buttons. `SoftwareTitlebar` does this by reading `TitlebarMetrics`
/// from the current `SystemStyle` and translating the values into
/// `padding-left` / `padding-right` on the container.
///
/// The title text uses:
/// - `white-space: nowrap` + `overflow: hidden` + `text-overflow: ellipsis`
///   so it never wraps to a second line.
/// - The system title font (`SystemFontType::TitleBold` on macOS,
///   `SystemFontType::Title` on other platforms) at the platform's standard
///   font-size (13 px on macOS, 12 px on Windows, …).
/// - `text-align: center` so the title appears centered inside the remaining
///   space (between left and right padding).
///
/// The container carries the CSS class `__azul-native-titlebar` which is
/// recognised by the event system for automatic window-drag activation on
/// `DragStart`.
///
/// # Example
///
/// ```rust,no_run
/// use azul_layout::widgets::SoftwareTitlebar;
///
/// // Without SystemStyle (uses compile-time defaults):
/// let tb = SoftwareTitlebar::new("My App".into());
///
/// // With SystemStyle (uses runtime-detected metrics):
/// // let tb = SoftwareTitlebar::from_system_style("My App".into(), &system_style);
///
/// let dom = tb.dom();
/// ```
#[derive(Debug, Clone)]
#[repr(C)]
pub struct SoftwareTitlebar {
    /// The title text to display.
    pub title: AzString,
    /// Height of the titlebar in CSS pixels.
    pub height: f32,
    /// Font size for the title text in CSS pixels.
    pub font_size: f32,
    /// Extra padding on the **left** side (px).
    /// Set to `button_area_width` when the buttons are on the left (macOS).
    pub padding_left: f32,
    /// Extra padding on the **right** side (px).
    /// Set to `button_area_width` when the buttons are on the right (Windows/Linux).
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

    /// Create a titlebar with a custom height (uses compile-time defaults for padding).
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

    /// Create a titlebar whose metrics come from a live [`SystemStyle`].
    ///
    /// This reads `system_style.metrics.titlebar` for:
    /// - `height`
    /// - `button_area_width`
    /// - `button_side`
    /// - `title_font_size`
    /// - `safe_area` (added to the respective padding side)
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

    /// Build the CSS properties for the container div.
    fn build_container_style(&self) -> CssPropertyWithConditionsVec {
        let mut props = Vec::with_capacity(8);

        // Flex row, center content
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_display(LayoutDisplay::Flex),
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

        // Fixed height
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_height(LayoutHeight::const_px(self.height as isize)),
        ));

        // Reserve space for the window-control buttons via padding
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

    /// Build the CSS properties for the title text node.
    fn build_title_style(&self) -> CssPropertyWithConditionsVec {
        let font_family = StyleFontFamilyVec::from_vec(vec![
            StyleFontFamily::SystemType(SystemFontType::TitleBold),
        ]);

        let mut props = Vec::with_capacity(6);

        // Font
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_size(StyleFontSize::const_px(self.font_size as isize)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_font_family(font_family),
        ));

        // Centre text, no line-break, clip overflow
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_text_align(StyleTextAlign::Center),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::WhiteSpace(StyleWhiteSpaceValue::Exact(StyleWhiteSpace::Nowrap)),
        ));
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_overflow_x(LayoutOverflow::Hidden),
        ));

        // Dark-grey text (works for light and dark titlebars with sidebar material)
        props.push(CssPropertyWithConditions::simple(
            CssProperty::const_text_color(StyleTextColor {
                inner: ColorU { r: 76, g: 76, b: 76, a: 255 },
            }),
        ));

        CssPropertyWithConditionsVec::from_vec(props)
    }

    /// Convert this titlebar into a DOM sub-tree.
    ///
    /// The root div carries class `__azul-native-titlebar`.
    /// Drag callbacks (DragStart / Drag / DoubleClick) are attached so that
    /// the user can move and maximize the window by interacting with the
    /// titlebar — no special event-system hooks required.
    #[inline]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter},
        };

        static TITLEBAR_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-titlebar"))];
        static TITLE_CLASS: &[IdOrClass] =
            &[Class(AzString::from_const_str("__azul-native-titlebar-title"))];

        let container_style = self.build_container_style();
        let title_style = self.build_title_style();

        let title_text = Dom::create_text(self.title)
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TITLE_CLASS))
            .with_css_props(title_style);

        #[derive(Debug, Clone, Copy)]
        struct TitlebarDragMarker;

        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TITLEBAR_CLASS))
            .with_css_props(container_style)
            .with_callbacks(
                vec![
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::DragStart),
                        callback: CoreCallback {
                            cb: self::drag::titlebar_drag_start as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(TitlebarDragMarker),
                    },
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Drag),
                        callback: CoreCallback {
                            cb: self::drag::titlebar_drag as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(TitlebarDragMarker),
                    },
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                        callback: CoreCallback {
                            cb: self::drag::titlebar_double_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(TitlebarDragMarker),
                    },
                ]
                .into(),
            )
            .with_child(title_text)
    }
}

impl From<SoftwareTitlebar> for Dom {
    fn from(t: SoftwareTitlebar) -> Dom {
        t.dom()
    }
}

impl Default for SoftwareTitlebar {
    fn default() -> Self {
        SoftwareTitlebar::new(AzString::from_const_str(""))
    }
}

// ── Drag / double-click callbacks ────────────────────────────────────────

/// Callback functions wired to the titlebar container so that the user can
/// drag-move and double-click-maximize the window.
///
/// These are plain `extern "C"` callbacks that operate through
/// `CallbackInfo::get_gesture_drag_manager()` and
/// `CallbackInfo::modify_window_state()` — no magic class detection needed.
mod drag {
    use azul_core::callbacks::Update;
    use azul_core::refany::RefAny;
    use crate::callbacks::CallbackInfo;

    /// Called on `HoverEventFilter::DragStart`.
    ///
    /// The gesture/drag manager already records the start position; we just
    /// return `DoNothing` to let the framework track the drag.
    pub(super) extern "C" fn titlebar_drag_start(
        _data: RefAny,
        _info: CallbackInfo,
    ) -> Update {
        Update::DoNothing
    }

    /// Called on `HoverEventFilter::Drag` (continuously while dragging).
    ///
    /// Reads the current drag delta from the gesture manager and moves the
    /// window accordingly via `modify_window_state`.
    pub(super) extern "C" fn titlebar_drag(
        _data: RefAny,
        mut info: CallbackInfo,
    ) -> Update {
        let gesture_manager = info.get_gesture_drag_manager();

        if let Some(new_position) = gesture_manager.get_window_position_from_drag() {
            let mut window_state = info.get_current_window_state().clone();
            window_state.position = new_position;
            info.modify_window_state(window_state);
        }

        Update::DoNothing
    }

    /// Called on `HoverEventFilter::DoubleClick`.
    ///
    /// Toggles the window between `Maximized` and `Normal`.
    pub(super) extern "C" fn titlebar_double_click(
        _data: RefAny,
        mut info: CallbackInfo,
    ) -> Update {
        use azul_core::window::WindowFrame;

        let mut state = info.get_current_window_state().clone();
        state.flags.frame = if state.flags.frame == WindowFrame::Maximized {
            WindowFrame::Normal
        } else {
            WindowFrame::Maximized
        };
        info.modify_window_state(state);
        Update::DoNothing
    }
}

// ── Backward-compatible alias ────────────────────────────────────────────

/// **Deprecated** – prefer [`SoftwareTitlebar`] which reads `TitlebarMetrics`
/// from `SystemStyle` for correct button-area padding.
pub type Titlebar = SoftwareTitlebar;

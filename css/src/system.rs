//! Discovers system-native styling for colors, fonts, and other metrics.
//!
//! This module provides a best-effort attempt to query the host operating system
//! for its UI theme information. This is gated behind the **`io`** feature flag.
//!
//! **Application-Specific Ricing:**
//! By default (if the `io` feature is enabled), Azul will look for an application-specific
//! stylesheet at `~/.config/azul/styles/<app_name>.css` (or `%APPDATA%\azul\styles\<app_name>.css`
//! on Windows). This allows end-users to override and "rice" any Azul application.
//! This behavior can be disabled by setting the `AZUL_DISABLE_RICING` environment variable.
//!
//! **Linux Customization Easter Egg:**
//! Linux users can set the `AZUL_SMOKE_AND_MIRRORS` environment variable to force Azul to
//! skip standard GNOME/KDE detection and prioritize discovery methods for "riced" desktops
//! (like parsing Hyprland configs or `pywal` caches), leaning into the car "ricing" subculture
//! where a flashy appearance is paramount.

#![cfg(feature = "parser")]

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
#[cfg(feature = "io")]
use core::time::Duration;

use crate::{
    corety::{AzString, OptionF32, OptionString, OptionU16},
    css::Stylesheet,
    parser2::{new_from_str, CssParseWarnMsg},
    props::{
        basic::{
            color::{parse_css_color, ColorU, OptionColorU},
            pixel::{PixelValue, OptionPixelValue},
        },
        style::scrollbar::{ComputedScrollbarStyle, OverscrollBehavior, ScrollBehavior, ScrollPhysics},
    },
};

// ── Native OS discovery via dlopen (feature = "system") ──────────────────

#[cfg(all(feature = "system", target_os = "macos"))]
#[path = "system_native_macos.rs"]
mod native_macos;

#[cfg(all(feature = "system", target_os = "windows"))]
#[path = "system_native_windows.rs"]
mod native_windows;

#[cfg(all(feature = "system", target_os = "linux"))]
#[path = "system_native_linux.rs"]
mod native_linux;

// --- Public Data Structures ---

/// Represents the detected platform.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum Platform {
    Windows,
    MacOs,
    Linux(DesktopEnvironment),
    Android,
    Ios,
    #[default]
    Unknown,
}

impl Platform {
    /// Get the current platform at compile time.
    #[inline]
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        { Platform::MacOs }
        #[cfg(target_os = "windows")]
        { Platform::Windows }
        #[cfg(target_os = "linux")]
        { Platform::Linux(DesktopEnvironment::Other(AzString::from_const_str("unknown"))) }
        #[cfg(target_os = "android")]
        { Platform::Android }
        #[cfg(target_os = "ios")]
        { Platform::Ios }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "android", target_os = "ios")))]
        { Platform::Unknown }
    }
}

/// Represents the detected Linux Desktop Environment.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum DesktopEnvironment {
    Gnome,
    Kde,
    Other(AzString),
}

/// The overall theme type.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum Theme {
    #[default]
    Light,
    Dark,
}

/// A unified collection of discovered system style properties.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct SystemStyle {
    pub fonts: SystemFonts,
    pub metrics: SystemMetrics,
    /// Linux-specific customisation (icon theme, cursor theme, GTK theme, ...)
    pub linux: LinuxCustomization,
    pub platform: Platform,
    /// Focus ring / indicator visual style
    pub focus_visuals: FocusVisuals,
    /// System language/locale in BCP 47 format (e.g., "en-US", "de-DE")
    /// Detected from OS settings at startup
    pub language: AzString,
    /// An optional, user-provided stylesheet loaded from a conventional
    /// location (`~/.config/azul/styles/<app_name>.css`), allowing for
    /// application-specific "ricing". This is only loaded when the "io"
    /// feature is enabled and not disabled by the `AZUL_DISABLE_RICING` env var.
    pub app_specific_stylesheet: Option<Box<Stylesheet>>,
    /// Scrollbar style information (boxed to ensure stable FFI size)
    pub scrollbar: Option<Box<ComputedScrollbarStyle>>,
    /// Global scroll physics configuration (momentum, friction, rubber-banding).
    /// Platform-specific defaults are applied during system style discovery.
    /// Applications can override this to change the "feel" of scrolling globally.
    pub scroll_physics: ScrollPhysics,
    pub theme: Theme,
    /// Detected OS version (e.g., Windows 11 22H2, macOS Sonoma, etc.)
    pub os_version: crate::dynamic_selector::OsVersion,
    /// User prefers reduced motion (accessibility setting)
    pub prefers_reduced_motion: crate::dynamic_selector::BoolCondition,
    /// User prefers high contrast (accessibility setting)
    pub prefers_high_contrast: crate::dynamic_selector::BoolCondition,
    /// Detailed accessibility settings (superset of prefers_reduced_motion / prefers_high_contrast)
    pub accessibility: AccessibilitySettings,
    /// Input interaction timing / distance thresholds from the OS
    pub input: InputMetrics,
    /// Text rendering / anti-aliasing hints from the OS
    pub text_rendering: TextRenderingHints,
    /// OS-level scrollbar visibility / click-behaviour preferences
    pub scrollbar_preferences: ScrollbarPreferences,
    /// Visual hints: icons in menus/buttons, toolbar style, tooltips
    pub visual_hints: VisualHints,
    /// Animation enable/disable, speed factor, focus indicator behaviour
    pub animation: AnimationMetrics,
    pub colors: SystemColors,
    /// Icon-specific styling options (grayscale, tinting, etc.)
    pub icon_style: IconStyleOptions,
    /// Audio feedback preferences (event sounds, input sounds)
    pub audio: AudioMetrics,
}

/// Icon-specific styling options for accessibility and theming.
///
/// These settings affect how icons are rendered, supporting accessibility
/// needs like reduced colors and high contrast modes.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct IconStyleOptions {
    /// If true, icons should be rendered in grayscale (for color-blind users
    /// or reduced color preference). Applies a CSS grayscale filter.
    pub prefer_grayscale: bool,
    /// Optional tint color to apply to icons. Useful for matching icons
    /// to the current theme or for high contrast modes.
    pub tint_color: OptionColorU,
    /// If true, icons should inherit the current text color instead of
    /// using their original colors. Works well with font-based icons.
    pub inherit_text_color: bool,
}

/// System font types that can be resolved at runtime based on OS settings.
/// 
/// This enum allows specifying semantic font roles that get resolved to
/// actual font families based on the current platform and user preferences.
/// For example, `Monospace` resolves to:
/// - macOS: SF Mono or Menlo
/// - Windows: Cascadia Mono or Consolas
/// - Linux: Ubuntu Mono or DejaVu Sans Mono
/// 
/// Font variants (bold, italic) can be combined with the base type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SystemFontType {
    /// UI font for buttons, labels, menus (SF Pro, Segoe UI, Cantarell)
    Ui,
    /// Bold variant of UI font
    UiBold,
    /// Monospace font for code (SF Mono, Consolas, Ubuntu Mono)
    Monospace,
    /// Bold variant of monospace font
    MonospaceBold,
    /// Italic variant of monospace font
    MonospaceItalic,
    /// Font for window titles
    Title,
    /// Bold variant of title font
    TitleBold,
    /// Font for menu items
    Menu,
    /// Small/caption font
    Small,
    /// Serif font for reading content (New York on macOS, Georgia on Windows)
    Serif,
    /// Bold variant of serif font
    SerifBold,
}

impl Default for SystemFontType {
    fn default() -> Self {
        SystemFontType::Ui
    }
}

impl SystemFontType {
    /// Parse a SystemFontType from a CSS string.
    /// 
    /// Supported formats:
    /// - `system:ui`, `system:ui:bold`
    /// - `system:monospace`, `system:monospace:bold`, `system:monospace:italic`
    /// - `system:title`, `system:title:bold`
    /// - `system:menu`
    /// - `system:small`
    /// - `system:serif`, `system:serif:bold`
    pub fn from_css_str(s: &str) -> Option<Self> {
        let s = s.trim();
        if !s.starts_with("system:") {
            return None;
        }
        let rest = &s[7..]; // Skip "system:"
        match rest {
            "ui" => Some(SystemFontType::Ui),
            "ui:bold" => Some(SystemFontType::UiBold),
            "monospace" => Some(SystemFontType::Monospace),
            "monospace:bold" => Some(SystemFontType::MonospaceBold),
            "monospace:italic" => Some(SystemFontType::MonospaceItalic),
            "title" => Some(SystemFontType::Title),
            "title:bold" => Some(SystemFontType::TitleBold),
            "menu" => Some(SystemFontType::Menu),
            "small" => Some(SystemFontType::Small),
            "serif" => Some(SystemFontType::Serif),
            "serif:bold" => Some(SystemFontType::SerifBold),
            _ => None,
        }
    }
    
    /// Get the CSS syntax for this system font type.
    pub fn as_css_str(&self) -> &'static str {
        match self {
            SystemFontType::Ui => "system:ui",
            SystemFontType::UiBold => "system:ui:bold",
            SystemFontType::Monospace => "system:monospace",
            SystemFontType::MonospaceBold => "system:monospace:bold",
            SystemFontType::MonospaceItalic => "system:monospace:italic",
            SystemFontType::Title => "system:title",
            SystemFontType::TitleBold => "system:title:bold",
            SystemFontType::Menu => "system:menu",
            SystemFontType::Small => "system:small",
            SystemFontType::Serif => "system:serif",
            SystemFontType::SerifBold => "system:serif:bold",
        }
    }
    
    /// Returns true if this system font type implies bold weight.
    /// Used when resolving system fonts to pass the correct weight to fontconfig.
    pub fn is_bold(&self) -> bool {
        matches!(
            self,
            SystemFontType::UiBold
                | SystemFontType::MonospaceBold
                | SystemFontType::TitleBold
                | SystemFontType::SerifBold
        )
    }
    
    /// Returns true if this system font type implies italic style.
    pub fn is_italic(&self) -> bool {
        matches!(self, SystemFontType::MonospaceItalic)
    }
}

/// Accessibility settings detected from the operating system.
/// 
/// These settings allow apps to adapt their UI for users with accessibility needs.
/// Detection methods:
/// - macOS: UIAccessibility APIs (isBoldTextEnabled, isReduceMotionEnabled, etc.)
/// - Windows: SystemParametersInfo (SPI_GETHIGHCONTRAST, SPI_GETCLIENTAREAANIMATION)
/// - Linux: gsettings (org.gnome.desktop.interface, org.gnome.desktop.a11y)
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct AccessibilitySettings {
    /// Text scaling factor (1.0 = normal, 1.5 = 150%, etc.)
    pub text_scale_factor: f32,
    /// User prefers bold text for better readability
    /// macOS: UIAccessibility.isBoldTextEnabled
    /// Windows: N/A (font scaling)
    /// Linux: org.gnome.desktop.interface text-scaling-factor
    pub prefers_bold_text: bool,
    /// User prefers larger text
    /// macOS: preferredContentSizeCategory
    /// Windows: SystemParametersInfo text scale factor
    /// Linux: org.gnome.desktop.interface text-scaling-factor
    pub prefers_larger_text: bool,
    /// User prefers high contrast colors
    /// macOS: UIAccessibility.isDarkerSystemColorsEnabled
    /// Windows: SPI_GETHIGHCONTRAST
    /// Linux: org.gnome.desktop.a11y.interface high-contrast
    pub prefers_high_contrast: bool,
    /// User prefers reduced motion/animations
    /// macOS: UIAccessibility.isReduceMotionEnabled
    /// Windows: SPI_GETCLIENTAREAANIMATION (inverted)
    /// Linux: org.gnome.desktop.interface enable-animations (inverted)
    pub prefers_reduced_motion: bool,
    /// User prefers reduced transparency
    /// macOS: UIAccessibility.isReduceTransparencyEnabled
    /// Windows: N/A
    /// Linux: N/A
    pub prefers_reduced_transparency: bool,
    /// Screen reader is active (VoiceOver, Narrator, Orca)
    pub screen_reader_active: bool,
    /// User prefers differentiate without color
    /// macOS: UIAccessibility.shouldDifferentiateWithoutColor
    pub differentiate_without_color: bool,
}

/// Common system colors used for UI elements.
/// 
/// These colors are queried from the operating system and automatically adapt
/// to the current theme (light/dark mode) and accent color settings.
/// 
/// On macOS, these correspond to NSColor semantic colors.
/// On Windows, these come from UISettings.
/// On Linux/GTK, these come from the GTK theme.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct SystemColors {
    // === Primary semantic colors ===
    /// Primary text color (NSColor.textColor on macOS)
    pub text: OptionColorU,
    /// Secondary text color for less prominent text (NSColor.secondaryLabelColor)
    pub secondary_text: OptionColorU,
    /// Tertiary text color for disabled/placeholder text (NSColor.tertiaryLabelColor)
    pub tertiary_text: OptionColorU,
    /// Background color for content areas (NSColor.textBackgroundColor)
    pub background: OptionColorU,
    
    // === Accent colors ===
    /// System accent color chosen by user (NSColor.controlAccentColor on macOS)
    pub accent: OptionColorU,
    /// Text color on accent backgrounds
    pub accent_text: OptionColorU,
    
    // === Control colors ===
    /// Button/control background (NSColor.controlColor)
    pub button_face: OptionColorU,
    /// Button/control text color (NSColor.controlTextColor)
    pub button_text: OptionColorU,
    /// Disabled control text color (NSColor.disabledControlTextColor)
    pub disabled_text: OptionColorU,
    
    // === Window colors ===
    /// Window background color (NSColor.windowBackgroundColor)
    pub window_background: OptionColorU,
    /// Under-page background color (NSColor.underPageBackgroundColor)
    pub under_page_background: OptionColorU,
    
    // === Selection colors ===
    /// Selection background when window is focused (NSColor.selectedContentBackgroundColor)
    pub selection_background: OptionColorU,
    /// Selection text color when window is focused
    pub selection_text: OptionColorU,
    /// Selection background when window is NOT focused (NSColor.unemphasizedSelectedContentBackgroundColor)
    /// This is used for :backdrop state styling
    pub selection_background_inactive: OptionColorU,
    /// Selection text color when window is NOT focused
    pub selection_text_inactive: OptionColorU,
    
    // === Additional semantic colors ===
    /// Link color (NSColor.linkColor)
    pub link: OptionColorU,
    /// Separator/divider color (NSColor.separatorColor)
    pub separator: OptionColorU,
    /// Grid/table line color (NSColor.gridColor)
    pub grid: OptionColorU,
    /// Find/search highlight color (NSColor.findHighlightColor)
    pub find_highlight: OptionColorU,
    
    // === Sidebar colors (macOS-specific) ===
    /// Sidebar background color
    pub sidebar_background: OptionColorU,
    /// Selected row in sidebar
    pub sidebar_selection: OptionColorU,
}

/// Common system font settings.
/// 
/// On macOS, these are queried from NSFont.
/// On Windows, these come from SystemParametersInfo.
/// On Linux, these come from GTK/gsettings.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct SystemFonts {
    /// The primary font used for UI elements like buttons and labels.
    /// On macOS: SF Pro (system font)
    /// On Windows: Segoe UI
    /// On Linux: Cantarell, Ubuntu, or system default
    pub ui_font: OptionString,
    /// The default font size for UI elements, in points.
    pub ui_font_size: OptionF32,
    /// The font used for code or other monospaced text.
    /// On macOS: SF Mono or Menlo
    /// On Windows: Cascadia Mono or Consolas
    /// On Linux: Ubuntu Mono or DejaVu Sans Mono
    pub monospace_font: OptionString,
    /// Monospace font size in points
    pub monospace_font_size: OptionF32,
    /// Bold variant of the UI font (if different)
    pub ui_font_bold: OptionString,
    /// Font for window titles
    pub title_font: OptionString,
    /// Title font size in points
    pub title_font_size: OptionF32,
    /// Font for menu items
    pub menu_font: OptionString,
    /// Menu font size in points
    pub menu_font_size: OptionF32,
    /// Small/caption font for less prominent text
    pub small_font: OptionString,
    /// Small font size in points
    pub small_font_size: OptionF32,
}

/// Common system metrics for UI element sizing and spacing.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct SystemMetrics {
    /// The corner radius for standard elements like buttons.
    pub corner_radius: OptionPixelValue,
    /// The width of standard borders.
    pub border_width: OptionPixelValue,
    /// The horizontal (left/right) padding for buttons and similar controls.
    pub button_padding_horizontal: OptionPixelValue,
    /// The vertical (top/bottom) padding for buttons and similar controls.
    pub button_padding_vertical: OptionPixelValue,
    /// Titlebar layout information (button positions, safe areas, etc.)
    pub titlebar: TitlebarMetrics,
}

/// Which side of the titlebar the window control buttons are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TitlebarButtonSide {
    /// Buttons are on the left (macOS default)
    Left,
    /// Buttons are on the right (Windows, most Linux DEs)
    #[default]
    Right,
}

/// Which window control buttons are available in the titlebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TitlebarButtons {
    /// Close button is available
    pub has_close: bool,
    /// Minimize button is available
    pub has_minimize: bool,
    /// Maximize/zoom button is available
    pub has_maximize: bool,
    /// Fullscreen button is available (macOS green button behavior)
    pub has_fullscreen: bool,
}

impl Default for TitlebarButtons {
    fn default() -> Self {
        Self {
            has_close: true,
            has_minimize: true,
            has_maximize: true,
            has_fullscreen: false,
        }
    }
}

/// Safe area insets for devices with notches, rounded corners, or sensor housings.
/// 
/// On devices like iPhones with notches or Dynamic Island, the safe area
/// indicates regions where content should not be placed to avoid being
/// obscured by hardware features.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct SafeAreaInsets {
    /// Inset from the top edge (notch, camera housing, etc.)
    pub top: OptionPixelValue,
    /// Inset from the bottom edge (home indicator on iPhone)
    pub bottom: OptionPixelValue,
    /// Inset from the left edge (rounded corners)
    pub left: OptionPixelValue,
    /// Inset from the right edge (rounded corners)
    pub right: OptionPixelValue,
}

/// Metrics for titlebar layout and window chrome.
/// 
/// This provides information needed to correctly position custom titlebar
/// content when using `WindowDecorations::NoTitle` (expanded title mode).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct TitlebarMetrics {
    /// Which side the window control buttons are on
    pub button_side: TitlebarButtonSide,
    /// Which buttons are available
    pub buttons: TitlebarButtons,
    /// Height of the titlebar in pixels
    pub height: OptionPixelValue,
    /// Width reserved for window control buttons (close/min/max)
    /// This is the space to avoid when drawing custom title text
    pub button_area_width: OptionPixelValue,
    /// Horizontal padding inside the titlebar
    pub padding_horizontal: OptionPixelValue,
    /// Safe area insets for notched/rounded displays
    pub safe_area: SafeAreaInsets,
    /// Title text font (from SystemFonts::title_font)
    pub title_font: OptionString,
    /// Title text font size
    pub title_font_size: OptionF32,
    /// Title text font weight (400 = normal, 600 = semibold, 700 = bold)
    pub title_font_weight: OptionU16,
}

impl Default for TitlebarMetrics {
    fn default() -> Self {
        Self {
            button_side: TitlebarButtonSide::Right,
            buttons: TitlebarButtons::default(),
            height: OptionPixelValue::Some(PixelValue::px(32.0)),
            button_area_width: OptionPixelValue::Some(PixelValue::px(100.0)),
            padding_horizontal: OptionPixelValue::Some(PixelValue::px(8.0)),
            safe_area: SafeAreaInsets::default(),
            title_font: OptionString::None,
            title_font_size: OptionF32::Some(13.0),
            title_font_weight: OptionU16::Some(600), // Semibold
        }
    }
}

impl TitlebarMetrics {
    /// Windows-style titlebar (buttons on right)
    pub fn windows() -> Self {
        Self {
            button_side: TitlebarButtonSide::Right,
            buttons: TitlebarButtons {
                has_close: true,
                has_minimize: true,
                has_maximize: true,
                has_fullscreen: false,
            },
            height: OptionPixelValue::Some(PixelValue::px(32.0)),
            button_area_width: OptionPixelValue::Some(PixelValue::px(138.0)), // 3 buttons * 46px
            padding_horizontal: OptionPixelValue::Some(PixelValue::px(8.0)),
            safe_area: SafeAreaInsets::default(),
            title_font: OptionString::Some("Segoe UI Variable Text".into()),
            title_font_size: OptionF32::Some(12.0),
            title_font_weight: OptionU16::Some(400), // Normal
        }
    }
    
    /// macOS-style titlebar (buttons on left, "traffic lights")
    pub fn macos() -> Self {
        Self {
            button_side: TitlebarButtonSide::Left,
            buttons: TitlebarButtons {
                has_close: true,
                has_minimize: true,
                has_maximize: false, // macOS has fullscreen instead
                has_fullscreen: true,
            },
            height: OptionPixelValue::Some(PixelValue::px(28.0)),
            button_area_width: OptionPixelValue::Some(PixelValue::px(78.0)), // 3 buttons with gaps
            padding_horizontal: OptionPixelValue::Some(PixelValue::px(8.0)),
            safe_area: SafeAreaInsets::default(),
            title_font: OptionString::Some(".SF NS".into()),
            title_font_size: OptionF32::Some(13.0),
            title_font_weight: OptionU16::Some(600), // Semibold
        }
    }
    
    /// Linux GNOME-style titlebar (buttons on right by default)
    pub fn linux_gnome() -> Self {
        Self {
            button_side: TitlebarButtonSide::Right, // Default, can be changed in settings
            buttons: TitlebarButtons {
                has_close: true,
                has_minimize: true,
                has_maximize: true,
                has_fullscreen: false,
            },
            height: OptionPixelValue::Some(PixelValue::px(35.0)),
            button_area_width: OptionPixelValue::Some(PixelValue::px(100.0)),
            padding_horizontal: OptionPixelValue::Some(PixelValue::px(12.0)),
            safe_area: SafeAreaInsets::default(),
            title_font: OptionString::Some("Cantarell".into()),
            title_font_size: OptionF32::Some(11.0),
            title_font_weight: OptionU16::Some(700), // Bold
        }
    }
    
    /// iOS-style safe area (for notched devices)
    pub fn ios() -> Self {
        Self {
            button_side: TitlebarButtonSide::Left,
            buttons: TitlebarButtons {
                has_close: false, // iOS apps don't have close buttons
                has_minimize: false,
                has_maximize: false,
                has_fullscreen: false,
            },
            height: OptionPixelValue::Some(PixelValue::px(44.0)),
            button_area_width: OptionPixelValue::Some(PixelValue::px(0.0)),
            padding_horizontal: OptionPixelValue::Some(PixelValue::px(16.0)),
            safe_area: SafeAreaInsets {
                // iPhone notch safe area
                top: OptionPixelValue::Some(PixelValue::px(47.0)),
                bottom: OptionPixelValue::Some(PixelValue::px(34.0)),
                left: OptionPixelValue::None,
                right: OptionPixelValue::None,
            },
            title_font: OptionString::Some(".SFUI-Semibold".into()),
            title_font_size: OptionF32::Some(17.0),
            title_font_weight: OptionU16::Some(600),
        }
    }
    
    /// Android-style titlebar (action bar)
    pub fn android() -> Self {
        Self {
            button_side: TitlebarButtonSide::Left, // Back button on left
            buttons: TitlebarButtons {
                has_close: false,
                has_minimize: false,
                has_maximize: false,
                has_fullscreen: false,
            },
            height: OptionPixelValue::Some(PixelValue::px(56.0)),
            button_area_width: OptionPixelValue::Some(PixelValue::px(48.0)), // Back button
            padding_horizontal: OptionPixelValue::Some(PixelValue::px(16.0)),
            safe_area: SafeAreaInsets::default(),
            title_font: OptionString::Some("Roboto Medium".into()),
            title_font_size: OptionF32::Some(20.0),
            title_font_weight: OptionU16::Some(500),
        }
    }
}

// ── Input interaction metrics ────────────────────────────────────────────

/// Input interaction timing and distance thresholds from the OS.
///
/// These values are queried from the operating system to match the user's
/// configured double-click speed, drag sensitivity, caret blink rate, etc.
///
/// # Platform APIs
/// - **macOS:** `NSEvent.doubleClickInterval`
/// - **Windows:** `GetDoubleClickTime()`, `GetSystemMetrics(SM_CXDOUBLECLK)`,
///   `GetCaretBlinkTime()`, `SystemParametersInfo(SPI_GETWHEELSCROLLLINES)`
/// - **Linux:** XDG Desktop Portal / gsettings
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct InputMetrics {
    /// Max milliseconds between clicks to register a double-click.
    pub double_click_time_ms: u32,
    /// Max pixels the mouse can move between clicks and still count.
    pub double_click_distance_px: f32,
    /// Pixels the mouse must move while held down before a drag starts.
    pub drag_threshold_px: f32,
    /// Caret blink rate in milliseconds (0 = no blink).
    pub caret_blink_rate_ms: u32,
    /// Width of the text caret/cursor in pixels (typically 1–2).
    pub caret_width_px: f32,
    /// Lines to scroll per mouse wheel notch.
    pub wheel_scroll_lines: u32,
    /// Milliseconds to wait before a hover triggers (e.g. tooltip delay).
    /// Windows: `SystemParametersInfo(SPI_GETMOUSEHOVERTIME)` — default 400.
    pub hover_time_ms: u32,
}

impl Default for InputMetrics {
    fn default() -> Self {
        Self {
            double_click_time_ms: 500,
            double_click_distance_px: 4.0,
            drag_threshold_px: 5.0,
            caret_blink_rate_ms: 530,
            caret_width_px: 1.0,
            wheel_scroll_lines: 3,
            hover_time_ms: 400,
        }
    }
}

// ── Text rendering hints ─────────────────────────────────────────────────

/// Subpixel rendering layout for font smoothing.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum SubpixelType {
    /// No subpixel rendering (grayscale anti-aliasing only).
    #[default]
    None,
    /// Horizontal RGB subpixel layout (most common for LCD monitors).
    Rgb,
    /// Horizontal BGR subpixel layout.
    Bgr,
    /// Vertical RGB subpixel layout.
    VRgb,
    /// Vertical BGR subpixel layout.
    VBgr,
}

/// Text rendering configuration from the OS.
///
/// These hints allow the framework to match the host's font smoothing
/// settings for crisp, consistent text rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct TextRenderingHints {
    /// Subpixel rendering type.
    pub subpixel_type: SubpixelType,
    /// Font smoothing gamma (1000 = default, higher = more contrast).
    pub font_smoothing_gamma: u32,
    /// Whether font smoothing (anti-aliasing) is enabled.
    pub font_smoothing_enabled: bool,
    /// User prefers increased text contrast.
    pub increased_contrast: bool,
}

impl Default for TextRenderingHints {
    fn default() -> Self {
        Self {
            subpixel_type: SubpixelType::None,
            font_smoothing_gamma: 1000,
            font_smoothing_enabled: true,
            increased_contrast: false,
        }
    }
}

// ── Focus ring visuals ───────────────────────────────────────────────────

/// Focus ring / indicator visual style.
///
/// When an element receives keyboard focus the OS typically draws a visible
/// ring or border.  These values come from the OS preferences.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct FocusVisuals {
    /// Focus ring / indicator colour.
    /// macOS: `NSColor.keyboardFocusIndicatorColor`
    pub focus_ring_color: OptionColorU,
    /// Width of focus border / ring.
    /// Windows: `SystemParametersInfo(SPI_GETFOCUSBORDERWIDTH)`
    pub focus_border_width: OptionPixelValue,
    /// Height of focus border / ring.
    pub focus_border_height: OptionPixelValue,
}

// ── Scrollbar preferences ────────────────────────────────────────────────

/// When scrollbars should be shown (OS-level preference).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum ScrollbarVisibility {
    /// Always show scrollbars.
    Always,
    /// Show only while scrolling, then fade out.
    #[default]
    WhenScrolling,
    /// Automatic: depends on input device (trackpad → overlay, mouse → always).
    Automatic,
}

/// What happens when clicking the scrollbar track area.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum ScrollbarTrackClick {
    /// Jump to the clicked position.
    JumpToPosition,
    /// Scroll by one page.
    #[default]
    PageUpDown,
}

/// OS-level scrollbar behaviour preferences.
///
/// These are separate from the CSS scrollbar *appearance* (`ComputedScrollbarStyle`).
/// They control *when* scrollbars appear and *how* clicking the track behaves.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct ScrollbarPreferences {
    /// How scrollbars should be shown.
    /// macOS: `NSScroller.preferredScrollerStyle`
    pub visibility: ScrollbarVisibility,
    /// What happens when clicking the scrollbar track.
    pub track_click: ScrollbarTrackClick,
}

impl Default for ScrollbarPreferences {
    fn default() -> Self {
        Self {
            visibility: ScrollbarVisibility::WhenScrolling,
            track_click: ScrollbarTrackClick::PageUpDown,
        }
    }
}

// ── Linux-specific customisation ─────────────────────────────────────────

/// Linux-specific customisation settings.
///
/// Read from GTK / KDE / XDG settings on Linux; `Default` (all `None` / 0)
/// on other platforms.
#[derive(Debug, Default, Clone, PartialEq)]
#[repr(C)]
pub struct LinuxCustomization {
    /// GTK theme name (e.g. "Adwaita", "Breeze", "Numix").
    pub gtk_theme: OptionString,
    /// Icon theme name (e.g. "Papirus", "Numix", "Breeze").
    pub icon_theme: OptionString,
    /// Cursor theme name (e.g. "Breeze_Snow", "DMZ-Black").
    pub cursor_theme: OptionString,
    /// Cursor size in pixels (0 = unset / use OS default).
    pub cursor_size: u32,
    /// GTK button layout string (e.g. "close,minimize,maximize:menu").
    /// Determines button side and order for CSD titlebars on Linux.
    pub titlebar_button_layout: OptionString,
}

// ── Visual hints (icons in menus / buttons / toolbar style) ──────────────

/// Toolbar display style (icons, text, or both).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum ToolbarStyle {
    /// Show only icons in toolbars.
    #[default]
    IconsOnly,
    /// Show only text labels in toolbars.
    TextOnly,
    /// Show text beside the icon (horizontal).
    TextBesideIcon,
    /// Show text below the icon (vertical).
    TextBelowIcon,
}

/// Visual hints from the OS about how icons and decorations should be shown.
///
/// These preferences differ heavily between Linux desktops (KDE vs GNOME)
/// and are less configurable on macOS / Windows where HIG rules apply.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct VisualHints {
    /// Toolbar display style.
    /// Linux: `org.gnome.desktop.interface toolbar-style`, KDE `ToolButtonStyle`.
    pub toolbar_style: ToolbarStyle,
    /// Show icons on push buttons?  (Common in KDE, rare in Win/Mac.)
    /// Linux: `org.gnome.desktop.interface buttons-have-icons`, KDE ShowIconsOnPushButtons.
    pub show_button_images: bool,
    /// Show icons in context menus?  (GNOME defaults off since 3.x; Win/Mac/KDE usually on.)
    /// Linux: `org.gnome.desktop.interface menus-have-icons`.
    pub show_menu_images: bool,
    /// Should tooltips be shown on hover?
    pub show_tooltips: bool,
    /// Flash the window taskbar entry on alert?
    pub flash_on_alert: bool,
}

impl Default for VisualHints {
    fn default() -> Self {
        Self {
            toolbar_style: ToolbarStyle::IconsOnly,
            show_button_images: false,
            show_menu_images: true,
            show_tooltips: true,
            flash_on_alert: true,
        }
    }
}

// ── Animation metrics ────────────────────────────────────────────────────

/// Focus indicator behaviour (always visible vs keyboard-only).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FocusBehavior {
    /// Focus indicators are always visible when an element has focus.
    #[default]
    AlwaysVisible,
    /// Focus indicators are hidden until the user presses a keyboard key
    /// (Alt, Tab, arrow keys, etc.).  Windows: `SPI_GETKEYBOARDCUES`.
    KeyboardOnly,
}

/// Animation-related preferences from the OS.
///
/// These control whether UI animations (transitions, fades, slides) should
/// play and at what speed.
///
/// # Platform APIs
/// - **Windows:** `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)`,
///   `SPI_GETKEYBOARDCUES`
/// - **macOS:** `NSWorkspace.accessibilityDisplayShouldReduceMotion`
/// - **Linux:** `org.gnome.desktop.interface enable-animations`,
///   KDE `AnimationDurationFactor`
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct AnimationMetrics {
    /// Global enable/disable for UI animations.
    pub animations_enabled: bool,
    /// Animation speed factor (1.0 = normal, 0.5 = 2× faster, 2.0 = 2× slower).
    /// Primarily used in KDE.
    pub animation_duration_factor: f32,
    /// When to show focus rectangles / rings.
    pub focus_indicator_behavior: FocusBehavior,
}

impl Default for AnimationMetrics {
    fn default() -> Self {
        Self {
            animations_enabled: true,
            animation_duration_factor: 1.0,
            focus_indicator_behavior: FocusBehavior::AlwaysVisible,
        }
    }
}

// ── Audio metrics ────────────────────────────────────────────────────────

/// Audio-feedback preferences from the OS.
///
/// Controls whether the app should make sounds on events (error pings,
/// notifications) or on input (clicks, key presses).
///
/// # Platform APIs
/// - **Windows:** `SystemParametersInfo(SPI_GETBEEP)`
/// - **macOS:** `NSSound.soundEffectAudioVolume`
/// - **Linux:** `org.gnome.desktop.sound event-sounds`,
///   `org.gnome.desktop.sound input-feedback-sounds`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct AudioMetrics {
    /// Should the app make sounds on events?  (Error ping, notification, etc.)
    pub event_sounds_enabled: bool,
    /// Should the app make sounds on input?  (Clicks, typing feedback.)
    pub input_feedback_sounds_enabled: bool,
}

impl Default for AudioMetrics {
    fn default() -> Self {
        Self {
            event_sounds_enabled: true,
            input_feedback_sounds_enabled: false,
        }
    }
}

/// Apple system font family names for font fallback chains.
/// 
/// These are the canonical names for Apple's system fonts, which should
/// be used in font fallback chains for proper rendering on Apple platforms.
/// Note: The names here must match what rust-fontconfig indexes from the font metadata.
pub mod apple_fonts {
    /// System Font - Primary system font for macOS
    /// This is how rust-fontconfig indexes the SF Pro font family
    pub const SYSTEM_FONT: &str = "System Font";
    
    /// SF NS variants as indexed by rust-fontconfig
    pub const SF_NS_ROUNDED: &str = "SF NS Rounded";
    
    /// SF Compact - System font optimized for watchOS
    /// Optimized for small sizes and narrow columns
    pub const SF_COMPACT: &str = "SF Compact";
    
    /// SF Mono - Monospaced font used in Xcode
    /// Enables alignment between rows and columns of text
    pub const SF_MONO: &str = "SF NS Mono Light";
    
    /// New York - Serif font for reading
    /// Performs as traditional reading face at small sizes
    pub const NEW_YORK: &str = "New York";
    
    /// SF Arabic - Arabic system font
    pub const SF_ARABIC: &str = "SF Arabic";
    
    /// SF Armenian - Armenian system font
    pub const SF_ARMENIAN: &str = "SF Armenian";
    
    /// SF Georgian - Georgian system font
    pub const SF_GEORGIAN: &str = "SF Georgian";
    
    /// SF Hebrew - Hebrew system font with niqqud support
    pub const SF_HEBREW: &str = "SF Hebrew";
    
    /// Legacy macOS fonts for fallback
    pub const MENLO: &str = "Menlo";
    pub const MENLO_REGULAR: &str = "Menlo Regular";
    pub const MENLO_BOLD: &str = "Menlo Bold";
    pub const MONACO: &str = "Monaco";
    pub const LUCIDA_GRANDE: &str = "Lucida Grande";
    pub const LUCIDA_GRANDE_BOLD: &str = "Lucida Grande Bold";
    pub const HELVETICA_NEUE: &str = "Helvetica Neue";
    pub const HELVETICA_NEUE_BOLD: &str = "Helvetica Neue Bold";
}

/// Windows system font family names.
pub mod windows_fonts {
    /// Modern Windows 11 fonts
    pub const SEGOE_UI_VARIABLE: &str = "Segoe UI Variable";
    pub const SEGOE_UI_VARIABLE_TEXT: &str = "Segoe UI Variable Text";
    pub const SEGOE_UI_VARIABLE_DISPLAY: &str = "Segoe UI Variable Display";
    
    /// Standard Windows fonts
    pub const SEGOE_UI: &str = "Segoe UI";
    pub const CONSOLAS: &str = "Consolas";
    pub const CASCADIA_CODE: &str = "Cascadia Code";
    pub const CASCADIA_MONO: &str = "Cascadia Mono";
    
    /// Legacy Windows fonts
    pub const TAHOMA: &str = "Tahoma";
    pub const MS_SANS_SERIF: &str = "MS Sans Serif";
    pub const LUCIDA_CONSOLE: &str = "Lucida Console";
    pub const COURIER_NEW: &str = "Courier New";
}

/// Linux/GTK common font family names.
pub mod linux_fonts {
    /// GNOME default fonts
    pub const CANTARELL: &str = "Cantarell";
    pub const ADWAITA: &str = "Adwaita";
    
    /// Ubuntu fonts
    pub const UBUNTU: &str = "Ubuntu";
    pub const UBUNTU_MONO: &str = "Ubuntu Mono";
    
    /// DejaVu fonts (widely available)
    pub const DEJAVU_SANS: &str = "DejaVu Sans";
    pub const DEJAVU_SANS_MONO: &str = "DejaVu Sans Mono";
    pub const DEJAVU_SERIF: &str = "DejaVu Serif";
    
    /// Liberation fonts (metrically compatible with Windows fonts)
    pub const LIBERATION_SANS: &str = "Liberation Sans";
    pub const LIBERATION_MONO: &str = "Liberation Mono";
    pub const LIBERATION_SERIF: &str = "Liberation Serif";
    
    /// Noto fonts (broad Unicode coverage)
    pub const NOTO_SANS: &str = "Noto Sans";
    pub const NOTO_MONO: &str = "Noto Sans Mono";
    pub const NOTO_SERIF: &str = "Noto Serif";
    
    /// KDE default fonts
    pub const HACK: &str = "Hack";
    
    /// Generic fallback names
    pub const MONOSPACE: &str = "Monospace";
    pub const SANS_SERIF: &str = "Sans";
    pub const SERIF: &str = "Serif";
}

impl SystemFontType {
    /// Returns the font fallback chain for this font type on the given platform.
    /// 
    /// The returned list contains font family names in order of preference.
    /// The first available font should be used.
    pub fn get_fallback_chain(&self, platform: &Platform) -> Vec<&'static str> {
        match platform {
            Platform::MacOs | Platform::Ios => self.macos_fallback_chain(),
            Platform::Windows => self.windows_fallback_chain(),
            Platform::Linux(_) => self.linux_fallback_chain(),
            Platform::Android => self.android_fallback_chain(),
            Platform::Unknown => self.generic_fallback_chain(),
        }
    }
    
    fn macos_fallback_chain(&self) -> Vec<&'static str> {
        match self {
            // For Normal weight, try System Font first, then Helvetica Neue
            SystemFontType::Ui => vec![
                apple_fonts::SYSTEM_FONT,
                apple_fonts::HELVETICA_NEUE,
                apple_fonts::LUCIDA_GRANDE,
            ],
            // For Bold weight, use Helvetica Neue first (System Font has no Bold variant in fontconfig)
            SystemFontType::UiBold => vec![
                apple_fonts::HELVETICA_NEUE, // Will be queried with weight=Bold -> "Helvetica Neue Bold"
                apple_fonts::LUCIDA_GRANDE,
            ],
            // Monospace fonts: Menlo has bold variant
            SystemFontType::Monospace => vec![
                apple_fonts::MENLO,
                apple_fonts::MONACO,
            ],
            SystemFontType::MonospaceBold | SystemFontType::MonospaceItalic => vec![
                apple_fonts::MENLO, // Menlo Bold exists
                apple_fonts::MONACO,
            ],
            // Title: same strategy - use Helvetica Neue for bold
            SystemFontType::Title => vec![
                apple_fonts::SYSTEM_FONT,
                apple_fonts::HELVETICA_NEUE,
            ],
            SystemFontType::TitleBold => vec![
                apple_fonts::HELVETICA_NEUE, // Will be queried with weight=Bold
                apple_fonts::LUCIDA_GRANDE,
            ],
            SystemFontType::Menu => vec![
                apple_fonts::SYSTEM_FONT,
                apple_fonts::HELVETICA_NEUE,
            ],
            SystemFontType::Small => vec![
                apple_fonts::SYSTEM_FONT,
                apple_fonts::HELVETICA_NEUE,
            ],
            // Serif fonts - Georgia has bold variant
            SystemFontType::Serif => vec![
                apple_fonts::NEW_YORK,
                "Georgia",
                "Times New Roman",
            ],
            SystemFontType::SerifBold => vec![
                "Georgia", // Georgia Bold exists
                "Times New Roman",
            ],
        }
    }
    
    fn windows_fallback_chain(&self) -> Vec<&'static str> {
        match self {
            SystemFontType::Ui | SystemFontType::UiBold => vec![
                windows_fonts::SEGOE_UI_VARIABLE_TEXT,
                windows_fonts::SEGOE_UI,
                windows_fonts::TAHOMA,
            ],
            SystemFontType::Monospace | SystemFontType::MonospaceBold | SystemFontType::MonospaceItalic => vec![
                windows_fonts::CASCADIA_MONO,
                windows_fonts::CASCADIA_CODE,
                windows_fonts::CONSOLAS,
                windows_fonts::LUCIDA_CONSOLE,
                windows_fonts::COURIER_NEW,
            ],
            SystemFontType::Title | SystemFontType::TitleBold => vec![
                windows_fonts::SEGOE_UI_VARIABLE_DISPLAY,
                windows_fonts::SEGOE_UI,
            ],
            SystemFontType::Menu => vec![
                windows_fonts::SEGOE_UI,
                windows_fonts::TAHOMA,
            ],
            SystemFontType::Small => vec![
                windows_fonts::SEGOE_UI,
            ],
            SystemFontType::Serif | SystemFontType::SerifBold => vec![
                "Cambria",
                "Georgia",
                "Times New Roman",
            ],
        }
    }
    
    fn linux_fallback_chain(&self) -> Vec<&'static str> {
        match self {
            SystemFontType::Ui | SystemFontType::UiBold => vec![
                linux_fonts::CANTARELL,
                linux_fonts::UBUNTU,
                linux_fonts::NOTO_SANS,
                linux_fonts::DEJAVU_SANS,
                linux_fonts::LIBERATION_SANS,
                linux_fonts::SANS_SERIF,
            ],
            SystemFontType::Monospace | SystemFontType::MonospaceBold | SystemFontType::MonospaceItalic => vec![
                linux_fonts::UBUNTU_MONO,
                linux_fonts::HACK,
                linux_fonts::NOTO_MONO,
                linux_fonts::DEJAVU_SANS_MONO,
                linux_fonts::LIBERATION_MONO,
                linux_fonts::MONOSPACE,
            ],
            SystemFontType::Title | SystemFontType::TitleBold => vec![
                linux_fonts::CANTARELL,
                linux_fonts::UBUNTU,
                linux_fonts::NOTO_SANS,
            ],
            SystemFontType::Menu => vec![
                linux_fonts::CANTARELL,
                linux_fonts::UBUNTU,
                linux_fonts::NOTO_SANS,
            ],
            SystemFontType::Small => vec![
                linux_fonts::CANTARELL,
                linux_fonts::UBUNTU,
                linux_fonts::NOTO_SANS,
            ],
            SystemFontType::Serif | SystemFontType::SerifBold => vec![
                linux_fonts::NOTO_SERIF,
                linux_fonts::DEJAVU_SERIF,
                linux_fonts::LIBERATION_SERIF,
                linux_fonts::SERIF,
            ],
        }
    }
    
    fn android_fallback_chain(&self) -> Vec<&'static str> {
        match self {
            SystemFontType::Ui | SystemFontType::UiBold => vec!["Roboto", "Noto Sans"],
            SystemFontType::Monospace | SystemFontType::MonospaceBold | SystemFontType::MonospaceItalic => {
                vec!["Roboto Mono", "Droid Sans Mono", "monospace"]
            }
            SystemFontType::Title | SystemFontType::TitleBold => vec!["Roboto", "Noto Sans"],
            SystemFontType::Menu => vec!["Roboto"],
            SystemFontType::Small => vec!["Roboto"],
            SystemFontType::Serif | SystemFontType::SerifBold => vec!["Noto Serif", "Droid Serif", "serif"],
        }
    }
    
    fn generic_fallback_chain(&self) -> Vec<&'static str> {
        match self {
            SystemFontType::Ui | SystemFontType::UiBold => vec!["sans-serif"],
            SystemFontType::Monospace | SystemFontType::MonospaceBold | SystemFontType::MonospaceItalic => {
                vec!["monospace"]
            }
            SystemFontType::Title | SystemFontType::TitleBold => vec!["sans-serif"],
            SystemFontType::Menu => vec!["sans-serif"],
            SystemFontType::Small => vec!["sans-serif"],
            SystemFontType::Serif | SystemFontType::SerifBold => vec!["serif"],
        }
    }
}

impl SystemStyle {

    /// Format the SystemStyle as a human-readable JSON string for debugging.
    ///
    /// This does NOT use serde — it manually formats the most important fields
    /// so that they can be verified against OS-reported values in a test script.
    pub fn to_json_string(&self) -> AzString {
        use alloc::format;

        fn opt_color(c: &OptionColorU) -> alloc::string::String {
            match c.as_ref() {
                Some(c) => format!("\"#{:02x}{:02x}{:02x}{:02x}\"", c.r, c.g, c.b, c.a),
                None => "null".into(),
            }
        }
        fn opt_str(s: &OptionString) -> alloc::string::String {
            match s.as_ref() {
                Some(s) => format!("\"{}\"", s.as_str()),
                None => "null".into(),
            }
        }
        fn opt_f32(v: &OptionF32) -> alloc::string::String {
            match v.into_option() {
                Some(v) => format!("{:.2}", v),
                None => "null".into(),
            }
        }
        fn opt_u16(v: &OptionU16) -> alloc::string::String {
            match v.into_option() {
                Some(v) => format!("{}", v),
                None => "null".into(),
            }
        }
        fn opt_px(v: &OptionPixelValue) -> alloc::string::String {
            match v.as_ref() {
                Some(v) => format!("{:.1}", v.to_pixels_internal(0.0, 0.0)),
                None => "null".into(),
            }
        }

        let tm = &self.metrics.titlebar;
        let inp = &self.input;
        let tr = &self.text_rendering;
        let acc = &self.accessibility;
        let sp = &self.scrollbar_preferences;
        let lnx = &self.linux;
        let vh = &self.visual_hints;
        let anim = &self.animation;
        let audio = &self.audio;

        let json = format!(
r#"{{
  "theme": "{:?}",
  "platform": "{:?}",
  "os_version": "{:?}:{}",
  "language": "{}",
  "prefers_reduced_motion": {:?},
  "prefers_high_contrast": {:?},
  "colors": {{
    "text": {},
    "secondary_text": {},
    "tertiary_text": {},
    "background": {},
    "accent": {},
    "accent_text": {},
    "button_face": {},
    "button_text": {},
    "disabled_text": {},
    "window_background": {},
    "under_page_background": {},
    "selection_background": {},
    "selection_text": {},
    "selection_background_inactive": {},
    "selection_text_inactive": {},
    "link": {},
    "separator": {},
    "grid": {},
    "find_highlight": {},
    "sidebar_background": {},
    "sidebar_selection": {}
  }},
  "fonts": {{
    "ui_font": {},
    "ui_font_size": {},
    "monospace_font": {},
    "title_font": {},
    "menu_font": {},
    "small_font": {}
  }},
  "titlebar": {{
    "button_side": "{:?}",
    "height": {},
    "button_area_width": {},
    "padding_horizontal": {},
    "title_font": {},
    "title_font_size": {},
    "title_font_weight": {},
    "has_close": {},
    "has_minimize": {},
    "has_maximize": {},
    "has_fullscreen": {}
  }},
  "input": {{
    "double_click_time_ms": {},
    "double_click_distance_px": {:.1},
    "drag_threshold_px": {:.1},
    "caret_blink_rate_ms": {},
    "caret_width_px": {:.1},
    "wheel_scroll_lines": {},
    "hover_time_ms": {}
  }},
  "text_rendering": {{
    "font_smoothing_enabled": {},
    "subpixel_type": "{:?}",
    "font_smoothing_gamma": {},
    "increased_contrast": {}
  }},
  "accessibility": {{
    "prefers_bold_text": {},
    "prefers_larger_text": {},
    "text_scale_factor": {:.2},
    "prefers_high_contrast": {},
    "prefers_reduced_motion": {},
    "prefers_reduced_transparency": {},
    "screen_reader_active": {},
    "differentiate_without_color": {}
  }},
  "scrollbar_preferences": {{
    "visibility": "{:?}",
    "track_click": "{:?}"
  }},
  "linux": {{
    "gtk_theme": {},
    "icon_theme": {},
    "cursor_theme": {},
    "cursor_size": {},
    "titlebar_button_layout": {}
  }},
  "visual_hints": {{
    "show_button_images": {},
    "show_menu_images": {},
    "toolbar_style": "{:?}",
    "show_tooltips": {}
  }},
  "animation": {{
    "animations_enabled": {},
    "animation_duration_factor": {:.2},
    "focus_indicator_behavior": "{:?}"
  }},
  "audio": {{
    "event_sounds_enabled": {},
    "input_feedback_sounds_enabled": {}
  }}
}}"#,
            // top-level
            self.theme,
            self.platform,
            self.os_version.os, self.os_version.version_id,
            self.language.as_str(),
            self.prefers_reduced_motion,
            self.prefers_high_contrast,
            // colors
            opt_color(&self.colors.text),
            opt_color(&self.colors.secondary_text),
            opt_color(&self.colors.tertiary_text),
            opt_color(&self.colors.background),
            opt_color(&self.colors.accent),
            opt_color(&self.colors.accent_text),
            opt_color(&self.colors.button_face),
            opt_color(&self.colors.button_text),
            opt_color(&self.colors.disabled_text),
            opt_color(&self.colors.window_background),
            opt_color(&self.colors.under_page_background),
            opt_color(&self.colors.selection_background),
            opt_color(&self.colors.selection_text),
            opt_color(&self.colors.selection_background_inactive),
            opt_color(&self.colors.selection_text_inactive),
            opt_color(&self.colors.link),
            opt_color(&self.colors.separator),
            opt_color(&self.colors.grid),
            opt_color(&self.colors.find_highlight),
            opt_color(&self.colors.sidebar_background),
            opt_color(&self.colors.sidebar_selection),
            // fonts
            opt_str(&self.fonts.ui_font),
            opt_f32(&self.fonts.ui_font_size),
            opt_str(&self.fonts.monospace_font),
            opt_str(&self.fonts.title_font),
            opt_str(&self.fonts.menu_font),
            opt_str(&self.fonts.small_font),
            // titlebar
            tm.button_side,
            opt_px(&tm.height),
            opt_px(&tm.button_area_width),
            opt_px(&tm.padding_horizontal),
            opt_str(&tm.title_font),
            opt_f32(&tm.title_font_size),
            opt_u16(&tm.title_font_weight),
            tm.buttons.has_close,
            tm.buttons.has_minimize,
            tm.buttons.has_maximize,
            tm.buttons.has_fullscreen,
            // input
            inp.double_click_time_ms,
            inp.double_click_distance_px,
            inp.drag_threshold_px,
            inp.caret_blink_rate_ms,
            inp.caret_width_px,
            inp.wheel_scroll_lines,
            inp.hover_time_ms,
            // text_rendering
            tr.font_smoothing_enabled,
            tr.subpixel_type,
            tr.font_smoothing_gamma,
            tr.increased_contrast,
            // accessibility
            acc.prefers_bold_text,
            acc.prefers_larger_text,
            acc.text_scale_factor,
            acc.prefers_high_contrast,
            acc.prefers_reduced_motion,
            acc.prefers_reduced_transparency,
            acc.screen_reader_active,
            acc.differentiate_without_color,
            // scrollbar_preferences
            sp.visibility,
            sp.track_click,
            // linux
            opt_str(&lnx.gtk_theme),
            opt_str(&lnx.icon_theme),
            opt_str(&lnx.cursor_theme),
            lnx.cursor_size,
            opt_str(&lnx.titlebar_button_layout),
            // visual_hints
            vh.show_button_images,
            vh.show_menu_images,
            vh.toolbar_style,
            vh.show_tooltips,
            // animation
            anim.animations_enabled,
            anim.animation_duration_factor,
            anim.focus_indicator_behavior,
            // audio
            audio.event_sounds_enabled,
            audio.input_feedback_sounds_enabled,
        );

        AzString::from(json)
    }

    /// Discovers the system's UI style, and loads an optional app-specific stylesheet.
    ///
    /// If the "io" feature is enabled, this function may be slow as it can
    /// involve running external commands and reading files.
    ///
    /// If the "io" feature is disabled, this returns a hard-coded, deterministic
    /// style based on the target operating system.
    pub fn detect() -> Self {
        // Step 1: Get the base style.
        //
        // Priority order:
        //   1. `system` feature → native OS APIs via dlopen / FFI (most accurate)
        //   2. `io` feature     → CLI-based discovery (slower, text-parsing)
        //   3. neither          → hard-coded compile-time defaults
        let mut style = {
            // ── Priority 1: native dlopen discovery ──────────────────────
            #[cfg(feature = "system")]
            {
                #[cfg(target_os = "macos")]
                {
                    native_macos::discover()
                }
                #[cfg(target_os = "windows")]
                {
                    native_windows::discover()
                }
                #[cfg(target_os = "linux")]
                {
                    native_linux::discover()
                }
                #[cfg(not(any(
                    target_os = "macos",
                    target_os = "windows",
                    target_os = "linux",
                )))]
                {
                    Self::default()
                }
            }
            // ── Priority 2: CLI-based discovery ──────────────────────────
            #[cfg(all(not(feature = "system"), feature = "io"))]
            {
                #[cfg(target_os = "linux")]
                {
                    discover_linux_style()
                }
                #[cfg(target_os = "windows")]
                {
                    discover_windows_style()
                }
                #[cfg(target_os = "macos")]
                {
                    discover_macos_style()
                }
                #[cfg(target_os = "android")]
                {
                    defaults::android_material_light()
                }
                #[cfg(target_os = "ios")]
                {
                    defaults::ios_light()
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "windows",
                    target_os = "macos",
                    target_os = "android",
                    target_os = "ios"
                )))]
                {
                    Self::default()
                }
            }
            // ── Priority 3: hard-coded compile-time defaults ─────────────
            #[cfg(not(any(feature = "system", feature = "io")))]
            {
                #[cfg(target_os = "windows")]
                {
                    defaults::windows_11_light()
                }
                #[cfg(target_os = "macos")]
                {
                    defaults::macos_modern_light()
                }
                #[cfg(target_os = "linux")]
                {
                    defaults::gnome_adwaita_light()
                }
                #[cfg(target_os = "android")]
                {
                    defaults::android_material_light()
                }
                #[cfg(target_os = "ios")]
                {
                    defaults::ios_light()
                }
                #[cfg(not(any(
                    target_os = "linux",
                    target_os = "windows",
                    target_os = "macos",
                    target_os = "android",
                    target_os = "ios"
                )))]
                {
                    Self::default()
                }
            }
        };

        // Step 2: Check for the opt-out env var for app-specific styling.
        #[cfg(feature = "io")]
        {
            if std::env::var("AZUL_DISABLE_RICING").is_ok() {
                return style; // User explicitly disabled it.
            }

            // Step 3: Try to load the app-specific stylesheet.
            if let Some(stylesheet) = load_app_specific_stylesheet() {
                style.app_specific_stylesheet = Some(Box::new(stylesheet));
            }
        }

        style
    }

    /// Alias for `detect` - kept for internal compatibility, not exposed in FFI.
    #[inline(always)]
    pub fn new() -> Self {
        Self::detect()
    }

    /// Create a CSS stylesheet for CSD (Client-Side Decorations) titlebar
    ///
    /// This generates CSS rules for the CSD titlebar using system colors,
    /// fonts, and metrics to match the native platform look.
    pub fn create_csd_stylesheet(&self) -> Stylesheet {
        use alloc::format;

        use crate::parser2::new_from_str;

        // Build CSS string from SystemStyle
        let mut css = String::new();

        // Get system colors with fallbacks
        let bg_color = self
            .colors
            .window_background
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(240, 240, 240));
        let text_color = self
            .colors
            .text
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(0, 0, 0));
        let accent_color = self
            .colors
            .accent
            .as_option()
            .copied()
            .unwrap_or(ColorU::new_rgb(0, 120, 215));
        let border_color = match self.theme {
            Theme::Dark => ColorU::new_rgb(60, 60, 60),
            Theme::Light => ColorU::new_rgb(200, 200, 200),
        };

        // Get system metrics with fallbacks
        let corner_radius = self
            .metrics
            .corner_radius
            .map(|px| {
                use crate::props::basic::pixel::DEFAULT_FONT_SIZE;
                format!("{}px", px.to_pixels_internal(1.0, DEFAULT_FONT_SIZE))
            })
            .unwrap_or_else(|| "4px".to_string());

        // Titlebar container
        css.push_str(&format!(
            ".csd-titlebar {{ width: 100%; height: 32px; background: rgb({}, {}, {}); \
             border-bottom: 1px solid rgb({}, {}, {}); display: flex; flex-direction: row; \
             align-items: center; justify-content: space-between; padding: 0 8px; \
             cursor: grab; user-select: none; }} ",
            bg_color.r, bg_color.g, bg_color.b, border_color.r, border_color.g, border_color.b,
        ));

        // Title text
        css.push_str(&format!(
            ".csd-title {{ color: rgb({}, {}, {}); font-size: 13px; flex-grow: 1; text-align: \
             center; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; \
             user-select: none; }} ",
            text_color.r, text_color.g, text_color.b,
        ));

        // Button container
        css.push_str(".csd-buttons { display: flex; flex-direction: row; gap: 4px; } ");

        // Buttons
        css.push_str(&format!(
            ".csd-button {{ width: 32px; height: 24px; border-radius: {}; background: \
             transparent; color: rgb({}, {}, {}); font-size: 16px; line-height: 24px; text-align: \
             center; cursor: pointer; user-select: none; }} ",
            corner_radius, text_color.r, text_color.g, text_color.b,
        ));

        // Button hover state
        let hover_color = match self.theme {
            Theme::Dark => ColorU::new_rgb(60, 60, 60),
            Theme::Light => ColorU::new_rgb(220, 220, 220),
        };
        css.push_str(&format!(
            ".csd-button:hover {{ background: rgb({}, {}, {}); }} ",
            hover_color.r, hover_color.g, hover_color.b,
        ));

        // Close button hover (red on all platforms)
        css.push_str(
            ".csd-close:hover { background: rgb(232, 17, 35); color: rgb(255, 255, 255); } ",
        );

        // Platform-specific button styling
        match self.platform {
            Platform::MacOs => {
                // macOS traffic light buttons (left side)
                css.push_str(".csd-buttons { position: absolute; left: 8px; } ");
                css.push_str(
                    ".csd-close { background: rgb(255, 95, 86); width: 12px; height: 12px; \
                     border-radius: 50%; } ",
                );
                css.push_str(
                    ".csd-minimize { background: rgb(255, 189, 46); width: 12px; height: 12px; \
                     border-radius: 50%; } ",
                );
                css.push_str(
                    ".csd-maximize { background: rgb(40, 201, 64); width: 12px; height: 12px; \
                     border-radius: 50%; } ",
                );
            }
            Platform::Linux(_) => {
                // Linux - title on left, buttons on right
                css.push_str(".csd-title { text-align: left; } ");
            }
            _ => {
                // Windows and others - standard layout
            }
        }

        // Parse CSS string into Stylesheet
        let (mut parsed_css, _warnings) = new_from_str(&css);

        // Return first stylesheet (should always exist)
        if !parsed_css.stylesheets.is_empty() {
            parsed_css.stylesheets.into_library_owned_vec().remove(0)
        } else {
            Stylesheet::default()
        }
    }
}

// -- Platform-Specific Implementations (with I/O) --

#[cfg(feature = "io")]
fn discover_linux_style() -> SystemStyle {
    // Check for the easter egg env var. If it's set, we skip straight to the "riced"
    // discovery, embracing the smoke and mirrors of a custom desktop.
    if std::env::var("AZUL_SMOKE_AND_MIRRORS").is_err() {
        // If the env var is NOT set, try the normal desktop environments first.
        if let Ok(kde_style) = discover_kde_style() {
            return kde_style;
        }
        if let Ok(gnome_style) = discover_gnome_style() {
            return gnome_style;
        }
    }

    // This also acts as a fallback for non-GNOME/KDE environments.
    if let Ok(riced_style) = discover_riced_style() {
        return riced_style;
    }

    // Absolute fallback if nothing can be determined.
    defaults::gnome_adwaita_light()
}

#[cfg(feature = "io")]
fn discover_gnome_style() -> Result<SystemStyle, ()> {
    use crate::dynamic_selector::BoolCondition;
    
    let theme_name = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "gtk-theme"],
        Duration::from_secs(1),
    )?;
    let theme_name = theme_name.trim().trim_matches('\'');

    let color_scheme = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "color-scheme"],
        Duration::from_secs(1),
    )
    .unwrap_or_default();
    let theme = if color_scheme.contains("prefer-dark") {
        Theme::Dark
    } else {
        Theme::Light
    };

    let ui_font = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "font-name"],
        Duration::from_secs(1),
    )
    .ok();
    let monospace_font = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "monospace-font-name"],
        Duration::from_secs(1),
    )
    .ok();

    let mut style = if theme == Theme::Dark {
        defaults::gnome_adwaita_dark()
    } else {
        defaults::gnome_adwaita_light()
    };

    style.platform = Platform::Linux(DesktopEnvironment::Gnome);
    style.language = detect_system_language();
    style.os_version = detect_linux_version();
    style.prefers_reduced_motion = detect_gnome_reduced_motion();
    style.prefers_high_contrast = detect_gnome_high_contrast();
    
    if let Some(font) = ui_font {
        style.fonts.ui_font = OptionString::Some(font.trim().trim_matches('\'').to_string().into());
    }
    if let Some(font) = monospace_font {
        style.fonts.monospace_font =
            OptionString::Some(font.trim().trim_matches('\'').to_string().into());
    }

    Ok(style)
}

#[cfg(feature = "io")]
fn discover_kde_style() -> Result<SystemStyle, ()> {
    use crate::dynamic_selector::BoolCondition;
    
    // Check for kreadconfig5. If it doesn't exist, we're likely not on KDE Plasma 5+.
    run_command_with_timeout("kreadconfig5", &["--version"], Duration::from_secs(1))?;

    // Get the color scheme name to determine light/dark theme.
    let scheme_name = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "General", "--key", "ColorScheme"],
        Duration::from_secs(1),
    )
    .unwrap_or_default();
    let theme = if scheme_name.to_lowercase().contains("dark") {
        Theme::Dark
    } else {
        Theme::Light
    };

    // Start with the appropriate Breeze default.
    let mut style = if theme == Theme::Dark {
        // NOTE: A specific "breeze_dark" default could be added for more accuracy.
        defaults::gnome_adwaita_dark()
    } else {
        defaults::kde_breeze_light()
    };
    style.platform = Platform::Linux(DesktopEnvironment::Kde);
    style.language = detect_system_language();
    style.os_version = detect_linux_version();
    style.prefers_reduced_motion = detect_kde_reduced_motion();
    style.prefers_high_contrast = BoolCondition::False; // KDE doesn't have a standard high contrast setting

    // Get the UI font. The format is "Font Name,Size,-1,5,50,0,0,0,0,0"
    if let Ok(font_str) = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "General", "--key", "font"],
        Duration::from_secs(1),
    ) {
        let mut parts = font_str.trim().split(',');
        if let Some(font_name) = parts.next() {
            style.fonts.ui_font = OptionString::Some(font_name.to_string().into());
        }
        if let Some(font_size_str) = parts.next() {
            if let Ok(size) = font_size_str.parse::<f32>() {
                style.fonts.ui_font_size = OptionF32::Some(size);
            }
        }
    }

    // Get the monospace font.
    if let Ok(font_str) = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "General", "--key", "fixed"],
        Duration::from_secs(1),
    ) {
        if let Some(font_name) = font_str.trim().split(',').next() {
            style.fonts.monospace_font = OptionString::Some(font_name.to_string().into());
        }
    }

    // Get the accent color (active titlebar color). Format is "R,G,B".
    if let Ok(color_str) = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "WM", "--key", "activeBackground"],
        Duration::from_secs(1),
    ) {
        let rgb: Vec<Result<u8, _>> = color_str
            .trim()
            .split(',')
            .map(|c| c.parse::<u8>())
            .collect();
        if rgb.len() == 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (&rgb[0], &rgb[1], &rgb[2]) {
                style.colors.accent = OptionColorU::Some(ColorU::new_rgb(*r, *g, *b));
            }
        }
    }

    Ok(style)
}

#[cfg(feature = "io")]
/// Attempts to discover styling from common "ricing" tools and window manager configs.
fn discover_riced_style() -> Result<SystemStyle, ()> {
    // We can confirm we're in a specific WM environment if needed.
    // For example, Hyprland sets this variable.
    let is_hyprland = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok();
    if !is_hyprland {
        // This function could be expanded to check for sway, i3, etc.
        // For now, we'll only proceed if we have a strong hint.
        return Err(());
    }

    let mut style = SystemStyle {
        platform: Platform::Linux(DesktopEnvironment::Other("Tiling WM".into())),
        // Start with a generic dark theme, as it's common for riced setups.
        ..defaults::gnome_adwaita_dark()
    };
    style.language = detect_system_language();

    // Strategy 3: Check for a `pywal` cache first, as it's a great source for colors.
    let home_dir = std::env::var("HOME").unwrap_or_default();
    let wal_cache_path = format!("{}/.cache/wal/colors.json", home_dir);
    if let Ok(json_content) = std::fs::read_to_string(wal_cache_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&json_content) {
            let colors = &json["colors"];
            style.colors.background = colors["color0"]
                .as_str()
                .and_then(|s| parse_css_color(s).ok())
                .map(OptionColorU::Some)
                .unwrap_or(OptionColorU::None);
            style.colors.text = colors["color7"]
                .as_str()
                .and_then(|s| parse_css_color(s).ok())
                .map(OptionColorU::Some)
                .unwrap_or(OptionColorU::None);
            style.colors.accent = colors["color4"]
                .as_str()
                .and_then(|s| parse_css_color(s).ok())
                .map(OptionColorU::Some)
                .unwrap_or(OptionColorU::None);
            style.theme = Theme::Dark; // Wal is often used with dark themes.
        }
    }

    // Strategy 2: Parse hyprland.conf for specifics like borders and radius.
    let hypr_conf_path = format!("{}/.config/hypr/hyprland.conf", home_dir);
    if let Ok(conf_content) = std::fs::read_to_string(hypr_conf_path) {
        for line in conf_content.lines() {
            let line = line.trim();
            if line.starts_with('#') || !line.contains('=') {
                continue;
            }
            let mut parts = line.splitn(2, '=').map(|s| s.trim());
            let key = parts.next();
            let value = parts.next();

            if let (Some(k), Some(v)) = (key, value) {
                match k {
                    "rounding" => {
                        if let Ok(px) = v.parse::<f32>() {
                            style.metrics.corner_radius = OptionPixelValue::Some(PixelValue::px(px));
                        }
                    }
                    "border_size" => {
                        if let Ok(px) = v.parse::<f32>() {
                            style.metrics.border_width = OptionPixelValue::Some(PixelValue::px(px));
                        }
                    }
                    // Use the active border as the accent color if `wal` didn't provide one.
                    "col.active_border" if style.colors.accent.is_none() => {
                        // Hyprland format is "rgba(RRGGBBAA)" or "rgb(RRGGBB)"
                        if let Some(hex_str) = v.split_whitespace().last() {
                            if let Ok(color) = parse_css_color(&format!("#{}", hex_str)) {
                                style.colors.accent = OptionColorU::Some(color);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Strategy 1: Finally, try to get the GTK font as a sensible default for UI text.
    if let Ok(font_str) = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "font-name"],
        Duration::from_secs(1),
    ) {
        if let Some(font_name) = font_str.trim().trim_matches('\'').split(' ').next() {
            style.fonts.ui_font = OptionString::Some(font_name.to_string().into());
        }
    }

    Ok(style)
}

#[cfg(feature = "io")]
fn discover_windows_style() -> SystemStyle {
    use crate::dynamic_selector::{BoolCondition, OsVersion};
    
    let mut style = defaults::windows_11_light(); // Start with a modern default
    style.platform = Platform::Windows;
    style.language = detect_system_language();
    style.os_version = detect_windows_version();
    style.prefers_reduced_motion = detect_windows_reduced_motion();
    style.prefers_high_contrast = detect_windows_high_contrast();

    let theme_val = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ],
        Duration::from_secs(1),
    );
    if let Ok(output) = theme_val {
        if output.contains("0x0") {
            style = defaults::windows_11_dark();
            style.os_version = detect_windows_version();
            style.prefers_reduced_motion = detect_windows_reduced_motion();
            style.prefers_high_contrast = detect_windows_high_contrast();
        }
    }

    let accent_val = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\DWM",
            "/v",
            "AccentColor",
        ],
        Duration::from_secs(1),
    );
    if let Ok(output) = accent_val {
        if let Some(hex_str) = output.split_whitespace().last() {
            if let Ok(hex_val) = u32::from_str_radix(hex_str.trim_start_matches("0x"), 16) {
                let a = (hex_val >> 24) as u8;
                let b = (hex_val >> 16) as u8;
                let g = (hex_val >> 8) as u8;
                let r = hex_val as u8;
                style.colors.accent = OptionColorU::Some(ColorU::new(r, g, b, a));
                // Windows uses accent color for selection by default
                style.colors.selection_background = OptionColorU::Some(ColorU::new(r, g, b, 255));
            }
        }
    }

    style
}

/// Detect Windows version from registry
#[cfg(feature = "io")]
fn detect_windows_version() -> crate::dynamic_selector::OsVersion {
    use crate::dynamic_selector::OsVersion;
    
    // Try to get Windows build number from registry
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            "CurrentBuildNumber",
        ],
        Duration::from_secs(1),
    ) {
        // Parse: "CurrentBuildNumber    REG_SZ    22631"
        for line in output.lines() {
            if line.contains("CurrentBuildNumber") {
                if let Some(build_str) = line.split_whitespace().last() {
                    if let Ok(build) = build_str.parse::<u32>() {
                        return match build {
                            // Windows 11 builds
                            22000..=22499 => OsVersion::WIN_11_21H2,
                            22500..=22620 => OsVersion::WIN_11_22H2,
                            22621..=22630 => OsVersion::WIN_11_23H2,
                            22631.. => OsVersion::WIN_11_24H2,
                            // Windows 10 builds
                            19041..=19042 => OsVersion::WIN_10_2004,
                            19043 => OsVersion::WIN_10_21H1,
                            19044 => OsVersion::WIN_10_21H2,
                            19045 => OsVersion::WIN_10_22H2,
                            18362..=18363 => OsVersion::WIN_10_1903,
                            17763 => OsVersion::WIN_10_1809,
                            17134 => OsVersion::WIN_10_1803,
                            16299 => OsVersion::WIN_10_1709,
                            15063 => OsVersion::WIN_10_1703,
                            14393 => OsVersion::WIN_10_1607,
                            10586 => OsVersion::WIN_10_1511,
                            10240 => OsVersion::WIN_10_1507,
                            // Older Windows
                            9600 => OsVersion::WIN_8_1,
                            9200 => OsVersion::WIN_8,
                            7601 => OsVersion::WIN_7,
                            6002 => OsVersion::WIN_VISTA,
                            2600 => OsVersion::WIN_XP,
                            _ => OsVersion::WIN_10, // Unknown build, assume Win10
                        };
                    }
                }
            }
        }
    }
    OsVersion::WIN_10 // Default fallback
}

/// Detect Windows reduced motion preference
#[cfg(feature = "io")]
fn detect_windows_reduced_motion() -> crate::dynamic_selector::BoolCondition {
    use crate::dynamic_selector::BoolCondition;
    
    // Check SystemParameters for animation settings
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\Desktop\WindowMetrics",
            "/v",
            "MinAnimate",
        ],
        Duration::from_secs(1),
    ) {
        if output.contains("0x0") {
            return BoolCondition::True;
        }
    }
    BoolCondition::False
}

/// Detect Windows high contrast preference
#[cfg(feature = "io")]
fn detect_windows_high_contrast() -> crate::dynamic_selector::BoolCondition {
    use crate::dynamic_selector::BoolCondition;
    
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\Accessibility\HighContrast",
            "/v",
            "Flags",
        ],
        Duration::from_secs(1),
    ) {
        // Check if HCF_HIGHCONTRASTON (bit 0) is set
        if let Some(hex_str) = output.split_whitespace().last() {
            if let Ok(flags) = u32::from_str_radix(hex_str.trim_start_matches("0x"), 16) {
                if flags & 1 != 0 {
                    return BoolCondition::True;
                }
            }
        }
    }
    BoolCondition::False
}

#[cfg(feature = "io")]
fn discover_macos_style() -> SystemStyle {
    use crate::dynamic_selector::BoolCondition;
    
    let mut style = defaults::macos_modern_light();
    style.platform = Platform::MacOs;
    style.language = detect_system_language();
    style.os_version = detect_macos_version();
    style.prefers_reduced_motion = detect_macos_reduced_motion();
    style.prefers_high_contrast = detect_macos_high_contrast();

    // Check dark mode
    let theme_val = run_command_with_timeout(
        "defaults",
        &["read", "-g", "AppleInterfaceStyle"],
        Duration::from_secs(1),
    );
    if theme_val.is_ok() {
        style = defaults::macos_modern_dark();
        style.os_version = detect_macos_version();
        style.prefers_reduced_motion = detect_macos_reduced_motion();
        style.prefers_high_contrast = detect_macos_high_contrast();
    }

    // Detect accent color (AppleAccentColor: -1=graphite, 0=red, 1=orange, 2=yellow, 3=green, 4=blue, 5=purple, 6=pink)
    if let Ok(accent_str) = run_command_with_timeout(
        "defaults",
        &["read", "-g", "AppleAccentColor"],
        Duration::from_secs(1),
    ) {
        let accent_color = match accent_str.trim() {
            "-1" => ColorU::new_rgb(142, 142, 147), // Graphite
            "0" => ColorU::new_rgb(255, 59, 48),    // Red
            "1" => ColorU::new_rgb(255, 149, 0),    // Orange
            "2" => ColorU::new_rgb(255, 204, 0),    // Yellow
            "3" => ColorU::new_rgb(40, 205, 65),    // Green
            "4" => ColorU::new_rgb(0, 122, 255),    // Blue (default)
            "5" => ColorU::new_rgb(175, 82, 222),   // Purple
            "6" => ColorU::new_rgb(255, 45, 85),    // Pink
            _ => ColorU::new_rgb(0, 122, 255),      // Default blue
        };
        style.colors.accent = OptionColorU::Some(accent_color);
    }

    // Detect highlight (selection) color
    // AppleHighlightColor format: "R G B" (0.0-1.0 floats, space-separated)
    if let Ok(highlight_str) = run_command_with_timeout(
        "defaults",
        &["read", "-g", "AppleHighlightColor"],
        Duration::from_secs(1),
    ) {
        let parts: Vec<&str> = highlight_str.trim().split_whitespace().collect();
        if parts.len() >= 3 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                parts[0].parse::<f32>(),
                parts[1].parse::<f32>(),
                parts[2].parse::<f32>(),
            ) {
                // Use 50% transparency for selection background (a=128)
                let selection_color = ColorU::new(
                    (r * 255.0) as u8,
                    (g * 255.0) as u8,
                    (b * 255.0) as u8,
                    128, // Semi-transparent for text selection
                );
                style.colors.selection_background = OptionColorU::Some(selection_color);
                // Selection text color: use theme-appropriate text color
                // (dark text on light theme, light text on dark theme)
                let selection_text = match style.theme {
                    Theme::Dark => ColorU::new_rgb(255, 255, 255),
                    Theme::Light => ColorU::new_rgb(0, 0, 0),
                };
                style.colors.selection_text = OptionColorU::Some(selection_text);
            }
        }
    }

    style
}

/// Detect macOS version from sw_vers
#[cfg(feature = "io")]
fn detect_macos_version() -> crate::dynamic_selector::OsVersion {
    use crate::dynamic_selector::OsVersion;
    
    if let Ok(output) = run_command_with_timeout(
        "sw_vers",
        &["-productVersion"],
        Duration::from_secs(1),
    ) {
        let version = output.trim();
        // Parse "14.3.1" -> (14, 3, 1)
        let parts: Vec<&str> = version.split('.').collect();
        if let Some(major_str) = parts.first() {
            if let Ok(major) = major_str.parse::<u32>() {
                return match major {
                    26 => OsVersion::MACOS_TAHOE,
                    15 => OsVersion::MACOS_SEQUOIA,
                    14 => OsVersion::MACOS_SONOMA,
                    13 => OsVersion::MACOS_VENTURA,
                    12 => OsVersion::MACOS_MONTEREY,
                    11 => OsVersion::MACOS_BIG_SUR,
                    10 => {
                        // Parse minor version for 10.x
                        if let Some(minor_str) = parts.get(1) {
                            if let Ok(minor) = minor_str.parse::<u32>() {
                                return match minor {
                                    15 => OsVersion::MACOS_CATALINA,
                                    14 => OsVersion::MACOS_MOJAVE,
                                    13 => OsVersion::MACOS_HIGH_SIERRA,
                                    12 => OsVersion::MACOS_SIERRA,
                                    11 => OsVersion::MACOS_EL_CAPITAN,
                                    10 => OsVersion::MACOS_YOSEMITE,
                                    9 => OsVersion::MACOS_MAVERICKS,
                                    _ => OsVersion::MACOS_CATALINA, // Default 10.x
                                };
                            }
                        }
                        OsVersion::MACOS_CATALINA
                    }
                    _ => OsVersion::MACOS_SONOMA, // Unknown, assume recent
                };
            }
        }
    }
    OsVersion::MACOS_SONOMA // Default fallback
}

/// Detect macOS reduced motion preference
#[cfg(feature = "io")]
fn detect_macos_reduced_motion() -> crate::dynamic_selector::BoolCondition {
    use crate::dynamic_selector::BoolCondition;
    
    if let Ok(output) = run_command_with_timeout(
        "defaults",
        &["read", "-g", "com.apple.universalaccess", "reduceMotion"],
        Duration::from_secs(1),
    ) {
        if output.trim() == "1" {
            return BoolCondition::True;
        }
    }
    BoolCondition::False
}

/// Detect macOS high contrast preference
#[cfg(feature = "io")]
fn detect_macos_high_contrast() -> crate::dynamic_selector::BoolCondition {
    use crate::dynamic_selector::BoolCondition;
    
    if let Ok(output) = run_command_with_timeout(
        "defaults",
        &["read", "-g", "com.apple.universalaccess", "increaseContrast"],
        Duration::from_secs(1),
    ) {
        if output.trim() == "1" {
            return BoolCondition::True;
        }
    }
    BoolCondition::False
}

// -- Linux Detection Functions --

/// Detect Linux kernel version from uname
#[cfg(feature = "io")]
fn detect_linux_version() -> crate::dynamic_selector::OsVersion {
    use crate::dynamic_selector::OsVersion;
    
    if let Ok(output) = run_command_with_timeout(
        "uname",
        &["-r"],
        Duration::from_secs(1),
    ) {
        // Parse "6.5.0-44-generic" -> (6, 5, 0)
        let version = output.trim();
        let parts: Vec<&str> = version.split('.').collect();
        if let Some(major_str) = parts.first() {
            if let Ok(major) = major_str.parse::<u32>() {
                return match major {
                    6 => OsVersion::LINUX_6_0,
                    5 => OsVersion::LINUX_5_0,
                    4 => OsVersion::LINUX_4_0,
                    3 => OsVersion::LINUX_3_0,
                    2 => OsVersion::LINUX_2_6,
                    _ => OsVersion::LINUX_6_0, // Unknown, assume recent
                };
            }
        }
    }
    OsVersion::LINUX_6_0 // Default fallback
}

/// Detect GNOME reduced motion preference
#[cfg(feature = "io")]
fn detect_gnome_reduced_motion() -> crate::dynamic_selector::BoolCondition {
    use crate::dynamic_selector::BoolCondition;
    
    if let Ok(output) = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.interface", "enable-animations"],
        Duration::from_secs(1),
    ) {
        // If animations are disabled, user prefers reduced motion
        if output.trim() == "false" {
            return BoolCondition::True;
        }
    }
    BoolCondition::False
}

/// Detect GNOME high contrast preference
#[cfg(feature = "io")]
fn detect_gnome_high_contrast() -> crate::dynamic_selector::BoolCondition {
    use crate::dynamic_selector::BoolCondition;
    
    if let Ok(output) = run_command_with_timeout(
        "gsettings",
        &["get", "org.gnome.desktop.a11y.interface", "high-contrast"],
        Duration::from_secs(1),
    ) {
        if output.trim() == "true" {
            return BoolCondition::True;
        }
    }
    BoolCondition::False
}

/// Detect KDE reduced motion preference
#[cfg(feature = "io")]
fn detect_kde_reduced_motion() -> crate::dynamic_selector::BoolCondition {
    use crate::dynamic_selector::BoolCondition;
    
    // KDE stores animation speed in kdeglobals
    if let Ok(output) = run_command_with_timeout(
        "kreadconfig5",
        &["--group", "KDE", "--key", "AnimationDurationFactor"],
        Duration::from_secs(1),
    ) {
        // Factor of 0 means no animations
        if let Ok(factor) = output.trim().parse::<f32>() {
            if factor == 0.0 {
                return BoolCondition::True;
            }
        }
    }
    BoolCondition::False
}

/// Detect Linux desktop environment from environment variables
#[cfg(feature = "io")]
pub fn detect_linux_desktop_env() -> DesktopEnvironment {
    // Check XDG_CURRENT_DESKTOP first (most reliable)
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        let desktop = desktop.to_lowercase();
        if desktop.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if desktop.contains("kde") || desktop.contains("plasma") {
            return DesktopEnvironment::Kde;
        }
        if desktop.contains("xfce") {
            return DesktopEnvironment::Other("XFCE".into());
        }
        if desktop.contains("unity") {
            return DesktopEnvironment::Other("Unity".into());
        }
        if desktop.contains("cinnamon") {
            return DesktopEnvironment::Other("Cinnamon".into());
        }
        if desktop.contains("mate") {
            return DesktopEnvironment::Other("MATE".into());
        }
        if desktop.contains("lxde") || desktop.contains("lxqt") {
            return DesktopEnvironment::Other(desktop.to_uppercase().into());
        }
        if desktop.contains("hyprland") {
            return DesktopEnvironment::Other("Hyprland".into());
        }
        if desktop.contains("sway") {
            return DesktopEnvironment::Other("Sway".into());
        }
        if desktop.contains("i3") {
            return DesktopEnvironment::Other("i3".into());
        }
    }
    
    // Check DESKTOP_SESSION as fallback
    if let Ok(session) = std::env::var("DESKTOP_SESSION") {
        let session = session.to_lowercase();
        if session.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if session.contains("plasma") || session.contains("kde") {
            return DesktopEnvironment::Kde;
        }
    }
    
    // Check for specific environment markers
    if std::env::var("GNOME_DESKTOP_SESSION_ID").is_ok() {
        return DesktopEnvironment::Gnome;
    }
    if std::env::var("KDE_FULL_SESSION").is_ok() {
        return DesktopEnvironment::Kde;
    }
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return DesktopEnvironment::Other("Hyprland".into());
    }
    if std::env::var("SWAYSOCK").is_ok() {
        return DesktopEnvironment::Other("Sway".into());
    }
    if std::env::var("I3SOCK").is_ok() {
        return DesktopEnvironment::Other("i3".into());
    }
    
    DesktopEnvironment::Other("Unknown".into())
}

#[cfg(feature = "io")]
fn discover_android_style() -> SystemStyle {
    // On-device detection is complex; return a modern default.
    defaults::android_material_light()
}

#[cfg(feature = "io")]
fn discover_ios_style() -> SystemStyle {
    // On-device detection is complex; return a modern default.
    defaults::ios_light()
}

// -- Helper Functions (IO-dependent) --

#[cfg(feature = "io")]
/// A simple helper to run a command and get its stdout, with a timeout.
fn run_command_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Result<String, ()> {
    use std::{
        process::{Command, Stdio},
        thread,
    };

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| ())?;

    let (tx, rx) = std::sync::mpsc::channel();

    let child_thread = thread::spawn(move || {
        let output = child.wait_with_output();
        tx.send(output).ok();
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) if output.status.success() => {
            Ok(String::from_utf8(output.stdout).unwrap_or_default())
        }
        _ => {
            // Ensure the child process is killed on timeout
            // This part is tricky without a more robust process management library
            child_thread.join().ok(); // Wait for the thread to finish
            Err(())
        }
    }
}

/// Loads an application-specific stylesheet from a conventional path.
///
/// Looks for `<config_dir>/azul/styles/<exe_name>.css`.
/// Returns `None` if the file doesn't exist, can't be read, or is empty.
#[cfg(feature = "io")]
/// Loads an application-specific stylesheet from a conventional path.
///
/// Looks for `<config_dir>/azul/styles/<exe_name>.css`.
/// Returns `None` if the file doesn't exist, can't be read, or is empty.
#[cfg(feature = "io")]
fn load_app_specific_stylesheet() -> Option<Stylesheet> {
    // Get the name of the currently running executable.
    let exe_path = std::env::current_exe().ok()?;
    let exe_name = exe_path.file_name()?.to_str()?;

    // Use `dirs-next` to find the conventional config directory for the current platform.
    // This correctly handles Linux ($XDG_CONFIG_HOME, ~/.config),
    // macOS (~/Library/Application Support), and Windows (%APPDATA%).
    let config_dir = dirs_next::config_dir()?;

    let css_path = config_dir
        .join("azul")
        .join("styles")
        .join(format!("{}.css", exe_name));

    // If the file doesn't exist or can't be read, `ok()` will gracefully convert the error
    // to `None`, which will then be returned by the function.
    let css_content = std::fs::read_to_string(css_path).ok()?;

    if css_content.trim().is_empty() {
        return None;
    }

    let (mut css, _warnings) = new_from_str(&css_content);

    // The parser returns a `Css` which contains a `Vec<Stylesheet>`.
    // For an app-specific theme file, we are only interested in the first stylesheet.
    if !css.stylesheets.is_empty() {
        let mut owned_vec = css.stylesheets.into_library_owned_vec();
        Some(owned_vec.remove(0))
    } else {
        None
    }
}

// -- Language Detection Functions --

/// Detect the system language and return a BCP 47 language tag.
/// Falls back to "en-US" if detection fails.
#[cfg(feature = "io")]
pub fn detect_system_language() -> AzString {
    #[cfg(target_os = "windows")]
    {
        detect_language_windows()
    }
    #[cfg(target_os = "macos")]
    {
        detect_language_macos()
    }
    #[cfg(target_os = "linux")]
    {
        detect_language_linux()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        AzString::from_const_str("en-US")
    }
}

/// Detect language on Windows using PowerShell
#[cfg(all(feature = "io", target_os = "windows"))]
fn detect_language_windows() -> AzString {
    // Try to get the system UI culture via PowerShell
    if let Ok(output) = run_command_with_timeout(
        "powershell",
        &["-Command", "(Get-Culture).Name"],
        Duration::from_secs(2),
    ) {
        let lang = output.trim();
        if !lang.is_empty() && lang.contains('-') {
            return AzString::from(lang.to_string());
        }
    }

    // Fallback: try registry
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\International",
            "/v",
            "LocaleName",
        ],
        Duration::from_secs(1),
    ) {
        // Parse registry output: "LocaleName    REG_SZ    de-DE"
        for line in output.lines() {
            if line.contains("LocaleName") {
                if let Some(lang) = line.split_whitespace().last() {
                    let lang = lang.trim();
                    if !lang.is_empty() {
                        return AzString::from(lang.to_string());
                    }
                }
            }
        }
    }

    AzString::from_const_str("en-US")
}

/// Detect language on macOS using defaults command
#[cfg(all(feature = "io", target_os = "macos"))]
fn detect_language_macos() -> AzString {
    // Try AppleLocale first (more specific)
    if let Ok(output) = run_command_with_timeout(
        "defaults",
        &["read", "-g", "AppleLocale"],
        Duration::from_secs(1),
    ) {
        let locale = output.trim();
        if !locale.is_empty() {
            // Convert underscore to hyphen: "de_DE" -> "de-DE"
            return AzString::from(locale.replace('_', "-"));
        }
    }

    // Fallback: try AppleLanguages array
    if let Ok(output) = run_command_with_timeout(
        "defaults",
        &["read", "-g", "AppleLanguages"],
        Duration::from_secs(1),
    ) {
        // Output is a plist array, extract first language
        // Example: "(\n    \"de-DE\",\n    \"en-US\"\n)"
        for line in output.lines() {
            let trimmed = line
                .trim()
                .trim_matches(|c| c == '"' || c == ',' || c == '(' || c == ')');
            if !trimmed.is_empty() && trimmed.contains('-') {
                return AzString::from(trimmed.to_string());
            }
        }
    }

    AzString::from_const_str("en-US")
}

/// Detect language on Linux using environment variables
#[cfg(all(feature = "io", target_os = "linux"))]
fn detect_language_linux() -> AzString {
    // Check LANGUAGE, LANG, LC_ALL, LC_MESSAGES in order of priority
    let env_vars = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"];

    for var in &env_vars {
        if let Ok(value) = std::env::var(var) {
            let value = value.trim();
            if value.is_empty() || value == "C" || value == "POSIX" {
                continue;
            }

            // Parse locale format: "de_DE.UTF-8" or "de_DE" or "de"
            let lang = value
                .split('.')  // Remove .UTF-8 suffix
                .next()
                .unwrap_or(value)
                .replace('_', "-"); // Convert to BCP 47

            if !lang.is_empty() {
                return AzString::from(lang);
            }
        }
    }

    AzString::from_const_str("en-US")
}

/// Default language when io feature is disabled
#[cfg(not(feature = "io"))]
pub fn detect_system_language() -> AzString {
    AzString::from_const_str("en-US")
}

pub mod defaults {
    //! A collection of hard-coded system style defaults that mimic the appearance
    //! of various operating systems and desktop environments. These are used as a
    //! fallback when the "io" feature is disabled, ensuring deterministic styles
    //! for testing and environments where system calls are not desired.

    use crate::{
        corety::{AzString, OptionF32, OptionString},
        dynamic_selector::{BoolCondition, OsVersion},
        props::{
            basic::{
                color::{ColorU, OptionColorU},
                pixel::{PixelValue, OptionPixelValue},
            },
            layout::{
                dimensions::LayoutWidth,
                spacing::{LayoutPaddingLeft, LayoutPaddingRight},
            },
            style::{
                background::StyleBackgroundContent,
                scrollbar::{
                    ComputedScrollbarStyle, OverflowScrolling, OverscrollBehavior, ScrollBehavior,
                    ScrollPhysics, ScrollbarInfo,
                    SCROLLBAR_ANDROID_DARK, SCROLLBAR_ANDROID_LIGHT, SCROLLBAR_CLASSIC_DARK,
                    SCROLLBAR_CLASSIC_LIGHT, SCROLLBAR_IOS_DARK, SCROLLBAR_IOS_LIGHT,
                    SCROLLBAR_MACOS_DARK, SCROLLBAR_MACOS_LIGHT, SCROLLBAR_WINDOWS_DARK,
                    SCROLLBAR_WINDOWS_LIGHT,
                },
            },
        },
        system::{
            DesktopEnvironment, Platform, SystemColors, SystemFonts, SystemMetrics, SystemStyle,
            Theme, IconStyleOptions, TitlebarMetrics,
        },
    };

    // --- Custom Scrollbar Style Constants for Nostalgia ---

    /// A scrollbar style mimicking the classic Windows 95/98/2000/XP look.
    pub const SCROLLBAR_WINDOWS_CLASSIC: ScrollbarInfo = ScrollbarInfo {
        width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(17)),
        padding_left: LayoutPaddingLeft {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        padding_right: LayoutPaddingRight {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        track: StyleBackgroundContent::Color(ColorU {
            r: 223,
            g: 223,
            b: 223,
            a: 255,
        }), // Scrollbar trough color
        thumb: StyleBackgroundContent::Color(ColorU {
            r: 208,
            g: 208,
            b: 208,
            a: 255,
        }), // Button face color
        button: StyleBackgroundContent::Color(ColorU {
            r: 208,
            g: 208,
            b: 208,
            a: 255,
        }),
        corner: StyleBackgroundContent::Color(ColorU {
            r: 223,
            g: 223,
            b: 223,
            a: 255,
        }),
        resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
        clip_to_container_border: false,
        scroll_behavior: ScrollBehavior::Auto,
        overscroll_behavior_x: OverscrollBehavior::None,
        overscroll_behavior_y: OverscrollBehavior::None,
        overflow_scrolling: OverflowScrolling::Auto,
    };

    /// A scrollbar style mimicking the macOS "Aqua" theme from the early 2000s.
    pub const SCROLLBAR_MACOS_AQUA: ScrollbarInfo = ScrollbarInfo {
        width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(15)),
        padding_left: LayoutPaddingLeft {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        padding_right: LayoutPaddingRight {
            inner: crate::props::basic::pixel::PixelValue::const_px(0),
        },
        track: StyleBackgroundContent::Color(ColorU {
            r: 238,
            g: 238,
            b: 238,
            a: 128,
        }), // Translucent track
        thumb: StyleBackgroundContent::Color(ColorU {
            r: 105,
            g: 173,
            b: 255,
            a: 255,
        }), // "Gel" blue
        button: StyleBackgroundContent::Color(ColorU {
            r: 105,
            g: 173,
            b: 255,
            a: 255,
        }),
        corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
        resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
        clip_to_container_border: true,
        scroll_behavior: ScrollBehavior::Smooth,
        overscroll_behavior_x: OverscrollBehavior::Auto,
        overscroll_behavior_y: OverscrollBehavior::Auto,
        overflow_scrolling: OverflowScrolling::Auto,
    };

    /// A scrollbar style mimicking the KDE Oxygen theme.
    pub const SCROLLBAR_KDE_OXYGEN: ScrollbarInfo = ScrollbarInfo {
        width: LayoutWidth::Px(crate::props::basic::pixel::PixelValue::const_px(14)),
        padding_left: LayoutPaddingLeft {
            inner: crate::props::basic::pixel::PixelValue::const_px(2),
        },
        padding_right: LayoutPaddingRight {
            inner: crate::props::basic::pixel::PixelValue::const_px(2),
        },
        track: StyleBackgroundContent::Color(ColorU {
            r: 242,
            g: 242,
            b: 242,
            a: 255,
        }),
        thumb: StyleBackgroundContent::Color(ColorU {
            r: 177,
            g: 177,
            b: 177,
            a: 255,
        }),
        button: StyleBackgroundContent::Color(ColorU {
            r: 216,
            g: 216,
            b: 216,
            a: 255,
        }),
        corner: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
        resizer: StyleBackgroundContent::Color(ColorU::TRANSPARENT),
        clip_to_container_border: false,
        scroll_behavior: ScrollBehavior::Auto,
        overscroll_behavior_x: OverscrollBehavior::Auto,
        overscroll_behavior_y: OverscrollBehavior::Auto,
        overflow_scrolling: OverflowScrolling::Auto,
    };

    /// Helper to convert a detailed `ScrollbarInfo` into the simplified `ComputedScrollbarStyle`.
    fn scrollbar_info_to_computed(info: &ScrollbarInfo) -> ComputedScrollbarStyle {
        ComputedScrollbarStyle {
            width: Some(info.width.clone()),
            thumb_color: match info.thumb {
                StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            },
            track_color: match info.track {
                StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            },
        }
    }

    // --- Windows Styles ---

    pub fn windows_11_light() -> SystemStyle {
        SystemStyle {
            theme: Theme::Light,
            platform: Platform::Windows,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                background: OptionColorU::Some(ColorU::new_rgb(243, 243, 243)),
                accent: OptionColorU::Some(ColorU::new_rgb(0, 95, 184)),
                window_background: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                selection_background: OptionColorU::Some(ColorU::new_rgb(0, 120, 215)),
                selection_text: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Segoe UI Variable Text".into()),
                ui_font_size: OptionF32::Some(9.0),
                monospace_font: OptionString::Some("Consolas".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(4.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(12.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(6.0)),
                titlebar: TitlebarMetrics::windows(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_WINDOWS_LIGHT))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_11,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            ..Default::default()
        }
    }

    pub fn windows_11_dark() -> SystemStyle {
        SystemStyle {
            theme: Theme::Dark,
            platform: Platform::Windows,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                background: OptionColorU::Some(ColorU::new_rgb(32, 32, 32)),
                accent: OptionColorU::Some(ColorU::new_rgb(0, 120, 215)),
                window_background: OptionColorU::Some(ColorU::new_rgb(25, 25, 25)),
                selection_background: OptionColorU::Some(ColorU::new_rgb(0, 120, 215)),
                selection_text: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Segoe UI Variable Text".into()),
                ui_font_size: OptionF32::Some(9.0),
                monospace_font: OptionString::Some("Consolas".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(4.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(12.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(6.0)),
                titlebar: TitlebarMetrics::windows(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_WINDOWS_DARK))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_11,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            ..Default::default()
        }
    }

    pub fn windows_7_aero() -> SystemStyle {
        SystemStyle {
            theme: Theme::Light,
            platform: Platform::Windows,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                background: OptionColorU::Some(ColorU::new_rgb(240, 240, 240)),
                accent: OptionColorU::Some(ColorU::new_rgb(51, 153, 255)),
                window_background: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                selection_background: OptionColorU::Some(ColorU::new_rgb(51, 153, 255)),
                selection_text: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Segoe UI".into()),
                ui_font_size: OptionF32::Some(9.0),
                monospace_font: OptionString::Some("Consolas".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(6.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(10.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(5.0)),
                titlebar: TitlebarMetrics::windows(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_LIGHT))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_7,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            ..Default::default()
        }
    }

    pub fn windows_xp_luna() -> SystemStyle {
        SystemStyle {
            theme: Theme::Light,
            platform: Platform::Windows,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                background: OptionColorU::Some(ColorU::new_rgb(236, 233, 216)),
                accent: OptionColorU::Some(ColorU::new_rgb(49, 106, 197)),
                window_background: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                selection_background: OptionColorU::Some(ColorU::new_rgb(49, 106, 197)),
                selection_text: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Tahoma".into()),
                ui_font_size: OptionF32::Some(8.0),
                monospace_font: OptionString::Some("Lucida Console".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(3.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(8.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(4.0)),
                titlebar: TitlebarMetrics::windows(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_WINDOWS_CLASSIC))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_XP,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            ..Default::default()
        }
    }

    // --- macOS Styles ---

    pub fn macos_modern_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::MacOs,
            theme: Theme::Light,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new(0, 0, 0, 221)),
                background: OptionColorU::Some(ColorU::new_rgb(242, 242, 247)),
                accent: OptionColorU::Some(ColorU::new_rgb(0, 122, 255)),
                window_background: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                // Default macOS selection uses accent color with transparency
                selection_background: OptionColorU::Some(ColorU::new(0, 122, 255, 128)),
                selection_text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some(".SF NS".into()),
                ui_font_size: OptionF32::Some(13.0),
                monospace_font: OptionString::Some("Menlo".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(8.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(16.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(6.0)),
                titlebar: TitlebarMetrics::macos(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_MACOS_LIGHT))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::MACOS_SONOMA,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::macos(),
            ..Default::default()
        }
    }

    pub fn macos_modern_dark() -> SystemStyle {
        SystemStyle {
            platform: Platform::MacOs,
            theme: Theme::Dark,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new(255, 255, 255, 221)),
                background: OptionColorU::Some(ColorU::new_rgb(28, 28, 30)),
                accent: OptionColorU::Some(ColorU::new_rgb(10, 132, 255)),
                window_background: OptionColorU::Some(ColorU::new_rgb(44, 44, 46)),
                // Default macOS selection uses accent color with transparency
                selection_background: OptionColorU::Some(ColorU::new(10, 132, 255, 128)),
                selection_text: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some(".SF NS".into()),
                ui_font_size: OptionF32::Some(13.0),
                monospace_font: OptionString::Some("SF Mono".into()),
                monospace_font_size: OptionF32::Some(12.0),
                title_font: OptionString::Some(".SF NS".into()),
                title_font_size: OptionF32::Some(13.0),
                menu_font: OptionString::Some(".SF NS".into()),
                menu_font_size: OptionF32::Some(13.0),
                small_font: OptionString::Some(".SF NS".into()),
                small_font_size: OptionF32::Some(11.0),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(8.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(16.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(6.0)),
                titlebar: TitlebarMetrics::macos(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_MACOS_DARK))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::MACOS_SONOMA,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::macos(),
            ..Default::default()
        }
    }

    pub fn macos_aqua() -> SystemStyle {
        SystemStyle {
            platform: Platform::MacOs,
            theme: Theme::Light,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                background: OptionColorU::Some(ColorU::new_rgb(229, 229, 229)),
                accent: OptionColorU::Some(ColorU::new_rgb(63, 128, 234)),
                window_background: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Lucida Grande".into()),
                ui_font_size: OptionF32::Some(13.0),
                monospace_font: OptionString::Some("Monaco".into()),
                monospace_font_size: OptionF32::Some(12.0),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(12.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(16.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(6.0)),
                titlebar: TitlebarMetrics::macos(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_MACOS_AQUA))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::MACOS_TIGER,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::macos(),
            ..Default::default()
        }
    }

    // --- Linux Styles ---

    pub fn gnome_adwaita_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Gnome),
            theme: Theme::Light,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(46, 52, 54)),
                background: OptionColorU::Some(ColorU::new_rgb(249, 249, 249)),
                accent: OptionColorU::Some(ColorU::new_rgb(53, 132, 228)),
                window_background: OptionColorU::Some(ColorU::new_rgb(237, 237, 237)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Cantarell".into()),
                ui_font_size: OptionF32::Some(11.0),
                monospace_font: OptionString::Some("Monospace".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(4.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(12.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(8.0)),
                titlebar: TitlebarMetrics::linux_gnome(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_LIGHT))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_6_0,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            ..Default::default()
        }
    }

    pub fn gnome_adwaita_dark() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Gnome),
            theme: Theme::Dark,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(238, 238, 236)),
                background: OptionColorU::Some(ColorU::new_rgb(36, 36, 36)),
                accent: OptionColorU::Some(ColorU::new_rgb(53, 132, 228)),
                window_background: OptionColorU::Some(ColorU::new_rgb(48, 48, 48)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Cantarell".into()),
                ui_font_size: OptionF32::Some(11.0),
                monospace_font: OptionString::Some("Monospace".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(4.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(12.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(8.0)),
                titlebar: TitlebarMetrics::linux_gnome(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_DARK))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_6_0,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            ..Default::default()
        }
    }

    pub fn gtk2_clearlooks() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Gnome),
            theme: Theme::Light,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                background: OptionColorU::Some(ColorU::new_rgb(239, 239, 239)),
                accent: OptionColorU::Some(ColorU::new_rgb(245, 121, 0)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("DejaVu Sans".into()),
                ui_font_size: OptionF32::Some(10.0),
                monospace_font: OptionString::Some("DejaVu Sans Mono".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(4.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(10.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(6.0)),
                titlebar: TitlebarMetrics::linux_gnome(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_CLASSIC_LIGHT))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_2_6,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            ..Default::default()
        }
    }

    pub fn kde_breeze_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Linux(DesktopEnvironment::Kde),
            theme: Theme::Light,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(31, 36, 39)),
                background: OptionColorU::Some(ColorU::new_rgb(239, 240, 241)),
                accent: OptionColorU::Some(ColorU::new_rgb(61, 174, 233)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Noto Sans".into()),
                ui_font_size: OptionF32::Some(10.0),
                monospace_font: OptionString::Some("Hack".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(4.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(12.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(6.0)),
                titlebar: TitlebarMetrics::linux_gnome(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_KDE_OXYGEN))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_6_0,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            ..Default::default()
        }
    }

    // --- Mobile Styles ---

    pub fn android_material_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Android,
            theme: Theme::Light,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                background: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                accent: OptionColorU::Some(ColorU::new_rgb(98, 0, 238)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Roboto".into()),
                ui_font_size: OptionF32::Some(14.0),
                monospace_font: OptionString::Some("Droid Sans Mono".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(12.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(16.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(10.0)),
                titlebar: TitlebarMetrics::android(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_ANDROID_LIGHT))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::ANDROID_14,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::android(),
            ..Default::default()
        }
    }

    pub fn android_holo_dark() -> SystemStyle {
        SystemStyle {
            platform: Platform::Android,
            theme: Theme::Dark,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(255, 255, 255)),
                background: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                accent: OptionColorU::Some(ColorU::new_rgb(51, 181, 229)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some("Roboto".into()),
                ui_font_size: OptionF32::Some(14.0),
                monospace_font: OptionString::Some("Droid Sans Mono".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(2.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(1.0)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(12.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(8.0)),
                titlebar: TitlebarMetrics::android(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_ANDROID_DARK))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::ANDROID_ICE_CREAM_SANDWICH,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::android(),
            ..Default::default()
        }
    }

    pub fn ios_light() -> SystemStyle {
        SystemStyle {
            platform: Platform::Ios,
            theme: Theme::Light,
            colors: SystemColors {
                text: OptionColorU::Some(ColorU::new_rgb(0, 0, 0)),
                background: OptionColorU::Some(ColorU::new_rgb(242, 242, 247)),
                accent: OptionColorU::Some(ColorU::new_rgb(0, 122, 255)),
                ..Default::default()
            },
            fonts: SystemFonts {
                ui_font: OptionString::Some(".SFUI-Display-Regular".into()),
                ui_font_size: OptionF32::Some(17.0),
                monospace_font: OptionString::Some("Menlo".into()),
                ..Default::default()
            },
            metrics: SystemMetrics {
                corner_radius: OptionPixelValue::Some(PixelValue::px(10.0)),
                border_width: OptionPixelValue::Some(PixelValue::px(0.5)),
                button_padding_horizontal: OptionPixelValue::Some(PixelValue::px(20.0)),
                button_padding_vertical: OptionPixelValue::Some(PixelValue::px(12.0)),
                titlebar: TitlebarMetrics::ios(),
            },
            scrollbar: Some(Box::new(scrollbar_info_to_computed(&SCROLLBAR_IOS_LIGHT))),
            app_specific_stylesheet: None,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::IOS_17,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::ios(),
            ..Default::default()
        }
    }
}

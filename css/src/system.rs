//! Discovers system-native styling for colors, fonts, and other metrics.
//!
//! This module provides a best-effort attempt to query the host operating system
//! for its UI theme information. This is gated behind the **`io`** feature flag.
//!
//! **End-user customization (`AZ_RICING`):**
//! By default (if the `io` feature is enabled), Azul looks for an
//! application-specific stylesheet at `~/.config/azul/styles/<app_name>.css`
//! (or `%APPDATA%\azul\styles\<app_name>.css` on Windows) and applies it as
//! the last layer of the cascade, letting end-users "rice" any Azul app.
//!
//! The `AZ_RICING` env var has three modes (case-insensitive):
//!
//! - unset (default): load the user CSS if present; on Linux, the
//!   detection chain is `KDE > GNOME > riced > defaults`.
//! - `AZ_RICING=off` (aliases: `disabled`, `none`, `0`): skip the user
//!   CSS file and the riced-desktop sources (Hyprland config, pywal
//!   cache). Use for kiosk builds or CI runs that mustn't pick up local
//!   customization.
//! - `AZ_RICING=force` (aliases: `prefer`, `aggressive`, `1`): on Linux,
//!   reorder the detection chain so riced-desktop sources win over
//!   GNOME/KDE — useful for tiling-WM users whose `XDG_CURRENT_DESKTOP`
//!   still says `gnome`. The user CSS file still loads.

#![cfg(feature = "parser")]

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use crate::{
    corety::{AzString, OptionF32, OptionString, OptionU16},
    css::Css,
    parser2::{new_from_str, CssParseWarnMsg},
    props::{
        basic::{
            color::{parse_css_color, ColorU, OptionColorU},
            pixel::{PixelValue, OptionPixelValue},
        },
        style::scrollbar::{ComputedScrollbarStyle, OverscrollBehavior, ScrollBehavior, ScrollPhysics},
    },
};

use crate::dynamic_selector::{BoolCondition, OsVersion};
use core::fmt::Write;

// --- End-user customization mode ---

/// User-customization mode controlled by the `AZ_RICING` env var.
///
/// See the module-level documentation for the full description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(Default)]
pub enum RicingMode {
    /// `AZ_RICING=off` (or `disabled` / `none` / `0`). Skip the user
    /// CSS file *and* the riced-desktop sources. Vanilla detection.
    Off,
    /// Unset. Load the user CSS if present; standard detection chain
    /// (`KDE > GNOME > riced > defaults` on Linux).
    #[default]
    Default,
    /// `AZ_RICING=force` (or `prefer` / `aggressive` / `1`). Reorder
    /// the Linux detection chain so riced-desktop sources win over
    /// GNOME/KDE. The user CSS file still loads.
    Force,
}


/// Read the `AZ_RICING` env var and classify it. Case-insensitive.
/// Anything we don't recognise falls through to `Default` so a typo
/// degrades gracefully instead of disabling the feature silently.
#[must_use] pub fn ricing_mode() -> RicingMode {
    let Ok(raw) = std::env::var("AZ_RICING") else {
        return RicingMode::Default;
    };
    match raw.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" | "none" | "0" | "false" => RicingMode::Off,
        "force" | "prefer" | "aggressive" | "1" | "true" => RicingMode::Force,
        _ => RicingMode::Default,
    }
}

/// True when the user CSS file at `~/.config/azul/styles/<app>.css`
/// should be read. False only when `AZ_RICING=off` is set.
#[must_use] pub fn ricing_enabled() -> bool {
    !matches!(ricing_mode(), RicingMode::Off)
}

// --- Public Data Structures ---
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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
    #[must_use] pub const fn current() -> Self {
        #[cfg(target_os = "macos")]
        { Self::MacOs }
        #[cfg(target_os = "windows")]
        { Self::Windows }
        #[cfg(target_os = "linux")]
        { Self::Linux(DesktopEnvironment::Other(AzString::from_const_str("unknown"))) }
        #[cfg(target_os = "android")]
        { Self::Android }
        #[cfg(target_os = "ios")]
        { Self::Ios }
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "android", target_os = "ios")))]
        { Self::Unknown }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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
#[derive(Debug, Clone, PartialEq)]
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
    /// application-specific "ricing". Only loaded when the "io" feature
    /// is enabled and `AZ_RICING` is not set to `off`.
    pub app_specific_stylesheet: Option<Box<Css>>,
    /// Scrollbar style information (boxed to ensure stable FFI size)
    pub scrollbar: Option<Box<ComputedScrollbarStyle>>,
    /// Global scroll physics configuration (momentum, friction, rubber-banding).
    /// Platform-specific defaults are applied during system style discovery.
    /// Applications can override this to change the "feel" of scrolling globally.
    pub scroll_physics: ScrollPhysics,
    pub theme: Theme,
    /// Detected OS version (e.g., Windows 11 22H2, macOS Sonoma, etc.)
    pub os_version: OsVersion,
    /// User prefers reduced motion (accessibility setting)
    pub prefers_reduced_motion: BoolCondition,
    /// User prefers high contrast (accessibility setting)
    pub prefers_high_contrast: BoolCondition,
    /// Detailed accessibility settings (superset of `prefers_reduced_motion` / `prefers_high_contrast`)
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
    /// FFI double-drop guard. `SystemStyle` owns two heap pointers
    /// (`app_specific_stylesheet`, `scrollbar`). The codegen Az wrapper
    /// (`AzSystemStyle`) gets an `impl Drop` -> `AzSystemStyle_delete` ->
    /// `drop_in_place::<SystemStyle>`, and is nested by value as
    /// `AzAppConfig.system_style`. Dropping an `AzAppConfig` by value
    /// therefore drops the real `SystemStyle` once (freeing both Boxes) and
    /// then re-runs `_delete` on the SAME bytes via drop-glue -> double free.
    /// Same class as `GlContextPtr` / `IconProviderHandle` (see core/src/icon.rs).
    /// The first `Drop` disarms this flag; the second sees it cleared and
    /// neutralizes itself (takes + forgets the already-freed Boxes) so the
    /// redundant drop-glue is a no-op. Defaults to `true` (own + free once).
    pub run_destructor: bool,
}

impl Default for SystemStyle {
    fn default() -> Self {
        Self {
            fonts: SystemFonts::default(),
            metrics: SystemMetrics::default(),
            linux: LinuxCustomization::default(),
            platform: Platform::default(),
            focus_visuals: FocusVisuals::default(),
            language: AzString::default(),
            app_specific_stylesheet: None,
            scrollbar: None,
            scroll_physics: ScrollPhysics::default(),
            theme: Theme::default(),
            os_version: OsVersion::default(),
            prefers_reduced_motion: BoolCondition::default(),
            prefers_high_contrast: BoolCondition::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            colors: SystemColors::default(),
            icon_style: IconStyleOptions::default(),
            audio: AudioMetrics::default(),
            run_destructor: true,
        }
    }
}

impl Drop for SystemStyle {
    fn drop(&mut self) {
        // Gate the heap frees on `run_destructor` to defuse the codegen
        // double-drop (see the `run_destructor` field docs). drop_in_place
        // runs THIS method, then the field drop-glue; so:
        //  * FIRST drop (flag set): disarm the flag, then let the field
        //    drop-glue free the two Boxes exactly once.
        //  * SECOND drop on the same bytes (flag cleared by the first): the
        //    Boxes are already freed but the fields still hold dangling
        //    `Some(ptr)`. Take them out (-> None) and forget the dangling
        //    values so the trailing drop-glue is a no-op (never derefs/frees).
        if self.run_destructor {
            self.run_destructor = false;
        } else {
            core::mem::forget(self.app_specific_stylesheet.take());
            core::mem::forget(self.scrollbar.take());
        }
    }
}

/// Icon-specific styling options for accessibility and theming.
///
/// These settings affect how icons are rendered, supporting accessibility
/// needs like reduced colors and high contrast modes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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
/// - Linux: Ubuntu Mono or `DejaVu` Sans Mono
/// 
/// Font variants (bold, italic) can be combined with the base type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
#[derive(Default)]
pub enum SystemFontType {
    /// UI font for buttons, labels, menus (SF Pro, Segoe UI, Cantarell)
    #[default]
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


impl SystemFontType {
    /// Parse a `SystemFontType` from a CSS string.
    /// 
    /// Supported formats:
    /// - `system:ui`, `system:ui:bold`
    /// - `system:monospace`, `system:monospace:bold`, `system:monospace:italic`
    /// - `system:title`, `system:title:bold`
    /// - `system:menu`
    /// - `system:small`
    /// - `system:serif`, `system:serif:bold`
    #[must_use] pub fn from_css_str(s: &str) -> Option<Self> {
        let s = s.trim();
        if !s.starts_with("system:") {
            return None;
        }
        let rest = &s[7..]; // Skip "system:"
        match rest {
            "ui" => Some(Self::Ui),
            "ui:bold" => Some(Self::UiBold),
            "monospace" => Some(Self::Monospace),
            "monospace:bold" => Some(Self::MonospaceBold),
            "monospace:italic" => Some(Self::MonospaceItalic),
            "title" => Some(Self::Title),
            "title:bold" => Some(Self::TitleBold),
            "menu" => Some(Self::Menu),
            "small" => Some(Self::Small),
            "serif" => Some(Self::Serif),
            "serif:bold" => Some(Self::SerifBold),
            _ => None,
        }
    }
    
    /// Get the CSS syntax for this system font type.
    #[must_use] pub const fn as_css_str(&self) -> &'static str {
        match self {
            Self::Ui => "system:ui",
            Self::UiBold => "system:ui:bold",
            Self::Monospace => "system:monospace",
            Self::MonospaceBold => "system:monospace:bold",
            Self::MonospaceItalic => "system:monospace:italic",
            Self::Title => "system:title",
            Self::TitleBold => "system:title:bold",
            Self::Menu => "system:menu",
            Self::Small => "system:small",
            Self::Serif => "system:serif",
            Self::SerifBold => "system:serif:bold",
        }
    }
    
    /// Returns true if this system font type implies bold weight.
    /// Used when resolving system fonts to pass the correct weight to fontconfig.
    #[must_use] pub const fn is_bold(&self) -> bool {
        matches!(
            self,
            Self::UiBold
                | Self::MonospaceBold
                | Self::TitleBold
                | Self::SerifBold
        )
    }
    
    /// Returns true if this system font type implies italic style.
    #[must_use] pub const fn is_italic(&self) -> bool {
        matches!(self, Self::MonospaceItalic)
    }
}

/// Accessibility settings detected from the operating system.
/// 
/// These settings allow apps to adapt their UI for users with accessibility needs.
/// Detection methods:
/// - macOS: `UIAccessibility` APIs (isBoldTextEnabled, isReduceMotionEnabled, etc.)
/// - Windows: `SystemParametersInfo` (`SPI_GETHIGHCONTRAST`, `SPI_GETCLIENTAREAANIMATION`)
/// - Linux: gsettings (org.gnome.desktop.interface, org.gnome.desktop.a11y)
#[derive(Debug, Default, Clone, Copy, PartialEq)]
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
    /// Windows: `SystemParametersInfo` text scale factor
    /// Linux: org.gnome.desktop.interface text-scaling-factor
    pub prefers_larger_text: bool,
    /// User prefers high contrast colors
    /// macOS: UIAccessibility.isDarkerSystemColorsEnabled
    /// Windows: `SPI_GETHIGHCONTRAST`
    /// Linux: org.gnome.desktop.a11y.interface high-contrast
    pub prefers_high_contrast: bool,
    /// User prefers reduced motion/animations
    /// macOS: UIAccessibility.isReduceMotionEnabled
    /// Windows: `SPI_GETCLIENTAREAANIMATION` (inverted)
    /// Linux: org.gnome.desktop.interface enable-animations (inverted)
    pub prefers_reduced_motion: bool,
    /// User prefers reduced transparency
    /// macOS: UIAccessibility.isReduceTransparencyEnabled
    /// Windows: N/A
    /// Linux: N/A
    pub prefers_reduced_transparency: bool,
    /// Screen reader is active (`VoiceOver`, Narrator, Orca)
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
/// On macOS, these correspond to `NSColor` semantic colors.
/// On Windows, these come from `UISettings`.
/// On Linux/GTK, these come from the GTK theme.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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
/// On macOS, these are queried from `NSFont`.
/// On Windows, these come from `SystemParametersInfo`.
/// On Linux, these come from GTK/gsettings.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
    /// On Linux: Ubuntu Mono or `DejaVu` Sans Mono
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Title text font (from `SystemFonts::title_font`)
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
    #[must_use] pub fn windows() -> Self {
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
    #[must_use] pub fn macos() -> Self {
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
    #[must_use] pub fn linux_gnome() -> Self {
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
    #[must_use] pub fn ios() -> Self {
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
    #[must_use] pub fn android() -> Self {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct LinuxCustomization {
    /// GTK theme name (e.g. "Adwaita", "Breeze", "Numix").
    pub gtk_theme: OptionString,
    /// Icon theme name (e.g. "Papirus", "Numix", "Breeze").
    pub icon_theme: OptionString,
    /// Cursor theme name (e.g. "`Breeze_Snow`", "DMZ-Black").
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct VisualHints {
    /// Toolbar display style.
    /// Linux: `org.gnome.desktop.interface toolbar-style`, KDE `ToolButtonStyle`.
    pub toolbar_style: ToolbarStyle,
    /// Show icons on push buttons?  (Common in KDE, rare in Win/Mac.)
    /// Linux: `org.gnome.desktop.interface buttons-have-icons`, KDE `ShowIconsOnPushButtons`.
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
    
    /// `DejaVu` fonts (widely available)
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
    #[must_use] pub fn get_fallback_chain(&self, platform: &Platform) -> Vec<&'static str> {
        match platform {
            Platform::MacOs | Platform::Ios => self.macos_fallback_chain(),
            Platform::Windows => self.windows_fallback_chain(),
            Platform::Linux(_) => self.linux_fallback_chain(),
            Platform::Android => self.android_fallback_chain(),
            Platform::Unknown => self.generic_fallback_chain(),
        }
    }
    
    fn macos_fallback_chain(self) -> Vec<&'static str> {
        match self {
            // Normal weight: System Font first, then Helvetica Neue.
            Self::Ui => vec![
                apple_fonts::SYSTEM_FONT,
                apple_fonts::HELVETICA_NEUE,
                apple_fonts::LUCIDA_GRANDE,
            ],
            // Bold weights: Helvetica Neue first (System Font has no Bold variant in fontconfig).
            Self::UiBold | Self::TitleBold => vec![
                apple_fonts::HELVETICA_NEUE,
                apple_fonts::LUCIDA_GRANDE,
            ],
            // Monospace: Menlo (has a Bold variant), then Monaco.
            Self::Monospace | Self::MonospaceBold | Self::MonospaceItalic => vec![
                apple_fonts::MENLO,
                apple_fonts::MONACO,
            ],
            // Title / Menu / Small: System Font then Helvetica Neue.
            Self::Title | Self::Menu | Self::Small => vec![
                apple_fonts::SYSTEM_FONT,
                apple_fonts::HELVETICA_NEUE,
            ],
            // Serif fonts - Georgia has bold variant
            Self::Serif => vec![
                apple_fonts::NEW_YORK,
                "Georgia",
                "Times New Roman",
            ],
            Self::SerifBold => vec![
                "Georgia", // Georgia Bold exists
                "Times New Roman",
            ],
        }
    }
    
    fn windows_fallback_chain(self) -> Vec<&'static str> {
        match self {
            Self::Ui | Self::UiBold => vec![
                windows_fonts::SEGOE_UI_VARIABLE_TEXT,
                windows_fonts::SEGOE_UI,
                windows_fonts::TAHOMA,
            ],
            Self::Monospace | Self::MonospaceBold | Self::MonospaceItalic => vec![
                windows_fonts::CASCADIA_MONO,
                windows_fonts::CASCADIA_CODE,
                windows_fonts::CONSOLAS,
                windows_fonts::LUCIDA_CONSOLE,
                windows_fonts::COURIER_NEW,
            ],
            Self::Title | Self::TitleBold => vec![
                windows_fonts::SEGOE_UI_VARIABLE_DISPLAY,
                windows_fonts::SEGOE_UI,
            ],
            Self::Menu => vec![
                windows_fonts::SEGOE_UI,
                windows_fonts::TAHOMA,
            ],
            Self::Small => vec![
                windows_fonts::SEGOE_UI,
            ],
            Self::Serif | Self::SerifBold => vec![
                "Cambria",
                "Georgia",
                "Times New Roman",
            ],
        }
    }
    
    fn linux_fallback_chain(self) -> Vec<&'static str> {
        match self {
            Self::Ui | Self::UiBold => vec![
                linux_fonts::CANTARELL,
                linux_fonts::UBUNTU,
                linux_fonts::NOTO_SANS,
                linux_fonts::DEJAVU_SANS,
                linux_fonts::LIBERATION_SANS,
                linux_fonts::SANS_SERIF,
            ],
            Self::Monospace | Self::MonospaceBold | Self::MonospaceItalic => vec![
                linux_fonts::UBUNTU_MONO,
                linux_fonts::HACK,
                linux_fonts::NOTO_MONO,
                linux_fonts::DEJAVU_SANS_MONO,
                linux_fonts::LIBERATION_MONO,
                linux_fonts::MONOSPACE,
            ],
            Self::Title | Self::TitleBold | Self::Menu | Self::Small => vec![
                linux_fonts::CANTARELL,
                linux_fonts::UBUNTU,
                linux_fonts::NOTO_SANS,
            ],
            Self::Serif | Self::SerifBold => vec![
                linux_fonts::NOTO_SERIF,
                linux_fonts::DEJAVU_SERIF,
                linux_fonts::LIBERATION_SERIF,
                linux_fonts::SERIF,
            ],
        }
    }
    
    fn android_fallback_chain(self) -> Vec<&'static str> {
        match self {
            Self::Ui | Self::UiBold | Self::Title | Self::TitleBold => vec!["Roboto", "Noto Sans"],
            Self::Monospace | Self::MonospaceBold | Self::MonospaceItalic => {
                vec!["Roboto Mono", "Droid Sans Mono", "monospace"]
            }
            Self::Menu | Self::Small => vec!["Roboto"],
            Self::Serif | Self::SerifBold => vec!["Noto Serif", "Droid Serif", "serif"],
        }
    }
    
    fn generic_fallback_chain(self) -> Vec<&'static str> {
        match self {
            Self::Ui | Self::UiBold | Self::Title | Self::TitleBold | Self::Menu | Self::Small => {
                vec!["sans-serif"]
            }
            Self::Monospace | Self::MonospaceBold | Self::MonospaceItalic => {
                vec!["monospace"]
            }
            Self::Serif | Self::SerifBold => vec!["serif"],
        }
    }
}

impl SystemStyle {

    /// Format the `SystemStyle` as a human-readable JSON string for debugging.
    ///
    /// This does NOT use serde — it manually formats the most important fields
    /// so that they can be verified against OS-reported values in a test script.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    #[must_use] pub fn to_json_string(&self) -> AzString {
        use alloc::format;

        fn opt_color(c: OptionColorU) -> alloc::string::String {
            c.as_ref().map_or_else(
                || "null".into(),
                |c| format!("\"#{:02x}{:02x}{:02x}{:02x}\"", c.r, c.g, c.b, c.a),
            )
        }
        fn opt_str(s: &OptionString) -> alloc::string::String {
            s.as_ref()
                .map_or_else(|| "null".into(), |s| format!("\"{}\"", s.as_str()))
        }
        fn opt_f32(v: OptionF32) -> alloc::string::String {
            v.into_option()
                .map_or_else(|| "null".into(), |v| format!("{v:.2}"))
        }
        fn opt_u16(v: OptionU16) -> alloc::string::String {
            v.into_option()
                .map_or_else(|| "null".into(), |v| format!("{v}"))
        }
        fn opt_px(v: &OptionPixelValue) -> alloc::string::String {
            v.as_ref().map_or_else(
                || "null".into(),
                |v| format!("{:.1}", v.to_pixels_internal(0.0, 0.0, 0.0)),
            )
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
            opt_color(self.colors.text),
            opt_color(self.colors.secondary_text),
            opt_color(self.colors.tertiary_text),
            opt_color(self.colors.background),
            opt_color(self.colors.accent),
            opt_color(self.colors.accent_text),
            opt_color(self.colors.button_face),
            opt_color(self.colors.button_text),
            opt_color(self.colors.disabled_text),
            opt_color(self.colors.window_background),
            opt_color(self.colors.under_page_background),
            opt_color(self.colors.selection_background),
            opt_color(self.colors.selection_text),
            opt_color(self.colors.selection_background_inactive),
            opt_color(self.colors.selection_text_inactive),
            opt_color(self.colors.link),
            opt_color(self.colors.separator),
            opt_color(self.colors.grid),
            opt_color(self.colors.find_highlight),
            opt_color(self.colors.sidebar_background),
            opt_color(self.colors.sidebar_selection),
            // fonts
            opt_str(&self.fonts.ui_font),
            opt_f32(self.fonts.ui_font_size),
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
            opt_f32(tm.title_font_size),
            opt_u16(tm.title_font_weight),
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

    /// Returns a platform-appropriate default system style.
    ///
    /// This returns hard-coded defaults based on the target OS. For actual
    /// runtime detection of the user's theme, colors, and fonts, use the
    /// platform discovery in `azul-dll` (called automatically by `App::create()`).
    #[must_use] pub fn detect() -> Self {
        Self::default_for_platform()
    }

    /// Returns hard-coded defaults for the current compile-time platform.
    #[must_use] pub fn default_for_platform() -> Self {
        #[cfg(target_os = "windows")]
        { defaults::windows_11_light() }
        #[cfg(target_os = "macos")]
        { defaults::macos_modern_light() }
        #[cfg(target_os = "linux")]
        { defaults::gnome_adwaita_light() }
        #[cfg(target_os = "android")]
        { defaults::android_material_light() }
        #[cfg(target_os = "ios")]
        { defaults::ios_light() }
        #[cfg(not(any(
            target_os = "linux",
            target_os = "windows",
            target_os = "macos",
            target_os = "android",
            target_os = "ios"
        )))]
        { Self::default() }
    }

    /// Alias for `detect` - kept for internal compatibility, not exposed in FFI.
    #[inline]
    #[must_use] pub fn new() -> Self {
        Self::detect()
    }

    /// Create a CSS stylesheet for CSD (Client-Side Decorations) titlebar
    ///
    /// This generates CSS rules for the CSD titlebar using system colors,
    /// fonts, and metrics to match the native platform look. Returned rules
    /// carry `rule_priority::SYSTEM`.
    #[must_use] pub fn create_csd_stylesheet(&self) -> Css {
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
                format!("{}px", px.to_pixels_internal(1.0, DEFAULT_FONT_SIZE, DEFAULT_FONT_SIZE))
            })
            .unwrap_or_else(|| "4px".to_string());

        // Titlebar container
        let _ = write!(css,
            ".csd-titlebar {{ width: 100%; height: 32px; background: rgb({}, {}, {}); \
             border-bottom: 1px solid rgb({}, {}, {}); display: flex; flex-direction: row; \
             align-items: center; justify-content: space-between; padding: 0 8px; \
             cursor: grab; user-select: none; }} ",
            bg_color.r, bg_color.g, bg_color.b, border_color.r, border_color.g, border_color.b,
        );

        // Title text
        let _ = write!(css,
            ".csd-title {{ color: rgb({}, {}, {}); font-size: 13px; flex-grow: 1; text-align: \
             center; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; \
             user-select: none; }} ",
            text_color.r, text_color.g, text_color.b,
        );

        // Button container
        css.push_str(".csd-buttons { display: flex; flex-direction: row; gap: 4px; } ");

        // Buttons
        let _ = write!(css,
            ".csd-button {{ width: 32px; height: 24px; border-radius: {}; background: \
             transparent; color: rgb({}, {}, {}); font-size: 16px; line-height: 24px; text-align: \
             center; cursor: pointer; user-select: none; }} ",
            corner_radius, text_color.r, text_color.g, text_color.b,
        );

        // Button hover state
        let hover_color = match self.theme {
            Theme::Dark => ColorU::new_rgb(60, 60, 60),
            Theme::Light => ColorU::new_rgb(220, 220, 220),
        };
        let _ = write!(css,
            ".csd-button:hover {{ background: rgb({}, {}, {}); }} ",
            hover_color.r, hover_color.g, hover_color.b,
        );

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

        // Parse CSS string into a Css.
        let (mut parsed_css, _warnings) = new_from_str(&css);
        // Tag every rule as system-level so author CSS overrides win.
        for rule in parsed_css.rules.as_mut() {
            rule.priority = crate::css::rule_priority::SYSTEM;
        }
        parsed_css
    }
}

/// Detect the Linux desktop environment from environment variables.
///
/// Checks `XDG_CURRENT_DESKTOP`, `DESKTOP_SESSION`, and specific env markers
/// to identify GNOME, KDE, XFCE, Cinnamon, MATE, Hyprland, Sway, i3, etc.
#[must_use] pub fn detect_linux_desktop_env() -> DesktopEnvironment {
    // Check XDG_CURRENT_DESKTOP first (most reliable)
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
        let desktop_lower = desktop.to_lowercase();
        if desktop_lower.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if desktop_lower.contains("kde") || desktop_lower.contains("plasma") {
            return DesktopEnvironment::Kde;
        }
        if desktop_lower.contains("xfce") {
            return DesktopEnvironment::Other(AzString::from_const_str("XFCE"));
        }
        if desktop_lower.contains("unity") {
            return DesktopEnvironment::Other(AzString::from_const_str("Unity"));
        }
        if desktop_lower.contains("cinnamon") {
            return DesktopEnvironment::Other(AzString::from_const_str("Cinnamon"));
        }
        if desktop_lower.contains("mate") {
            return DesktopEnvironment::Other(AzString::from_const_str("MATE"));
        }
        if desktop_lower.contains("lxde") || desktop_lower.contains("lxqt") {
            return DesktopEnvironment::Other(AzString::from(desktop.to_uppercase()));
        }
        if desktop_lower.contains("budgie") {
            return DesktopEnvironment::Other(AzString::from_const_str("Budgie"));
        }
        if desktop_lower.contains("pantheon") {
            return DesktopEnvironment::Other(AzString::from_const_str("Pantheon"));
        }
        if desktop_lower.contains("deepin") {
            return DesktopEnvironment::Other(AzString::from_const_str("Deepin"));
        }
        if desktop_lower.contains("hyprland") {
            return DesktopEnvironment::Other(AzString::from_const_str("Hyprland"));
        }
        if desktop_lower.contains("sway") {
            return DesktopEnvironment::Other(AzString::from_const_str("Sway"));
        }
        if desktop_lower.contains("i3") {
            return DesktopEnvironment::Other(AzString::from_const_str("i3"));
        }
        return DesktopEnvironment::Other(AzString::from(desktop));
    }

    // Check DESKTOP_SESSION as fallback
    if let Ok(session) = std::env::var("DESKTOP_SESSION") {
        let session_lower = session.to_lowercase();
        if session_lower.contains("gnome") {
            return DesktopEnvironment::Gnome;
        }
        if session_lower.contains("plasma") || session_lower.contains("kde") {
            return DesktopEnvironment::Kde;
        }
        if session_lower.contains("xfce") {
            return DesktopEnvironment::Other(AzString::from_const_str("XFCE"));
        }
        if session_lower.contains("cinnamon") {
            return DesktopEnvironment::Other(AzString::from_const_str("Cinnamon"));
        }
        return DesktopEnvironment::Other(AzString::from(session));
    }

    // Check for specific environment markers
    if std::env::var("GNOME_DESKTOP_SESSION_ID").is_ok() {
        return DesktopEnvironment::Gnome;
    }
    if std::env::var("KDE_FULL_SESSION").is_ok() {
        return DesktopEnvironment::Kde;
    }
    if std::env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return DesktopEnvironment::Other(AzString::from_const_str("Hyprland"));
    }
    if std::env::var("SWAYSOCK").is_ok() {
        return DesktopEnvironment::Other(AzString::from_const_str("Sway"));
    }
    if std::env::var("I3SOCK").is_ok() {
        return DesktopEnvironment::Other(AzString::from_const_str("i3"));
    }

    DesktopEnvironment::Other(AzString::from_const_str("Unknown"))
}

/// Detect the system language as a BCP 47 tag.
///
/// Checks `LANGUAGE`, `LC_ALL`, `LC_MESSAGES`, and `LANG` in priority order.
/// Returns `"en-US"` if detection fails. For runtime detection via native
/// OS APIs, the platform discovery in `azul-dll` overrides this.
#[must_use] pub fn detect_system_language() -> AzString {
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
                .split(':')  // LANGUAGE can be "de:en_US:en"
                .next()
                .unwrap_or(value);
            if !lang.is_empty() {
                return AzString::from(lang.replace('_', "-"));
            }
        }
    }
    AzString::from_const_str("en-US")
}

pub mod defaults {
    //! A collection of hard-coded system style defaults that mimic the appearance
    //! of various operating systems and desktop environments.
    //!
    //! These are used as a
    //! fallback when the "io" feature is disabled, ensuring deterministic styles
    //! for testing and environments where system calls are not desired.

    use super::{
        AccessibilitySettings, AnimationMetrics, AudioMetrics, FocusVisuals, InputMetrics,
        LinuxCustomization, ScrollbarPreferences, TextRenderingHints, VisualHints,
    };
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
        width: LayoutWidth::Px(PixelValue::const_px(17)),
        padding_left: LayoutPaddingLeft {
            inner: PixelValue::const_px(0),
        },
        padding_right: LayoutPaddingRight {
            inner: PixelValue::const_px(0),
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
        width: LayoutWidth::Px(PixelValue::const_px(15)),
        padding_left: LayoutPaddingLeft {
            inner: PixelValue::const_px(0),
        },
        padding_right: LayoutPaddingRight {
            inner: PixelValue::const_px(0),
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
        width: LayoutWidth::Px(PixelValue::const_px(14)),
        padding_left: LayoutPaddingLeft {
            inner: PixelValue::const_px(2),
        },
        padding_right: LayoutPaddingRight {
            inner: PixelValue::const_px(2),
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

    /// Windows 11 light mode defaults (Segoe UI Variable, `WinUI` 3 colors).
    #[must_use] pub fn windows_11_light() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_11,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// Windows 11 dark mode defaults (Segoe UI Variable, `WinUI` 3 dark colors).
    #[must_use] pub fn windows_11_dark() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_11,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// Windows 7 Aero theme defaults (Segoe UI, classic Aero colors).
    #[must_use] pub fn windows_7_aero() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_7,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// Windows XP Luna theme defaults (Tahoma, classic Luna blue).
    #[must_use] pub fn windows_xp_luna() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::WIN_XP,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::windows(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    // --- macOS Styles ---

    /// Modern macOS light mode defaults (SF Pro, rounded corners).
    #[must_use] pub fn macos_modern_light() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::MACOS_SONOMA,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::macos(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// Modern macOS dark mode defaults (SF Pro, dark background).
    #[must_use] pub fn macos_modern_dark() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::MACOS_SONOMA,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::macos(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// Classic macOS Aqua theme defaults (Lucida Grande, gel scrollbars).
    #[must_use] pub fn macos_aqua() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::MACOS_TIGER,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::macos(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    // --- Linux Styles ---

    /// GNOME Adwaita light theme defaults (Cantarell font).
    #[must_use] pub fn gnome_adwaita_light() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_6_0,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::default(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// GNOME Adwaita dark theme defaults (Cantarell font, dark background).
    #[must_use] pub fn gnome_adwaita_dark() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_6_0,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::default(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// GTK2 Clearlooks theme defaults (`DejaVu` Sans, orange accent).
    #[must_use] pub fn gtk2_clearlooks() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_2_6,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::default(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// KDE Breeze light theme defaults (Noto Sans, Oxygen scrollbars).
    #[must_use] pub fn kde_breeze_light() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::LINUX_6_0,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::default(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    // --- Mobile Styles ---

    /// Android Material Design light theme defaults (Roboto font).
    #[must_use] pub fn android_material_light() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::ANDROID_14,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::android(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// Android Holo dark theme defaults (Roboto font, dark background).
    #[must_use] pub fn android_holo_dark() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::ANDROID_ICE_CREAM_SANDWICH,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::android(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }

    /// iOS light theme defaults (SF UI font, rounded corners).
    #[must_use] pub fn ios_light() -> SystemStyle {
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
            run_destructor: true,
            icon_style: IconStyleOptions::default(),
            language: AzString::from_const_str("en-US"),
            os_version: OsVersion::IOS_17,
            prefers_reduced_motion: BoolCondition::False,
            prefers_high_contrast: BoolCondition::False,
            scroll_physics: ScrollPhysics::ios(),
            linux: LinuxCustomization::default(),
            focus_visuals: FocusVisuals::default(),
            accessibility: AccessibilitySettings::default(),
            input: InputMetrics::default(),
            text_rendering: TextRenderingHints::default(),
            scrollbar_preferences: ScrollbarPreferences::default(),
            visual_hints: VisualHints::default(),
            animation: AnimationMetrics::default(),
            audio: AudioMetrics::default(),
        }
    }
}

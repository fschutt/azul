//! CSS color types and parser.
//!
//! Core types: [`ColorU`] (u8 RGBA), [`ColorF`] (f32 RGBA), [`ColorOrSystem`]
//! (concrete color or runtime system-theme reference). The parser supports hex,
//! `rgb()`/`rgba()`, `hsl()`/`hsla()`, CSS named colors, and `system:*` syntax.

use alloc::string::{String, ToString};
use core::fmt;
use crate::corety::AzString;
use crate::props::basic::error::{ParseFloatError, ParseIntError};

use crate::{
    impl_option,
    props::basic::{
        direction::{
            parse_direction, CssDirectionParseError, CssDirectionParseErrorOwned, Direction,
        },
        length::{PercentageParseError, PercentageValue},
    },
};

/// Round-saturating `f32` → `u8` for colour channels. Rust's `as u8` already
/// saturates a float (NaN→0, negatives→0, >255→255, otherwise truncates toward
/// zero), so this is behaviour-preserving; it just names the intent and isolates
/// the one unavoidable float→int cast (there is no infallible `f32`→`u8` in std).
#[inline]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn channel_to_u8(v: f32) -> u8 {
    v as u8
}

/// u8-based color, range 0 to 255 (similar to webrenders `ColorU`)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub struct ColorU {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl_option!(
    ColorU,
    OptionColorU,
    [Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash]
);

impl Default for ColorU {
    fn default() -> Self {
        Self::BLACK
    }
}

impl fmt::Display for ColorU {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            f32::from(self.a) / 255.0
        )
    }
}

impl ColorU {
    pub const ALPHA_TRANSPARENT: u8 = 0;
    pub const ALPHA_OPAQUE: u8 = 255;
    pub const RED: Self = Self {
        r: 255,
        g: 0,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const GREEN: Self = Self {
        r: 0,
        g: 255,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const BLUE: Self = Self {
        r: 0,
        g: 0,
        b: 255,
        a: Self::ALPHA_OPAQUE,
    };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
        a: Self::ALPHA_OPAQUE,
    };
    pub const BLACK: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: Self::ALPHA_TRANSPARENT,
    };

    // Additional common colors
    pub const YELLOW: Self = Self { r: 255, g: 255, b: 0, a: Self::ALPHA_OPAQUE };
    pub const CYAN: Self = Self { r: 0, g: 255, b: 255, a: Self::ALPHA_OPAQUE };
    pub const MAGENTA: Self = Self { r: 255, g: 0, b: 255, a: Self::ALPHA_OPAQUE };
    pub const ORANGE: Self = Self { r: 255, g: 165, b: 0, a: Self::ALPHA_OPAQUE };
    pub const PINK: Self = Self { r: 255, g: 192, b: 203, a: Self::ALPHA_OPAQUE };
    pub const PURPLE: Self = Self { r: 128, g: 0, b: 128, a: Self::ALPHA_OPAQUE };
    pub const BROWN: Self = Self { r: 139, g: 69, b: 19, a: Self::ALPHA_OPAQUE };
    pub const GRAY: Self = Self { r: 128, g: 128, b: 128, a: Self::ALPHA_OPAQUE };
    pub const LIGHT_GRAY: Self = Self { r: 211, g: 211, b: 211, a: Self::ALPHA_OPAQUE };
    pub const DARK_GRAY: Self = Self { r: 64, g: 64, b: 64, a: Self::ALPHA_OPAQUE };
    pub const NAVY: Self = Self { r: 0, g: 0, b: 128, a: Self::ALPHA_OPAQUE };
    pub const TEAL: Self = Self { r: 0, g: 128, b: 128, a: Self::ALPHA_OPAQUE };
    pub const OLIVE: Self = Self { r: 128, g: 128, b: 0, a: Self::ALPHA_OPAQUE };
    pub const MAROON: Self = Self { r: 128, g: 0, b: 0, a: Self::ALPHA_OPAQUE };
    pub const LIME: Self = Self { r: 0, g: 255, b: 0, a: Self::ALPHA_OPAQUE };
    pub const AQUA: Self = Self { r: 0, g: 255, b: 255, a: Self::ALPHA_OPAQUE };
    pub const SILVER: Self = Self { r: 192, g: 192, b: 192, a: Self::ALPHA_OPAQUE };
    pub const FUCHSIA: Self = Self { r: 255, g: 0, b: 255, a: Self::ALPHA_OPAQUE };
    pub const INDIGO: Self = Self { r: 75, g: 0, b: 130, a: Self::ALPHA_OPAQUE };
    pub const GOLD: Self = Self { r: 255, g: 215, b: 0, a: Self::ALPHA_OPAQUE };
    pub const CORAL: Self = Self { r: 255, g: 127, b: 80, a: Self::ALPHA_OPAQUE };
    pub const SALMON: Self = Self { r: 250, g: 128, b: 114, a: Self::ALPHA_OPAQUE };
    pub const TURQUOISE: Self = Self { r: 64, g: 224, b: 208, a: Self::ALPHA_OPAQUE };
    pub const VIOLET: Self = Self { r: 238, g: 130, b: 238, a: Self::ALPHA_OPAQUE };
    pub const CRIMSON: Self = Self { r: 220, g: 20, b: 60, a: Self::ALPHA_OPAQUE };
    pub const CHOCOLATE: Self = Self { r: 210, g: 105, b: 30, a: Self::ALPHA_OPAQUE };
    pub const SKY_BLUE: Self = Self { r: 135, g: 206, b: 235, a: Self::ALPHA_OPAQUE };
    pub const FOREST_GREEN: Self = Self { r: 34, g: 139, b: 34, a: Self::ALPHA_OPAQUE };
    pub const SEA_GREEN: Self = Self { r: 46, g: 139, b: 87, a: Self::ALPHA_OPAQUE };
    pub const SLATE_GRAY: Self = Self { r: 112, g: 128, b: 144, a: Self::ALPHA_OPAQUE };
    pub const MIDNIGHT_BLUE: Self = Self { r: 25, g: 25, b: 112, a: Self::ALPHA_OPAQUE };
    pub const DARK_RED: Self = Self { r: 139, g: 0, b: 0, a: Self::ALPHA_OPAQUE };
    pub const DARK_GREEN: Self = Self { r: 0, g: 100, b: 0, a: Self::ALPHA_OPAQUE };
    pub const DARK_BLUE: Self = Self { r: 0, g: 0, b: 139, a: Self::ALPHA_OPAQUE };
    pub const LIGHT_BLUE: Self = Self { r: 173, g: 216, b: 230, a: Self::ALPHA_OPAQUE };
    pub const LIGHT_GREEN: Self = Self { r: 144, g: 238, b: 144, a: Self::ALPHA_OPAQUE };
    pub const LIGHT_YELLOW: Self = Self { r: 255, g: 255, b: 224, a: Self::ALPHA_OPAQUE };
    pub const LIGHT_PINK: Self = Self { r: 255, g: 182, b: 193, a: Self::ALPHA_OPAQUE };

    // Constructor functions for C API (become AzColorU_red(), AzColorU_cyan(), etc.)
    #[must_use] pub const fn red() -> Self { Self::RED }
    #[must_use] pub const fn green() -> Self { Self::GREEN }
    #[must_use] pub const fn blue() -> Self { Self::BLUE }
    #[must_use] pub const fn white() -> Self { Self::WHITE }
    #[must_use] pub const fn black() -> Self { Self::BLACK }
    #[must_use] pub const fn transparent() -> Self { Self::TRANSPARENT }
    #[must_use] pub const fn yellow() -> Self { Self::YELLOW }
    #[must_use] pub const fn cyan() -> Self { Self::CYAN }
    #[must_use] pub const fn magenta() -> Self { Self::MAGENTA }
    #[must_use] pub const fn orange() -> Self { Self::ORANGE }
    #[must_use] pub const fn pink() -> Self { Self::PINK }
    #[must_use] pub const fn purple() -> Self { Self::PURPLE }
    #[must_use] pub const fn brown() -> Self { Self::BROWN }
    #[must_use] pub const fn gray() -> Self { Self::GRAY }
    #[must_use] pub const fn light_gray() -> Self { Self::LIGHT_GRAY }
    #[must_use] pub const fn dark_gray() -> Self { Self::DARK_GRAY }
    #[must_use] pub const fn navy() -> Self { Self::NAVY }
    #[must_use] pub const fn teal() -> Self { Self::TEAL }
    #[must_use] pub const fn olive() -> Self { Self::OLIVE }
    #[must_use] pub const fn maroon() -> Self { Self::MAROON }
    #[must_use] pub const fn lime() -> Self { Self::LIME }
    #[must_use] pub const fn aqua() -> Self { Self::AQUA }
    #[must_use] pub const fn silver() -> Self { Self::SILVER }
    #[must_use] pub const fn fuchsia() -> Self { Self::FUCHSIA }
    #[must_use] pub const fn indigo() -> Self { Self::INDIGO }
    #[must_use] pub const fn gold() -> Self { Self::GOLD }
    #[must_use] pub const fn coral() -> Self { Self::CORAL }
    #[must_use] pub const fn salmon() -> Self { Self::SALMON }
    #[must_use] pub const fn turquoise() -> Self { Self::TURQUOISE }
    #[must_use] pub const fn violet() -> Self { Self::VIOLET }
    #[must_use] pub const fn crimson() -> Self { Self::CRIMSON }
    #[must_use] pub const fn chocolate() -> Self { Self::CHOCOLATE }
    #[must_use] pub const fn sky_blue() -> Self { Self::SKY_BLUE }
    #[must_use] pub const fn forest_green() -> Self { Self::FOREST_GREEN }
    #[must_use] pub const fn sea_green() -> Self { Self::SEA_GREEN }
    #[must_use] pub const fn slate_gray() -> Self { Self::SLATE_GRAY }
    #[must_use] pub const fn midnight_blue() -> Self { Self::MIDNIGHT_BLUE }
    #[must_use] pub const fn dark_red() -> Self { Self::DARK_RED }
    #[must_use] pub const fn dark_green() -> Self { Self::DARK_GREEN }
    #[must_use] pub const fn dark_blue() -> Self { Self::DARK_BLUE }
    #[must_use] pub const fn light_blue() -> Self { Self::LIGHT_BLUE }
    #[must_use] pub const fn light_green() -> Self { Self::LIGHT_GREEN }
    #[must_use] pub const fn light_yellow() -> Self { Self::LIGHT_YELLOW }
    #[must_use] pub const fn light_pink() -> Self { Self::LIGHT_PINK }

    /// Creates a new color with RGBA values.
    #[must_use] pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    /// Creates a new color with RGB values (alpha = 255).
    #[must_use] pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    /// Alias for `rgba` - kept for internal compatibility, not exposed in FFI.
    #[inline]
    #[must_use] pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(r, g, b, a)
    }
    /// Alias for `rgb` - kept for internal compatibility, not exposed in FFI.
    #[inline]
    #[must_use] pub const fn new_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgb(r, g, b)
    }

    /// Linearly interpolate all four RGBA channels between `self` and `other`.
    /// `t = 0.0` returns `self`, `t = 1.0` returns `other`.
    #[must_use] pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            r: channel_to_u8(libm::roundf(f32::from(self.r) + (f32::from(other.r) - f32::from(self.r)) * t)),
            g: channel_to_u8(libm::roundf(f32::from(self.g) + (f32::from(other.g) - f32::from(self.g)) * t)),
            b: channel_to_u8(libm::roundf(f32::from(self.b) + (f32::from(other.b) - f32::from(self.b)) * t)),
            a: channel_to_u8(libm::roundf(f32::from(self.a) + (f32::from(other.a) - f32::from(self.a)) * t)),
        }
    }
    
    /// Lighten a color by a percentage (0.0 to 1.0).
    /// Returns a new color blended towards white, preserving the original alpha.
    #[must_use] pub fn lighten(&self, amount: f32) -> Self {
        let mut c = self.interpolate(&Self::WHITE, amount.clamp(0.0, 1.0));
        c.a = self.a;
        c
    }

    /// Darken a color by a percentage (0.0 to 1.0).
    /// Returns a new color blended towards black, preserving the original alpha.
    #[must_use] pub fn darken(&self, amount: f32) -> Self {
        let mut c = self.interpolate(&Self::BLACK, amount.clamp(0.0, 1.0));
        c.a = self.a;
        c
    }
    
    /// Mix two colors together with a given ratio (0.0 = self, 1.0 = other).
    #[must_use] pub fn mix(&self, other: &Self, ratio: f32) -> Self {
        self.interpolate(other, ratio.clamp(0.0, 1.0))
    }
    
    /// Create a hover variant (slightly lighter for dark colors, darker for light colors).
    /// This is useful for button hover states.
    #[must_use] pub fn hover_variant(&self) -> Self {
        let luminance = self.relative_luminance();
        if luminance > 0.5 {
            self.darken(0.08)
        } else {
            self.lighten(0.12)
        }
    }

    /// Create an active/pressed variant (darker than hover).
    /// This is useful for button active states.
    #[must_use] pub fn active_variant(&self) -> Self {
        let luminance = self.relative_luminance();
        if luminance > 0.5 {
            self.darken(0.15)
        } else {
            self.lighten(0.05)
        }
    }
    
    /// Calculate approximate luminance (0.0 = black, 1.0 = white).
    ///
    /// **Note:** This applies BT.709 coefficients directly to gamma-encoded sRGB
    /// values without linearizing first, so it is only an approximation.
    /// For accurate results (e.g. WCAG contrast checks), use [`relative_luminance()`].
    #[must_use] pub fn luminance(&self) -> f32 {
        let r = f32::from(self.r) / 255.0;
        let g = f32::from(self.g) / 255.0;
        let b = f32::from(self.b) / 255.0;
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Returns white or black text color for best contrast on this background.
    #[must_use] pub fn contrast_text(&self) -> Self {
        self.best_contrast_text()
    }
    
    // ============================================================
    // WCAG Accessibility and Contrast Helpers
    // Based on W3C WCAG 2.1 guidelines and Chromium research
    // ============================================================
    
    /// Converts a single sRGB channel to linear RGB.
    /// Used for accurate luminance and contrast calculations.
    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.03928 {
            c / 12.92
        } else {
            libm::powf((c + 0.055) / 1.055, 2.4)
        }
    }
    
    /// Calculate relative luminance per WCAG 2.1 specification.
    /// Returns a value between 0.0 (darkest) and 1.0 (lightest).
    /// Uses the sRGB to linear conversion for accurate results.
    #[must_use] pub fn relative_luminance(&self) -> f32 {
        let r = Self::srgb_to_linear(f32::from(self.r) / 255.0);
        let g = Self::srgb_to_linear(f32::from(self.g) / 255.0);
        let b = Self::srgb_to_linear(f32::from(self.b) / 255.0);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
    
    /// Calculate the contrast ratio between this color and another.
    /// Returns a value between 1.0 (no contrast) and 21.0 (max contrast).
    /// 
    /// WCAG 2.1 requirements:
    /// - AA normal text: >= 4.5:1
    /// - AA large text: >= 3.0:1
    /// - AAA normal text: >= 7.0:1
    /// - AAA large text: >= 4.5:1
    #[must_use] pub fn contrast_ratio(&self, other: &Self) -> f32 {
        let l1 = self.relative_luminance();
        let l2 = other.relative_luminance();
        let lighter = if l1 > l2 { l1 } else { l2 };
        let darker = if l1 > l2 { l2 } else { l1 };
        (lighter + 0.05) / (darker + 0.05)
    }
    
    /// Check if the contrast ratio meets WCAG AA requirements for normal text (>= 4.5:1).
    #[must_use] pub fn meets_wcag_aa(&self, other: &Self) -> bool {
        self.contrast_ratio(other) >= 4.5
    }
    
    /// Check if the contrast ratio meets WCAG AA requirements for large text (>= 3.0:1).
    /// Large text is defined as 18pt+ or 14pt+ bold.
    #[must_use] pub fn meets_wcag_aa_large(&self, other: &Self) -> bool {
        self.contrast_ratio(other) >= 3.0
    }
    
    /// Check if the contrast ratio meets WCAG AAA requirements for normal text (>= 7.0:1).
    #[must_use] pub fn meets_wcag_aaa(&self, other: &Self) -> bool {
        self.contrast_ratio(other) >= 7.0
    }
    
    /// Check if the contrast ratio meets WCAG AAA requirements for large text (>= 4.5:1).
    #[must_use] pub fn meets_wcag_aaa_large(&self, other: &Self) -> bool {
        self.contrast_ratio(other) >= 4.5
    }
    
    /// Returns true if this color is considered "light" (relative luminance > 0.5).
    /// Useful for determining if dark or light text should be used.
    #[must_use] pub fn is_light(&self) -> bool {
        self.relative_luminance() > 0.5
    }

    /// Returns true if this color is considered "dark" (relative luminance <= 0.5).
    #[must_use] pub fn is_dark(&self) -> bool {
        self.relative_luminance() <= 0.5
    }
    
    /// Suggest the best text color (black or white) for this background,
    /// ensuring WCAG AA compliance for normal text.
    /// 
    /// If neither black nor white meets AA requirements (unlikely), 
    /// returns the one with higher contrast.
    #[must_use] pub fn best_contrast_text(&self) -> Self {
        let white_contrast = self.contrast_ratio(&Self::WHITE);
        let black_contrast = self.contrast_ratio(&Self::BLACK);
        
        if white_contrast >= black_contrast {
            Self::WHITE
        } else {
            Self::BLACK
        }
    }
    
    /// Adjust the color to ensure it meets the minimum contrast ratio against a background.
    /// Lightens or darkens the color as needed.
    /// 
    /// Returns the original color if it already meets the requirement,
    /// otherwise returns an adjusted color that meets the minimum contrast.
    #[must_use] pub fn ensure_contrast(&self, background: &Self, min_ratio: f32) -> Self {
        let current_ratio = self.contrast_ratio(background);
        if current_ratio >= min_ratio {
            return *self;
        }
        
        // Determine if we should lighten or darken
        let bg_luminance = background.relative_luminance();
        let should_lighten = bg_luminance < 0.5;
        
        // Binary search for the right amount
        let mut low = 0.0f32;
        let mut high = 1.0f32;
        let mut result = *self;
        
        for _ in 0..16 {
            let mid = f32::midpoint(low, high);
            let candidate = if should_lighten {
                self.lighten(mid)
            } else {
                self.darken(mid)
            };
            
            if candidate.contrast_ratio(background) >= min_ratio {
                result = candidate;
                high = mid;
            } else {
                low = mid;
            }
        }
        
        result
    }
    
    /// Calculate the APCA (Accessible Perceptual Contrast Algorithm) contrast.
    /// This is a newer algorithm that may replace WCAG contrast in future standards.
    /// Returns a value between -108 (white on black) and 106 (black on white).
    ///
    /// **Note:** This is an approximation — it reuses the WCAG piecewise sRGB
    /// linearization and BT.709 luminance coefficients rather than the APCA-specific
    /// TRC exponents and coefficients from the full 0.0.98G specification.
    ///
    /// The sign indicates polarity (negative = light text on dark bg).
    /// For most purposes, use the absolute value.
    #[must_use] pub fn apca_contrast(&self, background: &Self) -> f32 {
        // Convert to Y (luminance) using sRGB TRC
        let text_y = self.relative_luminance();
        let bg_y = background.relative_luminance();
        
        // Soft clamp
        let text_y = if text_y < 0.0 { 0.0 } else { text_y };
        let bg_y = if bg_y < 0.0 { 0.0 } else { bg_y };
        
        // APCA 0.0.98G constants
        const NORMBLKTXT: f32 = 0.56;
        const NORMWHT: f32 = 0.57;
        const REVTXT: f32 = 0.62;
        const REVWHT: f32 = 0.65;
        const BLKTHRS: f32 = 0.022;
        const SCALEBLKT: f32 = 1.414;
        const SCALEWHT: f32 = 1.14;
        
        // Clamp black levels
        let txt_clamp = if text_y < BLKTHRS { 
            text_y + libm::powf(BLKTHRS - text_y, SCALEBLKT)
        } else { 
            text_y 
        };
        let bg_clamp = if bg_y < BLKTHRS { 
            bg_y + libm::powf(BLKTHRS - bg_y, SCALEBLKT)
        } else { 
            bg_y 
        };
        
        // Calculate contrast
        if bg_clamp > txt_clamp {
            // Dark text on light bg
            let s = (libm::powf(bg_clamp, NORMWHT) - libm::powf(txt_clamp, NORMBLKTXT)) * SCALEWHT;
            if s < 0.1 { 0.0 } else { s * 100.0 }
        } else {
            // Light text on dark bg
            let s = (libm::powf(bg_clamp, REVWHT) - libm::powf(txt_clamp, REVTXT)) * SCALEWHT;
            if s > -0.1 { 0.0 } else { s * 100.0 }
        }
    }
    
    /// Check if the APCA contrast meets the recommended minimum for body text (|Lc| >= 60).
    #[must_use] pub fn meets_apca_body(&self, background: &Self) -> bool {
        libm::fabsf(self.apca_contrast(background)) >= 60.0
    }
    
    /// Check if the APCA contrast meets the minimum for large text (|Lc| >= 45).
    #[must_use] pub fn meets_apca_large(&self, background: &Self) -> bool {
        libm::fabsf(self.apca_contrast(background)) >= 45.0
    }
    
    /// Set the alpha channel while keeping RGB values.
    #[must_use] pub const fn with_alpha(&self, a: u8) -> Self {
        Self { r: self.r, g: self.g, b: self.b, a }
    }
    
    /// Set the alpha as a float (0.0 to 1.0).
    #[must_use] pub fn with_alpha_f32(&self, a: f32) -> Self {
        self.with_alpha(channel_to_u8(a.clamp(0.0, 1.0) * 255.0))
    }
    
    /// Invert the color (keeping alpha).
    #[must_use] pub const fn invert(&self) -> Self {
        Self {
            r: 255 - self.r,
            g: 255 - self.g,
            b: 255 - self.b,
            a: self.a,
        }
    }
    
    /// Convert to grayscale using luminance weights.
    #[must_use] pub fn to_grayscale(&self) -> Self {
        let gray = channel_to_u8(0.299 * f32::from(self.r) + 0.587 * f32::from(self.g) + 0.114 * f32::from(self.b));
        Self { r: gray, g: gray, b: gray, a: self.a }
    }

    /// Returns `true` if the alpha channel is not fully opaque (i.e. `a != 255`).
    #[must_use] pub const fn has_alpha(&self) -> bool {
        self.a != Self::ALPHA_OPAQUE
    }

    /// Format the color as an 8-digit lowercase hex string (e.g. `#ff0000ff`).
    #[must_use] pub fn to_hash(&self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    // ============================================================
    // Elementary OS color palette (with shade parameter 100-900)
    // ============================================================

    /// Strawberry color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn strawberry(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xff, 0x8c, 0x82),   // 100: #ff8c82
            201..=400 => Self::rgb(0xed, 0x53, 0x53), // 300: #ed5353
            401..=600 => Self::rgb(0xc6, 0x26, 0x2e), // 500: #c6262e
            601..=800 => Self::rgb(0xa1, 0x07, 0x05), // 700: #a10705
            _ => Self::rgb(0x7a, 0x00, 0x00),         // 900: #7a0000
        }
    }

    /// Orange color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn palette_orange(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xff, 0xc2, 0x7d),   // 100: #ffc27d
            201..=400 => Self::rgb(0xff, 0xa1, 0x54), // 300: #ffa154
            401..=600 => Self::rgb(0xf3, 0x73, 0x29), // 500: #f37329
            601..=800 => Self::rgb(0xcc, 0x3b, 0x02), // 700: #cc3b02
            _ => Self::rgb(0xa6, 0x21, 0x00),         // 900: #a62100
        }
    }

    /// Banana color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn banana(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xff, 0xf3, 0x94),   // 100: #fff394
            201..=400 => Self::rgb(0xff, 0xe1, 0x6b), // 300: #ffe16b
            401..=600 => Self::rgb(0xf9, 0xc4, 0x40), // 500: #f9c440
            601..=800 => Self::rgb(0xd4, 0x8e, 0x15), // 700: #d48e15
            _ => Self::rgb(0xad, 0x5f, 0x00),         // 900: #ad5f00
        }
    }

    /// Lime color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn palette_lime(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xd1, 0xff, 0x82),   // 100: #d1ff82
            201..=400 => Self::rgb(0x9b, 0xdb, 0x4d), // 300: #9bdb4d
            401..=600 => Self::rgb(0x68, 0xb7, 0x23), // 500: #68b723
            601..=800 => Self::rgb(0x3a, 0x91, 0x04), // 700: #3a9104
            _ => Self::rgb(0x20, 0x6b, 0x00),         // 900: #206b00
        }
    }

    /// Mint color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn mint(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0x89, 0xff, 0xdd),   // 100: #89ffdd
            201..=400 => Self::rgb(0x43, 0xd6, 0xb5), // 300: #43d6b5
            401..=600 => Self::rgb(0x28, 0xbc, 0xa3), // 500: #28bca3
            601..=800 => Self::rgb(0x0e, 0x9a, 0x83), // 700: #0e9a83
            _ => Self::rgb(0x00, 0x73, 0x67),         // 900: #007367
        }
    }

    /// Blueberry color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn blueberry(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0x8c, 0xd5, 0xff),   // 100: #8cd5ff
            201..=400 => Self::rgb(0x64, 0xba, 0xff), // 300: #64baff
            401..=600 => Self::rgb(0x36, 0x89, 0xe6), // 500: #3689e6
            601..=800 => Self::rgb(0x0d, 0x52, 0xbf), // 700: #0d52bf
            _ => Self::rgb(0x00, 0x2e, 0x99),         // 900: #002e99
        }
    }

    /// Grape color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn grape(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xe4, 0xc6, 0xfa),   // 100: #e4c6fa
            201..=400 => Self::rgb(0xcd, 0x9e, 0xf7), // 300: #cd9ef7
            401..=600 => Self::rgb(0xa5, 0x6d, 0xe2), // 500: #a56de2
            601..=800 => Self::rgb(0x72, 0x39, 0xb3), // 700: #7239b3
            _ => Self::rgb(0x45, 0x29, 0x81),         // 900: #452981
        }
    }

    /// Bubblegum color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn bubblegum(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xfe, 0x9a, 0xb8),   // 100: #fe9ab8
            201..=400 => Self::rgb(0xf4, 0x67, 0x9d), // 300: #f4679d
            401..=600 => Self::rgb(0xde, 0x3e, 0x80), // 500: #de3e80
            601..=800 => Self::rgb(0xbc, 0x24, 0x5d), // 700: #bc245d
            _ => Self::rgb(0x91, 0x0e, 0x38),         // 900: #910e38
        }
    }

    /// Cocoa color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn cocoa(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xa3, 0x90, 0x7c),   // 100: #a3907c
            201..=400 => Self::rgb(0x8a, 0x71, 0x5e), // 300: #8a715e
            401..=600 => Self::rgb(0x71, 0x53, 0x44), // 500: #715344
            601..=800 => Self::rgb(0x57, 0x39, 0x2d), // 700: #57392d
            _ => Self::rgb(0x3d, 0x21, 0x1b),         // 900: #3d211b
        }
    }

    /// Silver color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn palette_silver(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0xfa, 0xfa, 0xfa),   // 100: #fafafa
            201..=400 => Self::rgb(0xd4, 0xd4, 0xd4), // 300: #d4d4d4
            401..=600 => Self::rgb(0xab, 0xac, 0xae), // 500: #abacae
            601..=800 => Self::rgb(0x7e, 0x80, 0x87), // 700: #7e8087
            _ => Self::rgb(0x55, 0x57, 0x61),         // 900: #555761
        }
    }

    /// Slate color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn slate(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0x95, 0xa3, 0xab),   // 100: #95a3ab
            201..=400 => Self::rgb(0x66, 0x78, 0x85), // 300: #667885
            401..=600 => Self::rgb(0x48, 0x5a, 0x6c), // 500: #485a6c
            601..=800 => Self::rgb(0x27, 0x34, 0x45), // 700: #273445
            _ => Self::rgb(0x0e, 0x14, 0x1f),         // 900: #0e141f
        }
    }

    /// Dark color palette (shade: 100, 300, 500, 700, 900)
    #[must_use] pub const fn dark(shade: usize) -> Self {
        match shade {
            0..=200 => Self::rgb(0x66, 0x66, 0x66),   // 100: #666
            201..=400 => Self::rgb(0x4d, 0x4d, 0x4d), // 300: #4d4d4d
            401..=600 => Self::rgb(0x33, 0x33, 0x33), // 500: #333
            601..=800 => Self::rgb(0x1a, 0x1a, 0x1a), // 700: #1a1a1a
            _ => Self::rgb(0x00, 0x00, 0x00),         // 900: #000
        }
    }

    // ============================================================
    // Apple System Colors (light and dark variants)
    // ============================================================

    /// Apple Red (light mode)
    #[must_use] pub const fn apple_red() -> Self { Self::rgb(255, 59, 48) }
    /// Apple Red (dark mode)
    #[must_use] pub const fn apple_red_dark() -> Self { Self::rgb(255, 69, 58) }
    /// Apple Orange (light mode)
    #[must_use] pub const fn apple_orange() -> Self { Self::rgb(255, 149, 0) }
    /// Apple Orange (dark mode)
    #[must_use] pub const fn apple_orange_dark() -> Self { Self::rgb(255, 159, 10) }
    /// Apple Yellow (light mode)
    #[must_use] pub const fn apple_yellow() -> Self { Self::rgb(255, 204, 0) }
    /// Apple Yellow (dark mode)
    #[must_use] pub const fn apple_yellow_dark() -> Self { Self::rgb(255, 214, 10) }
    /// Apple Green (light mode)
    #[must_use] pub const fn apple_green() -> Self { Self::rgb(40, 205, 65) }
    /// Apple Green (dark mode)
    #[must_use] pub const fn apple_green_dark() -> Self { Self::rgb(40, 215, 75) }
    /// Apple Mint (light mode)
    #[must_use] pub const fn apple_mint() -> Self { Self::rgb(0, 199, 190) }
    /// Apple Mint (dark mode)
    #[must_use] pub const fn apple_mint_dark() -> Self { Self::rgb(102, 212, 207) }
    /// Apple Teal (light mode)
    #[must_use] pub const fn apple_teal() -> Self { Self::rgb(89, 173, 196) }
    /// Apple Teal (dark mode)
    #[must_use] pub const fn apple_teal_dark() -> Self { Self::rgb(106, 196, 220) }
    /// Apple Cyan (light mode)
    #[must_use] pub const fn apple_cyan() -> Self { Self::rgb(85, 190, 240) }
    /// Apple Cyan (dark mode)
    #[must_use] pub const fn apple_cyan_dark() -> Self { Self::rgb(90, 200, 245) }
    /// Apple Blue (light mode)
    #[must_use] pub const fn apple_blue() -> Self { Self::rgb(0, 122, 255) }
    /// Apple Blue (dark mode)
    #[must_use] pub const fn apple_blue_dark() -> Self { Self::rgb(10, 132, 255) }
    /// Apple Indigo (light mode)
    #[must_use] pub const fn apple_indigo() -> Self { Self::rgb(88, 86, 214) }
    /// Apple Indigo (dark mode)
    #[must_use] pub const fn apple_indigo_dark() -> Self { Self::rgb(94, 92, 230) }
    /// Apple Purple (light mode)
    #[must_use] pub const fn apple_purple() -> Self { Self::rgb(175, 82, 222) }
    /// Apple Purple (dark mode)
    #[must_use] pub const fn apple_purple_dark() -> Self { Self::rgb(191, 90, 242) }
    /// Apple Pink (light mode)
    #[must_use] pub const fn apple_pink() -> Self { Self::rgb(255, 45, 85) }
    /// Apple Pink (dark mode)
    #[must_use] pub const fn apple_pink_dark() -> Self { Self::rgb(255, 55, 95) }
    /// Apple Brown (light mode)
    #[must_use] pub const fn apple_brown() -> Self { Self::rgb(162, 132, 94) }
    /// Apple Brown (dark mode)
    #[must_use] pub const fn apple_brown_dark() -> Self { Self::rgb(172, 142, 104) }
    /// Apple Gray (light mode)
    #[must_use] pub const fn apple_gray() -> Self { Self::rgb(142, 142, 147) }
    /// Apple Gray (dark mode)
    #[must_use] pub const fn apple_gray_dark() -> Self { Self::rgb(152, 152, 157) }

    // ============================================================
    // Bootstrap-style semantic button colors
    // These provide consistent button styling across platforms
    // ============================================================

    /// Primary button color (blue) - used for main actions
    #[must_use] pub const fn bootstrap_primary() -> Self { Self::rgb(13, 110, 253) }
    #[must_use] pub const fn bootstrap_primary_hover() -> Self { Self::rgb(11, 94, 215) }
    #[must_use] pub const fn bootstrap_primary_active() -> Self { Self::rgb(10, 88, 202) }
    
    /// Secondary button color (gray) - used for secondary actions
    #[must_use] pub const fn bootstrap_secondary() -> Self { Self::rgb(108, 117, 125) }
    #[must_use] pub const fn bootstrap_secondary_hover() -> Self { Self::rgb(92, 99, 106) }
    #[must_use] pub const fn bootstrap_secondary_active() -> Self { Self::rgb(86, 94, 100) }
    
    /// Success button color (green) - used for confirmations
    #[must_use] pub const fn bootstrap_success() -> Self { Self::rgb(25, 135, 84) }
    #[must_use] pub const fn bootstrap_success_hover() -> Self { Self::rgb(21, 115, 71) }
    #[must_use] pub const fn bootstrap_success_active() -> Self { Self::rgb(20, 108, 67) }
    
    /// Danger button color (red) - used for destructive actions
    #[must_use] pub const fn bootstrap_danger() -> Self { Self::rgb(220, 53, 69) }
    #[must_use] pub const fn bootstrap_danger_hover() -> Self { Self::rgb(187, 45, 59) }
    #[must_use] pub const fn bootstrap_danger_active() -> Self { Self::rgb(176, 42, 55) }
    
    /// Warning button color (yellow) - used for warnings, uses BLACK text
    #[must_use] pub const fn bootstrap_warning() -> Self { Self::rgb(255, 193, 7) }
    #[must_use] pub const fn bootstrap_warning_hover() -> Self { Self::rgb(255, 202, 44) }
    #[must_use] pub const fn bootstrap_warning_active() -> Self { Self::rgb(255, 205, 57) }
    
    /// Info button color (teal/cyan) - used for informational actions
    #[must_use] pub const fn bootstrap_info() -> Self { Self::rgb(13, 202, 240) }
    #[must_use] pub const fn bootstrap_info_hover() -> Self { Self::rgb(49, 210, 242) }
    #[must_use] pub const fn bootstrap_info_active() -> Self { Self::rgb(61, 213, 243) }
    
    /// Light button color - used for light-themed buttons
    #[must_use] pub const fn bootstrap_light() -> Self { Self::rgb(248, 249, 250) }
    #[must_use] pub const fn bootstrap_light_hover() -> Self { Self::rgb(233, 236, 239) }
    #[must_use] pub const fn bootstrap_light_active() -> Self { Self::rgb(218, 222, 226) }
    
    /// Dark button color - used for dark-themed buttons
    #[must_use] pub const fn bootstrap_dark() -> Self { Self::rgb(33, 37, 41) }
    #[must_use] pub const fn bootstrap_dark_hover() -> Self { Self::rgb(66, 70, 73) }
    #[must_use] pub const fn bootstrap_dark_active() -> Self { Self::rgb(78, 81, 84) }
    
    /// Link button text color
    #[must_use] pub const fn bootstrap_link() -> Self { Self::rgb(13, 110, 253) }
    #[must_use] pub const fn bootstrap_link_hover() -> Self { Self::rgb(10, 88, 202) }
}

/// f32-based color, range 0.0 to 1.0 (similar to webrenders `ColorF`)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ColorF {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Default for ColorF {
    fn default() -> Self {
        Self::BLACK
    }
}

impl fmt::Display for ColorF {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "rgba({}, {}, {}, {})",
            self.r * 255.0,
            self.g * 255.0,
            self.b * 255.0,
            self.a
        )
    }
}

impl ColorF {
    pub const ALPHA_TRANSPARENT: f32 = 0.0;
    pub const ALPHA_OPAQUE: f32 = 1.0;
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Self::ALPHA_OPAQUE,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Self::ALPHA_TRANSPARENT,
    };
}

impl From<ColorU> for ColorF {
    fn from(input: ColorU) -> Self {
        Self {
            r: f32::from(input.r) / 255.0,
            g: f32::from(input.g) / 255.0,
            b: f32::from(input.b) / 255.0,
            a: f32::from(input.a) / 255.0,
        }
    }
}

impl From<ColorF> for ColorU {
    fn from(input: ColorF) -> Self {
        Self {
            r: channel_to_u8(input.r.min(1.0) * 255.0),
            g: channel_to_u8(input.g.min(1.0) * 255.0),
            b: channel_to_u8(input.b.min(1.0) * 255.0),
            a: channel_to_u8(input.a.min(1.0) * 255.0),
        }
    }
}

/// A color reference that can be either a concrete color or a system color.
/// System colors are lazily evaluated at runtime based on the user's system theme.
/// 
/// CSS syntax: `system:accent`, `system:text`, `system:background`, etc.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum ColorOrSystem {
    /// A concrete RGBA color value.
    Color(ColorU),
    /// A reference to a system color, resolved at runtime.
    System(SystemColorRef),
}

impl Default for ColorOrSystem {
    fn default() -> Self {
        Self::Color(ColorU::BLACK)
    }
}

impl From<ColorU> for ColorOrSystem {
    fn from(color: ColorU) -> Self {
        Self::Color(color)
    }
}

impl ColorOrSystem {
    /// Create a new `ColorOrSystem` from a concrete color.
    #[must_use] pub const fn color(c: ColorU) -> Self {
        Self::Color(c)
    }
    
    /// Create a new `ColorOrSystem` from a system color reference.
    #[must_use] pub const fn system(s: SystemColorRef) -> Self {
        Self::System(s)
    }
    
    /// Resolve the color against a `SystemColors` struct.
    /// Returns the system color if available, or falls back to the provided default.
    #[must_use] pub fn resolve(&self, system_colors: &crate::system::SystemColors, fallback: ColorU) -> ColorU {
        match self {
            Self::Color(c) => *c,
            Self::System(ref_type) => ref_type.resolve(system_colors, fallback),
        }
    }
    
    /// Returns the concrete color if available, or a default fallback for system colors.
    /// Use this when `SystemColors` is not available (e.g., during rendering setup).
    #[must_use] pub const fn to_color_u_with_fallback(&self, fallback: ColorU) -> ColorU {
        match self {
            Self::Color(c) => *c,
            Self::System(_) => fallback,
        }
    }
    
    /// Returns the concrete color if available, or a gray fallback for system colors.
    #[must_use] pub const fn to_color_u_default(&self) -> ColorU {
        self.to_color_u_with_fallback(ColorU { r: 128, g: 128, b: 128, a: 255 })
    }
}

/// Reference to a specific system color.
/// These are resolved at runtime based on the user's system preferences.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum SystemColorRef {
    /// System text color (e.g., black on light theme, white on dark)
    Text,
    /// System background color
    Background,
    /// System accent color (user-selected highlight color)
    Accent,
    /// Text color when on accent background
    AccentText,
    /// Button face background color
    ButtonFace,
    /// Button text color
    ButtonText,
    /// Window/panel background color
    WindowBackground,
    /// Selection/highlight background color
    SelectionBackground,
    /// Text color when selected
    SelectionText,
}

impl SystemColorRef {
    /// Resolve this system color reference against actual system colors.
    #[must_use] pub fn resolve(&self, colors: &crate::system::SystemColors, fallback: ColorU) -> ColorU {
        match self {
            Self::Text => colors.text.as_option().copied().unwrap_or(fallback),
            Self::Background => colors.background.as_option().copied().unwrap_or(fallback),
            Self::Accent => colors.accent.as_option().copied().unwrap_or(fallback),
            Self::AccentText => colors.accent_text.as_option().copied().unwrap_or(fallback),
            Self::ButtonFace => colors.button_face.as_option().copied().unwrap_or(fallback),
            Self::ButtonText => colors.button_text.as_option().copied().unwrap_or(fallback),
            Self::WindowBackground => colors.window_background.as_option().copied().unwrap_or(fallback),
            Self::SelectionBackground => colors.selection_background.as_option().copied().unwrap_or(fallback),
            Self::SelectionText => colors.selection_text.as_option().copied().unwrap_or(fallback),
        }
    }
    
    /// Get the CSS syntax for this system color reference.
    #[must_use] pub const fn as_css_str(&self) -> &'static str {
        match self {
            Self::Text => "system:text",
            Self::Background => "system:background",
            Self::Accent => "system:accent",
            Self::AccentText => "system:accent-text",
            Self::ButtonFace => "system:button-face",
            Self::ButtonText => "system:button-text",
            Self::WindowBackground => "system:window-background",
            Self::SelectionBackground => "system:selection-background",
            Self::SelectionText => "system:selection-text",
        }
    }
}

// --- PARSER ---

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub enum CssColorComponent {
    Red,
    Green,
    Blue,
    Hue,
    Saturation,
    Lightness,
    Alpha,
}

#[derive(Clone, PartialEq)]
pub enum CssColorParseError<'a> {
    InvalidColor(&'a str),
    InvalidFunctionName(&'a str),
    InvalidColorComponent(u8),
    IntValueParseErr(ParseIntError),
    FloatValueParseErr(ParseFloatError),
    FloatValueOutOfRange(f32),
    MissingColorComponent(CssColorComponent),
    ExtraArguments(&'a str),
    UnclosedColor(&'a str),
    EmptyInput,
    DirectionParseError(CssDirectionParseError<'a>),
    UnsupportedDirection(&'a str),
    InvalidPercentage(PercentageParseError),
}

impl_debug_as_display!(CssColorParseError<'a>);
impl_display! {CssColorParseError<'a>, {
    InvalidColor(i) => format!("Invalid CSS color: \"{}\"", i),
    InvalidFunctionName(i) => format!("Invalid function name, expected one of: \"rgb\", \"rgba\", \"hsl\", \"hsla\" got: \"{}\"", i),
    InvalidColorComponent(i) => format!("Invalid color component when parsing CSS color: \"{}\"", i),
    IntValueParseErr(e) => format!("CSS color component: Value not in range between 00 - FF: \"{}\"", e),
    FloatValueParseErr(e) => format!("CSS color component: Value cannot be parsed as floating point number: \"{}\"", e),
    FloatValueOutOfRange(v) => format!("CSS color component: Value not in range between 0.0 - 1.0: \"{}\"", v),
    MissingColorComponent(c) => format!("CSS color is missing {:?} component", c),
    ExtraArguments(a) => format!("Extra argument to CSS color: \"{}\"", a),
    EmptyInput => format!("Empty color string."),
    UnclosedColor(i) => format!("Unclosed color: \"{}\"", i),
    DirectionParseError(e) => format!("Could not parse direction argument for CSS color: \"{}\"", e),
    UnsupportedDirection(d) => format!("Unsupported direction type for CSS color: \"{}\"", d),
    InvalidPercentage(p) => format!("Invalid percentage when parsing CSS color: \"{}\"", p),
}}

impl From<ParseIntError> for CssColorParseError<'_> {
    fn from(e: ParseIntError) -> Self {
        CssColorParseError::IntValueParseErr(e)
    }
}
impl From<ParseFloatError> for CssColorParseError<'_> {
    fn from(e: ParseFloatError) -> Self {
        CssColorParseError::FloatValueParseErr(e)
    }
}
impl From<core::num::ParseIntError> for CssColorParseError<'_> {
    fn from(e: core::num::ParseIntError) -> Self {
        CssColorParseError::IntValueParseErr(ParseIntError::from(e))
    }
}
impl From<core::num::ParseFloatError> for CssColorParseError<'_> {
    fn from(e: core::num::ParseFloatError) -> Self {
        CssColorParseError::FloatValueParseErr(ParseFloatError::from(e))
    }
}
impl_from!(
    CssDirectionParseError<'a>,
    CssColorParseError::DirectionParseError
);

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssColorParseErrorOwned {
    InvalidColor(AzString),
    InvalidFunctionName(AzString),
    InvalidColorComponent(u8),
    IntValueParseErr(ParseIntError),
    FloatValueParseErr(ParseFloatError),
    FloatValueOutOfRange(f32),
    MissingColorComponent(CssColorComponent),
    ExtraArguments(AzString),
    UnclosedColor(AzString),
    EmptyInput,
    DirectionParseError(CssDirectionParseErrorOwned),
    UnsupportedDirection(AzString),
    InvalidPercentage(PercentageParseError),
}

impl CssColorParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssColorParseErrorOwned {
        match self {
            CssColorParseError::InvalidColor(s) => {
                CssColorParseErrorOwned::InvalidColor((*s).to_string().into())
            }
            CssColorParseError::InvalidFunctionName(s) => {
                CssColorParseErrorOwned::InvalidFunctionName((*s).to_string().into())
            }
            CssColorParseError::InvalidColorComponent(n) => {
                CssColorParseErrorOwned::InvalidColorComponent(*n)
            }
            CssColorParseError::IntValueParseErr(e) => {
                CssColorParseErrorOwned::IntValueParseErr((*e))
            }
            CssColorParseError::FloatValueParseErr(e) => {
                CssColorParseErrorOwned::FloatValueParseErr((*e))
            }
            CssColorParseError::FloatValueOutOfRange(n) => {
                CssColorParseErrorOwned::FloatValueOutOfRange(*n)
            }
            CssColorParseError::MissingColorComponent(c) => {
                CssColorParseErrorOwned::MissingColorComponent(*c)
            }
            CssColorParseError::ExtraArguments(s) => {
                CssColorParseErrorOwned::ExtraArguments((*s).to_string().into())
            }
            CssColorParseError::UnclosedColor(s) => {
                CssColorParseErrorOwned::UnclosedColor((*s).to_string().into())
            }
            CssColorParseError::EmptyInput => CssColorParseErrorOwned::EmptyInput,
            CssColorParseError::DirectionParseError(e) => {
                CssColorParseErrorOwned::DirectionParseError(e.to_contained())
            }
            CssColorParseError::UnsupportedDirection(s) => {
                CssColorParseErrorOwned::UnsupportedDirection((*s).to_string().into())
            }
            CssColorParseError::InvalidPercentage(e) => {
                CssColorParseErrorOwned::InvalidPercentage(e.clone())
            }
        }
    }
}

impl CssColorParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssColorParseError<'_> {
        match self {
            Self::InvalidColor(s) => CssColorParseError::InvalidColor(s),
            Self::InvalidFunctionName(s) => {
                CssColorParseError::InvalidFunctionName(s)
            }
            Self::InvalidColorComponent(n) => {
                CssColorParseError::InvalidColorComponent(*n)
            }
            Self::IntValueParseErr(e) => {
                CssColorParseError::IntValueParseErr(*e)
            }
            Self::FloatValueParseErr(e) => {
                CssColorParseError::FloatValueParseErr(*e)
            }
            Self::FloatValueOutOfRange(n) => {
                CssColorParseError::FloatValueOutOfRange(*n)
            }
            Self::MissingColorComponent(c) => {
                CssColorParseError::MissingColorComponent(*c)
            }
            Self::ExtraArguments(s) => CssColorParseError::ExtraArguments(s),
            Self::UnclosedColor(s) => CssColorParseError::UnclosedColor(s),
            Self::EmptyInput => CssColorParseError::EmptyInput,
            Self::DirectionParseError(e) => {
                CssColorParseError::DirectionParseError(e.to_shared())
            }
            Self::UnsupportedDirection(s) => {
                CssColorParseError::UnsupportedDirection(s)
            }
            Self::InvalidPercentage(e) => {
                CssColorParseError::InvalidPercentage(e.clone())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_css_color(input: &str) -> Result<ColorU, CssColorParseError<'_>> {
    let input = input.trim();
    if input.starts_with('#') {
        parse_color_no_hash(&input[1..])
    } else {
        use crate::props::basic::parse::{parse_parentheses, ParenthesisParseError};
        match parse_parentheses(input, &["rgba", "rgb", "hsla", "hsl"]) {
            Ok((stopword, inner_value)) => match stopword {
                "rgba" => parse_color_rgb(inner_value, true),
                "rgb" => parse_color_rgb(inner_value, false),
                "hsla" => parse_color_hsl(inner_value, true),
                "hsl" => parse_color_hsl(inner_value, false),
                _ => unreachable!(),
            },
            Err(e) => match e {
                ParenthesisParseError::UnclosedBraces => {
                    Err(CssColorParseError::UnclosedColor(input))
                }
                ParenthesisParseError::EmptyInput => Err(CssColorParseError::EmptyInput),
                ParenthesisParseError::StopWordNotFound(stopword) => {
                    Err(CssColorParseError::InvalidFunctionName(stopword))
                }
                ParenthesisParseError::NoClosingBraceFound => {
                    Err(CssColorParseError::UnclosedColor(input))
                }
                ParenthesisParseError::NoOpeningBraceFound => parse_color_builtin(input),
            },
        }
    }
}

/// Parse a color that can be either a concrete color or a system color reference.
/// 
/// Supports all standard CSS color formats plus:
/// - `system:accent` - System accent/highlight color
/// - `system:text` - System text color
/// - `system:background` - System background color
/// - `system:selection-background` - Selection/highlight background
/// - `system:selection-text` - Text color when selected
/// - `system:button-face` - Button background color
/// - `system:button-text` - Button text color
/// - `system:window-background` - Window background color
/// - `system:accent-text` - Text color on accent background
#[cfg(feature = "parser")]
pub fn parse_color_or_system(input: &str) -> Result<ColorOrSystem, CssColorParseError<'_>> {
    let input = input.trim();
    
    // Check for system color syntax: "system:name"
    if let Some(system_name) = input.strip_prefix("system:") {
        let system_ref = match system_name.trim() {
            "text" => SystemColorRef::Text,
            "background" => SystemColorRef::Background,
            "accent" => SystemColorRef::Accent,
            "accent-text" => SystemColorRef::AccentText,
            "button-face" => SystemColorRef::ButtonFace,
            "button-text" => SystemColorRef::ButtonText,
            "window-background" => SystemColorRef::WindowBackground,
            "selection-background" => SystemColorRef::SelectionBackground,
            "selection-text" => SystemColorRef::SelectionText,
            _ => return Err(CssColorParseError::InvalidColor(input)),
        };
        return Ok(ColorOrSystem::System(system_ref));
    }
    
    // Otherwise parse as regular color
    parse_css_color(input).map(ColorOrSystem::Color)
}

#[cfg(feature = "parser")]
fn parse_color_no_hash(input: &str) -> Result<ColorU, CssColorParseError<'_>> {
    #[inline]
    const fn from_hex<'a>(c: u8) -> Result<u8, CssColorParseError<'a>> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(CssColorParseError::InvalidColorComponent(c)),
        }
    }

    match input.len() {
        3 => {
            let mut bytes = input.bytes();
            let r = bytes.next().unwrap();
            let g = bytes.next().unwrap();
            let b = bytes.next().unwrap();
            Ok(ColorU::new_rgb(
                from_hex(r)? * 17,
                from_hex(g)? * 17,
                from_hex(b)? * 17,
            ))
        }
        4 => {
            let mut bytes = input.bytes();
            let r = bytes.next().unwrap();
            let g = bytes.next().unwrap();
            let b = bytes.next().unwrap();
            let a = bytes.next().unwrap();
            Ok(ColorU::new(
                from_hex(r)? * 17,
                from_hex(g)? * 17,
                from_hex(b)? * 17,
                from_hex(a)? * 17,
            ))
        }
        6 => {
            let val = u32::from_str_radix(input, 16)?;
            Ok(ColorU::new_rgb(
                ((val >> 16) & 0xFF) as u8,
                ((val >> 8) & 0xFF) as u8,
                (val & 0xFF) as u8,
            ))
        }
        8 => {
            let val = u32::from_str_radix(input, 16)?;
            Ok(ColorU::new(
                ((val >> 24) & 0xFF) as u8,
                ((val >> 16) & 0xFF) as u8,
                ((val >> 8) & 0xFF) as u8,
                (val & 0xFF) as u8,
            ))
        }
        _ => Err(CssColorParseError::InvalidColor(input)),
    }
}

#[cfg(feature = "parser")]
fn parse_color_rgb(
    input: &str,
    parse_alpha: bool,
) -> Result<ColorU, CssColorParseError<'_>> {
    let mut components = input.split(',').map(str::trim);
    let rgb_color = parse_color_rgb_components(&mut components)?;
    let a = if parse_alpha {
        parse_alpha_component(&mut components)?
    } else {
        255
    };
    if let Some(arg) = components.next() {
        return Err(CssColorParseError::ExtraArguments(arg));
    }
    Ok(ColorU { a, ..rgb_color })
}

#[cfg(feature = "parser")]
fn parse_color_rgb_components<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn component_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<u8, CssColorParseError<'a>> {
        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }
        Ok(c.parse::<u8>()?)
    }
    Ok(ColorU {
        r: component_from_str(components, CssColorComponent::Red)?,
        g: component_from_str(components, CssColorComponent::Green)?,
        b: component_from_str(components, CssColorComponent::Blue)?,
        a: 255,
    })
}

#[cfg(feature = "parser")]
fn parse_color_hsl(
    input: &str,
    parse_alpha: bool,
) -> Result<ColorU, CssColorParseError<'_>> {
    let mut components = input.split(',').map(str::trim);
    let rgb_color = parse_color_hsl_components(&mut components)?;
    let a = if parse_alpha {
        parse_alpha_component(&mut components)?
    } else {
        255
    };
    if let Some(arg) = components.next() {
        return Err(CssColorParseError::ExtraArguments(arg));
    }
    Ok(ColorU { a, ..rgb_color })
}

#[cfg(feature = "parser")]
fn parse_color_hsl_components<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn angle_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<f32, CssColorParseError<'a>> {
        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }
        let dir = parse_direction(c)?;
        match dir {
            Direction::Angle(deg) => Ok(deg.to_degrees()),
            Direction::FromTo(_) => Err(CssColorParseError::UnsupportedDirection(c)),
        }
    }

    #[inline]
    fn percent_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<f32, CssColorParseError<'a>> {
        use crate::props::basic::parse_percentage_value;

        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }

        // Modern CSS allows both percentage and unitless values for HSL
        Ok(parse_percentage_value(c)
            .map_err(CssColorParseError::InvalidPercentage)?
            .normalized()
            * 100.0)
    }

    #[inline]
    fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
        let s = s / 100.0;
        let l = l / 100.0;
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h_prime = h / 60.0;
        let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
        let (r1, g1, b1) = if (0.0..1.0).contains(&h_prime) {
            (c, x, 0.0)
        } else if (1.0..2.0).contains(&h_prime) {
            (x, c, 0.0)
        } else if (2.0..3.0).contains(&h_prime) {
            (0.0, c, x)
        } else if (3.0..4.0).contains(&h_prime) {
            (0.0, x, c)
        } else if (4.0..5.0).contains(&h_prime) {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        let m = l - c / 2.0;
        (
            channel_to_u8((r1 + m) * 255.0),
            channel_to_u8((g1 + m) * 255.0),
            channel_to_u8((b1 + m) * 255.0),
        )
    }

    let (h, s, l) = (
        angle_from_str(components, CssColorComponent::Hue)?,
        percent_from_str(components, CssColorComponent::Saturation)?,
        percent_from_str(components, CssColorComponent::Lightness)?,
    );

    let (r, g, b) = hsl_to_rgb(h, s, l);
    Ok(ColorU { r, g, b, a: 255 })
}

#[cfg(feature = "parser")]
fn parse_alpha_component<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<u8, CssColorParseError<'a>> {
    let a_str = components
        .next()
        .ok_or(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha,
        ))?;
    if a_str.is_empty() {
        return Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha,
        ));
    }
    let a = a_str.parse::<f32>()?;
    if !(0.0..=1.0).contains(&a) {
        return Err(CssColorParseError::FloatValueOutOfRange(a));
    }
    Ok(channel_to_u8((a * 255.0).round()))
}

#[cfg(feature = "parser")]
fn parse_color_builtin(input: &str) -> Result<ColorU, CssColorParseError<'_>> {
    let (r, g, b, a) = match input.to_lowercase().as_str() {
        "aliceblue" => (240, 248, 255, 255),
        "antiquewhite" => (250, 235, 215, 255),
        "aqua" => (0, 255, 255, 255),
        "aquamarine" => (127, 255, 212, 255),
        "azure" => (240, 255, 255, 255),
        "beige" => (245, 245, 220, 255),
        "bisque" => (255, 228, 196, 255),
        "black" => (0, 0, 0, 255),
        "blanchedalmond" => (255, 235, 205, 255),
        "blue" => (0, 0, 255, 255),
        "blueviolet" => (138, 43, 226, 255),
        "brown" => (165, 42, 42, 255),
        "burlywood" => (222, 184, 135, 255),
        "cadetblue" => (95, 158, 160, 255),
        "chartreuse" => (127, 255, 0, 255),
        "chocolate" => (210, 105, 30, 255),
        "coral" => (255, 127, 80, 255),
        "cornflowerblue" => (100, 149, 237, 255),
        "cornsilk" => (255, 248, 220, 255),
        "crimson" => (220, 20, 60, 255),
        "cyan" => (0, 255, 255, 255),
        "darkblue" => (0, 0, 139, 255),
        "darkcyan" => (0, 139, 139, 255),
        "darkgoldenrod" => (184, 134, 11, 255),
        "darkgray" | "darkgrey" => (169, 169, 169, 255),
        "darkgreen" => (0, 100, 0, 255),
        "darkkhaki" => (189, 183, 107, 255),
        "darkmagenta" => (139, 0, 139, 255),
        "darkolivegreen" => (85, 107, 47, 255),
        "darkorange" => (255, 140, 0, 255),
        "darkorchid" => (153, 50, 204, 255),
        "darkred" => (139, 0, 0, 255),
        "darksalmon" => (233, 150, 122, 255),
        "darkseagreen" => (143, 188, 143, 255),
        "darkslateblue" => (72, 61, 139, 255),
        "darkslategray" | "darkslategrey" => (47, 79, 79, 255),
        "darkturquoise" => (0, 206, 209, 255),
        "darkviolet" => (148, 0, 211, 255),
        "deeppink" => (255, 20, 147, 255),
        "deepskyblue" => (0, 191, 255, 255),
        "dimgray" | "dimgrey" => (105, 105, 105, 255),
        "dodgerblue" => (30, 144, 255, 255),
        "firebrick" => (178, 34, 34, 255),
        "floralwhite" => (255, 250, 240, 255),
        "forestgreen" => (34, 139, 34, 255),
        "fuchsia" => (255, 0, 255, 255),
        "gainsboro" => (220, 220, 220, 255),
        "ghostwhite" => (248, 248, 255, 255),
        "gold" => (255, 215, 0, 255),
        "goldenrod" => (218, 165, 32, 255),
        "gray" | "grey" => (128, 128, 128, 255),
        "green" => (0, 128, 0, 255),
        "greenyellow" => (173, 255, 47, 255),
        "honeydew" => (240, 255, 240, 255),
        "hotpink" => (255, 105, 180, 255),
        "indianred" => (205, 92, 92, 255),
        "indigo" => (75, 0, 130, 255),
        "ivory" => (255, 255, 240, 255),
        "khaki" => (240, 230, 140, 255),
        "lavender" => (230, 230, 250, 255),
        "lavenderblush" => (255, 240, 245, 255),
        "lawngreen" => (124, 252, 0, 255),
        "lemonchiffon" => (255, 250, 205, 255),
        "lightblue" => (173, 216, 230, 255),
        "lightcoral" => (240, 128, 128, 255),
        "lightcyan" => (224, 255, 255, 255),
        "lightgoldenrodyellow" => (250, 250, 210, 255),
        "lightgray" | "lightgrey" => (211, 211, 211, 255),
        "lightgreen" => (144, 238, 144, 255),
        "lightpink" => (255, 182, 193, 255),
        "lightsalmon" => (255, 160, 122, 255),
        "lightseagreen" => (32, 178, 170, 255),
        "lightskyblue" => (135, 206, 250, 255),
        "lightslategray" | "lightslategrey" => (119, 136, 153, 255),
        "lightsteelblue" => (176, 196, 222, 255),
        "lightyellow" => (255, 255, 224, 255),
        "lime" => (0, 255, 0, 255),
        "limegreen" => (50, 205, 50, 255),
        "linen" => (250, 240, 230, 255),
        "magenta" => (255, 0, 255, 255),
        "maroon" => (128, 0, 0, 255),
        "mediumaquamarine" => (102, 205, 170, 255),
        "mediumblue" => (0, 0, 205, 255),
        "mediumorchid" => (186, 85, 211, 255),
        "mediumpurple" => (147, 112, 219, 255),
        "mediumseagreen" => (60, 179, 113, 255),
        "mediumslateblue" => (123, 104, 238, 255),
        "mediumspringgreen" => (0, 250, 154, 255),
        "mediumturquoise" => (72, 209, 204, 255),
        "mediumvioletred" => (199, 21, 133, 255),
        "midnightblue" => (25, 25, 112, 255),
        "mintcream" => (245, 255, 250, 255),
        "mistyrose" => (255, 228, 225, 255),
        "moccasin" => (255, 228, 181, 255),
        "navajowhite" => (255, 222, 173, 255),
        "navy" => (0, 0, 128, 255),
        "oldlace" => (253, 245, 230, 255),
        "olive" => (128, 128, 0, 255),
        "olivedrab" => (107, 142, 35, 255),
        "orange" => (255, 165, 0, 255),
        "orangered" => (255, 69, 0, 255),
        "orchid" => (218, 112, 214, 255),
        "palegoldenrod" => (238, 232, 170, 255),
        "palegreen" => (152, 251, 152, 255),
        "paleturquoise" => (175, 238, 238, 255),
        "palevioletred" => (219, 112, 147, 255),
        "papayawhip" => (255, 239, 213, 255),
        "peachpuff" => (255, 218, 185, 255),
        "peru" => (205, 133, 63, 255),
        "pink" => (255, 192, 203, 255),
        "plum" => (221, 160, 221, 255),
        "powderblue" => (176, 224, 230, 255),
        "purple" => (128, 0, 128, 255),
        "rebeccapurple" => (102, 51, 153, 255),
        "red" => (255, 0, 0, 255),
        "rosybrown" => (188, 143, 143, 255),
        "royalblue" => (65, 105, 225, 255),
        "saddlebrown" => (139, 69, 19, 255),
        "salmon" => (250, 128, 114, 255),
        "sandybrown" => (244, 164, 96, 255),
        "seagreen" => (46, 139, 87, 255),
        "seashell" => (255, 245, 238, 255),
        "sienna" => (160, 82, 45, 255),
        "silver" => (192, 192, 192, 255),
        "skyblue" => (135, 206, 235, 255),
        "slateblue" => (106, 90, 205, 255),
        "slategray" | "slategrey" => (112, 128, 144, 255),
        "snow" => (255, 250, 250, 255),
        "springgreen" => (0, 255, 127, 255),
        "steelblue" => (70, 130, 180, 255),
        "tan" => (210, 180, 140, 255),
        "teal" => (0, 128, 128, 255),
        "thistle" => (216, 191, 216, 255),
        "tomato" => (255, 99, 71, 255),
        "transparent" => (0, 0, 0, 0),
        "turquoise" => (64, 224, 208, 255),
        "violet" => (238, 130, 238, 255),
        "wheat" => (245, 222, 179, 255),
        "white" => (255, 255, 255, 255),
        "whitesmoke" => (245, 245, 245, 255),
        "yellow" => (255, 255, 0, 255),
        "yellowgreen" => (154, 205, 50, 255),
        _ => return Err(CssColorParseError::InvalidColor(input)),
    };
    Ok(ColorU { r, g, b, a })
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_keywords() {
        assert_eq!(parse_css_color("red").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("blue").unwrap(), ColorU::BLUE);
        assert_eq!(parse_css_color("transparent").unwrap(), ColorU::TRANSPARENT);
        assert_eq!(
            parse_css_color("rebeccapurple").unwrap(),
            ColorU::new_rgb(102, 51, 153)
        );
    }

    #[test]
    fn test_parse_color_hex() {
        // 3-digit
        assert_eq!(parse_css_color("#f00").unwrap(), ColorU::RED);
        // 4-digit
        assert_eq!(
            parse_css_color("#f008").unwrap(),
            ColorU::new(255, 0, 0, 136)
        );
        // 6-digit
        assert_eq!(parse_css_color("#00ff00").unwrap(), ColorU::GREEN);
        // 8-digit
        assert_eq!(
            parse_css_color("#0000ff80").unwrap(),
            ColorU::new(0, 0, 255, 128)
        );
        // Uppercase
        assert_eq!(
            parse_css_color("#FFC0CB").unwrap(),
            ColorU::new_rgb(255, 192, 203)
        ); // Pink
    }

    #[test]
    fn test_parse_color_rgb() {
        assert_eq!(parse_css_color("rgb(255, 0, 0)").unwrap(), ColorU::RED);
        assert_eq!(
            parse_css_color("rgba(0, 255, 0, 0.5)").unwrap(),
            ColorU::new(0, 255, 0, 128)
        );
        assert_eq!(
            parse_css_color("rgba(10, 20, 30, 1)").unwrap(),
            ColorU::new_rgb(10, 20, 30)
        );
        assert_eq!(parse_css_color("rgb( 0 , 0 , 0 )").unwrap(), ColorU::BLACK);
    }

    #[test]
    fn test_parse_color_hsl() {
        assert_eq!(parse_css_color("hsl(0, 100%, 50%)").unwrap(), ColorU::RED);
        assert_eq!(
            parse_css_color("hsl(120, 100%, 50%)").unwrap(),
            ColorU::GREEN
        );
        assert_eq!(
            parse_css_color("hsla(240, 100%, 50%, 0.5)").unwrap(),
            ColorU::new(0, 0, 255, 128)
        );
        assert_eq!(parse_css_color("hsl(0, 0%, 0%)").unwrap(), ColorU::BLACK);
    }

    #[test]
    fn test_parse_color_errors() {
        assert!(parse_css_color("redd").is_err());
        assert!(parse_css_color("#12345").is_err()); // Invalid length
        assert!(parse_css_color("#ggg").is_err()); // Invalid hex digit
        assert!(parse_css_color("rgb(255, 0)").is_err()); // Missing component
        assert!(parse_css_color("rgba(255, 0, 0, 2)").is_err()); // Alpha out of range
        assert!(parse_css_color("rgb(256, 0, 0)").is_err()); // Value out of range
                                                             // Modern CSS allows both hsl(0, 100%, 50%) and hsl(0 100 50)
        assert!(parse_css_color("hsl(0, 100, 50%)").is_ok()); // Valid in modern CSS
        assert!(parse_css_color("rgb(255 0 0)").is_err()); // Missing commas (this implementation
                                                           // requires commas)
    }

    #[test]
    fn test_parse_system_colors() {
        // Test parsing system color syntax
        assert_eq!(
            parse_color_or_system("system:accent").unwrap(),
            ColorOrSystem::System(SystemColorRef::Accent)
        );
        assert_eq!(
            parse_color_or_system("system:text").unwrap(),
            ColorOrSystem::System(SystemColorRef::Text)
        );
        assert_eq!(
            parse_color_or_system("system:background").unwrap(),
            ColorOrSystem::System(SystemColorRef::Background)
        );
        assert_eq!(
            parse_color_or_system("system:selection-background").unwrap(),
            ColorOrSystem::System(SystemColorRef::SelectionBackground)
        );
        assert_eq!(
            parse_color_or_system("system:selection-text").unwrap(),
            ColorOrSystem::System(SystemColorRef::SelectionText)
        );
        assert_eq!(
            parse_color_or_system("system:accent-text").unwrap(),
            ColorOrSystem::System(SystemColorRef::AccentText)
        );
        assert_eq!(
            parse_color_or_system("system:button-face").unwrap(),
            ColorOrSystem::System(SystemColorRef::ButtonFace)
        );
        assert_eq!(
            parse_color_or_system("system:button-text").unwrap(),
            ColorOrSystem::System(SystemColorRef::ButtonText)
        );
        assert_eq!(
            parse_color_or_system("system:window-background").unwrap(),
            ColorOrSystem::System(SystemColorRef::WindowBackground)
        );
        
        // Invalid system color should error
        assert!(parse_color_or_system("system:invalid").is_err());
        
        // Regular colors should still work
        assert_eq!(
            parse_color_or_system("red").unwrap(),
            ColorOrSystem::Color(ColorU::RED)
        );
        assert_eq!(
            parse_color_or_system("#ff0000").unwrap(),
            ColorOrSystem::Color(ColorU::RED)
        );
    }

    #[test]
    fn test_system_color_resolution() {
        use crate::system::SystemColors;
        
        let system_colors = SystemColors {
            text: OptionColorU::Some(ColorU::BLACK),
            secondary_text: OptionColorU::None,
            tertiary_text: OptionColorU::None,
            background: OptionColorU::Some(ColorU::WHITE),
            accent: OptionColorU::Some(ColorU::new_rgb(0, 122, 255)), // macOS blue
            accent_text: OptionColorU::Some(ColorU::WHITE),
            button_face: OptionColorU::Some(ColorU::new_rgb(240, 240, 240)),
            button_text: OptionColorU::Some(ColorU::BLACK),
            disabled_text: OptionColorU::None,
            window_background: OptionColorU::Some(ColorU::WHITE),
            under_page_background: OptionColorU::None,
            selection_background: OptionColorU::Some(ColorU::new_rgb(0, 120, 215)),
            selection_text: OptionColorU::Some(ColorU::WHITE),
            selection_background_inactive: OptionColorU::None,
            selection_text_inactive: OptionColorU::None,
            link: OptionColorU::None,
            separator: OptionColorU::None,
            grid: OptionColorU::None,
            find_highlight: OptionColorU::None,
            sidebar_background: OptionColorU::None,
            sidebar_selection: OptionColorU::None,
        };
        
        // Test resolution of system colors
        let accent_ref = ColorOrSystem::System(SystemColorRef::Accent);
        let resolved = accent_ref.resolve(&system_colors, ColorU::GRAY);
        assert_eq!(resolved, ColorU::new_rgb(0, 122, 255));
        
        // Test resolution with fallback when color is not set
        let empty_colors = SystemColors::default();
        let resolved_fallback = accent_ref.resolve(&empty_colors, ColorU::GRAY);
        assert_eq!(resolved_fallback, ColorU::GRAY);
        
        // Test that concrete colors just return themselves
        let concrete = ColorOrSystem::Color(ColorU::RED);
        let resolved_concrete = concrete.resolve(&system_colors, ColorU::GRAY);
        assert_eq!(resolved_concrete, ColorU::RED);
    }

    #[test]
    fn test_system_color_css_str() {
        assert_eq!(SystemColorRef::Accent.as_css_str(), "system:accent");
        assert_eq!(SystemColorRef::Text.as_css_str(), "system:text");
        assert_eq!(SystemColorRef::Background.as_css_str(), "system:background");
        assert_eq!(SystemColorRef::SelectionBackground.as_css_str(), "system:selection-background");
    }
}

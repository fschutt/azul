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
const fn channel_to_u8(v: f32) -> u8 {
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

// Colour math keeps explicit `a*b + c` rather than `mul_add`: the latter is a
// software `fmaf` (slower) without target `+fma` and changes results bit-for-bit.
#[allow(clippy::suboptimal_flops)]
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
        // APCA 0.0.98G constants
        const NORMBLKTXT: f32 = 0.56;
        const NORMWHT: f32 = 0.57;
        const REVTXT: f32 = 0.62;
        const REVWHT: f32 = 0.65;
        const BLKTHRS: f32 = 0.022;
        const SCALEBLKT: f32 = 1.414;
        const SCALEWHT: f32 = 1.14;

        // Convert to Y (luminance) using sRGB TRC
        let text_y = self.relative_luminance();
        let bg_y = background.relative_luminance();
        
        // Soft clamp
        let text_y = if text_y < 0.0 { 0.0 } else { text_y };
        let bg_y = if bg_y < 0.0 { 0.0 } else { bg_y };
        
        
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
                CssColorParseErrorOwned::IntValueParseErr(*e)
            }
            CssColorParseError::FloatValueParseErr(e) => {
                CssColorParseErrorOwned::FloatValueParseErr(*e)
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `css-color` value.
pub fn parse_css_color(input: &str) -> Result<ColorU, CssColorParseError<'_>> {
    use crate::props::basic::parse::{parse_parentheses, ParenthesisParseError};

    let input = input.trim();
    if let Some(rest) = input.strip_prefix('#') {
        return parse_color_no_hash(rest);
    }

    match parse_parentheses(input, &["rgba", "rgb", "hsla", "hsl"]) {
        Ok((stopword, inner_value)) => match stopword {
            "rgba" => parse_color_rgb(inner_value, true),
            "rgb" => parse_color_rgb(inner_value, false),
            "hsla" => parse_color_hsl(inner_value, true),
            "hsl" => parse_color_hsl(inner_value, false),
            _ => unreachable!(),
        },
        Err(e) => match e {
            ParenthesisParseError::UnclosedBraces | ParenthesisParseError::NoClosingBraceFound => {
                Err(CssColorParseError::UnclosedColor(input))
            }
            ParenthesisParseError::EmptyInput => Err(CssColorParseError::EmptyInput),
            ParenthesisParseError::StopWordNotFound(stopword) => {
                Err(CssColorParseError::InvalidFunctionName(stopword))
            }
            ParenthesisParseError::NoOpeningBraceFound => parse_color_builtin(input),
        },
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
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `color-or-system` value.
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
#[allow(clippy::many_single_char_names)] // domain-standard h/s/l/r/g/b colour component names
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
    #[allow(clippy::suboptimal_flops)] // explicit FP; mul_add slower without +fma
    #[allow(clippy::many_single_char_names)] // domain-standard colour/coordinate component names
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
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
fn parse_color_builtin(input: &str) -> Result<ColorU, CssColorParseError<'_>> {
    let (r, g, b, a) = match input.to_lowercase().as_str() {
        "aliceblue" => (240, 248, 255, 255),
        "antiquewhite" => (250, 235, 215, 255),
        "aqua" | "cyan" => (0, 255, 255, 255),
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
        "fuchsia" | "magenta" => (255, 0, 255, 255),
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    use super::*;

    /// Every `ColorU` this module sweeps over. Chosen to hit the interesting
    /// channel boundaries (0 / 1 / 127 / 128 / 254 / 255) plus a few real colors.
    const SAMPLES: [ColorU; 10] = [
        ColorU { r: 0, g: 0, b: 0, a: 0 },
        ColorU { r: 0, g: 0, b: 0, a: 255 },
        ColorU { r: 255, g: 255, b: 255, a: 255 },
        ColorU { r: 255, g: 255, b: 255, a: 0 },
        ColorU { r: 1, g: 2, b: 3, a: 4 },
        ColorU { r: 127, g: 128, b: 129, a: 254 },
        ColorU { r: 254, g: 1, b: 128, a: 1 },
        ColorU { r: 128, g: 128, b: 128, a: 255 },
        ColorU { r: 255, g: 0, b: 0, a: 255 },
        ColorU { r: 13, g: 110, b: 253, a: 200 },
    ];

    // =====================================================================
    // numeric: channel_to_u8 (private) — the one float→int cast in the file
    // =====================================================================

    #[test]
    fn channel_to_u8_zero_and_negative_zero() {
        assert_eq!(channel_to_u8(0.0), 0);
        assert_eq!(channel_to_u8(-0.0), 0);
    }

    #[test]
    fn channel_to_u8_truncates_toward_zero_and_does_not_round() {
        assert_eq!(channel_to_u8(0.9), 0);
        assert_eq!(channel_to_u8(127.5), 127);
        assert_eq!(channel_to_u8(254.999), 254);
        assert_eq!(channel_to_u8(255.0), 255);
        assert_eq!(channel_to_u8(255.9), 255);
    }

    #[test]
    fn channel_to_u8_saturates_on_overflow_instead_of_wrapping() {
        assert_eq!(channel_to_u8(256.0), 255);
        assert_eq!(channel_to_u8(1e30), 255);
        assert_eq!(channel_to_u8(f32::MAX), 255);
    }

    #[test]
    fn channel_to_u8_negative_saturates_to_zero() {
        assert_eq!(channel_to_u8(-0.5), 0);
        assert_eq!(channel_to_u8(-1.0), 0);
        assert_eq!(channel_to_u8(-1e30), 0);
        assert_eq!(channel_to_u8(f32::MIN), 0);
    }

    #[test]
    fn channel_to_u8_nan_and_inf_are_defined_and_do_not_panic() {
        assert_eq!(channel_to_u8(f32::NAN), 0);
        assert_eq!(channel_to_u8(-f32::NAN), 0);
        assert_eq!(channel_to_u8(f32::INFINITY), 255);
        assert_eq!(channel_to_u8(f32::NEG_INFINITY), 0);
    }

    #[test]
    fn channel_to_u8_subnormal_inputs_do_not_panic() {
        assert_eq!(channel_to_u8(f32::MIN_POSITIVE), 0);
        assert_eq!(channel_to_u8(1e-45), 0);
        assert_eq!(channel_to_u8(-1e-45), 0);
    }

    // =====================================================================
    // constructors: rgba / rgb / new / new_rgb / with_alpha / with_alpha_f32
    // =====================================================================

    #[test]
    fn rgba_fields_match_args_at_min_and_max() {
        let min = ColorU::rgba(0, 0, 0, 0);
        assert_eq!((min.r, min.g, min.b, min.a), (0, 0, 0, 0));
        let max = ColorU::rgba(u8::MAX, u8::MAX, u8::MAX, u8::MAX);
        assert_eq!((max.r, max.g, max.b, max.a), (255, 255, 255, 255));
        let mixed = ColorU::rgba(1, 2, 3, 4);
        assert_eq!((mixed.r, mixed.g, mixed.b, mixed.a), (1, 2, 3, 4));
    }

    #[test]
    fn rgb_defaults_alpha_to_opaque() {
        assert_eq!(ColorU::rgb(0, 0, 0), ColorU::BLACK);
        assert_eq!(ColorU::rgb(1, 2, 3).a, ColorU::ALPHA_OPAQUE);
        assert_eq!(ColorU::rgb(u8::MAX, u8::MAX, u8::MAX), ColorU::WHITE);
    }

    #[test]
    fn new_and_new_rgb_are_exact_aliases() {
        for c in SAMPLES {
            assert_eq!(ColorU::new(c.r, c.g, c.b, c.a), ColorU::rgba(c.r, c.g, c.b, c.a));
            assert_eq!(ColorU::new_rgb(c.r, c.g, c.b), ColorU::rgb(c.r, c.g, c.b));
        }
    }

    #[test]
    fn with_alpha_keeps_rgb_for_every_alpha() {
        let base = ColorU::rgba(13, 110, 253, 7);
        for a in 0..=u8::MAX {
            let c = base.with_alpha(a);
            assert_eq!((c.r, c.g, c.b), (base.r, base.g, base.b));
            assert_eq!(c.a, a);
        }
    }

    #[test]
    fn with_alpha_f32_clamps_out_of_range_and_nan() {
        let base = ColorU::rgb(1, 2, 3);
        assert_eq!(base.with_alpha_f32(0.0).a, 0);
        assert_eq!(base.with_alpha_f32(1.0).a, 255);
        // Out of range clamps rather than wrapping.
        assert_eq!(base.with_alpha_f32(-1.0).a, 0);
        assert_eq!(base.with_alpha_f32(-1e30).a, 0);
        assert_eq!(base.with_alpha_f32(2.0).a, 255);
        assert_eq!(base.with_alpha_f32(1e30).a, 255);
        assert_eq!(base.with_alpha_f32(f32::INFINITY).a, 255);
        assert_eq!(base.with_alpha_f32(f32::NEG_INFINITY).a, 0);
        // `clamp` propagates NaN, and `NaN as u8` is 0 — fully transparent, not a panic.
        assert_eq!(base.with_alpha_f32(f32::NAN).a, 0);
        // RGB is never touched, whatever the alpha input.
        for a in [-1.0, 0.0, 0.5, 1.0, 2.0, f32::NAN, f32::INFINITY] {
            let c = base.with_alpha_f32(a);
            assert_eq!((c.r, c.g, c.b), (1, 2, 3));
        }
    }

    #[test]
    fn with_alpha_f32_truncates_rather_than_rounds() {
        // 0.5 * 255.0 == 127.5, and `as u8` truncates => 127.
        // NOTE: the `rgba(..., 0.5)` parser rounds the same value to 128
        // (`parse_alpha_component` calls `.round()` first). See report.
        assert_eq!(ColorU::rgb(0, 0, 0).with_alpha_f32(0.5).a, 127);
    }

    // =====================================================================
    // numeric: interpolate / lighten / darken / mix
    // =====================================================================

    #[test]
    fn interpolate_endpoints_are_exact() {
        for a in SAMPLES {
            for b in SAMPLES {
                assert_eq!(a.interpolate(&b, 0.0), a, "t=0 must return self");
                assert_eq!(a.interpolate(&b, 1.0), b, "t=1 must return other");
            }
        }
    }

    #[test]
    fn interpolate_midpoint_rounds_half_away_from_zero() {
        // 0 + 255 * 0.5 = 127.5, roundf => 128.
        assert_eq!(
            ColorU::BLACK.interpolate(&ColorU::WHITE, 0.5),
            ColorU::rgba(128, 128, 128, 255)
        );
    }

    #[test]
    fn interpolate_is_symmetric_under_swapped_endpoints() {
        for a in SAMPLES {
            for b in SAMPLES {
                assert_eq!(a.interpolate(&b, 0.25), b.interpolate(&a, 0.75));
            }
        }
    }

    #[test]
    fn interpolate_nan_t_is_defined_and_does_not_panic() {
        // t = NaN makes every channel NaN, and `NaN as u8` == 0.
        for a in SAMPLES {
            for b in SAMPLES {
                assert_eq!(a.interpolate(&b, f32::NAN), ColorU::rgba(0, 0, 0, 0));
            }
        }
    }

    #[test]
    fn interpolate_infinite_t_saturates_differing_channels() {
        // Channels that differ run off to +/-inf and saturate at the u8 bounds.
        let c = ColorU::rgba(0, 0, 0, 0).interpolate(&ColorU::WHITE, f32::INFINITY);
        assert_eq!(c, ColorU::rgba(255, 255, 255, 255));
        let c = ColorU::WHITE.interpolate(&ColorU::rgba(0, 0, 0, 0), f32::INFINITY);
        assert_eq!(c, ColorU::rgba(0, 0, 0, 0));
    }

    #[test]
    fn interpolate_infinite_t_zeroes_equal_channels() {
        // Where a channel is EQUAL in both colors the delta is 0.0, and
        // `0.0 * inf == NaN` => that channel collapses to 0 instead of
        // staying put. Both endpoints here are alpha=255, so alpha => 0.
        let c = ColorU::BLACK.interpolate(&ColorU::WHITE, f32::INFINITY);
        assert_eq!(c, ColorU::rgba(255, 255, 255, 0));
        // Interpolating a color with ITSELF at t=inf wipes it out entirely.
        assert_eq!(
            ColorU::RED.interpolate(&ColorU::RED, f32::INFINITY),
            ColorU::rgba(0, 0, 0, 0)
        );
    }

    #[test]
    fn interpolate_out_of_range_t_saturates_instead_of_wrapping() {
        // Extrapolating past the endpoints overshoots the u8 range; the cast must
        // saturate, not wrap (0 + 255*2 == 510 -> 255, not 254).
        assert_eq!(
            ColorU::BLACK.interpolate(&ColorU::WHITE, 2.0),
            ColorU::rgba(255, 255, 255, 255)
        );
        assert_eq!(
            ColorU::WHITE.interpolate(&ColorU::BLACK, -1.0),
            ColorU::rgba(255, 255, 255, 255)
        );
        assert_eq!(
            ColorU::WHITE.interpolate(&ColorU::BLACK, 2.0),
            ColorU::rgba(0, 0, 0, 255)
        );
        assert_eq!(
            ColorU::BLACK.interpolate(&ColorU::WHITE, -1.0),
            ColorU::rgba(0, 0, 0, 255)
        );
        // And the whole sample matrix must stay panic-free and deterministic.
        for t in [-1e30, -1.0, -0.5, 1.5, 2.0, 1e30] {
            for a in SAMPLES {
                for b in SAMPLES {
                    assert_eq!(a.interpolate(&b, t), a.interpolate(&b, t));
                }
            }
        }
    }

    #[test]
    fn lighten_and_darken_clamp_the_amount() {
        let base = ColorU::rgba(128, 128, 128, 77);
        // Below 0 clamps to 0 => unchanged.
        assert_eq!(base.lighten(0.0), base);
        assert_eq!(base.darken(0.0), base);
        assert_eq!(base.lighten(-1.0), base);
        assert_eq!(base.darken(-1e30), base);
        assert_eq!(base.lighten(f32::NEG_INFINITY), base);
        // Above 1 clamps to 1 => full white / full black, alpha preserved.
        assert_eq!(base.lighten(1.0), ColorU::rgba(255, 255, 255, 77));
        assert_eq!(base.lighten(2.0), ColorU::rgba(255, 255, 255, 77));
        assert_eq!(base.lighten(f32::INFINITY), ColorU::rgba(255, 255, 255, 77));
        assert_eq!(base.darken(1.0), ColorU::rgba(0, 0, 0, 77));
        assert_eq!(base.darken(1e30), ColorU::rgba(0, 0, 0, 77));
        assert_eq!(base.darken(f32::INFINITY), ColorU::rgba(0, 0, 0, 77));
    }

    #[test]
    fn lighten_and_darken_always_preserve_alpha() {
        for c in SAMPLES {
            for amount in [-1.0, 0.0, 0.3, 1.0, 2.0, f32::NAN, f32::INFINITY] {
                assert_eq!(c.lighten(amount).a, c.a);
                assert_eq!(c.darken(amount).a, c.a);
            }
        }
    }

    #[test]
    fn lighten_nan_amount_is_defined_and_does_not_panic() {
        // `f32::clamp` propagates NaN, so the RGB channels collapse to 0 while
        // alpha is explicitly restored afterwards.
        let c = ColorU::rgba(255, 0, 0, 200);
        assert_eq!(c.lighten(f32::NAN), ColorU::rgba(0, 0, 0, 200));
        assert_eq!(c.darken(f32::NAN), ColorU::rgba(0, 0, 0, 200));
    }

    #[test]
    fn mix_clamps_ratio_to_the_endpoints() {
        let a = ColorU::rgba(10, 20, 30, 40);
        let b = ColorU::rgba(200, 210, 220, 230);
        assert_eq!(a.mix(&b, 0.0), a);
        assert_eq!(a.mix(&b, 1.0), b);
        assert_eq!(a.mix(&b, -1.0), a);
        assert_eq!(a.mix(&b, f32::NEG_INFINITY), a);
        assert_eq!(a.mix(&b, 2.0), b);
        assert_eq!(a.mix(&b, 1e30), b);
        assert_eq!(a.mix(&b, f32::INFINITY), b);
    }

    #[test]
    fn mix_nan_ratio_is_defined_and_does_not_panic() {
        // Unlike lighten/darken, mix does NOT restore alpha => fully transparent.
        assert_eq!(
            ColorU::RED.mix(&ColorU::BLUE, f32::NAN),
            ColorU::rgba(0, 0, 0, 0)
        );
    }

    // =====================================================================
    // numeric: srgb_to_linear (private)
    // =====================================================================

    #[test]
    fn srgb_to_linear_endpoints_and_monotonicity() {
        assert_eq!(ColorU::srgb_to_linear(0.0), 0.0);
        assert!((ColorU::srgb_to_linear(1.0) - 1.0).abs() < 1e-5);
        // Monotonically non-decreasing over the whole 8-bit ramp.
        let mut prev = f32::NEG_INFINITY;
        for i in 0..=255u16 {
            let v = ColorU::srgb_to_linear(f32::from(i) / 255.0);
            assert!(v >= prev, "srgb_to_linear not monotonic at {i}");
            assert!((0.0..=1.0).contains(&v), "out of range at {i}: {v}");
            prev = v;
        }
    }

    #[test]
    fn srgb_to_linear_handles_the_piecewise_boundary() {
        // The branch flips at c == 0.03928 (linear below, gamma above).
        let below = ColorU::srgb_to_linear(0.03928);
        assert!((below - 0.03928 / 12.92).abs() < 1e-9);
        let above = ColorU::srgb_to_linear(0.03929);
        assert!(above > below, "must not go backwards across the boundary");
    }

    #[test]
    fn srgb_to_linear_nan_inf_and_negative_do_not_panic() {
        assert!(ColorU::srgb_to_linear(f32::NAN).is_nan());
        assert_eq!(ColorU::srgb_to_linear(f32::INFINITY), f32::INFINITY);
        assert_eq!(ColorU::srgb_to_linear(f32::NEG_INFINITY), f32::NEG_INFINITY);
        // Negative inputs take the linear branch and stay negative (deterministic).
        assert!(ColorU::srgb_to_linear(-1.0) < 0.0);
        assert_eq!(ColorU::srgb_to_linear(-0.0), -0.0);
    }

    // =====================================================================
    // getters: luminance / relative_luminance / is_light / is_dark
    // =====================================================================

    #[test]
    fn luminance_endpoints_and_range() {
        assert!((ColorU::BLACK.luminance() - 0.0).abs() < 1e-6);
        assert!((ColorU::WHITE.luminance() - 1.0).abs() < 1e-6);
        for r in (0..=255u16).step_by(17) {
            for g in (0..=255u16).step_by(51) {
                for b in (0..=255u16).step_by(85) {
                    #[allow(clippy::cast_possible_truncation)]
                    let l = ColorU::rgb(r as u8, g as u8, b as u8).luminance();
                    assert!(l.is_finite() && (-1e-6..=1.000_001).contains(&l), "luminance {l}");
                }
            }
        }
    }

    #[test]
    fn luminance_ignores_alpha() {
        for a in [0u8, 1, 128, 254, 255] {
            assert!((ColorU::rgba(10, 20, 30, a).luminance()
                - ColorU::rgba(10, 20, 30, 255).luminance())
                .abs()
                < 1e-9);
        }
    }

    #[test]
    fn relative_luminance_endpoints_and_range() {
        assert!((ColorU::BLACK.relative_luminance() - 0.0).abs() < 1e-6);
        assert!((ColorU::WHITE.relative_luminance() - 1.0).abs() < 1e-6);
        for i in 0..=255u16 {
            #[allow(clippy::cast_possible_truncation)]
            let l = ColorU::rgb(i as u8, i as u8, i as u8).relative_luminance();
            assert!(l.is_finite(), "non-finite relative_luminance at {i}");
            assert!((-1e-6..=1.000_001).contains(&l), "out of range at {i}: {l}");
        }
    }

    #[test]
    fn relative_luminance_is_monotonic_along_the_gray_ramp() {
        let mut prev = f32::NEG_INFINITY;
        for i in 0..=255u16 {
            #[allow(clippy::cast_possible_truncation)]
            let l = ColorU::rgb(i as u8, i as u8, i as u8).relative_luminance();
            assert!(l >= prev, "gray ramp not monotonic at {i}");
            prev = l;
        }
    }

    #[test]
    fn is_light_and_is_dark_are_exact_complements() {
        // The two predicates split at exactly 0.5 with no overlap and no gap,
        // for every single 8-bit color on the gray ramp plus the samples.
        for i in 0..=255u16 {
            #[allow(clippy::cast_possible_truncation)]
            let c = ColorU::rgb(i as u8, i as u8, i as u8);
            assert_ne!(c.is_light(), c.is_dark(), "not complementary at {i}");
        }
        for c in SAMPLES {
            assert_ne!(c.is_light(), c.is_dark());
        }
    }

    #[test]
    fn is_light_and_is_dark_known_values() {
        assert!(ColorU::WHITE.is_light());
        assert!(!ColorU::WHITE.is_dark());
        assert!(ColorU::BLACK.is_dark());
        assert!(!ColorU::BLACK.is_light());
        // Default (BLACK) is dark.
        assert!(ColorU::default().is_dark());
        // Mid gray is "dark" under WCAG relative luminance (~0.216, not 0.5).
        assert!(ColorU::rgb(128, 128, 128).is_dark());
    }

    // =====================================================================
    // contrast: contrast_ratio / meets_wcag_* / best_contrast_text
    // =====================================================================

    #[test]
    fn contrast_ratio_is_symmetric() {
        for a in SAMPLES {
            for b in SAMPLES {
                let ab = a.contrast_ratio(&b);
                let ba = b.contrast_ratio(&a);
                assert!((ab - ba).abs() < 1e-6, "asymmetric: {ab} vs {ba}");
            }
        }
    }

    #[test]
    fn contrast_ratio_stays_within_1_and_21() {
        for a in SAMPLES {
            for b in SAMPLES {
                let r = a.contrast_ratio(&b);
                assert!(r.is_finite(), "non-finite contrast ratio");
                assert!((0.999..=21.001).contains(&r), "contrast ratio out of range: {r}");
            }
            // Self-contrast is exactly 1.
            assert!((a.contrast_ratio(&a) - 1.0).abs() < 1e-6);
        }
        // Max contrast (fp gives 20.999998, not a clean 21.0).
        let max = ColorU::BLACK.contrast_ratio(&ColorU::WHITE);
        assert!((max - 21.0).abs() < 0.01, "black/white contrast was {max}");
    }

    #[test]
    fn meets_wcag_thresholds_agree_with_contrast_ratio() {
        for a in SAMPLES {
            for b in SAMPLES {
                let r = a.contrast_ratio(&b);
                assert_eq!(a.meets_wcag_aa(&b), r >= 4.5);
                assert_eq!(a.meets_wcag_aa_large(&b), r >= 3.0);
                assert_eq!(a.meets_wcag_aaa(&b), r >= 7.0);
                assert_eq!(a.meets_wcag_aaa_large(&b), r >= 4.5);
            }
        }
    }

    #[test]
    fn meets_wcag_known_true_and_false() {
        assert!(ColorU::BLACK.meets_wcag_aa(&ColorU::WHITE));
        assert!(ColorU::BLACK.meets_wcag_aaa(&ColorU::WHITE));
        assert!(ColorU::WHITE.meets_wcag_aa_large(&ColorU::BLACK));
        // A color has no contrast against itself.
        assert!(!ColorU::RED.meets_wcag_aa(&ColorU::RED));
        assert!(!ColorU::RED.meets_wcag_aa_large(&ColorU::RED));
        assert!(!ColorU::WHITE.meets_wcag_aaa(&ColorU::WHITE));
    }

    #[test]
    fn best_contrast_text_only_ever_returns_black_or_white() {
        for c in SAMPLES {
            let t = c.best_contrast_text();
            assert!(t == ColorU::WHITE || t == ColorU::BLACK, "got {t:?}");
            // contrast_text is documented as an alias.
            assert_eq!(c.contrast_text(), t);
        }
        for i in 0..=255u16 {
            #[allow(clippy::cast_possible_truncation)]
            let c = ColorU::rgb(i as u8, i as u8, i as u8);
            let t = c.best_contrast_text();
            assert!(t == ColorU::WHITE || t == ColorU::BLACK);
        }
    }

    #[test]
    fn best_contrast_text_picks_the_higher_contrast_option() {
        assert_eq!(ColorU::WHITE.best_contrast_text(), ColorU::BLACK);
        assert_eq!(ColorU::BLACK.best_contrast_text(), ColorU::WHITE);
        for c in SAMPLES {
            let t = c.best_contrast_text();
            let other = if t == ColorU::WHITE { ColorU::BLACK } else { ColorU::WHITE };
            assert!(
                c.contrast_ratio(&t) >= c.contrast_ratio(&other),
                "{c:?} picked the lower-contrast text color"
            );
        }
    }

    // =====================================================================
    // numeric: ensure_contrast (binary search — must terminate + saturate)
    // =====================================================================

    #[test]
    fn ensure_contrast_returns_self_when_already_compliant() {
        // 21:1 already, nothing to do.
        assert_eq!(
            ColorU::BLACK.ensure_contrast(&ColorU::WHITE, 4.5),
            ColorU::BLACK
        );
        let gray = ColorU::rgb(128, 128, 128);
        // 5.3:1 against black already clears 4.5.
        assert_eq!(gray.ensure_contrast(&ColorU::BLACK, 4.5), gray);
    }

    #[test]
    fn ensure_contrast_actually_reaches_the_requested_ratio() {
        let gray = ColorU::rgb(128, 128, 128);
        let fixed = gray.ensure_contrast(&ColorU::WHITE, 4.5);
        assert!(
            fixed.contrast_ratio(&ColorU::WHITE) >= 4.5,
            "adjusted color {fixed:?} still fails 4.5:1"
        );
        // Darkening against a light background must not make it lighter.
        assert!(fixed.r <= gray.r && fixed.g <= gray.g && fixed.b <= gray.b);
    }

    #[test]
    fn ensure_contrast_degenerate_min_ratios_return_self() {
        let gray = ColorU::rgb(128, 128, 128);
        // <= current ratio: early return.
        assert_eq!(gray.ensure_contrast(&ColorU::WHITE, 0.0), gray);
        assert_eq!(gray.ensure_contrast(&ColorU::WHITE, -1.0), gray);
        assert_eq!(gray.ensure_contrast(&ColorU::WHITE, f32::NEG_INFINITY), gray);
        // Unsatisfiable / NaN: every comparison is false, so `result` never
        // moves off `*self`. Terminates (fixed 16 iterations), never hangs.
        assert_eq!(gray.ensure_contrast(&ColorU::WHITE, f32::INFINITY), gray);
        assert_eq!(gray.ensure_contrast(&ColorU::WHITE, f32::NAN), gray);
        assert_eq!(gray.ensure_contrast(&ColorU::WHITE, 1e30), gray);
    }

    #[test]
    fn ensure_contrast_terminates_for_every_sample_pair() {
        for c in SAMPLES {
            for bg in SAMPLES {
                for min in [1.0, 3.0, 4.5, 7.0, 21.0, 25.0] {
                    let out = c.ensure_contrast(&bg, min);
                    // Alpha is carried through lighten/darken untouched.
                    assert_eq!(out.a, c.a);
                }
            }
        }
    }

    // =====================================================================
    // APCA
    // =====================================================================

    #[test]
    fn apca_contrast_sign_encodes_polarity() {
        let dark_on_light = ColorU::BLACK.apca_contrast(&ColorU::WHITE);
        let light_on_dark = ColorU::WHITE.apca_contrast(&ColorU::BLACK);
        assert!(dark_on_light > 0.0, "black-on-white should be positive");
        assert!(light_on_dark < 0.0, "white-on-black should be negative");
        assert!(dark_on_light.is_finite() && light_on_dark.is_finite());
    }

    #[test]
    fn apca_contrast_of_a_color_against_itself_is_zero() {
        for c in SAMPLES {
            assert_eq!(c.apca_contrast(&c), 0.0, "{c:?} vs itself");
        }
    }

    #[test]
    fn apca_contrast_is_finite_for_every_sample_pair() {
        for a in SAMPLES {
            for b in SAMPLES {
                assert!(a.apca_contrast(&b).is_finite(), "{a:?} on {b:?}");
            }
        }
    }

    #[test]
    fn meets_apca_thresholds_agree_with_apca_contrast() {
        for a in SAMPLES {
            for b in SAMPLES {
                let lc = libm::fabsf(a.apca_contrast(&b));
                assert_eq!(a.meets_apca_body(&b), lc >= 60.0);
                assert_eq!(a.meets_apca_large(&b), lc >= 45.0);
            }
        }
        assert!(ColorU::BLACK.meets_apca_body(&ColorU::WHITE));
        assert!(ColorU::BLACK.meets_apca_large(&ColorU::WHITE));
        assert!(!ColorU::RED.meets_apca_body(&ColorU::RED));
        assert!(!ColorU::RED.meets_apca_large(&ColorU::RED));
    }

    // =====================================================================
    // getters / predicates: hover_variant, active_variant, invert,
    //                       to_grayscale, has_alpha, to_hash
    // =====================================================================

    #[test]
    fn hover_and_active_variants_preserve_alpha_and_never_panic() {
        for c in SAMPLES {
            assert_eq!(c.hover_variant().a, c.a);
            assert_eq!(c.active_variant().a, c.a);
        }
        // Light colors get darker, dark colors get lighter.
        assert!(ColorU::WHITE.hover_variant().r < 255);
        assert!(ColorU::BLACK.hover_variant().r > 0);
        assert!(ColorU::WHITE.active_variant().r < ColorU::WHITE.hover_variant().r);
    }

    #[test]
    fn invert_is_its_own_inverse() {
        for c in SAMPLES {
            assert_eq!(c.invert().invert(), c);
            assert_eq!(c.invert().a, c.a, "invert must keep alpha");
        }
        assert_eq!(ColorU::BLACK.invert(), ColorU::WHITE);
        assert_eq!(ColorU::WHITE.invert(), ColorU::BLACK);
    }

    #[test]
    fn invert_does_not_underflow_at_the_channel_bounds() {
        // `255 - self.r` on u8 would panic in debug on underflow; it cannot,
        // but pin the boundary values anyway.
        assert_eq!(ColorU::rgba(0, 0, 0, 0).invert(), ColorU::rgba(255, 255, 255, 0));
        assert_eq!(
            ColorU::rgba(255, 255, 255, 255).invert(),
            ColorU::rgba(0, 0, 0, 255)
        );
    }

    #[test]
    fn to_grayscale_produces_equal_channels_and_keeps_alpha() {
        for c in SAMPLES {
            let g = c.to_grayscale();
            assert_eq!(g.r, g.g);
            assert_eq!(g.g, g.b);
            assert_eq!(g.a, c.a);
        }
    }

    #[test]
    fn to_grayscale_boundary_values() {
        assert_eq!(ColorU::BLACK.to_grayscale(), ColorU::BLACK);
        assert_eq!(ColorU::WHITE.to_grayscale(), ColorU::WHITE);
        assert_eq!(
            ColorU::rgb(128, 128, 128).to_grayscale(),
            ColorU::rgb(128, 128, 128)
        );
        // An already-gray color is (near enough) a fixed point of to_grayscale:
        // the BT.601 weights sum to 1.0, so only the truncating cast can shave
        // off at most one level.
        for i in 0..=255u16 {
            #[allow(clippy::cast_possible_truncation)]
            let c = ColorU::rgb(i as u8, i as u8, i as u8);
            let drift = i32::from(c.r) - i32::from(c.to_grayscale().r);
            assert!((0..=1).contains(&drift), "gray {i} drifted by {drift}");
        }
    }

    #[test]
    fn has_alpha_is_true_for_everything_but_255() {
        assert!(!ColorU::rgba(0, 0, 0, 255).has_alpha());
        assert!(!ColorU::WHITE.has_alpha());
        assert!(ColorU::rgba(0, 0, 0, 254).has_alpha());
        assert!(ColorU::TRANSPARENT.has_alpha());
        for a in 0..=u8::MAX {
            assert_eq!(ColorU::rgba(1, 2, 3, a).has_alpha(), a != 255);
        }
    }

    #[test]
    fn to_hash_is_always_nine_lowercase_chars() {
        assert_eq!(ColorU::RED.to_hash(), "#ff0000ff");
        assert_eq!(ColorU::TRANSPARENT.to_hash(), "#00000000");
        assert_eq!(ColorU::rgba(1, 2, 3, 4).to_hash(), "#01020304");
        assert_eq!(ColorU::WHITE.to_hash(), "#ffffffff");
        for c in SAMPLES {
            let h = c.to_hash();
            assert_eq!(h.len(), 9, "{h} is not 9 bytes");
            assert!(h.starts_with('#'));
            assert!(
                h[1..].chars().all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase()),
                "{h} is not lowercase hex"
            );
        }
    }

    // =====================================================================
    // serializer: Display for ColorU / ColorF
    // =====================================================================

    #[test]
    fn coloru_display_is_well_formed() {
        assert_eq!(format!("{}", ColorU::RED), "rgba(255, 0, 0, 1)");
        assert_eq!(format!("{}", ColorU::TRANSPARENT), "rgba(0, 0, 0, 0)");
        assert_eq!(format!("{}", ColorU::default()), "rgba(0, 0, 0, 1)");
        // Alpha is normalized to 0.0..=1.0.
        assert_eq!(format!("{}", ColorU::rgba(1, 2, 3, 128)), "rgba(1, 2, 3, 0.5019608)");
        for c in SAMPLES {
            let s = format!("{c}");
            assert!(s.starts_with("rgba(") && s.ends_with(')') && s.len() > 6);
        }
    }

    #[test]
    fn colorf_display_survives_nan_and_inf() {
        assert_eq!(format!("{}", ColorF::BLACK), "rgba(0, 0, 0, 1)");
        assert_eq!(format!("{}", ColorF::WHITE), "rgba(255, 255, 255, 1)");
        assert_eq!(format!("{}", ColorF::TRANSPARENT), "rgba(0, 0, 0, 0)");
        assert_eq!(format!("{}", ColorF::default()), format!("{}", ColorF::BLACK));

        let nan = ColorF { r: f32::NAN, g: f32::NAN, b: f32::NAN, a: f32::NAN };
        assert_eq!(format!("{nan}"), "rgba(NaN, NaN, NaN, NaN)");

        let inf = ColorF {
            r: f32::INFINITY,
            g: f32::NEG_INFINITY,
            b: f32::MAX,
            a: f32::INFINITY,
        };
        let s = format!("{inf}");
        assert!(s.starts_with("rgba(inf, -inf, ") && s.ends_with(", inf)"), "{s}");
    }

    // =====================================================================
    // round-trip: ColorU <-> ColorF, to_hash -> parse, Display -> parse
    // =====================================================================

    #[test]
    fn coloru_to_colorf_and_back_is_lossless_for_all_256_channel_values() {
        for i in 0..=255u16 {
            #[allow(clippy::cast_possible_truncation)]
            let c = ColorU::rgba(i as u8, (255 - i) as u8, i as u8, (255 - i) as u8);
            let f: ColorF = c.into();
            let back: ColorU = f.into();
            assert_eq!(back, c, "round-trip lost information at {i}");
        }
    }

    #[test]
    fn colorf_to_coloru_clamps_out_of_range_channels() {
        // > 1.0 is clamped by `.min(1.0)`.
        let over = ColorF { r: 2.0, g: 1e30, b: f32::INFINITY, a: 1.5 };
        assert_eq!(ColorU::from(over), ColorU::rgba(255, 255, 255, 255));
        // < 0.0 is NOT clamped by `.min`, but `as u8` saturates it to 0 anyway.
        let under = ColorF { r: -1.0, g: -1e30, b: f32::NEG_INFINITY, a: -0.5 };
        assert_eq!(ColorU::from(under), ColorU::rgba(0, 0, 0, 0));
    }

    #[test]
    fn colorf_to_coloru_maps_nan_channels_to_255() {
        // `f32::min` returns the NON-NaN operand, so `NaN.min(1.0) == 1.0`,
        // and a NaN channel comes out fully saturated rather than 0.
        let nan = ColorF { r: f32::NAN, g: 0.0, b: 0.0, a: f32::NAN };
        assert_eq!(ColorU::from(nan), ColorU::rgba(255, 0, 0, 255));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn to_hash_round_trips_through_the_parser() {
        assert_eq!(parse_css_color(&ColorU::RED.to_hash()).unwrap(), ColorU::RED);
        for r in (0..=255u16).step_by(51) {
            for g in (0..=255u16).step_by(51) {
                for b in (0..=255u16).step_by(85) {
                    for a in (0..=255u16).step_by(85) {
                        #[allow(clippy::cast_possible_truncation)]
                        let c = ColorU::rgba(r as u8, g as u8, b as u8, a as u8);
                        let encoded = c.to_hash();
                        let decoded = parse_css_color(&encoded)
                            .unwrap_or_else(|e| panic!("{encoded} failed to parse: {e}"));
                        assert_eq!(decoded, c, "{encoded} decoded to the wrong color");
                    }
                }
            }
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn coloru_display_round_trips_through_the_parser() {
        // Display emits `rgba(r, g, b, a/255)`, which parse_css_color accepts.
        for a in 0..=255u16 {
            #[allow(clippy::cast_possible_truncation)]
            let c = ColorU::rgba(13, 110, 253, a as u8);
            let encoded = format!("{c}");
            let decoded = parse_css_color(&encoded)
                .unwrap_or_else(|e| panic!("{encoded} failed to parse: {e}"));
            assert_eq!(decoded, c, "{encoded} decoded to the wrong color");
        }
        for c in SAMPLES {
            assert_eq!(parse_css_color(&format!("{c}")).unwrap(), c);
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn system_color_ref_css_str_round_trips_for_every_variant() {
        let all = [
            SystemColorRef::Text,
            SystemColorRef::Background,
            SystemColorRef::Accent,
            SystemColorRef::AccentText,
            SystemColorRef::ButtonFace,
            SystemColorRef::ButtonText,
            SystemColorRef::WindowBackground,
            SystemColorRef::SelectionBackground,
            SystemColorRef::SelectionText,
        ];
        for variant in all {
            let encoded = variant.as_css_str();
            assert!(encoded.starts_with("system:"), "{encoded}");
            assert_eq!(
                parse_color_or_system(encoded).unwrap(),
                ColorOrSystem::System(variant),
                "{encoded} did not round-trip"
            );
        }
    }

    // =====================================================================
    // ColorOrSystem / SystemColorRef
    // =====================================================================

    #[test]
    fn color_or_system_constructors_and_fallbacks() {
        let c = ColorOrSystem::color(ColorU::RED);
        assert_eq!(c, ColorOrSystem::Color(ColorU::RED));
        assert_eq!(c.to_color_u_with_fallback(ColorU::BLUE), ColorU::RED);
        assert_eq!(c.to_color_u_default(), ColorU::RED);

        let s = ColorOrSystem::system(SystemColorRef::Accent);
        assert_eq!(s, ColorOrSystem::System(SystemColorRef::Accent));
        // A system ref has no concrete value, so the fallback wins.
        assert_eq!(s.to_color_u_with_fallback(ColorU::BLUE), ColorU::BLUE);
        assert_eq!(s.to_color_u_default(), ColorU::rgba(128, 128, 128, 255));

        // Default is opaque black, and From<ColorU> agrees with ::color().
        assert_eq!(ColorOrSystem::default(), ColorOrSystem::Color(ColorU::BLACK));
        assert_eq!(ColorOrSystem::from(ColorU::RED), ColorOrSystem::color(ColorU::RED));
    }

    #[test]
    fn system_color_ref_resolve_falls_back_when_unset() {
        use crate::system::SystemColors;

        let empty = SystemColors::default();
        let all = [
            SystemColorRef::Text,
            SystemColorRef::Background,
            SystemColorRef::Accent,
            SystemColorRef::AccentText,
            SystemColorRef::ButtonFace,
            SystemColorRef::ButtonText,
            SystemColorRef::WindowBackground,
            SystemColorRef::SelectionBackground,
            SystemColorRef::SelectionText,
        ];
        for variant in all {
            assert_eq!(variant.resolve(&empty, ColorU::RED), ColorU::RED, "{variant:?}");
            assert_eq!(
                ColorOrSystem::System(variant).resolve(&empty, ColorU::RED),
                ColorU::RED
            );
        }
        // A concrete color ignores both the SystemColors and the fallback.
        assert_eq!(
            ColorOrSystem::Color(ColorU::BLUE).resolve(&empty, ColorU::RED),
            ColorU::BLUE
        );
    }

    // =====================================================================
    // palettes: shade is a `usize`, so every value must land somewhere
    // =====================================================================

    #[test]
    fn palette_shades_are_total_over_usize_and_always_opaque() {
        type Palette = fn(usize) -> ColorU;
        const PALETTES: [Palette; 12] = [
            ColorU::strawberry,
            ColorU::palette_orange,
            ColorU::banana,
            ColorU::palette_lime,
            ColorU::mint,
            ColorU::blueberry,
            ColorU::grape,
            ColorU::bubblegum,
            ColorU::cocoa,
            ColorU::palette_silver,
            ColorU::slate,
            ColorU::dark,
        ];
        for p in PALETTES {
            for shade in [0, 1, 100, 200, 201, 300, 400, 401, 500, 600, 601, 700, 800, 801, 900, 1000, usize::MAX] {
                assert_eq!(p(shade).a, 255, "shade {shade} was not opaque");
            }
            // Every out-of-band shade collapses into the 900 bucket.
            assert_eq!(p(usize::MAX), p(900));
            assert_eq!(p(801), p(900));
            // The documented buckets are distinct at their boundaries.
            assert_eq!(p(0), p(200));
            assert_ne!(p(200), p(201));
            assert_ne!(p(400), p(401));
            assert_ne!(p(600), p(601));
            assert_ne!(p(800), p(801));
        }
    }

    #[test]
    fn palette_known_values() {
        assert_eq!(ColorU::strawberry(100), ColorU::rgb(0xff, 0x8c, 0x82));
        assert_eq!(ColorU::strawberry(900), ColorU::rgb(0x7a, 0x00, 0x00));
        assert_eq!(ColorU::dark(900), ColorU::BLACK);
        assert_eq!(ColorU::dark(usize::MAX), ColorU::BLACK);
    }

    // =====================================================================
    // named / themed constructors: every one must be a valid opaque color
    // =====================================================================

    #[test]
    fn named_constructors_match_their_constants() {
        assert_eq!(ColorU::red(), ColorU::RED);
        assert_eq!(ColorU::green(), ColorU::GREEN);
        assert_eq!(ColorU::blue(), ColorU::BLUE);
        assert_eq!(ColorU::white(), ColorU::WHITE);
        assert_eq!(ColorU::black(), ColorU::BLACK);
        assert_eq!(ColorU::transparent(), ColorU::TRANSPARENT);
        assert_eq!(ColorU::yellow(), ColorU::YELLOW);
        assert_eq!(ColorU::cyan(), ColorU::CYAN);
        assert_eq!(ColorU::magenta(), ColorU::MAGENTA);
        assert_eq!(ColorU::orange(), ColorU::ORANGE);
        assert_eq!(ColorU::pink(), ColorU::PINK);
        assert_eq!(ColorU::purple(), ColorU::PURPLE);
        assert_eq!(ColorU::brown(), ColorU::BROWN);
        assert_eq!(ColorU::gray(), ColorU::GRAY);
        assert_eq!(ColorU::light_gray(), ColorU::LIGHT_GRAY);
        assert_eq!(ColorU::dark_gray(), ColorU::DARK_GRAY);
        assert_eq!(ColorU::navy(), ColorU::NAVY);
        assert_eq!(ColorU::teal(), ColorU::TEAL);
        assert_eq!(ColorU::olive(), ColorU::OLIVE);
        assert_eq!(ColorU::maroon(), ColorU::MAROON);
        assert_eq!(ColorU::lime(), ColorU::LIME);
        assert_eq!(ColorU::aqua(), ColorU::AQUA);
        assert_eq!(ColorU::silver(), ColorU::SILVER);
        assert_eq!(ColorU::fuchsia(), ColorU::FUCHSIA);
        assert_eq!(ColorU::indigo(), ColorU::INDIGO);
        assert_eq!(ColorU::gold(), ColorU::GOLD);
        assert_eq!(ColorU::coral(), ColorU::CORAL);
        assert_eq!(ColorU::salmon(), ColorU::SALMON);
        assert_eq!(ColorU::turquoise(), ColorU::TURQUOISE);
        assert_eq!(ColorU::violet(), ColorU::VIOLET);
        assert_eq!(ColorU::crimson(), ColorU::CRIMSON);
        assert_eq!(ColorU::chocolate(), ColorU::CHOCOLATE);
        assert_eq!(ColorU::sky_blue(), ColorU::SKY_BLUE);
        assert_eq!(ColorU::forest_green(), ColorU::FOREST_GREEN);
        assert_eq!(ColorU::sea_green(), ColorU::SEA_GREEN);
        assert_eq!(ColorU::slate_gray(), ColorU::SLATE_GRAY);
        assert_eq!(ColorU::midnight_blue(), ColorU::MIDNIGHT_BLUE);
        assert_eq!(ColorU::dark_red(), ColorU::DARK_RED);
        assert_eq!(ColorU::dark_green(), ColorU::DARK_GREEN);
        assert_eq!(ColorU::dark_blue(), ColorU::DARK_BLUE);
        assert_eq!(ColorU::light_blue(), ColorU::LIGHT_BLUE);
        assert_eq!(ColorU::light_green(), ColorU::LIGHT_GREEN);
        assert_eq!(ColorU::light_yellow(), ColorU::LIGHT_YELLOW);
        assert_eq!(ColorU::light_pink(), ColorU::LIGHT_PINK);
    }

    #[test]
    fn every_named_constructor_except_transparent_is_opaque() {
        type Ctor = fn() -> ColorU;
        const CTORS: [Ctor; 43] = [
            ColorU::red, ColorU::green, ColorU::blue, ColorU::white, ColorU::black,
            ColorU::yellow, ColorU::cyan, ColorU::magenta, ColorU::orange, ColorU::pink,
            ColorU::purple, ColorU::brown, ColorU::gray, ColorU::light_gray, ColorU::dark_gray,
            ColorU::navy, ColorU::teal, ColorU::olive, ColorU::maroon, ColorU::lime,
            ColorU::aqua, ColorU::silver, ColorU::fuchsia, ColorU::indigo, ColorU::gold,
            ColorU::coral, ColorU::salmon, ColorU::turquoise, ColorU::violet, ColorU::crimson,
            ColorU::chocolate, ColorU::sky_blue, ColorU::forest_green, ColorU::sea_green,
            ColorU::slate_gray, ColorU::midnight_blue, ColorU::dark_red, ColorU::dark_green,
            ColorU::dark_blue, ColorU::light_blue, ColorU::light_green, ColorU::light_yellow,
            ColorU::light_pink,
        ];
        for ctor in CTORS {
            let c = ctor();
            assert_eq!(c.a, ColorU::ALPHA_OPAQUE);
            assert!(!c.has_alpha());
        }
        // The one exception.
        assert_eq!(ColorU::transparent().a, ColorU::ALPHA_TRANSPARENT);
        assert!(ColorU::transparent().has_alpha());
    }

    #[test]
    fn apple_and_bootstrap_palettes_are_opaque_and_distinct() {
        type Ctor = fn() -> ColorU;
        const APPLE: [Ctor; 26] = [
            ColorU::apple_red, ColorU::apple_red_dark,
            ColorU::apple_orange, ColorU::apple_orange_dark,
            ColorU::apple_yellow, ColorU::apple_yellow_dark,
            ColorU::apple_green, ColorU::apple_green_dark,
            ColorU::apple_mint, ColorU::apple_mint_dark,
            ColorU::apple_teal, ColorU::apple_teal_dark,
            ColorU::apple_cyan, ColorU::apple_cyan_dark,
            ColorU::apple_blue, ColorU::apple_blue_dark,
            ColorU::apple_indigo, ColorU::apple_indigo_dark,
            ColorU::apple_purple, ColorU::apple_purple_dark,
            ColorU::apple_pink, ColorU::apple_pink_dark,
            ColorU::apple_brown, ColorU::apple_brown_dark,
            ColorU::apple_gray, ColorU::apple_gray_dark,
        ];
        const BOOTSTRAP: [Ctor; 23] = [
            ColorU::bootstrap_primary, ColorU::bootstrap_primary_hover, ColorU::bootstrap_primary_active,
            ColorU::bootstrap_secondary, ColorU::bootstrap_secondary_hover, ColorU::bootstrap_secondary_active,
            ColorU::bootstrap_success, ColorU::bootstrap_success_hover, ColorU::bootstrap_success_active,
            ColorU::bootstrap_danger, ColorU::bootstrap_danger_hover, ColorU::bootstrap_danger_active,
            ColorU::bootstrap_warning, ColorU::bootstrap_warning_hover, ColorU::bootstrap_warning_active,
            ColorU::bootstrap_info, ColorU::bootstrap_info_hover, ColorU::bootstrap_info_active,
            ColorU::bootstrap_light, ColorU::bootstrap_light_hover, ColorU::bootstrap_light_active,
            ColorU::bootstrap_dark, ColorU::bootstrap_dark_hover,
        ];
        for ctor in APPLE.iter().chain(BOOTSTRAP.iter()) {
            assert_eq!(ctor().a, 255);
        }
        // Each light/dark pair must actually differ.
        for pair in APPLE.chunks_exact(2) {
            assert_ne!(pair[0](), pair[1](), "an apple light/dark pair is identical");
        }
        // bootstrap_link duplicates bootstrap_primary by design; check the hover shifts.
        assert_eq!(ColorU::bootstrap_link(), ColorU::bootstrap_primary());
        assert_ne!(ColorU::bootstrap_link_hover(), ColorU::bootstrap_link());
        assert_ne!(ColorU::bootstrap_dark_active(), ColorU::bootstrap_dark());
    }

    // =====================================================================
    // parser: parse_css_color — malformed / huge / boundary / unicode
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_valid_minimal_positive_controls() {
        assert_eq!(parse_css_color("red").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("#f00").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("#ff0000").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("rgb(255,0,0)").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("hsl(0,100%,50%)").unwrap(), ColorU::RED);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_empty_and_whitespace_only_are_errors() {
        assert!(parse_css_color("").is_err());
        assert!(parse_css_color("   ").is_err());
        assert!(parse_css_color("\t\n\r ").is_err());
        assert!(parse_css_color("#").is_err());
        assert_eq!(parse_css_color(""), Err(CssColorParseError::EmptyInput));
        assert_eq!(parse_css_color("  \t "), Err(CssColorParseError::EmptyInput));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_garbage_is_rejected_without_panicking() {
        for garbage in [
            "!@#$%^&*()", "\0\0\0", "rgb", "rgb(", "rgb)", ")(", "()", "#-1", "#+1",
            "notacolor", "0", "-0", "1e10", "NaN", "inf", "-inf", ";", ",,,", "\\",
            "rgb(,,)", "hsl(,,)", "rgba(,,,)", "#\u{0}\u{0}\u{0}",
        ] {
            assert!(
                parse_css_color(garbage).is_err(),
                "{garbage:?} was unexpectedly accepted"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_extremely_long_input_does_not_hang_or_panic() {
        // Hex path: rejected on the length check alone.
        let long_hex = format!("#{}", "f".repeat(1_000_000));
        assert!(parse_css_color(&long_hex).is_err());
        // Named-color path: lowercases 100k bytes, then fails the match.
        let long_name = "a".repeat(100_000);
        assert!(parse_css_color(&long_name).is_err());
        // Function path with a huge component list.
        let long_rgb = format!("rgb({})", "1,".repeat(50_000));
        assert!(parse_css_color(&long_rgb).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_deeply_nested_input_does_not_stack_overflow() {
        // The parser is iterative, not recursive — these must simply be errors.
        let nested_parens = "(".repeat(10_000);
        assert!(parse_css_color(&nested_parens).is_err());
        let unclosed = "rgb(".repeat(10_000);
        assert!(parse_css_color(&unclosed).is_err());
        let balanced = format!("{}{}", "rgb(".repeat(5_000), ")".repeat(5_000));
        assert!(parse_css_color(&balanced).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_unicode_input_does_not_panic() {
        // The 3/4-byte hex branches read raw bytes, so a multi-byte char must
        // fail cleanly rather than slice through a char boundary.
        for input in [
            "\u{1F600}",        // emoji
            "#\u{1F600}",       // 4 bytes after '#' -> hits the len==4 branch
            "#\u{e9}1",         // 3 bytes after '#' -> hits the len==3 branch
            "#\u{e9}\u{e9}\u{e9}", // 6 bytes -> hits the from_str_radix branch
            "r\u{e9}d",
            "\u{0301}\u{0301}",  // bare combining marks
            "\u{4e2d}\u{6587}",  // CJK
            "rgb(\u{1F600},0,0)",
            "rgba(0,0,0,\u{1F600})",
            "hsl(\u{1F600},100%,50%)",
        ] {
            assert!(
                parse_css_color(input).is_err(),
                "{input:?} was unexpectedly accepted"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_boundary_numbers() {
        // rgb components are u8: 0 and 255 in, 256 and -1 out.
        assert_eq!(parse_css_color("rgb(0,0,0)").unwrap(), ColorU::BLACK);
        assert_eq!(parse_css_color("rgb(255,255,255)").unwrap(), ColorU::WHITE);
        assert!(parse_css_color("rgb(256,0,0)").is_err());
        assert!(parse_css_color("rgb(-1,0,0)").is_err());
        assert!(parse_css_color("rgb(9223372036854775807,0,0)").is_err());
        assert!(parse_css_color("rgb(340282350000000000000000000000000000000,0,0)").is_err());
        // Alpha is a float clamped to 0.0..=1.0 (inclusive), out-of-range rejected.
        assert_eq!(parse_css_color("rgba(0,0,0,0)").unwrap().a, 0);
        assert_eq!(parse_css_color("rgba(0,0,0,1)").unwrap().a, 255);
        assert_eq!(parse_css_color("rgba(0,0,0,1.0)").unwrap().a, 255);
        assert_eq!(parse_css_color("rgba(0,0,0,-0)").unwrap().a, 0);
        assert!(parse_css_color("rgba(0,0,0,1.0001)").is_err());
        assert!(parse_css_color("rgba(0,0,0,-0.0001)").is_err());
        assert!(parse_css_color("rgba(0,0,0,2)").is_err());
        // NaN / inf are valid f32 literals to FromStr, but must fail the range check.
        assert!(parse_css_color("rgba(0,0,0,NaN)").is_err());
        assert!(parse_css_color("rgba(0,0,0,nan)").is_err());
        assert!(parse_css_color("rgba(0,0,0,inf)").is_err());
        assert!(parse_css_color("rgba(0,0,0,-inf)").is_err());
        assert!(parse_css_color("rgba(0,0,0,infinity)").is_err());
        // Subnormals round down to a fully transparent alpha rather than panicking.
        assert_eq!(parse_css_color("rgba(0,0,0,1e-45)").unwrap().a, 0);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_alpha_rounds_to_nearest() {
        // `(a * 255.0).round()` — half rounds away from zero.
        assert_eq!(parse_css_color("rgba(0,0,0,0.5)").unwrap().a, 128);
        assert_eq!(parse_css_color("rgba(0,0,0,0.0)").unwrap().a, 0);
        assert_eq!(parse_css_color("rgba(0,0,0,0.999)").unwrap().a, 255);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_arity_errors() {
        assert!(parse_css_color("rgb(255,0)").is_err());       // missing blue
        assert!(parse_css_color("rgb(255)").is_err());         // missing green
        assert!(parse_css_color("rgb()").is_err());            // missing everything
        assert!(parse_css_color("rgb(0,0,0,0)").is_err());     // extra arg to rgb()
        assert!(parse_css_color("rgba(0,0,0)").is_err());      // missing alpha
        assert!(parse_css_color("rgba(0,0,0,1,1)").is_err());  // extra arg to rgba()
        assert!(parse_css_color("hsl(0,100%)").is_err());      // missing lightness
        assert!(parse_css_color("hsla(0,100%,50%)").is_err()); // missing alpha
        // This implementation requires commas; space-separated CSS4 syntax is not supported.
        assert!(parse_css_color("rgb(255 0 0)").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_leading_and_trailing_whitespace_is_trimmed() {
        assert_eq!(parse_css_color("  red  ").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("\t#f00\n").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("  rgb( 255 , 0 , 0 )  ").unwrap(), ColorU::RED);
        // Trailing junk after a bare keyword IS rejected.
        assert!(parse_css_color("red;garbage").is_err());
        assert!(parse_css_color("red red").is_err());
        assert!(parse_css_color("#f00;").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_accepts_trailing_junk_after_a_function_call() {
        // KNOWN DEVIATION (pinned, not endorsed): parse_parentheses slices between
        // the FIRST '(' and the LAST ')', so anything after the closing paren is
        // silently dropped instead of being rejected as an error. See report.
        assert_eq!(parse_css_color("rgb(1,2,3)garbage").unwrap(), ColorU::rgb(1, 2, 3));
        assert_eq!(parse_css_color("rgb(1,2,3);").unwrap(), ColorU::rgb(1, 2, 3));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_hex_is_case_insensitive_and_length_checked() {
        assert_eq!(parse_css_color("#ABCDEF").unwrap(), parse_css_color("#abcdef").unwrap());
        assert_eq!(parse_css_color("#FFF").unwrap(), ColorU::WHITE);
        // 3/4-digit shorthand expands by *17 (f -> 0xff).
        assert_eq!(parse_css_color("#f00f").unwrap(), ColorU::rgba(255, 0, 0, 255));
        assert_eq!(parse_css_color("#0008").unwrap(), ColorU::rgba(0, 0, 0, 136));
        // Only lengths 3, 4, 6 and 8 are legal.
        for bad_len in ["#", "#f", "#ff", "#fffff", "#fffffff", "#fffffffff"] {
            assert!(parse_css_color(bad_len).is_err(), "{bad_len} accepted");
        }
        // Non-hex digits.
        assert!(parse_css_color("#ggg").is_err());
        assert!(parse_css_color("#gggggg").is_err());
        assert!(parse_css_color("#-12345").is_err());
        assert!(parse_css_color("#+f0000").is_err());
        assert!(parse_css_color("#ff ff").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_builtin_names_are_case_insensitive() {
        assert_eq!(parse_css_color("RED").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("ReD").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("TRANSPARENT").unwrap(), ColorU::TRANSPARENT);
        assert_eq!(parse_css_color("transparent").unwrap().a, 0);
        // Near-miss names are rejected, not fuzzy-matched.
        for near_miss in ["redd", "re", "r ed", "red1", "gray2", "greyish", "blackk"] {
            assert!(parse_css_color(near_miss).is_err(), "{near_miss} accepted");
        }
        // ...but surrounding whitespace really is just trimmed.
        assert_eq!(parse_css_color(" grey ").unwrap(), ColorU::GRAY);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_hsl_boundaries_and_hue_wraparound() {
        assert_eq!(parse_css_color("hsl(0,100%,50%)").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("hsl(120,100%,50%)").unwrap(), ColorU::GREEN);
        assert_eq!(parse_css_color("hsl(240,100%,50%)").unwrap(), ColorU::BLUE);
        // A full extra turn lands back on red.
        assert_eq!(parse_css_color("hsl(720,100%,50%)").unwrap(), ColorU::RED);
        // Achromatic ends.
        assert_eq!(parse_css_color("hsl(0,0%,0%)").unwrap(), ColorU::BLACK);
        assert_eq!(parse_css_color("hsl(0,0%,100%)").unwrap(), ColorU::WHITE);
        // Huge but finite hues must not panic.
        for hue in ["1000000", "99999999", "-360"] {
            let s = format!("hsl({hue},100%,50%)");
            let _ = parse_css_color(&s).map(|c| assert_eq!(c.a, 255));
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_css_color_unitless_hsl_components_are_scaled_wrong() {
        // KNOWN DEVIATION (pinned, not endorsed): `parse_percentage_value` turns a
        // unitless value into `value * 100` percent, and `percent_from_str` then
        // re-normalizes it, so a unitless component is a 0..1 FRACTION rather than
        // the CSS Color 4 "number of percent". `hsl(0 100 50)` — plain red in every
        // browser — comes out CYAN here. Pinned so a fix shows up as a diff.
        assert_eq!(parse_css_color("hsl(0,100,50)").unwrap(), ColorU::rgb(0, 255, 255));
        // The fraction spelling is what currently means "100% / 50%".
        assert_eq!(parse_css_color("hsl(0,1,0.5)").unwrap(), ColorU::RED);
        // The mixed spelling that the existing suite smoke-tests happens to land on
        // red by coincidence (the out-of-range saturation clips back into gamut).
        assert_eq!(parse_css_color("hsl(0,100,50%)").unwrap(), ColorU::RED);
    }

    // =====================================================================
    // parser: private helpers
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_no_hash_only_accepts_3_4_6_and_8_bytes() {
        assert_eq!(parse_color_no_hash("fff").unwrap(), ColorU::WHITE);
        assert_eq!(parse_color_no_hash("000f").unwrap(), ColorU::BLACK);
        assert_eq!(parse_color_no_hash("ff0000").unwrap(), ColorU::RED);
        assert_eq!(parse_color_no_hash("ff000080").unwrap(), ColorU::rgba(255, 0, 0, 128));
        for bad in ["", "f", "ff", "fffff", "fffffff", "fffffffff", "   ", "zzz"] {
            assert!(parse_color_no_hash(bad).is_err(), "{bad:?} accepted");
        }
        // `input.len()` is a BYTE length, so a 3-byte multi-byte string reaches
        // the byte-reading branch and must error, not slice through a char.
        assert!(parse_color_no_hash("\u{e9}1").is_err());
        assert!(parse_color_no_hash("\u{1F600}").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_rgb_alpha_flag_controls_arity() {
        assert_eq!(parse_color_rgb("1,2,3", false).unwrap(), ColorU::rgb(1, 2, 3));
        assert_eq!(parse_color_rgb("1,2,3,1", true).unwrap(), ColorU::rgba(1, 2, 3, 255));
        // parse_alpha=true but no alpha given.
        assert!(parse_color_rgb("1,2,3", true).is_err());
        // parse_alpha=false but an alpha given.
        assert!(parse_color_rgb("1,2,3,1", false).is_err());
        // Empty / whitespace components.
        assert!(parse_color_rgb("", false).is_err());
        assert!(parse_color_rgb("   ", false).is_err());
        assert!(parse_color_rgb(",,", false).is_err());
        assert!(parse_color_rgb("1,,3", false).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_rgb_components_boundaries() {
        let mut ok = ["0", "128", "255"].into_iter();
        assert_eq!(
            parse_color_rgb_components(&mut ok).unwrap(),
            ColorU::rgb(0, 128, 255)
        );
        // An empty iterator is a missing-component error, not a panic.
        let mut empty = core::iter::empty::<&str>();
        assert!(parse_color_rgb_components(&mut empty).is_err());
        // Too few components.
        let mut short = ["1", "2"].into_iter();
        assert!(parse_color_rgb_components(&mut short).is_err());
        // Overflow / underflow / garbage.
        for bad in [
            ["256", "0", "0"],
            ["-1", "0", "0"],
            ["0", "0", "1e3"],
            ["0.5", "0", "0"],
            ["abc", "0", "0"],
            ["", "0", "0"],
            ["+0", "0", "999999999999999999999"],
        ] {
            let mut it = bad.into_iter();
            assert!(parse_color_rgb_components(&mut it).is_err(), "{bad:?} accepted");
        }
        // Extra components past the third are simply not consumed here.
        let mut extra = ["1", "2", "3", "4", "5"].into_iter();
        assert_eq!(
            parse_color_rgb_components(&mut extra).unwrap(),
            ColorU::rgb(1, 2, 3)
        );
        assert_eq!(extra.next(), Some("4"));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_hsl_components_boundaries() {
        let mut red = ["0", "100%", "50%"].into_iter();
        assert_eq!(parse_color_hsl_components(&mut red).unwrap(), ColorU::RED);
        // KNOWN DEVIATION (pinned, not endorsed): a unitless component is read as
        // a 0.0..=1.0 FRACTION, not as a number of percent, so CSS Color 4's
        // `hsl(0 100 50)` saturation/lightness are scaled 100x too far. Only the
        // fraction spelling currently round-trips to red. See report.
        let mut fractions = ["0", "1", "0.5"].into_iter();
        assert_eq!(parse_color_hsl_components(&mut fractions).unwrap(), ColorU::RED);
        let mut unitless = ["0", "100", "50"].into_iter();
        assert_eq!(
            parse_color_hsl_components(&mut unitless).unwrap(),
            ColorU::rgb(0, 255, 255),
            "unitless hsl(0 100 50) should be red, not cyan"
        );
        // Missing components error rather than panic.
        let mut empty = core::iter::empty::<&str>();
        assert!(parse_color_hsl_components(&mut empty).is_err());
        let mut short = ["0", "100%"].into_iter();
        assert!(parse_color_hsl_components(&mut short).is_err());
        for bad in [
            ["", "100%", "50%"],
            ["notanangle", "100%", "50%"],
            ["to left", "100%", "50%"], // Direction::FromTo is unsupported for hue
            ["0", "", "50%"],
            ["0", "100%", ""],
        ] {
            let mut it = bad.into_iter();
            assert!(parse_color_hsl_components(&mut it).is_err(), "{bad:?} accepted");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_alpha_component_range_and_rounding() {
        let cases: [(&str, u8); 5] = [("0", 0), ("0.0", 0), ("0.5", 128), ("1", 255), ("1.0", 255)];
        for (input, expected) in cases {
            let mut it = [input].into_iter();
            assert_eq!(
                parse_alpha_component(&mut it).unwrap(),
                expected,
                "alpha {input}"
            );
        }
        // Out of range / unparseable / NaN / inf all produce Err, never a panic.
        for bad in ["", " ", "-0.0001", "1.0001", "2", "-1", "NaN", "inf", "-inf", "abc", "0,5", "50%"] {
            let mut it = [bad].into_iter();
            assert!(parse_alpha_component(&mut it).is_err(), "{bad:?} accepted");
        }
        // Missing entirely.
        let mut empty = core::iter::empty::<&str>();
        assert!(parse_alpha_component(&mut empty).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_builtin_rejects_junk_without_panicking() {
        assert_eq!(parse_color_builtin("red").unwrap(), ColorU::RED);
        assert_eq!(parse_color_builtin("REBECCAPURPLE").unwrap(), ColorU::rgb(102, 51, 153));
        assert_eq!(parse_color_builtin("transparent").unwrap(), ColorU::TRANSPARENT);
        // Not trimmed at this level — the caller is responsible for that.
        assert!(parse_color_builtin(" red").is_err());
        assert!(parse_color_builtin("").is_err());
        // to_lowercase() on exotic input must not panic (dotted capital I expands).
        assert!(parse_color_builtin("\u{130}").is_err());
        assert!(parse_color_builtin("\u{1F600}").is_err());
        assert!(parse_color_builtin(&"z".repeat(100_000)).is_err());
    }

    // =====================================================================
    // parser: parse_color_or_system
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_or_system_rejects_bad_system_names() {
        for bad in [
            "system:",
            "system:invalid",
            "system: ",
            "system::text",
            "system:text-",
            "system:TEXT",      // the variant table is case-SENSITIVE
            "SYSTEM:text",      // the prefix is case-SENSITIVE
            "system:text;junk",
            "system:\u{1F600}",
        ] {
            assert!(parse_color_or_system(bad).is_err(), "{bad:?} accepted");
        }
        // Empty / whitespace.
        assert!(parse_color_or_system("").is_err());
        assert!(parse_color_or_system("   ").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_or_system_trims_and_falls_through_to_colors() {
        assert_eq!(
            parse_color_or_system("  system:accent  ").unwrap(),
            ColorOrSystem::System(SystemColorRef::Accent)
        );
        // The name after the prefix is trimmed too.
        assert_eq!(
            parse_color_or_system("system: accent ").unwrap(),
            ColorOrSystem::System(SystemColorRef::Accent)
        );
        // Non-system input is delegated to parse_css_color.
        assert_eq!(
            parse_color_or_system("  #f00 ").unwrap(),
            ColorOrSystem::Color(ColorU::RED)
        );
        assert_eq!(
            parse_color_or_system("rgba(0,0,0,0)").unwrap(),
            ColorOrSystem::Color(ColorU::TRANSPARENT)
        );
        assert!(parse_color_or_system("definitely-not-a-color").is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_color_or_system_long_and_nested_input_does_not_hang() {
        let long = format!("system:{}", "a".repeat(100_000));
        assert!(parse_color_or_system(&long).is_err());
        let nested = "rgb(".repeat(10_000);
        assert!(parse_color_or_system(&nested).is_err());
    }

    // =====================================================================
    // error types: to_contained / to_shared round-trip
    // =====================================================================

    #[cfg(feature = "parser")]
    #[test]
    fn css_color_parse_error_round_trips_through_owned() {
        // One representative input per reachable error variant.
        let errors = [
            parse_css_color("notacolor").unwrap_err(),        // InvalidColor
            parse_css_color("foo(1,2)").unwrap_err(),          // InvalidFunctionName
            parse_css_color("#zzz").unwrap_err(),              // InvalidColorComponent
            parse_css_color("rgb(300,0,0)").unwrap_err(),      // IntValueParseErr
            parse_css_color("rgba(0,0,0,x)").unwrap_err(),     // FloatValueParseErr
            parse_css_color("rgba(0,0,0,2)").unwrap_err(),     // FloatValueOutOfRange
            parse_css_color("rgb(1,2)").unwrap_err(),          // MissingColorComponent
            parse_css_color("rgb(1,2,3,4)").unwrap_err(),      // ExtraArguments
            parse_css_color("rgb(1,2,3").unwrap_err(),         // UnclosedColor
            parse_css_color("").unwrap_err(),                  // EmptyInput
            parse_css_color("hsl(x,1%,1%)").unwrap_err(),      // DirectionParseError
            parse_css_color("hsl(0,x%,1%)").unwrap_err(),      // InvalidPercentage
        ];
        for e in &errors {
            let owned = e.to_contained();
            let shared = owned.to_shared();
            // Borrowed -> owned -> borrowed -> owned must be a fixed point.
            assert_eq!(shared.to_contained(), owned, "error did not round-trip: {e}");
            // Debug/Display must both produce something non-empty.
            assert!(!format!("{e}").is_empty());
            assert!(!format!("{e:?}").is_empty());
            assert!(!format!("{owned:?}").is_empty());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn css_color_parse_error_carries_the_offending_input() {
        assert_eq!(
            parse_css_color("notacolor"),
            Err(CssColorParseError::InvalidColor("notacolor"))
        );
        assert_eq!(
            parse_css_color("rgb(1,2,3,4)"),
            Err(CssColorParseError::ExtraArguments("4"))
        );
        assert_eq!(
            parse_css_color("rgb(1,2)"),
            Err(CssColorParseError::MissingColorComponent(
                CssColorComponent::Blue
            ))
        );
        assert_eq!(
            parse_css_color("rgba(1,2,3)"),
            Err(CssColorParseError::MissingColorComponent(
                CssColorComponent::Alpha
            ))
        );
        assert_eq!(
            parse_css_color("rgba(0,0,0,2)"),
            Err(CssColorParseError::FloatValueOutOfRange(2.0))
        );
        // The byte, not the char, is reported for a bad hex digit.
        assert_eq!(
            parse_css_color("#zzz"),
            Err(CssColorParseError::InvalidColorComponent(b'z'))
        );
    }
}

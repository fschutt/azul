//! # rust-fontconfig
//!
//! Pure-Rust rewrite of the Linux fontconfig library (no system dependencies). Enable the `parsing` feature to parse `.woff`, `.woff2`, `.ttc`, `.otf` and `.ttf` with allsorts.
//!
//! **NOTE**: Also works on Windows, macOS and WASM - without external dependencies!
//!
//! ## Usage
//!
//! ### Basic Font Query
//!
//! ```rust,no_run
//! use rust_fontconfig::{FcFontCache, FcPattern};
//!
//! fn main() {
//!     // Build the font cache
//!     let cache = FcFontCache::build();
//!
//!     // Query a font by name
//!     let results = cache.query(
//!         &FcPattern {
//!             name: Some(String::from("Arial")),
//!             ..Default::default()
//!         },
//!         &mut Vec::new() // Trace messages container
//!     );
//!
//!     if let Some(font_match) = results {
//!         println!("Font match ID: {:?}", font_match.id);
//!         println!("Font unicode ranges: {:?}", font_match.unicode_ranges);
//!     } else {
//!         println!("No matching font found");
//!     }
//! }
//! ```
//!
//! ### Resolve Font Chain and Query for Text
//!
//! ```rust,no_run
//! use rust_fontconfig::{FcFontCache, FcWeight, PatternMatch};
//!
//! fn main() {
//!     # #[cfg(feature = "std")]
//!     # {
//!     let cache = FcFontCache::build();
//!
//!     // Build font fallback chain (without text parameter)
//!     let font_chain = cache.resolve_font_chain(
//!         &["Arial".to_string(), "sans-serif".to_string()],
//!         FcWeight::Normal,
//!         PatternMatch::DontCare,
//!         PatternMatch::DontCare,
//!         &mut Vec::new(),
//!     );
//!
//!     // Query which fonts to use for specific text
//!     let text = "Hello 你好 Здравствуйте";
//!     let font_runs = font_chain.query_for_text(&cache, text);
//!
//!     println!("Text split into {} font runs:", font_runs.len());
//!     for run in font_runs {
//!         println!("  '{}' -> font {:?}", run.text, run.font_id);
//!     }
//!     # }
//! }
//! ```

#![allow(non_snake_case)]

// As of v4.1 this crate is std-only. The v4.0 `no_std` path is gone —
// it never supported the registry / multi-thread parsing anyway, and
// the shared-state `FcFontCache` refactor depends on `std::sync::RwLock`
// which is unavailable without std. Keeping the `alloc::` import paths
// means the existing call sites in this file and submodules keep
// compiling — in std builds `alloc` is just `core::alloc`'s companion
// crate already linked by the standard library.
extern crate alloc;

use alloc::collections::btree_map::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
#[cfg(all(feature = "std", feature = "parsing"))]
use allsorts::binary::read::ReadScope;
#[cfg(all(feature = "std", feature = "parsing"))]
use allsorts::get_name::fontcode_get_name;
#[cfg(all(feature = "std", feature = "parsing"))]
use allsorts::tables::os2::Os2;
#[cfg(all(feature = "std", feature = "parsing"))]
use allsorts::tables::{FontTableProvider, HheaTable, HmtxTable, MaxpTable};
#[cfg(all(feature = "std", feature = "parsing"))]
use allsorts::tag;
#[cfg(feature = "std")]
use std::path::PathBuf;

pub mod utils;
#[cfg(feature = "std")]
pub mod config;

#[cfg(feature = "ffi")]
pub mod ffi;

#[cfg(feature = "async-registry")]
pub mod scoring;
#[cfg(feature = "async-registry")]
pub mod registry;
#[cfg(feature = "async-registry")]
pub mod multithread;
#[cfg(feature = "cache")]
pub mod disk_cache;

#[cfg(all(target_os = "ios", feature = "std", feature = "parsing"))]
mod mobile_ios;

/// Operating system type for generic font family resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatingSystem {
    Windows,
    Linux,
    MacOS,
    IOS,
    Android,
    Wasm,
}

impl OperatingSystem {
    /// Detect the current operating system at compile time
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        return OperatingSystem::Windows;

        #[cfg(target_os = "linux")]
        return OperatingSystem::Linux;

        #[cfg(target_os = "macos")]
        return OperatingSystem::MacOS;

        #[cfg(target_os = "ios")]
        return OperatingSystem::IOS;

        #[cfg(target_os = "android")]
        return OperatingSystem::Android;

        #[cfg(target_family = "wasm")]
        return OperatingSystem::Wasm;

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos", target_os = "ios", target_os = "android", target_family = "wasm")))]
        return OperatingSystem::Linux; // Default fallback
    }
    
    /// Get system-specific fonts for the "serif" generic family
    /// Prioritizes fonts based on Unicode range coverage
    pub fn get_serif_fonts(&self, unicode_ranges: &[UnicodeRange]) -> Vec<String> {
        let has_cjk = has_cjk_ranges(unicode_ranges);
        let has_arabic = has_arabic_ranges(unicode_ranges);
        let _has_cyrillic = has_cyrillic_ranges(unicode_ranges);
        
        match self {
            OperatingSystem::Windows => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["MS Mincho", "SimSun", "MingLiU"]);
                }
                if has_arabic {
                    fonts.push("Traditional Arabic");
                }
                fonts.push("Times New Roman");
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Linux => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["Noto Serif CJK SC", "Noto Serif CJK JP", "Noto Serif CJK KR"]);
                }
                if has_arabic {
                    fonts.push("Noto Serif Arabic");
                }
                fonts.extend_from_slice(&[
                    "Times", "Times New Roman", "DejaVu Serif", "Free Serif", 
                    "Noto Serif", "Bitstream Vera Serif", "Roman", "Regular"
                ]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::MacOS | OperatingSystem::IOS => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["Hiragino Mincho ProN", "STSong", "AppleMyungjo"]);
                }
                if has_arabic {
                    fonts.push("Geeza Pro");
                }
                fonts.extend_from_slice(&["Times New Roman", "Times", "New York", "Palatino"]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Android => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["Noto Serif CJK SC", "Noto Serif CJK JP", "Noto Serif CJK KR"]);
                }
                if has_arabic {
                    fonts.push("Noto Naskh Arabic");
                }
                fonts.extend_from_slice(&["Noto Serif", "Roboto Serif", "Droid Serif"]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Wasm => Vec::new(),
        }
    }

    /// Get system-specific fonts for the "sans-serif" generic family
    /// Prioritizes fonts based on Unicode range coverage
    pub fn get_sans_serif_fonts(&self, unicode_ranges: &[UnicodeRange]) -> Vec<String> {
        let has_cjk = has_cjk_ranges(unicode_ranges);
        let has_arabic = has_arabic_ranges(unicode_ranges);
        let _has_cyrillic = has_cyrillic_ranges(unicode_ranges);
        let has_hebrew = has_hebrew_ranges(unicode_ranges);
        let has_thai = has_thai_ranges(unicode_ranges);
        
        match self {
            OperatingSystem::Windows => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["Microsoft YaHei", "MS Gothic", "Malgun Gothic", "SimHei"]);
                }
                if has_arabic {
                    fonts.push("Segoe UI Arabic");
                }
                if has_hebrew {
                    fonts.push("Segoe UI Hebrew");
                }
                if has_thai {
                    fonts.push("Leelawadee UI");
                }
                fonts.extend_from_slice(&["Segoe UI", "Tahoma", "Microsoft Sans Serif", "MS Sans Serif", "Helv"]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Linux => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&[
                        "Noto Sans CJK SC", "Noto Sans CJK JP", "Noto Sans CJK KR",
                        "WenQuanYi Micro Hei", "Droid Sans Fallback"
                    ]);
                }
                if has_arabic {
                    fonts.push("Noto Sans Arabic");
                }
                if has_hebrew {
                    fonts.push("Noto Sans Hebrew");
                }
                if has_thai {
                    fonts.push("Noto Sans Thai");
                }
                fonts.extend_from_slice(&["Ubuntu", "Arial", "DejaVu Sans", "Noto Sans", "Liberation Sans"]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::MacOS | OperatingSystem::IOS => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&[
                        "Hiragino Sans", "Hiragino Kaku Gothic ProN",
                        "PingFang SC", "PingFang TC", "Apple SD Gothic Neo"
                    ]);
                }
                if has_arabic {
                    fonts.push("Geeza Pro");
                }
                if has_hebrew {
                    fonts.push("Arial Hebrew");
                }
                if has_thai {
                    fonts.push("Thonburi");
                }
                fonts.extend_from_slice(&[
                    "San Francisco", ".AppleSystemUIFont", ".SFUIText", ".SFUI-Regular",
                    "Helvetica Neue", "Helvetica", "Lucida Grande",
                ]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Android => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&[
                        "Noto Sans CJK SC", "Noto Sans CJK JP", "Noto Sans CJK KR",
                        "Droid Sans Fallback",
                    ]);
                }
                if has_arabic {
                    fonts.push("Noto Sans Arabic");
                }
                if has_hebrew {
                    fonts.push("Noto Sans Hebrew");
                }
                if has_thai {
                    fonts.push("Noto Sans Thai");
                }
                fonts.extend_from_slice(&[
                    "Roboto", "Roboto-Regular", "Noto Sans", "Droid Sans",
                ]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Wasm => Vec::new(),
        }
    }

    /// Get system-specific fonts for the "monospace" generic family
    /// Prioritizes fonts based on Unicode range coverage
    pub fn get_monospace_fonts(&self, unicode_ranges: &[UnicodeRange]) -> Vec<String> {
        let has_cjk = has_cjk_ranges(unicode_ranges);
        
        match self {
            OperatingSystem::Windows => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["MS Gothic", "SimHei"]);
                }
                fonts.extend_from_slice(&["Segoe UI Mono", "Courier New", "Cascadia Code", "Cascadia Mono", "Consolas"]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Linux => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["Noto Sans Mono CJK SC", "Noto Sans Mono CJK JP", "WenQuanYi Zen Hei Mono"]);
                }
                fonts.extend_from_slice(&[
                    "Source Code Pro", "Cantarell", "DejaVu Sans Mono", 
                    "Roboto Mono", "Ubuntu Monospace", "Droid Sans Mono"
                ]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::MacOS | OperatingSystem::IOS => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["Hiragino Sans", "PingFang SC"]);
                }
                fonts.extend_from_slice(&["SF Mono", "Menlo", "Monaco", "Courier", "Oxygen Mono", "Source Code Pro", "Fira Mono"]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Android => {
                let mut fonts = Vec::new();
                if has_cjk {
                    fonts.extend_from_slice(&["Noto Sans Mono CJK SC", "Noto Sans Mono CJK JP"]);
                }
                fonts.extend_from_slice(&["Roboto Mono", "Droid Sans Mono", "Noto Sans Mono", "DejaVu Sans Mono"]);
                fonts.iter().map(|s| s.to_string()).collect()
            }
            OperatingSystem::Wasm => Vec::new(),
        }
    }
    
    /// Expand a generic CSS font family to system-specific font names
    /// Returns the original name if not a generic family
    /// Prioritizes fonts based on Unicode range coverage
    pub fn expand_generic_family(&self, family: &str, unicode_ranges: &[UnicodeRange]) -> Vec<String> {
        match family.to_ascii_lowercase().as_str() {
            "serif" => self.get_serif_fonts(unicode_ranges),
            "sans-serif" => self.get_sans_serif_fonts(unicode_ranges),
            "monospace" => self.get_monospace_fonts(unicode_ranges),
            "cursive" | "fantasy" | "system-ui" => {
                // Use sans-serif as fallback for these
                self.get_sans_serif_fonts(unicode_ranges)
            }
            _ => vec![family.to_string()],
        }
    }
}

/// Expand a CSS font-family stack with generic families resolved to OS-specific fonts
/// Prioritizes fonts based on Unicode range coverage
/// Example: ["Arial", "sans-serif"] on macOS with CJK ranges -> ["Arial", "PingFang SC", "Hiragino Sans", ...]
pub fn expand_font_families(families: &[String], os: OperatingSystem, unicode_ranges: &[UnicodeRange]) -> Vec<String> {
    let mut expanded = Vec::new();
    
    for family in families {
        expanded.extend(os.expand_generic_family(family, unicode_ranges));
    }
    
    expanded
}

/// UUID to identify a font (collections are broken up into separate fonts)
#[derive(Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
pub struct FontId(pub u128);

impl core::fmt::Debug for FontId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
    }
}

impl core::fmt::Display for FontId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let id = self.0;
        write!(
            f,
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            (id >> 96) & 0xFFFFFFFF,
            (id >> 80) & 0xFFFF,
            (id >> 64) & 0xFFFF,
            (id >> 48) & 0xFFFF,
            id & 0xFFFFFFFFFFFF
        )
    }
}

impl FontId {
    /// Generate a new unique FontId using an atomic counter
    pub fn new() -> Self {
        use core::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
        FontId(id)
    }
}

/// Whether a field is required to match (yes / no / don't care)
#[derive(Debug, Default, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub enum PatternMatch {
    /// Default: don't particularly care whether the requirement matches
    #[default]
    DontCare,
    /// Requirement has to be true for the selected font
    True,
    /// Requirement has to be false for the selected font
    False,
}

impl PatternMatch {
    fn needs_to_match(&self) -> bool {
        matches!(self, PatternMatch::True | PatternMatch::False)
    }

    fn matches(&self, other: &PatternMatch) -> bool {
        match (self, other) {
            (PatternMatch::DontCare, _) => true,
            (_, PatternMatch::DontCare) => true,
            (a, b) => a == b,
        }
    }
}

/// Font weight values as defined in CSS specification
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub enum FcWeight {
    Thin = 100,
    ExtraLight = 200,
    Light = 300,
    Normal = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
    ExtraBold = 800,
    Black = 900,
}

impl FcWeight {
    pub fn from_u16(weight: u16) -> Self {
        match weight {
            0..=149 => FcWeight::Thin,
            150..=249 => FcWeight::ExtraLight,
            250..=349 => FcWeight::Light,
            350..=449 => FcWeight::Normal,
            450..=549 => FcWeight::Medium,
            550..=649 => FcWeight::SemiBold,
            650..=749 => FcWeight::Bold,
            750..=849 => FcWeight::ExtraBold,
            _ => FcWeight::Black,
        }
    }

    pub fn find_best_match(&self, available: &[FcWeight]) -> Option<FcWeight> {
        if available.is_empty() {
            return None;
        }

        // Exact match
        if available.contains(self) {
            return Some(*self);
        }

        // Get numeric value
        let self_value = *self as u16;

        match *self {
            FcWeight::Normal => {
                // For Normal (400), try Medium (500) first
                if available.contains(&FcWeight::Medium) {
                    return Some(FcWeight::Medium);
                }
                // Then try lighter weights
                for weight in &[FcWeight::Light, FcWeight::ExtraLight, FcWeight::Thin] {
                    if available.contains(weight) {
                        return Some(*weight);
                    }
                }
                // Last, try heavier weights
                for weight in &[
                    FcWeight::SemiBold,
                    FcWeight::Bold,
                    FcWeight::ExtraBold,
                    FcWeight::Black,
                ] {
                    if available.contains(weight) {
                        return Some(*weight);
                    }
                }
            }
            FcWeight::Medium => {
                // For Medium (500), try Normal (400) first
                if available.contains(&FcWeight::Normal) {
                    return Some(FcWeight::Normal);
                }
                // Then try lighter weights
                for weight in &[FcWeight::Light, FcWeight::ExtraLight, FcWeight::Thin] {
                    if available.contains(weight) {
                        return Some(*weight);
                    }
                }
                // Last, try heavier weights
                for weight in &[
                    FcWeight::SemiBold,
                    FcWeight::Bold,
                    FcWeight::ExtraBold,
                    FcWeight::Black,
                ] {
                    if available.contains(weight) {
                        return Some(*weight);
                    }
                }
            }
            FcWeight::Thin | FcWeight::ExtraLight | FcWeight::Light => {
                // For lightweight fonts (<400), first try lighter or equal weights
                let mut best_match = None;
                let mut smallest_diff = u16::MAX;

                // Find the closest lighter weight
                for weight in available {
                    let weight_value = *weight as u16;
                    // Only consider weights <= self (per test expectation)
                    if weight_value <= self_value {
                        let diff = self_value - weight_value;
                        if diff < smallest_diff {
                            smallest_diff = diff;
                            best_match = Some(*weight);
                        }
                    }
                }

                if best_match.is_some() {
                    return best_match;
                }

                // If no lighter weight, find the closest heavier weight
                best_match = None;
                smallest_diff = u16::MAX;

                for weight in available {
                    let weight_value = *weight as u16;
                    if weight_value > self_value {
                        let diff = weight_value - self_value;
                        if diff < smallest_diff {
                            smallest_diff = diff;
                            best_match = Some(*weight);
                        }
                    }
                }

                return best_match;
            }
            FcWeight::SemiBold | FcWeight::Bold | FcWeight::ExtraBold | FcWeight::Black => {
                // For heavyweight fonts (>500), first try heavier or equal weights
                let mut best_match = None;
                let mut smallest_diff = u16::MAX;

                // Find the closest heavier weight
                for weight in available {
                    let weight_value = *weight as u16;
                    // Only consider weights >= self
                    if weight_value >= self_value {
                        let diff = weight_value - self_value;
                        if diff < smallest_diff {
                            smallest_diff = diff;
                            best_match = Some(*weight);
                        }
                    }
                }

                if best_match.is_some() {
                    return best_match;
                }

                // If no heavier weight, find the closest lighter weight
                best_match = None;
                smallest_diff = u16::MAX;

                for weight in available {
                    let weight_value = *weight as u16;
                    if weight_value < self_value {
                        let diff = self_value - weight_value;
                        if diff < smallest_diff {
                            smallest_diff = diff;
                            best_match = Some(*weight);
                        }
                    }
                }

                return best_match;
            }
        }

        // If nothing matches by now, return the first available weight
        Some(available[0])
    }
}

impl Default for FcWeight {
    fn default() -> Self {
        FcWeight::Normal
    }
}

/// CSS font-stretch values
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub enum FcStretch {
    UltraCondensed = 1,
    ExtraCondensed = 2,
    Condensed = 3,
    SemiCondensed = 4,
    Normal = 5,
    SemiExpanded = 6,
    Expanded = 7,
    ExtraExpanded = 8,
    UltraExpanded = 9,
}

impl FcStretch {
    pub fn is_condensed(&self) -> bool {
        use self::FcStretch::*;
        match self {
            UltraCondensed => true,
            ExtraCondensed => true,
            Condensed => true,
            SemiCondensed => true,
            Normal => false,
            SemiExpanded => false,
            Expanded => false,
            ExtraExpanded => false,
            UltraExpanded => false,
        }
    }
    pub fn from_u16(width_class: u16) -> Self {
        match width_class {
            1 => FcStretch::UltraCondensed,
            2 => FcStretch::ExtraCondensed,
            3 => FcStretch::Condensed,
            4 => FcStretch::SemiCondensed,
            5 => FcStretch::Normal,
            6 => FcStretch::SemiExpanded,
            7 => FcStretch::Expanded,
            8 => FcStretch::ExtraExpanded,
            9 => FcStretch::UltraExpanded,
            _ => FcStretch::Normal,
        }
    }

    /// Follows CSS spec for stretch matching
    pub fn find_best_match(&self, available: &[FcStretch]) -> Option<FcStretch> {
        if available.is_empty() {
            return None;
        }

        if available.contains(self) {
            return Some(*self);
        }

        // For 'normal' or condensed values, narrower widths are checked first, then wider values
        if *self <= FcStretch::Normal {
            // Find narrower values first
            let mut closest_narrower = None;
            for stretch in available.iter() {
                if *stretch < *self
                    && (closest_narrower.is_none() || *stretch > closest_narrower.unwrap())
                {
                    closest_narrower = Some(*stretch);
                }
            }

            if closest_narrower.is_some() {
                return closest_narrower;
            }

            // Otherwise, find wider values
            let mut closest_wider = None;
            for stretch in available.iter() {
                if *stretch > *self
                    && (closest_wider.is_none() || *stretch < closest_wider.unwrap())
                {
                    closest_wider = Some(*stretch);
                }
            }

            return closest_wider;
        } else {
            // For expanded values, wider values are checked first, then narrower values
            let mut closest_wider = None;
            for stretch in available.iter() {
                if *stretch > *self
                    && (closest_wider.is_none() || *stretch < closest_wider.unwrap())
                {
                    closest_wider = Some(*stretch);
                }
            }

            if closest_wider.is_some() {
                return closest_wider;
            }

            // Otherwise, find narrower values
            let mut closest_narrower = None;
            for stretch in available.iter() {
                if *stretch < *self
                    && (closest_narrower.is_none() || *stretch > closest_narrower.unwrap())
                {
                    closest_narrower = Some(*stretch);
                }
            }

            return closest_narrower;
        }
    }
}

impl Default for FcStretch {
    fn default() -> Self {
        FcStretch::Normal
    }
}

/// Unicode range representation for font matching
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
pub struct UnicodeRange {
    pub start: u32,
    pub end: u32,
}

/// The default set of Unicode-block fallback scripts that
/// [`FcFontCache::resolve_font_chain`] pulls in when no explicit
/// `scripts_hint` is supplied.
///
/// Keeping this exposed lets callers that *do* want the default
/// behaviour build the set explicitly — typically by union-ing it
/// with a detected-from-document set before calling
/// [`FcFontCache::resolve_font_chain_with_scripts`].
pub const DEFAULT_UNICODE_FALLBACK_SCRIPTS: &[UnicodeRange] = &[
    UnicodeRange { start: 0x0400, end: 0x04FF }, // Cyrillic
    UnicodeRange { start: 0x0600, end: 0x06FF }, // Arabic
    UnicodeRange { start: 0x0900, end: 0x097F }, // Devanagari
    UnicodeRange { start: 0x3040, end: 0x309F }, // Hiragana
    UnicodeRange { start: 0x30A0, end: 0x30FF }, // Katakana
    UnicodeRange { start: 0x4E00, end: 0x9FFF }, // CJK Unified Ideographs
    UnicodeRange { start: 0xAC00, end: 0xD7A3 }, // Hangul Syllables
];

impl UnicodeRange {
    pub fn contains(&self, c: char) -> bool {
        let c = c as u32;
        c >= self.start && c <= self.end
    }

    pub fn overlaps(&self, other: &UnicodeRange) -> bool {
        self.start <= other.end && other.start <= self.end
    }

    pub fn is_subset_of(&self, other: &UnicodeRange) -> bool {
        self.start >= other.start && self.end <= other.end
    }
}

/// Check if any range covers CJK Unified Ideographs, Hiragana, Katakana, or Hangul
pub fn has_cjk_ranges(ranges: &[UnicodeRange]) -> bool {
    ranges.iter().any(|r| {
        (r.start >= 0x4E00 && r.start <= 0x9FFF) ||
        (r.start >= 0x3040 && r.start <= 0x309F) ||
        (r.start >= 0x30A0 && r.start <= 0x30FF) ||
        (r.start >= 0xAC00 && r.start <= 0xD7AF)
    })
}

/// Check if any range covers the Arabic block
pub fn has_arabic_ranges(ranges: &[UnicodeRange]) -> bool {
    ranges.iter().any(|r| r.start >= 0x0600 && r.start <= 0x06FF)
}

/// Check if any range covers the Cyrillic block
pub fn has_cyrillic_ranges(ranges: &[UnicodeRange]) -> bool {
    ranges.iter().any(|r| r.start >= 0x0400 && r.start <= 0x04FF)
}

/// Check if any range covers the Hebrew block
pub fn has_hebrew_ranges(ranges: &[UnicodeRange]) -> bool {
    ranges.iter().any(|r| r.start >= 0x0590 && r.start <= 0x05FF)
}

/// Check if any range covers the Thai block
pub fn has_thai_ranges(ranges: &[UnicodeRange]) -> bool {
    ranges.iter().any(|r| r.start >= 0x0E00 && r.start <= 0x0E7F)
}

/// Log levels for trace messages
#[derive(Debug, Clone, Copy, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub enum TraceLevel {
    Debug,
    Info,
    Warning,
    Error,
}

/// Reason for font matching failure or success
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum MatchReason {
    NameMismatch {
        requested: Option<String>,
        found: Option<String>,
    },
    FamilyMismatch {
        requested: Option<String>,
        found: Option<String>,
    },
    StyleMismatch {
        property: &'static str,
        requested: String,
        found: String,
    },
    WeightMismatch {
        requested: FcWeight,
        found: FcWeight,
    },
    StretchMismatch {
        requested: FcStretch,
        found: FcStretch,
    },
    UnicodeRangeMismatch {
        character: char,
        ranges: Vec<UnicodeRange>,
    },
    Success,
}

/// Trace message for debugging font matching
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceMsg {
    pub level: TraceLevel,
    pub path: String,
    pub reason: MatchReason,
}

/// Hinting style for font rendering.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
pub enum FcHintStyle {
    #[default]
    None = 0,
    Slight = 1,
    Medium = 2,
    Full = 3,
}

/// Subpixel rendering order.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
pub enum FcRgba {
    #[default]
    Unknown = 0,
    Rgb = 1,
    Bgr = 2,
    Vrgb = 3,
    Vbgr = 4,
    None = 5,
}

/// LCD filter mode for subpixel rendering.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
pub enum FcLcdFilter {
    #[default]
    None = 0,
    Default = 1,
    Light = 2,
    Legacy = 3,
}

/// Per-font rendering configuration from system font config (Linux fonts.conf).
///
/// All fields are `Option<T>` -- `None` means "use system default".
/// On non-Linux platforms, this is always all-None (no per-font overrides).
#[derive(Debug, Default, Clone, PartialEq, PartialOrd)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
pub struct FcFontRenderConfig {
    pub antialias: Option<bool>,
    pub hinting: Option<bool>,
    pub hintstyle: Option<FcHintStyle>,
    pub autohint: Option<bool>,
    pub rgba: Option<FcRgba>,
    pub lcdfilter: Option<FcLcdFilter>,
    pub embeddedbitmap: Option<bool>,
    pub embolden: Option<bool>,
    pub dpi: Option<f64>,
    pub scale: Option<f64>,
    pub minspace: Option<bool>,
}

/// Helper newtype to provide Eq/Ord for Option<f64> via total-order bit comparison.
/// This allows FcFontRenderConfig to be used inside FcPattern which derives Eq + Ord.
impl Eq for FcFontRenderConfig {}

impl Ord for FcFontRenderConfig {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Compare all non-f64 fields first
        let ord = self.antialias.cmp(&other.antialias)
            .then_with(|| self.hinting.cmp(&other.hinting))
            .then_with(|| self.hintstyle.cmp(&other.hintstyle))
            .then_with(|| self.autohint.cmp(&other.autohint))
            .then_with(|| self.rgba.cmp(&other.rgba))
            .then_with(|| self.lcdfilter.cmp(&other.lcdfilter))
            .then_with(|| self.embeddedbitmap.cmp(&other.embeddedbitmap))
            .then_with(|| self.embolden.cmp(&other.embolden))
            .then_with(|| self.minspace.cmp(&other.minspace));

        // For f64 fields, use to_bits() for total ordering
        let ord = ord.then_with(|| {
            let a = self.dpi.map(|v| v.to_bits());
            let b = other.dpi.map(|v| v.to_bits());
            a.cmp(&b)
        });
        ord.then_with(|| {
            let a = self.scale.map(|v| v.to_bits());
            let b = other.scale.map(|v| v.to_bits());
            a.cmp(&b)
        })
    }
}

/// Font pattern for matching
#[derive(Default, Clone, PartialOrd, Ord, PartialEq, Eq)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct FcPattern {
    // font name
    pub name: Option<String>,
    // family name
    pub family: Option<String>,
    // "italic" property
    pub italic: PatternMatch,
    // "oblique" property
    pub oblique: PatternMatch,
    // "bold" property
    pub bold: PatternMatch,
    // "monospace" property
    pub monospace: PatternMatch,
    // "condensed" property
    pub condensed: PatternMatch,
    // font weight
    pub weight: FcWeight,
    // font stretch
    pub stretch: FcStretch,
    // unicode ranges to match
    pub unicode_ranges: Vec<UnicodeRange>,
    // extended font metadata
    pub metadata: FcFontMetadata,
    // per-font rendering configuration (from system fonts.conf on Linux)
    pub render_config: FcFontRenderConfig,
}

impl core::fmt::Debug for FcPattern {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut d = f.debug_struct("FcPattern");

        if let Some(name) = &self.name {
            d.field("name", name);
        }

        if let Some(family) = &self.family {
            d.field("family", family);
        }

        if self.italic != PatternMatch::DontCare {
            d.field("italic", &self.italic);
        }

        if self.oblique != PatternMatch::DontCare {
            d.field("oblique", &self.oblique);
        }

        if self.bold != PatternMatch::DontCare {
            d.field("bold", &self.bold);
        }

        if self.monospace != PatternMatch::DontCare {
            d.field("monospace", &self.monospace);
        }

        if self.condensed != PatternMatch::DontCare {
            d.field("condensed", &self.condensed);
        }

        if self.weight != FcWeight::Normal {
            d.field("weight", &self.weight);
        }

        if self.stretch != FcStretch::Normal {
            d.field("stretch", &self.stretch);
        }

        if !self.unicode_ranges.is_empty() {
            d.field("unicode_ranges", &self.unicode_ranges);
        }

        // Only show non-empty metadata fields
        let empty_metadata = FcFontMetadata::default();
        if self.metadata != empty_metadata {
            d.field("metadata", &self.metadata);
        }

        // Only show render_config when it differs from default
        let empty_render_config = FcFontRenderConfig::default();
        if self.render_config != empty_render_config {
            d.field("render_config", &self.render_config);
        }

        d.finish()
    }
}

/// Font metadata from the OS/2 table
#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
pub struct FcFontMetadata {
    pub copyright: Option<String>,
    pub designer: Option<String>,
    pub designer_url: Option<String>,
    pub font_family: Option<String>,
    pub font_subfamily: Option<String>,
    pub full_name: Option<String>,
    pub id_description: Option<String>,
    pub license: Option<String>,
    pub license_url: Option<String>,
    pub manufacturer: Option<String>,
    pub manufacturer_url: Option<String>,
    pub postscript_name: Option<String>,
    pub preferred_family: Option<String>,
    pub preferred_subfamily: Option<String>,
    pub trademark: Option<String>,
    pub unique_id: Option<String>,
    pub version: Option<String>,
}

impl FcPattern {
    /// Check if this pattern would match the given character
    pub fn contains_char(&self, c: char) -> bool {
        if self.unicode_ranges.is_empty() {
            return true; // No ranges specified means match all characters
        }

        for range in &self.unicode_ranges {
            if range.contains(c) {
                return true;
            }
        }

        false
    }
}

/// Font match result with UUID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontMatch {
    pub id: FontId,
    pub unicode_ranges: Vec<UnicodeRange>,
    pub fallbacks: Vec<FontMatchNoFallback>,
}

/// Font match result with UUID (without fallback)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontMatchNoFallback {
    pub id: FontId,
    pub unicode_ranges: Vec<UnicodeRange>,
}

/// A run of text that uses the same font
/// Returned by FontFallbackChain::query_for_text()
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFontRun {
    /// The text content of this run
    pub text: String,
    /// Start byte index in the original text
    pub start_byte: usize,
    /// End byte index in the original text (exclusive)
    pub end_byte: usize,
    /// The font to use for this run (None if no font found)
    pub font_id: Option<FontId>,
    /// Which CSS font-family this came from
    pub css_source: String,
}

/// Resolved font fallback chain for a CSS font-family stack
/// This represents the complete chain of fonts to use for rendering text
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFallbackChain {
    /// CSS-based fallbacks: Each CSS font expanded to its system fallbacks
    /// Example: ["NotoSansJP" -> [Hiragino Sans, PingFang SC], "sans-serif" -> [Helvetica]]
    pub css_fallbacks: Vec<CssFallbackGroup>,
    
    /// Unicode-based fallbacks: Fonts added to cover missing Unicode ranges
    /// Only populated if css_fallbacks don't cover all requested characters
    pub unicode_fallbacks: Vec<FontMatch>,
    
    /// The original CSS font-family stack that was requested
    pub original_stack: Vec<String>,
}

impl FontFallbackChain {
    /// Resolve which font should be used for a specific character
    /// Returns (FontId, css_source_name) where css_source_name indicates which CSS font matched
    /// Returns None if no font in the chain can render this character
    pub fn resolve_char(&self, cache: &FcFontCache, ch: char) -> Option<(FontId, String)> {
        let codepoint = ch as u32;

        // Check CSS fallbacks in order
        for group in &self.css_fallbacks {
            for font in &group.fonts {
                let Some(meta) = cache.get_metadata_by_id(&font.id) else { continue };
                if meta.unicode_ranges.is_empty() {
                    continue; // No range info — don't assume it covers everything
                }
                if meta.unicode_ranges.iter().any(|r| codepoint >= r.start && codepoint <= r.end) {
                    return Some((font.id, group.css_name.clone()));
                }
            }
        }

        // Check Unicode fallbacks
        for font in &self.unicode_fallbacks {
            let Some(meta) = cache.get_metadata_by_id(&font.id) else { continue };
            if meta.unicode_ranges.iter().any(|r| codepoint >= r.start && codepoint <= r.end) {
                return Some((font.id, "(unicode-fallback)".to_string()));
            }
        }

        // WEB-LIFT LAST-RESORT (re-added 2026-06-03; the `with_memory_fonts` trap that
        // previously made touching this file fatal is now fixed by the byte-atomic remill
        // fork support). The lifted web path fails coverage-based resolution above for TWO
        // reasons that both mis-lift: the chain mis-builds to empty AND/OR `get_metadata_by_id`
        // (a HashMap<FontId,_> lookup) returns None in the lift. So instead of gating on the
        // chain being empty, fire whenever NOTHING matched above AND the cache holds exactly
        // the single registered fallback font — the headless/web case. This bypasses BOTH the
        // chain and the metadata HashMap, returning the only font's id directly. Native caches
        // hold many system fonts, so `len()==1` is false there → native is unaffected.
        let registered = cache.list();
        if registered.len() == 1 {
            return Some((registered[0].1, "(web-last-resort)".to_string()));
        }

        None
    }
    
    /// Resolve all characters in a text string to their fonts
    /// Returns a vector of (character, FontId, css_source) tuples
    pub fn resolve_text(&self, cache: &FcFontCache, text: &str) -> Vec<(char, Option<(FontId, String)>)> {
        text.chars()
            .map(|ch| (ch, self.resolve_char(cache, ch)))
            .collect()
    }
    
    /// Query which fonts should be used for a text string, grouped by font
    /// Returns runs of consecutive characters that use the same font
    /// This is the main API for text shaping - call this to get font runs, then shape each run
    pub fn query_for_text(&self, cache: &FcFontCache, text: &str) -> Vec<ResolvedFontRun> {
        if text.is_empty() {
            return Vec::new();
        }
        
        let mut runs: Vec<ResolvedFontRun> = Vec::new();
        let mut current_font: Option<FontId> = None;
        let mut current_css_source: Option<String> = None;
        let mut current_start_byte: usize = 0;
        
        for (byte_idx, ch) in text.char_indices() {
            let resolved = self.resolve_char(cache, ch);
            let (font_id, css_source) = match &resolved {
                Some((id, source)) => (Some(*id), Some(source.clone())),
                None => (None, None),
            };
            
            // Check if we need to start a new run
            let font_changed = font_id != current_font;
            
            if font_changed && byte_idx > 0 {
                // Finalize the current run
                let run_text = &text[current_start_byte..byte_idx];
                runs.push(ResolvedFontRun {
                    text: run_text.to_string(),
                    start_byte: current_start_byte,
                    end_byte: byte_idx,
                    font_id: current_font,
                    css_source: current_css_source.clone().unwrap_or_default(),
                });
                current_start_byte = byte_idx;
            }
            
            current_font = font_id;
            current_css_source = css_source;
        }
        
        // Finalize the last run
        if current_start_byte < text.len() {
            let run_text = &text[current_start_byte..];
            runs.push(ResolvedFontRun {
                text: run_text.to_string(),
                start_byte: current_start_byte,
                end_byte: text.len(),
                font_id: current_font,
                css_source: current_css_source.unwrap_or_default(),
            });
        }
        
        runs
    }
}

/// A group of fonts that are fallbacks for a single CSS font-family name
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssFallbackGroup {
    /// The CSS font name (e.g., "NotoSansJP", "sans-serif")
    pub css_name: String,
    
    /// System fonts that match this CSS name
    /// First font in list is the best match
    pub fonts: Vec<FontMatch>,
}

/// Cache key for font fallback chain queries
///
/// IMPORTANT: This key intentionally does NOT include per-text unicode
/// ranges — fallback chains are cached by CSS properties only. Different
/// texts with the same CSS font-stack share the same chain.
///
/// `scripts_hint_hash` distinguishes *which set of Unicode-fallback
/// scripts* the caller asked for. `None` means "the default set of 7
/// major scripts" (Cyrillic/Arabic/Devanagari/Hiragana/Katakana/CJK/Hangul,
/// back-compat behaviour of `resolve_font_chain`). `Some(h)` is a
/// stable hash of a caller-supplied script list so an ASCII-only
/// query doesn't collide with a CJK-aware one.
#[cfg(feature = "std")]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct FontChainCacheKey {
    /// CSS font stack (expanded to OS-specific fonts)
    pub(crate) font_families: Vec<String>,
    /// Font weight
    pub(crate) weight: FcWeight,
    /// Font style flags
    pub(crate) italic: PatternMatch,
    pub(crate) oblique: PatternMatch,
    /// Hash of the caller-supplied script hint (or `None` for the default set).
    pub(crate) scripts_hint_hash: Option<u64>,
}

/// Hash a `scripts_hint` slice into a stable u64 for use as a
/// [`FontChainCacheKey`] component. Order-insensitive: we sort a
/// local copy before hashing so `[CJK, Arabic]` and `[Arabic, CJK]`
/// key into the same cache slot.
#[cfg(feature = "std")]
fn hash_scripts_hint(ranges: &[UnicodeRange]) -> u64 {
    let mut sorted: Vec<UnicodeRange> = ranges.to_vec();
    sorted.sort();
    let mut buf = Vec::with_capacity(sorted.len() * 8);
    for r in &sorted {
        buf.extend_from_slice(&r.start.to_le_bytes());
        buf.extend_from_slice(&r.end.to_le_bytes());
    }
    crate::utils::content_hash_u64(&buf)
}

/// Path to a font file
///
/// `bytes_hash` is a deterministic 64-bit hash of the file's full
/// byte contents (see [`crate::utils::content_hash_u64`]). All faces
/// of a given `.ttc` file share the same `bytes_hash`, and two
/// different paths pointing at the same file contents also do —
/// so the cache can share a single `Arc<[u8]>` across them via
/// [`FcFontCache::get_font_bytes`]. A value of `0` means "hash
/// not computed" (e.g. built from a filename-only scan, or loaded
/// from a legacy v1 disk cache); callers must treat `0` as opaque
/// and fall back to unshared reads.
#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
#[cfg_attr(feature = "cache", derive(serde::Serialize, serde::Deserialize))]
#[repr(C)]
pub struct FcFontPath {
    pub path: String,
    pub font_index: usize,
    /// 64-bit content hash of the file's bytes. 0 = not computed.
    #[cfg_attr(feature = "cache", serde(default))]
    pub bytes_hash: u64,
}

/// In-memory font data
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct FcFont {
    pub bytes: Vec<u8>,
    pub font_index: usize,
    pub id: String, // For identification in tests
}

/// Owned font-source descriptor, returned by
/// [`FcFontCache::get_font_by_id`].
///
/// In v4.0 this was a borrowed enum (`FontSource<'a>` with refs into
/// the pattern map). With v4.1's shared-state cache, the map lives
/// behind an `RwLock`, so returning a reference would require the
/// caller to hold a read guard for the full lifetime of the result —
/// which bleeds the locking strategy into every call site. The owned
/// variant clones the small `FcFont` / `FcFontPath` struct and
/// releases the lock immediately. Bytes/mmap are not cloned — those
/// go through `get_font_bytes` which hands out `Arc<FontBytes>`.
#[derive(Debug, Clone)]
pub enum OwnedFontSource {
    /// Font loaded from memory (small metadata + owned `Vec<u8>`).
    Memory(FcFont),
    /// Font loaded from disk.
    Disk(FcFontPath),
}

/// A handle to font bytes returned by [`FcFontCache::get_font_bytes`].
///
/// On disk, an `Mmap` is used so untouched pages don't count toward
/// process RSS. In-memory fonts (`FcFont`) come back as `Owned` since
/// they're already on the heap.
///
/// `FontBytes` derefs to `[u8]` and implements `AsRef<[u8]>`, so any
/// existing API that wants `&[u8]` (allsorts, ttf-parser, …) can
/// accept it without code changes.
///
/// Both variants are `Send + Sync` (mmaps and `Arc<[u8]>` are both
/// safe to share across threads).
#[cfg(feature = "std")]
pub enum FontBytes {
    /// Heap-owned bytes. Used for `FontSource::Memory` and as a
    /// fallback when mmap is unavailable.
    Owned(std::sync::Arc<[u8]>),
    /// File-backed mmap. Read-only; pages are demand-loaded by the
    /// kernel. Absent on wasm targets, where `mmapio` is unavailable
    /// (the optional dep is gated to `cfg(not(target_family="wasm"))`).
    #[cfg(not(target_family = "wasm"))]
    Mmapped(mmapio::Mmap),
}

#[cfg(feature = "std")]
impl FontBytes {
    /// Borrow the underlying byte slice.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            FontBytes::Owned(arc) => arc,
            #[cfg(not(target_family = "wasm"))]
            FontBytes::Mmapped(m) => &m[..],
        }
    }
}

#[cfg(feature = "std")]
impl core::ops::Deref for FontBytes {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

#[cfg(feature = "std")]
impl AsRef<[u8]> for FontBytes {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

#[cfg(feature = "std")]
impl core::fmt::Debug for FontBytes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let kind = match self {
            FontBytes::Owned(_) => "Owned",
            #[cfg(not(target_family = "wasm"))]
            FontBytes::Mmapped(_) => "Mmapped",
        };
        write!(f, "FontBytes::{}({} bytes)", kind, self.as_slice().len())
    }
}

/// Open a font file as an mmap-backed [`FontBytes`]. Falls back to a
/// heap read if mmap fails (e.g. the file is on a network share that
/// doesn't support mmap, or we're on a target without `std`-mmap).
#[cfg(feature = "std")]
fn open_font_bytes_mmap(path: &str) -> Option<std::sync::Arc<FontBytes>> {
    use std::fs::File;
    use std::sync::Arc;

    #[cfg(not(target_family = "wasm"))]
    {
        if let Ok(file) = File::open(path) {
            // Safety: `Mmap::map` requires that the file is not
            // mutated while mapped. For system fonts that's the
            // overwhelming common case; if a user replaces the file
            // we accept reading the snapshot we mapped earlier.
            if let Ok(mmap) = unsafe { mmapio::MmapOptions::new().map(&file) } {
                return Some(Arc::new(FontBytes::Mmapped(mmap)));
            }
        }
    }
    let bytes = std::fs::read(path).ok()?;
    Some(Arc::new(FontBytes::Owned(Arc::from(bytes))))
}

/// A named font to be added to the font cache from memory.
/// This is the primary way to supply custom fonts to the application.
#[derive(Debug, Clone)]
pub struct NamedFont {
    /// Human-readable name for this font (e.g., "My Custom Font")
    pub name: String,
    /// The raw font file bytes (TTF, OTF, WOFF, WOFF2, TTC)
    pub bytes: Vec<u8>,
}

impl NamedFont {
    /// Create a new named font from bytes
    pub fn new(name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            name: name.into(),
            bytes,
        }
    }
}

/// Font cache, initialized at startup.
///
/// Thread-safe, shared font cache.
///
/// As of v4.1 the cache internally owns its state via
/// `Arc<RwLock<FcFontCacheInner>>`: cloning an `FcFontCache` returns
/// a handle that shares the same underlying data. Writes by one holder
/// (typically the background builder inside `FcFontRegistry`) become
/// immediately visible to every other holder (layout engines,
/// shape-time resolvers, etc.).
///
/// Before 4.1 the clone deep-copied every map, so external holders
/// were frozen at the moment they took the snapshot — the mismatch
/// between "live registry cache" and "frozen font manager cache"
/// was the root of the silent-text regression when lazy scout mode
/// was enabled. The shared-state design eliminates that entire class
/// of staleness bugs by construction.
pub struct FcFontCache {
    pub(crate) shared: std::sync::Arc<FcFontCacheShared>,
}

/// Shared interior of `FcFontCache`. Always accessed through an
/// `Arc` — never referenced directly by external callers.
// Internal lock wrapper for the cache state. Two implementations selected by feature:
//
// DEFAULT (general builds): backed by std `RwLock`. `read`/`write`/`lock` return
// `Result<_, Infallible>` for a uniform call site (a poisoned lock is recovered via
// `into_inner` — a memoisation cache is still valid to read after a panic).
//
// `single-thread-unsafe-locks` feature: a bare `UnsafeCell` with NO atomics; `read`/`write`/
// `lock` hand out a guard immediately. UNSOUND in a multi-threaded program — enable ONLY for a
// known single-threaded environment. Exists for the azul remill-lifted web backend
// (single-threaded wasm), where std's queue-based RwLock `lock_contended` path spins forever
// (no other thread ever unparks it) and hangs the layout solver.

#[cfg(not(feature = "single-thread-unsafe-locks"))]
pub struct StLock<T> {
    lock: std::sync::RwLock<T>,
}
#[cfg(not(feature = "single-thread-unsafe-locks"))]
impl<T> core::fmt::Debug for StLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("StLock(..)")
    }
}
#[cfg(not(feature = "single-thread-unsafe-locks"))]
impl<T> StLock<T> {
    pub fn new(v: T) -> Self {
        Self { lock: std::sync::RwLock::new(v) }
    }
    pub fn read(&self) -> Result<StReadGuard<'_, T>, core::convert::Infallible> {
        Ok(StReadGuard { g: self.lock.read().unwrap_or_else(|e| e.into_inner()) })
    }
    pub fn write(&self) -> Result<StWriteGuard<'_, T>, core::convert::Infallible> {
        Ok(StWriteGuard { g: self.lock.write().unwrap_or_else(|e| e.into_inner()) })
    }
    pub fn lock(&self) -> Result<StWriteGuard<'_, T>, core::convert::Infallible> {
        self.write()
    }
}
#[cfg(not(feature = "single-thread-unsafe-locks"))]
pub struct StReadGuard<'a, T> {
    g: std::sync::RwLockReadGuard<'a, T>,
}
#[cfg(not(feature = "single-thread-unsafe-locks"))]
impl<'a, T> core::ops::Deref for StReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { &self.g }
}
#[cfg(not(feature = "single-thread-unsafe-locks"))]
pub struct StWriteGuard<'a, T> {
    g: std::sync::RwLockWriteGuard<'a, T>,
}
#[cfg(not(feature = "single-thread-unsafe-locks"))]
impl<'a, T> core::ops::Deref for StWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { &self.g }
}
#[cfg(not(feature = "single-thread-unsafe-locks"))]
impl<'a, T> core::ops::DerefMut for StWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.g }
}

#[cfg(feature = "single-thread-unsafe-locks")]
pub struct StLock<T> {
    cell: std::cell::UnsafeCell<T>,
}
#[cfg(feature = "single-thread-unsafe-locks")]
unsafe impl<T> Sync for StLock<T> {}
#[cfg(feature = "single-thread-unsafe-locks")]
unsafe impl<T> Send for StLock<T> {}
#[cfg(feature = "single-thread-unsafe-locks")]
impl<T> core::fmt::Debug for StLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("StLock(..)")
    }
}
#[cfg(feature = "single-thread-unsafe-locks")]
impl<T> StLock<T> {
    pub fn new(v: T) -> Self {
        Self { cell: std::cell::UnsafeCell::new(v) }
    }
    pub fn read(&self) -> Result<StReadGuard<'_, T>, core::convert::Infallible> {
        Ok(StReadGuard { r: unsafe { &*self.cell.get() } })
    }
    pub fn write(&self) -> Result<StWriteGuard<'_, T>, core::convert::Infallible> {
        Ok(StWriteGuard { r: unsafe { &mut *self.cell.get() } })
    }
    pub fn lock(&self) -> Result<StWriteGuard<'_, T>, core::convert::Infallible> {
        Ok(StWriteGuard { r: unsafe { &mut *self.cell.get() } })
    }
}
#[cfg(feature = "single-thread-unsafe-locks")]
pub struct StReadGuard<'a, T> {
    r: &'a T,
}
#[cfg(feature = "single-thread-unsafe-locks")]
impl<'a, T> core::ops::Deref for StReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { self.r }
}
#[cfg(feature = "single-thread-unsafe-locks")]
pub struct StWriteGuard<'a, T> {
    r: &'a mut T,
}
#[cfg(feature = "single-thread-unsafe-locks")]
impl<'a, T> core::ops::Deref for StWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T { self.r }
}
#[cfg(feature = "single-thread-unsafe-locks")]
impl<'a, T> core::ops::DerefMut for StWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T { self.r }
}

pub(crate) struct FcFontCacheShared {
    /// Main pattern/metadata state, guarded by a reader-writer lock.
    /// Builder threads take the write lock to insert a parsed font;
    /// all query paths take the read lock.
    pub(crate) state: StLock<FcFontCacheInner>,
    /// Font fallback chain cache. Not part of the RwLock-guarded
    /// state because cache insertions happen under `&self` on read
    /// paths (they're a memoisation, not observable state).
    pub(crate) chain_cache: StLock<std::collections::HashMap<FontChainCacheKey, FontFallbackChain>>,
    /// Shared file-bytes cache: content-hash → weak [`FontBytes`].
    ///
    /// [`FcFontCache::get_font_bytes`] populates this so that multiple
    /// FontIds backed by the same file (e.g. every face of a `.ttc`)
    /// return the same `Arc<FontBytes>` — and therefore the same mmap
    /// — instead of each allocating their own buffer. We hold `Weak`
    /// references so the mmap unmap as soon as no parsed font holds
    /// it alive.
    pub(crate) shared_bytes: StLock<std::collections::HashMap<u64, std::sync::Weak<FontBytes>>>,
}

/// The actual font-pattern state, held behind the RwLock in
/// `FcFontCacheShared`. Private — all access goes through
/// `FcFontCache` methods which lock transparently.
#[derive(Default, Debug)]
pub(crate) struct FcFontCacheInner {
    /// Pattern to FontId mapping (query index)
    pub(crate) patterns: BTreeMap<FcPattern, FontId>,
    /// On-disk font paths
    pub(crate) disk_fonts: BTreeMap<FontId, FcFontPath>,
    /// In-memory fonts
    pub(crate) memory_fonts: BTreeMap<FontId, FcFont>,
    /// Metadata cache (patterns stored by ID for quick lookup)
    pub(crate) metadata: BTreeMap<FontId, FcPattern>,
    /// Token index: maps lowercase tokens ("noto", "sans", "jp") to sets of FontIds.
    /// Enables fast fuzzy search by intersecting token sets.
    pub(crate) token_index: BTreeMap<String, alloc::collections::BTreeSet<FontId>>,
    /// Pre-tokenized font names (lowercase): FontId -> Vec<lowercase tokens>.
    /// Avoids re-tokenization during fuzzy search.
    pub(crate) font_tokens: BTreeMap<FontId, Vec<String>>,
}

impl FcFontCacheInner {
    /// Add a font pattern to the token index. Called under the
    /// write lock by insertion paths.
    pub(crate) fn index_pattern_tokens(&mut self, _pattern: &FcPattern, _id: FontId) {
        // WEB-LIFT (2026-06-02): no-op on the azul web fork. The tokenizer
        // (`extract_font_name_tokens` char-classification + lowercasing) pulls unicode tables
        // whose jump-tables the remill/web lift leaves un-devirt'd → MISSING_BLOCK trap inside
        // `with_memory_fonts`. `token_index`/`font_tokens` feed ONLY the separate token-fuzzy
        // search path (query_fuzzy); the main `query`→`query_internal_locked` scores by
        // unicode-compatibility + style over the registered patterns/metadata (populated before
        // this call), so leaving the token index empty does not affect normal font matching.
    }
}

impl Clone for FcFontCache {
    /// Shallow clone — the returned handle shares the same underlying
    /// state as `self`. Writes through either are visible to both.
    /// This is the whole point of the v4.1 redesign; callers that need
    /// an isolated frozen copy must explicitly request one (e.g. via
    /// `snapshot_state`, which is intentionally not provided because
    /// we no longer have a use case for it).
    fn clone(&self) -> Self {
        Self {
            shared: std::sync::Arc::clone(&self.shared),
        }
    }
}

impl core::fmt::Debug for FcFontCache {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let state = self.state_read();
        f.debug_struct("FcFontCache")
            .field("patterns_len", &state.patterns.len())
            .field("metadata_len", &state.metadata.len())
            .field("disk_fonts_len", &state.disk_fonts.len())
            .field("memory_fonts_len", &state.memory_fonts.len())
            .finish()
    }
}

impl Default for FcFontCache {
    fn default() -> Self {
        Self {
            shared: std::sync::Arc::new(FcFontCacheShared {
                state: StLock::new(FcFontCacheInner::default()),
                chain_cache: StLock::new(std::collections::HashMap::new()),
                shared_bytes: StLock::new(std::collections::HashMap::new()),
            }),
        }
    }
}

impl FcFontCache {
    /// Acquire a read guard on the cache's state. Panics if the lock
    /// was poisoned by a panic inside the write guard — same
    /// contract as `RwLock::read().expect(..)`.
    #[inline]
    pub(crate) fn state_read(
        &self,
    ) -> StReadGuard<'_, FcFontCacheInner> {
        // [az-web-lift] StLock::read() is Infallible (never poisons/spins).
        match self.shared.state.read() {
            Ok(g) => g,
            Err(e) => match e {},
        }
    }

    /// Acquire a write guard on the cache's state. Panics on
    /// poisoning, same as `state_read`.
    #[inline]
    pub(crate) fn state_write(
        &self,
    ) -> StWriteGuard<'_, FcFontCacheInner> {
        // [az-web-lift] StLock::write() is Infallible (never poisons/spins).
        match self.shared.state.write() {
            Ok(g) => g,
            Err(e) => match e {},
        }
    }

    /// Adds in-memory font files.
    ///
    /// Note: takes `&self` — the shared cache handles interior
    /// mutability via the RwLock.
    pub fn with_memory_fonts(&self, fonts: Vec<(FcPattern, FcFont)>) -> &Self {
        // Auto-detect Unicode coverage for any naively-registered font
        // (empty `unicode_ranges`) BEFORE taking the write lock, so we don't
        // hold it across font parsing. See `populate_memory_font_ranges`.
        let fonts: Vec<(FcPattern, FcFont)> = fonts
            .into_iter()
            .map(|(pattern, font)| (Self::populate_memory_font_ranges(pattern, &font), font))
            .collect();
        let mut state = self.state_write();
        for (pattern, font) in fonts {
            let id = FontId::new();
            state.patterns.insert(pattern.clone(), id);
            state.metadata.insert(id, pattern.clone());
            state.memory_fonts.insert(id, font);
            state.index_pattern_tokens(&pattern, id);
        }
        self
    }

    /// Adds a memory font with a specific ID (for testing).
    pub fn with_memory_font_with_id(
        &self,
        id: FontId,
        pattern: FcPattern,
        font: FcFont,
    ) -> &Self {
        let pattern = Self::populate_memory_font_ranges(pattern, &font);
        let mut state = self.state_write();
        state.patterns.insert(pattern.clone(), id);
        state.metadata.insert(id, pattern.clone());
        state.memory_fonts.insert(id, font);
        state.index_pattern_tokens(&pattern, id);
        self
    }

    /// Fill in a memory font's `unicode_ranges` from its raw bytes when the
    /// caller left them empty.
    ///
    /// A normal caller of [`FcFontCache::with_memory_fonts`] just hands over
    /// a name and the font bytes — they don't hand-compute the cmap. But
    /// [`FontFallbackChain::resolve_char`] deliberately skips any font that
    /// reports *no* coverage (it refuses to assume a blank range list means
    /// "covers everything"). Without this step a naively-registered bundled
    /// font could never be selected for any character — the exact bug that
    /// bites headless / wasm / embedder-bundled-font setups.
    ///
    /// With the `parsing` feature we reuse the *same* OS/2 + cmap detection
    /// pipeline the on-disk builder uses (via [`FcParseFontBytes`] →
    /// `parse_font_faces`). Without `parsing` the pattern is returned
    /// unchanged and the caller must populate `unicode_ranges` themselves.
    #[cfg(all(feature = "std", feature = "parsing"))]
    fn populate_memory_font_ranges(mut pattern: FcPattern, font: &FcFont) -> FcPattern {
        if !pattern.unicode_ranges.is_empty() {
            return pattern;
        }
        if let Some(faces) = FcParseFontBytes(&font.bytes, &font.id) {
            // A `.ttc` yields several faces; pick the one matching this
            // font's index, else fall back to the first parsed face. All
            // patterns of a single face share the same `unicode_ranges`.
            let ranges = faces
                .iter()
                .find(|(_, f)| f.font_index == font.font_index)
                .or_else(|| faces.first())
                .map(|(p, _)| p.unicode_ranges.clone())
                .unwrap_or_default();
            if !ranges.is_empty() {
                pattern.unicode_ranges = ranges;
            }
        }
        pattern
    }

    /// Without the `parsing` feature there is no cmap/OS2 parser available,
    /// so the caller-provided pattern is stored verbatim.
    #[cfg(not(all(feature = "std", feature = "parsing")))]
    fn populate_memory_font_ranges(pattern: FcPattern, _font: &FcFont) -> FcPattern {
        pattern
    }

    /// Register a newly-parsed on-disk font. Called by the builder
    /// thread inside `FcFontRegistry`. Allocates a fresh `FontId`,
    /// inserts the pattern + path + metadata in one write lock, and
    /// invalidates the chain cache so subsequent resolutions pick
    /// up the new font.
    pub fn insert_builder_font(&self, pattern: FcPattern, path: FcFontPath) {
        let id = FontId::new();
        {
            let mut state = self.state_write();
            state.index_pattern_tokens(&pattern, id);
            state.patterns.insert(pattern.clone(), id);
            state.disk_fonts.insert(id, path);
            state.metadata.insert(id, pattern);
        }
        // Invalidate chain cache so callers see the new font on the
        // next resolve. Scoped after the state write to keep lock
        // nesting shallow.
        if let Ok(mut cc) = self.shared.chain_cache.lock() {
            cc.clear();
        }
    }

    #[cfg(feature = "std")]
    #[doc(hidden)]
    pub fn chain_cache_len(&self) -> usize {
        self.shared.chain_cache.lock().map(|c| c.len()).unwrap_or(0)
    }

    /// Insert a *fast-probed* pattern into the cache and return its
    /// fresh `FontId`. Used by [`FcFontRegistry::request_fonts_fast`]
    /// when a cmap probe discovers a font that covers some subset of
    /// the requested codepoints. Unlike [`insert_builder_font`] this
    /// does **not** populate the token index (we don't have NAME
    /// table data), so fuzzy-name lookups on fast-probed fonts fall
    /// through to the filename-guess in `known_paths`.
    pub fn insert_fast_pattern(&self, pattern: FcPattern, path: FcFontPath) -> FontId {
        let id = FontId::new();
        let mut state = self.state_write();
        state.patterns.insert(pattern.clone(), id);
        state.disk_fonts.insert(id, path);
        state.metadata.insert(id, pattern);
        id
    }

    /// Look up all `FontId`s whose `FcFontPath` matches `path`.
    /// Cheap way for `request_fonts_fast` to reuse fast-probed
    /// entries across layout passes without re-reading the cmap.
    ///
    /// O(n) over the disk_fonts map; fine for the typical case of
    /// <100 parsed fonts, and we skip the scan entirely when a
    /// stack's first candidate covers.
    pub fn lookup_paths_cached(&self, path: &str) -> Option<Vec<FontId>> {
        let state = self.state_read();
        let mut out = Vec::new();
        for (id, font_path) in &state.disk_fonts {
            if font_path.path == path {
                out.push(*id);
            }
        }
        if out.is_empty() { None } else { Some(out) }
    }

    /// Get font data for a given font ID.
    ///
    /// Returns owned values (not references) because the underlying
    /// maps live behind an RwLock — a reference could not outlive
    /// the read guard. In-memory fonts come back as cloned `FcFont`
    /// instances; disk fonts return their `FcFontPath`.
    pub fn get_font_by_id(&self, id: &FontId) -> Option<OwnedFontSource> {
        let state = self.state_read();
        if let Some(font) = state.memory_fonts.get(id) {
            return Some(OwnedFontSource::Memory(font.clone()));
        }
        if let Some(path) = state.disk_fonts.get(id) {
            return Some(OwnedFontSource::Disk(path.clone()));
        }
        None
    }

    /// Get metadata for a font ID. Returns an owned `FcPattern`
    /// (cloned out of the shared map) because we can't return a
    /// reference across the RwLock boundary.
    pub fn get_metadata_by_id(&self, id: &FontId) -> Option<FcPattern> {
        self.state_read().metadata.get(id).cloned()
    }

    /// Get the font bytes for `id` as a shared [`FontBytes`].
    ///
    /// On disk the returned `Arc<FontBytes>` wraps an mmap of the file
    /// (`FontBytes::Mmapped`). Untouched pages of the file never count
    /// toward the process's RSS — for a font where layout shapes only
    /// a handful of glyphs, this is the difference between paying for
    /// the whole 4 MiB `.ttc` and paying for the cmap + a few glyf
    /// pages.
    ///
    /// In-memory fonts (`FontSource::Memory`) come back as
    /// `FontBytes::Owned`, since the bytes are already on the heap.
    ///
    /// Multiple `FontId`s backed by the same file content (every face
    /// of a `.ttc`, or two paths with identical bytes) return the
    /// *same* `Arc<FontBytes>` thanks to a content-hash → `Weak`
    /// cache. Bytes get unmapped automatically when the last consumer
    /// drops the Arc.
    ///
    /// `FontBytes` derefs to `[u8]`, so callers that only need
    /// `&[u8]` (allsorts, ttf-parser, …) can pass it through without
    /// thinking about the backing.
    ///
    /// Failure modes: returns `None` if the path is unknown, or the
    /// file no longer exists / cannot be opened, or the mmap call
    /// fails. Callers may retry with a fresh `get_font_bytes` if they
    /// suspect the file was replaced underneath them; the next call
    /// re-opens cleanly.
    #[cfg(feature = "std")]
    pub fn get_font_bytes(&self, id: &FontId) -> Option<std::sync::Arc<FontBytes>> {
        use std::sync::Arc;
        match self.get_font_by_id(id)? {
            OwnedFontSource::Memory(font) => Some(Arc::new(FontBytes::Owned(
                Arc::from(font.bytes.as_slice()),
            ))),
            OwnedFontSource::Disk(path) => {
                let hash = path.bytes_hash;
                if hash != 0 {
                    if let Ok(guard) = self.shared.shared_bytes.lock() {
                        if let Some(weak) = guard.get(&hash) {
                            if let Some(arc) = weak.upgrade() {
                                return Some(arc);
                            }
                        }
                    }
                }

                let arc = open_font_bytes_mmap(&path.path)?;
                if hash != 0 {
                    if let Ok(mut guard) = self.shared.shared_bytes.lock() {
                        // Overwrite any stale weak ref that failed to upgrade.
                        guard.insert(hash, Arc::downgrade(&arc));
                    }
                }
                Some(arc)
            }
        }
    }

    /// Returns an empty font cache (no_std / no filesystem).
    #[cfg(not(feature = "std"))]
    pub fn build() -> Self { Self::default() }

    /// Scans system font directories using filename heuristics (no allsorts).
    #[cfg(all(feature = "std", not(feature = "parsing")))]
    pub fn build() -> Self { Self::build_from_filenames() }

    /// Scans and parses all system fonts via allsorts for full metadata.
    #[cfg(all(feature = "std", feature = "parsing"))]
    pub fn build() -> Self { Self::build_inner(None) }

    /// Filename-only scan: discovers fonts on disk, guesses metadata from
    /// the filename using [`config::tokenize_font_stem`].
    #[cfg(all(feature = "std", not(feature = "parsing")))]
    fn build_from_filenames() -> Self {
        let cache = Self::default();
        {
            let mut state = cache.state_write();
            for dir in crate::config::font_directories(OperatingSystem::current()) {
                for path in FcCollectFontFilesRecursive(dir) {
                    let pattern = match pattern_from_filename(&path) {
                        Some(p) => p,
                        None => continue,
                    };
                    let id = FontId::new();
                    state.disk_fonts.insert(id, FcFontPath {
                        path: path.to_string_lossy().to_string(),
                        font_index: 0,
                        // Filename-only scan — we never read the bytes,
                        // so there's no dedup key. Leave as 0.
                        bytes_hash: 0,
                    });
                    state.index_pattern_tokens(&pattern, id);
                    state.metadata.insert(id, pattern.clone());
                    state.patterns.insert(pattern, id);
                }
            }
        }
        cache
    }
    
    /// Builds a font cache with only specific font families (and their fallbacks).
    /// 
    /// This is a performance optimization for applications that know ahead of time
    /// which fonts they need. Instead of scanning all system fonts (which can be slow
    /// on systems with many fonts), only fonts matching the specified families are loaded.
    /// 
    /// Generic family names like "sans-serif", "serif", "monospace" are expanded
    /// to OS-specific font names (e.g., "sans-serif" on macOS becomes "Helvetica Neue", 
    /// "San Francisco", etc.).
    /// 
    /// **Note**: This will NOT automatically load fallback fonts for scripts not covered
    /// by the requested families. If you need Arabic, CJK, or emoji support, either:
    /// - Add those families explicitly to the filter
    /// - Use `with_memory_fonts()` to add bundled fonts
    /// - Use `build()` to load all system fonts
    /// 
    /// # Arguments
    /// * `families` - Font family names to load (e.g., ["Arial", "sans-serif"])
    /// 
    /// # Example
    /// ```ignore
    /// // Only load Arial and sans-serif fallback fonts
    /// let cache = FcFontCache::build_with_families(&["Arial", "sans-serif"]);
    /// ```
    #[cfg(all(feature = "std", feature = "parsing"))]
    pub fn build_with_families(families: &[impl AsRef<str>]) -> Self {
        // Expand generic families to OS-specific names
        let os = OperatingSystem::current();
        let mut target_families: Vec<String> = Vec::new();
        
        for family in families {
            let family_str = family.as_ref();
            let expanded = os.expand_generic_family(family_str, &[]);
            if expanded.is_empty() || (expanded.len() == 1 && expanded[0] == family_str) {
                target_families.push(family_str.to_string());
            } else {
                target_families.extend(expanded);
            }
        }
        
        Self::build_inner(Some(&target_families))
    }
    
    /// Inner build function that handles both filtered and unfiltered font loading.
    /// 
    /// # Arguments
    /// * `family_filter` - If Some, only load fonts matching these family names.
    ///                     If None, load all fonts.
    #[cfg(all(feature = "std", feature = "parsing"))]
    fn build_inner(family_filter: Option<&[String]>) -> Self {
        let cache = FcFontCache::default();

        // Normalize filter families for matching
        let filter_normalized: Option<Vec<String>> = family_filter.map(|families| {
            families
                .iter()
                .map(|f| crate::utils::normalize_family_name(f))
                .collect()
        });

        // Helper closure to check if a pattern matches the filter
        let matches_filter = |pattern: &FcPattern| -> bool {
            match &filter_normalized {
                None => true, // No filter = accept all
                Some(targets) => {
                    pattern.name.as_ref().map_or(false, |name| {
                        let name_norm = crate::utils::normalize_family_name(name);
                        targets.iter().any(|target| name_norm.contains(target))
                    }) || pattern.family.as_ref().map_or(false, |family| {
                        let family_norm = crate::utils::normalize_family_name(family);
                        targets.iter().any(|target| family_norm.contains(target))
                    })
                }
            }
        };

        let mut state = cache.state_write();

        #[cfg(target_os = "linux")]
        {
            if let Some((font_entries, render_configs)) = FcScanDirectories() {
                for (mut pattern, path) in font_entries {
                    if matches_filter(&pattern) {
                        // Apply per-font render config if a matching family rule exists
                        if let Some(family) = pattern.name.as_ref().or(pattern.family.as_ref()) {
                            if let Some(rc) = render_configs.get(family) {
                                pattern.render_config = rc.clone();
                            }
                        }
                        let id = FontId::new();
                        state.patterns.insert(pattern.clone(), id);
                        state.metadata.insert(id, pattern.clone());
                        state.disk_fonts.insert(id, path);
                        state.index_pattern_tokens(&pattern, id);
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            let system_root = std::env::var("SystemRoot")
                .or_else(|_| std::env::var("WINDIR"))
                .unwrap_or_else(|_| "C:\\Windows".to_string());

            let user_profile = std::env::var("USERPROFILE")
                .unwrap_or_else(|_| "C:\\Users\\Default".to_string());

            let font_dirs = vec![
                (None, format!("{}\\Fonts\\", system_root)),
                (None, format!("{}\\AppData\\Local\\Microsoft\\Windows\\Fonts\\", user_profile)),
            ];

            let font_entries = FcScanDirectoriesInner(&font_dirs);
            for (pattern, path) in font_entries {
                if matches_filter(&pattern) {
                    let id = FontId::new();
                    state.patterns.insert(pattern.clone(), id);
                    state.metadata.insert(id, pattern.clone());
                    state.disk_fonts.insert(id, path);
                    state.index_pattern_tokens(&pattern, id);
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            let font_dirs = vec![
                (None, "~/Library/Fonts".to_owned()),
                (None, "/System/Library/Fonts".to_owned()),
                (None, "/Library/Fonts".to_owned()),
                (None, "/System/Library/AssetsV2".to_owned()),
            ];

            let font_entries = FcScanDirectoriesInner(&font_dirs);
            for (pattern, path) in font_entries {
                if matches_filter(&pattern) {
                    let id = FontId::new();
                    state.patterns.insert(pattern.clone(), id);
                    state.metadata.insert(id, pattern.clone());
                    state.disk_fonts.insert(id, path);
                    state.index_pattern_tokens(&pattern, id);
                }
            }
        }

        // iOS: the app sandbox denies a plain `read_dir` on `/System/Library/...`,
        // but `CTFontManagerCopyAvailableFontURLs` returns sandbox-mediated
        // `CFURL`s that *are* openable. We enumerate via CoreText, then feed
        // each URL into the same `FcParseFont` path the desktop arms use.
        #[cfg(target_os = "ios")]
        {
            let font_files = crate::mobile_ios::copy_available_font_urls();
            let font_entries = FcParseFontFiles(&font_files);
            for (pattern, path) in font_entries {
                if matches_filter(&pattern) {
                    let id = FontId::new();
                    state.patterns.insert(pattern.clone(), id);
                    state.metadata.insert(id, pattern.clone());
                    state.disk_fonts.insert(id, path);
                    state.index_pattern_tokens(&pattern, id);
                }
            }
        }

        // Android: system fonts live at world-readable paths. Vendor partitions
        // (`/product/fonts`, `/system_ext/fonts`) carry OEM-specific families
        // on Samsung One UI / MIUI / EMUI; `/data/fonts` is the per-user font
        // dir on recent ROMs.
        #[cfg(target_os = "android")]
        {
            let font_dirs = vec![
                (None, "/system/fonts".to_owned()),
                (None, "/product/fonts".to_owned()),
                (None, "/system_ext/fonts".to_owned()),
                (None, "/data/fonts".to_owned()),
            ];

            let font_entries = FcScanDirectoriesInner(&font_dirs);
            for (pattern, path) in font_entries {
                if matches_filter(&pattern) {
                    let id = FontId::new();
                    state.patterns.insert(pattern.clone(), id);
                    state.metadata.insert(id, pattern.clone());
                    state.disk_fonts.insert(id, path);
                    state.index_pattern_tokens(&pattern, id);
                }
            }
        }

        drop(state);
        cache
    }
    
    /// Check if a font ID is a memory font (preferred over disk fonts)
    pub fn is_memory_font(&self, id: &FontId) -> bool {
        self.state_read().memory_fonts.contains_key(id)
    }

    /// Returns the list of fonts and font patterns.
    ///
    /// Returns owned `FcPattern` values (cloned out of the shared
    /// state) — this is the v4.1 API change described on
    /// [`FcFontCache`]. Callers that need to iterate without
    /// cloning should use [`FcFontCache::for_each_pattern`].
    pub fn list(&self) -> Vec<(FcPattern, FontId)> {
        self.state_read()
            .patterns
            .iter()
            .map(|(pattern, id)| (pattern.clone(), *id))
            .collect()
    }

    /// Iterate over every `(pattern, id)` pair under a single read
    /// guard. `f` is called once per entry — avoids the per-entry
    /// clone that [`list`] incurs.
    pub fn for_each_pattern<F: FnMut(&FcPattern, &FontId)>(&self, mut f: F) {
        let state = self.state_read();
        for (pattern, id) in &state.patterns {
            f(pattern, id);
        }
    }

    /// Returns true if the cache contains no font patterns
    pub fn is_empty(&self) -> bool {
        self.state_read().patterns.is_empty()
    }

    /// Returns the number of font patterns in the cache
    pub fn len(&self) -> usize {
        self.state_read().patterns.len()
    }

    /// Queries a font from the in-memory cache, returns the first found font (early return)
    /// Memory fonts are always preferred over disk fonts with the same match quality.
    pub fn query(&self, pattern: &FcPattern, trace: &mut Vec<TraceMsg>) -> Option<FontMatch> {
        let state = self.state_read();
        let mut matches = Vec::new();

        for (stored_pattern, id) in &state.patterns {
            if Self::query_matches_internal(stored_pattern, pattern, trace) {
                let metadata = state.metadata.get(id).unwrap_or(stored_pattern);

                // Calculate Unicode compatibility score
                let unicode_compatibility = if pattern.unicode_ranges.is_empty() {
                    // No specific Unicode requirements, use general coverage
                    Self::calculate_unicode_coverage(&metadata.unicode_ranges) as i32
                } else {
                    // Calculate how well this font covers the requested Unicode ranges
                    Self::calculate_unicode_compatibility(&pattern.unicode_ranges, &metadata.unicode_ranges)
                };

                let style_score = Self::calculate_style_score(pattern, metadata);

                // Memory fonts get a bonus to prefer them over disk fonts
                let is_memory = state.memory_fonts.contains_key(id);

                matches.push((*id, unicode_compatibility, style_score, metadata.clone(), is_memory));
            }
        }

        // Sort by: 1. Memory font (preferred), 2. Unicode compatibility, 3. Style score
        matches.sort_by(|a, b| {
            // Memory fonts first
            b.4.cmp(&a.4)
                .then_with(|| b.1.cmp(&a.1)) // Unicode compatibility (higher is better)
                .then_with(|| a.2.cmp(&b.2)) // Style score (lower is better)
        });

        matches.first().map(|(id, _, _, metadata, _)| {
            FontMatch {
                id: *id,
                unicode_ranges: metadata.unicode_ranges.clone(),
                fallbacks: Vec::new(), // Fallbacks computed lazily via compute_fallbacks()
            }
        })
    }

    /// Queries all fonts matching a pattern (internal use only).
    ///
    /// Note: This function is now private. Use resolve_font_chain() to build a font fallback chain,
    /// then call FontFallbackChain::query_for_text() to resolve fonts for specific text.
    fn query_internal(&self, pattern: &FcPattern, trace: &mut Vec<TraceMsg>) -> Vec<FontMatch> {
        let state = self.state_read();
        self.query_internal_locked(&state, pattern, trace)
    }

    /// Internal variant used when the caller already holds a read
    /// guard on the state. Avoids re-locking.
    fn query_internal_locked(
        &self,
        state: &FcFontCacheInner,
        pattern: &FcPattern,
        trace: &mut Vec<TraceMsg>,
    ) -> Vec<FontMatch> {
        let mut matches = Vec::new();

        for (stored_pattern, id) in &state.patterns {
            if Self::query_matches_internal(stored_pattern, pattern, trace) {
                let metadata = state.metadata.get(id).unwrap_or(stored_pattern);

                // Calculate Unicode compatibility score
                let unicode_compatibility = if pattern.unicode_ranges.is_empty() {
                    Self::calculate_unicode_coverage(&metadata.unicode_ranges) as i32
                } else {
                    Self::calculate_unicode_compatibility(&pattern.unicode_ranges, &metadata.unicode_ranges)
                };

                let style_score = Self::calculate_style_score(pattern, metadata);
                matches.push((*id, unicode_compatibility, style_score, metadata.clone()));
            }
        }

        // Sort by style score (lowest first), THEN by Unicode compatibility (highest first)
        // Style matching (weight, italic, etc.) is now the primary criterion
        // Deterministic tiebreaker: prefer non-italic, then alphabetical by name
        matches.sort_by(|a, b| {
            a.2.cmp(&b.2) // Style score (lower is better)
                .then_with(|| b.1.cmp(&a.1)) // Unicode compatibility (higher is better)
                .then_with(|| a.3.italic.cmp(&b.3.italic)) // Prefer non-italic
                .then_with(|| a.3.name.cmp(&b.3.name)) // Alphabetical tiebreaker
        });

        matches
            .into_iter()
            .map(|(id, _, _, metadata)| {
                FontMatch {
                    id,
                    unicode_ranges: metadata.unicode_ranges.clone(),
                    fallbacks: Vec::new(), // Fallbacks computed lazily via compute_fallbacks()
                }
            })
            .collect()
    }

    /// Compute fallback fonts for a given font
    /// This is a lazy operation that can be expensive - only call when actually needed
    /// (e.g., for FFI or debugging, not needed for resolve_char)
    pub fn compute_fallbacks(
        &self,
        font_id: &FontId,
        trace: &mut Vec<TraceMsg>,
    ) -> Vec<FontMatchNoFallback> {
        let state = self.state_read();
        let pattern = match state.metadata.get(font_id) {
            Some(p) => p.clone(),
            None => return Vec::new(),
        };
        drop(state);

        self.compute_fallbacks_for_pattern(&pattern, Some(font_id), trace)
    }

    fn compute_fallbacks_for_pattern(
        &self,
        pattern: &FcPattern,
        exclude_id: Option<&FontId>,
        _trace: &mut Vec<TraceMsg>,
    ) -> Vec<FontMatchNoFallback> {
        let state = self.state_read();
        let mut candidates = Vec::new();

        // Collect all potential fallbacks (excluding original pattern)
        for (stored_pattern, id) in &state.patterns {
            // Skip if this is the original font
            if exclude_id.is_some() && exclude_id.unwrap() == id {
                continue;
            }

            // Check if this font supports any of the unicode ranges
            if !stored_pattern.unicode_ranges.is_empty() && !pattern.unicode_ranges.is_empty() {
                // Calculate Unicode compatibility
                let unicode_compatibility = Self::calculate_unicode_compatibility(
                    &pattern.unicode_ranges,
                    &stored_pattern.unicode_ranges
                );

                // Only include if there's actual overlap
                if unicode_compatibility > 0 {
                    let style_score = Self::calculate_style_score(pattern, stored_pattern);
                    candidates.push((
                        FontMatchNoFallback {
                            id: *id,
                            unicode_ranges: stored_pattern.unicode_ranges.clone(),
                        },
                        unicode_compatibility,
                        style_score,
                        stored_pattern.clone(),
                    ));
                }
            } else if pattern.unicode_ranges.is_empty() && !stored_pattern.unicode_ranges.is_empty() {
                // No specific Unicode requirements, use general coverage
                let coverage = Self::calculate_unicode_coverage(&stored_pattern.unicode_ranges) as i32;
                let style_score = Self::calculate_style_score(pattern, stored_pattern);
                candidates.push((
                    FontMatchNoFallback {
                        id: *id,
                        unicode_ranges: stored_pattern.unicode_ranges.clone(),
                    },
                    coverage,
                    style_score,
                    stored_pattern.clone(),
                ));
            }
        }

        drop(state);

        // Sort by Unicode compatibility (highest first), THEN by style score (lowest first)
        candidates.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| a.2.cmp(&b.2))
        });

        // Deduplicate by keeping only the best match per unique unicode range
        let mut seen_ranges = Vec::new();
        let mut deduplicated = Vec::new();

        for (id, _, _, pattern) in candidates {
            let mut is_new_range = false;

            for range in &pattern.unicode_ranges {
                if !seen_ranges.iter().any(|r: &UnicodeRange| r.overlaps(range)) {
                    seen_ranges.push(*range);
                    is_new_range = true;
                }
            }

            if is_new_range {
                deduplicated.push(id);
            }
        }

        deduplicated
    }

    /// Get in-memory font data (cloned out of the shared state).
    pub fn get_memory_font(&self, id: &FontId) -> Option<FcFont> {
        self.state_read().memory_fonts.get(id).cloned()
    }

    /// Check if a pattern matches the query, with detailed tracing
    fn trace_path(k: &FcPattern) -> String {
        k.name.as_ref().cloned().unwrap_or_else(|| "<unknown>".to_string())
    }

    pub fn query_matches_internal(
        k: &FcPattern,
        pattern: &FcPattern,
        trace: &mut Vec<TraceMsg>,
    ) -> bool {
        // Check name - substring match
        if let Some(ref name) = pattern.name {
            if !k.name.as_ref().map_or(false, |kn| kn.contains(name)) {
                trace.push(TraceMsg {
                    level: TraceLevel::Info,
                    path: Self::trace_path(k),
                    reason: MatchReason::NameMismatch {
                        requested: pattern.name.clone(),
                        found: k.name.clone(),
                    },
                });
                return false;
            }
        }

        // Check family - substring match
        if let Some(ref family) = pattern.family {
            if !k.family.as_ref().map_or(false, |kf| kf.contains(family)) {
                trace.push(TraceMsg {
                    level: TraceLevel::Info,
                    path: Self::trace_path(k),
                    reason: MatchReason::FamilyMismatch {
                        requested: pattern.family.clone(),
                        found: k.family.clone(),
                    },
                });
                return false;
            }
        }

        // Check style properties
        let style_properties = [
            (
                "italic",
                pattern.italic.needs_to_match(),
                pattern.italic.matches(&k.italic),
            ),
            (
                "oblique",
                pattern.oblique.needs_to_match(),
                pattern.oblique.matches(&k.oblique),
            ),
            (
                "bold",
                pattern.bold.needs_to_match(),
                pattern.bold.matches(&k.bold),
            ),
            (
                "monospace",
                pattern.monospace.needs_to_match(),
                pattern.monospace.matches(&k.monospace),
            ),
            (
                "condensed",
                pattern.condensed.needs_to_match(),
                pattern.condensed.matches(&k.condensed),
            ),
        ];

        for (property_name, needs_to_match, matches) in style_properties {
            if needs_to_match && !matches {
                let (requested, found) = match property_name {
                    "italic" => (format!("{:?}", pattern.italic), format!("{:?}", k.italic)),
                    "oblique" => (format!("{:?}", pattern.oblique), format!("{:?}", k.oblique)),
                    "bold" => (format!("{:?}", pattern.bold), format!("{:?}", k.bold)),
                    "monospace" => (
                        format!("{:?}", pattern.monospace),
                        format!("{:?}", k.monospace),
                    ),
                    "condensed" => (
                        format!("{:?}", pattern.condensed),
                        format!("{:?}", k.condensed),
                    ),
                    _ => (String::new(), String::new()),
                };

                trace.push(TraceMsg {
                    level: TraceLevel::Info,
                    path: Self::trace_path(k),
                    reason: MatchReason::StyleMismatch {
                        property: property_name,
                        requested,
                        found,
                    },
                });
                return false;
            }
        }

        // Check weight - hard filter if non-normal weight is requested
        if pattern.weight != FcWeight::Normal && pattern.weight != k.weight {
            trace.push(TraceMsg {
                level: TraceLevel::Info,
                path: Self::trace_path(k),
                reason: MatchReason::WeightMismatch {
                    requested: pattern.weight,
                    found: k.weight,
                },
            });
            return false;
        }

        // Check stretch - hard filter if non-normal stretch is requested
        if pattern.stretch != FcStretch::Normal && pattern.stretch != k.stretch {
            trace.push(TraceMsg {
                level: TraceLevel::Info,
                path: Self::trace_path(k),
                reason: MatchReason::StretchMismatch {
                    requested: pattern.stretch,
                    found: k.stretch,
                },
            });
            return false;
        }

        // Check unicode ranges if specified
        if !pattern.unicode_ranges.is_empty() {
            let mut has_overlap = false;

            for p_range in &pattern.unicode_ranges {
                for k_range in &k.unicode_ranges {
                    if p_range.overlaps(k_range) {
                        has_overlap = true;
                        break;
                    }
                }
                if has_overlap {
                    break;
                }
            }

            if !has_overlap {
                trace.push(TraceMsg {
                    level: TraceLevel::Info,
                    path: Self::trace_path(k),
                    reason: MatchReason::UnicodeRangeMismatch {
                        character: '\0', // No specific character to report
                        ranges: k.unicode_ranges.clone(),
                    },
                });
                return false;
            }
        }

        true
    }
    
    /// Resolve a complete font fallback chain for a CSS font-family stack
    /// This is the main entry point for font resolution with caching
    /// Automatically expands generic CSS families (serif, sans-serif, monospace) to OS-specific fonts
    /// 
    /// # Arguments
    /// * `font_families` - CSS font-family stack (e.g., ["Arial", "sans-serif"])
    /// * `text` - The text to render (used to extract Unicode ranges)
    /// * `weight` - Font weight
    /// * `italic` - Italic style requirement
    /// * `oblique` - Oblique style requirement
    /// * `trace` - Debug trace messages
    /// 
    /// # Returns
    /// A complete font fallback chain with CSS fallbacks and Unicode fallbacks
    /// 
    /// # Example
    /// ```no_run
    /// # use rust_fontconfig::{FcFontCache, FcWeight, PatternMatch};
    /// let cache = FcFontCache::build();
    /// let families = vec!["Arial".to_string(), "sans-serif".to_string()];
    /// let chain = cache.resolve_font_chain(&families, FcWeight::Normal, 
    ///                                       PatternMatch::DontCare, PatternMatch::DontCare, 
    ///                                       &mut Vec::new());
    /// // On macOS: families expanded to ["Arial", "San Francisco", "Helvetica Neue", "Lucida Grande"]
    /// ```
    #[cfg(feature = "std")]
    pub fn resolve_font_chain(
        &self,
        font_families: &[String],
        weight: FcWeight,
        italic: PatternMatch,
        oblique: PatternMatch,
        trace: &mut Vec<TraceMsg>,
    ) -> FontFallbackChain {
        self.resolve_font_chain_with_os(font_families, weight, italic, oblique, trace, OperatingSystem::current())
    }
    
    /// Resolve font chain with explicit OS specification (useful for testing)
    #[cfg(feature = "std")]
    pub fn resolve_font_chain_with_os(
        &self,
        font_families: &[String],
        weight: FcWeight,
        italic: PatternMatch,
        oblique: PatternMatch,
        trace: &mut Vec<TraceMsg>,
        os: OperatingSystem,
    ) -> FontFallbackChain {
        self.resolve_font_chain_impl(font_families, weight, italic, oblique, None, trace, os)
    }

    /// Resolve a font fallback chain, restricting Unicode fallbacks to the
    /// caller-supplied set of scripts (usually derived from the actual
    /// text content of the document).
    ///
    /// - `scripts_hint: None` → back-compat behaviour, equivalent to
    ///   [`FcFontCache::resolve_font_chain`]: pulls in fallback fonts for
    ///   the full [`DEFAULT_UNICODE_FALLBACK_SCRIPTS`] set.
    /// - `scripts_hint: Some(&[])` → no Unicode fallbacks attached. For
    ///   an ASCII-only page this avoids pulling Arial Unicode MS,
    ///   CJK fonts, etc. into memory when they're not needed.
    /// - `scripts_hint: Some(&[CJK])` → only CJK fallback attached.
    ///
    /// The chain cache is keyed so an ASCII-only resolution cannot be
    /// served from a slot populated by a default/all-scripts resolution.
    #[cfg(feature = "std")]
    pub fn resolve_font_chain_with_scripts(
        &self,
        font_families: &[String],
        weight: FcWeight,
        italic: PatternMatch,
        oblique: PatternMatch,
        scripts_hint: Option<&[UnicodeRange]>,
        trace: &mut Vec<TraceMsg>,
    ) -> FontFallbackChain {
        self.resolve_font_chain_impl(
            font_families, weight, italic, oblique, scripts_hint,
            trace, OperatingSystem::current(),
        )
    }

    /// Shared entry used by [`resolve_font_chain_with_os`] and
    /// [`resolve_font_chain_with_scripts`]. Handles the cache lookup,
    /// generic-family expansion, and delegation to the uncached builder.
    #[cfg(feature = "std")]
    fn resolve_font_chain_impl(
        &self,
        font_families: &[String],
        weight: FcWeight,
        italic: PatternMatch,
        oblique: PatternMatch,
        scripts_hint: Option<&[UnicodeRange]>,
        trace: &mut Vec<TraceMsg>,
        os: OperatingSystem,
    ) -> FontFallbackChain {
        // Check cache FIRST - key uses original (unexpanded) families
        // plus a hash over the scripts_hint so ASCII-only callers don't
        // consume a slot filled by a default-scripts caller.
        let scripts_hint_hash = scripts_hint.map(hash_scripts_hint);
        let cache_key = FontChainCacheKey {
            font_families: font_families.to_vec(),
            weight,
            italic,
            oblique,
            scripts_hint_hash,
        };

        if let Some(cached) = self
            .shared
            .chain_cache
            .lock()
            .ok()
            .and_then(|c| c.get(&cache_key).cloned())
        {
            return cached;
        }

        // Expand generic CSS families to OS-specific fonts
        let expanded_families = expand_font_families(font_families, os, &[]);

        // Keep the originally-requested generic families ("serif",
        // "sans-serif", "monospace", ...) around. The expansion above turns
        // them into a hardcoded list of real OS font names and drops the
        // generic name itself; the chain builder uses this list to fall back
        // to *registered* fonts when none of those OS names exist (wasm,
        // headless caches, or an embedder that only registered an in-memory
        // bundled font). See `resolve_font_chain_uncached`.
        let generic_fallbacks: Vec<String> = font_families
            .iter()
            .filter(|f| config::is_generic_family(f))
            .cloned()
            .collect();

        // Build the chain
        let chain = self.resolve_font_chain_uncached(
            &expanded_families,
            &generic_fallbacks,
            weight,
            italic,
            oblique,
            scripts_hint,
            trace,
        );

        if let Ok(mut cache) = self.shared.chain_cache.lock() {
            cache.insert(cache_key, chain.clone());
        }

        chain
    }
    
    /// Internal implementation without caching.
    ///
    /// `scripts_hint`:
    /// - `None` pulls in the full [`DEFAULT_UNICODE_FALLBACK_SCRIPTS`]
    ///   set (the original, back-compat behaviour).
    /// - `Some(&[])` attaches no Unicode fallbacks.
    /// - `Some(ranges)` attaches fallbacks only for those ranges.
    #[cfg(feature = "std")]
    fn resolve_font_chain_uncached(
        &self,
        font_families: &[String],
        generic_fallbacks: &[String],
        weight: FcWeight,
        italic: PatternMatch,
        oblique: PatternMatch,
        scripts_hint: Option<&[UnicodeRange]>,
        trace: &mut Vec<TraceMsg>,
    ) -> FontFallbackChain {
        let mut css_fallbacks = Vec::new();
        
        // Resolve each CSS font-family to its system fallbacks
        for (_i, family) in font_families.iter().enumerate() {
            // Check if this is a generic font family
            let (pattern, is_generic) = if config::is_generic_family(family) {
                let monospace = if family.eq_ignore_ascii_case("monospace") {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                };
                let pattern = FcPattern {
                    name: None,
                    weight,
                    italic,
                    oblique,
                    monospace,
                    unicode_ranges: Vec::new(),
                    ..Default::default()
                };
                (pattern, true)
            } else {
                // Specific font family name
                let pattern = FcPattern {
                    name: Some(family.clone()),
                    weight,
                    italic,
                    oblique,
                    unicode_ranges: Vec::new(),
                    ..Default::default()
                };
                (pattern, false)
            };
            
            // Use fuzzy matching for specific fonts (fast token-based lookup)
            // For generic families, use query (slower but necessary for property matching)
            let mut matches = if is_generic {
                // Generic families need full pattern matching
                self.query_internal(&pattern, trace)
            } else {
                // Specific font names: use fast token-based fuzzy matching
                self.fuzzy_query_by_name(family, weight, italic, oblique, &[], trace)
            };
            
            // For generic families, limit to top 5 fonts to avoid too many matches
            if is_generic && matches.len() > 5 {
                matches.truncate(5);
            }
            
            // Always add the CSS fallback group to preserve CSS ordering
            // even if no fonts were found for this family
            css_fallbacks.push(CssFallbackGroup {
                css_name: family.clone(),
                fonts: matches,
            });
        }

        // Headless / wasm / memory-only fallback.
        //
        // Generic CSS families ("serif"/"sans-serif"/"monospace"/...) were
        // expanded by the caller to a hardcoded list of real OS font names.
        // On a system that actually has those fonts the loop above matched
        // them and we're done. But on wasm, a headless cache, or an embedder
        // that only registered an in-memory bundled font, NONE of those OS
        // names exist — and the original generic name was dropped, so a
        // registered font (whatever its family name) would never be reached.
        //
        // So: if the whole expanded stack matched nothing at all, retry each
        // originally-requested generic family as a generic `name: None`
        // query, which any registered font can satisfy. This runs ONLY when
        // nothing else matched, so on systems with real fonts it adds nothing
        // and never reorders real matches (any such fallback must come AFTER
        // real matches).
        if !generic_fallbacks.is_empty()
            && css_fallbacks.iter().all(|g| g.fonts.is_empty())
        {
            for generic in generic_fallbacks {
                let monospace = if generic.eq_ignore_ascii_case("monospace") {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                };
                let pattern = FcPattern {
                    name: None,
                    weight,
                    italic,
                    oblique,
                    monospace,
                    unicode_ranges: Vec::new(),
                    ..Default::default()
                };
                let mut matches = self.query_internal(&pattern, trace);
                if matches.len() > 5 {
                    matches.truncate(5);
                }
                if !matches.is_empty() {
                    css_fallbacks.push(CssFallbackGroup {
                        css_name: generic.clone(),
                        fonts: matches,
                    });
                }
            }
        }

        // Populate unicode_fallbacks. CSS fallback fonts may falsely claim
        // coverage of a script via the OS/2 unicode-range bits without
        // actually having glyphs, so we supplement the CSS chain with an
        // explicit lookup for each requested script block. resolve_char()
        // prefers CSS fallbacks first (earlier in the chain wins).
        //
        // The set of script blocks to cover is caller-controlled via
        // `scripts_hint`: `None` keeps the back-compat DEFAULT_UNICODE_FALLBACK_SCRIPTS
        // behaviour (7 scripts) so existing `resolve_font_chain` consumers
        // stay unchanged; `Some(&[])` opts into "no unicode fallbacks at all"
        // for ASCII-only documents, eliminating the big CJK / Arabic fonts
        // from the resolved chain (and therefore from eager downstream parses).
        let important_ranges: &[UnicodeRange] =
            scripts_hint.unwrap_or(DEFAULT_UNICODE_FALLBACK_SCRIPTS);
        let unicode_fallbacks = if important_ranges.is_empty() {
            Vec::new()
        } else {
            let all_uncovered = vec![false; important_ranges.len()];
            self.find_unicode_fallbacks(
                important_ranges,
                &all_uncovered,
                &css_fallbacks,
                weight,
                italic,
                oblique,
                trace,
            )
        };

        // WEB-LIFT LAST-RESORT (2026-06-03; the `with_memory_fonts` trap that previously made
        // editing this file fatal is now fixed by the byte-atomic remill fork support). In the
        // lifted web backend `find_unicode_fallbacks` returns 0 fonts even though one IS
        // registered (the matching/iteration mis-lifts), so BOTH chain lists come back empty →
        // every consumer (resolve_char, query_for_text, prune_chain_to_used_chars) sees no font
        // → the layout unwraps a None → OOB. When the chain would be empty, append the first
        // registered font so the chain is non-empty. Native chains are never empty here.
        let mut unicode_fallbacks = unicode_fallbacks;
        if css_fallbacks.is_empty() && unicode_fallbacks.is_empty() {
            let st = self.state_read();
            if let Some((pat, id)) = st.patterns.iter().next() {
                unicode_fallbacks.push(FontMatch {
                    id: *id,
                    unicode_ranges: pat.unicode_ranges.clone(),
                    fallbacks: Vec::new(),
                });
            }
        }

        FontFallbackChain {
            css_fallbacks,
            unicode_fallbacks,
            original_stack: font_families.to_vec(),
        }
    }

    /// Extract Unicode ranges from text
    #[allow(dead_code)]
    fn extract_unicode_ranges(text: &str) -> Vec<UnicodeRange> {
        let mut chars: Vec<char> = text.chars().collect();
        chars.sort_unstable();
        chars.dedup();
        
        if chars.is_empty() {
            return Vec::new();
        }
        
        let mut ranges = Vec::new();
        let mut range_start = chars[0] as u32;
        let mut range_end = range_start;
        
        for &c in &chars[1..] {
            let codepoint = c as u32;
            if codepoint == range_end + 1 {
                range_end = codepoint;
            } else {
                ranges.push(UnicodeRange { start: range_start, end: range_end });
                range_start = codepoint;
                range_end = codepoint;
            }
        }
        
        ranges.push(UnicodeRange { start: range_start, end: range_end });
        ranges
    }
    
    /// Fuzzy query for fonts by name when exact match fails
    /// Uses intelligent token-based matching with inverted index for speed:
    /// 1. Break name into tokens (e.g., "NotoSansJP" -> ["noto", "sans", "jp"])
    /// 2. Use token_index to find candidate fonts via BTreeSet intersection
    /// 3. Score only the candidate fonts (instead of all 800+ patterns)
    /// 4. Prioritize fonts matching more tokens + Unicode coverage
    #[cfg(feature = "std")]
    fn fuzzy_query_by_name(
        &self,
        requested_name: &str,
        weight: FcWeight,
        italic: PatternMatch,
        oblique: PatternMatch,
        unicode_ranges: &[UnicodeRange],
        _trace: &mut Vec<TraceMsg>,
    ) -> Vec<FontMatch> {
        // Extract tokens from the requested name (e.g., "NotoSansJP" -> ["noto", "sans", "jp"])
        let tokens = Self::extract_font_name_tokens(requested_name);
        
        if tokens.is_empty() {
            return Vec::new();
        }
        
        // Convert tokens to lowercase for case-insensitive lookup
        let tokens_lower: Vec<String> = tokens.iter().map(|t| t.to_ascii_lowercase()).collect();
        
        // Progressive token matching strategy:
        // Start with first token, then progressively narrow down with each additional token
        // If adding a token results in 0 matches, use the previous (broader) set
        // Example: ["Noto"] -> 10 fonts, ["Noto","Sans"] -> 2 fonts, ["Noto","Sans","JP"] -> 0 fonts => use 2 fonts
        
        let state = self.state_read();

        // Start with the first token
        let first_token = &tokens_lower[0];
        let mut candidate_ids = match state.token_index.get(first_token) {
            Some(ids) if !ids.is_empty() => ids.clone(),
            _ => {
                // First token not found - no fonts match, quit immediately
                return Vec::new();
            }
        };

        // Progressively narrow down with each additional token
        for token in &tokens_lower[1..] {
            if let Some(token_ids) = state.token_index.get(token) {
                // Calculate intersection
                let intersection: alloc::collections::BTreeSet<FontId> =
                    candidate_ids.intersection(token_ids).copied().collect();

                if intersection.is_empty() {
                    // Adding this token results in 0 matches - keep previous set and stop
                    break;
                } else {
                    // Successfully narrowed down - use intersection
                    candidate_ids = intersection;
                }
            } else {
                // Token not in index - keep current set and stop
                break;
            }
        }

        // Now score only the candidate fonts (HUGE speedup!)
        let mut candidates = Vec::new();

        for id in candidate_ids {
            let pattern = match state.metadata.get(&id) {
                Some(p) => p,
                None => continue,
            };
            
            // Get pre-tokenized font name (already lowercase)
            let font_tokens_lower = match state.font_tokens.get(&id) {
                Some(tokens) => tokens,
                None => continue,
            };
            
            if font_tokens_lower.is_empty() {
                continue;
            }
            
            // Calculate token match score (how many requested tokens appear in font name)
            // Both tokens_lower and font_tokens_lower are already lowercase, so direct comparison
            let token_matches = tokens_lower.iter()
                .filter(|req_token| {
                    font_tokens_lower.iter().any(|font_token| {
                        // Both already lowercase — exact token match (index guarantees candidates)
                        font_token == *req_token
                    })
                })
                .count();
            
            // Skip if no tokens match (shouldn't happen due to index, but safety check)
            if token_matches == 0 {
                continue;
            }
            
            // Calculate token similarity score (0-100)
            let token_similarity = (token_matches * 100 / tokens.len()) as i32;
            
            // Calculate Unicode range similarity
            let unicode_similarity = if !unicode_ranges.is_empty() && !pattern.unicode_ranges.is_empty() {
                Self::calculate_unicode_compatibility(unicode_ranges, &pattern.unicode_ranges)
            } else {
                0
            };
            
            // CRITICAL: If we have Unicode requirements, ONLY accept fonts that cover them
            // A font with great name match but no Unicode coverage is useless
            if !unicode_ranges.is_empty() && unicode_similarity == 0 {
                continue;
            }
            
            let style_score = Self::calculate_style_score(&FcPattern {
                weight,
                italic,
                oblique,
                ..Default::default()
            }, pattern);
            
            candidates.push((
                id,
                token_similarity,
                unicode_similarity,
                style_score,
                pattern.clone(),
            ));
        }
        
        // Sort by:
        // 1. Token matches (more matches = better)
        // 2. Unicode compatibility (if ranges provided)
        // 3. Style score (lower is better)
        // 4. Deterministic tiebreaker: prefer non-italic, then by font name
        candidates.sort_by(|a, b| {
            if !unicode_ranges.is_empty() {
                // When we have Unicode requirements, prioritize coverage
                b.1.cmp(&a.1) // Token similarity (higher is better) - PRIMARY
                    .then_with(|| b.2.cmp(&a.2)) // Unicode similarity (higher is better) - SECONDARY
                    .then_with(|| a.3.cmp(&b.3)) // Style score (lower is better) - TERTIARY
                    .then_with(|| a.4.italic.cmp(&b.4.italic)) // Prefer non-italic (False < True)
                    .then_with(|| a.4.name.cmp(&b.4.name)) // Alphabetical by name
            } else {
                // No Unicode requirements, token similarity is primary
                b.1.cmp(&a.1) // Token similarity (higher is better)
                    .then_with(|| a.3.cmp(&b.3)) // Style score (lower is better)
                    .then_with(|| a.4.italic.cmp(&b.4.italic)) // Prefer non-italic (False < True)
                    .then_with(|| a.4.name.cmp(&b.4.name)) // Alphabetical by name
            }
        });
        
        // Take top 5 matches
        candidates.truncate(5);
        
        // Convert to FontMatch
        candidates
            .into_iter()
            .map(|(id, _token_sim, _unicode_sim, _style, pattern)| {
                FontMatch {
                    id,
                    unicode_ranges: pattern.unicode_ranges.clone(),
                    fallbacks: Vec::new(), // Fallbacks computed lazily via compute_fallbacks()
                }
            })
            .collect()
    }
    
    /// Extract tokens from a font name
    /// E.g., "NotoSansJP" -> ["Noto", "Sans", "JP"]
    /// E.g., "Noto Sans CJK JP" -> ["Noto", "Sans", "CJK", "JP"]
    pub fn extract_font_name_tokens(name: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        let mut current_token = String::new();
        let mut last_was_lower = false;
        
        for c in name.chars() {
            if c.is_whitespace() || c == '-' || c == '_' {
                // Word separator
                if !current_token.is_empty() {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
                last_was_lower = false;
            } else if c.is_uppercase() && last_was_lower && !current_token.is_empty() {
                // CamelCase boundary (e.g., "Noto" | "Sans")
                tokens.push(current_token.clone());
                current_token.clear();
                current_token.push(c);
                last_was_lower = false;
            } else {
                current_token.push(c);
                last_was_lower = c.is_lowercase();
            }
        }
        
        if !current_token.is_empty() {
            tokens.push(current_token);
        }
        
        tokens
    }
    
    /// Find fonts to cover missing Unicode ranges
    /// Uses intelligent matching: prefers fonts with similar names to existing ones
    /// Early quits once all Unicode ranges are covered for performance
    fn find_unicode_fallbacks(
        &self,
        unicode_ranges: &[UnicodeRange],
        covered_chars: &[bool],
        existing_groups: &[CssFallbackGroup],
        _weight: FcWeight,
        _italic: PatternMatch,
        _oblique: PatternMatch,
        trace: &mut Vec<TraceMsg>,
    ) -> Vec<FontMatch> {
        // Extract uncovered ranges
        let mut uncovered_ranges = Vec::new();
        for (i, &covered) in covered_chars.iter().enumerate() {
            if !covered && i < unicode_ranges.len() {
                uncovered_ranges.push(unicode_ranges[i].clone());
            }
        }
        
        if uncovered_ranges.is_empty() {
            return Vec::new();
        }

        // Query for fonts that cover these ranges.
        // Use DontCare for weight/italic/oblique — we want ANY font that covers
        // the missing characters, regardless of style. The similarity sort below
        // will prefer fonts matching the existing chain's style anyway.
        let pattern = FcPattern {
            name: None,
            weight: FcWeight::Normal, // Normal weight is not filtered by query_matches_internal (line 1836)
            italic: PatternMatch::DontCare,
            oblique: PatternMatch::DontCare,
            unicode_ranges: uncovered_ranges.clone(),
            ..Default::default()
        };
        
        let mut candidates = self.query_internal(&pattern, trace);

        // Intelligent sorting: prefer fonts with similar names to existing ones
        // Extract font family prefixes from existing fonts (e.g., "Noto Sans" from "Noto Sans JP")
        let existing_prefixes: Vec<String> = existing_groups
            .iter()
            .flat_map(|group| {
                group.fonts.iter().filter_map(|font| {
                    self.get_metadata_by_id(&font.id)
                        .and_then(|meta| meta.family.clone())
                        .and_then(|family| {
                            // Extract prefix (e.g., "Noto Sans" from "Noto Sans JP")
                            family.split_whitespace()
                                .take(2)
                                .collect::<Vec<_>>()
                                .join(" ")
                                .into()
                        })
                })
            })
            .collect();
        
        // Sort candidates by:
        // 1. Name similarity to existing fonts (highest priority)
        // 2. Unicode coverage (secondary)
        candidates.sort_by(|a, b| {
            let a_meta = self.get_metadata_by_id(&a.id);
            let b_meta = self.get_metadata_by_id(&b.id);

            let a_score = Self::calculate_font_similarity_score(a_meta.as_ref(), &existing_prefixes);
            let b_score = Self::calculate_font_similarity_score(b_meta.as_ref(), &existing_prefixes);
            
            b_score.cmp(&a_score) // Higher score = better match
                .then_with(|| {
                    let a_coverage = Self::calculate_unicode_compatibility(&uncovered_ranges, &a.unicode_ranges);
                    let b_coverage = Self::calculate_unicode_compatibility(&uncovered_ranges, &b.unicode_ranges);
                    b_coverage.cmp(&a_coverage)
                })
        });
        
        // Early quit optimization: only take fonts until all ranges are covered
        let mut result = Vec::new();
        let mut remaining_uncovered: Vec<bool> = vec![true; uncovered_ranges.len()];
        for candidate in candidates {
            // Check which ranges this font covers
            let mut covers_new_range = false;

            for (i, range) in uncovered_ranges.iter().enumerate() {
                if remaining_uncovered[i] {
                    // Check if this font covers this range
                    for font_range in &candidate.unicode_ranges {
                        if font_range.overlaps(range) {
                            remaining_uncovered[i] = false;
                            covers_new_range = true;
                            break;
                        }
                    }
                }
            }

            // Only add fonts that cover at least one new range
            if covers_new_range {
                result.push(candidate);

                // Early quit: if all ranges are covered, stop
                if remaining_uncovered.iter().all(|&uncovered| !uncovered) {
                    break;
                }
            }
        }

        result
    }
    
    /// Calculate similarity score between a font and existing font prefixes
    /// Higher score = more similar
    fn calculate_font_similarity_score(
        font_meta: Option<&FcPattern>,
        existing_prefixes: &[String],
    ) -> i32 {
        let Some(meta) = font_meta else { return 0; };
        let Some(family) = &meta.family else { return 0; };
        
        // Check if this font's family matches any existing prefix
        for prefix in existing_prefixes {
            if family.starts_with(prefix) {
                return 100; // Strong match
            }
            if family.contains(prefix) {
                return 50; // Partial match
            }
        }
        
        0 // No match
    }
    
    /// Find fallback fonts for a given pattern
    // Helper to calculate total unicode coverage
    pub fn calculate_unicode_coverage(ranges: &[UnicodeRange]) -> u64 {
        ranges
            .iter()
            .map(|range| (range.end - range.start + 1) as u64)
            .sum()
    }

    /// Calculate how well a font's Unicode ranges cover the requested ranges
    /// Returns a compatibility score (higher is better, 0 means no overlap)
    pub fn calculate_unicode_compatibility(
        requested: &[UnicodeRange],
        available: &[UnicodeRange],
    ) -> i32 {
        if requested.is_empty() {
            // No specific requirements, return total coverage
            return Self::calculate_unicode_coverage(available) as i32;
        }
        
        let mut total_coverage = 0u32;
        
        for req_range in requested {
            for avail_range in available {
                // Calculate overlap between requested and available ranges
                let overlap_start = req_range.start.max(avail_range.start);
                let overlap_end = req_range.end.min(avail_range.end);
                
                if overlap_start <= overlap_end {
                    // There is overlap
                    let overlap_size = overlap_end - overlap_start + 1;
                    total_coverage += overlap_size;
                }
            }
        }
        
        total_coverage as i32
    }

    pub fn calculate_style_score(original: &FcPattern, candidate: &FcPattern) -> i32 {

        let mut score = 0_i32;

        // Weight calculation with special handling for bold property
        if (original.bold == PatternMatch::True && candidate.weight == FcWeight::Bold)
            || (original.bold == PatternMatch::False && candidate.weight != FcWeight::Bold)
        {
            // No weight penalty when bold is requested and font has Bold weight
            // No weight penalty when non-bold is requested and font has non-Bold weight
        } else {
            // Apply normal weight difference penalty
            let weight_diff = (original.weight as i32 - candidate.weight as i32).abs();
            score += weight_diff as i32;
        }

        // Exact weight match bonus: reward fonts whose weight matches the request exactly,
        // with an extra bonus when both are Normal (the most common case for body text)
        if original.weight == candidate.weight {
            score -= 15;
            if original.weight == FcWeight::Normal {
                score -= 10; // Extra bonus for Normal-Normal match
            }
        }

        // Stretch calculation with special handling for condensed property
        if (original.condensed == PatternMatch::True && candidate.stretch.is_condensed())
            || (original.condensed == PatternMatch::False && !candidate.stretch.is_condensed())
        {
            // No stretch penalty when condensed is requested and font has condensed stretch
            // No stretch penalty when non-condensed is requested and font has non-condensed stretch
        } else {
            // Apply normal stretch difference penalty
            let stretch_diff = (original.stretch as i32 - candidate.stretch as i32).abs();
            score += (stretch_diff * 100) as i32;
        }

        // Handle style properties with standard penalties and bonuses
        let style_props = [
            (original.italic, candidate.italic, 300, 150),
            (original.oblique, candidate.oblique, 200, 100),
            (original.bold, candidate.bold, 300, 150),
            (original.monospace, candidate.monospace, 100, 50),
            (original.condensed, candidate.condensed, 100, 50),
        ];

        for (orig, cand, mismatch_penalty, dontcare_penalty) in style_props {
            if orig.needs_to_match() {
                if orig == PatternMatch::False && cand == PatternMatch::DontCare {
                    // Requesting non-italic but font doesn't declare: small penalty
                    // (less than a full mismatch but more than a perfect match)
                    score += dontcare_penalty / 2;
                } else if !orig.matches(&cand) {
                    if cand == PatternMatch::DontCare {
                        score += dontcare_penalty;
                    } else {
                        score += mismatch_penalty;
                    }
                } else if orig == PatternMatch::True && cand == PatternMatch::True {
                    // Give bonus for exact True match
                    score -= 20;
                } else if orig == PatternMatch::False && cand == PatternMatch::False {
                    // Give bonus for exact False match (prefer explicitly non-italic
                    // over fonts with unknown/DontCare italic status)
                    score -= 20;
                }
            } else {
                // orig == DontCare: prefer "normal" fonts over styled ones.
                // When the caller doesn't specify italic/bold/etc., a font
                // that IS italic/bold should score slightly worse than one
                // that isn't, so Regular is chosen over Italic by default.
                if cand == PatternMatch::True {
                    score += dontcare_penalty / 3;
                }
            }
        }

        // ── Name-based "base font" detection ──
        // The shorter the font name relative to its family, the more "basic" the
        // variant.  E.g. "System Font" (the base) should score better than
        // "System Font Regular Italic" (a variant) when the user hasn't
        // explicitly requested italic.
        if let (Some(name), Some(family)) = (&candidate.name, &candidate.family) {
            let name_lower = name.to_ascii_lowercase();
            let family_lower = family.to_ascii_lowercase();

            // Strip the family prefix from the name to get the "extra" part
            let extra = if name_lower.starts_with(&family_lower) {
                name_lower[family_lower.len()..].to_string()
            } else {
                String::new()
            };

            // Strip common neutral descriptors that don't indicate a style variant
            let stripped = extra
                .replace("regular", "")
                .replace("normal", "")
                .replace("book", "")
                .replace("roman", "");
            let stripped = stripped.trim();

            if stripped.is_empty() {
                // This is a "base font" – name is just the family (± "Regular")
                score -= 50;
            } else {
                // Name has extra style descriptors – add a penalty per extra word
                let extra_words = stripped.split_whitespace().count();
                score += (extra_words as i32) * 25;
            }
        }

        // ── Subfamily "Regular" bonus ──
        // Fonts whose OpenType subfamily is exactly "Regular" are the canonical
        // base variant and should be strongly preferred.
        if let Some(ref subfamily) = candidate.metadata.font_subfamily {
            let sf_lower = subfamily.to_ascii_lowercase();
            if sf_lower == "regular" {
                score -= 30;
            }
        }

        score
    }
}

#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn FcScanDirectories() -> Option<(Vec<(FcPattern, FcFontPath)>, BTreeMap<String, FcFontRenderConfig>)> {
    use std::fs;
    use std::path::Path;

    const BASE_FONTCONFIG_PATH: &str = "/etc/fonts/fonts.conf";

    if !Path::new(BASE_FONTCONFIG_PATH).exists() {
        return None;
    }

    let mut font_paths = Vec::with_capacity(32);
    let mut paths_to_visit = vec![(None, PathBuf::from(BASE_FONTCONFIG_PATH))];
    let mut render_configs: BTreeMap<String, FcFontRenderConfig> = BTreeMap::new();

    while let Some((prefix, path_to_visit)) = paths_to_visit.pop() {
        let path = match process_path(&prefix, path_to_visit, true) {
            Some(path) => path,
            None => continue,
        };

        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };

        if metadata.is_file() {
            let xml_utf8 = match fs::read_to_string(&path) {
                Ok(xml_utf8) => xml_utf8,
                Err(_) => continue,
            };

            if ParseFontsConf(&xml_utf8, &mut paths_to_visit, &mut font_paths).is_none() {
                continue;
            }

            // Also parse render config blocks from this file
            ParseFontsConfRenderConfig(&xml_utf8, &mut render_configs);
        } else if metadata.is_dir() {
            let dir_entries = match fs::read_dir(&path) {
                Ok(dir_entries) => dir_entries,
                Err(_) => continue,
            };

            for entry_result in dir_entries {
                let entry = match entry_result {
                    Ok(entry) => entry,
                    Err(_) => continue,
                };

                let entry_path = entry.path();

                // `fs::metadata` traverses symbolic links
                let entry_metadata = match fs::metadata(&entry_path) {
                    Ok(metadata) => metadata,
                    Err(_) => continue,
                };

                if !entry_metadata.is_file() {
                    continue;
                }

                let file_name = match entry_path.file_name() {
                    Some(name) => name,
                    None => continue,
                };

                let file_name_str = file_name.to_string_lossy();
                if file_name_str.starts_with(|c: char| c.is_ascii_digit())
                    && file_name_str.ends_with(".conf")
                {
                    paths_to_visit.push((None, entry_path));
                }
            }
        }
    }

    if font_paths.is_empty() {
        return None;
    }

    Some((FcScanDirectoriesInner(&font_paths), render_configs))
}

// Parses the fonts.conf file
#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn ParseFontsConf(
    input: &str,
    paths_to_visit: &mut Vec<(Option<String>, PathBuf)>,
    font_paths: &mut Vec<(Option<String>, String)>,
) -> Option<()> {
    use xmlparser::Token::*;
    use xmlparser::Tokenizer;

    const TAG_INCLUDE: &str = "include";
    const TAG_DIR: &str = "dir";
    const ATTRIBUTE_PREFIX: &str = "prefix";

    let mut current_prefix: Option<&str> = None;
    let mut current_path: Option<&str> = None;
    let mut is_in_include = false;
    let mut is_in_dir = false;

    for token_result in Tokenizer::from(input) {
        let token = match token_result {
            Ok(token) => token,
            Err(_) => return None,
        };

        match token {
            ElementStart { local, .. } => {
                if is_in_include || is_in_dir {
                    return None; /* error: nested tags */
                }

                match local.as_str() {
                    TAG_INCLUDE => {
                        is_in_include = true;
                    }
                    TAG_DIR => {
                        is_in_dir = true;
                    }
                    _ => continue,
                }

                current_path = None;
            }
            Text { text, .. } => {
                let text = text.as_str().trim();
                if text.is_empty() {
                    continue;
                }
                if is_in_include || is_in_dir {
                    current_path = Some(text);
                }
            }
            Attribute { local, value, .. } => {
                if !is_in_include && !is_in_dir {
                    continue;
                }
                // attribute on <include> or <dir> node
                if local.as_str() == ATTRIBUTE_PREFIX {
                    current_prefix = Some(value.as_str());
                }
            }
            ElementEnd { end, .. } => {
                let end_tag = match end {
                    xmlparser::ElementEnd::Close(_, a) => a,
                    _ => continue,
                };

                match end_tag.as_str() {
                    TAG_INCLUDE => {
                        if !is_in_include {
                            continue;
                        }

                        if let Some(current_path) = current_path.as_ref() {
                            paths_to_visit.push((
                                current_prefix.map(ToOwned::to_owned),
                                PathBuf::from(*current_path),
                            ));
                        }
                    }
                    TAG_DIR => {
                        if !is_in_dir {
                            continue;
                        }

                        if let Some(current_path) = current_path.as_ref() {
                            font_paths.push((
                                current_prefix.map(ToOwned::to_owned),
                                (*current_path).to_owned(),
                            ));
                        }
                    }
                    _ => continue,
                }

                is_in_include = false;
                is_in_dir = false;
                current_path = None;
                current_prefix = None;
            }
            _ => {}
        }
    }

    Some(())
}

/// Parses `<match target="font">` blocks from fonts.conf XML and returns
/// a map from family name to per-font rendering configuration.
///
/// Example fonts.conf snippet that this handles:
/// ```xml
/// <match target="font">
///   <test name="family"><string>Inconsolata</string></test>
///   <edit name="antialias" mode="assign"><bool>true</bool></edit>
///   <edit name="hintstyle" mode="assign"><const>hintslight</const></edit>
/// </match>
/// ```
#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn ParseFontsConfRenderConfig(
    input: &str,
    configs: &mut BTreeMap<String, FcFontRenderConfig>,
) {
    use xmlparser::Token::*;
    use xmlparser::Tokenizer;

    // Parser state machine
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        /// Outside any relevant block
        Idle,
        /// Inside <match target="font">
        InMatchFont,
        /// Inside <test name="family"> within a match block
        InTestFamily,
        /// Inside <edit name="..."> within a match block
        InEdit,
        /// Inside a value element (<bool>, <double>, <const>, <string>) within <edit> or <test>
        InValue,
    }

    let mut state = State::Idle;
    let mut match_is_font_target = false;
    let mut current_family: Option<String> = None;
    let mut current_edit_name: Option<String> = None;
    let mut current_value: Option<String> = None;
    let mut value_tag: Option<String> = None;
    let mut config = FcFontRenderConfig::default();
    let mut in_test = false;
    let mut test_name: Option<String> = None;

    for token_result in Tokenizer::from(input) {
        let token = match token_result {
            Ok(token) => token,
            Err(_) => continue,
        };

        match token {
            ElementStart { local, .. } => {
                let tag = local.as_str();
                match tag {
                    "match" => {
                        // Reset state for a new match block
                        match_is_font_target = false;
                        current_family = None;
                        config = FcFontRenderConfig::default();
                    }
                    "test" if state == State::InMatchFont => {
                        in_test = true;
                        test_name = None;
                    }
                    "edit" if state == State::InMatchFont => {
                        current_edit_name = None;
                    }
                    "bool" | "double" | "const" | "string" | "int" => {
                        if state == State::InTestFamily || state == State::InEdit {
                            value_tag = Some(tag.to_owned());
                            current_value = None;
                        }
                    }
                    _ => {}
                }
            }
            Attribute { local, value, .. } => {
                let attr_name = local.as_str();
                let attr_value = value.as_str();

                match attr_name {
                    "target" => {
                        if attr_value == "font" {
                            match_is_font_target = true;
                        }
                    }
                    "name" => {
                        if in_test && state == State::InMatchFont {
                            test_name = Some(attr_value.to_owned());
                        } else if state == State::InMatchFont {
                            current_edit_name = Some(attr_value.to_owned());
                        }
                    }
                    _ => {}
                }
            }
            Text { text, .. } => {
                let text = text.as_str().trim();
                if !text.is_empty() && (state == State::InTestFamily || state == State::InEdit) {
                    current_value = Some(text.to_owned());
                }
            }
            ElementEnd { end, .. } => {
                match end {
                    xmlparser::ElementEnd::Open => {
                        // Tag just opened (after attributes processed)
                        if match_is_font_target && state == State::Idle {
                            state = State::InMatchFont;
                            match_is_font_target = false;
                        } else if in_test {
                            if test_name.as_deref() == Some("family") {
                                state = State::InTestFamily;
                            }
                            in_test = false;
                        } else if current_edit_name.is_some() && state == State::InMatchFont {
                            state = State::InEdit;
                        }
                    }
                    xmlparser::ElementEnd::Close(_, local) => {
                        let tag = local.as_str();
                        match tag {
                            "match" => {
                                // End of match block: store config if we have a family
                                if let Some(family) = current_family.take() {
                                    let empty = FcFontRenderConfig::default();
                                    if config != empty {
                                        configs.insert(family, config.clone());
                                    }
                                }
                                state = State::Idle;
                                config = FcFontRenderConfig::default();
                            }
                            "test" => {
                                if state == State::InTestFamily {
                                    // Extract the family name from the value we collected
                                    if let Some(ref val) = current_value {
                                        current_family = Some(val.clone());
                                    }
                                    state = State::InMatchFont;
                                }
                                current_value = None;
                                value_tag = None;
                            }
                            "edit" => {
                                if state == State::InEdit {
                                    // Apply the collected value to the config
                                    if let (Some(ref name), Some(ref val)) = (&current_edit_name, &current_value) {
                                        apply_edit_value(&mut config, name, val, value_tag.as_deref());
                                    }
                                    state = State::InMatchFont;
                                }
                                current_edit_name = None;
                                current_value = None;
                                value_tag = None;
                            }
                            "bool" | "double" | "const" | "string" | "int" => {
                                // value_tag and current_value already set by Text handler
                            }
                            _ => {}
                        }
                    }
                    xmlparser::ElementEnd::Empty => {
                        // Self-closing tags: nothing to do
                    }
                }
            }
            _ => {}
        }
    }
}

/// Apply a parsed edit value to the render config.
#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn apply_edit_value(
    config: &mut FcFontRenderConfig,
    edit_name: &str,
    value: &str,
    value_tag: Option<&str>,
) {
    match edit_name {
        "antialias" => {
            config.antialias = parse_bool_value(value);
        }
        "hinting" => {
            config.hinting = parse_bool_value(value);
        }
        "autohint" => {
            config.autohint = parse_bool_value(value);
        }
        "embeddedbitmap" => {
            config.embeddedbitmap = parse_bool_value(value);
        }
        "embolden" => {
            config.embolden = parse_bool_value(value);
        }
        "minspace" => {
            config.minspace = parse_bool_value(value);
        }
        "hintstyle" => {
            config.hintstyle = parse_hintstyle_const(value);
        }
        "rgba" => {
            config.rgba = parse_rgba_const(value);
        }
        "lcdfilter" => {
            config.lcdfilter = parse_lcdfilter_const(value);
        }
        "dpi" => {
            if let Ok(v) = value.parse::<f64>() {
                config.dpi = Some(v);
            }
        }
        "scale" => {
            if let Ok(v) = value.parse::<f64>() {
                config.scale = Some(v);
            }
        }
        _ => {
            // Unknown edit property, ignore
        }
    }
}

#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn parse_bool_value(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn parse_hintstyle_const(value: &str) -> Option<FcHintStyle> {
    match value {
        "hintnone" => Some(FcHintStyle::None),
        "hintslight" => Some(FcHintStyle::Slight),
        "hintmedium" => Some(FcHintStyle::Medium),
        "hintfull" => Some(FcHintStyle::Full),
        _ => None,
    }
}

#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn parse_rgba_const(value: &str) -> Option<FcRgba> {
    match value {
        "unknown" => Some(FcRgba::Unknown),
        "rgb" => Some(FcRgba::Rgb),
        "bgr" => Some(FcRgba::Bgr),
        "vrgb" => Some(FcRgba::Vrgb),
        "vbgr" => Some(FcRgba::Vbgr),
        "none" => Some(FcRgba::None),
        _ => None,
    }
}

#[cfg(all(feature = "std", feature = "parsing", target_os = "linux"))]
fn parse_lcdfilter_const(value: &str) -> Option<FcLcdFilter> {
    match value {
        "lcdnone" => Some(FcLcdFilter::None),
        "lcddefault" => Some(FcLcdFilter::Default),
        "lcdlight" => Some(FcLcdFilter::Light),
        "lcdlegacy" => Some(FcLcdFilter::Legacy),
        _ => None,
    }
}

// Unicode range bit positions to actual ranges (full table from OpenType spec).
// Based on: https://learn.microsoft.com/en-us/typography/opentype/spec/os2#ur
#[cfg(all(feature = "std", feature = "parsing"))]
const UNICODE_RANGE_MAPPINGS: &[(usize, u32, u32)] = &[
    // ulUnicodeRange1 (bits 0-31)
    (0, 0x0000, 0x007F), // Basic Latin
    (1, 0x0080, 0x00FF), // Latin-1 Supplement
    (2, 0x0100, 0x017F), // Latin Extended-A
    (3, 0x0180, 0x024F), // Latin Extended-B
    (4, 0x0250, 0x02AF), // IPA Extensions
    (5, 0x02B0, 0x02FF), // Spacing Modifier Letters
    (6, 0x0300, 0x036F), // Combining Diacritical Marks
    (7, 0x0370, 0x03FF), // Greek and Coptic
    (8, 0x2C80, 0x2CFF), // Coptic
    (9, 0x0400, 0x04FF), // Cyrillic
    (10, 0x0530, 0x058F), // Armenian
    (11, 0x0590, 0x05FF), // Hebrew
    (12, 0x0600, 0x06FF), // Arabic
    (13, 0x0700, 0x074F), // Syriac
    (14, 0x0780, 0x07BF), // Thaana
    (15, 0x0900, 0x097F), // Devanagari
    (16, 0x0980, 0x09FF), // Bengali
    (17, 0x0A00, 0x0A7F), // Gurmukhi
    (18, 0x0A80, 0x0AFF), // Gujarati
    (19, 0x0B00, 0x0B7F), // Oriya
    (20, 0x0B80, 0x0BFF), // Tamil
    (21, 0x0C00, 0x0C7F), // Telugu
    (22, 0x0C80, 0x0CFF), // Kannada
    (23, 0x0D00, 0x0D7F), // Malayalam
    (24, 0x0E00, 0x0E7F), // Thai
    (25, 0x0E80, 0x0EFF), // Lao
    (26, 0x10A0, 0x10FF), // Georgian
    (27, 0x1B00, 0x1B7F), // Balinese
    (28, 0x1100, 0x11FF), // Hangul Jamo
    (29, 0x1E00, 0x1EFF), // Latin Extended Additional
    (30, 0x1F00, 0x1FFF), // Greek Extended
    (31, 0x2000, 0x206F), // General Punctuation
    // ulUnicodeRange2 (bits 32-63)
    (32, 0x2070, 0x209F), // Superscripts And Subscripts
    (33, 0x20A0, 0x20CF), // Currency Symbols
    (34, 0x20D0, 0x20FF), // Combining Diacritical Marks For Symbols
    (35, 0x2100, 0x214F), // Letterlike Symbols
    (36, 0x2150, 0x218F), // Number Forms
    (37, 0x2190, 0x21FF), // Arrows
    (38, 0x2200, 0x22FF), // Mathematical Operators
    (39, 0x2300, 0x23FF), // Miscellaneous Technical
    (40, 0x2400, 0x243F), // Control Pictures
    (41, 0x2440, 0x245F), // Optical Character Recognition
    (42, 0x2460, 0x24FF), // Enclosed Alphanumerics
    (43, 0x2500, 0x257F), // Box Drawing
    (44, 0x2580, 0x259F), // Block Elements
    (45, 0x25A0, 0x25FF), // Geometric Shapes
    (46, 0x2600, 0x26FF), // Miscellaneous Symbols
    (47, 0x2700, 0x27BF), // Dingbats
    (48, 0x3000, 0x303F), // CJK Symbols And Punctuation
    (49, 0x3040, 0x309F), // Hiragana
    (50, 0x30A0, 0x30FF), // Katakana
    (51, 0x3100, 0x312F), // Bopomofo
    (52, 0x3130, 0x318F), // Hangul Compatibility Jamo
    (53, 0x3190, 0x319F), // Kanbun
    (54, 0x31A0, 0x31BF), // Bopomofo Extended
    (55, 0x31C0, 0x31EF), // CJK Strokes
    (56, 0x31F0, 0x31FF), // Katakana Phonetic Extensions
    (57, 0x3200, 0x32FF), // Enclosed CJK Letters And Months
    (58, 0x3300, 0x33FF), // CJK Compatibility
    (59, 0x4E00, 0x9FFF), // CJK Unified Ideographs
    (60, 0xA000, 0xA48F), // Yi Syllables
    (61, 0xA490, 0xA4CF), // Yi Radicals
    (62, 0xAC00, 0xD7AF), // Hangul Syllables
    (63, 0xD800, 0xDFFF), // Non-Plane 0 (note: surrogates, not directly usable)
    // ulUnicodeRange3 (bits 64-95)
    (64, 0x10000, 0x10FFFF), // Phoenician and other non-BMP (bit 64 indicates non-BMP support)
    (65, 0xF900, 0xFAFF), // CJK Compatibility Ideographs
    (66, 0xFB00, 0xFB4F), // Alphabetic Presentation Forms
    (67, 0xFB50, 0xFDFF), // Arabic Presentation Forms-A
    (68, 0xFE00, 0xFE0F), // Variation Selectors
    (69, 0xFE10, 0xFE1F), // Vertical Forms
    (70, 0xFE20, 0xFE2F), // Combining Half Marks
    (71, 0xFE30, 0xFE4F), // CJK Compatibility Forms
    (72, 0xFE50, 0xFE6F), // Small Form Variants
    (73, 0xFE70, 0xFEFF), // Arabic Presentation Forms-B
    (74, 0xFF00, 0xFFEF), // Halfwidth And Fullwidth Forms
    (75, 0xFFF0, 0xFFFF), // Specials
    (76, 0x0F00, 0x0FFF), // Tibetan
    (77, 0x0700, 0x074F), // Syriac
    (78, 0x0780, 0x07BF), // Thaana
    (79, 0x0D80, 0x0DFF), // Sinhala
    (80, 0x1000, 0x109F), // Myanmar
    (81, 0x1200, 0x137F), // Ethiopic
    (82, 0x13A0, 0x13FF), // Cherokee
    (83, 0x1400, 0x167F), // Unified Canadian Aboriginal Syllabics
    (84, 0x1680, 0x169F), // Ogham
    (85, 0x16A0, 0x16FF), // Runic
    (86, 0x1780, 0x17FF), // Khmer
    (87, 0x1800, 0x18AF), // Mongolian
    (88, 0x2800, 0x28FF), // Braille Patterns
    (89, 0xA000, 0xA48F), // Yi Syllables
    (90, 0x1680, 0x169F), // Ogham
    (91, 0x16A0, 0x16FF), // Runic
    (92, 0x1700, 0x171F), // Tagalog
    (93, 0x1720, 0x173F), // Hanunoo
    (94, 0x1740, 0x175F), // Buhid
    (95, 0x1760, 0x177F), // Tagbanwa
    // ulUnicodeRange4 (bits 96-127)
    (96, 0x1900, 0x194F), // Limbu
    (97, 0x1950, 0x197F), // Tai Le
    (98, 0x1980, 0x19DF), // New Tai Lue
    (99, 0x1A00, 0x1A1F), // Buginese
    (100, 0x2C00, 0x2C5F), // Glagolitic
    (101, 0x2D30, 0x2D7F), // Tifinagh
    (102, 0x4DC0, 0x4DFF), // Yijing Hexagram Symbols
    (103, 0xA800, 0xA82F), // Syloti Nagri
    (104, 0x10000, 0x1007F), // Linear B Syllabary
    (105, 0x10080, 0x100FF), // Linear B Ideograms
    (106, 0x10100, 0x1013F), // Aegean Numbers
    (107, 0x10140, 0x1018F), // Ancient Greek Numbers
    (108, 0x10300, 0x1032F), // Old Italic
    (109, 0x10330, 0x1034F), // Gothic
    (110, 0x10380, 0x1039F), // Ugaritic
    (111, 0x103A0, 0x103DF), // Old Persian
    (112, 0x10400, 0x1044F), // Deseret
    (113, 0x10450, 0x1047F), // Shavian
    (114, 0x10480, 0x104AF), // Osmanya
    (115, 0x10800, 0x1083F), // Cypriot Syllabary
    (116, 0x10A00, 0x10A5F), // Kharoshthi
    (117, 0x1D000, 0x1D0FF), // Byzantine Musical Symbols
    (118, 0x1D100, 0x1D1FF), // Musical Symbols
    (119, 0x1D200, 0x1D24F), // Ancient Greek Musical Notation
    (120, 0x1D300, 0x1D35F), // Tai Xuan Jing Symbols
    (121, 0x1D400, 0x1D7FF), // Mathematical Alphanumeric Symbols
    (122, 0x1F000, 0x1F02F), // Mahjong Tiles
    (123, 0x1F030, 0x1F09F), // Domino Tiles
    (124, 0x1F300, 0x1F9FF), // Miscellaneous Symbols And Pictographs (Emoji)
    (125, 0x1F680, 0x1F6FF), // Transport And Map Symbols
    (126, 0x1F700, 0x1F77F), // Alchemical Symbols
    (127, 0x1F900, 0x1F9FF), // Supplemental Symbols and Pictographs
];

/// Intermediate parsed data from a single font face within a font file.
/// Used to share parsing logic between `FcParseFont` and `FcParseFontBytesInner`.
#[cfg(all(feature = "std", feature = "parsing"))]
struct ParsedFontFace {
    pattern: FcPattern,
    font_index: usize,
}

/// Parse all font table data from a single font face and return the extracted patterns.
///
/// This is the shared core of `FcParseFont` and `FcParseFontBytesInner`:
/// TTC detection, font table parsing, OS/2/head/post reading, unicode range extraction,
/// CMAP verification, monospace detection, metadata extraction, and pattern creation.
#[cfg(all(feature = "std", feature = "parsing"))]
fn parse_font_faces(font_bytes: &[u8]) -> Option<Vec<ParsedFontFace>> {
    use allsorts::{
        binary::read::ReadScope,
        font_data::FontData,
        get_name::fontcode_get_name,
        post::PostTable,
        tables::{
            os2::Os2, HeadTable, NameTable,
        },
        tag,
    };
    use std::collections::BTreeSet;

    const FONT_SPECIFIER_NAME_ID: u16 = 4;
    const FONT_SPECIFIER_FAMILY_ID: u16 = 1;

    let max_fonts = if font_bytes.len() >= 12 && &font_bytes[0..4] == b"ttcf" {
        // Read numFonts from TTC header (offset 8, 4 bytes)
        let num_fonts =
            u32::from_be_bytes([font_bytes[8], font_bytes[9], font_bytes[10], font_bytes[11]]);
        // Cap at a reasonable maximum as a safety measure
        std::cmp::min(num_fonts as usize, 100)
    } else {
        // Not a collection, just one font
        1
    };

    let scope = ReadScope::new(font_bytes);
    let font_file = scope.read::<FontData<'_>>().ok()?;

    // Handle collections properly by iterating through all fonts
    let mut results = Vec::new();

    for font_index in 0..max_fonts {
        let provider = font_file.table_provider(font_index).ok()?;
        let head_data = provider.table_data(tag::HEAD).ok()??.into_owned();
        let head_table = ReadScope::new(&head_data).read::<HeadTable>().ok()?;

        let is_bold = head_table.is_bold();
        let is_italic = head_table.is_italic();
        let mut detected_monospace = None;

        let post_data = provider.table_data(tag::POST).ok()??;
        if let Ok(post_table) = ReadScope::new(&post_data).read::<PostTable>() {
            // isFixedPitch here - https://learn.microsoft.com/en-us/typography/opentype/spec/post#header
            detected_monospace = Some(post_table.header.is_fixed_pitch != 0);
        }

        // Get font properties from OS/2 table
        let os2_data = provider.table_data(tag::OS_2).ok()??;
        let os2_table = ReadScope::new(&os2_data)
            .read_dep::<Os2>(os2_data.len())
            .ok()?;

        // Extract additional style information
        let is_oblique = os2_table
            .fs_selection
            .contains(allsorts::tables::os2::FsSelection::OBLIQUE);
        let weight = FcWeight::from_u16(os2_table.us_weight_class);
        let stretch = FcStretch::from_u16(os2_table.us_width_class);

        // Extract unicode ranges from OS/2 table (fast, but may be inaccurate)
        // These are hints about what the font *should* support
        // For actual glyph coverage verification, query the font file directly
        let mut unicode_ranges = Vec::new();

        // Process the 4 Unicode range bitfields from OS/2 table
        let os2_ranges = [
            os2_table.ul_unicode_range1,
            os2_table.ul_unicode_range2,
            os2_table.ul_unicode_range3,
            os2_table.ul_unicode_range4,
        ];

        for &(bit, start, end) in UNICODE_RANGE_MAPPINGS {
            let range_idx = bit / 32;
            let bit_pos = bit % 32;
            if range_idx < 4 && (os2_ranges[range_idx] & (1 << bit_pos)) != 0 {
                unicode_ranges.push(UnicodeRange { start, end });
            }
        }

        // Verify OS/2 reported ranges against actual CMAP support
        // OS/2 ulUnicodeRange bits can be unreliable - fonts may claim support
        // for ranges they don't actually have glyphs for
        unicode_ranges = verify_unicode_ranges_with_cmap(&provider, unicode_ranges);

        // If still empty (OS/2 had no ranges or all were invalid), do full CMAP analysis
        if unicode_ranges.is_empty() {
            if let Some(cmap_ranges) = analyze_cmap_coverage(&provider) {
                unicode_ranges = cmap_ranges;
            }
        }

        // Use the shared detect_monospace helper for PANOSE + hmtx fallback
        let is_monospace = detect_monospace(&provider, &os2_table, detected_monospace)
            .unwrap_or(false);

        let name_data = provider.table_data(tag::NAME).ok()??.into_owned();
        let name_table = ReadScope::new(&name_data).read::<NameTable>().ok()?;

        // Extract metadata from name table
        let mut metadata = FcFontMetadata::default();

        const NAME_ID_COPYRIGHT: u16 = 0;
        const NAME_ID_FAMILY: u16 = 1;
        const NAME_ID_SUBFAMILY: u16 = 2;
        const NAME_ID_UNIQUE_ID: u16 = 3;
        const NAME_ID_FULL_NAME: u16 = 4;
        const NAME_ID_VERSION: u16 = 5;
        const NAME_ID_POSTSCRIPT_NAME: u16 = 6;
        const NAME_ID_TRADEMARK: u16 = 7;
        const NAME_ID_MANUFACTURER: u16 = 8;
        const NAME_ID_DESIGNER: u16 = 9;
        const NAME_ID_DESCRIPTION: u16 = 10;
        const NAME_ID_VENDOR_URL: u16 = 11;
        const NAME_ID_DESIGNER_URL: u16 = 12;
        const NAME_ID_LICENSE: u16 = 13;
        const NAME_ID_LICENSE_URL: u16 = 14;
        const NAME_ID_PREFERRED_FAMILY: u16 = 16;
        const NAME_ID_PREFERRED_SUBFAMILY: u16 = 17;

        metadata.copyright = get_name_string(&name_data, NAME_ID_COPYRIGHT);
        metadata.font_family = get_name_string(&name_data, NAME_ID_FAMILY);
        metadata.font_subfamily = get_name_string(&name_data, NAME_ID_SUBFAMILY);
        metadata.full_name = get_name_string(&name_data, NAME_ID_FULL_NAME);
        metadata.unique_id = get_name_string(&name_data, NAME_ID_UNIQUE_ID);
        metadata.version = get_name_string(&name_data, NAME_ID_VERSION);
        metadata.postscript_name = get_name_string(&name_data, NAME_ID_POSTSCRIPT_NAME);
        metadata.trademark = get_name_string(&name_data, NAME_ID_TRADEMARK);
        metadata.manufacturer = get_name_string(&name_data, NAME_ID_MANUFACTURER);
        metadata.designer = get_name_string(&name_data, NAME_ID_DESIGNER);
        metadata.id_description = get_name_string(&name_data, NAME_ID_DESCRIPTION);
        metadata.designer_url = get_name_string(&name_data, NAME_ID_DESIGNER_URL);
        metadata.manufacturer_url = get_name_string(&name_data, NAME_ID_VENDOR_URL);
        metadata.license = get_name_string(&name_data, NAME_ID_LICENSE);
        metadata.license_url = get_name_string(&name_data, NAME_ID_LICENSE_URL);
        metadata.preferred_family = get_name_string(&name_data, NAME_ID_PREFERRED_FAMILY);
        metadata.preferred_subfamily = get_name_string(&name_data, NAME_ID_PREFERRED_SUBFAMILY);

        // One font can support multiple patterns
        let mut f_family = None;

        let patterns = name_table
            .name_records
            .iter()
            .filter_map(|name_record| {
                let name_id = name_record.name_id;
                if name_id == FONT_SPECIFIER_FAMILY_ID {
                    if let Ok(Some(family)) =
                        fontcode_get_name(&name_data, FONT_SPECIFIER_FAMILY_ID)
                    {
                        f_family = Some(family);
                    }
                    None
                } else if name_id == FONT_SPECIFIER_NAME_ID {
                    let family = f_family.as_ref()?;
                    let name = fontcode_get_name(&name_data, FONT_SPECIFIER_NAME_ID).ok()??;
                    if name.to_bytes().is_empty() {
                        None
                    } else {
                        let mut name_str =
                            String::from_utf8_lossy(name.to_bytes()).to_string();
                        let mut family_str =
                            String::from_utf8_lossy(family.as_bytes()).to_string();
                        if name_str.starts_with('.') {
                            name_str = name_str[1..].to_string();
                        }
                        if family_str.starts_with('.') {
                            family_str = family_str[1..].to_string();
                        }
                        Some((
                            FcPattern {
                                name: Some(name_str),
                                family: Some(family_str),
                                bold: if is_bold {
                                    PatternMatch::True
                                } else {
                                    PatternMatch::False
                                },
                                italic: if is_italic {
                                    PatternMatch::True
                                } else {
                                    PatternMatch::False
                                },
                                oblique: if is_oblique {
                                    PatternMatch::True
                                } else {
                                    PatternMatch::False
                                },
                                monospace: if is_monospace {
                                    PatternMatch::True
                                } else {
                                    PatternMatch::False
                                },
                                condensed: if stretch <= FcStretch::Condensed {
                                    PatternMatch::True
                                } else {
                                    PatternMatch::False
                                },
                                weight,
                                stretch,
                                unicode_ranges: unicode_ranges.clone(),
                                metadata: metadata.clone(),
                                render_config: FcFontRenderConfig::default(),
                            },
                            font_index,
                        ))
                    }
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();

        results.extend(patterns.into_iter().map(|(pat, idx)| ParsedFontFace {
            pattern: pat,
            font_index: idx,
        }));
    }

    if results.is_empty() {
        None
    } else {
        Some(results)
    }
}

// Remaining implementation for font scanning, parsing, etc.
#[cfg(all(feature = "std", feature = "parsing"))]
pub(crate) fn FcParseFont(filepath: &PathBuf) -> Option<Vec<(FcPattern, FcFontPath)>> {
    #[cfg(all(not(target_family = "wasm"), feature = "std"))]
    use mmapio::MmapOptions;
    use std::fs::File;

    // Try parsing the font file and see if the postscript name matches
    let file = File::open(filepath).ok()?;

    #[cfg(all(not(target_family = "wasm"), feature = "std"))]
    let font_bytes = unsafe { MmapOptions::new().map(&file).ok()? };

    #[cfg(not(all(not(target_family = "wasm"), feature = "std")))]
    let font_bytes = std::fs::read(filepath).ok()?;

    let faces = parse_font_faces(&font_bytes[..])?;
    let path_str = filepath.to_string_lossy().to_string();
    // Hash once per file — every face of a .ttc shares this value,
    // so the shared-bytes cache can return the same Arc<[u8]> for
    // all of them. Use the cheap sampled variant so the scout doesn't
    // page-fault the full file into RSS just to produce a dedup key.
    let bytes_hash = crate::utils::content_dedup_hash_u64(&font_bytes[..]);

    Some(
        faces
            .into_iter()
            .map(|face| {
                (
                    face.pattern,
                    FcFontPath {
                        path: path_str.clone(),
                        font_index: face.font_index,
                        bytes_hash,
                    },
                )
            })
            .collect(),
    )
}

/// Coverage info returned by a fast-probe parse.
///
/// Produced by [`FcParseFontFaceFast`] / [`FcProbeCoverage`] — the
/// v4.2 "cheap cmap-only" entry point. Unlike `parse_font_faces`,
/// this path does **not** read NAME, OS/2, POST, HHEA, HMTX, HEAD's
/// style metadata, or anything else. It only reads the table
/// directory, `head.macStyle` (2 bytes), and the cmap subtable that
/// matches the codepoints we care about. ~1 ms/face on warm FS
/// cache vs ~13 ms for the full parse.
///
/// The `pattern.unicode_ranges` is populated from the *actual* cmap
/// contents (one `UnicodeRange` per covered codepoint in the input
/// set) rather than the OS/2 `ulUnicodeRange` bitfield. That's more
/// precise (OS/2 bits lie on many fonts — they're hints, not ground
/// truth) and means `FontFallbackChain::resolve_char`'s coverage
/// check matches what the shaper can actually render.
#[cfg(all(feature = "std", feature = "parsing"))]
#[derive(Debug, Clone)]
pub struct FastCoverage {
    /// Metadata pattern with `unicode_ranges` populated from the
    /// codepoints this face covered from the request set. `name` /
    /// `family` fields are left empty — callers already have the
    /// filename-guessed family in [`FcFontRegistry.known_paths`];
    /// we avoid the NAME table read entirely.
    pub pattern: FcPattern,
    /// Subset of the input codepoints that this face covers (maps
    /// to a non-zero gid via the best cmap subtable). May be empty
    /// if the face covers none, in which case callers should fall
    /// through to the next candidate path.
    pub covered: alloc::collections::BTreeSet<char>,
    /// `head.macStyle.bold` (bit 0).
    pub is_bold: bool,
    /// `head.macStyle.italic` (bit 1).
    pub is_italic: bool,
}

/// Fast per-face coverage probe.
///
/// Opens the provided font bytes as a `FontData` (detects TTC
/// collections), walks the given face, reads `head.macStyle` for
/// bold/italic flags, picks the best cmap subtable, and records
/// which of the requested codepoints have a non-zero gid.
///
/// Cost: table-dir parse + head (54 bytes) + cmap (5-100 KiB,
/// faulted in from mmap). No heap allocation besides the
/// covered-codepoints set and the returned `FcPattern`.
///
/// Returns `None` only if the font bytes are structurally bad or
/// the face index is out of range — empty coverage returns
/// `Some` with `covered.is_empty()`, so the caller can distinguish
/// "this face doesn't have the char we want" (try next face) from
/// "this file is corrupt" (give up on the whole file).
#[cfg(all(feature = "std", feature = "parsing"))]
#[allow(non_snake_case)]
pub fn FcParseFontFaceFast(
    font_bytes: &[u8],
    font_index: usize,
    codepoints: &alloc::collections::BTreeSet<char>,
) -> Option<FastCoverage> {
    use allsorts::{
        binary::read::ReadScope,
        font_data::FontData,
        tables::{
            cmap::{Cmap, CmapSubtable},
            FontTableProvider, HeadTable,
        },
        tag,
    };

    let scope = ReadScope::new(font_bytes);
    let font_file = scope.read::<FontData<'_>>().ok()?;
    let provider = font_file.table_provider(font_index).ok()?;

    // head — 54 bytes, macStyle at offset 44. Cheap.
    let head_data = provider.table_data(tag::HEAD).ok()??;
    let head_table = ReadScope::new(&head_data).read::<HeadTable>().ok()?;
    let is_bold = head_table.is_bold();
    let is_italic = head_table.is_italic();

    // cmap — find the best Unicode subtable, probe each codepoint.
    // The mmap page-cache only faults in the bytes we touch.
    let cmap_data = provider.table_data(tag::CMAP).ok()??;
    let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>().ok()?;
    let encoding_record = find_best_cmap_subtable(&cmap)?;
    let cmap_subtable = ReadScope::new(&cmap_data)
        .offset(encoding_record.offset as usize)
        .read::<CmapSubtable<'_>>()
        .ok()?;

    let mut covered: alloc::collections::BTreeSet<char> =
        alloc::collections::BTreeSet::new();
    let mut covered_ranges: Vec<UnicodeRange> = Vec::new();
    for ch in codepoints {
        let cp = *ch as u32;
        if let Ok(Some(gid)) = cmap_subtable.map_glyph(cp) {
            if gid != 0 {
                covered.insert(*ch);
                // Accumulate into ranges for the FcPattern. Merge
                // adjacent codepoints so `unicode_ranges` stays
                // compact (common case on Western text: one range).
                if let Some(last) = covered_ranges.last_mut() {
                    if cp == last.end + 1 {
                        last.end = cp;
                        continue;
                    }
                }
                covered_ranges.push(UnicodeRange { start: cp, end: cp });
            }
        }
    }

    let weight = if is_bold {
        FcWeight::Bold
    } else {
        FcWeight::Normal
    };
    let italic_match = if is_italic {
        PatternMatch::True
    } else {
        PatternMatch::False
    };

    let pattern = FcPattern {
        name: None,
        family: None,
        weight,
        italic: italic_match,
        oblique: PatternMatch::DontCare,
        monospace: PatternMatch::DontCare,
        unicode_ranges: covered_ranges,
        ..Default::default()
    };

    Some(FastCoverage {
        pattern,
        covered,
        is_bold,
        is_italic,
    })
}

/// Count the number of faces inside a TTC, or `1` for a single-face
/// font file. Used by [`FcFontRegistry::request_fonts_fast`] to
/// iterate every face in a `.ttc` without paying the full-parse
/// cost (the TTC header is 12 bytes).
#[cfg(all(feature = "std", feature = "parsing"))]
#[allow(non_snake_case)]
pub fn FcCountFontFaces(font_bytes: &[u8]) -> usize {
    if font_bytes.len() >= 12 && &font_bytes[0..4] == b"ttcf" {
        let num_fonts = u32::from_be_bytes([
            font_bytes[8], font_bytes[9], font_bytes[10], font_bytes[11],
        ]);
        // Same cap as parse_font_faces, for safety.
        std::cmp::min(num_fonts as usize, 100).max(1)
    } else {
        1
    }
}

/// Parse font bytes and extract font patterns for in-memory fonts.
///
/// This is the public API for parsing in-memory font data to create
/// `(FcPattern, FcFont)` tuples that can be added to an `FcFontCache`
/// via `with_memory_fonts()`.
///
/// # Arguments
/// * `font_bytes` - The raw bytes of a TrueType/OpenType font file
/// * `font_id` - An identifier string for this font (used internally)
///
/// # Returns
/// A vector of `(FcPattern, FcFont)` tuples, one for each font face in the file.
/// Returns `None` if the font could not be parsed.
///
/// # Example
/// ```ignore
/// use rust_fontconfig::{FcFontCache, FcParseFontBytes};
///
/// let font_bytes = include_bytes!("path/to/font.ttf");
/// let mut cache = FcFontCache::default();
///
/// if let Some(fonts) = FcParseFontBytes(font_bytes, "MyFont") {
///     cache.with_memory_fonts(fonts);
/// }
/// ```
#[cfg(all(feature = "std", feature = "parsing"))]
#[allow(non_snake_case)]
pub fn FcParseFontBytes(font_bytes: &[u8], font_id: &str) -> Option<Vec<(FcPattern, FcFont)>> {
    FcParseFontBytesInner(font_bytes, font_id)
}

/// Internal implementation for parsing font bytes.
/// Delegates to `parse_font_faces` for shared parsing logic and wraps results as `FcFont`.
#[cfg(all(feature = "std", feature = "parsing"))]
fn FcParseFontBytesInner(font_bytes: &[u8], font_id: &str) -> Option<Vec<(FcPattern, FcFont)>> {
    let faces = parse_font_faces(font_bytes)?;
    let id = font_id.to_string();
    let bytes = font_bytes.to_vec();

    Some(
        faces
            .into_iter()
            .map(|face| {
                (
                    face.pattern,
                    FcFont {
                        bytes: bytes.clone(),
                        font_index: face.font_index,
                        id: id.clone(),
                    },
                )
            })
            .collect(),
    )
}

#[cfg(all(feature = "std", feature = "parsing"))]
fn FcScanDirectoriesInner(paths: &[(Option<String>, String)]) -> Vec<(FcPattern, FcFontPath)> {
    #[cfg(all(feature = "multithreading", not(target_family = "wasm")))]
    {
        use rayon::prelude::*;

        // scan directories in parallel
        paths
            .par_iter()
            .filter_map(|(prefix, p)| {
                process_path(prefix, PathBuf::from(p), false).map(FcScanSingleDirectoryRecursive)
            })
            .flatten()
            .collect()
    }
    // wasm has no rayon (it's target-gated off), so even with `multithreading`
    // enabled wasm falls back to the sequential path.
    #[cfg(not(all(feature = "multithreading", not(target_family = "wasm"))))]
    {
        paths
            .iter()
            .filter_map(|(prefix, p)| {
                process_path(prefix, PathBuf::from(p), false).map(FcScanSingleDirectoryRecursive)
            })
            .flatten()
            .collect()
    }
}

/// Recursively collect all files from a directory (no parsing, no allsorts).
#[cfg(feature = "std")]
fn FcCollectFontFilesRecursive(dir: PathBuf) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut dirs_to_parse = vec![dir];

    loop {
        let mut new_dirs = Vec::new();
        for dir in &dirs_to_parse {
            let entries = match std::fs::read_dir(dir) {
                Ok(o) => o,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    new_dirs.push(path);
                } else {
                    files.push(path);
                }
            }
        }
        if new_dirs.is_empty() {
            break;
        }
        dirs_to_parse = new_dirs;
    }

    files
}

#[cfg(all(feature = "std", feature = "parsing"))]
fn FcScanSingleDirectoryRecursive(dir: PathBuf) -> Vec<(FcPattern, FcFontPath)> {
    let files = FcCollectFontFilesRecursive(dir);
    FcParseFontFiles(&files)
}

#[cfg(all(feature = "std", feature = "parsing"))]
fn FcParseFontFiles(files_to_parse: &[PathBuf]) -> Vec<(FcPattern, FcFontPath)> {
    let result = {
        #[cfg(all(feature = "multithreading", not(target_family = "wasm")))]
        {
            use rayon::prelude::*;

            files_to_parse
                .par_iter()
                .filter_map(|file| FcParseFont(file))
                .collect::<Vec<Vec<_>>>()
        }
        #[cfg(not(all(feature = "multithreading", not(target_family = "wasm"))))]
        {
            files_to_parse
                .iter()
                .filter_map(|file| FcParseFont(file))
                .collect::<Vec<Vec<_>>>()
        }
    };

    result.into_iter().flat_map(|f| f.into_iter()).collect()
}

#[cfg(all(feature = "std", feature = "parsing"))]
/// Takes a path & prefix and resolves them to a usable path, or `None` if they're unsupported/unavailable.
///
/// Behaviour is based on: https://www.freedesktop.org/software/fontconfig/fontconfig-user.html
fn process_path(
    prefix: &Option<String>,
    mut path: PathBuf,
    is_include_path: bool,
) -> Option<PathBuf> {
    use std::env::var;

    const HOME_SHORTCUT: &str = "~";
    const CWD_PATH: &str = ".";

    const HOME_ENV_VAR: &str = "HOME";
    const XDG_CONFIG_HOME_ENV_VAR: &str = "XDG_CONFIG_HOME";
    const XDG_CONFIG_HOME_DEFAULT_PATH_SUFFIX: &str = ".config";
    const XDG_DATA_HOME_ENV_VAR: &str = "XDG_DATA_HOME";
    const XDG_DATA_HOME_DEFAULT_PATH_SUFFIX: &str = ".local/share";

    const PREFIX_CWD: &str = "cwd";
    const PREFIX_DEFAULT: &str = "default";
    const PREFIX_XDG: &str = "xdg";

    // These three could, in theory, be cached, but the work required to do so outweighs the minor benefits
    fn get_home_value() -> Option<PathBuf> {
        var(HOME_ENV_VAR).ok().map(PathBuf::from)
    }
    fn get_xdg_config_home_value() -> Option<PathBuf> {
        var(XDG_CONFIG_HOME_ENV_VAR)
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                get_home_value()
                    .map(|home_path| home_path.join(XDG_CONFIG_HOME_DEFAULT_PATH_SUFFIX))
            })
    }
    fn get_xdg_data_home_value() -> Option<PathBuf> {
        var(XDG_DATA_HOME_ENV_VAR)
            .ok()
            .map(PathBuf::from)
            .or_else(|| {
                get_home_value().map(|home_path| home_path.join(XDG_DATA_HOME_DEFAULT_PATH_SUFFIX))
            })
    }

    // Resolve the tilde character in the path, if present
    if path.starts_with(HOME_SHORTCUT) {
        if let Some(home_path) = get_home_value() {
            path = home_path.join(
                path.strip_prefix(HOME_SHORTCUT)
                    .expect("already checked that it starts with the prefix"),
            );
        } else {
            return None;
        }
    }

    // Resolve prefix values
    match prefix {
        Some(prefix) => match prefix.as_str() {
            PREFIX_CWD | PREFIX_DEFAULT => {
                let mut new_path = PathBuf::from(CWD_PATH);
                new_path.push(path);

                Some(new_path)
            }
            PREFIX_XDG => {
                if is_include_path {
                    get_xdg_config_home_value()
                        .map(|xdg_config_home_path| xdg_config_home_path.join(path))
                } else {
                    get_xdg_data_home_value()
                        .map(|xdg_data_home_path| xdg_data_home_path.join(path))
                }
            }
            _ => None, // Unsupported prefix
        },
        None => Some(path),
    }
}

// Helper function to extract a string from the name table
#[cfg(all(feature = "std", feature = "parsing"))]
fn get_name_string(name_data: &[u8], name_id: u16) -> Option<String> {
    fontcode_get_name(name_data, name_id)
        .ok()
        .flatten()
        .map(|name| String::from_utf8_lossy(name.to_bytes()).to_string())
}

/// Representative test codepoints for each Unicode block.
/// These are carefully chosen to be actual script characters (not punctuation/symbols)
/// that a font claiming to support this script should definitely have.
#[cfg(all(feature = "std", feature = "parsing"))]
fn get_verification_codepoints(start: u32, end: u32) -> Vec<u32> {
    match start {
        // Basic Latin - test uppercase, lowercase, and digits
        0x0000 => vec!['A' as u32, 'M' as u32, 'Z' as u32, 'a' as u32, 'm' as u32, 'z' as u32],
        // Latin-1 Supplement - common accented letters
        0x0080 => vec![0x00C0, 0x00C9, 0x00D1, 0x00E0, 0x00E9, 0x00F1], // À É Ñ à é ñ
        // Latin Extended-A
        0x0100 => vec![0x0100, 0x0110, 0x0141, 0x0152, 0x0160], // Ā Đ Ł Œ Š
        // Latin Extended-B
        0x0180 => vec![0x0180, 0x01A0, 0x01B0, 0x01CD], // ƀ Ơ ư Ǎ
        // IPA Extensions
        0x0250 => vec![0x0250, 0x0259, 0x026A, 0x0279], // ɐ ə ɪ ɹ
        // Greek and Coptic
        0x0370 => vec![0x0391, 0x0392, 0x0393, 0x03B1, 0x03B2, 0x03C9], // Α Β Γ α β ω
        // Cyrillic
        0x0400 => vec![0x0410, 0x0411, 0x0412, 0x0430, 0x0431, 0x042F], // А Б В а б Я
        // Armenian
        0x0530 => vec![0x0531, 0x0532, 0x0533, 0x0561, 0x0562], // Ա Բ Գ ա բ
        // Hebrew
        0x0590 => vec![0x05D0, 0x05D1, 0x05D2, 0x05E9, 0x05EA], // א ב ג ש ת
        // Arabic
        0x0600 => vec![0x0627, 0x0628, 0x062A, 0x062C, 0x0645], // ا ب ت ج م
        // Syriac
        0x0700 => vec![0x0710, 0x0712, 0x0713, 0x0715], // ܐ ܒ ܓ ܕ
        // Devanagari
        0x0900 => vec![0x0905, 0x0906, 0x0915, 0x0916, 0x0939], // अ आ क ख ह
        // Bengali
        0x0980 => vec![0x0985, 0x0986, 0x0995, 0x0996], // অ আ ক খ
        // Gurmukhi
        0x0A00 => vec![0x0A05, 0x0A06, 0x0A15, 0x0A16], // ਅ ਆ ਕ ਖ
        // Gujarati
        0x0A80 => vec![0x0A85, 0x0A86, 0x0A95, 0x0A96], // અ આ ક ખ
        // Oriya
        0x0B00 => vec![0x0B05, 0x0B06, 0x0B15, 0x0B16], // ଅ ଆ କ ଖ
        // Tamil
        0x0B80 => vec![0x0B85, 0x0B86, 0x0B95, 0x0BA4], // அ ஆ க த
        // Telugu
        0x0C00 => vec![0x0C05, 0x0C06, 0x0C15, 0x0C16], // అ ఆ క ఖ
        // Kannada
        0x0C80 => vec![0x0C85, 0x0C86, 0x0C95, 0x0C96], // ಅ ಆ ಕ ಖ
        // Malayalam
        0x0D00 => vec![0x0D05, 0x0D06, 0x0D15, 0x0D16], // അ ആ ക ഖ
        // Thai
        0x0E00 => vec![0x0E01, 0x0E02, 0x0E04, 0x0E07, 0x0E40], // ก ข ค ง เ
        // Lao
        0x0E80 => vec![0x0E81, 0x0E82, 0x0E84, 0x0E87], // ກ ຂ ຄ ງ
        // Myanmar
        0x1000 => vec![0x1000, 0x1001, 0x1002, 0x1010, 0x1019], // က ခ ဂ တ မ
        // Georgian
        0x10A0 => vec![0x10D0, 0x10D1, 0x10D2, 0x10D3], // ა ბ გ დ
        // Hangul Jamo
        0x1100 => vec![0x1100, 0x1102, 0x1103, 0x1161, 0x1162], // ᄀ ᄂ ᄃ ᅡ ᅢ
        // Ethiopic
        0x1200 => vec![0x1200, 0x1208, 0x1210, 0x1218], // ሀ ለ ሐ መ
        // Cherokee
        0x13A0 => vec![0x13A0, 0x13A1, 0x13A2, 0x13A3], // Ꭰ Ꭱ Ꭲ Ꭳ
        // Khmer
        0x1780 => vec![0x1780, 0x1781, 0x1782, 0x1783], // ក ខ គ ឃ
        // Mongolian
        0x1800 => vec![0x1820, 0x1821, 0x1822, 0x1823], // ᠠ ᠡ ᠢ ᠣ
        // Hiragana
        0x3040 => vec![0x3042, 0x3044, 0x3046, 0x304B, 0x304D, 0x3093], // あ い う か き ん
        // Katakana
        0x30A0 => vec![0x30A2, 0x30A4, 0x30A6, 0x30AB, 0x30AD, 0x30F3], // ア イ ウ カ キ ン
        // Bopomofo
        0x3100 => vec![0x3105, 0x3106, 0x3107, 0x3108], // ㄅ ㄆ ㄇ ㄈ
        // CJK Unified Ideographs - common characters
        0x4E00 => vec![0x4E00, 0x4E2D, 0x4EBA, 0x5927, 0x65E5, 0x6708], // 一 中 人 大 日 月
        // Hangul Syllables
        0xAC00 => vec![0xAC00, 0xAC01, 0xAC04, 0xB098, 0xB2E4], // 가 각 간 나 다
        // CJK Compatibility Ideographs
        0xF900 => vec![0xF900, 0xF901, 0xF902], // 豈 更 車
        // Arabic Presentation Forms-A
        0xFB50 => vec![0xFB50, 0xFB51, 0xFB52, 0xFB56], // ﭐ ﭑ ﭒ ﭖ
        // Arabic Presentation Forms-B
        0xFE70 => vec![0xFE70, 0xFE72, 0xFE74, 0xFE76], // ﹰ ﹲ ﹴ ﹶ
        // Halfwidth and Fullwidth Forms
        0xFF00 => vec![0xFF01, 0xFF21, 0xFF41, 0xFF61], // ！ Ａ ａ ｡
        // Default: sample at regular intervals
        _ => {
            let range_size = end - start;
            if range_size > 20 {
                vec![
                    start + range_size / 5,
                    start + 2 * range_size / 5,
                    start + 3 * range_size / 5,
                    start + 4 * range_size / 5,
                ]
            } else {
                vec![start, start + range_size / 2]
            }
        }
    }
}

/// Find the best Unicode CMAP subtable from a font provider.
/// Tries multiple platform/encoding combinations in priority order.
#[cfg(all(feature = "std", feature = "parsing"))]
fn find_best_cmap_subtable<'a>(
    cmap: &allsorts::tables::cmap::Cmap<'a>,
) -> Option<allsorts::tables::cmap::EncodingRecord> {
    use allsorts::tables::cmap::{PlatformId, EncodingId};

    cmap.find_subtable(PlatformId::UNICODE, EncodingId(3))
        .or_else(|| cmap.find_subtable(PlatformId::UNICODE, EncodingId(4)))
        .or_else(|| cmap.find_subtable(PlatformId::WINDOWS, EncodingId(1)))
        .or_else(|| cmap.find_subtable(PlatformId::WINDOWS, EncodingId(10)))
        .or_else(|| cmap.find_subtable(PlatformId::UNICODE, EncodingId(0)))
        .or_else(|| cmap.find_subtable(PlatformId::UNICODE, EncodingId(1)))
}

/// Verify OS/2 reported Unicode ranges against actual CMAP support.
/// Returns only ranges that are actually supported by the font's CMAP table.
#[cfg(all(feature = "std", feature = "parsing"))]
fn verify_unicode_ranges_with_cmap(
    provider: &impl FontTableProvider,
    os2_ranges: Vec<UnicodeRange>
) -> Vec<UnicodeRange> {
    use allsorts::tables::cmap::{Cmap, CmapSubtable};

    if os2_ranges.is_empty() {
        return Vec::new();
    }

    // Try to get CMAP subtable
    let cmap_data = match provider.table_data(tag::CMAP) {
        Ok(Some(data)) => data,
        _ => return os2_ranges, // Can't verify, trust OS/2
    };

    let cmap = match ReadScope::new(&cmap_data).read::<Cmap<'_>>() {
        Ok(c) => c,
        Err(_) => return os2_ranges,
    };

    let encoding_record = match find_best_cmap_subtable(&cmap) {
        Some(r) => r,
        None => return os2_ranges, // No suitable subtable, trust OS/2
    };

    let cmap_subtable = match ReadScope::new(&cmap_data)
        .offset(encoding_record.offset as usize)
        .read::<CmapSubtable<'_>>()
    {
        Ok(st) => st,
        Err(_) => return os2_ranges,
    };

    // Verify each range
    let mut verified_ranges = Vec::new();

    for range in os2_ranges {
        let test_codepoints = get_verification_codepoints(range.start, range.end);

        // Require at least 50% of test codepoints to have valid glyphs
        // This is stricter than before to avoid false positives
        let required_hits = (test_codepoints.len() + 1) / 2; // ceil(len/2)
        let mut hits = 0;

        for cp in test_codepoints {
            if cp >= range.start && cp <= range.end {
                if let Ok(Some(gid)) = cmap_subtable.map_glyph(cp) {
                    if gid != 0 {
                        hits += 1;
                        if hits >= required_hits {
                            break;
                        }
                    }
                }
            }
        }

        if hits >= required_hits {
            verified_ranges.push(range);
        }
    }

    verified_ranges
}

/// Analyze CMAP table to discover font coverage when OS/2 provides no info.
/// This is the fallback when OS/2 ulUnicodeRange bits are all zero.
#[cfg(all(feature = "std", feature = "parsing"))]
fn analyze_cmap_coverage(provider: &impl FontTableProvider) -> Option<Vec<UnicodeRange>> {
    use allsorts::tables::cmap::{Cmap, CmapSubtable};

    let cmap_data = provider.table_data(tag::CMAP).ok()??;
    let cmap = ReadScope::new(&cmap_data).read::<Cmap<'_>>().ok()?;

    let encoding_record = find_best_cmap_subtable(&cmap)?;

    let cmap_subtable = ReadScope::new(&cmap_data)
        .offset(encoding_record.offset as usize)
        .read::<CmapSubtable<'_>>()
        .ok()?;

    // Standard Unicode blocks to probe
    let blocks_to_check: &[(u32, u32)] = &[
        (0x0000, 0x007F), // Basic Latin
        (0x0080, 0x00FF), // Latin-1 Supplement
        (0x0100, 0x017F), // Latin Extended-A
        (0x0180, 0x024F), // Latin Extended-B
        (0x0250, 0x02AF), // IPA Extensions
        (0x0300, 0x036F), // Combining Diacritical Marks
        (0x0370, 0x03FF), // Greek and Coptic
        (0x0400, 0x04FF), // Cyrillic
        (0x0500, 0x052F), // Cyrillic Supplement
        (0x0530, 0x058F), // Armenian
        (0x0590, 0x05FF), // Hebrew
        (0x0600, 0x06FF), // Arabic
        (0x0700, 0x074F), // Syriac
        (0x0900, 0x097F), // Devanagari
        (0x0980, 0x09FF), // Bengali
        (0x0A00, 0x0A7F), // Gurmukhi
        (0x0A80, 0x0AFF), // Gujarati
        (0x0B00, 0x0B7F), // Oriya
        (0x0B80, 0x0BFF), // Tamil
        (0x0C00, 0x0C7F), // Telugu
        (0x0C80, 0x0CFF), // Kannada
        (0x0D00, 0x0D7F), // Malayalam
        (0x0E00, 0x0E7F), // Thai
        (0x0E80, 0x0EFF), // Lao
        (0x1000, 0x109F), // Myanmar
        (0x10A0, 0x10FF), // Georgian
        (0x1100, 0x11FF), // Hangul Jamo
        (0x1200, 0x137F), // Ethiopic
        (0x13A0, 0x13FF), // Cherokee
        (0x1780, 0x17FF), // Khmer
        (0x1800, 0x18AF), // Mongolian
        (0x2000, 0x206F), // General Punctuation
        (0x20A0, 0x20CF), // Currency Symbols
        (0x2100, 0x214F), // Letterlike Symbols
        (0x2190, 0x21FF), // Arrows
        (0x2200, 0x22FF), // Mathematical Operators
        (0x2500, 0x257F), // Box Drawing
        (0x25A0, 0x25FF), // Geometric Shapes
        (0x2600, 0x26FF), // Miscellaneous Symbols
        (0x3000, 0x303F), // CJK Symbols and Punctuation
        (0x3040, 0x309F), // Hiragana
        (0x30A0, 0x30FF), // Katakana
        (0x3100, 0x312F), // Bopomofo
        (0x3130, 0x318F), // Hangul Compatibility Jamo
        (0x4E00, 0x9FFF), // CJK Unified Ideographs
        (0xAC00, 0xD7AF), // Hangul Syllables
        (0xF900, 0xFAFF), // CJK Compatibility Ideographs
        (0xFB50, 0xFDFF), // Arabic Presentation Forms-A
        (0xFE70, 0xFEFF), // Arabic Presentation Forms-B
        (0xFF00, 0xFFEF), // Halfwidth and Fullwidth Forms
    ];

    let mut ranges = Vec::new();

    for &(start, end) in blocks_to_check {
        let test_codepoints = get_verification_codepoints(start, end);
        let required_hits = (test_codepoints.len() + 1) / 2;
        let mut hits = 0;

        for cp in test_codepoints {
            if let Ok(Some(gid)) = cmap_subtable.map_glyph(cp) {
                if gid != 0 {
                    hits += 1;
                    if hits >= required_hits {
                        break;
                    }
                }
            }
        }

        if hits >= required_hits {
            ranges.push(UnicodeRange { start, end });
        }
    }

    if ranges.is_empty() {
        None
    } else {
        Some(ranges)
    }
}

// Helper function to extract unicode ranges (unused, kept for reference)
#[cfg(all(feature = "std", feature = "parsing"))]
#[allow(dead_code)]
fn extract_unicode_ranges(os2_table: &Os2) -> Vec<UnicodeRange> {
    let mut unicode_ranges = Vec::new();

    let ranges = [
        os2_table.ul_unicode_range1,
        os2_table.ul_unicode_range2,
        os2_table.ul_unicode_range3,
        os2_table.ul_unicode_range4,
    ];

    for &(bit, start, end) in UNICODE_RANGE_MAPPINGS {
        let range_idx = bit / 32;
        let bit_pos = bit % 32;
        if range_idx < 4 && (ranges[range_idx] & (1 << bit_pos)) != 0 {
            unicode_ranges.push(UnicodeRange { start, end });
        }
    }

    unicode_ranges
}

// Helper function to detect if a font is monospace
#[cfg(all(feature = "std", feature = "parsing"))]
fn detect_monospace(
    provider: &impl FontTableProvider,
    os2_table: &Os2,
    detected_monospace: Option<bool>,
) -> Option<bool> {
    if let Some(is_monospace) = detected_monospace {
        return Some(is_monospace);
    }

    // Try using PANOSE classification
    if os2_table.panose[0] == 2 {
        // 2 = Latin Text
        return Some(os2_table.panose[3] == 9); // 9 = Monospaced
    }

    // Check glyph widths in hmtx table
    let hhea_data = provider.table_data(tag::HHEA).ok()??;
    let hhea_table = ReadScope::new(&hhea_data).read::<HheaTable>().ok()?;
    let maxp_data = provider.table_data(tag::MAXP).ok()??;
    let maxp_table = ReadScope::new(&maxp_data).read::<MaxpTable>().ok()?;
    let hmtx_data = provider.table_data(tag::HMTX).ok()??;
    let hmtx_table = ReadScope::new(&hmtx_data)
        .read_dep::<HmtxTable<'_>>((
            usize::from(maxp_table.num_glyphs),
            usize::from(hhea_table.num_h_metrics),
        ))
        .ok()?;

    let mut monospace = true;
    let mut last_advance = 0;

    // Check if all advance widths are the same
    for i in 0..hhea_table.num_h_metrics as usize {
        let advance = hmtx_table.h_metrics.read_item(i).ok()?.advance_width;
        if i > 0 && advance != last_advance {
            monospace = false;
            break;
        }
        last_advance = advance;
    }

    Some(monospace)
}

/// Guess font metadata from a filename using the existing tokenizer.
///
/// Uses [`config::tokenize_font_stem`] and [`config::FONT_STYLE_TOKENS`]
/// to extract the family name and detect style hints from the filename.
///
/// Only compiled for the filename-only (`not(parsing)`) scan path — its
/// sole caller is [`FcFontCache::build_from_filenames`]. With `parsing`
/// on, allsorts reads real metadata and this fallback is unused.
#[cfg(all(feature = "std", not(feature = "parsing")))]
fn pattern_from_filename(path: &std::path::Path) -> Option<FcPattern> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "ttf" | "otf" | "ttc" | "woff" | "woff2" => {}
        _ => return None,
    }

    let stem = path.file_stem()?.to_str()?;
    let all_tokens = crate::config::tokenize_lowercase(stem);

    // Style detection: check if any token matches a known style keyword
    let has_token = |kw: &str| all_tokens.iter().any(|t| t == kw);
    let is_bold = has_token("bold") || has_token("heavy");
    let is_italic = has_token("italic");
    let is_oblique = has_token("oblique");
    let is_mono = has_token("mono") || has_token("monospace");
    let is_condensed = has_token("condensed");

    // Family = non-style tokens joined
    let family_tokens = crate::config::tokenize_font_stem(stem);
    if family_tokens.is_empty() { return None; }
    let family = family_tokens.join(" ");

    Some(FcPattern {
        name: Some(stem.to_string()),
        family: Some(family),
        bold: if is_bold { PatternMatch::True } else { PatternMatch::False },
        italic: if is_italic { PatternMatch::True } else { PatternMatch::False },
        oblique: if is_oblique { PatternMatch::True } else { PatternMatch::DontCare },
        monospace: if is_mono { PatternMatch::True } else { PatternMatch::DontCare },
        condensed: if is_condensed { PatternMatch::True } else { PatternMatch::DontCare },
        weight: if is_bold { FcWeight::Bold } else { FcWeight::Normal },
        stretch: if is_condensed { FcStretch::Condensed } else { FcStretch::Normal },
        unicode_ranges: Vec::new(),
        metadata: FcFontMetadata::default(),
        render_config: FcFontRenderConfig::default(),
    })
}

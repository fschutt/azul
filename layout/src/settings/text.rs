//! The settings of text: shaping, the fonts and the glyphs the CPU rasteriser draws.
//!
//! A `FontManager` owns one value (`FontManager::settings`), the shaping cache of a pass takes a
//! copy from it (`TextShapingCache::set_settings`), and the glyph cache of a window takes one
//! for the rasteriser (`GlyphCache::set_text_settings`). A manager made without settings has
//! the defaults, which is the behaviour of an empty environment: a library user (a PDF writer)
//! gets the shipped text, not a degraded one.
//!
//! The variables of the group are named in this file only; the code that uses a setting reads a
//! plain field.

use azul_css::env::EnvSource;

/// The settings of text.
///
/// `Default` is the behaviour of an empty environment: no trace, hinting by the platform default
/// (off on macOS, where CoreText never runs TrueType instructions; on elsewhere), light hinting
/// and sub-pixel glyph placement on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct TextSettings {
    /// Pagination and font-chain timings to stderr (`AZ_PAGINATE_TRACE`).
    pub trace_paginate: bool,
    /// Trace the font resolution and the text flow to stderr (`TEXTDBG`).
    pub trace_text: bool,
    /// Trace the fast font-chain resolver to stderr (`AZ_FAST_RESOLVE_DEBUG`).
    pub trace_font_resolve: bool,
    /// Trace the garbage collection of fonts to stderr (`AZ_FONT_GC_TRACE`).
    pub trace_font_gc: bool,
    /// Trace the font-fallback segmentation of shaped text to stderr
    /// (`AZ_FONT_FALLBACK_DEBUG`).
    pub trace_font_fallback: bool,
    /// The negative-control knob of the shaping-cache identity test (`AZ_T2_SKIP_RESTAMP`): a
    /// hit hands back the cached group unmodified, the very defect the re-stamp fixed. Production
    /// never sets it.
    pub skip_restamp: bool,
    /// Run the font's TrueType hinting bytecode at all (`AZ_TEXT_HINTING`: `0` or `false` off,
    /// anything else on). The platform default is OFF on macOS and ON elsewhere.
    pub hinting: bool,
    /// Light hinting, CoreText style: the grid-fitted Y with the unhinted fractional X
    /// (`AZ_HINT_LIGHT`: `0` or `false` is the Windows-style full grid-fit). On by default.
    pub hint_light: bool,
    /// Place each glyph origin at a sub-pixel horizontal position instead of a whole pixel
    /// (`AZ_TEXT_SUBPIXEL`: `0` or `false` snaps). On by default.
    pub subpixel: bool,
}

/// Whether the TrueType hinting bytecode runs when nothing says: CoreText never executes
/// TrueType instructions (no macOS browser hints text), and running the untuned programs of the
/// Apple system fonts snapped single glyphs' stems and x-heights to other pixel rows than their
/// neighbours. `FreeType` and `ClearType` conventions expect grid-fitting, so Linux and Windows
/// keep it.
const PLATFORM_HINTING: bool = cfg!(not(target_os = "macos"));

/// The value of a switch that is on unless it says `0` or `false` (in any case).
fn on_unless_off(value: &str) -> bool {
    !(value == "0" || value.eq_ignore_ascii_case("false"))
}

impl Default for TextSettings {
    fn default() -> Self {
        Self {
            trace_paginate: false,
            trace_text: false,
            trace_font_resolve: false,
            trace_font_gc: false,
            trace_font_fallback: false,
            skip_restamp: false,
            hinting: PLATFORM_HINTING,
            hint_light: true,
            subpixel: true,
        }
    }
}

impl TextSettings {
    /// The settings `env` asks for. A trace is on when its variable is set to anything, an empty
    /// value included; the three glyph switches read their value.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            trace_paginate: env.is_set("AZ_PAGINATE_TRACE"),
            trace_text: env.is_set("TEXTDBG"),
            trace_font_resolve: env.is_set("AZ_FAST_RESOLVE_DEBUG"),
            trace_font_gc: env.is_set("AZ_FONT_GC_TRACE"),
            trace_font_fallback: env.is_set("AZ_FONT_FALLBACK_DEBUG"),
            skip_restamp: env.is_set("AZ_T2_SKIP_RESTAMP"),
            hinting: env
                .var("AZ_TEXT_HINTING")
                .map_or(PLATFORM_HINTING, |v| on_unless_off(&v)),
            hint_light: env.var("AZ_HINT_LIGHT").is_none_or(|v| on_unless_off(&v)),
            subpixel: env.var("AZ_TEXT_SUBPIXEL").is_none_or(|v| on_unless_off(&v)),
        }
    }
}

#[cfg(test)]
#[path = "text_test.rs"]
mod tests;

//! The switches of the CPU renderer that the environment sets (group L3 of the env ledger), and
//! the one place of the crate that names their variables.
//!
//! Nothing here reads the process: every constructor takes a `&dyn EnvSource`
//! (`azul_css::env`), and `Default` is the behaviour of an EMPTY environment, the switches that
//! are on by default included (`AZ_CPU_GRID`, `AZ_CPU_TURNED_TEXT`, `AZ_DL_DIFF_REFINE` and
//! `AZ_HALO_SLOTS` are on unless a variable turns them off). The dll builds the settings once
//! at the start of the process; a window carries them in `LayoutWindow::settings`, and the CPU
//! backend takes them from the window at every frame.

use std::path::PathBuf;

use azul_css::env::{EnvSource, NoEnv};

/// `AZ_CPU_THREADS` as a thread count: unset, empty, zero or not a number is `None`.
#[must_use]
pub(crate) fn parse_threads(v: Option<&str>) -> Option<usize> {
    v.and_then(|s| s.trim().parse::<usize>().ok()).filter(|n| *n > 0)
}

/// `AZ_CPU_RETAINED_MB` as a byte count: unset, empty or not a number is `None`.
#[must_use]
pub(crate) fn parse_megabytes(v: Option<&str>) -> Option<usize> {
    v.and_then(|s| s.trim().parse::<usize>().ok())
        .map(|mb| mb.saturating_mul(1024 * 1024))
}

/// `AZ_DEBUG_FILL` as the colour a damaged region is cleared to: `1` is red, `0` and the empty
/// text are white (the shipping colour), a hexadecimal number (`RRGGBB`, with or without a
/// `#`) is that colour, and anything else is red, so that whoever asked for a debug fill gets a
/// visible one.
#[must_use]
pub(crate) fn parse_damage_fill(v: Option<&str>) -> (u8, u8, u8, u8) {
    const WHITE: (u8, u8, u8, u8) = (255, 255, 255, 255);
    const RED: (u8, u8, u8, u8) = (255, 0, 0, 255);
    match v {
        None => WHITE,
        // Unset is the shipping default; "0" turns the knob off explicitly
        // without having to unset it.
        Some("0" | "") => WHITE,
        Some("1") => RED,
        Some(v) => u32::from_str_radix(v.trim_start_matches('#'), 16).map_or(RED, |n| {
            (
                ((n >> 16) & 0xff) as u8,
                ((n >> 8) & 0xff) as u8,
                (n & 0xff) as u8,
                255,
            )
        }),
    }
}

/// The knobs the walks, the damage passes and the compositor of the CPU renderer are asked:
/// what a frame costs or says, and (`debug_fill`) what a damaged region is cleared to. A small
/// `Copy` value, so that it travels with the state of a frame (`CpuRenderState`) and into the
/// threads that paint its bands.
#[allow(clippy::struct_excessive_bools)] // one independent switch per variable
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaintSwitches {
    /// `AZ_CPU_TURNED_TEXT`: turned text frames are drawn through their matrix (the default),
    /// not into a surface that is composited (`AZ_CPU_TURNED_TEXT=group`, the path before, for
    /// comparing the two).
    pub turned_text_direct: bool,
    /// `AZ_CPU_GRID`: a walk asks the grids of a list's levels which children can paint (the
    /// default); with `off` or `0` every walk tests every item.
    pub item_grid: bool,
    /// `AZ_DL_DIFF_REFINE`: the damage refinements of the display-list diff (skipping the items
    /// that paint nothing, the strips of a resized rect) are on; `0` or `off` is the kill
    /// switch.
    pub dl_diff_refine: bool,
    /// `AZ_DEBUG_FILL`: the colour a damaged region is cleared to before it is repainted
    /// (`None`: the window's own clear colour). Whatever is still that colour at the end of the
    /// frame was cleared but never repainted.
    pub debug_fill: Option<(u8, u8, u8, u8)>,
    /// `AZ_DEBUG_DAMAGE` is set: log what each damaged repaint touched.
    pub debug_damage: bool,
    /// `AZ_CLIP_DEBUG` is set: report a `PopClip` without a `PushClip`.
    pub clip_debug: bool,
    /// `AZ_BLIT_DEBUG` is set: trace the blits of moved pixels and the display-list diff.
    pub blit_debug: bool,
    /// `AZ_MAP_DEBUG` is set: trace the layers and the view items a map paints.
    pub map_debug: bool,
}

impl PaintSwitches {
    /// What the environment `env` asks for; an empty environment asks for the shipping paths
    /// and no trace.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            turned_text_direct: env.var("AZ_CPU_TURNED_TEXT").is_none_or(|v| v != "group"),
            item_grid: env.var("AZ_CPU_GRID").is_none_or(|v| v != "off" && v != "0"),
            dl_diff_refine: env
                .var("AZ_DL_DIFF_REFINE")
                .is_none_or(|v| !(v == "0" || v.eq_ignore_ascii_case("off"))),
            debug_fill: env
                .is_set("AZ_DEBUG_FILL")
                .then(|| parse_damage_fill(env.var("AZ_DEBUG_FILL").as_deref())),
            debug_damage: env.is_set("AZ_DEBUG_DAMAGE"),
            clip_debug: env.var("AZ_CLIP_DEBUG").is_some(),
            blit_debug: env.is_set("AZ_BLIT_DEBUG"),
            map_debug: env.var("AZ_MAP_DEBUG").is_some(),
        }
    }
}

impl Default for PaintSwitches {
    /// The behaviour of an empty environment.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

/// What the environment asks of the CPU renderer of a window: the paint switches, the
/// repaint's thread budget, the bound of the kept group surfaces, the text halos, and the dumps
/// of display lists and frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSettings {
    /// The knobs of the walks, the damage passes and the compositor.
    pub paint: PaintSwitches,
    /// `AZ_CPU_FULL=layers`: a full repaint goes through the layered compositor, as it did
    /// before the walk painted full frames in bands (for comparing the two).
    pub full_in_layers: bool,
    /// `AZ_CPU_THREADS`: the most threads a repaint uses (`1` paints everything on the calling
    /// thread); `None` is the default (4, and never more than the machine has).
    pub threads: Option<usize>,
    /// `AZ_CPU_RETAINED_MB`, in bytes: the bound of the kept group surfaces (`0` keeps
    /// nothing); `None` is the default bound of `RetainedFrames`.
    pub retained_bytes: Option<usize>,
    /// `AZ_HALO_SLOTS`: a halo's own picture takes the place of the picture that stood in for
    /// it (the default); `0` turns it off.
    pub halo_slots: bool,
    /// `AZ_HALO_STATS`: one line on stderr for every scan of the text halos that drew,
    /// dropped, left or stood in for a halo (set, not empty and not `0`).
    pub halo_stats: bool,
    /// `AZ_DUMP_DL_DIR`: where the first display lists of a window go as JSON (`None`: nowhere).
    pub dump_display_lists: Option<PathBuf>,
    /// `AZ_DUMP_FRAME_DIR`: where every rendered frame of a window goes as a numbered PNG
    /// (`None`: nowhere).
    pub dump_frames: Option<PathBuf>,
}

impl RenderSettings {
    /// What the environment `env` asks for; an empty environment asks for the shipping
    /// behaviour and no dump.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            paint: PaintSwitches::from_lookup(env),
            full_in_layers: env.var("AZ_CPU_FULL").is_some_and(|v| v == "layers"),
            threads: parse_threads(env.var("AZ_CPU_THREADS").as_deref()),
            retained_bytes: parse_megabytes(env.var("AZ_CPU_RETAINED_MB").as_deref()),
            halo_slots: env.var("AZ_HALO_SLOTS").is_none_or(|v| v != "0"),
            halo_stats: env
                .var("AZ_HALO_STATS")
                .is_some_and(|v| !v.is_empty() && v != "0"),
            dump_display_lists: env.var_os("AZ_DUMP_DL_DIR").map(PathBuf::from),
            dump_frames: env.var_os("AZ_DUMP_FRAME_DIR").map(PathBuf::from),
        }
    }
}

impl Default for RenderSettings {
    /// The behaviour of an empty environment.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "render_test.rs"]
mod tests;

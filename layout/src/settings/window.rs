//! The switches of the layout window that the environment sets (group L4 of the env ledger), and
//! the one place of the crate that names their variables.
//!
//! Nothing here reads the process: every constructor takes a `&dyn EnvSource`
//! (`azul_css::env`), and `Default` is the behaviour of an EMPTY environment, the switches that
//! are on by default included (`AZ_DIFF_FIRST` runs every step, `AZ_EAGER_STATE_COLORS` and
//! `AZ_MOVE_IN_PLACE` are on unless a variable says `0`). The dll builds the settings once at the
//! start of the process and hands them to every window (`LayoutWindow::set_settings`); the window
//! and the managers it owns read plain fields.

use std::path::PathBuf;

use azul_css::env::{EnvSource, NoEnv};

/// What the environment asks of a layout window: which of the alternative paths of a rebuild
/// are on, and which traces and verifications run.
#[allow(clippy::struct_excessive_bools)] // one independent switch per variable
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSettings {
    /// `AZ_DIFF_FIRST`, as written (`None`: unset, which is every step): the steps of the
    /// diff-first rebuild that are on (`window::diff_first_steps_of`).
    pub diff_first: Option<String>,
    /// `AZ_EAGER_STATE_COLORS`: a build of a display list binds the background of every node
    /// that has a `:hover` / `:active` / `:focus` background to a colour key from the start
    /// (the default); `0` binds a node when a state first paints it.
    pub eager_state_colors: bool,
    /// `AZ_MOVE_IN_PLACE`: a write of `left` / `top` alone moves the node's boxes and items
    /// where they lie (the default); `0` lays the document out.
    pub move_in_place: bool,
    /// `AZ_NATURAL_SCROLL`: the scroll direction the environment pins, which outranks the app's
    /// and the platform's choice (`None`: unset; the values `1`, `true` and `TRUE` are natural,
    /// every other value is the traditional direction).
    pub natural_scroll: Option<bool>,
    /// `AZ_PATCH_VERIFY` is set: after every layout pass a patched display list is compared
    /// with the wholesale build, and a mismatch is a panic.
    pub patch_verify: bool,
    /// `AZ_PATCH_VERIFY_DUMP`: where both lists go, in full, when `patch_verify` finds a
    /// mismatch (`None`: nowhere).
    pub patch_verify_dump: Option<PathBuf>,
    /// `AZ_MALLOC_TRIM=1`: `malloc_trim` after a relayout.
    pub malloc_trim: bool,
    /// `AZ_MAP_DEBUG` is set: trace the map widget's view scan and placeholder swaps.
    pub map_debug: bool,
    /// `AZ_FOCUS_DEBUG` is set: trace the finalisation of focus changes.
    pub focus_debug: bool,
    /// `AZ_DENSE_TEXT=verify`: every dense cursor step is compared with the sparse one.
    pub dense_text_verify: bool,
    /// `AZ_ANIM_DEBUG` is set: trace animation and tween steps.
    pub anim_debug: bool,
    /// `AZ_FONT_FALLBACK_DEBUG` is set: trace the font-fallback segmentation at edit time.
    pub font_fallback_debug: bool,
    /// `AZ_SCROLL_CONTRACT_TRACE` is set: trace the planning of scroll contracts.
    pub scroll_contract_trace: bool,
    /// `AZ_VV_RECONCILE_TRACE` is set: trace the reconcile of a re-rendered virtual view.
    pub vv_reconcile_trace: bool,
    /// `AZ_VV_TRACE` is set: one line per virtual-view render.
    pub vv_trace: bool,
    /// `AZ_TRACE_HIT` is set: print every hit-test query's hits, front to back.
    pub trace_hit: bool,
    /// `AZ_WALK_SCROLL_BOXES` is set: every frame walks every box of every tree, as before the
    /// list of scroll-driven boxes existed.
    pub walk_scroll_boxes: bool,
    /// `AZ_SCROLL_DEBUG=1`: a per-event and per-tick trace of the scroll pipeline.
    pub scroll_debug: bool,
}

impl WindowSettings {
    /// What the environment `env` asks for; an empty environment asks for the shipping paths
    /// and no trace.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            diff_first: env.var("AZ_DIFF_FIRST"),
            eager_state_colors: env.var("AZ_EAGER_STATE_COLORS").is_none_or(|v| v != "0"),
            move_in_place: env.var("AZ_MOVE_IN_PLACE").is_none_or(|v| v != "0"),
            natural_scroll: env.is_set("AZ_NATURAL_SCROLL").then(|| {
                matches!(
                    env.var("AZ_NATURAL_SCROLL").as_deref(),
                    Some("1" | "true" | "TRUE")
                )
            }),
            patch_verify: env.is_set("AZ_PATCH_VERIFY"),
            patch_verify_dump: env.var_os("AZ_PATCH_VERIFY_DUMP").map(PathBuf::from),
            malloc_trim: env.var("AZ_MALLOC_TRIM").as_deref() == Some("1"),
            map_debug: env.is_set("AZ_MAP_DEBUG"),
            focus_debug: env.is_set("AZ_FOCUS_DEBUG"),
            dense_text_verify: env.var("AZ_DENSE_TEXT").as_deref() == Some("verify"),
            anim_debug: env.is_set("AZ_ANIM_DEBUG"),
            font_fallback_debug: env.is_set("AZ_FONT_FALLBACK_DEBUG"),
            scroll_contract_trace: env.var("AZ_SCROLL_CONTRACT_TRACE").is_some(),
            vv_reconcile_trace: env.is_set("AZ_VV_RECONCILE_TRACE"),
            vv_trace: env.is_set("AZ_VV_TRACE"),
            trace_hit: env.is_set("AZ_TRACE_HIT"),
            walk_scroll_boxes: env.is_set("AZ_WALK_SCROLL_BOXES"),
            scroll_debug: env.var("AZ_SCROLL_DEBUG").is_some_and(|v| v == "1"),
        }
    }
}

impl Default for WindowSettings {
    /// The behaviour of an empty environment.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "window_test.rs"]
mod tests;

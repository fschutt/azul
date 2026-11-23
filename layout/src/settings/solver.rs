//! The settings of the layout solver (`solver3`).
//!
//! What the passes of one document run under: how they build the display list, the storage form
//! of text layouts, the switches back to the old behaviour that exist for measuring, and the
//! traces printed to stderr. A window owns one value (`LayoutWindow::display_list_options`) and
//! writes it into the cache of every pass it runs (`LayoutCache::display_list_options`); the pass
//! copies it into its memo (`PassMemo::settings`). It is no process-wide switch: two windows - two
//! tests of one binary - can differ.
//!
//! The variables of the groups are named in this file only; the code that uses a setting reads a
//! plain field.

use azul_css::env::EnvSource;

/// What a patched display-list build does with the cached run of a box that MOVED since the list
/// was built (`DisplayListGenerator::try_copy_cached_run` says why a translated copy is not the
/// item a paint makes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovedRunPolicy {
    /// The default. The box is painted anew - except on a frame of a layout tween that another
    /// frame follows: that frame copies the run shifted by the move and records the box with the
    /// list (`DisplayList::translated_since_painted`), and the frame that ends the tween, like
    /// every pass that is no tween frame, paints the recorded boxes anew. A list at rest is bit
    /// for bit a painted one; a list during a tween is up to a rounding per frame off.
    TweenFramesTranslate,
    /// The behaviour before the audit, for measuring (`AZ_DL_TRANSLATE_MOVED_RUNS=1`): every pass
    /// copies the run shifted by the move, nothing is recorded and nothing is painted anew for
    /// having been translated.
    AlwaysTranslate,
    /// The other extreme, for measuring (`AZ_DL_TRANSLATE_MOVED_RUNS=0`): a moved box is painted
    /// anew by every pass, the frames of a tween too.
    NeverTranslate,
}

/// How the layout of a text is stored (`AZ_DENSE_TEXT`).
///
/// The dense model is the stored form in production: its retirement of the per-item sparse layout
/// plateaued at 65.2 MB of heap against 73.6 MB with it on the 960-line corpus.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DenseTextMode {
    /// `AZ_DENSE_TEXT=0` (or `off`), the escape hatch: the sparse reference walker makes the
    /// paint runs and the sparse layout is what is kept.
    Off,
    /// The default: the compact model is built and walked, and a layout of pure clusters keeps
    /// the dense view alone (the sparse layout is retired).
    #[default]
    On,
    /// `AZ_DENSE_TEXT=verify`: the dense path PLUS an A/B assert against the reference; both
    /// forms are kept. The e2e corpus run with it is the proof of the flip over real layouts.
    Verify,
}

/// The settings of the layout solver.
///
/// `Default` is the behaviour of an empty environment: patching on, a moved box's run painted
/// anew, the dense text model, absolute-box interior reuse and the taffy measure journal on, no
/// trace. The fields are named positively: where a variable turned a thing OFF
/// (`AZ_NO_DL_PATCH`), the field says what is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SolverSettings {
    /// Patch the display list that is kept: the runs of the boxes that neither reflowed, nor
    /// changed size, nor moved are copied from it and only the rest is painted. Off
    /// (`AZ_NO_DL_PATCH`): every pass paints every box. Both ways build the same list; the
    /// golden-equality tests lay a page out twice, once each way.
    pub patching: bool,
    /// What a patched build does with the cached run of a box that moved
    /// (`AZ_DL_TRANSLATE_MOVED_RUNS`: `1` translated by every pass, `0` painted anew by every
    /// pass, anything else and unset the default).
    pub moved_runs: MovedRunPolicy,
    /// Build no display list: the pass computes every box's place and returns an empty list. The
    /// web backend emits DOM patches instead of a list. Set by the host, no variable.
    pub skip: bool,
    /// How the layout of a text is stored (`AZ_DENSE_TEXT`).
    pub dense_text: DenseTextMode,
    /// Read an absolutely positioned box's interior off its last run instead of laying it out
    /// again (`AZ_NO_ABS_INTERIOR_REUSE` is the switch back to a run per box per pass).
    pub abs_interior_reuse: bool,
    /// Lay a reused interior out anyway and compare it with what the reuse kept; a difference
    /// panics (`AZ_CHECK_ABS_INTERIOR_REUSE`). The tests of the crate do it always.
    pub check_abs_interior_reuse: bool,
    /// Journal the measures of taffy so that the nodes below a measure are put back as they were
    /// (`AZ_NO_TAFFY_MEASURE_JOURNAL` is the switch back to the old way).
    pub taffy_measure_journal: bool,
    /// Trace the reconcile of the layout tree to stderr (`AZ_RECON_DEBUG`).
    pub trace_reconcile: bool,
    /// Trace the display-list patching to stderr (`AZ_PATCH_DEBUG`).
    pub trace_patch: bool,
    /// Trace the scrollbars of virtual views to stderr (`AZ_VV_BAR_TRACE`).
    pub trace_virtual_view_bars: bool,
    /// Trace the display-list cache of the animation frames to stderr (`AZ_ANIM_DEBUG`).
    pub trace_animation: bool,
    /// One line per taffy cache lookup, with the full key (`AZ_TAFFY_DEBUG`).
    pub trace_taffy: bool,
    /// Print, once, which OS the UA scrollbar cascade resolved against
    /// (`AZ_DUMP_SCROLLBAR_OS`).
    pub dump_scrollbar_os: bool,
}

impl Default for SolverSettings {
    fn default() -> Self {
        Self {
            patching: true,
            moved_runs: MovedRunPolicy::TweenFramesTranslate,
            skip: false,
            dense_text: DenseTextMode::On,
            abs_interior_reuse: true,
            check_abs_interior_reuse: false,
            taffy_measure_journal: true,
            trace_reconcile: false,
            trace_patch: false,
            trace_virtual_view_bars: false,
            trace_animation: false,
            trace_taffy: false,
            dump_scrollbar_os: false,
        }
    }
}

impl SolverSettings {
    /// The settings `env` asks for. A switch that is "set" is set to anything, an empty value
    /// included; `AZ_DL_TRANSLATE_MOVED_RUNS` and `AZ_DENSE_TEXT` read their value, and anything
    /// they do not know is the default.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            patching: !env.is_set("AZ_NO_DL_PATCH"),
            moved_runs: match env.var("AZ_DL_TRANSLATE_MOVED_RUNS").as_deref() {
                Some("1") => MovedRunPolicy::AlwaysTranslate,
                Some("0") => MovedRunPolicy::NeverTranslate,
                _ => MovedRunPolicy::TweenFramesTranslate,
            },
            skip: false,
            dense_text: match env.var("AZ_DENSE_TEXT").as_deref() {
                Some("0" | "off") => DenseTextMode::Off,
                Some("verify") => DenseTextMode::Verify,
                _ => DenseTextMode::On,
            },
            abs_interior_reuse: !env.is_set("AZ_NO_ABS_INTERIOR_REUSE"),
            check_abs_interior_reuse: env.is_set("AZ_CHECK_ABS_INTERIOR_REUSE"),
            taffy_measure_journal: !env.is_set("AZ_NO_TAFFY_MEASURE_JOURNAL"),
            trace_reconcile: env.is_set("AZ_RECON_DEBUG"),
            trace_patch: env.is_set("AZ_PATCH_DEBUG"),
            trace_virtual_view_bars: env.is_set("AZ_VV_BAR_TRACE"),
            trace_animation: env.is_set("AZ_ANIM_DEBUG"),
            trace_taffy: env.is_set("AZ_TAFFY_DEBUG"),
            dump_scrollbar_os: env.is_set("AZ_DUMP_SCROLLBAR_OS"),
        }
    }
}

#[cfg(test)]
#[path = "solver_test.rs"]
mod tests;

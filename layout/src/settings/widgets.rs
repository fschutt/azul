//! The switches of the widgets that the environment sets (group L5 of the env ledger), and the
//! one place of the crate that names their variables.
//!
//! Nothing here reads the process: every constructor takes a `&dyn EnvSource`
//! (`azul_css::env`), and `Default` is the behaviour of an EMPTY environment. The value is a small
//! `Copy` struct: a widget that has no window to ask (a render callback, a worker thread) captures
//! the part it needs from `LayoutWindow::settings` when it is mounted.

use azul_css::env::{EnvSource, NoEnv};

/// What the environment asks of the widgets: the traces of the map and the video widget.
///
/// Only widgets that can ask a window have a switch here: the builders of the colour picker and
/// of the node graph run in the app's layout callback with no window in reach, so their A/B
/// variables (`AZ_COLOR_PICKER_CHECKER`, `AZ_NODEGRAPH_PAN`) are gone, not wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WidgetSettings {
    /// `AZ_MAP_DEBUG` is set: the map widget's debug log.
    pub map_debug: bool,
    /// `AZ_MAP_STATS` is set, not blank and not `0`: the map prints its counters on stdout.
    pub map_stats: bool,
    /// `AZ_VIDEO_FRAMELOG` is set: log the video widget's renders and decode steps.
    pub video_framelog: bool,
}

impl WidgetSettings {
    /// What the environment `env` asks for; an empty environment asks for the shipping builds
    /// and no trace.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            map_debug: env.is_set("AZ_MAP_DEBUG"),
            map_stats: env
                .var("AZ_MAP_STATS")
                .is_some_and(|v| !v.trim().is_empty() && v.trim() != "0"),
            video_framelog: env.var("AZ_VIDEO_FRAMELOG").is_some(),
        }
    }
}

impl Default for WidgetSettings {
    /// The behaviour of an empty environment.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

#[cfg(test)]
#[path = "widgets_test.rs"]
mod tests;
